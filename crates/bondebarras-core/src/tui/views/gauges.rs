//! Two per-repository gauges: Actions cache usage against GitHub's default
//! included per-repository threshold — a cost threshold, not the
//! repository's real limit, which no endpoint exposes — and Actions
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

/// GitHub's default *included* per-repository Actions cache threshold:
/// 10 GB, decimal — 10_000_000_000 bytes, not 10 GiB.
///
/// **Not a ceiling.** An authorized administrator can raise a repository's
/// cache limit above it, and storage past it is billed rather than refused.
/// Eviction *to make room* starts only once the repository reaches its
/// *configured* limit, which can therefore sit well above this figure. And
/// independently of any limit, GitHub removes every cache entry that has
/// not been accessed in over 7 days — that rule never waits for a threshold.
///
/// The configured limit is exposed by no endpoint this crate can reach —
/// `actions/cache/usage` only ever returns the org-wide total — so the
/// included threshold is hardcoded here rather than read from a response,
/// and every line gauged against it says it is a cost threshold, not the
/// repository's known capacity.
///
/// **Decimal, deliberately.** This was `10 * 1024 * 1024 * 1024` until #19,
/// under a 2026-09-11 ruling that fixed it at 10 GiB. The controller lifted
/// that ruling with #19: it had been taken while the figure was a visual
/// marker carrying no claim about money. Now that every sentence the product
/// prints about billing hangs off this constant, the binary value
/// contradicted them — GitHub bills a repository holding 10.5 GB while
/// `cache_over_included` still answered `false` and the gauge read 98 %.
/// `model::human_size` already formats in decimal units for exactly this
/// reason (it matches GitHub's own billing UI), so the gauge now compares
/// like with like. Do not restore the binary value without answering that.
pub const CACHE_INCLUDED_BYTES: u64 = 10_000_000_000;

/// Whether a repository's caches are past the included 10 GB.
///
/// Strictly above: exactly `CACHE_INCLUDED_BYTES` is still inside it. Past
/// it the excess storage is billed at its hourly peak — unconditionally,
/// not as one branch of an alternative — and, *separately*, GitHub evicts
/// least-recently-read entries once the repository reaches its configured
/// limit, which this crate cannot read. Both can apply at once. Being past
/// the included threshold is therefore worth a ⚠ and nothing stronger: it
/// is never proof that eviction has already begun.
pub fn cache_over_included(cache_bytes: u64) -> bool {
    cache_bytes > CACHE_INCLUDED_BYTES
}

/// `used / basis` as a whole percentage.
///
/// A pure `(used, basis)` function, not `(used)` alone against a baked-in
/// constant: the cache basis is a compile-time constant (the included
/// threshold), but the minutes basis is the org's plan allowance
/// (`billing::included_minutes_for`).
/// An unknown allowance never reaches here — its gauges show no percentage
/// at all — yet a zero basis still reads 0 %, never a division by zero.
///
/// `pub(crate)`, not private: `views::billing::gauge_line` and
/// `views::billing::storage_percent` (the storage ratio, in hundredths of a
/// GB-hour) share this exact business rule (an uncapped, zero-guarded
/// percentage) rather than keeping their own copy of the same formula — the
/// two gauges here and the Billing tab's two would otherwise need to change
/// in lockstep with no single source of truth. Each call site has its own
/// zero-basis test: `gauges::tests::a_zero_basis_does_not_divide_by_zero`
/// here, `views::billing::tests::a_zero_allowance_does_not_divide_by_zero`
/// through the Billing tab's minutes gauge, and
/// `views::billing::tests::storage_percent_is_the_shared_percentage_in_hundredths`
/// for its storage ratio.
pub(crate) fn percent(used: u64, basis: u64) -> u64 {
    if basis == 0 {
        0
    } else {
        (used as f64 / basis as f64 * 100.0).round() as u64
    }
}

/// Widest the filled portion of a bar gets, in cells — the same maximum
/// `views::billing::gauge_line` uses.
const BAR_CELLS: usize = 20;

/// How far in an explanation starts on a row of its own.
const INDENT: &str = "  ";

