//! Terminal user interface.

pub mod app;
pub mod theme;
pub mod views;

use crate::api::{Client, caches};
use crate::clean::{self, Plan, Progress};
use crate::model::{OrgSummary, ResourceKind};
use crate::scan;
use anyhow::Result;
use app::{App, Focus, View};
use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::sync::Arc;
use std::time::Duration;
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

    while !app.should_quit {
        terminal.draw(|f| views::render(&mut app, f, pending.as_ref()))?;

        // Drain deletion progress without blocking the draw.
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Progress::Done {
                    kind,
                    id,
                    owner,
                    repo,
                } => {
                    if kind == ResourceKind::Repository {
                        // Not a deletion: the repository stays in the tree,
                        // now read-only. `owner`/`repo` name the archive
                        // this message is actually about — Findings 3 and 4
                        // of the final review: reading `app.selected_repo`
                        // here instead, as an earlier version did, updated
                        // whatever the tree currently had ticked, which a
                        // second archive started before this one landed
                        // could easily have replaced.
                        app.archive_done(&owner, &repo);
                    } else {
                        app.resources.retain(|r| !(r.kind == kind && r.id == id));
                        app.selected.remove(&(kind, id));
                    }
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
                        app.selected.remove(&(kind, id));
                        app.status = format!("Erreur : suppression de {id} — {reason}");
                    }
                }
                Progress::Finished {
                    freed,
                    failures,
                    deleted,
                    deleted_sizeless,
                    archived_repo,
                } => {
                    // One purge is done. `purge_finished` only disarms the
                    // quit guard once every in-flight purge has settled — a
                    // second purge started before this one landed must keep
                    // `q` guarded.
                    app.purge_finished();
                    // `archived_repo` comes straight off this message —
                    // `clean::execute` computed it from its own `Plan` — not
                    // from a shared `archiving_target` local the way an
                    // earlier version of this loop did: a second archive
                    // confirmed before this `Finished` landed could
                    // overwrite that local before this one ever read it. See
                    // `Progress::Finished`'s own doc comment (Findings 3 and
                    // 4 of the final review).
                    app.status = purge_finished_status(
                        archived_repo.as_deref(),
                        freed,
                        failures,
                        deleted,
                        deleted_sizeless,
                    );

                    // The recap above talks about bytes freed; the orgs column
                    // must agree on the same frame, not show the pre-purge
                    // total for the org the purge just happened in. One
                    // request is enough — a stale number would be worse than
                    // just leaving the old one if this fails.
                    //
                    // Reads `purging_org`, not `loaded`: the purge runs on a
                    // spawned task while this loop keeps handling keys, so the
                    // user can load a different repo — possibly in a
                    // different org — before `Finished` lands. `loaded` would
                    // then name the wrong org. `take()` both reads it and
                    // clears it, success or not.
                    if let Some(org_login) = app.purging_org.take()
                        && let Ok(repos) = caches::usage_by_repository(&client, &org_login).await
                    {
                        app.refresh_org_cache(&org_login, repos);
                    }
                }
            }
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
                // Stage 2: load the selected repository on demand. The target
                // is cloned out first — holding a borrow on `app.orgs` while
                // assigning `app.status` would not compile.
                //
                // Calls `repo_detail_with_warnings` directly, not the
                // `repo_detail` stderr wrapper: Finding 5 of the final
                // review needs the failed family names as data so
                // `App::finish_loading` can put them where the user is
                // actually looking, `app.status` — the wrapper's `eprintln!`
                // writes to a stream the alternate screen hides.
                if let Some((org, repo)) = app.current_target() {
                    app.status = format!("Chargement de {org}/{repo} …");
                    match scan::repo_detail_with_warnings(&client, &org, &repo).await {
                        Ok((items, failed)) => app.finish_loading(org, repo, items, failed),
                        Err(e) => app.status = format!("Erreur : {e}"),
                    }
                }
            }
            // The repos column (`Focus::Repos`) ticks a repository for
            // archiving; every other focus keeps ticking a resource, as
            // before. Two different guards, two different storage slots —
            // see `App::toggle_repo_selected`'s own doc comment for why a
            // repository cannot share `toggle_selected`'s.
            KeyCode::Char(' ') => match app.focus {
                Focus::Repos => app.toggle_repo_selected(),
                _ => app.toggle_selected(),
            },
            KeyCode::Char('s') => app.cycle_sort(),
            KeyCode::Char('A') => app.select_all_stale(),
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
            KeyCode::Char('f') => app.filter_mode = true,
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

#[cfg(test)]
mod tests {
    use super::*;

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
