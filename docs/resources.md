# Supported resources

The eight resource families bondebarras handles, and exactly what it will and
will not do with each. Every cell below is verified against the code; where a
family behaves unexpectedly, the reason is given rather than smoothed over.

For *why* the rules are shaped this way — the safety levels, the confirmations,
the limits of the model — see the [safety model](safety.md).

## At a glance

| Resource | Known size | Individual deletion | Bulk selection | Main criterion | Reversible |
| --- | --- | --- | --- | --- | --- |
| Actions cache | Yes | Yes | Yes | Closed/merged PR, merged or vanished branch | No — but a re-run produces a new one |
| Artifact | Yes | Yes | Yes | Expired, then age ≥ 30 days | No — but a re-run produces a new one |
| Workflow run | **No — always `0`** | Yes | Yes | Merged PR, then age ≥ 90 days | No — a re-run is a *new* run |
| Package version (GHCR) | No — shown `—` | Yes | Untagged / orphaned attestation only | Tags and attestations | No |
| Branch | No — shown `—` | Yes, except default/protected | Merged-PR branches only | Merge status and protections | No |
| Tag | No — shown `—` | Yes | **Never** | Always protected | No |
| Release asset | Yes | Yes | Yes | Release depth (not age) | No |
| Repository | No — frees nothing, by design | Archive only, from the tree | **Never** | Human choice only | **Yes** — un-archive |

