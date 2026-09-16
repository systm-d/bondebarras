//! The Billing tab: strictly diagnostic, no destructive action.
//!
//! Minutes cannot be reclaimed retroactively, nor GB-hours already counted,
//! so the only useful thing this view can do is name the repository behind
//! them.

use crate::billing::{
    self, BillingReport, Budget, MinuteLine, StorageLine, StorageQuota, included_minutes_for,
    sku_multiplier,
};
use crate::model::{ArtifactRetention, OrgSummary};
use crate::tui::app::App;
use crate::tui::theme;
use crate::tui::views::{self, gauges};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::collections::HashSet;

/// The breakdowns — minutes and storage — are the tab's reason to exist, but
/// the area is bounded: past this many rows the tail is noise the user came
/// here to avoid, not signal. Both blocks share it, as #13 asks.
const MAX_BREAKDOWN_LINES: usize = 8;

/// What GitHub's documentation insists on, split in two so both halves
/// survive a narrow frame: deleting artifacts stops the accumulation but
/// refunds nothing already counted.
const DELETION_DOES_NOT_REFUND: [&str; 2] = [
    "Supprimer des artefacts arrête l'accumulation,",
    "  mais ne rend pas les GB-heures déjà comptées.",
];

/// The two exact points #15 asks the tab to state, split to stay legible in
/// a narrow frame. True whatever the setting, so always shown.
const RETENTION_NOTES: [&str; 4] = [
    "Note : retention-days, dans un workflow, fixe la durée",
    "  de cet artefact, dans la limite de ce réglage.",
    "Note : un changement de rétention ne vaut que pour",
    "  les nouveaux artefacts et journaux.",
];

/// What the storage gauge says for a report with no month at all: without a
/// month there is no hour count to build a quota from, whatever the plan.
const NO_USAGE: &str = "aucun usage signalé";

/// One line summarising allowance consumption.
///
/// Deliberately not clamped at 100 %: an org well past its included minutes
/// is exactly the situation the tab exists to surface. The percentage itself
/// comes from `gauges::percent`, shared with the resources column's two
/// gauges rather than kept as a second copy of the same uncapped,
/// zero-guarded formula. Without a known allowance — no plan read, or a plan
/// this crate has no figure for — the line gives the total and says why
/// there is no percentage, rather than dividing by a guess.
pub fn gauge_line(used: u64, allowance: Option<u64>) -> String {
    let Some(allowance) = allowance else {
        return format!("{} min   formule inconnue, pas de quota", thousands(used));
    };
    let percent = gauges::percent(used, allowance);
    format!(
        "{} / {}   {}  {} %",
        thousands(used),
        thousands(allowance),
        bar(percent),
        percent
    )
}

/// The gauge's bar: one cell per 10 %, at most 20 so overshoot stays legible.
fn bar(percent: u64) -> String {
    capped_bar(percent, 20)
}

/// Widest the storage gauge's bar gets, in cells — half the minutes bar.
/// The storage line also carries the hour base, which must stay whole in a
/// 60-column frame however far past 100 % the organization is: a clipped
/// `base 72` would be a wrong figure.
const STORAGE_BAR_CELLS: usize = 10;

/// One cell per 10 %, at most `cells`.
fn capped_bar(percent: u64, cells: usize) -> String {
    "█".repeat((percent as usize / 10).min(cells))
}

/// A storage figure in hundredths of a GB-hour — the usage report's own
/// precision — so it can go through `gauges::percent`, the crate's single
/// percentage, which counts in whole units.
fn centi_gbh(gbh: f64) -> u64 {
    (gbh * 100.0).round() as u64
}

/// The storage ratio as a whole percentage: `gauges::percent` on hundredths
/// of a GB-hour, never a second formula. Every reader of that ratio goes
/// through here, so none can round it differently.
fn storage_percent(used: f64, quota: StorageQuota) -> u64 {
    gauges::percent(centi_gbh(used), centi_gbh(quota.gbh))
}

