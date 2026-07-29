//! Left pane: organizations, biggest cache footprint first.

use crate::model::{RepoSummary, human_size};
use crate::repos::RepoClass;
use crate::tui::app::App;
use crate::tui::app::Focus;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

/// How wide the left tree pane is, in the real layout
/// (`tui::views::mod::render`, which reads this constant rather than
/// hardcoding its own — the two must never be able to drift apart the way
/// they did for Finding 2 of the final review).
///
/// Wide enough for the repository row's four columns — checkbox, name,
/// status, cache — at their own widths below, plus the block's left/right
/// border. Finding 2: the row used to be squeezed to fit a bare 26 columns
/// by dropping the cache-byte column outright, in a tool whose entire point
/// is volume; this widens the pane instead, past what the checkbox, the
/// widest status (a leading separator space plus `déjà archivé`, 12
/// characters) and the cache column together actually need — one column of
/// slack past that spends on the separator itself, one stays unused.
pub(crate) const PANE_WIDTH: u16 = 38;

/// Widest a repository name renders as before this truncates it with a
/// trailing `…`. Narrower than `tui::views::repo::LABEL_WIDTH`, and than this
/// pane's own previous width: reduced from 16 to make room for the
/// cache-byte column Finding 2 restores, per that finding's own suggested
/// fix — a repository name is not the only thing this row has to show.
const REPO_NAME_WIDTH: usize = 10;

fn display_repo_name(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() > REPO_NAME_WIDTH {
        let head: String = chars[..REPO_NAME_WIDTH - 1].iter().collect();
        format!("{head}…")
    } else {
        format!("{name:<REPO_NAME_WIDTH$}")
    }
}

