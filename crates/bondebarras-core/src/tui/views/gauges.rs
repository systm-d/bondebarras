//! Two per-repository gauges: Actions cache usage against GitHub's
//! documented (but API-unexposed) per-repository ceiling, and Actions
//! minutes against the free monthly allowance.
//!
//! Drawn at the head of the resources column (`tui::views::repo`), for the
//! repository its resources were loaded from.

use crate::billing::FREE_MINUTES_PER_MONTH;
use crate::model::human_size;
use crate::tui::theme;
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
/// constant: the cache ceiling is a compile-time constant today, but the
/// minutes ceiling already reads from `billing::FREE_MINUTES_PER_MONTH`, and
/// issue #11 will replace that single source with a per-plan figure that can
/// legitimately be unknown (ceiling zero).
///
/// `pub(crate)`, not private: `views::billing::gauge_line` shares this exact
/// business rule (an uncapped, zero-guarded percentage) rather than keeping
/// its own copy of the same formula — the two gauges here and the Billing
/// tab's gauge would otherwise need to change in lockstep with no single
/// source of truth. This is also where the module's zero-ceiling test
/// already lives, so both call sites stay covered by one test rather than
/// two that could drift apart.
pub(crate) fn percent(used: u64, ceiling: u64) -> u64 {
    if ceiling == 0 {
        0
    } else {
        (used as f64 / ceiling as f64 * 100.0).round() as u64
    }
}

/// Width of the filled portion of the bar, in cells.
///
/// Capped independently of `percent` (which is never capped) so a gauge well
/// past its ceiling still draws a legible, full-looking bar instead of one
/// that would need hundreds of cells. Also capped by the available `width`,
/// scaled down from the same 20-cell maximum `views::billing::gauge_line`
/// uses, so the bar never pushes the percentage or the eviction warning off
/// the edge of a narrow pane.
fn bar(percent: u64, width: u16) -> String {
    let capacity = (width as usize).saturating_sub(30).clamp(4, 20);
    let filled = ((percent as usize) * capacity / 100).min(capacity);
    "█".repeat(filled)
}

/// Cache usage against GitHub's documented, hardcoded, 10 GiB per-repository
/// ceiling.
///
/// Never clamped at 100 %: past it, GitHub itself is already evicting the
/// least-recently-read caches — including the default branch's — to make
/// room for closed pull requests' still-warm ones. Clamping the number would
/// hide exactly the fact this gauge exists to show.
///
/// The bar and the percentage come right after the label, before the byte
/// count and the "not exposed by the API" caveat: at the narrowest width
/// this pane renders at, a `Line` that overflows clips from the right, and
/// the percentage — the one figure this gauge exists to show — must survive
/// that clip even when the trailing explanation does not.
pub fn cache_gauge_line(used: u64, width: u16) -> Vec<Line<'static>> {
    let pct = percent(used, CACHE_CEILING_BYTES);
    let mut lines = vec![Line::from(Span::styled(
        format!(
            "Cache   {}  {pct:>3} %   {} / 10 Gio (plafond GitHub, non exposé par l'API)",
            bar(pct, width),
            human_size(used),
        ),
        theme::text_style(),
    ))];
    if pct > 100 {
        lines.push(Line::from(Span::styled(
            "  ⚠ évince : GitHub supprime déjà les caches les moins récemment lus, y \
             compris ceux de la branche par défaut, au profit des PR fermées"
                .to_string(),
            theme::status_warn(),
        )));
    }
    lines
}

/// Actions minutes against the free monthly allowance
/// (`billing::FREE_MINUTES_PER_MONTH`).
///
/// A public repository's Actions runs are free and unlimited — GitHub's own
/// billing report never even lists them against the allowance (see
/// `billing::BillingReport::included_minutes`) — so it always reads 0 %, but
/// with the reason spelled out: a bare 0 % would otherwise read as
/// comfortable headroom, when it actually means this repository cannot
/// consume the allowance at all.
pub fn minutes_gauge_line(used: u64, is_public: bool, width: u16) -> Vec<Line<'static>> {
    if is_public {
        return vec![Line::from(Span::styled(
            "Minutes    0 % (dépôt public : minutes Actions gratuites et illimitées, \
             hors plafond)"
                .to_string(),
            theme::muted(),
        ))];
    }
    let pct = percent(used, FREE_MINUTES_PER_MONTH);
    vec![Line::from(Span::styled(
        format!(
            "Minutes {}  {pct:>3} %   {used} / {FREE_MINUTES_PER_MONTH}",
            bar(pct, width),
        ),
        theme::text_style(),
    ))]
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
        let line = text(&minutes_gauge_line(0, true, 60));
        assert!(line.contains("public"), "got: {line}");
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
