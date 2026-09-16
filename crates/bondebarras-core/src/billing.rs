//! Billing aggregation. Pure calculation over a usage report — no network,
//! no rendering.
//!
//! Minutes cannot be cleaned up retroactively: once burnt they are burnt. All
//! this module can do is say *where they went*, which is the only useful
//! answer for the Actions-minutes axis of the problem.
//!
//! Actions storage is the other axis, and unlike minutes it can be acted on:
//! it is billed in GB-hours — every hour a gigabyte of artifacts exists — so
//! deleting artifacts stops the accumulation, though never the hours already
//! counted. The usage report already carries it per repository; naming the
//! repository holding it is the same job as naming the one burning minutes.

use std::collections::HashSet;

/// One line of GitHub's usage report: a month × a repository × a SKU.
#[derive(Debug, Clone)]
pub struct UsageItem {
    /// `YYYY-MM`, derived from the report's RFC 3339 `date`.
    pub month: String,
    pub product: String,
    pub sku: String,
    pub quantity: f64,
    pub unit_type: String,
    pub gross: f64,
    /// The part absorbed by the plan's included allowance.
    pub discount: f64,
    /// What is actually paid.
    pub net: f64,
    pub repo: String,
}

/// A whole organization's usage report.
#[derive(Debug, Clone, Default)]
pub struct BillingReport {
    pub items: Vec<UsageItem>,
}

/// One row of the per-repository breakdown: which repo ran which runner, and
/// what that costs against the allowance once the multiplier is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinuteLine {
    pub repo: String,
    pub sku: String,
    pub quantity: u64,
    pub equivalent: u64,
}

/// Included Actions minutes per month for an organization's GitHub plan, in
/// Linux-equivalent minutes — GitHub's own table.
///
/// `None` for a plan this crate has no figure for, and for no plan at all:
/// `GET /orgs/{org}` only returns `plan` to an owner. Never a default. A
/// guessed allowance made exec-d, on Team, read 50 % for 1 004 minutes when
/// 33 % was true, and SecondBrain-io, on Enterprise, read 901 % for 36 %.
pub fn included_minutes_for(plan: Option<&str>) -> Option<u64> {
    match plan? {
        "free" => Some(2_000),
        "team" => Some(3_000),
        "enterprise" => Some(50_000),
        _ => None,
    }
}

/// The usage report's Actions storage SKU.
const ACTIONS_STORAGE_SKU: &str = "Actions storage";

/// The unit GitHub bills Actions storage in.
const GIGABYTE_HOURS: &str = "GigabyteHours";

/// One row of the storage breakdown: which repository held how many
/// GB-hours in the month.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageLine {
    pub repo: String,
    pub gbh: f64,
}

/// A plan's included Actions storage for one month, in GB-hours, with the
/// hour base it was computed on — the Billing tab states that base, because
/// it is an open measurement (720 or 744 hours, see the design spec).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StorageQuota {
    pub gbh: f64,
    pub hours: u32,
}

/// Included Actions storage for an organization's plan, in GB — GitHub's
/// table. `None` for no plan or an unknown one, exactly like
/// `included_minutes_for`.
pub fn included_storage_gb_for(plan: Option<&str>) -> Option<f64> {
    match plan? {
        "free" => Some(0.5),
        "team" => Some(2.0),
        "enterprise" => Some(50.0),
        _ => None,
    }
}

/// Hours in a `YYYY-MM` month: its days × 24.
///
/// GitHub's documentation converts GB-hours to GB-months "by dividing by the
/// hours in the month (usually 720 hours for a 30-day month)". exec-d's
/// September 2026 report suggests a 744-hour base instead; that is an open
/// measurement, and this is the one place to change if it is confirmed.
/// `None` for anything that is not a real month.
pub fn hours_in_month(month: &str) -> Option<u32> {
    let (year, mon) = month.split_once('-')?;
    let year: i32 = year.parse().ok()?;
    let mon: u32 = mon.parse().ok()?;
    let first = chrono::NaiveDate::from_ymd_opt(year, mon, 1)?;
    let next = if mon == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        chrono::NaiveDate::from_ymd_opt(year, mon + 1, 1)?
    };
    u32::try_from((next - first).num_days() * 24).ok()
}

