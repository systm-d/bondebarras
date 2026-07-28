//! Two-stage scanning.
//!
//! Stage 1 runs at launch and only touches org-level aggregates — two requests
//! per org, so fifteen orgs land in about three seconds. Stage 2 fetches a
//! repository's individual resources, and only when the user opens it. Paying
//! only for what you look at is what keeps manual navigation viable across a
//! hundred repositories.

use crate::api::{Client, artifacts, caches, prs, repos, runs};
use crate::model::{OrgSummary, Resource};
use crate::stale::is_stale;
use anyhow::Result;
use std::collections::HashSet;

/// Stage 1: cache aggregates and repository list for each org.
///
/// An org that fails — revoked permission, network blip — is dropped from the
/// result rather than failing the whole scan. With fifteen orgs, one bad
/// permission must not blank the screen.
pub async fn overview(client: &Client, orgs: &[String]) -> Vec<OrgSummary> {
    let futures = orgs.iter().map(|org| async move {
        let summaries = caches::usage_by_repository(client, org).await.ok()?;
        let names = repos::list(client, org).await.ok()?;

        let cache_bytes = summaries.iter().map(|r| r.cache_bytes).sum();
        let cache_count = summaries.iter().map(|r| r.cache_count).sum();

        // Repos with no cache still belong in the tree: they may hold
        // artifacts or runs, which stage 2 will surface.
        let mut repos_out = summaries;
        for name in names {
            if !repos_out.iter().any(|r| r.name == name) {
                repos_out.push(crate::model::RepoSummary {
                    name,
                    cache_bytes: 0,
                    cache_count: 0,
                });
            }
        }
        repos_out.sort_by_key(|r| std::cmp::Reverse(r.cache_bytes));

        Some(OrgSummary {
            login: org.clone(),
            cache_bytes,
            cache_count,
            repos: repos_out,
        })
    });

    let mut out: Vec<OrgSummary> = futures::future::join_all(futures)
        .await
        .into_iter()
        .flatten()
        .collect();
    out.sort_by_key(|o| std::cmp::Reverse(o.cache_bytes));
    out
}

/// Stage 2: every deletable resource of one repository, already flagged.
pub async fn repo_detail(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>> {
    let (caches_r, artifacts_r, runs_r, closed) = futures::join!(
        caches::list(client, owner, repo),
        artifacts::list(client, owner, repo),
        runs::list(client, owner, repo),
        prs::closed_numbers(client, owner, repo),
    );

    let mut items = caches_r?;
    items.extend(artifacts_r?);
    items.extend(runs_r?);

    // A failed PR listing costs the flag, not the listing: everything still
    // shows, just without the ⚑ shortcut.
    mark_stale(&mut items, &closed.unwrap_or_default());

    items.sort_by_key(|i| std::cmp::Reverse(i.size_bytes));
    Ok(items)
}

/// Flag every resource whose ref belongs to a closed pull request.
pub fn mark_stale(items: &mut [Resource], closed_prs: &HashSet<u64>) {
    for item in items.iter_mut() {
        item.stale_pr = is_stale(item.git_ref.as_deref(), closed_prs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ResourceKind;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cache(id: u64, git_ref: &str) -> Resource {
        Resource {
            kind: ResourceKind::Cache,
            id,
            label: format!("cache-{id}"),
            size_bytes: 1_000,
            age_days: 12,
            git_ref: Some(git_ref.to_string()),
            stale_pr: false,
        }
    }

    #[test]
    fn mark_stale_flags_only_caches_of_closed_prs() {
        let mut items = vec![
            cache(1, "refs/pull/32/merge"),
            cache(2, "refs/heads/main"),
            cache(3, "refs/pull/99/merge"),
        ];
        let closed = HashSet::from([32_u64]);

        mark_stale(&mut items, &closed);

        assert!(items[0].stale_pr);
        assert!(!items[1].stale_pr);
        assert!(!items[2].stale_pr);
    }

    #[tokio::test]
    async fn an_org_that_fails_is_dropped_not_fatal() {
        // `overview` tolerates a failing org so one broken permission does not
        // blank the whole screen: it must drop the org, not error out or
        // panic the whole scan.
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/orgs/healthy-org/actions/cache/usage-by-repository"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 1,
                "repository_cache_usages": [
                    { "full_name": "healthy-org/josephine",
                      "active_caches_size_in_bytes": 12_372_371_816_u64,
                      "active_caches_count": 30 }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/healthy-org/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "josephine" }
            ])))
            .mount(&server)
            .await;

        // The broken org's cache endpoint 404s — a revoked permission or a
        // network blip, exactly the case `overview` must tolerate.
        Mock::given(method("GET"))
            .and(path("/orgs/broken-org/actions/cache/usage-by-repository"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/broken-org/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let orgs = vec!["healthy-org".to_string(), "broken-org".to_string()];

        let summaries = overview(&client, &orgs).await;

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].login, "healthy-org");
        assert!(summaries.iter().all(|s| s.login != "broken-org"));
    }
}
