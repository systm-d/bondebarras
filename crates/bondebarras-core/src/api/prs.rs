//! Closed pull requests, used to flag dead caches.

use super::Client;
use anyhow::Result;
use std::collections::HashSet;

/// GitHub's maximum page size for this endpoint.
const PAGE_SIZE: usize = 100;

/// Hard stop on pagination. Ten pages covers a thousand closed pull requests;
/// beyond that the drill-down would cost more requests than the flag is worth.
const MAX_PR_PAGES: u32 = 10;

/// Numbers of every pull request that is no longer open.
///
/// `state=closed` covers merged PRs too — GitHub reports a merged PR as
/// closed, which is exactly the semantics we want: its caches are dead either
/// way.
pub async fn closed_numbers(client: &Client, owner: &str, repo: &str) -> Result<HashSet<u64>> {
    let mut out = HashSet::new();

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
        out.extend(items.iter().filter_map(|item| item["number"].as_u64()));

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
    async fn closed_numbers_collects_closed_and_merged_prs() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "number": 25, "state": "closed" },
                { "number": 32, "state": "closed" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let closed = closed_numbers(&client, "systm-d", "claudine")
            .await
            .unwrap();

        assert!(closed.contains(&25));
        assert!(closed.contains(&32));
        assert_eq!(closed.len(), 2);
    }

    #[tokio::test]
    async fn closed_numbers_follows_pagination() {
        let server = MockServer::start().await;
        // A full page means "there may be more"; the short second page ends it.
        let full: Vec<serde_json::Value> = (1..=100)
            .map(|n| serde_json::json!({ "number": n, "state": "closed" }))
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
                { "number": 101, "state": "closed" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let closed = closed_numbers(&client, "systm-d", "claudine")
            .await
            .unwrap();

        assert_eq!(closed.len(), 101);
        assert!(closed.contains(&1));
        assert!(closed.contains(&101));
    }
}
