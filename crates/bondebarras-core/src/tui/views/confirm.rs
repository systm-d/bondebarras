//! Tier-1 confirmation modal.
//!
//! v0.1 only deletes regenerable resources, so a single [y/N] is the right
//! amount of friction. Tiers 2 and 3 land with packages and repositories.

use crate::clean::Plan;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// A centred box, sized as a percentage of the frame.
fn centered(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(v[1])[1]
}

pub fn render(plan: &Plan, f: &mut Frame, area: Rect) {
    let zone = centered(60, 22, area);
    f.render_widget(Clear, zone);

    let body = vec![
        Line::from(Span::styled(plan.summary(), theme::text_style())),
        Line::from(Span::styled(
            format!("{}/{}", plan.owner, plan.repo),
            theme::muted(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Ces éléments sont régénérables par un re-run.",
            theme::muted(),
        )),
        Line::from(Span::styled("Supprimer ?   [y/N]", theme::title_style())),
    ];

    f.render_widget(
        Paragraph::new(body).block(
            Block::default()
                .title(" Confirmation ")
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        ),
        zone,
    );
}