/// Actions storage consumed against the plan's included GB-hours, with the
/// hour base written out — the base is an open measurement, so the line
/// never lets a percentage stand without it. Not clamped, like the minutes.
pub fn storage_gauge_line(used: f64, quota: Option<StorageQuota>) -> String {
    let Some(quota) = quota else {
        return format!("{used:.2} GB-h   formule inconnue, pas de quota");
    };
    let percent = storage_percent(used, quota);
    format!(
        "{used:.2} / {} GB-h   {}  {} %   base {} h",
        thousands(quota.gbh.round() as u64),
        capped_bar(percent, STORAGE_BAR_CELLS),
        percent,
        quota.hours
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

/// One row of the storage breakdown: the repository, cut to its 20 cells so
/// the GB-hours keep their column, a space, then its GB-hours right-aligned
/// on 13 cells (room for `99999.99 GB-h`). The space is written out rather
/// than left to the padding: a figure as wide as its field has none, and a
/// cut name would run into it.
fn storage_line_row(line: &StorageLine) -> Line<'static> {
    let amount = format!("{:.2} GB-h", line.gbh);
    Line::from(Span::styled(
        format!("   {} {amount:>13}", views::fit(&line.repo, 20)),
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

/// The organization's Actions budget and what it does past the allowance —
/// or why nothing can be said. "No budget" and "unreadable" never share a
/// line: the first means overage is billed, the second that nobody knows.
fn budget_lines(budgets: Option<&[Budget]>) -> Vec<Line<'static>> {
    let Some(budgets) = budgets else {
        return vec![
            Line::from(Span::styled("Budget Actions : illisible", theme::muted())),
            Line::from(Span::styled(
                "  (réservé aux admins et gestionnaires de facturation)",
                theme::muted(),
            )),
        ];
    };
    let mut lines = match billing::actions_budget(budgets) {
        Some(b) if b.blocking => vec![Line::from(Span::styled(
            format!("Budget Actions : {} · bloquant", usd(b.amount as f64)),
            theme::text_style(),
        ))],
        Some(b) => vec![Line::from(Span::styled(
            format!(
                "Budget Actions : {} · alerte seule, sans blocage",
                usd(b.amount as f64)
            ),
            theme::text_style(),
        ))],
        None => vec![
            Line::from(Span::styled(
                "Budget Actions : aucun, dépassement facturé sans plafond",
                theme::text_style(),
            )),
            Line::from(Span::styled(
                "  (si un moyen de paiement est enregistré)",
                theme::muted(),
            )),
        ],
    };
    // Never observed, so named rather than interpreted.
    for b in billing::actions_sku_budgets(budgets) {
        let mode = if b.blocking {
            "bloquant"
        } else {
            "alerte seule"
        };
        lines.push(Line::from(Span::styled(
            format!("Budget SKU {} : {} · {mode}", b.sku, usd(b.amount as f64)),
            theme::muted(),
        )));
        lines.push(Line::from(Span::styled(
            "  signalé, non pris en compte par les avertissements",
            theme::muted(),
        )));
    }
    lines
}

/// Under a gauge at `billing::BUDGET_WARNING_PERCENT` or more with a
/// blocking Actions budget: what GitHub will do once the allowance runs out.
/// Takes the percentage the gauge displays, so the two never disagree.
fn budget_warning_lines(quota: &str, percent: u64, budget: Option<&Budget>) -> Vec<Line<'static>> {
    let Some(b) = budget.filter(|_| billing::nears_blocking_budget(percent, budget)) else {
        return Vec::new();
    };
    let consequence = if b.amount == 0 {
        "  GitHub bloquera l'usage Actions au quota atteint.".to_string()
    } else {
        format!(
            "  facturé jusqu'à {}, puis usage Actions bloqué.",
            usd(b.amount as f64)
        )
    };
    vec![
        Line::from(Span::styled(
            format!(
                "⚠ {percent} % du quota de {quota}, budget {} bloquant :",
                usd(b.amount as f64)
            ),
            theme::status_warn(),
        )),
        Line::from(Span::styled(consequence, theme::status_warn())),
    ]
}

/// `exec-d · formule team`, or `formule inconnue` when the plan was not read.
fn header_line(login: &str, plan: Option<&str>) -> Line<'static> {
    let plan = plan.unwrap_or("inconnue");
    Line::from(Span::styled(
        format!("{login} · formule {plan}"),
        theme::title_style(),
    ))
}

/// Every month the tab pages through is measured against today's plan:
/// `plan.name` is the only plan GitHub reports, and a mid-month change
/// (exec-d moved from Free to Team on 2026-09-10) is invisible to it. The
/// allowance is GitHub's documented figure for that plan, not one read from
/// a response, and the line says so — as the cache gauge says its ceiling
/// is.
fn month_line(month: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("{month} · quota documenté de la formule actuelle"),
        theme::muted(),
    ))
}

/// On `enterprise`, the allowance belongs to the enterprise account and is
/// shared by its organizations. bondebarras only sees this one org's usage,
/// so every percentage below is a floor — and the tab says so.
fn enterprise_lines(plan: Option<&str>) -> Vec<Line<'static>> {
    if plan != Some("enterprise") {
        return Vec::new();
    }
    vec![
        Line::from(Span::styled(
            "Formule enterprise : quota partagé par tout le compte",
            theme::muted(),
        )),
        Line::from(Span::styled(
            "  entreprise, ces pourcentages sont des minimums.",
            theme::muted(),
        )),
    ]
}

fn unreadable_line() -> Line<'static> {
    Line::from(Span::styled(
        "⚠ facturation illisible — vous n'êtes pas propriétaire de cette organisation",
        theme::status_warn(),
    ))
}

/// The month under `month_cursor`, counted back from the report's newest:
/// the tab opens on the current period, `←` pages to older months
/// (`tui::page_months`). Clamped: the cursor can outlive a switch to an org
/// with fewer months.
fn displayed_month(report: &BillingReport, month_cursor: usize) -> String {
    let months = report.months();
    months
        .iter()
        .rev()
        .nth(month_cursor.min(months.len().saturating_sub(1)))
        .cloned()
        .unwrap_or_default()
}

/// The only signal that separates "public, free forever" from "private,
/// covered by the allowance": GitHub's usage report discounts both
/// identically, so the repo listing stage 1 already fetched is the sole
/// place this distinction survives.
fn private_repos(org: &OrgSummary) -> HashSet<String> {
    org.repos
        .iter()
        .filter(|r| r.private)
        .map(|r| r.name.clone())
        .collect()
}

/// The first `MAX_BREAKDOWN_LINES` rows, then `… et N autre(s) {rest}` when
/// some were left out: a truncation that leaves no trace would bury the
/// count of hidden rows. Every breakdown of the tab truncates through here.
fn breakdown<T>(rows: &[T], row: impl Fn(&T) -> Line<'static>, rest: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = rows.iter().take(MAX_BREAKDOWN_LINES).map(row).collect();
    if rows.len() > MAX_BREAKDOWN_LINES {
        lines.push(Line::from(Span::styled(
            format!(
                "   … et {} autre(s) {rest}",
                rows.len() - MAX_BREAKDOWN_LINES
            ),
            theme::muted(),
        )));
    }
    lines
}

