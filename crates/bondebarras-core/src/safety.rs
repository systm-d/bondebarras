//! How safe a resource is to delete, on three levels.
//!
//! Two levels would force an arbitrary call. An asset from the release before
//! last and a cache from a closed pull request are not dead in the same way:
//! someone may still be pulling the first, while nothing references the
//! second. Collapsing them means lying in one direction or the other.

use crate::model::{Resource, ResourceKind};
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
    /// Branches that still exist.
    pub live_branches: HashSet<String>,
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
/// `protected` outranks everything: it is the bulk-selection gate, and a
/// resource behind it must never be offered as safe regardless of its age or
/// its refs.
pub fn classify(r: &Resource, ctx: &RepoContext) -> Safety {
    if r.protected {
        return Safety::Keep;
    }

    match r.kind {
        // Never marked at any level. `pushed_at` is not proof of abandonment.
        ResourceKind::Repository => Safety::Keep,
        ResourceKind::Tag => Safety::Keep,

        ResourceKind::Cache | ResourceKind::WorkflowRun => classify_by_ref(r, ctx),
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
        // Untagged and orphaned attestations already carry `protected: false`
        // from `packages::classify`; a tagged version carries `protected`.
        ResourceKind::PackageVersion => Safety::Safe,
        ResourceKind::Branch => {
            let name = r.label.as_str();
            if ctx.merged_refs.contains(name) {
                Safety::Safe
            } else {
                Safety::Check
            }
        }
    }
}

/// A cache or a run, judged by the ref it is attached to.
fn classify_by_ref(r: &Resource, ctx: &RepoContext) -> Safety {
    // A cache pinned to a closed pull request carries `refs/pull/32/merge`,
    // which names no branch — `merged_refs` holds `head.ref` values and would
    // never match it. `stale_pr` already resolves that case, from the same
    // closed-PR listing, and has since v0.1. Reuse it rather than re-deriving.
    if r.stale_pr {
        return Safety::Safe;
    }

    let Some(name) = r.git_ref.as_deref().map(strip_ref_prefix) else {
        return if r.kind == ResourceKind::WorkflowRun && r.age_days >= RUN_CHECK_DAYS {
            Safety::Check
        } else {
            Safety::Keep
        };
    };

    if name == ctx.default_branch {
        return Safety::Keep;
    }
    if ctx.merged_refs.contains(name) {
        return Safety::Safe;
    }
    // A ref naming a branch that no longer exists: nothing pulls it by name,
    // and no pull request will bring it back.
    if !ctx.live_branches.contains(name) && !name.starts_with("refs/pull/") {
        return Safety::Safe;
    }
    Safety::Check
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
        // A tag absent from the listing: its release is older than the page we
        // fetched, so it is at least as old as the oldest we know.
        None => Safety::Safe,
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
            merged_refs: ["claude/landing".to_string()].into_iter().collect(),
            live_branches: ["main".to_string(), "wip".to_string()]
                .into_iter()
                .collect(),
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
    fn a_cache_on_a_closed_prs_ref_is_safe() {
        let mut c = res(ResourceKind::Cache, 12);
        c.git_ref = Some("claude/landing".into());
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

    #[test]
    fn a_protected_resource_is_never_safe() {
        // `protected` is the bulk-selection gate and outranks every other
        // signal. A tag carries it, and a tag is what releases point at.
        let mut t = res(ResourceKind::Tag, 400);
        t.protected = true;
        assert_eq!(classify(&t, &ctx()), Safety::Keep);
    }

    #[test]
    fn a_repository_is_never_marked_whatever_its_age() {
        // v0.5's rule: `pushed_at` is not proof of abandonment. A finished
        // library does not move for two years without being dead.
        let old = res(ResourceKind::Repository, 775);
        assert_eq!(classify(&old, &ctx()), Safety::Keep);
    }
}
