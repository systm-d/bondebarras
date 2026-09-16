# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Three-column TUI**, replacing the split-pane tree: organizations, the
  current org's repositories, and the loaded repository's resources, each
  its own column, visible together on a wide terminal and folding from the
  **left** as it narrows (never the resources column) — three columns from
  100 terminal columns wide, two from 78, one below that.
- **Three-level safety marking** (`Resource.safety`) on every resource: ⛑
  *safe* (nothing live references it), • *worth checking*, or unmarked
  *keep* — computed once at scan time from the repository's own listings, at
  no extra request.
- **`[V]`**, alongside `[A]`: `[A]` selects every ⛑ row, `[V]` adds every •
  row too. Neither ever takes a protected resource, and the status line
  says how many such rows a press left unticked.
- **Two per-repository gauges**, at the head of the resources column: Actions
  cache usage against GitHub's documented (but API-unexposed) 10 GiB
  per-repository ceiling, and Actions minutes against the allowance of the
  organization's plan — `formule inconnue`, with no percentage, when the plan
  cannot be read — both uncapped past 100 %, since GitHub itself does not
  clamp there either.
- **Load-after-pause with a per-session cache**: a repository's resources
  load once the cursor rests on it for 300 ms, are kept for the rest of the
  session, and `Entrée` forces an immediate reload, bypassing both the pause
  and the cache. A repository whose listing is on its way, or failed, shows
  `(chargement…)` / `(échec du chargement)` instead of an empty list.
- **A progress row**, between the status line and the footer, while a purge,
  an archive, or a repository load runs — a real, counted done/total, never
  an estimate. Several purges running at once share one bar.
