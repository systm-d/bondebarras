//! Closed pull requests, used to flag dead caches and dead branches.

use super::Client;
use anyhow::Result;
use std::collections::HashSet;

/// GitHub's maximum page size for this endpoint.
const PAGE_SIZE: usize = 100;

/// Hard stop on pagination. Ten pages covers a thousand closed pull requests;
/// beyond that the drill-down would cost more requests than the flag is worth.
const MAX_PR_PAGES: u32 = 10;

/// Everything the closed-PR listing tells us, from one paginated fetch.
///
/// Both fields come from the same pages: fetching them separately would cost
/// a second full pagination of the same endpoint for no reason.
#[derive(Debug, Clone, Default)]
pub struct ClosedPrs {
    /// Numbers of every pull request that is no longer open, for the caches'
    /// ⚑ flag. `state=closed` covers merged PRs too — GitHub reports a merged
    /// PR as closed, which is exactly the semantics we want: its caches are
    /// dead either way.
    pub numbers: HashSet<u64>,
    /// `head.ref` of the PRs that were actually merged, for dead branches. A
    /// PR closed without merging contributes nothing here — that work was
    /// rejected, not integrated, and its branch may still be resumed.
    pub merged_refs: HashSet<String>,
}

/// Fetch every closed pull request once, and derive both the caches' ⚑ flag
/// data and the dead-branch data from the same pages.
///
/// A `compare` call per branch would find the same dead branches, but at one
/// request per branch — a hundred on a real repository. This costs zero
/// extra requests: `head.ref` and `merged_at` already ride along on the
/// listing this endpoint was already fetching for the ⚑ flag.
pub async fn closed_prs(client: &Client, owner: &str, repo: &str) -> Result<ClosedPrs> {
    let mut out = ClosedPrs::default();

    // A single page would silently drop a busy repo's older closed PRs, and
    // every cache pinned to them would stay unflagged. That under-flags rather
    // than over-flags, so nothing gets wrongly deleted — but it is reclaimable
    // space the user never sees, in a tool whose whole job is to show it.
    for page in 1..=MAX_PR_PAGES {
        let v = client
            .get_json(&format!(
                "/repos/{owner}/{repo}/pulls?state=closed&per_page={PAGE_SIZE}&page={page}"
            ))
            .await?;

        let Some(items) = v.as_array() else { break };
        for item in items {
            if let Some(n) = item["number"].as_u64() {
                out.numbers.insert(n);
            }
            // Only a PR that was actually merged retires its branch. A PR
            // closed without merging leaves `merged_at` null.
            if !item["merged_at"].is_null()
                && let Some(head_ref) = item["head"]["ref"].as_str()
            {
                out.merged_refs.insert(head_ref.to_string());
            }
        }

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
    async fn closed_prs_collects_closed_and_merged_pr_numbers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "number": 25, "state": "closed", "merged_at": null,
                  "head": { "ref": "feature/rejected" } },
                { "number": 32, "state": "closed", "merged_at": "2026-01-01T00:00:00Z",
                  "head": { "ref": "claude/landing-3jbqk4" } }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let closed = closed_prs(&client, "systm-d", "claudine").await.unwrap();

        assert!(closed.numbers.contains(&25));
        assert!(closed.numbers.contains(&32));
        assert_eq!(closed.numbers.len(), 2);
    }

    #[tokio::test]
    async fn closed_prs_follows_pagination() {
        let server = MockServer::start().await;
        // A full page means "there may be more"; the short second page ends it.
        let full: Vec<serde_json::Value> = (1..=100)
            .map(|n| serde_json::json!({ "number": n, "state": "closed", "merged_at": null }))
            .collect();
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(full))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "number": 101, "state": "closed", "merged_at": null }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let closed = closed_prs(&client, "systm-d", "claudine").await.unwrap();

        assert_eq!(closed.numbers.len(), 101);
        assert!(closed.numbers.contains(&1));
        assert!(closed.numbers.contains(&101));
    }

    /// The API-level counterpart of `refs::a_branch_with_no_merged_pr_is_alive`:
    /// a PR closed *without* merging must contribute nothing to `merged_refs`,
    /// even though its number still belongs in `numbers`. An implementation
    /// that keyed merged-ref extraction on `state == "closed"` alone — instead
    /// of on `merged_at` being non-null — would pass every other test here
    /// (both closed and merged PRs share `state: "closed"` on GitHub) and
    /// still leak a rejected branch's ref into `merged_refs`, which is exactly
    /// the "offer to delete work someone meant to revisit" failure the design
    /// note calls out.
    #[tokio::test]
    async fn closed_prs_extracts_merged_refs_only_for_merged_prs() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "number": 25, "state": "closed", "merged_at": null,
                  "head": { "ref": "feature/rejected" } },
                { "number": 32, "state": "closed", "merged_at": "2026-01-01T00:00:00Z",
                  "head": { "ref": "claude/landing-3jbqk4" } }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let closed = closed_prs(&client, "systm-d", "claudine").await.unwrap();

        assert!(closed.merged_refs.contains("claude/landing-3jbqk4"));
        assert!(!closed.merged_refs.contains("feature/rejected"));
        assert_eq!(closed.merged_refs.len(), 1);
    }

    /// Production data (`systm-d/claudine`) showed one branch backing PRs
    /// #29, #30 *and* #31 — `merged_refs` must dedup across PRs, which a
    /// `Vec<String>` would not do for free.
    #[tokio::test]
    async fn closed_prs_dedups_merged_refs_shared_by_several_prs() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "number": 29, "state": "closed", "merged_at": "2026-01-01T00:00:00Z",
                  "head": { "ref": "claude/landing-3jbqk4" } },
                { "number": 30, "state": "closed", "merged_at": "2026-01-02T00:00:00Z",
                  "head": { "ref": "claude/landing-3jbqk4" } },
                { "number": 31, "state": "closed", "merged_at": "2026-01-03T00:00:00Z",
                  "head": { "ref": "claude/landing-3jbqk4" } }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let closed = closed_prs(&client, "systm-d", "claudine").await.unwrap();

        assert_eq!(closed.merged_refs.len(), 1);
        assert_eq!(closed.numbers.len(), 3);
    }
}
