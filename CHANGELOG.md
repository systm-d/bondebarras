# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-07-29

### Added

- Container package versions (GHCR): untagged layers and orphaned
  attestations (a signature tagged `sha256-<digest>` whose signed image is
  gone) are detected and offered for deletion, the same way a cache attached
  to a closed PR is. Tagged versions are shown but never preselected —
  deleting `latest` breaks deployments.
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