- `scan --json`: `plan` and `minutes_allowance` per organization, `null` when
  unknown. (#11)
- Billing tab: Actions storage, in GB-hours, against the plan's included
  storage (`free` 0.5 GB, `team` 2 GB, `enterprise` 50 GB) times the
  displayed month's hours — the base is written on the gauge — with the
  heaviest repositories named and a fixed line saying deleting artifacts
  stops the accumulation but refunds nothing already counted. Public
  repositories' storage is counted, and the tab says so. No extra request:
  the usage report loaded at stage 1 already carried it. Measured on
  2026-09-10: exec-d at 371.85 GB-hours in September, 359.88 of them in
  `disconnected`. (#13)
- Repositories column: a repository's GB-hours for the most recent month of
  the usage report, with that month, on a detail line under its row, and a
  ⚠ before a cache footprint past the included 10 GiB (10.7 Go as
  displayed). (#13)
- `scan --json`: `billing_month`, `storage_gbh` and `storage_allowance_gbh`
  per organization, `storage_gbh` per repository. (#13)
- Billing tab: the organization's Actions budget — its amount and whether it
  blocks — read at stage 1, and what it means past the allowance: Actions
  stopped at the allowance (0.00 $, as on exec-d), billed up to the budget
  then stopped (5.00 $, as on cloudalpes), or billed without a ceiling (no
  budget, as on SecondBrain-io). A gauge at 90 % or more with a blocking
  budget carries a warning, on the report's most recent month. A per-SKU
  Actions budget is named, not interpreted. Budgets that cannot be read — a
  400 on organizations the account does not own — read `illisible`, never "no
  budget". Read-only, permanently. (#14)
- `scan --json`: `budgets_readable`, `actions_budget` and
  `actions_sku_budgets`. (#14)
- Billing tab: each organization's artifact and log retention, beside the
  storage it governs, highlighted at 90 days or more when the organization
  holds at least 36 GB-hours that month. The tab states that a workflow's
  `retention-days` is bounded by this setting, and that a change only applies
  to new artifacts and logs — verified on 2026-09-10, when an APK uploaded the
  day before exec-d moved to 7 days kept its 2026-12-08 expiry. Read-only:
  nothing here changes the setting. (#15)
- `scan --json`: `artifact_retention_days`. (#15)

### Changed

- The footer now always opens with the movement keys (`←`/`→`, `↑`/`↓`)
  before the active column's own actions, so navigating the TUI no longer
  depends on discovering undocumented keys. When the terminal is too narrow
  for every action, it keeps `[d]` first wherever `d` acts, then the
  selection keys (`[espace]`, `[A]`, `[V]`), then the rest — `[d]` is
  announced at every width from 60 columns.
- `Entrée` no longer moves focus to the resources column nor clears the
  filter: it forces a fresh load of the repository under the repositories
  column's cursor, skipping the pause and the cache. A listing lands on its
  own time, so it leaves focus and the filter to the user's keys.
- The resources column's title labels its size as the ticked rows' —
  `cochés 0 o` — which it always was, unlabelled.
- **`[A]` now takes ⛑ (safe) rows rather than ⚑ rows** — the ⚑ stale-PR
  caches are among them, since a closed or merged PR always classifies as
  safe.
- `espace`, `A`, `V`, `s`, and `f` now act **only in the column that has
  focus**; `d` acts only from the column owning its plan (the resources
  column deletes, the repositories column archives the ticked repository).
  A key that used to reach the resource list from any pane no longer does,
  even when that list is off screen at a narrow width.
- Building from source now needs a C compiler alongside Rust. octocrab
  0.54 requires a JWT crypto backend even though bondebarras never signs a
  JWT; its default one pulls `rsa` (RUSTSEC-2023-0071, no fix available),
  so bondebarras selects `aws-lc-rs` instead. Precompiled packages are
  unaffected.
- Billing tab: the minutes gauge measures against the allowance of the
  organization's current plan — `free` 2,000, `team` 3,000, `enterprise`
  50,000 — read from `GET /orgs/{org}` at stage 1, instead of the Free plan's
  2,000 for every organization. Measured on 2026-09-10: exec-d (Team) read
  50 % for 1,004 minutes where 33 % is right; SecondBrain-io (Enterprise) read
  901 % for 18,016 where 36 % is right. An unreadable or unknown plan shows
  the total and `formule inconnue`, with no percentage anywhere. (#11)
- The Billing tab opens on the report's most recent month, not its oldest:
  `←` pages back to older months, `→` forward to newer ones. (#11)
- Token scopes: `admin:org` is optional — only the Billing tab's retention
  line needs it and reads `illisible` without it; reading budgets needs a
  role (organization admin or billing manager), not a scope. No scope joins
  the required list. (#14, #15)
- Billing tab: fits its content to the terminal's height instead of always
  reserving a fixed row budget for the two per-repository breakdowns —
  they shrink first, each keeping its `… et N autre(s)` line naming what
  it left out. Past that, the tab drops content from the bottom, one whole
  block at a time. The densest case — an `enterprise` organization, a
  blocking-budget warning under both gauges, a flagged retention and a
  runner SKU with no known multiplier — needs 33 rows to show everything;
  below that it loses the unknown-SKU line, then the cost line, then the
  retention notes.

### Fixed

- Billing tab: an organization that spent its Actions minutes on public
  repositories now reads why its allowance shows `0 / 3 000` beside a real
  bill — a public repository's runs are free and never counted against it.
  The tab showed both figures and explained neither, so it looked like it
  contradicted itself; it says this in the same words the resources column
  has used since #11.
- Resources column: `1 élément`, not `1 éléments`.
- Billing tab: the per-repository storage rows group their thousands like
  the gauge above them — `12 345.67 GB-h`, not `12345.67 GB-h`.
- Three-column TUI: growing the terminal back after making it shorter no
  longer leaves a column scrolled where the short frame had put it. With
  the cursor low in a long repositories list, 100x50 → 80x24 → 100x50 drew
  the list from its tenth row — hiding the only ⚠ one — with nineteen blank
  rows underneath and room for the whole list. The cursor stayed visible
  throughout, so nothing flagged it.
- Billing tab: a note written across two lines is no longer cut after its
  first line when the terminal is too short for it — it is shown whole or
  not at all, like every other multi-line explanation the tab carries. At
  80×24 the tab used to end on `Note : retention-days, dans un workflow,
  fixe la durée`, with the rest of that sentence gone.
- Billing tab: amounts are shown in US dollars (`6.15 $`), the currency of
  GitHub's usage report (`pricePerUnit` is 0.006 for Actions Linux). They were
  printed with a `€` sign — the right figure in the wrong currency. Nothing is
  converted: bondebarras has no exchange rate and does not invent one. (#12)

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