/// The cache gauge's caveat: 10 Go is GitHub's *default included* threshold,
/// not the repository's real limit — which no endpoint exposes, so the gauge
/// is a cost marker rather than a capacity one.
///
/// 52 cells, and the length is load-bearing: this line sits on the banner at
/// every usage, so every cell it gains costs a row on every repository once
/// it stops fitting. `the_cache_banner_stays_within_its_line_budget` pins
/// the rows it buys.
const CACHE_CAVEAT: &str = "(seuil inclus ; limite réelle non exposée par l'API)";

/// What a cache gauge past 100 % adds.
///
/// Billing is *not* conditional on the repository's configured limit — the
/// excess is billed either way — while eviction is. Stated as two separate
/// facts rather than as an alternative, because presenting them with "or …
/// depending on the configured limit" made the billing half conditional on
/// something it does not depend on.
const OVER_INCLUDED: &str = "⚠ dépasse le seuil inclus : le stockage en excès est facturé ; \
                             l'éviction, elle, attend la limite configurée du dépôt";

/// Why a public repository's minutes gauge reads 0 %.
///
/// `pub(crate)`: the Billing tab says the same thing when an organization's
/// whole allowance reads 0 while its report shows minutes and a bill
/// (`views::billing::minutes_fixed_lines`, smoke S4). One sentence, said in
/// one place — two phrasings of the same fact would each look like a
/// different fact.
pub(crate) const PUBLIC_REASON: &str =
    "(dépôt public : minutes Actions gratuites et illimitées, hors plafond)";

/// Why a minutes gauge gives a total and no percentage: no allowance is
/// known for the org's plan.
const UNKNOWN_PLAN: &str = "· formule inconnue, pas de quota";

/// The filled portion of a bar, in at most `room` cells.
///
/// Capped independently of `percent` (which is never capped) so a gauge well
/// past its basis still draws a legible, full-looking bar instead of one
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
/// — used against its basis, with their units. Only the bar shrinks.
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
///
/// `UNKNOWN_PLAN` carries a leading `· ` for the beside-the-figures case —
/// the separator between the figures and its reason, the way `gauge_line`
/// puts one between two spans on one line. Review T5-m6: on rows of their
/// own, that same `· ` became the first character of an otherwise bare row,
/// a floating bullet with nothing to its left; stripped here, since a row
/// with nothing before it needs no separator from it. `PUBLIC_REASON` never
/// carries this prefix — its own parentheses already mark it off — so it
/// never had this defect and needs no matching change.
fn explained(figures: String, style: Style, explanation: &str, width: u16) -> Vec<Line<'static>> {
    let beside = format!(" {explanation}");
    if views::cells(&figures) + views::cells(&beside) <= usize::from(width) {
        return vec![Line::from(vec![
            Span::styled(figures, style),
            Span::styled(beside, style),
        ])];
    }
    let own = explanation.strip_prefix("· ").unwrap_or(explanation);
    let mut lines = vec![Line::from(Span::styled(figures, style))];
    lines.extend(own_rows(own, style, width));
    lines
}

