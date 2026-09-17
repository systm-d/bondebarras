//! Terminal user interface.

pub mod app;
pub mod theme;
pub mod views;

use crate::api::{Client, caches};
use crate::clean::{self, Plan, Progress};
use crate::model::{OrgSummary, Resource, ResourceKind};
use crate::scan;
use anyhow::Result;
use app::{App, Focus, Load, LoadGeneration, View};
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
    let (tx, mut rx) = mpsc::unbounded_channel::<Tagged>();
    let (landings_tx, mut landings) = mpsc::unbounded_channel::<Landing>();
    let (ticks_tx, mut ticks) = mpsc::unbounded_channel::<LoadGeneration>();

    while !app.should_quit {
        // Spec §3: the resources column follows the repository under the
        // column-2 cursor, and its load starts once that cursor has rested
        // `app::LOAD_PAUSE`. Checked on every pass — after each key, and on
        // each `TICK` when no key comes — so the load starts within one tick
        // of the pause ending. `App::follow_cursor` says what arms it.
        if let Some(load) = app.follow_cursor(Instant::now()) {
            spawn_load(&client, &landings_tx, &ticks_tx, load);
        }

        terminal.draw(|f| views::render(&mut app, f, pending.as_ref()))?;

        // Drain deletion progress without blocking the draw.
        while let Ok((purge, msg)) = rx.try_recv() {
            // A purge's `Finished` writes a recap of bytes freed; the orgs
            // column must agree on the same frame, not show the pre-purge
            // total for the org the purge just happened in. One request is
            // enough — a stale number would be worse than just leaving the
            // old one if this fails. `apply_progress` says which org.
            if let Some(org_login) = apply_progress(&mut app, &purge, msg)
                && let Ok(repos) = caches::usage_by_repository(&client, &org_login).await
            {
                app.refresh_org_cache(&org_login, repos);
            }
        }

        // Ticks of the loads in flight, each under its load's generation:
        // `App::load_ticked` drops those of a load superseded since, as
        // `land_load` drops its listing. Drained before the listings, which
        // their loads send only once every tick has gone.
        while let Ok(generation) = ticks.try_recv() {
            app.load_ticked(generation);
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
                // Recorded now, from the plan, not read from `loaded` or the
                // cursor when `Finished` lands: the user can navigate to a
                // different org or repository while this runs.
                app.purge_launched(&plan);
                spawn_purge(&client, &tx, plan);
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
                KeyCode::Left | KeyCode::Right => page_months(&mut app, key.code),
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
            KeyCode::Up | KeyCode::Down => move_cursor(&mut app, key.code),
            KeyCode::Enter => {
                // Spec §3: load the repository under the column-2 cursor
                // now, without waiting for the pause. `App::force_load`
                // requests nothing when its listing is already shown or on
                // its way.
                if let Some(load) = app.force_load() {
                    spawn_load(&client, &landings_tx, &ticks_tx, load);
                }
            }
            // `espace`, `s`, `A`, `V` and `f` act on the focused column only —
            // see `column_action`.
            KeyCode::Char(' ' | 's' | 'A' | 'V' | 'f') => column_action(&mut app, key.code),
            // The plan of the column `d` is pressed in, and none from the
            // orgs column — see `plan_for_d`.
            KeyCode::Char('d') => {
                if let Some(plan) = plan_for_d(&app) {
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
/// under, and what `scan::repo_detail_ticking` returned.
type Landing = (Load, Result<(Vec<Resource>, Vec<&'static str>)>);

/// Fetches `load`'s listing on a spawned task and sends it back, with its
/// `Load`, on `landings` — and, as each of its calls lands, a tick under the
/// load's generation on `ticks`, for the load bar (`App::load_ticked`).
///
/// Spawned, not awaited — the purge's shape. `Entrée` used to await the nine
/// listings inline, freezing the draw and the keys for the whole load; a
/// load that starts on its own after a pause cannot do that, the cursor has
/// to stay free while it runs, and a cursor free to leave is what the
/// `Load`'s generation exists for. A task on the runtime the purges already
/// run on, not a thread.
///
/// Calls `repo_detail_ticking` — `repo_detail_with_warnings`, ticking —
/// directly, not the `repo_detail` stderr wrapper: Finding 5 of the final
/// review needs the failed family names as data so `App::finish_loading` can
/// put them where the user is actually looking — the wrapper's `eprintln!`
/// writes to a stream the alternate screen hides.
///
/// The ticks go out under the load's generation, since they travel apart
/// from the listing, and every one of them before the listing: the run drops
/// its tick sender as it returns, which ends the forwarding, and only then is
/// the listing sent. No tick of a load trails its own landing.
fn spawn_load(
    client: &Arc<Client>,
    landings: &mpsc::UnboundedSender<Landing>,
    ticks: &mpsc::UnboundedSender<LoadGeneration>,
    load: Load,
) {
    let client = Arc::clone(client);
    let landings = landings.clone();
    let ticks = ticks.clone();
    tokio::spawn(async move {
        let generation = load.generation();
        let (tick, mut landed) = mpsc::unbounded_channel::<()>();
        let forward = async {
            while landed.recv().await.is_some() {
                // A closed channel means the event loop has ended — the user
                // quit — and no one is left to show the bar to.
                let _ = ticks.send(generation);
            }
        };
        let (outcome, ()) = tokio::join!(
            scan::repo_detail_ticking(&client, &load.org, &load.repo, tick),
            forward
        );
        // A closed channel means the event loop has ended — the user quit —
        // and there is no one left to show the listing to.
        let _ = landings.send((load, outcome));
    });
}

/// A purge message as it reaches the event loop: tagged with the repository
/// the purge's plan concerns, recorded from the plan at launch
/// (`spawn_purge`).
type Tagged = ((String, String), Progress);

/// Runs `plan` on a spawned task, forwarding each message it sends to
/// `progress` tagged with the repository the plan concerns.
///
/// Spawned, not awaited: the loop keeps drawing and draining `progress`
/// while the purge runs. The tag is recorded from the plan here, at launch:
/// `Progress::Finished` names no repository, and `clean.rs` stays as it is
/// (spec §6), yet the end of a purge must forget the repository it concerned
/// — never the one the cursor happens to be on by then (`App::purge_ended`).
/// One task runs the purge and its forwarding side by side; `execute` drops
/// its sender when it returns, which ends the forwarding.
fn spawn_purge(client: &Arc<Client>, progress: &mpsc::UnboundedSender<Tagged>, plan: Plan) {
    let purge = (plan.owner.clone(), plan.repo.clone());
    let client = Arc::clone(client);
    let progress = progress.clone();
    tokio::spawn(async move {
        let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();
        let forward = async {
            while let Some(msg) = rx.recv().await {
                // A closed channel means the event loop has ended — the user
                // quit — and no one is left to show the purge to.
                let _ = progress.send((purge.clone(), msg));
            }
        };
        tokio::join!(clean::execute(&client, plan, tx), forward);
    });
}

/// Applies one purge message to `app`, tagged with the repository the
/// purge's plan concerns (`spawn_purge`), and returns the org whose cache
/// figures the loop must refresh when it is the purge's `Finished`.
///
/// Split out of `event_loop` so what each message changes can be asserted on
/// without a terminal; the refresh needs the client and stays in the loop.
///
/// Ruling F3 (2026-09-11): a purge starts no reload. A deletion takes its
/// row out of its repository's kept listing and out of the list on screen
/// (`App::resource_deleted`); a refused one leaves it, since the resource
/// still exists. Only the purge's `Finished` forgets that repository's
/// listing (`App::purge_ended`) — under the tag, because `Finished` names no
/// repository and the cursor can be anywhere by then.
fn apply_progress(app: &mut App, purge: &(String, String), msg: Progress) -> Option<String> {
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
            app.purge_advanced();
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
            // A refused item is processed, not still waiting: the bar counts
            // it like a deletion, or a purge refused throughout would never
            // reach its end.
            app.purge_advanced();
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
            let org = app.purging_org.take();
            // The repository this purge concerned — its tag, recorded from
            // its plan at launch — is forgotten now, not at each deletion:
            // its listing was kept current row by row, so the purge itself
            // started no reload (ruling F3).
            app.purge_ended(&purge.0, &purge.1);
            org
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

/// `←`/`→` on the Billing tab, paging through the org's report. The tab
/// opens on the newest month — `month_cursor` counts back from it
/// (`views::billing::displayed_month`) — so `←` goes to an older month and
/// `→` back toward the newest, each stopping at its end. Any other key
/// leaves the cursor alone.
fn page_months(app: &mut App, code: KeyCode) {
    let oldest = app
        .orgs
        .get(app.org_cursor)
        .and_then(|o| o.billing.as_ref())
        .map_or(0, |b| b.months().len().saturating_sub(1));
    match code {
        KeyCode::Left => app.month_cursor = (app.month_cursor + 1).min(oldest),
        KeyCode::Right => app.month_cursor = app.month_cursor.saturating_sub(1),
        _ => {}
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

/// Applies `↑` or `↓` to the cursor of the column focus is on. Split out of
/// `event_loop` so what each move changes can be asserted on without a
/// terminal.
fn move_cursor(app: &mut App, code: KeyCode) {
    match (code, app.focus) {
        (KeyCode::Down, Focus::Orgs) => {
            let next = (app.org_cursor + 1).min(app.orgs.len().saturating_sub(1));
            move_org_cursor(app, next);
        }
        (KeyCode::Down, Focus::Repos) => {
            let max = app
                .orgs
                .get(app.org_cursor)
                .map_or(0, |o| o.repos.len().saturating_sub(1));
            app.repo_cursor = (app.repo_cursor + 1).min(max);
        }
        (KeyCode::Down, Focus::Resources) => {
            let max = app.visible_resources().len().saturating_sub(1);
            app.res_cursor = (app.res_cursor + 1).min(max);
        }
        (KeyCode::Up, Focus::Orgs) => move_org_cursor(app, app.org_cursor.saturating_sub(1)),
        (KeyCode::Up, Focus::Repos) => app.repo_cursor = app.repo_cursor.saturating_sub(1),
        (KeyCode::Up, Focus::Resources) => app.res_cursor = app.res_cursor.saturating_sub(1),
        _ => {}
    }
}

/// Puts the org cursor on `next`, and resets what is scoped beneath it — the
/// repos cursor, the month, the repository tick (`App::reset_scoped_cursors`)
/// — only when that is another org. `↓` on the last org and `↑` on the first
/// move no org, and ruling R7-1 lets only an org move clear the tick (F-M2).
fn move_org_cursor(app: &mut App, next: usize) {
    if next != app.org_cursor {
        app.org_cursor = next;
        app.reset_scoped_cursors();
    }
}

/// Applies `espace`, `s`, `A`, `V` or `f` to the column focus is on, and
/// only to it.
///
/// - The resources column: `espace` ticks the row under the cursor, whatever
///   its safety level; `A` adds the ⛑ rows (`App::select_safe`), `V` the •
///   rows too (`App::select_safe_and_check`); `s` cycles the sort, `f` opens
///   the filter.
/// - The repos column: `espace` ticks the repository for archiving, under
///   `App::toggle_repo_selected`'s own rules — a repository cannot share
///   `toggle_selected`'s guard or storage (see that method's doc comment).
///   `A`, `V`, `s` and `f` do nothing.
/// - The orgs column: nothing.
///
/// These keys used to reach the resource list from every column. Once the
/// columns fold with the width, that list can be off screen while the orgs
/// or the repos have focus — `espace` and `A` then ticked resources the user
/// could not see, and `d` deleted them. The per-column footer
/// (`tui::views::column_actions`) announces these keys only where they act.
///
/// In a terminal `A` is Shift+a: spec §4.2's "Maj+A" is no keystroke of its
/// own. So `[A]` keeps its key and its narrower meaning, and the • level
/// takes a key of its own, `V`, rather than widening the one `A` has been.
///
/// None of them acts from the resources column while the last frame drew no
/// row of its list (`App::resources_too_short`) — the same hidden list, by
/// height: a terminal a dozen lines high left the column's head every line,
/// and `A` ticked rows nobody saw (final review I2). The column says the
/// window is too short instead. Nor does `espace` act from the repos column
/// while the last frame drew no repository row there
/// (`App::repos_too_short`, pre-flight 4.14), whose title then says so too.
fn column_action(app: &mut App, code: KeyCode) {
    match (app.focus, code) {
        (Focus::Resources, _) if app.resources_too_short => {}
        (Focus::Repos, _) if app.repos_too_short => {}
        (Focus::Resources, KeyCode::Char(' ')) => app.toggle_selected(),
        (Focus::Resources, KeyCode::Char('A')) => app.select_safe(),
        (Focus::Resources, KeyCode::Char('V')) => app.select_safe_and_check(),
        (Focus::Resources, KeyCode::Char('s')) => app.cycle_sort(),
        (Focus::Resources, KeyCode::Char('f')) => app.filter_mode = true,
        (Focus::Repos, KeyCode::Char(' ')) => app.toggle_repo_selected(),
        _ => {}
    }
}

/// The plan `d` asks to confirm, if any: the focused column's own
/// (`App::take_focused_plan`) — the ticked resources from the resources
/// column, the ticked repository from the repos column, nothing from the
/// orgs column — and none when it holds nothing to do.
///
/// None either from the resources column while the last frame drew no row of
/// its list (`App::resources_too_short`): Tier 1's bare confirmation does not
/// list its items, and none would be on screen (final review I2). Nor from
/// the repos column while the last frame drew no repository row there
/// (`App::repos_too_short`): it would archive a repository no frame showed
/// (pre-flight 4.14).
///
/// Split out of `event_loop` so what `d` opens can be asserted on without a
/// terminal.
fn plan_for_d(app: &App) -> Option<Plan> {
    if app.focus == Focus::Resources && app.resources_too_short {
        return None;
    }
    if app.focus == Focus::Repos && app.repos_too_short {
        return None;
    }
    app.take_focused_plan()
        .filter(|plan| !plan.items.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::Safety;
    use crate::tui::app::SortKey;

    /// One org with one archivable repository, loaded, holding one cache of
    /// each safety level, as `safety::classify` levels them: a closed pull
    /// request's (⛑), one on another branch (•) and the default branch's (no
    /// marker). `A` would take the first, `V` the second too, and the cursor
    /// sits on the third — the biggest, first under the default size sort —
    /// so `espace` would tick it whatever its level. A key that reaches the
    /// resource list from the wrong column has something to change.
    ///
    /// Plus a live, unmerged branch, as production builds it: • and
    /// `protected` (fix round 1). `V` reaches its level and must still leave
    /// it unticked; sizeless, it sorts last and never sits under the cursor.
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
            ..Default::default()
        }]);
        app.loaded = Some(("systm-d".into(), "josephine".into()));
        let cache =
            |id: u64, git_ref: &str, stale_pr: bool, safety: Safety| crate::model::Resource {
                kind: ResourceKind::Cache,
                id,
                label: format!("cache-{id}"),
                size_bytes: 100 * id,
                age_days: 40,
                git_ref: Some(git_ref.into()),
                stale_pr,
                protected: false,
                branch_class: None,
                safety,
            };
        app.resources = vec![
            cache(1, "refs/pull/32/merge", true, Safety::Safe),
            cache(2, "refs/heads/feature/x", false, Safety::Check),
            cache(3, "refs/heads/main", false, Safety::Keep),
            crate::model::Resource {
                kind: ResourceKind::Branch,
                id: 4,
                label: "feature/wip".into(),
                size_bytes: 0,
                age_days: 3,
                git_ref: None,
                stale_pr: false,
                protected: true,
                branch_class: Some(crate::refs::BranchClass::Live),
                safety: Safety::Check,
            },
        ];
        app
    }

    /// The ids of the resources ticked, in increasing order.
    fn ticked(app: &App) -> Vec<u64> {
        let mut ids: Vec<u64> = app.selected.iter().map(|&(_, id)| id).collect();
        ids.sort_unstable();
        ids
    }

    /// Presses `espace`, `A`, `V`, `s` and `f`, in that order, through the
    /// same dispatch the event loop uses. `f` goes last: once the filter is
    /// open the loop hands every key to it instead.
    fn press_list_keys(app: &mut App) {
        for key in [' ', 'A', 'V', 's', 'f'] {
            column_action(app, KeyCode::Char(key));
        }
    }

    /// The resources column takes all five keys: `espace` ticks the row
    /// under the cursor whatever its level, `A` adds the ⛑ rows, `V` the •
    /// rows too, `s` cycles the sort, `f` opens the filter. The positive
    /// control for the three tests below.
    #[test]
    fn list_keys_act_on_the_resource_list_in_the_resources_column() {
        let mut app = app_with_loaded_resources();
        app.focus = Focus::Resources;

        column_action(&mut app, KeyCode::Char(' '));
        assert_eq!(
            ticked(&app),
            vec![3],
            "espace must tick the row under the cursor, whatever its level"
        );
        column_action(&mut app, KeyCode::Char('A'));
        assert_eq!(
            ticked(&app),
            vec![1, 3],
            "A must add the ⛑ row, and it alone"
        );
        assert!(
            app.status.is_empty(),
            "A left no protected ⛑ row, yet wrote: {}",
            app.status
        );
        column_action(&mut app, KeyCode::Char('V'));
        assert_eq!(ticked(&app), vec![1, 2, 3], "V must add the • row");
        assert_eq!(
            app.status, "1 branche vivante protégée non cochée.",
            "V must say it left the protected live branch unticked"
        );
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

    /// `d` acts only from the column that owns its plan (Task 7): the
    /// resources column deletes the ticked resources, the repos column
    /// archives the ticked repository, the orgs column does nothing. One
    /// column on screen, focus on the orgs, a resource ticked earlier from
    /// the resources column and now off screen: `d` must take no plan. It
    /// used to build one from that tick, and Tier 1's bare confirmation does
    /// not list its items — the user would confirm deleting a row they
    /// cannot see.
    #[test]
    fn d_takes_no_plan_from_the_orgs_column_even_with_resources_ticked_off_screen() {
        let mut app = app_with_loaded_resources();
        app.focus = Focus::Resources;
        column_action(&mut app, KeyCode::Char(' '));
        assert_eq!(ticked(&app), vec![3], "the fixture needs a resource ticked");
        app.focus = Focus::Orgs;
        let (_, columns) = views::testing::layout(&app, 70, 24);
        assert!(
            columns.resources.is_none(),
            "the fixture must hide the resources column at width 70"
        );

        assert!(
            app.take_focused_plan().is_none(),
            "d took a plan from the orgs column"
        );
        assert!(
            plan_for_d(&app).is_none(),
            "d opened a confirmation from the orgs column"
        );

        // The positive control: from the resources column, the same tick is
        // what `d` asks to confirm.
        app.focus = Focus::Resources;
        let plan = plan_for_d(&app).expect("d confirms the ticked resource");
        assert_eq!(plan.items.iter().map(|r| r.id).collect::<Vec<_>>(), vec![3]);
    }

    /// The final review's I2 probe as a fixture: a repository whose name
    /// (`SecondBrain-organisation/claudine-landing-positioning`) takes two
    /// lines of a narrow resources column, private and over its included
    /// cache threshold — both gauges, and the over-threshold warning; no
    /// plan read, so the
    /// minutes gauge is the taller unknown-plan one, with its `formule
    /// inconnue` explanation (#11) — with sizeless rows
    /// whose explanation leaves the title, and package versions whose class
    /// takes a second line of their list item. Loaded, focus on the
    /// resources. The biggest row, under the cursor, is a ⛑ cache: `espace`,
    /// `A` and `V` each tick something whenever they act.
    fn app_with_a_tall_resources_head() -> App {
        const ORG: &str = "SecondBrain-organisation";
        const REPO: &str = "claudine-landing-positioning";
        let report = crate::billing::BillingReport {
            items: vec![crate::billing::UsageItem {
                month: "2026-07".into(),
                product: "actions".into(),
                sku: "Actions Linux".into(),
                quantity: 1_000.0,
                unit_type: "Minutes".into(),
                gross: 6.0,
                discount: 0.0,
                net: 6.0,
                repo: REPO.into(),
            }],
        };
        let mut app = App::new(vec![crate::model::OrgSummary {
            login: ORG.into(),
            cache_bytes: 12_360_000_000,
            cache_count: 2,
            repos: vec![crate::model::RepoSummary {
                name: REPO.into(),
                cache_bytes: 12_360_000_000,
                cache_count: 2,
                private: true,
                age_days: 5,
                class: crate::repos::RepoClass::Archivable,
            }],
            billing: Some(report),
            ..Default::default()
        }]);
        app.loaded = Some((ORG.into(), REPO.into()));
        let cache = |id: u64, size_bytes: u64, safety: Safety| crate::model::Resource {
            kind: ResourceKind::Cache,
            id,
            label: format!("v0-rust-coverage-Linux-x64-{id}"),
            size_bytes,
            age_days: 40,
            git_ref: Some("refs/heads/feature/x".into()),
            stale_pr: false,
            protected: false,
            branch_class: None,
            safety,
        };
        let version = |id: u64, digest: &str, class| {
            let v = crate::packages::PackageVersion {
                id,
                digest: digest.into(),
                tags: vec![],
                age_days: 5,
            };
            crate::model::Resource {
                kind: ResourceKind::PackageVersion,
                id,
                label: crate::scan::version_label(&v, class),
                size_bytes: 0,
                age_days: 5,
                git_ref: None,
                stale_pr: false,
                protected: false,
                branch_class: None,
                safety: Safety::Safe,
            }
        };
        app.resources = vec![
            cache(1, 467_000_000, Safety::Safe),
            cache(2, 12_000_000, Safety::Check),
            version(
                3,
                "sha256:9a26c70801010123223adb5e73ff703aca86c15e19b30124ede5628a1e185826",
                crate::packages::VersionClass::Untagged,
            ),
            version(
                4,
                "sha256:1d7018e5672547cced06883706367832e5f1be5fa90bc2038ad308e19958e80e",
                crate::packages::VersionClass::OrphanedAttestation,
            ),
        ];
        app.focus = Focus::Resources;
        app
    }

    /// Final review I2: the resources column's head — the repository's name,
    /// the gauges, the size explanation — took every line it wanted and left
    /// the list the rest, down to none. At 80×12 `A` ticked rows no frame
    /// showed and `d` built a plan of them: ruling A's defect, by height.
    ///
    /// Swept over every height from 3 to 40 at several widths, with and
    /// without a progress row, each key pressed on a fresh app after a real
    /// frame, read off the resources column's real rect: `espace`, `A`, `V`,
    /// `f`, `s` and `d` act exactly when a row of the list is on screen —
    /// never without one, and not refused while one shows — and a column
    /// with no row says the window is too short. Every failing size is
    /// collected, so the output names each one.
    ///
    /// Under both the size sort, a one-line cache under the cursor, and the
    /// name sort, a package version whose class takes its item's second line:
    /// ratatui draws nothing of a row taller than the list's area, so one
    /// line left for the list is not a row on screen.
    #[test]
    fn list_keys_act_only_while_a_row_of_the_resource_list_is_on_screen() {
        use std::collections::BTreeMap;
        let mut failures: BTreeMap<(String, u16, bool, String), Vec<u16>> = BTreeMap::new();
        let rows_shown = |column: &str| column.contains("[ ]") || column.contains("[x]");

        for (bar, sort) in [
            (false, SortKey::Size),
            (true, SortKey::Size),
            (false, SortKey::Name),
            (true, SortKey::Name),
        ] {
            for width in [60u16, 78, 80, 100, 120, 160, 200] {
                for height in 3u16..=40 {
                    let fresh = |tick: bool| {
                        let mut app = app_with_a_tall_resources_head();
                        app.sort = sort;
                        if bar {
                            app.purge = Some(views::progress::Work::new("suppression", 4));
                        }
                        if tick {
                            app.selected.insert((ResourceKind::Cache, 1));
                        }
                        let buf = views::testing::draw(&mut app, width, height);
                        let (_, columns) = views::testing::layout(&app, width, height);
                        let rect = columns.resources.expect("the focused column is on screen");
                        let column = views::testing::text_in(&buf, rect);
                        (app, rect, column)
                    };
                    let mut fail = |what: &str| {
                        failures
                            .entry((what.to_string(), width, bar, format!("{sort:?}")))
                            .or_default()
                            .push(height);
                    };

                    let (_, rect, column) = fresh(false);
                    let shown = rows_shown(&column);
                    if !shown && rect.height >= 1 && !column.contains("fenêtre trop basse") {
                        fail("no row, and no stand-in saying the window is too short");
                    }

                    for key in [' ', 'A', 'V', 'f', 's'] {
                        let (mut app, _, _) = fresh(false);
                        column_action(&mut app, KeyCode::Char(key));
                        let acted = match key {
                            'f' => app.filter_mode,
                            's' => app.sort != sort,
                            _ => !app.selected.is_empty(),
                        };
                        if acted && !shown {
                            fail(&format!("[{key}] acted with no row on screen"));
                        }
                        if !acted && shown {
                            fail(&format!("[{key}] refused while a row is on screen"));
                        }
                    }

                    let (app, _, column) = fresh(true);
                    let planned = plan_for_d(&app).is_some();
                    let shown = rows_shown(&column);
                    if planned && !shown {
                        fail("[d] built a plan with no row on screen");
                    }
                    if !planned && shown {
                        fail("[d] refused while a row is on screen");
                    }
                }
            }
        }

        assert!(
            failures.is_empty(),
            "{}",
            failures
                .iter()
                .map(|((what, width, bar, sort), heights)| format!(
                    "{what} — width {width}, sort {sort}{}, heights {heights:?}",
                    if *bar { ", progress row" } else { "" }
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// Pre-flight 4.14, final review I2 in the repos column: with too few
    /// lines to draw one repository row — none inside the column's borders,
    /// or one under a two-line item — `espace` ticked, and `d` planned the
    /// archive of, a repository no frame showed.
    ///
    /// Swept over every height from 5 to 30 at 60, 80 and 100 columns, each
    /// key pressed on a fresh app after a real frame, read off the repos
    /// column's real rect, the repository holding storage (a two-line item
    /// wherever the column has room for one): `espace` and `d` act exactly
    /// when the repository's row is on screen — never without it, and not
    /// refused while it shows. The sweep has to meet a height with no row,
    /// or it proves nothing.
    #[test]
    fn repo_keys_act_only_while_a_repository_row_is_on_screen() {
        const ORG: &str = "exec-d";
        const REPO: &str = "vitrine";
        let mut failures = Vec::new();
        let mut hidden = 0;
        for width in [60u16, 80, 100] {
            for height in 5u16..=30 {
                let fresh = |tick: bool| {
                    let mut app = App::new(vec![crate::model::OrgSummary {
                        login: ORG.into(),
                        repos: vec![crate::model::RepoSummary {
                            name: REPO.into(),
                            cache_bytes: 1_000,
                            cache_count: 1,
                            private: true,
                            age_days: 400,
                            class: crate::repos::RepoClass::Archivable,
                        }],
                        billing: Some(crate::billing::BillingReport {
                            items: vec![crate::billing::UsageItem {
                                month: "2026-09".into(),
                                product: "actions".into(),
                                sku: "Actions storage".into(),
                                quantity: 359.88,
                                unit_type: "GigabyteHours".into(),
                                gross: 0.0,
                                discount: 0.0,
                                net: 0.0,
                                repo: REPO.into(),
                            }],
                        }),
                        ..Default::default()
                    }]);
                    app.focus = Focus::Repos;
                    if tick {
                        app.selected_repo = Some((ORG.into(), REPO.into()));
                    }
                    let buf = views::testing::draw(&mut app, width, height);
                    let (_, columns) = views::testing::layout(&app, width, height);
                    let rect = columns.repos.expect("the focused column is on screen");
                    let shown = views::testing::text_in(&buf, rect).contains(REPO);
                    (app, shown)
                };
                let at = format!("{width}x{height}");

                let (mut app, shown) = fresh(false);
                if !shown {
                    hidden += 1;
                }
                column_action(&mut app, KeyCode::Char(' '));
                let ticked = app.selected_repo.is_some();
                if ticked && !shown {
                    failures.push(format!(
                        "[espace] ticked a repository no frame showed at {at}"
                    ));
                }
                if !ticked && shown {
                    failures.push(format!(
                        "[espace] refused while the row is on screen at {at}"
                    ));
                }

                let (app, shown) = fresh(true);
                let planned = plan_for_d(&app).is_some();
                if planned && !shown {
                    failures.push(format!("[d] planned an archive no frame showed at {at}"));
                }
                if !planned && shown {
                    failures.push(format!("[d] refused while the row is on screen at {at}"));
                }
            }
        }
        assert!(
            hidden > 0,
            "no swept height hid the row: this sweep proves nothing"
        );
        assert!(failures.is_empty(), "{}", failures.join("\n"));
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
            ..Default::default()
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

    /// `(org, repo)` in `systm-d` — a listing's key, and a purge's tag.
    fn key(repo: &str) -> (String, String) {
        ("systm-d".into(), repo.into())
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn cache_row(id: u64) -> crate::model::Resource {
        crate::model::Resource {
            kind: ResourceKind::Cache,
            id,
            label: format!("cache-{id}"),
            size_bytes: 100,
            age_days: 3,
            git_ref: None,
            stale_pr: false,
            protected: false,
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        }
    }

    fn plan(repo: &str, items: Vec<crate::model::Resource>) -> Plan {
        Plan {
            items,
            owner: "systm-d".into(),
            repo: repo.into(),
        }
    }

    fn done(id: u64, repo: &str) -> Progress {
        Progress::Done {
            kind: ResourceKind::Cache,
            id,
            owner: "systm-d".into(),
            repo: repo.into(),
        }
    }

    fn failed(id: u64, repo: &str) -> Progress {
        Progress::Failed {
            kind: ResourceKind::Cache,
            id,
            reason: "timeout".into(),
            owner: "systm-d".into(),
            repo: repo.into(),
        }
    }

    fn finished() -> Progress {
        Progress::Finished {
            freed: 0,
            failures: 0,
            deleted: 0,
            deleted_sizeless: 0,
            archived_repo: None,
        }
    }

    /// Ruling F3 (a): a purge starts no reload. Josephine's three caches are
    /// listed and purged; after each deletion the user steps to claudine and
    /// back to wait on josephine, well past the pause. Not one load starts,
    /// and the deleted rows leave one by one — from the list on screen, and
    /// from the kept listing the cursor comes back to.
    #[test]
    fn a_purge_updates_its_repositorys_listing_row_by_row_without_a_single_load() {
        let mut app = app_with_two_repositories();
        let rows: Vec<_> = (1..=3).map(cache_row).collect();
        app.remember(key("josephine"), rows.clone());
        app.remember(key("claudine"), vec![]);
        let mut now = Instant::now();
        assert!(app.follow_cursor(now).is_none());
        app.purge_launched(&plan("josephine", rows));
        let purge = key("josephine");

        for id in 1..=3u64 {
            let left = 3 - usize::try_from(id).unwrap();
            apply_progress(&mut app, &purge, done(id, "josephine"));
            assert_eq!(
                app.resources.len(),
                left,
                "the list on screen kept deleted row {id}"
            );

            app.repo_cursor = 1;
            now += ms(10);
            let away = app.follow_cursor(now);
            app.repo_cursor = 0;
            now += ms(10);
            let back = app.follow_cursor(now);
            now += ms(400);
            let waited = app.follow_cursor(now);
            assert!(
                away.is_none() && back.is_none() && waited.is_none(),
                "a load started while josephine's purge runs, after deleting row {id}"
            );
            assert_eq!(
                app.shown(),
                app::Shown::Listing {
                    org: "systm-d".into(),
                    repo: "josephine".into()
                },
                "josephine is not listed when the cursor comes back after row {id}"
            );
            assert_eq!(
                app.resources.len(),
                left,
                "the kept listing still holds deleted row {id}"
            );
        }
    }

    /// Ruling F3 (b): when the purge finishes, the repository its plan
    /// concerned is forgotten — even though the cursor moved to claudine
    /// mid-purge, and it is claudine's listing that stays. Back on
    /// josephine, one load, after the pause.
    #[test]
    fn a_finished_purge_forgets_its_own_repository_even_after_the_cursor_left() {
        let mut app = app_with_two_repositories();
        let rows: Vec<_> = (1..=2).map(cache_row).collect();
        app.remember(key("josephine"), rows.clone());
        app.remember(key("claudine"), vec![cache_row(9)]);
        let t0 = Instant::now();
        assert!(app.follow_cursor(t0).is_none());
        app.purge_launched(&plan("josephine", rows));
        let purge = key("josephine");

        app.repo_cursor = 1;
        assert!(app.follow_cursor(t0 + ms(10)).is_none());
        apply_progress(&mut app, &purge, done(1, "josephine"));
        apply_progress(&mut app, &purge, done(2, "josephine"));
        apply_progress(&mut app, &purge, finished());

        assert!(
            app.cached(("systm-d", "josephine")).is_none(),
            "the finished purge's repository kept its listing"
        );
        assert!(
            app.cached(("systm-d", "claudine")).is_some(),
            "the finished purge forgot the cursor's repository"
        );

        app.repo_cursor = 0;
        let back = t0 + ms(1000);
        assert!(app.follow_cursor(back).is_none(), "josephine waits a pause");
        let load = app
            .follow_cursor(back + app::LOAD_PAUSE)
            .expect("one load when the cursor rests on josephine again");
        assert_eq!(load.repo, "josephine");
        assert!(
            app.follow_cursor(back + ms(3000)).is_none(),
            "a second load started"
        );
    }

    /// An archive is a purge too: when it finishes, the archived repository
    /// is forgotten — not the one under the cursor.
    #[test]
    fn a_finished_archive_forgets_the_archived_repository_not_the_cursors() {
        let mut app = app_with_two_repositories();
        app.remember(key("josephine"), vec![]);
        app.remember(key("claudine"), vec![]);
        assert!(app.follow_cursor(Instant::now()).is_none());
        let archive = app.orgs[0].repos[1].clone();
        let item = crate::model::Resource {
            kind: ResourceKind::Repository,
            id: crate::api::refs::resource_id(&archive.name),
            label: archive.name.clone(),
            ..cache_row(0)
        };
        app.purge_launched(&plan("claudine", vec![item.clone()]));
        let purge = key("claudine");

        apply_progress(
            &mut app,
            &purge,
            Progress::Done {
                kind: item.kind,
                id: item.id,
                owner: "systm-d".into(),
                repo: "claudine".into(),
            },
        );
        apply_progress(
            &mut app,
            &purge,
            Progress::Finished {
                freed: 0,
                failures: 0,
                deleted: 1,
                deleted_sizeless: 1,
                archived_repo: Some("claudine".into()),
            },
        );

        assert!(app.cached(("systm-d", "claudine")).is_none());
        assert!(app.cached(("systm-d", "josephine")).is_some());
    }

    /// Ruling F3 (c): a refused deletion leaves its row where it is — on
    /// screen and in the kept listing: the resource still exists. A deleted
    /// one beside it is the positive control.
    #[test]
    fn a_failed_item_stays_listed_and_kept() {
        let mut app = app_with_two_repositories();
        let rows: Vec<_> = (1..=2).map(cache_row).collect();
        app.remember(key("josephine"), rows.clone());
        assert!(app.follow_cursor(Instant::now()).is_none());
        app.purge_launched(&plan("josephine", rows));
        let purge = key("josephine");

        apply_progress(&mut app, &purge, done(1, "josephine"));
        apply_progress(&mut app, &purge, failed(2, "josephine"));

        let ids = |rows: &[crate::model::Resource]| rows.iter().map(|r| r.id).collect::<Vec<_>>();
        assert_eq!(ids(app.resources.as_slice()), vec![2]);
        assert_eq!(
            ids(app.cached(("systm-d", "josephine")).expect("kept")),
            vec![2]
        );
    }

    /// No load starts on its own for a repository while a purge concerning
    /// it runs: its listing would be read mid-deletion. Two purges of
    /// josephine; the first finishes, forgetting her listing, while the
    /// second still runs — the cursor waits on her, well past the pause, and
    /// nothing is requested until the second one finishes too.
    #[test]
    fn no_load_starts_on_its_own_for_a_repository_while_a_purge_concerning_it_runs() {
        let mut app = app_with_two_repositories();
        app.remember(key("josephine"), vec![cache_row(1), cache_row(2)]);
        app.remember(key("claudine"), vec![]);
        let t0 = Instant::now();
        assert!(app.follow_cursor(t0).is_none());
        app.purge_launched(&plan("josephine", vec![cache_row(1)]));
        app.purge_launched(&plan("josephine", vec![cache_row(2)]));
        let purge = key("josephine");

        app.repo_cursor = 1;
        assert!(app.follow_cursor(t0 + ms(10)).is_none());
        apply_progress(&mut app, &purge, done(1, "josephine"));
        apply_progress(&mut app, &purge, finished());
        assert!(app.cached(("systm-d", "josephine")).is_none());

        app.repo_cursor = 0;
        let back = t0 + ms(100);
        for wait in [0, 300, 1000, 5000] {
            assert!(
                app.follow_cursor(back + ms(wait)).is_none(),
                "a load started {wait} ms into the wait while josephine's second purge runs"
            );
        }

        apply_progress(&mut app, &purge, done(2, "josephine"));
        apply_progress(&mut app, &purge, finished());
        let load = app
            .follow_cursor(back + ms(5100))
            .expect("the load starts once no purge of josephine runs");
        assert_eq!(load.repo, "josephine");
    }

    /// The in-flight half: a purge of claudine, down to its `Finished`,
    /// leaves josephine's listing — in flight under the cursor — to land and
    /// show.
    #[test]
    fn a_purge_elsewhere_leaves_the_cursors_listing_in_flight_alone() {
        let mut app = app_with_two_repositories();
        assert!(app.follow_cursor(Instant::now()).is_none());
        let load = app.force_load().expect("josephine's load starts");
        app.purge_launched(&plan("claudine", vec![cache_row(1)]));
        let purge = key("claudine");

        apply_progress(&mut app, &purge, done(1, "claudine"));
        apply_progress(&mut app, &purge, finished());
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

        apply_progress(&mut app, &key("claudine"), outcome(false, "claudine"));
        apply_progress(&mut app, &key("claudine"), outcome(true, "claudine"));
        assert_eq!(
            app.resources.len(),
            1,
            "claudine's deletion removed josephine's row"
        );
        assert!(
            app.selected.contains(&tick),
            "claudine's purge unticked josephine's row"
        );

        apply_progress(&mut app, &key("josephine"), outcome(true, "josephine"));
        assert!(app.resources.is_empty());
        assert!(app.selected.is_empty());
    }

    /// The item an archive plan of `repo` holds: the repository itself.
    fn repository_item(repo: &str) -> crate::model::Resource {
        crate::model::Resource {
            kind: ResourceKind::Repository,
            id: crate::api::refs::resource_id(repo),
            label: repo.into(),
            ..cache_row(0)
        }
    }

    /// The purge bar, as `(label, done, total)`.
    fn bar(app: &App) -> Option<(&str, usize, usize)> {
        app.purge
            .as_ref()
            .map(|w| (w.label.as_str(), w.done, w.total))
    }

    /// Several purges run at once — the cursor stays free during one, and
    /// `d` works again — and they share one bar that counts them all. The
    /// second of josephine's two purges is confirmed while the first is
    /// still deleting: a second purge overwriting the slot would read 0/4
    /// where 2/7 is true, and the first one's `Finished` clearing it would
    /// announce the end of four deletions still to come.
    #[test]
    fn purge_bar_counts_every_purge_in_flight_and_leaves_with_the_last() {
        let mut app = app_with_two_repositories();
        let purge = key("josephine");
        app.purge_launched(&plan("josephine", (1..=3).map(cache_row).collect()));
        apply_progress(&mut app, &purge, done(1, "josephine"));
        apply_progress(&mut app, &purge, done(2, "josephine"));
        assert_eq!(bar(&app), Some(("suppression", 2, 3)));

        app.purge_launched(&plan("josephine", (4..=7).map(cache_row).collect()));
        assert_eq!(
            bar(&app),
            Some(("suppression", 2, 7)),
            "the second purge overwrote the first one's count"
        );

        apply_progress(&mut app, &purge, done(3, "josephine"));
        apply_progress(&mut app, &purge, finished());
        assert_eq!(
            bar(&app),
            Some(("suppression", 3, 7)),
            "the first purge's end ended the bar with four deletions still running"
        );

        for id in 4..=7 {
            apply_progress(&mut app, &purge, done(id, "josephine"));
        }
        assert_eq!(bar(&app), Some(("suppression", 7, 7)));
        apply_progress(&mut app, &purge, finished());
        assert_eq!(bar(&app), None, "the bar outlived the last purge");
    }

    /// A refused deletion is an item processed, not one still waiting: with
    /// `Done` alone counted, this bar would stop at 1/3 with nothing left to
    /// do.
    #[test]
    fn purge_bar_counts_a_refused_item_as_processed() {
        let mut app = app_with_two_repositories();
        let purge = key("josephine");
        app.purge_launched(&plan("josephine", (1..=3).map(cache_row).collect()));

        apply_progress(&mut app, &purge, done(1, "josephine"));
        apply_progress(&mut app, &purge, failed(2, "josephine"));
        apply_progress(&mut app, &purge, failed(3, "josephine"));

        assert_eq!(bar(&app), Some(("suppression", 3, 3)));
    }

    /// An archive is never called a deletion — on the bar no more than in
    /// its confirmation modal or its recap.
    #[test]
    fn purge_bar_calls_an_archive_archivage() {
        let mut app = app_with_two_repositories();
        app.purge_launched(&plan("claudine", vec![repository_item("claudine")]));
        assert_eq!(bar(&app), Some(("archivage", 0, 1)));
    }

    /// A deletion and an archive running together share the bar, and its
    /// label names both: its counts hold the archive's item as well as the
    /// deletions', so `suppression` alone would count an archive as a
    /// deletion. The archive is refused — its `Failed` counts, from the
    /// archive branch of the message — and its end leaves the deletion's bar
    /// running, still named for both since its count still holds the
    /// archive.
    #[test]
    fn purge_bar_names_both_when_a_deletion_and_an_archive_run_together() {
        let mut app = app_with_two_repositories();
        let deletion = key("josephine");
        let archive = key("claudine");
        let item = repository_item("claudine");
        app.purge_launched(&plan("josephine", (1..=2).map(cache_row).collect()));
        app.purge_launched(&plan("claudine", vec![item.clone()]));
        assert_eq!(bar(&app), Some(("suppression et archivage", 0, 3)));

        apply_progress(
            &mut app,
            &archive,
            Progress::Failed {
                kind: item.kind,
                id: item.id,
                reason: "403".into(),
                owner: "systm-d".into(),
                repo: "claudine".into(),
            },
        );
        apply_progress(
            &mut app,
            &archive,
            Progress::Finished {
                freed: 0,
                failures: 1,
                deleted: 0,
                deleted_sizeless: 0,
                archived_repo: Some("claudine".into()),
            },
        );
        assert_eq!(bar(&app), Some(("suppression et archivage", 1, 3)));

        apply_progress(&mut app, &deletion, done(1, "josephine"));
        apply_progress(&mut app, &deletion, done(2, "josephine"));
        apply_progress(&mut app, &deletion, finished());
        assert_eq!(bar(&app), None);
    }

    /// A plan with no item sets no bar: it ends before it could be drawn,
    /// and a bar at 0 % that vanishes at once is a flicker, not news. Nor
    /// does one joining a running bar change its label or its count.
    #[test]
    fn purge_bar_is_not_set_by_an_empty_plan() {
        let mut app = app_with_two_repositories();
        app.purge_launched(&plan("josephine", vec![]));
        assert_eq!(bar(&app), None, "an empty plan set a bar");
        apply_progress(&mut app, &key("josephine"), finished());

        app.purge_launched(&plan("claudine", vec![repository_item("claudine")]));
        app.purge_launched(&plan("josephine", vec![]));
        assert_eq!(
            bar(&app),
            Some(("archivage", 0, 1)),
            "an empty plan changed the running bar"
        );
    }

    /// The event loop's half of a load's bar: `spawn_load` forwards every
    /// tick of its run under its load's generation — the one
    /// `App::load_ticked` accepts — and all of them before the listing
    /// lands, so no tick of a load trails its own landing. Against a server
    /// that knows no route: every call still completes, and ticks.
    #[tokio::test]
    async fn a_spawned_load_forwards_every_tick_under_its_generation_before_it_lands() {
        let server = wiremock::MockServer::start().await;
        let client = Arc::new(Client::with_base("t0ken", &server.uri()).unwrap());
        let mut app = app_with_two_repositories();
        assert!(app.follow_cursor(Instant::now()).is_none());
        let load = app.force_load().expect("josephine's load starts");
        let (landings_tx, mut landings) = mpsc::unbounded_channel::<Landing>();
        let (ticks_tx, mut ticks) = mpsc::unbounded_channel();

        spawn_load(&client, &landings_tx, &ticks_tx, load);
        let (load, outcome) = tokio::time::timeout(Duration::from_secs(10), landings.recv())
            .await
            .expect("the listing never landed")
            .expect("the landings channel closed");

        let mut forwarded = 0;
        while let Ok(generation) = ticks.try_recv() {
            app.load_ticked(generation);
            forwarded += 1;
        }
        assert_eq!(
            forwarded,
            scan::TOTAL_CALLS,
            "ticks forwarded before the landing"
        );
        assert_eq!(
            app.loading.as_ref().map(|w| w.done),
            Some(scan::TOTAL_CALLS),
            "the forwarded ticks are not the load's own"
        );
        app.land_load(load, outcome);
        assert_eq!(app.loading, None);
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

    /// The Billing tab opens on the report's newest month (pre-flight 5.1),
    /// so `←` pages back to older months and stops at the oldest, and `→`
    /// comes forward again and stops at the newest. With the arrows the
    /// 0.5.0 way round, `←` would be dead on opening and `→` would leave the
    /// current month. Read off the Billing panel's real rect, three months.
    #[test]
    fn billing_left_pages_to_older_months_and_right_back_to_newer() {
        let minutes = |month: &str| crate::billing::UsageItem {
            month: month.into(),
            product: "actions".into(),
            sku: "Actions Linux".into(),
            quantity: 10.0,
            unit_type: "Minutes".into(),
            gross: 0.06,
            discount: 0.06,
            net: 0.0,
            repo: "disconnected".into(),
        };
        let mut app = App::new(vec![OrgSummary {
            login: "exec-d".into(),
            billing: Some(crate::billing::BillingReport {
                items: vec![minutes("2026-09"), minutes("2026-07"), minutes("2026-08")],
            }),
            ..Default::default()
        }]);
        app.view = View::Billing;
        let shown = |app: &mut App| {
            let buf = views::testing::draw(app, 100, 30);
            let (rows, _) = views::testing::layout(app, 100, 30);
            let body = views::testing::text_in(&buf, rows.body);
            ["2026-07", "2026-08", "2026-09"]
                .into_iter()
                .filter(|month| body.contains(&format!("{month} ·")))
                .collect::<Vec<_>>()
        };

        assert_eq!(shown(&mut app), ["2026-09"], "on opening");
        let mut pressed = Vec::new();
        for (key, expected) in [
            (KeyCode::Left, "2026-08"),
            (KeyCode::Left, "2026-07"),
            (KeyCode::Left, "2026-07"),
            (KeyCode::Right, "2026-08"),
            (KeyCode::Right, "2026-09"),
            (KeyCode::Right, "2026-09"),
        ] {
            page_months(&mut app, key);
            pressed.push(key);
            assert_eq!(shown(&mut app), [expected], "after {pressed:?}");
        }
    }

    /// Final review minor, promoted (F-M2): `↓` on the last org and `↑` on
    /// the first moved no org, yet reset the cursors scoped beneath it and
    /// dropped the repository tick — ruling R7-1 lets only an org move clear
    /// it. A second org, exec-d, makes both a still key and a real move
    /// possible: `↑` on systm-d keeps its tick and month, `↓` to exec-d
    /// clears them (the positive control), and `↓` on exec-d, the last org,
    /// keeps exec-d's own tick and month.
    #[test]
    fn an_org_key_that_moves_no_org_keeps_the_repository_tick() {
        let mut app = app_with_loaded_resources();
        app.orgs.push(OrgSummary {
            login: "exec-d".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![crate::model::RepoSummary {
                name: "lokiprint".into(),
                cache_bytes: 0,
                cache_count: 0,
                private: false,
                age_days: 5,
                class: crate::repos::RepoClass::Archivable,
            }],
            billing: None,
            ..Default::default()
        });
        let tick = |app: &mut App, month: usize| {
            app.focus = Focus::Repos;
            column_action(app, KeyCode::Char(' '));
            app.focus = Focus::Orgs;
            app.month_cursor = month;
        };
        let state = |app: &App| (app.org_cursor, app.selected_repo.clone(), app.month_cursor);
        let repo = |org: &str, repo: &str| Some((org.to_string(), repo.to_string()));

        tick(&mut app, 2);
        move_cursor(&mut app, KeyCode::Up);
        assert_eq!(
            state(&app),
            (0, repo("systm-d", "josephine"), 2),
            "↑ on the first org dropped what belongs to the org it stayed on"
        );

        move_cursor(&mut app, KeyCode::Down);
        assert_eq!(
            state(&app),
            (1, None, 0),
            "a real org move must clear the tick and the month"
        );

        tick(&mut app, 1);
        move_cursor(&mut app, KeyCode::Down);
        assert_eq!(
            state(&app),
            (1, repo("exec-d", "lokiprint"), 1),
            "↓ on the last org dropped what belongs to the org it stayed on"
        );
    }
}
