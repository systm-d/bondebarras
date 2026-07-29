//! Repository archiving.
//!
//! Archiving turns a repository read-only, GitHub's own Actions included —
//! which is why it belongs in this tool at all (see `crate::repos`'s module
//! doc): an archived repository stops producing the caches, artifacts and
//! workflow runs v0.1 exists to clean, instead of mopping them up forever.
//! It frees no bytes of its own — the repository's size is unchanged — and
//! it is reversible on GitHub's side, unlike everything else this crate
//! deletes.

use super::Client;
use anyhow::Result;

/// Archive one repository: `PATCH /repos/{owner}/{repo}` with
/// `{"archived": true}`.
pub async fn archive(client: &Client, owner: &str, repo: &str) -> Result<()> {
    client
        .patch(
            &format!("/repos/{owner}/{repo}"),
            &serde_json::json!({ "archived": true }),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn archive_sends_the_archived_flag() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/repos/maxds-lyon/lokiprint"))
            .and(body_json(serde_json::json!({ "archived": true })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        archive(&client, "maxds-lyon", "lokiprint").await.unwrap();
    }

    #[tokio::test]
    async fn a_403_is_an_error_not_a_silent_success() {
        // The user is not an admin. Reporting success would tell them a repo
        // is read-only when it is not.
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/repos/maxds-lyon/lokiprint"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let err = archive(&client, "maxds-lyon", "lokiprint")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("403"));
    }
}
