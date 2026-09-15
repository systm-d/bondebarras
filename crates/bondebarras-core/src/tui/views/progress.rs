//! The progress row: one line between the status line and the footer, drawn
//! only while a purge, an archive or a repository load runs.
//!
//! A row of its own, never the footer's nor the status line's. The footer
//! carries the movement keys: taking them during a purge would bring back
//! spec §1's defect — a user who cannot see how to move — at the worst
//! moment. The status line carries the errors (`Erreur : suppression de 9 —
//! 404`), and a bar drawn over it would hide a failure behind the progress of
//! the purge that failed. `tui::views::screen` gives the row no line at all
//! while nothing runs.

use crate::tui::app::App;
use crate::tui::theme;
use crate::tui::views::cells;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// A purge's label.
pub(crate) const DELETION: &str = "suppression";
/// An archive's label (`clean::Plan::is_archive`): an archive is not a
/// deletion, and nothing on screen calls it one.
pub(crate) const ARCHIVE: &str = "archivage";
/// The label of a bar whose count holds deletions and an archive together —
/// see `Work::joined`.
pub(crate) const DELETION_AND_ARCHIVE: &str = "suppression et archivage";
/// A repository load's label.
pub(crate) const LOAD: &str = "chargement";

/// A unit of work in flight, and how far along it is.
///
/// Both denominators are counted, never estimated: a purge knows its item
/// count from its own `Plan`, and a repository load knows it makes exactly
/// nine calls. This project has twice published a percentage its data could
/// not support — the "818 %" of v0.2 and the "890 Mo" of v0.3 — and a bar
/// that animates without measuring anything would be the same lie in a
/// prettier shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    pub label: String,
    pub done: usize,
    pub total: usize,
}

impl Work {
    /// Nothing done yet out of `total`, labelled `label`.
    pub fn new(label: &str, total: usize) -> Work {
        Work {
            label: label.to_string(),
            done: 0,
            total,
        }
    }

    /// Ratio in 0..=1. `total == 0` yields 0, never a division by zero.
    ///
    /// Never above 1 either: the bar fills `ratio` of its cells, and a count
    /// past its total would ask for more cells than the row has. That count
    /// would be a bug, and the `done/total` written beside the bar shows it
    /// as it is; the bar stays full.
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.done as f64 / self.total as f64).min(1.0)
        }
    }

    /// This purge bar once a plan of `count` more items, labelled `label`
    /// (`DELETION` or `ARCHIVE`), is launched while it runs.
    ///
    /// Several purges can run at once, and they share one bar counting the
    /// batch since the bar appeared: a purge that finishes keeps its items in
    /// the count until the last one's `Finished` clears the bar
    /// (`App::purge_finished`). Dropping them at its end would move the bar
    /// backwards — 8/15 back to 3/10 — while work still runs.
    ///
    /// The label names everything the count holds. A deletion and an archive
    /// in one batch read `suppression et archivage`, and still do once the
    /// archive has ended: its item is still one of the counted ones, and
    /// `suppression` alone would count an archive as a deletion.
    pub(crate) fn joined(self, label: &str, count: usize) -> Work {
        Work {
            label: if self.label == label {
                self.label
            } else {
                DELETION_AND_ARCHIVE.to_string()
            },
            done: self.done,
            total: self.total + count,
        }
    }
}

/// The work the progress row shows: the purge first, the load otherwise.
///
/// Both slots can be full at once — the cursor stays free during a purge, so
/// a load can start while deletions are still landing — and one row shows
/// one of them. The purge wins: it is the one destroying data.
pub fn shown(app: &App) -> Option<&Work> {
    app.purge.as_ref().or(app.loading.as_ref())
}

