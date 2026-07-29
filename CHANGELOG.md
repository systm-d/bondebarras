# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-07-29

### Added

- Repository archiving — the first candidate that lives in the tree itself
  (the left pane, org and repo) rather than the right-hand resource list. An
  archivable repository is ticked individually from its own row (`espace`)
  and archived through the same confirmation-and-execute path as every other
  family (`d`, then `[y/N]`). An already-archived repository, or one this
  token cannot administer, is shown but **never tickable at all** — not even
  one row at a time, unlike a protected tag or a live branch — since GitHub
  would answer 403 to the second and there is nothing left to do for the
  first.
- **`[A]` (select every flagged row) and headless `clean` both refuse a
  repository unconditionally.** `pushed_at` is not proof of abandonment — a
  finished, stable library can go years without a push without being dead —
  so nothing here is ever preselected, at any age. This is the first time
  bondebarras refuses an entire resource *family* headlessly, not just a
  tier or a single protected instance: there is no `--archive` flag, and
  none is planned.
- The repository's row shows its age (`775 j`) when it is a genuine
  candidate, or its class (`déjà archivé`, `sans droits`) when it is not.

### Note on what this buys

**Archiving frees no bytes** — a repository's own size is unchanged either
way. It earns a place in this tool anyway because an archived repository has
its Actions disabled, so it stops *producing* the caches, artifacts and
workflow runs v0.1 exists to clean up — closing the tap instead of mopping
the floor forever. It is also **reversible**: un-archiving restores it on
GitHub's side, which is exactly what keeps it at Tier 2 rather than Tier 3,
and what keeps repository *deletion* permanently out of scope — there is
nothing a delete could offer here that un-archiving doesn't already cover
more safely.

Measured across five of the author's organizations before writing a line of
code: a dozen repositories with no push in 500 to 775 days —
`maxds-lyon/.github` at 775, `maxds-lyon/lokiprint` at 685 — and exactly
**one** already archived.

## [0.4.0] - 2026-07-29

### Added

- Merged branches, tags, and release assets: three new resource families,
  offered for deletion the same way v0.1's caches and v0.3's package
  versions are.
  - **A branch is offered dead the moment a pull request merges it** —
    detected at zero extra requests, by reading `head.ref`/`merged_at` off
    the same closed-PR listing the caches' ⚑ flag already fetches, rather
    than a `compare` call per branch (a hundred requests on a single busy
    repository). A PR closed *without* merging leaves its branch alone: the
    work may still be resumed. The default branch, any GitHub-protected
    branch, and every tag are shown but never bulk-selectable — a tag is
    what a release, a `go get`, or a `Cargo.toml` points at by name.
  - **Release assets are the volume story of this release**: unlike a
    package version, GitHub does expose an asset's size. Measured across
    four of the author's organizations before writing a line of code:
    **7.3 GB**, led by `exec-d/terminus` (1,453 MB across 25 releases) and
    `delfour-co/githero` (1,371 MB across 27).
  - **The release itself is never deletable — only its assets are.** A
    release is a point in the repository's history (a tag, notes, a date);
    its weight is entirely in whatever binaries are attached to it, and
    deleting those frees the space without erasing the trace. There is no
    flag, at any tier, that removes a release.
  - All three join Tier 2: an itemised recap plus an explicit
    irreversibility warning, since none of them come back once deleted.
- `--branches`, `--tags`, and `--assets` join `clean`'s family flags; like
  every other family, naming none of them still selects nothing. A live
  branch and every tag are refused headlessly regardless of the flag —
  `Resource.protected` handles it, the same guard that already protects a
  tagged package version.
- The resource list now shows each branch and tag's classification: a dead
  branch reads `mergée ⚑`, painted the same way a stale cache is; a live one
  reads `protégée`; a tag always reads `protégé`. A release asset's row
  already carries its release's tag in the label.

### Note on scopes

No new token scope: `repo`, already required since v0.1, covers listing and
deleting branches, tags, and release assets.

## [0.3.0] - 2026-07-29

### Added

- Container package versions (GHCR): untagged layers and orphaned
  attestations (a signature tagged `sha256-<digest>` whose signed image is
  gone) are detected and offered for deletion, the same way a cache attached
  to a closed PR is. A tagged version is `protected`: bulk selection refuses
  it outright — headless `clean --packages --yes` included, not just the
  TUI's preselection — since deleting `latest` breaks every deployment
  pulling it and there is no human at the other end of a cron to notice.
  Deleting one is still possible, one row at a time, from the interactive
  TUI.
- `--packages` joins `clean`'s family flags; like the others, naming no
  family still selects nothing.
- Tier-2 confirmation modal: an itemised recap plus an explicit warning that
  the selection will not come back, since a deleted package version leaves
  the registry for good — unlike a cache or an artifact, which a re-run
  regenerates. Also names the one risk GitHub's API cannot rule out: an
  untagged version may still be a layer of a multi-architecture image, and
  deleting it would break the parent manifest.

### Note on sizing

**GitHub exposes no size for a package version**, under any field name, and
no billing SKU covers package storage either — verified against the live API
on 2026-07-29. So this family reports no bytes: the resource list shows `—`
instead of a size, with a header line saying so explicitly, and nothing here
should be read as a volume feature. It is a hygiene cleanup, measured in
versions, not gigabytes: across the author's fifteen organizations, the
real footprint is **7 packages, 45 versions, 23 of them untagged** — on one
organization alone, 20 of its 28 versions carry no tag at all.

## [0.2.0] - 2026-07-29

### Added

- Billing tab: per-organization Actions-minutes usage against the free
  allowance, with a per-repository breakdown of what is burning it and the
  monthly cost split into gross / covered / billed. The allowance gauge and
  breakdown count **private repositories only** — a public repo's Actions
  runs are free and unlimited regardless of volume. Navigate months with
  `←`/`→`; a 403 (not an org owner) degrades to an "unreadable" notice
  instead of blocking the rest of the tool.
- `bondebarras scan --json` for a machine-readable overview — pure JSON on
  stdout, progress and diagnostics on stderr.
- `bondebarras clean` — non-interactive cleanup for cron: `--org`, `--repo`,
  the `--caches`/`--artifacts`/`--runs` family flags, `--stale-pr`,
  `--older-than`, and `--yes`. Without `--yes` it only prints the plan.

### Fixed

- Quitting while a purge is running now warns once and requires a second
  press, instead of silently dropping whatever deletions were still queued.

### Tests

- Locked the v0.1 fix that drops cache/artifact/workflow-run entries with a
  missing or non-numeric `id` instead of coercing them to `0`.

## [0.1.0] - 2026-07-28

### Added

- Two-stage scan across every organization the token can see.
- Split-pane TUI: organizations on the left, resources on the right.
- Actions caches, artifacts and workflow runs, with individual deletion.
- Flagging of caches attached to a closed pull request.
- Ad-hoc bulk selection: sort, filter, select-all-flagged.
- Tier-1 confirmation before any deletion.
- `bondebarras scan` for a non-interactive overview.
