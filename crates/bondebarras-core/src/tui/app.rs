//! Application state: navigation, selection, sorting and filtering.
//!
//! Selection primitives are deliberately ad hoc — sort, filter, flag-select —
//! and nothing is persisted. There is no rules engine and no config file:
//! the user decides, every time.

use crate::clean::Plan;
use crate::model::{OrgSummary, RepoSummary, Resource, ResourceKind};
use crate::safety::Safety;
use crate::tui::views::progress::{self, Work};
use ratatui::widgets::ListState;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Which column the keyboard drives, each with its own cursor: the orgs,
/// the current org's repositories, then the loaded repository's resources.
///
/// The three values used to name the levels of a folded tree; since spec
/// §2 they name the three columns of `tui::views::column_areas`, whichever
/// of them the terminal's width leaves on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Orgs,
    Repos,
    Resources,
}

impl Focus {
    /// The column to the right, wrapping from the resources back to the
    /// orgs — where `→` and `Tab` move.
    pub fn next(self) -> Focus {
        match self {
            Focus::Orgs => Focus::Repos,
            Focus::Repos => Focus::Resources,
            Focus::Resources => Focus::Orgs,
        }
    }

    /// The column to the left, wrapping from the orgs round to the
    /// resources — where `←` moves.
    pub fn previous(self) -> Focus {
        match self {
            Focus::Orgs => Focus::Resources,
            Focus::Repos => Focus::Orgs,
            Focus::Resources => Focus::Repos,
        }
    }
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

/// How long the column-2 cursor rests on a repository before its resources
/// load — spec §3. Loading one costs nine requests
/// (`scan::repo_detail_with_warnings`): walking the cursor down an org's 33
/// repositories would otherwise spend 297 of them on rows the user only
/// passed over.
pub const LOAD_PAUSE: Duration = Duration::from_millis(300);

/// Which load a listing belongs to.
///
/// Minted by `App::begin_load` alone — its field is private — carried by the
/// task that fetches the listing, inside its `Load`, and compared by
/// `App::accepts_load` when the listing lands. A listing whose generation is
/// no longer current is dropped: since it started, the cursor has left its
/// repository or a purge has made it suspect. A type, not a convention — the
/// shape `clean::Progress` gave a purge's identity in v0.5, after an archive
/// resolved against whatever the tree had ticked when it finished rather
/// than against its own target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadGeneration(u64);

/// One repository load: the repository, and the generation it was started
/// under. The event loop hands it to the task fetching the listing and gets
/// it back with the listing (`App::land_load`), so a listing always lands
/// with its own identity, never the cursor's.
#[derive(Debug)]
pub struct Load {
    generation: LoadGeneration,
    pub org: String,
    pub repo: String,
}

impl Load {
    /// The generation this load was started under, for the ticks its task
    /// sends (`App::load_ticked`): they travel apart from its listing, and
    /// must carry the same identity.
    pub fn generation(&self) -> LoadGeneration {
        self.generation
    }
}

/// What the resources column shows, measured against the repository under
/// the column-2 cursor — see `App::shown`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// No repository under the cursor, and none loaded: no org, or an org
    /// without repositories.
    Nothing,
    /// `App::resources` is this repository's listing.
    Listing { org: String, repo: String },
    /// This repository's listing is on its way: its pause is running, or
    /// its load is in flight.
    Loading { org: String, repo: String },
    /// This repository's last load failed. The pause does not retry it;
    /// `Entrée` does.
    Failed { org: String, repo: String },
}

impl Shown {
    /// Whether the column draws `App::resources`: for a listing, or for the
    /// nothing an org without repositories has. Never while a listing is on
    /// its way or has failed — `resources` then holds nothing of the
    /// repository the column names, and an empty list would read as "this
    /// repository holds nothing" (spec §3).
    pub fn draws_resources(&self) -> bool {
        matches!(self, Shown::Nothing | Shown::Listing { .. })
    }
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
    /// The repository `resources` were loaded from, and the one a plan
    /// targets (`take_plan`). It follows the column-2 cursor
    /// (`follow_cursor`): `None` from the moment the cursor leaves it until
    /// the next repository's listing is shown. A purge's own messages still
    /// name their repository rather than reading this — the cursor, and so
    /// this, can move while the purge runs.
    pub loaded: Option<(String, String)>,
    /// Persistent cursor state for the orgs column. Without it ratatui only
    /// ever draws the rows that fit and the cursor walks off screen past
    /// that point.
    pub org_state: ListState,
    /// Persistent cursor state for the repos column. Same reason.
    pub repo_state: ListState,
    /// Persistent cursor state for the resources column. Same reason.
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
    /// The purge or archive in flight, if any.
    ///
    /// One slot for every purge running at once, apart from `loading`: the
    /// cursor stays free during a purge, so a load can start while deletions
    /// land, and one slot for both would let either announce the end of the
    /// other. Set by `purge_launched`, advanced by `purge_advanced`, cleared
    /// by `purge_finished` once the last purge in flight has finished.
    pub purge: Option<Work>,
    /// The repository drill-down in flight, if any.
    ///
    /// The load of `in_flight`, under `load_generation`: set when that load
    /// starts, advanced by the ticks its task sends (`load_ticked`), and
    /// cleared wherever `in_flight` is.
    pub loading: Option<Work>,
    /// The repository ticked for archiving, from the repos column — `(org,
    /// repo)`. A repository lives one level above `resources`, not inside
    /// it, so it cannot share `selected`'s `(ResourceKind, u64)` set the way
    /// every other kind does; this is its own, deliberately single-slot
    /// state instead of a `HashSet`, because `clean::Plan` can only ever
    /// target one repository at a time — there is no "select several repos,
    /// archive them together" shape to build towards. Only ever set by
    /// `toggle_repo_selected`, which refuses anything but
    /// `repos::RepoClass::Archivable` — never by any bulk operation; see
    /// `select_levels`'s own guard for why that matters.
    ///
    /// It survives the repos cursor, and the listings that land as it moves
    /// (ruling R7-1, 2026-09-11): only an untick, the end of its own archive
    /// (`archive_done`, `archive_failed`) or an org move
    /// (`reset_scoped_cursors`) clears it.
    pub selected_repo: Option<(String, String)>,
    /// Every repository listing fetched this session, by `(org, repo)` —
    /// spec §3: a repository already loaded shows at once, with no request.
    /// Written by `land_load` (a refresh by `Entrée` replaces it), kept
    /// current row by row while a purge deletes from it (`resource_deleted`),
    /// and dropped when that purge ends (`purge_ended`).
    pub repo_cache: HashMap<(String, String), Vec<Resource>>,
    /// The families whose listing was refused when a kept listing was
    /// fetched, under the same key — ruling F2 (2026-09-11): kept with the
    /// listing and said again on every visit, so kept rows never read as if
    /// a refused family were empty. No entry when nothing was refused.
    repo_refused: HashMap<(String, String), Vec<&'static str>>,
    /// How many purges in flight concern each repository — recorded from
    /// each plan when it is launched (`purge_launched`), released when that
    /// purge ends (`purge_ended`). No load starts on its own for a
    /// repository recorded here (`follow_cursor`).
    purging_repos: HashMap<(String, String), usize>,
    /// The generation a listing must carry to be accepted when it lands
    /// (`accepts_load`). Moves on whenever an outstanding load stops being
    /// wanted: another load begins (`begin_load`), the cursor leaves its
    /// repository (`follow_cursor`), or a purge makes its listing suspect
    /// (`forget`).
    pub load_generation: u64,
    /// When the repository under the column-2 cursor started waiting for its
    /// load: `follow_cursor` starts the load `LOAD_PAUSE` later. `None` once
    /// the load has started, and whenever nothing waits.
    pub pending_since: Option<Instant>,
    /// The repository under the column-2 cursor when `follow_cursor` last
    /// looked — what a change is measured against. `None` before the first
    /// look, so the repository under the cursor at start-up counts as a
    /// change and loads after the same pause.
    cursor_repo: Option<(String, String)>,
    /// The repository whose load is in flight under `load_generation`.
    in_flight: Option<(String, String)>,
    /// The repository whose last load failed, until the cursor leaves it or
    /// `Entrée` retries it.
    load_failed: Option<(String, String)>,
    /// The message the last load wrote to `status` — a refused family's
    /// warning, or a failed load's error — so the repository's next listing,
    /// or the cursor leaving it, clears that message and never one written
    /// since: a purge's recap, the quit guard's warning. A listing lands on
    /// its own time, not on a key.
    load_warning: Option<String>,
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
            repo_state: ListState::default(),
            res_state: ListState::default(),
            purging_org: None,
            view: View::Orgs,
            month_cursor: 0,
            purges_in_flight: 0,
            quit_armed: false,
            purge: None,
            loading: None,
            selected_repo: None,
            repo_cache: HashMap::new(),
            repo_refused: HashMap::new(),
            purging_repos: HashMap::new(),
            load_generation: 0,
            pending_since: None,
            cursor_repo: None,
            in_flight: None,
            load_failed: None,
            load_warning: None,
        }
    }

    /// Resources after filtering and sorting — what the resources column draws.
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

    /// Tick or untick the repository row under the cursor, in the repos column —
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
                safety: crate::safety::Safety::Keep,
            }],
            owner,
            repo,
        })
    }

    /// Apply a `Progress::Done` for a repository archive: mark that
    /// repository's own row read-only, and clear the tree's tick — but only
    /// when it still points at *this* repository.
    ///
    /// Findings 3 and 4 of the final review: an earlier version read
    /// `app.selected_repo` unconditionally to find both which row to update
    /// and whether to clear the tick, so a second archive ticked (and so
    /// overwriting `selected_repo`) before this one's `Done` landed made the
    /// update land on the wrong row and dropped a still-in-flight archive's
    /// own tick. `owner`/`repo` come from the `Progress` message itself —
    /// carried since `clean::execute`, the operation's own identity — never
    /// from whatever the tree currently has ticked.
    pub fn archive_done(&mut self, owner: &str, repo: &str) {
        if let Some(r) = self
            .orgs
            .iter_mut()
            .find(|o| o.login == owner)
            .and_then(|org| org.repos.iter_mut().find(|r| r.name == repo))
        {
            r.class = crate::repos::RepoClass::AlreadyArchived;
        }
        if self
            .selected_repo
            .as_ref()
            .is_some_and(|(o, r)| o == owner && r == repo)
        {
            self.selected_repo = None;
        }
    }

    /// Apply a `Progress::Failed` for a repository archive: clear the
    /// tree's tick, but only when it still points at *this* repository —
    /// same reasoning as `archive_done`. A refused archive for one
    /// repository must not drop a different, still in-flight archive's own
    /// tick.
    pub fn archive_failed(&mut self, owner: &str, repo: &str) {
        if self
            .selected_repo
            .as_ref()
            .is_some_and(|(o, r)| o == owner && r == repo)
        {
            self.selected_repo = None;
        }
    }

    /// The plan `[d]` builds: the plan of the column focus is on, never
    /// whichever of `take_repo_plan`/`take_plan` happens to return `Some`.
    ///
    /// - The repos column archives the ticked repository
    ///   (`take_repo_plan`), wherever the repos cursor has moved since: the
    ///   tick survives the cursor (ruling R7-1), and the archive modal names
    ///   the repository it targets.
    /// - The resources column deletes the ticked resources (`take_plan`).
    /// - The orgs column owns no plan (Task 7): `None`. With one column on
    ///   screen and focus there, the resource list is off screen, and a plan
    ///   built from resources ticked earlier would reach a Tier 1
    ///   confirmation that does not list them.
    ///
    /// Finding 1 of the final review: the old dispatch was
    /// `take_repo_plan().or_else(|| take_plan())`, so a repository ticked
    /// earlier outranked a resource selection made afterward. `Focus::Repos`
    /// is the only focus `toggle_repo_selected` can be reached from (see
    /// `tui::column_action`, which `[espace]` goes through), so it is also
    /// the only focus this archives from: a repository ticked, then left
    /// ticked while the user `Tab`s over to the resources column, never
    /// outranks the resources ticked there.
    pub fn take_focused_plan(&self) -> Option<Plan> {
        match self.focus {
            Focus::Repos => self.take_repo_plan(),
            Focus::Resources => self.take_plan(),
            Focus::Orgs => None,
        }
    }

    /// Starts a new load generation and returns it: a listing started under
    /// any earlier one is dropped when it lands (`accepts_load`).
    pub fn begin_load(&mut self) -> LoadGeneration {
        self.load_generation += 1;
        LoadGeneration(self.load_generation)
    }

    /// Whether a listing started under `generation` may still be shown:
    /// only if nothing has superseded it since — see `load_generation`.
    pub fn accepts_load(&self, generation: LoadGeneration) -> bool {
        generation.0 == self.load_generation
    }

    /// Moves the load bar one call on, for a tick of the load started under
    /// `generation` — only if `accepts_load` still accepts it.
    ///
    /// A load the cursor left, or one a purge made suspect, keeps ticking on
    /// its task. Its ticks are dropped here as its listing is in `land_load`:
    /// otherwise the bar of the repository now looked at would move at the
    /// pace of the one left behind.
    pub fn load_ticked(&mut self, generation: LoadGeneration) {
        if !self.accepts_load(generation) {
            return;
        }
        if let Some(bar) = self.loading.as_mut() {
            bar.done += 1;
        }
    }

    /// Keeps `items` as `key`'s listing for the rest of the session, with no
    /// refused family.
    pub fn remember(&mut self, key: (String, String), items: Vec<Resource>) {
        self.keep(key, items, Vec::new());
    }

    /// Keeps `items` as `key`'s listing, together with the families whose
    /// listing was refused — ruling F2: both come back on every visit.
    fn keep(&mut self, key: (String, String), items: Vec<Resource>, refused: Vec<&'static str>) {
        if refused.is_empty() {
            self.repo_refused.remove(&key);
        } else {
            self.repo_refused.insert(key.clone(), refused);
        }
        self.repo_cache.insert(key, items);
    }

    /// `(org, repo)`'s listing, if one was kept this session.
    pub fn cached(&self, (org, repo): (&str, &str)) -> Option<&[Resource]> {
        self.repo_cache
            .get(&(org.to_string(), repo.to_string()))
            .map(Vec::as_slice)
    }

    /// Drops `(org, repo)`'s kept listing, and its refused families: a purge
    /// concerning that repository has just ended (`purge_ended`), and the
    /// next visit must read a fresh listing.
    ///
    /// A load of that repository already in flight is superseded too — its
    /// listing may have been read before the purge's deletions — and
    /// `follow_cursor` gives the repository a fresh pause. The caller passes
    /// the purge's own identity, never the cursor's.
    pub fn forget(&mut self, (org, repo): (&str, &str)) {
        let key = (org.to_string(), repo.to_string());
        self.repo_cache.remove(&key);
        self.repo_refused.remove(&key);
        if self.in_flight.as_ref() == Some(&key) {
            self.in_flight = None;
            self.loading = None;
            self.load_generation += 1;
        }
    }

    /// Records a purge — or an archive — confirmed and about to run: the
    /// quit guard's count, the org to refresh when it finishes, and the
    /// repository its plan concerns.
    ///
    /// The repository is recorded from the plan itself, at launch: nothing
    /// that arrives later names it reliably — `Progress::Finished` names no
    /// repository, and the cursor can be anywhere by then. While it is
    /// recorded, no load starts on its own for that repository
    /// (`follow_cursor`); `purge_ended` releases it.
    ///
    /// Its items join the purge bar (`purge`, `Work::joined`), counted from
    /// the plan itself. A plan with no item sets no bar and changes none: it
    /// ends before it could be drawn.
    pub fn purge_launched(&mut self, plan: &Plan) {
        self.purging_org = Some(plan.owner.clone());
        self.purges_in_flight += 1;
        *self
            .purging_repos
            .entry((plan.owner.clone(), plan.repo.clone()))
            .or_insert(0) += 1;

        if !plan.items.is_empty() {
            let label = if plan.is_archive() {
                progress::ARCHIVE
            } else {
                progress::DELETION
            };
            let count = plan.items.len();
            self.purge = Some(match self.purge.take() {
                Some(running) => running.joined(label, count),
                None => Work::new(label, count),
            });
        }
    }

    /// Advances the purge bar by one item: a purge's `Done` or `Failed`. A
    /// refused item is processed, not still waiting.
    pub fn purge_advanced(&mut self) {
        if let Some(bar) = self.purge.as_mut() {
            bar.done += 1;
        }
    }

    /// Applies the end of one purge of `(owner, repo)`: its `Finished`, which
    /// the event loop tags with the repository recorded from its plan at
    /// launch (`tui::spawn_purge`) — never the cursor's.
    ///
    /// Releases that purge's claim on the repository, and forgets the
    /// repository's kept listing (ruling F3, 2026-09-11). A deletion changes
    /// more than its own row — a workflow run takes its artifacts with it, a
    /// deleted branch changes how its caches are marked — so the next visit
    /// reads a fresh listing. Until the end, the listing was kept current
    /// row by row instead (`resource_deleted`), and the purge caused no load.
    pub fn purge_ended(&mut self, owner: &str, repo: &str) {
        let key = (owner.to_string(), repo.to_string());
        if let Some(count) = self.purging_repos.get_mut(&key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.purging_repos.remove(&key);
            }
        }
        self.forget((owner, repo));
    }

    /// What the resources column shows, for the repository under the
    /// column-2 cursor.
    ///
    /// `resources` count as that repository's listing only when `loaded`
    /// names it; otherwise the column shows that repository loading, or
    /// failed — never the rows of one the cursor has left.
    pub fn shown(&self) -> Shown {
        match (self.loaded.clone(), self.current_target()) {
            (Some(loaded), Some(target)) if loaded == target => Shown::Listing {
                org: target.0,
                repo: target.1,
            },
            (_, Some((org, repo))) => {
                if self
                    .load_failed
                    .as_ref()
                    .is_some_and(|(o, r)| *o == org && *r == repo)
                {
                    Shown::Failed { org, repo }
                } else {
                    Shown::Loading { org, repo }
                }
            }
            (Some((org, repo)), None) => Shown::Listing { org, repo },
            (None, None) => Shown::Nothing,
        }
    }

    /// Makes the resources column follow the repository under the column-2
    /// cursor, and returns the load to start, if one is due at `now`.
    ///
    /// The event loop calls this at the top of every pass — after each key,
    /// and on each `poll` tick (120 ms) when no key comes — with the current
    /// instant; tests inject theirs.
    ///
    /// **What arms the pause: a change of the repository under the column-2
    /// cursor, and nothing else.** The change is measured on `(org, repo)`
    /// against the last look, not on the key that caused it: `↑`/`↓` in the
    /// repos column, an org change in column 1 resetting that cursor — row 0
    /// before and after, another repository all the same — and the first
    /// look at start-up all count. A key that leaves the same repository
    /// under the cursor — a column change, a tick, a sort, `↓` on the last
    /// row — does not re-arm: the pause keeps its start. On a change:
    ///
    /// - whatever is in flight is superseded (`load_generation` moves on),
    ///   so the listing of the repository left is dropped when it lands —
    ///   even during the next pause, before any newer load has begun;
    /// - the column drops its listing, with the selection, row cursor,
    ///   filter and load warning that belonged to it: it never lists one
    ///   repository under another's cursor;
    /// - a repository kept in `repo_cache` shows at once, with no request
    ///   and no pause, and with the warning for any family its listing had
    ///   refused (ruling F2); any other waits `LOAD_PAUSE` from `now`.
    ///
    /// One case arms without a change: a repository still loading with no
    /// pause running and no load in flight — its load was superseded when a
    /// purge concerning it ended (`forget`). It gets a fresh pause rather
    /// than an immediate request.
    ///
    /// No load starts here for a repository a purge in flight concerns
    /// (`purge_launched`), whether its pause was armed by a cursor move or a
    /// cache miss — ruling F3: a purge must not start a reload storm. The
    /// load waits for the purge's end. A failed load is not retried here
    /// either; `Entrée` does that (`force_load`).
    pub fn follow_cursor(&mut self, now: Instant) -> Option<Load> {
        let target = self.current_target();
        if target != self.cursor_repo {
            self.cursor_repo = target.clone();
            self.leave_listing();
            if let Some(key) = target {
                match self.repo_cache.get(&key).cloned() {
                    Some(items) => {
                        let refused = self.repo_refused.get(&key).cloned().unwrap_or_default();
                        self.finish_loading(key.0, key.1, items, refused);
                    }
                    None => self.pending_since = Some(now),
                }
            }
            return None;
        }

        let Shown::Loading { org, repo } = self.shown() else {
            return None;
        };
        let key = (org, repo);
        if self.in_flight.as_ref() == Some(&key) {
            return None;
        }
        match self.pending_since {
            None => {
                self.pending_since = Some(now);
                None
            }
            Some(since) if now.saturating_duration_since(since) < LOAD_PAUSE => None,
            // A purge concerning this repository still runs: a listing read
            // now would be read mid-deletion. The load waits for the purge's
            // end (`purge_ended`); the pause, long elapsed by then, lets it
            // start on the next look.
            Some(_) if self.purging_repos.contains_key(&key) => None,
            Some(_) => self.start_load(),
        }
    }

    /// `Entrée`: a fresh load of the repository under the column-2 cursor,
    /// now — spec §3, and ruling F1 (2026-09-11).
    ///
    /// It skips the pause, and the cache too: `Entrée` is the one way to
    /// refresh a repository within a session. A listing already shown leaves
    /// the column, which says `(chargement…)` until the fresh one lands and
    /// replaces it, on screen and in `repo_cache` (`land_load`). The kept
    /// listing stays until then: a cursor that leaves meanwhile supersedes
    /// the refresh, and finds the kept listing on its way back. A failed load
    /// is retried the same way.
    ///
    /// `None` when there is nothing to request: its load is already in
    /// flight, or no repository is under the cursor. A refresh is a key, not
    /// a load starting on its own, so a purge concerning the repository does
    /// not hold it back.
    pub fn force_load(&mut self) -> Option<Load> {
        let target = self.current_target()?;
        if self.in_flight.as_ref() == Some(&target) {
            return None;
        }
        if self.loaded.as_ref() == Some(&target) {
            self.loaded = None;
            self.resources.clear();
            self.selected.clear();
            self.res_cursor = 0;
            self.clear_load_warning();
        }
        self.start_load()
    }

    /// Applies a listing that has landed, with the `Load` it was started
    /// under.
    ///
    /// Dropped — neither shown nor kept — unless `accepts_load` still
    /// accepts its generation. Otherwise it is shown (`finish_loading`) and
    /// kept in `repo_cache`, replacing whatever was kept before, together
    /// with its refused families if any (ruling F2): a later visit says them
    /// again rather than show the kept rows as if a refused family were
    /// empty. A failed load is said on the status line, and the column says
    /// it failed rather than loading for ever. That error is the load's own
    /// message (`load_warning`), like a refused family's warning: the next
    /// listing of the repository, or the cursor leaving it, clears it — a
    /// retry that lands in full must not leave the status line saying the
    /// load failed (fix round 1).
    pub fn land_load(
        &mut self,
        load: Load,
        outcome: anyhow::Result<(Vec<Resource>, Vec<&'static str>)>,
    ) {
        if !self.accepts_load(load.generation) {
            return;
        }
        self.in_flight = None;
        self.loading = None;
        match outcome {
            Ok((items, failed)) => {
                self.keep(
                    (load.org.clone(), load.repo.clone()),
                    items.clone(),
                    failed.clone(),
                );
                self.finish_loading(load.org, load.repo, items, failed);
            }
            Err(e) => {
                self.status = format!("Erreur : chargement de {}/{} — {e}", load.org, load.repo);
                self.load_warning = Some(self.status.clone());
                self.load_failed = Some((load.org, load.repo));
            }
        }
    }

    /// Starts the load of the repository under the cursor, under a new
    /// generation, and records it as in flight.
    fn start_load(&mut self) -> Option<Load> {
        let (org, repo) = self.current_target()?;
        self.pending_since = None;
        self.load_failed = None;
        let generation = self.begin_load();
        self.in_flight = Some((org.clone(), repo.clone()));
        self.loading = Some(Work::new(progress::LOAD, crate::scan::TOTAL_CALLS));
        Some(Load {
            generation,
            org,
            repo,
        })
    }

    /// Clears what belonged to the repository the column showed, once the
    /// cursor has left it: its listing, selection, row cursor and filter,
    /// the warning its load left, and any load of it pending or in flight.
    fn leave_listing(&mut self) {
        self.load_generation += 1;
        self.in_flight = None;
        self.loading = None;
        self.load_failed = None;
        self.pending_since = None;
        self.loaded = None;
        self.resources.clear();
        self.selected.clear();
        self.res_cursor = 0;
        self.filter.clear();
        self.filter_mode = false;
        self.clear_load_warning();
    }

    /// Shows `items` as `(org, repo)`'s listing — from `repo_cache` at once,
    /// or from a load that has landed.
    ///
    /// Leaves focus and the filter alone: they belong to the user's keys,
    /// and a listing lands at any moment. Leaves the repository tick alone
    /// too, whichever repository it is on (ruling R7-1, 2026-09-11): the
    /// listing follows the repos cursor, so dropping a tick left on another
    /// repository here dropped it on every cursor rest, and `d` from the
    /// repos column had nothing left to archive. `d` archives from the repos
    /// column only (`take_focused_plan`), so a tick left there never
    /// outranks the resources ticked in this listing; an org move still
    /// drops it (`reset_scoped_cursors`).
    fn show_listing(&mut self, org: String, repo: String, items: Vec<Resource>) {
        self.resources = items;
        self.res_cursor = 0;
        self.selected.clear();
        self.loaded = Some((org, repo));
    }

    /// Clears `status` if it still holds the warning a load wrote there.
    fn clear_load_warning(&mut self) {
        if self
            .load_warning
            .take()
            .is_some_and(|warning| warning == self.status)
        {
            self.status.clear();
        }
    }

    /// Shows a listing that has landed (`land_load`), and reports which
    /// families' listings — if any — failed.
    ///
    /// Extracted from `event_loop` so the state transition — the actual bug
    /// surface of Findings 1 and 5 of the final review — can be asserted on
    /// directly, without spinning up a terminal and a mock server.
    ///
    /// Since spec §3 a listing lands on its own time rather than on a key,
    /// so this no longer moves focus to the resources column nor clears the
    /// filter: a listing landing while the user types a filter would turn
    /// their next letters into commands. What belonged to the previous
    /// repository was cleared when the cursor left it (`follow_cursor`).
    ///
    /// Leaves the repository tick alone, whichever repository it is on
    /// (`show_listing`, ruling R7-1).
    ///
    /// `failed` — the family names `scan::repo_detail_with_warnings` could
    /// not list — become `app.status` instead of being discarded: Finding 5
    /// of the final review. `scan::repo_detail`'s stderr wrapper writes to a
    /// stream the alternate screen hides, so a refused listing read exactly
    /// like an empty one. A clean listing clears the warning a previous load
    /// left, and nothing else (`load_warning`).
    pub fn finish_loading(
        &mut self,
        org: String,
        repo: String,
        items: Vec<Resource>,
        failed: Vec<&'static str>,
    ) {
        self.show_listing(org, repo, items);
        self.clear_load_warning();
        if !failed.is_empty() {
            let warning = format!(
                "Avertissement : le listing de {} a échoué et est ignoré.",
                failed.join(", ")
            );
            self.status = warning.clone();
            self.load_warning = Some(warning);
        }
    }

    /// Applies a `Progress::Done` for a deleted resource: drops its row from
    /// the repository's kept listing, and its row and tick from the list on
    /// screen — the listing of the repository it was deleted from, and no
    /// other.
    ///
    /// Ruling F3 (2026-09-11): the kept listing is updated rather than
    /// forgotten, so a purge starts no reload — the cursor coming back to
    /// the repository mid-purge finds its listing, less what was deleted.
    /// The purge's end forgets it (`purge_ended`).
    ///
    /// The list on screen follows the cursor, so it can belong to another
    /// repository by the time a deletion lands; and a branch's or a tag's id
    /// is a hash of its name (`api::refs::resource_id`), the same in every
    /// repository. `owner`/`repo` come off the message, as `archive_done`'s
    /// do.
    pub fn resource_deleted(&mut self, kind: ResourceKind, id: u64, owner: &str, repo: &str) {
        let deleted = |r: &Resource| r.kind == kind && r.id == id;
        if let Some(items) = self
            .repo_cache
            .get_mut(&(owner.to_string(), repo.to_string()))
        {
            items.retain(|r| !deleted(r));
        }
        if self.lists(owner, repo) {
            self.resources.retain(|r| !deleted(r));
            self.selected.remove(&(kind, id));
        }
    }

    /// Applies a `Progress::Failed` for a resource: unticks it — in the
    /// listing of the repository it belongs to only, for `resource_deleted`'s
    /// reason.
    pub fn resource_failed(&mut self, kind: ResourceKind, id: u64, owner: &str, repo: &str) {
        if self.lists(owner, repo) {
            self.selected.remove(&(kind, id));
        }
    }

    /// Whether `resources` is `(owner, repo)`'s listing.
    fn lists(&self, owner: &str, repo: &str) -> bool {
        self.loaded
            .as_ref()
            .is_some_and(|(o, r)| o == owner && r == repo)
    }

    /// `[A]`: tick every ⛑ row — `Safety::Safe` (spec §4.2). The ⚑ rows `[A]`
    /// took before are among them: `safety::classify` levels a cache whose
    /// pull request closed as safe.
    pub fn select_safe(&mut self) {
        self.select_levels(&[Safety::Safe]);
    }

    /// `[V]`: tick every ⛑ row and every • row — `Safety::Safe` and
    /// `Safety::Check`. Spec §4.2's middle level is shown on every row but
    /// never ticked by `[A]`; it takes this second key.
    pub fn select_safe_and_check(&mut self) {
        self.select_levels(&[Safety::Safe, Safety::Check]);
    }

    /// Ticks every visible row whose level is one of `levels` — the one path
    /// both selection keys share, so their guards cannot drift apart.
    ///
    /// Iterates `visible_resources()`, not `self.resources` — the column
    /// shows the filtered list, and a bulk select feeding an irreversible
    /// delete must act on what is actually on screen.
    ///
    /// Never takes a `protected` resource, whatever its level: `protected`
    /// stays the bulk-selection gate (spec §4.3), as `commands::clean::
    /// select` applies it headless. `safety::classify` never levels a
    /// protected resource ⛑ today; this guard keeps a classifier bug from
    /// ever reaching a bulk delete.
    ///
    /// Excludes `ResourceKind::Repository` explicitly, defensively — not
    /// because one can reach `self.resources` today (it can't: a repository
    /// lives in the repos column, see `App::selected_repo`), but because
    /// nothing else here would stop it if one ever did. Neither key may ever
    /// take a repository, at any age or level: `pushed_at` alone is not proof
    /// of abandonment, and archiving is the only family in this product with
    /// no preselection path whatsoever — see
    /// `bulk_selection_never_takes_a_repository`.
    ///
    /// Individual selection (`toggle_selected`) is not bounded by `Safety`:
    /// a visible row stays tickable one at a time, whatever its level.
    fn select_levels(&mut self, levels: &[Safety]) {
        let keys: Vec<(ResourceKind, u64)> = self
            .visible_resources()
            .into_iter()
            .filter(|r| levels.contains(&r.safety))
            .filter(|r| !r.protected)
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

    /// Reset the cursors — and the repository tick — scoped beneath the org
    /// cursor, on every org move.
    ///
    /// `repo_cursor` already did this. `month_cursor` did not: paging to
    /// month 5 on a six-month org, then switching to a two-month org, left
    /// the cursor at 5 — the Billing tab clamps it on render, but the cursor
    /// itself stayed stranded high, so `←` read as dead until it walked all
    /// the way back down on its own.
    ///
    /// `selected_repo` joins them for Finding 1 of the final review: without
    /// this, ticking a repository in one org and then moving to a different
    /// org — still at `Focus::Repos` — left the old tick in place, so `d`
    /// would archive a repository the screen no longer shows any trace of
    /// having selected.
    pub fn reset_scoped_cursors(&mut self) {
        self.repo_cursor = 0;
        self.month_cursor = 0;
        self.selected_repo = None;
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
    /// `resources` was loaded from — `loaded`, never the live cursor
    /// position. `follow_cursor` keeps the two together and clears the
    /// selection whenever they part, but the plan names the listing it was
    /// built from rather than rely on that. `None` when nothing is loaded.
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
    ///
    /// The purge bar goes on the same condition, and for the same reason: a
    /// bar cleared by the first `Finished` would announce the end of work
    /// still running.
    pub fn purge_finished(&mut self) {
        self.purges_in_flight = self.purges_in_flight.saturating_sub(1);
        if self.purges_in_flight == 0 {
            self.quit_armed = false;
            self.purge = None;
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
            safety: crate::safety::Safety::Keep,
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

    /// One cache per entry of `levels`, in that order, ids from 1 — `app()`'s
    /// shape, with a safety level of the test's choosing on each row.
    fn app_with_levels(levels: &[Safety]) -> App {
        let mut a = App::new(vec![]);
        a.resources = levels
            .iter()
            .zip(1u64..)
            .map(|(&safety, id)| Resource {
                safety,
                ..res(id, &format!("cache-{id}"), 100 * id, 10, false)
            })
            .collect();
        a
    }

    /// The levels of the rows ticked, in the order the rows are listed.
    fn selected_levels(a: &App) -> Vec<Safety> {
        a.resources
            .iter()
            .filter(|r| a.selected.contains(&(r.kind, r.id)))
            .map(|r| r.safety)
            .collect()
    }

    /// Marks every row `protected`, whatever its level.
    fn protect_all_resources(a: &mut App) {
        for r in &mut a.resources {
            r.protected = true;
        }
    }

    #[test]
    fn select_safe_takes_only_the_safe_rows() {
        // One row of each level: a key that also took Check would pass a
        // fixture holding only Safe rows.
        let mut a = app_with_levels(&[Safety::Safe, Safety::Check, Safety::Keep]);
        a.select_safe();
        assert_eq!(selected_levels(&a), vec![Safety::Safe]);
    }

    #[test]
    fn select_safe_and_check_adds_check_and_nothing_else() {
        let mut a = app_with_levels(&[Safety::Safe, Safety::Check, Safety::Keep]);
        a.select_safe_and_check();
        assert_eq!(selected_levels(&a), vec![Safety::Safe, Safety::Check]);
    }

    #[test]
    fn a_protected_row_is_never_taken_in_bulk_even_if_marked_safe() {
        // classify never returns Safe for a protected row today; this pins the
        // selection's own guard, so a future classifier bug cannot reach a
        // bulk delete.
        let mut a = app_with_levels(&[Safety::Safe]);
        protect_all_resources(&mut a);
        a.select_safe();
        assert!(selected_levels(&a).is_empty(), "[A] took a protected row");
        a.select_safe_and_check();
        assert!(selected_levels(&a).is_empty());
    }

    /// THE test of this whole version, per the task-4 brief's own
    /// self-review: without this, a future extension of `[A]` would flip a
    /// whole organisation to read-only on one keystroke. Neither `[A]` nor
    /// `[V]` may ever take a repository, at any age — `pushed_at` alone is
    /// not proof of abandonment, and this is the only family in the entire
    /// product with no preselection path whatsoever.
    ///
    /// A `Repository` resource can never actually reach `self.resources` in
    /// production today — a repository lives in the repos column, not the
    /// resource list (see `App::selected_repo`) — which is exactly why this
    /// guard has to be asserted defensively rather than trusted as an
    /// emergent property of "nothing puts one there". Spec §4 never marks a
    /// repository, but both keys select on `Resource.safety`: a future
    /// refactor that merged a repository row into `resources`, carrying a
    /// level some classifier bug gave it, would start bulk-selecting it. So
    /// the fixture's repositories are marked ⛑ and •, unprotected, of every
    /// age: a guard keyed on the level alone takes them all.
    #[test]
    fn bulk_selection_never_takes_a_repository() {
        let mut a = App::new(vec![]);
        let repository = |id: u64, label: &str, age_days: i64, safety: Safety| Resource {
            kind: ResourceKind::Repository,
            id,
            label: label.into(),
            size_bytes: 0,
            age_days,
            git_ref: None,
            stale_pr: true,
            protected: false,
            branch_class: None,
            safety,
        };
        a.resources = vec![
            repository(1, "young-repo", 10, Safety::Safe),
            repository(2, "lokiprint", 685, Safety::Check),
            repository(3, ".github", 775, Safety::Safe),
        ];

        a.select_safe();
        assert!(
            a.selected.is_empty(),
            "[A] took a Repository: got {:?}",
            a.selected
        );
        a.select_safe_and_check();
        assert!(
            a.selected.is_empty(),
            "[V] took a Repository: got {:?}",
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
                safety: crate::safety::Safety::Keep,
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
                safety: crate::safety::Safety::Keep,
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
    /// everything loaded. Before the fix, the bulk select iterated
    /// `self.resources` directly and selected rows the filter was hiding.
    /// `[V]` keeps the same rule.
    #[test]
    fn select_safe_does_not_select_rows_hidden_by_the_filter() {
        let mut a = app();
        // Levelled as `safety::classify` levels such caches: a closed pull
        // request's is ⛑, the other one •.
        for r in &mut a.resources {
            r.safety = if r.stale_pr {
                Safety::Safe
            } else {
                Safety::Check
            };
        }
        // Hides "coverage-macos" (id 3, ⛑) and "ubuntu-22-test" (id 2, •),
        // keeps "coverage-linux" (id 1, ⛑) visible.
        a.filter = "linux".into();
        a.select_safe();

        assert!(a.selected.contains(&(ResourceKind::Cache, 1)));
        assert!(!a.selected.contains(&(ResourceKind::Cache, 3)));

        a.select_safe_and_check();
        assert_eq!(
            a.selected.len(),
            1,
            "[V] took a row the filter hides: {:?}",
            a.selected
        );
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
            safety: crate::safety::Safety::Keep,
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
            safety: crate::safety::Safety::Keep,
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

    /// Findings 3 and 4 of the final review: two archives in flight at once
    /// must each resolve against their own target, not against whatever the
    /// tree currently has ticked. `"second"` is what the tree is showing
    /// ticked (its own archive is still running); `"first"` is the one that
    /// actually just finished. On the old `self.selected_repo.take()`
    /// reading, this would wrongly mark `"second"` archived — the repo still
    /// mid-flight — and leave `"first"`, which genuinely finished, untouched.
    #[test]
    fn archive_done_updates_the_finished_repos_own_row_not_whatever_is_currently_ticked() {
        let repo = |name: &str| RepoSummary {
            name: name.to_string(),
            cache_bytes: 0,
            cache_count: 0,
            private: false,
            age_days: 1,
            class: crate::repos::RepoClass::Archivable,
        };
        let mut a = App::new(vec![OrgSummary {
            login: "org".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![repo("first"), repo("second")],
            billing: None,
        }]);
        a.selected_repo = Some(("org".to_string(), "second".to_string()));

        a.archive_done("org", "first");

        assert_eq!(
            a.orgs[0].repos[0].class,
            crate::repos::RepoClass::AlreadyArchived,
            "the repo that actually finished must be marked archived"
        );
        assert_eq!(
            a.orgs[0].repos[1].class,
            crate::repos::RepoClass::Archivable,
            "the still in-flight repo must not be marked archived early"
        );
        assert_eq!(
            a.selected_repo,
            Some(("org".to_string(), "second".to_string())),
            "the still-ticked, still in-flight repo's own tick must survive"
        );
    }

    /// The tick is only cleared when it actually still points at the repo
    /// that just finished — clearing it unconditionally, as `archive_done`
    /// used to, would drop a second, still-running archive's own tick the
    /// instant an unrelated first one completed.
    #[test]
    fn archive_done_clears_the_tick_when_it_matches_the_finished_repo() {
        let mut a = app_with_one_repo(crate::repos::RepoClass::Archivable);
        a.toggle_repo_selected();
        assert_eq!(
            a.selected_repo,
            Some(("maxds-lyon".to_string(), "lokiprint".to_string()))
        );

        a.archive_done("maxds-lyon", "lokiprint");

        assert_eq!(a.selected_repo, None);
    }

    /// Same reasoning as `archive_done`'s own tests, for a refused archive:
    /// a failure belonging to one repo must not clear a different repo's
    /// still-valid, still in-flight tick.
    #[test]
    fn archive_failed_only_clears_the_tick_if_it_still_points_at_the_failed_repo() {
        let mut a = App::new(vec![]);
        a.selected_repo = Some(("org".to_string(), "second".to_string()));

        a.archive_failed("org", "first");

        assert_eq!(
            a.selected_repo,
            Some(("org".to_string(), "second".to_string())),
            "a still in-flight, still-ticked repo's tick must survive an unrelated failure"
        );
    }

    #[test]
    fn archive_failed_clears_the_tick_when_it_matches_the_failed_repo() {
        let mut a = App::new(vec![]);
        a.selected_repo = Some(("org".to_string(), "first".to_string()));

        a.archive_failed("org", "first");

        assert_eq!(a.selected_repo, None);
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

    /// Finding 1 of the final review: moving the org cursor while a
    /// repository from a *different* org is still ticked must drop that
    /// tick. `reset_scoped_cursors` already runs on every org move (see
    /// `tui::event_loop`'s `Up`/`Down` handlers for `Focus::Orgs`); without
    /// this, `d` back at `Focus::Repos` in the new org would still prefer
    /// archiving a repository the screen has moved entirely away from.
    #[test]
    fn reset_scoped_cursors_also_drops_a_stale_repo_tick() {
        let mut a = App::new(vec![]);
        a.selected_repo = Some(("old-org".to_string(), "old-repo".to_string()));

        a.reset_scoped_cursors();

        assert_eq!(
            a.selected_repo, None,
            "an org move must drop a tick left over from a different org"
        );
    }

    /// Finding 1 of the final review, the scenario the report itself
    /// describes: a repository ticked earlier must not outrank a resource
    /// selection made after focus has actually moved to the resource pane.
    /// On the old `take_repo_plan().or_else(|| take_plan())` preference
    /// order, this plan targets `lokiprint` — the repo ticked first — even
    /// though the cursor, `loaded` and the pending selection have all since
    /// moved to `claudine`.
    #[test]
    fn take_focused_plan_prefers_the_resource_plan_once_focus_leaves_the_repo_tree() {
        let repo = |name: &str| RepoSummary {
            name: name.to_string(),
            cache_bytes: 0,
            cache_count: 0,
            private: false,
            age_days: 685,
            class: crate::repos::RepoClass::Archivable,
        };
        let mut a = App::new(vec![OrgSummary {
            login: "maxds-lyon".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![repo("lokiprint"), repo("claudine")],
            billing: None,
        }]);
        a.focus = Focus::Repos;
        a.repo_cursor = 0;
        a.toggle_repo_selected();
        assert_eq!(
            a.selected_repo,
            Some(("maxds-lyon".to_string(), "lokiprint".to_string())),
            "fixture must actually tick lokiprint first"
        );

        // The user drills into a different repository's resources and ticks
        // one there — nothing here touches `selected_repo` on its own.
        a.loaded = Some(("maxds-lyon".to_string(), "claudine".to_string()));
        a.resources = vec![res(1, "cache-1", 100, 1, false)];
        a.selected.insert((ResourceKind::Cache, 1));
        a.focus = Focus::Resources;

        let plan = a.take_focused_plan().expect("a resource is selected");
        assert_eq!(
            plan.repo, "claudine",
            "must target what focus is actually on, not the stale tick"
        );
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].kind, ResourceKind::Cache);
    }

    /// The positive counterpart: with focus still on the repos column, a ticked
    /// repository must still be what `d` builds.
    #[test]
    fn take_focused_plan_returns_the_repo_plan_while_focus_is_on_the_repo_tree() {
        let mut a = app_with_one_repo(crate::repos::RepoClass::Archivable);
        a.toggle_repo_selected();

        let plan = a.take_focused_plan().expect("a repo was ticked");
        assert_eq!(plan.repo, "lokiprint");
        assert_eq!(plan.items[0].kind, ResourceKind::Repository);
    }

    /// Finding 5 of the final review: `repo_detail`'s stderr wrapper is
    /// invisible behind the TUI's alternate screen, so a refused listing
    /// used to read exactly like an empty one — "nothing here" instead of
    /// "the listing was refused". The failed family names must reach
    /// `app.status`, the one place the user is actually looking.
    #[test]
    fn finish_loading_surfaces_failed_families_in_status() {
        let mut a = App::new(vec![]);

        a.finish_loading(
            "org".to_string(),
            "repo".to_string(),
            vec![],
            vec!["caches", "tags"],
        );

        assert!(
            !a.status.is_empty(),
            "a refused listing must not read the same as an empty one"
        );
        assert!(a.status.contains("caches"), "got: {}", a.status);
        assert!(a.status.contains("tags"), "got: {}", a.status);
    }

    /// The other side of finding 5: nothing failed, so the status must not
    /// carry a leftover warning from a previous load. The leftover is a real
    /// previous load's warning: nothing writes "Chargement de …" to the
    /// status line any more — the resources column says `(chargement…)`.
    #[test]
    fn finish_loading_clears_status_when_nothing_failed() {
        let mut a = App::new(vec![]);
        a.finish_loading(
            "org".to_string(),
            "repo".to_string(),
            vec![],
            vec!["caches"],
        );
        assert!(!a.status.is_empty(), "the fixture needs a leftover warning");

        a.finish_loading("org".to_string(), "repo".to_string(), vec![], vec![]);

        assert!(a.status.is_empty(), "got: {}", a.status);
    }

    /// One org whose repositories are `names`, all archivable — the load
    /// tests below only care which repository the cursor is on.
    fn org_named(login: &str, names: &[&str]) -> OrgSummary {
        OrgSummary {
            login: login.to_string(),
            cache_bytes: 0,
            cache_count: 0,
            repos: names
                .iter()
                .map(|name| repo_summary(name, crate::repos::RepoClass::Archivable, 1))
                .collect(),
            billing: None,
        }
    }

    fn key(org: &str, repo: &str) -> (String, String) {
        (org.to_string(), repo.to_string())
    }

    fn ms(n: u64) -> std::time::Duration {
        std::time::Duration::from_millis(n)
    }

    const PAUSE: std::time::Duration = std::time::Duration::from_millis(300);

    #[test]
    fn a_result_that_arrives_after_the_cursor_moved_is_ignored() {
        // Stop on three repositories in a row and the first result must not
        // appear under the third one's name. This is the same shape as the
        // v0.5 defect where a purge resolved against whatever `app` pointed
        // at, rather than against its own identity.
        let mut a = App::new(vec![]);
        let stale = a.begin_load(); // generation 1
        let _current = a.begin_load(); // generation 2
        assert!(!a.accepts_load(stale), "a superseded load must be dropped");
    }

    #[test]
    fn a_cached_repo_is_served_without_a_request() {
        let mut a = App::new(vec![]);
        a.remember(("org".into(), "repo".into()), vec![]);
        assert!(a.cached(("org", "repo")).is_some());
    }

    #[test]
    fn a_purge_invalidates_the_repos_cache() {
        // Otherwise the screen keeps showing what was just deleted.
        let mut a = App::new(vec![]);
        a.remember(("org".into(), "repo".into()), vec![]);
        a.forget(("org", "repo"));
        assert!(a.cached(("org", "repo")).is_none());
    }

    /// Spec §3: the load starts once the column-2 cursor has rested 300 ms
    /// on a repository — not a tick earlier, and only once. The clock is
    /// injected: `t0` is when the loop first saw the repository under the
    /// cursor, here at start-up.
    #[test]
    fn a_load_starts_once_the_cursor_has_rested_the_pause() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine", "claudine"])]);
        let t0 = std::time::Instant::now();

        assert!(
            a.follow_cursor(t0).is_none(),
            "nothing starts before the pause"
        );
        assert!(
            a.follow_cursor(t0 + ms(299)).is_none(),
            "299 ms is no pause"
        );
        let load = a
            .follow_cursor(t0 + PAUSE)
            .expect("300 ms of rest starts the load");
        assert_eq!(
            (load.org.as_str(), load.repo.as_str()),
            ("systm-d", "josephine")
        );
        assert!(
            a.follow_cursor(t0 + ms(900)).is_none(),
            "one pause started two loads"
        );
    }

    /// Moving the column-2 cursor to another repository re-arms the pause,
    /// so traversing a list requests nothing. Without the re-arm,
    /// josephine's pause — started at `t0` — would fire at 300 ms for
    /// claudine, which the cursor reached only 100 ms before.
    #[test]
    fn moving_the_repos_cursor_rearms_the_pause() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine", "claudine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());

        a.repo_cursor = 1;
        assert!(a.follow_cursor(t0 + ms(200)).is_none());
        assert!(
            a.follow_cursor(t0 + PAUSE).is_none(),
            "the pause did not restart when the cursor moved"
        );
        let load = a
            .follow_cursor(t0 + ms(200) + PAUSE)
            .expect("claudine rested her own pause");
        assert_eq!(load.repo, "claudine");
    }

    /// An org change in column 1 resets the repos cursor to the first row:
    /// index 0 before, index 0 after, and still another repository. The
    /// pause is armed by the repository under the cursor, not by its index.
    #[test]
    fn an_org_change_rearms_the_pause_through_the_reset_repos_cursor() {
        let mut a = App::new(vec![
            org_named("systm-d", &["josephine"]),
            org_named("exec-d", &["lokiprint"]),
        ]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());

        a.org_cursor = 1;
        a.reset_scoped_cursors();
        assert!(a.follow_cursor(t0 + ms(200)).is_none());
        assert!(
            a.follow_cursor(t0 + PAUSE).is_none(),
            "the org change did not restart the pause"
        );
        let load = a
            .follow_cursor(t0 + ms(200) + PAUSE)
            .expect("lokiprint rested its own pause");
        assert_eq!(
            (load.org.as_str(), load.repo.as_str()),
            ("exec-d", "lokiprint")
        );
    }

    /// A key that leaves the same repository under the column-2 cursor — a
    /// column change, a tick, a sort — does not restart the pause: the load
    /// still starts 300 ms after the cursor reached the repository.
    #[test]
    fn a_key_that_keeps_the_same_repository_does_not_rearm_the_pause() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine", "claudine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());

        a.focus = Focus::Repos;
        a.toggle_repo_selected();
        a.cycle_sort();
        assert!(a.follow_cursor(t0 + ms(200)).is_none());

        let load = a
            .follow_cursor(t0 + PAUSE)
            .expect("the pause kept its start");
        assert_eq!(load.repo, "josephine");
    }

    /// `Entrée` starts the load at once, without the pause — and once: the
    /// pause has nothing left to start, and a second `Entrée` while the
    /// listing is on its way requests nothing more.
    #[test]
    fn enter_loads_at_once_and_only_once() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());

        let load = a.force_load().expect("Entrée does not wait for the pause");
        assert_eq!(load.repo, "josephine");
        assert!(
            a.force_load().is_none(),
            "a second Entrée re-requested a load in flight"
        );
        assert!(
            a.follow_cursor(t0 + PAUSE).is_none(),
            "the pause started a second load after Entrée"
        );
    }

    /// Spec §3: a repository already loaded shows at once, with no request —
    /// not when the cursor comes back to it, not after the pause. `Entrée`
    /// does request it: see
    /// `enter_refreshes_a_cached_repository_and_its_listing_replaces_the_kept_one`.
    #[test]
    fn a_cached_repository_shows_at_once_when_the_cursor_returns() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine", "claudine"])]);
        let t0 = std::time::Instant::now();
        a.remember(
            key("systm-d", "claudine"),
            vec![res(7, "claudine-cache", 100, 1, false)],
        );
        assert!(a.follow_cursor(t0).is_none());

        a.repo_cursor = 1;
        assert!(a.follow_cursor(t0 + ms(10)).is_none());
        assert_eq!(
            a.loaded,
            Some(key("systm-d", "claudine")),
            "the cached listing must show at once"
        );
        assert_eq!(
            a.resources.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![7]
        );
        assert!(
            a.follow_cursor(t0 + ms(10) + PAUSE).is_none(),
            "the pause re-requested a cached repository"
        );
    }

    /// Ruling F1 (2026-09-11): `Entrée` is the one way to refresh a
    /// repository within a session, so it loads even one already kept —
    /// past the pause and past the cache — and the fresh listing replaces the
    /// kept one. A forced load the cursor then leaves is dropped by
    /// generation, like any other.
    #[test]
    fn enter_refreshes_a_cached_repository_and_its_listing_replaces_the_kept_one() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine", "claudine"])]);
        let t0 = std::time::Instant::now();
        a.remember(
            key("systm-d", "josephine"),
            vec![res(1, "before", 100, 1, false)],
        );
        assert!(a.follow_cursor(t0).is_none());
        assert_eq!(
            a.loaded,
            Some(key("systm-d", "josephine")),
            "the fixture needs josephine listed from the cache"
        );
        let ids = |rows: &[Resource]| rows.iter().map(|r| r.id).collect::<Vec<_>>();

        let refresh = a
            .force_load()
            .expect("Entrée must refresh a cached repository");
        assert_eq!(refresh.repo, "josephine");
        a.land_load(refresh, Ok((vec![res(2, "after", 100, 1, false)], vec![])));
        assert_eq!(
            ids(a.cached(("systm-d", "josephine")).expect("kept")),
            vec![2],
            "the fresh listing must replace the kept one"
        );
        assert_eq!(ids(a.resources.as_slice()), vec![2]);

        let superseded = a.force_load().expect("Entrée refreshes again");
        a.repo_cursor = 1;
        assert!(a.follow_cursor(t0 + ms(100)).is_none());
        a.land_load(
            superseded,
            Ok((vec![res(3, "stale", 100, 1, false)], vec![])),
        );
        assert_eq!(
            ids(a.cached(("systm-d", "josephine")).expect("kept")),
            vec![2],
            "a forced load the cursor left replaced the kept listing"
        );
        assert!(
            !a.resources.iter().any(|r| r.id == 3),
            "a forced load the cursor left is shown under claudine"
        );
    }

    /// Spec §3's cancellation, in the case the generation exists for: the
    /// cursor leaves josephine while her listing is in flight, and it lands
    /// during claudine's pause — before claudine's own load has begun, so no
    /// newer `begin_load` has superseded it. The cursor move itself must
    /// have: the listing is neither shown nor cached.
    #[test]
    fn a_listing_that_lands_after_the_cursor_left_is_dropped() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine", "claudine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());
        let josephine = a
            .follow_cursor(t0 + PAUSE)
            .expect("josephine's load starts");

        a.repo_cursor = 1;
        assert!(a.follow_cursor(t0 + ms(400)).is_none());
        a.land_load(
            josephine,
            Ok((vec![res(1, "josephine-cache", 100, 1, false)], vec![])),
        );

        assert!(
            a.resources.is_empty(),
            "josephine's listing is shown under claudine's cursor"
        );
        assert_eq!(a.loaded, None);
        assert!(
            a.cached(("systm-d", "josephine")).is_none(),
            "a dropped listing was cached"
        );
        assert_eq!(
            a.shown(),
            Shown::Loading {
                org: "systm-d".into(),
                repo: "claudine".into()
            }
        );
    }

    /// The positive control: the listing of the repository still under the
    /// cursor is shown, and kept for the session.
    #[test]
    fn a_listing_that_lands_for_the_cursors_repository_is_shown_and_cached() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());
        let load = a.follow_cursor(t0 + PAUSE).expect("the load starts");

        a.land_load(
            load,
            Ok((vec![res(1, "josephine-cache", 100, 1, false)], vec![])),
        );

        assert_eq!(a.loaded, Some(key("systm-d", "josephine")));
        assert_eq!(a.resources.len(), 1);
        assert!(a.cached(("systm-d", "josephine")).is_some());
        assert_eq!(
            a.shown(),
            Shown::Listing {
                org: "systm-d".into(),
                repo: "josephine".into()
            }
        );
    }

    /// Ruling F2 (2026-09-11), Finding 5 meeting the cache: a listing with a
    /// refused family is kept together with that refusal, and its warning
    /// comes back on every visit. The second visit requests nothing and
    /// still says which listing failed — the kept rows never read as if the
    /// refused family were empty. `Entrée` is the retry.
    #[test]
    fn a_second_visit_to_a_listing_with_a_refused_family_requests_nothing_and_warns_again() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine", "claudine"])]);
        let t0 = std::time::Instant::now();
        a.remember(key("systm-d", "claudine"), vec![]);
        assert!(a.follow_cursor(t0).is_none());
        let load = a
            .follow_cursor(t0 + PAUSE)
            .expect("josephine's load starts");
        a.land_load(
            load,
            Ok((
                vec![res(1, "josephine-cache", 100, 1, false)],
                vec!["caches"],
            )),
        );
        assert!(a.status.contains("caches"), "got: {}", a.status);

        a.repo_cursor = 1;
        assert!(a.follow_cursor(t0 + ms(1000)).is_none());
        assert!(
            !a.status.contains("caches"),
            "the warning must leave with josephine: {}",
            a.status
        );

        a.repo_cursor = 0;
        let back = t0 + ms(2000);
        assert!(a.follow_cursor(back).is_none());
        assert!(
            a.follow_cursor(back + PAUSE).is_none(),
            "the second visit re-requested josephine"
        );
        assert_eq!(a.loaded, Some(key("systm-d", "josephine")));
        assert_eq!(a.resources.len(), 1);
        assert!(
            a.status.contains("caches"),
            "the second visit lost the warning: {}",
            a.status
        );
    }

    /// A load that fails outright is said so, and the pause does not retry
    /// it on its own — a failing repository would otherwise be requested
    /// again every 300 ms. `Entrée` retries.
    #[test]
    fn a_failed_load_is_reported_and_retried_only_on_enter() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());
        let load = a.follow_cursor(t0 + PAUSE).expect("the load starts");

        a.land_load(load, Err(anyhow::anyhow!("503 Service Unavailable")));

        assert_eq!(
            a.shown(),
            Shown::Failed {
                org: "systm-d".into(),
                repo: "josephine".into()
            }
        );
        assert!(a.status.contains("503"), "got: {}", a.status);
        assert!(
            a.follow_cursor(t0 + ms(5000)).is_none(),
            "a failed repository was requested again without Entrée"
        );
        assert!(a.force_load().is_some(), "Entrée must retry");
    }

    /// Fix round 1: a failed load's error is the load's own message, like a
    /// refused family's warning, so it never outlives the failure it
    /// reports. Retried with `Entrée` and landing in full, the retry clears
    /// it; a failure the cursor leaves behind leaves with it. The status
    /// line must not say a load failed while the column lists that
    /// repository. Claudine is never loaded: the cursor leaving josephine
    /// finds no listing whose landing could clear the error in its place.
    #[test]
    fn a_load_error_leaves_the_status_line_once_retried_or_left_behind() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine", "claudine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());
        let load = a.follow_cursor(t0 + PAUSE).expect("the load starts");
        a.land_load(load, Err(anyhow::anyhow!("503 Service Unavailable")));
        assert!(a.status.contains("503"), "got: {}", a.status);

        let retry = a.force_load().expect("Entrée retries");
        a.land_load(
            retry,
            Ok((vec![res(1, "josephine-cache", 100, 1, false)], vec![])),
        );
        assert_eq!(
            a.shown(),
            Shown::Listing {
                org: "systm-d".into(),
                repo: "josephine".into()
            }
        );
        assert!(
            !a.status.contains("503"),
            "the error outlived a successful retry: {}",
            a.status
        );

        let refresh = a.force_load().expect("Entrée refreshes");
        a.land_load(refresh, Err(anyhow::anyhow!("502 Bad Gateway")));
        assert!(a.status.contains("502"), "got: {}", a.status);
        a.repo_cursor = 1;
        assert!(a.follow_cursor(t0 + ms(2000)).is_none());
        assert!(
            !a.status.contains("502"),
            "josephine's error stayed on claudine: {}",
            a.status
        );
    }

    /// A listing lands on its own time, not on a key: it must not move the
    /// keyboard out of the column it is in.
    #[test]
    fn a_listing_that_lands_leaves_the_keyboard_in_its_column() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());
        let load = a.follow_cursor(t0 + PAUSE).expect("the load starts");
        a.focus = Focus::Repos;

        a.land_load(load, Ok((vec![res(1, "c", 100, 1, false)], vec![])));

        assert_eq!(a.focus, Focus::Repos);
    }

    /// Nor may it close a filter being typed while the listing was on its
    /// way: once closed, the next letter typed runs as a command — `A`
    /// ticks, `d` deletes.
    #[test]
    fn a_listing_that_lands_does_not_close_a_filter_being_typed() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());
        let load = a.follow_cursor(t0 + PAUSE).expect("the load starts");
        a.focus = Focus::Resources;
        a.filter_mode = true;
        a.filter.push_str("cov");

        a.land_load(load, Ok((vec![res(1, "coverage", 100, 1, false)], vec![])));

        assert!(a.filter_mode, "the filter closed under the user's typing");
        assert_eq!(a.filter, "cov");
    }

    /// A repository ticked for archiving during its own pause keeps its tick
    /// when its listing lands: dropping it would lose the user's selection to
    /// a timer. A tick left on another repository keeps it as well (ruling
    /// R7-1, `a_repository_tick_survives_another_repositorys_listing_landing`).
    #[test]
    fn a_tick_on_the_repository_whose_listing_lands_survives() {
        let mut a = App::new(vec![org_named("systm-d", &["lokiprint"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());
        a.focus = Focus::Repos;
        a.toggle_repo_selected();
        let load = a.follow_cursor(t0 + PAUSE).expect("the load starts");

        a.land_load(load, Ok((vec![], vec![])));

        assert_eq!(a.selected_repo, Some(key("systm-d", "lokiprint")));
    }

    /// Two orgs: `maxds-lyon`, holding two archivable repositories —
    /// lokiprint, then claudine — and `exec-d`, for an org move.
    fn app_with_two_archivable_repos() -> App {
        App::new(vec![
            org_named("maxds-lyon", &["lokiprint", "claudine"]),
            org_named("exec-d", &["alertu"]),
        ])
    }

    /// Ticks lokiprint from the repos column, the cursor on it, then moves
    /// that cursor down to claudine — the steps the tests below share.
    fn tick_lokiprint_then_move_to_claudine(a: &mut App, t0: std::time::Instant) {
        assert!(a.follow_cursor(t0).is_none());
        a.focus = Focus::Repos;
        a.toggle_repo_selected();
        assert_eq!(
            a.selected_repo,
            Some(key("maxds-lyon", "lokiprint")),
            "the fixture ticks lokiprint"
        );
        a.repo_cursor = 1;
    }

    /// `repo`, in `maxds-lyon`, is still the repository ticked, and `d` from
    /// the repos column builds its archive plan — wherever the cursor is.
    fn assert_d_archives(a: &App, repo: &str) {
        assert_eq!(
            a.selected_repo,
            Some(key("maxds-lyon", repo)),
            "the tick on {repo} was lost"
        );
        assert_eq!(a.focus, Focus::Repos);
        let plan = a
            .take_focused_plan()
            .expect("d from the repos column must build the ticked repository's archive plan");
        assert_eq!(
            (plan.owner.as_str(), plan.repo.as_str()),
            ("maxds-lyon", repo)
        );
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].kind, ResourceKind::Repository);
        assert_eq!(plan.items[0].label, repo);
    }

    /// Ruling R7-1 (2026-09-11): the repository tick stays single, as in
    /// v0.5, and it survives the cursor. The resources column follows the
    /// repos cursor, so resting on another repository shows that
    /// repository's listing — and a listing showing used to drop a tick left
    /// on any other repository: a tick never outlived a cursor rest, and `d`
    /// from the repos column had nothing left to archive. Here claudine's
    /// listing is kept, and shows at once.
    #[test]
    fn a_repository_tick_survives_resting_on_a_cached_repository() {
        let mut a = app_with_two_archivable_repos();
        let t0 = std::time::Instant::now();
        a.remember(
            key("maxds-lyon", "claudine"),
            vec![res(7, "claudine-cache", 100, 1, false)],
        );

        tick_lokiprint_then_move_to_claudine(&mut a, t0);
        assert!(a.follow_cursor(t0 + ms(10)).is_none());
        assert_eq!(
            a.loaded,
            Some(key("maxds-lyon", "claudine")),
            "the fixture needs claudine's kept listing on screen"
        );

        assert_d_archives(&a, "lokiprint");
    }

    /// Ruling R7-1, claudine not kept: her pause runs out, her load starts
    /// and her listing lands — and lokiprint is still the repository `d`
    /// archives from the repos column.
    #[test]
    fn a_repository_tick_survives_another_repositorys_listing_landing() {
        let mut a = app_with_two_archivable_repos();
        let t0 = std::time::Instant::now();

        tick_lokiprint_then_move_to_claudine(&mut a, t0);
        let rest = t0 + ms(10);
        assert!(a.follow_cursor(rest).is_none(), "claudine waits a pause");
        let load = a
            .follow_cursor(rest + PAUSE)
            .expect("claudine's load starts after the pause");
        assert_eq!(load.repo, "claudine");
        a.land_load(
            load,
            Ok((vec![res(7, "claudine-cache", 100, 1, false)], vec![])),
        );
        assert_eq!(
            a.loaded,
            Some(key("maxds-lyon", "claudine")),
            "the fixture needs claudine's listing landed"
        );

        assert_d_archives(&a, "lokiprint");
    }

    /// Ruling R7-1's other half — Finding 1 of the v0.5 final review: an org
    /// move still drops the tick, so `d` never archives a repository of an
    /// org the screen has left. The tick first survives a rest on claudine,
    /// the positive control; then the org cursor moves to exec-d the way
    /// `tui::event_loop` moves it.
    #[test]
    fn an_org_move_drops_a_repository_tick_that_survived_the_cursor() {
        let mut a = app_with_two_archivable_repos();
        let t0 = std::time::Instant::now();
        a.remember(key("maxds-lyon", "claudine"), vec![]);
        tick_lokiprint_then_move_to_claudine(&mut a, t0);
        assert!(a.follow_cursor(t0 + ms(10)).is_none());
        assert_d_archives(&a, "lokiprint");

        a.org_cursor = 1;
        a.reset_scoped_cursors();
        assert!(a.follow_cursor(t0 + ms(20)).is_none());

        assert_eq!(a.selected_repo, None, "an org move kept the tick");
        assert!(
            a.take_focused_plan().is_none(),
            "d would archive a repository of the org the screen left"
        );
    }

    /// A clean listing lands on its own time, so it may clear the warning a
    /// previous load left and nothing else. The quit guard's warning is the
    /// one that matters: cleared by a listing landing, the next `q` quits
    /// mid-purge with nothing on screen saying so.
    #[test]
    fn a_clean_listing_leaves_a_message_it_did_not_write_alone() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());
        let load = a.follow_cursor(t0 + PAUSE).expect("the load starts");
        a.status = "Purge en cours — [q] à nouveau pour quitter sans l'achever.".into();

        a.land_load(load, Ok((vec![], vec![])));

        assert_eq!(
            a.status,
            "Purge en cours — [q] à nouveau pour quitter sans l'achever."
        );
    }

    /// A load's warning is about the repository it listed: once the cursor
    /// shows another one, the warning leaves with it.
    #[test]
    fn a_load_warning_leaves_with_its_repository() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine", "claudine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());
        let load = a.follow_cursor(t0 + PAUSE).expect("the load starts");
        a.land_load(load, Ok((vec![], vec!["tags"])));
        assert!(a.status.contains("tags"), "got: {}", a.status);

        a.repo_cursor = 1;
        assert!(a.follow_cursor(t0 + ms(1000)).is_none());

        assert!(
            a.status.is_empty(),
            "josephine's warning stayed on claudine: {}",
            a.status
        );
    }

    /// A purge changing a repository while its listing is in flight makes
    /// that listing suspect: it may have been read before the deletion.
    /// `forget` drops it on arrival like a superseded one, and the pause
    /// starts over so a fresh listing follows.
    #[test]
    fn forgetting_a_repository_supersedes_its_listing_in_flight() {
        let mut a = App::new(vec![org_named("systm-d", &["josephine"])]);
        let t0 = std::time::Instant::now();
        assert!(a.follow_cursor(t0).is_none());
        let suspect = a.follow_cursor(t0 + PAUSE).expect("the load starts");

        a.forget(("systm-d", "josephine"));
        a.land_load(
            suspect,
            Ok((vec![res(1, "deleted-cache", 100, 1, false)], vec![])),
        );
        assert!(
            a.resources.is_empty(),
            "a listing read before the purge was shown"
        );
        assert!(a.cached(("systm-d", "josephine")).is_none());

        let t1 = t0 + ms(1000);
        assert!(
            a.follow_cursor(t1).is_none(),
            "the fresh listing waits a pause"
        );
        let fresh = a
            .follow_cursor(t1 + PAUSE)
            .expect("a fresh listing follows the purge");
        assert_eq!(fresh.repo, "josephine");
    }

    /// `→`/`Tab` and `←` walk the three columns (spec §2), both ways, and
    /// wrap. The repo level was unreachable at one point because `Focus`
    /// only had two variants. The test this replaces re-implemented the
    /// cycle inline and asserted on its own copy, so nothing the event loop
    /// did could make it fail.
    #[test]
    fn focus_walks_the_three_columns_both_ways_and_wraps() {
        assert_eq!(Focus::Orgs.next(), Focus::Repos);
        assert_eq!(Focus::Repos.next(), Focus::Resources);
        assert_eq!(Focus::Resources.next(), Focus::Orgs);
        assert_eq!(Focus::Orgs.previous(), Focus::Resources);
        assert_eq!(Focus::Repos.previous(), Focus::Orgs);
        assert_eq!(Focus::Resources.previous(), Focus::Repos);
    }
}