/// Draws `work` across `area`: its label on the left, the bar, then
/// `done/total` written out — a percentage alone does not say whether two
/// items remain or two hundred.
///
/// The bar is drawn as cells rather than with ratatui's `Gauge`: a `Gauge`
/// centres a label on the bar, and even an empty one blanks — colours
/// swapped — the cell it is centred on once the fill passes it, a gap in the
/// middle of the bar (smoke S4). The fill runs unbroken from the bar's start
/// for `ratio` of its cells, and the track is drawn in the border colour so
/// the bar's full length shows while it is empty.
pub(crate) fn render(work: &Work, f: &mut Frame, area: Rect) {
    let label = format!(" {}  ", work.label);
    let count = format!("  {}/{} ", work.done, work.total);
    let width = |text: &str| u16::try_from(cells(text)).unwrap_or(u16::MAX);
    let [label_area, bar_area, count_area] = Layout::horizontal([
        Constraint::Length(width(&label)),
        Constraint::Min(0),
        Constraint::Length(width(&count)),
    ])
    .areas(area);

    f.render_widget(
        Paragraph::new(Span::styled(label, theme::text_style())),
        label_area,
    );
    let cells_wide = usize::from(bar_area.width);
    let filled = ((cells_wide as f64) * work.ratio()).round() as usize;
    let bar = Style::default().fg(theme::PRIMARY).bg(theme::BORDER);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("█".repeat(filled.min(cells_wide)), bar),
            Span::styled(" ".repeat(cells_wide.saturating_sub(filled)), bar),
        ])),
        bar_area,
    );
    f.render_widget(
        Paragraph::new(Span::styled(count, theme::muted())),
        count_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OrgSummary, RepoSummary};
    use crate::repos::RepoClass;
    use crate::scan::TOTAL_CALLS;
    use crate::tui::app::LOAD_PAUSE;
    use crate::tui::views::testing;
    use std::time::{Duration, Instant};

    #[test]
    fn an_empty_plan_does_not_divide_by_zero() {
        // A plan can be empty — `[A]` on a repository whose every resource
        // is protected selects nothing. 0/0 must be 0, not NaN, and NaN
        // reaches ratatui's Gauge as a panic in debug.
        let w = Work {
            label: "purge".into(),
            done: 0,
            total: 0,
        };
        assert_eq!(w.ratio(), 0.0);
    }

    #[test]
    fn a_finished_work_is_exactly_one() {
        let w = Work {
            label: "purge".into(),
            done: 9,
            total: 9,
        };
        assert_eq!(w.ratio(), 1.0);
    }

    /// A count past its total is a bug, but it must draw — full, with its
    /// real count beside it — never take the interface down with a purge
    /// still running.
    #[test]
    fn a_count_past_its_total_draws_a_full_bar_not_a_panic() {
        let w = Work {
            label: "chargement".into(),
            done: 10,
            total: 9,
        };
        assert_eq!(w.ratio(), 1.0);
        let mut app = App::new(vec![]);
        app.loading = Some(w);
        let buf = testing::draw(&mut app, 100, 12);
        let (rows, _) = testing::layout(&app, 100, 12);
        assert!(testing::text_in(&buf, rows.progress).contains("10/9"));
    }

    #[test]
    fn a_purge_outranks_a_load_that_started_under_it() {
        // The cursor stays live during a purge, so a drill-down can start
        // while deletions are still landing. With one slot the load would
        // overwrite the purge and the bar would report the wrong work as
        // finished. The fixture needs BOTH present, or it proves nothing.
        let mut app = App::new(vec![]);
        app.purge = Some(Work {
            label: "suppression".into(),
            done: 2,
            total: 7,
        });
        app.loading = Some(Work {
            label: "chargement".into(),
            done: 8,
            total: 9,
        });
        let shown = shown(&app).unwrap();
        assert_eq!(shown.label, "suppression");
        assert_eq!(shown.done, 2);
    }

    #[test]
    fn a_load_shows_when_no_purge_runs() {
        let mut app = App::new(vec![]);
        app.loading = Some(Work {
            label: "chargement".into(),
            done: 3,
            total: 9,
        });
        assert_eq!(shown(&app).unwrap().label, "chargement");
    }

    #[test]
    fn nothing_running_draws_no_row() {
        assert!(shown(&App::new(vec![])).is_none());
    }

    /// The row must appear and disappear, and taking it must never cost the
    /// footer its keys — that footer is what this whole plan exists to fix.
    ///
    /// Read from the rects `render` draws into, never the whole frame, and
    /// those rects are pinned to the bottom of the frame in order — status,
    /// progress, footer — so a bar drawn over the footer, or a footer pushed
    /// off the frame, cannot pass by being found somewhere else.
    #[test]
    fn the_progress_row_never_costs_the_footer_its_keys() {
        for height in 6u16..=40 {
            for width in [72u16, 100, 140] {
                let mut app = App::new(vec![]);
                app.purge = Some(Work {
                    label: "suppression".into(),
                    done: 1,
                    total: 4,
                });
                let buf = testing::draw(&mut app, width, height);
                let (rows, _) = testing::layout(&app, width, height);
                assert_eq!(
                    (rows.status.y, rows.progress.y, rows.footer.y),
                    (height - 3, height - 2, height - 1),
                    "status, progress and footer are not the last three rows at {width}x{height}"
                );
                assert_eq!(
                    rows.progress.height, 1,
                    "no progress row at {width}x{height}"
                );
                let footer = testing::text_in(&buf, rows.footer);
                assert!(
                    footer.contains("[q] quitter"),
                    "footer lost its keys at {width}x{height} with the bar shown: {footer}"
                );
                let bar = testing::text_in(&buf, rows.progress);
                assert!(
                    bar.contains("suppression") && bar.contains("1/4"),
                    "the bar is not drawn in its own row at {width}x{height}: {bar}"
                );
            }
        }
    }

    /// The status line carries the errors (`Erreur : suppression de 9 —
    /// 404`): a bar drawn over it would hide the failure behind the progress
    /// of the purge that failed.
    #[test]
    fn the_progress_row_never_hides_the_status_lines_error() {
        const ERROR: &str = "Erreur : suppression de 9 — 404";
        for height in 6u16..=40 {
            for width in [72u16, 100, 140] {
                let mut app = App::new(vec![]);
                app.status = ERROR.into();
                app.purge = Some(Work {
                    label: "suppression".into(),
                    done: 1,
                    total: 4,
                });
                let buf = testing::draw(&mut app, width, height);
                let (rows, _) = testing::layout(&app, width, height);
                let status = testing::text_in(&buf, rows.status);
                assert!(
                    status.contains(ERROR),
                    "the bar hid the status line's error at {width}x{height}: {status}"
                );
            }
        }
    }

    /// A frame too short for all five rows keeps the rows it had without a
    /// bar: whatever the height, the bar never takes the status line's line
    /// nor the footer's. At three rows, left to ratatui's solver, the bar
    /// took the status line's — an error on it went with it. Every height
    /// from 1, not just the ones the sweeps above start at.
    #[test]
    fn a_frame_too_short_for_the_bar_keeps_its_status_line_and_footer() {
        const ERROR: &str = "Erreur : suppression de 9 — 404";
        for height in 1u16..=40 {
            for width in [72u16, 100, 140] {
                let idle = App::new(vec![]);
                let (without, _) = testing::layout(&idle, width, height);
                let mut busy = App::new(vec![]);
                busy.status = ERROR.into();
                busy.purge = Some(Work::new("suppression", 4));
                let buf = testing::draw(&mut busy, width, height);
                let (with, _) = testing::layout(&busy, width, height);
                assert_eq!(
                    (with.status.height, with.footer.height),
                    (without.status.height, without.footer.height),
                    "the bar took a line from the status line or the footer at {width}x{height}"
                );
                if with.status.height > 0 {
                    assert!(
                        testing::text_in(&buf, with.status).contains(ERROR),
                        "the status line lost its error at {width}x{height}"
                    );
                }
            }
        }
    }

    /// Nothing running: the row takes no line at all, and the status line
    /// sits right above the footer as it did before there was a bar.
    #[test]
    fn the_progress_row_leaves_no_line_when_nothing_runs() {
        for height in 6u16..=40 {
            let mut app = App::new(vec![]);
            let buf = testing::draw(&mut app, 100, height);
            let (rows, _) = testing::layout(&app, 100, height);
            assert_eq!(rows.progress.height, 0, "a progress row at height {height}");
            assert_eq!(
                (rows.status.y, rows.footer.y),
                (height - 2, height - 1),
                "the status line left the footer's side at height {height}"
            );
            assert!(testing::text_in(&buf, rows.footer).contains("[q] quitter"));
        }
    }

    /// One org, `systm-d`, holding josephine then claudine.
    fn two_repositories() -> App {
        let repo = |name: &str| RepoSummary {
            name: name.into(),
            cache_bytes: 0,
            cache_count: 0,
            private: false,
            age_days: 5,
            class: RepoClass::Archivable,
        };
        App::new(vec![OrgSummary {
            login: "systm-d".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![repo("josephine"), repo("claudine")],
            billing: None,
        }])
    }

    /// How far the load bar has gone, as `(done, total)`.
    fn load_bar(app: &App) -> Option<(usize, usize)> {
        app.loading.as_ref().map(|w| (w.done, w.total))
    }

    /// A load's bar: none during the pause, when no call has left yet; at 0
    /// of the nine calls when the load starts; one call on per tick of that
    /// load; gone when its listing lands — whether it landed in full or
    /// failed.
    #[test]
    fn a_load_bar_counts_its_calls_from_start_to_landing() {
        let mut app = two_repositories();
        let t0 = Instant::now();
        assert!(app.follow_cursor(t0).is_none());
        assert_eq!(app.loading, None, "a bar during the pause");

        let load = app
            .follow_cursor(t0 + LOAD_PAUSE)
            .expect("josephine's load starts");
        assert_eq!(
            app.loading,
            Some(Work {
                label: "chargement".into(),
                done: 0,
                total: TOTAL_CALLS,
            })
        );
        for _ in 0..3 {
            app.load_ticked(load.generation());
        }
        assert_eq!(load_bar(&app), Some((3, TOTAL_CALLS)));
        app.land_load(load, Ok((vec![], vec![])));
        assert_eq!(app.loading, None, "the bar outlived its listing");

        let refresh = app.force_load().expect("Entrée refreshes");
        assert_eq!(load_bar(&app), Some((0, TOTAL_CALLS)));
        app.load_ticked(refresh.generation());
        app.land_load(refresh, Err(anyhow::anyhow!("503")));
        assert_eq!(app.loading, None, "the bar outlived a failed load");
    }

    /// Task 5's cancellation reaches the ticks: an abandoned load keeps
    /// ticking, and its ticks must never move the bar of the repository now
    /// looked at. Josephine's load starts and ticks; the cursor moves to
    /// claudine — josephine's bar leaves with it — and claudine's own load
    /// starts after its pause. Every tick josephine's load sends meanwhile
    /// changes nothing; claudine's own tick moves her bar.
    #[test]
    fn a_tick_from_an_abandoned_load_never_moves_the_bar_now_shown() {
        let mut app = two_repositories();
        let t0 = Instant::now();
        assert!(app.follow_cursor(t0).is_none());
        let josephine = app
            .follow_cursor(t0 + LOAD_PAUSE)
            .expect("josephine's load starts");
        app.load_ticked(josephine.generation());

        app.repo_cursor = 1;
        let left = t0 + Duration::from_millis(400);
        assert!(app.follow_cursor(left).is_none());
        app.load_ticked(josephine.generation());
        assert_eq!(
            app.loading, None,
            "josephine's bar stayed once the cursor left her"
        );

        let claudine = app
            .follow_cursor(left + LOAD_PAUSE)
            .expect("claudine's load starts");
        for _ in 0..5 {
            app.load_ticked(josephine.generation());
        }
        assert_eq!(
            load_bar(&app),
            Some((0, TOTAL_CALLS)),
            "josephine's abandoned load moved claudine's bar"
        );

        app.load_ticked(claudine.generation());
        assert_eq!(load_bar(&app), Some((1, TOTAL_CALLS)));
    }

    /// A purge's end makes a load in flight of its repository suspect, and
    /// supersedes it (`App::forget`): its bar goes with it, and its ticks do
    /// not move the fresh load's bar that follows.
    #[test]
    fn a_load_superseded_by_a_purge_takes_its_bar_with_it() {
        let mut app = two_repositories();
        let t0 = Instant::now();
        assert!(app.follow_cursor(t0).is_none());
        let suspect = app
            .follow_cursor(t0 + LOAD_PAUSE)
            .expect("josephine's load starts");

        app.forget(("systm-d", "josephine"));
        assert_eq!(app.loading, None, "a superseded load kept its bar");

        let t1 = t0 + Duration::from_millis(1000);
        assert!(app.follow_cursor(t1).is_none());
        let _fresh = app
            .follow_cursor(t1 + LOAD_PAUSE)
            .expect("a fresh load follows");
        app.load_ticked(suspect.generation());
        assert_eq!(load_bar(&app), Some((0, TOTAL_CALLS)));
    }

    /// Smoke S4: the filled part of the bar showed a blank cell in its middle
    /// (`████…████ ██████`). ratatui's `Gauge` centres its label, and an
    /// empty one still blanks, colours swapped, the cell it is centred on
    /// once the fill passes it.
    ///
    /// Swept over every width from 60 to 200 and every count of a seven-item
    /// purge, on the progress row's real rect: between the label and the
    /// count, the filled cells run unbroken from the bar's start for as many
    /// cells as the ratio gives, only track follows, and every cell of the
    /// bar keeps the track's colour behind it.
    #[test]
    fn the_progress_bar_fills_without_a_gap_across_swept_widths() {
        const TOTAL: usize = 7;
        for width in 60..=200u16 {
            for done in 0..=TOTAL {
                let mut app = App::new(vec![]);
                app.purge = Some(Work {
                    label: DELETION.into(),
                    done,
                    total: TOTAL,
                });
                let buf = testing::draw(&mut app, width, 12);
                let (rows, _) = testing::layout(&app, width, 12);
                let row = testing::text_in(&buf, rows.progress);
                let label = cells(&format!(" {DELETION}  "));
                let count = cells(&format!("  {done}/{TOTAL} "));
                let bar_width = usize::from(rows.progress.width) - label - count;
                let expected = (bar_width as f64 * done as f64 / TOTAL as f64).round() as usize;
                let bar: Vec<&ratatui::buffer::Cell> = (0..bar_width)
                    .map(|i| &buf[(rows.progress.x + (label + i) as u16, rows.progress.y)])
                    .collect();
                let filled = bar.iter().take_while(|c| c.symbol() == "█").count();
                assert_eq!(
                    filled, expected,
                    "the fill stops short of {expected} cells at width {width}, {done}/{TOTAL}: {row:?}"
                );
                assert!(
                    bar[filled..].iter().all(|c| c.symbol() == " "),
                    "a filled cell after a gap at width {width}, {done}/{TOTAL}: {row:?}"
                );
                assert!(
                    bar.iter().all(|c| c.bg == theme::BORDER),
                    "a bar cell lost the track's colour at width {width}, {done}/{TOTAL}: {row:?}"
                );
            }
        }
    }
}
