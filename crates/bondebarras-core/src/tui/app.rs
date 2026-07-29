//! Application state: navigation, selection, sorting and filtering.
//!
//! Selection primitives are deliberately ad hoc — sort, filter, flag-select —
//! and nothing is persisted. There is no rules engine and no config file:
//! the user decides, every time.

use crate::clean::Plan;
use crate::model::{OrgSummary, RepoSummary, Resource, ResourceKind};
use ratatui::widgets::ListState;
use std::collections::HashSet;

/// Which pane the keyboard drives. The tree has three levels, and each one
/// needs its own cursor: an org, one of its repos, then that repo's resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Orgs,
    Repos,
    Resources,
}

/// Which top-level view is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Orgs,
    Billing,
}

/// Sort order of the resource pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    /// Biggest first — the default, because size is why the user is here.
    Size,
    /// Oldest first.
    Age,
    /// Alphabetical, for hunting a known key.
    Name,
}

pub struct App {
    pub orgs: Vec<OrgSummary>,
    pub org_cursor: usize,
    pub repo_cursor: usize,
    pub resources: Vec<Resource>,
    pub res_cursor: usize,
    /// Keyed on `(kind, id)`, not `id` alone: GitHub numbers caches,
    /// artifacts and workflow runs in independent namespaces, so a cache 5
    /// and an artifact 5 are different things that must not share a slot.
    pub selected: HashSet<(ResourceKind, u64)>,
    pub focus: Focus,
    pub sort: SortKey,
    pub filter: String,
    /// True while the user is typing into the filter. Filtering has to be a
    /// mode: without one, the shortcut keys shadow every character they use,
    /// and a cache key containing `s` or `d` becomes untypeable.
    pub filter_mode: bool,
    pub status: String,
    pub should_quit: bool,
    /// The repository `resources` were loaded from. A plan must target this,
    /// not wherever the cursor has wandered since — they are not the same
    /// thing the moment the user moves after loading.
    pub loaded: Option<(String, String)>,
    /// Persistent cursor state for the left (orgs/repos tree) pane. Without
    /// it ratatui only ever draws the rows that fit and the cursor walks off
    /// screen past that point.
    pub org_state: ListState,
    /// Persistent cursor state for the right (resources) pane. Same reason.
    pub res_state: ListState,
    /// The org a running purge belongs to, for the post-purge cache refresh.
    /// The user can navigate away while it runs — purges execute on a
    /// spawned task while the event loop keeps handling keys — so `loaded`
    /// is not it: it can point somewhere else by the time the purge
    /// finishes. Captured when a purge starts, overwritten if a second one
    /// starts before the first's `Finished` lands — the refresh target is
    /// best-effort under overlap, unlike the quit guard below, which must
    /// stay correct.
    pub purging_org: Option<String>,
    /// Which top-level tab is on screen.
    pub view: View,
    /// Index into `BillingReport::months()` for the org under the cursor —
    /// which month the Billing tab shows.
    pub month_cursor: usize,
    /// How many purges are currently running on a spawned task. Incremented
    /// when one starts, decremented when one's `Finished` lands. A single
    /// `Option<String>` cannot represent this: starting a second purge before
    /// the first finishes would let that first `Finished` clear the quit
    /// guard while the second purge is still running, exactly when the guard
    /// must not clear.
    pub purges_in_flight: usize,
    /// Set by a first quit press while a purge is running: it warns instead
    /// of quitting outright, and only a second press goes through. A purge
    /// runs on a spawned task, so an unattended quit would otherwise drop
    /// whatever deletions are still queued with no summary shown. Disarmed by
    /// `purge_finished` only once every in-flight purge has settled.
    pub quit_armed: bool,
    /// The repository ticked for archiving, from the left tree — `(org,
    /// repo)`. A repository lives one level above `resources`, not inside
    /// it, so it cannot share `selected`'s `(ResourceKind, u64)` set the way
    /// every other kind does; this is its own, deliberately single-slot
    /// state instead of a `HashSet`, because `clean::Plan` can only ever
    /// target one repository at a time — there is no "select several repos,
    /// archive them together" shape to build towards. Only ever written by
    /// `toggle_repo_selected`, which refuses anything but
    /// `repos::RepoClass::Archivable` — never by any bulk operation; see
    /// `select_all_stale`'s own guard for why that matters.
    pub selected_repo: Option<(String, String)>,
}

