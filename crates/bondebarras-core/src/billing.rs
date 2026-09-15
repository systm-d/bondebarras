//! Billing aggregation. Pure calculation over a usage report — no network,
//! no rendering.
//!
//! Minutes cannot be cleaned up retroactively: once burnt they are burnt. All
//! this module can do is say *where they went*, which is the only useful
//! answer for the Actions-minutes axis of the problem.

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
    /// The part absorbed by the free allowance.
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

/// Free Actions allowance for an organization, in Linux-equivalent minutes.
pub const FREE_MINUTES_PER_MONTH: u64 = 2_000;

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

    /// Minutes charged against the free allowance, in Linux equivalents.
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
    /// inside its free allowance exactly like a public repo — `gross ==
    /// discount` for both. The old `.filter(|i| i.gross > i.discount)`
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
    /// still covered by the free allowance is fully discounted just like a
    /// public one, so the old `gross > discount` filter hid it from the one
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
}
