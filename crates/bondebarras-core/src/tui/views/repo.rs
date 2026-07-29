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
        ResourceKind::PackageVersion => "pkg  ",
    };

    // GitHub exposes no size for a package version, under any name (see
    // `api::packages`). `size_bytes` is hardcoded to 0 for that kind, and a
    // bare "0 o" here would read as "empty" — the opposite of the truth — so
    // this column shows the gap explicitly instead of formatting the 0.
    let size = if r.kind == ResourceKind::PackageVersion {
        "—".to_string()
    } else {
        human_size(r.size_bytes)
    };

    let mut spans = vec![
        Span::styled(
            if checked { "[x] " } else { "[ ] " }.to_string(),
            theme::text_style(),
        ),
        Span::styled(format!("{kind}  "), theme::muted()),
        Span::styled(format!("{:<34}", r.label), theme::text_style()),
        Span::styled(format!("{size:>10}  "), theme::muted()),
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

/// The list block's title: item count and byte tally, plus — when the
/// visible list holds at least one package version — the caveat that GitHub
/// exposes no size for that family.
///
/// Not optional when `has_packages` is true: a column of `—` in a tool that
/// shows bytes on every other screen reads as "these are empty", which is
/// the opposite of the truth.
fn list_title(count: usize, bytes: u64, has_packages: bool) -> String {
    if has_packages {
        format!(
            " {count} éléments · {} · ⚠ GitHub n'expose pas la taille des versions de packages ",
            human_size(bytes)
        )
    } else {
        format!(" {count} éléments · {} ", human_size(bytes))
    }
}

/// Renders the resource list as a stateful list so ratatui scrolls to keep
/// the selection visible. On a 69-cache repo, an 80x24 terminal only fits
/// about 19 rows without this — the plain `render_widget` used before left
/// most of them unreachable.
pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    // Read before `items` is built, from the same shared borrow, so both can
    // draw from `app.visible_resources()` before anything is borrowed
    // mutably below.
    let has_packages = app
        .visible_resources()
        .iter()
        .any(|r| r.kind == ResourceKind::PackageVersion);

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

    let title = list_title(items.len(), app.selection_bytes(), has_packages);

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

    fn package_resource() -> Resource {
        Resource {
            kind: ResourceKind::PackageVersion,
            id: 9,
            label: "sha256:9a26c7080… (sans tag)".into(),
            size_bytes: 0,
            age_days: 5,
            git_ref: None,
            stale_pr: false,
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

    #[test]
    fn a_package_row_says_its_size_is_unknown_not_zero() {
        // Every other screen shows bytes. A bare "0 o" here would read as
        // "empty", which is the opposite of the truth.
        let line = text(&row_spans(&package_resource(), false));
        assert!(!line.contains("0 o"), "got: {line}");
        assert!(line.contains('—'), "got: {line}");
    }

    #[test]
    fn the_title_warns_when_the_list_holds_a_package_version() {
        // A wrong implementation that never surfaces the caveat would leave
        // the "—" size column reading as "empty" instead of "unmeasured".
        let title = list_title(3, 0, true);
        assert!(title.contains("GitHub"), "got: {title}");
        assert!(title.to_lowercase().contains("taille"), "got: {title}");
    }

    #[test]
    fn the_title_carries_no_warning_without_a_package_version() {
        // A wrong implementation that always shows the caveat would clutter
        // every ordinary cache/artifact/run listing with an irrelevant line.
        let title = list_title(3, 100, false);
        assert!(!title.contains("GitHub"), "got: {title}");
        assert!(title.contains("100 o"), "got: {title}");
    }
}
