//! Left pane: organizations, biggest cache footprint first.

use crate::model::human_size;
use crate::tui::app::App;
use crate::tui::app::Focus;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

pub fn render(app: &App, f: &mut Frame, area: Rect) {
    let mut items: Vec<ListItem> = Vec::new();

    for (i, org) in app.orgs.iter().enumerate() {
        let is_current = i == app.org_cursor;
        let line = Line::from(vec![
            Span::styled(
                format!("{} {:<12}", if is_current { "▾" } else { "▸" }, org.login),
                theme::text_style(),
            ),
            Span::styled(
                format!("{:>8}", human_size(org.cache_bytes)),
                theme::muted(),
            ),
        ]);
        items.push(if is_current && app.focus == Focus::Orgs {
            ListItem::new(line).style(theme::selection_style())
        } else {
            ListItem::new(line)
        });

        // The current org unfolds: its repos are the second level of the tree,
        // and the only way to reach anything but the biggest one.
        if is_current {
            for (j, repo) in org.repos.iter().enumerate() {
                let line = Line::from(vec![
                    Span::styled(format!("   {:<11}", repo.name), theme::text_style()),
                    Span::styled(
                        format!("{:>8}", human_size(repo.cache_bytes)),
                        theme::muted(),
                    ),
                ]);
                items.push(if j == app.repo_cursor && app.focus == Focus::Repos {
                    ListItem::new(line).style(theme::selection_style())
                } else {
                    ListItem::new(line)
                });
            }
        }
    }

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
