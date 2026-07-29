//! Repository classification: which repositories are archivable candidates.
//!
//! Archiving turns a repository read-only, GitHub's own Actions included —
//! which is why it belongs in this tool despite freeing no bytes of its own
//! (see `model::ResourceKind::Repository`): it closes the tap that produces
//! the caches, artifacts and workflow runs v0.1 exists to clean, instead of
//! mopping them up forever. Reversible, unlike everything else this crate
//! deletes, which is exactly what keeps it at `RiskTier::Medium` rather than
//! the nuclear tier — and what makes repository *deletion* unnecessary,
//! permanently out of scope.

/// Why a repository is or is not an archiving candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoClass {
    /// Not archived yet, and the caller can administer it: a genuine
    /// candidate.
    Archivable,
    /// Already read-only. Offering it again would not be a lie GitHub
    /// contradicts, but it would not be useful either — there is nothing
    /// left to do.
    AlreadyArchived,
    /// The token cannot administer this repository. GitHub would answer 403
    /// to the archive request; offering a tick the API will refuse is a lie
    /// the API then contradicts, in front of the user.
    NoAdminRights,
}

/// Classify one repository for archiving.
///
/// Two independent disqualifications, checked in the order that makes the
/// resulting message useful: already archived first, since that is true
/// regardless of rights and is the more informative thing to say, then
/// missing admin rights. A repository that is both already archived *and*
/// off-limits to this token reads as `AlreadyArchived` — there is nothing
/// archiving-shaped left to refuse it for, so naming the rights problem
/// would be a red herring.
pub fn classify_repo(archived: bool, admin: bool) -> RepoClass {
    if archived {
        RepoClass::AlreadyArchived
    } else if !admin {
        RepoClass::NoAdminRights
    } else {
        RepoClass::Archivable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_repo_the_user_administers_is_archivable() {
        assert_eq!(classify_repo(false, true), RepoClass::Archivable);
    }

    #[test]
    fn an_already_archived_repo_is_not_a_candidate() {
        assert_eq!(classify_repo(true, true), RepoClass::AlreadyArchived);
    }

    #[test]
    fn a_repo_without_admin_rights_is_not_a_candidate() {
        // The API would answer 403. Offering an action it will refuse is a
        // lie the API then contradicts, in front of the user.
        assert_eq!(classify_repo(false, false), RepoClass::NoAdminRights);
    }

    #[test]
    fn already_archived_outranks_missing_rights() {
        // Both disqualify; the message should say the useful one.
        assert_eq!(classify_repo(true, false), RepoClass::AlreadyArchived);
    }
}
