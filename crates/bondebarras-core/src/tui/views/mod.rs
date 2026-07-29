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
/// must not permanently bury. The filter is shown only once there is nothing
/// more urgent to say. The cursor mark (▏) only appears while actively
/// typing.
fn status_line(app: &App) -> String {
    if !app.status.is_empty() {
        app.status.clone()
    } else if !app.filter.is_empty() {
        if app.filter_mode {
            format!(" filtre : {}▏", app.filter)
        } else {
            format!(" filtre : {}", app.filter)
        }
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks finding 3: a filter left typed in must not hide per-item
    /// deletion errors or the purge recap, both of which are written to
    /// `app.status`. On the old priority (filter first, status as fallback)
    /// this returns the filter string instead.
    #[test]
    fn status_takes_priority_over_an_active_filter() {
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

    #[test]
    fn the_typing_cursor_only_appears_in_filter_mode() {
        let mut app = App::new(vec![]);
        app.filter = "linux".into();
        app.filter_mode = true;

        assert_eq!(status_line(&app), " filtre : linux▏");
    }
}
