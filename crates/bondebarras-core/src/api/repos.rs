//! Organization repository listing.

use super::Client;
use anyhow::Result;

pub async fn list(client: &Client, org: &str) -> Result<Vec<String>> {
    let v = client
        .get_json(&format!("/orgs/{org}/repos?per_page=100"))
        .await?;

    Ok(v.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["name"].as_str().map(str::to_string))
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
                { "name": "josephine" },
                { "name": "claudine" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let names = list(&client, "systm-d").await.unwrap();

        assert_eq!(names, vec!["josephine".to_string(), "claudine".to_string()]);
    }
}