/// The minutes gauge, the budget warning it may carry, and the
/// per-repository breakdown behind it.
///
/// The breakdown is the tab's reason to exist: minutes cannot be reclaimed
/// once burnt, so the actionable part is *which repository* burnt them.
fn minutes_block(
    report: &BillingReport,
    month: &str,
    private: &HashSet<String>,
    plan: Option<&str>,
    budget: Option<&Budget>,
) -> Vec<Line<'static>> {
    let used = report.included_minutes(month, private);
    let allowance = included_minutes_for(plan);
    let mut lines = vec![
        Line::from(Span::styled(
            "Minutes équivalent-inclus",
            theme::text_style(),
        )),
        Line::from(Span::styled(
            gauge_line(used, allowance),
            theme::text_style(),
        )),
    ];
    // No allowance, no percentage — and so nothing to warn against either:
    // the budget would have no ratio to be near.
    if let Some(allowance) = allowance {
        let percent = gauges::percent(used, allowance);
        lines.extend(budget_warning_lines("minutes", percent, budget));
    }
    // Counted in rows, not repositories: one repo can contribute several rows
    // (one per SKU).
    lines.extend(breakdown(
        &report.minute_lines(month, private),
        minute_line_row,
        "ligne(s)",
    ));
    lines
}

/// The storage gauge, the budget warning it may carry, the repositories
/// holding the storage, and what deleting can and cannot do about it. No
/// request of its own: the usage report stage 1 loaded carries every line.
fn storage_block(
    report: &BillingReport,
    month: &str,
    plan: Option<&str>,
    budget: Option<&Budget>,
) -> Vec<Line<'static>> {
    let used = report.storage_gbh(month);
    let quota = billing::storage_quota(plan, month);
    // An empty month is a report with no usage: the quota is missing for
    // want of an hour count, whatever the plan, so `formule inconnue` would
    // give the wrong reason.
    let gauge = if month.is_empty() {
        format!("{used:.2} GB-h   {NO_USAGE}")
    } else {
        storage_gauge_line(used, quota)
    };
    let mut lines = vec![
        Line::from(Span::styled(
            "Stockage Actions · GB-heures, dépôts publics compris",
            theme::text_style(),
        )),
        Line::from(Span::styled(gauge, theme::text_style())),
    ];
    // Same rule as the minutes: no quota, no percentage, no warning.
    if let Some(quota) = quota {
        let percent = storage_percent(used, quota);
        lines.extend(budget_warning_lines("stockage", percent, budget));
    }
    lines.extend(breakdown(
        &report.storage_lines(month),
        storage_line_row,
        "dépôt(s)",
    ));
    for text in DELETION_DOES_NOT_REFUND {
        lines.push(Line::from(Span::styled(text, theme::muted())));
    }
    lines
}

/// The retention setting, beside the storage it governs. Highlighted — ⚠,
/// warning colour, and the reason — when it is at least 90 days on an org
/// whose storage counts (`billing::retention_worth_flagging`). Read-only:
/// the tab shows the tap, it does not turn it.
fn retention_lines(
    retention: Option<ArtifactRetention>,
    storage_gbh: Option<f64>,
) -> Vec<Line<'static>> {
    let Some(r) = retention else {
        return vec![
            Line::from(Span::styled(
                "Rétention artefacts et journaux : illisible",
                theme::muted(),
            )),
            Line::from(Span::styled(
                "  (scope admin:org requis pour la lire)",
                theme::muted(),
            )),
        ];
    };
    let maximum = r
        .maximum_allowed_days
        .map(|m| format!(" (max. {m} j)"))
        .unwrap_or_default();
    let text = format!("Rétention artefacts et journaux : {} j{maximum}", r.days);
    if billing::retention_worth_flagging(r.days, storage_gbh) {
        vec![
            Line::from(Span::styled(format!("⚠ {text}"), theme::status_warn())),
            Line::from(Span::styled(
                "  c'est ce réglage qui fait durer le stockage",
                theme::status_warn(),
            )),
        ]
    } else {
        vec![Line::from(Span::styled(text, theme::text_style()))]
    }
}

/// #15's two notes, shown whatever the tab could read: both are true of the
/// setting itself, not of any figure beside it.
fn retention_notes() -> Vec<Line<'static>> {
    RETENTION_NOTES
        .iter()
        .map(|text| Line::from(Span::styled(*text, theme::muted())))
        .collect()
}

/// The month's costs, then any runner SKU `sku_multiplier` does not know.
fn cost_block(report: &BillingReport, month: &str) -> Vec<Line<'static>> {
    let (gross, covered, billed) = report.cost(month);
    let style = if billed > 0.0 {
        theme::status_warn()
    } else {
        theme::muted()
    };
    let mut lines = vec![Line::from(Span::styled(
        cost_line(gross, covered, billed),
        style,
    ))];
    for sku in report.unknown_skus(month) {
        lines.push(Line::from(Span::styled(
            format!("⚠ SKU inconnu, compté ×1 : {sku}"),
            theme::status_warn(),
        )));
    }
    lines
}

/// Every line of the tab for one org, top to bottom.
///
/// Built from owned lines so the borrow of `app.orgs` ends before rendering,
/// and so each block can be asserted on through the real render.
fn tab_lines(org: &OrgSummary, month_cursor: usize) -> Vec<Line<'static>> {
    let plan = org.plan.as_deref();
    let budgets = org.budgets.as_deref();
    // Built once: the budgets are their own endpoint, refused on their own,
    // so they are shown whether or not the usage report could be read. Each
    // branch below only places them.
    let budget_block = budget_lines(budgets);
    let mut lines = vec![header_line(&org.login, plan)];

    if let Some(report) = &org.billing {
        let month = displayed_month(report, month_cursor);
        let private = private_repos(org);
        // The warning is in the future tense — GitHub *will* block once the
        // allowance runs out — so it belongs to the month still running. Paged
        // back to a closed month, the budget carries no warning: that month's
        // outcome is already settled, whatever its gauges read.
        let budget = budgets
            .and_then(billing::actions_budget)
            .filter(|_| report.months().last() == Some(&month));
        // A readable report with no usage at all has no month: its line would
        // be a bare ` · quota documenté…`.
        if !month.is_empty() {
            lines.push(month_line(&month));
        }
        lines.extend(enterprise_lines(plan));
        lines.extend(budget_block);
        lines.push(Line::from(""));
        lines.extend(minutes_block(report, &month, &private, plan, budget));
        lines.push(Line::from(""));
        lines.extend(storage_block(report, &month, plan, budget));
        // Beside the storage it governs, and weighed against that month's
        // own GB-hours.
        lines.extend(retention_lines(
            org.retention,
            Some(report.storage_gbh(&month)),
        ));
        lines.push(Line::from(""));
        lines.extend(cost_block(report, &month));
    } else {
        lines.push(unreadable_line());
        lines.extend(budget_block);
        // Storage unknown: the retention is shown, never highlighted —
        // nobody can say whether it counts.
        lines.extend(retention_lines(org.retention, None));
    }

    // #15's two notes close the tab, whatever it could read.
    lines.push(Line::from(""));
    lines.extend(retention_notes());
    lines
}

pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let Some(org) = app.orgs.get(app.org_cursor) else {
        f.render_widget(
            Paragraph::new(Span::styled("Aucune organisation.", theme::muted())),
            area,
        );
        return;
    };
    let lines = tab_lines(org, app.month_cursor);
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
    use crate::billing::{Budget, UsageItem};
    use crate::model::{ArtifactRetention, RepoSummary};
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
            ..Default::default()
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
        let line = gauge_line(16_369, Some(2_000));
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
        let line = gauge_line(100, Some(0));
        assert!(line.ends_with(" 0 %"), "got: {line}");
        assert!(!line.contains(&u64::MAX.to_string()), "got: {line}");
    }

    #[test]
    fn an_unused_month_reads_zero_percent() {
        // `.contains('0')` would pass on any percentage: the allowance operand
        // "2 000" carries a zero of its own.
        assert!(gauge_line(0, Some(2_000)).ends_with(" 0 %"));
    }

    #[test]
    fn gauge_line_without_an_allowance_has_no_percentage() {
        assert_eq!(
            gauge_line(1_004, None),
            "1 004 min   formule inconnue, pas de quota"
        );
    }

    /// #11's real figures: exec-d, on Team since 2026-09-10, burnt 1 004
    /// private minutes in September. Against the Free plan's 2 000 every org
    /// used to get, the tab read 50 %; against Team's 3 000, 33 % is right.
    /// Both are asserted: a constant allowance satisfies neither.
    #[test]
    fn exec_d_september_reads_33_percent_on_team() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "1 004 / 3 000");
        assert_shown_at_every_size(&mut app, " 33 %");
        assert_absent_at_every_width(&mut app, "50 %");
    }

    /// No plan, no percentage — anywhere in the tab, not only on the minutes
    /// line. Later blocks (storage, budget warnings) must keep this green.
    #[test]
    fn an_unknown_plan_shows_no_percentage_anywhere_in_the_tab() {
        let mut app = billing_app(exec_d_september());
        assert_shown_at_every_size(&mut app, "formule inconnue, pas de quota");
        assert_absent_at_every_width(&mut app, "%");
    }

    #[test]
    fn the_billing_header_names_the_current_plan() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "exec-d · formule team");
        assert_shown_at_every_size(&mut app, "2026-09 · quota documenté de la formule actuelle");
    }

    #[test]
    fn an_enterprise_org_says_its_quota_is_shared() {
        let mut org = exec_d_september();
        org.login = "SecondBrain-io".into();
        org.plan = Some("enterprise".into());
        let mut app = billing_app(org);
        assert_shown_at_every_size(
            &mut app,
            "Formule enterprise : quota partagé par tout le compte",
        );
        assert_shown_at_every_size(&mut app, "entreprise, ces pourcentages sont des minimums.");

        // Said on enterprise only.
        let mut team = exec_d_september();
        team.plan = Some("team".into());
        let mut app = billing_app(team);
        assert_absent_at_every_width(&mut app, "Formule enterprise");
    }

    /// Pre-flight 5.1: the tab opened on the report's oldest month, so the
    /// gauge, and every later block, described a closed month until `→` had
    /// walked the whole report. Two months, August pushed after September:
    /// neither the oldest-first `months()` nor the report's last item names
    /// the newest.
    #[test]
    fn the_billing_tab_opens_on_the_newest_month() {
        let mut org = exec_d_september();
        org.billing
            .as_mut()
            .expect("exec-d's fixture carries a report")
            .items
            .push(usage(
                "2026-08",
                "Actions Linux",
                "Minutes",
                250.0,
                1.5,
                "disconnected",
            ));
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "2026-09 ·");
        assert_absent_at_every_width(&mut app, "2026-08 ·");
    }

    /// Task 5 review m4: a readable report with no usage at all has no month
    /// to name, and the month line read ` · quota documenté de la formule
    /// actuelle` — a separator hanging off nothing.
    #[test]
    fn an_empty_report_names_no_month() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        org.billing = Some(BillingReport { items: Vec::new() });
        let mut app = billing_app(org);
        // The readable tab is drawn, so the absences below are not vacuous.
        assert_shown_at_every_size(&mut app, "exec-d · formule team");
        assert_shown_at_every_size(&mut app, "Minutes équivalent-inclus");
        assert_absent_at_every_width(&mut app, "quota documenté");
        for width in 60..=200u16 {
            let s = screen(&mut app, width, 50);
            for line in s.lines() {
                let inner = line.trim_matches(|c: char| c == '│' || c.is_whitespace());
                assert!(
                    !inner.starts_with('·'),
                    "dangling separator at {width}x50:\n{s}"
                );
            }
        }
    }

    /// An org whose ten private repositories each carry one usage line of
    /// `sku` in `unit`: `depot-01` the heaviest, `depot-10` the lightest,
    /// `step` apart. Two more than `MAX_BREAKDOWN_LINES`.
    fn ten_repos_of(sku: &str, unit: &str, step: f64) -> OrgSummary {
        let names: Vec<String> = (1..=10).map(|n| format!("depot-{n:02}")).collect();
        OrgSummary {
            login: "exec-d".into(),
            repos: names.iter().map(|name| private_repo(name)).collect(),
            billing: Some(BillingReport {
                items: names
                    .iter()
                    .zip((1..=10).rev())
                    .map(|(name, weight)| {
                        usage("2026-09", sku, unit, f64::from(weight) * step, 0.0, name)
                    })
                    .collect(),
            }),
            ..Default::default()
        }
    }

    /// Past `MAX_BREAKDOWN_LINES` rows the minutes breakdown stops and says
    /// how many rows it left out: the two lightest are hidden, never dropped
    /// without a trace.
    #[test]
    fn the_minutes_block_stops_at_eight_rows_and_counts_the_rest() {
        let mut app = billing_app(ten_repos_of("Actions Linux", "Minutes", 100.0));
        assert_shown_at_every_size(&mut app, "depot-08");
        assert_shown_at_every_size(&mut app, "… et 2 autre(s) ligne(s)");
        assert_absent_at_every_width(&mut app, "depot-09");
        assert_absent_at_every_width(&mut app, "depot-10");
    }

    /// exec-d's September lines per repository, on Team. Listed lightest
    /// first so the ranking has work to do; storage amounts are not under
    /// test here and are left at zero.
    fn exec_d_september_by_repo() -> OrgSummary {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        org.billing = Some(BillingReport {
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
                    11.21,
                    0.0,
                    "ptitjardinier-app",
                ),
                usage(
                    "2026-09",
                    "Actions storage",
                    "GigabyteHours",
                    359.88,
                    0.0,
                    "disconnected",
                ),
            ],
        });
        org
    }

    /// exec-d in September on Free: 371.85 GB-h against 0.5 GB × 720 h. GitHub
    /// discounted all of it; the 103 % shown is the spec's open measurement
    /// (720 or 744 hours) — stated on the line, not hidden.
    #[test]
    fn storage_gauge_states_its_hour_base() {
        let september = storage_gauge_line(371.85, billing::storage_quota(Some("free"), "2026-09"));
        assert_eq!(
            september,
            "371.85 / 360 GB-h   ██████████  103 %   base 720 h"
        );

        let july = storage_gauge_line(371.85, billing::storage_quota(Some("free"), "2026-07"));
        assert!(july.starts_with("371.85 / 372 GB-h"), "got: {july}");
        assert!(july.ends_with("100 %   base 744 h"), "got: {july}");
    }

    #[test]
    fn storage_gauge_without_a_plan_has_no_percentage() {
        assert_eq!(
            storage_gauge_line(371.85, None),
            "371.85 GB-h   formule inconnue, pas de quota"
        );
    }

    #[test]
    fn the_storage_block_says_public_repos_count() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        let mut app = billing_app(org);
        assert_shown_at_every_size(
            &mut app,
            "Stockage Actions · GB-heures, dépôts publics compris",
        );
        // Team: 371.85 of 2 GB × 720 h.
        assert_shown_at_every_size(&mut app, "371.85 / 1 440 GB-h");
        assert_shown_at_every_size(&mut app, "base 720 h");
    }

    /// Ranked by row position of the *storage figures*: "disconnected" also
    /// names a minutes row above the block, so searching for the repository
    /// name would find that row and pass whatever the storage order.
    #[test]
    fn the_storage_block_names_the_heaviest_repo_first() {
        let mut app = billing_app(exec_d_september_by_repo());
        assert_shown_at_every_size(&mut app, "359.88 GB-h");
        assert_shown_at_every_size(&mut app, "11.21 GB-h");

        let s = screen(&mut app, 100, 50);
        let row_of = |needle: &str| {
            s.lines()
                .position(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("{needle:?} missing:\n{s}"))
        };
        assert!(
            row_of("359.88 GB-h") < row_of("11.21 GB-h"),
            "heaviest first:\n{s}"
        );
        let heaviest = s.lines().nth(row_of("359.88 GB-h")).unwrap();
        assert!(heaviest.contains("disconnected"), "got: {heaviest}");
    }

    /// GitHub's documentation: deleting artifacts "does not remove charges
    /// already recorded". Mandatory, and both halves must survive a narrow
    /// frame, or the line says only the reassuring half.
    #[test]
    fn the_storage_block_says_deleting_does_not_refund() {
        let mut app = billing_app(exec_d_september());
        assert_shown_at_every_size(&mut app, "Supprimer des artefacts arrête l'accumulation,");
        assert_shown_at_every_size(&mut app, "mais ne rend pas les GB-heures déjà comptées.");
    }

    /// The storage ratio is `gauges::percent` on hundredths of a GB-hour, not
    /// a formula of its own. 142.2 GB-h is exactly 39.5 % of Free's 360 in
    /// September: the shared percentage, on 14 220 / 36 000, reads 40, while
    /// a second `f64` formula (142.2 / 360 × 100 lands just under 39.5) and
    /// whole GB-hours (142 / 360) both read 39. A zero quota reads the shared
    /// guard's 0 %, not a saturated `u64::MAX`.
    #[test]
    fn storage_percent_is_the_shared_percentage_in_hundredths() {
        let free_september =
            billing::storage_quota(Some("free"), "2026-09").expect("free has a storage quota");
        assert_eq!(storage_percent(142.2, free_september), 40);
        let zero = StorageQuota {
            gbh: 0.0,
            hours: 720,
        };
        assert_eq!(storage_percent(5.0, zero), 0);
    }

    /// Past `MAX_BREAKDOWN_LINES` repositories the storage breakdown stops
    /// and says how many it left out, as the minutes breakdown does.
    #[test]
    fn the_storage_block_stops_at_eight_repos_and_counts_the_rest() {
        let mut app = billing_app(ten_repos_of("Actions storage", "GigabyteHours", 10.0));
        assert_shown_at_every_size(&mut app, "depot-08");
        assert_shown_at_every_size(&mut app, "… et 2 autre(s) dépôt(s)");
        assert_absent_at_every_width(&mut app, "depot-09");
        assert_absent_at_every_width(&mut app, "depot-10");
    }

    /// The figure, the quota and the hour base all follow the month on
    /// screen: July, paged back to, is its own GB-hours against 2 GB × 744 h —
    /// not September's figure, nor September's 720 hours.
    #[test]
    fn the_storage_block_follows_the_displayed_month() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        org.billing
            .as_mut()
            .expect("exec-d's fixture carries a report")
            .items
            .push(usage(
                "2026-07",
                "Actions storage",
                "GigabyteHours",
                120.5,
                0.0,
                "disconnected",
            ));
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "371.85 / 1 440 GB-h");
        app.month_cursor = 1;
        assert_shown_at_every_size(&mut app, "120.50 / 1 488 GB-h");
        assert_shown_at_every_size(&mut app, "base 744 h");
    }

    /// Task 7 review m1: a readable month with no storage reads a plain zero,
    /// never the `-0.00` an empty `f64` sum prints. November, because
    /// `2026-09` itself contains `-0`: this way no `-0` may appear anywhere.
    #[test]
    fn the_storage_block_reads_a_positive_zero_without_storage() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        org.billing = Some(BillingReport {
            items: vec![usage(
                "2026-11",
                "Actions Linux",
                "Minutes",
                1_004.0,
                6.024,
                "disconnected",
            )],
        });
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "0.00 / 1 440 GB-h");
        assert_absent_at_every_width(&mut app, "-0");
    }

    /// A readable report with no usage at all has no month, so no hour count
    /// and no storage quota — but the plan is known, and the header names it.
    /// The gauge must not claim `formule inconnue` under `formule team`.
    #[test]
    fn the_storage_block_says_no_usage_without_a_month() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        org.billing = Some(BillingReport { items: Vec::new() });
        let mut app = billing_app(org);
        // Positive anchors: the readable tab and its storage block are drawn.
        assert_shown_at_every_size(&mut app, "exec-d · formule team");
        assert_shown_at_every_size(
            &mut app,
            "Stockage Actions · GB-heures, dépôts publics compris",
        );
        assert_shown_at_every_size(&mut app, "0.00 GB-h   aucun usage signalé");
        assert_absent_at_every_width(&mut app, "formule inconnue");
        assert_absent_at_every_width(&mut app, "-0");
    }

    /// A repository name longer than its 20 cells is cut with `…`, so the
    /// GB-hours stay in their column instead of being pushed right.
    #[test]
    fn a_long_repo_name_keeps_the_storage_figure_in_its_column() {
        let text = |repo: &str| -> String {
            storage_line_row(&StorageLine {
                repo: repo.into(),
                gbh: 359.88,
            })
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
        };
        let long = text("ptitjardinier-app-monorepo");
        assert_eq!(long, "   ptitjardinier-app-m…   359.88 GB-h");
        assert_eq!(long.chars().count(), text("disconnected").chars().count());
    }

    /// An org whose only usage is `gbh` GB-hours of Actions storage in
    /// September, on `plan`.
    fn storage_only(plan: &str, gbh: f64) -> OrgSummary {
        let mut org = exec_d_september();
        org.plan = Some(plan.into());
        org.billing = Some(BillingReport {
            items: vec![usage(
                "2026-09",
                "Actions storage",
                "GigabyteHours",
                gbh,
                0.0,
                "disconnected",
            )],
        });
        org
    }

    /// Past 100 % the bar grows, and a 20-cell bar pushed the hour base off a
    /// 60-column frame: `base 72` is a wrong figure, in exactly the case the
    /// tab exists for. Each org overshoots its own plan's quota, and each
    /// line would pass 58 cells with a 20-cell bar (63, 60 and 62).
    #[test]
    fn the_storage_gauge_keeps_its_hour_base_whole_past_its_quota() {
        // Team: 2 880 of 2 GB × 720 h.
        let mut app = billing_app(storage_only("team", 2_880.0));
        assert_shown_at_every_size(&mut app, "200 %   base 720 h");
        // Enterprise: 54 000 of 50 GB × 720 h.
        let mut app = billing_app(storage_only("enterprise", 54_000.0));
        assert_shown_at_every_size(&mut app, "150 %   base 720 h");
        // Free: 3 612.34 of 0.5 GB × 720 h, a four-digit used figure.
        let mut app = billing_app(storage_only("free", 3_612.34));
        assert_shown_at_every_size(&mut app, "1003 %   base 720 h");
    }

    /// The no-usage wording is for an empty month only. A month with usage
    /// and no known plan keeps `formule inconnue` on the storage line itself;
    /// the minutes line says it too, so the needle is the storage text whole.
    #[test]
    fn the_storage_block_keeps_formule_inconnue_for_a_month_with_usage() {
        let mut app = billing_app(exec_d_september());
        assert_shown_at_every_size(&mut app, "371.85 GB-h   formule inconnue, pas de quota");
        assert_absent_at_every_width(&mut app, NO_USAGE);
    }

    /// A name cut to its 20 cells beside a five-digit figure: one space still
    /// parts them, or the row reads as a single word.
    #[test]
    fn the_storage_block_parts_a_cut_name_from_a_five_digit_figure() {
        let mut org = exec_d_september();
        org.billing = Some(BillingReport {
            items: vec![usage(
                "2026-09",
                "Actions storage",
                "GigabyteHours",
                12_345.67,
                0.0,
                "ptitjardinier-app-monorepo",
            )],
        });
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "ptitjardinier-app-m… 12345.67 GB-h");
    }

    /// `org` with the budgets stage 1 read: `None` when the listing itself
    /// could not be read, `Some(vec![])` when the org simply has none.
    fn with_budgets(mut org: OrgSummary, budgets: Option<Vec<Budget>>) -> App {
        org.budgets = budgets;
        billing_app(org)
    }

    /// The organization-wide Actions budget, `amount` whole US dollars.
    fn actions(amount: u64, blocking: bool) -> Budget {
        Budget {
            budget_type: "ProductPricing".into(),
            sku: "actions".into(),
            scope: "organization".into(),
            amount,
            blocking,
        }
    }

    /// 2 850 of Team's 3 000 minutes: 95 %. An illustrative figure — #14
    /// gives budgets, not a 95 % month.
    fn team_at_95_percent() -> OrgSummary {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        org.billing = Some(BillingReport {
            items: vec![usage(
                "2026-09",
                "Actions Linux",
                "Minutes",
                2_850.0,
                17.1,
                "disconnected",
            )],
        });
        org
    }

    #[test]
    fn budget_line_reads_zero_dollars_blocking() {
        let mut app = with_budgets(exec_d_september(), Some(vec![actions(0, true)]));
        assert_shown_at_every_size(&mut app, "Budget Actions : 0.00 $ · bloquant");
    }

    #[test]
    fn budget_line_reads_five_dollars_blocking() {
        let mut org = exec_d_september();
        org.login = "cloudalpes".into();
        let mut app = with_budgets(org, Some(vec![actions(5, true)]));
        assert_shown_at_every_size(&mut app, "Budget Actions : 5.00 $ · bloquant");
    }

    #[test]
    fn budget_line_reads_no_budget_as_billed_overage() {
        let mut org = exec_d_september();
        org.login = "SecondBrain-io".into();
        let mut app = with_budgets(org, Some(vec![]));
        assert_shown_at_every_size(
            &mut app,
            "Budget Actions : aucun, dépassement facturé sans plafond",
        );
        assert_shown_at_every_size(&mut app, "(si un moyen de paiement est enregistré)");
        // Scoped: Task 14's retention line may say `illisible` on its own.
        assert_absent_at_every_width(&mut app, "Budget Actions : illisible");
    }

    #[test]
    fn budget_line_reads_unreadable_budgets() {
        let mut org = exec_d_september();
        org.login = "le-vilain-petit-dev".into();
        let mut app = with_budgets(org, None);
        assert_shown_at_every_size(&mut app, "Budget Actions : illisible");
        assert_shown_at_every_size(
            &mut app,
            "(réservé aux admins et gestionnaires de facturation)",
        );
        assert_absent_at_every_width(&mut app, "aucun, dépassement");
    }

    /// A budget on one Actions SKU has never been observed on the author's
    /// organizations, so the tab names it and says it means nothing to the
    /// warnings, rather than folding it into the product budget.
    ///
    /// Two of them, the second alert-only: with a single entry a loop
    /// rendering only the first would pass, and the `alerte seule` arm would
    /// never be rendered by any test.
    #[test]
    fn a_sku_budget_line_is_signalled_not_interpreted() {
        let sku = |name: &str, amount: u64, blocking: bool| Budget {
            budget_type: "SkuPricing".into(),
            sku: name.into(),
            scope: "organization".into(),
            amount,
            blocking,
        };
        let mut app = with_budgets(
            exec_d_september(),
            Some(vec![
                actions(0, true),
                sku("actions_linux", 5, true),
                sku("actions_windows", 7, false),
            ]),
        );
        assert_shown_at_every_size(&mut app, "Budget SKU actions_linux : 5.00 $ · bloquant");
        assert_shown_at_every_size(
            &mut app,
            "Budget SKU actions_windows : 7.00 $ · alerte seule",
        );
        assert_shown_at_every_size(
            &mut app,
            "signalé, non pris en compte par les avertissements",
        );
    }

    #[test]
    fn a_blocking_budget_at_95_percent_warns_under_the_gauge() {
        let mut app = with_budgets(team_at_95_percent(), Some(vec![actions(0, true)]));
        assert_shown_at_every_size(
            &mut app,
            "⚠ 95 % du quota de minutes, budget 0.00 $ bloquant :",
        );
        assert_shown_at_every_size(
            &mut app,
            "GitHub bloquera l'usage Actions au quota atteint.",
        );

        // *Under the gauge*: the warning is the row right after the figures
        // it comments on, not merely somewhere on the tab.
        let s = screen(&mut app, 100, 50);
        let row_of = |needle: &str| {
            s.lines()
                .position(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("{needle:?} missing:\n{s}"))
        };
        assert_eq!(
            row_of("⚠ 95 % du quota de minutes"),
            row_of("2 850 / 3 000") + 1,
            "the warning belongs directly under the gauge:\n{s}"
        );
    }

    #[test]
    fn a_five_dollar_blocking_budget_warns_it_bills_then_blocks() {
        let mut app = with_budgets(team_at_95_percent(), Some(vec![actions(5, true)]));
        assert_shown_at_every_size(
            &mut app,
            "⚠ 95 % du quota de minutes, budget 5.00 $ bloquant :",
        );
        assert_shown_at_every_size(
            &mut app,
            "facturé jusqu'à 5.00 $, puis usage Actions bloqué.",
        );
    }

    /// exec-d's real September storage, on Free, with its real 0 $ blocking
    /// Actions budget: 103 % of 360 GB-h — the storage gauge warns too.
    #[test]
    fn the_storage_gauge_warns_on_a_blocking_budget_too() {
        let mut org = exec_d_september();
        org.plan = Some("free".into());
        let mut app = with_budgets(org, Some(vec![actions(0, true)]));
        assert_shown_at_every_size(
            &mut app,
            "⚠ 103 % du quota de stockage, budget 0.00 $ bloquant :",
        );
        // 1 004 of Free's 2 000 minutes is 50 %: no minutes warning.
        assert_absent_at_every_width(&mut app, "du quota de minutes");
    }

    /// The same 95 % month must stay quiet when nothing will be blocked, or
    /// when nobody can say: no budget, an alert-only budget, unreadable.
    #[test]
    fn no_budget_warning_without_a_blocking_budget() {
        for budgets in [Some(vec![]), Some(vec![actions(5, false)]), None] {
            let mut app = with_budgets(team_at_95_percent(), budgets);
            // The 95 % gauge is on screen, so the absence below is not vacuous.
            assert_shown_at_every_size(&mut app, "2 850 / 3 000");
            assert_absent_at_every_width(&mut app, "du quota de");
        }
    }

    /// Two months at 95 %, so only the month decides: the newest is the one
    /// still running, and the only one GitHub can still block.
    fn two_months_at_95_percent() -> OrgSummary {
        let mut org = team_at_95_percent();
        org.billing = Some(BillingReport {
            items: vec![
                usage(
                    "2026-08",
                    "Actions Linux",
                    "Minutes",
                    2_850.0,
                    17.1,
                    "disconnected",
                ),
                usage(
                    "2026-09",
                    "Actions Linux",
                    "Minutes",
                    2_850.0,
                    17.1,
                    "disconnected",
                ),
            ],
        });
        org
    }

    /// Pre-flight 5.1: the warning is in the future tense — GitHub *will*
    /// block — so it belongs to the month still running. Paged back with `←`,
    /// the same 95 % against the same blocking budget says nothing: that
    /// month's outcome is already settled.
    #[test]
    fn an_older_month_never_warns_about_a_blocking_budget() {
        let mut app = with_budgets(two_months_at_95_percent(), Some(vec![actions(0, true)]));
        assert_shown_at_every_size(&mut app, "2026-09 ·");
        assert_shown_at_every_size(&mut app, "⚠ 95 % du quota de minutes");

        app.month_cursor = 1;
        assert_shown_at_every_size(&mut app, "2026-08 ·");
        assert_shown_at_every_size(&mut app, "2 850 / 3 000");
        assert_absent_at_every_width(&mut app, "du quota de");
    }

    /// The usage report and the budgets are two endpoints, refused
    /// separately: an organization whose report is unreadable can still have
    /// a budget worth knowing about, and the tab says the one it read rather
    /// than falling silent on both.
    #[test]
    fn an_unreadable_report_still_shows_the_budget() {
        let mut org = exec_d_september();
        org.billing = None;
        let mut app = with_budgets(org, Some(vec![actions(0, true)]));
        // Cut short: the whole sentence is wider than a 60-column frame.
        assert_shown_at_every_size(&mut app, "⚠ facturation illisible");
        assert_shown_at_every_size(&mut app, "Budget Actions : 0.00 $ · bloquant");
        // No report, no month, no gauge — so nothing to warn under.
        assert_absent_at_every_width(&mut app, "du quota de");
    }

    /// `org` with the retention stage 1 read: `None` when GitHub refused it
    /// (no `admin:org`), `Some(days)` otherwise. 400 days is the ceiling
    /// GitHub reports, and the only figure `maximum_allowed_days` may show.
    fn with_retention(mut org: OrgSummary, days: Option<u32>) -> App {
        org.retention = days.map(|days| ArtifactRetention {
            days,
            maximum_allowed_days: Some(400),
        });
        billing_app(org)
    }

    /// exec-d before its change: 90 days, 371.85 GB-h in September — the
    /// setting that made its storage overflow.
    #[test]
    fn retention_line_flags_ninety_days_when_storage_counts() {
        let mut app = with_retention(exec_d_september(), Some(90));
        assert_shown_at_every_size(
            &mut app,
            "⚠ Rétention artefacts et journaux : 90 j (max. 400 j)",
        );
        assert_shown_at_every_size(&mut app, "c'est ce réglage qui fait durer le stockage");
    }

    /// exec-d after its change. Asserting the highlighted form is absent —
    /// not merely that "7 j" is present — is what fails a highlight that
    /// ignores `days`.
    #[test]
    fn retention_line_leaves_seven_days_quiet() {
        let mut app = with_retention(exec_d_september(), Some(7));
        assert_shown_at_every_size(
            &mut app,
            "Rétention artefacts et journaux : 7 j (max. 400 j)",
        );
        assert_absent_at_every_width(&mut app, "⚠ Rétention");
        assert_absent_at_every_width(&mut app, "fait durer le stockage");
    }

    /// 90 days on an org holding what systm-d/josephine alone held in
    /// September (12.9 GB-h): below 36, no highlight. The storage half of
    /// the rule must be able to fail too.
    #[test]
    fn retention_line_leaves_ninety_days_quiet_when_storage_is_negligible() {
        let mut org = exec_d_september();
        org.billing = Some(BillingReport {
            items: vec![usage(
                "2026-09",
                "Actions storage",
                "GigabyteHours",
                12.9,
                0.0,
                "josephine",
            )],
        });
        let mut app = with_retention(org, Some(90));
        assert_shown_at_every_size(
            &mut app,
            "Rétention artefacts et journaux : 90 j (max. 400 j)",
        );
        assert_absent_at_every_width(&mut app, "⚠ Rétention");
    }

    #[test]
    fn retention_line_reads_unreadable() {
        let mut app = with_retention(exec_d_september(), None);
        assert_shown_at_every_size(&mut app, "Rétention artefacts et journaux : illisible");
        assert_shown_at_every_size(&mut app, "(scope admin:org requis pour la lire)");
    }

    /// The two exact points of #15, on a readable and an unreadable billing
    /// report alike: they are true whatever the tab can read.
    #[test]
    fn the_tab_carries_both_retention_notes() {
        let mut unreadable = exec_d_september();
        unreadable.billing = None;
        for org in [exec_d_september(), unreadable] {
            let mut app = with_retention(org, Some(7));
            for note in RETENTION_NOTES {
                assert_shown_at_every_size(&mut app, note.trim_start());
            }
        }
    }
}
