//! Rendering. Layout mirrors claudine: header, body, status line, footer,
//! with modals drawn on top conditionally.

pub mod confirm;
pub mod orgs;
pub mod repo;

use crate::clean::Plan;
use crate::tui::app::App;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;

const FOOTER: &str =
    " [espace] cocher  [s] trier  [f] filtrer  [A] tout ⚑  [d] supprimer  [q] quitter";

pub fn render(app: &App, f: &mut Frame, pending: Option<&Plan>) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(f.area());

    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" bondebarras · {} orgs ", app.orgs.len()),
            theme::title_style(),
        )),
        rows[0],
    );

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(26), Constraint::Min(20)])
        .split(rows[1]);
    orgs::render(app, f, cols[0]);
    repo::render(app, f, cols[1]);

    let status = if app.filter_mode {
        format!(" filtre : {}▏", app.filter)
    } else {
        app.status.clone()
    };
    f.render_widget(
        Paragraph::new(Span::styled(status, theme::muted())),
        rows[2],
    );
    f.render_widget(
        Paragraph::new(Span::styled(FOOTER, theme::muted())),
        rows[3],
    );

    if let Some(plan) = pending {
        confirm::render(plan, f, f.area());
    }
}
