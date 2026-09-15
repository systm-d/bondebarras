//! Organization details — today, only the plan name.

use super::Client;

/// The organization's plan name (`free`, `team`, `enterprise`), or `None`.
///
/// GitHub only includes `plan` in `GET /orgs/{org}` for an owner of the org.
/// A missing field, a refusal, or any other failure all read the same way —
/// plan unknown — and never drop the org: the plan only decides whether a
/// percentage can be shown, the same degradation as `api::billing::fetch`.
pub async fn plan(client: &Client, org: &str) -> Option<String> {
    let v = client.get_json(&format!("/orgs/{org}")).await.ok()?;
    v["plan"]["name"].as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn plan_reads_the_orgs_plan_name() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "login": "exec-d",
                "plan": { "name": "team" }
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert_eq!(plan(&client, "exec-d").await.as_deref(), Some("team"));
    }

    /// Not an owner: GitHub still answers 200 with the organization, minus
    /// `plan`. That must read as "unknown", not as any plan in particular.
    #[tokio::test]
    async fn plan_is_none_when_the_field_is_absent() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/le-vilain-petit-dev"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "login": "le-vilain-petit-dev"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(plan(&client, "le-vilain-petit-dev").await.is_none());
    }

    /// Modelled on `api::billing::a_403_degrades_to_none_rather_than_failing`.
    #[tokio::test]
    async fn plan_is_none_when_the_org_read_is_refused() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/maxds-lyon-archives"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(plan(&client, "maxds-lyon-archives").await.is_none());
    }
}
