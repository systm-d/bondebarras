//! The Billing tab: strictly diagnostic, no destructive action.
//!
//! Minutes cannot be reclaimed retroactively, so the only useful thing this
//! view can do is name the repository burning them.

use crate::billing::{FREE_MINUTES_PER_MONTH, MinuteLine, sku_multiplier};
use crate::tui::app::App;
use crate::tui::theme;
use crate::tui::views::gauges;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::collections::HashSet;

/// The breakdown is the tab's reason to exist, but the area is bounded: past
/// this many rows the tail is noise the user came here to avoid, not signal.
const MAX_MINUTE_LINES: usize = 8;

/// One line summarising allowance consumption.
///
/// Deliberately not clamped at 100 %: an org at 818 % of its included minutes
/// is exactly the situation the tab exists to surface, and a full bar would
/// say nothing. The percentage itself comes from `gauges::percent`, shared
/// with the resource pane's own two gauges rather than kept as a second copy
/// of the same uncapped, zero-guarded formula.
pub fn gauge_line(used: u64, allowance: u64) -> String {
    let percent = gauges::percent(used, allowance);
    let filled = (percent as usize / 10).min(20);
    format!(
        "{} / {}   {}  {} %",
        thousands(used),
        thousands(allowance),
        "█".repeat(filled),
        percent
    )
}

/// Groups digits with a narrow space, as French convention wants.
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Short runner name plus its multiplier, e.g. `"Windows ×2"`. An unknown SKU
/// (no multiplier) is shown by its raw name rather than guessed at — it is
/// also named separately by the ⚠ line below, but a breakdown row that
/// dropped it silently would misreport which repository it belongs to.
fn sku_label(sku: &str) -> String {
    let name = if sku.starts_with("Actions macOS") {
        "macOS"
    } else {
        sku.strip_prefix("Actions ").unwrap_or(sku)
    };
    match sku_multiplier(sku) {
        Some(mult) if mult > 1 => format!("{name} ×{mult}"),
        _ => name.to_string(),
    }
}

/// One row of the per-repository breakdown, in the shape of the design
/// mockup: repo, raw quantity, runner (with its multiplier), equivalent.
fn minute_line_row(line: &MinuteLine) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            "   {:<12}{:>8} {:<12}{:>10}",
            line.repo,
            thousands(line.quantity),
            sku_label(&line.sku),
            thousands(line.equivalent),
        ),
        theme::muted(),
    ))
}

/// A usage-report amount, in the report's own currency.
///
/// `grossAmount`, `discountAmount` and `netAmount` are US dollars:
/// `pricePerUnit` is 0.006 for `Actions Linux`, GitHub's published
/// per-minute dollar rate. Suffixed like the `€` this replaces, never
/// converted — this crate has no exchange rate and must not invent one.
fn usd(amount: f64) -> String {
    format!("{amount:.2} $")
}

/// The month's cost line: gross, covered by the allowance, actually billed.
fn cost_line(gross: f64, covered: f64, billed: f64) -> String {
    format!(
        "Coûts   brut {}   couvert {}   facturé {}",
        usd(gross),
        usd(covered),
        usd(billed)
    )
}

pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let Some(org) = app.orgs.get(app.org_cursor) else {
        f.render_widget(
            Paragraph::new(Span::styled("Aucune organisation.", theme::muted())),
            area,
        );
        return;
    };

    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        org.login.clone(),
        theme::title_style(),
    ))];

    // The only signal that separates "public, free forever" from "private,
    // covered by the allowance": GitHub's usage report discounts both
    // identically, so the repo listing stage 1 already fetched is the sole
    // place this distinction survives.
    let private: HashSet<String> = org
        .repos
        .iter()
        .filter(|r| r.private)
        .map(|r| r.name.clone())
        .collect();

    match &org.billing {
        None => lines.push(Line::from(Span::styled(
            "⚠ facturation illisible — vous n'êtes pas propriétaire de cette organisation",
            theme::status_warn(),
        ))),
        Some(report) => {
            let months = report.months();
            let month = months
                .get(app.month_cursor.min(months.len().saturating_sub(1)))
                .cloned()
                .unwrap_or_default();

            lines.push(Line::from(Span::styled(month.clone(), theme::muted())));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Minutes équivalent-inclus",
                theme::text_style(),
            )));
            lines.push(Line::from(Span::styled(
                gauge_line(
                    report.included_minutes(&month, &private),
                    FREE_MINUTES_PER_MONTH,
                ),
                theme::text_style(),
            )));

            // The gauge says the quota is blown; this is the only actionable
            // part of the tab — which repository to go and fix. Minutes
            // cannot be reclaimed retroactively, so naming the offender is
            // not decoration, it is the tab's reason to exist.
            let minute_lines = report.minute_lines(&month, &private);
            for minute_line in minute_lines.iter().take(MAX_MINUTE_LINES) {
                lines.push(minute_line_row(minute_line));
            }
            // The cap keeps the block compact, but a truncation that leaves
            // no trace would hide the count of rows the user cannot see —
            // the one number a diagnostic view must never bury. Counted in
            // rows, not repositories: one repo can contribute several rows
            // (one per SKU), so "dépôt(s)" here would overstate how many
            // distinct repositories are actually hidden.
            if minute_lines.len() > MAX_MINUTE_LINES {
                lines.push(Line::from(Span::styled(
                    format!(
                        "   … et {} autre(s) ligne(s)",
                        minute_lines.len() - MAX_MINUTE_LINES
                    ),
                    theme::muted(),
                )));
            }

            let (gross, covered, billed) = report.cost(&month);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                cost_line(gross, covered, billed),
                if billed > 0.0 {
                    theme::status_warn()
                } else {
                    theme::muted()
                },
            )));

            for sku in report.unknown_skus(&month) {
                lines.push(Line::from(Span::styled(
                    format!("⚠ SKU inconnu, compté ×1 : {sku}"),
                    theme::status_warn(),
                )));
            }
        }
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Billing ")
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::{BillingReport, UsageItem};
    use crate::model::{OrgSummary, RepoSummary};
    use crate::tui::app::View;

    /// One usage-report line with its unit spelled out: minutes and storage
    /// share the report, and later tests need both. Fully discounted, as
    /// exec-d's September report was.
    fn usage(
        month: &str,
        sku: &str,
        unit: &str,
        quantity: f64,
        gross: f64,
        repo: &str,
    ) -> UsageItem {
        UsageItem {
            month: month.into(),
            product: "actions".into(),
            sku: sku.into(),
            quantity,
            unit_type: unit.into(),
            gross,
            discount: gross,
            net: 0.0,
            repo: repo.into(),
        }
    }

    fn private_repo(name: &str) -> RepoSummary {
        RepoSummary {
            name: name.into(),
            cache_bytes: 0,
            cache_count: 0,
            private: true,
            age_days: 0,
            class: crate::repos::RepoClass::Archivable,
        }
    }

    /// exec-d in September 2026, measured on 2026-09-10: 1 004 private
    /// Linux-equivalent minutes (the org's real monthly total, attributed to
    /// one repository for the fixture's sake) and 371.85 GB-hours of Actions
    /// storage, both fully discounted.
    fn exec_d_september() -> OrgSummary {
        OrgSummary {
            login: "exec-d".into(),
            cache_bytes: 0,
            cache_count: 0,
            repos: vec![private_repo("disconnected")],
            billing: Some(BillingReport {
                items: vec![
                    usage(
                        "2026-09",
                        "Actions Linux",
                        "Minutes",
                        1_004.0,
                        6.024,
                        "disconnected",
                    ),
                    usage(
                        "2026-09",
                        "Actions storage",
                        "GigabyteHours",
                        371.85,
                        0.1249,
                        "disconnected",
                    ),
                ],
            }),
        }
    }

    fn billing_app(org: OrgSummary) -> App {
        let mut app = App::new(vec![org]);
        app.view = View::Billing;
        app
    }

    /// The whole screen, row by row, rendered through `views::render` — the
    /// real layout, so the Billing panel gets the `Rect` it gets in
    /// production, never the bare frame.
    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let buf = crate::tui::views::testing::draw(app, width, height);
        let (rows, _) = crate::tui::views::testing::layout(app, width, height);
        crate::tui::views::testing::text_in(&buf, rows.body)
    }

    /// `needle` must be on screen at every width from 60 to 200 (at a height
    /// that fits the whole tab), then at every height from the first one
    /// that can hold its row up to 50 (at width 100). Sweeps, never samples:
    /// this project once shipped a modal whose prompt vanished at exactly one
    /// height per width.
    fn assert_shown_at_every_size(app: &mut App, needle: &str) {
        for width in 60..=200u16 {
            let s = screen(app, width, 50);
            assert!(s.contains(needle), "{needle:?} missing at {width}x50:\n{s}");
        }
        let tall = screen(app, 100, 50);
        let row = tall
            .lines()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} missing at 100x50:\n{tall}"));
        // Below the needle's row: the panel's bottom border, the status row
        // and the footer — the progress row is zero-high while nothing runs.
        let floor = u16::try_from(row).unwrap() + 1 + 1 + 1 + 2;
        for height in floor..=50 {
            let s = screen(app, 100, height);
            assert!(
                s.contains(needle),
                "{needle:?} missing at 100x{height}:\n{s}"
            );
        }
    }

    /// `needle` must appear nowhere on screen, at any width from 60 to 200.
    fn assert_absent_at_every_width(app: &mut App, needle: &str) {
        for width in 60..=200u16 {
            let s = screen(app, width, 50);
            assert!(
                !s.contains(needle),
                "{needle:?} present at {width}x50:\n{s}"
            );
        }
    }

    /// GitHub's amounts are US dollars (`pricePerUnit` 0.006 for Actions
    /// Linux). The tab printed the right figure with the wrong currency.
    /// Looks for the figure *with* its sign, so a stray `$` elsewhere cannot
    /// satisfy it.
    #[test]
    fn the_rendered_cost_line_is_in_dollars_at_every_size() {
        let mut app = billing_app(exec_d_september());
        // 6.024 + 0.1249 = 6.1489, gross and covered alike.
        assert_shown_at_every_size(&mut app, "brut 6.15 $");
        assert_shown_at_every_size(&mut app, "facturé 0.00 $");
        assert_absent_at_every_width(&mut app, "€");
    }

    #[test]
    fn cost_line_reads_dollars_never_euros() {
        let line = cost_line(6.1489, 6.1489, 0.0);
        assert_eq!(
            line,
            "Coûts   brut 6.15 $   couvert 6.15 $   facturé 0.00 $"
        );
    }

    #[test]
    fn the_gauge_reports_overshoot_rather_than_capping_at_full() {
        // An illustrative figure, not a measured one — clamping overshoot to
        // 100 % would hide exactly the thing the tab exists to show.
        let line = gauge_line(16_369, 2_000);
        assert!(line.contains("818"), "got: {line}");
        assert!(
            line.contains("16 369") || line.contains("16369"),
            "got: {line}"
        );
    }

    #[test]
    fn a_zero_allowance_does_not_divide_by_zero() {
        // Without the guard this computes inf and the cast saturates to
        // u64::MAX, which renders as a large finite number — so asserting the
        // absence of "NaN"/"inf" would not catch it. Assert the value.
        let line = gauge_line(100, 0);
        assert!(line.ends_with(" 0 %"), "got: {line}");
        assert!(!line.contains(&u64::MAX.to_string()), "got: {line}");
    }

    #[test]
    fn an_unused_month_reads_zero_percent() {
        // `.contains('0')` would pass on any percentage: the allowance operand
        // "2 000" carries a zero of its own.
        assert!(gauge_line(0, 2_000).ends_with(" 0 %"));
    }
}
