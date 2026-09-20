<p align="center">
  <img src="https://raw.githubusercontent.com/systm-d/bondebarras/main/site/static/logo-h.png"
       alt="bondebarras" width="420">
</p>

<p align="center"><em>Good riddance.</em></p>

**bondebarras** is a Rust TUI/CLI that audits and clears the resources piling
up across your GitHub organizations: Actions caches, artifacts and workflow
runs, container package versions, merged branches, tags and release assets —
the stuff CI leaves behind that nobody ever comes back to delete — plus
repository archiving, which turns off the tap producing them rather than
mopping up after it forever. A Billing tab prices what is left.

[![Pre-release](https://img.shields.io/badge/release-v1.0.0--rc.2%20%E2%80%94%20pre--release-d97757)](https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.2)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![CI](https://github.com/systm-d/bondebarras/actions/workflows/ci.yml/badge.svg)](https://github.com/systm-d/bondebarras/actions/workflows/ci.yml)
[![Pages](https://github.com/systm-d/bondebarras/actions/workflows/pages.yml/badge.svg)](https://github.com/systm-d/bondebarras/actions/workflows/pages.yml)
[![Release](https://github.com/systm-d/bondebarras/actions/workflows/release.yml/badge.svg)](https://github.com/systm-d/bondebarras/actions/workflows/release.yml)

**Site:** <https://systm-d.github.io/bondebarras> · **Docs:**
[`docs/`](docs/README.md)

> **Status: `v1.0.0-rc.2` — pre-release.** There is no stable release yet.
> The tool is feature-complete and safe to run — every deletion goes through a
> confirmation, and nothing is ever deleted without one — but the CLI flags and
> the `scan --json` schema may still change before `v1.0.0`, so pin the version
> if you script against them. Homebrew, the AUR and winget are
> [not published yet](docs/installation.md#channels-that-are-not-available-yet);
> install from the [release binaries][rc] or from source.

---

## Why

An account with fifteen organizations accumulates a surprising amount of dead
weight: on the author's own account, GitHub Actions caches alone add up to
**51.4 GB**, with a single repository holding **69 caches for 11.1 GB** —
almost all of it pinned to pull requests closed months ago. That junk crowds
out the caches that still matter and slows down every CI run that has to
rebuild what should have stayed cached.

bondebarras' headline feature is exactly that: **flagging Actions caches
pinned to a closed pull request** — the safest, highest-volume cleanup
available, since a cache tied to a merged or closed PR can never be read again
by anything.

The 10 GB it gauges a repository's caches against is GitHub's *default
included* threshold, not a ceiling: an administrator can raise the real limit,
storage past the threshold is billed, eviction *to make room* waits for the
repository's configured limit — which no API exposes — and, independently of
any limit, GitHub removes every entry not read in over 7 days
([GitHub's usage limits and eviction policy][gh-cache]). The tool states those
facts apart instead of guessing between them; the whole picture is in
[billing and GitHub limits](docs/billing.md).

## Safety, before you install anything

This tool deletes things. Four guarantees hold everywhere, TUI and headless
alike:

- **Every mutation is confirmed** before anything happens — a deletion as much
  as an archive. Headless, that confirmation is `--yes`: without it, `clean`
  prints the plan and deletes nothing.
- **There is no trash and no undo** for anything GitHub lets this tool delete,
  so it never pretends otherwise. Repository archiving is the one reversible
  operation, and it is worded as such rather than borrowing a deletion's
  wording.
- **A protected resource is excluded from every bulk selection** — a tagged
  package version, a live branch, every tag — whether the selection comes from
  a keystroke or from a cron. Individual selection stays available, one row at
  a time.
- **A repository is only ever archived from the TUI**, one tick at a time, and
  never headlessly: there is no `--archive` flag, and none is planned.

Every resource row carries one of three markers: **⛑** safe according to bondebarras'
documented rules, **•** worth checking, unmarked keep. `[A]` takes every ⛑ row,
`[V]` adds every • row, and neither ever takes a protected one.

Review the plan before confirming: ⛑ means safe according to documented rules,
not proof that nothing outside GitHub's API still references the resource. The
full model — every family's classification, the tiered confirmations, rate
limits and retries, and the limits of the model itself — is in
[the safety model](docs/safety.md).

## Quick start

```sh
# 1. Install — from source, on every platform (Rust ≥ 1.88 and a C compiler)
cargo install --git https://github.com/systm-d/bondebarras bondebarras

# 2. Authenticate — the gh session is read first, $GITHUB_TOKEN second
gh auth login

# 3. Launch the TUI
bondebarras
```

Prebuilt binaries for Linux, macOS (Apple Silicon) and Windows, plus a `.deb`
and an `.rpm`, are attached to [`v1.0.0-rc.2`][rc] — every binary artifact with
its own `.sha256` sidecar. Arch builds from the `PKGBUILD` attached to the same
release; there is no AUR package. Every platform, every channel and how to
verify a download are in [installation](docs/installation.md).

**Token scopes:** `repo`, `read:org`, `read:packages`, `delete:packages`.
`admin:org` is optional and buys one display (artifact and log retention).
Billing and budgets are a question of your *role* in the organization, not of a
scope; anything unreadable degrades to `illisible` and never drops the
organization — see
[authentication and permissions](docs/authentication.md).

On startup bondebarras lists every organization the token can see and loads the
org-level overview. The screen is three columns — organizations, the current
org's repositories, the loaded repository's resources — and a repository's own
resources are fetched only once the cursor rests on it for 300 ms. The keys
worth knowing first:

| Key | Action |
| --- | --- |
| `←` `→` `↑` `↓` | Move between and within columns |
| `espace` | Tick the row under the cursor: a resource, or a repository to archive. Not in the organizations column |
| `A` / `V` | Tick every ⛑ row / every ⛑ **and** • row |
| `d`, then `y` | Open the confirmation — for the ticked resources, or the ticked repository. Nothing is touched until `y` |
| `b` | Billing tab, and back; `q` quits |

Every key, every column, the gauges at the head of the resources column and
what each one measures: [using the TUI](docs/tui.md).

**Terminal size.** The three columns need 100 columns; from 78 they fold to
two and below that to one, always from the left, so the column deletion happens
in is never the one dropped. Height matters for the Billing tab: it shows even
its densest organization whole from a 33-row terminal, and below that drops
whole blocks from the bottom rather than cutting a sentence in half.

## What it cleans

| Family | What a bulk selection takes | Size |
| --- | --- | --- |
| Actions caches | Pinned to a closed or merged PR, on a merged or vanished branch | Reported by GitHub |
| Artifacts | Expired, then 30 days and older | Reported by GitHub |
| Workflow runs | Whose PR merged, then 90 days and older | `—` — GitHub reports none |
| Package versions (GHCR) | Untagged layers and orphaned attestations — never a tagged one | `—` — none exists, ever |
| Branches | Only a branch a merged PR came from — never a live one | `—` |
| Tags | **Nothing:** every tag is protected | `—` |
| Release assets | Two releases back or more — never the release itself | Reported by GitHub |
| Repositories | **Nothing:** archived one tick at a time, from the TUI only | Archiving frees no bytes, by design |

Archiving earns its place anyway: an archived repository has its Actions
disabled, so it stops *producing* the caches, artifacts and workflow runs every
other family here cleans up. It is also the one **reversible** operation, which
is why repository *deletion* is permanently out of scope. Family by family —
criteria, protections, reversibility — in
[supported resources](docs/resources.md).

## Headless

`scan` and `clean` cover the same ground for scripts and cron jobs:

```sh
# Non-interactive overview, as JSON — stdout carries JSON and nothing else
bondebarras scan --org systm-d --json

# Delete every cache attached to a closed PR, unattended
bondebarras clean --org systm-d --repo josephine --caches --stale-pr --yes

# Free up release-asset space — the releases stay; only their binaries go
bondebarras clean --org exec-d --repo terminus --assets --older-than 180 --yes
```

**Without `--yes`, `clean` prints the plan and deletes nothing**, and naming no
resource family selects nothing: a `clean` that quietly meant "everything"
would be the worst possible default for an unattended, irreversible operation.
Flags, the JSON schema, exit codes and cron recipes are in the
[CLI reference](docs/cli.md).

`bondebarras update` checks GitHub Releases on demand — never at startup, and
with no token — and acts on the channel you installed through, refusing
anything whose published checksum it cannot verify.

## Documentation

The full user documentation lives in [`docs/`](docs/README.md):

- [Installation](docs/installation.md) — every platform, checksums, updating
- [Authentication and permissions](docs/authentication.md) — scopes, roles,
  what degrades without them
- [Using the TUI](docs/tui.md) — the three columns, the gauges, every key
- [CLI reference](docs/cli.md) — `scan`, `clean`, `update`, JSON, exit codes
- [Safety model](docs/safety.md) — levels, protections, confirmations, limits
- [Supported resources](docs/resources.md) — the eight families in detail
- [Billing and GitHub limits](docs/billing.md) — what the Billing tab reads,
  and never modifies
- [Troubleshooting](docs/troubleshooting.md) — symptoms, causes, resolutions
- [Releases and versioning](docs/releases.md) — pre-releases, checksums, SemVer

`docs/` also holds the project's design history — see
[the index](docs/README.md#design-history) for what that is and is not.

## Contributing

Bug reports, ideas and pull requests are welcome — see
[CONTRIBUTING.md](CONTRIBUTING.md) for the workflow, and
[CONVENTIONS.md](CONVENTIONS.md#quality-gate-run-before-every-pr) for the
quality gate every change must pass. Vulnerabilities go through
[SECURITY.md](SECURITY.md) instead of an issue.

## License

Dual-licensed under **MIT OR Apache-2.0**, at your option — see
[LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

Copyright © 2026 Kevin Delfour / systm-d.

[rc]: https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.2
[gh-cache]: https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy
