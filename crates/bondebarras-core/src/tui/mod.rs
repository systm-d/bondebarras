//! Terminal user interface.

pub mod app;
pub mod theme;
pub mod views;

use crate::api::{Client, caches};
use crate::clean::{self, Plan, Progress};
use crate::model::{OrgSummary, Resource, ResourceKind};
use crate::scan;
use anyhow::Result;
use app::{App, Focus, Load, View};
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// How long a draw waits for a key before looping. Short enough that deletion
/// progress lands smoothly, long enough not to spin.
const TICK: Duration = Duration::from_millis(120);

/// Restores the terminal when it goes out of scope — including on an unwind.
///
/// Without this, a panic inside the event loop leaves raw mode enabled and the
/// alternate screen active, and the user's shell needs `reset` to recover.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        // ratatui hides the cursor on every draw and never shows it again on
        // its own: without `Show` here, a panic exits with an invisible
        // cursor and the shell needs `reset` to recover it.
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen, Show);
    }
}

/// Run the TUI until the user quits, restoring the terminal on every path.
///
/// The client arrives behind an `Arc` because deletions run on a spawned task:
/// awaiting them inline would freeze the interface for the whole purge, which
/// is exactly what the progress channel exists to avoid.
pub async fn run_tui(client: Arc<Client>, orgs: Vec<OrgSummary>) -> Result<()> {
    enable_raw_mode()?;
    // Armed before anything else can fail: every path from here on restores.
    let _guard = TerminalGuard;

    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = event_loop(client, &mut terminal, App::new(orgs)).await;

    terminal.show_cursor()?;
    result
}