impl App {
    pub fn new(orgs: Vec<OrgSummary>) -> Self {
        App {
            orgs,
            org_cursor: 0,
            repo_cursor: 0,
            resources: Vec::new(),
            res_cursor: 0,
            selected: HashSet::new(),
            focus: Focus::Orgs,
            sort: SortKey::Size,
            filter: String::new(),
            filter_mode: false,
            status: String::new(),
            should_quit: false,
            loaded: None,
            org_state: ListState::default(),
            res_state: ListState::default(),
            purging_org: None,
            view: View::Orgs,
            month_cursor: 0,
            purges_in_flight: 0,
            quit_armed: false,
            selected_repo: None,
        }
    }

    /// Resources after filtering and sorting — what the right pane draws.
    pub fn visible_resources(&self) -> Vec<&Resource> {
        let needle = self.filter.to_lowercase();
        let mut out: Vec<&Resource> = self
            .resources
            .iter()
            .filter(|r| needle.is_empty() || r.label.to_lowercase().contains(&needle))
            .collect();

        match self.sort {
            SortKey::Size => out.sort_by_key(|r| std::cmp::Reverse(r.size_bytes)),
            SortKey::Age => out.sort_by_key(|r| std::cmp::Reverse(r.age_days)),
            SortKey::Name => out.sort_by(|a, b| a.label.cmp(&b.label)),
        }
        out
    }

    /// Toggle the row under the cursor, in the order currently displayed.
    ///
    /// Refuses a branch GitHub itself would refuse to delete — the default
    /// branch (`BranchClass::Default`) or one it protects directly
    /// (`BranchClass::Protected`) — since offering the tick is a lie the API
    /// then contradicts. This is a narrower guard than `Resource.protected`,
    /// which stays the bulk-selection gate it already was (see
    /// `commands::clean::select`) and is deliberately left alone: a
    /// `BranchClass::Live` branch, every tag and every package version stay
    /// individually tickable, exactly the v0.3 decision that a human may
    /// knowingly delete a `latest` tag one row at a time, applied here to a
    /// branch a human recognises as dead even though no merged PR proves it.
    pub fn toggle_selected(&mut self) {
        use crate::refs::BranchClass;

        let Some(r) = self.visible_resources().get(self.res_cursor).copied() else {
            return;
        };

        if r.kind == ResourceKind::Branch
            && matches!(
                r.branch_class,
                Some(BranchClass::Default) | Some(BranchClass::Protected)
            )
        {
            self.status = "Cette branche est protégée par GitHub : sélection refusée.".to_string();
            return;
        }

        let key = (r.kind, r.id);
        if !self.selected.remove(&key) {
            self.selected.insert(key);
        }
    }

    /// Tick or untick the repository row under the cursor, in the left tree —
    /// the repository's own equivalent of `toggle_selected`, since it lives
    /// one level above `resources` and cannot share that method's cursor or
    /// storage.
    ///
    /// Refuses anything but `repos::RepoClass::Archivable`: an
    /// already-archived repository, or one this token cannot administer, is
    /// not tickable at all — GitHub answers 403 to the second, and offering a
    /// tick the API will refuse is a lie the API then contradicts, in front
    /// of the user. The repository-tree analogue of `toggle_selected`'s own
    /// default-branch guard.
    ///
    /// Ticking a different repository replaces whichever one was ticked
    /// before: there is no multi-repository selection to build towards,
    /// since `clean::Plan` — and the archive endpoint itself — can only ever
    /// target one repository at a time.
    pub fn toggle_repo_selected(&mut self) {
        use crate::repos::RepoClass;

        let Some(org) = self.orgs.get(self.org_cursor) else {
            return;
        };
        let Some(repo) = org.repos.get(self.repo_cursor) else {
            return;
        };

        match repo.class {
            RepoClass::AlreadyArchived => {
                self.status = "Ce dépôt est déjà archivé : rien à faire.".to_string();
                return;
            }
            RepoClass::NoAdminRights => {
                self.status = "Droits d'admin requis sur ce dépôt : sélection refusée.".to_string();
                return;
            }
            RepoClass::Archivable => {}
        }

        let key = (org.login.clone(), repo.name.clone());
        if self.selected_repo.as_ref() == Some(&key) {
            self.selected_repo = None;
        } else {
            self.selected_repo = Some(key);
        }
    }

