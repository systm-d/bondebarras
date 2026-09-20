//! Two-stage scanning.
//!
//! Stage 1 runs at launch and only touches org-level data: the two reads
//! that define an org (cache usage, repository list), and degradable reads
//! that only enrich it — each refused on its own, never dropping the org.
//! Fifteen orgs still land in a few seconds. Stage 2
//! fetches a repository's individual resources, and only when the user opens
//! it. Paying only for what you look at is what keeps manual navigation
//! viable across a hundred repositories.

use crate::api::releases::ReleaseAsset;
use crate::api::{Client, artifacts, caches, packages, prs, refs, releases, repos, runs};
use crate::model::{OrgSummary, Resource, ResourceKind};
use crate::packages::{PackageVersion, VersionClass, classify};
use crate::refs::{BranchClass, BranchRef, classify_branch};
use crate::stale::is_stale;
use anyhow::Result;
use std::collections::HashSet;
use tokio::sync::mpsc::UnboundedSender;

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
        // here take their real visibility, age and archiving class from
        // `repos::list`; a repo that appears only in the cache report — never
        // in the repo listing — keeps the defaults the cache report gave it
        // (`private: false`, `age_days: 0`, `class: NoAdminRights`).
        let mut repos_out = summaries;
        for repo in refs {
            // `repos` here is `api::repos` (this file's own `use` alias);
            // `classify_repo` lives in the top-level `crate::repos` module —
            // same name, different module, hence the full path.
            let class = crate::repos::classify_repo(repo.archived, repo.admin);
            if let Some(existing) = repos_out.iter_mut().find(|r| r.name == repo.name) {
                existing.private = repo.private;
                existing.age_days = repo.age_days;
                existing.class = class;
            } else {
                repos_out.push(crate::model::RepoSummary {
                    name: repo.name,
                    cache_bytes: 0,
                    cache_count: 0,
                    private: repo.private,
                    age_days: repo.age_days,
                    class,
                });
            }
        }
        repos_out.sort_by_key(|r| std::cmp::Reverse(r.cache_bytes));

        // The degradable stage-1 reads, joined: none depends on another, and
        // none is `?`-propagated — an org whose billing, plan, budgets or
        // retention is refused is still worth showing.
        let (billing, plan, budgets, retention) = futures::join!(
            crate::api::billing::fetch(client, org),
            crate::api::orgs::plan(client, org),
            crate::api::budgets::fetch(client, org),
            crate::api::retention::fetch(client, org),
        );

        Some(OrgSummary {
            login: org.clone(),
            cache_bytes,
            cache_count,
            repos: repos_out,
            billing,
            plan,
            budgets,
            retention,
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
///
/// A thin wrapper over `repo_detail_with_warnings`: prints one line per
/// failed family to stderr (Debt 4 of the v0.4 final review — see that
/// function's own doc comment) and returns just the items, for the many
/// existing callers that only ever wanted those.
pub async fn repo_detail(client: &Client, owner: &str, repo: &str) -> Result<Vec<Resource>> {
    let (items, failed) = repo_detail_with_warnings(client, owner, repo).await?;
    for family in failed {
        eprintln!("Avertissement : le listing des {family} a échoué et est ignoré.");
    }
    Ok(items)
}

/// Stage 2, plus the name of every one of the seven resource families whose
/// listing failed.
///
/// Every one of the seven listings this joins degrades independently on its
/// own failure — a token missing one scope must not break a cleanup that
/// never needed it. See the `take` calls below. That degradation is
/// deliberate and unchanged by this function; what it adds is Debt 4 of the
/// v0.4 final review: a refused listing used to contribute zero rows with no
/// signal at all, indistinguishable from a repository genuinely holding none
/// of that family — so a headless `clean` that hit a permission wall read
/// "Rien à supprimer." with nothing to say why. The failed names are
/// returned as data, not printed directly here, so they can be asserted on
/// without capturing process stderr; `repo_detail` above is the thin
/// stderr-printing wrapper most callers want.
///
/// Only the seven `ResourceKind` families are tracked — `closed_r` (the PR
/// listing) and `default_branch_r` feed classification, not rows of their
/// own, and are not named in Finding 4's "sept familles."
pub async fn repo_detail_with_warnings(
    client: &Client,
    owner: &str,
    repo: &str,
) -> Result<(Vec<Resource>, Vec<&'static str>)> {
    detail(client, owner, repo, None).await
}

/// `repo_detail_with_warnings`, plus one tick per completed call.
///
/// The nine listings are joined, so without this the caller sees nothing
/// between "started" and "all nine done" — a bar over that has two states
/// and is worth less than the `(chargement…)` text it would replace.
/// Each future sends its tick as it lands; ticks arrive in completion
/// order, which is the order the user is actually waiting on.
pub async fn repo_detail_ticking(
    client: &Client,
    owner: &str,
    repo: &str,
    tick: UnboundedSender<()>,
) -> Result<(Vec<Resource>, Vec<&'static str>)> {
    detail(client, owner, repo, Some(tick)).await
}

/// How many calls a repository's drill-down joins — the futures of
/// `detail`'s `futures::join!`, right below — and so how many ticks
/// `repo_detail_ticking` sends: the denominator of the TUI's load bar.
///
/// It must follow the number of futures joined there. A tenth call joined
/// without raising it would push the bar past its end, and a tenth family's
/// call left unticked would leave the bar full while that call still runs.
/// `repo_detail_ticking_ticks_exactly_total_calls_times` anchors the two.
pub const TOTAL_CALLS: usize = 9;

/// The one code path behind `repo_detail_with_warnings` and
/// `repo_detail_ticking`: the nine calls, joined, then every family's rows
/// and their safety classification. `tick`, when given, gets one `()` per
/// call as that call lands (`ticked`); dropped when this returns, which
/// closes its channel.
async fn detail(
    client: &Client,
    owner: &str,
    repo: &str,
    tick: Option<UnboundedSender<()>>,
) -> Result<(Vec<Resource>, Vec<&'static str>)> {
    let tick = tick.as_ref();
    // `TOTAL_CALLS`, just above, counts these futures: one more or one
    // fewer here means changing it too.
    let (
        caches_r,
        artifacts_r,
        runs_r,
        versions_r,
        closed_r,
        branches_r,
        tags_r,
        assets_r,
        default_branch_r,
    ) = futures::join!(
        ticked(caches::list(client, owner, repo), tick),
        ticked(artifacts::list(client, owner, repo), tick),
        ticked(runs::list(client, owner, repo), tick),
        // This account's convention: a repo's image, when it publishes one,
        // is named after the repo. A repo with no image 404s — `versions`
        // already turns that into an empty list, not an error.
        ticked(packages::versions(client, owner, repo), tick),
        ticked(prs::closed_prs(client, owner, repo), tick),
        ticked(refs::branches(client, owner, repo), tick),
        ticked(refs::tags(client, owner, repo), tick),
        ticked(releases::assets(client, owner, repo), tick),
        ticked(refs::default_branch(client, owner, repo), tick),
    );

    let mut failed: Vec<&'static str> = Vec::new();

    // Every one of the seven listings degrades the same way: a token
    // missing one scope, or one family's transient outage, costs only that
    // family's rows, never the rest of the drill-down. Caches, artifacts and
    // workflow runs used to `?`-propagate here instead — a 403 on any one of
    // them failed the whole function, branches/tags/release assets
    // included, even though none of those three needed the scope that
    // failed.
    let mut items = take(caches_r, "caches", &mut failed);
    items.extend(take(artifacts_r, "artifacts", &mut failed));
    items.extend(take(runs_r, "workflow runs", &mut failed));
    // A failed packages listing — a token without `read:packages`, or a
    // GHCR outage — costs the package rows, not the rest of the drill-down:
    // caches, artifacts and workflow runs are a different family, and the
    // user may not even have asked about packages. Same pattern as the PR,
    // branch, tag and release-asset listings below: each family's failure
    // costs only that family's rows.
    items.extend(version_resources(take(
        versions_r,
        "versions de packages",
        &mut failed,
    )));

    // A failed PR listing costs the ⚑ flag and the dead-branch
    // classification, not the listing: everything still shows, just with
    // every branch reading as merely "not known to be dead". Not one of the
    // seven families — see this function's own doc comment.
    let closed = closed_r.unwrap_or_default();
    // A failed default-branch fetch degrades to "" — no real branch is ever
    // named that, so `branch_is_dead`'s default-branch exclusion simply
    // never fires. The branch is still safe for *bulk* selection: it cannot
    // be offered unless it is genuinely a merged PR's head ref, which a real
    // default branch essentially never is (PRs merge *into* it, not from
    // it). For the TUI's finer-grained *individual*-selection guard,
    // `classify_branch` itself (Debt 3 of the v0.4 final review) errs toward
    // `BranchClass::Protected` rather than `Live` for an unmatched branch
    // whenever `default_branch` is empty — see its own doc comment, and
    // `api::refs::default_branch`'s. Also not one of the seven families.
    let default_branch = default_branch_r.unwrap_or_default();
    // A failed listing degrades to `BranchListing::default()`: no branch,
    // and not complete.
    let listing = take(branches_r, "branches", &mut failed);
    // Read off the listing before its branches are moved into
    // `branch_resources` below: the safety classification (this function's
    // tail) also needs the set of branches that still exist, and this is the
    // one place that can supply it without a second request. Kept only when
    // the listing is whole — a failed or truncated one cannot say a branch is
    // gone (final review I1).
    let live_branches: Option<HashSet<String>> = listing
        .complete
        .then(|| listing.branches.iter().map(|b| b.name.clone()).collect());
    items.extend(branch_resources(
        listing.branches,
        &default_branch,
        &closed.merged_refs,
    ));
    items.extend(tag_resources(take(tags_r, "tags", &mut failed)));
    let assets = take(assets_r, "assets de releases", &mut failed);
    // Same reasoning as `live_branches` above: read before the move.
    let release_tags = distinct_release_tags(&assets);
    items.extend(asset_resources(assets));

    mark_stale(&mut items, &closed.numbers);

    // The safety classification, in the single code path that assembles
    // every family's listing — a `RepoContext` built entirely from data this
    // function already holds, at zero extra HTTP cost. This must run after
    // `mark_stale`: `classify`'s cache and workflow-run rules both read
    // `stale_pr`. `repo_detail_ticking` goes through this same path, not a
    // copy of it.
    let ctx = crate::safety::RepoContext {
        merged_refs: closed.merged_refs,
        live_branches,
        default_branch,
        release_tags,
    };
    for item in items.iter_mut() {
        item.safety = crate::safety::classify(item, &ctx);
    }

    // Biggest first, with the families GitHub reports no size for after all
    // of them rather than ranked among them on the `0` placeholder they
    // carry (#51) — the same rule, and the same key, as the TUI's own
    // `tui::app::App::visible_resources`, whose doc comment argues it. This
    // order is what the headless `clean` dry-run lists and what `scan
    // --json` emits, so the two screens rank a repository the same way.
    // `sort_by_key` is stable, so each group keeps the order the families
    // were assembled in above.
    items.sort_by_key(crate::clean::size_rank);
    Ok((items, failed))
}

/// `call`, then one tick on `tick` as it lands — whatever it returned: a
/// refused listing is a call done all the same.
///
/// The send's error is dropped on purpose: a closed channel means the user
/// has left the screen, and a tick nobody reads must not fail a load that
/// succeeded.
async fn ticked<F: Future>(call: F, tick: Option<&UnboundedSender<()>>) -> F::Output {
    let landed = call.await;
    if let Some(tick) = tick {
        let _ = tick.send(());
    }
    landed
}

/// Distinct release tags, newest first, in the order the releases API
/// returned them — built from the already-fetched asset listing instead of a
/// second call, since `releases::assets` flattens each release's tag onto
/// every one of its assets. A release with zero assets contributes no tag,
/// but nothing here ever asks about such a release: `classify_asset` only
/// ever looks up the tag an actual asset's own label carries.
fn distinct_release_tags(assets: &[ReleaseAsset]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in assets {
        if !out.contains(&a.release_tag) {
            out.push(a.release_tag.clone());
        }
    }
    out
}

/// Unwrap a family listing's result to its default on failure, recording its
/// name in `failed` — the data half of Debt 4's fix (see
/// `repo_detail_with_warnings`'s doc comment). Does not change the
/// degradation itself, only makes it visible.
fn take<T: Default>(result: Result<T>, family: &'static str, failed: &mut Vec<&'static str>) -> T {
    match result {
        Ok(v) => v,
        Err(_) => {
            failed.push(family);
            T::default()
        }
    }
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
            // Only a `Branch` row carries a classification.
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        })
        .collect()
}

