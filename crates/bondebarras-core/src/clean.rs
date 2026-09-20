//! Deletion planning and execution.
//!
//! Nothing here is reversible on GitHub's side, so there is no trash and no
//! undo — promising either would be a lie. What we offer instead is an
//! accurate recap before, and a per-item verdict after.

use crate::api::{Client, archive, artifacts, caches, packages, refs, releases, runs};
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

    /// The bytes this plan would free, counting only the families GitHub
    /// reports a size for.
    ///
    /// `size_bytes` is a hardcoded `0` for every kind
    /// `ResourceKind::has_known_size` answers `false` for, so this sum is
    /// numerically unchanged by the filter today — that is exactly the
    /// reason to write it: a placeholder that happens to be zero is being
    /// excluded because it is a placeholder, not tolerated because of what
    /// it happens to equal (#51). A family whose placeholder is ever
    /// something other than zero cannot quietly inflate a total through
    /// here.
    pub fn total_bytes(&self) -> u64 {
        self.items
            .iter()
            .filter(|i| i.kind.has_known_size())
            .map(|i| i.size_bytes)
            .sum()
    }

    /// How many of the plan's items are a kind
    /// `ResourceKind::has_known_size` answers `false` for — what
    /// `total_bytes` above could not count, and what `summary` therefore
    /// has to name rather than leave out.
    fn sizeless_items(&self) -> usize {
        self.items
            .iter()
            .filter(|i| !i.kind.has_known_size())
            .count()
    }

    /// Whether this plan archives a repository rather than deleting
    /// anything. Such a plan is always exactly one `Repository` item — it is
    /// built directly from a single tick on the tree's own row (see
    /// `tui::app::App::take_repo_plan`), never a bulk selection, since
    /// `[A]` and headless `clean` both permanently refuse the kind. Used
    /// wherever the wording must not call archiving a deletion: it does not
    /// come back the way v0.3's package versions do, but it is reversible on
    /// GitHub's side, unlike everything else this crate touches.
    pub fn is_archive(&self) -> bool {
        !self.items.is_empty()
            && self
                .items
                .iter()
                .all(|i| i.kind == ResourceKind::Repository)
    }

    /// User-facing recap shown in the confirmation modal.
    ///
    /// An archive plan is checked first, and separately from the sizeless
    /// branch: `Repository` also has no known size (see
    /// `ResourceKind::has_known_size`), so it would otherwise fall into the
    /// same "taille inconnue" wording a sizeless deletion gets — Finding 6 of
    /// the final review. That phrasing implies the size question merely
    /// cannot be answered; for archiving it is answered, and the answer is
    /// zero, by design — un-archiving aside, nothing about the repository's
    /// own content changes, so no byte is ever freed (see `Plan::is_archive`'s
    /// own doc comment). The summary says what the plan actually does
    /// instead of borrowing a deletion's uncertainty.
    ///
    /// A plan made up entirely of sizeless deletions — whatever
    /// `ResourceKind::has_known_size` answers `false` for — always totals 0
    /// bytes, since `size_bytes` is hardcoded to `0` for every such kind.
    /// That is not the same thing as an empty plan: printing "0 o" would
    /// read as "nothing was selected", so the honest recap names the count
    /// instead and says plainly that the size is unknown.
    ///
    /// The set is read off `has_known_size`, through `all_sizeless`, rather
    /// than restated here: this comment used to list "package versions,
    /// branches, tags" while the predicate had already gained a fourth
    /// family, the workflow run (#41) — a list in a comment rots, a call
    /// does not.
    ///
    /// A *mixed* plan — some sizeless items among sized ones — names what
    /// its figure left out, through `sizeless_tail` (#51). "40 élément(s) ·
    /// 4.1 Go" over a plan of 3 caches and 37 workflow runs is true about
    /// the 3 and silent about the 37, and silence here reads as "4.1 Go is
    /// what those 40 rows weigh". Since `[A]` preselects a run whose pull
    /// request merged, that mix is the ordinary case, not the exotic one.
    /// This modal is the surface that can afford the whole sentence: it
    /// wraps (`tui::views::confirm` measures its own height with
    /// `wrapped_row_count`), so nothing here is ever clipped.
    pub fn summary(&self) -> String {
        let sizeless = self.sizeless_items();
        if self.is_archive() {
            "Archivage · 0 o libéré, par nature".to_string()
        } else if all_sizeless(self.items.len(), sizeless) {
            format!("{} élément(s) · taille inconnue", self.items.len())
        } else {
            format!(
                "{} élément(s) · {}{}",
                self.items.len(),
                human_size(self.total_bytes()),
                sizeless_tail(sizeless)
            )
        }
    }
}

