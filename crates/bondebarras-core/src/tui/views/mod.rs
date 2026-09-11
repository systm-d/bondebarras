//! Rendering. Layout mirrors claudine: header, body, status line, footer,
//! with modals drawn on top conditionally — plus, while a purge, an archive
//! or a load runs, a progress row between the status line and the footer
//! (`progress`).
//!
//! The body of `View::Orgs` is three columns — the orgs, the current org's
//! repositories, the loaded repository's resources — and a narrow terminal
//! drops them from the left (`columns_for`), never the resources column.

pub mod billing;
pub mod confirm;
pub mod gauges;
pub mod orgs;
pub mod progress;
pub mod repo;
pub mod repos;
pub mod shown;

use crate::clean::Plan;
use crate::tui::app::{App, Focus, View};
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

/// No selection, no `d`, nothing destructive: the Billing tab is strictly
/// diagnostic, and its footer must not advertise a key it does not act on.
const FOOTER_BILLING: &str = " [←/→] mois  [b] orgs  [q] quitter";

/// How `View::Orgs`'s footer opens, in every column and at every width.
///
/// Spec §1, the defect the columns exist to fix: the footer used to list
/// only actions, so `Tab`, the arrows and `Entrée` all worked and none was
/// announced — the user saw the orgs and could not find how to reach a
/// repository.
const FOOTER_MOVES: &str = " [←/→] colonne  [↑/↓] ligne";

/// How `View::Orgs`'s footer closes, in every column and at every width.
const FOOTER_QUIT: &str = "  [q] quitter";

/// The actions `View::Orgs`'s footer offers in each column, in the order it
/// keeps them when the width runs short. Only keys `tui::event_loop`
/// actually handles: a footer promising a key that does nothing is the same
/// defect as one hiding a key that works.
///
/// `[Entrée] charger` leads the orgs and repos columns: it loads the
/// repository under the repos cursor at once, where resting the cursor there
/// waits for the pause first (spec §3, `App::follow_cursor`). `[d]` keeps
/// Finding 8 of the v0.5 final review: it archives from the repos column
/// (`App::take_focused_plan`) and deletes from the other two, and each
/// label says which.
fn column_actions(focus: Focus) -> &'static [&'static str] {
    match focus {
        Focus::Orgs => &["[Entrée] charger", "[d] supprimer", "[b] billing"],
        Focus::Repos => &[
            "[Entrée] charger",
            "[espace] cocher",
            "[d] archiver",
            "[b] billing",
        ],
        Focus::Resources => &[
            "[espace] cocher",
            "[A] tout ⚑",
            "[d] supprimer",
            "[f] filtrer",
            "[s] trier",
            "[b] billing",
        ],
    }
}

/// `View::Orgs`'s footer for the column `focus` is on, fitted to `width`.
///
/// Always opens with the movement keys and closes with `[q] quitter`: they
/// are the fix for spec §1's defect, and a footer that clipped them off the
/// right edge would bring it back on exactly the narrow terminals that also
/// hide columns. The column's actions fill the space between, in
/// `column_actions`' order, and the first one that does not fit ends the
/// list — a fixed prefix, so a key never vanishes while a later one stays.
/// Below the movement keys and `[q] quitter` themselves (40 cells) the
/// footer clips like any other line.
fn footer_orgs(focus: Focus, width: u16) -> String {
    let budget = usize::from(width);
    let mut footer = String::from(FOOTER_MOVES);
    for action in column_actions(focus) {
        if cells(&footer) + 2 + cells(action) + cells(FOOTER_QUIT) > budget {
            break;
        }
        footer.push_str("  ");
        footer.push_str(action);
    }
    footer.push_str(FOOTER_QUIT);
    footer
}

/// How many terminal cells `text` takes, measured the way ratatui lays it
/// out — for text fitted to a width before it is drawn.
pub(crate) fn cells(text: &str) -> usize {
    Span::raw(text).width()
}

/// How many columns `View::Orgs` draws side by side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Columns {
    Three,
    Two,
    One,
}