/// A plan's included storage for `month`, in GB-hours: included GB × the
/// month's hours. `None` when either is unknown — and then no percentage.
pub fn storage_quota(plan: Option<&str>, month: &str) -> Option<StorageQuota> {
    let gb = included_storage_gb_for(plan)?;
    let hours = hours_in_month(month)?;
    Some(StorageQuota {
        gbh: gb * f64::from(hours),
        hours,
    })
}

/// `YYYY-MM` for an instant, in UTC — how `scan --json` names the current
/// month (`billing_month`). Takes the instant rather than reading the clock,
/// so it stays testable. The TUI never reads the clock for a month: its
/// columns read the newest month the usage report carries.
pub fn month_of(now: chrono::DateTime<chrono::Utc>) -> String {
    now.format("%Y-%m").to_string()
}

fn is_actions_storage(item: &UsageItem) -> bool {
    item.sku == ACTIONS_STORAGE_SKU && item.unit_type == GIGABYTE_HOURS
}

/// One budget from `GET /organizations/{org}/settings/billing/budgets`.
///
/// Read-only, permanently: changing a budget commits money. Every field is
/// required — an entry missing one makes the whole listing unreadable (see
/// `api::budgets::fetch`), since the dropped entry could be the Actions
/// budget, and "no budget: overage billed" would then be said of an org
/// GitHub actually blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    /// `budget_type`: `ProductPricing` or `SkuPricing`.
    pub budget_type: String,
    /// `budget_product_sku`: a product (`actions`) or, for `SkuPricing`, a SKU.
    pub sku: String,
    /// `budget_scope`: `organization`, `repository`, `enterprise`, …
    pub scope: String,
    /// `budget_amount`, whole US dollars — an integer in GitHub's schema.
    pub amount: u64,
    /// `prevent_further_usage`: GitHub stops the usage once the budget is spent.
    pub blocking: bool,
}

/// From this percentage of a gauge, a blocking budget earns a warning.
pub const BUDGET_WARNING_PERCENT: u64 = 90;

/// The organization's Actions budget: scope `organization`, type
/// `ProductPricing`, product `actions`. `None` when there is none — which,
/// on a readable listing, means overage is billed with no ceiling.
pub fn actions_budget(budgets: &[Budget]) -> Option<&Budget> {
    budgets.iter().find(|b| {
        b.scope == "organization" && b.budget_type == "ProductPricing" && b.sku == "actions"
    })
}

/// Organization budgets on a single Actions SKU (`SkuPricing`). Never
/// observed on the author's organizations, so never interpreted: the tab
/// names each one instead of folding it into the product budget's meaning.
pub fn actions_sku_budgets(budgets: &[Budget]) -> Vec<&Budget> {
    budgets
        .iter()
        .filter(|b| {
            b.scope == "organization"
                && b.budget_type == "SkuPricing"
                && b.sku.to_ascii_lowercase().starts_with("actions")
        })
        .collect()
}

/// Whether a gauge at `percent` should warn that GitHub will stop Actions
/// once the allowance runs out: at least `BUDGET_WARNING_PERCENT`, with a
/// blocking Actions budget. Takes the percentage the gauge displays, so the
/// warning can never contradict it.
pub fn nears_blocking_budget(percent: u64, budget: Option<&Budget>) -> bool {
    percent >= BUDGET_WARNING_PERCENT && budget.is_some_and(|b| b.blocking)
}

/// How many Linux-equivalent minutes one minute of this runner costs.
///
/// `None` means the SKU is unknown — a new runner family GitHub added. The
/// caller counts it at ×1 *and* surfaces it, because a silent multiplier
/// would skew the gauge with no way to notice.
pub fn sku_multiplier(sku: &str) -> Option<u32> {
    match sku {
        "Actions Linux" => Some(1),
        "Actions Windows" => Some(2),
        s if s.starts_with("Actions macOS") => Some(10),
        _ => None,
    }
}

impl BillingReport {
    /// Every month present in the report, oldest first.
    pub fn months(&self) -> Vec<String> {
        let mut out: Vec<String> = self.items.iter().map(|i| i.month.clone()).collect();
        out.sort();
        out.dedup();
        out
    }

