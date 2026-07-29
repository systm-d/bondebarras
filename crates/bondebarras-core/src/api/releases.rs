//! Release asset endpoints.
//!
//! Releases themselves are never deletable — only their assets are (see the
//! design doc's §3/§7: a release is a point in the repository's history, a
//! tag with notes and a date, and deleting the whole thing is out of scope
//! forever). `assets` therefore flattens every release's asset list into one
//! collection, carrying `release_tag` along so a caller can still say which
//! release an asset came from, even though the release itself never becomes
//! its own row.

use super::Client;
use super::caches::age_days;
use anyhow::Result;

/// GitHub's maximum page size for this endpoint.
const PAGE_SIZE: usize = 100;

/// Hard stop on pagination, mirroring `prs::MAX_PR_PAGES`. The measured data
/// (25 releases on `exec-d/terminus`) never approaches this, but every other
/// paginated listing in this crate follows the same idiom rather than
/// assuming today's shape holds forever.
const MAX_RELEASE_PAGES: u32 = 10;

/// One release's binary, flattened out of its parent release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub id: u64,
    pub name: String,
    pub size: u64,
    /// The tag of the release this asset belongs to — the release itself is
    /// never surfaced as its own row, so this is the only trace of it left.
    pub release_tag: String,
    pub age_days: i64,
}

/// Every asset of every release, across as many pages of releases as it takes.
pub async fn assets(client: &Client, owner: &str, repo: &str) -> Result<Vec<ReleaseAsset>> {
    let mut out = Vec::new();

    for page in 1..=MAX_RELEASE_PAGES {
        let v = client
            .get_json(&format!(
                "/repos/{owner}/{repo}/releases?per_page={PAGE_SIZE}&page={page}"
            ))
            .await?;

        let Some(page_releases) = v.as_array() else {
            break;
        };
        for release in page_releases {
            let release_tag = release["tag_name"].as_str().unwrap_or_default().to_string();
            let Some(release_assets) = release["assets"].as_array() else {
                continue;
            };
            // An asset with no addressable id is one we must not offer to
            // delete — coercing it to id 0 would collide every such asset
            // into one selection slot, in a tool with no undo.
            out.extend(release_assets.iter().filter_map(|a| {
                let id = a["id"].as_u64()?;
                Some(ReleaseAsset {
                    id,
                    name: a["name"].as_str().unwrap_or_default().to_string(),
                    size: a["size"].as_u64().unwrap_or(0),
                    release_tag: release_tag.clone(),
                    age_days: age_days(a["created_at"].as_str()),
                })
            }));
        }

        // A short page is the last one.
        if page_releases.len() < PAGE_SIZE {
            break;
        }
    }

    Ok(out)
}

pub async fn delete_asset(client: &Client, owner: &str, repo: &str, id: u64) -> Result<()> {
    client
        .delete(&format!("/repos/{owner}/{repo}/releases/assets/{id}"))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn assets_are_flattened_out_of_their_releases_and_carry_the_release_tag() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/exec-d/terminus/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "tag_name": "v0.1.1",
                    "assets": [
                        { "id": 1, "name": "terminus-linux-x86_64.tar.gz",
                          "size": 2_400_000_u64, "created_at": "2026-06-01T00:00:00Z" }
                    ]
                },
                {
                    "tag_name": "v0.1.0",
                    "assets": [
                        { "id": 2, "name": "terminus-macos-arm64.tar.gz",
                          "size": 1_800_000_u64, "created_at": "2026-05-01T00:00:00Z" }
                    ]
                }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = assets(&client, "exec-d", "terminus").await.unwrap();

        assert_eq!(out.len(), 2);
        let first = out.iter().find(|a| a.id == 1).unwrap();
        assert_eq!(first.name, "terminus-linux-x86_64.tar.gz");
        assert_eq!(first.size, 2_400_000);
        assert_eq!(first.release_tag, "v0.1.1");
        let second = out.iter().find(|a| a.id == 2).unwrap();
        assert_eq!(second.release_tag, "v0.1.0");
    }

    #[tokio::test]
    async fn a_release_with_no_assets_produces_nothing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/exec-d/terminus/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "tag_name": "v0.1.2", "assets": [] }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = assets(&client, "exec-d", "terminus").await.unwrap();

        assert!(out.is_empty());
    }

    /// An asset with no addressable id is one we must not offer to delete —
    /// coercing it to id 0 would collide every such asset into one selection
    /// slot, in a tool with no undo. Same reasoning as
    /// `api::caches::an_item_without_a_usable_id_is_dropped`.
    #[tokio::test]
    async fn an_asset_without_a_usable_id_is_dropped() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/exec-d/terminus/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "tag_name": "v0.1.1",
                    "assets": [
                        { "id": 1, "name": "ok.tar.gz", "size": 100,
                          "created_at": "2026-06-01T00:00:00Z" },
                        { "name": "no-id.tar.gz", "size": 200,
                          "created_at": "2026-06-01T00:00:00Z" }
                    ]
                }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = assets(&client, "exec-d", "terminus").await.unwrap();

        assert_eq!(out.len(), 1, "the id-less asset must be dropped");
        assert_eq!(out[0].id, 1);
    }

    #[tokio::test]
    async fn assets_follows_release_pagination() {
        let server = MockServer::start().await;
        let full: Vec<serde_json::Value> = (1..=100)
            .map(|n| {
                serde_json::json!({
                    "tag_name": format!("v0.{n}.0"),
                    "assets": [
                        { "id": n, "name": format!("asset-{n}.tar.gz"), "size": 10,
                          "created_at": "2026-06-01T00:00:00Z" }
                    ]
                })
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/repos/exec-d/terminus/releases"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(full))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/exec-d/terminus/releases"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "tag_name": "v0.101.0",
                    "assets": [
                        { "id": 101, "name": "asset-101.tar.gz", "size": 10,
                          "created_at": "2026-06-01T00:00:00Z" }
                    ]
                }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = assets(&client, "exec-d", "terminus").await.unwrap();

        assert_eq!(
            out.len(),
            101,
            "the second page of releases must not be dropped"
        );
        assert!(out.iter().any(|a| a.id == 101));
    }

    #[tokio::test]
    async fn delete_asset_targets_the_release_assets_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/exec-d/terminus/releases/assets/9"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        delete_asset(&client, "exec-d", "terminus", 9)
            .await
            .unwrap();
    }
}