/// How many columns fit in a terminal `width` cells wide.
///
/// Derived from the columns' own widths rather than set by hand: three once
/// the orgs column, the repos column and the resources column's minimum all
/// fit; two once the repos column — the wider of the two that can sit on
/// the left — and that minimum do; one below. The resources column keeps
/// `repo::MIN_WIDTH` whenever it shares the screen: it is where deletion
/// happens, so it is never the column a narrow terminal squeezes or drops.
pub fn columns_for(width: u16) -> Columns {
    if width >= orgs::COLUMN_WIDTH + repos::COLUMN_WIDTH + repo::MIN_WIDTH {
        Columns::Three
    } else if width >= repos::COLUMN_WIDTH + repo::MIN_WIDTH {
        Columns::Two
    } else {
        Columns::One
    }
}

/// The rows every frame is cut into.
pub(crate) struct Screen {
    pub header: Rect,
    pub body: Rect,
    pub status: Rect,
    /// The progress row (`progress`): one line while a purge, an archive or
    /// a repository load runs, none otherwise.
    pub progress: Rect,
    pub footer: Rect,
}

/// The rows a frame needs before the progress row gets a line: the header,
/// one line of body, the status line, the progress row and the footer.
const ROWS_WITH_PROGRESS: u16 = 5;

/// Cuts `area` into the header, body, status, progress and footer rows —
/// decided here once, for `render` and the render tests alike.
///
/// The progress row is a row of its own, between the status line and the
/// footer, and only `with_progress`: at `Length(0)` ratatui draws nothing, so
/// the row does not exist while nothing runs. Its line comes out of the
/// body's, never the status line's nor the footer's — see `progress`'s own
/// doc comment for why both must stay. A frame shorter than
/// `ROWS_WITH_PROGRESS` has no body line left to give, and ratatui's solver
/// would take the status line's instead: there the bar gets no line at all.
pub(crate) fn screen(area: Rect, with_progress: bool) -> Screen {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(u16::from(
            with_progress && area.height >= ROWS_WITH_PROGRESS,
        )),
        Constraint::Length(1),
    ])
    .split(area);
    Screen {
        header: rows[0],
        body: rows[1],
        status: rows[2],
        progress: rows[3],
        footer: rows[4],
    }
}

/// Where each column of `View::Orgs` lands; `None` for a column the width
/// hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ColumnAreas {
    pub orgs: Option<Rect>,
    pub repos: Option<Rect>,
    pub resources: Option<Rect>,
}

/// Lays the columns out across `body` for the column `focus` is on.
///
/// `body` spans the whole frame's width (only rows are cut above it), so
/// `columns_for(body.width)` is `columns_for` of the terminal's width.
///
/// - Three columns: all of them.
/// - Two: the resources column, and on its left the focused column — the
///   repos column, unless focus is on the orgs. Pinning the left column to
///   the repos would leave the orgs unreachable on an 80-column terminal.
/// - One: the focused column alone, so `←`/`→` change which column is on
///   screen.
pub(crate) fn column_areas(body: Rect, focus: Focus) -> ColumnAreas {
    let hidden = ColumnAreas {
        orgs: None,
        repos: None,
        resources: None,
    };
    match columns_for(body.width) {
        Columns::Three => {
            let c = Layout::horizontal([
                Constraint::Length(orgs::COLUMN_WIDTH),
                Constraint::Length(repos::COLUMN_WIDTH),
                Constraint::Min(repo::MIN_WIDTH),
            ])
            .split(body);
            ColumnAreas {
                orgs: Some(c[0]),
                repos: Some(c[1]),
                resources: Some(c[2]),
            }
        }
        Columns::Two => {
            let left = match focus {
                Focus::Orgs => orgs::COLUMN_WIDTH,
                Focus::Repos | Focus::Resources => repos::COLUMN_WIDTH,
            };
            let c =
                Layout::horizontal([Constraint::Length(left), Constraint::Min(repo::MIN_WIDTH)])
                    .split(body);
            match focus {
                Focus::Orgs => ColumnAreas {
                    orgs: Some(c[0]),
                    resources: Some(c[1]),
                    ..hidden
                },
                Focus::Repos | Focus::Resources => ColumnAreas {
                    repos: Some(c[0]),
                    resources: Some(c[1]),
                    ..hidden
                },
            }
        }
        Columns::One => match focus {
            Focus::Orgs => ColumnAreas {
                orgs: Some(body),
                ..hidden
            },
            Focus::Repos => ColumnAreas {
                repos: Some(body),
                ..hidden
            },
            Focus::Resources => ColumnAreas {
                resources: Some(body),
                ..hidden
            },
        },
    }
}