/// A version's row label, carrying the reason it is offered so the user does
/// not have to trust the classification blindly.
///
/// `pub(crate)` rather than private so tests elsewhere (the resource-row
/// render test in `tui::views::repo`) can build a fixture by calling the
/// real function instead of hand-typing a label string that happens to look
/// like its output — the whole point of that test is to exercise this path.
///
/// A genuinely tagged version is identified by its real tag(s) — `latest`,
/// `2.0.2` — since that is what a human recognises on sight. Everything else
/// (untagged, orphaned attestation) is identified by its digest instead: an
/// orphaned attestation's own "tag" is the `sha256-<digest>` it signs, an
/// implementation detail nobody reading the row needs to see.
pub(crate) fn version_label(v: &PackageVersion, class: VersionClass) -> String {
    let ident = if class == VersionClass::Tagged {
        v.tags.join(", ")
    } else {
        elide_digest(&v.digest)
    };
    match class {
        VersionClass::Untagged => format!("{ident} (sans tag)"),
        VersionClass::OrphanedAttestation => format!("{ident} (attestation orpheline)"),
        // Never preselected — deleting a real tag like `latest` breaks
        // deployments — so the label carries no extra warning of its own.
        VersionClass::Tagged => ident,
    }
}

/// `sha256:<64 hex>` down to `sha256:<9 hex>…`.
///
/// The full 71-character digest consumes the whole row at 80 columns and
/// clips the `—` size marker and the class suffix (`(sans tag)` /
/// `(attestation orpheline)`) off entirely — the two offered classes then
/// look identical, since neither suffix is on screen. Nine hex characters is
/// the length the design doc's own mockup (§6) uses.
fn elide_digest(digest: &str) -> String {
    match digest.split_once(':') {
        Some((prefix, hex)) if hex.len() > 9 => format!("{prefix}:{}…", &hex[..9]),
        _ => digest.to_string(),
    }
}

