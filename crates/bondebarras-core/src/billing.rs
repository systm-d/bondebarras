//! Billing aggregation. Pure calculation over a usage report — no network,
//! no rendering.
//!
//! Minutes cannot be cleaned up retroactively: once burnt they are burnt. All
//! this module can do is say *where they went*, which is the only useful
//! answer for the Actions-minutes axis of the problem.

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

/// Free Actions allowance for an organization, in Linux-equivalent minutes.
pub const FREE_MINUTES_PER_MONTH: u64 = 2_000;

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
    /// Fully discounted items are excluded: a public repository bills nothing
    /// and consumes no allowance, so counting it would make an open-source
    /// project look like it had blown the quota.
    pub fn included_minutes(&self, month: &str) -> u64 {
        self.items
            .iter()
            .filter(|i| i.month == month && i.unit_type == "Minutes")
            .filter(|i| i.gross > i.discount)
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
        assert_eq!(r.included_minutes("2026-07"), 400);
    }

    #[test]
    fn a_fully_discounted_item_does_not_consume_the_allowance() {
        // Public repos bill nothing: gross == discount. Counting them would
        // make an open-source project look like it had blown the quota.
        let r = BillingReport {
            items: vec![
                item("2026-07", "Actions Linux", 5000.0, 30.0, 30.0, "public"),
                item("2026-07", "Actions Linux", 100.0, 0.6, 0.0, "private"),
            ],
        };
        assert_eq!(r.included_minutes("2026-07"), 100);
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
        assert_eq!(r.included_minutes("2026-07"), 42);
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
        assert_eq!(r.included_minutes("2026-07"), 0);
        assert_eq!(r.cost("2026-07"), (0.0, 0.0, 0.0));
        assert!(r.months().is_empty());
    }
}
