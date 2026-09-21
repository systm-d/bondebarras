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

/// A run of rows the tab shows whole or not at all.
///
/// The tab is a `Paragraph` with no scroll, so a body shorter than its
/// content simply drops the tail — and smoke S2 caught that tail falling
/// inside a sentence: at 80x24 the last row on screen was `Note :
/// retention-days, dans un workflow, fixe la durée`, with the rest of that
/// sentence gone. A sentence written across two rows is one passage, and
/// `passages_within` keeps it whole or leaves it out. Half a sentence is the
/// prose equivalent of the clipped figures FR-tui-1 to FR-tui-3 removed.
///
/// A row that carries a whole statement on its own — the header, the month,
/// a gauge, a breakdown row, a blank separator — is a passage of one row,
/// cut like any other row.
///
/// Named a passage rather than a block: `ratatui::widgets::Block` is the
/// panel's frame, which `render` draws right below.
type Passage = Vec<Line<'static>>;

/// How many rows `passages` take together.
fn lines_in(passages: &[Passage]) -> usize {
    passages.iter().map(Vec::len).sum()
}

/// The rows of `passages`, top to bottom, while each whole passage still
/// fits in `height`.
///
/// Stops at the first passage too tall for what is left rather than skipping
/// it and going on: the tab loses its content from the bottom, in order, so
/// what a shorter terminal drops stays predictable, and nothing from further
/// down is promoted over what was dropped. The rows a dropped passage would
/// have taken stay blank — a row bought at the price of half a sentence is
/// not a row worth having.
fn passages_within(passages: Vec<Passage>, height: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for passage in passages {
        if lines.len() + passage.len() > height {
            break;
        }
        lines.extend(passage);
    }
    lines
}

/// What GitHub's documentation insists on, split in two so both halves
/// survive a narrow frame: deleting artifacts stops the accumulation but
/// refunds nothing already counted.
const DELETION_DOES_NOT_REFUND: [&str; 2] = [
    "Supprimer des artefacts arrête l'accumulation,",
    "  mais ne rend pas les GB-heures déjà comptées.",
];

/// The two exact points #15 asks the tab to state, split to stay legible in
/// a narrow frame. True whatever the setting, so always shown.
///
/// A note, not a line, is the unit: each entry holds the two rows one
/// sentence takes, because a sentence is shown whole or not at all (smoke
/// S2 — at 80x24 the tab used to end on "… fixe la durée", the first row of
/// the first note, with the rest of its sentence cut). `passages_within`
/// enforces that; nesting the rows here is what lets it.
const RETENTION_NOTES: [[&str; 2]; 2] = [
    [
        "Note : retention-days, dans un workflow, fixe la durée",
        "  de cet artefact, dans la limite de ce réglage.",
    ],
    [
        "Note : un changement de rétention ne vaut que pour",
        "  les nouveaux artefacts et journaux.",
    ],
];

/// What the storage gauge says for a report with no month at all: without a
/// month there is no hour count to build a quota from, whatever the plan.
const NO_USAGE: &str = "aucun usage signalé";

/// Why a gauge shows a total with no percentage: either no plan was read at
/// all (`formule inconnue`), or one was — the header already names it — but
/// this crate has no included-quota figure for it (`quota inconnu`). Review
/// T5-m5: the two are different facts, and read as one contradicted the
/// other — the header saying `· formule legacy-plan` while the gauge right
/// below said `formule inconnue`, when the plan was in fact known, only its
/// quota was not.
fn no_quota_reason(plan: Option<&str>) -> String {
    match plan {
        Some(name) => format!("formule {name}, quota inconnu"),
        None => "formule inconnue, pas de quota".to_string(),
    }
}

