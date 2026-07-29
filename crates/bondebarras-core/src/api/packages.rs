//! Container package version endpoints.
//!
//! Not in octocrab's typed surface, so this goes through
//! `Client::get_json_or_missing` / `Client::delete`, like the other raw
//! endpoints in this module.

use super::Client;
use super::caches::age_days;
use crate::packages::PackageVersion;
use anyhow::Result;

/// Every version of a repository's homonymous container package.
///
/// Most repositories publish no image at all, so a 404 here is the normal
/// case: it yields an empty list, not an error.
pub async fn versions(client: &Client, org: &str, package: &str) -> Result<Vec<PackageVersion>> {
    let Some(v) = client
        .get_json_or_missing(&format!(
            "/orgs/{org}/packages/container/{package}/versions?per_page=100"
        ))
        .await?
    else {
        return Ok(Vec::new());
    };

    Ok(v.as_array()
        .map(|items| {
            items
                .iter()
                // An item with no addressable id is one we must not offer to
                // delete — coercing it to id 0 would collide every such item
                // into one selection slot, in a tool with no undo.
                .filter_map(|item| {
                    let id = item["id"].as_u64()?;
                    let tags = item["metadata"]["container"]["tags"]
                        .as_array()
                        .map(|tags| {
                            tags.iter()
                                .filter_map(|t| t.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(PackageVersion {
                        id,
                        digest: item["name"].as_str().unwrap_or_default().to_string(),
                        tags,
                        age_days: age_days(item["created_at"].as_str()),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub async fn delete_version(client: &Client, org: &str, package: &str, id: u64) -> Result<()> {
    client
        .delete(&format!(
            "/orgs/{org}/packages/container/{package}/versions/{id}"
        ))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn versions_carry_their_digest_and_tags() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/repolens/versions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": 862511118,
                  "name": "sha256:1d7018e5672547cced06883706367832e5f1be5fa90bc2038ad308e19958e80e",
                  "created_at": "2026-05-13T16:11:32Z",
                  "metadata": { "container": { "tags": ["sha256-1a65eb30f0e36fc41bb07724b11e53ada5e810382f39143698b00c470f019b80"] } } },
                { "id": 862511085,
                  "name": "sha256:9a26c708801010123223adb5e73ff703aca86c15e19b30124ede5628a1e185826",
                  "created_at": "2026-05-13T16:11:30Z",
                  "metadata": { "container": { "tags": [] } } },
                // No usable id: must be dropped, never coerced to 0.
                { "name": "sha256:deadbeef", "created_at": "2026-05-13T16:11:29Z",
                  "metadata": { "container": { "tags": [] } } }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = versions(&client, "systm-d", "repolens").await.unwrap();

        assert_eq!(out.len(), 2, "the version with no id must be dropped");
        assert_eq!(out[0].id, 862511118);
        assert_eq!(out[0].tags.len(), 1);
        assert_eq!(out[1].tags.len(), 0);
        assert!(out[1].digest.starts_with("sha256:9a26c7"));
    }

    #[tokio::test]
    async fn a_repo_without_a_package_yields_nothing_not_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/no-such/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        // Most repositories publish no image at all. A 404 is the normal case,
        // not a failure to report.
        assert!(
            versions(&client, "systm-d", "no-such")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn delete_version_targets_the_versions_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path(
                "/orgs/systm-d/packages/container/repolens/versions/862511085",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        delete_version(&client, "systm-d", "repolens", 862511085)
            .await
            .unwrap();
    }
}