/// One repository row of the tree, as styled spans — this pane's analogue of
/// `tui::views::repo::row_spans`, split out for the same reason: testable
/// without a terminal, and directly reusable by `render` below.
///
/// The repository is the first candidate that lives in the tree rather than
/// the right-hand resource list, and its row is unlike every resource row in
/// two ways at once:
///
/// - **The trailing field is never a bare age.** An `Archivable` repo shows
///   its age (`"775 j"`) — but `pushed_at` alone is not proof of
///   abandonment, so unlike a stale cache's flag this is never painted as
///   urgent, and it never drives any preselection (see
///   `App::select_all_stale`'s own guard). `AlreadyArchived` and
///   `NoAdminRights` show their class name instead (`"déjà archivé"`,
///   `"sans droits"`) — the same "classification replaces the age" shape a
///   `Branch` row already has (see `tui::views::repo::row_spans`), applied
///   here to a family with three classes instead of four.
/// - **A non-`Archivable` row carries no checkbox at all**, not an empty
///   one: neither class is tickable (`App::toggle_repo_selected` refuses
///   both outright), and a checkbox that can never be checked would be a UI
///   lie of its own — unlike a protected tag or a live branch, which stay
///   individually tickable even though bulk selection refuses them.
/// - **It carries the repo's own cache footprint**, right-aligned like the
///   org row's own total. Finding 2 of the final review: an earlier version
///   of this row dropped that column outright to fit a too-narrow pane — in
///   a tool whose whole point is showing where the volume is, the repo tree
///   is exactly where a single repo's own share of it belongs on screen.
pub fn repo_row_spans(repo: &RepoSummary, checked: bool) -> Vec<Span<'static>> {
    let checkbox = match repo.class {
        RepoClass::Archivable if checked => "[x] ",
        RepoClass::Archivable => "[ ] ",
        RepoClass::AlreadyArchived | RepoClass::NoAdminRights => "    ",
    };
    let status = match repo.class {
        RepoClass::Archivable => format!("{} j", repo.age_days),
        RepoClass::AlreadyArchived => "déjà archivé".to_string(),
        RepoClass::NoAdminRights => "sans droits".to_string(),
    };
    // A leading space, then padded to a fixed width — unlike every other
    // status this pane or the resource pane shows: it sits mid-row here,
    // between the name and the cache column, rather than trailing the line
    // the way every other classification does. The trailing padding keeps a
    // short status (an age like "5 j") from butting straight up against the
    // cache figure; the leading space does the same job on the other side —
    // `display_repo_name` pads a short name but has nothing left to give a
    // truncated one (at or over `REPO_NAME_WIDTH`), so without it a name
    // like this project's own "bondebarras" (11 characters, truncated) ran
    // straight into its status: "bondebarr…5 j". Both spend from the two
    // columns `PANE_WIDTH` leaves unused past what the four columns
    // themselves need; one is spent here, one stays spare.
    let status_col = format!(" {status:<12}");

    vec![
        Span::styled(checkbox.to_string(), theme::text_style()),
        Span::styled(display_repo_name(&repo.name), theme::text_style()),
        Span::styled(status_col, theme::muted()),
        Span::styled(
            format!("{:>8}", human_size(repo.cache_bytes)),
            theme::muted(),
        ),
    ]
}

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
                let checked =
                    app.selected_repo.as_ref() == Some(&(org.login.clone(), repo.name.clone()));
                items.push(ListItem::new(Line::from(repo_row_spans(repo, checked))));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Flattens a row's spans to plain text for substring assertions —
    /// mirrors `tui::views::repo::tests::text`.
    fn text(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn repo(name: &str, class: RepoClass, age_days: i64) -> RepoSummary {
        RepoSummary {
            name: name.to_string(),
            cache_bytes: 0,
            cache_count: 0,
            private: false,
            age_days,
            class,
        }
    }

    #[test]
    fn an_archivable_repo_shows_its_age_not_a_class_name() {
        let line = text(&repo_row_spans(
            &repo("lokiprint", RepoClass::Archivable, 685),
            false,
        ));
        assert!(line.contains("685 j"), "got: {line}");
        assert!(!line.to_lowercase().contains("archivé"));
        assert!(!line.to_lowercase().contains("droits"));
    }

    #[test]
    fn an_already_archived_repo_shows_its_class_not_an_age() {
        let line = text(&repo_row_spans(
            &repo(".github", RepoClass::AlreadyArchived, 775),
            false,
        ));
        assert!(line.contains("déjà archivé"), "got: {line}");
        assert!(!line.contains("775"), "got: {line}");
    }

    #[test]
    fn a_repo_without_admin_rights_shows_its_class_not_an_age() {
        let line = text(&repo_row_spans(
            &repo("private-thing", RepoClass::NoAdminRights, 42),
            false,
        ));
        assert!(line.contains("sans droits"), "got: {line}");
        assert!(!line.contains('4'), "got: {line}");
    }

    #[test]
    fn an_archivable_repo_carries_a_checkbox() {
        let unchecked = text(&repo_row_spans(
            &repo("lokiprint", RepoClass::Archivable, 685),
            false,
        ));
        assert!(unchecked.contains("[ ]"), "got: {unchecked}");

        let checked = text(&repo_row_spans(
            &repo("lokiprint", RepoClass::Archivable, 685),
            true,
        ));
        assert!(checked.contains("[x]"), "got: {checked}");
    }

    /// Rule 2 made visible: a class that is not tickable must not even offer
    /// the shape of a checkbox — `[ ]` sitting next to "déjà archivé" would
    /// read as an invitation the guard then refuses, the same lie an
    /// offered-then-403'd tick would be.
    #[test]
    fn a_non_archivable_repo_carries_no_checkbox_at_all() {
        for class in [RepoClass::AlreadyArchived, RepoClass::NoAdminRights] {
            let line = text(&repo_row_spans(&repo("x", class, 1), false));
            assert!(
                !line.contains('[') && !line.contains(']'),
                "got: {line} for {class:?}"
            );
        }
    }

    /// A name at or over `REPO_NAME_WIDTH` leaves `display_repo_name` no
    /// room to pad it with a separating space of its own (the truncated
    /// form fills the column exactly: head chars plus the ellipsis). This
    /// project's own name is the concrete case: "bondebarras" is 11
    /// characters, one over the width, so it always truncates.
    #[test]
    fn a_truncated_repo_name_does_not_butt_against_its_status() {
        let line = text(&repo_row_spans(
            &repo("bondebarras", RepoClass::Archivable, 5),
            false,
        ));
        assert!(line.contains('…'), "got: {line}");
        assert!(line.contains("… 5 j"), "got: {line}");
    }

    #[test]
    fn a_long_repo_name_is_truncated_with_an_ellipsis() {
        let line = text(&repo_row_spans(
            &repo(
                "an-extremely-long-repository-name-that-will-not-fit",
                RepoClass::Archivable,
                1,
            ),
            false,
        ));
        assert!(line.contains('…'), "got: {line}");
        // The status must still be there — the whole point of bounding the
        // name is to leave it room, not just to look tidy.
        assert!(line.contains("1 j"), "got: {line}");
    }

    /// v0.3 shipped rows correct in code and clipped on screen, and its
    /// follow-up modal defect (`tui::views::confirm`) recurred at exactly
    /// one height per width — three sampled sizes missed it. This sweeps
    /// rather than samples.
    ///
    /// Finding 2 of the final review: the previous version of this test
    /// called `render` directly against the whole `TestBackend` frame,
    /// bypassing the real layout's fixed-width split
    /// (`tui::views::mod::render`) entirely — the row always had the whole
    /// frame's width to itself, so this could never fail no matter how wide
    /// its own content grew, the ninth test of that kind on this project.
    /// Re-anchored here on the actual `Rect` the pane receives in
    /// production: the same horizontal split `tui::views::mod::render`
    /// performs, at the same [`PANE_WIDTH`] the two share, with `render`
    /// handed only `cols[0]` — never the frame.
    ///
    /// The fixture carries all three classes at once: an `Archivable` repo's
    /// age, an `AlreadyArchived` one's class name, and a `NoAdminRights`
    /// one's — `déjà archivé` (12 characters) is the widest of the three and
    /// the one that actually determines whether the row fits.
    ///
    /// The `Archivable` repo also carries a real, non-zero `cache_bytes`
    /// (`11.1 Go`) rather than the `0`-filled fixtures every other test in
    /// this module uses. Before this, nothing in the suite asserted on the
    /// rendered cache figure at all: a reviewer's scratch deletion of the
    /// cache span from `repo_row_spans` left the full suite green, the
    /// tenth test on this project to name the right property and be unable
    /// to fail on it. This closes that: the loop below asserts the figure
    /// survives every swept width, not just the three status strings.
    #[test]
    fn a_repository_row_stays_legible_across_swept_widths() {
        let mut app = App::new(vec![crate::model::OrgSummary {
            login: "maxds-lyon".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![
                RepoSummary {
                    cache_bytes: 11_100_000_000,
                    ..repo("lokiprint", RepoClass::Archivable, 685)
                },
                repo(".github", RepoClass::AlreadyArchived, 775),
                repo("private-thing", RepoClass::NoAdminRights, 42),
            ],
            billing: None,
        }]);

        // Floor: the smallest total frame width at which the horizontal
        // split still hands this pane its full `PANE_WIDTH` — below it,
        // `Constraint::Min(20)` on the resource pane starts eating into the
        // `Length(PANE_WIDTH)` request. Swept well past it (to 300, checked
        // empirically with no regression) since the pane's own width is
        // constant once the split has room to honour it in full — the sweep
        // proves that stays true, rather than assuming it.
        const FLOOR: u16 = PANE_WIDTH + 20;
        for width in FLOOR..=300 {
            let backend = ratatui::backend::TestBackend::new(width, 10);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    let cols = ratatui::layout::Layout::default()
                        .direction(ratatui::layout::Direction::Horizontal)
                        .constraints([
                            ratatui::layout::Constraint::Length(PANE_WIDTH),
                            ratatui::layout::Constraint::Min(20),
                        ])
                        .split(f.area());
                    render(&mut app, f, cols[0]);
                })
                .unwrap();

            let rendered: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect();

            assert!(
                rendered.contains("685 j"),
                "the archivable repo's age clipped at width {width}: {rendered}"
            );
            assert!(
                rendered.contains("déjà archivé"),
                "the already-archived class clipped at width {width}: {rendered}"
            );
            assert!(
                rendered.contains("sans droits"),
                "the no-admin-rights class clipped at width {width}: {rendered}"
            );
            assert!(
                rendered.contains("11.1 Go"),
                "the archivable repo's cache size clipped at width {width}: {rendered}"
            );
        }
    }
}
