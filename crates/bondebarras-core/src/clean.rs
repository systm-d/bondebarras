//! Deletion planning and execution.
//!
//! Nothing here is reversible on GitHub's side, so there is no trash and no
//! undo — promising either would be a lie. What we offer instead is an
//! accurate recap before, and a per-item verdict after.

use crate::api::{Client, artifacts, caches, packages, runs};
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
    ///
    /// A plan made up entirely of package versions always totals 0 bytes —
    /// GitHub exposes no size for that family, `size_bytes` is hardcoded to
    /// 0 for every one of them (see `scan::version_resources`) — but that is
    /// not the same thing as an empty plan. Printing "0 o" would read as
    /// "nothing was selected"; the honest recap names the count instead and
    /// says plainly that the size is unknown.
    pub fn summary(&self) -> String {
        if !self.items.is_empty()
            && self
                .items
                .iter()
                .all(|i| i.kind == ResourceKind::PackageVersion)
        {
            format!("{} élément(s) · taille inconnue", self.items.len())
        } else {
            format!(
                "{} élément(s) · {}",
                self.items.len(),
                human_size(self.total_bytes())
            )
        }
    }
}

/// The "N libérés" fragment of the post-purge recap, shared by the TUI's
/// status line and the headless `clean` command so the wording never drifts
/// between the two.
///
/// `freed == 0` legitimately happens two ways: nothing was deleted, or only
/// package versions were — GitHub exposes no size for that family, so their
/// bytes are always 0 even when `deleted` is well into the dozens. Saying "0
/// o libérés" either way would read as "nothing happened" for the second
/// case, which is false: 45 versions can vanish while the byte total stays 0.
pub fn finished_recap(freed: u64, deleted: usize) -> String {
    if freed == 0 && deleted > 0 {
        format!("{deleted} élément(s) supprimé(s) · taille inconnue")
    } else {
        format!("{} libérés", human_size(freed))
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
        /// How many items were actually deleted. `freed` alone cannot carry
        /// this: a purge of package versions frees 0 bytes by construction
        /// (GitHub exposes no size for that family) even when dozens were
        /// deleted, so the recap needs the count to say something true.
        deleted: usize,
    },
}

/// Delete every item of the plan, reporting each outcome as it lands.
pub async fn execute(client: &Client, plan: Plan, tx: UnboundedSender<Progress>) {
    let mut freed = 0_u64;
    let mut failures = 0_usize;
    let mut deleted = 0_usize;

    for item in &plan.items {
        let result = match item.kind {
            ResourceKind::Cache => caches::delete(client, &plan.owner, &plan.repo, item.id).await,
            ResourceKind::Artifact => {
                artifacts::delete(client, &plan.owner, &plan.repo, item.id).await
            }
            ResourceKind::WorkflowRun => {
                runs::delete(client, &plan.owner, &plan.repo, item.id).await
            }
            // `Plan` carries no separate package name: `plan.repo` doubles as
            // the package name, per this account's convention that a repo's
            // image is named after the repo itself (see `scan::repo_detail`).
            ResourceKind::PackageVersion => {
                packages::delete_version(client, &plan.owner, &plan.repo, item.id).await
            }
        };

        match result {
            Ok(()) => {
                freed += item.size_bytes;
                deleted += 1;
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

    let _ = tx.send(Progress::Finished {
        freed,
        failures,
        deleted,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn item(kind: ResourceKind, id: u64, size: u64) -> Resource {
        Resource {
            kind,
            id,
            label: format!("item-{id}"),
            size_bytes: size,
            age_days: 30,
            git_ref: None,
            stale_pr: false,
            protected: false,
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

    #[test]
    fn summary_says_size_is_unknown_for_an_all_package_version_plan() {
        // GitHub exposes no size for a package version, so a plan made up of
        // them always totals 0 bytes even though real deletions happen —
        // "0 o" here would read as "nothing was selected".
        let p = plan(vec![
            item(ResourceKind::PackageVersion, 1, 0),
            item(ResourceKind::PackageVersion, 2, 0),
        ]);
        let s = p.summary();
        assert!(!s.contains("0 o"), "got: {s}");
        assert!(s.contains('2'), "got: {s}");
        assert!(s.to_lowercase().contains("inconnue"), "got: {s}");
    }

    #[test]
    fn summary_reports_a_genuinely_zero_byte_cache_plainly() {
        // A wrong implementation keyed on `total_bytes() == 0` rather than
        // the resource kind would also call this "unknown" — but a cache's
        // size is always known, zero included. Only tell apart from the test
        // above by resource kind, not by the coincidence of a zero total.
        let p = plan(vec![item(ResourceKind::Cache, 1, 0)]);
        let s = p.summary();
        assert!(s.contains("0 o"), "got: {s}");
        assert!(!s.to_lowercase().contains("inconnue"), "got: {s}");
    }

    #[test]
    fn finished_recap_says_the_count_when_bytes_are_meaningless() {
        // A purge of package versions frees 0 bytes by construction, even
        // when dozens were deleted.
        let s = finished_recap(0, 45);
        assert!(!s.contains("0 o"), "got: {s}");
        assert!(s.contains("45"), "got: {s}");
    }

    #[test]
    fn finished_recap_reports_zero_bytes_plainly_when_nothing_was_deleted() {
        let s = finished_recap(0, 0);
        assert!(s.contains("0 o"), "got: {s}");
    }

    #[test]
    fn finished_recap_reports_real_bytes_when_they_exist() {
        // Discriminates a wrong implementation that always reports the
        // count, ignoring `freed` even when it is meaningful.
        let s = finished_recap(3_000_000, 2);
        assert!(s.contains("3.0 Mo"), "got: {s}");
    }

    #[tokio::test]
    async fn execute_deletes_a_package_version_via_the_repos_homonymous_package() {
        // `Plan` carries no separate package name: `plan.repo` doubles as the
        // package name, per this account's convention.
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions/9"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let p = plan(vec![item(ResourceKind::PackageVersion, 9, 0)]);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        execute(&client, p, tx).await;

        let mut done = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Progress::Done { kind, id } => {
                    assert_eq!(kind, ResourceKind::PackageVersion);
                    assert_eq!(id, 9);
                    done = true;
                }
                Progress::Failed { reason, .. } => panic!("unexpected failure: {reason}"),
                Progress::Finished { .. } => {}
            }
        }
        assert!(done, "the package version must be reported as deleted");
    }
}
