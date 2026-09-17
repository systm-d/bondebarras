# Safety model

This page is the contractual reference behind every safety claim bondebarras
makes — on the site, in the README, and in the interface itself. Where a
summary elsewhere is shorter, this page is the one that is binding.

Everything below is verified against the code, and the rules that matter are
enforced by named tests, cited inline. A promise backed by a test is worth
more than a promise in prose.

## Philosophy

Four rules shape the whole tool.

1. **Nothing is mutated without a confirmation.** Every deletion, and every
   archive, goes through a confirmation modal in the TUI, or through
   `--yes` in a headless run. Without `--yes`, `clean` prints the plan and
   deletes nothing.
2. **There is no trash and no undo.** Nothing GitHub lets this tool *delete*
   is reversible on GitHub's side, so the tool never pretends otherwise.
   Repository archiving is the one exception, and it is worded as such
   rather than borrowing a deletion's wording.
3. **A protected resource is never taken in bulk** — in the TUI or from a
   cron. The guard lives on the resource itself, not in a caller's
   discipline, because an unattended run has no human to notice a broken
   deployment.
4. **The risk tier is carried by the type, not by the interface.**
   `model::risk_tier` is an exhaustive `match` over `ResourceKind`: a
   destructive kind added without a tier assigned does not compile.

## The three levels

Every resource row carries one of three markers, shown beside its checkbox in
the resources column:

| Marker | Level | Meaning |
| --- | --- | --- |
| `⛑` | `Safety::Safe` | Safe **according to the rules documented on this page** |
| `•` | `Safety::Check` | Plausibly dead, but a person should look |
| *(none)* | `Safety::Keep` | Keep by default — live, protected, or unproven |

Two levels would have forced an arbitrary call: an asset from the release
before last and a cache from a closed pull request are not dead in the same
way, and collapsing them means lying in one direction or the other.

The two bulk keys map onto these levels directly:

- `[A]` (`App::select_safe`) ticks every `⛑` row, and only those.
- `[V]` (`App::select_safe_and_check`) ticks every `⛑` row **and** every `•`
  row.

Neither key ever ticks an unmarked row. Both go through one shared code path
(`App::select_levels`), so their guards cannot drift apart.

### What decides each level, family by family

These rules live in `safety::classify`, and every one of them is computed from
data the drill-down already fetched — they cost no extra request.

