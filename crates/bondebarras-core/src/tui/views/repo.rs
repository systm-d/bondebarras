//! Right pane: the resources of the selected repository.

use crate::model::{Resource, ResourceKind, human_size};
use crate::stale::pr_number_from_ref;
use crate::tui::app::App;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

/// One row of the resource list, as styled spans.
///
/// Split out from the widget so it can be asserted on without a terminal.
pub fn row_spans(r: &Resource, checked: bool) -> Vec<Span<'static>> {
    let kind = match r.kind {
        ResourceKind::Cache => "cache",
        ResourceKind::Artifact => "artif",
        ResourceKind::WorkflowRun => "run  ",
    };

    let mut spans = vec![
        Span::styled(
            if checked { "[x] " } else { "[ ] " }.to_string(),
            theme::text_style(),
        ),
        Span::styled(format!("{kind}  "), theme::muted()),
        Span::styled(format!("{:<34}", r.label), theme::text_style()),
        Span::styled(
            format!("{:>10}  ", human_size(r.size_bytes)),
            theme::muted(),
        ),
    ];

    // A stale row earns its own colour and the PR that made it dead weight.
    match r.git_ref.as_deref().and_then(pr_number_from_ref) {
        Some(n) if r.stale_pr => {
            spans.push(Span::styled(format!("PR#{n} ⚑"), theme::stale_style()))
        }
        _ => spans.push(Span::styled(format!("{}j", r.age_days), theme::muted())),
    }
    spans
}

/// Renders the resource list as a stateful list so ratatui scrolls to keep
/// the selection visible. On a 69-cache repo, an 80x24 terminal only fits
/// about 19 rows without this — the plain `render_widget` used before left
/// most of them unreachable.
pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    // Built first, from a shared borrow of `app` only: the items own their
    // strings (`ListItem<'static>`), so the borrow ends here, before
    // `app.res_state` is borrowed mutably below.
    let items: Vec<ListItem<'static>> = app
        .visible_resources()
        .into_iter()
        .map(|r| {
            let spans = row_spans(r, app.selected.contains(&(r.kind, r.id)));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = format!(
        " {} éléments · {} ",
        items.len(),
        human_size(app.selection_bytes())
    );

    app.res_state.select(if items.is_empty() {
        None
    } else {
        Some(app.res_cursor.min(items.len() - 1))
    });

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        )
        .highlight_style(theme::selection_style());

    f.render_stateful_widget(list, area, &mut app.res_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ResourceKind;

    fn res(stale: bool) -> Resource {
        Resource {
            kind: ResourceKind::Cache,
            id: 1,
            label: "v0-rust-coverage-Linux-x64".into(),
            size_bytes: 273_678_336,
            age_days: 40,
            git_ref: Some("refs/pull/32/merge".into()),
            stale_pr: stale,
        }
    }

    fn text(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn a_row_shows_the_checkbox_kind_label_and_size() {
        let line = text(&row_spans(&res(false), true));
        assert!(line.contains("[x]"));
        assert!(line.contains("cache"));
        assert!(line.contains("v0-rust-coverage-Linux-x64"));
        assert!(line.contains("273.7 Mo"));
    }

    #[test]
    fn a_stale_row_carries_the_flag_and_its_pr_number() {
        let line = text(&row_spans(&res(true), false));
        assert!(line.contains("[ ]"));
        assert!(line.contains("PR#32"));
        assert!(line.contains('⚑'));
    }

    #[test]
    fn a_fresh_row_carries_no_flag() {
        assert!(!text(&row_spans(&res(false), false)).contains('⚑'));
    }

    /// The flag is the safest thing on screen to delete, so it must never be
    /// painted like an error. Task 11 locked `STALE != ERROR` in the theme;
    /// this locks the row actually reaching for the right one — asserting on
    /// content alone would let a swapped style through unnoticed.
    #[test]
    fn the_flag_is_painted_stale_not_error() {
        let spans = row_spans(&res(true), false);
        let flag = spans
            .last()
            .expect("a row always ends with a flag or an age");
        assert_eq!(flag.style, theme::stale_style());
        assert_ne!(flag.style, theme::status_error());
    }
}