/// Turn every branch into a drill-down row.
///
/// A branch backing a merged PR (`BranchClass::Merged`) is offered for bulk
/// deletion; every other branch — the default, one GitHub itself marks
/// `protected`, or simply one with no merged PR behind it — is shown but
/// never bulk-selectable. `protected` stays the only mechanism
/// `commands::clean::select` and the TUI's bulk shortcuts need: no change
/// there. `branch_class` carries the finer distinction `protected` alone
/// collapses away — which of the three non-merged cases actually applies —
/// for the row's own label and for the TUI's individual-selection guard
/// (`tui::app::App::toggle_selected`), neither of which `protected` alone
/// can drive correctly.
fn branch_resources(
    branches: Vec<BranchRef>,
    default_branch: &str,
    merged_refs: &HashSet<String>,
) -> Vec<Resource> {
    branches
        .into_iter()
        .map(|b| {
            let class = classify_branch(&b, default_branch, merged_refs);
            Resource {
                kind: ResourceKind::Branch,
                id: refs::resource_id(&b.name),
                label: b.name,
                // GitHub exposes no size for a branch, under any name.
                size_bytes: 0,
                // Neither the branch listing nor the closed-PR listing
                // carries a per-branch timestamp.
                age_days: 0,
                // No pull-request ref to carry: `mark_stale` must leave
                // `stale_pr` at its default `false` for every branch, dead
                // or not, exactly as the task 4 conversion table requires.
                git_ref: None,
                stale_pr: false,
                protected: class != BranchClass::Merged,
                branch_class: Some(class),
                safety: crate::safety::Safety::Keep,
            }
        })
        .collect()
}

/// Turn every tag name into a drill-down row.
///
/// A tag is always `protected`: it is what a release, a `go get`, or a
/// `Cargo.toml` points at by name, and unlike a branch there is no "dead"
/// classification for one — it is never offered for bulk deletion.
fn tag_resources(tags: Vec<String>) -> Vec<Resource> {
    tags.into_iter()
        .map(|name| Resource {
            kind: ResourceKind::Tag,
            id: refs::resource_id(&name),
            label: name,
            size_bytes: 0,
            age_days: 0,
            git_ref: None,
            stale_pr: false,
            protected: true,
            // Only a `Branch` row carries a classification.
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        })
        .collect()
}