Two cells in that table deserve their own explanation, because they are the
ones most likely to be read wrong. They are covered under
[Workflow run](#workflow-run) and [Repository](#repository) below.

## Actions cache

The headline family, and the highest-volume cleanup available.

- **Size:** reported by GitHub, per cache entry.
- **Individual deletion:** yes.
- **Bulk selection:** yes — never protected.
- **Headless flag:** `--caches`.
- **Main criterion:** a cache attached to a closed or merged pull request is
  `⛑` and carries the `⚑` flag. So is a cache on a branch a merged PR came
  from, and one on a ref that no longer exists — the latter only when the
  branch listing came back whole and the default branch is known. A cache on
  the default branch is unmarked; anything else is `•`.
- **Age is not a criterion here.** It is available as the `--older-than`
  filter, which applies to every family, but it plays no part in a cache's
  classification.
- **Reversible:** no. A re-run repopulates the cache; it does not restore the
  entry that was deleted.

The `⚑` flag costs no extra request: the closed-pull-request listing it reads
is the same one the dead-branch classification uses.

## Artifact

- **Size:** reported by GitHub.
- **Individual deletion:** yes.
- **Bulk selection:** yes — never protected.
- **Headless flag:** `--artifacts`.
- **Main criterion:** an expired artifact is `⛑` — GitHub has already made it
  undownloadable, so it only occupies a row until deleted. An unexpired
  artifact 30 days old or more is `•`; anything newer is unmarked.
- **Reversible:** no.

## Workflow run

- **Size: none — and this is the one cell worth reading twice.** GitHub's runs
  endpoint reports no size for a workflow run, so bondebarras stores `0`. But
  unlike the other sizeless families below, a run is **not** displayed as `—`:
  it renders as a real `0 o`, because `WorkflowRun` is classed as a kind with a
  known size. The reclaimed space is real — it comes from the logs and
  artifacts GitHub drops alongside the run — it is simply never quantified.
  Do not read a run's `0 o` as "this run occupies nothing".
- **Individual deletion:** yes.
- **Bulk selection:** yes — never protected.
- **Headless flag:** `--runs`.
- **Main criterion:** a run whose work landed — a merged pull request — is
  `⛑`, at any age. Otherwise age decides: 90 days or more is `•`, newer is
  unmarked. A run is judged by whether its work landed, never by ref topology:
  a run is a record, and a branch disappearing says nothing about whether
  anyone still wants to read it.
- **Reversible:** no. Re-running produces a new run; the deleted record and its
  logs are gone.

## Package version (GHCR)

- **Size: none, ever.** GitHub exposes no size field for a package version
  under any name, and no billing SKU covers package storage either. The size
  is hardcoded to `0` and displayed as `—`, never as `0 o`, which would read
  as "empty" — the opposite of the truth. The resources column carries a
  header line explaining the `—`. **Never estimate or extrapolate a figure
  here.**
- **Individual deletion:** yes, including a tagged version — one row at a
  time, from the TUI.
- **Bulk selection:** untagged versions and orphaned attestations only. A
  tagged version is protected.
- **Headless flag:** `--packages` — which still cannot take a tagged version.
- **Main criterion:** a version with no tag is untagged; a version whose every
  tag is an attestation for an image that is gone is an orphaned attestation;
  anything with at least one real tag is tagged, and protected.
- **Reversible:** no — the layer leaves the registry.

An attestation is recognised only by a `sha256-` prefix followed by exactly 64
lowercase hex characters. The strictness is not pedantry: the cost of reading a
user's own tag as an attestation is deleting a real image.

This family is a hygiene cleanup, not a volume one.

## Branch

- **Size:** none — a ref carries no size. Displayed `—`.
- **Individual deletion:** yes, **except** the default branch and any branch
  GitHub protects, which are refused even individually. A merely unmerged
  branch stays individually tickable.
- **Bulk selection:** only a branch a merged pull request came from.
- **Headless flag:** `--branches` — which still takes only merged-PR branches.
- **Main criterion:** classified in strict priority order — default branch
  first, then GitHub-protected, then backed by a merged PR, then simply live.
- **Reversible:** no.

**A branch is only ever offered dead because a pull request merged it.** The
classification reads `head.ref` and `merged_at` off the closed-PR listing that
was already fetched — one call, two uses, zero marginal requests. A PR closed
*without* merging leaves its branch alone: that work was rejected, not
integrated, and someone may still intend to resume it. The test that guards
this is `a_branch_with_no_merged_pr_is_alive`, and the negative case is the one
that matters — a classifier keying on "closed" alone would offer to delete
work someone meant to come back to.

When the default-branch lookup fails, an unmatched branch errs toward
*protected* rather than live, so the row most likely to actually be the default
branch is the one covered when the lookup that would have proven it failed.

## Tag

- **Size:** none. Displayed `—`.
- **Individual deletion:** yes.
- **Bulk selection: never.** Every tag is protected, unconditionally, so
  `--tags --yes` from a crontab selects nothing at all.
- **Headless flag:** `--tags` — which exists, and by itself takes nothing.
- **Main criterion:** none needed; a tag is always unmarked and always
  protected. It is what a release, a `go get` or a `Cargo.toml` points at by
  name.
- **Reversible:** no.

## Release asset

- **Size:** reported by GitHub. This is the volume story of the v0.4 family —
  unlike a package version, an asset's bytes are real and known.
- **Individual deletion:** yes.
- **Bulk selection:** yes — an asset is never protected.
- **Headless flag:** `--assets`.
- **Main criterion: release depth, not age.** An asset of the newest release
  is unmarked, one of the release just before it is `•`, and one two releases
  back or more is `⛑`. An asset whose release tag cannot be parsed out of its
  label is `•`, never `⛑` — nothing proves it old.
- **Reversible:** no.

**A release is never deletable — only its assets are.** There is no `Release`
variant and never will be: a release is a point in the repository's history — a
tag, notes, a date — and its weight is entirely in whatever binaries are
attached to it. Every release's assets are flattened into rows, each carrying
its release's tag in the label, since the release itself never becomes a row.

## Repository

The repository is the one candidate that lives in the tree itself rather than
the resource list, and the only one that is archived rather than deleted.

- **Size: archiving frees no bytes, by design.** The repository's size is
  unchanged; only its Actions are disabled. This is stated plainly everywhere
  it matters, including the confirmation's own summary —
  `Archivage · 0 o libéré, par nature` — rather than being dressed up as an
  unknown quantity.
- **Individual selection:** its own tick in the repositories column, and only
  for a genuine candidate. An already-archived repository, or one this token
  cannot administer, is not tickable at all.
- **Bulk selection: never** — excluded by kind from `[A]` and `[V]`.
- **Headless: never, unconditionally.** There is no `--archive` flag and none
  is planned. This is the only entire *family* the product refuses headlessly.
- **Main criterion: a human's judgement.** `pushed_at` is shown on the row but
  is never a trigger: a finished, stable library does not move for two years
  without being dead. There is no preselection path whatsoever.
- **Reversible: yes** — un-archiving restores it on GitHub. It is the only
  mutation here that GitHub can undo, which is exactly why repository
  *deletion* is permanently out of scope: nothing a delete could offer is not
  already covered more safely.

Archiving earns its place by closing the tap: an archived repository stops
producing the caches, artifacts and workflow runs every other family here
cleans up.

A repository's row shows either its age in days or its class — `déjà archivé`,
`sans droits` — the same "classification replaces the age" shape a branch row
already has.

## The two rules that cut across every family

**`protected` is refused in bulk, unconditionally.** A tagged package version,
a live branch and every tag set it; every other family always sets `false`. The
filter applies before any other rule, headless or not, because the guard
belongs on the resource rather than in a caller's discipline. Individual
selection is unaffected.

**Naming no family selects nothing.** A headless `clean` that quietly meant
"everything" would be the worst possible default for an irreversible operation
running unattended.
