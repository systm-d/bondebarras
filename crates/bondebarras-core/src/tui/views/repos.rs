//! Column 2: the repositories of the org under the cursor.

use crate::model::{OrgSummary, RepoSummary, human_size};
use crate::repos::RepoClass;
use crate::tui::app::{App, Focus};
use crate::tui::theme;
use crate::tui::views::{self, gauges};
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
/// Wide enough for the repository row's five columns — checkbox, name,
/// status, ceiling mark, cache — at their own widths below, plus the block's
/// left/right border. Finding 2: the row used to be squeezed to fit a bare
/// 26 columns by dropping the cache-byte column outright, in a tool whose
/// entire point is volume; this widens the pane instead, past what the
/// checkbox, the widest status (`déjà archivé`, 12 characters) and the cache
/// column together actually need. Of the two columns of slack past that,
/// one spends on the status's leading separator space; the other, unused
/// until #13, carries `ceiling_mark`. The row now fills all 36 inner cells
/// with no margin left, which is why a repository's storage goes on a
/// detail line of its own (`repo_item`) rather than on its row. The
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
///   `App::select_safe`'s own guard). `AlreadyArchived` and
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
///   screen. One cell before it, `ceiling_mark`: `⚠` past the included
///   10 GiB, a space otherwise (#13).
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
    // straight into its status: "bondebarr…5 j". The leading space spends
    // one of the two columns `COLUMN_WIDTH` leaves past what the fields
    // themselves need; the other is `ceiling_mark`'s cell, just before the
    // cache figure.
    let status_col = format!(" {status:<12}");

    vec![
        Span::styled(checkbox.to_string(), theme::text_style()),
        Span::styled(views::fit(&repo.name, REPO_NAME_WIDTH), theme::text_style()),
        Span::styled(status_col, theme::muted()),
        ceiling_mark(repo.cache_bytes),
        Span::styled(
            format!("{:>8}", human_size(repo.cache_bytes)),
            theme::muted(),
        ),
    ]
}

/// `⚠` before the cache figure of a repository past the included 10 GiB
/// (`gauges::cache_over_ceiling`), a space otherwise, so figures stay
/// aligned. The row has no room to say why; column 3's cache gauge does, the
/// moment the repository is opened.
pub fn ceiling_mark(cache_bytes: u64) -> Span<'static> {
    if gauges::cache_over_ceiling(cache_bytes) {
        Span::styled("⚠", theme::status_warn())
    } else {
        Span::raw(" ")
    }
}

/// The detail line under a repository that held Actions storage in the
/// newest month its org's usage report carries, naming that month, so the
/// one holding it is found without opening every repository. On its own
/// line because the repository row has no cell left once `ceiling_mark`
/// takes its one.
pub fn storage_detail_line(gbh: f64, month: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  ↳ {gbh:.1} GB-h, {month}"),
        theme::muted(),
    ))
}

