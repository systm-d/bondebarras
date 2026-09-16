//! Column 1: organizations, biggest cache footprint first.

use crate::model::human_size;
use crate::tui::app::{App, Focus};
use crate::tui::theme;
use crate::tui::views;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem};

/// How wide the orgs column is whenever it shares the screen. The real
/// layout (`tui::views::column_areas`) and the thresholds it degrades at
/// (`tui::views::columns_for`) both read this constant, never a copy of it.
///
/// Room for an org row — the login at `ORG_NAME_WIDTH`, the cache total at
/// `ORG_SIZE_WIDTH` — plus the block's left and right borders.
pub(crate) const COLUMN_WIDTH: u16 = 22;

/// Widest an org login renders as before `views::fit` elides it.
const ORG_NAME_WIDTH: usize = 11;

/// The cache total's right-aligned field. `human_size` writes at most eight
/// characters (`999.9 Go`), which leaves one space from the login; a value
/// that rounds up to `1000.0 Go` still fits whole.
const ORG_SIZE_WIDTH: usize = 9;

/// Renders the orgs as a stateful list so ratatui scrolls to keep the
/// cursor visible — without a persistent `ListState` it only draws the rows
/// that fit and the cursor walks off screen past that point.
///
/// Organizations only. The current org's repositories used to unfold here,
/// indented under it, and nothing but the highlight told the two levels
/// apart (spec §1); they have their own column now, `tui::views::repos`.
pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Orgs;

    // Built first, from shared borrows only: the items own their strings
    // (`ListItem<'static>`), so the borrow of `app` ends here, before
    // `app.org_state` is borrowed mutably below.
    let items: Vec<ListItem<'static>> = app
        .orgs
        .iter()
        .map(|org| {
            ListItem::new(Line::from(vec![
                Span::styled(views::fit(&org.login, ORG_NAME_WIDTH), theme::text_style()),
                Span::styled(
                    format!("{:>ORG_SIZE_WIDTH$}", human_size(org.cache_bytes)),
                    theme::muted(),
                ),
            ]))
        })
        .collect();

    app.org_state.select(if items.is_empty() {
        None
    } else {
        Some(app.org_cursor.min(items.len() - 1))
    });
    // Smoke S3: an offset the last, shorter frame needed is not one this
    // frame needs (`views::clamp_offset`).
    let heights: Vec<usize> = items.iter().map(ListItem::height).collect();
    views::clamp_offset(&mut app.org_state, &heights, area.height.saturating_sub(2));

    let list = List::new(items)
        .block(views::column_block(" ORGS ", focused))
        .highlight_style(views::cursor_style(focused));

    f.render_stateful_widget(list, area, &mut app.org_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OrgSummary, RepoSummary};
    use crate::repos::RepoClass;

    /// The tree is gone (spec §2): the orgs column lists organizations and
    /// nothing else — the current org's repositories have their own column
    /// now. And an org row keeps its cache total inside the rect the column
    /// is really given: a login longer than the column is elided, never
    /// allowed to push the total off the edge. Swept over every width the
    /// render tests cover, focus on the orgs so the column is on screen in
    /// all three layouts.
    #[test]
    fn the_orgs_column_lists_orgs_only_and_keeps_their_totals_at_every_width() {
        let mut app = App::new(vec![OrgSummary {
            login: "SecondBrain-organisation".into(),
            cache_bytes: 14_900_000_000,
            cache_count: 1,
            repos: vec![RepoSummary {
                name: "lokiprint".into(),
                cache_bytes: 0,
                cache_count: 0,
                private: false,
                age_days: 685,
                class: RepoClass::Archivable,
            }],
            billing: None,
            ..Default::default()
        }]);

        for width in 60..=200u16 {
            let (_, column) =
                crate::tui::views::testing::focused_column(&mut app, Focus::Orgs, width, 12);
            assert!(
                column.contains("14.9 Go"),
                "the org's cache total clipped at width {width}:\n{column}"
            );
            assert!(
                column.contains("SecondBr"),
                "the org's login vanished at width {width}:\n{column}"
            );
            assert!(
                !column.contains("lokiprint"),
                "a repository unfolded into the orgs column at width {width}:\n{column}"
            );
        }
    }
}
