//! How safe a resource is to delete, on three levels.
//!
//! Two levels would force an arbitrary call. An asset from the release before
//! last and a cache from a closed pull request are not dead in the same way:
//! someone may still be pulling the first, while nothing references the
//! second. Collapsing them means lying in one direction or the other.

use crate::model::{Resource, ResourceKind};
use crate::refs::BranchClass;
use std::collections::HashSet;

/// How safe a resource is to delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Safety {
    /// Nothing live references it. `[A]` takes these.
    Safe,
    /// Plausibly dead, but a person should look. Shown, never preselected.
    Check,
    /// Live, or protected. Never offered in bulk.
    Keep,
}

/// What the repository's other listings say, so a resource can be judged
/// against them. Every field comes from `repo_detail`'s existing calls — this
/// costs no extra request.
#[derive(Debug, Clone, Default)]
pub struct RepoContext {
    /// `head.ref` of the pull requests that were actually merged.
    pub merged_refs: HashSet<String>,
    /// Every branch that still exists — `None` unless the branches listing
    /// answered in full. A listing that failed, or stopped at its page cap
    /// (`api::refs::BranchListing::complete`), says nothing about a name it
    /// does not hold, so it is not kept at all (final review I1).
    pub live_branches: Option<HashSet<String>>,
    /// The repository's default branch; `""` when its lookup failed.
    pub default_branch: String,
    /// Release tags, newest first, as the API returns them.
    pub release_tags: Vec<String>,
}

/// How many releases back an asset must be before it counts as safe.
const SAFE_RELEASE_DEPTH: usize = 2;

/// An artifact this old is worth checking even if it has not expired.
const ARTIFACT_CHECK_DAYS: i64 = 30;

/// A workflow run this old is worth checking.
const RUN_CHECK_DAYS: i64 = 90;

/// Judge one resource against its repository's context.
///
/// Delegates to `classify_inner` for the level each family's own rules
/// produce, then applies one rule that outranks all of them: §4.3 of the
/// design doc — a protected resource is never `Safe`, not, as an earlier
/// version of this function enforced, always `Keep`. The distinction matters
/// for `Branch`: a live, unmerged branch is `protected` (see
/// `scan::branch_resources`'s `protected: class != BranchClass::Merged`) but
/// must still read `Check`, not `Keep`, so a human can look at it —
/// collapsing every protected resource to `Keep` made that row of the table
/// unreachable.
pub fn classify(r: &Resource, ctx: &RepoContext) -> Safety {
    let level = classify_inner(r, ctx);
    // Spec §4.3: a protected resource is never Safe. Every arm below already
    // honours this for the families that exist today — a tagged package
    // version and a Default/Protected branch both reach Keep on their own.
    // This backstop exists so a family added later cannot reintroduce the
    // hole by forgetting: it makes the rule structural rather than a
    // property each arm happens to have.
    if r.protected && level == Safety::Safe {
        return Safety::Keep;
    }
    level
}