/// Whether a set of `total` resources is sizeless through and through:
/// `sizeless` of them are kinds `ResourceKind::has_known_size` answers
/// `false` for, and that accounts for all of them.
///
/// The one rule behind every "the size is unknown" this crate prints —
/// `Plan::summary`, `finished_recap`, and the resources column's own title
/// (`tui::app::App::selection_size_display`). It is subtle in both
/// directions, which is why it lives in one place: an empty set is *not*
/// sizeless (nothing was selected, and "0 o" says exactly that), and a set
/// holding one sized item still has real bytes to report, however partial.
///
/// Three call sites, one predicate: #41 was born of two of them disagreeing
/// about whether a workflow run had a size.
pub fn all_sizeless(total: usize, sizeless: usize) -> bool {
    total > 0 && sizeless == total
}

/// Where a resource ranks in a sort by size: its group first — measured
/// before unmeasured — then its bytes, biggest first.
///
/// The one key behind both size sorts, `tui::app::App::visible_resources`
/// (the TUI's resource column) and `scan::repo_detail`'s own (the headless
/// listing and `scan --json`). #51 was the two of them agreeing on the
/// wrong thing: both ranked on `size_bytes` alone, so both filed a workflow
/// run — `size_bytes: 0`, a placeholder, never a measurement — as the
/// lightest row in the repository while its own line read `—`. Written once
/// here so the next family GitHub gives no size for cannot be fixed in one
/// sort and missed in the other, the way `has_known_size` itself was missed
/// in three places before v0.4 gathered it.
///
/// The group is the first field on purpose: it makes `sort_by_key`'s
/// stability carry the rest, leaving every row's order within its group
/// exactly as the caller assembled it.
///
/// `tui::app::App::visible_resources`'s doc comment argues *why* the
/// unmeasured go last rather than first; this is only where the two callers
/// share the answer.
pub fn size_rank(r: &Resource) -> (bool, std::cmp::Reverse<u64>) {
    (!r.kind.has_known_size(), std::cmp::Reverse(r.size_bytes))
}

/// What a byte figure could not count, as the tail of the line that shows
/// it: `" + 37 de taille inconnue"`, or nothing at all when it counted
/// everything.
///
/// The mixed case's wording, in one place, for the two surfaces that have a
/// whole line to say it on: the confirmation modal (`Plan::summary`) and
/// the post-purge recap (`finished_recap`). `all_sizeless` above answers
/// the all-or-nothing question; this answers what is left when the answer
/// is "some" (#51).
///
/// It reuses "taille inconnue" verbatim rather than inventing a second
/// phrase for the same fact: a user who has seen a run-only purge say
/// "taille inconnue" must recognise the same words here, not learn a
/// synonym. The count is bare — `+ 37`, not `+ 37 éléments` — because both
/// call sites already name what they are counting just before it.
pub fn sizeless_tail(sizeless: usize) -> String {
    if sizeless == 0 {
        String::new()
    } else {
        format!(" + {sizeless} de taille inconnue")
    }
}

