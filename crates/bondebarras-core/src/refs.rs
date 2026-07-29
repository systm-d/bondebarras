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

/// True when the branch can be offered for deletion.
///
/// Three exclusions, in order of how badly getting them wrong would hurt:
/// the default branch (deleting it breaks the repository), a protected branch
/// (someone deliberately said no), and a branch whose pull request was closed
/// *without* merging — that work was rejected, not integrated, and deleting it
/// throws away something a person may still intend to revisit.
pub fn branch_is_dead(b: &BranchRef, default_branch: &str, merged_refs: &HashSet<String>) -> bool {
    if b.name == default_branch || b.protected {
        return false;
    }
    merged_refs.contains(&b.name)
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
}
