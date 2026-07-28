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

pub fn render(app: &App, f: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app
        .visible_resources()
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let spans = row_spans(r, app.selected.contains(&r.id));
            let line = Line::from(spans);
            if i == app.res_cursor {
                ListItem::new(line).style(theme::selection_style())
            } else {
                ListItem::new(line)
            }
        })
        .collect();

    let title = format!(
        " {} éléments · {} ",
        items.len(),
        human_size(app.selection_bytes())
    );
    f.render_widget(
        List::new(items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        ),
        area,
    );
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