    /// Freeze the ticked repository into an archiving plan.
    ///
    /// Cannot reuse `take_plan`: a repository lives in the tree, not
    /// `resources` (see `App::selected_repo`'s own doc comment), so its
    /// single `Resource` is synthesised here rather than filtered out of a
    /// list it was never a part of. `None` when nothing is ticked, or the
    /// ticked repository has since left the tree — either way, there is
    /// nothing to build a plan from.
    pub fn take_repo_plan(&self) -> Option<Plan> {
        let (owner, repo) = self.selected_repo.clone()?;
        let summary = self
            .orgs
            .iter()
            .find(|o| o.login == owner)?
            .repos
            .iter()
            .find(|r| r.name == repo)?;

        Some(Plan {
            items: vec![Resource {
                kind: ResourceKind::Repository,
                id: crate::api::refs::resource_id(&repo),
                label: repo.clone(),
                size_bytes: 0,
                age_days: summary.age_days,
                git_ref: None,
                stale_pr: false,
                protected: false,
                branch_class: None,
            }],
            owner,
            repo,
        })
    }

    /// Select every ⚑ row: the whole point of the flag is this one keystroke.
    ///
    /// Iterates `visible_resources()`, not `self.resources` — the pane shows
    /// the filtered list, and a bulk select feeding an irreversible delete
    /// must act on what is actually on screen.
    ///
    /// Excludes `ResourceKind::Repository` explicitly, defensively — not
    /// because one can reach `self.resources` today (it can't: a repository
    /// lives in the tree, see `App::selected_repo`), but because nothing
    /// else here would stop it if one ever did. `[A]` must never take a
    /// repository, at any age: `pushed_at` alone is not proof of
    /// abandonment, and archiving is the only family in this product with no
    /// preselection path whatsoever — see `bulk_selection_never_takes_a_repository`.
    pub fn select_all_stale(&mut self) {
        let keys: Vec<(ResourceKind, u64)> = self
            .visible_resources()
            .into_iter()
            .filter(|r| r.stale_pr)
            .filter(|r| r.kind != ResourceKind::Repository)
            .map(|r| (r.kind, r.id))
            .collect();
        self.selected.extend(keys);
    }

    pub fn cycle_sort(&mut self) {
        self.sort = match self.sort {
            SortKey::Size => SortKey::Age,
            SortKey::Age => SortKey::Name,
            SortKey::Name => SortKey::Size,
        };
        self.res_cursor = 0;
    }

    /// Reset the cursors scoped beneath the org cursor, on every org move.
    ///
    /// `repo_cursor` already did this. `month_cursor` did not: paging to
    /// month 5 on a six-month org, then switching to a two-month org, left
    /// the cursor at 5 — the Billing tab clamps it on render, but the cursor
    /// itself stayed stranded high, so `←` read as dead until it walked all
    /// the way back down on its own.
    pub fn reset_scoped_cursors(&mut self) {
        self.repo_cursor = 0;
        self.month_cursor = 0;
    }

    pub fn selection_bytes(&self) -> u64 {
        self.resources
            .iter()
            .filter(|r| self.selected.contains(&(r.kind, r.id)))
            .map(|r| r.size_bytes)
            .sum()
    }

    /// `(org, repo)` under the cursor, as owned strings.
    ///
    /// Owned rather than borrowed on purpose: every caller goes on to mutate
    /// `app`, and holding a borrow into `self.orgs` across that mutation does
    /// not borrow-check.
    pub fn current_target(&self) -> Option<(String, String)> {
        let org = self.orgs.get(self.org_cursor)?;
        let repo = org.repos.get(self.repo_cursor)?;
        Some((org.login.clone(), repo.name.clone()))
    }

    /// Freeze the current selection into a plan, targeting the repository
    /// `resources` was loaded from — not wherever the cursor sits now.
    /// `resources` only changes on `Enter`, so a plan built from the live
    /// cursor position can target a repository the user only glanced at
    /// afterwards. `None` when nothing has been loaded yet.
    pub fn take_plan(&self) -> Option<Plan> {
        let (owner, repo) = self.loaded.clone()?;
        Some(Plan {
            items: self
                .resources
                .iter()
                .filter(|r| self.selected.contains(&(r.kind, r.id)))
                .cloned()
                .collect(),
            owner,
            repo,
        })
    }

