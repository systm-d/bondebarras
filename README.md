# bondebarras

*Good riddance.*

**bondebarras** is a Rust TUI/CLI to audit and clean up the resources piling
up across your GitHub organizations: Actions caches, artifacts, and workflow
runs — the stuff CI leaves behind that nobody ever comes back to delete.

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
- **⚑ Stale-PR flag** — every cache is checked against the repository's
  closed pull requests; a cache attached to a closed or merged PR is flagged
  as safe to delete in one keystroke.
- **Ad-hoc bulk selection** — sort by size/age/name, filter incrementally by
  label, or select every flagged row at once. Nothing is persisted: no rules
  engine, no config file, you decide every time.
- **Tier-1 confirmation** before any deletion, followed by a background purge
  with a per-item result — the TUI stays responsive throughout.
- **Billing tab** — per-organization Actions-minutes usage against the free
  allowance, month by month, with a per-repository breakdown of what is
  burning it. On the author's own account, `systm-d` sat at **818%** of its
  included minutes in July 2026 (16,369 Linux-equivalent minutes against a
  2,000-minute allowance) — exactly the kind of runaway usage the tab exists
  to surface, since minutes cannot be reclaimed after the fact.
- **Headless CLI** — `bondebarras scan --json` for a machine-readable
  overview, and `bondebarras clean` for non-interactive cleanup, e.g. from a
  cron job.

## Safety

- Nothing this tool deletes is reversible on GitHub's side, so it never
  pretends otherwise: there is **no trash and no undo**.
- Every deletion goes through a confirmation before anything happens.
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
| `Space`              | Check / uncheck the row under the cursor                   |
| `s`                  | Cycle sort: size → age → name                              |
| `f`                  | Enter filter mode (incremental match on the label)          |
| `A`                  | Select every ⚑-flagged row                                 |
| `d`                  | Delete the current selection (opens the confirmation modal) |
| `y` / `N`            | Confirm / cancel a pending deletion                         |
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
```

| Flag | Effect |
| --- | --- |
| `--org <name>` | Limits `scan` to one organization; absent = every one the token can see |
| `--json` | Machine-readable output on stdout — nothing else goes to stdout |
| `--repo <name>` | Repository targeted by `clean` |
| `--caches` `--artifacts` `--runs` | Resource families `clean` should touch, cumulative |
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

### Required token scopes

`repo` and `read:org` are enough for everything bondebarras does — reading
and deleting caches, artifacts, and workflow runs; listing the organizations
and repositories a token can see; and reading the Billing tab's usage report
(a 403 there just means the token's owner isn't an org owner — the org stays
otherwise navigable). Repository *deletion* is explicitly and permanently
out of scope for this tool, so `delete_repo` is never required.

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
