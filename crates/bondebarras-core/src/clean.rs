//! Deletion planning and execution.
//!
//! Nothing here is reversible on GitHub's side, so there is no trash and no
//! undo — promising either would be a lie. What we offer instead is an
//! accurate recap before, and a per-item verdict after.

use crate::api::{Client, artifacts, caches, runs};
use crate::model::{Resource, ResourceKind, RiskTier, human_size, risk_tier};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{Duration, sleep};

/// Deletions are spaced out: GitHub's secondary rate limit rejects a burst,
/// and a purge of 132 caches is exactly such a burst.
const DELETE_SPACING: Duration = Duration::from_millis(120);

/// A confirmed selection, scoped to one repository.
pub struct Plan {
    pub items: Vec<Resource>,
    pub owner: String,
    pub repo: String,
}

impl Plan {
    /// The friction the plan must go through: the most severe tier it contains.
    pub fn tier(&self) -> RiskTier {
        self.items
            .iter()
            .map(|i| risk_tier(i.kind))
            .max()
            .unwrap_or(RiskTier::Low)
    }

    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|i| i.size_bytes).sum()
    }

    /// User-facing recap shown in the confirmation modal.
    pub fn summary(&self) -> String {
        format!(
            "{} élément(s) · {}",
            self.items.len(),
            human_size(self.total_bytes())
        )
    }
}

/// Emitted as the deletion runs, so the TUI stays responsive.
///
/// `Done` and `Failed` carry `kind` alongside `id` because ids are only
/// unique within one resource kind: without it, the event loop cannot tell
/// which row to remove from `app.resources` when a cache and an artifact
/// happen to share an id.
#[derive(Debug, Clone)]
pub enum Progress {
    Done {
        kind: ResourceKind,
        id: u64,
    },
    Failed {
        kind: ResourceKind,
        id: u64,
        reason: String,
    },
    Finished {
        freed: u64,
        failures: usize,
    },
}

/// Delete every item of the plan, reporting each outcome as it lands.
pub async fn execute(client: &Client, plan: Plan, tx: UnboundedSender<Progress>) {
    let mut freed = 0_u64;
    let mut failures = 0_usize;

    for item in &plan.items {
        let result = match item.kind {
            ResourceKind::Cache => caches::delete(client, &plan.owner, &plan.repo, item.id).await,
            ResourceKind::Artifact => {
                artifacts::delete(client, &plan.owner, &plan.repo, item.id).await
            }
            ResourceKind::WorkflowRun => {
                runs::delete(client, &plan.owner, &plan.repo, item.id).await
            }
        };

        match result {
            Ok(()) => {
                freed += item.size_bytes;
                let _ = tx.send(Progress::Done {
                    kind: item.kind,
                    id: item.id,
                });
            }
            Err(e) => {
                failures += 1;
                let _ = tx.send(Progress::Failed {
                    kind: item.kind,
                    id: item.id,
                    reason: e.to_string(),
                });
            }
        }

        sleep(DELETE_SPACING).await;
    }

    let _ = tx.send(Progress::Finished { freed, failures });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: ResourceKind, id: u64, size: u64) -> Resource {
        Resource {
            kind,
            id,
            label: format!("item-{id}"),
            size_bytes: size,
            age_days: 30,
            git_ref: None,
            stale_pr: false,
        }
    }

    fn plan(items: Vec<Resource>) -> Plan {
        Plan {
            items,
            owner: "systm-d".into(),
            repo: "claudine".into(),
        }
    }

    #[test]
    fn a_plan_takes_the_highest_tier_of_its_items() {
        let p = plan(vec![
            item(ResourceKind::Cache, 1, 100),
            item(ResourceKind::Artifact, 2, 200),
        ]);
        assert_eq!(p.tier(), RiskTier::Low);
    }

    #[test]
    fn an_empty_plan_is_low_risk() {
        assert_eq!(plan(vec![]).tier(), RiskTier::Low);
    }

    #[test]
    fn total_bytes_sums_the_selection() {
        let p = plan(vec![
            item(ResourceKind::Cache, 1, 1_000_000),
            item(ResourceKind::Cache, 2, 2_000_000),
        ]);
        assert_eq!(p.total_bytes(), 3_000_000);
        assert!(p.summary().contains("3.0 Mo"));
        assert!(p.summary().contains('2'));
    }
}