/// The same fact as `sizeless_tail`, for a surface with no room for a
/// sentence: `"≥ 4.1 Go"` when some of what the figure should have covered
/// has no size, the plain figure otherwise.
///
/// The resources column's title (`tui::app::App::selection_size_display`,
/// drawn by `tui::views::repo::list_title`) is written on a block's top
/// border, and ratatui clips whatever overflows it. That border is 38 cells
/// at the narrowest layout the width sweeps cover, of which `compact_title`
/// — the last rung of its own degradation ladder — already spends about 35.
/// `sizeless_tail`'s 23 cells cannot fit there under any arrangement, and a
/// half-drawn "+ 37 de taille inc" is precisely the clipping that ladder
/// exists to prevent. So the title states the weaker claim it *can* state
/// whole: the ticked rows weigh **at least** this. The full accounting is
/// one keystroke away, in the modal `d` opens.
///
/// Two renderings, one decision: both ask `ResourceKind::has_known_size`,
/// through the counts their callers pass, and neither invents a set of its
/// own. What differs is the room each has, not what either believes.
pub fn at_least_size(bytes: u64, sizeless: usize) -> String {
    if sizeless == 0 {
        human_size(bytes)
    } else {
        format!("≥ {}", human_size(bytes))
    }
}

/// The "N libérés" fragment of the post-purge recap, shared by the TUI's
/// status line and the headless `clean` command so the wording never drifts
/// between the two.
///
/// `deleted_sizeless` — how many of `deleted` were a kind GitHub exposes no
/// size for at all (`!ResourceKind::has_known_size`, which is the list;
/// spelling it out again here is how this comment came to name three
/// families while the predicate counted four) — is what this needs to say
/// something true: keying the
/// decision on `freed == 0` alone, as an earlier version did, cannot tell
/// "every deleted item's size is unknown" apart from "every deleted item was
/// genuinely zero bytes" — two real, empty caches deleted would then read
/// "taille inconnue," which is false, their size was known and it was zero.
/// Only "unknown" once every deletion that happened was one where the size
/// genuinely cannot be known.
///
/// A mixed purge — some sizeless items among sized resources — names what
/// `freed` could not count, through `sizeless_tail` (#51): a purge of 3
/// caches and 37 runs reads "4.1 Go libérés + 37 de taille inconnue".
/// Reporting `freed` alone was accurate as far as it went, and that was the
/// whole problem: it went as far as the 3 caches and stayed silent about
/// the 37 runs deleted beside them, so the one figure on screen read as the
/// whole purge's yield. Rare while the sizeless families were package
/// versions, branches and tags; ordinary since `[A]` began preselecting
/// workflow runs whose pull request merged.
pub fn finished_recap(freed: u64, deleted: usize, deleted_sizeless: usize) -> String {
    if all_sizeless(deleted, deleted_sizeless) {
        format!("{deleted} élément(s) supprimé(s) · taille inconnue")
    } else {
        format!(
            "{} libérés{}",
            human_size(freed),
            sizeless_tail(deleted_sizeless)
        )
    }
}