/// A column's frame: a bordered block whose border takes the primary colour
/// while the keyboard drives that column.
///
/// Spec §1's second half: with the repositories indented under their org in
/// one tree, nothing but the highlight's colour told "on an org" from "on a
/// repository". Titled columns fix the shape; lighting the focused one's
/// border says which of them the arrows move in.
pub(crate) fn column_block<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(if focused {
            theme::title_style()
        } else {
            theme::border_style()
        })
}

/// The cursor row's highlight: the full selection in the focused column,
/// the primary colour alone in the others — they still show which org and
/// which repository the screen belongs to, without competing with the row
/// the arrows actually move.
pub(crate) fn cursor_style(focused: bool) -> Style {
    if focused {
        theme::selection_style()
    } else {
        theme::title_style()
    }
}

/// `text` cut or padded to exactly `width` characters, ending in `…` when
/// cut — for the names and labels in the columns' rows. Counts characters,
/// as every row of this TUI always has.
pub(crate) fn fit(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        format!("{text:<width$}")
    } else if width == 0 {
        String::new()
    } else {
        let head: String = chars[..width - 1].iter().collect();
        format!("{head}…")
    }
}

pub fn render(app: &mut App, f: &mut Frame, pending: Option<&Plan>) {
    let rows = screen(f.area(), progress::shown(app).is_some());
    let columns = match app.view {
        View::Orgs => Some(column_areas(rows.body, app.focus)),
        View::Billing => None,
    };

    f.render_widget(
        Paragraph::new(Span::styled(
            header_text(app, columns.as_ref()),
            theme::title_style(),
        )),
        rows.header,
    );

    match columns {
        Some(columns) => {
            if let Some(area) = columns.orgs {
                orgs::render(app, f, area);
            }
            if let Some(area) = columns.repos {
                repos::render(app, f, area);
            }
            if let Some(area) = columns.resources {
                repo::render(app, f, area);
            }
        }
        None => billing::render(app, f, rows.body),
    }

    f.render_widget(
        Paragraph::new(Span::styled(status_line(app), theme::muted())),
        rows.status,
    );
    let footer = match app.view {
        View::Orgs => footer_orgs(app.focus, rows.footer.width),
        View::Billing => FOOTER_BILLING.to_string(),
    };
    f.render_widget(
        Paragraph::new(Span::styled(footer, theme::muted())),
        rows.footer,
    );
    if let Some(work) = progress::shown(app) {
        progress::render(work, f, rows.progress);
    }

    if let Some(plan) = pending {
        confirm::render(plan, f, f.area());
    }
}

/// The header row: the tab, and — when the width hides a column a visible
/// one depends on — what that column held (`hidden_context`).
fn header_text(app: &App, columns: Option<&ColumnAreas>) -> String {
    let tab = match app.view {
        View::Orgs => "Orgs",
        View::Billing => "Billing",
    };
    let orgs = app.orgs.len();
    match columns.and_then(|c| hidden_context(app, c)) {
        Some(context) => format!(" bondebarras · {orgs} orgs · {tab} · {context} "),
        None => format!(" bondebarras · {orgs} orgs · {tab} "),
    }
}

