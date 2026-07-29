//! Which branches are dead, decided without a request per branch.
//!
//! Comparing every branch against the default would cost one `compare` call
//! each — a hundred on a real repository. It is not needed: a closed pull
//! request carries `head.ref` and `merged_at`, and the closed-PR listing is
//! already fetched for the caches' ⚑ flag. A branch a merged PR came from is
//! a branch nobody works on any more.

use std::collections::HashSet;

/// A branch as the listing endpoint reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRef {
    pub name: String,
    pub protected: bool,
}

/// Why a branch is or is not a candidate for deletion.
///
/// `branch_is_dead` collapses this down to one bool — "offer it or don't" —
/// which is the right amount of detail for bulk selection, but the wrong
/// amount for what a row tells a human: under the old scheme `Default`,
/// `Protected` and `Live` all rendered the same word, "protégée", even
/// though only the first two are backed by anything GitHub actually
/// enforces — a `Live` branch is simply unmerged, a fact about the
/// repository's history, not a permission GitHub granted or refused.
/// `classify_branch` keeps the same three exclusions `branch_is_dead`
/// checks, in the same order of severity, but names which one applies
/// instead of folding all three into one bool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchClass {
    /// A merged pull request came from it. Nobody works on it any more.
    Merged,
    /// The repository's default branch. Deleting it breaks the repository.
    Default,
    /// Protected by a GitHub branch rule — the API says so.
    Protected,
    /// Simply not merged. Someone may still be working on it, so the tool
    /// does not preselect it — but it makes no claim beyond that.
    Live,
}

/// Classify one branch, in the same order of severity `branch_is_dead` used
/// to check: the default branch first (deleting it breaks the repository),
/// then a GitHub-protected one (someone deliberately said no), then whether
/// a merged pull request came from it. Anything left over is simply
/// unmerged — alive, not because anything protects it, only because nothing
/// has proven it dead.
pub fn classify_branch(
    b: &BranchRef,
    default_branch: &str,
    merged_refs: &HashSet<String>,
) -> BranchClass {
    if b.name == default_branch {
        BranchClass::Default
    } else if b.protected {
        BranchClass::Protected
    } else if merged_refs.contains(&b.name) {
        BranchClass::Merged
    } else {
        BranchClass::Live
    }
}

/// True when the branch can be offered for deletion.
///
/// Three exclusions, in order of how badly getting them wrong would hurt:
/// the default branch (deleting it breaks the repository), a protected branch
/// (someone deliberately said no), and a branch whose pull request was closed
/// *without* merging — that work was rejected, not integrated, and deleting it
/// throws away something a person may still intend to revisit. A thin
/// wrapper over `classify_branch` now, so the two can never disagree about
/// which branches count as dead.
pub fn branch_is_dead(b: &BranchRef, default_branch: &str, merged_refs: &HashSet<String>) -> bool {
    classify_branch(b, default_branch, merged_refs) == BranchClass::Merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(name: &str, protected: bool) -> BranchRef {
        BranchRef {
            name: name.to_string(),
            protected,
        }
    }

    fn merged(refs: &[&str]) -> HashSet<String> {
        refs.iter().map(|r| r.to_string()).collect()
    }

    #[test]
    fn a_branch_whose_pr_was_merged_is_dead() {
        let m = merged(&["claude/landing-3jbqk4"]);
        assert!(branch_is_dead(
            &b("claude/landing-3jbqk4", false),
            "main",
            &m
        ));
    }

    #[test]
    fn a_branch_with_no_merged_pr_is_alive() {
        // THE test. A PR closed *without* merging leaves its branch alive —
        // the work was rejected, not integrated, and may still be resumed.
        // Without this case a classifier keying on "closed" alone would pass
        // and the tool would offer to delete work someone meant to revisit.
        let m = merged(&["claude/landing-3jbqk4"]);
        assert!(!branch_is_dead(&b("feature/rejected", false), "main", &m));
    }

    #[test]
    fn the_default_branch_is_never_dead() {
        // Even if a merged PR targeted it — a PR merged *into* main puts
        // main nowhere near the head refs, but a mis-shaped fixture could.
        let m = merged(&["main"]);
        assert!(!branch_is_dead(&b("main", false), "main", &m));
    }

    #[test]
    fn a_protected_branch_is_never_dead() {
        let m = merged(&["release/2.0"]);
        assert!(!branch_is_dead(&b("release/2.0", true), "main", &m));
    }

    /// Exercises all four `BranchClass` outcomes from one fixture, in the
    /// same priority order `classify_branch` checks them: a branch that is
    /// simultaneously the default branch, GitHub-protected, *and* backed by
    /// a merged PR must classify as `Default`, not `Protected` or `Merged` —
    /// a wrong priority order (e.g. checking `merged_refs` first) would
    /// return the wrong class for this exact row while still passing a
    /// fixture that only tested one property in isolation.
    #[test]
    fn classify_branch_names_all_four_classes_in_priority_order() {
        let m = merged(&["main", "claude/landing-3jbqk4"]);

        assert_eq!(
            classify_branch(&b("main", true), "main", &m),
            BranchClass::Default,
            "the default branch outranks both protected and merged"
        );
        assert_eq!(
            classify_branch(&b("release/2.0", true), "main", &m),
            BranchClass::Protected
        );
        assert_eq!(
            classify_branch(&b("claude/landing-3jbqk4", false), "main", &m),
            BranchClass::Merged
        );
        assert_eq!(
            classify_branch(&b("feature/rejected", false), "main", &m),
            BranchClass::Live
        );
    }

    /// `branch_is_dead` must still say exactly what it always said — the
    /// four fixtures above, distilled to yes/no. Locks the delegation to
    /// `classify_branch` against silently changing which branches bulk
    /// selection offers.
    #[test]
    fn branch_is_dead_agrees_with_classify_branch_on_every_class() {
        let m = merged(&["main", "claude/landing-3jbqk4"]);
        assert!(!branch_is_dead(&b("main", true), "main", &m));
        assert!(!branch_is_dead(&b("release/2.0", true), "main", &m));
        assert!(branch_is_dead(
            &b("claude/landing-3jbqk4", false),
            "main",
            &m
        ));
        assert!(!branch_is_dead(&b("feature/rejected", false), "main", &m));
    }
}
