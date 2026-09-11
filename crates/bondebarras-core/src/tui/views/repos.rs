//! Column 2: the repositories of the org under the cursor.

use crate::model::{RepoSummary, human_size};
use crate::repos::RepoClass;
use crate::tui::app::{App, Focus};
use crate::tui::theme;
use crate::tui::views;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem};

/// How wide the repos column is whenever it shares the screen. The real
/// layout (`tui::views::column_areas`) and the thresholds it degrades at
/// (`tui::views::columns_for`) both read this constant rather than
/// hardcoding their own — the two must never be able to drift apart the way
/// they did for Finding 2 of the v0.5 final review.
///
/// Wide enough for the repository row's four columns — checkbox, name,
/// status, cache — at their own widths below, plus the block's left/right
/// border. Finding 2: the row used to be squeezed to fit a bare 26 columns
/// by dropping the cache-byte column outright, in a tool whose entire point
/// is volume; this widens the pane instead, past what the checkbox, the
/// widest status (a leading separator space plus `déjà archivé`, 12
/// characters) and the cache column together actually need — one column of
/// slack past that spends on the separator itself, one stays unused. The
/// three-column design first set this column at 26 again, and was amended
/// back to this width for the same reason (Amendement 2, 2026-09-11).
pub(crate) const COLUMN_WIDTH: u16 = 38;

/// Widest a repository name renders as before `views::fit` truncates it
/// with a trailing `…`. Narrower than `tui::views::repo::LABEL_WIDTH`:
/// reduced from 16 to make room for the cache-byte column Finding 2
/// restores, per that finding's own suggested fix — a repository name is
/// not the only thing this row has to show.
const REPO_NAME_WIDTH: usize = 10;

/// One row of the repos column, as styled spans — this column's analogue of
/// `tui::views::repo::row_spans`, split out for the same reason: testable
/// without a terminal, and directly reusable by `render` below.
///
/// The repository is the one candidate that lives in its own column rather
/// than the resource list, and its row is unlike every resource row in
/// three ways at once:
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
///   a tool whose whole point is showing where the volume is, the repos
///   column is exactly where a single repo's own share of it belongs on
///   screen.
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
    // status this column or the resource column shows: it sits mid-row here,
    // between the name and the cache column, rather than trailing the line
    // the way every other classification does. The trailing padding keeps a
    // short status (an age like "5 j") from butting straight up against the
    // cache figure; the leading space does the same job on the other side —
    // `views::fit` pads a short name but has nothing left to give a
    // truncated one (at or over `REPO_NAME_WIDTH`), so without it a name
    // like this project's own "bondebarras" (11 characters, truncated) ran
    // straight into its status: "bondebarr…5 j". Both spend from the two
    // columns `COLUMN_WIDTH` leaves unused past what the four columns
    // themselves need; one is spent here, one stays spare.
    let status_col = format!(" {status:<12}");

    vec![
        Span::styled(checkbox.to_string(), theme::text_style()),
        Span::styled(views::fit(&repo.name, REPO_NAME_WIDTH), theme::text_style()),
        Span::styled(status_col, theme::muted()),
        Span::styled(
            format!("{:>8}", human_size(repo.cache_bytes)),
            theme::muted(),
        ),
    ]
}

/// Renders the repositories of the org under the cursor
/// (`app.orgs[app.org_cursor].repos`) as a stateful list, for the same
/// scrolling reason as `tui::views::orgs::render`.
pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Repos;

    // Built first, from shared borrows only: the items own their strings, so
    // the borrow of `app` ends here, before `app.repo_state` is borrowed
    // mutably below.
    let items: Vec<ListItem<'static>> = app
        .orgs
        .get(app.org_cursor)
        .map(|org| {
            org.repos
                .iter()
                .map(|repo| {
                    let checked = app
                        .selected_repo
                        .as_ref()
                        .is_some_and(|(owner, name)| *owner == org.login && *name == repo.name);
                    ListItem::new(Line::from(repo_row_spans(repo, checked)))
                })
                .collect()
        })
        .unwrap_or_default();

    app.repo_state.select(if items.is_empty() {
        None
    } else {
        Some(app.repo_cursor.min(items.len() - 1))
    });

    let list = List::new(items)
        .block(views::column_block(" DÉPÔTS ", focused))
        .highlight_style(views::cursor_style(focused));

    f.render_stateful_widget(list, area, &mut app.repo_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OrgSummary;
    use crate::tui::views::testing;

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

    /// A name at or over `REPO_NAME_WIDTH` leaves `views::fit` no room to
    /// pad it with a separating space of its own (the truncated form fills
    /// the column exactly: head chars plus the ellipsis). This project's own
    /// name is the concrete case: "bondebarras" is 11 characters, one over
    /// the width, so it always truncates.
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
    /// bypassing the real layout's fixed-width split entirely — the row
    /// always had the whole frame's width to itself, so this could never
    /// fail no matter how wide its own content grew, the ninth test of that
    /// kind on this project. Anchored on the actual `Rect` the column
    /// receives in production — `tui::views::column_areas`, through
    /// `views::testing::focused_column`, with the whole real `render` drawn
    /// and only that `Rect` read back, never the frame. Since the three
    /// columns that `Rect` is [`COLUMN_WIDTH`] wide in the three- and
    /// two-column layouts, and the whole terminal in the one-column layout.
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
        let mut app = App::new(vec![OrgSummary {
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

        // Floor: the column's own width, the narrowest terminal whose
        // one-column layout still gives the column all of it. Swept well past
        // it (to 300), since the column's width is constant once it shares
        // the screen — the sweep proves that stays true, rather than
        // assuming it.
        for width in COLUMN_WIDTH..=300 {
            let (_, rendered) = testing::focused_column(&mut app, Focus::Repos, width, 10);

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

    /// Spec §2: the repos column lists `app.orgs[app.org_cursor].repos` —
    /// the org under the cursor, not the first one, and not every org's
    /// repositories at once. Two orgs with distinct repositories, cursor on
    /// the second: only its repository may appear. Swept, focus on the repos
    /// so the column is on screen in all three layouts.
    #[test]
    fn the_repos_column_lists_the_cursor_orgs_repositories_at_every_width() {
        let org = |login: &str, name: &str| OrgSummary {
            login: login.into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![repo(name, RepoClass::Archivable, 1)],
            billing: None,
        };
        let mut app = App::new(vec![
            org("systm-d", "josephine"),
            org("exec-d", "lokiprint"),
        ]);
        app.org_cursor = 1;

        for width in 60..=200u16 {
            let (_, column) = testing::focused_column(&mut app, Focus::Repos, width, 12);
            assert!(
                column.contains("lokiprint"),
                "the cursor org's repository is missing at width {width}:\n{column}"
            );
            assert!(
                !column.contains("josephine"),
                "another org's repository is listed at width {width}:\n{column}"
            );
        }
    }
}