/// Emitted as the deletion runs, so the TUI stays responsive.
///
/// `Done` and `Failed` carry `kind` alongside `id` because ids are only
/// unique within one resource kind: without it, the event loop cannot tell
/// which row to remove from `app.resources` when a cache and an artifact
/// happen to share an id.
///
/// `Done` and `Failed` also carry `owner`/`repo` — Findings 3 and 4 of the
/// final review. A `Repository` archive shares this same one channel with
/// every other kind (see `execute`'s own `Repository` arm), so the event
/// loop's `Done`/`Failed` handler for it used to identify *which* repository
/// just archived by reading `app.selected_repo` — whatever the tree
/// currently ticked, not necessarily the repository this specific message
/// was about. Starting a second archive before the first's `Done`/`Failed`
/// landed then let the second tick silently steal the first one's
/// attribution, or vice versa. Carrying the plan's own `owner`/`repo`
/// directly on the message — `execute` already has both in scope for every
/// item it sends — makes each message self-describing: two concurrent
/// `execute` calls each report against their own target, with no shared
/// slot for one to stomp on the other's.
#[derive(Debug, Clone)]
pub enum Progress {
    Done {
        kind: ResourceKind,
        id: u64,
        owner: String,
        repo: String,
    },
    Failed {
        kind: ResourceKind,
        id: u64,
        reason: String,
        owner: String,
        repo: String,
    },
    Finished {
        freed: u64,
        failures: usize,
        /// How many items were actually deleted. `freed` alone cannot carry
        /// this: a purge of sizeless items frees 0 bytes by construction
        /// (GitHub exposes no size for their kinds) even when dozens were
        /// deleted, so the recap needs the count to say something true.
        deleted: usize,
        /// How many of `deleted` were a kind `ResourceKind::has_known_size`
        /// answers `false` for — the predicate is the list, so this comment
        /// does not copy it. `finished_recap` needs this, not `deleted`
        /// alone, to tell "every deletion's size is unknown" apart from
        /// "every deletion was a real, empty resource."
        deleted_sizeless: usize,
        /// `Some(repo)` when the plan that just finished was an archive plan
        /// (`Plan::is_archive`) — the repository it archived. Computed once,
        /// by `execute`, from its own `Plan`, rather than tracked as a
        /// shared `archiving_target` slot in the event loop (Findings 3 and
        /// 4 of the final review): a second archive confirmed before the
        /// first's `Finished` landed used to overwrite that slot, so the
        /// first `Finished` — arriving after — read back the *second*
        /// plan's repository, or `None` if the slot had already been
        /// consumed once. Each `execute` call now derives this from the one
        /// `Plan` it alone owns, so there is nothing left for a second call
        /// to desynchronise.
        archived_repo: Option<String>,
    },
}