    /// Minutes charged against the plan's included allowance, in Linux
    /// equivalents.
    ///
    /// Only private repos are counted: a public repository's Actions runs are
    /// free and unlimited, so it never draws on the allowance. `gross <=
    /// discount` is *not* the right test for that — GitHub's usage report
    /// discounts a private repo still inside its allowance exactly the same
    /// way it discounts a public repo, so that filter dropped every
    /// in-allowance private minute along with the public ones and made the
    /// gauge read 0 % until the org had actually started being billed.
    pub fn included_minutes(&self, month: &str, private_repos: &HashSet<String>) -> u64 {
        self.items
            .iter()
            .filter(|i| i.month == month && i.unit_type == "Minutes")
            .filter(|i| private_repos.contains(&i.repo))
            .map(|i| {
                let mult = sku_multiplier(&i.sku).unwrap_or(1) as f64;
                (i.quantity * mult).round() as u64
            })
            .sum()
    }

    /// `(gross, covered, billed)` for the month, all products together.
    pub fn cost(&self, month: &str) -> (f64, f64, f64) {
        self.items
            .iter()
            .filter(|i| i.month == month)
            .fold((0.0, 0.0, 0.0), |(g, c, b), i| {
                (g + i.gross, c + i.discount, b + i.net)
            })
    }

    /// Billable minute usage for the month, heaviest allowance consumer first.
    ///
    /// This is the tab's reason to exist. Minutes are gone once burnt, so the
    /// only useful answer is *which repository burnt them* — an aggregate says
    /// the quota is blown without saying what to go and fix.
    ///
    /// Same filter as `included_minutes`: minute-typed items belonging to a
    /// private repo, so a public repo — free and unlimited — never appears
    /// here.
    pub fn minute_lines(&self, month: &str, private_repos: &HashSet<String>) -> Vec<MinuteLine> {
        let mut out: Vec<MinuteLine> = self
            .items
            .iter()
            .filter(|i| i.month == month && i.unit_type == "Minutes")
            .filter(|i| private_repos.contains(&i.repo))
            .map(|i| {
                let mult = sku_multiplier(&i.sku).unwrap_or(1);
                MinuteLine {
                    repo: i.repo.clone(),
                    sku: i.sku.clone(),
                    quantity: i.quantity.round() as u64,
                    equivalent: (i.quantity * mult as f64).round() as u64,
                }
            })
            .collect();
        out.sort_by_key(|l| std::cmp::Reverse(l.equivalent));
        out
    }

