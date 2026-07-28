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

    // A filter left in place must stay visible even once the user stops
    // typing — otherwise it silently keeps hiding rows with no indication
    // why. The cursor mark (▏) only appears while actively typing.
    let status = if !app.filter.is_empty() {
        if app.filter_mode {
            format!(" filtre : {}▏", app.filter)
        } else {
            format!(" filtre : {}", app.filter)
        }
    } else {
        app.status.clone()
    };
    f.render_widget(
        Paragraph::new(Span::styled(status, theme::muted())),
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
