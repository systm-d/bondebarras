//! The one boundary that knows about octocrab.
//!
//! Typed endpoints and raw ones live behind the same two primitives, so the
//! rest of the crate never learns which responses octocrab models and which
//! we deserialise by hand.

pub mod artifacts;
pub mod billing;
pub mod caches;
pub mod packages;
pub mod prs;
pub mod refs;
pub mod releases;
pub mod repos;
pub mod runs;

use crate::auth::Scopes;
use anyhow::{Context, Result, bail};
use http::StatusCode;
use octocrab::Octocrab;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::sleep;

/// GitHub's public API root.
const DEFAULT_BASE: &str = "https://api.github.com";

/// Hard ceiling on any single network call. Without it a half-open TCP
/// connection can hang a scan indefinitely.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Concurrent read requests. GitHub's primary limit (5000/h) is never the
/// binding constraint at this scale; the secondary limit on burst concurrency
/// is.
const READ_CONCURRENCY: usize = 8;

/// How many times a throttled deletion is retried before giving up. Three
/// covers the transient case without stalling the interface indefinitely.
const MAX_DELETE_RETRIES: usize = 3;

/// Wait used when GitHub throttles without naming a delay in `Retry-After`.
const DEFAULT_BACKOFF: Duration = Duration::from_secs(5);

pub struct Client {
    gh: Octocrab,
    sem: Arc<Semaphore>,
    scopes: Scopes,
}

impl Client {
    pub fn new(token: &str) -> Result<Self> {
        Self::with_base(token, DEFAULT_BASE)
    }

    /// Build a client against an arbitrary API root. Tests point this at a
    /// wiremock server.
    pub fn with_base(token: &str, base: &str) -> Result<Self> {
        let gh = Octocrab::builder()
            .personal_token(token.to_string())
            .set_connect_timeout(Some(HTTP_TIMEOUT))
            .set_read_timeout(Some(HTTP_TIMEOUT))
            .base_uri(base)
            .context("URL de base invalide")?
            .build()
            .context("construction du client GitHub")?;

        Ok(Client {
            gh,
            sem: Arc::new(Semaphore::new(READ_CONCURRENCY)),
            scopes: Scopes::default(),
        })
    }

    pub fn scopes(&self) -> &Scopes {
        &self.scopes
    }

    /// Record the scopes advertised by the API. Called once at startup.
    pub fn set_scopes(&mut self, scopes: Scopes) {
        self.scopes = scopes;
    }

    /// GET returning parsed JSON, throttled by the read semaphore.
    pub async fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        let _permit = self.sem.acquire().await.expect("semaphore never closed");
        self.gh
            .get::<serde_json::Value, _, ()>(path, None::<&()>)
            .await
            .with_context(|| format!("GET {path}"))
    }

    /// GET whose absence is the ordinary case, not a failure.
    ///
    /// A repository's homonymous container package, for one, 404s far more
    /// often than it exists — most repositories publish no image at all.
    /// `get_json` cannot be used for this: on any non-2xx status, octocrab
    /// tries to parse the response body as its own `{message, ...}` GitHub
    /// error shape *before* it ever looks at the status code. GitHub's bare
    /// `404` — no JSON body at all, which is both what a repo with no image
    /// sends and what wiremock's default `ResponseTemplate::new(404)` sends
    /// in tests — fails that parse. The status is never reached, so
    /// `get_json` would surface "no image published" as a JSON-parse error,
    /// indistinguishable from a genuinely malformed response.
    ///
    /// This method goes through the raw `_get` instead, and decides the
    /// missing case from the HTTP status alone, before any attempt to read a
    /// body. Only a non-404 response goes on to `map_github_error` and JSON
    /// parsing — so a malformed body on a real `200` still surfaces as an
    /// error here, exactly as it would through `get_json`; only the 404 is
    /// allowed to degrade to "nothing here".
    pub async fn get_json_or_missing(&self, path: &str) -> Result<Option<serde_json::Value>> {
        let _permit = self.sem.acquire().await.expect("semaphore never closed");
        let response = self
            .gh
            ._get(path)
            .await
            .with_context(|| format!("GET {path}"))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let response = octocrab::map_github_error(response)
            .await
            .with_context(|| format!("GET {path}"))?;
        let body = self
            .gh
            .body_to_string(response)
            .await
            .with_context(|| format!("GET {path}"))?;

        serde_json::from_str(&body)
            .with_context(|| format!("GET {path} : réponse JSON invalide"))
            .map(Some)
    }

    /// DELETE ignoring the (usually empty) body, retrying when GitHub throttles.
    ///
    /// `Octocrab::delete` would try to deserialise the empty 204 body, so we
    /// go through `_delete` and read the status ourselves. `BaseUriLayer`
    /// still supplies scheme and authority, so a bare path is enough.
    ///
    /// Retry lives here rather than on reads because deletions are what hit
    /// the ceiling: a purge of 132 caches is a burst, and GitHub answers a
    /// burst with its secondary rate limit. A stage-1 scan is 60 reads at a
    /// concurrency of 8 and never gets close.
    pub async fn delete(&self, path: &str) -> Result<()> {
        let mut attempt = 0;
        loop {
            let wait = {
                let _permit = self.sem.acquire().await.expect("semaphore never closed");
                let response = self
                    .gh
                    ._delete(path, None::<&()>)
                    .await
                    .with_context(|| format!("DELETE {path}"))?;

                let status = response.status();
                if status.is_success() {
                    return Ok(());
                }

                let delay = retry_after(response.headers());

                // 429 is unambiguous. 403 is not: GitHub returns it both for
                // the secondary rate limit and for a missing scope, and the
                // second is far more common. Only the throttling one carries
                // `Retry-After` — without that header a 403 is a permission
                // error, and retrying it would just delay the real message by
                // three backoffs. Any other status is a real failure too.
                let throttled = status == StatusCode::TOO_MANY_REQUESTS
                    || (status == StatusCode::FORBIDDEN && delay.is_some());
                if !throttled || attempt == MAX_DELETE_RETRIES {
                    bail!("DELETE {path} a échoué : {status}");
                }
                delay.unwrap_or(DEFAULT_BACKOFF)
            };

            attempt += 1;
            // The permit has dropped here: the backoff does not hold a slot.
            sleep(wait).await;
        }
    }
}

