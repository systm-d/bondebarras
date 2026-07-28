//! Terminal user interface.

pub mod app;
pub mod theme;
pub mod views;

use crate::api::{Client, caches};
use crate::clean::{self, Plan, Progress};
use crate::model::{OrgSummary, human_size};
use crate::scan;
use anyhow::Result;
use app::{App, Focus};
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
                Progress::Done { kind, id } => {
                    app.resources.retain(|r| !(r.kind == kind && r.id == id));
                    app.selected.remove(&(kind, id));
                }
                Progress::Failed { kind, id, reason } => {
                    app.selected.remove(&(kind, id));
                    app.status = format!("Erreur : suppression de {id} — {reason}");
                }
                Progress::Finished { freed, failures } => {
                    app.status = if failures == 0 {
                        format!("Bon débarras ! {} libérés.", human_size(freed))
                    } else {
                        format!(
                            "Bon débarras ! {} libérés, {failures} échec(s).",
                            human_size(freed)
                        )
                    };

                    // The recap above talks about bytes freed; the left pane
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
                app.status = format!("Suppression de {} …", plan.summary());
                // Captured now, not read from `loaded` when `Finished` lands:
                // the user can navigate to a different org while this runs.
                app.purging_org = Some(plan.owner.clone());
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

        match key.code {
            KeyCode::Esc if !app.filter.is_empty() => {
                app.filter.clear();
                app.res_cursor = 0;
            }
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            KeyCode::Tab => {
                app.focus = match app.focus {
                    Focus::Orgs => Focus::Repos,
                    Focus::Repos => Focus::Resources,
                    Focus::Resources => Focus::Orgs,
                }
            }
            KeyCode::Down => match app.focus {
                Focus::Orgs => {
                    app.org_cursor = (app.org_cursor + 1).min(app.orgs.len().saturating_sub(1));
                    app.repo_cursor = 0;
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
                    app.repo_cursor = 0;
                }
                Focus::Repos => app.repo_cursor = app.repo_cursor.saturating_sub(1),
                Focus::Resources => app.res_cursor = app.res_cursor.saturating_sub(1),
            },
            KeyCode::Enter => {
                // Stage 2: load the selected repository on demand. The target
                // is cloned out first — holding a borrow on `app.orgs` while
                // assigning `app.status` would not compile.
                if let Some((org, repo)) = app.current_target() {
                    app.status = format!("Chargement de {org}/{repo} …");
                    match scan::repo_detail(&client, &org, &repo).await {
                        Ok(items) => {
                            app.resources = items;
                            app.res_cursor = 0;
                            app.selected.clear();
                            // A filter typed for the previous repository must
                            // not silently keep hiding rows in this one.
                            app.filter.clear();
                            app.filter_mode = false;
                            app.loaded = Some((org, repo));
                            app.focus = Focus::Resources;
                            app.status.clear();
                        }
                        Err(e) => app.status = format!("Erreur : {e}"),
                    }
                }
            }
            KeyCode::Char(' ') => app.toggle_selected(),
            KeyCode::Char('s') => app.cycle_sort(),
            KeyCode::Char('A') => app.select_all_stale(),
            KeyCode::Char('d') => {
                if let Some(plan) = app.take_plan()
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
