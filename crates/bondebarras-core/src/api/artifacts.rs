//! Actions artifact endpoints.

use super::Client;
use super::caches::age_days;
use crate::model::{Resource, ResourceKind};
use anyhow::Result;

pub async fn list(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>> {
    let v = client
        .get_json(&format!(
            "/repos/{owner}/{repo}/actions/artifacts?per_page=100"
        ))
        .await?;

    Ok(v["artifacts"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let name = item["name"].as_str().unwrap_or_default();
                    let expired = item["expired"].as_bool().unwrap_or(false);
                    Resource {
                        kind: ResourceKind::Artifact,
                        id: item["id"].as_u64().unwrap_or(0),
                        // An expired artifact still occupies a row until it is
                        // deleted, so it is worth showing — and worth marking.
                        label: if expired {
                            format!("{name} (expiré)")
                        } else {
                            name.to_string()
                        },
                        size_bytes: item["size_in_bytes"].as_u64().unwrap_or(0),
                        age_days: age_days(item["created_at"].as_str()),
                        git_ref: None,
                        stale_pr: false,
                    }
                })
                .collect()
        })
        .unwrap_or_default())
}

pub async fn delete(client: &Client, owner: &str, repo: &str, id: u64) -> Result<()> {
    client
        .delete(&format!("/repos/{owner}/{repo}/actions/artifacts/{id}"))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn list_marks_expired_artifacts_in_the_label() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/josephine/actions/artifacts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 2,
                "artifacts": [
                    { "id": 1, "name": "github-pages", "size_in_bytes": 1_112_447,
                      "expired": false, "created_at": "2026-07-28T12:35:26Z" },
                    { "id": 2, "name": "github-pages", "size_in_bytes": 1_112_275,
                      "expired": true, "created_at": "2026-07-27T09:50:08Z" }
                ]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = list(&client, "systm-d", "josephine").await.unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, ResourceKind::Artifact);
        assert_eq!(items[0].label, "github-pages");
        assert_eq!(items[1].label, "github-pages (expiré)");
        assert_eq!(items[1].size_bytes, 1_112_275);
    }
}
