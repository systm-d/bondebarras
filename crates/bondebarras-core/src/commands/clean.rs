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
    /// Container package versions. Like every other family, absent means not
    /// selected — a `clean` that named no family must never mean "everything".
    pub packages: bool,
    pub stale_pr: bool,
    pub older_than: Option<i64>,
}

/// Resources matching the filter.
///
/// Naming no family selects **nothing**. A `clean` that quietly meant
/// "everything" would be the worst possible default for an irreversible
/// operation running unattended.
///
/// A `protected` resource is never returned, regardless of the filter: it is
/// still live-referenced by name (a tag like `latest`, today), and headless
/// has no human at the other end of a cron to notice a broken deployment.
/// The TUI's individual `espace` selection is the only path left to it — see
/// `tui::app::App::toggle_selected`.
pub fn select(items: &[Resource], filter: &CleanFilter) -> Vec<Resource> {
    items
        .iter()
        .filter(|r| match r.kind {
            ResourceKind::Cache => filter.caches,
            ResourceKind::Artifact => filter.artifacts,
            ResourceKind::WorkflowRun => filter.runs,
            ResourceKind::PackageVersion => filter.packages,
        })
        // A protected resource is never taken in bulk. Headless has no human
        // to override that, so this is not a default — it is the rule.
        .filter(|r| !r.protected)
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
            protected: false,
        }
    }

    fn filter() -> CleanFilter {
        CleanFilter {
            caches: false,
            artifacts: false,
            runs: false,
            packages: false,
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

    #[test]
    fn a_package_version_is_only_selected_when_its_family_is_named() {
        // This arm was `unreachable!()` until repo_detail started producing
        // package rows, at which point a headless clean panicked against any
        // repo publishing an image. The fixture must contain the kind, or the
        // arm is never exercised at all.
        let items = vec![res(ResourceKind::PackageVersion, 1, 1, false)];
        assert!(select(&items, &filter()).is_empty());

        let f = CleanFilter {
            packages: true,
            ..filter()
        };
        let picked: Vec<u64> = select(&items, &f).iter().map(|r| r.id).collect();
        assert_eq!(picked, vec![1]);
    }

    // The test above uses a fixture holding only `PackageVersion` items, so
    // it cannot tell a correctly wired arm from one accidentally shared with
    // another kind — e.g. `ResourceKind::WorkflowRun | ResourceKind::PackageVersion
    // => filter.packages` (a plausible copy/paste of the match arm above it)
    // still passes that test by accident: there is no `WorkflowRun` item in
    // its fixture to leak into the selection. Verified by injecting exactly
    // that mutation: the single-kind test above stayed green, and only this
    // fixture — which carries all four kinds at once — caught it, with a
    // `WorkflowRun` id showing up in `picked` alongside the package version.
    #[test]
    fn packages_alone_isolates_the_package_version_from_the_other_three_families() {
        let items = vec![
            res(ResourceKind::Cache, 1, 1, false),
            res(ResourceKind::Artifact, 2, 1, false),
            res(ResourceKind::WorkflowRun, 3, 1, false),
            res(ResourceKind::PackageVersion, 4, 1, false),
        ];

        // No family named: nothing, from any of the four.
        assert!(select(&items, &filter()).is_empty());

        // --packages alone: only the PackageVersion item.
        let f = CleanFilter {
            packages: true,
            ..filter()
        };
        let picked: Vec<u64> = select(&items, &f).iter().map(|r| r.id).collect();
        assert_eq!(picked, vec![4]);
    }

    #[test]
    fn a_protected_resource_is_never_taken_in_bulk() {
        // `latest` in a cron is the scenario: no human, no confirmation, and a
        // broken deployment for everyone pulling that tag.
        let mut tagged = res(ResourceKind::PackageVersion, 1, 90, false);
        tagged.protected = true;
        let untagged = res(ResourceKind::PackageVersion, 2, 90, false);

        let f = CleanFilter {
            packages: true,
            ..filter()
        };
        let picked: Vec<u64> = select(&[tagged, untagged], &f)
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(
            picked,
            vec![2],
            "a tagged version must never be selected headlessly"
        );
    }
}