    /// Update one org's cache figures from a fresh `usage_by_repository`
    /// report, after a purge.
    ///
    /// Updates the existing rows in place rather than replacing `org.repos`
    /// wholesale: `scan::overview` deliberately adds every repo the org has,
    /// including cache-free ones, because they may still hold artifacts or
    /// runs stage 2 can surface. Replacing the vec with the fresh report
    /// would drop every repo the report has nothing to say about — a repo
    /// missing from it has simply lost the last of its cache, not left the
    /// tree. This also does not re-sort: the order was fixed at scan time,
    /// and reordering right after a purge would move rows out from under the
    /// user's cursor, which is worse than a now-stale size order.
    pub fn refresh_org_cache(&mut self, org_login: &str, fresh: Vec<RepoSummary>) {
        let Some(org) = self.orgs.iter_mut().find(|o| o.login == org_login) else {
            return;
        };
        for existing in org.repos.iter_mut() {
            let found = fresh.iter().find(|r| r.name == existing.name);
            existing.cache_bytes = found.map_or(0, |r| r.cache_bytes);
            existing.cache_count = found.map_or(0, |r| r.cache_count);
        }
        org.cache_bytes = org.repos.iter().map(|r| r.cache_bytes).sum();
        org.cache_count = org.repos.iter().map(|r| r.cache_count).sum();
    }