/// A repository's list item: its row, plus the storage detail line when
/// `storage` — `(GB-hours, month)`, from `repo_storage_this_month` — has
/// something to show. Only those repositories take a second line; the
/// cursor still moves one repository at a time.
pub fn repo_item(row: Vec<Span<'static>>, storage: Option<(f64, String)>) -> ListItem<'static> {
    match storage {
        Some((gbh, month)) => {
            ListItem::new(vec![Line::from(row), storage_detail_line(gbh, &month)])
        }
        None => ListItem::new(Line::from(row)),
    }
}

/// A repository's Actions storage in the newest month its org's usage
/// report carries, as `(GB-hours, month)`: the month column 3's minutes
/// gauge reads (`views::repo`'s `repo_minutes_used`), so the two columns
/// never speak of different months, and drawing reads no clock.
///
/// `None` when there is nothing to show: billing unreadable (unknown, not
/// zero), a report with no month in it, or a repository that held none that
/// month (nothing to say).
fn repo_storage_this_month(org: &OrgSummary, repo: &str) -> Option<(f64, String)> {
    let report = org.billing.as_ref()?;
    // The last of `BillingReport::months()`, without sorting every item of
    // the report again for every row.
    let month = report.items.iter().map(|i| i.month.as_str()).max()?;
    let gbh = report.storage_gbh_for_repo(month, repo);
    (gbh > 0.0).then(|| (gbh, month.to_string()))
}

/// The column's title while it has no line inside its borders for a
/// repository row (`App::repos_too_short`): `espace` and `d` do nothing from
/// the column then, and the title is the one place left to say why — the
/// resources column's `TOO_SHORT_TITLE`, in the same words.
const TOO_SHORT_TITLE: &str = " DÉPÔTS · fenêtre trop basse ";

/// Renders the repositories of the org under the cursor
/// (`app.orgs[app.org_cursor].repos`) as a stateful list, for the same
/// scrolling reason as `tui::views::orgs::render`, each with its storage
/// detail line (`repo_item`) when the column has room for one.
///
/// Pre-flight 4.14, final review I2's defect in this column: ratatui draws
/// nothing of an item taller than the list's area, so a two-line item under
/// the cursor, in a column with one inner line, took the cursor's repository
/// off screen. Detail lines are drawn only from two inner lines
/// (`area.height >= 4`), so whatever line the column has, every item fits in
/// it. With no inner line at all, no repository row can be drawn: the
/// column records it in `App::repos_too_short`, which keeps `espace` and `d`
/// from acting from this column on a repository no frame shows, and its
/// title says why (`TOO_SHORT_TITLE`).
pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Repos;
    // The column's inside: the title does not change it.
    let inner = views::column_block("", focused).inner(area);
    let room_for_detail = inner.height >= 2;

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
                    repo_item(
                        repo_row_spans(repo, checked),
                        repo_storage_this_month(org, &repo.name).filter(|_| room_for_detail),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let too_short = inner.height == 0;
    app.repos_too_short = too_short;
    app.repo_state.select(if items.is_empty() {
        None
    } else {
        Some(app.repo_cursor.min(items.len() - 1))
    });

    let title = if too_short {
        TOO_SHORT_TITLE
    } else {
        " DÉPÔTS "
    };
    let list = List::new(items)
        .block(views::column_block(title, focused))
        .highlight_style(views::cursor_style(focused));

    f.render_stateful_widget(list, area, &mut app.repo_state);
}

#[cfg(test)]
mod tests {
    use super::*;
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
            ..Default::default()
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
            ..Default::default()
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

    /// Controller ruling on task 9's concern 1, the I2 precedent: while the
    /// repos column cannot draw one repository row, `espace` and `d` do
    /// nothing from it (`App::repos_too_short`), and its title says why, the
    /// way the resources column's does.
    ///
    /// Swept over every height from 5 to 30 at 60, 80 and 100 columns, read
    /// off the repos column's real rect: with no inner line, its top border
    /// reads ` DÉPÔTS · fenêtre trop basse `, whole; from the first height
    /// with a row, a repository row is on screen and the column says nothing
    /// of the window. Both kinds of height must be met, or the sweep proves
    /// nothing.
    #[test]
    fn the_repos_column_says_when_it_is_too_short_at_every_height() {
        let (mut too_short_seen, mut drawable_seen) = (0, 0);
        let mut failures = Vec::new();
        for width in [60u16, 80, 100] {
            for height in 5u16..=30 {
                let mut app = exec_d_with_storage(1_000);
                let (rect, column) = testing::focused_column(&mut app, Focus::Repos, width, height);
                let inner = rect.height.saturating_sub(2);
                let title = column.lines().next().unwrap_or_default();
                let at = format!("{width}x{height} (inner {inner})");
                if inner == 0 {
                    too_short_seen += 1;
                    if !title.contains(" DÉPÔTS · fenêtre trop basse ") {
                        failures.push(format!("no whole too-short title at {at}: {title:?}"));
                    }
                } else {
                    drawable_seen += 1;
                    if !title.contains(" DÉPÔTS ") || !column.contains("disconnec…") {
                        failures.push(format!("no title or no repository row at {at}:\n{column}"));
                    }
                    if column.contains("fenêtre trop basse") {
                        failures.push(format!("says too short with a row on screen at {at}"));
                    }
                }
            }
        }
        assert!(
            too_short_seen > 0 && drawable_seen > 0,
            "the sweep met {too_short_seen} too-short and {drawable_seen} drawable sizes"
        );
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Pre-flight 4.14: ratatui draws nothing of a list item taller than the
    /// list's area, so a two-line repository item under the cursor, in a
    /// column with one inner line, took the cursor's repository off screen.
    ///
    /// Swept over every height from 5 to 30 at 60, 80 and 100 columns (one,
    /// two and three columns), on a fresh app each time, read off the repos
    /// column's real rect: ten repositories all holding storage, the cursor
    /// on the last — a public one, whose storage counts like any other. Its
    /// name is on screen whenever the column has one inner line; its own
    /// detail line whenever it has two; and with only one, no detail line
    /// at all. Every failing size is collected, so the output names each.
    #[test]
    fn the_cursor_repository_stays_on_screen_at_every_height() {
        let names = [
            "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india",
            "vitrine",
        ];
        let fresh = || {
            let repo = |name: &str| RepoSummary {
                name: name.into(),
                cache_bytes: 1_000,
                cache_count: 1,
                private: name != "vitrine",
                age_days: 3,
                class: RepoClass::Archivable,
            };
            let gbh = |name: &str| if name == "vitrine" { 7.5 } else { 42.0 };
            let mut app = App::new(vec![OrgSummary {
                login: "exec-d".into(),
                repos: names.iter().map(|n| repo(n)).collect(),
                billing: Some(crate::billing::BillingReport {
                    items: names
                        .iter()
                        .map(|n| storage("2026-09", n, gbh(n)))
                        .collect(),
                }),
                ..Default::default()
            }]);
            app.repo_cursor = names.len() - 1;
            app
        };

        let mut failures = Vec::new();
        for width in [60u16, 80, 100] {
            for height in 5u16..=30 {
                let mut app = fresh();
                let (rect, column) = testing::focused_column(&mut app, Focus::Repos, width, height);
                let inner = rect.height.saturating_sub(2);
                let at = format!("{width}x{height} (inner {inner})");
                if inner >= 1 && !column.contains("vitrine") {
                    failures.push(format!("the cursor's repository is off screen at {at}"));
                }
                if inner >= 2 && !column.contains("↳ 7.5 GB-h, 2026-09") {
                    failures.push(format!("the cursor's detail line is missing at {at}"));
                }
                if inner == 1 && column.contains('↳') {
                    failures.push(format!("a detail line in a one-line column at {at}"));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// One `Actions storage` usage item: `gbh` GB-hours held by `repo` in
    /// `month`.
    fn storage(month: &str, repo: &str, gbh: f64) -> crate::billing::UsageItem {
        crate::billing::UsageItem {
            month: month.into(),
            product: "actions".into(),
            sku: "Actions storage".into(),
            quantity: gbh,
            unit_type: "GigabyteHours".into(),
            gross: 0.0,
            discount: 0.0,
            net: 0.0,
            repo: repo.into(),
        }
    }

    /// Storage as exec-d's report carried it on 2026-09-10 — `disconnected`
    /// at 359.88 GB-hours in September, the newest month — plus an August
    /// for both repositories, and a September in which `quiet` held none.
    ///
    /// Each figure tells a wrong reading apart: August's comes first and
    /// last in the report, so only the newest month, not the first or the
    /// last item's, gives September; `quiet`'s 0.0 shows only if a
    /// repository with nothing to say still gets a line.
    fn exec_d_report() -> crate::billing::BillingReport {
        crate::billing::BillingReport {
            items: vec![
                storage("2026-08", "disconnected", 120.0),
                storage("2026-09", "disconnected", 359.88),
                storage("2026-09", "quiet", 0.0),
                storage("2026-08", "quiet", 5.0),
            ],
        }
    }

    /// exec-d's `disconnected`, holding the newest month's storage, beside a
    /// quiet repository; cursor on the first, focus on the repos.
    /// `disconnected`'s `cache_bytes` is the only parameter.
    fn exec_d_with_storage(disconnected_cache_bytes: u64) -> App {
        let repo = |name: &str, cache_bytes: u64| RepoSummary {
            name: name.into(),
            cache_bytes,
            cache_count: 1,
            private: true,
            age_days: 3,
            class: RepoClass::Archivable,
        };
        let mut app = App::new(vec![OrgSummary {
            login: "exec-d".into(),
            repos: vec![
                repo("disconnected", disconnected_cache_bytes),
                repo("quiet", 1_000),
            ],
            billing: Some(exec_d_report()),
            ..Default::default()
        }]);
        app.focus = Focus::Repos;
        app
    }

    #[test]
    fn repo_storage_detail_line_shows_gigabyte_hours() {
        let text: String = storage_detail_line(359.88, "2026-09")
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(text, "  ↳ 359.9 GB-h, 2026-09");
        let quiet: String = ceiling_mark(gauges::CACHE_CEILING_BYTES)
            .content
            .to_string();
        assert_eq!(quiet, " ", "exactly at the ceiling: no mark");
        let marked: String = ceiling_mark(gauges::CACHE_CEILING_BYTES + 1)
            .content
            .to_string();
        assert_eq!(marked, "⚠", "one byte past the ceiling: marked");
    }

    /// Pre-flight 5.2, arbitrage A: the column reads the newest month the
    /// usage report carries — the month column 3's minutes gauge reads —
    /// and never the clock. `None` whenever there is nothing to show:
    /// billing unreadable (unknown, not zero), a readable report with no
    /// month in it, a repository that held none in the newest month.
    #[test]
    fn repo_storage_this_month_reads_the_newest_reported_month_or_nothing() {
        let org = |billing| OrgSummary {
            login: "exec-d".into(),
            billing,
            ..Default::default()
        };
        let readable = org(Some(exec_d_report()));
        assert_eq!(
            repo_storage_this_month(&readable, "disconnected"),
            Some((359.88, "2026-09".to_string()))
        );
        assert_eq!(
            repo_storage_this_month(&readable, "quiet"),
            None,
            "none in the newest month, though some in an older one"
        );
        assert_eq!(repo_storage_this_month(&readable, "absent"), None);

        assert_eq!(
            repo_storage_this_month(&org(None), "disconnected"),
            None,
            "unreadable billing"
        );
        let no_months = org(Some(crate::billing::BillingReport { items: vec![] }));
        assert_eq!(
            repo_storage_this_month(&no_months, "disconnected"),
            None,
            "a readable report with no month"
        );
    }

    /// #13: find the repository holding the storage without opening it, and
    /// see a cache past 10 GiB marked — in the real layout, read off the
    /// repos column's own rect (`testing::focused_column`), at every width
    /// from 60 to 200 (one, two and three columns, focus on the repos so
    /// the column is on screen in each) and at every height from 8. The
    /// one-byte-under twin proves the ⚠ comes from the mark and nowhere
    /// else: both caches display as `10.7 Go`, and only the one past 10 GiB
    /// is marked.
    #[test]
    fn the_repos_column_shows_storage_and_the_ceiling_mark_at_every_width() {
        let sizes = (60..=200u16)
            .map(|w| (w, 30))
            .chain((8..=30u16).map(|h| (100, h)));
        let mut over = exec_d_with_storage(gauges::CACHE_CEILING_BYTES + 1);
        let mut at = exec_d_with_storage(gauges::CACHE_CEILING_BYTES);
        for (width, height) in sizes {
            let (_, column) = testing::focused_column(&mut over, Focus::Repos, width, height);
            assert!(
                column.contains("↳ 359.9 GB-h, 2026-09"),
                "the newest month's storage detail is missing at {width}x{height}:\n{column}"
            );
            assert!(
                !column.contains("↳ 0.0") && !column.contains("2026-08"),
                "a detail line for nothing, or for an older month, at {width}x{height}:\n{column}"
            );
            assert!(
                column.contains("⚠ 10.7 Go"),
                "ceiling mark or its figure missing at {width}x{height}:\n{column}"
            );

            let (_, column) = testing::focused_column(&mut at, Focus::Repos, width, height);
            assert!(
                !column.contains('⚠') && column.contains("10.7 Go"),
                "a cache at exactly 10 GiB is marked, or its figure is gone, at \
                 {width}x{height}:\n{column}"
            );
        }
    }
}
