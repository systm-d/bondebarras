//! The organization's artifact and log retention setting — read, never written.

use super::Client;
use crate::model::ArtifactRetention;

/// The retention setting, or `None` when it cannot be read.
///
/// GitHub requires the classic `admin:org` scope or the fine-grained
/// "Actions policies" permission, and bondebarras requires neither: a
/// refusal reads as "rétention illisible" and never drops the org. A body
/// without an integer `days` reads the same way — GitHub's 90-day default is
/// never assumed. The `PUT` on the same path exists and is deliberately not
/// called: this round is read-only.
pub async fn fetch(client: &Client, org: &str) -> Option<ArtifactRetention> {
    let v = client
        .get_json(&format!(
            "/orgs/{org}/actions/permissions/artifact-and-log-retention"
        ))
        .await
        .ok()?;
    Some(ArtifactRetention {
        days: u32::try_from(v["days"].as_u64()?).ok()?,
        maximum_allowed_days: v["maximum_allowed_days"]
            .as_u64()
            .and_then(|d| u32::try_from(d).ok()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn retention_fetch_maps_days_and_maximum() {
        let server = MockServer::start().await;
        // exec-d's real response, before it moved to 7 days.
        Mock::given(method("GET"))
            .and(path(
                "/orgs/exec-d/actions/permissions/artifact-and-log-retention",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "days": 90,
                "maximum_allowed_days": 400
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert_eq!(
            fetch(&client, "exec-d").await,
            Some(ArtifactRetention {
                days: 90,
                maximum_allowed_days: Some(400)
            })
        );
    }

    /// A token without `admin:org` — the README's own scopes: refused, and
    /// never fatal. Modelled on `api::billing::a_403_degrades_to_none_rather_than_failing`.
    #[tokio::test]
    async fn retention_fetch_degrades_a_refusal_to_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/orgs/systm-d/actions/permissions/artifact-and-log-retention",
            ))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "systm-d").await.is_none());
    }

    /// No `days`, no retention: GitHub's 90-day default is never assumed.
    #[tokio::test]
    async fn retention_fetch_never_assumes_a_default() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/orgs/exec-d/actions/permissions/artifact-and-log-retention",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "maximum_allowed_days": 400 })),
            )
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "exec-d").await.is_none());
    }
}
