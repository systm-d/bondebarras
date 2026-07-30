//! Workflow run endpoints. Deleting a run also drops its logs and artifacts.

use super::Client;
use super::caches::age_days;
use crate::model::{Resource, ResourceKind};
use anyhow::Result;

pub async fn list(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>> {
    let v = client
        .get_json(&format!("/repos/{owner}/{repo}/actions/runs?per_page=100"))
        .await?;

    Ok(v["workflow_runs"]
        .as_array()
        .map(|items| {
            items
                .iter()
                // An item with no addressable id is one we must not offer to
                // delete — coercing it to id 0 would risk colliding with a
                // real run 0 and deleting the wrong thing.
                .filter_map(|item| {
                    let id = item["id"].as_u64()?;
                    Some(Resource {
                        kind: ResourceKind::WorkflowRun,
                        id,
                        label: format!(
                            "{} #{}",
                            item["name"].as_str().unwrap_or("workflow"),
                            item["run_number"].as_u64().unwrap_or(0)
                        ),
                        // The API reports no size for a run. The reclaimed space
                        // comes from the logs and artifacts deleted alongside it.
                        size_bytes: 0,
                        age_days: age_days(item["created_at"].as_str()),
                        git_ref: item["head_branch"]
                            .as_str()
                            .map(|b| format!("refs/heads/{b}")),
                        stale_pr: false,
                        // No equivalent of a live reference by name for a run.
                        protected: false,
                        // Only a `Branch` row carries a classification.
                        branch_class: None,
                        safety: crate::safety::Safety::Keep,
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub async fn delete(client: &Client, owner: &str, repo: &str, id: u64) -> Result<()> {
    client
        .delete(&format!("/repos/{owner}/{repo}/actions/runs/{id}"))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn list_labels_runs_with_their_workflow_and_number() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/josephine/actions/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 1,
                "workflow_runs": [
                    { "id": 4471, "name": "CI", "run_number": 128,
                      "head_branch": "main", "created_at": "2026-05-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = list(&client, "systm-d", "josephine").await.unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ResourceKind::WorkflowRun);
        assert_eq!(items[0].id, 4471);
        assert_eq!(items[0].label, "CI #128");
        // The runs endpoint reports no size; the gain comes from the logs and
        // artifacts GitHub drops along with the run.
        assert_eq!(items[0].size_bytes, 0);
    }

    #[tokio::test]
    async fn an_item_without_a_usable_id_is_dropped() {
        // An item we cannot address is an item we must not offer to delete.
        // Coercing a missing id to 0 would collide every such item into one
        // selection slot.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/josephine/actions/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "workflow_runs": [
                    { "id": 4471, "name": "CI", "run_number": 128,
                      "head_branch": "main", "created_at": "2026-05-01T00:00:00Z" },
                    { "name": "CI", "run_number": 129,
                      "head_branch": "main", "created_at": "2026-05-01T00:00:00Z" },
                    { "id": "77", "name": "CI", "run_number": 130,
                      "head_branch": "main", "created_at": "2026-05-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = list(&client, "systm-d", "josephine").await.unwrap();

        assert_eq!(items.len(), 1, "only the addressable item survives");
        assert_eq!(items[0].id, 4471);
    }
}