async fn event_loop<B>(client: Arc<Client>, terminal: &mut Terminal<B>, mut app: App) -> Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let mut pending: Option<Plan> = None;
    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();
    let (landings_tx, mut landings) = mpsc::unbounded_channel::<Landing>();

    while !app.should_quit {
        // Spec §3: the resources column follows the repository under the
        // column-2 cursor, and its load starts once that cursor has rested
        // `app::LOAD_PAUSE`. Checked on every pass — after each key, and on
        // each `TICK` when no key comes — so the load starts within one tick
        // of the pause ending. `App::follow_cursor` says what arms it.
        if let Some(load) = app.follow_cursor(Instant::now()) {
            spawn_load(&client, &landings_tx, load);
        }

        terminal.draw(|f| views::render(&mut app, f, pending.as_ref()))?;

        // Drain deletion progress without blocking the draw.
        while let Ok(msg) = rx.try_recv() {
            // A purge's `Finished` writes a recap of bytes freed; the orgs
            // column must agree on the same frame, not show the pre-purge
            // total for the org the purge just happened in. One request is
            // enough — a stale number would be worse than just leaving the
            // old one if this fails. `apply_progress` says which org.
            if let Some(org_login) = apply_progress(&mut app, msg)
                && let Ok(repos) = caches::usage_by_repository(&client, &org_login).await
            {
                app.refresh_org_cache(&org_login, repos);
            }
        }

        // Listings that landed, each with the `Load` it was started under:
        // `App::land_load` drops the ones superseded since.
        while let Ok((load, outcome)) = landings.try_recv() {
            app.land_load(load, outcome);
        }

        if !event::poll(TICK)? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // The confirmation modal swallows every key while it is up.
        if let Some(plan) = pending.take() {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                if plan.is_archive() {
                    // Not a deletion — say so. `plan.repo` is the whole
                    // plan here: an archive plan is always exactly one
                    // Repository item (see `Plan::is_archive`'s own doc
                    // comment), so there is nothing an itemised summary
                    // would add.
                    app.status = format!("Archivage de {} …", plan.repo);
                } else {
                    app.status = format!("Suppression de {} …", plan.summary());
                }
                // Captured now, not read from `loaded` when `Finished` lands:
                // the user can navigate to a different org while this runs.
                app.purging_org = Some(plan.owner.clone());
                app.purges_in_flight += 1;
                // Spawned, not awaited: the loop keeps drawing and draining
                // `rx` while the purge runs.
                let tx = tx.clone();
                let client = Arc::clone(&client);
                tokio::spawn(async move { clean::execute(&client, plan, tx).await });
            } else {
                app.status = "Annulé.".into();
            }
            continue;
        }

        // Filter input owns the keyboard while it is active.
        if app.filter_mode {
            match key.code {
                KeyCode::Enter | KeyCode::Esc => app.filter_mode = false,
                KeyCode::Backspace => {
                    app.filter.pop();
                    app.res_cursor = 0;
                }
                KeyCode::Char(c) => {
                    app.filter.push(c);
                    app.res_cursor = 0;
                }
                _ => {}
            }
            continue;
        }

        // The Billing tab is strictly diagnostic. Everything below this
        // point — including `d` — is `Orgs`-only, so as long as this block
        // `continue`s, no selection and no deletion is reachable while
        // Billing is on screen; only quitting, switching tabs back, and
        // moving between months are.
        if app.view == View::Billing {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => request_quit(&mut app),
                KeyCode::Char('b') => app.view = View::Orgs,
                KeyCode::Left => app.month_cursor = app.month_cursor.saturating_sub(1),
                KeyCode::Right => {
                    let max = app
                        .orgs
                        .get(app.org_cursor)
                        .and_then(|o| o.billing.as_ref())
                        .map_or(0, |b| b.months().len().saturating_sub(1));
                    app.month_cursor = (app.month_cursor + 1).min(max);
                }
                _ => {}
            }
            continue;
        }

        // `←`/`→`/`Tab` move between the columns, in every layout — with a
        // single column on screen they change which one it is.
        if let Some(focus) = column_after(app.focus, key.code) {
            app.focus = focus;
            continue;
        }

        match key.code {
            KeyCode::Esc if !app.filter.is_empty() => {
                app.filter.clear();
                app.res_cursor = 0;
            }
            KeyCode::Char('q') | KeyCode::Esc => request_quit(&mut app),
            KeyCode::Char('b') => app.view = View::Billing,
            KeyCode::Down => match app.focus {
                Focus::Orgs => {
                    app.org_cursor = (app.org_cursor + 1).min(app.orgs.len().saturating_sub(1));
                    app.reset_scoped_cursors();
                }
                Focus::Repos => {
                    let max = app
                        .orgs
                        .get(app.org_cursor)
                        .map_or(0, |o| o.repos.len().saturating_sub(1));
                    app.repo_cursor = (app.repo_cursor + 1).min(max);
                }
                Focus::Resources => {
                    let max = app.visible_resources().len().saturating_sub(1);
                    app.res_cursor = (app.res_cursor + 1).min(max);
                }
            },
            KeyCode::Up => match app.focus {
                Focus::Orgs => {
                    app.org_cursor = app.org_cursor.saturating_sub(1);
                    app.reset_scoped_cursors();
                }
                Focus::Repos => app.repo_cursor = app.repo_cursor.saturating_sub(1),
                Focus::Resources => app.res_cursor = app.res_cursor.saturating_sub(1),
            },
            KeyCode::Enter => {
                // Spec §3: load the repository under the column-2 cursor
                // now, without waiting for the pause. `App::force_load`
                // requests nothing when its listing is already shown or on
                // its way.
                if let Some(load) = app.force_load() {
                    spawn_load(&client, &landings_tx, load);
                }
            }
            // `espace`, `s`, `A` and `f` act on the focused column only — see
            // `column_action`.
            KeyCode::Char(' ' | 's' | 'A' | 'f') => column_action(&mut app, key.code),
            // Dispatches on focus, not on preferring a ticked repository
            // unconditionally — see `App::take_focused_plan`'s own doc
            // comment for Finding 1 of the final review, which this used to
            // get wrong.
            KeyCode::Char('d') => {
                if let Some(plan) = app.take_focused_plan()
                    && !plan.items.is_empty()
                {
                    pending = Some(plan);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// The status line's wording once a purge — or a repository archive —
/// finishes.
///
/// A repository archive is not a deletion: nothing is freed, and the
/// repository is simply turned read-only, not removed from anywhere.
/// Reusing `clean::finished_recap`'s "N libérés"/"N élément(s) supprimé(s) ·
/// taille inconnue" wording for it would call an archived repository
/// "supprimé" — false, and this project treats a false status line the same
/// as any other lie to the user. `archived_repo` — the repo name, captured
/// at the moment a `clean::Plan::is_archive` plan was confirmed — is what
/// lets this tell the two cases apart; `None` falls back to exactly the
/// ordinary recap, unchanged.
fn purge_finished_status(
    archived_repo: Option<&str>,
    freed: u64,
    failures: usize,
    deleted: usize,
    deleted_sizeless: usize,
) -> String {
    if let Some(repo) = archived_repo {
        if failures == 0 {
            format!("Dépôt {repo} archivé.")
        } else {
            format!("Erreur : archivage de {repo} refusé.")
        }
    } else {
        let recap = clean::finished_recap(freed, deleted, deleted_sizeless);
        if failures == 0 {
            format!("Bon débarras ! {recap}.")
        } else {
            format!("Bon débarras ! {recap}, {failures} échec(s).")
        }
    }
}

/// A listing as it lands back in the event loop: the `Load` it was started
/// under, and what `scan::repo_detail_with_warnings` returned.
type Landing = (Load, Result<(Vec<Resource>, Vec<&'static str>)>);

/// Fetches `load`'s listing on a spawned task and sends it back, with its
/// `Load`, on `landings`.
///
/// Spawned, not awaited — the purge's shape. `Entrée` used to await the nine
/// listings inline, freezing the draw and the keys for the whole load; a
/// load that starts on its own after a pause cannot do that, the cursor has
/// to stay free while it runs, and a cursor free to leave is what the
/// `Load`'s generation exists for. A task on the runtime the purges already
/// run on, not a thread.
///
/// Calls `repo_detail_with_warnings` directly, not the `repo_detail` stderr
/// wrapper: Finding 5 of the final review needs the failed family names as
/// data so `App::finish_loading` can put them where the user is actually
/// looking — the wrapper's `eprintln!` writes to a stream the alternate
/// screen hides.
fn spawn_load(client: &Arc<Client>, landings: &mpsc::UnboundedSender<Landing>, load: Load) {
    let client = Arc::clone(client);
    let landings = landings.clone();
    tokio::spawn(async move {
        let outcome = scan::repo_detail_with_warnings(&client, &load.org, &load.repo).await;
        // A closed channel means the event loop has ended — the user quit —
        // and there is no one left to show the listing to.
        let _ = landings.send((load, outcome));
    });
}

/// Applies one purge message to `app`, and returns the org whose cache
/// figures the loop must refresh when it is a purge's `Finished`.
///
/// Split out of `event_loop` so what each message changes can be asserted on
/// without a terminal; the refresh needs the client and stays in the loop.
///
/// Every item outcome — a deletion, a refused one, an archive — invalidates
/// its repository's kept listing (`App::forget`), under the identity the
/// message carries, never the repository under the cursor, which can have
/// moved since the purge began. At the item rather than at `Finished`:
/// `Finished` names no repository (`archived_repo` is an archive's bare
/// name, without its org), and between a first deletion and `Finished` a
/// listing kept from before the purge would show what was just deleted as
/// soon as the cursor came back to it. A refused deletion invalidates too: a
/// timeout can hide one GitHub carried out.
fn apply_progress(app: &mut App, msg: Progress) -> Option<String> {
    match msg {
        Progress::Done {
            kind,
            id,
            owner,
            repo,
        } => {
            if kind == ResourceKind::Repository {
                // Not a deletion: the repository stays in the tree, now
                // read-only. `owner`/`repo` name the archive this message is
                // actually about — Findings 3 and 4 of the final review:
                // reading `app.selected_repo` here instead, as an earlier
                // version did, updated whatever the tree currently had
                // ticked, which a second archive started before this one
                // landed could easily have replaced.
                app.archive_done(&owner, &repo);
            } else {
                app.resource_deleted(kind, id, &owner, &repo);
            }
            app.forget((&owner, &repo));
            None
        }
        Progress::Failed {
            kind,
            id,
            reason,
            owner,
            repo,
        } => {
            if kind == ResourceKind::Repository {
                app.archive_failed(&owner, &repo);
                app.status = format!("Erreur : archivage de {repo} refusé — {reason}");
            } else {
                app.resource_failed(kind, id, &owner, &repo);
                app.status = format!("Erreur : suppression de {id} — {reason}");
            }
            app.forget((&owner, &repo));
            None
        }
        Progress::Finished {
            freed,
            failures,
            deleted,
            deleted_sizeless,
            archived_repo,
        } => {
            // One purge is done. `purge_finished` only disarms the quit
            // guard once every in-flight purge has settled — a second purge
            // started before this one landed must keep `q` guarded.
            app.purge_finished();
            // `archived_repo` comes straight off this message —
            // `clean::execute` computed it from its own `Plan` — not from a
            // shared `archiving_target` local the way an earlier version of
            // the loop did: a second archive confirmed before this
            // `Finished` landed could overwrite that local before this one
            // ever read it. See `Progress::Finished`'s own doc comment
            // (Findings 3 and 4 of the final review).
            app.status = purge_finished_status(
                archived_repo.as_deref(),
                freed,
                failures,
                deleted,
                deleted_sizeless,
            );
            // Reads `purging_org`, not `loaded`: the purge runs on a spawned
            // task while the loop keeps handling keys, so the user can move
            // to a different repo — possibly in a different org — before
            // `Finished` lands. `loaded` would then name the wrong org.
            // `take()` both reads it and clears it, success or not.
            app.purging_org.take()
        }
    }
}

/// The column a key moves focus to, or `None` when it is not a column key.
///
/// `→` and `Tab` are one key, as spec §2's key table has it, and both wrap
/// from the resources back to the orgs; `←` wraps the other way. They work
/// in every layout: with one column on screen they change which column that
/// is, with two they bring the orgs column in on the left
/// (`tui::views::column_areas`). Split out of `event_loop` so the mapping
/// can be asserted on without a terminal.
fn column_after(focus: Focus, code: KeyCode) -> Option<Focus> {
    match code {
        KeyCode::Right | KeyCode::Tab => Some(focus.next()),
        KeyCode::Left => Some(focus.previous()),
        _ => None,
    }
}

/// Handle a `q`/`Esc` press, from either top-level view.
///
/// A purge runs on a spawned task while the event loop keeps handling keys;
/// quitting mid-purge drops whatever is still queued and shows no summary.
/// Say so once and let a second press through — an unattended quit must not
/// silently cut an irreversible operation short.
fn request_quit(app: &mut App) {
    if app.purges_in_flight > 0 && !app.quit_armed {
        app.quit_armed = true;
        app.status = "Purge en cours — [q] à nouveau pour quitter sans l'achever.".into();
    } else {
        app.should_quit = true;
    }
}

/// Applies `espace`, `s`, `A` or `f` to the column focus is on, and only to
/// it.
///
/// - The resources column: `espace` ticks the row under the cursor, `A`
///   takes the ⚑ rows, `s` cycles the sort, `f` opens the filter.
/// - The repos column: `espace` ticks the repository for archiving, under
///   `App::toggle_repo_selected`'s own rules — a repository cannot share
///   `toggle_selected`'s guard or storage (see that method's doc comment).
///   `A`, `s` and `f` do nothing.
/// - The orgs column: nothing.
///
/// These keys used to reach the resource list from every column. Once the
/// columns fold with the width, that list can be off screen while the orgs
/// or the repos have focus — `espace` and `A` then ticked resources the user
/// could not see, and `d` deleted them. The per-column footer
/// (`tui::views::column_actions`) announces these keys only where they act.
fn column_action(app: &mut App, code: KeyCode) {
    match (app.focus, code) {
        (Focus::Resources, KeyCode::Char(' ')) => app.toggle_selected(),
        (Focus::Resources, KeyCode::Char('A')) => app.select_all_stale(),
        (Focus::Resources, KeyCode::Char('s')) => app.cycle_sort(),
        (Focus::Resources, KeyCode::Char('f')) => app.filter_mode = true,
        (Focus::Repos, KeyCode::Char(' ')) => app.toggle_repo_selected(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::SortKey;

    /// One org with one archivable repository, loaded, holding two caches a
    /// closed pull request left behind — stale, so `A` would take both, and
    /// the cursor on one, so `espace` would tick it. A key that reaches the
    /// resource list from the wrong column has something to change.
    fn app_with_loaded_resources() -> App {
        let mut app = App::new(vec![crate::model::OrgSummary {
            login: "systm-d".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![crate::model::RepoSummary {
                name: "josephine".into(),
                cache_bytes: 0,
                cache_count: 0,
                private: false,
                age_days: 5,
                class: crate::repos::RepoClass::Archivable,
            }],
            billing: None,
        }]);
        app.loaded = Some(("systm-d".into(), "josephine".into()));
        app.resources = (1..=2)
            .map(|id| crate::model::Resource {
                kind: ResourceKind::Cache,
                id,
                label: format!("cache-{id}"),
                size_bytes: 100 * id,
                age_days: 40,
                git_ref: Some("refs/pull/32/merge".into()),
                stale_pr: true,
                protected: false,
                branch_class: None,
                safety: crate::safety::Safety::Keep,
            })
            .collect();
        app
    }

    /// Presses `espace`, `A`, `s` and `f`, in that order, through the same
    /// dispatch the event loop uses. `f` goes last: once the filter is open
    /// the loop hands every key to it instead.
    fn press_list_keys(app: &mut App) {
        for key in [' ', 'A', 's', 'f'] {
            column_action(app, KeyCode::Char(key));
        }
    }

    /// The resources column keeps all four keys as they were: `espace` ticks
    /// the row under the cursor, `A` takes the ⚑ rows, `s` cycles the sort,
    /// `f` opens the filter. The positive control for the three tests below.
    #[test]
    fn list_keys_act_on_the_resource_list_in_the_resources_column() {
        let mut app = app_with_loaded_resources();
        app.focus = Focus::Resources;

        column_action(&mut app, KeyCode::Char(' '));
        assert_eq!(
            app.selected.len(),
            1,
            "espace must tick the row under the cursor"
        );
        column_action(&mut app, KeyCode::Char('A'));
        assert_eq!(app.selected.len(), 2, "A must take every ⚑ row");
        column_action(&mut app, KeyCode::Char('s'));
        assert_eq!(app.sort, SortKey::Age, "s must cycle the sort");
        column_action(&mut app, KeyCode::Char('f'));
        assert!(app.filter_mode, "f must open the filter");
    }

    /// The repos column: `espace` keeps its v0.5 meaning — tick the
    /// repository for archiving, under `App::toggle_repo_selected`'s own
    /// rules — and `A`, `s`, `f` do nothing. None of the four may reach the
    /// resource list: its column is not the one the keyboard drives.
    #[test]
    fn list_keys_leave_the_resource_list_alone_in_the_repos_column() {
        let mut app = app_with_loaded_resources();
        app.focus = Focus::Repos;

        press_list_keys(&mut app);

        assert_eq!(
            app.selected_repo,
            Some(("systm-d".to_string(), "josephine".to_string())),
            "espace must still tick the repository"
        );
        assert!(
            app.selected.is_empty(),
            "a resource was ticked from the repos column: {:?}",
            app.selected
        );
        assert_eq!(app.sort, SortKey::Size, "s sorted from the repos column");
        assert!(
            !app.filter_mode,
            "f opened the filter from the repos column"
        );
    }

    /// The orgs column: none of the four keys does anything.
    #[test]
    fn list_keys_do_nothing_in_the_orgs_column() {
        let mut app = app_with_loaded_resources();
        app.focus = Focus::Orgs;

        press_list_keys(&mut app);

        assert!(
            app.selected.is_empty(),
            "a resource was ticked from the orgs column: {:?}",
            app.selected
        );
        assert_eq!(
            app.selected_repo, None,
            "a repository was ticked from the orgs column"
        );
        assert_eq!(app.sort, SortKey::Size, "s sorted from the orgs column");
        assert!(!app.filter_mode, "f opened the filter from the orgs column");
    }

    /// The case that made this a safety defect: a terminal narrow enough for
    /// one column, focus on the orgs — the resource list is not on screen at
    /// all, yet `espace` and `A` used to tick resources in it, and `d` would
    /// then delete what the user never saw. Checks first that the resources
    /// column really is hidden at this width (through the layout `render`
    /// uses), then that the keys changed nothing and `d` finds nothing to
    /// delete.
    #[test]
    fn list_keys_cannot_reach_a_resource_list_hidden_by_the_one_column_layout() {
        let mut app = app_with_loaded_resources();
        app.focus = Focus::Orgs;
        let (_, columns) = views::testing::layout(&app, 70, 24);
        assert!(
            columns.resources.is_none(),
            "the fixture must hide the resources column at width 70"
        );

        press_list_keys(&mut app);

        assert!(
            app.selected.is_empty(),
            "resources off screen were ticked: {:?}",
            app.selected
        );
        assert_eq!(app.sort, SortKey::Size);
        assert!(!app.filter_mode);
        assert!(
            app.take_focused_plan()
                .is_none_or(|plan| plan.items.is_empty()),
            "d would delete resources the user never saw"
        );
    }

    #[test]
    fn purge_finished_status_names_the_repo_when_archiving_succeeds() {
        // A repository archive is not a deletion — nothing is freed, the
        // repository just turns read-only. Reusing `finished_recap`'s
        // "supprimé" wording for it would be a lie: the whole point of this
        // helper is that the two paths never share text.
        let s = purge_finished_status(Some("lokiprint"), 0, 0, 1, 1);
        assert!(s.contains("lokiprint"), "got: {s}");
        assert!(s.contains("archivé"), "got: {s}");
        assert!(!s.to_lowercase().contains("supprimé"), "got: {s}");
        assert!(!s.contains("Bon débarras"), "got: {s}");
    }

    #[test]
    fn purge_finished_status_names_the_repo_when_archiving_fails() {
        let s = purge_finished_status(Some("lokiprint"), 0, 1, 0, 0);
        assert!(s.contains("lokiprint"), "got: {s}");
        assert!(s.to_lowercase().contains("erreur"), "got: {s}");
    }

    /// Without `archived_repo`, this must fall back to exactly
    /// `clean::finished_recap`'s own wording — the ordinary deletion path
    /// must not change at all.
    #[test]
    fn purge_finished_status_falls_back_to_the_ordinary_recap_when_nothing_was_archived() {
        let s = purge_finished_status(None, 3_000_000, 0, 2, 0);
        assert!(s.contains("Bon débarras"), "got: {s}");
        assert!(s.contains("3.0 Mo"), "got: {s}");
    }

    #[test]
    fn purge_finished_status_reports_failure_count_for_an_ordinary_purge() {
        let s = purge_finished_status(None, 0, 2, 3, 0);
        assert!(s.contains("2 échec"), "got: {s}");
    }

    #[test]
    fn request_quit_arms_the_guard_while_a_purge_is_in_flight_then_quits_on_a_second_press() {
        let mut app = App::new(vec![]);
        app.purges_in_flight = 1;

        request_quit(&mut app);
        assert!(app.quit_armed, "first press must warn, not quit");
        assert!(!app.should_quit);

        request_quit(&mut app);
        assert!(
            app.should_quit,
            "a second press must go through despite the purge"
        );
    }

    #[test]
    fn request_quit_quits_immediately_with_no_purge_running() {
        let mut app = App::new(vec![]);
        request_quit(&mut app);
        assert!(app.should_quit);
    }

    /// One org, `systm-d`, holding josephine then claudine.
    fn app_with_two_repositories() -> App {
        let repo = |name: &str| crate::model::RepoSummary {
            name: name.into(),
            cache_bytes: 0,
            cache_count: 0,
            private: false,
            age_days: 5,
            class: crate::repos::RepoClass::Archivable,
        };
        App::new(vec![crate::model::OrgSummary {
            login: "systm-d".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![repo("josephine"), repo("claudine")],
            billing: None,
        }])
    }

    fn branch_named(name: &str) -> crate::model::Resource {
        crate::model::Resource {
            kind: ResourceKind::Branch,
            id: crate::api::refs::resource_id(name),
            label: name.into(),
            size_bytes: 0,
            age_days: 3,
            git_ref: None,
            stale_pr: false,
            protected: false,
            branch_class: Some(crate::refs::BranchClass::Merged),
            safety: crate::safety::Safety::Keep,
        }
    }

    /// Every purge outcome for claudine — a deletion, a refused deletion, an
    /// archive — lands while the cursor is on josephine, both cached. The
    /// entry that goes is claudine's, the identity the message carries; a
    /// purge resolved against the cursor would drop josephine's instead.
    #[test]
    fn a_purge_outcome_invalidates_its_own_repository_not_the_cursors() {
        let outcomes = [
            Progress::Done {
                kind: ResourceKind::Cache,
                id: 1,
                owner: "systm-d".into(),
                repo: "claudine".into(),
            },
            Progress::Failed {
                kind: ResourceKind::Cache,
                id: 1,
                reason: "timeout".into(),
                owner: "systm-d".into(),
                repo: "claudine".into(),
            },
            Progress::Done {
                kind: ResourceKind::Repository,
                id: 1,
                owner: "systm-d".into(),
                repo: "claudine".into(),
            },
        ];
        for outcome in outcomes {
            let mut app = app_with_two_repositories();
            app.remember(("systm-d".into(), "josephine".into()), vec![]);
            app.remember(("systm-d".into(), "claudine".into()), vec![]);
            assert!(app.follow_cursor(std::time::Instant::now()).is_none());
            let label = format!("{outcome:?}");

            apply_progress(&mut app, outcome);

            assert!(
                app.cached(("systm-d", "claudine")).is_none(),
                "the purged repository's entry survived {label}"
            );
            assert!(
                app.cached(("systm-d", "josephine")).is_some(),
                "the cursor's repository was invalidated instead, on {label}"
            );
        }
    }

    /// The in-flight half: a purge landing for claudine leaves josephine's
    /// listing, in flight under the cursor, to land and show.
    #[test]
    fn a_purge_elsewhere_leaves_the_cursors_listing_in_flight_alone() {
        let mut app = app_with_two_repositories();
        assert!(app.follow_cursor(std::time::Instant::now()).is_none());
        let load = app.force_load().expect("josephine's load starts");

        apply_progress(
            &mut app,
            Progress::Done {
                kind: ResourceKind::Cache,
                id: 1,
                owner: "systm-d".into(),
                repo: "claudine".into(),
            },
        );
        app.land_load(load, Ok((vec![branch_named("feature/x")], vec![])));

        assert_eq!(app.loaded, Some(("systm-d".into(), "josephine".into())));
        assert_eq!(app.resources.len(), 1);
    }

    /// A branch's id is a hash of its name (`api::refs::resource_id`), so
    /// `feature/x` has the same id in every repository. A deletion — or a
    /// refused one — landing for claudine must leave josephine's row and
    /// tick alone, now that the list on screen follows the cursor. The same
    /// message for josephine is the positive control.
    #[test]
    fn a_deletion_in_another_repository_leaves_the_shown_list_alone() {
        let mut app = app_with_two_repositories();
        let row = branch_named("feature/x");
        let tick = (row.kind, row.id);
        app.remember(("systm-d".into(), "josephine".into()), vec![row]);
        assert!(app.follow_cursor(std::time::Instant::now()).is_none());
        app.selected.insert(tick);
        let outcome = |kind_done: bool, repo: &str| {
            if kind_done {
                Progress::Done {
                    kind: tick.0,
                    id: tick.1,
                    owner: "systm-d".into(),
                    repo: repo.into(),
                }
            } else {
                Progress::Failed {
                    kind: tick.0,
                    id: tick.1,
                    reason: "422".into(),
                    owner: "systm-d".into(),
                    repo: repo.into(),
                }
            }
        };

        apply_progress(&mut app, outcome(false, "claudine"));
        apply_progress(&mut app, outcome(true, "claudine"));
        assert_eq!(
            app.resources.len(),
            1,
            "claudine's deletion removed josephine's row"
        );
        assert!(
            app.selected.contains(&tick),
            "claudine's purge unticked josephine's row"
        );

        apply_progress(&mut app, outcome(true, "josephine"));
        assert!(app.resources.is_empty());
        assert!(app.selected.is_empty());
    }

    /// Spec §2's key table: `→` and `Tab` go to the next column, `←` to the
    /// previous one, wrapping, in every layout. Any other key is not a
    /// column key and must fall through to the rest of the loop — `↓` moves
    /// a cursor, it must not also move the column.
    #[test]
    fn right_and_tab_move_to_the_next_column_and_left_to_the_previous() {
        assert_eq!(
            column_after(Focus::Orgs, KeyCode::Right),
            Some(Focus::Repos)
        );
        assert_eq!(column_after(Focus::Orgs, KeyCode::Tab), Some(Focus::Repos));
        assert_eq!(
            column_after(Focus::Resources, KeyCode::Right),
            Some(Focus::Orgs)
        );
        assert_eq!(column_after(Focus::Repos, KeyCode::Left), Some(Focus::Orgs));
        assert_eq!(
            column_after(Focus::Orgs, KeyCode::Left),
            Some(Focus::Resources)
        );
        assert_eq!(column_after(Focus::Repos, KeyCode::Down), None);
    }
}
