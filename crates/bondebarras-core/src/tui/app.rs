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
    /// The org a running purge belongs to. The user can navigate away while
    /// it runs — purges execute on a spawned task while the event loop keeps
    /// handling keys — so `loaded` is not it: it can point somewhere else by
    /// the time the purge finishes. Captured when the purge starts.
    pub purging_org: Option<String>,
    /// Which top-level tab is on screen.
    pub view: View,
    /// Index into `BillingReport::months()` for the org under the cursor —
    /// which month the Billing tab shows.
    pub month_cursor: usize,
    /// Set by a first quit press while a purge is running: it warns instead
    /// of quitting outright, and only a second press goes through. A purge
    /// runs on a spawned task, so an unattended quit would otherwise drop
    /// whatever deletions are still queued with no summary shown. Reset on
    /// `Progress::Finished`.
    pub quit_armed: bool,
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
            quit_armed: false,
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
    pub fn toggle_selected(&mut self) {
        let Some(key) = self
            .visible_resources()
            .get(self.res_cursor)
            .map(|r| (r.kind, r.id))
        else {
            return;
        };
        if !self.selected.remove(&key) {
            self.selected.insert(key);
        }
    }

    /// Select every ⚑ row: the whole point of the flag is this one keystroke.
    ///
    /// Iterates `visible_resources()`, not `self.resources` — the pane shows
    /// the filtered list, and a bulk select feeding an irreversible delete
    /// must act on what is actually on screen.
    pub fn select_all_stale(&mut self) {
        let keys: Vec<(ResourceKind, u64)> = self
            .visible_resources()
            .into_iter()
            .filter(|r| r.stale_pr)
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
            },
            Resource {
                kind: ResourceKind::Artifact,
                id: 5,
                label: "artifact-5".into(),
                size_bytes: 200,
                age_days: 1,
                git_ref: None,
                stale_pr: false,
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
