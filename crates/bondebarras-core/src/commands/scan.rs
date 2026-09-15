//! Non-interactive overview.

use crate::api::Client;
use crate::billing;
use crate::model::{OrgSummary, human_size};
use crate::scan;
use anyhow::Result;

/// Print the stage-1 overview.
///
/// With `json`, **stdout carries JSON and nothing else** — every progress or
/// diagnostic line goes to stderr, so a `| jq` pipeline always parses.
pub async fn run(client: &Client, orgs: &[String], json: bool) -> Result<()> {
    let summaries = scan::overview(client, orgs).await;

    if json {
        let month = billing::month_of(chrono::Utc::now());
        println!(
            "{}",
            serde_json::to_string_pretty(&overview_json(&summaries, &month))?
        );
    } else {
        for o in &summaries {
            println!(
                "{:<24} {:>10}  ({} caches)",
                o.login,
                human_size(o.cache_bytes),
                o.cache_count
            );
        }
    }
    Ok(())
}

/// The `--json` document: one object per organization.
///
/// Pure — no network, no stdout — so every field can be asserted on directly.
/// A figure the API did not give is `null`, never a default:
/// `minutes_allowance` is `null` for an unreadable or unknown plan, not the
/// Free plan's 2 000, and storage is `null` when billing is unreadable, not 0.
/// `month` is the month storage is read for; `run` passes the current UTC
/// month, and the document names it (`billing_month`).
pub fn overview_json(summaries: &[OrgSummary], month: &str) -> serde_json::Value {
    serde_json::Value::Array(summaries.iter().map(|o| org_json(o, month)).collect())
}

fn org_json(o: &OrgSummary, month: &str) -> serde_json::Value {
    let plan = o.plan.as_deref();
    serde_json::json!({
        "org": o.login,
        "cache_bytes": o.cache_bytes,
        "cache_count": o.cache_count,
        "billing_readable": o.billing.is_some(),
        "plan": o.plan,
        "minutes_allowance": billing::included_minutes_for(plan),
        "billing_month": month,
        "storage_gbh": o.billing.as_ref().map(|b| b.storage_gbh(month)),
        "storage_allowance_gbh": billing::storage_quota(plan, month).map(|q| q.gbh),
        "repos": o.repos.iter().map(|r| serde_json::json!({
            "name": r.name,
            "cache_bytes": r.cache_bytes,
            "cache_count": r.cache_count,
            "storage_gbh": o.billing.as_ref().map(|b| b.storage_gbh_for_repo(month, &r.name)),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::{BillingReport, UsageItem};
    use crate::model::RepoSummary;

    fn org(login: &str, plan: Option<&str>) -> OrgSummary {
        OrgSummary {
            login: login.into(),
            plan: plan.map(str::to_string),
            ..Default::default()
        }
    }

    fn repo(name: &str) -> RepoSummary {
        RepoSummary {
            name: name.into(),
            cache_bytes: 0,
            cache_count: 0,
            private: true,
            age_days: 0,
            class: crate::repos::RepoClass::Archivable,
        }
    }

    fn storage(month: &str, gbh: f64, repo: &str) -> UsageItem {
        UsageItem {
            month: month.into(),
            product: "actions".into(),
            sku: "Actions storage".into(),
            quantity: gbh,
            unit_type: "GigabyteHours".into(),
            gross: 0.0,
            discount: 0.0,
            net: 0.0,
            repo: repo.into(),
        }
    }

    fn number(v: &serde_json::Value) -> f64 {
        v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
    }

    #[test]
    fn scan_json_carries_plan_and_minutes_allowance() {
        let v = overview_json(
            &[
                org("exec-d", Some("team")),
                org("le-vilain-petit-dev", None),
            ],
            "2026-09",
        );

        assert_eq!(v[0]["org"], "exec-d");
        assert_eq!(v[0]["plan"], "team");
        // Team's 3 000 — not the 2 000 every org used to be measured against.
        assert_eq!(v[0]["minutes_allowance"], 3_000);
        assert!(v[1]["plan"].is_null(), "got: {}", v[1]);
        assert!(v[1]["minutes_allowance"].is_null(), "got: {}", v[1]);
        // The fields that existed before stay.
        assert_eq!(v[1]["billing_readable"], false);
        assert!(v[1]["repos"].as_array().is_some_and(|r| r.is_empty()));
    }

    #[test]
    fn scan_json_carries_storage_figures() {
        let exec_d = OrgSummary {
            login: "exec-d".into(),
            plan: Some("free".into()),
            repos: vec![repo("disconnected"), repo("quiet")],
            billing: Some(BillingReport {
                items: vec![
                    storage("2026-09", 359.88, "disconnected"),
                    storage("2026-09", 11.21, "ptitjardinier-app"),
                    // August must not leak into September's figures.
                    storage("2026-08", 100.0, "disconnected"),
                ],
            }),
            ..Default::default()
        };
        let unreadable = OrgSummary {
            login: "le-vilain-petit-dev".into(),
            repos: vec![repo("private-thing")],
            ..Default::default()
        };

        let v = overview_json(&[exec_d, unreadable], "2026-09");

        assert_eq!(v[0]["billing_month"], "2026-09");
        assert!(
            (number(&v[0]["storage_gbh"]) - 371.09).abs() < 1e-9,
            "got: {}",
            v[0]
        );
        // Free's 0.5 GB × September's 720 hours.
        assert!(
            (number(&v[0]["storage_allowance_gbh"]) - 360.0).abs() < 1e-9,
            "got: {}",
            v[0]
        );
        assert!((number(&v[0]["repos"][0]["storage_gbh"]) - 359.88).abs() < 1e-9);
        // A readable report that lists nothing for this repository: a real zero.
        assert!(number(&v[0]["repos"][1]["storage_gbh"]).abs() < 1e-9);
        // Unreadable billing, unknown plan: nulls, never zeros.
        assert!(v[1]["storage_gbh"].is_null(), "got: {}", v[1]);
        assert!(v[1]["storage_allowance_gbh"].is_null(), "got: {}", v[1]);
        assert!(v[1]["repos"][0]["storage_gbh"].is_null(), "got: {}", v[1]);
    }
}