/// What a hidden column held, for the header to recall.
///
/// - The orgs column gone while the repos column shows: the org under the
///   cursor, whose repositories those are.
/// - The repos column gone while the resources column shows: the repository
///   the resources column names — `shown::subject`, the same identity the
///   column draws on its first lines, whether its listing is shown, on its
///   way or failed. Reading `app.loaded` instead recalled nothing while a
///   listing was on its way, since `loaded` is `None` from the moment the
///   cursor leaves a repository (ruling F4, 2026-09-11).
/// - Otherwise nothing: all three columns are on screen, or the orgs column
///   is alone and depends on nothing hidden.
fn hidden_context(app: &App, columns: &ColumnAreas) -> Option<String> {
    if columns.orgs.is_none() && columns.repos.is_some() {
        app.orgs.get(app.org_cursor).map(|org| org.login.clone())
    } else if columns.repos.is_none() && columns.resources.is_some() {
        let shown = app.shown();
        shown::subject(&shown).map(|(org, repo)| format!("{org}/{repo}"))
    } else {
        None
    }
}

/// What the status row shows.
///
/// Split out from `render` so it can be asserted on without a terminal, and
/// because the priority itself is the fix: `app.status` carries one-shot
/// messages — per-item deletion errors (`Erreur : suppression de {id} —
/// {reason}`) and the « Bon débarras ! » recap — that a filter left typed in
/// must not permanently bury, so it outranks a passive filter indicator.
///
/// But active typing outranks even that: `app.status` is only ever cleared
/// when a clean listing replaces the warning a load left there
/// (`App::finish_loading`), so after any purge, cancel, or error it stays
/// set indefinitely. Giving it priority unconditionally made every
/// keystroke into the filter invisible — no text, no cursor — the moment any
/// status message was pending, even though the keystrokes were still
/// filtering the list underneath. `filter_mode` is checked first so the
/// field the user is looking at is always the one they are typing into.
fn status_line(app: &App) -> String {
    if app.filter_mode {
        format!(" filtre : {}▏", app.filter)
    } else if !app.status.is_empty() {
        app.status.clone()
    } else if !app.filter.is_empty() {
        format!(" filtre : {}", app.filter)
    } else {
        String::new()
    }
}

#[cfg(test)]
pub(crate) mod testing {
    //! Render-test support for the three column modules: draw the real
    //! `render`, then read back only the cells of the rect a region was
    //! given — never the whole frame, where a column drawn across the full
    //! width or at the wrong offset would still be found.