/// The level each family's own rules produce, before `classify`'s
/// `protected`-is-never-`Safe` backstop is applied.
fn classify_inner(r: &Resource, ctx: &RepoContext) -> Safety {
    match r.kind {
        // Never marked at any level. `pushed_at` is not proof of abandonment.
        ResourceKind::Repository => Safety::Keep,
        ResourceKind::Tag => Safety::Keep,

        ResourceKind::Cache => classify_by_ref(r, ctx),
        ResourceKind::WorkflowRun => classify_run(r, ctx),
        ResourceKind::Artifact => {
            if r.label.contains("(expiré)") {
                Safety::Safe
            } else if r.age_days >= ARTIFACT_CHECK_DAYS {
                Safety::Check
            } else {
                Safety::Keep
            }
        }
        ResourceKind::ReleaseAsset => classify_asset(r, ctx),
        ResourceKind::PackageVersion => {
            // Table's "tagué → rien" row: a tagged version is `protected`
            // and reaches `Keep`; an untagged version or an orphaned
            // attestation — both `protected: false` from `packages::
            // classify` — reach `Safe`. Spelled out explicitly rather than
            // leaning on `classify`'s backstop, even though the backstop
            // alone would land on the same answer here: this arm should
            // read correctly against the table on its own.
            if r.protected {
                Safety::Keep
            } else {
                Safety::Safe
            }
        }
        // `protected` alone cannot tell a `Default`/`Protected` branch
        // (GitHub itself refuses these) apart from a merely-unmerged `Live`
        // one: `scan::branch_resources` sets `protected: class !=
        // BranchClass::Merged`, so both `Default` and `Live` carry it.
        // `branch_class` already makes that distinction (see `refs::
        // BranchClass`) — reading it instead of `protected` is what makes
        // this arm's `Check` row reachable at all.
        ResourceKind::Branch => match r.branch_class {
            Some(BranchClass::Merged) => Safety::Safe,
            Some(BranchClass::Default) | Some(BranchClass::Protected) => Safety::Keep,
            // Live, or unknown. Someone may still be working on it; the
            // tool shows it and never preselects it. `None` lands here
            // deliberately — an unclassified branch is treated as live,
            // the cautious direction.
            Some(BranchClass::Live) | None => Safety::Check,
        },
    }
}

/// A cache, judged by the ref it is attached to.
///
/// Not shared with `WorkflowRun` any more: a cache is disposable — nothing
/// breaks when one is gone, so a vanished branch is already enough to call
/// it safe. See `classify_run`'s own doc comment for why a run needs a
/// different rule set entirely.
fn classify_by_ref(r: &Resource, ctx: &RepoContext) -> Safety {
    // A cache pinned to a closed pull request carries `refs/pull/32/merge`,
    // which names no branch — `merged_refs` holds `head.ref` values and would
    // never match it. `stale_pr` already resolves that case, from the same
    // closed-PR listing, and has since v0.1. Reuse it rather than re-deriving.
    if r.stale_pr {
        return Safety::Safe;
    }

    let Some(name) = r.git_ref.as_deref().map(strip_ref_prefix) else {
        return Safety::Keep;
    };

    if name == ctx.default_branch {
        return Safety::Keep;
    }
    if ctx.merged_refs.contains(name) {
        return Safety::Safe;
    }
    // A ref naming a branch that no longer exists: nothing pulls it by name,
    // and no pull request will bring it back.
    //
    // Only a whole branch listing can say a name is absent from it, and only
    // a known default branch can say this ref is not the default one — both
    // checks above read `""` as "no default branch", which no real branch is
    // named. Without either, absence of data would pass for proof of
    // absence, and a live branch's cache — `main`'s, when both calls failed
    // — would read ⛑ (final review I1). Such a cache falls to `Check`.
    if let Some(live) = &ctx.live_branches
        && !ctx.default_branch.is_empty()
        && !live.contains(name)
        && !name.starts_with("refs/pull/")
    {
        return Safety::Safe;
    }
    Safety::Check
}

/// A workflow run, judged by whether its work landed — never by ref
/// topology.
///
/// A cache is an optimisation: nothing breaks when one is gone, so a
/// vanished branch is already enough to call it safe. A run is a *record* —
/// the log of what happened — and a branch disappearing says nothing about
/// whether anyone still wants to read it. So the only thing that makes a run
/// safe is the positive fact that its work landed: a merged PR. Everything
/// else falls back to age, which is what the spec's row for this family says
/// and what sharing `classify_by_ref` with `Cache` used to silently
/// override — a 1-day-old run on a branch deleted without ever merging used
/// to read `Safe` the moment the branch was gone, which `[A]` would have
/// preselected for one-keystroke deletion.
fn classify_run(r: &Resource, ctx: &RepoContext) -> Safety {
    let merged = r.stale_pr
        || r.git_ref
            .as_deref()
            .map(strip_ref_prefix)
            .is_some_and(|name| ctx.merged_refs.contains(name));
    if merged {
        return Safety::Safe;
    }
    if r.age_days >= RUN_CHECK_DAYS {
        return Safety::Check;
    }
    Safety::Keep
}

