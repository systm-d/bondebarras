//! Two per-repository gauges: Actions cache usage against GitHub's
//! documented (but API-unexposed) per-repository ceiling, and Actions
//! minutes against the allowance of the organization's plan
//! (`billing::included_minutes_for`) — with no percentage when that
//! allowance is unknown.
//!
//! Drawn at the head of the resources column (`tui::views::repo`), for the
//! repository its resources were loaded from.

use crate::model::human_size;
use crate::tui::{theme, views};
use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// GitHub's documented per-repository Actions cache ceiling: 10 GiB.
///
/// `actions/cache/usage` only ever returns the org-wide total, never a
/// per-repository ceiling — GitHub does not expose this figure through any
/// endpoint — so it is hardcoded here rather than read from a response, and
/// both gauge lines that use it say so.
pub const CACHE_CEILING_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// `used / ceiling` as a whole percentage.
///
/// A pure `(used, ceiling)` function, not `(used)` alone against a baked-in
/// constant: the cache ceiling is a compile-time constant, but the minutes
/// ceiling is the org's plan allowance (`billing::included_minutes_for`).
/// An unknown allowance never reaches here — its gauges show no percentage
/// at all — yet a zero ceiling still reads 0 %, never a division by zero.
///
/// `pub(crate)`, not private: `views::billing::gauge_line` shares this exact
/// business rule (an uncapped, zero-guarded percentage) rather than keeping
/// its own copy of the same formula — the two gauges here and the Billing
/// tab's gauge would otherwise need to change in lockstep with no single
/// source of truth. Each call site has its own zero-ceiling test:
/// `gauges::tests::a_zero_ceiling_does_not_divide_by_zero` here, and
/// `views::billing::tests::a_zero_allowance_does_not_divide_by_zero` through
/// the Billing tab's gauge.
pub(crate) fn percent(used: u64, ceiling: u64) -> u64 {
    if ceiling == 0 {
        0
    } else {
        (used as f64 / ceiling as f64 * 100.0).round() as u64
    }
}

/// Widest the filled portion of a bar gets, in cells — the same maximum
/// `views::billing::gauge_line` uses.
const BAR_CELLS: usize = 20;

/// How far in an explanation starts on a row of its own.
const INDENT: &str = "  ";

/// The cache gauge's caveat: its ceiling is GitHub's documented figure, not
/// one read from a response.
const CACHE_CAVEAT: &str = "(plafond GitHub, non exposé par l'API)";

/// What a cache gauge past 100 % adds.
const EVICTION: &str = "⚠ évince : GitHub supprime déjà les caches les moins récemment lus, y \
                        compris ceux de la branche par défaut, au profit des PR fermées";

/// Why a public repository's minutes gauge reads 0 %.
const PUBLIC_REASON: &str =
    "(dépôt public : minutes Actions gratuites et illimitées, hors plafond)";

/// Why a minutes gauge gives a total and no percentage: no allowance is
/// known for the org's plan.
const UNKNOWN_PLAN: &str = "· formule inconnue, pas de quota";

/// The filled portion of a bar, in at most `room` cells.
///
/// Capped independently of `percent` (which is never capped) so a gauge well
/// past its ceiling still draws a legible, full-looking bar instead of one
/// that would need hundreds of cells. Capped by `room` too — the cells its
/// row leaves once the figures have theirs — so the bar is the part that
/// gives in a narrow column, never the figures.
fn bar(percent: u64, room: usize) -> String {
    let capacity = room.min(BAR_CELLS);
    let filled = ((percent as usize) * capacity / 100).min(capacity);
    "█".repeat(filled)
}

/// A gauge's figures row, `width` cells at most while its figures fit: the
/// label, the bar in whatever room is left, the percentage, then `figures`
/// — used against ceiling, with their units. Only the bar shrinks.
fn figures_row(label: &str, pct: u64, figures: &str, width: u16) -> String {
    let tail = format!("  {pct:>3} %   {figures}");
    let room = usize::from(width).saturating_sub(views::cells(label) + views::cells(&tail));
    format!("{label}{}{tail}", bar(pct, room))
}

/// `text` on rows of its own, indented, broken at spaces to `width` cells
/// (`views::wrap_words`): every word on screen, none cut at the border.
fn own_rows(text: &str, style: Style, width: u16) -> Vec<Line<'static>> {
    let room = usize::from(width).saturating_sub(INDENT.len());
    views::wrap_words(text, room)
        .into_iter()
        .map(|row| Line::from(Span::styled(format!("{INDENT}{row}"), style)))
        .collect()
}

/// A figures row and its `explanation`: on the same row when the whole row
/// fits in `width`, otherwise the figures alone and the explanation on rows
/// of its own under them — the rule `views::repo::column_head` follows for
/// the size explanation (ruling B). Never clipped either way.
fn explained(figures: String, style: Style, explanation: &str, width: u16) -> Vec<Line<'static>> {
    let beside = format!(" {explanation}");
    if views::cells(&figures) + views::cells(&beside) <= usize::from(width) {
        return vec![Line::from(vec![
            Span::styled(figures, style),
            Span::styled(beside, style),
        ])];
    }
    let mut lines = vec![Line::from(Span::styled(figures, style))];
    lines.extend(own_rows(explanation, style, width));
    lines
}

