# bondebarras

*Good riddance.*

**bondebarras** is a Rust TUI/CLI to audit and clean up the resources piling
up across your GitHub organizations: Actions caches, artifacts, workflow
runs, and container package versions — the stuff CI leaves behind that
nobody ever comes back to delete.

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![CI](https://github.com/systm-d/bondebarras/actions/workflows/ci.yml/badge.svg)](https://github.com/systm-d/bondebarras/actions/workflows/ci.yml)
[![Pages](https://github.com/systm-d/bondebarras/actions/workflows/pages.yml/badge.svg)](https://github.com/systm-d/bondebarras/actions/workflows/pages.yml)
[![Release](https://github.com/systm-d/bondebarras/actions/workflows/release.yml/badge.svg)](https://github.com/systm-d/bondebarras/actions/workflows/release.yml)

**Site:** <https://systm-d.github.io/bondebarras>

---

## Why

An account with fifteen organizations can accumulate a surprising amount of
dead weight: on the author's own account, GitHub Actions caches alone add up
to **51.4 GB**, with a single repository holding **69 caches for 11.1 GB** —
almost all of it CI caches pinned to pull requests that were closed months
ago. GitHub only evicts a repo's caches once it crosses 10 GB or after seven
days of no reads, so this kind of junk just sits there, crowding out the
caches that still matter and slowing down every CI run that has to rebuild
what should have stayed cached.

bondebarras' headline feature is exactly that: **flagging Actions caches
pinned to a closed pull request** — the safest, highest-volume cleanup
available, since a cache tied to a merged or closed PR can never be read
again by anything.

## Features

- **Two-stage scan** — org-level aggregates load in seconds for every
  organization the token can see; a repository's individual resources are
  only fetched when you drill into it, so you only pay for what you look at.
- **Split-pane TUI** — organizations (and their repositories) on the left,
  the selected repository's resources on the right.
- **Actions caches, artifacts, and workflow runs**, each deletable
  individually or as part of a bulk selection.
- **Container package versions** (GHCR) — untagged layers and orphaned
  attestations, flagged the same way a cache attached to a closed PR is.
  GitHub's API exposes **no size** for a package version, so this family is
  never measured in bytes: it is a hygiene cleanup, not a volume one. On the
  author's own account, the real footprint across fifteen organizations is
  **7 packages, 45 versions, 23 of them untagged** — one organization alone
  (`maxds-lyon`) carries 20 of its 28 versions with no tag at all.
- **Merged branches, tags, and release assets** — a branch is offered dead
  the moment a pull request merges it, at zero extra requests: the same
  closed-PR listing the ⚑ flag already fetches carries `head.ref` and
  `merged_at`. A closed-but-unmerged PR leaves its branch alone — the work
  may still be resumed. The default branch and any GitHub-protected one are
  shown but never bulk-selectable, and neither is a tag: it's what a
  release, a `go get`, or a `Cargo.toml` points at by name. **Release assets
  are the volume story of this family** — GitHub does expose their size,
  unlike a package version — and the release itself is never deletable,
  only its binaries: a release is a point in the repository's history, and
  its weight is entirely in what's attached to it. Measured across four of
  the author's organizations: **7.3 GB in release assets alone**, led by
  `exec-d/terminus` (1,453 MB across 25 releases) and `delfour-co/githero`
  (1,371 MB across 27).
- **Repository archiving** — the repository itself, ticked one row at a time
  from its own place in the tree (the left pane, not the resource list on
  the right) and archived through the same confirmation flow as every
  deletion. **Archiving frees no bytes** — a repository's size doesn't
  change — but an archived repository has its Actions disabled, so it stops
  *producing* the caches, artifacts and workflow runs every other feature
  here cleans up: closing the tap instead of mopping the floor forever. It
  is also **reversible** — un-archiving restores it — which is exactly why
  repository *deletion* stays permanently out of scope: there is nothing a
  delete could offer that un-archiving doesn't already cover more safely. An
  already-archived repository, or one this token cannot administer, is shown
  but never tickable at all, and `[A]`/headless `clean` refuse the whole
  family unconditionally — no `--archive` flag exists. Measured across five
  of the author's organizations: a dozen repositories with no push in 500 to
  775 days (`maxds-lyon/.github` at 775, `maxds-lyon/lokiprint` at 685), and
  exactly **one** already archived.
- **⚑ Stale-PR flag** — every cache is checked against the repository's
  closed pull requests; a cache attached to a closed or merged PR is flagged
  as safe to delete in one keystroke.
- **Ad-hoc bulk selection** — sort by size/age/name, filter incrementally by
  label, or select every flagged row at once. Nothing is persisted: no rules
  engine, no config file, you decide every time.
- **Tiered confirmation** before any deletion — a bare `[y/N]` for the
  regenerable Tier 1 (caches, artifacts, workflow runs), an itemised recap
  plus an explicit irreversibility warning for Tier 2 (package versions,
  merged branches, tags, and release assets — none of them come back once
  deleted) — followed by a background purge with a per-item result. The TUI
  stays responsive throughout.
- **Billing tab** — per-organization Actions-minutes usage against the free
  allowance, month by month, with a per-repository breakdown of what is
  burning it. The gauge counts **private repositories only**: GitHub's usage
  report discounts a private repo still inside its allowance exactly like a
  public one, so visibility — not the discount fields — is the only signal
  that tells them apart, and a public repo's Actions runs are free and
  unlimited regardless of volume. On the author's own account,
  `SecondBrain-io`'s `monolith-back` burnt **24,632 private
  Linux-equivalent minutes in July 2026** — exactly the kind of runaway usage
  the tab exists to surface, since minutes cannot be reclaimed after the
  fact. (That org is on a different plan, so no allowance percentage is
  given here.)
- **Headless CLI** — `bondebarras scan --json` for a machine-readable
  overview, and `bondebarras clean` for non-interactive cleanup, e.g. from a
  cron job.

## Safety

- Nothing this tool deletes is reversible on GitHub's side, so it never
  pretends otherwise: there is **no trash and no undo**. Repository
  archiving is the one exception, and the confirmation modal says so
  explicitly, in different words from a deletion's — it never claims
  something reversible is permanent, any more than it would claim the
  reverse.
- Every deletion — and every archive — goes through a confirmation before
  anything happens.
- Deletions run in the background, spaced out and retried on GitHub's
  secondary rate limit (`Retry-After` on 429 and 403), so a purge of a
  hundred-plus caches doesn't get itself throttled or rejected.
- Each deleted item reports its own outcome; a run that ends with failures
  says so instead of hiding it.

## Installation

### From source

Rust ≥ 1.88 required.

```sh
git clone https://github.com/systm-d/bondebarras
cd bondebarras
cargo install --path crates/bondebarras
```

### Precompiled packages

Every tag `v*` triggers the [Release](.github/workflows/release.yml) workflow,
which publishes artifacts for the most common platforms:

| Platform            | Artifact                                          |
| -------------------- | ------------------------------------------------- |
| Windows (Microsoft)  | `bondebarras-windows-x86_64.exe` (+ `.zip`)        |
| macOS Apple Silicon  | `bondebarras-macos-aarch64.tar.gz`                 |
| Linux (generic)      | `bondebarras-linux-x86_64.tar.gz`                  |
| Debian / Ubuntu      | `bondebarras_<version>_amd64.deb`                  |
| Fedora / RHEL        | `bondebarras-<version>.x86_64.rpm`                 |
| Arch Linux           | AUR (source) — `yay -S bondebarras`                |

> Intel Macs are covered by Homebrew, which builds from source (no
> precompiled Intel binary).

```sh
# Debian / Ubuntu
sudo dpkg -i bondebarras_*.deb
# Fedora / RHEL
sudo rpm -i bondebarras-*.rpm
```

#### Package managers

**Arch Linux — AUR:**

```sh
yay -S bondebarras   # or: paru -S bondebarras
```

Every release also publishes a ready-to-use `PKGBUILD`
([`packaging/aur/PKGBUILD`](packaging/aur/PKGBUILD)) for a manual install
from source:

```sh
curl -LO https://github.com/systm-d/bondebarras/releases/latest/download/PKGBUILD
makepkg -si
```

**macOS — Homebrew:**

```sh
brew tap systm-d/bondebarras https://github.com/systm-d/bondebarras
brew install bondebarras
```

**Windows — winget** (once the package is published to `winget-pkgs`, see
[`packaging/winget`](packaging/winget/README.md)):

```powershell
winget install bondebarras
```

Otherwise, download `bondebarras-windows-x86_64.exe` from the release and
place it in a folder on your `PATH`.

---

## Usage

### Launch the TUI

```sh
bondebarras
```

On startup, bondebarras resolves a GitHub token (`gh auth token`, falling
back to `$GITHUB_TOKEN`), fetches the organizations it can see, and loads the
stage-1 overview: Actions cache totals and repository lists for each one.

### Keyboard shortcuts (TUI)

| Key                 | Action                                                    |
| -------------------- | ---------------------------------------------------------- |
| `↑` `↓`              | Move the cursor within the focused pane                   |
| `Tab`                | Cycle focus: organizations → repositories → resources     |
| `Enter`              | Drill into the selected repository (stage 2)               |
| `Space`              | Check / uncheck the row under the cursor — a repository row, on the tree, ticks it for archiving instead |
| `s`                  | Cycle sort: size → age → name                              |
| `f`                  | Enter filter mode (incremental match on the label)          |
| `A`                  | Select every ⚑-flagged row (never a repository, at any age) |
| `d`                  | Delete the current selection, or archive a ticked repository (opens the confirmation modal either way) |
| `y` / `N`            | Confirm / cancel what's pending                             |
| `b`                  | Switch to the Billing tab (and back)                        |
| `←` `→`              | Move between months, on the Billing tab                     |
| `Esc`                | Clear an active filter, or quit if there is none            |
| `q`                  | Quit (twice, to confirm, while a purge is running)          |

### CLI subcommands

With no subcommand, `bondebarras` opens the TUI. Two subcommands cover the
same ground headlessly, for scripts and cron jobs:

```sh
# Non-interactive overview, as JSON
bondebarras scan --org systm-d --json

# Delete every cache attached to a closed PR, unattended
bondebarras clean --org systm-d --repo josephine --caches --stale-pr --yes

# Delete every untagged/orphaned package version in a repo, unattended —
# a tagged version (latest, 2.0.2, …) is never touched, headless or not
bondebarras clean --org systm-d --repo repolens --packages --yes

# Free up release-asset space, unattended — the releases themselves stay;
# only their binaries go
bondebarras clean --org exec-d --repo terminus --assets --older-than 180 --yes
```

| Flag | Effect |
| --- | --- |
| `--org <name>` | Limits `scan` to one organization; absent = every one the token can see |
| `--json` | Machine-readable output on stdout — nothing else goes to stdout |
| `--repo <name>` | Repository targeted by `clean` |
| `--caches` `--artifacts` `--runs` `--packages` `--branches` `--tags` `--assets` | Resource families `clean` should touch, cumulative |
| `--stale-pr` | Restricts `clean` to resources flagged ⚑ (attached to a closed PR) |
| `--older-than <days>` | Restricts `clean` to resources at least that old |
| `--yes` | Confirms without a prompt |

**Without `--yes`, `clean` prints the plan and deletes nothing** — the same
dry-run-by-default rule as the TUI's confirmation modal, just without a
keypress to drive it. Naming no resource family selects nothing either: a
`clean` invocation that quietly meant "everything" would be the worst
possible default for an irreversible, unattended operation. The nuclear tier
(Tier 3) is always refused headlessly, with no flag to bypass it — nothing in
bondebarras' current scope reaches it, but the rule is set now, while the CLI
surface is still small, so a future destructive operation can't slip into a
cron job by accident.

**`--packages` cannot delete a tagged package version, headless or not.**
`latest`, `2.0.2`, and any other real tag are *protected*: the rule lives on
the resource itself, not in the TUI's selection step, so it applies just the
same to a `clean --packages --yes` run from a crontab. Only untagged layers
and orphaned attestations are ever taken in bulk. Deleting a tagged version
is still possible, but only one row at a time, from the interactive TUI
(`espace`) — a human looking at that specific row is the case the tool
allows it in.

**`--branches` and `--tags` follow the same rule.** A live branch — the
default one, a GitHub-protected one, or simply one with no merged pull
request behind it — is *protected*, and so is every tag: `--branches --yes`
from a crontab only ever takes a branch a merged PR made dead weight, never
one still in use. `--assets` never touches the release itself, only its
binaries — there is no flag that deletes a release, on any tier, because
bondebarras never offers to.

**There is no `--archive` flag, and `clean` never archives a repository —
this is unconditional, not something any combination of flags can reach.**
Every other family above at least has *some* headless path, gated by
`protected` or a family flag; a repository has none at all. Archiving turns
a whole repository read-only, and that is not a decision a cron job gets to
make on its own — the tree's own tick (`espace`, in the interactive TUI) is
the only way in.

### Required token scopes

`repo`, `read:org`, `read:packages`, and `delete:packages` are enough for
everything bondebarras does — reading and deleting caches, artifacts,
workflow runs, and container package versions; listing the organizations and
repositories a token can see; and reading the Billing tab's usage report (a
403 there just means the token's owner isn't an org owner — the org stays
otherwise navigable). Branches, tags, and release assets need no scope
beyond `repo`, already in that list — nothing new to grant for them.
Repository archiving needs no new scope either: it goes through the same
`repo`-scoped endpoint as everything else, and requires admin rights on the
repository itself — a right the token either has or doesn't, not a scope to
grant. Repository *deletion* is explicitly and permanently out of scope for
this tool, so `delete_repo` is never required.

---

## Development

```sh
# Build
cargo build --workspace

# Tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format (rustfmt.toml: edition 2024, max_width 100)
cargo fmt
```

> **Note:** this project uses `cargo fmt`; CI checks `cargo fmt --check`.
> Run `cargo fmt` before every commit.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## License

Dual-licensed under **MIT OR Apache-2.0**, at your option — see
[LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

Copyright © 2026 Kevin Delfour / systm-d.
