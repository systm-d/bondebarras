//! Core data model: resources, risk tiers, and display formatting.

/// A deletable — or, since v0.5, archivable — GitHub resource family. v0.1
/// covers the three regenerable ones; v0.3 adds container package versions,
/// the first irreversible one; v0.4 adds branches, tags and release assets;
/// v0.5 adds the repository itself, archived rather than deleted (see
/// `repos::classify_repo` and `api::archive`) — repository *deletion* stays
/// permanently out of scope.
///
/// `Hash` matters as much as `Eq` here: GitHub numbers caches, artifacts,
/// workflow runs and package versions in independent namespaces, so a
/// selection set keyed on `id` alone would collide across kinds. Keying on
/// `(ResourceKind, u64)` needs both derives. Branches and tags carry no
/// numeric id at all — `Resource.id` is a hash of the name for those two (see
/// task 3's `api::refs`), which is exactly why the pair still needs `Hash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Cache,
    Artifact,
    WorkflowRun,
    PackageVersion,
    Branch,
    Tag,
    ReleaseAsset,
    Repository,
}

impl ResourceKind {
    /// Every variant, so tests can assert the `risk_tier` match stays exhaustive.
    pub const ALL: [ResourceKind; 8] = [
        ResourceKind::Cache,
        ResourceKind::Artifact,
        ResourceKind::WorkflowRun,
        ResourceKind::PackageVersion,
        ResourceKind::Branch,
        ResourceKind::Tag,
        ResourceKind::ReleaseAsset,
        ResourceKind::Repository,
    ];

    /// Whether GitHub reports a real size for this family.
    ///
    /// `false` for the four sizeless kinds: a package version (no size field
    /// exists, under any name — see `api::packages`), a branch and a tag (a
    /// ref carries no size of its own), and a repository (archiving frees no
    /// bytes — the repository's size is unchanged, only its Actions are
    /// disabled). `true` for every other kind, whose `size_bytes` is a real
    /// GitHub-reported number, zero included.
    ///
    /// An exhaustive `match`, not the `matches!` shorthand this used to be:
    /// that version, `!matches!(self, PackageVersion | Branch | Tag)`,
    /// silently defaulted every kind absent from its list to `true` — the
    /// wrong direction for a sizeless family added later, since nothing
    /// forced a decision when `Repository` joined `ResourceKind` in v0.5.
    /// Debt 1 of the v0.4 final review; closed here rather than carried into
    /// v0.5 as a fifth debt. One place to update when a family is added —
    /// before this existed, `size_display`, `Plan::summary` and the
    /// sizeless-deletion count in `clean::execute` each spelled out their own
    /// `kind == PackageVersion` check, and only one of the three was ever
    /// updated when branches and tags joined the sizeless set: a branch
    /// rendered a bare "0 o", the exact "reads as empty" defect v0.3 spent a
    /// whole fix wave on.
    pub fn has_known_size(self) -> bool {
        match self {
            ResourceKind::Cache
            | ResourceKind::Artifact
            | ResourceKind::WorkflowRun
            | ResourceKind::ReleaseAsset => true,
            ResourceKind::PackageVersion
            | ResourceKind::Branch
            | ResourceKind::Tag
            | ResourceKind::Repository => false,
        }
    }
}

/// How much friction a deletion — or, since v0.5, an archive — must go
/// through. Ordered by severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskTier {
    /// Regenerable by re-running a workflow: a single confirmation.
    Low,
    /// Not undoable by a re-run: either the change is irreversible outright
    /// (a package version, a branch, a tag, a release asset — the layer or
    /// ref is gone for good), or, since v0.5, it turns a whole repository
    /// read-only (reversible on GitHub's side, but not by anything this
    /// re-run-shaped tool can trigger). Either way: itemised recap plus
    /// confirmation.
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
        ResourceKind::PackageVersion
        // Irreversible for the same reason a package version is: none of the
        // three comes back from a re-run. A branch or tag ref, once deleted,
        // is gone from the repository outright; a release asset is gone from
        // the release. Not nuclear — nothing here destroys the repository or
        // the release itself, only what these three individually name.
        | ResourceKind::Branch
        | ResourceKind::Tag
        | ResourceKind::ReleaseAsset => RiskTier::Medium,
        // Reversible, unlike every other kind at this tier — un-archiving
        // restores it — but not a re-run away either, and it turns the whole
        // repository read-only in the meantime. Medium, not Low: the blast
        // radius is bigger than one cache key even though nothing here is
        // permanent. Not Nuclear: repository *deletion*, the operation that
        // tier exists for, is permanently out of scope (see
        // `tui::views::confirm`'s module doc).
        ResourceKind::Repository => RiskTier::Medium,
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
    /// Why a `Branch` row is or is not offered — `None` for every other
    /// kind. `protected` alone collapses `refs::BranchClass::Default`,
    /// `Protected` and `Live` into the same bool, which is right for bulk
    /// selection (see `commands::clean::select`) but cannot tell a human
    /// which of the three actually applies. This is what lets a branch row
    /// say "protégée" only when GitHub itself refuses the branch, and what
    /// lets the TUI's individual-selection guard (`tui::app::App::
    /// toggle_selected`) tell "GitHub-backed" apart from "merely unmerged".
    pub branch_class: Option<crate::refs::BranchClass>,
    /// How safe this resource is to delete — see `safety::classify`.
    pub safety: crate::safety::Safety,
}

