//! Classification of container package versions.
//!
//! GitHub exposes no size for a package version — not in the versions API, not
//! as a billing SKU. So unlike caches and artifacts, this family cannot be
//! ranked or justified by bytes. What it can be ranked by is *deadness*: a
//! layer no tag points at, and a signature whose image is gone.

/// One version of a container package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageVersion {
    pub id: u64,
    /// The version's own digest, in `sha256:<64 hex>` form.
    pub digest: String,
    pub tags: Vec<String>,
    pub age_days: i64,
}

/// What a version is, from safest to riskiest to delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionClass {
    /// No tag points at it. The bulk of the waste.
    Untagged,
    /// A signature or attestation whose subject image no longer exists.
    OrphanedAttestation,
    /// At least one real tag. Never preselected — deleting `latest` breaks
    /// deployments.
    Tagged,
}

/// The digest an attestation tag signs, if the tag is one.
///
/// Cosign and GitHub attach attestations by tagging them
/// `sha256-<digest of the signed image>`. The separator is the only difference
/// from a digest: `-` in the tag, `:` in the `name`.
///
/// The 64-lowercase-hex check is not pedantry. A tag someone chose that merely
/// starts with `sha256-` must not be read as an attestation, because the
/// consequence of getting it wrong is deleting a real image.
pub fn attested_digest(tag: &str) -> Option<String> {
    let hex = tag.strip_prefix("sha256-")?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    Some(format!("sha256:{hex}"))
}

/// Classify every version, resolving attestations against the versions present.
pub fn classify(versions: &[PackageVersion]) -> Vec<(u64, VersionClass)> {
    let present: std::collections::HashSet<&str> =
        versions.iter().map(|v| v.digest.as_str()).collect();

    versions
        .iter()
        .map(|v| {
            let class = if v.tags.is_empty() {
                VersionClass::Untagged
            } else if v
                .tags
                .iter()
                .all(|t| attested_digest(t).is_some_and(|d| !present.contains(d.as_str())))
            {
                // Every tag is an attestation for something that is gone. If
                // even one tag is a real tag, or points at a live image, this
                // is not orphaned.
                VersionClass::OrphanedAttestation
            } else {
                VersionClass::Tagged
            };
            (v.id, class)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(id: u64, digest: &str, tags: &[&str]) -> PackageVersion {
        PackageVersion {
            id,
            digest: digest.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            age_days: 30,
        }
    }

    /// Real digests captured from `systm-d/repolens` on 2026-07-29.
    const IMG: &str = "sha256:1a65eb30f0e36fc41bb07724b11e53ada5e810382f39143698b00c470f019b80";
    const ATT_TAG: &str = "sha256-1a65eb30f0e36fc41bb07724b11e53ada5e810382f39143698b00c470f019b80";
    const OTHER: &str = "sha256:9a26c70801010123223adb5e73ff703aca86c15e19b30124ede5628a1e185826";

    #[test]
    fn an_attestation_tag_yields_the_digest_it_signs() {
        assert_eq!(attested_digest(ATT_TAG).as_deref(), Some(IMG));
    }

    #[test]
    fn ordinary_and_malformed_tags_are_not_attestations() {
        // Only 64 lowercase hex after the prefix counts. Anything else is a
        // tag someone chose, and deleting it would delete a real image.
        assert_eq!(attested_digest("latest"), None);
        assert_eq!(attested_digest("2.0.2"), None);
        assert_eq!(attested_digest("sha256-"), None);
        assert_eq!(attested_digest("sha256-zzz"), None);
        assert_eq!(attested_digest("sha256-1a65eb30"), None); // too short
        assert_eq!(
            attested_digest(
                "sha256-1A65EB30F0E36FC41BB07724B11E53ADA5E810382F39143698B00C470F019B80"
            ),
            None
        );
    }

    #[test]
    fn an_attestation_whose_subject_survives_is_not_orphaned() {
        // THE test of this task. Flagging a live image's signature would
        // offer to delete the proof that a deployed image is authentic.
        let versions = vec![
            v(1, "sha256:1d7018e5", &[ATT_TAG]),
            v(2, IMG, &["latest", "2.0.2"]),
        ];
        let classes = classify(&versions);
        assert_eq!(
            classes,
            vec![(1, VersionClass::Tagged), (2, VersionClass::Tagged)]
        );
    }

    #[test]
    fn an_attestation_whose_subject_is_gone_is_orphaned() {
        let versions = vec![
            v(1, "sha256:1d7018e5", &[ATT_TAG]),
            v(2, OTHER, &["latest"]),
        ];
        let classes = classify(&versions);
        assert_eq!(
            classes,
            vec![
                (1, VersionClass::OrphanedAttestation),
                (2, VersionClass::Tagged)
            ]
        );
    }

    #[test]
    fn a_version_with_no_tag_is_untagged() {
        let classes = classify(&[v(1, OTHER, &[])]);
        assert_eq!(classes, vec![(1, VersionClass::Untagged)]);
    }
}