**Cache.** A cache attached to a closed or merged pull request is `⛑` (this is
the `⚑` flag's own signal). A cache on a branch a merged PR came from is `⛑`.
A cache on a ref that no longer exists is `⛑` — but only when the branch
listing came back whole *and* the default branch is known, because otherwise
absence of data would pass for proof of absence. A cache on the default branch
is unmarked. Anything else is `•`. Age plays **no part** in a cache's level.
Tests: `a_cache_pinned_to_a_closed_pr_is_safe_via_stale_pr`,
`a_cache_on_a_merged_branchs_ref_is_safe`, `a_cache_on_a_vanished_branch_is_safe`,
`an_unknown_branch_set_never_makes_a_cache_safe_by_absence`,
`a_cache_on_the_default_branch_is_kept`.

**Artifact.** An expired artifact is `⛑` — GitHub has already made it
undownloadable, it only occupies a row. An unexpired artifact 30 days old or
more is `•`. Anything newer is unmarked. Tests:
`an_expired_artifact_is_safe_but_a_recent_one_is_kept`,
`an_old_unexpired_artifact_is_only_worth_checking`.

**Workflow run.** A run whose work landed — a merged pull request — is `⛑`,
at any age. Otherwise a run 90 days old or more is `•`, and anything newer is
unmarked. A run is judged by whether its work landed, never by ref topology: a
run is a *record*, and a branch disappearing says nothing about whether anyone
still wants to read it. Test:
`a_workflow_run_is_judged_by_merge_and_age_not_by_ref_topology`.

**Package version.** An untagged version, or an attestation whose subject
image is gone, is `⛑`. A version carrying at least one real tag is unmarked.
Test: `a_tagged_package_version_is_kept_and_an_untagged_one_is_safe`.

**Branch.** A branch a merged pull request came from is `⛑`. The default
branch and a GitHub-protected branch are unmarked. A branch that is simply
unmerged is `•` — someone may still be working on it. Test:
`every_branch_class_reaches_its_own_safety_level`.

**Tag.** Always unmarked, at any age. Test: `a_tag_is_always_kept`.

**Release asset.** Judged by how far back its release is, **not by age**: an
asset of the newest release is unmarked, one of the release just before it is
`•`, and one two releases back or more is `⛑`. An asset whose release tag
cannot be read out of its label is `•`, never `⛑`. Tests:
`an_asset_two_releases_back_is_safe_and_the_previous_one_is_not`,
`an_asset_whose_tag_is_absent_from_the_release_list_is_only_worth_checking`.

**Repository.** Never marked at any level, at any age. `pushed_at` alone is
not proof of abandonment — a finished, stable library does not move for two
years without being dead. Test: `a_repository_is_never_marked_whatever_its_age`.

### The backstop

Above every rule above sits one that outranks them all: **a `protected`
resource is never `⛑`.** Every family already honours this on its own today;
the backstop in `classify` exists so a family added later cannot reintroduce
the hole by forgetting. Test: `a_protected_resource_is_never_safe`.

## The complete table

`Classification` is the level a row can reach; `Protection` is whether
`Resource.protected` is set, which is what bulk selection reads.

| Resource kind | Classification | Protection (`protected`) | Reversible? |
| --- | --- | --- | --- |
| Cache | `⛑` / `•` / unmarked | Never | No — but a re-run produces a new cache |
| Artifact | `⛑` / `•` / unmarked | Never | No — but a re-run produces a new artifact |
| Workflow run | `⛑` / `•` / unmarked | Never | No — the record is gone; a re-run is a *new* run |
| Package version | `⛑` / unmarked | When tagged | No |
| Branch | `⛑` / `•` / unmarked | Unless a merged PR came from it | No |
| Tag | unmarked only | Always | No |
| Release asset | `⛑` / `•` / unmarked | Never | No |
| Repository | unmarked only | Not used — the kind itself is refused in bulk | **Yes** — un-archiving restores it |

Per-family detail, including what each family is measured in and which flag
selects it headlessly, is in [Supported resources](resources.md).

## Individual selection is not bulk selection

The distinction is deliberate, and it is where most of the nuance lives: the
rules above govern what a *key* may tick on your behalf. They are not a list
of what you are forbidden to delete.

**Individually (`espace`, in the TUI)** a visible row stays tickable whatever
its level — `⛑`, `•` or unmarked. A tag, a tagged package version and a live
unmerged branch are all individually tickable. A human looking at that one row
is the case the tool allows it in.

There is exactly one individual refusal among resources: a branch GitHub
itself would refuse to delete — the default branch, or one covered by a branch
protection rule. Offering that tick would be a lie the API then contradicts,
so the status line answers instead:

```text
Cette branche est protégée par GitHub : sélection refusée.
```

**In bulk (`[A]` / `[V]`)** the selection is narrowed four ways. It takes only
rows at the requested levels; only rows currently visible, so a filter hides
rows from the keys as well as from the eye; never a `protected` row; and never
a `Repository`, which is excluded by kind, explicitly and defensively.

When a bulk key leaves visible rows unticked because they are protected, the
status line says how many, rather than appearing to skip rows for no reason —
and names live branches as such when they are all it left:

```text
3 branches vivantes protégées non cochées.
```

## The repository is ticked somewhere else entirely

A repository is not a row in the resources column; it lives in the
repositories tree, and it has its own tick (`App::toggle_repo_selected`),
which refuses anything but a genuine candidate:

| Repository class | Tickable? | What the status line says |
| --- | --- | --- |
| `Archivable` | Yes | — |
| `AlreadyArchived` | **No** | `Ce dépôt est déjà archivé : rien à faire.` |
| `NoAdminRights` | **No** | `Droits d'admin requis sur ce dépôt : sélection refusée.` |

This is a harder refusal than a protected tag or a live branch, which stay
tickable one row at a time. Ticking a different repository replaces whichever
one was ticked before: there is no multi-repository archive, because the
archive endpoint targets one repository at a time.

A repository whose rights this token could not confirm defaults to
`NoAdminRights` — when the real rights are unknown, the safe direction is to
offer nothing rather than invite a tick the API would answer with a 403.

## The TUI and headless runs differ, deliberately

A cron job has no human at the other end. Four rules follow from that.

**Naming no resource family selects nothing.** A `clean` invocation that
quietly meant "everything" would be the worst possible default for an
irreversible, unattended operation. Test: `no_family_flag_selects_nothing`.

**A protected resource is filtered out before any other rule**, headless or
not. `--packages --yes` from a crontab never touches a tagged version;
`--branches --yes` never touches a live one; `--tags --yes` never touches
anything at all, since every tag is protected. Test:
`a_protected_resource_is_never_taken_in_bulk`.

**A repository is never archived headlessly — unconditionally.** There is no
`--archive` flag, and none is planned. Every other family has *some* headless
path; this one has none, at all. This is the only time the product refuses an
entire resource *family* headlessly rather than a tier or a single protected
instance. Test: `headless_select_never_returns_a_repository`.

**Tier 3 is always refused headlessly**, with no flag to bypass it. Nothing in
bondebarras' current scope reaches that tier, but the rule is set now, while
the CLI surface is still small, so a future destructive operation cannot slip
into a cron job by accident.

Beyond that, `--stale-pr` and `--older-than <days>` narrow the selection
within the families named; `--older-than` is inclusive of its boundary.

## Confirmations: Tier 1 and Tier 2

A plan's tier is the most severe tier among its items.

**Tier 1 — `Cache`, `Artifact`, `WorkflowRun`.** Regenerable by re-running a
workflow, so a bare confirmation is the right amount of friction:

```text
Ces éléments sont régénérables par un re-run.
Supprimer ?   [y/N]
```

**Tier 2 — `PackageVersion`, `Branch`, `Tag`, `ReleaseAsset`.** None of these
comes back from a re-run, so the modal lists what will go — up to eight items,
past which the rest collapses into a count — and says so plainly:

```text
Ces éléments ne reviendront pas : une fois supprimés, ils quittent le registre pour de bon.
```

When the plan actually contains a package version, the modal adds a caveat
the API gives no way to resolve: an untagged version may be one layer of a
multi-architecture image, and deleting it would break its parent manifest.

**Tier 2 — `Repository`.** Also Tier 2, but for the opposite reason to every
other family there: it is *reversible*. So it never reuses the deletion
wording, and the verb changes too — nothing here is deleted:

```text
Ce dépôt passera en lecture seule et ses Actions seront désactivées — réversible en désarchivant sur GitHub.
Archiver ?   [y/N]
```

Claiming a reversible action is permanent would be as much a lie as the
reverse. The test that guards this is
`an_archive_plan_never_claims_irreversibility_and_prompts_to_archive`: it
asserts an archive modal says `Archiver` and `réversible`, and says neither
`Supprimer` nor `ne reviendront pas`.

**Tier 3** exists in the type and is deliberately unused: repository
*deletion*, the operation it was conceived for, is permanently out of scope.
The modal falls back to the more cautious itemised form for it, so a future
tier-3 resource added without updating the modal degrades to "too much
friction", never "too little".

The confirmation footer — the prompt and the warning that justifies it — is
never truncated to fit a small terminal. The recap above it gives way first,
all the way down to nothing: a user who cannot see what is about to be deleted
can still refuse, but a user who cannot see the prompt can do neither.

## What is irreversible

Everything this tool deletes. There is no trash and no undo.

The Tier 1 families are called *regenerable*, not *recoverable*, and the
difference is real: re-running a workflow produces a **new** cache, artifact or
run. It does not restore the one that was deleted, and for a workflow run —
which is a record of what happened, with its logs — the original is gone for
good.

The Tier 2 deletions are gone outright: a package version leaves the registry,
a branch or tag ref leaves the repository, a release asset leaves its release.

**A release is never deletable — only its assets are.** `ResourceKind` has no
`Release` variant and never will: a release is a point in the repository's
history, and its weight is entirely in the binaries attached to it. There is no
flag that deletes a release, on any tier.

**Repository deletion is permanently out of scope.** Archiving is reversible
and covers the same need more safely, so nothing in this tool ever needs the
`delete_repo` scope.

## Archiving: the one reversible operation

Repository archiving is the only mutation bondebarras performs that GitHub can
undo — un-archiving restores the repository. That is exactly what keeps it off
Tier 3 despite turning a whole repository read-only.

**It frees no bytes, and the tool says so rather than glossing over it.** The
repository's size is unchanged; only its Actions are disabled. The plan summary
states it outright — `Archivage · 0 o libéré, par nature` — instead of
borrowing the "size unknown" wording a sizeless deletion gets, because for
archiving the size question *is* answered, and the answer is zero.

It earns its place anyway: an archived repository stops *producing* the caches,
artifacts and workflow runs every other family here cleans up. Closing the tap,
rather than mopping the floor forever.

## Rate limits, retries, and per-item outcomes

Reads run at a concurrency of 8, under a 30-second ceiling on any single call —
GitHub's primary limit (5,000/hour) is never the binding constraint at this
scale; the secondary limit on burst concurrency is.

Deletions are spaced 120 ms apart, because a purge of a hundred-plus caches is
exactly the kind of burst GitHub's secondary rate limit rejects.

**A throttled deletion is retried up to three times.** The delay is whatever
GitHub named in `Retry-After`, or five seconds when it named none. What counts
as throttling is deliberately narrow: a 429 always, and a 403 **only when it
carries `Retry-After`**. GitHub returns 403 both for the secondary rate limit
and for a missing permission, and the second is far more common — retrying a
permission error would only delay the real message by three backoffs. Tests:
`a_permission_403_is_not_retried`, `retry_after_reads_the_header_as_seconds`.

Archiving goes through a `PATCH` with no retry loop: one archive call is not a
burst.

**Every item reports its own outcome.** Each deletion emits a success or a
failure carrying its own kind, id and target repository, so two runs in flight
can never be attributed to one another. A run that ends with failures says so
rather than hiding it; headless, it prints each failure and exits non-zero. A
403 on an archive surfaces as a failure, never as a silent success — test
`a_403_is_an_error_not_a_silent_success`.

A family whose listing is refused costs only that family's rows: a token
missing one scope, or one endpoint's transient outage, never fails the whole
drill-down. The interface names the families it could not read, so an empty
list never quietly reads as "this repository holds nothing".

## The limits of this model

**`⛑` means "safe according to bondebarras' documented rules" — it is not an
absolute guarantee, and this page will not pretend otherwise.** Review the plan
before confirming.

The rules are computed from what GitHub's API exposes, and that has limits:

- **Nothing can prove the absence of every reference.** A cache key, a
  container digest, a branch name or an asset URL can be referenced by
  something outside GitHub's API — another workflow, an external script, a
  deployment system, a bookmark. The tool reasons about what GitHub reports,
  not about the whole world.
- **An untagged package version may be one layer of a multi-architecture
  image.** Deleting it would break its parent manifest, and GitHub's API gives
  no way to check. This is why the confirmation says so instead of resolving
  it.
- **Listings are paginated with a hard cap.** A repository past the cap yields
  a truncated listing, and a truncated branch listing withholds the
  vanished-branch rule entirely rather than letting absence pass for proof of
  absence.
- **A failed lookup degrades the classification, in the cautious direction.**
  When the default-branch lookup fails, an unmatched branch reads as protected
  rather than live. When the branch listing is incomplete, a cache on a ref
  missing from it falls to `•` instead of `⛑`.
- **Age is not proof of abandonment.** It is why a repository is never marked
  and never preselected, whatever its `pushed_at`.
- **Classification depends on the closed-pull-request listing.** When that
  listing fails, every branch simply reads as "not known to be dead" — the
  cautious direction, but a less useful one.

If something matters and the tool cannot prove it, the tool shows it and lets
you decide. That is the whole design.