/// Cache usage against GitHub's hardcoded 10 GB default included
/// per-repository threshold, in a column `width` cells wide.
///
/// Never clamped at 100 %: past the included threshold the repository is
/// being billed for the excess, and is *additionally* having its
/// least-recently-read entries evicted once it reaches a configured limit
/// the API never exposes. Clamping the number would hide exactly the fact
/// this gauge exists to show, and the warning under it keeps the two apart
/// rather than asserting the one it cannot check.
///
/// Nothing is clipped at any width the column is drawn at (final review
/// I4): the percentage and the `used / 10 Go` figures stay whole on the
/// gauge's row, the bar shrinking to leave them room — cut, `12.4 Go / 10`
/// read as 124 % — and the caveat and the over-threshold warning go on rows
/// of their own when the row cannot hold them.
pub fn cache_gauge_line(used: u64, width: u16) -> Vec<Line<'static>> {
    let pct = percent(used, CACHE_INCLUDED_BYTES);
    let figures = figures_row(
        "Cache   ",
        pct,
        &format!("{} / 10 Go", human_size(used)),
        width,
    );
    let mut lines = explained(figures, theme::text_style(), CACHE_CAVEAT, width);
    if pct > 100 {
        lines.extend(own_rows(OVER_INCLUDED, theme::status_warn(), width));
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
            format!("Minutes  {} min", views::thousands(used)),
            theme::muted(),
            UNKNOWN_PLAN,
            width,
        );
    };
    let pct = percent(used, allowance);
    let figures = figures_row(
        "Minutes ",
        pct,
        &format!(
            "{} / {}",
            views::thousands(used),
            views::thousands(allowance)
        ),
        width,
    );
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

    /// `text`, with every run of whitespace collapsed to a single space.
    ///
    /// An explanation pushed onto rows of its own is concatenated by `text`
    /// with no separator between rows, and each row carries `INDENT`, so a
    /// phrase straddling a wrap boundary reads as `…est  facturé…` and a
    /// plain `contains` for the sentence never matches — which is exactly
    /// how a correct string would look like a regression. Collapsing
    /// restores the sentence as the constant spells it, at any width.
    fn prose(lines: &[Line<'static>]) -> String {
        text(lines).split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn the_cache_gauge_reports_overshoot_rather_than_capping() {
        // josephine sits at 12.36 Go against the 10 GB included threshold
        // (`CACHE_INCLUDED_BYTES`): 12_360_000_000 / 10_000_000_000 = 124 %.
        // It read 115 % until #19, when the constant was still 10 GiB — a
        // basis GitHub does not bill on, so the gauge flattered a repository
        // that was already over. Clamping to 100 % would hide the one fact
        // the gauge exists to show: this repository is past what its plan
        // includes, and is paying for it.
        let line = prose(&cache_gauge_line(12_360_000_000, 60));
        assert!(line.contains("124"), "got: {line}");
        assert!(line.contains("dépasse le seuil inclus"), "got: {line}");
        // #19: the warning names billing *and* eviction, and pins them to
        // the configured limit. Asserting "évince" alone would have passed
        // just as well on the old line, which claimed eviction outright.
        assert!(line.contains("facturé"), "got: {line}");
        assert!(line.contains("limite configurée"), "got: {line}");
        // Review I3: billing must not read as conditional on the configured
        // limit. The clause that is conditional is the eviction one, and it
        // is the only one the sentence hangs on `limite configurée`.
        assert!(
            line.contains("le stockage en excès est facturé"),
            "billing is stated unconditionally: {line}"
        );
    }

    /// Review I5: the caveat rides on the banner at *every* usage, so a
    /// longer one costs a row on every repository, not only on the ones past
    /// the threshold. #19's first wording (63 cells) pushed the quiet banner
    /// from two rows to three at every inner width from 40 to 64, and the
    /// warned banner up by two at 47-51. Pinned here so the next rewording
    /// cannot grow it in silence.
    ///
    /// 58 is the resources column's inner width at a 60-column terminal (one
    /// column, less the block's two borders); 38 is `repo::MIN_WIDTH` less
    /// its borders — the narrowest the column is ever drawn at.
    #[test]
    fn the_cache_banner_stays_within_its_line_budget() {
        for (used, width, rows, case) in [
            (4_000_000_000u64, 58u16, 2usize, "quiet, inner 58"),
            (12_360_000_000, 58, 5, "warned, inner 58"),
            (4_000_000_000, 38, 3, "quiet, inner 38"),
            (12_360_000_000, 38, 7, "warned, inner 38"),
        ] {
            assert_eq!(
                cache_gauge_line(used, width).len(),
                rows,
                "{case}: {:?}",
                cache_gauge_line(used, width)
            );
        }
    }

    #[test]
    fn the_cache_gauge_stays_quiet_below_the_included_threshold() {
        let line = prose(&cache_gauge_line(4_000_000_000, 60));
        assert!(!line.contains("dépasse le seuil inclus"), "got: {line}");
        // `éviction`, not `évince`: since #19 no production string says
        // "évince" at all, so the old needle could no longer fail here.
        assert!(!line.contains("éviction"), "got: {line}");
        // #19: the caveat is there at every usage, and it never calls the
        // 10 Go a plafond — the word the product used to print.
        assert!(line.contains("seuil inclus"), "got: {line}");
        assert!(!line.contains("plafond"), "got: {line}");
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
        // Grouped like the Billing tab's own gauge for the same allowance
        // (review FR-tui-3): the same figure read two different ways in one
        // session would look like two different numbers.
        assert!(line.contains("1 004 / 3 000"), "got: {line}");
    }

    #[test]
    fn minutes_gauge_without_a_plan_shows_no_percentage() {
        let line = text(&minutes_gauge_line(1_004, false, None, 60));
        assert!(!line.contains('%'), "got: {line}");
        assert!(line.contains("formule inconnue"), "got: {line}");
    }

    /// Review RW-1: `UNKNOWN_PLAN` carries its own `· ` for the case where
    /// the explanation sits beside the figures, and after A6 split the
    /// wrapped form's assertions into two halves nothing pinned that joined
    /// form any more — deleting `· ` from the constant left the suite green.
    /// Both forms are pinned here: the whole row when it fits on one, and
    /// the halves, dot-free, when it does not.
    #[test]
    fn the_unknown_plan_explanation_joins_its_figures_with_a_middle_dot() {
        let inline = text(&minutes_gauge_line(1_000, false, None, 60));
        assert_eq!(
            inline,
            "Minutes  1 000 min · formule inconnue, pas de quota"
        );

        let wrapped = minutes_gauge_line(1_000, false, None, 20);
        let rows: Vec<String> = wrapped
            .iter()
            .map(|line| text(std::slice::from_ref(line)).trim().to_string())
            .collect();
        assert_eq!(rows[0], "Minutes  1 000 min");
        assert!(
            rows[1..].iter().all(|row| !row.starts_with('·')),
            "a wrapped row opens on the separator: {rows:?}"
        );
        assert_eq!(rows[1..].join(" "), "formule inconnue, pas de quota");
    }

    /// T5-m6: on rows of their own — an inner width under 50 cells, forcing
    /// the explanation off the figures' row — `UNKNOWN_PLAN`'s leading `· `
    /// used to become the first character of an otherwise bare row.
    /// `PUBLIC_REASON` carries no such prefix, so it never had a matching
    /// defect to fix.
    #[test]
    fn an_unknown_plan_explanation_does_not_wrap_starting_with_the_middle_dot() {
        let lines = minutes_gauge_line(1_004, false, None, 20);
        assert!(
            lines.len() > 1,
            "expected the explanation pushed to rows of its own: {lines:?}"
        );
        let first_explanation_row = text(&lines[1..2]);
        assert!(
            !first_explanation_row.trim_start().starts_with('·'),
            "got: {first_explanation_row:?}"
        );
    }

    /// Amended (2026-09-11, controller): the un-amended version of this test
    /// called `cache_gauge_line(0, 60)`, which pins the basis to the
    /// nonzero `CACHE_INCLUDED_BYTES` constant and only ever exercises zero
    /// *usage* — it cannot fail on the zero-*basis* property its own name
    /// promises, since production can never actually reach a zero basis
    /// through that entry point. The percentage instead goes through a pure
    /// `(used, basis)` helper — `percent` — shared by both gauges, and this
    /// calls it directly with a basis of zero: the shape issue #11 gave the
    /// minutes basis when it made it the plan's allowance, a figure read
    /// from data rather than a constant.
    #[test]
    fn a_zero_basis_does_not_divide_by_zero() {
        assert_eq!(percent(0, 0), 0);
        // A nonzero usage against a zero basis is the case that actually
        // divides by zero without the guard — asserting the value, not just
        // the absence of "NaN"/"inf", the way `views::billing::gauge_line`'s
        // own zero-allowance test does: a saturating float-to-int cast turns
        // both NaN and +inf into a large *finite* u64, which no substring
        // check for "NaN"/"inf" would ever catch.
        assert_eq!(percent(100, 0), 0);
    }

    /// #13: exactly 10 GB is still inside the included cache storage; one
    /// byte more is not.
    #[test]
    fn cache_over_included_is_strictly_above_ten_gigabytes() {
        assert!(!cache_over_included(CACHE_INCLUDED_BYTES));
        assert!(cache_over_included(CACHE_INCLUDED_BYTES + 1));
        assert!(!cache_over_included(0));
    }
}
