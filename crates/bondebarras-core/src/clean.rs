//! Deletion planning and execution.
//!
//! Nothing here is reversible on GitHub's side, so there is no trash and no
//! undo — promising either would be a lie. What we offer instead is an
//! accurate recap before, and a per-item verdict after.

use crate::api::{Client, artifacts, caches, packages, refs, releases, runs};
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
/// `deleted_sizeless` — how many of `deleted` were package versions, the one
/// family GitHub exposes no size for — is what this needs to say something
/// true: keying the decision on `freed == 0` alone, as an earlier version
/// did, cannot tell "every deleted item's size is unknown" apart from
/// "every deleted item was genuinely zero bytes" — two real, empty caches
/// deleted would then read "taille inconnue," which is false, their size
/// was known and it was zero. Only "unknown" once every deletion that
/// happened was one where the size genuinely cannot be known.
///
/// A mixed purge (some package versions among sized resources) still just
/// reports `freed` here — accurate as far as it goes, even though it says
/// nothing about the package versions in the mix. Making that case honest
/// too is a separate concern, out of scope for this fix.
pub fn finished_recap(freed: u64, deleted: usize, deleted_sizeless: usize) -> String {
    if deleted > 0 && deleted_sizeless == deleted {
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
        /// How many of `deleted` were package versions — the resource kind
        /// GitHub exposes no size for. `finished_recap` needs this, not
        /// `deleted` alone, to tell "every deletion's size is unknown" apart
        /// from "every deletion was a real, empty resource."
        deleted_sizeless: usize,
    },
}

/// Delete every item of the plan, reporting each outcome as it lands.
pub async fn execute(client: &Client, plan: Plan, tx: UnboundedSender<Progress>) {
    let mut freed = 0_u64;
    let mut failures = 0_usize;
    let mut deleted = 0_usize;
    let mut deleted_sizeless = 0_usize;

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
            // A branch and a tag have no numeric id from GitHub — `item.id`
            // is a hash of the name (see `api::refs::resource_id`), a
            // selection key only. Deleting by it would be deleting by an
            // implementation detail that GitHub's API knows nothing about;
            // `item.label` carries the real name, so the arm deletes by
            // that instead. A hash collision — vanishingly unlikely as it
            // is — can therefore never delete the wrong ref.
            ResourceKind::Branch => {
                refs::delete_branch(client, &plan.owner, &plan.repo, &item.label).await
            }
            ResourceKind::Tag => {
                refs::delete_tag(client, &plan.owner, &plan.repo, &item.label).await
            }
            // Unlike a branch or a tag, a release asset carries a real
            // numeric GitHub id (see `api::releases::assets`) — no name
            // ambiguity, so it deletes by `item.id` like every v0.1-v0.3
            // family above.
            ResourceKind::ReleaseAsset => {
                releases::delete_asset(client, &plan.owner, &plan.repo, item.id).await
            }
        };

        match result {
            Ok(()) => {
                freed += item.size_bytes;
                deleted += 1;
                if item.kind == ResourceKind::PackageVersion {
                    deleted_sizeless += 1;
                }
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
        deleted_sizeless,
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
        // when dozens were deleted — and every one of the 45 deleted here
        // was one of them.
        let s = finished_recap(0, 45, 45);
        assert!(!s.contains("0 o"), "got: {s}");
        assert!(s.contains("45"), "got: {s}");
    }

    #[test]
    fn finished_recap_reports_zero_bytes_plainly_when_nothing_was_deleted() {
        let s = finished_recap(0, 0, 0);
        assert!(s.contains("0 o"), "got: {s}");
    }

    #[test]
    fn finished_recap_reports_real_bytes_when_they_exist() {
        // Discriminates a wrong implementation that always reports the
        // count, ignoring `freed` even when it is meaningful.
        let s = finished_recap(3_000_000, 2, 0);
        assert!(s.contains("3.0 Mo"), "got: {s}");
    }

    #[test]
    fn finished_recap_reports_zero_bytes_plainly_for_genuinely_zero_byte_deletions() {
        // Two real caches, truly empty: their size is known, and it is
        // zero — unlike a package version's, which is unknown. A version
        // keyed on `freed == 0` alone (rather than on `deleted_sizeless`)
        // could not tell this apart from an all-package-version purge, and
        // would have called two genuinely empty caches "taille inconnue".
        let s = finished_recap(0, 2, 0);
        assert!(s.contains("0 o"), "got: {s}");
        assert!(!s.to_lowercase().contains("inconnue"), "got: {s}");
    }

    /// A branch must be deleted by its **name** — carried in `label` — not
    /// by the hashed `id`: the hash is a selection key only. If `execute`
    /// were to send `item.id` to the DELETE path instead, the mock below
    /// would never match (it only listens on the literal branch name) and
    /// the call would 404, so this doubles as the load-bearing-label proof.
    #[tokio::test]
    async fn execute_deletes_a_branch_by_name_not_by_its_hashed_id() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path(
                "/repos/systm-d/claudine/git/refs/heads/claude/landing-3jbqk4",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let mut branch = item(ResourceKind::Branch, 999, 0);
        branch.label = "claude/landing-3jbqk4".to_string();
        let p = plan(vec![branch]);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        execute(&client, p, tx).await;

        let mut done = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Progress::Done { kind, id } => {
                    assert_eq!(kind, ResourceKind::Branch);
                    assert_eq!(id, 999, "the reported id is still the hashed selection key");
                    done = true;
                }
                Progress::Failed { reason, .. } => panic!("unexpected failure: {reason}"),
                Progress::Finished { .. } => {}
            }
        }
        assert!(done, "the branch must be reported as deleted");
    }

    /// Same load-bearing-label proof as the branch arm above, for a tag.
    #[tokio::test]
    async fn execute_deletes_a_tag_by_name_not_by_its_hashed_id() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/git/refs/tags/v0.1.3"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let mut tag = item(ResourceKind::Tag, 42, 0);
        tag.label = "v0.1.3".to_string();
        let p = plan(vec![tag]);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        execute(&client, p, tx).await;

        let mut done = false;
        while let Ok(msg) = rx.try_recv() {
            if let Progress::Done { kind, .. } = msg {
                assert_eq!(kind, ResourceKind::Tag);
                done = true;
            } else if let Progress::Failed { reason, .. } = msg {
                panic!("unexpected failure: {reason}");
            }
        }
        assert!(done, "the tag must be reported as deleted");
    }

    /// A release asset, unlike a branch or a tag, deletes by its real
    /// numeric GitHub id — never by name.
    #[tokio::test]
    async fn execute_deletes_a_release_asset_by_id() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/releases/assets/9"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let p = plan(vec![item(ResourceKind::ReleaseAsset, 9, 2_400_000)]);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        execute(&client, p, tx).await;

        let mut done = false;
        let mut freed = 0;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Progress::Done { kind, id } => {
                    assert_eq!(kind, ResourceKind::ReleaseAsset);
                    assert_eq!(id, 9);
                    done = true;
                }
                Progress::Failed { reason, .. } => panic!("unexpected failure: {reason}"),
                Progress::Finished { freed: f, .. } => freed = f,
            }
        }
        assert!(done, "the release asset must be reported as deleted");
        assert_eq!(
            freed, 2_400_000,
            "a release asset's real size must be freed"
        );
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
