//! Two-stage scanning.
//!
//! Stage 1 runs at launch and only touches org-level aggregates — three
//! requests per org, so fifteen orgs still land in a few seconds. Stage 2
//! fetches a repository's individual resources, and only when the user opens
//! it. Paying only for what you look at is what keeps manual navigation
//! viable across a hundred repositories.

use crate::api::{Client, artifacts, caches, packages, prs, repos, runs};
use crate::model::{OrgSummary, Resource, ResourceKind};
use crate::packages::{PackageVersion, VersionClass, classify};
use crate::stale::is_stale;
use anyhow::Result;
use std::collections::HashSet;

/// Stage 1: cache aggregates and repository list for each org.
///
/// An org that fails — revoked permission, network blip — is dropped from the
/// result rather than failing the whole scan. With fifteen orgs, one bad
/// permission must not blank the screen.
pub async fn overview(client: &Client, orgs: &[String]) -> Vec<OrgSummary> {
    let futures = orgs.iter().map(|org| async move {
        let summaries = caches::usage_by_repository(client, org).await.ok()?;
        let refs = repos::list(client, org).await.ok()?;

        let cache_bytes = summaries.iter().map(|r| r.cache_bytes).sum();
        let cache_count = summaries.iter().map(|r| r.cache_count).sum();

        // Repos with no cache still belong in the tree: they may hold
        // artifacts or runs, which stage 2 will surface. Repos merged in
        // here take their real visibility from `repos::list`; a repo that
        // appears only in the cache report — never in the repo listing —
        // keeps the `private: false` the cache report defaulted it to.
        let mut repos_out = summaries;
        for repo in refs {
            if let Some(existing) = repos_out.iter_mut().find(|r| r.name == repo.name) {
                existing.private = repo.private;
            } else {
                repos_out.push(crate::model::RepoSummary {
                    name: repo.name,
                    cache_bytes: 0,
                    cache_count: 0,
                    private: repo.private,
                });
            }
        }
        repos_out.sort_by_key(|r| std::cmp::Reverse(r.cache_bytes));

        // Third and last stage-1 request. Deliberately not `?`-propagated: an
        // org whose billing is refused is still worth showing.
        let billing = crate::api::billing::fetch(client, org).await;

        Some(OrgSummary {
            login: org.clone(),
            cache_bytes,
            cache_count,
            repos: repos_out,
            billing,
        })
    });

    let mut out: Vec<OrgSummary> = futures::future::join_all(futures)
        .await
        .into_iter()
        .flatten()
        .collect();
    out.sort_by_key(|o| std::cmp::Reverse(o.cache_bytes));
    out
}

