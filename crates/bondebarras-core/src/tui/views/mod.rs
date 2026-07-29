//! Rendering. Layout mirrors claudine: header, body, status line, footer,
//! with modals drawn on top conditionally.

pub mod billing;
pub mod confirm;
pub mod orgs;
pub mod repo;

use crate::clean::Plan;
use crate::tui::app::{App, View};
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

const FOOTER_ORGS: &str =
    " [espace] cocher  [s] trier  [f] filtrer  [A] tout ⚑  [d] supprimer  [b] billing  [q] quitter";
/// No selection, no `d`, nothing destructive: the Billing tab is strictly
/// diagnostic, and its footer must not advertise a key it does not act on.
const FOOTER_BILLING: &str = " [←/→] mois  [b] orgs  [q] quitter";

pub fn render(app: &mut App, f: &mut Frame, pending: Option<&Plan>) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(f.area());

    let tab = match app.view {
        View::Orgs => "Orgs",
        View::Billing => "Billing",
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" bondebarras · {} orgs · {tab} ", app.orgs.len()),
            theme::title_style(),
        )),
        rows[0],
    );

    match app.view {
        View::Orgs => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(26), Constraint::Min(20)])
                .split(rows[1]);
            orgs::render(app, f, cols[0]);
            repo::render(app, f, cols[1]);
        }
        View::Billing => billing::render(app, f, rows[1]),
    }

    f.render_widget(
        Paragraph::new(Span::styled(status_line(app), theme::muted())),
        rows[2],
    );
    let footer = match app.view {
        View::Orgs => FOOTER_ORGS,
        View::Billing => FOOTER_BILLING,
    };
    f.render_widget(
        Paragraph::new(Span::styled(footer, theme::muted())),
        rows[3],
    );

    if let Some(plan) = pending {
        confirm::render(plan, f, f.area());
    }
}

/// What the status row shows.
///
/// Split out from `render` so it can be asserted on without a terminal, and
/// because the priority itself is the fix: `app.status` carries one-shot
/// messages — per-item deletion errors (`Erreur : suppression de {id} —
/// {reason}`) and the « Bon débarras ! » recap — that a filter left typed in
/// must not permanently bury, so it outranks a passive filter indicator.
///
/// But active typing outranks even that: `app.status` is only ever cleared
/// on a successful `Enter` load, so after any purge, cancel, or error it
/// stays set indefinitely. Giving it priority unconditionally made every
/// keystroke into the filter invisible — no text, no cursor — the moment any
/// status message was pending, even though the keystrokes were still
/// filtering the list underneath. `filter_mode` is checked first so the
/// field the user is looking at is always the one they are typing into.
fn status_line(app: &App) -> String {
    if app.filter_mode {
        format!(" filtre : {}▏", app.filter)
    } else if !app.status.is_empty() {
        app.status.clone()
    } else if !app.filter.is_empty() {
        format!(" filtre : {}", app.filter)
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks finding 3: a filter left typed in (but no longer being typed
    /// into) must not hide per-item deletion errors or the purge recap, both
    /// of which are written to `app.status`. On the old priority (filter
    /// first, status as fallback) this returns the filter string instead.
    #[test]
    fn status_takes_priority_over_a_passive_filter() {
        let mut app = App::new(vec![]);
        app.filter = "linux".into();
        app.status = "Erreur : suppression de 9 — 404".into();

        assert_eq!(status_line(&app), "Erreur : suppression de 9 — 404");
    }

    #[test]
    fn the_filter_shows_once_status_is_empty() {
        let mut app = App::new(vec![]);
        app.filter = "linux".into();

        assert_eq!(status_line(&app), " filtre : linux");
    }

    /// Locks the re-review's residual 1: `app.status` is only ever cleared on
    /// a successful `Enter` load, so after any purge, cancel, or error it
    /// stays set indefinitely. Giving it priority over the filter
    /// unconditionally made every keystroke typed into the filter invisible
    /// — no text, no cursor — while it was still filtering the list
    /// underneath. On the priority this locks in, active typing must win
    /// even over a pending status message.
    #[test]
    fn active_typing_outranks_a_pending_status() {
        let mut app = App::new(vec![]);
        app.status = "Bon débarras ! 3.0 Go libérés.".into();
        app.filter_mode = true;
        app.filter = "lin".into();

        assert_eq!(status_line(&app), " filtre : lin▏");
    }

    /// Both halves of the name are asserted here — not split across sibling
    /// tests — so an implementation that always appends the cursor mark
    /// fails on this test alone, rather than only via a neighbour.
    #[test]
    fn the_typing_cursor_only_appears_in_filter_mode() {
        let mut app = App::new(vec![]);
        app.filter = "linux".into();

        app.filter_mode = true;
        assert_eq!(status_line(&app), " filtre : linux▏");

        app.filter_mode = false;
        assert_eq!(status_line(&app), " filtre : linux");
    }
}
