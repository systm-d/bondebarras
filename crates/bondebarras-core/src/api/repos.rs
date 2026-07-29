//! Organization repository listing.

use super::Client;
use super::caches::age_days;
use anyhow::Result;

/// GitHub's maximum page size for this endpoint.
const PAGE_SIZE: usize = 100;

/// Hard stop on pagination. Ten pages covers a thousand repositories; beyond
/// that stage 1 would cost more requests than the overview is worth.
const MAX_REPO_PAGES: u32 = 10;

/// A repository as stage 1 sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub name: String,
    /// Private repos draw on the org's Actions allowance; public ones are
    /// free and unlimited. GitHub's usage report discounts both identically,
    /// so visibility is the only way to tell them apart.
    pub private: bool,
    /// Already read-only on GitHub's side. A missing field defaults to
    /// `false` — see `repos::classify_repo`'s own doc comment for why
    /// under-reporting archivability is the safe direction.
    pub archived: bool,
    /// Whether this token can administer the repository — `permissions.admin`
    /// on the repository object. A missing field (an unexpected API shape,
    /// or a token whose permissions GitHub declined to report) defaults to
    /// `false`: the archive endpoint answers 403 to a token that cannot
    /// administer, so assuming rights that were never confirmed is the wrong
    /// direction to guess in.
    pub admin: bool,
    /// Whole days since `pushed_at`. Unparseable or missing reads as `0` —
    /// see `api::caches::age_days`, shared with every other timestamp this
    /// crate reads.
    pub age_days: i64,
}