    /// Record that one purge's `Finished` message landed.
    ///
    /// Decrements `purges_in_flight` and disarms the quit guard only once it
    /// reaches zero. Starting a second purge before the first finishes must
    /// not let that first `Finished` clear the guard while the second purge
    /// is still running — `q` would then quit silently, exactly the case the
    /// guard exists to prevent.
    pub fn purge_finished(&mut self) {
        self.purges_in_flight = self.purges_in_flight.saturating_sub(1);
        if self.purges_in_flight == 0 {
            self.quit_armed = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RepoSummary, ResourceKind};

    fn res(id: u64, label: &str, size: u64, age: i64, stale: bool) -> Resource {
        Resource {
            kind: ResourceKind::Cache,
            id,
            label: label.to_string(),
            size_bytes: size,
            age_days: age,
            git_ref: None,
            stale_pr: stale,
            protected: false,
            branch_class: None,
        }
    }

    fn app() -> App {
        let mut a = App::new(vec![]);
        a.resources = vec![
            res(1, "coverage-linux", 300, 40, true),
            res(2, "ubuntu-22-test", 200, 5, false),
            res(3, "coverage-macos", 100, 20, true),
        ];
        a
    }

    #[test]
    fn select_all_stale_takes_only_flagged_items() {
        let mut a = app();
        a.select_all_stale();
        assert_eq!(a.selection_bytes(), 400);
        assert!(a.selected.contains(&(ResourceKind::Cache, 1)));
        assert!(a.selected.contains(&(ResourceKind::Cache, 3)));
        assert!(!a.selected.contains(&(ResourceKind::Cache, 2)));
    }

    /// THE test of this whole version, per the task-4 brief's own
    /// self-review: without this, a future extension of `[A]` would flip a
    /// whole organisation to read-only on one keystroke. `[A]` must never
    /// take a repository, at any age — `pushed_at` alone is not proof of
    /// abandonment, and this is the only family in the entire product with
    /// no preselection path whatsoever.
    ///
    /// A `Repository` resource can never actually reach `self.resources` in
    /// production today — a repository lives in the tree, not the right-hand
    /// list (see `App::selected_repo`) — which is exactly why this guard has
    /// to be asserted defensively rather than trusted as an emergent
    /// property of "nothing puts one there": `select_all_stale` filters
    /// purely on `r.stale_pr`, with no kind exclusion of its own, so a
    /// future refactor that ever did merge a repository row into
    /// `resources` would silently start bulk-selecting it the moment that
    /// row also happened to carry `stale_pr: true`. The fixture below
    /// carries a `Repository` item of every age precisely to prove the
    /// exclusion holds regardless of how old — not just that today's
    /// plumbing happens not to produce one.
    #[test]
    fn bulk_selection_never_takes_a_repository() {
        let mut a = App::new(vec![]);
        a.resources = vec![
            Resource {
                kind: ResourceKind::Repository,
                id: 1,
                label: "young-repo".into(),
                size_bytes: 0,
                age_days: 10,
                git_ref: None,
                stale_pr: true,
                protected: false,
                branch_class: None,
            },
            Resource {
                kind: ResourceKind::Repository,
                id: 2,
                label: "lokiprint".into(),
                size_bytes: 0,
                age_days: 685,
                git_ref: None,
                stale_pr: true,
                protected: false,
                branch_class: None,
            },
            Resource {
                kind: ResourceKind::Repository,
                id: 3,
                label: ".github".into(),
                size_bytes: 0,
                age_days: 775,
                git_ref: None,
                stale_pr: true,
                protected: false,
                branch_class: None,
            },
        ];

        a.select_all_stale();

        assert!(
            a.selected.is_empty(),
            "a Repository must never be bulk-selected, at any age: got {:?}",
            a.selected
        );
    }

    /// Locks the fix for the cross-cutting review's most severe finding:
    /// caches, artifacts and workflow runs number their ids in independent
    /// namespaces, so a cache 5 and an artifact 5 must not share a selection
    /// slot. On the old `HashSet<u64>` this test fails — toggling the
    /// artifact also selects the cache, and `selection_bytes` double-counts.
    #[test]
    fn selection_is_keyed_by_kind_not_just_id() {
        let mut a = App::new(vec![]);
        a.resources = vec![
            Resource {
                kind: ResourceKind::Cache,
                id: 5,
                label: "cache-5".into(),
                size_bytes: 100,
                age_days: 1,
                git_ref: None,
                stale_pr: false,
                protected: false,
                branch_class: None,
            },
            Resource {
                kind: ResourceKind::Artifact,
                id: 5,
                label: "artifact-5".into(),
                size_bytes: 200,
                age_days: 1,
                git_ref: None,
                stale_pr: false,
                protected: false,
                branch_class: None,
            },
        ];
        // Default sort is by size descending, so the artifact (200) is row 0
        // and the cache (100) is row 1.
        a.res_cursor = 0;
        a.toggle_selected();

        assert!(a.selected.contains(&(ResourceKind::Artifact, 5)));
        assert!(!a.selected.contains(&(ResourceKind::Cache, 5)));
        assert_eq!(a.selection_bytes(), 200);
    }

    /// Locks finding 4's third leg: `[A]` must act on what is visible, not on
    /// everything loaded. Before the fix, `select_all_stale` iterated
    /// `self.resources` directly and selected rows the filter was hiding.
    #[test]
    fn select_all_stale_does_not_select_rows_hidden_by_the_filter() {
        let mut a = app();
        // Hides "coverage-macos" (id 3, stale) but keeps "coverage-linux"
        // (id 1, stale) visible.
        a.filter = "linux".into();
        a.select_all_stale();

        assert!(a.selected.contains(&(ResourceKind::Cache, 1)));
        assert!(!a.selected.contains(&(ResourceKind::Cache, 3)));
    }

    #[test]
    fn the_filter_matches_labels_case_insensitively() {
        let mut a = app();
        a.filter = "COVERAGE".into();
        let visible: Vec<u64> = a.visible_resources().iter().map(|r| r.id).collect();
        assert_eq!(visible, vec![1, 3]);
    }

    #[test]
    fn sorting_cycles_size_then_age_then_name() {
        let mut a = app();
        assert_eq!(a.sort, SortKey::Size);
        assert_eq!(a.visible_resources()[0].id, 1);

        a.cycle_sort();
        assert_eq!(a.sort, SortKey::Age);
        assert_eq!(a.visible_resources()[0].id, 1);

        a.cycle_sort();
        assert_eq!(a.sort, SortKey::Name);
        assert_eq!(a.visible_resources()[0].label, "coverage-linux");

        a.cycle_sort();
        assert_eq!(a.sort, SortKey::Size);
    }

    #[test]
    fn toggling_twice_clears_the_selection() {
        let mut a = app();
        a.res_cursor = 1;
        a.toggle_selected();
        assert_eq!(a.selection_bytes(), 200);
        a.toggle_selected();
        assert_eq!(a.selection_bytes(), 0);
    }

    /// `protected: class != BranchClass::Merged` and `branch_class:
    /// Some(class)` mirror exactly what `scan::branch_resources` sets.
    fn branch(id: u64, label: &str, class: crate::refs::BranchClass) -> Resource {
        Resource {
            kind: ResourceKind::Branch,
            id,
            label: label.to_string(),
            size_bytes: 0,
            age_days: 0,
            git_ref: None,
            stale_pr: false,
            protected: class != crate::refs::BranchClass::Merged,
            branch_class: Some(class),
        }
    }

    /// Finding 2 of the v0.4 final review: `toggle_selected` had no guard at
    /// all, so the default branch — GitHub refuses to delete it outright —
    /// could be ticked and queued for deletion like any other row. A wrong
    /// fix that reused `Resource.protected` for this guard would also
    /// refuse a `Live` branch and a protected tag, which must stay tickable
    /// (the discriminating tests below), so this asserts on `branch_class`
    /// directly.
    #[test]
    fn toggle_selected_refuses_the_default_branch() {
        use crate::refs::BranchClass;
        let mut a = App::new(vec![]);
        a.resources = vec![branch(1, "main", BranchClass::Default)];
        a.res_cursor = 0;

        a.toggle_selected();

        assert!(
            a.selected.is_empty(),
            "the default branch must not be selectable"
        );
        assert!(
            !a.status.is_empty(),
            "refusing silently would look like a dead key"
        );
    }

    #[test]
    fn toggle_selected_refuses_a_github_protected_branch() {
        use crate::refs::BranchClass;
        let mut a = App::new(vec![]);
        a.resources = vec![branch(1, "release/2.0", BranchClass::Protected)];
        a.res_cursor = 0;

        a.toggle_selected();

        assert!(
            a.selected.is_empty(),
            "a GitHub-protected branch must not be selectable"
        );
    }

    /// The discriminating case: a `Live` branch is merely unmerged, nothing
    /// GitHub itself refuses, so a human may still knowingly delete it one
    /// row at a time — the same v0.3 reasoning kept for tags below. A guard
    /// keyed on `Resource.protected` (which is `true` for `Live` too) would
    /// wrongly refuse this and pass the two tests above regardless.
    #[test]
    fn toggle_selected_still_allows_a_live_unmerged_branch() {
        use crate::refs::BranchClass;
        let mut a = App::new(vec![]);
        a.resources = vec![branch(1, "feature/rejected", BranchClass::Live)];
        a.res_cursor = 0;

        a.toggle_selected();

        assert!(
            a.selected.contains(&(ResourceKind::Branch, 1)),
            "an unmerged branch must stay individually selectable"
        );
    }

    #[test]
    fn toggle_selected_still_allows_a_merged_branch() {
        use crate::refs::BranchClass;
        let mut a = App::new(vec![]);
        a.resources = vec![branch(1, "claude/landing-3jbqk4", BranchClass::Merged)];
        a.res_cursor = 0;

        a.toggle_selected();

        assert!(a.selected.contains(&(ResourceKind::Branch, 1)));
    }

    /// The v0.3 decision this task must not disturb: a human looking at a
    /// protected tag (or a tagged package version) may still knowingly
    /// delete it one row at a time. The new branch guard is scoped to
    /// `ResourceKind::Branch` only — this proves it does not leak onto a
    /// different kind that also happens to carry `protected: true`.
    #[test]
    fn toggle_selected_still_allows_a_protected_tag() {
        let mut a = App::new(vec![]);
        a.resources = vec![Resource {
            kind: ResourceKind::Tag,
            id: 1,
            label: "v0.1.3".into(),
            size_bytes: 0,
            age_days: 0,
            git_ref: None,
            stale_pr: false,
            protected: true,
            branch_class: None,
        }];
        a.res_cursor = 0;

        a.toggle_selected();

        assert!(
            a.selected.contains(&(ResourceKind::Tag, 1)),
            "a protected tag must stay individually selectable, per the v0.3 decision"
        );
    }

    #[test]
    fn current_target_is_none_without_orgs() {
        assert_eq!(app().current_target(), None);
    }

    #[test]
    fn current_target_follows_both_cursors() {
        let repo = |name: &str| RepoSummary {
            name: name.to_string(),
            cache_bytes: 0,
            cache_count: 0,
            private: false,
            age_days: 0,
            class: crate::repos::RepoClass::Archivable,
        };
        let mut a = App::new(vec![OrgSummary {
            login: "systm-d".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![repo("josephine"), repo("claudine")],
            billing: None,
        }]);
        a.repo_cursor = 1;

        assert_eq!(
            a.current_target(),
            Some(("systm-d".to_string(), "claudine".to_string()))
        );
    }

    /// Locks finding 3: `resources` only refreshes on `Enter`, so a plan must
    /// target `loaded`, not wherever the cursors sit when `d` is pressed. On
    /// the old `take_plan(&self, owner, repo)` reading the live cursor, a
    /// plan built after the cursor wanders would name the wrong repository.
    #[test]
    fn take_plan_targets_the_loaded_repo_not_the_wandered_cursor() {
        let repo = |name: &str| RepoSummary {
            name: name.to_string(),
            cache_bytes: 0,
            cache_count: 0,
            private: false,
            age_days: 0,
            class: crate::repos::RepoClass::Archivable,
        };
        let mut a = App::new(vec![
            OrgSummary {
                login: "claudine-org".into(),
                cache_bytes: 0,
                cache_count: 0,
                repos: vec![repo("claudine")],
                billing: None,
            },
            OrgSummary {
                login: "josephine-org".into(),
                cache_bytes: 0,
                cache_count: 0,
                repos: vec![repo("josephine")],
                billing: None,
            },
        ]);
        a.resources = vec![res(1, "cache-1", 100, 1, false)];
        a.loaded = Some(("claudine-org".to_string(), "claudine".to_string()));
        a.selected.insert((ResourceKind::Cache, 1));

        // The cursor wanders to a different org/repo after the load.
        a.org_cursor = 1;
        a.repo_cursor = 0;
        assert_eq!(
            a.current_target(),
            Some(("josephine-org".to_string(), "josephine".to_string()))
        );

        let plan = a.take_plan().expect("a repo was loaded");
        assert_eq!(plan.owner, "claudine-org");
        assert_eq!(plan.repo, "claudine");
        assert_eq!(plan.items.len(), 1);
    }

    #[test]
    fn take_plan_is_none_before_anything_loads() {
        assert!(app().take_plan().is_none());
    }

    /// `RepoSummary` fixture with an explicit class and age — every repo-tree
    /// test below needs to control both, unlike `current_target`'s helper
    /// closures which hardcode a benign `Archivable`/`0`.
    fn repo_summary(name: &str, class: crate::repos::RepoClass, age_days: i64) -> RepoSummary {
        RepoSummary {
            name: name.to_string(),
            cache_bytes: 0,
            cache_count: 0,
            private: false,
            age_days,
            class,
        }
    }

    fn app_with_one_repo(class: crate::repos::RepoClass) -> App {
        let mut a = App::new(vec![OrgSummary {
            login: "maxds-lyon".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![repo_summary("lokiprint", class, 685)],
            billing: None,
        }]);
        a.focus = Focus::Repos;
        a
    }

    #[test]
    fn toggle_repo_selected_ticks_an_archivable_repo() {
        let mut a = app_with_one_repo(crate::repos::RepoClass::Archivable);
        a.toggle_repo_selected();
        assert_eq!(
            a.selected_repo,
            Some(("maxds-lyon".to_string(), "lokiprint".to_string()))
        );
    }

    #[test]
    fn toggle_repo_selected_untick_by_toggling_twice() {
        let mut a = app_with_one_repo(crate::repos::RepoClass::Archivable);
        a.toggle_repo_selected();
        a.toggle_repo_selected();
        assert_eq!(a.selected_repo, None);
    }

    /// Rule 2: an already-archived repo is not tickable at all — this is a
    /// harder refusal than `toggle_selected`'s branch guard, which still
    /// allows an individual tick on plenty of "protected" rows (a live
    /// branch, any tag). Here there is no override.
    #[test]
    fn toggle_repo_selected_refuses_an_already_archived_repo() {
        let mut a = app_with_one_repo(crate::repos::RepoClass::AlreadyArchived);
        a.toggle_repo_selected();
        assert_eq!(
            a.selected_repo, None,
            "an already-archived repo must never become tickable"
        );
        assert!(
            !a.status.is_empty(),
            "refusing silently would look like a dead key"
        );
    }

    /// Rule 2's other half: GitHub would answer 403 to an archive request
    /// this token cannot administer. Offering the tick anyway is a lie the
    /// API then contradicts, in front of the user.
    #[test]
    fn toggle_repo_selected_refuses_a_repo_without_admin_rights() {
        let mut a = app_with_one_repo(crate::repos::RepoClass::NoAdminRights);
        a.toggle_repo_selected();
        assert_eq!(a.selected_repo, None);
        assert!(!a.status.is_empty());
    }

    #[test]
    fn take_repo_plan_is_none_when_nothing_is_ticked() {
        let a = app_with_one_repo(crate::repos::RepoClass::Archivable);
        assert!(a.take_repo_plan().is_none());
    }

    /// The plan `clean::execute`'s `Repository` arm needs: one `Resource` of
    /// that kind, scoped to the ticked repository, carrying its real age —
    /// not the resource-list plumbing `take_plan` builds, which this
    /// repository was never a part of.
    #[test]
    fn take_repo_plan_targets_the_ticked_repo() {
        let mut a = app_with_one_repo(crate::repos::RepoClass::Archivable);
        a.toggle_repo_selected();

        let plan = a.take_repo_plan().expect("a repo was ticked");
        assert_eq!(plan.owner, "maxds-lyon");
        assert_eq!(plan.repo, "lokiprint");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].kind, ResourceKind::Repository);
        assert_eq!(plan.items[0].label, "lokiprint");
        assert_eq!(plan.items[0].age_days, 685);
    }

    /// Locks the re-review's first regression: `scan::overview` deliberately
    /// keeps cache-free repos in the tree (they may still hold artifacts or
    /// runs), so a post-purge refresh must not replace `org.repos` wholesale
    /// with a report that only lists repos that still have a cache — that
    /// would silently drop every cache-free repo, including the one just
    /// purged to zero, from the tree.
    #[test]
    fn refresh_org_cache_keeps_repos_the_fresh_report_omits() {
        let repo = |name: &str, bytes: u64, count: u32| RepoSummary {
            name: name.to_string(),
            cache_bytes: bytes,
            cache_count: count,
            private: false,
            age_days: 0,
            class: crate::repos::RepoClass::Archivable,
        };
        let mut a = App::new(vec![OrgSummary {
            login: "systm-d".into(),
            cache_bytes: 11_130_027_303,
            cache_count: 69,
            // "josephine" holds no cache — `scan::overview` put it here
            // anyway because it may hold artifacts or runs.
            repos: vec![
                repo("claudine", 11_130_027_303, 69),
                repo("josephine", 0, 0),
            ],
            billing: None,
        }]);

        // The fresh report is exactly what usage-by-repository returns after
        // purging "claudine" to zero: it omits any repo with no cache left,
        // "josephine" included.
        a.refresh_org_cache("systm-d", vec![]);

        assert_eq!(
            a.orgs[0].repos.len(),
            2,
            "no repo should vanish from the tree"
        );
        assert_eq!(
            a.orgs[0].repos[0].name, "claudine",
            "order must stay stable"
        );
        assert_eq!(a.orgs[0].repos[0].cache_bytes, 0);
        assert_eq!(a.orgs[0].repos[1].name, "josephine");
        assert_eq!(a.orgs[0].repos[1].cache_bytes, 0);
        assert_eq!(a.orgs[0].cache_bytes, 0);
        assert_eq!(a.orgs[0].cache_count, 0);
    }

    /// Locks finding 2: a second purge started before the first's `Finished`
    /// message lands must not disarm the quit guard early. On the old
    /// `purging_org: Option<String>` clearing `quit_armed` unconditionally on
    /// every `Finished`, the first purge finishing would disarm the guard
    /// while the second is still running — `q` would then quit silently,
    /// exactly the case the guard exists to prevent.
    #[test]
    fn purge_finished_disarms_the_guard_only_once_every_purge_has_settled() {
        let mut a = App::new(vec![]);
        a.purges_in_flight = 2;
        a.quit_armed = true;

        a.purge_finished();
        assert_eq!(a.purges_in_flight, 1);
        assert!(
            a.quit_armed,
            "a second purge is still running; the guard must stay armed"
        );

        a.purge_finished();
        assert_eq!(a.purges_in_flight, 0);
        assert!(
            !a.quit_armed,
            "the last purge settled; the guard must disarm"
        );
    }

    /// Locks finding 4: paging to month 5 on an org with six months, then
    /// switching orgs, must not strand `month_cursor` at 5. The render
    /// clamps it for display, but the cursor itself stayed put on the old
    /// code, so `←` read as dead until it was pressed enough times to walk
    /// back down on its own.
    #[test]
    fn reset_scoped_cursors_clears_repo_and_month_cursors() {
        let mut a = App::new(vec![]);
        a.repo_cursor = 3;
        a.month_cursor = 5;

        a.reset_scoped_cursors();

        assert_eq!(a.repo_cursor, 0);
        assert_eq!(a.month_cursor, 0);
    }

    #[test]
    fn focus_cycles_through_all_three_levels() {
        // The repo level was unreachable at one point because Focus only had
        // two variants; this locks the tree's shape.
        let mut f = Focus::Orgs;
        for expected in [Focus::Repos, Focus::Resources, Focus::Orgs] {
            f = match f {
                Focus::Orgs => Focus::Repos,
                Focus::Repos => Focus::Resources,
                Focus::Resources => Focus::Orgs,
            };
            assert_eq!(f, expected);
        }
    }
}