/// A resource's size, formatted for display.
///
/// GitHub exposes no size at all for a package version, a branch or a tag
/// (see `api::packages` and `api::refs`); `size_bytes` is hardcoded to `0`
/// for all three, and a bare "0 o" would read as "empty" — the opposite of
/// the truth. Shown as `—` instead, everywhere a resource's size reaches a
/// screen: the TUI's resource list and the headless `clean` dry-run listing
/// both go through this one function and `ResourceKind::has_known_size`, so
/// the two cannot drift the way independent `if r.kind == PackageVersion`
/// checks did before v0.4 added the other two sizeless kinds.
pub fn size_display(r: &Resource) -> String {
    if r.kind.has_known_size() {
        human_size(r.size_bytes)
    } else {
        "—".to_string()
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
    /// Days since the last push. Not proof of abandonment on its own — a
    /// finished, stable library does not move for two years without being
    /// dead — which is exactly why nothing in this crate ever preselects a
    /// repository from it (see `tui::app::App::select_all_stale`'s own guard
    /// and `commands::clean::select`'s permanent refusal). Shown to the human
    /// who decides, on the repository's own row in the tree.
    pub age_days: i64,
    /// Whether this repository is a genuine archiving candidate, already
    /// archived, or off-limits to this token — see `repos::classify_repo`.
    /// A repo that only appears in the cache report (never in the repo
    /// listing) defaults to `RepoClass::NoAdminRights`: the safe direction
    /// when this token's real rights are unknown is to offer nothing, not to
    /// invite a tick the API might refuse.
    pub class: crate::repos::RepoClass,
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
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        }
    }

    /// Debt 1 of the v0.4 final review: the old version of this test
    /// recomputed `has_known_size`'s own `!matches!(...)` expression to
    /// build `expected`, so both sides of the assertion always agreed no
    /// matter which kinds the predicate actually covered — a sizeless family
    /// added later, `Repository` among them, would default to `true` on
    /// both sides and this "test" would stay green regardless. Every arm is
    /// typed out by hand here instead, one per `ResourceKind::ALL` entry, so
    /// the table and the predicate are two independent sources of truth.
    #[test]
    fn has_known_size_matches_a_hardcoded_table() {
        let expected: [(ResourceKind, bool); 8] = [
            (ResourceKind::Cache, true),
            (ResourceKind::Artifact, true),
            (ResourceKind::WorkflowRun, true),
            (ResourceKind::PackageVersion, false),
            (ResourceKind::Branch, false),
            (ResourceKind::Tag, false),
            (ResourceKind::ReleaseAsset, true),
            // Archiving frees no bytes — the repository's own size is
            // unchanged — so this reads `—` like the other three sizeless
            // kinds, not a misleading "0 o".
            (ResourceKind::Repository, false),
        ];
        // Every `ResourceKind::ALL` entry must appear in the table exactly
        // once — otherwise a variant added to the enum but forgotten here
        // would silently fall out of this sweep instead of failing it.
        assert_eq!(
            expected.len(),
            ResourceKind::ALL.len(),
            "the hardcoded table must cover every ResourceKind variant"
        );
        for kind in ResourceKind::ALL {
            let (_, expect) = expected
                .iter()
                .find(|(k, _)| *k == kind)
                .unwrap_or_else(|| panic!("{kind:?} is missing from the hardcoded table"));
            assert_eq!(kind.has_known_size(), *expect, "wrong answer for {kind:?}");
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

    /// v0.4 adds two more sizeless kinds, branches and tags — a bare "0 o"
    /// here is the exact "reads as empty" defect v0.3's whole fix wave was
    /// about, recurring for the two new families a `kind ==
    /// ResourceKind::PackageVersion` check cannot see.
    #[test]
    fn size_display_shows_unknown_not_zero_for_a_branch_or_a_tag() {
        for kind in [ResourceKind::Branch, ResourceKind::Tag] {
            let s = size_display(&resource(kind, 0));
            assert!(!s.contains("0 o"), "got: {s} for {kind:?}");
            assert!(s.contains('—'), "got: {s} for {kind:?}");
        }
    }

    #[test]
    fn size_display_shows_real_bytes_for_every_other_kind() {
        for kind in [
            ResourceKind::Cache,
            ResourceKind::Artifact,
            ResourceKind::WorkflowRun,
            ResourceKind::ReleaseAsset,
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

    #[test]
    fn deleting_a_branch_is_medium_risk() {
        // A merged branch is regenerable in principle (the commits live on
        // in the default branch through the merge), but deleting the ref
        // itself is not undoable by a re-run the way a cache or artifact is.
        assert_eq!(risk_tier(ResourceKind::Branch), RiskTier::Medium);
        assert!(RiskTier::Low < risk_tier(ResourceKind::Branch));
    }

    #[test]
    fn deleting_a_tag_is_medium_risk() {
        assert_eq!(risk_tier(ResourceKind::Tag), RiskTier::Medium);
        assert!(RiskTier::Low < risk_tier(ResourceKind::Tag));
    }

    #[test]
    fn deleting_a_release_asset_is_medium_risk() {
        assert_eq!(risk_tier(ResourceKind::ReleaseAsset), RiskTier::Medium);
        assert!(RiskTier::Low < risk_tier(ResourceKind::ReleaseAsset));
    }
}
