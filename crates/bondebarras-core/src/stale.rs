//! Detection of caches attached to a closed pull request.
//!
//! GitHub keys Actions caches per git ref. A cache on `refs/pull/32/merge`
//! becomes dead weight the moment PR #32 is closed or merged, but GitHub only
//! evicts at the 10 GB per-repo ceiling or after 7 days without a read — so it
//! lingers, and it crowds out the caches that still matter. Flagging those is
//! the highest-volume, lowest-risk cleanup the tool offers.

use std::collections::HashSet;

/// Pull request number carried by a ref, if it is a pull ref.
///
/// `refs/pull/32/merge` -> `Some(32)`; anything else -> `None`.
pub fn pr_number_from_ref(git_ref: &str) -> Option<u64> {
    git_ref
        .strip_prefix("refs/pull/")?
        .split('/')
        .next()?
        .parse()
        .ok()
}

/// True when the ref belongs to a pull request that is no longer open.
pub fn is_stale(git_ref: Option<&str>, closed_prs: &HashSet<u64>) -> bool {
    git_ref
        .and_then(pr_number_from_ref)
        .is_some_and(|n| closed_prs.contains(&n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn extracts_the_pr_number_from_a_pull_ref() {
        assert_eq!(pr_number_from_ref("refs/pull/32/merge"), Some(32));
        assert_eq!(pr_number_from_ref("refs/pull/7/head"), Some(7));
    }

    #[test]
    fn branch_refs_carry_no_pr_number() {
        assert_eq!(pr_number_from_ref("refs/heads/main"), None);
        assert_eq!(pr_number_from_ref("refs/tags/v1.0.0"), None);
        assert_eq!(pr_number_from_ref("refs/pull/abc/merge"), None);
    }

    #[test]
    fn a_cache_is_stale_only_when_its_pr_is_closed() {
        let closed = HashSet::from([25_u64, 32]);
        assert!(is_stale(Some("refs/pull/32/merge"), &closed));
        assert!(!is_stale(Some("refs/pull/99/merge"), &closed));
        assert!(!is_stale(Some("refs/heads/main"), &closed));
        assert!(!is_stale(None, &closed));
    }

    /// A wrong `Some(n)` here would flag a live cache as dead weight, and the
    /// ⚑ shortcut deletes every flagged row in one keystroke. These lock the
    /// fail-closed behaviour against a future refactor of the parse chain.
    #[test]
    fn malformed_pull_refs_never_yield_a_number() {
        assert_eq!(pr_number_from_ref(""), None);
        assert_eq!(pr_number_from_ref("refs/pull/"), None);
        assert_eq!(pr_number_from_ref("refs/pull//merge"), None);
        assert_eq!(pr_number_from_ref("refs/pull/-1/merge"), None);
        // 20 digits — overflows u64, whose max is ~1.8e19.
        assert_eq!(
            pr_number_from_ref("refs/pull/99999999999999999999/merge"),
            None
        );
    }
}