/// Stage 2: every deletable resource of one repository, already flagged.
pub async fn repo_detail(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>> {
    let (caches_r, artifacts_r, runs_r, versions_r, closed) = futures::join!(
        caches::list(client, owner, repo),
        artifacts::list(client, owner, repo),
        runs::list(client, owner, repo),
        // This account's convention: a repo's image, when it publishes one,
        // is named after the repo. A repo with no image 404s — `versions`
        // already turns that into an empty list, not an error.
        packages::versions(client, owner, repo),
        prs::closed_numbers(client, owner, repo),
    );

    let mut items = caches_r?;
    items.extend(artifacts_r?);
    items.extend(runs_r?);
    items.extend(version_resources(versions_r?));

    // A failed PR listing costs the flag, not the listing: everything still
    // shows, just without the ⚑ shortcut.
    mark_stale(&mut items, &closed.unwrap_or_default());

    items.sort_by_key(|i| std::cmp::Reverse(i.size_bytes));
    Ok(items)
}

/// Turn classified package versions into drill-down rows.
///
/// `classify` maps over `versions` in order and returns exactly one
/// `(id, VersionClass)` per input, so zipping is safe and needs no lookup.
fn version_resources(versions: Vec<PackageVersion>) -> Vec<Resource> {
    let classes = classify(&versions);
    versions
        .into_iter()
        .zip(classes)
        .map(|(v, (_, class))| Resource {
            kind: ResourceKind::PackageVersion,
            id: v.id,
            label: version_label(&v, class),
            // No size field exists for a package version, under any name —
            // see `api::packages`.
            size_bytes: 0,
            age_days: v.age_days,
            // Packages carry no git ref: the ⚑ stale-PR flag does not apply.
            git_ref: None,
            stale_pr: false,
            // Only a tagged version is live-referenced by name — deleting
            // `latest` breaks whatever pulls it. Untagged and orphaned
            // attestations are exactly the two classes nothing depends on.
            protected: class == VersionClass::Tagged,
        })
        .collect()
}

/// A version's row label, carrying the reason it is offered so the user does
/// not have to trust the classification blindly.
fn version_label(v: &PackageVersion, class: VersionClass) -> String {
    let ident = if v.tags.is_empty() {
        v.digest.clone()
    } else {
        v.tags.join(", ")
    };
    match class {
        VersionClass::Untagged => format!("{ident} (sans tag)"),
        VersionClass::OrphanedAttestation => format!("{ident} (attestation orpheline)"),
        // Never preselected — deleting a real tag like `latest` breaks
        // deployments — so the label carries no extra warning of its own.
        VersionClass::Tagged => ident,
    }
}

/// Flag every resource whose ref belongs to a closed pull request.
pub fn mark_stale(items: &mut [Resource], closed_prs: &HashSet<u64>) {
    for item in items.iter_mut() {
        item.stale_pr = is_stale(item.git_ref.as_deref(), closed_prs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cache(id: u64, git_ref: &str) -> Resource {
        Resource {
            kind: ResourceKind::Cache,
            id,
            label: format!("cache-{id}"),
            size_bytes: 1_000,
            age_days: 12,
            git_ref: Some(git_ref.to_string()),
            stale_pr: false,
            protected: false,
        }
    }

    #[test]
    fn mark_stale_flags_only_caches_of_closed_prs() {
        let mut items = vec![
            cache(1, "refs/pull/32/merge"),
            cache(2, "refs/heads/main"),
            cache(3, "refs/pull/99/merge"),
        ];
        let closed = HashSet::from([32_u64]);

        mark_stale(&mut items, &closed);

        assert!(items[0].stale_pr);
        assert!(!items[1].stale_pr);
        assert!(!items[2].stale_pr);
    }

    /// A tagged version arriving as `protected: false` would make
    /// `commands::clean::select`'s bulk-selection guard useless — the whole
    /// point of the flag is that it is set from the classification, not left
    /// at its default. `Untagged` and `OrphanedAttestation` are exactly the
    /// two classes real deployments do not depend on by name, so both must
    /// come through unprotected.
    #[test]
    fn only_a_tagged_version_is_protected() {
        // The digest #1's attestation tag signs — deliberately absent from
        // this fixture's own digests, which is what makes #1 an orphaned
        // attestation rather than a live one (see
        // `packages::an_attestation_whose_subject_is_gone_is_orphaned`: the
        // signed image must be gone, not merely a sibling in the list).
        const SIGNED_DIGEST: &str =
            "sha256-1a65eb30f0e36fc41bb07724b11e53ada5e810382f39143698b00c470f019b80";
        let versions = vec![
            PackageVersion {
                id: 1,
                digest: "sha256:1d7018e5672547cced06883706367832e5f1be5fa90bc2038ad308e19958e80e"
                    .into(),
                tags: vec![SIGNED_DIGEST.into()],
                age_days: 30,
            },
            PackageVersion {
                id: 2,
                digest: "sha256:9a26c70801010123223adb5e73ff703aca86c15e19b30124ede5628a1e185826"
                    .into(),
                tags: vec![],
                age_days: 30,
            },
            PackageVersion {
                id: 3,
                digest: "sha256:deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                    .into(),
                tags: vec!["latest".into()],
                age_days: 30,
            },
        ];
        // classify(): #1's tag signs a digest none of these three carry, so
        // #1 is OrphanedAttestation; #2 has no tags, so Untagged; #3 carries
        // a real tag, so Tagged.
        let items = version_resources(versions);

        let protected = |id: u64| items.iter().find(|r| r.id == id).unwrap().protected;
        assert!(
            !protected(1),
            "an orphaned attestation must not be protected"
        );
        assert!(!protected(2), "an untagged version must not be protected");
        assert!(protected(3), "a tagged version must be protected");
    }

    #[tokio::test]
    async fn repo_detail_folds_in_the_repos_homonymous_package_versions() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/repolens/actions/caches"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "actions_caches": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/repolens/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/repolens/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/repolens/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        // The package name follows the repo name on this account: `repolens`
        // publishes `ghcr.io/systm-d/repolens`, not some other name.
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/repolens/versions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "id": 862511085,
                  "name": "sha256:9a26c70801010123223adb5e73ff703aca86c15e19b30124ede5628a1e185826",
                  "created_at": "2026-05-13T16:11:30Z",
                  "metadata": { "container": { "tags": [] } } }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "repolens").await.unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ResourceKind::PackageVersion);
        assert_eq!(items[0].id, 862511085);
        // GitHub reports no size for a package version, under any name.
        assert_eq!(items[0].size_bytes, 0);
        // Packages carry no git ref: the ⚑ stale-PR flag does not apply to them.
        assert!(!items[0].stale_pr);
        // The label must carry why this row is offered.
        assert!(
            items[0].label.contains("sans tag"),
            "an untagged version's label must say so: {:?}",
            items[0].label
        );
    }

    #[tokio::test]
    async fn an_org_that_fails_is_dropped_not_fatal() {
        // `overview` tolerates a failing org so one broken permission does not
        // blank the whole screen: it must drop the org, not error out or
        // panic the whole scan.
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/orgs/healthy-org/actions/cache/usage-by-repository"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 1,
                "repository_cache_usages": [
                    { "full_name": "healthy-org/josephine",
                      "active_caches_size_in_bytes": 12_372_371_816_u64,
                      "active_caches_count": 30 }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/healthy-org/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "josephine" }
            ])))
            .mount(&server)
            .await;

        // The broken org's cache endpoint 404s — a revoked permission or a
        // network blip, exactly the case `overview` must tolerate.
        Mock::given(method("GET"))
            .and(path("/orgs/broken-org/actions/cache/usage-by-repository"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/broken-org/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let orgs = vec!["healthy-org".to_string(), "broken-org".to_string()];

        let summaries = overview(&client, &orgs).await;

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].login, "healthy-org");
        assert!(summaries.iter().all(|s| s.login != "broken-org"));
    }

    /// Locks finding 1's plumbing: `billing::included_minutes` can only tell
    /// a private repo from a public one if `overview` actually carries the
    /// `private` flag from `repos::list` onto `RepoSummary` — for a repo that
    /// has cache usage (merged into an existing row) and one that does not
    /// (pushed as a new row). A wrong implementation that keeps the cache
    /// report's default `false` for both would pass every other scan test
    /// while silently reporting both repos as public.
    #[tokio::test]
    async fn overview_carries_repo_visibility_from_the_repo_listing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/orgs/SecondBrain-io/actions/cache/usage-by-repository",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "repository_cache_usages": [
                    { "full_name": "SecondBrain-io/monolith-back",
                      "active_caches_size_in_bytes": 1000, "active_caches_count": 2 }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/SecondBrain-io/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                // Has cache usage above: merged into the existing row.
                { "name": "monolith-back", "private": true },
                // No cache usage: pushed as a new row.
                { "name": "empty-private-repo", "private": true },
                { "name": "public-site", "private": false }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/organizations/SecondBrain-io/settings/billing/usage"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(&client, &["SecondBrain-io".to_string()]).await;

        let repos = &out[0].repos;
        let find = |name: &str| repos.iter().find(|r| r.name == name).unwrap();
        assert!(
            find("monolith-back").private,
            "merged row must stay private"
        );
        assert!(
            find("empty-private-repo").private,
            "pushed row must stay private"
        );
        assert!(!find("public-site").private, "public repo must stay public");
    }

    #[tokio::test]
    async fn an_org_without_billing_access_is_still_scanned() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/actions/cache/usage-by-repository"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "repository_cache_usages": [
                    { "full_name": "systm-d/josephine",
                      "active_caches_size_in_bytes": 1000, "active_caches_count": 2 }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "josephine" }
            ])))
            .mount(&server)
            .await;
        // Billing refused: the org must survive with `billing: None`.
        Mock::given(method("GET"))
            .and(path("/organizations/systm-d/settings/billing/usage"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(&client, &["systm-d".to_string()]).await;

        assert_eq!(out.len(), 1, "a billing 403 must not drop the org");
        assert_eq!(out[0].cache_bytes, 1000);
        assert!(out[0].billing.is_none());
    }
}