/// Turn every release asset into a drill-down row.
///
/// Unlike a branch or a tag, an asset carries a real numeric id from GitHub
/// — no hashing needed — and a real size: this is the one v0.4 family
/// actually measured in bytes (7.3 Go across four orgs). The release itself
/// never becomes a row of its own (see `api::releases`'s doc comment); its
/// tag is folded into the label instead, since nothing else here carries it
/// forward.
fn asset_resources(assets: Vec<ReleaseAsset>) -> Vec<Resource> {
    assets
        .into_iter()
        .map(|a| Resource {
            kind: ResourceKind::ReleaseAsset,
            id: a.id,
            label: format!("{} ({})", a.name, a.release_tag),
            size_bytes: a.size,
            age_days: a.age_days,
            git_ref: None,
            stale_pr: false,
            protected: false,
            // Only a `Branch` row carries a classification.
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        })
        .collect()
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
            branch_class: None,
            safety: crate::safety::Safety::Keep,
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

    /// The full 71-character digest consumes the whole row at 80 columns and
    /// clips the size marker and class suffix off — see
    /// `tui::views::repo::a_package_row_survives_at_eighty_columns` for the
    /// render-level proof. This locks the elision itself, calling the real
    /// `version_label` rather than a fixture that hardcodes the already-cut
    /// string production never produces on its own.
    #[test]
    fn version_label_elides_a_long_digest_to_nine_hex_characters() {
        let v = PackageVersion {
            id: 1,
            digest: "sha256:9a26c70801010123223adb5e73ff703aca86c15e19b30124ede5628a1e185826"
                .into(),
            tags: vec![],
            age_days: 5,
        };
        let label = version_label(&v, VersionClass::Untagged);
        assert_eq!(label, "sha256:9a26c7080… (sans tag)", "got: {label}");
    }

    /// An orphaned attestation's "tag" is the `sha256-<digest>` it signs —
    /// an implementation detail, not something a human should have to read
    /// in full. It must be elided exactly like a plain digest, from the
    /// version's own digest, not left at the full 71 characters.
    #[test]
    fn version_label_elides_an_orphaned_attestations_digest_too() {
        let v = PackageVersion {
            id: 1,
            digest: "sha256:1d7018e5672547cced06883706367832e5f1be5fa90bc2038ad308e19958e80e"
                .into(),
            tags: vec![
                "sha256-1a65eb30f0e36fc41bb07724b11e53ada5e810382f39143698b00c470f019b80".into(),
            ],
            age_days: 5,
        };
        let label = version_label(&v, VersionClass::OrphanedAttestation);
        assert_eq!(
            label, "sha256:1d7018e56… (attestation orpheline)",
            "got: {label}"
        );
    }

    /// A tagged version is identified by its real tag(s), not a digest, and
    /// those must survive untouched — `latest` elided to nine characters
    /// would just be `latest` mangled for no reason, since it is not a hex
    /// digest to begin with.
    #[test]
    fn version_label_leaves_real_tags_untouched() {
        let v = PackageVersion {
            id: 1,
            digest: "sha256:deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                .into(),
            tags: vec!["latest".into(), "2.0.2".into()],
            age_days: 5,
        };
        assert_eq!(version_label(&v, VersionClass::Tagged), "latest, 2.0.2");
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
        // End-to-end through the real JSON deserialization path: the digest
        // must arrive elided, not at its full 71 characters.
        assert_eq!(items[0].label, "sha256:9a26c7080… (sans tag)");
    }

    /// A token without `read:packages`, or a GHCR outage, must not break the
    /// rest of the drill-down: caches, artifacts and workflow runs are a
    /// different family, and the user may not even have asked about
    /// packages. Same pattern as the failed-PR-listing degradation right
    /// below it in `repo_detail` (`prs::closed_prs`'s
    /// `.unwrap_or_default()` at the same call site).
    #[tokio::test]
    async fn a_failed_packages_listing_costs_only_the_package_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/repolens/actions/caches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "actions_caches": [
                    { "id": 1, "key": "coverage-linux", "size_in_bytes": 1000,
                      "last_accessed_at": "2026-06-01T00:00:00Z" }
                ]
            })))
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
        // No `read:packages` scope, or GHCR down — either way, not a 404
        // ("no image published"), so this must not silently mean "empty".
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/repolens/versions"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "repolens")
            .await
            .expect("a failed packages listing must not fail the whole drill-down");

        assert_eq!(items.len(), 1, "the cache must still show");
        assert_eq!(items[0].kind, ResourceKind::Cache);
        assert!(
            items.iter().all(|i| i.kind != ResourceKind::PackageVersion),
            "no package row should appear when the listing failed"
        );
    }

    fn branch(name: &str, protected: bool) -> BranchRef {
        BranchRef {
            name: name.to_string(),
            protected,
        }
    }

    /// Exercises every `branch_is_dead` exclusion at once — the default
    /// branch, a GitHub-protected one, a branch with no merged PR behind it,
    /// and a genuinely dead one — so a wrong `protected: dead` (inverted)
    /// implementation, or one that always returns `true`/`false`, fails
    /// obviously rather than by accident on a single-branch fixture.
    #[test]
    fn branch_resources_marks_only_a_dead_branch_unprotected() {
        let branches = vec![
            branch("main", false),
            branch("release/2.0", true),
            branch("claude/landing-3jbqk4", false),
            branch("feature/rejected", false),
        ];
        let merged = HashSet::from(["claude/landing-3jbqk4".to_string()]);

        let items = branch_resources(branches, "main", &merged);

        let find = |name: &str| items.iter().find(|r| r.label == name).unwrap();
        assert!(find("main").protected, "the default branch stays protected");
        assert!(
            find("release/2.0").protected,
            "a GitHub-protected branch stays protected"
        );
        assert!(
            !find("claude/landing-3jbqk4").protected,
            "a dead branch must be bulk-selectable"
        );
        assert!(
            find("feature/rejected").protected,
            "a branch with no merged PR is alive, not offered"
        );

        // `protected` alone cannot tell these four apart — `branch_class`
        // must. A wrong wiring that always set `Some(BranchClass::Live)` (or
        // always `None`) would still pass every `protected` assertion above.
        assert_eq!(find("main").branch_class, Some(BranchClass::Default));
        assert_eq!(
            find("release/2.0").branch_class,
            Some(BranchClass::Protected)
        );
        assert_eq!(
            find("claude/landing-3jbqk4").branch_class,
            Some(BranchClass::Merged)
        );
        assert_eq!(
            find("feature/rejected").branch_class,
            Some(BranchClass::Live)
        );

        for item in &items {
            assert_eq!(item.kind, ResourceKind::Branch);
            assert_eq!(item.size_bytes, 0);
            assert!(!item.stale_pr);
            assert!(item.git_ref.is_none());
        }
        // The id must actually come from the shared hash, not e.g. a
        // per-call index that would happen to look plausible here too.
        assert_eq!(find("main").id, crate::api::refs::resource_id("main"));
    }

    #[test]
    fn tag_resources_are_always_protected_and_zero_sized() {
        let items = tag_resources(vec!["v0.1.3".to_string(), "v0.1.2".to_string()]);

        assert_eq!(items.len(), 2);
        for item in &items {
            assert_eq!(item.kind, ResourceKind::Tag);
            assert!(item.protected, "a tag must never be bulk-selectable");
            assert_eq!(item.size_bytes, 0);
            assert!(!item.stale_pr);
        }
        assert_eq!(
            items[0].id,
            crate::api::refs::resource_id("v0.1.3"),
            "the id must come from the shared name hash"
        );
    }

    #[test]
    fn asset_resources_carry_their_size_and_are_never_protected() {
        let assets = vec![ReleaseAsset {
            id: 1,
            name: "terminus-linux-x86_64.tar.gz".into(),
            size: 2_400_000,
            release_tag: "v0.1.1".into(),
            age_days: 10,
        }];

        let items = asset_resources(assets);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ResourceKind::ReleaseAsset);
        assert_eq!(items[0].id, 1, "a release asset keeps its real GitHub id");
        assert_eq!(items[0].size_bytes, 2_400_000);
        assert_eq!(items[0].age_days, 10);
        assert!(!items[0].protected);
        assert!(items[0].label.contains("terminus-linux-x86_64.tar.gz"));
        assert!(
            items[0].label.contains("v0.1.1"),
            "the release tag must survive somewhere, since the release itself \
             never becomes its own row: got {:?}",
            items[0].label
        );
    }

    /// End-to-end through `repo_detail`'s real `futures::join!` wiring, not
    /// just the conversion helpers in isolation: a merged PR makes one
    /// branch dead, GitHub marks a second branch protected directly, the
    /// default-branch fetch names a third, and a tag and a release asset
    /// round out the fixture — one call exercising every new-in-v0.4 row at
    /// once, the way `SecondBrain-io/claudine` actually looks.
    #[tokio::test]
    async fn repo_detail_folds_in_branches_tags_and_release_assets() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "actions_caches": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "number": 31, "state": "closed", "merged_at": "2026-07-24T13:33:32Z",
                  "head": { "ref": "claude/landing-3jbqk4" } }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "main", "protected": false },
                { "name": "release/2.0", "protected": true },
                { "name": "claude/landing-3jbqk4", "protected": false }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "v0.1.3" }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "tag_name": "v0.1.1",
                  "assets": [
                      { "id": 9, "name": "claudine-linux-x86_64.tar.gz",
                        "size": 2_400_000_u64, "created_at": "2026-06-01T00:00:00Z" }
                  ] }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "claudine",
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "claudine").await.unwrap();

        let branches: Vec<&Resource> = items
            .iter()
            .filter(|r| r.kind == ResourceKind::Branch)
            .collect();
        assert_eq!(branches.len(), 3);
        let branch = |name: &str| branches.iter().find(|r| r.label == name).unwrap();
        assert!(branch("main").protected, "the default branch is protected");
        assert!(
            branch("release/2.0").protected,
            "the GitHub-protected branch is protected"
        );
        assert!(
            !branch("claude/landing-3jbqk4").protected,
            "the merged branch is bulk-selectable"
        );

        let tags: Vec<&Resource> = items
            .iter()
            .filter(|r| r.kind == ResourceKind::Tag)
            .collect();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].label, "v0.1.3");
        assert!(tags[0].protected);

        let assets: Vec<&Resource> = items
            .iter()
            .filter(|r| r.kind == ResourceKind::ReleaseAsset)
            .collect();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].id, 9);
        assert_eq!(assets[0].size_bytes, 2_400_000);
        assert!(!assets[0].protected);
    }

    /// #51, on the order `repo_detail` hands to the headless dry-run
    /// listing and to `scan --json`: the families GitHub reports no size for
    /// used to rank on the `0` placeholder they carry, which filed them as
    /// the lightest rows in the repository.
    ///
    /// The empty release asset is what makes this assertion discriminate.
    /// Every sizeless row ties at zero with it, so under the old key the
    /// tie broke on the order the families are assembled in above — runs
    /// third, assets last — and the asset came out *below* the run. Its
    /// size is measured, and measured at zero; the run's is not measured at
    /// all. A fixture without it would sort identically before and after
    /// this fix, since caches happen to be assembled first.
    #[tokio::test]
    async fn repo_detail_ranks_measured_resources_before_the_ones_github_cannot_size() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/tri/actions/caches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "actions_caches": [
                    { "id": 1, "key": "coverage-linux", "size_in_bytes": 1000,
                      "last_accessed_at": "2026-06-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/tri/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/tri/actions/runs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "workflow_runs": [
                    { "id": 128, "name": "CI", "run_number": 128,
                      "created_at": "2026-06-01T00:00:00Z", "head_branch": "main" }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/tri/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/tri/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/tri/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/tri/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "v0.1.3" }
            ])))
            .mount(&server)
            .await;
        // A release asset of zero bytes: rare, but a real measurement —
        // GitHub reports `size` for an asset, and here it says zero.
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/tri/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "tag_name": "v0.1.1",
                  "assets": [
                      { "id": 9, "name": "empty.tar.gz",
                        "size": 0, "created_at": "2026-06-01T00:00:00Z" }
                  ] }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/tri"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "tri").await.unwrap();

        let order: Vec<(ResourceKind, u64)> = items.iter().map(|r| (r.kind, r.id)).collect();
        assert_eq!(
            order,
            vec![
                (ResourceKind::Cache, 1),
                (ResourceKind::ReleaseAsset, 9),
                (ResourceKind::WorkflowRun, 128),
                (ResourceKind::Tag, crate::api::refs::resource_id("v0.1.3")),
            ],
            "measured rows rank first, biggest first; the unmeasured ones follow"
        );
    }

    /// A failed branches listing must cost only the branch rows — same
    /// degradation pattern as the failed packages listing above.
    #[tokio::test]
    async fn a_failed_branches_listing_costs_only_the_branch_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "actions_caches": [
                    { "id": 1, "key": "coverage-linux", "size_in_bytes": 1000,
                      "last_accessed_at": "2026-06-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        // No `repo` scope, or an outage — either way, not the ordinary case.
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "claudine")
            .await
            .expect("a failed branches listing must not fail the whole drill-down");

        assert_eq!(items.len(), 1, "the cache must still show");
        assert_eq!(items[0].kind, ResourceKind::Cache);
        assert!(items.iter().all(|i| i.kind != ResourceKind::Branch));
    }

    /// A failed tags listing must cost only the tag rows.
    #[tokio::test]
    async fn a_failed_tags_listing_costs_only_the_tag_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "actions_caches": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "main", "protected": true }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "claudine")
            .await
            .expect("a failed tags listing must not fail the whole drill-down");

        assert_eq!(items.len(), 1, "the branch must still show");
        assert_eq!(items[0].kind, ResourceKind::Branch);
        assert!(items.iter().all(|i| i.kind != ResourceKind::Tag));
    }

    /// Finding 5 of the v0.4 final review: `caches_r`, `artifacts_r` and
    /// `runs_r` were `?`-propagated instead of degraded like the other four
    /// listings, so a failure in any one of them — a token missing one
    /// scope, a transient Actions outage — failed the *entire* drill-down,
    /// branches/tags/release assets included, even though none of those
    /// three needed the scope that failed. Same degradation pattern as
    /// `a_failed_tags_listing_costs_only_the_tag_rows` above, mirrored for
    /// caches.
    #[tokio::test]
    async fn a_failed_caches_listing_costs_only_the_cache_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "main", "protected": true }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "claudine")
            .await
            .expect("a failed caches listing must not fail the whole drill-down");

        assert_eq!(items.len(), 1, "the branch must still show");
        assert_eq!(items[0].kind, ResourceKind::Branch);
        assert!(items.iter().all(|i| i.kind != ResourceKind::Cache));
    }

    /// Same as above, for artifacts.
    #[tokio::test]
    async fn a_failed_artifacts_listing_costs_only_the_artifact_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "actions_caches": [
                    { "id": 1, "key": "coverage-linux", "size_in_bytes": 1000,
                      "last_accessed_at": "2026-06-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "claudine")
            .await
            .expect("a failed artifacts listing must not fail the whole drill-down");

        assert_eq!(items.len(), 1, "the cache must still show");
        assert_eq!(items[0].kind, ResourceKind::Cache);
        assert!(items.iter().all(|i| i.kind != ResourceKind::Artifact));
    }

    /// Same as above, for workflow runs.
    #[tokio::test]
    async fn a_failed_runs_listing_costs_only_the_run_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "actions_caches": [
                    { "id": 1, "key": "coverage-linux", "size_in_bytes": 1000,
                      "last_accessed_at": "2026-06-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "claudine")
            .await
            .expect("a failed runs listing must not fail the whole drill-down");

        assert_eq!(items.len(), 1, "the cache must still show");
        assert_eq!(items[0].kind, ResourceKind::Cache);
        assert!(items.iter().all(|i| i.kind != ResourceKind::WorkflowRun));
    }

    /// A failed release-assets listing must cost only the asset rows — the
    /// brief's own example: "a token that cannot read releases must not
    /// break a cache cleanup."
    #[tokio::test]
    async fn a_failed_release_assets_listing_costs_only_the_asset_rows() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "actions_caches": [
                    { "id": 1, "key": "coverage-linux", "size_in_bytes": 1000,
                      "last_accessed_at": "2026-06-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "claudine")
            .await
            .expect("a failed release-assets listing must not fail the whole drill-down");

        assert_eq!(items.len(), 1, "the cache must still show");
        assert_eq!(items[0].kind, ResourceKind::Cache);
        assert!(items.iter().all(|i| i.kind != ResourceKind::ReleaseAsset));
    }

    /// A failed default-branch fetch must degrade to "" rather than fail the
    /// drill-down — and a branch that is merely not merged (like "main" here)
    /// must still come through protected, because it was never in
    /// `merged_refs` to begin with. This is the fail-safe case
    /// `refs::default_branch`'s own doc comment describes.
    #[tokio::test]
    async fn a_failed_default_branch_fetch_still_leaves_an_unmerged_branch_protected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "actions_caches": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "main", "protected": false }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        // The repo-info fetch itself fails.
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "claudine")
            .await
            .expect("a failed default-branch fetch must not fail the whole drill-down");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, ResourceKind::Branch);
        assert!(
            items[0].protected,
            "main was never merged, so it must stay protected even without a default-branch name"
        );
        // Debt 3 of the v0.4 final review: `protected` alone was already
        // `true` here before the fix too (`Live` and `Protected` both set
        // it) — it is `branch_class` that the TUI's individual-selection
        // guard actually reads (`tui::app::App::toggle_selected`), and that
        // guard only refuses `Default` and `Protected`. Before the fix this
        // branch classified `Live` — merely unmerged, still individually
        // selectable — precisely when it might in fact *be* the default
        // branch the failed fetch could not name.
        assert_eq!(
            items[0].branch_class,
            Some(BranchClass::Protected),
            "an unknown default-branch name must classify an unmatched branch toward Protected, \
             not Live, so the TUI's individual-selection guard still covers it"
        );
    }

    /// Task 2: `repo_detail` must compute `Resource.safety` itself, from the
    /// same nine listings it already fetches — zero extra requests. Two
    /// caches distinguish the wiring from an implementation that would mark
    /// every resource the same level: one sits on a merged pull request's
    /// branch (`Safe`, via `merged_refs`), the other on the default branch
    /// (`Keep`, via `default_branch`). A fixture with only one of the two
    /// could not tell a real classifier from one that always answers with
    /// that one level.
    #[tokio::test]
    async fn repo_detail_marks_a_cache_on_a_merged_branch_safe_and_one_on_main_kept() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/surete/actions/caches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "actions_caches": [
                    { "id": 1, "key": "cache-merged", "ref": "refs/heads/claude/landing-3jbqk4",
                      "size_in_bytes": 1000, "last_accessed_at": "2026-06-01T00:00:00Z" },
                    { "id": 2, "key": "cache-main", "ref": "refs/heads/main",
                      "size_in_bytes": 1000, "last_accessed_at": "2026-06-01T00:00:00Z" }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/surete/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/surete/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/surete/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/surete/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "number": 31, "state": "closed", "merged_at": "2026-07-24T13:33:32Z",
                  "head": { "ref": "claude/landing-3jbqk4" } }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/surete/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "main", "protected": true }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/surete/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/surete/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/surete"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let items = repo_detail(&client, "systm-d", "surete").await.unwrap();

        let caches: Vec<&Resource> = items
            .iter()
            .filter(|r| r.kind == ResourceKind::Cache)
            .collect();
        assert_eq!(caches.len(), 2);
        let merged = caches.iter().find(|r| r.id == 1).unwrap();
        let on_main = caches.iter().find(|r| r.id == 2).unwrap();
        assert_eq!(
            merged.safety,
            crate::safety::Safety::Safe,
            "a cache on a merged pull request's branch must be safe"
        );
        assert_eq!(
            on_main.safety,
            crate::safety::Safety::Keep,
            "a cache on the default branch must be kept"
        );
    }

    /// `systm-d/{repo}` holding four caches, one per way `safety::classify`
    /// can judge a cache, with its branches listing and its default-branch
    /// lookup answering `branches` and `default_branch`:
    ///
    /// - id 1, on closed pull request 54's merge ref: safe through
    ///   `stale_pr`, a fact the closed-PR listing proves on its own;
    /// - id 2, on `claude/landing`, the head of merged pull request 31: safe
    ///   through `merged_refs`, same listing;
    /// - id 3, on `wip`, a live branch no pull request merged;
    /// - id 4, on `main`, the default branch.
    ///
    /// The last two are the caches only the branches listing and the
    /// default-branch lookup can tell apart from a vanished branch's.
    async fn mount_four_caches(
        server: &MockServer,
        repo: &str,
        branches: ResponseTemplate,
        default_branch: ResponseTemplate,
    ) {
        let cache = |id: u64, git_ref: &str| {
            serde_json::json!({
                "id": id, "key": format!("cache-{id}"), "ref": git_ref,
                "size_in_bytes": 1000 * id, "last_accessed_at": "2026-06-01T00:00:00Z"
            })
        };
        let empty = |body: serde_json::Value| ResponseTemplate::new(200).set_body_json(body);
        let routes = [
            (
                format!("/repos/systm-d/{repo}/actions/caches"),
                empty(serde_json::json!({ "actions_caches": [
                    cache(1, "refs/pull/54/merge"),
                    cache(2, "refs/heads/claude/landing"),
                    cache(3, "refs/heads/wip"),
                    cache(4, "refs/heads/main"),
                ] })),
            ),
            (
                format!("/repos/systm-d/{repo}/actions/artifacts"),
                empty(serde_json::json!({ "artifacts": [] })),
            ),
            (
                format!("/repos/systm-d/{repo}/actions/runs"),
                empty(serde_json::json!({ "workflow_runs": [] })),
            ),
            (
                format!("/orgs/systm-d/packages/container/{repo}/versions"),
                ResponseTemplate::new(404),
            ),
            (
                format!("/repos/systm-d/{repo}/pulls"),
                empty(serde_json::json!([
                    { "number": 54, "state": "closed", "merged_at": null,
                      "head": { "ref": "feature/abandoned" } },
                    { "number": 31, "state": "closed", "merged_at": "2026-07-24T13:33:32Z",
                      "head": { "ref": "claude/landing" } }
                ])),
            ),
            (format!("/repos/systm-d/{repo}/branches"), branches),
            (
                format!("/repos/systm-d/{repo}/tags"),
                empty(serde_json::json!([])),
            ),
            (
                format!("/repos/systm-d/{repo}/releases"),
                empty(serde_json::json!([])),
            ),
            (format!("/repos/systm-d/{repo}"), default_branch),
        ];
        for (route, response) in routes {
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(response)
                .mount(server)
                .await;
        }
    }

    /// Each of `mount_four_caches`' caches' safety level, by id, as
    /// `repo_detail` classified it.
    async fn cache_levels(server: &MockServer, repo: &str) -> Vec<(u64, crate::safety::Safety)> {
        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let mut levels: Vec<(u64, crate::safety::Safety)> = repo_detail(&client, "systm-d", repo)
            .await
            .unwrap()
            .into_iter()
            .filter(|r| r.kind == ResourceKind::Cache)
            .map(|r| (r.id, r.safety))
            .collect();
        levels.sort_by_key(|&(id, _)| id);
        levels
    }

    /// Final review I1: a branches listing that failed used to degrade to an
    /// empty set of live branches, and the vanished-branch rule then read
    /// every cache on a live branch as a cache on a branch that no longer
    /// exists — ⛑, taken by `[A]`. Absence of data is not proof of absence:
    /// `wip`'s cache must fall to `Check`. The two caches made safe by the
    /// closed-PR listing alone keep their level, and `main`'s stays kept.
    #[tokio::test]
    async fn safety_never_reads_a_failed_branches_listing_as_vanished_branches() {
        use crate::safety::Safety;
        let server = MockServer::start().await;
        mount_four_caches(
            &server,
            "refusee",
            ResponseTemplate::new(403),
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "default_branch": "main" })),
        )
        .await;

        assert_eq!(
            cache_levels(&server, "refusee").await,
            vec![
                (1, Safety::Safe),
                (2, Safety::Safe),
                (3, Safety::Check),
                (4, Safety::Keep)
            ],
            "(id, level) of the closed PR's, merged head's, live branch's and main's caches"
        );
    }

    /// The other way the branch set is incomplete: `api::refs::branches`
    /// stops after its page cap. Every page here is full, so the listing is
    /// cut there, and `wip` — past the last page read — is absent from what
    /// was read. Its cache must fall to `Check`, not read as a vanished
    /// branch's.
    #[tokio::test]
    async fn safety_never_reads_a_truncated_branches_listing_as_vanished_branches() {
        use crate::safety::Safety;
        let server = MockServer::start().await;
        let full_page: Vec<serde_json::Value> = (1..=100)
            .map(|n| serde_json::json!({ "name": format!("branch-{n}"), "protected": false }))
            .collect();
        mount_four_caches(
            &server,
            "tronquee",
            ResponseTemplate::new(200).set_body_json(full_page),
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "default_branch": "main" })),
        )
        .await;

        assert_eq!(
            cache_levels(&server, "tronquee").await,
            vec![
                (1, Safety::Safe),
                (2, Safety::Safe),
                (3, Safety::Check),
                (4, Safety::Keep)
            ],
            "(id, level) of the closed PR's, merged head's, live branch's and main's caches"
        );
    }

    /// The final review's joint blip: both calls hit `/repos/{o}/{r}`'s
    /// endpoint family. With no default branch and no branch set, `main`'s
    /// cache used to read ⛑ too. Neither `wip`'s nor `main`'s may be `Safe`;
    /// the closed PR's and the merged head's still are.
    #[tokio::test]
    async fn safety_never_marks_the_default_branchs_cache_safe_when_neither_listing_answers() {
        use crate::safety::Safety;
        let server = MockServer::start().await;
        mount_four_caches(
            &server,
            "muette",
            ResponseTemplate::new(403),
            ResponseTemplate::new(403),
        )
        .await;

        assert_eq!(
            cache_levels(&server, "muette").await,
            vec![
                (1, Safety::Safe),
                (2, Safety::Safe),
                (3, Safety::Check),
                (4, Safety::Check)
            ],
            "(id, level) of the closed PR's, merged head's, live branch's and main's caches"
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

    /// The repository is the first candidate that lives in the tree: its row
    /// needs its class and age, for both a repo merged into an existing
    /// cache-report row and one pushed as a new row (`overview` builds
    /// `RepoSummary` two different ways depending on which case a repo falls
    /// into, and both must carry the same real data — the exact split
    /// `overview_carries_repo_visibility_from_the_repo_listing` above locks
    /// for `private`).
    #[tokio::test]
    async fn overview_carries_repo_class_and_age_from_the_repo_listing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/maxds-lyon/actions/cache/usage-by-repository"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "repository_cache_usages": [
                    { "full_name": "maxds-lyon/lokiprint",
                      "active_caches_size_in_bytes": 1000, "active_caches_count": 2 }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/maxds-lyon/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                // Has cache usage above: merged into the existing row.
                // Archived, so it must classify AlreadyArchived even though
                // admin is true.
                { "name": "lokiprint", "private": false, "archived": true,
                  "permissions": { "admin": true },
                  "pushed_at": "2024-06-15T00:00:00Z" },
                // No cache usage: pushed as a new row. Not archived, no
                // admin rights, so it must classify NoAdminRights.
                { "name": ".github", "private": false, "archived": false,
                  "permissions": { "admin": false },
                  "pushed_at": "2024-06-15T00:00:00Z" }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/organizations/maxds-lyon/settings/billing/usage"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(&client, &["maxds-lyon".to_string()]).await;

        let repos = &out[0].repos;
        let find = |name: &str| repos.iter().find(|r| r.name == name).unwrap();

        let merged = find("lokiprint");
        assert_eq!(
            merged.class,
            crate::repos::RepoClass::AlreadyArchived,
            "a merged row must classify from the repo listing's own archived flag"
        );
        assert!(merged.age_days > 300, "got {}", merged.age_days);

        let pushed = find(".github");
        assert_eq!(
            pushed.class,
            crate::repos::RepoClass::NoAdminRights,
            "a pushed row must classify from the repo listing's own permissions"
        );
        assert!(pushed.age_days > 300, "got {}", pushed.age_days);
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

    /// #11: the plan decides whether any percentage can be shown at all, so
    /// `overview` must carry it from `api::orgs::plan` onto the summary.
    #[tokio::test]
    async fn overview_carries_the_orgs_plan() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d/actions/cache/usage-by-repository"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "repository_cache_usages": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d/repos"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/exec-d"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "login": "exec-d",
                "plan": { "name": "team" }
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(&client, &["exec-d".to_string()]).await;

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].plan.as_deref(), Some("team"));
    }

    /// A refused plan costs the plan, never the org — same rule as billing.
    #[tokio::test]
    async fn overview_keeps_an_org_whose_plan_is_refused() {
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
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/SecondBrain-io"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(&client, &["SecondBrain-io".to_string()]).await;

        assert_eq!(out.len(), 1, "a refused plan must not drop the org");
        assert_eq!(out[0].cache_bytes, 1000);
        assert!(out[0].plan.is_none());
    }

    /// #14: budgets ride along at stage 1, and a refusal — observed as a 400
    /// — costs the budgets only, never the org.
    ///
    /// Two organizations, each with its own plan *and* its own budgets
    /// answer: the joined stage-1 reads are only correct if each one is
    /// asked about the org it is paired with. A `join!` that passed a fixed
    /// login to `orgs::plan` — the defect Task 4 could only catch with a
    /// throwaway test — gives both orgs the same plan here, and fails.
    #[tokio::test]
    async fn overview_carries_budgets_and_keeps_the_org_when_refused() {
        let server = MockServer::start().await;
        for (org, plan) in [("exec-d", "team"), ("le-vilain-petit-dev", "free")] {
            Mock::given(method("GET"))
                .and(path(format!(
                    "/orgs/{org}/actions/cache/usage-by-repository"
                )))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "repository_cache_usages": [] })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/orgs/{org}/repos")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/orgs/{org}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "login": org,
                    "plan": { "name": plan }
                })))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/organizations/exec-d/settings/billing/budgets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "budgets": [{
                    "budget_type": "ProductPricing", "budget_product_sku": "actions",
                    "budget_scope": "organization", "budget_amount": 0,
                    "prevent_further_usage": true
                }],
                "has_next_page": false
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/organizations/le-vilain-petit-dev/settings/billing/budgets",
            ))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({ "message": "Unable to get budgets." })),
            )
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(
            &client,
            &["exec-d".to_string(), "le-vilain-petit-dev".to_string()],
        )
        .await;

        let find = |login: &str| out.iter().find(|o| o.login == login).unwrap();
        assert_eq!(out.len(), 2, "a refused budgets read must not drop the org");
        assert_eq!(find("exec-d").budgets.as_ref().map(Vec::len), Some(1));
        assert!(find("le-vilain-petit-dev").budgets.is_none());
        // Each org's plan is its own: the pairing inside the `join!` holds.
        assert_eq!(find("exec-d").plan.as_deref(), Some("team"));
        assert_eq!(find("le-vilain-petit-dev").plan.as_deref(), Some("free"));
    }

    /// #15: retention rides along at stage 1; a token without `admin:org`
    /// costs the retention only.
    #[tokio::test]
    async fn overview_carries_retention() {
        let server = MockServer::start().await;
        for org in ["exec-d", "systm-d"] {
            Mock::given(method("GET"))
                .and(path(format!(
                    "/orgs/{org}/actions/cache/usage-by-repository"
                )))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "repository_cache_usages": [] })),
                )
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path(format!("/orgs/{org}/repos")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path(
                "/orgs/exec-d/actions/permissions/artifact-and-log-retention",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "days": 7 })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/orgs/systm-d/actions/permissions/artifact-and-log-retention",
            ))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = overview(&client, &["exec-d".to_string(), "systm-d".to_string()]).await;

        let find = |login: &str| out.iter().find(|o| o.login == login).unwrap();
        assert_eq!(
            out.len(),
            2,
            "a refused retention read must not drop the org"
        );
        assert_eq!(find("exec-d").retention.map(|r| r.days), Some(7));
        assert!(find("systm-d").retention.is_none());
    }

    /// Debt 4 of the v0.4 final review: all seven family listings degrade
    /// identically on failure — an empty contribution to `items`, exactly
    /// what a repository genuinely holding none of that family also
    /// produces. Headless, a refused listing then reads as "Rien à
    /// supprimer." with no signal that anything was refused at all.
    /// `repo_detail_with_warnings` is the same drill-down, plus which
    /// families' listings failed — kept as returned data, not a direct
    /// `eprintln!`, so the signal can be asserted on here without capturing
    /// process stderr. `repo_detail` (below) is a thin wrapper that prints
    /// each name and returns just the items, unchanged for its existing
    /// callers.
    #[tokio::test]
    async fn repo_detail_with_warnings_names_a_failed_family() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let (items, failed) = repo_detail_with_warnings(&client, "systm-d", "claudine")
            .await
            .expect("a failed caches listing must not fail the whole drill-down");

        assert!(items.is_empty());
        assert_eq!(
            failed,
            vec!["caches"],
            "the failed family must be named, and only that one"
        );
    }

    /// A wrong implementation that stops at the first failure (or only ever
    /// reports one) would still pass the single-family test above — this
    /// fails two families that are not adjacent in `repo_detail`'s own
    /// `futures::join!` order (caches is first, tags is second-to-last), so
    /// only a version that checks every listing independently names both.
    #[tokio::test]
    async fn repo_detail_with_warnings_names_every_failed_family_not_just_the_first() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let (_, failed) = repo_detail_with_warnings(&client, "systm-d", "claudine")
            .await
            .unwrap();

        assert_eq!(failed, vec!["caches", "tags"]);
    }

    /// The healthy path must stay silent — a wrong implementation that
    /// always names every family (or one hardcoded regardless of outcome)
    /// would still pass the two tests above by accident, since neither
    /// checks the non-failing families are actually absent from `failed`.
    #[tokio::test]
    async fn repo_detail_with_warnings_is_empty_when_every_listing_succeeds() {
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
            .and(path("/orgs/systm-d/packages/container/repolens/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/repolens/pulls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/repolens/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/repolens/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/repolens/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/repolens"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let (_, failed) = repo_detail_with_warnings(&client, "systm-d", "repolens")
            .await
            .unwrap();

        assert!(failed.is_empty(), "got: {failed:?}");
    }

    /// `closed_r` (the PR listing) and `default_branch_r` are not one of the
    /// "seven familles" Finding 4 names — they feed classification, not
    /// `items` rows of their own — so their failure must not be reported the
    /// same way. A wrong implementation folding all nine joined futures into
    /// `failed` would name "pulls" here even though nothing in Finding 4's
    /// own wording covers it.
    #[tokio::test]
    async fn a_failed_pr_or_default_branch_listing_is_not_named_in_warnings() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/caches"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "actions_caches": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/artifacts"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "artifacts": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/actions/runs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({ "workflow_runs": [] })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/orgs/systm-d/packages/container/claudine/versions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/pulls"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/releases"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let (_, failed) = repo_detail_with_warnings(&client, "systm-d", "claudine")
            .await
            .unwrap();

        assert!(failed.is_empty(), "got: {failed:?}");
    }

    /// Mounts the nine calls `repo_detail_with_warnings` joins for
    /// `systm-d/{repo}`, each answering a healthy, empty page — and no
    /// package published, like most repositories.
    async fn mount_an_empty_repository(server: &MockServer, repo: &str) {
        let empty = |body: serde_json::Value| ResponseTemplate::new(200).set_body_json(body);
        let routes = [
            (
                format!("/repos/systm-d/{repo}/actions/caches"),
                empty(serde_json::json!({ "actions_caches": [] })),
            ),
            (
                format!("/repos/systm-d/{repo}/actions/artifacts"),
                empty(serde_json::json!({ "artifacts": [] })),
            ),
            (
                format!("/repos/systm-d/{repo}/actions/runs"),
                empty(serde_json::json!({ "workflow_runs": [] })),
            ),
            (
                format!("/orgs/systm-d/packages/container/{repo}/versions"),
                ResponseTemplate::new(404),
            ),
            (
                format!("/repos/systm-d/{repo}/pulls"),
                empty(serde_json::json!([])),
            ),
            (
                format!("/repos/systm-d/{repo}/branches"),
                empty(serde_json::json!([])),
            ),
            (
                format!("/repos/systm-d/{repo}/tags"),
                empty(serde_json::json!([])),
            ),
            (
                format!("/repos/systm-d/{repo}/releases"),
                empty(serde_json::json!([])),
            ),
            (
                format!("/repos/systm-d/{repo}"),
                empty(serde_json::json!({ "default_branch": "main" })),
            ),
        ];
        for (route, response) in routes {
            Mock::given(method("GET"))
                .and(path(route))
                .respond_with(response)
                .mount(server)
                .await;
        }
    }

    /// `TOTAL_CALLS` is the load bar's denominator, so it must be the number
    /// of calls the drill-down really joins. A complete run ticks exactly
    /// that many times — a tenth listing joined with its tick would
    /// overshoot it — and sends exactly that many requests — a tenth joined
    /// without one would leave the bar full while it still runs, and a
    /// constant left at nine after a tenth family would stop the bar at
    /// 90 %. The channel closes with the run: the event loop's forwarder
    /// waits for that before the listing lands.
    #[tokio::test]
    async fn repo_detail_ticking_ticks_exactly_total_calls_times() {
        let server = MockServer::start().await;
        mount_an_empty_repository(&server, "tics").await;
        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let (tick, mut ticks) = tokio::sync::mpsc::unbounded_channel();

        let (_, failed) = repo_detail_ticking(&client, "systm-d", "tics", tick)
            .await
            .unwrap();
        assert!(
            failed.is_empty(),
            "the fixture must be a complete run: {failed:?}"
        );

        let counted = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut n = 0;
            while ticks.recv().await.is_some() {
                n += 1;
            }
            n
        })
        .await
        .expect("the tick channel outlived the run");
        assert_eq!(counted, TOTAL_CALLS, "ticks of a complete run");
        let requests = server
            .received_requests()
            .await
            .expect("wiremock records requests");
        assert_eq!(
            requests.len(),
            TOTAL_CALLS,
            "requests of a complete run: {:?}",
            requests.iter().map(|r| r.url.path()).collect::<Vec<_>>()
        );
    }

    /// Each call ticks as it lands, not all nine once the join is over: a
    /// bar fed that way has two states, and says less than the
    /// `(chargement…)` it sits under. The releases listing answers a second
    /// after the eight others, whose eight ticks must all arrive while it is
    /// still running.
    #[tokio::test]
    async fn repo_detail_ticking_ticks_each_call_as_it_lands() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/tics/releases"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([]))
                    .set_delay(std::time::Duration::from_secs(1)),
            )
            .with_priority(1)
            .mount(&server)
            .await;
        mount_an_empty_repository(&server, "tics").await;
        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let (tick, mut ticks) = tokio::sync::mpsc::unbounded_channel();

        let run = repo_detail_ticking(&client, "systm-d", "tics", tick);
        tokio::pin!(run);
        let eight = async {
            for _ in 1..TOTAL_CALLS {
                ticks
                    .recv()
                    .await
                    .expect("the run closed its ticks before its fast calls ticked");
            }
        };
        tokio::select! {
            biased;
            _ = &mut run => panic!("the slow call landed before the eight fast ones had ticked"),
            () = eight => {}
        }
        run.await.unwrap();
        assert!(ticks.recv().await.is_some(), "the slow call never ticked");
    }
}