/// The delay GitHub asked us to wait, when it named one in `Retry-After`.
///
/// The header is seconds-as-integer in the throttling responses GitHub sends.
/// An absent or unparseable value is not an error — the caller falls back to
/// [`DEFAULT_BACKOFF`].
fn retry_after(headers: &http::HeaderMap) -> Option<Duration> {
    headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn get_json_reads_from_the_injected_base() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/actions/cache/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_active_caches_size_in_bytes": 37_166_609_585_u64,
                "total_active_caches_count": 132,
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let v = client
            .get_json("/orgs/systm-d/actions/cache/usage")
            .await
            .unwrap();

        assert_eq!(v["total_active_caches_count"], 132);
    }

    #[tokio::test]
    async fn get_json_or_missing_reads_none_from_a_bodyless_404() {
        // The wiremock default: a bare 404 with no response body at all —
        // exactly what "this repo publishes no image" looks like on the
        // real API. `get_json` would fail trying to parse this as an error
        // body; this method must not.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/no-such/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = client
            .get_json_or_missing("/orgs/systm-d/packages/container/no-such/versions")
            .await
            .unwrap();

        assert!(out.is_none());
    }

    #[tokio::test]
    async fn get_json_or_missing_reads_the_body_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/repolens/versions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{ "id": 1 }])),
            )
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = client
            .get_json_or_missing("/orgs/systm-d/packages/container/repolens/versions")
            .await
            .unwrap()
            .expect("a 200 must yield a body");

        assert_eq!(out[0]["id"], 1);
    }

    #[tokio::test]
    async fn get_json_or_missing_still_errors_on_a_malformed_200_body() {
        // Only a 404 is allowed to degrade to "nothing here". A wrong
        // implementation that swallows every failure alike (as `billing::fetch`
        // deliberately does, for a different reason) would turn a genuinely
        // broken response into a silent empty list — indistinguishable from
        // the ordinary "no image published" case this method exists for.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/repolens/versions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let err = client
            .get_json_or_missing("/orgs/systm-d/packages/container/repolens/versions")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("réponse JSON invalide"));
    }

    #[tokio::test]
    async fn delete_accepts_a_204_with_no_body() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/actions/caches/9"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        client
            .delete("/repos/systm-d/claudine/actions/caches/9")
            .await
            .unwrap();
    }

    #[test]
    fn retry_after_reads_the_header_as_seconds() {
        let mut headers = http::HeaderMap::new();
        assert_eq!(retry_after(&headers), None);

        headers.insert("retry-after", "42".parse().unwrap());
        assert_eq!(retry_after(&headers), Some(Duration::from_secs(42)));

        // GitHub only ever sends integer seconds here; an HTTP-date or any
        // other shape must read as "no delay named", not as an error.
        headers.insert(
            "retry-after",
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(retry_after(&headers), None);
    }

    #[tokio::test]
    async fn delete_retries_when_github_throttles() {
        let server = MockServer::start().await;
        // A 0-second Retry-After keeps the test fast while still exercising
        // the header path; the retry loop is what is under test here.
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/actions/caches/9"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/actions/caches/9"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        client
            .delete("/repos/systm-d/claudine/actions/caches/9")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_permission_403_is_not_retried() {
        let server = MockServer::start().await;
        // No Retry-After: this is a missing-scope 403, not the secondary rate
        // limit. It must surface immediately instead of costing three backoffs.
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/actions/caches/9"))
            .respond_with(ResponseTemplate::new(403))
            .expect(1)
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let err = client
            .delete("/repos/systm-d/claudine/actions/caches/9")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("403"));
        // `expect(1)` is verified when the server drops: a retry would fail it.
    }

    #[tokio::test]
    async fn delete_surfaces_a_failing_status() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/actions/caches/9"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let err = client
            .delete("/repos/systm-d/claudine/actions/caches/9")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("404"));
    }
}