/// Every repository of an org, across as many pages as it takes.
///
/// A single page silently dropped every repo past the 100th — and since
/// finding 1, that is not just a cosmetic gap in the tree: a private repo
/// beyond page 1 is absent from the private set the Billing tab builds, so
/// its minutes vanish from the gauge with no indication anything was cut.
/// Under-reporting is still the safe direction, but silently is not good
/// enough once the number is one the user acts on.
pub async fn list(client: &Client, org: &str) -> Result<Vec<RepoRef>> {
    let mut out = Vec::new();

    for page in 1..=MAX_REPO_PAGES {
        let v = client
            .get_json(&format!(
                "/orgs/{org}/repos?per_page={PAGE_SIZE}&page={page}"
            ))
            .await?;

        let Some(items) = v.as_array() else { break };
        out.extend(items.iter().filter_map(|item| {
            let name = item["name"].as_str()?.to_string();
            // A missing field defaults to public: under-reporting the
            // gauge is the safe direction for a number the user may
            // act on, unlike inventing consumption that never
            // happened.
            let private = item["private"].as_bool().unwrap_or(false);
            let archived = item["archived"].as_bool().unwrap_or(false);
            let admin = item["permissions"]["admin"].as_bool().unwrap_or(false);
            let age_days = age_days(item["pushed_at"].as_str());
            Some(RepoRef {
                name,
                private,
                archived,
                admin,
                age_days,
            })
        }));

        // A short page is the last one.
        if items.len() < PAGE_SIZE {
            break;
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn list_returns_short_repo_names() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "josephine", "private": false },
                { "name": "claudine", "private": false }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let names: Vec<String> = list(&client, "systm-d")
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.name)
            .collect();

        assert_eq!(names, vec!["josephine".to_string(), "claudine".to_string()]);
    }

    /// Locks finding 1: visibility is the only field that lets `billing`
    /// distinguish "public, free forever" from "private, covered by the
    /// allowance" once GitHub has discounted both identically. A wrong
    /// implementation that drops the field, or always reports `false`, must
    /// fail this on the private repo.
    #[tokio::test]
    async fn list_carries_the_private_flag() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/SecondBrain-io/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "monolith-back", "private": true },
                { "name": "public-site", "private": false }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let repos = list(&client, "SecondBrain-io").await.unwrap();

        assert_eq!(
            repos,
            vec![
                RepoRef {
                    name: "monolith-back".to_string(),
                    private: true,
                    archived: false,
                    admin: false,
                    age_days: 0,
                },
                RepoRef {
                    name: "public-site".to_string(),
                    private: false,
                    archived: false,
                    admin: false,
                    age_days: 0,
                },
            ]
        );
    }

    /// Locks task 3's rule 2: an already-archived repository, or one this
    /// token cannot administer, must never be offered as a tick — the API
    /// would answer 403 to the second, and offering the first again is
    /// pointless. `classify_repo` decides the row's fate from these two
    /// fields, so `list` must carry them faithfully.
    #[tokio::test]
    async fn list_carries_archived_status_and_admin_rights() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/maxds-lyon/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": ".github", "private": false, "archived": true,
                  "permissions": { "admin": true, "push": true, "pull": true } },
                { "name": "lokiprint", "private": false, "archived": false,
                  "permissions": { "admin": false, "push": true, "pull": true } },
                { "name": "repolens", "private": false, "archived": false,
                  "permissions": { "admin": true, "push": true, "pull": true } }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let repos = list(&client, "maxds-lyon").await.unwrap();
        let find = |name: &str| repos.iter().find(|r| r.name == name).unwrap();

        assert!(
            find(".github").archived,
            "already-archived must carry through"
        );
        assert!(
            !find("lokiprint").admin,
            "missing admin rights must carry through"
        );
        assert!(
            find("repolens").admin,
            "a genuine admin repo must read true"
        );
        assert!(!find("repolens").archived);
    }

    /// A repo listing that omits `archived` or `permissions` entirely (an
    /// unexpected API shape) must default to the safer reading in each
    /// direction: `archived: false` under-reports archivability the same way
    /// a missing `private` under-reports visibility, and `admin: false`
    /// never invites a tick the API would refuse with a 403.
    #[tokio::test]
    async fn list_defaults_archived_and_admin_when_fields_are_missing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "no-status-fields" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let repos = list(&client, "systm-d").await.unwrap();

        assert!(!repos[0].archived);
        assert!(!repos[0].admin);
    }

    /// `age_days` must actually be computed from `pushed_at`, not hardcoded —
    /// a repository with no push in over two years is exactly the shape the
    /// tree's own age column exists to surface. A fixed, far-past date avoids
    /// asserting an exact day count against the live clock.
    #[tokio::test]
    async fn list_computes_age_from_pushed_at() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/maxds-lyon/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "lokiprint", "private": false, "pushed_at": "2024-01-01T00:00:00Z" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let repos = list(&client, "maxds-lyon").await.unwrap();

        assert!(
            repos[0].age_days > 300,
            "got {} — pushed_at must feed a real age, not a hardcoded 0",
            repos[0].age_days
        );
    }

    /// A repo listing that omits `private` (an unexpected API shape) must
    /// default to public, not private: under-reporting the gauge is the safe
    /// direction, over-reporting invents consumption that never happened.
    #[tokio::test]
    async fn list_defaults_a_missing_private_field_to_public() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "no-visibility-field" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let repos = list(&client, "systm-d").await.unwrap();

        assert!(!repos[0].private);
    }

    /// Locks residual 1 of the re-review: a single page silently dropped
    /// every repo past the 100th, and since finding 1 that is not just a
    /// cosmetic gap — a private repo beyond page 1 would be absent from the
    /// private set the Billing tab builds, so its minutes would vanish from
    /// the gauge unannounced. Modeled directly on
    /// `prs::closed_prs_follows_pagination`.
    #[tokio::test]
    async fn list_follows_pagination() {
        let server = MockServer::start().await;
        // A full page means "there may be more"; the short second page ends it.
        let full: Vec<serde_json::Value> = (1..=100)
            .map(|n| serde_json::json!({ "name": format!("repo-{n}"), "private": false }))
            .collect();
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/repos"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(full))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/repos"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "monolith-back", "private": true }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let repos = list(&client, "systm-d").await.unwrap();

        assert_eq!(repos.len(), 101, "the second page must not be dropped");
        assert!(
            repos.iter().any(|r| r.name == "monolith-back" && r.private),
            "a private repo on the second page must still carry its flag"
        );
    }
}