    /// SKUs in this month that `sku_multiplier` does not know.
    pub fn unknown_skus(&self, month: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .items
            .iter()
            .filter(|i| i.month == month && i.unit_type == "Minutes")
            .filter(|i| sku_multiplier(&i.sku).is_none())
            .map(|i| i.sku.clone())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Actions storage used in the month, in GB-hours, public repositories
    /// included: the documentation says a public repository's *minutes* are
    /// free, but says nothing of its storage, and the report discounts both
    /// kinds alike. Counting it is the cautious reading, and the tab says so.
    pub fn storage_gbh(&self, month: &str) -> f64 {
        // Folded from +0.0, not `sum()`: an empty `f64` sum is -0.0.
        self.items
            .iter()
            .filter(|i| i.month == month && is_actions_storage(i))
            .fold(0.0, |total, i| total + i.quantity)
    }

    /// One repository's Actions storage in the month, in GB-hours. A
    /// repository the report does not list for that month held none.
    pub fn storage_gbh_for_repo(&self, month: &str, repo: &str) -> f64 {
        // Folded from +0.0, not `sum()`: an empty `f64` sum is -0.0.
        self.items
            .iter()
            .filter(|i| i.month == month && i.repo == repo && is_actions_storage(i))
            .fold(0.0, |total, i| total + i.quantity)
    }

    /// Storage per repository for the month, heaviest first — the storage
    /// counterpart of `minute_lines`: an alert says the quota is nearly
    /// used, this says which repository to go and clean.
    pub fn storage_lines(&self, month: &str) -> Vec<StorageLine> {
        let mut out: Vec<StorageLine> = Vec::new();
        for i in self
            .items
            .iter()
            .filter(|i| i.month == month && is_actions_storage(i))
        {
            match out.iter_mut().find(|l| l.repo == i.repo) {
                Some(line) => line.gbh += i.quantity,
                None => out.push(StorageLine {
                    repo: i.repo.clone(),
                    gbh: i.quantity,
                }),
            }
        }
        out.sort_by(|a, b| b.gbh.total_cmp(&a.gbh));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(month: &str, sku: &str, qty: f64, gross: f64, discount: f64, repo: &str) -> UsageItem {
        UsageItem {
            month: month.into(),
            product: "actions".into(),
            sku: sku.into(),
            quantity: qty,
            unit_type: "Minutes".into(),
            gross,
            discount,
            net: gross - discount,
            repo: repo.into(),
        }
    }

    fn private_repos(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    /// One `Actions storage` line, in GigabyteHours, as GitHub reports it.
    fn storage(month: &str, gbh: f64, repo: &str) -> UsageItem {
        UsageItem {
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

    #[test]
    fn included_minutes_follow_the_plan() {
        assert_eq!(included_minutes_for(Some("free")), Some(2_000));
        assert_eq!(included_minutes_for(Some("team")), Some(3_000));
        assert_eq!(included_minutes_for(Some("enterprise")), Some(50_000));
    }

    #[test]
    fn an_unknown_or_unread_plan_has_no_minutes_allowance() {
        // `pro` is a real GitHub plan — for a personal account, never an org.
        // No figure here, so no percentage anywhere.
        assert_eq!(included_minutes_for(Some("pro")), None);
        assert_eq!(included_minutes_for(Some("")), None);
        assert_eq!(included_minutes_for(None), None);
    }

    #[test]
    fn multipliers_follow_githubs_billing_ratios() {
        assert_eq!(sku_multiplier("Actions Linux"), Some(1));
        assert_eq!(sku_multiplier("Actions Windows"), Some(2));
        assert_eq!(sku_multiplier("Actions macOS 3-core"), Some(10));
        assert_eq!(sku_multiplier("Actions macOS XL"), Some(10));
        // An unknown runner must be reported, not silently counted as Linux.
        assert_eq!(sku_multiplier("Actions Quantum"), None);
    }

    #[test]
    fn included_minutes_apply_the_multipliers() {
        let r = BillingReport {
            items: vec![
                item("2026-07", "Actions Linux", 100.0, 0.6, 0.0, "a"),
                item("2026-07", "Actions Windows", 100.0, 1.0, 0.0, "a"),
                item("2026-07", "Actions macOS 3-core", 10.0, 0.62, 0.0, "a"),
            ],
        };
        // 100 + 100x2 + 10x10 = 400
        assert_eq!(r.included_minutes("2026-07", &private_repos(&["a"])), 400);
    }

    #[test]
    fn a_public_repo_is_excluded_regardless_of_its_discount() {
        // Visibility decides membership now, not the discount fields — the
        // public item keeps `gross == discount` here to show the exclusion
        // holds on its own, without relying on that coincidence.
        let r = BillingReport {
            items: vec![
                item("2026-07", "Actions Linux", 5000.0, 30.0, 30.0, "public"),
                item("2026-07", "Actions Linux", 100.0, 0.6, 0.0, "private"),
            ],
        };
        assert_eq!(
            r.included_minutes("2026-07", &private_repos(&["private"])),
            100
        );
    }

    /// Locks finding 1 (critical): GitHub discounts a private repo still
    /// inside its plan's included allowance exactly like a public repo —
    /// `gross == discount` for both. The old `.filter(|i| i.gross > i.discount)`
    /// therefore dropped this item too, and `included_minutes` read 0 right
    /// up until GitHub actually started billing. Figures are the real ones
    /// measured on SecondBrain-io/monolith-back in July 2026. Proved RED
    /// against the old filter: it returns 0 here, where the fix returns
    /// 24 632.
    #[test]
    fn a_private_repo_within_its_allowance_still_consumes_it() {
        let r = BillingReport {
            items: vec![item(
                "2026-07",
                "Actions Linux",
                24_632.0,
                147.792,
                147.792,
                "monolith-back",
            )],
        };
        assert_eq!(
            r.included_minutes("2026-07", &private_repos(&["monolith-back"])),
            24_632,
            "a private repo inside its allowance must still count"
        );
    }

    #[test]
    fn an_unknown_sku_is_reported_not_swallowed() {
        let r = BillingReport {
            items: vec![item("2026-07", "Actions Quantum", 42.0, 1.0, 0.0, "a")],
        };
        assert_eq!(
            r.unknown_skus("2026-07"),
            vec!["Actions Quantum".to_string()]
        );
        // It still counts, at x1, rather than vanishing from the total.
        assert_eq!(r.included_minutes("2026-07", &private_repos(&["a"])), 42);
    }

    #[test]
    fn cost_splits_gross_into_covered_and_billed() {
        let r = BillingReport {
            items: vec![
                item("2026-07", "Actions Linux", 100.0, 10.0, 10.0, "a"),
                item("2026-07", "Actions Linux", 100.0, 4.0, 1.0, "b"),
            ],
        };
        let (gross, covered, billed) = r.cost("2026-07");
        assert!((gross - 14.0).abs() < 1e-9);
        assert!((covered - 11.0).abs() < 1e-9);
        assert!((billed - 3.0).abs() < 1e-9);
    }

    #[test]
    fn cost_trusts_githubs_net_rather_than_recomputing_it() {
        // GitHub sends all three amounts. When they disagree — rounding, a
        // credit, a promotional adjustment — `net` is the authoritative billed
        // figure, and recomputing `gross - discount` would silently diverge
        // from the real invoice. This fixture cannot be satisfied by both.
        let r = BillingReport {
            items: vec![UsageItem {
                month: "2026-07".into(),
                product: "actions".into(),
                sku: "Actions Linux".into(),
                quantity: 100.0,
                unit_type: "Minutes".into(),
                gross: 10.0,
                discount: 4.0,
                net: 2.0, // deliberately not gross - discount
                repo: "a".into(),
            }],
        };
        let (_, _, billed) = r.cost("2026-07");
        assert!(
            (billed - 2.0).abs() < 1e-9,
            "cost must sum net, got {billed}"
        );
    }

    #[test]
    fn storage_line_items_do_not_pollute_the_minutes_gauge() {
        // A real systm-d report carries `Actions storage` in GigabyteHours
        // alongside the minute SKUs. Counting it would add gigabyte-hours to a
        // minutes total, and it would also show up as an unknown runner.
        let mut storage = item("2026-07", "Actions storage", 41.5, 0.014, 0.0, "a");
        storage.unit_type = "GigabyteHours".into();
        let r = BillingReport {
            items: vec![
                item("2026-07", "Actions Linux", 100.0, 0.6, 0.0, "a"),
                storage,
            ],
        };
        assert_eq!(r.included_minutes("2026-07", &private_repos(&["a"])), 100);
        assert!(r.unknown_skus("2026-07").is_empty());
    }

    #[test]
    fn months_are_sorted_and_deduplicated() {
        let r = BillingReport {
            items: vec![
                item("2026-07", "Actions Linux", 1.0, 0.0, 0.0, "a"),
                item("2026-05", "Actions Linux", 1.0, 0.0, 0.0, "a"),
                item("2026-07", "Actions Linux", 1.0, 0.0, 0.0, "b"),
            ],
        };
        assert_eq!(
            r.months(),
            vec!["2026-05".to_string(), "2026-07".to_string()]
        );
    }

    #[test]
    fn an_empty_month_yields_zeroes_not_a_panic() {
        let r = BillingReport { items: vec![] };
        assert_eq!(r.included_minutes("2026-07", &HashSet::new()), 0);
        assert_eq!(r.cost("2026-07"), (0.0, 0.0, 0.0));
        assert!(r.months().is_empty());
    }

    #[test]
    fn minute_lines_name_the_repo_and_rank_by_allowance_cost() {
        let r = BillingReport {
            items: vec![
                // josephine burns more wall-clock minutes; claudine burns more
                // allowance. Ranking by raw quantity would put josephine first
                // and point the user at the wrong repository — the fixture is
                // built so the two sort keys disagree.
                item("2026-07", "Actions Linux", 8000.0, 48.0, 0.0, "josephine"),
                item("2026-07", "Actions Windows", 6079.0, 60.7, 0.0, "claudine"),
                item(
                    "2026-07",
                    "Actions Linux",
                    5000.0,
                    30.0,
                    30.0,
                    "public-repo",
                ),
            ],
        };
        let lines = r.minute_lines("2026-07", &private_repos(&["josephine", "claudine"]));

        assert_eq!(lines.len(), 2, "the public repo must not appear");
        assert_eq!(
            lines[0].repo, "claudine",
            "ranking must follow allowance cost"
        );
        assert_eq!(lines[0].quantity, 6079);
        assert_eq!(lines[0].equivalent, 12158);
        assert_eq!(lines[1].repo, "josephine");
        assert_eq!(lines[1].quantity, 8000);
        assert_eq!(lines[1].equivalent, 8000);
    }

    /// Same bug as `included_minutes`, for the breakdown: a private repo
    /// still covered by the plan's included allowance is fully discounted
    /// just like a public one, so the old `gross > discount` filter hid it from the one
    /// view meant to name the repository burning the allowance. Figures are
    /// the real ones measured on SecondBrain-io/monolith-back in July 2026.
    #[test]
    fn minute_lines_includes_a_private_repo_within_its_allowance() {
        let r = BillingReport {
            items: vec![item(
                "2026-07",
                "Actions Linux",
                24_632.0,
                147.792,
                147.792,
                "monolith-back",
            )],
        };
        let lines = r.minute_lines("2026-07", &private_repos(&["monolith-back"]));

        assert_eq!(
            lines.len(),
            1,
            "a private repo inside its allowance must still appear"
        );
        assert_eq!(lines[0].repo, "monolith-back");
        assert_eq!(lines[0].quantity, 24_632);
    }

    /// The counterpart of `storage_line_items_do_not_pollute_the_minutes_gauge`:
    /// the storage total must not pick up minutes, another month, or another
    /// product's GigabyteHours.
    #[test]
    fn storage_gbh_sums_only_actions_storage_gigabyte_hours() {
        // An illustrative GigabyteHours line of another product: its exact SKU
        // name is not under test, only that it is not Actions storage.
        let mut other_product = storage("2026-09", 500.0, "disconnected");
        other_product.product = "codespaces".into();
        other_product.sku = "Codespaces storage".into();
        // An Actions storage line in another unit: the SKU half of the filter
        // alone cannot fail without this — the GigabyteHours half must too.
        let mut odd = storage("2026-09", 999.0, "disconnected");
        odd.unit_type = "Minutes".into();
        let r = BillingReport {
            items: vec![
                storage("2026-09", 359.88, "disconnected"),
                storage("2026-09", 11.21, "ptitjardinier-app"),
                item(
                    "2026-09",
                    "Actions Linux",
                    1_004.0,
                    6.024,
                    6.024,
                    "disconnected",
                ),
                storage("2026-08", 100.0, "disconnected"),
                other_product,
                odd,
            ],
        };
        // exec-d's real September lines for the two repositories #13 names.
        // The report's 371.85 total also counts 0.76 GB-h the issue does not
        // attribute; the fixture does not invent a repository for them.
        let got = r.storage_gbh("2026-09");
        assert!((got - 371.09).abs() < 1e-9, "got {got}");
    }

    #[test]
    fn storage_lines_rank_exec_d_september() {
        let r = BillingReport {
            items: vec![
                // Lightest first, so the ranking has work to do.
                storage("2026-09", 11.21, "ptitjardinier-app"),
                storage("2026-09", 359.88, "disconnected"),
                // Minutes must never rank a repository.
                item(
                    "2026-09",
                    "Actions Linux",
                    5_000.0,
                    30.0,
                    30.0,
                    "ptitjardinier-app",
                ),
            ],
        };
        assert_eq!(
            r.storage_lines("2026-09"),
            vec![
                StorageLine {
                    repo: "disconnected".into(),
                    gbh: 359.88
                },
                StorageLine {
                    repo: "ptitjardinier-app".into(),
                    gbh: 11.21
                },
            ]
        );
    }

    /// One row per repository: two storage items of the same repository and
    /// month add up rather than producing two rows.
    #[test]
    fn storage_lines_merge_items_of_one_repo() {
        let r = BillingReport {
            items: vec![
                storage("2026-09", 1.5, "alertU"),
                storage("2026-09", 2.25, "alertU"),
            ],
        };
        let lines = r.storage_lines("2026-09");
        assert_eq!(lines.len(), 1);
        assert!((lines[0].gbh - 3.75).abs() < 1e-9, "got {}", lines[0].gbh);
    }

    #[test]
    fn storage_gbh_for_repo_reads_one_repo() {
        let r = BillingReport {
            items: vec![
                storage("2026-09", 359.88, "disconnected"),
                storage("2026-09", 11.21, "ptitjardinier-app"),
                storage("2026-08", 100.0, "disconnected"),
            ],
        };
        let got = r.storage_gbh_for_repo("2026-09", "disconnected");
        assert!((got - 359.88).abs() < 1e-9, "got {got}");
        let quiet = r.storage_gbh_for_repo("2026-09", "quiet");
        assert!(quiet.abs() < 1e-9, "got {quiet}");
    }

    /// An empty `f64` sum is -0.0, which the Billing tab prints as `-0.00`
    /// and `scan --json` writes as `-0.0`. A readable month with no storage,
    /// for the organization or for one repository, must be a plain zero.
    /// `== 0.0` holds for both zeros, so the sign is asserted too.
    #[test]
    fn an_empty_storage_sum_is_positive_zero() {
        let r = BillingReport {
            items: vec![
                // Minutes in September, storage only in August: September
                // has usage, just no storage.
                item(
                    "2026-09",
                    "Actions Linux",
                    1_004.0,
                    6.024,
                    6.024,
                    "disconnected",
                ),
                storage("2026-08", 100.0, "disconnected"),
            ],
        };
        let org = r.storage_gbh("2026-09");
        assert!(org == 0.0 && org.is_sign_positive(), "got {org:?}");
        let quiet = r.storage_gbh_for_repo("2026-08", "quiet");
        assert!(quiet == 0.0 && quiet.is_sign_positive(), "got {quiet:?}");
    }

    #[test]
    fn included_storage_follows_the_plan() {
        assert_eq!(included_storage_gb_for(Some("free")), Some(0.5));
        assert_eq!(included_storage_gb_for(Some("team")), Some(2.0));
        assert_eq!(included_storage_gb_for(Some("enterprise")), Some(50.0));
        assert_eq!(included_storage_gb_for(Some("pro")), None);
        assert_eq!(included_storage_gb_for(None), None);
    }

    /// The hour base is the displayed month's own — days × 24, GitHub's
    /// documented formula. A 720 constant fails on July, a 744 constant on
    /// September. See the spec's open measurement on 720 vs 744 before
    /// changing this.
    #[test]
    fn hours_in_month_counts_the_displayed_months_days() {
        assert_eq!(hours_in_month("2026-09"), Some(720));
        assert_eq!(hours_in_month("2026-07"), Some(744));
        assert_eq!(hours_in_month("2026-02"), Some(672));
        assert_eq!(hours_in_month("2028-02"), Some(696));
        assert_eq!(hours_in_month("2026-12"), Some(744));
        assert_eq!(hours_in_month("2026-13"), None);
        assert_eq!(hours_in_month(""), None);
    }

    #[test]
    fn storage_quota_is_included_gb_times_month_hours() {
        // exec-d in September on Free, its plan until 2026-09-10: 0.5 × 720.
        assert_eq!(
            storage_quota(Some("free"), "2026-09"),
            Some(StorageQuota {
                gbh: 360.0,
                hours: 720
            })
        );
        // On Team, its current plan: 2 × 720.
        assert_eq!(
            storage_quota(Some("team"), "2026-09"),
            Some(StorageQuota {
                gbh: 1_440.0,
                hours: 720
            })
        );
        assert_eq!(
            storage_quota(Some("free"), "2026-07"),
            Some(StorageQuota {
                gbh: 372.0,
                hours: 744
            })
        );
        assert_eq!(storage_quota(None, "2026-09"), None);
        assert_eq!(storage_quota(Some("team"), ""), None);
    }

    #[test]
    fn month_of_formats_year_and_month() {
        let t = chrono::DateTime::parse_from_rfc3339("2026-09-10T08:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(month_of(t), "2026-09");
    }

    fn budget(budget_type: &str, sku: &str, amount: u64, blocking: bool) -> Budget {
        Budget {
            budget_type: budget_type.into(),
            sku: sku.into(),
            scope: "organization".into(),
            amount,
            blocking,
        }
    }

    /// exec-d's real response on 2026-09-10: four org-level product budgets,
    /// all 0 $ and blocking. Identical amounts, so the test must check *which*
    /// budget came back — and the Actions one is third, so "the first
    /// budget" fails.
    #[test]
    fn actions_budget_picks_the_org_actions_product_budget() {
        let exec_d = vec![
            budget("ProductPricing", "codespaces", 0, true),
            budget("ProductPricing", "packages", 0, true),
            budget("ProductPricing", "actions", 0, true),
            budget("ProductPricing", "git_lfs", 0, true),
        ];
        let b = actions_budget(&exec_d).expect("exec-d has an Actions budget");
        assert_eq!(b.sku, "actions");
        assert_eq!((b.amount, b.blocking), (0, true));

        // cloudalpes: 5 $, blocking.
        let cloudalpes = vec![budget("ProductPricing", "actions", 5, true)];
        assert_eq!(actions_budget(&cloudalpes).map(|b| b.amount), Some(5));

        // SecondBrain-io: no budget at all.
        assert!(actions_budget(&[]).is_none());

        // An Actions budget of another scope is not the organization's.
        let mut repo_scoped = budget("ProductPricing", "actions", 5, true);
        repo_scoped.scope = "repository".into();
        assert!(actions_budget(&[repo_scoped]).is_none());
    }

    /// The SKU alone cannot tell a `SkuPricing` budget from the
    /// organization's `ProductPricing` one when the two happen to share a
    /// name: only the `budget_type` check keeps them apart.
    #[test]
    fn actions_budget_requires_the_product_pricing_type() {
        let budgets = vec![budget("SkuPricing", "actions", 5, true)];
        assert!(actions_budget(&budgets).is_none());
    }

    /// Never observed, so never interpreted — and never silently ignored.
    /// SKU names here are illustrative: the real shape is an open
    /// measurement.
    #[test]
    fn a_sku_pricing_actions_budget_is_surfaced_not_ignored() {
        let budgets = vec![
            budget("SkuPricing", "actions_linux", 5, true),
            budget("SkuPricing", "codespaces_storage", 5, true),
            budget("ProductPricing", "actions", 0, true),
        ];
        let skus: Vec<&str> = actions_sku_budgets(&budgets)
            .iter()
            .map(|b| b.sku.as_str())
            .collect();
        assert_eq!(skus, vec!["actions_linux"]);
        // It does not stand in for the product budget.
        assert_eq!(
            actions_budget(&budgets).map(|b| b.sku.as_str()),
            Some("actions")
        );
    }

    /// Same reasoning as `actions_budget`'s own scope check: a
    /// repository-scoped SKU budget must not be counted among the
    /// organization's.
    #[test]
    fn actions_sku_budgets_requires_organization_scope() {
        let mut repo_scoped = budget("SkuPricing", "actions_linux", 5, true);
        repo_scoped.scope = "repository".into();
        assert!(actions_sku_budgets(&[repo_scoped]).is_empty());
    }

    #[test]
    fn nears_blocking_budget_needs_ninety_percent_and_a_blocking_budget() {
        let blocking = budget("ProductPricing", "actions", 0, true);
        let alert_only = budget("ProductPricing", "actions", 5, false);
        assert!(nears_blocking_budget(90, Some(&blocking)));
        assert!(nears_blocking_budget(103, Some(&blocking)));
        assert!(!nears_blocking_budget(89, Some(&blocking)));
        assert!(!nears_blocking_budget(95, Some(&alert_only)));
        assert!(!nears_blocking_budget(95, None));
    }
}
