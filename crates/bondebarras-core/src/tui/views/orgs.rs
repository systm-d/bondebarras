//! Left pane: organizations, biggest cache footprint first.

use crate::model::human_size;
use crate::tui::app::App;
use crate::tui::app::Focus;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

/// Renders the org/repo tree as a stateful list so ratatui scrolls to keep
/// the selection visible — without a persistent `ListState` it only draws
/// the rows that fit and the cursor walks off screen past that point.
pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    // Built first, from shared borrows only: the items own their strings
    // (`ListItem<'static>`), so the borrow of `app` ends here, before
    // `app.org_state` is borrowed mutably below.
    let mut items: Vec<ListItem<'static>> = Vec::new();

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
        items.push(ListItem::new(line));

        // The current org unfolds: its repos are the second level of the tree,
        // and the only way to reach anything but the biggest one.
        if is_current {
            for repo in &org.repos {
                let line = Line::from(vec![
                    Span::styled(format!("   {:<11}", repo.name), theme::text_style()),
                    Span::styled(
                        format!("{:>8}", human_size(repo.cache_bytes)),
                        theme::muted(),
                    ),
                ]);
                items.push(ListItem::new(line));
            }
        }
    }

    // Flattened index of the row the cursor is on: the org row itself while
    // focus is on the org level, otherwise the unfolded repo row at
    // `repo_cursor`. Focus on Resources reuses the same `repo_cursor` row
    // rather than clearing the selection (`None`) — a `List` with nothing
    // selected does not keep the viewport anchored anywhere, so it would
    // leave the tree scrolled wherever it last happened to be instead of
    // showing which repo the right pane belongs to.
    let flat_index = match app.focus {
        Focus::Orgs => app.org_cursor,
        Focus::Repos | Focus::Resources => app.org_cursor + 1 + app.repo_cursor,
    };
    app.org_state.select(if items.is_empty() {
        None
    } else {
        Some(flat_index.min(items.len() - 1))
    });

    let list = List::new(items)
        .block(
            Block::default()
                .title(" ORGS ")
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        )
        .highlight_style(theme::selection_style());

    f.render_stateful_widget(list, area, &mut app.org_state);
}