    use super::{ColumnAreas, Screen, column_areas, progress, render, screen};
    use crate::tui::app::{App, Focus};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    /// Draws the real `views::render` — header, columns, status line,
    /// progress row, footer — on a `width` × `height` test terminal.
    pub(crate) fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
        let backend = ratatui::backend::TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| render(app, f, None)).unwrap();
        terminal.backend().buffer().clone()
    }

    /// The rects `render` draws into at this size, from the same two
    /// functions it calls.
    pub(crate) fn layout(app: &App, width: u16, height: u16) -> (Screen, ColumnAreas) {
        let rows = screen(
            Rect::new(0, 0, width, height),
            progress::shown(app).is_some(),
        );
        let columns = column_areas(rows.body, app.focus);
        (rows, columns)
    }

    /// The text of the cells inside `rect`, one line per row.
    pub(crate) fn text_in(buf: &Buffer, rect: Rect) -> String {
        (rect.top()..rect.bottom())
            .map(|y| {
                (rect.left()..rect.right())
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Puts focus on `focus`, draws the real `render`, and returns the rect
    /// that column was given — the focused column is on screen in every
    /// layout — with the text inside it.
    pub(crate) fn focused_column(
        app: &mut App,
        focus: Focus,
        width: u16,
        height: u16,
    ) -> (Rect, String) {
        app.focus = focus;
        let buf = draw(app, width, height);
        let (_, columns) = layout(app, width, height);
        let rect = match focus {
            Focus::Orgs => columns.orgs,
            Focus::Repos => columns.repos,
            Focus::Resources => columns.resources,
        }
        .expect("the focused column is on screen in every layout");
        (rect, text_in(&buf, rect))
    }

    /// A column's text as prose: each row stripped of its two side borders
    /// and its padding, blank rows dropped, rows joined by single spaces.
    /// A sentence `render` broke across rows at spaces reads back whole; one
    /// clipped at a border — or split inside a word — does not.
    pub(crate) fn unwrapped(column: &str) -> String {
        column
            .lines()
            .map(|row| {
                let cells: Vec<char> = row.chars().collect();
                let inside: String = match cells.len() {
                    0..=2 => String::new(),
                    n => cells[1..n - 1].iter().collect(),
                };
                inside.trim().to_string()
            })
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OrgSummary, RepoSummary, Resource, ResourceKind};
    use crate::repos::RepoClass;

    /// Locks finding 3: a filter left typed in (but no longer being typed
    /// into) must not hide per-item deletion errors or the purge recap, both
    /// of which are written to `app.status`. On the old priority (filter
    /// first, status as fallback) this returns the filter string instead.
    #[test]
    fn status_takes_priority_over_a_passive_filter() {
        let mut app = App::new(vec![]);
        app.filter = "linux".into();
        app.status = "Erreur : suppression de 9 — 404".into();

        assert_eq!(status_line(&app), "Erreur : suppression de 9 — 404");
    }

    #[test]
    fn the_filter_shows_once_status_is_empty() {
        let mut app = App::new(vec![]);
        app.filter = "linux".into();

        assert_eq!(status_line(&app), " filtre : linux");
    }

    /// Locks the re-review's residual 1: `app.status` is only ever cleared on
    /// a successful `Enter` load, so after any purge, cancel, or error it
    /// stays set indefinitely. Giving it priority over the filter
    /// unconditionally made every keystroke typed into the filter invisible
    /// — no text, no cursor — while it was still filtering the list
    /// underneath. On the priority this locks in, active typing must win
    /// even over a pending status message.
    #[test]
    fn active_typing_outranks_a_pending_status() {
        let mut app = App::new(vec![]);
        app.status = "Bon débarras ! 3.0 Go libérés.".into();
        app.filter_mode = true;
        app.filter = "lin".into();

        assert_eq!(status_line(&app), " filtre : lin▏");
    }

    /// Both halves of the name are asserted here — not split across sibling
    /// tests — so an implementation that always appends the cursor mark
    /// fails on this test alone, rather than only via a neighbour.
    #[test]
    fn the_typing_cursor_only_appears_in_filter_mode() {
        let mut app = App::new(vec![]);
        app.filter = "linux".into();

        app.filter_mode = true;
        assert_eq!(status_line(&app), " filtre : linux▏");

        app.filter_mode = false;
        assert_eq!(status_line(&app), " filtre : linux");
    }

    /// Finding 8 of the v0.5 final review: `d` archives while focus is on
    /// the repos column (`App::take_focused_plan`'s own doc comment) — the
    /// footer must say "archiver" there, not the "supprimer" every other
    /// column still gets right. Wide enough for every action to fit.
    #[test]
    fn footer_orgs_says_archiver_while_focus_is_on_the_repos_column() {
        let f = footer_orgs(Focus::Repos, u16::MAX);
        assert!(f.contains("[d] archiver"), "got: {f}");
        assert!(!f.contains("supprimer"), "got: {f}");
    }

    /// The negative case, for both other columns: `take_focused_plan` falls
    /// back to an ordinary resource deletion everywhere `d` cannot archive
    /// from, so the footer must keep that wording there, not the new one.
    #[test]
    fn footer_orgs_says_supprimer_everywhere_d_cannot_archive_from() {
        for focus in [Focus::Orgs, Focus::Resources] {
            let f = footer_orgs(focus, u16::MAX);
            assert!(f.contains("[d] supprimer"), "got: {f} for {focus:?}");
            assert!(!f.contains("archiver"), "got: {f} for {focus:?}");
        }
    }

    /// Spec §1's defect was a footer that announced no movement key. For
    /// each column, at every width the render sweeps cover: both movement
    /// keys open the footer, `[q] quitter` closes it, it never runs past the
    /// edge, it keeps at least the column's first action, and the actions
    /// it keeps are an in-order prefix of the column's list — a key never
    /// vanishes while a later one stays. At 200, every action fits.
    #[test]
    fn every_column_footer_keeps_its_movement_keys_and_quitter_at_every_width() {
        for focus in [Focus::Orgs, Focus::Repos, Focus::Resources] {
            let actions = column_actions(focus);
            for width in 60..=200u16 {
                let footer = footer_orgs(focus, width);
                assert!(
                    footer.starts_with(" [←/→] colonne  [↑/↓] ligne"),
                    "{focus:?} at width {width}: {footer}"
                );
                assert!(
                    footer.ends_with("  [q] quitter"),
                    "{focus:?} at width {width}: {footer}"
                );
                assert!(
                    Span::raw(footer.as_str()).width() <= usize::from(width),
                    "{focus:?}'s footer runs past the edge at width {width}: {footer}"
                );
                let kept: Vec<&str> = actions
                    .iter()
                    .copied()
                    .filter(|a| footer.contains(a))
                    .collect();
                assert!(
                    !kept.is_empty(),
                    "{focus:?} kept no action at width {width}: {footer}"
                );
                assert_eq!(
                    kept.as_slice(),
                    &actions[..kept.len()],
                    "{focus:?}'s actions out of order at width {width}: {footer}"
                );
            }
            let widest = footer_orgs(focus, 200);
            assert!(
                actions.iter().all(|a| widest.contains(a)),
                "{focus:?} dropped an action at width 200: {widest}"
            );
        }
    }

    /// Amendement 2: the thresholds are derived from the column widths —
    /// `22 + 38 + 40` for three, `38 + 40` for two — so a terminal of 80
    /// keeps two columns, and the boundaries below sit exactly on them.
    #[test]
    fn the_layout_degrades_from_the_left() {
        // The resources column is where deletion happens; it is never the one
        // dropped. Context is recalled in the header instead.
        assert_eq!(columns_for(120), Columns::Three);
        assert_eq!(columns_for(100), Columns::Three);
        assert_eq!(columns_for(99), Columns::Two);
        assert_eq!(columns_for(78), Columns::Two);
        assert_eq!(columns_for(77), Columns::One);
        assert_eq!(columns_for(40), Columns::One);
    }

    fn repo_summary(name: &str) -> RepoSummary {
        RepoSummary {
            name: name.into(),
            cache_bytes: 0,
            cache_count: 0,
            private: false,
            age_days: 5,
            class: RepoClass::Archivable,
        }
    }

    /// One org holding one repository, loaded, with one resource — a cache a
    /// closed pull request left behind, so its row carries both a size
    /// (`273.7 Mo`) and a flag (`PR#32 ⚑`) that must survive the narrowest
    /// resources column. No fixture string contains a column title.
    fn loaded_app() -> App {
        let mut app = App::new(vec![OrgSummary {
            login: "systm-d".into(),
            cache_bytes: 273_678_336,
            cache_count: 1,
            repos: vec![repo_summary("josephine")],
            billing: None,
        }]);
        app.loaded = Some(("systm-d".into(), "josephine".into()));
        app.resources = vec![Resource {
            kind: ResourceKind::Cache,
            id: 1,
            label: "v0-rust-coverage-Linux-x64".into(),
            size_bytes: 273_678_336,
            age_days: 40,
            git_ref: Some("refs/pull/32/merge".into()),
            stale_pr: true,
            protected: false,
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        }];
        app
    }

    /// Spec §7's render line, swept rather than sampled: at every width from
    /// 60 to 200 — focus on the resources, so their column is on screen in
    /// all three layouts — the resources column is drawn in the `Rect` the
    /// layout gives it, its row keeps its size and flag, the orgs and repos
    /// columns leave from the left at exactly the derived thresholds, and
    /// the footer keeps its movement keys and `[q] quitter`.
    ///
    /// Each region is read back from the rect `render` was meant to draw it
    /// in (`testing::text_in`), never from the whole buffer: a column drawn
    /// across the full frame, or at the wrong offset, would still put
    /// "RESSOURCES" somewhere on screen.
    #[test]
    fn the_resources_column_survives_every_width() {
        // Three tests at three sampled sizes is how this project shipped a
        // modal whose prompt vanished at exactly one height per width.
        let mut app = loaded_app();
        app.focus = Focus::Resources;
        for width in 60..=200u16 {
            let buf = testing::draw(&mut app, width, 30);
            let (rows, columns) = testing::layout(&app, width, 30);

            let Some(resources) = columns.resources else {
                panic!("resources column missing at width {width}");
            };
            let column = testing::text_in(&buf, resources);
            assert!(
                column.contains("RESSOURCES"),
                "resources column missing at width {width}:\n{column}"
            );
            assert!(
                column.contains("273.7 Mo") && column.contains("PR#32 ⚑"),
                "a resource row lost its size or its flag at width {width}:\n{column}"
            );

            // (orgs shown, repos shown, resources width), by the thresholds
            // Amendement 2 derives: 22 + 38 + 40 and 38 + 40.
            let expected = match width {
                100.. => (true, true, width - 22 - 38),
                78..=99 => (false, true, width - 38),
                _ => (false, false, width),
            };
            let body = testing::text_in(&buf, rows.body);
            assert_eq!(
                (
                    body.contains("ORGS"),
                    body.contains("DÉPÔTS"),
                    resources.width
                ),
                expected,
                "(orgs shown, repos shown, resources width) at width {width}"
            );

            let footer = testing::text_in(&buf, rows.footer);
            for key in ["[←/→] colonne", "[↑/↓] ligne", "[q] quitter"] {
                assert!(
                    footer.contains(key),
                    "the footer lost {key} at width {width}: {footer}"
                );
            }
        }
    }

    /// Amendement 2's ruling 3: in one-column mode only the active column is
    /// drawn, and in two-column mode the left column follows focus — orgs +
    /// resources while focus is on the orgs, repos + resources otherwise.
    /// Swept for each focus over every width. Read from the whole body on
    /// purpose: the property is as much which column titles are absent as
    /// which are present.
    #[test]
    fn the_active_column_is_the_one_shown_at_every_width() {
        for focus in [Focus::Orgs, Focus::Repos, Focus::Resources] {
            let mut app = loaded_app();
            app.focus = focus;
            for width in 60..=200u16 {
                let buf = testing::draw(&mut app, width, 30);
                let (rows, _) = testing::layout(&app, width, 30);
                let body = testing::text_in(&buf, rows.body);
                let shown = [
                    body.contains("ORGS"),
                    body.contains("DÉPÔTS"),
                    body.contains("RESSOURCES"),
                ];
                let expected = match (width, focus) {
                    (100.., _) => [true, true, true],
                    (78..=99, Focus::Orgs) => [true, false, true],
                    (78..=99, _) => [false, true, true],
                    (_, Focus::Orgs) => [true, false, false],
                    (_, Focus::Repos) => [false, true, false],
                    (_, Focus::Resources) => [false, false, true],
                };
                assert_eq!(
                    shown, expected,
                    "[ORGS, DÉPÔTS, RESSOURCES] at width {width}, focus on {focus:?}"
                );
            }
        }
    }

    /// When the width hides a column, the header says what it held: the org
    /// whose repositories the repos column lists, once the orgs column is
    /// gone; the repository the resources column names, once the repos
    /// column is gone; nothing when no visible column depends on a hidden
    /// one.
    ///
    /// Built on states the event loop produces (ruling F4, 2026-09-11), not
    /// on a hand-set `loaded`: josephine's listing is on screen, then the org
    /// cursor moves to exec-d, whose lokiprint the resources column names —
    /// first during the pause, then with lokiprint's load in flight. In both
    /// `loaded` is `None`: a header reading it recalls nothing, and one that
    /// kept the last listing recalls josephine. The org recall and the
    /// repository recall are told apart by their segment, ` · exec-d ` alone
    /// or `exec-d/lokiprint`. Swept over every width, for each focus.
    #[test]
    fn the_header_recalls_what_the_hidden_columns_held() {
        for focus in [Focus::Orgs, Focus::Repos, Focus::Resources] {
            let mut app = loaded_app();
            app.orgs.push(OrgSummary {
                login: "exec-d".into(),
                cache_bytes: 0,
                cache_count: 0,
                repos: vec![repo_summary("lokiprint")],
                billing: None,
            });
            let t0 = std::time::Instant::now();
            let listing = app.resources.clone();
            app.remember(("systm-d".into(), "josephine".into()), listing);
            assert!(app.follow_cursor(t0).is_none());
            assert_eq!(app.loaded, Some(("systm-d".into(), "josephine".into())));

            app.org_cursor = 1;
            app.reset_scoped_cursors();
            let pause = std::time::Duration::from_millis(100);
            assert!(app.follow_cursor(t0 + pause).is_none());
            sweep_the_header(&mut app, focus, "during the pause");

            let in_flight = std::time::Duration::from_millis(400);
            assert!(
                app.follow_cursor(t0 + in_flight).is_some(),
                "lokiprint's load starts"
            );
            sweep_the_header(&mut app, focus, "load in flight");
        }
    }

    /// `the_header_recalls_what_the_hidden_columns_held`'s sweep, for one
    /// focus and one state of the loop.
    fn sweep_the_header(app: &mut App, focus: Focus, phase: &str) {
        app.focus = focus;
        for width in 60..=200u16 {
            let buf = testing::draw(app, width, 30);
            let (rows, _) = testing::layout(app, width, 30);
            let header = testing::text_in(&buf, rows.header);
            // (the cursor's org recalled, the named repository recalled)
            let expected = match (width, focus) {
                (100.., _) => (false, false),
                (78..=99, Focus::Orgs) => (false, true),
                (78..=99, _) => (true, false),
                (_, Focus::Orgs) => (false, false),
                (_, Focus::Repos) => (true, false),
                (_, Focus::Resources) => (false, true),
            };
            assert_eq!(
                (
                    header.contains("· exec-d "),
                    header.contains("exec-d/lokiprint")
                ),
                expected,
                "{phase}: header at width {width}, focus on {focus:?}: {header}"
            );
            assert!(
                !header.contains("josephine"),
                "{phase}: the header recalls the repository the cursor left at width \
                 {width}, focus on {focus:?}: {header}"
            );
        }
    }

    /// The other half of spec §1's defect: nothing but the highlight's
    /// colour told "on an org" from "on a repository". Each column's border
    /// is drawn in the primary colour while the keyboard drives it and in
    /// the plain border colour otherwise — read off the top-left corner of
    /// the rect each column was given, for each focus. One width: the
    /// colour does not depend on it, and the sweeps above cover the rects.
    #[test]
    fn only_the_focused_column_wears_the_primary_border() {
        for focus in [Focus::Orgs, Focus::Repos, Focus::Resources] {
            let mut app = loaded_app();
            app.focus = focus;
            let buf = testing::draw(&mut app, 120, 30);
            let (_, columns) = testing::layout(&app, 120, 30);
            for (column, rect) in [
                (Focus::Orgs, columns.orgs),
                (Focus::Repos, columns.repos),
                (Focus::Resources, columns.resources),
            ] {
                let rect = rect.expect("three columns fit in 120");
                let expected = if column == focus {
                    theme::PRIMARY
                } else {
                    theme::BORDER
                };
                assert_eq!(
                    buf[(rect.x, rect.y)].fg,
                    expected,
                    "{column:?}'s border, focus on {focus:?}"
                );
            }
        }
    }
}
