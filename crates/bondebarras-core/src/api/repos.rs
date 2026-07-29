//! Organization repository listing.

use super::Client;
use anyhow::Result;

/// A repository as stage 1 sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub name: String,
    /// Private repos draw on the org's Actions allowance; public ones are
    /// free and unlimited. GitHub's usage report discounts both identically,
    /// so visibility is the only way to tell them apart.
    pub private: bool,
}

pub async fn list(client: &Client, org: &str) -> Result<Vec<RepoRef>> {
    let v = client
        .get_json(&format!("/orgs/{org}/repos?per_page=100"))
        .await?;

    Ok(v.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let name = item["name"].as_str()?.to_string();
                    // A missing field defaults to public: under-reporting the
                    // gauge is the safe direction for a number the user may
                    // act on, unlike inventing consumption that never
                    // happened.
                    let private = item["private"].as_bool().unwrap_or(false);
                    Some(RepoRef { name, private })
                })
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
    async fn list_returns_short_repo_names() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "josephine", "private": false },
                { "name": "claudine", "private": false }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let names: Vec<String> = list(&client, "systm-d")
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.name)
            .collect();

        assert_eq!(names, vec!["josephine".to_string(), "claudine".to_string()]);
    }

    /// Locks finding 1: visibility is the only field that lets `billing`
    /// distinguish "public, free forever" from "private, covered by the
    /// allowance" once GitHub has discounted both identically. A wrong
    /// implementation that drops the field, or always reports `false`, must
    /// fail this on the private repo.
    #[tokio::test]
    async fn list_carries_the_private_flag() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/SecondBrain-io/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "monolith-back", "private": true },
                { "name": "public-site", "private": false }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let repos = list(&client, "SecondBrain-io").await.unwrap();

        assert_eq!(
            repos,
            vec![
                RepoRef {
                    name: "monolith-back".to_string(),
                    private: true
                },
                RepoRef {
                    name: "public-site".to_string(),
                    private: false
                },
            ]
        );
    }

    /// A repo listing that omits `private` (an unexpected API shape) must
    /// default to public, not private: under-reporting the gauge is the safe
    /// direction, over-reporting invents consumption that never happened.
    #[tokio::test]
    async fn list_defaults_a_missing_private_field_to_public() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "no-visibility-field" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let repos = list(&client, "systm-d").await.unwrap();

        assert!(!repos[0].private);
    }
}
