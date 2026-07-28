//! Closed pull requests, used to flag dead caches.

use super::Client;
use anyhow::Result;
use std::collections::HashSet;

/// Numbers of every pull request that is no longer open.
///
/// `state=closed` covers merged PRs too — GitHub reports a merged PR as
/// closed, which is exactly the semantics we want: its caches are dead either
/// way.
pub async fn closed_numbers(client: &Client, owner: &str, repo: &str) -> Result<HashSet<u64>> {
    let v = client
        .get_json(&format!(
            "/repos/{owner}/{repo}/pulls?state=closed&per_page=100"
        ))
        .await?;

    Ok(v.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["number"].as_u64())
                .collect()
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn closed_numbers_collects_closed_and_merged_prs() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
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
}
