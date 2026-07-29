//! Core data model: resources, risk tiers, and display formatting.

/// A deletable GitHub resource family. v0.1 covers the three regenerable ones;
/// v0.3 adds container package versions, the first irreversible one.
///
/// `Hash` matters as much as `Eq` here: GitHub numbers caches, artifacts,
/// workflow runs and package versions in independent namespaces, so a
/// selection set keyed on `id` alone would collide across kinds. Keying on
/// `(ResourceKind, u64)` needs both derives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Cache,
    Artifact,
    WorkflowRun,
    PackageVersion,
}

impl ResourceKind {
    /// Every variant, so tests can assert the `risk_tier` match stays exhaustive.
    pub const ALL: [ResourceKind; 4] = [
        ResourceKind::Cache,
        ResourceKind::Artifact,
        ResourceKind::WorkflowRun,
        ResourceKind::PackageVersion,
    ];
}

/// How much friction a deletion must go through. Ordered by severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskTier {
    /// Regenerable by re-running a workflow: a single confirmation.
    Low,
    /// Irreversible but rarely critical: itemised recap plus confirmation.
    Medium,
    /// Definitive destruction: the user must type the target's name.
    Nuclear,
}

/// The tier is carried by the type, never by the UI — this exhaustive `match`
/// is what makes it impossible to add a destructive kind without assigning it
/// a tier.
pub fn risk_tier(kind: ResourceKind) -> RiskTier {
    match kind {
        ResourceKind::Cache | ResourceKind::Artifact | ResourceKind::WorkflowRun => RiskTier::Low,
        // Irreversible: the layer leaves the registry. A cache or an artifact
        // comes back with a re-run; this does not.
        ResourceKind::PackageVersion => RiskTier::Medium,
    }
}

/// Decimal units (Go, not Gio) — matches what GitHub's own billing UI shows.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    if bytes < 1000 {
        return format!("{bytes} o");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// One deletable item inside a repository.
#[derive(Debug, Clone)]
pub struct Resource {
    pub kind: ResourceKind,
    pub id: u64,
    pub label: String,
    pub size_bytes: u64,
    pub age_days: i64,
    /// Git ref the resource is attached to, when GitHub exposes one.
    pub git_ref: Option<String>,
    /// True when `git_ref` points at a closed or merged pull request.
    pub stale_pr: bool,
    /// Something live still references this resource by name — a container tag
    /// like `latest`, today. Bulk selection must never take it: the TUI does
    /// not preselect it, and a headless run refuses it outright, because there
    /// is no human at the other end of a cron to notice.
    pub protected: bool,
}

/// A resource's size, formatted for display.
///
/// GitHub exposes no size for a package version, under any name (see
/// `api::packages`); `size_bytes` is hardcoded to `0` for that kind, and a
/// bare "0 o" would read as "empty" — the opposite of the truth. Shown as
/// `—` instead, everywhere a resource's size reaches a screen: the TUI's
/// resource list and the headless `clean` dry-run listing both go through
/// this one function, so the two cannot drift the way two independent
/// `if r.kind == PackageVersion` checks would risk.
pub fn size_display(r: &Resource) -> String {
    if r.kind == ResourceKind::PackageVersion {
        "—".to_string()
    } else {
        human_size(r.size_bytes)
    }
}

/// Per-repository cache aggregate, from the org-level endpoint.
#[derive(Debug, Clone)]
pub struct RepoSummary {
    pub name: String,
    pub cache_bytes: u64,
    pub cache_count: u32,
    /// Whether this repo draws on the org's Actions allowance. Repos merged
    /// in from `repos::list` carry their real visibility; a repo that only
    /// appears in the cache report (never in the repo listing) defaults to
    /// `false` — see `scan::overview`.
    pub private: bool,
}

/// Stage-1 view of one organization.
#[derive(Debug, Clone)]
pub struct OrgSummary {
    pub login: String,
    pub cache_bytes: u64,
    pub cache_count: u32,
    pub repos: Vec<RepoSummary>,
    /// The org's usage report, or `None` when billing is not readable —
    /// GitHub answers 403 to anyone who is not an owner. A 403 degrades this
    /// one column; it never drops the org.
    pub billing: Option<crate::billing::BillingReport>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_uses_decimal_units() {
        assert_eq!(human_size(0), "0 o");
        assert_eq!(human_size(999), "999 o");
        assert_eq!(human_size(1_500), "1.5 Ko");
        assert_eq!(human_size(37_166_609_585), "37.2 Go");
    }

    fn resource(kind: ResourceKind, size_bytes: u64) -> Resource {
        Resource {
            kind,
            id: 1,
            label: "r".into(),
            size_bytes,
            age_days: 1,
            git_ref: None,
            stale_pr: false,
            protected: false,
        }
    }

    #[test]
    fn size_display_shows_unknown_not_zero_for_a_package_version() {
        // Every other screen shows bytes. A bare "0 o" here would read as
        // "empty", the opposite of the truth: GitHub exposes no size for a
        // package version at all.
        let s = size_display(&resource(ResourceKind::PackageVersion, 0));
        assert!(!s.contains("0 o"), "got: {s}");
        assert!(s.contains('—'), "got: {s}");
    }

    #[test]
    fn size_display_shows_real_bytes_for_every_other_kind() {
        for kind in [
            ResourceKind::Cache,
            ResourceKind::Artifact,
            ResourceKind::WorkflowRun,
        ] {
            assert_eq!(size_display(&resource(kind, 1_500)), "1.5 Ko");
        }
    }

    #[test]
    fn every_v01_kind_is_low_risk() {
        // `ResourceKind::ALL` now spans every family, v0.3's included, so it
        // can no longer stand in for "the v0.1 set" here — that is exactly
        // what `deleting_a_package_version_is_medium_risk` below exists to
        // tell apart from this one.
        for kind in [
            ResourceKind::Cache,
            ResourceKind::Artifact,
            ResourceKind::WorkflowRun,
        ] {
            assert_eq!(risk_tier(kind), RiskTier::Low);
        }
    }

    #[test]
    fn risk_tiers_are_ordered_by_severity() {
        assert!(RiskTier::Low < RiskTier::Medium);
        assert!(RiskTier::Medium < RiskTier::Nuclear);
    }

    #[test]
    fn deleting_a_package_version_is_medium_risk() {
        // Irreversible and not regenerable by a re-run, unlike a cache: the
        // layer is gone from the registry. But it is not the nuclear tier —
        // nothing here destroys a repository.
        assert_eq!(risk_tier(ResourceKind::PackageVersion), RiskTier::Medium);
        assert!(RiskTier::Low < risk_tier(ResourceKind::PackageVersion));
    }
}
