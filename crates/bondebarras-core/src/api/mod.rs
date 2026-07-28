//! The one boundary that knows about octocrab.
//!
//! Typed endpoints and raw ones live behind the same two primitives, so the
//! rest of the crate never learns which responses octocrab models and which
//! we deserialise by hand.

pub mod artifacts;
pub mod caches;
pub mod prs;
pub mod repos;
pub mod runs;

use crate::auth::Scopes;
use anyhow::{Context, Result, bail};
use octocrab::Octocrab;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// GitHub's public API root.
const DEFAULT_BASE: &str = "https://api.github.com";

/// Hard ceiling on any single network call. Without it a half-open TCP
/// connection can hang a scan indefinitely.
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Concurrent read requests. GitHub's primary limit (5000/h) is never the
/// binding constraint at this scale; the secondary limit on burst concurrency
/// is.
const READ_CONCURRENCY: usize = 8;

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

    /// DELETE ignoring the (usually empty) body, surfacing the status code.
    ///
    /// `Octocrab::delete` would try to deserialise the empty 204 body, so we
    /// go through `_delete` and read the status ourselves. `BaseUriLayer`
    /// still supplies scheme and authority, so a bare path is enough.
    pub async fn delete(&self, path: &str) -> Result<()> {
        let _permit = self.sem.acquire().await.expect("semaphore never closed");
        let response = self
            .gh
            ._delete(path, None::<&()>)
            .await
            .with_context(|| format!("DELETE {path}"))?;

        let status = response.status();
        if !status.is_success() {
            bail!("DELETE {path} a échoué : {status}");
        }
        Ok(())
    }
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
