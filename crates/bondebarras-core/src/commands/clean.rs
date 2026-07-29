//! Non-interactive cleanup, for a monthly cron.

use crate::api::Client;
use crate::clean::{self, Plan, Progress};
use crate::model::{Resource, ResourceKind, RiskTier, human_size};
use crate::scan;
use anyhow::{Result, bail};
use std::process::ExitCode;
use tokio::sync::mpsc;

/// Which resources a headless run should touch.
pub struct CleanFilter {
    pub caches: bool,
    pub artifacts: bool,
    pub runs: bool,
    pub stale_pr: bool,
    pub older_than: Option<i64>,
}

/// Resources matching the filter.
///
/// Naming no family selects **nothing**. A `clean` that quietly meant
/// "everything" would be the worst possible default for an irreversible
/// operation running unattended.
pub fn select(items: &[Resource], filter: &CleanFilter) -> Vec<Resource> {
    items
        .iter()
        .filter(|r| match r.kind {
            ResourceKind::Cache => filter.caches,
            ResourceKind::Artifact => filter.artifacts,
            ResourceKind::WorkflowRun => filter.runs,
            // Task 6 adds the `--packages` flag this arm would read. Until
            // then `CleanFilter` has no field for it, and `scan::repo_detail`
            // (task 4) never puts a `PackageVersion` into `items`, so this is
            // unreachable today — not a silent `false` standing in for a
            // flag that does not exist yet.
            ResourceKind::PackageVersion => unreachable!(
                "PackageVersion never reaches select() before task 4 wires scan::repo_detail \
                 and task 6 adds the --packages flag"
            ),
        })
        .filter(|r| !filter.stale_pr || r.stale_pr)
        .filter(|r| filter.older_than.is_none_or(|d| r.age_days >= d))
        .cloned()
        .collect()
}

/// Run the cleanup. Returns the process exit code: failure if any deletion did.
pub async fn run(
    client: &Client,
    org: &str,
    repo: &str,
    filter: &CleanFilter,
    yes: bool,
) -> Result<ExitCode> {
    let items = scan::repo_detail(client, org, repo).await?;
    let picked = select(&items, filter);

    let plan = Plan {
        items: picked,
        owner: org.to_string(),
        repo: repo.to_string(),
    };

    // The nuclear tier demands typing the target's name, which no headless
    // run can do. There is deliberately no flag to bypass this.
    if plan.tier() >= RiskTier::Nuclear {
        bail!(
            "le palier 3 exige une confirmation interactive et ne peut pas s'exécuter sans interface"
        );
    }

    if plan.items.is_empty() {
        eprintln!("Rien à supprimer.");
        return Ok(ExitCode::SUCCESS);
    }

    if !yes {
        eprintln!(
            "Plan ({}) — relancez avec --yes pour l'appliquer :",
            plan.summary()
        );
        for r in &plan.items {
            eprintln!("  {:<40} {:>10}", r.label, human_size(r.size_bytes));
        }
        return Ok(ExitCode::SUCCESS);
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();
    clean::execute(client, plan, tx).await;

    let mut failures = 0usize;
    while let Ok(msg) = rx.try_recv() {
        match msg {
            Progress::Failed { id, reason, .. } => {
                failures += 1;
                eprintln!("Erreur : suppression de {id} — {reason}");
            }
            Progress::Finished { freed, failures: f } => {
                failures = f;
                eprintln!("Bon débarras ! {} libérés.", human_size(freed));
            }
            Progress::Done { .. } => {}
        }
    }

    Ok(if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ResourceKind;

    fn res(kind: ResourceKind, id: u64, age: i64, stale: bool) -> Resource {
        Resource {
            kind,
            id,
            label: format!("r{id}"),
            size_bytes: 100,
            age_days: age,
            git_ref: None,
            stale_pr: stale,
        }
    }

    fn filter() -> CleanFilter {
        CleanFilter {
            caches: false,
            artifacts: false,
            runs: false,
            stale_pr: false,
            older_than: None,
        }
    }

    #[test]
    fn no_family_flag_selects_nothing() {
        // A `clean` with no family named must be a no-op, never "everything".
        let items = vec![res(ResourceKind::Cache, 1, 90, true)];
        assert!(select(&items, &filter()).is_empty());
    }

    #[test]
    fn families_are_cumulative() {
        let items = vec![
            res(ResourceKind::Cache, 1, 1, false),
            res(ResourceKind::Artifact, 2, 1, false),
            res(ResourceKind::WorkflowRun, 3, 1, false),
        ];
        let f = CleanFilter {
            caches: true,
            artifacts: true,
            ..filter()
        };
        let picked: Vec<u64> = select(&items, &f).iter().map(|r| r.id).collect();
        assert_eq!(picked, vec![1, 2]);
    }

    #[test]
    fn stale_pr_narrows_within_the_chosen_families() {
        let items = vec![
            res(ResourceKind::Cache, 1, 1, true),
            res(ResourceKind::Cache, 2, 1, false),
        ];
        let f = CleanFilter {
            caches: true,
            stale_pr: true,
            ..filter()
        };
        let picked: Vec<u64> = select(&items, &f).iter().map(|r| r.id).collect();
        assert_eq!(picked, vec![1]);
    }

    #[test]
    fn older_than_is_inclusive_of_the_boundary() {
        let items = vec![
            res(ResourceKind::Cache, 1, 30, false),
            res(ResourceKind::Cache, 2, 29, false),
        ];
        let f = CleanFilter {
            caches: true,
            older_than: Some(30),
            ..filter()
        };
        let picked: Vec<u64> = select(&items, &f).iter().map(|r| r.id).collect();
        assert_eq!(picked, vec![1]);
    }

    // `families_are_cumulative` sets both `caches` and `artifacts`, so it
    // cannot tell a correct kind-to-flag mapping from one where those two
    // are swapped. Exercising each flag alone closes that gap.
    #[test]
    fn each_family_flag_selects_only_its_own_kind() {
        let items = vec![
            res(ResourceKind::Cache, 1, 1, false),
            res(ResourceKind::Artifact, 2, 1, false),
            res(ResourceKind::WorkflowRun, 3, 1, false),
        ];
        let ids =
            |f: &CleanFilter| -> Vec<u64> { select(&items, f).iter().map(|r| r.id).collect() };
        assert_eq!(
            ids(&CleanFilter {
                caches: true,
                ..filter()
            }),
            vec![1]
        );
        assert_eq!(
            ids(&CleanFilter {
                artifacts: true,
                ..filter()
            }),
            vec![2]
        );
        assert_eq!(
            ids(&CleanFilter {
                runs: true,
                ..filter()
            }),
            vec![3]
        );
    }

    // `stale_pr_narrows_within_the_chosen_families` only exercises
    // `stale_pr: true`, so it cannot tell "no restriction" from "require
    // exact equality with the (false) flag" — both happen to keep only the
    // non-stale item there. This fixture puts a stale item through with the
    // flag off, where the two behaviours diverge.
    #[test]
    fn stale_pr_off_does_not_exclude_stale_items() {
        let items = vec![
            res(ResourceKind::Cache, 1, 1, true),
            res(ResourceKind::Cache, 2, 1, false),
        ];
        let f = CleanFilter {
            caches: true,
            ..filter()
        };
        let picked: Vec<u64> = select(&items, &f).iter().map(|r| r.id).collect();
        assert_eq!(picked, vec![1, 2]);
    }
}