/// Cache usage against GitHub's documented, hardcoded, 10 GiB per-repository
/// ceiling, in a column `width` cells wide.
///
/// Never clamped at 100 %: past it, GitHub itself is already evicting the
/// least-recently-read caches — including the default branch's — to make
/// room for closed pull requests' still-warm ones. Clamping the number would
/// hide exactly the fact this gauge exists to show.
///
/// Nothing is clipped at any width the column is drawn at (final review
/// I4): the percentage and the `used / 10 Gio` figures stay whole on the
/// gauge's row, the bar shrinking to leave them room — cut, `12.4 Go / 10`
/// read as 124 % — and the caveat and the eviction warning go on rows of
/// their own when the row cannot hold them.
pub fn cache_gauge_line(used: u64, width: u16) -> Vec<Line<'static>> {
    let pct = percent(used, CACHE_CEILING_BYTES);
    let figures = figures_row(
        "Cache   ",
        pct,
        &format!("{} / 10 Gio", human_size(used)),
        width,
    );
    let mut lines = explained(figures, theme::text_style(), CACHE_CAVEAT, width);
    if pct > 100 {
        lines.extend(own_rows(EVICTION, theme::status_warn(), width));
    }
    lines
}

/// Actions minutes against the allowance of the org's plan
/// (`billing::included_minutes_for`), in a column `width` cells wide.
///
/// A public repository's Actions runs are free and unlimited — GitHub's own
/// billing report never even lists them against the allowance (see
/// `billing::BillingReport::included_minutes`) — so it always reads 0 %, but
/// with the reason spelled out: a bare 0 % would otherwise read as
/// comfortable headroom, when it actually means this repository cannot
/// consume the allowance at all.
///
/// With no known `allowance` — no plan read, or a plan this crate has no
/// figure for — the total is real and a percentage would be invented, so the
/// gauge gives the total and says why: the same rule as a package version's
/// size. Either explanation goes on rows of its own when the gauge's row
/// cannot hold it, never clipped.
pub fn minutes_gauge_line(
    used: u64,
    is_public: bool,
    allowance: Option<u64>,
    width: u16,
) -> Vec<Line<'static>> {
    if is_public {
        return explained(
            "Minutes    0 %".to_string(),
            theme::muted(),
            PUBLIC_REASON,
            width,
        );
    }
    let Some(allowance) = allowance else {
        return explained(
            format!("Minutes  {used} min"),
            theme::muted(),
            UNKNOWN_PLAN,
            width,
        );
    };
    let pct = percent(used, allowance);
    let figures = figures_row("Minutes ", pct, &format!("{used} / {allowance}"), width);
    vec![Line::from(Span::styled(figures, theme::text_style()))]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect()
    }

    #[test]
    fn the_cache_gauge_reports_overshoot_rather_than_capping() {
        // josephine sits at 11.5 Gio against a 10 GiB (`CACHE_CEILING_BYTES`)
        // ceiling: 12_360_000_000 / 10_737_418_240 = 115 %, not 123 % — 123 %
        // would assume a decimal 10_000_000_000 ceiling, contrary to the
        // binary constant this module actually uses. Clamping to 100 % would
        // hide the one fact the gauge exists to show: GitHub is already
        // evicting, and it evicts by least-recently-read, so it takes main's
        // caches to make room for closed PRs'.
        let line = text(&cache_gauge_line(12_360_000_000, 60));
        assert!(line.contains("115"), "got: {line}");
        assert!(line.contains("évince"), "got: {line}");
    }

    #[test]
    fn the_cache_gauge_stays_quiet_below_the_ceiling() {
        let line = text(&cache_gauge_line(4_000_000_000, 60));
        assert!(!line.contains("évince"), "got: {line}");
    }

    #[test]
    fn a_public_repo_reads_zero_with_its_reason() {
        // A bare 0 % would read as comfortable headroom. It means this
        // repository cannot consume the allowance at all.
        let line = text(&minutes_gauge_line(0, true, Some(3_000), 60));
        assert!(line.contains("public"), "got: {line}");
    }

    #[test]
    fn minutes_gauge_divides_by_the_plans_allowance() {
        // exec-d on Team: 1 004 of 3 000. Against the old 2 000, 50 %.
        let line = text(&minutes_gauge_line(1_004, false, Some(3_000), 60));
        assert!(line.contains(" 33 %"), "got: {line}");
        assert!(line.contains("1004 / 3000"), "got: {line}");
    }

    #[test]
    fn minutes_gauge_without_a_plan_shows_no_percentage() {
        let line = text(&minutes_gauge_line(1_004, false, None, 60));
        assert!(!line.contains('%'), "got: {line}");
        assert!(line.contains("formule inconnue"), "got: {line}");
    }

    /// Amended (2026-09-11, controller): the un-amended version of this test
    /// called `cache_gauge_line(0, 60)`, which pins the ceiling to the
    /// nonzero `CACHE_CEILING_BYTES` constant and only ever exercises zero
    /// *usage* — it cannot fail on the zero-*ceiling* property its own name
    /// promises, since production can never actually reach a zero ceiling
    /// through that entry point. The percentage instead goes through a pure
    /// `(used, ceiling)` helper — `percent` — shared by both gauges, and this
    /// calls it directly with a ceiling of zero: exactly the shape issue #11
    /// will hit once the minutes ceiling becomes a data-dependent per-plan
    /// figure that can legitimately be absent.
    #[test]
    fn a_zero_ceiling_does_not_divide_by_zero() {
        assert_eq!(percent(0, 0), 0);
        // A nonzero usage against a zero ceiling is the case that actually
        // divides by zero without the guard — asserting the value, not just
        // the absence of "NaN"/"inf", the way `views::billing::gauge_line`'s
        // own zero-allowance test does: a saturating float-to-int cast turns
        // both NaN and +inf into a large *finite* u64, which no substring
        // check for "NaN"/"inf" would ever catch.
        assert_eq!(percent(100, 0), 0);
    }
}
