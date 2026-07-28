//! Actions cache endpoints.
//!
//! None of these are in octocrab's typed surface, so they go through
//! `Client::get_json` / `Client::delete`.

use super::Client;
use crate::model::{RepoSummary, Resource, ResourceKind};
use anyhow::Result;
use chrono::{DateTime, Utc};

/// Per-repository cache totals for a whole org — one request for the lot.
/// This is what makes the stage-1 overview instant.
pub async fn usage_by_repository(client: &Client, org: &str) -> Result<Vec<RepoSummary>> {
    let v = client
        .get_json(&format!("/orgs/{org}/actions/cache/usage-by-repository"))
        .await?;

    Ok(v["repository_cache_usages"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let full = item["full_name"].as_str().unwrap_or_default();
                    RepoSummary {
                        // `full_name` is "org/repo"; the org prefix is noise here.
                        name: full.split_once('/').map_or(full, |(_, r)| r).to_string(),
                        cache_bytes: item["active_caches_size_in_bytes"].as_u64().unwrap_or(0),
                        cache_count: item["active_caches_count"].as_u64().unwrap_or(0) as u32,
                    }
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Individual caches of one repository, for the drill-down pane.
pub async fn list(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>> {
    let v = client
        .get_json(&format!(
            "/repos/{owner}/{repo}/actions/caches?per_page=100"
        ))
        .await?;

    Ok(v["actions_caches"]
        .as_array()
        .map(|items| {
            items
                .iter()
                // An item with no addressable id is one we must not offer to
                // delete — coercing it to id 0 would risk colliding with a
                // real cache 0 and deleting the wrong thing.
                .filter_map(|item| {
                    let id = item["id"].as_u64()?;
                    Some(Resource {
                        kind: ResourceKind::Cache,
                        id,
                        label: item["key"].as_str().unwrap_or_default().to_string(),
                        size_bytes: item["size_in_bytes"].as_u64().unwrap_or(0),
                        age_days: age_days(item["last_accessed_at"].as_str()),
                        git_ref: item["ref"].as_str().map(str::to_string),
                        // Filled in by `scan`, which knows the repo's closed PRs.
                        stale_pr: false,
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub async fn delete(client: &Client, owner: &str, repo: &str, id: u64) -> Result<()> {
    client
        .delete(&format!("/repos/{owner}/{repo}/actions/caches/{id}"))
        .await
}

/// Whole days between an RFC 3339 timestamp and now. Unparseable or missing
/// timestamps read as 0 rather than failing the whole listing.
pub(crate) fn age_days(timestamp: Option<&str>) -> i64 {
    timestamp
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| (Utc::now() - t.with_timezone(&Utc)).num_days())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn usage_by_repository_maps_every_repo() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/actions/cache/usage-by-repository"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 2,
                "repository_cache_usages": [
                    { "full_name": "systm-d/josephine",
                      "active_caches_size_in_bytes": 12_372_371_816_u64,
                      "active_caches_count": 30 },
                    { "full_name": "systm-d/claudine",
                      "active_caches_size_in_bytes": 11_130_027_303_u64,
                      "active_caches_count": 69 }
                ]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let repos = usage_by_repository(&client, "systm-d").await.unwrap();

        assert_eq!(repos.len(), 2);
        // `full_name` is split: the org prefix is redundant in the repo column.
        assert_eq!(repos[0].name, "josephine");
        assert_eq!(repos[0].cache_bytes, 12_372_371_816);
        assert_eq!(repos[1].cache_count, 69);
    }

    #[tokio::test]
    async fn list_carries_the_ref_and_size_of_each_cache() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 1,
                "actions_caches": [
                    { "id": 9, "ref": "refs/pull/32/merge",
                      "key": "v0-rust-coverage-Linux-x64-db7c195c",
                      "size_in_bytes": 273_678_336_u64,
                      "last_accessed_at": "2026-06-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = list(&client, "systm-d", "claudine").await.unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ResourceKind::Cache);
        assert_eq!(items[0].id, 9);
        assert_eq!(items[0].git_ref.as_deref(), Some("refs/pull/32/merge"));
        assert_eq!(items[0].size_bytes, 273_678_336);
        // Staleness is decided later, once the PR list is known.
        assert!(!items[0].stale_pr);
    }
}
