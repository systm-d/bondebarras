//! The organization usage report.
//!
//! The legacy billing endpoints (`/orgs/{org}/settings/billing/actions`,
//! `/packages`, `/shared-storage`) all return **410 Gone** — GitHub moved to a
//! unified billing platform. This one replaces them and is richer: it reports
//! per repository × per SKU × per month.

use super::Client;
use crate::billing::{BillingReport, UsageItem};

/// Fetch an org's usage report, or `None` when it is not readable.
///
/// A 403 means the user is not an owner of that org. That is not fatal: the
/// org stays navigable for caches, artifacts and runs, and only the billing
/// column is marked unavailable.
pub async fn fetch(client: &Client, org: &str) -> Option<BillingReport> {
    let v = client
        .get_json(&format!("/organizations/{org}/settings/billing/usage"))
        .await
        .ok()?;

    let items = v["usageItems"]
        .as_array()?
        .iter()
        .map(|i| {
            let date = i["date"].as_str().unwrap_or_default();
            UsageItem {
                // "2026-07-01T00:00:00Z" -> "2026-07". Anything shorter keeps
                // whatever is there rather than panicking on a slice.
                month: date.get(..7).unwrap_or(date).to_string(),
                product: i["product"].as_str().unwrap_or_default().to_string(),
                sku: i["sku"].as_str().unwrap_or_default().to_string(),
                quantity: i["quantity"].as_f64().unwrap_or(0.0),
                unit_type: i["unitType"].as_str().unwrap_or_default().to_string(),
                gross: i["grossAmount"].as_f64().unwrap_or(0.0),
                discount: i["discountAmount"].as_f64().unwrap_or(0.0),
                net: i["netAmount"].as_f64().unwrap_or(0.0),
                repo: i["repositoryName"].as_str().unwrap_or_default().to_string(),
            }
        })
        .collect();

    Some(BillingReport { items })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn fetch_maps_the_usage_items() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/organizations/systm-d/settings/billing/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "usageItems": [{
                    "date": "2026-07-01T00:00:00Z",
                    "product": "actions",
                    "sku": "Actions Linux",
                    "quantity": 3311.0,
                    "unitType": "Minutes",
                    "pricePerUnit": 0.006,
                    "grossAmount": 19.866,
                    "discountAmount": 19.866,
                    "netAmount": 0.0,
                    "organizationName": "systm-d",
                    "repositoryName": "josephine"
                }]
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let report = fetch(&client, "systm-d")
            .await
            .expect("200 yields a report");

        assert_eq!(report.items.len(), 1);
        let it = &report.items[0];
        assert_eq!(it.month, "2026-07");
        assert_eq!(it.sku, "Actions Linux");
        assert_eq!(it.repo, "josephine");
        assert!((it.gross - 19.866).abs() < 1e-9);
        assert!((it.net - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn a_403_degrades_to_none_rather_than_failing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/organizations/le-vilain-petit-dev/settings/billing/usage",
            ))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        assert!(fetch(&client, "le-vilain-petit-dev").await.is_none());
    }
}