/// One line summarising allowance consumption.
///
/// Deliberately not clamped at 100 %: an org well past its included minutes
/// is exactly the situation the tab exists to surface. The percentage itself
/// comes from `gauges::percent`, shared with the resources column's two
/// gauges rather than kept as a second copy of the same uncapped,
/// zero-guarded formula. Without a known allowance — no plan read, or a plan
/// this crate has no figure for — the line gives the total and says why
/// there is no percentage (`no_quota_reason`), rather than dividing by a
/// guess.
pub fn gauge_line(used: u64, allowance: Option<u64>, plan: Option<&str>) -> String {
    let Some(allowance) = allowance else {
        return format!("{} min   {}", views::thousands(used), no_quota_reason(plan));
    };
    let percent = gauges::percent(used, allowance);
    format!(
        "{} / {}   {}  {} %",
        views::thousands(used),
        views::thousands(allowance),
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
///
/// `used` is grouped the same way `quota.gbh` is (`thousands_gbh`, review
/// FR-tui-2): an Enterprise org near its quota used to read
/// `54000.00 / 36 000 GB-h` — the same quantity, formatted two ways on one
/// line, because only one side of the `/` went through `thousands`.
pub fn storage_gauge_line(used: f64, quota: Option<StorageQuota>, plan: Option<&str>) -> String {
    let Some(quota) = quota else {
        return format!("{} GB-h   {}", thousands_gbh(used), no_quota_reason(plan));
    };
    let percent = storage_percent(used, quota);
    format!(
        "{} / {} GB-h   {}  {} %   base {} h",
        thousands_gbh(used),
        views::thousands(quota.gbh.round() as u64),
        capped_bar(percent, STORAGE_BAR_CELLS),
        percent,
        quota.hours
    )
}

/// A GB-hours figure with its integer part grouped like `views::thousands`
/// groups a whole number, its two decimals kept ungrouped: `54 000.00`, not
/// `54000.00` nor `54 000.00`'s decimals split apart. Built on `{n:.2}`'s
/// own rounding — the same rounding every other GB-hours figure in this
/// module already uses — rather than a second, float-based rounding that
/// could disagree with it by a cent.
fn thousands_gbh(n: f64) -> String {
    let formatted = format!("{n:.2}");
    match formatted.split_once('.') {
        Some((whole, cents)) => match whole.parse::<u64>() {
            Ok(whole) => format!("{}.{cents}", views::thousands(whole)),
            // Never observed (GB-hours are never negative), but a figure
            // `thousands` cannot group is shown as `{n:.2}` gave it rather
            // than panicking.
            Err(_) => formatted,
        },
        None => formatted,
    }
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

/// Widest a repository name gets in `minute_line_row` before it is cut with
/// `…` (`views::fit`, review FR-tui-1). The row's other fields — the
/// leading indent, the quantity, a separating space, the SKU label and the
/// equivalent — need 34 cells at their nominal, padded-but-unbounded
/// widths (`3 + 8 + 1 + 12 + 10`); this leaves 20 of the 58-cell inner
/// width a 60-column frame gives the row (matching `storage_line_row`'s own
/// `views::fit(&line.repo, 20)` just below), with 4 cells of slack for a
/// quantity or equivalent that runs slightly past its own nominal width —
/// the same margin `STORAGE_BAR_CELLS` leaves the hour base. Without this,
/// a name past 12 characters (`claudine-landing-positioning`, 28, already a
/// fixture elsewhere in this branch) pushed the row past 58 cells, and the
/// equivalent figure was clipped into a different, shorter number
/// (`1 004` -> `1 0`).
const MINUTE_REPO_WIDTH: usize = 20;

/// How wide an explanation needing rows of its own is broken to
/// (`views::wrap_words`), its two-space indent not counted.
///
/// 54 plus that indent is 56 cells, the line budget this tab works to (plan
/// item 4.16), well inside the 58 a 60-column frame leaves the panel. Broken
/// at spaces, so no word of it is ever cut.
const REASON_WIDTH: usize = 54;

/// One row of the per-repository breakdown, in the shape of the design
/// mockup: repo, raw quantity, runner (with its multiplier), equivalent.
fn minute_line_row(line: &MinuteLine) -> Line<'static> {
    Line::from(Span::styled(
        format!(
            "   {}{:>8} {:<12}{:>10}",
            views::fit(&line.repo, MINUTE_REPO_WIDTH),
            views::thousands(line.quantity),
            sku_label(&line.sku),
            views::thousands(line.equivalent),
        ),
        theme::muted(),
    ))
}

/// One row of the storage breakdown: the repository, cut to its 20 cells so
/// the GB-hours keep their column, a space, then its GB-hours right-aligned
/// on 14 cells (room for `99 999.99 GB-h`). The space is written out rather
/// than left to the padding: a figure as wide as its field has none, and a
/// cut name would run into it.
///
/// Grouped through `thousands_gbh` (review RW-2): the storage gauge a few
/// rows above reads `54 000.00 / 36 000 GB-h`, and the same quantity left
/// ungrouped here read as a different number. Grouping costs the field one
/// cell — 13 to 14 — which the row absorbs: 38 of the 58 cells a 60-column
/// frame leaves inside the panel.
fn storage_line_row(line: &StorageLine) -> Line<'static> {
    let amount = format!("{} GB-h", thousands_gbh(line.gbh));
    Line::from(Span::styled(
        format!("   {} {amount:>14}", views::fit(&line.repo, 20)),
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
fn budget_lines(budgets: Option<&[Budget]>) -> Vec<Passage> {
    let Some(budgets) = budgets else {
        return vec![vec![
            Line::from(Span::styled("Budget Actions : illisible", theme::muted())),
            Line::from(Span::styled(
                "  (réservé aux admins et gestionnaires de facturation)",
                theme::muted(),
            )),
        ]];
    };
    let mut lines = match billing::actions_budget(budgets) {
        Some(b) if b.blocking => vec![vec![Line::from(Span::styled(
            format!("Budget Actions : {} · bloquant", usd(b.amount as f64)),
            theme::text_style(),
        ))]],
        Some(b) => vec![vec![Line::from(Span::styled(
            format!(
                "Budget Actions : {} · alerte seule, sans blocage",
                usd(b.amount as f64)
            ),
            theme::text_style(),
        ))]],
        None => vec![vec![
            Line::from(Span::styled(
                "Budget Actions : aucun, dépassement facturé sans plafond",
                theme::text_style(),
            )),
            Line::from(Span::styled(
                "  (si un moyen de paiement est enregistré)",
                theme::muted(),
            )),
        ]],
    };
    // Never observed, so named rather than interpreted.
    for b in billing::actions_sku_budgets(budgets) {
        let mode = if b.blocking {
            "bloquant"
        } else {
            "alerte seule"
        };
        // Review T12-m3: `mode` right after "SKU" rather than trailing the
        // line, so a long SKU name (`b.sku` is never bounded — GitHub's own
        // name, past roughly 27 characters at 60 columns) cannot push the
        // `bloquant` / `alerte seule` distinction — the whole reason this
        // line exists — off the frame. Ratatui clips a line's tail when it
        // outgrows its area, never its head, so anything placed early
        // always survives.
        lines.push(vec![
            Line::from(Span::styled(
                format!("Budget SKU {mode} {} : {}", b.sku, usd(b.amount as f64)),
                theme::muted(),
            )),
            Line::from(Span::styled(
                "  signalé, non pris en compte par les avertissements",
                theme::muted(),
            )),
        ]);
    }
    lines
}

/// Under a gauge at `billing::BUDGET_WARNING_PERCENT` or more with a
/// blocking Actions budget: what GitHub will do once the allowance runs out.
/// Takes the percentage the gauge displays, so the two never disagree.
fn budget_warning_lines(quota: &str, percent: u64, budget: Option<&Budget>) -> Vec<Passage> {
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
    // One passage: the header ends on the colon that introduces the
    // consequence, so a frame that cannot hold both shows neither.
    vec![vec![
        // Review T12-m3: `bloquant :` right after the amount, ahead of
        // `percent` and `quota`, so a five-digit budget with a four-digit
        // percentage (`9999 % du quota de stockage`, the widest realistic
        // case at 60 columns) cannot clip the trailing colon that
        // introduces `consequence` on the next line — the same
        // head-survives-a-clipped-tail reasoning as the SKU budget line
        // just above.
        Line::from(Span::styled(
            format!(
                "⚠ budget {} bloquant : {percent} % du quota de {quota}",
                usd(b.amount as f64)
            ),
            theme::status_warn(),
        )),
        Line::from(Span::styled(consequence, theme::status_warn())),
    ]]
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
/// a response, and the line says so — as the cache gauge says the same of
/// its own included threshold.
fn month_line(month: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("{month} · quota documenté de la formule actuelle"),
        theme::muted(),
    ))
}

/// On `enterprise`, the allowance belongs to the enterprise account and is
/// shared by its organizations. bondebarras only sees this one org's usage,
/// so every percentage below is a floor — and the tab says so.
fn enterprise_lines(plan: Option<&str>) -> Vec<Passage> {
    if plan != Some("enterprise") {
        return Vec::new();
    }
    vec![vec![
        Line::from(Span::styled(
            "Formule enterprise : quota partagé par tout le compte",
            theme::muted(),
        )),
        Line::from(Span::styled(
            "  entreprise, ces pourcentages sont des minimums.",
            theme::muted(),
        )),
    ]]
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

/// How many of `rows_len` rows a breakdown may show as its own lines before
/// it must summarise the rest instead, given `share` — its slice of the
/// body height `tab_lines` computed after every fixed line took its own
/// (final-review-inputs.md, "the no-scroll item").
///
/// `MAX_BREAKDOWN_LINES` stays the ceiling whenever `share` has room for it
/// and the one summary line it may still need — the ordinary case, where
/// the height sweep in `tab_lines`'s own tests passes 50 rows and neither
/// breakdown is capped below it. Only once `share` cannot hold that does
/// the cap give way to it, always leaving one line free for the summary —
/// so `breakdown` never shows more rows than `share` allows *together with*
/// its own `… et N autre(s)` line, except the zero-share floor: a breakdown
/// given no room at all still shows that line alone, one line over its
/// empty budget, "rather than a silently truncated list with no sign of
/// it."
fn breakdown_cap(rows_len: usize, share: usize) -> usize {
    let ideal = rows_len.min(MAX_BREAKDOWN_LINES);
    let ideal_total = ideal + usize::from(rows_len > MAX_BREAKDOWN_LINES);
    if ideal_total <= share {
        ideal
    } else {
        share.saturating_sub(1)
    }
}

/// The first `cap` rows, then `… et N autre(s) {rest}` when some were left
/// out: a truncation that leaves no trace would bury the count of hidden
/// rows. Every breakdown of the tab truncates through here, `cap` coming
/// from `breakdown_cap`.
fn breakdown<T>(
    rows: &[T],
    row: impl Fn(&T) -> Line<'static>,
    rest: &str,
    cap: usize,
) -> Vec<Passage> {
    let mut lines: Vec<Passage> = rows.iter().take(cap).map(|item| vec![row(item)]).collect();
    if rows.len() > cap {
        lines.push(vec![Line::from(Span::styled(
            format!("   … et {} autre(s) {rest}", rows.len() - cap),
            theme::muted(),
        ))]);
    }
    lines
}

/// The minutes gauge and the budget warning it may carry — everything about
/// the minutes block except its per-repository breakdown, whose length
/// `tab_lines` now decides from the available height rather than always
/// `MAX_BREAKDOWN_LINES`.
fn minutes_fixed_lines(
    report: &BillingReport,
    month: &str,
    private: &HashSet<String>,
    plan: Option<&str>,
    budget: Option<&Budget>,
) -> Vec<Passage> {
    let used = report.included_minutes(month, private);
    let allowance = included_minutes_for(plan);
    let mut lines = vec![vec![
        Line::from(Span::styled(
            "Minutes équivalent-inclus",
            theme::text_style(),
        )),
        Line::from(Span::styled(
            gauge_line(used, allowance, plan),
            theme::text_style(),
        )),
    ]];
    // Smoke S4: 0 against the allowance beside a real bill is right — a
    // public repository's Actions runs are free and never counted — and
    // reads as a contradiction until the tab says why. The resources
    // column's own minutes gauge has carried this sentence since #11; this
    // is that sentence, not a second phrasing of the same fact.
    if used == 0 && report.uncounted_minutes(month, private) > 0 {
        lines.push(
            views::wrap_words(gauges::PUBLIC_REASON, REASON_WIDTH)
                .into_iter()
                .map(|row| Line::from(Span::styled(format!("  {row}"), theme::muted())))
                .collect(),
        );
    }
    // No allowance, no percentage — and so nothing to warn against either:
    // the budget would have no ratio to be near.
    if let Some(allowance) = allowance {
        let percent = gauges::percent(used, allowance);
        lines.extend(budget_warning_lines("minutes", percent, budget));
    }
    lines
}

/// The storage gauge and the budget warning it may carry — everything about
/// the storage block except its per-repository breakdown (see
/// `minutes_fixed_lines`) and the deletion notice, which `tab_lines` places
/// right after the breakdown, as before.
fn storage_fixed_lines(
    report: &BillingReport,
    month: &str,
    plan: Option<&str>,
    budget: Option<&Budget>,
) -> Vec<Passage> {
    let used = report.storage_gbh(month);
    let quota = billing::storage_quota(plan, month);
    // An empty month is a report with no usage: the quota is missing for
    // want of an hour count, whatever the plan, so `formule inconnue` would
    // give the wrong reason.
    let gauge = if month.is_empty() {
        format!("{used:.2} GB-h   {NO_USAGE}")
    } else {
        storage_gauge_line(used, quota, plan)
    };
    let mut lines = vec![vec![
        Line::from(Span::styled(
            "Stockage Actions · GB-heures, dépôts publics compris",
            theme::text_style(),
        )),
        Line::from(Span::styled(gauge, theme::text_style())),
    ]];
    // Same rule as the minutes: no quota, no percentage, no warning.
    if let Some(quota) = quota {
        let percent = storage_percent(used, quota);
        lines.extend(budget_warning_lines("stockage", percent, budget));
    }
    lines
}

/// What GitHub's documentation insists on (`DELETION_DOES_NOT_REFUND`), as
/// styled lines.
fn deletion_notice() -> Passage {
    DELETION_DOES_NOT_REFUND
        .iter()
        .map(|text| Line::from(Span::styled(*text, theme::muted())))
        .collect()
}

/// The retention setting, beside the storage it governs. Highlighted — ⚠,
/// warning colour, and the reason — when it is at least 90 days on an org
/// whose storage counts (`billing::retention_worth_flagging`). Read-only:
/// the tab shows the tap, it does not turn it.
fn retention_lines(retention: Option<ArtifactRetention>, storage_gbh: Option<f64>) -> Passage {
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
fn retention_notes() -> Vec<Passage> {
    RETENTION_NOTES
        .iter()
        .map(|note| {
            note.iter()
                .map(|text| Line::from(Span::styled(*text, theme::muted())))
                .collect()
        })
        .collect()
}

/// The retention setting and the two notes that qualify it: three passages
/// — the setting with its reason, then one per note.
///
/// Review I1: the notes used to close the tab, which is a `Paragraph` with no
/// scroll — so on a 24-row terminal they were the first thing cut, and #15's
/// second point vanished entirely. They describe the setting, not the tab, so
/// they belong directly under it, where the tab's height cannot silence them.
fn retention_block(retention: Option<ArtifactRetention>, storage_gbh: Option<f64>) -> Vec<Passage> {
    let mut passages = vec![retention_lines(retention, storage_gbh)];
    passages.extend(retention_notes());
    passages
}

/// The month's costs, then any runner SKU `sku_multiplier` does not know.
fn cost_block(report: &BillingReport, month: &str) -> Vec<Passage> {
    let (gross, covered, billed) = report.cost(month);
    let style = if billed > 0.0 {
        theme::status_warn()
    } else {
        theme::muted()
    };
    let mut lines = vec![vec![Line::from(Span::styled(
        cost_line(gross, covered, billed),
        style,
    ))]];
    for sku in report.unknown_skus(month) {
        lines.push(vec![Line::from(Span::styled(
            format!("⚠ SKU inconnu, compté ×1 : {sku}"),
            theme::status_warn(),
        ))]);
    }
    lines
}

/// Every passage of the tab for one org, top to bottom — before
/// `passages_within` drops whatever the body cannot hold.
///
/// Split from `tab_lines` so this module's height census can ask what the
/// tab *has* rather than what fits: `tab_lines` truncates to `body_height`
/// by construction, so measuring its length would only ever hand the height
/// back.
///
/// Built from owned lines so the borrow of `app.orgs` ends before rendering,
/// and so each block can be asserted on through the real render.
///
/// `body_height` is the Billing panel's own content height — `render`'s
/// `area.height` less the block's two border rows — the same rect
/// `tui::views::testing` reads back in its tests. Everything on the tab
/// except the two per-repository breakdowns is fixed: header, month,
/// enterprise note, both gauges, budget lines and warnings, the deletion
/// notice, retention line and its two notes, cost block. `tab_lines` sizes
/// those first, then gives what is left of `body_height` to the
/// breakdowns, split between minutes and storage — controller ruling,
/// final-review-inputs.md "the no-scroll item": no scroll, no reordering,
/// the breakdowns yield instead.
///
/// Once the breakdowns have given everything they have, whatever still does
/// not fit is dropped from the bottom by whole passages (`passages_within`) —
/// never inside a sentence, which is what smoke S2 caught it doing at 80x24.
fn tab_passages(org: &OrgSummary, month_cursor: usize, body_height: usize) -> Vec<Passage> {
    let plan = org.plan.as_deref();
    let budgets = org.budgets.as_deref();
    // Built once: the budgets are their own endpoint, refused on their own,
    // so they are shown whether or not the usage report could be read. Each
    // branch below only places them.
    let budget_block = budget_lines(budgets);
    let mut passages: Vec<Passage> = vec![vec![header_line(&org.login, plan)]];

    if let Some(report) = &org.billing {
        let month = displayed_month(report, month_cursor);
        let private = private_repos(org);
        // The warning is in the future tense — GitHub *will* block once the
        // allowance runs out — so it belongs to the report's most recent
        // month. Paged back to an older one, the budget carries no warning:
        // that month's outcome is already settled, whatever its gauges read.
        let budget = budgets
            .and_then(billing::actions_budget)
            .filter(|_| report.months().last() == Some(&month));
        // A readable report with no usage at all has no month: its line would
        // be a bare ` · quota documenté…`.
        let has_month_line = !month.is_empty();
        let enterprise = enterprise_lines(plan);
        let minutes_fixed = minutes_fixed_lines(report, &month, &private, plan, budget);
        let storage_fixed = storage_fixed_lines(report, &month, plan, budget);
        let deletion = deletion_notice();
        let retention = retention_block(org.retention, Some(report.storage_gbh(&month)));
        let cost = cost_block(report, &month);
        let minutes_rows = report.minute_lines(&month, &private);
        let storage_rows = report.storage_lines(&month);

        // Everything above and below the two breakdowns: three blank
        // separators, both fixed prefixes, the deletion notice, retention
        // (with its two notes) and the cost block. What is left of
        // `body_height` once this is subtracted is what the breakdowns get.
        let fixed_lines = 1 // header
            + usize::from(has_month_line)
            + lines_in(&enterprise)
            + lines_in(&budget_block)
            + 1 // blank before the minutes block
            + lines_in(&minutes_fixed)
            + 1 // blank before the storage block
            + lines_in(&storage_fixed)
            + deletion.len()
            + lines_in(&retention)
            + 1 // blank before the cost block
            + lines_in(&cost);
        let remainder = body_height.saturating_sub(fixed_lines);
        // Minutes first: an odd remaining row goes to the block the tab
        // lists first, rather than splitting it arbitrarily.
        let minutes_share = remainder.div_ceil(2);
        let storage_share = remainder - minutes_share;
        let minutes_cap = breakdown_cap(minutes_rows.len(), minutes_share);
        let storage_cap = breakdown_cap(storage_rows.len(), storage_share);

        if has_month_line {
            passages.push(vec![month_line(&month)]);
        }
        passages.extend(enterprise);
        passages.extend(budget_block);
        passages.push(vec![Line::from("")]);
        passages.extend(minutes_fixed);
        passages.extend(breakdown(
            &minutes_rows,
            minute_line_row,
            "ligne(s)",
            minutes_cap,
        ));
        passages.push(vec![Line::from("")]);
        passages.extend(storage_fixed);
        passages.extend(breakdown(
            &storage_rows,
            storage_line_row,
            "dépôt(s)",
            storage_cap,
        ));
        passages.push(deletion);
        passages.extend(retention);
        passages.push(vec![Line::from("")]);
        passages.extend(cost);
    } else {
        passages.push(vec![unreadable_line()]);
        passages.extend(budget_block);
        // Storage unknown: the retention is shown, never highlighted —
        // nobody can say whether it counts.
        passages.extend(retention_block(org.retention, None));
    }

    passages
}

/// The tab's rows for one org, fitted to `body_height` — the Billing
/// panel's own content height, which `render` reads as `area.height` less
/// the block's two border rows.
fn tab_lines(org: &OrgSummary, month_cursor: usize, body_height: usize) -> Vec<Line<'static>> {
    passages_within(tab_passages(org, month_cursor, body_height), body_height)
}

pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let Some(org) = app.orgs.get(app.org_cursor) else {
        f.render_widget(
            Paragraph::new(Span::styled("Aucune organisation.", theme::muted())),
            area,
        );
        return;
    };
    // The Billing panel's own content height: `render`'s `area` less the
    // block's two border rows, the same budget `tab_lines` splits between
    // its fixed content and the two breakdowns.
    let body_height = usize::from(area.height.saturating_sub(2));
    let lines = tab_lines(org, app.month_cursor, body_height);
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
    use crate::model::RepoSummary;
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
        //
        // Plus one more (smoke S2): a needle that *opens* a two-row passage
        // needs the row under it as well, since `passages_within` shows a
        // passage whole or not at all. One row is the exact allowance, not a
        // margin — every multi-row passage this tab emits is two rows, so a
        // needle can never need more than one row past its own. Rows above
        // the needle never grow as the terminal shrinks (only the two
        // breakdowns give room back, and they sit above every needle that
        // has one under it), so this floor stays tight.
        let floor = u16::try_from(row).unwrap() + 1 + 1 + 1 + 2 + 1;
        for height in floor..=50 {
            let s = screen(app, 100, height);
            assert!(
                s.contains(needle),
                "{needle:?} missing at 100x{height}:\n{s}"
            );
        }
    }

    /// `needle` must be on screen at every width from 60 to 200, at a
    /// height (50) generous enough that neither breakdown is capped.
    ///
    /// For a needle that is itself a per-repository breakdown row (a repo
    /// name, or its `… et N autre(s)` summary): since `tab_lines` now sizes
    /// the two breakdowns from the available height (final-review-inputs.md
    /// "the no-scroll item"), such a needle's presence is no longer
    /// height-invariant the way `assert_shown_at_every_size`'s own height
    /// sweep assumes — its floor is derived from the needle's row at height
    /// 50, but a shorter terminal can shrink the breakdown that row belongs
    /// to before it shrinks anything above the needle. The ruling's own
    /// guarantee is scoped the same way: "at 50 rows, nothing is capped."
    fn assert_shown_at_every_width(app: &mut App, needle: &str) {
        for width in 60..=200u16 {
            let s = screen(app, width, 50);
            assert!(s.contains(needle), "{needle:?} missing at {width}x50:\n{s}");
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
        let line = gauge_line(16_369, Some(2_000), Some("team"));
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
        let line = gauge_line(100, Some(0), Some("team"));
        assert!(line.ends_with(" 0 %"), "got: {line}");
        assert!(!line.contains(&u64::MAX.to_string()), "got: {line}");
    }

    #[test]
    fn an_unused_month_reads_zero_percent() {
        // `.contains('0')` would pass on any percentage: the allowance operand
        // "2 000" carries a zero of its own.
        assert!(gauge_line(0, Some(2_000), Some("team")).ends_with(" 0 %"));
    }

    #[test]
    fn gauge_line_without_an_allowance_has_no_percentage() {
        assert_eq!(
            gauge_line(1_004, None, None),
            "1 004 min   formule inconnue, pas de quota"
        );
    }

    /// T5-m5: the plan was read (the header says `· formule legacy-plan`),
    /// but this crate has no included-minutes figure for it — a different
    /// fact from "no plan was read at all", and the gauge must not say
    /// `formule inconnue` when the header just said otherwise. No `%`
    /// either: a quota-less gauge never divides by a guess.
    #[test]
    fn gauge_line_names_the_plan_when_only_its_quota_is_unknown() {
        assert_eq!(
            gauge_line(1_004, None, Some("legacy-plan")),
            "1 004 min   formule legacy-plan, quota inconnu"
        );
        assert!(!gauge_line(1_004, None, Some("legacy-plan")).contains('%'));
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

    /// Why the tab reads `0 / 3 000` beside a real bill, word for word as
    /// the user sees it — typed out here rather than read from the
    /// production constant, the way `views::repo`'s own gauge tests do, so a
    /// change to what the screen says cannot pass unnoticed.
    const PUBLIC_MINUTES_REASON: &str =
        "(dépôt public : minutes Actions gratuites et illimitées, hors plafond)";

    /// The tab's rows as prose: side borders stripped, blank rows dropped,
    /// rows joined by single spaces — so a sentence broken across rows reads
    /// back whole, and one clipped at a border does not.
    fn prose(screen: &str) -> String {
        crate::tui::views::testing::unwrapped(screen)
    }

    /// Smoke S4: exec-d's Billing tab showed `0 / 3 000     0 %` beside
    /// `brut 9.43 $` and explained neither. Both figures are right — the
    /// 1 549 Linux minutes GitHub billed that month belong to `terminus-32`,
    /// a *public* repository, and a public repository's Actions runs never
    /// draw on the allowance — but a reader sees "no minutes used" next to
    /// nearly ten dollars of Actions, and the tab looks like it contradicts
    /// itself. The resources column has had the sentence for this since #11;
    /// the tab now says it in the same words, not a second phrasing.
    ///
    /// The reason is read back as prose, not as a row: it is wider than the
    /// tab's 56-cell line budget and goes on rows of its own, broken at
    /// spaces.
    #[test]
    fn a_zero_allowance_beside_a_real_bill_says_the_minutes_are_public() {
        assert_eq!(
            PUBLIC_MINUTES_REASON,
            gauges::PUBLIC_REASON,
            "the tab and the resources column must say this in the same words"
        );
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        // `terminus-32` is not in `repos`, so it is not private — the one
        // signal that separates a public repository from a private one.
        org.repos = vec![private_repo("disconnected")];
        org.billing = Some(BillingReport {
            items: vec![usage(
                "2026-09",
                "Actions Linux",
                "Minutes",
                1_549.0,
                9.43,
                "terminus-32",
            )],
        });
        let mut app = billing_app(org);
        let s = screen(&mut app, 80, 50);
        assert!(s.contains("0 / 3 000"), "the gauge is not zero:\n{s}");
        assert!(s.contains("brut 9.43 $"), "the bill is not on screen:\n{s}");
        assert!(
            prose(&s).contains(PUBLIC_MINUTES_REASON),
            "the zero allowance is left unexplained beside a real bill:\n{s}"
        );
    }

    /// The other half of the rule, and the one that can fail on a condition
    /// written too loosely: an organization whose minutes *are* private and
    /// counted says nothing of the sort, at any width. Zero is not the
    /// trigger on its own either — `exec_d_september`'s 1 004 minutes are
    /// real, and an unread plan already has its own reason to give.
    #[test]
    fn a_counted_month_never_claims_its_minutes_are_public() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "1 004 / 3 000");
        assert_absent_at_every_width(&mut app, "dépôt public");
    }

    /// No plan, no percentage — anywhere in the tab, not only on the minutes
    /// line. Later blocks (storage, budget warnings) must keep this green.
    #[test]
    fn an_unknown_plan_shows_no_percentage_anywhere_in_the_tab() {
        let mut app = billing_app(exec_d_september());
        assert_shown_at_every_size(&mut app, "formule inconnue, pas de quota");
        assert_absent_at_every_width(&mut app, "%");
    }

    /// T5-m5, through the full render: a plan GitHub actually returned but
    /// this crate has no figure for (`legacy-plan`, unlike the case above,
    /// where no plan was read at all). The header names it
    /// (`· formule legacy-plan`); both gauges must say so too, not the
    /// `formule inconnue` that would flatly contradict it — and, as when no
    /// plan is known at all, no percentage anywhere on the tab, since there
    /// is still no quota to divide by.
    #[test]
    fn a_read_but_unfigured_plan_names_itself_instead_of_saying_inconnue() {
        let mut org = exec_d_september();
        org.plan = Some("legacy-plan".into());
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "exec-d · formule legacy-plan");
        assert_shown_at_every_size(&mut app, "formule legacy-plan, quota inconnu");
        assert_absent_at_every_width(&mut app, "formule inconnue");
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
        // Breakdown-row needles: width-only, at the generous height where
        // neither breakdown is capped (see `assert_shown_at_every_width`).
        assert_shown_at_every_width(&mut app, "depot-08");
        assert_shown_at_every_width(&mut app, "… et 2 autre(s) ligne(s)");
        assert_absent_at_every_width(&mut app, "depot-09");
        assert_absent_at_every_width(&mut app, "depot-10");
    }

    /// Review FR-tui-1: a repository name longer than `MINUTE_REPO_WIDTH`
    /// (`claudine-landing-positioning`, 28 characters, already a fixture
    /// elsewhere in this branch) used to shift every field after it right,
    /// unbounded, until the row outgrew the frame and ratatui clipped the
    /// last few cells off the end — the equivalent figure, cut into a
    /// shorter, wrong number.
    ///
    /// Two lines with different quantities and equivalents so the number
    /// under test (`12 344`, the Windows-repo row's *equivalent*) cannot be
    /// satisfied by anything else on screen — not that row's own quantity
    /// (`6 172`), not the other row's, and not the gauge's total (`12 394`,
    /// the sum of both).
    #[test]
    fn a_long_repo_name_does_not_clip_the_minutes_equivalent_figure() {
        let org = OrgSummary {
            login: "exec-d".into(),
            repos: vec![
                private_repo("claudine-landing-positioning"),
                private_repo("other-repo"),
            ],
            billing: Some(BillingReport {
                items: vec![
                    usage(
                        "2026-09",
                        "Actions Windows",
                        "Minutes",
                        6_172.0,
                        0.0,
                        "claudine-landing-positioning",
                    ),
                    usage(
                        "2026-09",
                        "Actions Linux",
                        "Minutes",
                        50.0,
                        0.0,
                        "other-repo",
                    ),
                ],
            }),
            ..Default::default()
        };
        let mut app = billing_app(org);
        // Breakdown-row needle: width-only sweep (see
        // `assert_shown_at_every_width`), at the 60-column width review
        // FR-tui-1 measured the defect at.
        assert_shown_at_every_width(&mut app, "12 344");
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
        let september = storage_gauge_line(
            371.85,
            billing::storage_quota(Some("free"), "2026-09"),
            Some("free"),
        );
        assert_eq!(
            september,
            "371.85 / 360 GB-h   ██████████  103 %   base 720 h"
        );

        let july = storage_gauge_line(
            371.85,
            billing::storage_quota(Some("free"), "2026-07"),
            Some("free"),
        );
        assert!(july.starts_with("371.85 / 372 GB-h"), "got: {july}");
        assert!(july.ends_with("100 %   base 744 h"), "got: {july}");
    }

    #[test]
    fn storage_gauge_without_a_plan_has_no_percentage() {
        assert_eq!(
            storage_gauge_line(371.85, None, None),
            "371.85 GB-h   formule inconnue, pas de quota"
        );
    }

    /// T5-m5, the storage gauge's own version: a plan was read, but this
    /// crate has no included-storage figure for it, so the gauge names the
    /// plan and says its quota is unknown — never `formule inconnue`, which
    /// would contradict the header naming that very plan right above it.
    #[test]
    fn storage_gauge_names_the_plan_when_only_its_quota_is_unknown() {
        assert_eq!(
            storage_gauge_line(371.85, None, Some("legacy-plan")),
            "371.85 GB-h   formule legacy-plan, quota inconnu"
        );
        assert!(!storage_gauge_line(371.85, None, Some("legacy-plan")).contains('%'));
    }

    /// Review FR-tui-2: `quota.gbh` went through `thousands` but `used` did
    /// not, so an Enterprise org near its quota read `54000.00 / 36 000
    /// GB-h` — the same quantity, formatted two ways, on one line.
    /// `gauge_line` (the minutes gauge) already grouped both sides; this is
    /// its exact illustrative figure, the one `the_storage_gauge_keeps_its_
    /// hour_base_whole_past_its_quota` also uses for "150 %".
    #[test]
    fn storage_gauge_groups_used_like_it_groups_the_quota() {
        let quota = billing::storage_quota(Some("enterprise"), "2026-09")
            .expect("enterprise has a storage quota");
        assert_eq!(
            storage_gauge_line(54_000.0, Some(quota), Some("enterprise")),
            "54 000.00 / 36 000 GB-h   ██████████  150 %   base 720 h"
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
        // Breakdown-row needles: width-only sweep (see
        // `assert_shown_at_every_width`).
        assert_shown_at_every_width(&mut app, "359.88 GB-h");
        assert_shown_at_every_width(&mut app, "11.21 GB-h");

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
        // Breakdown-row needles: width-only sweep (see
        // `assert_shown_at_every_width`).
        assert_shown_at_every_width(&mut app, "depot-08");
        assert_shown_at_every_width(&mut app, "… et 2 autre(s) dépôt(s)");
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
        assert_eq!(long, "   ptitjardinier-app-m…    359.88 GB-h");
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
        // Breakdown-row needle: width-only sweep (see
        // `assert_shown_at_every_width`).
        assert_shown_at_every_width(&mut app, "ptitjardinier-app-m… 12 345.67 GB-h");
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
        assert_shown_at_every_size(&mut app, "Budget SKU bloquant actions_linux : 5.00 $");
        assert_shown_at_every_size(&mut app, "Budget SKU alerte seule actions_windows : 7.00 $");
        assert_shown_at_every_size(
            &mut app,
            "signalé, non pris en compte par les avertissements",
        );
    }

    /// Review T12-m3: a SKU name past roughly 27 characters used to push
    /// ` · bloquant` off a 60-column frame — the one thing the line exists
    /// to say. `mode` now sits right after "SKU", so the widest realistic
    /// SKU name GitHub's API can hand back (a runner identifier with its
    /// core count, well past that threshold) never touches it.
    #[test]
    fn a_long_sku_name_does_not_clip_the_budget_mode_at_sixty_columns() {
        let sku = Budget {
            budget_type: "SkuPricing".into(),
            sku: "actions_linux_64_core_large_runner".into(),
            scope: "organization".into(),
            amount: 5,
            blocking: true,
        };
        let mut app = with_budgets(exec_d_september(), Some(vec![actions(0, true), sku]));
        let s = screen(&mut app, 60, 50);
        assert!(
            s.contains("Budget SKU bloquant actions_linux_64_core_large_runner"),
            "got:\n{s}"
        );
    }

    #[test]
    fn a_blocking_budget_at_95_percent_warns_under_the_gauge() {
        let mut app = with_budgets(team_at_95_percent(), Some(vec![actions(0, true)]));
        assert_shown_at_every_size(
            &mut app,
            "⚠ budget 0.00 $ bloquant : 95 % du quota de minutes",
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
            row_of("95 % du quota de minutes"),
            row_of("2 850 / 3 000") + 1,
            "the warning belongs directly under the gauge:\n{s}"
        );
    }

    #[test]
    fn a_five_dollar_blocking_budget_warns_it_bills_then_blocks() {
        let mut app = with_budgets(team_at_95_percent(), Some(vec![actions(5, true)]));
        assert_shown_at_every_size(
            &mut app,
            "⚠ budget 5.00 $ bloquant : 95 % du quota de minutes",
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
            "⚠ budget 0.00 $ bloquant : 103 % du quota de stockage",
        );
        // 1 004 of Free's 2 000 minutes is 50 %: no minutes warning.
        assert_absent_at_every_width(&mut app, "du quota de minutes");
    }

    /// Review T12-m3: a five-digit budget with a four-digit percentage —
    /// 299 970 of Team's 3 000 minutes is 9 999 % — used to clip the
    /// trailing `:` that introduces the consequence line right under it.
    /// `bloquant :` now sits right after the amount, ahead of `percent` and
    /// `quota`, so it survives regardless of how wide either grows.
    #[test]
    fn a_five_digit_budget_and_four_digit_percent_keep_their_colon_at_sixty_columns() {
        let mut org = exec_d_september();
        org.plan = Some("team".into());
        org.billing = Some(BillingReport {
            items: vec![usage(
                "2026-09",
                "Actions Linux",
                "Minutes",
                299_970.0,
                0.0,
                "disconnected",
            )],
        });
        let mut app = with_budgets(org, Some(vec![actions(99_999, true)]));
        let s = screen(&mut app, 60, 50);
        assert!(
            s.contains("⚠ budget 99999.00 $ bloquant : 9999 % du quota de minutes"),
            "got:\n{s}"
        );
    }

    /// Review RW-3: the same widest case on the *storage* gauge, which the
    /// fix wave's report claimed was covered while the test right above it
    /// exercises `minutes`. `stockage` is one cell longer, so this line is
    /// exactly 58 cells — the whole inner width a 60-column frame leaves the
    /// panel, with nothing to spare. 35 996.4 GB-hours against Free's
    /// 0.5 GB × 720 h is 9 999 %, and the budget is five digits.
    #[test]
    fn a_five_digit_budget_and_four_digit_percent_keep_their_colon_on_the_storage_gauge() {
        let mut app = with_budgets(
            storage_only("free", 35_996.4),
            Some(vec![actions(99_999, true)]),
        );
        let s = screen(&mut app, 60, 50);
        assert!(
            s.contains("⚠ budget 99999.00 $ bloquant : 9999 % du quota de stockage"),
            "got:\n{s}"
        );
        // The line the colon introduces is there too: a colon kept over a
        // consequence that fell off would say nothing.
        assert!(
            s.contains("GitHub bloquera l'usage Actions au quota atteint.")
                || s.contains("facturé jusqu'à 99999.00 $, puis usage Actions bloqué."),
            "got:\n{s}"
        );
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

    /// Two months at 95 %, so only the month decides: the report's most
    /// recent is the only one GitHub can still block.
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
    /// block — so it belongs to the report's most recent month. Paged back
    /// with `←`, the same 95 % against the same blocking budget says nothing:
    /// that month's outcome is already settled.
    #[test]
    fn an_older_month_never_warns_about_a_blocking_budget() {
        let mut app = with_budgets(two_months_at_95_percent(), Some(vec![actions(0, true)]));
        assert_shown_at_every_size(&mut app, "2026-09 ·");
        assert_shown_at_every_size(&mut app, "95 % du quota de minutes");

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

    /// GitHub does not always report a ceiling. Without one the line carries
    /// the days alone — never a ` (max.  j)` with a hole where the figure
    /// should be. `with_retention` always sets one, so this path needs its
    /// own fixture.
    #[test]
    fn retention_line_without_a_maximum_shows_only_the_days() {
        let mut org = exec_d_september();
        org.retention = Some(ArtifactRetention {
            days: 7,
            maximum_allowed_days: None,
        });
        let mut app = billing_app(org);
        assert_shown_at_every_size(&mut app, "Rétention artefacts et journaux : 7 j");
        assert_absent_at_every_width(&mut app, "max.");
    }

    /// Review I1: the tab is a `Paragraph` with no scroll, so its last lines
    /// are simply cut on a short terminal — and #15's two notes closed it, so
    /// on the 80×24 every terminal still has, the whole second point was
    /// invisible. They qualify the setting, not the tab, so they sit directly
    /// under it. The height floor `assert_shown_at_every_size` derives from a
    /// needle's own row cannot catch this, so the size is named outright.
    #[test]
    fn the_retention_notes_fit_an_eighty_by_twenty_four_terminal() {
        let mut app = with_retention(exec_d_september(), Some(7));
        let s = screen(&mut app, 80, 24);
        // Positive anchors: the readable tab, and the line the notes qualify.
        assert!(s.contains("exec-d ·"), "no tab at 80x24:\n{s}");
        assert!(
            s.contains("Rétention artefacts et journaux : 7 j (max. 400 j)"),
            "the retention line is missing at 80x24:\n{s}"
        );
        for note in RETENTION_NOTES.iter().flatten() {
            assert!(
                s.contains(note.trim_start()),
                "{:?} missing at 80x24:\n{s}",
                note.trim_start()
            );
        }
    }

    /// SecondBrain-io as the smoke run read it on a real terminal:
    /// `enterprise` (so the shared-quota note, two rows), no Actions budget
    /// at all (two rows where a blocking one takes one), and a retention
    /// flagged by the storage it governs. One row denser than
    /// `worst_case_org`, which is a `team` organization with a blocking
    /// budget — the fixture the branch called "the worst case" while this
    /// combination existed in the wild.
    fn secondbrain_september() -> OrgSummary {
        let mut org = exec_d_september();
        org.login = "SecondBrain-io".into();
        org.plan = Some("enterprise".into());
        org.budgets = Some(Vec::new());
        org.retention = Some(ArtifactRetention {
            days: 90,
            maximum_allowed_days: Some(400),
        });
        org
    }

    /// Smoke S2, and the rule it settled: a note is shown whole or not at
    /// all. At 80x24 the tab used to end on `Note : retention-days, dans un
    /// workflow, fixe la durée` with `de cet artefact, dans la limite de ce
    /// réglage.` gone — a sentence cut between its two rows — and the height
    /// sweep's own needle was that surviving half, so no test could see it.
    ///
    /// Three fixtures at the smoke run's size, so the property is exercised
    /// in each of the three states it has: exec-d at 7 days has room for
    /// both notes, `worst_case_org` for the first only, and
    /// `secondbrain_september` for neither. The two flags make the loop
    /// non-vacuous — a tab that dropped every note, or one that always had
    /// room, satisfies the all-or-nothing assertion and fails here.
    #[test]
    fn a_retention_note_is_shown_whole_or_not_at_all_at_eighty_by_twenty_four() {
        let mut exec_d = exec_d_september();
        exec_d.retention = Some(ArtifactRetention {
            days: 7,
            maximum_allowed_days: Some(400),
        });

        let mut saw_a_whole_note = false;
        let mut saw_a_dropped_note = false;
        for (who, org) in [
            ("exec-d at 7 days", exec_d),
            ("the blocking-budget case", worst_case_org()),
            ("SecondBrain-io", secondbrain_september()),
        ] {
            let mut app = billing_app(org);
            let s = screen(&mut app, 80, 24);
            assert!(s.contains(" · formule "), "no tab at 80x24 for {who}:\n{s}");
            for note in RETENTION_NOTES {
                let shown = note.map(|row| s.contains(row.trim_start()));
                assert_eq!(
                    shown[0], shown[1],
                    "{who}: {note:?} is half shown at 80x24:\n{s}"
                );
                saw_a_whole_note |= shown[0];
                saw_a_dropped_note |= !shown[0];
            }
            // Nor is any row of a note cut short across the frame: the same
            // sentence, broken the other way.
            for row in s.lines() {
                let drawn = row.trim_matches(|c: char| c == '│' || c.is_whitespace());
                for full in RETENTION_NOTES.iter().flatten().map(|r| r.trim_start()) {
                    assert!(
                        drawn.is_empty() || drawn.len() >= full.len() || !full.starts_with(drawn),
                        "{who}: {drawn:?} is {full:?} cut short at 80x24:\n{s}"
                    );
                }
            }
        }
        assert!(
            saw_a_whole_note && saw_a_dropped_note,
            "the sweep never saw both a note shown whole and a note dropped"
        );
    }

    #[test]
    fn retention_line_reads_unreadable() {
        let mut app = with_retention(exec_d_september(), None);
        assert_shown_at_every_size(&mut app, "Rétention artefacts et journaux : illisible");
        assert_shown_at_every_size(&mut app, "(scope admin:org requis pour la lire)");
    }

    /// The worst realistic case final-review-inputs.md's "no-scroll item"
    /// names: a readable report close enough to a blocking budget to warn
    /// under the minutes gauge, and a retention setting flagged because the
    /// storage it governs is notable. Team (a blocking budget and a 90 %+
    /// gauge both need a real allowance to warn against): 2 850 of 3 000
    /// minutes (95 %), 371.85 GB-h of storage — well past
    /// `NOTABLE_STORAGE_GBH` — and 90-day retention.
    fn worst_case_org() -> OrgSummary {
        let mut org = team_at_95_percent();
        org.billing
            .as_mut()
            .expect("team_at_95_percent carries a report")
            .items
            .push(usage(
                "2026-09",
                "Actions storage",
                "GigabyteHours",
                371.85,
                0.0,
                "disconnected",
            ));
        org.budgets = Some(vec![actions(0, true)]);
        org.retention = Some(ArtifactRetention {
            days: 90,
            maximum_allowed_days: Some(400),
        });
        org
    }

    /// One organization for the height census below: a plan, a budget, a
    /// retention setting, whether each gauge sits near enough its quota for
    /// a blocking budget to warn under it, whether the month holds a SKU
    /// `sku_multiplier` does not know — one more row in the cost block — and
    /// whether its minutes belong to a public repository, which costs the
    /// minutes block the two rows of `gauges::PUBLIC_REASON` (smoke S4).
    ///
    /// The figures are derived from the plan rather than written down, so
    /// "near its quota" means 95 % of whatever that plan includes, for every
    /// plan the census walks.
    fn census_org(
        plan: Option<&str>,
        budgets: Option<Vec<Budget>>,
        retention: Option<ArtifactRetention>,
        minutes_near_quota: bool,
        storage_near_quota: bool,
        unknown_sku: bool,
        public_minutes: bool,
    ) -> OrgSummary {
        let allowance = included_minutes_for(plan).unwrap_or(2_000) as f64;
        let minutes = if minutes_near_quota {
            allowance * 0.95
        } else {
            10.0
        };
        let quota = billing::storage_quota(plan, "2026-09").map_or(360.0, |q| q.gbh);
        // Past `NOTABLE_STORAGE_GBH` either way, so a 90-day retention is
        // flagged whether or not the gauge is near its quota.
        let storage = if storage_near_quota {
            quota * 0.95
        } else {
            40.0
        };
        // Whose minutes these are decides whether they count at all: a
        // repository absent from `repos` is not private, so its minutes are
        // free, the gauge reads 0, and the tab explains that zero instead of
        // warning about it.
        let minutes_repo = if public_minutes {
            "terminus-32"
        } else {
            "disconnected"
        };
        let mut items = vec![
            usage(
                "2026-09",
                "Actions Linux",
                "Minutes",
                minutes,
                6.0,
                minutes_repo,
            ),
            usage(
                "2026-09",
                "Actions storage",
                "GigabyteHours",
                storage,
                0.1,
                "disconnected",
            ),
        ];
        if unknown_sku {
            items.push(usage(
                "2026-09",
                "Actions Neptune",
                "Minutes",
                5.0,
                0.0,
                minutes_repo,
            ));
        }
        OrgSummary {
            login: "exec-d".into(),
            repos: vec![private_repo("disconnected")],
            plan: plan.map(str::to_string),
            budgets,
            retention,
            billing: Some(BillingReport { items }),
            ..Default::default()
        }
    }

    /// 90 days on an organization whose storage counts: the flagged
    /// retention line, two rows, plus its two notes.
    fn flagged_retention() -> Option<ArtifactRetention> {
        Some(ArtifactRetention {
            days: 90,
            maximum_allowed_days: Some(400),
        })
    }

    /// The densest tab this code can build, named rather than found by
    /// accident — `enterprise` (the shared-quota note, two rows), a blocking
    /// Actions budget with *both* gauges near enough to carry its warning
    /// (two rows each, where "no budget" would buy one row and no warning at
    /// all), a flagged retention (two rows and its two notes), and a SKU the
    /// month's report names but `sku_multiplier` does not know (one row in
    /// the cost block).
    ///
    /// The census test asserts this fixture *is* the maximum, so a denser
    /// combination appearing later fails there rather than going unnoticed.
    fn densest_org() -> OrgSummary {
        census_org(
            Some("enterprise"),
            Some(vec![actions(0, true)]),
            flagged_retention(),
            true,
            true,
            true,
            false,
        )
    }

    /// Every combination of the four independent yes/no facts `census_org`
    /// takes, as `[minutes near quota, storage near quota, unknown SKU,
    /// public minutes]` — flattened so the census below stays four loops
    /// deep instead of seven.
    fn flag_sets() -> Vec<[bool; 4]> {
        let mut out = Vec::new();
        for minutes_near in [false, true] {
            for storage_near in [false, true] {
                for unknown_sku in [false, true] {
                    for public_minutes in [false, true] {
                        out.push([minutes_near, storage_near, unknown_sku, public_minutes]);
                    }
                }
            }
        }
        out
    }

    /// The terminal height at which `org`'s tab shows everything it has: the
    /// smallest body height that holds every passage `tab_passages` builds
    /// for it, plus the five rows the layout spends around the panel — its
    /// two border rows, the header, the status line and the footer.
    ///
    /// A fixed point, not a sum written out by hand: how much the two
    /// breakdowns yield depends on the very height being measured, so the
    /// only honest way to ask what a tab needs is to ask the code that
    /// builds it. `tab_passages`, not `tab_lines`: the latter has already
    /// dropped what did not fit, so its length can never exceed the height
    /// and every height would look sufficient.
    fn needed_rows(org: &OrgSummary) -> u16 {
        let body = (1..=120usize)
            .find(|height| lines_in(&tab_passages(org, 0, *height)) <= *height)
            .expect("the tab's content is bounded");
        u16::try_from(body).expect("a body height fits a u16") + 2 + 3
    }

    /// Smoke S1: the documented floor was measured on `worst_case_org`, a
    /// `team` fixture — so the `enterprise` note and the two-row "no budget"
    /// block were never in the count, and a real organization
    /// (SecondBrain-io) needed more rows than the README promised.
    ///
    /// The figure is not patched by hand here either. This walks every
    /// combination the tab can render — five plan variants (including the
    /// two that yield no quota at all), four budget states, three retention
    /// states, each gauge near its quota or not, with and without an unknown
    /// SKU, and with the month's minutes public or private — measures each
    /// through `tab_passages`, and takes the maximum. The README and the
    /// CHANGELOG are then read from disk and must carry that very number:
    /// change what the tab draws and this test fails until the documentation
    /// follows.
    ///
    /// Not covered by the floor, and deliberately: a per-SKU budget adds two
    /// rows and GitHub allows any number of them, as it does of unknown
    /// SKUs past the first. A floor over an unbounded list would be a
    /// different promise.
    #[test]
    fn the_documented_height_floor_is_the_densest_tab_the_code_can_build() {
        let quiet_retention = Some(ArtifactRetention {
            days: 7,
            maximum_allowed_days: Some(400),
        });
        let mut census: Vec<(String, u16)> = Vec::new();
        for plan in [
            None,
            Some("free"),
            Some("team"),
            Some("enterprise"),
            Some("legacy-plan"),
        ] {
            for (budget_name, budgets) in [
                ("illisible", None),
                ("aucun", Some(vec![])),
                ("alerte seule", Some(vec![actions(5, false)])),
                ("bloquant", Some(vec![actions(0, true)])),
            ] {
                for (retention_name, retention) in [
                    ("flagged", flagged_retention()),
                    ("quiet", quiet_retention),
                    ("illisible", None),
                ] {
                    for [minutes_near, storage_near, unknown_sku, public_minutes] in flag_sets() {
                        let org = census_org(
                            plan,
                            budgets.clone(),
                            retention,
                            minutes_near,
                            storage_near,
                            unknown_sku,
                            public_minutes,
                        );
                        census.push((
                            format!(
                                "plan {plan:?}, budget {budget_name}, retention \
                                 {retention_name}, minutes near {minutes_near}, storage near \
                                 {storage_near}, unknown SKU {unknown_sku}, public minutes \
                                 {public_minutes}"
                            ),
                            needed_rows(&org),
                        ));
                    }
                }
            }
        }
        // The census as a table, for the record — `cargo test
        // the_documented_height_floor -- --nocapture` prints it, with the
        // three named fixtures this module measures elsewhere.
        let mut heights: Vec<u16> = census.iter().map(|(_, rows)| *rows).collect();
        heights.sort_unstable();
        heights.dedup();
        for height in heights {
            let example = census
                .iter()
                .find(|(_, rows)| *rows == height)
                .map_or("", |(label, _)| label.as_str());
            println!("{height} rows — e.g. {example}");
        }
        println!(
            "named: worst_case_org {}, secondbrain_september {}, densest_org {}",
            needed_rows(&worst_case_org()),
            needed_rows(&secondbrain_september()),
            needed_rows(&densest_org())
        );

        let (densest, needed) = census
            .iter()
            .max_by_key(|(_, rows)| *rows)
            .expect("the census walks at least one combination");
        let needed = *needed;
        assert_eq!(
            needed,
            needed_rows(&densest_org()),
            "the census found a denser tab than `densest_org`: {densest} needs {needed} rows"
        );

        // The documentation carries the measurement, not a number typed
        // beside it. Read from disk so the two cannot drift apart silently.
        let readme = include_str!("../../../../../README.md");
        let changelog = include_str!("../../../../../CHANGELOG.md");
        assert!(
            readme.contains(&format!("from a {needed}-row terminal")),
            "README.md does not say the measured floor of {needed} rows ({densest})"
        );
        assert!(
            changelog.contains(&format!("needs {needed} rows")),
            "CHANGELOG.md does not say the measured floor of {needed} rows ({densest})"
        );
        // `docs/billing.md` cites the floor twice — once as the figure, once
        // as the shape it was measured on — and #33 found both held by
        // nothing: the page explains how the measurement is taken while
        // being the one document free to disagree with it. Both spellings
        // are checked, so neither half can be left behind.
        let page = include_str!("../../../../../docs/billing.md");
        for claim in [
            format!("from a {needed}-row terminal"),
            format!("the {needed}-row floor"),
        ] {
            assert!(
                page.contains(&claim),
                "docs/billing.md does not say {claim:?}, the measured floor ({densest})"
            );
        }

        // And the measurement is true of the real render, not only of
        // `tab_lines`: at that height the densest tab is whole, and one row
        // short it is not.
        let mut app = billing_app(densest_org());
        let s = screen(&mut app, 80, needed);
        for needle in [
            "Formule enterprise : quota partagé par tout le compte",
            "entreprise, ces pourcentages sont des minimums.",
            "Budget Actions : 0.00 $ · bloquant",
            "⚠ budget 0.00 $ bloquant : 95 % du quota de minutes",
            "⚠ budget 0.00 $ bloquant : 95 % du quota de stockage",
            "GitHub bloquera l'usage Actions au quota atteint.",
            "⚠ Rétention artefacts et journaux : 90 j (max. 400 j)",
            "c'est ce réglage qui fait durer le stockage",
            "Coûts   brut",
            "⚠ SKU inconnu, compté ×1 : Actions Neptune",
        ] {
            assert!(
                s.contains(needle),
                "{needle:?} missing at 80x{needed}:\n{s}"
            );
        }
        for note in RETENTION_NOTES.iter().flatten() {
            assert!(
                s.contains(note.trim_start()),
                "{:?} missing at 80x{needed}:\n{s}",
                note.trim_start()
            );
        }
        let shorter = screen(&mut app, 80, needed - 1);
        assert!(
            !shorter.contains("SKU inconnu"),
            "the floor is not tight: one row short, nothing is lost:\n{shorter}"
        );
    }

    /// Ten repositories each burning both minutes and storage: enough rows
    /// in *both* breakdowns that neither can show them all even generously,
    /// so both `… et N autre(s)` lines are exercised together.
    fn ten_repos_of_both() -> OrgSummary {
        let names: Vec<String> = (1..=10).map(|n| format!("depot-{n:02}")).collect();
        OrgSummary {
            login: "exec-d".into(),
            repos: names.iter().map(|name| private_repo(name)).collect(),
            billing: Some(BillingReport {
                items: names
                    .iter()
                    .zip((1..=10).rev())
                    .flat_map(|(name, weight)| {
                        let weight = f64::from(weight);
                        vec![
                            usage(
                                "2026-09",
                                "Actions Linux",
                                "Minutes",
                                weight * 100.0,
                                0.0,
                                name,
                            ),
                            usage(
                                "2026-09",
                                "Actions storage",
                                "GigabyteHours",
                                weight * 10.0,
                                0.0,
                                name,
                            ),
                        ]
                    })
                    .collect(),
            }),
            ..Default::default()
        }
    }

    /// Controller ruling, final-review-inputs.md "the no-scroll item": the
    /// two breakdowns yield to the available height rather than pushing the
    /// tab's fixed content off the bottom. At 80x24 (the body holds 19
    /// content lines — `the_retention_notes_fit_an_eighty_by_twenty_four_terminal`
    /// measures the same rect), both breakdowns are squeezed to their
    /// `… et N autre(s)` summary — proof the mechanism ran, not that it sat
    /// idle — while the fixed lines ahead of them stay whole: both gauges,
    /// the budget line, the minutes warning, and the retention line with its
    /// explanation and first note.
    ///
    /// What this fixture does *not* claim: with a blocking-budget warning
    /// (2 lines) stacked on a flagged retention (2 lines + 4 notes), the
    /// fixed content alone — before either breakdown contributes a single
    /// row — is 21 lines, already 2 more than the 19-line body; even
    /// reducing both breakdowns to their one-line summary (the least either
    /// can ever show once there is any usage to report) still leaves 23,
    /// short by 4. Neither breakdown can give back a line it does not have,
    /// so the second retention note and the cost line do not fit at 80x24
    /// in this exact combination — and the second note is left out whole
    /// rather than cut after its first row (smoke S2,
    /// `a_retention_note_is_shown_whole_or_not_at_all_at_eighty_by_twenty_four`).
    /// See `the_blocking_budget_case_fits_at_eighty_by_twenty_eight` for the
    /// height at which they do fit, the fallback the ruling itself names
    /// ("accepting the gap with a documented minimum height").
    #[test]
    fn both_breakdowns_yield_to_the_fixed_content_at_eighty_by_twenty_four() {
        let mut app = billing_app(worst_case_org());
        let s = screen(&mut app, 80, 24);
        for needle in [
            "2 850 / 3 000",                                          // minutes gauge
            "371.85 / 1 440 GB-h",                                    // storage gauge
            "Budget Actions : 0.00 $ · bloquant",                     // budget line
            "⚠ budget 0.00 $ bloquant : 95 % du quota de minutes",    // warning
            "GitHub bloquera l'usage Actions au quota atteint.",      // warning, 2nd line
            "⚠ Rétention artefacts et journaux : 90 j (max. 400 j)",  // retention line
            "c'est ce réglage qui fait durer le stockage",            // retention explanation
            "Note : retention-days, dans un workflow, fixe la durée", // retention note 1
            "de cet artefact, dans la limite de ce réglage.",         // retention note 1, cont'd
            "… et 1 autre(s) ligne(s)",                               // minutes breakdown yielded
            "… et 1 autre(s) dépôt(s)",                               // storage breakdown yielded
        ] {
            assert!(s.contains(needle), "{needle:?} missing at 80x24:\n{s}");
        }
    }

    /// Whatever the breakdowns show, their `… et N autre(s)` line is
    /// present — even generously, at 80x24, with ten repositories in each
    /// breakdown (nowhere near this fixture's own worst case above: no
    /// blocking budget, no flagged retention, so there is ample height and
    /// the only question is whether the summary line survives on its own
    /// terms). Can-fail: drop the summary line under a cap, and this fails.
    #[test]
    fn the_et_n_autres_line_survives_whatever_the_breakdowns_show() {
        let mut app = billing_app(ten_repos_of_both());
        let s = screen(&mut app, 80, 24);
        assert!(
            s.contains("autre(s) ligne(s)"),
            "minutes truncation notice missing at 80x24:\n{s}"
        );
        assert!(
            s.contains("autre(s) dépôt(s)"),
            "storage truncation notice missing at 80x24:\n{s}"
        );
    }

    /// The fallback the controller's own ruling names for the gap
    /// `both_breakdowns_yield_to_the_fixed_content_at_eighty_by_twenty_four`
    /// documents: "accepting the gap with a documented minimum height."
    /// Measured (not guessed): this fixture's fixed content plus both
    /// breakdowns' one-line-each floor is 23 lines, so the body needs 23
    /// (content) + 2 (the panel's own border rows) = 25 rows, which this
    /// layout gives at a 28-row terminal (header, status and footer take
    /// the other 3).
    ///
    /// This is one combination's own floor, not the documented one: the
    /// figure the README carries belongs to the densest tab the code can
    /// build, measured by
    /// `the_documented_height_floor_is_the_densest_tab_the_code_can_build`
    /// — a `team` organization with a blocking budget is not it (smoke S1).
    ///
    /// Review RW-4: this doc comment used to claim the breakdowns show
    /// "their real rows rather than a summary" while the test asserted no
    /// breakdown row at all. It asserts them now — one real row on each
    /// side, and neither `… et N autre(s)` line, which is what tells a real
    /// row apart from the summary it degrades to at 80x24.
    #[test]
    fn the_blocking_budget_case_fits_at_eighty_by_twenty_eight() {
        let mut app = billing_app(worst_case_org());
        let s = screen(&mut app, 80, 28);
        for needle in [
            "2 850 / 3 000",
            "371.85 / 1 440 GB-h",
            "Budget Actions : 0.00 $ · bloquant",
            "⚠ budget 0.00 $ bloquant : 95 % du quota de minutes",
            "GitHub bloquera l'usage Actions au quota atteint.",
            "⚠ Rétention artefacts et journaux : 90 j (max. 400 j)",
            "c'est ce réglage qui fait durer le stockage",
        ] {
            assert!(s.contains(needle), "{needle:?} missing at 80x28:\n{s}");
        }
        for note in RETENTION_NOTES.iter().flatten() {
            assert!(
                s.contains(note.trim_start()),
                "{:?} missing at 80x28:\n{s}",
                note.trim_start()
            );
        }
        assert!(
            s.contains("Coûts   brut"),
            "cost line missing at 80x28:\n{s}"
        );
        // Both breakdowns show a real row, not the summary they fall back to
        // at 80x24: the minutes row names its runner, the storage row its
        // GB-hours (the gauge's own `371.85 / 1 440 GB-h` cannot satisfy it).
        assert!(
            s.contains("2 850 Linux"),
            "the minutes breakdown shows no real row at 80x28:\n{s}"
        );
        assert!(
            s.contains("371.85 GB-h"),
            "the storage breakdown shows no real row at 80x28:\n{s}"
        );
        assert!(
            !s.contains("autre(s) ligne(s)") && !s.contains("autre(s) dépôt(s)"),
            "a breakdown is still summarised at 80x28:\n{s}"
        );
    }

    /// A sweep over heights, at the worst case's own fixture: the
    /// load-bearing lines that fit at 80x24 — the gauges, the budget line,
    /// the warning, the retention line and its first note — never
    /// disappear as the terminal grows from there. `tab_lines` only ever
    /// gives the breakdowns *more* room as height grows (`breakdown_cap` is
    /// monotonic in `share`), so nothing that already fit at the smallest
    /// height in the sweep can be pushed back off by a taller one.
    #[test]
    fn the_load_bearing_lines_that_fit_never_disappear_as_height_grows() {
        let mut app = billing_app(worst_case_org());
        for height in 24..=45u16 {
            let s = screen(&mut app, 80, height);
            for needle in [
                "2 850 / 3 000",
                "371.85 / 1 440 GB-h",
                "Budget Actions : 0.00 $ · bloquant",
                "⚠ budget 0.00 $ bloquant : 95 % du quota de minutes",
                "⚠ Rétention artefacts et journaux : 90 j (max. 400 j)",
                "Note : retention-days, dans un workflow, fixe la durée",
            ] {
                assert!(
                    s.contains(needle),
                    "{needle:?} missing at 80x{height}:\n{s}"
                );
            }
        }
    }

    /// The two exact points of #15, on a readable and an unreadable billing
    /// report alike: they are true whatever the tab can read.
    #[test]
    fn the_tab_carries_both_retention_notes() {
        let mut unreadable = exec_d_september();
        unreadable.billing = None;
        for org in [exec_d_september(), unreadable] {
            let mut app = with_retention(org, Some(7));
            for note in RETENTION_NOTES.iter().flatten() {
                assert_shown_at_every_size(&mut app, note.trim_start());
            }
        }
    }
}