/// `refs/heads/foo` and `refs/tags/foo` both name `foo`; a pull ref keeps its
/// full form, since it names no branch.
fn strip_ref_prefix(git_ref: &str) -> &str {
    git_ref
        .strip_prefix("refs/heads/")
        .or_else(|| git_ref.strip_prefix("refs/tags/"))
        .unwrap_or(git_ref)
}

/// An asset, judged by how far back its release is.
fn classify_asset(r: &Resource, ctx: &RepoContext) -> Safety {
    let Some(tag) = tag_in_label(&r.label) else {
        return Safety::Check;
    };
    match ctx.release_tags.iter().position(|t| t == tag) {
        Some(0) => Safety::Keep,
        Some(n) if n < SAFE_RELEASE_DEPTH => Safety::Check,
        Some(_) => Safety::Safe,
        // A tag absent from the list does not mean an older release:
        // `scan::distinct_release_tags` builds the list from these very
        // assets, so every real asset's tag is in it. Absent means
        // `tag_in_label` read the label wrong — a tag holding its own
        // parenthesis, `v1.0(beta)`, reads `beta)` — and the release could be
        // the newest. Nothing proves the asset old, so a person looks.
        None => Safety::Check,
    }
}

/// The release tag an asset's label carries, written `… (v1.2.3)`.
fn tag_in_label(label: &str) -> Option<&str> {
    let start = label.rfind('(')? + 1;
    let end = label.rfind(')')?;
    (start < end).then(|| &label[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ResourceKind;

    fn ctx() -> RepoContext {
        RepoContext {
            merged_refs: ["claude/landing".to_string(), "still-there".to_string()]
                .into_iter()
                .collect(),
            // `still-there` also lives here: a ref this test's cache is on
            // must find no vanished-branch fallback available, or a test
            // reading it could not tell the `merged_refs` rule from that one.
            live_branches: Some(
                [
                    "main".to_string(),
                    "wip".to_string(),
                    "still-there".to_string(),
                ]
                .into_iter()
                .collect(),
            ),
            default_branch: "main".to_string(),
            release_tags: vec!["v0.12.0".into(), "v0.11.0".into(), "v0.10.0".into()],
        }
    }

    fn res(kind: ResourceKind, age: i64) -> Resource {
        Resource {
            kind,
            id: 1,
            label: String::new(),
            size_bytes: 0,
            age_days: age,
            git_ref: None,
            stale_pr: false,
            protected: false,
            // `Resource` already carries `branch_class` before this task;
            // the brief's fixture predates that field and omits it, so it is
            // supplied here to compile — every other row in this fixture is
            // not a `Branch`, so `None` is also the semantically correct
            // value (see `scan::branch_resources`).
            branch_class: None,
            safety: Safety::Keep,
        }
    }

    #[test]
    fn a_cache_on_a_merged_branchs_ref_is_safe() {
        // The branch still exists, so the vanished-branch fallback cannot
        // fire — `merged_refs` is the only rule that can return Safe here.
        // With the ref absent from `live_branches`, both rules would answer
        // Safe and the test could not tell them apart.
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("refs/heads/still-there".into());
        assert_eq!(classify(&c, &ctx()), Safety::Safe);
    }

    #[test]
    fn a_cache_pinned_to_a_closed_pr_is_safe_via_stale_pr() {
        // A pull ref names no branch, so `merged_refs` — which holds
        // `head.ref` values — can never match it. This is the path that
        // covers the 320 caches on the author's own `josephine`.
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("refs/pull/54/merge".into());
        c.stale_pr = true;
        assert_eq!(classify(&c, &ctx()), Safety::Safe);

        // The same ref shape with an open PR must NOT be safe, or the tool
        // would offer to delete the cache of work in progress.
        let mut open = res(ResourceKind::Cache, 12);
        open.git_ref = Some("refs/pull/99/merge".into());
        assert_eq!(classify(&open, &ctx()), Safety::Check);
    }

    #[test]
    fn a_cache_on_a_vanished_branch_is_safe() {
        // Nothing can pull it by name any more, and no PR will resurrect it.
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("gone-branch".into());
        assert_eq!(classify(&c, &ctx()), Safety::Safe);
    }

    /// Final review I1, at the rule itself: with no whole branch listing, a
    /// ref missing from the set proves nothing, and `gone-branch`'s cache —
    /// `Safe` in the sibling test above, where the listing is whole — falls
    /// to `Check`. The caches the closed-PR listing makes safe on its own
    /// keep their level.
    #[test]
    fn an_unknown_branch_set_never_makes_a_cache_safe_by_absence() {
        let unknown = RepoContext {
            live_branches: None,
            ..ctx()
        };
        let mut vanished = res(ResourceKind::Cache, 12);
        vanished.git_ref = Some("gone-branch".into());
        assert_eq!(classify(&vanished, &unknown), Safety::Check);

        let mut closed_pr = res(ResourceKind::Cache, 12);
        closed_pr.git_ref = Some("refs/pull/54/merge".into());
        closed_pr.stale_pr = true;
        assert_eq!(classify(&closed_pr, &unknown), Safety::Safe);

        let mut merged = res(ResourceKind::Cache, 12);
        merged.git_ref = Some("refs/heads/still-there".into());
        assert_eq!(classify(&merged, &unknown), Safety::Safe);
    }

    /// The rule's other precondition. A whole listing always holds the
    /// default branch, so this fixture — `main` missing from a whole set —
    /// has no producer today; artificial by construction, like
    /// `a_protected_resource_is_never_safe`'s. It is the one case where the
    /// default-branch guard alone decides: with `default_branch` unknown,
    /// `main`'s cache is not called vanished.
    #[test]
    fn an_unknown_default_branch_never_makes_a_cache_safe_by_absence() {
        let no_default = RepoContext {
            live_branches: Some(["wip".to_string()].into_iter().collect()),
            default_branch: String::new(),
            ..ctx()
        };
        let mut on_main = res(ResourceKind::Cache, 12);
        on_main.git_ref = Some("refs/heads/main".into());
        assert_eq!(classify(&on_main, &no_default), Safety::Check);
    }

    #[test]
    fn a_cache_on_the_default_branch_is_kept() {
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("main".into());
        assert_eq!(classify(&c, &ctx()), Safety::Keep);
    }

    #[test]
    fn a_cache_on_a_live_branch_is_only_worth_checking() {
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("wip".into());
        assert_eq!(classify(&c, &ctx()), Safety::Check);
    }

    #[test]
    fn a_workflow_run_is_judged_by_merge_and_age_not_by_ref_topology() {
        // THE regression test for the Critical review finding: before this
        // fix, `WorkflowRun` shared `classify_by_ref` with `Cache`, so a
        // recent run on a vanished, never-merged branch answered `Safe` via
        // the vanished-branch rule — a rule that belongs to cache, not to a
        // run. `[A]` would have preselected it for one-keystroke deletion.
        let mut vanished_recent = res(ResourceKind::WorkflowRun, 1);
        vanished_recent.git_ref = Some("refs/heads/force-deleted-abandoned".into());
        assert_eq!(classify(&vanished_recent, &ctx()), Safety::Keep);

        // Same vanished, never-merged ref, just old enough: `Check`, not
        // `Keep`. Only `age_days` moved between this and the assertion
        // above — proving the branch's own existence plays no part at all,
        // which is exactly the "no vanished-branch rule for a run" property
        // the table specifies and the old shared function did not honour.
        let mut vanished_old = res(ResourceKind::WorkflowRun, RUN_CHECK_DAYS);
        vanished_old.git_ref = Some("refs/heads/force-deleted-abandoned".into());
        assert_eq!(classify(&vanished_old, &ctx()), Safety::Check);

        // A merged run is Safe regardless of age — the one positive signal
        // that overrides everything else.
        let mut merged = res(ResourceKind::WorkflowRun, 400);
        merged.git_ref = Some("refs/heads/still-there".into());
        assert_eq!(classify(&merged, &ctx()), Safety::Safe);
    }

    #[test]
    fn an_expired_artifact_is_safe_but_a_recent_one_is_kept() {
        // GitHub already made an expired artifact undownloadable; it only
        // occupies a row until deleted. A recent one is still live.
        let mut expired = res(ResourceKind::Artifact, 2);
        expired.label = "github-pages (expiré)".into();
        assert_eq!(classify(&expired, &ctx()), Safety::Safe);

        let recent = res(ResourceKind::Artifact, 2);
        assert_eq!(classify(&recent, &ctx()), Safety::Keep);
    }

    #[test]
    fn an_old_unexpired_artifact_is_only_worth_checking() {
        // Neither of the two fixtures above reaches `age_days >=
        // ARTIFACT_CHECK_DAYS` without also being expired, so this is the
        // one test that can catch that whole branch being deleted (which
        // would collapse straight to `Keep`).
        let mut old = res(ResourceKind::Artifact, ARTIFACT_CHECK_DAYS);
        old.label = "coverage-report".into();
        assert_eq!(classify(&old, &ctx()), Safety::Check);
    }

    #[test]
    fn an_asset_two_releases_back_is_safe_and_the_previous_one_is_not() {
        // THE test of this task. Collapsing these two into one level would
        // force an arbitrary call, and the tool would either offer to delete
        // the release someone is still on, or refuse to clean anything old.
        let mut old = res(ResourceKind::ReleaseAsset, 40);
        old.label = "josephine-linux (v0.10.0)".into();
        assert_eq!(classify(&old, &ctx()), Safety::Safe);

        let mut previous = res(ResourceKind::ReleaseAsset, 20);
        previous.label = "josephine-linux (v0.11.0)".into();
        assert_eq!(classify(&previous, &ctx()), Safety::Check);

        let mut latest = res(ResourceKind::ReleaseAsset, 2);
        latest.label = "josephine-linux (v0.12.0)".into();
        assert_eq!(classify(&latest, &ctx()), Safety::Keep);
    }

    /// `classify_asset`'s `None` arm, at its level: a tag absent from
    /// `release_tags` is a label the parse got wrong, not an old release
    /// (F-M1), so it reads `Check` — never `Safe`, which `[A]` would take,
    /// nor `Keep`, which would hide it from `[V]`. The fixture's tag, well
    /// formed but absent, isolates the arm from the parse itself.
    #[test]
    fn an_asset_whose_tag_is_absent_from_the_release_list_is_only_worth_checking() {
        let mut unlisted = res(ResourceKind::ReleaseAsset, 900);
        unlisted.label = "josephine-linux (v0.1.0)".into();
        assert_eq!(classify(&unlisted, &ctx()), Safety::Check);
    }

    /// Final review minor, promoted (F-M1): `scan::distinct_release_tags`
    /// builds `release_tags` from the very assets whose labels
    /// `tag_in_label` parses, so a tag missing from the list never means an
    /// older release — only a label the parse got wrong. A tag holding its
    /// own parenthesis is one: `josephine-linux (v1.0(beta))` reads `beta)`.
    /// That asset belongs to the newest release, and must not read ⛑.
    #[test]
    fn the_newest_release_tagged_with_a_parenthesis_has_no_safe_asset() {
        let tags = RepoContext {
            release_tags: vec!["v1.0(beta)".into(), "v0.9.0".into(), "v0.8.0".into()],
            ..ctx()
        };
        let mut newest = res(ResourceKind::ReleaseAsset, 2);
        newest.label = format!("{} ({})", "josephine-linux", "v1.0(beta)");
        assert_ne!(classify(&newest, &tags), Safety::Safe);
    }

    #[test]
    fn a_tagged_package_version_is_kept_and_an_untagged_one_is_safe() {
        let mut tagged = res(ResourceKind::PackageVersion, 10);
        tagged.protected = true;
        assert_eq!(classify(&tagged, &ctx()), Safety::Keep);

        let untagged = res(ResourceKind::PackageVersion, 10);
        assert_eq!(classify(&untagged, &ctx()), Safety::Safe);
    }

    #[test]
    fn every_branch_class_reaches_its_own_safety_level() {
        // The `Check` row was unreachable before this fix: `scan::
        // branch_resources` sets `protected: class != BranchClass::Merged`,
        // so `Live` carried `protected: true` and the old top-level `if
        // r.protected { Keep }` resolved it before the `Branch` arm ever
        // ran. Reading `branch_class` instead of `protected` is what makes
        // this row reachable again.
        let mut merged = res(ResourceKind::Branch, 10);
        merged.branch_class = Some(BranchClass::Merged);
        assert_eq!(classify(&merged, &ctx()), Safety::Safe);

        let mut default = res(ResourceKind::Branch, 10);
        default.branch_class = Some(BranchClass::Default);
        default.protected = true; // matches scan::branch_resources' own computation
        assert_eq!(classify(&default, &ctx()), Safety::Keep);

        let mut protected = res(ResourceKind::Branch, 10);
        protected.branch_class = Some(BranchClass::Protected);
        protected.protected = true;
        assert_eq!(classify(&protected, &ctx()), Safety::Keep);

        let mut live = res(ResourceKind::Branch, 10);
        live.branch_class = Some(BranchClass::Live);
        // `scan::branch_resources` sets this too — the fix is that
        // `protected` no longer overrides the branch arm's own answer to
        // `Keep`; it only ever forbids `Safe`.
        live.protected = true;
        assert_eq!(classify(&live, &ctx()), Safety::Check);
    }

    #[test]
    fn a_tag_is_always_kept() {
        let t = res(ResourceKind::Tag, 400);
        assert_eq!(classify(&t, &ctx()), Safety::Keep);
    }

    #[test]
    fn a_protected_resource_is_never_safe() {
        // A merged branch is `Safe` on its own — so `protected` is the only
        // thing standing between this fixture and `Safe`, and removing the
        // backstop in `classify` flips the answer. This also doubles as the
        // backstop's own regression test: `scan::branch_resources` never
        // actually produces a `Merged` branch with `protected: true`
        // (`protected` is `class != BranchClass::Merged`, and `Merged` is
        // excluded from that), so this fixture — the one case that can reach
        // `Safe` while `protected` — has no real producer today. Artificial
        // by construction; that is the point: it guards the family that
        // might add one later.
        let mut b = res(ResourceKind::Branch, 40);
        b.branch_class = Some(BranchClass::Merged);
        b.protected = true;
        assert_eq!(classify(&b, &ctx()), Safety::Keep);

        // Same branch without the flag: `Safe`. This is the half that proves
        // the fixture reaches the branch arm's `Merged` case at all.
        let mut unguarded = res(ResourceKind::Branch, 40);
        unguarded.branch_class = Some(BranchClass::Merged);
        assert_eq!(classify(&unguarded, &ctx()), Safety::Safe);
    }

    #[test]
    fn a_repository_is_never_marked_whatever_its_age() {
        // v0.5's rule: `pushed_at` is not proof of abandonment. A finished
        // library does not move for two years without being dead.
        let old = res(ResourceKind::Repository, 775);
        assert_eq!(classify(&old, &ctx()), Safety::Keep);
    }
}
