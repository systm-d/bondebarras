//! Non-interactive overview.

use crate::api::Client;
use crate::model::human_size;
use crate::scan;
use anyhow::Result;

/// Print the stage-1 overview.
///
/// With `json`, **stdout carries JSON and nothing else** — every progress or
/// diagnostic line goes to stderr, so a `| jq` pipeline always parses.
pub async fn run(client: &Client, orgs: &[String], json: bool) -> Result<()> {
    let summaries = scan::overview(client, orgs).await;

    if json {
        let value: Vec<serde_json::Value> = summaries
            .iter()
            .map(|o| {
                serde_json::json!({
                    "org": o.login,
                    "cache_bytes": o.cache_bytes,
                    "cache_count": o.cache_count,
                    "billing_readable": o.billing.is_some(),
                    "repos": o.repos.iter().map(|r| serde_json::json!({
                        "name": r.name,
                        "cache_bytes": r.cache_bytes,
                        "cache_count": r.cache_count,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&value)?);
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
