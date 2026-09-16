//! Organization budgets — read-only, permanently.

use super::Client;
use crate::billing::Budget;

/// GitHub's maximum page size for this endpoint (its default is 10).
const PAGE_SIZE: usize = 100;

/// Hard stop on pagination: a thousand budgets. Past it, the listing reads
/// as unreadable rather than truncated — see `fetch`.
const MAX_BUDGET_PAGES: u32 = 10;

/// Every budget of an organization, or `None` when they cannot be read.
///
/// GitHub reserves the endpoint to organization admins and billing managers.
/// Its documentation announces 403, 404 or 500 for anyone else; the real
/// answer on three orgs, on 2026-09-10, was a 400 `Unable to get budgets.`.
/// Any failure degrades the same way, like `api::billing::fetch`.
///
/// `None` — unreadable — is never collapsed into an empty list — no budget.
/// A failed page, an entry missing one of the five fields read, and a tenth
/// page still announcing a next one all read as unreadable: each could hide
/// the Actions budget, and "no budget: overage billed" would then be said of
/// an org GitHub actually blocks. An absent `has_next_page` is the last page.
pub async fn fetch(client: &Client, org: &str) -> Option<Vec<Budget>> {
    let mut out = Vec::new();
    for page in 1..=MAX_BUDGET_PAGES {
        let v = client
            .get_json(&format!(
                "/organizations/{org}/settings/billing/budgets?per_page={PAGE_SIZE}&page={page}"
            ))
            .await
            .ok()?;
        for item in v["budgets"].as_array()? {
            out.push(parse_budget(item)?);
        }
        if !v["has_next_page"].as_bool().unwrap_or(false) {
            return Some(out);
        }
    }
    None
}

/// One budget, or `None` when any of the five fields read is missing or
/// mistyped.
fn parse_budget(item: &serde_json::Value) -> Option<Budget> {
    Some(Budget {
        budget_type: item["budget_type"].as_str()?.to_string(),
        sku: item["budget_product_sku"].as_str()?.to_string(),
        scope: item["budget_scope"].as_str()?.to_string(),
        amount: item["budget_amount"].as_u64()?,
        blocking: item["prevent_further_usage"].as_bool()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const BUDGETS: &str = "/organizations/exec-d/settings/billing/budgets";

    /// exec-d's real response on 2026-09-10, identifiers masked as in #14.
    fn exec_d_response() -> serde_json::Value {
        serde_json::json!({
          "budgets": [
            {"id": "…", "budget_type": "ProductPricing", "budget_product_sku": "codespaces", "budget_scope": "organization", "budget_amount": 0, "prevent_further_usage": true, "budget_entity_name": "exec-d", "budget_alerting": {"will_alert": true, "alert_recipients": ["…"]}},
            {"id": "…", "budget_type": "ProductPricing", "budget_product_sku": "packages", "budget_scope": "organization", "budget_amount": 0, "prevent_further_usage": true, "budget_entity_name": "exec-d", "budget_alerting": {"will_alert": true, "alert_recipients": ["…"]}},
            {"id": "…", "budget_type": "ProductPricing", "budget_product_sku": "actions", "budget_scope": "organization", "budget_amount": 0, "prevent_further_usage": true, "budget_entity_name": "exec-d", "budget_alerting": {"will_alert": true, "alert_recipients": ["…"]}},
            {"id": "…", "budget_type": "ProductPricing", "budget_product_sku": "git_lfs", "budget_scope": "organization", "budget_amount": 0, "prevent_further_usage": true, "budget_entity_name": "exec-d", "budget_alerting": {"will_alert": true, "alert_recipients": ["…"]}}
          ],
          "has_next_page": false,
          "total_count": 4
        })
    }

    fn budget_json(sku: &str, amount: u64) -> serde_json::Value {
        serde_json::json!({
            "budget_type": "ProductPricing", "budget_product_sku": sku,
            "budget_scope": "organization", "budget_amount": amount,
            "prevent_further_usage": true
        })
    }

    #[tokio::test]
    async fn budgets_fetch_maps_every_budget_of_the_sample() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(BUDGETS))
            .respond_with(ResponseTemplate::new(200).set_body_json(exec_d_response()))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let budgets = fetch(&client, "exec-d").await.expect("a 200 is readable");

        assert_eq!(budgets.len(), 4);
        assert_eq!(budgets[2].sku, "actions");
        assert_eq!(budgets[2].budget_type, "ProductPricing");
        assert_eq!(budgets[2].scope, "organization");
        assert_eq!((budgets[2].amount, budgets[2].blocking), (0, true));
        let actions = crate::billing::actions_budget(&budgets).expect("one Actions budget");
        assert_eq!(actions.sku, "actions");
    }

    /// Observed on the three orgs the account does not own: 400, although
    /// the documentation announces 403, 404 or 500. Any non-2xx degrades.
    #[tokio::test]
    async fn budgets_fetch_degrades_a_400_to_unreadable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/organizations/le-vilain-petit-dev/settings/billing/budgets",
            ))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({ "message": "Unable to get budgets." })),
            )
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "le-vilain-petit-dev").await.is_none());
    }

    /// The Actions budget sits on page 2: a single-page read finds only
    /// codespaces, and would call exec-d an org with no Actions budget.
    #[tokio::test]
    async fn budgets_fetch_reads_the_next_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(BUDGETS))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "budgets": [budget_json("codespaces", 0)],
                "has_next_page": true,
                "total_count": 2
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(BUDGETS))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "budgets": [budget_json("actions", 5)],
                "has_next_page": false,
                "total_count": 2
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let budgets = fetch(&client, "exec-d").await.expect("readable");

        assert_eq!(budgets.len(), 2);
        assert_eq!(
            crate::billing::actions_budget(&budgets).map(|b| b.amount),
            Some(5)
        );
    }

    /// Dropping the malformed entry would leave `Some([codespaces])` — "no
    /// Actions budget, overage billed" — for an org whose Actions budget
    /// merely lacked an amount. Unreadable is the only honest answer.
    #[tokio::test]
    async fn budgets_fetch_treats_a_malformed_entry_as_unreadable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(BUDGETS))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "budgets": [
                    budget_json("codespaces", 0),
                    { "budget_type": "ProductPricing", "budget_product_sku": "actions",
                      "budget_scope": "organization", "prevent_further_usage": true }
                ],
                "has_next_page": false
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "exec-d").await.is_none());
    }

    /// A tenth page that still announces a next one: a partial list could
    /// miss the Actions budget, so it reads as unreadable, not as truncated.
    #[tokio::test]
    async fn budgets_fetch_gives_up_rather_than_truncate() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(BUDGETS))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "budgets": [budget_json("codespaces", 0)],
                "has_next_page": true
            })))
            .expect(10)
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "exec-d").await.is_none());
    }
}
