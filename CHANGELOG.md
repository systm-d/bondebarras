# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **The Actions cache threshold is no longer presented as a fixed ceiling.**
  10 GiB is GitHub's *default included* threshold per repository: an
  authorized administrator can raise the real limit, usage above it may be
  billed, and eviction starts only once a repository reaches its
  **configured** limit — a figure no endpoint exposes. The cache gauge's
  caveat now reads `(seuil inclus par défaut ; limite réelle non exposée par
  l'API)` in place of `(plafond GitHub, non exposé par l'API)`, and a gauge
  past 100 % warns `⚠ dépasse le seuil inclus : au-delà, GitHub facture le
  stockage ou évince les caches les moins récemment lus, selon la limite
  configurée du dépôt` instead of asserting that eviction was already under
  way. `gauges::CACHE_CEILING_BYTES` and `gauges::cache_over_ceiling` became
  `CACHE_INCLUDED_BYTES` and `cache_over_included`, so the mistake cannot be
  read back out of the code; the README and both landing pages say the same.
- **Only install channels that actually exist are advertised.** The README
  and both landing pages showed `yay -S bondebarras`, `brew install
  bondebarras` and `winget install bondebarras` as runnable commands. None
  of the three is published: the release workflow skips all of them for a
  pre-release tag, on purpose. Installation is now given in two levels —
  what works today (the release's binaries and packages, and `cargo install
  --git`) and what comes after the first stable release, each marked *not
  published*. The `.deb` and `.rpm` instructions now say where the file
  comes from before showing `dpkg -i`, which installs a file already
  downloaded rather than fetching one.
- **Removed `Formula/bondebarras.rb`.** It carried an all-zero sha256 and a
  URL for a tag whose formula the release workflow deliberately never
  publishes, so it looked installable and was not. The workflow regenerates
  it on a stable tag from [`packaging/homebrew/bondebarras.rb`](packaging/homebrew/bondebarras.rb),
  which is unchanged, so nothing is lost by deleting it now.

## [1.0.0-rc.1] - 2026-09-17

First release candidate, and the only entry in this file: it describes
bondebarras as it stands, not a step away from anything earlier.

### Added

- **Two-stage scan** across every organization the token can see: the
  org-level aggregates — cache totals, repository lists, plan, billing —
  land in seconds, and a repository's own resources are fetched only when
  you drill into it, so requests are spent on what you actually look at.
- **Three-column TUI** — organizations, the current org's repositories, and
  the loaded repository's resources — visible together on a wide terminal
  and folding from the **left** as it narrows, never the resources column
  where deletion happens: three columns from 100 terminal columns wide, two
  from 78, one below that.
- **Actions caches, artifacts and workflow runs**, deletable one row at a
  time or in bulk. Tier 1: a re-run regenerates them, so they are confirmed
  with a bare `[y/N]`. Measured on the author's own account: **51.4 GB** of
  Actions caches, a single repository holding **69 caches for 11.1 GB**.
- **⚑ stale-PR flag** — every cache is checked against the repository's
  closed pull requests, and one pinned to a closed or merged PR is flagged
  and marked ⛑ safe: nothing can ever read it again.
- **Container package versions** (GHCR): untagged layers and orphaned
  attestations — a signature tagged `sha256-<digest>` whose signed image is
  gone. The confirmation names the one risk GitHub's API cannot rule out: an
  untagged version may still be a layer of a multi-architecture image, and
  deleting it would break the parent manifest.
- **Merged branches, tags and release assets.** A branch is offered dead
  **only because a pull request merged it**, at zero extra requests — the
  closed-PR listing the ⚑ flag already fetches carries `head.ref` and
  `merged_at`, so there is no `compare` call per branch. A PR closed
  *without* merging leaves its branch alone: the work may still be resumed.
  The release itself is never deletable, only its assets — a release is a
  point in the repository's history, and its weight is entirely in what is
  attached to it. Measured across four organizations: **7.3 GB in release
  assets**, led by `exec-d/terminus` (1,453 MB across 25 releases) and
  `delfour-co/githero` (1,371 MB across 27).
- **Repository archiving** — the one candidate that lives in the
  repositories column rather than the resource list, ticked one row at a
  time and archived through the same confirmation-and-execute path as every
  deletion. The row shows its age (`775 j`) when it is a genuine candidate,
  or its class (`déjà archivé`, `sans droits`) when it is not. Measured
  across five organizations: a dozen repositories with no push in 500 to
  775 days — `maxds-lyon/.github` at 775, `maxds-lyon/lokiprint` at 685 —
  and exactly **one** already archived.
- **Three-level safety marking** (`Resource.safety`) on every resource: ⛑
  *safe* (nothing live references it), • *worth checking*, or unmarked
  *keep* — computed at scan time from the repository's own listings, at no
  extra request.
- **`[A]` and `[V]`**: `[A]` selects every ⛑ row, `[V]` adds every • row
  too. Neither ever takes a protected resource, and the status line says how
  many such rows a press left unticked.
- **Tiered confirmation** before anything happens: a bare `[y/N]` for the
  regenerable Tier 1, an itemised recap plus an explicit irreversibility
  warning for Tier 2 — package versions, branches, tags and release assets
  do not come back. Archiving sits in Tier 2 for the opposite reason: it is
  the one operation here GitHub can undo, and its modal never reuses the
  deletion wording, since claiming a reversible action is permanent would be
  as much a lie as the reverse.
- **Two per-repository gauges** at the head of the resources column: Actions
  cache usage against GitHub's **default included** 10 GiB per-repository
  threshold — not a ceiling, and the gauge says so: the real limit can be
  raised, usage above it may be billed, and eviction starts only at the
  repository's configured limit, which no endpoint exposes — and Actions
  minutes against the allowance of the organization's plan — `formule
  inconnue`, with no percentage, when the plan cannot be read. Neither is
  clamped past 100 %, since GitHub does not clamp there either.
- **Load-after-pause with a per-session cache**: a repository's resources
  load once the cursor rests on it for 300 ms, are kept for the rest of the
  session, and `Entrée` forces an immediate reload, bypassing both the pause
  and the cache. A listing on its way, or failed, reads `(chargement…)` /
  `(échec du chargement)` rather than an empty list.
- **A progress row** between the status line and the footer while a purge,
  an archive or a repository load runs — a real, counted done/total, never
  an estimate. Several purges running at once share one bar.
- **Deletions run in the background**, spaced out and retried on GitHub's
  secondary rate limit (`Retry-After` on 429 and 403), so a purge of a
  hundred-plus caches is not throttled away. Each item reports its own
  outcome, and a run that ends with failures says so instead of hiding it;
  quitting mid-purge warns once and requires a second press.
- **Billing tab** (`b`), month by month (`←`/`→`), opening on the usage
  report's most recent month:
  - **Actions minutes** against the allowance of the organization's current
    plan — `free` 2,000, `team` 3,000, `enterprise` 50,000 — read from
    `GET /orgs/{org}` and never guessed: an unreadable or unknown plan shows
    the total and `formule inconnue`, with no percentage anywhere. The gauge
    counts private repositories only and says so, a public repository's runs
    being free whatever their volume; on `enterprise` the allowance belongs
    to the enterprise account and is shared, so the percentage is a minimum.
    `SecondBrain-io`'s `monolith-back` burnt **24,632 private
    Linux-equivalent minutes in July 2026** — the kind of runaway usage the
    tab exists to surface, since minutes cannot be reclaimed after the fact.
  - **Actions storage**, in GB-hours, against the plan's included storage
    (`free` 0.5 GB, `team` 2 GB, `enterprise` 50 GB) times the displayed
    month's hours — the base is written on the gauge — with the heaviest
    repositories named and a fixed line saying that deleting artifacts stops
    the accumulation but refunds nothing already counted. Public
    repositories' storage *is* counted, and the tab says so. Measured on
    2026-09-10: exec-d at 371.85 GB-hours in September, 359.88 of them in
    `disconnected`.
  - **The organization's Actions budget**, and what it means past the
    allowance: Actions stopped at the allowance (0.00 $, as on exec-d),
    billed up to the budget then stopped (5.00 $, as on cloudalpes), or
    billed without a ceiling (no budget, as on SecondBrain-io). A gauge at
    90 % or more with a blocking budget carries a warning, on the report's
    most recent month — the only one GitHub can still block. A per-SKU
    budget is named, not interpreted. Budgets that cannot be read — a 400 on
    organizations the account does not own — read `illisible`, never "no
    budget". Read-only, permanently: changing one commits money.
  - **Artifact and log retention**, beside the storage it governs,
    highlighted at 90 days or more when the organization holds at least
    36 GB-hours that month. The tab states that a workflow's
    `retention-days` is bounded by this setting, and that a change only
    applies to new artifacts and logs — verified on 2026-09-10, when an APK
    uploaded the day before exec-d moved to 7 days kept its 2026-12-08
    expiry. Nothing here changes the setting.
  - Amounts are in US dollars (`6.15 $`), the currency of GitHub's usage
    report. Nothing is converted: bondebarras has no exchange rate and does
    not invent one.
  - The tab fits its content to the terminal's height instead of reserving a
    fixed row budget: the two per-repository breakdowns shrink first, each
    keeping its `… et N autre(s)` line naming what it left out, and past
    that the tab drops content from the bottom, one whole block at a time.
    The densest case — an `enterprise` organization, a blocking-budget
    warning under both gauges, a flagged retention and a runner SKU with no
    known multiplier — needs 33 rows to show everything; below that it loses
    the unknown-SKU line, then the cost line, then the retention notes. A
    note is shown whole or not at all, never cut after its first line.
- **The repositories column** carries a repository's GB-hours for the most
  recent month of the usage report, with that month, on a detail line under
  its row, and a ⚠ before a cache footprint past the included 10 GiB (10.7 Go
  as displayed).
- **`bondebarras scan --json`** — one object per organization on stdout,
  progress and diagnostics on stderr: `org`, `cache_bytes`, `cache_count`,
  `billing_readable`, `plan`, `minutes_allowance`, `billing_month`,
  `storage_gbh`, `storage_allowance_gbh`, `budgets_readable`,
  `actions_budget`, `actions_sku_budgets`, `artifact_retention_days` and
  `repos`. A figure the API did not give is `null`, never a default;
  `actions_sku_budgets` is `[]` for a readable organization with no budget
  and `null` when budgets are unreadable, so the two stay apart.
- **`bondebarras clean`** — unattended cleanup, for a cron job: `--org`,
  `--repo`, the `--caches` / `--artifacts` / `--runs` / `--packages` /
  `--branches` / `--tags` / `--assets` family flags, `--stale-pr`,
  `--older-than` and `--yes`. Without `--yes` it prints the plan and deletes
  nothing; naming no family selects nothing either.
- **`bondebarras update`** — checks GitHub Releases on demand, never at
  startup, and without a token: the repository is public, and a version
  check must not require authentication. It acts on the install channel it
  detects rather than replacing the binary blindly — it runs the package
  manager's own command for a `.deb` or `.rpm`, and only *prints* the
  command for Homebrew, the AUR, Nix or `cargo install`, where overwriting a
  managed file would desynchronize that manager's database. `--check`
  reports availability and installs nothing, and a local build newer than
  every published release is reported as such, not as "up to date".

### Security

- `bondebarras update` checks a downloaded asset against the release's
  published `.sha256` sidecar and **refuses on all three failure modes**,
  kept distinct rather than collapsed into a single "proceed anyway": no
  checksum published at all, a checksum that could not be fetched or parsed,
  and a checksum that disagrees with the file. The release workflow
  publishes one `.sha256` per artifact, so the verification fails closed
  rather than open.

### Note on sizing

**GitHub exposes no size for a package version**, under any field name, and
no billing SKU covers package storage either. That family therefore reports
no bytes: the resource list shows `—` instead of formatting a zero, with a
header line saying why, and nothing here should be read as a volume feature.
It is a hygiene cleanup, measured in versions: across the author's fifteen
organizations, **7 packages, 45 versions, 23 of them untagged** — on one
organization alone, 20 of its 28 versions carry no tag at all.

**Archiving frees no bytes** either, and the tool says so wherever it
matters. It earns its place because an archived repository has its Actions
disabled, so it stops *producing* the caches, artifacts and workflow runs
every other family here cleans up: closing the tap rather than mopping the
floor forever.

### Note on refusals

- `Resource.protected` is refused in bulk, unconditionally: a tagged package
  version (`latest`, and any other real tag), a live branch (the default
  one, GitHub-protected, or simply with no merged PR behind it), and every
  tag — what a release, a `go get` or a `Cargo.toml` points at by name. The
  guard lives on the resource rather than in a caller's discipline, because
  a cron job has no human at the other end to notice a broken deployment.
  Individual selection (`espace`, in the TUI) is unaffected.
- **A repository is never preselected, and `clean` never archives one at
  all.** `pushed_at` is not proof of abandonment — a finished, stable
  library can go years without a push — so archiving has no headless path
  whatsoever: there is no `--archive` flag, and none is planned. An
  already-archived repository, or one this token cannot administer, is not
  individually tickable either.
- Repository **deletion** is permanently out of scope, so `delete_repo` is
  never required. Nothing this tool deletes is reversible on GitHub's side,
  and it never pretends otherwise: no trash, no undo.

### Note on scopes

`repo`, `read:org`, `read:packages` and `delete:packages` cover everything
bondebarras does, including the Billing tab's usage report — a 403 there just
means the token's owner is not an org owner, and the organization stays
navigable. Branches, tags, release assets and repository archiving need
nothing beyond `repo`: archiving is gated by admin rights on that one
repository, not by a scope to grant. `admin:org` is **optional** and needed
for one display only — artifact and log retention, which reads `illisible`
without it. Reading budgets is not a scope question at all: GitHub reserves
that endpoint for organization admins and billing managers.

### Note on building from source

Rust **1.88** (edition 2024) and a C compiler are required. octocrab 0.54
wants a JWT crypto backend even though bondebarras never signs a JWT, and
its default one pulls `rsa` (RUSTSEC-2023-0071, no fix available), so
bondebarras selects `aws-lc-rs` instead. Precompiled packages are unaffected.
