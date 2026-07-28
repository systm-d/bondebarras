//! Left pane: organizations, biggest cache footprint first.

use crate::model::human_size;
use crate::tui::app::App;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

pub fn render(app: &App, f: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app
        .orgs
        .iter()
        .enumerate()
        .map(|(i, org)| {
            let line = Line::from(vec![
                Span::styled(format!("{:<14}", org.login), theme::text_style()),
                Span::styled(
                    format!("{:>8}", human_size(org.cache_bytes)),
                    theme::muted(),
                ),
            ]);
            if i == app.org_cursor {
                ListItem::new(line).style(theme::selection_style())
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    f.render_widget(
        List::new(items).block(
            Block::default()
                .title(" ORGS ")
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        ),
        area,
    );
}
