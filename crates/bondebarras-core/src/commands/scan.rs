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
        println!(
            "{}",
            serde_json::to_string_pretty(&overview_json(&summaries))?
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
/// Free plan's 2 000.
pub fn overview_json(summaries: &[OrgSummary]) -> serde_json::Value {
    serde_json::Value::Array(summaries.iter().map(org_json).collect())
}

fn org_json(o: &OrgSummary) -> serde_json::Value {
    serde_json::json!({
        "org": o.login,
        "cache_bytes": o.cache_bytes,
        "cache_count": o.cache_count,
        "billing_readable": o.billing.is_some(),
        "plan": o.plan,
        "minutes_allowance": billing::included_minutes_for(o.plan.as_deref()),
        "repos": o.repos.iter().map(|r| serde_json::json!({
            "name": r.name,
            "cache_bytes": r.cache_bytes,
            "cache_count": r.cache_count,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn org(login: &str, plan: Option<&str>) -> OrgSummary {
        OrgSummary {
            login: login.into(),
            plan: plan.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn scan_json_carries_plan_and_minutes_allowance() {
        let v = overview_json(&[
            org("exec-d", Some("team")),
            org("le-vilain-petit-dev", None),
        ]);

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
}