/// Delete every item of the plan, reporting each outcome as it lands.
pub async fn execute(client: &Client, plan: Plan, tx: UnboundedSender<Progress>) {
    let mut freed = 0_u64;
    let mut failures = 0_usize;
    let mut deleted = 0_usize;
    let mut deleted_sizeless = 0_usize;
    // Computed once, from this call's own `Plan`, before `plan.items` is
    // even walked — see `Progress::Finished`'s own doc comment for why this
    // must not be tracked as shared state anywhere else.
    let archived_repo = plan.is_archive().then(|| plan.repo.clone());

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
            // Not a deletion — the repository stays, only its Actions turn
            // off — but it goes through the same execute/Progress plumbing
            // as every other kind: one confirmed plan, one outcome per item.
            // `plan.owner`/`plan.repo` already name the target repository
            // directly, unlike every arm above: the item itself carries no
            // id `archive` needs to address anything by.
            ResourceKind::Repository => archive::archive(client, &plan.owner, &plan.repo).await,
        };

        match result {
            Ok(()) => {
                deleted += 1;
                // One branch, so a deletion is counted in exactly one of the
                // two tallies and `has_known_size` is asked once: either its
                // bytes are a real measurement and join `freed`, or there
                // are none to join and it joins `deleted_sizeless` instead
                // (#51). The `freed += item.size_bytes` this replaces added
                // a hardcoded placeholder to a figure the recap presents as
                // measured — zero today for every sizeless family, which is
                // why it never showed, and not a reason to keep summing it.
                if item.kind.has_known_size() {
                    freed += item.size_bytes;
                } else {
                    deleted_sizeless += 1;
                }
                let _ = tx.send(Progress::Done {
                    kind: item.kind,
                    id: item.id,
                    owner: plan.owner.clone(),
                    repo: plan.repo.clone(),
                });
            }
            Err(e) => {
                failures += 1;
                let _ = tx.send(Progress::Failed {
                    kind: item.kind,
                    id: item.id,
                    reason: e.to_string(),
                    owner: plan.owner.clone(),
                    repo: plan.repo.clone(),
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
        archived_repo,
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
            branch_class: None,
            safety: crate::safety::Safety::Keep,
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

    /// #51: `total_bytes` summed `size_bytes` over every item, whatever its
    /// kind. The placeholder those kinds carry is `0` today, so the figure
    /// came out right — by luck, not by rule. This run's placeholder is not
    /// zero, which is the only fixture that can tell "left out because it
    /// is unmeasured" apart from "taken in, and happens to add nothing".
    #[test]
    fn total_bytes_counts_only_the_families_github_measures() {
        let p = plan(vec![
            item(ResourceKind::Cache, 1, 1_000_000),
            item(ResourceKind::WorkflowRun, 2, 9_999),
        ]);
        assert_eq!(p.total_bytes(), 1_000_000);
    }

    /// The mixed plan #41 left behind, and the ordinary one since `[A]`
    /// began preselecting runs whose pull request merged: 1 cache and 2
    /// runs used to announce "3 élément(s) · 3.0 Mo" — true of the cache,
    /// silent about the runs, and read as the weight of all three.
    #[test]
    fn summary_names_what_a_mixed_plan_could_not_count() {
        let p = plan(vec![
            item(ResourceKind::Cache, 1, 3_000_000),
            item(ResourceKind::WorkflowRun, 2, 0),
            item(ResourceKind::WorkflowRun, 3, 0),
        ]);
        let s = p.summary();
        assert!(
            s.contains("3.0 Mo"),
            "the bytes it does know must survive: {s}"
        );
        assert!(s.contains("+ 2 de taille inconnue"), "got: {s}");
    }

    /// The discriminating half: an implementation appending the tail
    /// unconditionally would write "+ 0 de taille inconnue" over two
    /// ordinary caches, admitting an uncertainty that does not exist.
    #[test]
    fn summary_adds_no_tail_when_every_item_is_measured() {
        let p = plan(vec![
            item(ResourceKind::Cache, 1, 1_000_000),
            item(ResourceKind::Artifact, 2, 2_000_000),
        ]);
        let s = p.summary();
        assert!(!s.to_lowercase().contains("inconnue"), "got: {s}");
    }

    /// #51's sort rule, at the key both sorts share. The empty cache is the
    /// row that discriminates: its size is *known*, and it is zero. A rule
    /// that merely pushed zero-byte rows to the end — or that kept ranking
    /// on `size_bytes` alone — would file it with the runs instead of above
    /// them.
    #[test]
    fn a_measured_row_outranks_an_unmeasured_one_even_at_zero_bytes() {
        let empty_cache = item(ResourceKind::Cache, 1, 0);
        let run = item(ResourceKind::WorkflowRun, 2, 0);
        assert!(size_rank(&empty_cache) < size_rank(&run));
    }

    /// The title's rendering of the same fact: a figure that covers
    /// everything ticked is stated flat, one that does not is marked as a
    /// floor. Asserted on exact strings — the whole difference is two cells
    /// a substring check would read straight past.
    #[test]
    fn a_size_that_could_not_count_everything_is_marked_as_a_floor() {
        assert_eq!(at_least_size(200, 0), "200 o");
        assert_eq!(at_least_size(200, 1), "≥ 200 o");
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

    /// Finding 6 of the final review: `summary` reused "taille inconnue" for
    /// an archive plan too, since `Repository` has no known size — but
    /// archiving is not an unanswerable size question, it never frees a
    /// byte, by design (see `Plan::is_archive`'s own doc comment:
    /// reversible, unlike everything else this crate touches). The summary
    /// must say what the plan actually does, not lump it in with a package
    /// version's genuinely unknown size.
    #[test]
    fn summary_names_the_archive_and_says_plainly_it_frees_nothing() {
        let p = Plan {
            items: vec![item(ResourceKind::Repository, 1, 0)],
            owner: "maxds-lyon".into(),
            repo: "lokiprint".into(),
        };
        assert!(p.is_archive(), "fixture must actually be an archive plan");

        let s = p.summary();
        assert!(!s.to_lowercase().contains("inconnue"), "got: {s}");
        assert!(s.to_lowercase().contains("archiv"), "got: {s}");
        assert!(
            s.contains("0 o"),
            "must say plainly that it frees nothing: got: {s}"
        );
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

    /// Finding 3 of the v0.4 final review: branches and tags are sizeless
    /// too, but `summary` only checked `kind == ResourceKind::PackageVersion`
    /// — a plan of only branches and tags totalled 0 bytes and printed a
    /// bare "0 o", the exact "reads as empty" defect v0.3's whole fix wave
    /// was about, recurring for two families the old check could not see.
    #[test]
    fn summary_says_size_is_unknown_for_an_all_branch_and_tag_plan() {
        let p = plan(vec![
            item(ResourceKind::Branch, 1, 0),
            item(ResourceKind::Tag, 2, 0),
        ]);
        let s = p.summary();
        assert!(!s.contains("0 o"), "got: {s}");
        assert!(s.contains('2'), "got: {s}");
        assert!(s.to_lowercase().contains("inconnue"), "got: {s}");
    }

    /// #41: a workflow run is sizeless too — `api::runs::list` hardcodes
    /// `size_bytes: 0` for a family GitHub reports no size for anywhere — so
    /// a plan of only runs totals 0 bytes and printed a bare "0 o", the same
    /// "reads as nothing was selected" defect this branch already fixed for
    /// package versions, branches and tags. Deleting those runs does free
    /// space, through the logs and artifacts that go with them; it is the
    /// amount that is unknown, not the effect.
    #[test]
    fn summary_says_size_is_unknown_for_an_all_workflow_run_plan() {
        let p = plan(vec![
            item(ResourceKind::WorkflowRun, 1, 0),
            item(ResourceKind::WorkflowRun, 2, 0),
        ]);
        let s = p.summary();
        assert!(!s.contains("0 o"), "got: {s}");
        assert!(s.contains('2'), "got: {s}");
        assert!(s.to_lowercase().contains("inconnue"), "got: {s}");
    }

    /// The `execute`-level counterpart of the test above: a purge of only
    /// runs must report its count, not "0 o libérés". Keyed on
    /// `deleted_sizeless`, which `execute` now increments for a run since
    /// `has_known_size` is what it asks — a version keyed on `freed == 0`
    /// could not tell this from a purge of two genuinely empty caches.
    #[test]
    fn finished_recap_says_the_count_for_a_purge_of_only_workflow_runs() {
        let s = finished_recap(0, 12, 12);
        assert!(!s.contains("0 o"), "got: {s}");
        assert!(s.contains("12"), "got: {s}");
        assert!(s.to_lowercase().contains("inconnue"), "got: {s}");
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
                Progress::Done { kind, id, .. } => {
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

    /// Finding 3 of the v0.4 final review: `deleted_sizeless` only counted
    /// `ResourceKind::PackageVersion`, so a purge of only branches (or tags)
    /// reported `deleted_sizeless: 0` and `finished_recap` fell through to
    /// `human_size(freed)` — "0 o libérés" for a purge that genuinely
    /// deleted something, the false-emptiness defect stated one level up
    /// the call stack from where `Plan::summary` has the same bug.
    #[tokio::test]
    async fn execute_counts_a_deleted_branch_as_sizeless_too() {
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

        let mut sizeless = None;
        while let Ok(msg) = rx.try_recv() {
            if let Progress::Finished {
                deleted_sizeless, ..
            } = msg
            {
                sizeless = Some(deleted_sizeless);
            }
        }
        assert_eq!(
            sizeless,
            Some(1),
            "a deleted branch must count as sizeless, same as a package version"
        );
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
                Progress::Done { kind, id, .. } => {
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

    /// #51, end to end on the purge that has become the ordinary one: an
    /// asset GitHub measures deleted beside a run it does not. Two things
    /// must hold at once — `freed` carries only measured bytes, and the
    /// recap built from it names what those bytes left out.
    ///
    /// The run's `size_bytes` is deliberately non-zero, which `api::runs::
    /// list` never writes: with a zero there, an implementation that still
    /// added every item's bytes to `freed` would pass this test unchanged,
    /// and the assertion would be proving nothing.
    #[tokio::test]
    async fn a_mixed_purge_frees_only_measured_bytes_and_says_what_it_could_not_count() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/releases/assets/9"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/actions/runs/128"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let p = plan(vec![
            item(ResourceKind::ReleaseAsset, 9, 2_400_000),
            item(ResourceKind::WorkflowRun, 128, 9_999),
        ]);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        execute(&client, p, tx).await;

        let mut finished = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Progress::Failed { reason, .. } => panic!("unexpected failure: {reason}"),
                Progress::Finished {
                    freed,
                    deleted,
                    deleted_sizeless,
                    ..
                } => finished = Some((freed, deleted, deleted_sizeless)),
                Progress::Done { .. } => {}
            }
        }

        let (freed, deleted, sizeless) = finished.expect("the purge must report Finished");
        assert_eq!(freed, 2_400_000, "the run's placeholder reached `freed`");
        assert_eq!((deleted, sizeless), (2, 1));

        let recap = finished_recap(freed, deleted, sizeless);
        assert!(recap.contains("2.4 Mo"), "got: {recap}");
        assert!(recap.contains("+ 1 de taille inconnue"), "got: {recap}");
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
                Progress::Done { kind, id, .. } => {
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

    /// `execute`'s `Repository` arm must call the real archive endpoint, not
    /// stand in as a stub — the wiring this task exists to finish.
    /// `PATCH /repos/{owner}/{repo}` targets `plan.owner`/`plan.repo`
    /// directly: unlike every arm above, the item itself carries no id the
    /// call needs, so a mock listening only on the exact owner/repo path
    /// (not on any item id) is what proves this, not a coincidence of a
    /// shared URL shape.
    #[tokio::test]
    async fn execute_archives_a_repository_via_the_archive_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let p = plan(vec![item(ResourceKind::Repository, 1, 0)]);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        execute(&client, p, tx).await;

        let mut done = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Progress::Done { kind, .. } => {
                    assert_eq!(kind, ResourceKind::Repository);
                    done = true;
                }
                Progress::Failed { reason, .. } => panic!("unexpected failure: {reason}"),
                Progress::Finished { .. } => {}
            }
        }
        assert!(done, "the repository must be reported as archived");
    }

    /// A 403 (not an admin) must surface as a failure, not a silent success —
    /// the same guarantee `api::archive`'s own test locks at the endpoint
    /// level, proven again here through the full `execute` plumbing.
    #[tokio::test]
    async fn execute_reports_a_refused_archive_as_a_failure_not_a_success() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let p = plan(vec![item(ResourceKind::Repository, 1, 0)]);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        execute(&client, p, tx).await;

        let mut failed = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Progress::Done { .. } => panic!("a 403 must not be reported as success"),
                Progress::Failed { kind, .. } => {
                    assert_eq!(kind, ResourceKind::Repository);
                    failed = true;
                }
                Progress::Finished { .. } => {}
            }
        }
        assert!(failed, "the refused archive must be reported as a failure");
    }

    #[test]
    fn a_single_repository_item_plan_is_an_archive_plan() {
        let p = plan(vec![item(ResourceKind::Repository, 1, 0)]);
        assert!(p.is_archive());
    }

    #[test]
    fn an_empty_plan_is_not_an_archive_plan() {
        assert!(!plan(vec![]).is_archive());
    }

    /// A wrong implementation keyed on `items.len() == 1` alone, rather than
    /// the kind, would call a lone cache an archive plan too — this is the
    /// fixture that tells the two apart.
    #[test]
    fn a_single_non_repository_item_is_not_an_archive_plan() {
        assert!(!plan(vec![item(ResourceKind::Cache, 1, 100)]).is_archive());
    }

    /// A plan mixing a repository with anything else must not read as an
    /// archive plan — `take_repo_plan` never builds one this way, but
    /// `is_archive` itself should not assume that invariant silently.
    #[test]
    fn a_mixed_plan_is_not_an_archive_plan() {
        let p = plan(vec![
            item(ResourceKind::Repository, 1, 0),
            item(ResourceKind::Cache, 2, 100),
        ]);
        assert!(!p.is_archive());
    }
}
