<p align="center">
  <img src="https://raw.githubusercontent.com/systm-d/bondebarras/main/site/static/logo-h.png"
       alt="bondebarras" width="420">
</p>

<p align="center"><em>Good riddance.</em></p>

**bondebarras** is a Rust TUI/CLI to audit and clean up the resources piling
up across your GitHub organizations: Actions caches, artifacts and workflow
runs, container package versions, merged branches, tags and release assets —
the stuff CI leaves behind that nobody ever comes back to delete — plus
repository archiving, which turns off the tap producing them rather than
mopping up after it forever. A Billing tab prices what is left: Actions
minutes against the allowance of your plan, Actions storage in GB-hours, the
Actions budget that decides what happens once that allowance runs out, and
the artifact and log retention feeding all of it.

[![Pre-release](https://img.shields.io/badge/release-v1.0.0--rc.1%20%E2%80%94%20pre--release-d97757)](https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.1)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)
[![CI](https://github.com/systm-d/bondebarras/actions/workflows/ci.yml/badge.svg)](https://github.com/systm-d/bondebarras/actions/workflows/ci.yml)
[![Pages](https://github.com/systm-d/bondebarras/actions/workflows/pages.yml/badge.svg)](https://github.com/systm-d/bondebarras/actions/workflows/pages.yml)
[![Release](https://github.com/systm-d/bondebarras/actions/workflows/release.yml/badge.svg)](https://github.com/systm-d/bondebarras/actions/workflows/release.yml)

**Site:** <https://systm-d.github.io/bondebarras>

> **Status: `v1.0.0-rc.1` — pre-release.** There is no stable release yet.
> What that means for you: the tool is feature-complete and safe to run —
> every deletion goes through a confirmation, and nothing is ever deleted
> without one — but the CLI flags and the `scan --json` schema may still
> change before `v1.0.0`, so pin the version if you script against them.
> Homebrew, the AUR and winget are
> [not published yet](#after-the-first-stable-release); install from the
> [release binaries][rc] or from source.

---

## Why

An account with fifteen organizations can accumulate a surprising amount of
dead weight: on the author's own account, GitHub Actions caches alone add up
to **51.4 GB**, with a single repository holding **69 caches for 11.1 GB** —
almost all of it CI caches pinned to pull requests that were closed months
ago. And 10 GB is only the *default included* cache threshold per repository
— an administrator can raise the real limit, and storage past it is billed.
GitHub evicts *to make room* only once a repository reaches its
**configured** limit; separately from any limit, it removes every cache
entry that has not been accessed in over 7 days. Either way this kind of
junk just sits there, crowding out the caches that still matter and slowing
down every CI run that has to rebuild what should have stayed cached.

bondebarras' headline feature is exactly that: **flagging Actions caches
pinned to a closed pull request** — the safest, highest-volume cleanup
available, since a cache tied to a merged or closed PR can never be read
again by anything.

## Features

- **Two-stage scan** — org-level aggregates load in seconds for every
  organization the token can see; a repository's individual resources are
  only fetched when you drill into it, so you only pay for what you look at.
- **Three-column TUI** — organizations, the current org's repositories, and
  the selected repository's resources, all three visible at once on a wide
  terminal and folding from the left as it narrows. Resources load
  automatically once the cursor rests on a repository for 300 ms, kept for
  the rest of the session so revisiting one costs no request.
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
  from its own place in the repositories column (not the resources column)
  and archived through the same confirmation flow as every deletion.
  **Archiving frees no bytes** — a repository's size doesn't
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
  ⚑ and marked ⛑ safe, so `[A]` takes it in one keystroke.
- **Three-level safety marking**, on every visible resource — ⛑ *safe*, •
  *worth checking*, or unmarked *keep* — see [Safety](#safety) below.
  `[A]` selects every ⛑ row, `[V]` adds every • row, and neither ever takes
  a protected one.
- **Two per-repository gauges** at the head of the resources column: Actions
  cache usage against GitHub's **default included 10 GB per-repository
  threshold** — *not* a ceiling, and the gauge says so (`seuil inclus ;
  limite réelle non exposée par l'API`): the limit can be raised by an
  administrator, and storage above it is billed. Eviction *to make room*
  starts only at the repository's **configured** limit, which no endpoint
  exposes; separately from any limit, GitHub removes every cache entry not
  accessed in over 7 days. Past 100 % the gauge states both facts apart —
  the excess is billed, and eviction waits on that configured limit —
  rather than guessing between them. The threshold is the decimal 10 GB
  GitHub bills on, not 10 GiB, so a repository at 10.5 GB is flagged
  instead of reading 98 %. Beside it, Actions minutes against the allowance
  of the organization's plan (`formule inconnue`, with no percentage, when
  the plan cannot be read). Neither is ever clamped at 100 %: a number past
  the threshold is real, not an error. Source: GitHub's
  [usage limits and eviction policy][gh-cache].
- **A progress row** appears between the status line and the footer while a
  purge, an archive, or a repository load is running, with a real, counted
  done/total — never an estimate.
- **Ad-hoc bulk selection** — sort by size/age/name, filter incrementally by
  label, or select every safe (or safe-and-worth-checking) row at once.
  Nothing is persisted: no rules engine, no config file, you decide every
  time.
- **Tiered confirmation** before any deletion — a bare `[y/N]` for the
  regenerable Tier 1 (caches, artifacts, workflow runs), an itemised recap
  plus an explicit irreversibility warning for Tier 2 (package versions,
  merged branches, tags, and release assets — none of them come back once
  deleted) — followed by a background purge with a per-item result. The TUI
  stays responsive throughout.
- **Billing tab** — per-organization Actions-minutes usage against the
  allowance of the organization's **current plan** (`free` 2,000, `team`
  3,000, `enterprise` 50,000 minutes a month), month by month, with a
  per-repository breakdown of what is burning it. The plan comes from
  `GET /orgs/{org}`, which only tells an owner: when it cannot be read, or
  names a plan bondebarras has no figure for, the tab shows the total and says
  `formule inconnue` — **never a percentage against a guessed allowance**.
  Every month the tab pages through is measured against today's plan, and the
  tab says so (`quota documenté de la formule actuelle`). On `enterprise`, the
  allowance belongs to the enterprise account and is shared by its
  organizations, so the percentage is a minimum. The gauge counts **private
  repositories only**: GitHub's usage report discounts a private repo still
  inside its allowance exactly like a public one, so visibility — not the
  discount fields — is the only signal that tells them apart, and a public
  repo's Actions runs are free and unlimited regardless of volume. On the
  author's own account, `SecondBrain-io`'s `monolith-back` burnt **24,632
  private Linux-equivalent minutes in July 2026** — exactly the kind of
  runaway usage the tab exists to surface, since minutes cannot be reclaimed
  after the fact. (That org is on `enterprise`: 49 % of its 50,000 included
  minutes — a minimum, since that allowance is shared across the enterprise.)
  Below the minutes, **Actions storage**, billed in GB-hours — every hour a
  gigabyte of artifacts exists — against the plan's included storage
  (`free` 0.5 GB, `team` 2 GB, `enterprise` 50 GB) times the hours of the
  displayed month, a base the gauge states (`base 720 h`). Public
  repositories' storage is counted: GitHub's documentation says their
  minutes are free, and says nothing of their storage. The repositories
  holding it are named, heaviest first, and the tab says what deleting
  artifacts can and cannot do: it stops the accumulation, it does not refund
  hours already counted. The repositories column shows, under a repository's
  row, its GB-hours for the most recent month of the usage report — the
  month the resources column's minutes gauge reads, named on the line — and
  marks with ⚠ a repository whose caches are past the included 10 GB
  (10.0 Go as displayed) — past which GitHub bills the excess at its hourly
  peak, and evicts least-recently-read entries once the repository reaches
  the configured limit it does not publish.
  The tab also says what happens once an allowance runs out, from the
  organization's **Actions budget**: `0.00 $ · bloquant` (GitHub stops
  Actions at the allowance), `5.00 $ · bloquant` (billed up to 5 $, then
  stopped), or no budget at all (overage billed with no ceiling, if a payment
  method is on file). A gauge at 90 % or more with a blocking budget carries a
  warning under it, on the report's most recent month — the only one GitHub
  can still block. A budget on a single Actions SKU is named as such, never
  interpreted. Budgets are read, never changed: changing one commits money,
  and that is permanently out of scope.
  Beside the storage, the organization's **artifact and log retention**
  (90 days is GitHub's default), highlighted when it is 90 days or more on an
  organization holding at least 36 GB-hours of storage that month — 10 % of
  the smallest plan's included storage (0.5 GB × 720 h = 360 GB-h), a fixed
  figure independent of the displayed month's own hour count. It is the tap:
  every artifact a workflow uploads is kept that long. The tab states the two
  things worth knowing before changing it: a workflow's `retention-days` sets
  that one artifact's duration, within this setting; and a change only
  applies to new artifacts and logs. bondebarras only reads the setting.
  The tab fits everything on screen from a 33-row terminal in its densest
  case — an `enterprise` organization (whose shared-quota note takes two
  lines), a blocking Actions budget warning under *both* gauges, a flagged
  retention setting, and a runner SKU the usage report names but
  bondebarras has no multiplier for. Below that height, the two
  per-repository breakdowns shrink first, each keeping its `… et N
  autre(s)` line naming what it left out; only then does the tab drop
  content, always from the bottom and always one whole block at a time —
  the unknown-SKU line, then the cost line, then the second retention note,
  then the first. A note is shown whole or not at all, never cut after its
  first line. At 80×24 that densest case gets as far as the deletion
  notice; an organization with no budget warning gets as far as its
  retention line and the reason under it. A budget on a single SKU adds two
  more lines, and GitHub allows any number of those.
- **Headless CLI** — `bondebarras scan --json` for a machine-readable
  overview, and `bondebarras clean` for non-interactive cleanup, e.g. from a
  cron job.

## Safety

Every visible resource carries a three-level marker, shown in the resources
column beside its checkbox:

| Marker | Meaning | Example |
| --- | --- | --- |
| ⛑ | Safe — nothing live references it | a cache on a closed/merged PR, an expired artifact, a release two behind, an untagged package version, a merged branch |
| • | Worth checking before deciding | a live but unmerged branch, an artifact or workflow run past its age threshold, the release just before the latest |
| *(none)* | Keep | the default/protected branch, a tagged package version, the latest release, a tag, or a repository (never marked, at any age) |

`[A]` ticks every ⛑ row; `[V]` adds every • row too. Neither ever takes a
resource GitHub itself protects — a tagged package version, the default or a
protected branch, a live but unmerged one, every tag — and if either key
leaves protected rows unticked, the status line says how many (a live
branch left unticked is named as such). Individual selection (`espace`)
stays available one row at a time regardless of level.

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

bondebarras currently ships as a **pre-release**, [`v1.0.0-rc.1`][rc] —
there is no stable version yet. Two channels work today; the package
managers further down are **not published yet**, and are listed so you know
what is coming, not as commands to run.

### Available now — [`v1.0.0-rc.1`][rc]

Download the file for your platform from [the release page][rc]:

| Platform            | File on the release page                    |
| ------------------- | ------------------------------------------- |
| Windows x86-64      | `bondebarras-windows-x86_64.exe` (+ `.zip`) |
| macOS Apple Silicon | `bondebarras-macos-aarch64.tar.gz`          |
| Linux x86-64        | `bondebarras-linux-x86_64.tar.gz`           |
| Debian / Ubuntu     | `bondebarras_<version>_amd64.deb`           |
| Fedora / RHEL       | `bondebarras-<version>.x86_64.rpm`          |

The `.deb` and `.rpm` commands below install a file you have **already
downloaded** from that page — neither fetches anything:

```sh
# Debian / Ubuntu, from the directory you downloaded it into
sudo dpkg -i bondebarras_*_amd64.deb
# Fedora / RHEL
sudo rpm -i bondebarras-*.x86_64.rpm
```

On Windows, put `bondebarras-windows-x86_64.exe` in a folder on your `PATH`.

**Or build from source** — every platform, Intel Macs included. Rust ≥ 1.88
and a C compiler (gcc, clang, or MSVC's) are required: the `aws-lc-rs` crypto
backend builds a C library.

```sh
cargo install --git https://github.com/systm-d/bondebarras bondebarras
```

On Arch, the `PKGBUILD` published with every release
([`packaging/aur/PKGBUILD`](packaging/aur/PKGBUILD)) builds the same thing:

```sh
curl -LO https://github.com/systm-d/bondebarras/releases/latest/download/PKGBUILD
makepkg -si
```

### After the first stable release

None of these three is published, so none of their commands works today. For
Homebrew and winget, the release workflow skips the step on a pre-release
tag — one carrying a `-`, like `v1.0.0-rc.1` — on purpose: a release
candidate is not what `brew install bondebarras` should hand out. For the
AUR there is nothing to skip — no AUR job exists at all, and no package has
ever been submitted. Each will be documented here as available only once its
package has actually been published through that channel.

| Channel          | Status        | Why not yet                                                                                                   |
| ---------------- | ------------- | ------------------------------------------------------------------------------------------------------------- |
| Homebrew (macOS) | Not published | The tap serves stable releases only; the workflow renders the formula from [`packaging/homebrew/bondebarras.rb`](packaging/homebrew/bondebarras.rb) on a stable tag |
| AUR (Arch Linux) | Not published | No AUR page exists yet — the `PKGBUILD` above is the supported path meanwhile                                   |
| winget (Windows) | Not published | The manifest has not been accepted into `winget-pkgs` yet (see [`packaging/winget`](packaging/winget/README.md)) |

[rc]: https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.1
[gh-cache]: https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy

---

## Usage

### Launch the TUI

```sh
bondebarras
```

On startup, bondebarras resolves a GitHub token (`gh auth token`, falling
back to `$GITHUB_TOKEN`), fetches the organizations it can see, and loads the
stage-1 overview: Actions cache totals and repository lists for each one.

The screen is three columns — organizations, the current org's
repositories, and the loaded repository's resources — that fold from the
**left** as the terminal narrows, so the resources column (where deletion
happens) is never the one squeezed or dropped: three columns from 100
terminal columns wide, two from 78 (the left one follows focus — orgs or
repos — with the hidden one recalled in the header), one below that (the
focused column alone, `←`/`→` change which one).

```
 ORGS                 DÉPÔTS                                 RESSOURCES
 ──────────────────── ─────────────────────────────────────  ──────────────────────────────
 systm-d      36.4 Go  josephine          5 j       ⚠12.4 Go  Cache   ████████████▓ 124 %
 SecondBrain… 14.9 Go  claudine          12 j       ⚠11.8 Go  Minutes ▓▓▓▓▓▓▓▓▓▓▓▓▓   0 %
 delfour-co    161 Mo  alertU     déjà archivé         8.0 Go ────────────────────────────
 exec-d         71 Mo  anonymous          3 j          4.3 Go [ ]⛑ cache v0-rust-cover…  467Mo PR#54 ⚑
                                                                [ ]• artif github-pages    1.1Mo 40j
                                                                [ ]  asset jos… (v0.12.0)  4.0Mo 2j
```

*(A sketch of the layout, not a screenshot — column widths not to scale.)*
The resources column opens with the two gauges above, for the repository it
shows; while a purge, an archive, or a repository load is running, a
progress row with a real, counted done/total appears between the status
line and the footer.

Resources are not fetched just because the cursor passes over a repository:
the load starts once the cursor **rests on it for 300 ms**, and the result
is kept for the rest of the session — revisiting a repository shows it
instantly, with no new request. `Entrée` forces an immediate (re)load,
skipping both the pause and the cache. While a repository's resources are on
the way, or failed, the column shows `(chargement…)` / `(échec du
chargement)` instead of an empty list, which would otherwise read as "this
repository holds nothing".

### Keyboard shortcuts (TUI)

Movement works from any column, in every layout:

| Key | Action |
| --- | --- |
| `←` / `→` | Previous / next column (wraps) — with one column on screen, changes which one is shown |
| `Tab` | Same as `→` |
| `↑` / `↓` | Move the cursor within the focused column |
| `Entrée` | Force an immediate reload of the repository under the repositories-column cursor, skipping the 300 ms pause and any cached listing |

`espace`, `A`, `V`, `s`, and `f` act **only in the column that has focus** —
the footer always shows which keys apply where. When the terminal is too
narrow for all of them, it keeps `[d]` first wherever `d` acts, then the
selection keys (`[espace]`, `[A]`, `[V]`), then the rest — `[d]` is announced
at every width from 60 columns:

| Key | Organizations | Repositories | Resources |
| --- | --- | --- | --- |
| `espace` | — | tick/untick *that* repository for archiving (one at a time) | check/uncheck the row under the cursor |
| `A` | — | — | select every ⛑ *safe* row |
| `V` | — | — | also select every • *worth-checking* row |
| `s` | — | — | cycle sort: size → age → name |
| `f` | — | — | enter filter mode (incremental match on the label) |
| `d` | — (not offered) | archive the ticked repository | delete the checked resources |

Either `d` opens the same confirmation modal, worded for what it is about to
do (archive or delete).

Everywhere:

| Key | Action |
| --- | --- |
| `y` / `N` | Confirm / cancel what's pending |
| `b` | Switch to the Billing tab (and back) |
| `←` / `→` (Billing tab only) | Move between months |
| `Esc` | Clear an active filter, or quit if there is none |
| `q` | Quit (twice, to confirm, while a purge is running) |

### CLI subcommands

With no subcommand, `bondebarras` opens the TUI. `scan` and `clean` cover the
same ground headlessly, for scripts and cron jobs; `update` — documented
below — keeps the tool itself current:

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

`scan --json` prints one object per organization: `org`, `cache_bytes`,
`cache_count`, `billing_readable` and `repos`, plus `plan` (the plan name, or
`null` when it cannot be read) and `minutes_allowance` (that plan's included
minutes, or `null` — never a default). `billing_month` names the current
month (`YYYY-MM`, in UTC), for which `storage_gbh` and
`storage_allowance_gbh` are given per organization and `storage_gbh` per
repository — `null` when billing or the plan cannot be read, never zero.
`budgets_readable`, `actions_budget` (`{"amount", "blocking"}`, or `null` when
the organization has no Actions budget) and `actions_sku_budgets` (`null`, not
`[]`, when budgets cannot be read) keep "no budget" and "unreadable" apart.
`artifact_retention_days` is the retention setting, or `null` when it cannot
be read.

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
make on its own — the repositories column's own tick (`espace`, in the
interactive TUI) is the only way in.

### Keeping bondebarras up to date

```sh
bondebarras update           # update, or say how to
bondebarras update --check   # only report whether a newer version exists
```

`update` asks GitHub Releases **on demand — never at startup** — and needs no
token: the repository is public, and a version check must not require
authentication. It then acts on the install channel it detects instead of
replacing the binary blindly: for a `.deb` or `.rpm` it runs the package
manager's own command, and for Homebrew, the AUR, Nix or `cargo install` it
*prints* the command and installs nothing, since overwriting a file that
manager owns would desynchronize its database. A downloaded asset is checked
against the release's published `.sha256` and refused if that checksum is
missing, unreadable, or disagrees — three distinct failures, not one
"proceed anyway". A local build newer than every published release is
reported as such, never as "up to date".

### Required token scopes

`repo`, `read:org`, `read:packages`, and `delete:packages` are enough for
everything bondebarras does but one optional display (see `admin:org`
below) — reading and deleting caches, artifacts, workflow runs, and
container package versions; listing the organizations and repositories a
token can see; and reading the Billing tab's usage report (a 403 there just
means the token's owner isn't an org owner — the org stays otherwise
navigable). Branches, tags, and release assets need no scope
beyond `repo`, already in that list — nothing new to grant for them.
Repository archiving needs no new scope either: it goes through the same
`repo`-scoped endpoint as everything else, and requires admin rights on the
repository itself — a right the token either has or doesn't, not a scope to
grant. Repository *deletion* is explicitly and permanently out of scope for
this tool, so `delete_repo` is never required.

Reading **budgets** is not a scope question: GitHub reserves the budgets
endpoint for organization admins and billing managers. Anyone else is refused
— observed as a 400, not the 403 the documentation announces — and the
Billing tab reads `Budget Actions : illisible` instead of guessing, while
`scan --json` reports `budgets_readable: false`. The rest of the tab, and the
organization itself, are unaffected.

`admin:org` is **optional**, and needed for one thing only: displaying an
organization's artifact and log retention, which GitHub only reveals to that
scope (or to the fine-grained "Actions policies" permission). bondebarras
never changes the setting. Without it, the Billing tab reads
`Rétention artefacts et journaux : illisible` and `scan --json` reports
`artifact_retention_days: null`; everything else works unchanged.

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
