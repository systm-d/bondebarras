# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **`--help` names the binary `bondebarras` on all three platforms** (#55,
  second review). clap defaults its `bin_name` to `argv[0]`, which Windows
  spells `bondebarras.exe`, so `Usage:` carried that suffix there and only
  there — and `docs/cli.md`, a page that quotes one rendering and claims it
  is the binary's own output, was literally true on two platforms out of
  three. The name is pinned on the parser now, and `bondebarras` is in any
  case the invocation that works, under PowerShell as under `cmd`. The
  normalisation the help guard carried for that suffix is gone with it: it
  was global and anchored on nothing, so forcing the suffix to `" [COMMAND]"`
  and deleting that token from the page left the test green. A guard that
  patches the binary's output into the shape the page expects is not a guard.
- **The Homebrew formula is proposed, never pushed** (#35). The `homebrew` job
  committed `Formula/bondebarras.rb` straight onto the default branch — this
  repository being its own tap, that meant writing without review or recourse
  into the very file `brew install` serves, and the "main protected" rule
  having no bypass actor, the push would have failed at the first `v1.0.0`
  anyway. The formula is now opened as a pull request from a branch named
  after the tag, so a `v1.0.1` released before `v1.0.0` is merged cannot
  rewrite a pull request a reviewer is reading. Merging stays a human
  decision. The job also stopped undoing human gestures on a re-run: the
  branch is never force-pushed — a hand-pushed correction survives, and a
  branch that exists without carrying the formula rendered here stops the job
  rather than being overwritten — and `gh pr list` is read `--state all`, so
  a pull request closed *without* merging, which is a refusal, is no longer
  answered with a second one. A re-run finding its pull request open or
  merged exits green; one finding it refused exits red, that red being the
  only visible trace that the tap did not move.
- **Two runs of the same tag no longer cross** (#35, review). The release
  workflow takes a `concurrency` group keyed on the ref, so a re-run queues
  behind the run it would otherwise race — both could read "no pull request
  exists" at the same moment and both push. The group is keyed on the ref
  rather than on the workflow on purpose: two tags each have their own
  `homebrew/<tag>` branch and are built to coexist, and a shared group would
  cancel a queued release outright, which costs more than it saves.

### Fixed

- **Windows is tested, and `update` knows how to serve it** (#55). Windows was
  the only one of the three platforms `CLAUDE.md` promises that nothing
  verified and that `update` could not serve. CI now runs the test matrix on
  `windows-latest` too — **six targets, not five** — so the `windows-x86_64`
  branch of `current_target_spells_the_platform_as_the_release_workflow_does`
  is finally executed rather than merely green. And `detect_channel_for` now
  recognises a Windows install instead of answering `Unknown`: separators are
  normalised before any pattern is matched, so `C:\Users\…\.cargo\bin\` reads
  as Cargo; a winget install reads as winget in **both** scopes; and anything
  else on Windows reads as the manual-archive channel, served the
  `bondebarras-windows-x86_64.zip` every release has published all along. A
  Windows user was previously told `Impossible de déterminer comment
  bondebarras a été installé` while standing in front of a release page
  holding an archive for their exact machine — the same class of lie #49
  corrected for platforms: saying "I don't know" when the information is
  right there.
- **A machine-scope winget install no longer escapes that detection** (#55,
  review). `%PROGRAMFILES%\WinGet\` carries no `Microsoft\` segment —
  winget's `portablePackageMachineRoot` default is
  `%PROGRAMFILES%/WinGet/Packages/`, against
  `%LOCALAPPDATA%/Microsoft/WinGet/Packages/` for the user scope — so
  matching `Microsoft/WinGet/` alone classified every machine-wide install as
  a manual archive. That invited the user to swap the `.exe` by hand while
  winget's database went on naming the version it installed: the exact
  desynchronisation detecting winget at all exists to prevent, arrived at
  from the other side. Both the package root and the `Links` shim directory
  are now matched, in either scope.
- **The update messages are held by tests, not only the classification that
  produces them** (#55, review). Three messages could be rewritten into
  falsehoods with the whole suite staying green: the winget note could
  promise `winget upgrade`, a command that finds nothing; the Windows archive
  note could lose the reserve that a running `.exe` cannot be overwritten;
  and the packageless archive arm could borrow the `Impossible de déterminer`
  sentence. Each is pinned now — the last as an equivalence over every
  channel at once, so no arm can borrow that sentence back. `Tarball` had in
  fact borrowed it, and now says what it knows. It also gained the Unix
  counterpart of the Windows reserve, worded for the fact that actually holds
  there: a `cp` over a running binary fails on `ETXTBSY`, while a rename over
  it succeeds. Reusing the Windows sentence would have been false, and saying
  nothing left that failure unexplained.
- **A Windows path is no longer classified by a Unix-only pattern** (#55,
  review). `linuxbrew` and `Cellar` were matched ahead of the platform
  branch, so `C:\Cellar\…` answered `Homebrew` and would have pointed a
  Windows user at a tap that publishes nothing. Those two patterns are gated
  on the operating system now; cargo's and winget's directories stay ungated,
  being spelled the same wherever they appear.
- **Two path patterns that matched too much, and one that matched too
  little** (#55, second review). `…\Downloads\winget\Links\` was read as a
  winget install, because the machine-scope directories were searched for
  anywhere in a path rather than under a Program Files root — its owner would
  have been told to uninstall through a winget that never installed them. And
  `C:\…\.CARGO\bin\` was *not* read as a cargo install, that one pattern
  being compared case-sensitively while Windows compares paths
  case-insensitively. Case is now folded where the filesystem folds it and
  nowhere else, so `/home/x/.CARGO/` on Linux stays somebody else's
  directory. Two winget layouts remain outside the detection by decision — a
  portable root redirected through winget's `settings.json`, and a binary
  left in the bare machine root — and are written down in
  `docs/installation.md` rather than guessed at.
- **`docs/releases.md`'s promise now covers the three channels it names**
  (#55, second review). The page swears `update` never prints a command that
  could not succeed, for Arch, macOS and winget; only the winget message was
  held by a test. Rewriting the Homebrew one into « Lancez `brew upgrade
  bondebarras` » left the suite green, as did turning the packageless archive
  arm into « Votre bondebarras est déjà à jour ». Each is pinned now, and the
  page cites the tests. Nix stays unpinned on purpose: its message names no
  command at all, only the reader's own flake input or channel.
- **A release no longer renders a recipe with `sed`, nor checksums an empty
  stream** (#35). In a `sed` replacement `&` means the matched text and `|`
  ends the command, and git accepts both in a tag name: the `url` line came
  out wrong with the job still green. Both recipes are rendered line by line
  now, the substituted value never being reinterpreted, rather than escaping
  every metacharacter of every `sed`. And the release-asset render had neither
  `shell: bash` nor `pipefail`, so a failing `curl` left `sha256sum` reading
  an empty stream and producing a perfectly well-formed checksum — the one for
  zero bytes — which shipped in both recipes with the job green. The `winget`
  job carried that same defect in its `InstallerSha256`, and is closed the
  same way. Nothing re-read the rendered recipes either: the two substituted
  fields are now compared exactly *and* counted, and the result passed through
  `ruby -c` (`bash -n` for the `PKGBUILD`). The two checks catch each other's
  blind spot — a reindented template yields a formula with its placeholder
  intact, which is valid Ruby; a tag carrying a quote closes the string while
  both fields still match.

### Security

- **A tag name can no longer inject code into a rendered recipe** (#35,
  review). Git accepts in a tag name everything it does not refuse by name,
  so backticks, `$`, `;`, quotes and `#{…}` are all legal. A tag
  `v1.0.0#{…}` rendered a formula whose `url` line `grep -cxF` counted and
  `ruby -c` validated — and which Ruby interpolates when `brew` evaluates it,
  on the reader's own machine; a tag `v1.0.0;id` rendered a `pkgver=1.0.0;id`
  that `bash -n` accepts and `makepkg` runs. Escaping per destination was
  refused as a fix: these values land in Ruby, in shell and in YAML, and the
  next destination added would start out uncovered. The tag name is
  constrained on the way in instead, before anything is rendered, to the shape
  this project actually uses — `v<major>.<minor>.<patch>`, optional
  `-<pre-release>`, no build metadata — in all three jobs that substitute it.
  Anything else fails the job loudly and renders nothing. It also makes the
  pre-release test the workflow already relied on ("the tag contains a `-`")
  exact rather than approximate.
- **A missing `ruby` now fails the release instead of waving the formula
  through** (#35, review). The formula's syntax check sat behind
  `command -v ruby`, whose else-branch printed a note on stdout and left
  `fautes=0`: on a runner without ruby, an invalid formula would have gone out
  green, and been proposed for merge into the file `brew install` serves. A
  check that silently becomes a non-gesture is worse than no check — it
  manufactures the confidence that someone looked. The `PKGBUILD` never had
  the defect: its `bash -n` runs under the bash already executing the step and
  cannot be missing.

## [1.0.0-rc.3] - 2026-09-20

### Fixed

- **`update` no longer offers another platform's archive** (#49).
  `ReleaseInfo::asset_for` matched a release asset on the install channel's
  suffix alone, and a release publishes two `.tar.gz` — `linux-x86_64` and
  `macos-aarch64`. Whichever GitHub listed first won, so a macOS user on a
  manual install could be handed the Linux archive: a download that passes
  its checksum and then cannot execute. The asset is now filtered on the
  running target as well, spelled exactly as `release.yml` spells it
  (`linux-x86_64`, `macos-aarch64`, `windows-x86_64`) — the workflow already
  names its targets the way Rust does, so `std::env::consts` composes the
  token with no translation table to drift. When nothing matches,
  `update` **refuses and says so** rather than falling back on another
  build: naming the detected platform is the one fact the user cannot check
  for themselves in front of a release page visibly full of archives. The
  refusal is worded per channel — a `.deb` or `.rpm` carries no target in
  its name, so for those it says the release publishes no such package,
  instead of accusing the platform of a gap it did not cause. Injecting the
  target also makes the rule testable: CI builds on five platforms, and a
  test keyed on the host's own target would assert something different on
  each.
- **A workflow run no longer reads `0 o`** (#41). `api::runs::list` hardcodes
  `size_bytes: 0` because GitHub reports no size for a run anywhere — no size
  field on the run object under any name, billable *milliseconds* (not bytes)
  from `GET .../actions/runs/{id}/timing`, and a bare redirect from
  `.../logs` — but `ResourceKind::WorkflowRun.has_known_size()` still
  answered `true`, so the size column printed a confident `0 o` over a figure
  nobody ever measured, and `model`'s own doc comment promised "a real
  GitHub-reported number" for the family. The same fault as the cache ceiling
  fixed in rc.2: an invented value presented as a measured one. The run now
  joins the package version, the branch and the tag as a sizeless kind:
  every screen shows `—` — the resources column and the headless `clean`
  dry-run listing alike — a plan made only of runs summarises as `taille
  inconnue` instead of `0 o`, and a purge of only runs reports its count
  rather than `0 o libérés`. The resources column's own title follows the same
  rule: ticking runs alone reads `cochés —` rather than summing their
  placeholder zeros into a `cochés 0 o` that stood above a column of `—` and
  contradicted the confirmation modal on the same screen; a selection holding
  one sized row still shows the bytes it does know.
  Deleting a run still frees space, since its logs
  and artifacts go with it; GitHub simply never says how much, and the
  artifacts it drops are listed and sized in their own right. Consequence
  worth stating plainly: a listing holding runs — nearly every repository —
  now carries the `⚠ GitHub n'expose pas la taille de certaines ressources`
  banner that has always accompanied `—`.
- **The TUI states GitHub's seven-day cache rule** (#37). Every cache entry
  not read for over 7 days is deleted regardless of any limit. That rule was
  in the README, both landing pages, the CHANGELOG and the code's own
  comments, and nowhere in the interface — so the over-threshold warning's
  clause about eviction waiting for the repository's configured limit read,
  out of context, as a universal statement about eviction. It is not: the
  configured limit governs eviction *to make room*, not the age sweep, which
  waits for nothing. The warning now names which eviction the limit governs
  and states the rule that ignores it: `⚠ dépasse le seuil inclus : le
  stockage en excès est facturé ; l'éviction pour faire de la place, elle,
  attend la limite configurée du dépôt ; et, indépendamment de toute limite,
  toute entrée non lue depuis plus de 7 jours est supprimée`. It rides on the
  line drawn only past the threshold, never on the caveat drawn at every
  usage, so **the banner at rest is unchanged, row for row** — the price is
  paid only by a gauge that was already warning: 5 rows to 7 at the resources
  column's 58-cell inner width, 7 to 10 at its narrowest 38.

### Changed

- **`CONTRIBUTING.md` no longer restates the quality gate** (#28). It links
  to `CONVENTIONS.md`, the single source of truth. The two copies had
  drifted — the clippy command in `CONTRIBUTING.md` had lost `--all-targets`,
  so a contributor following it ran a narrower lint than CI does. Checked
  against `.github/workflows/ci.yml` on the way past, since the workflow is
  what actually gates a merge: `CONVENTIONS.md` now records that CI runs
  `cargo test --workspace --locked` across a five-target matrix, that the
  release build lives in `release.yml` rather than `ci.yml` — per target and
  per binary, so the workspace-wide release build stays a local-only check —
  and that CI also runs `cargo audit` and `cargo deny check`.
  `.github/PULL_REQUEST_TEMPLATE.md` held the third copy, and the most harmful
  one since it is ticked at every PR: its clippy box also lacked
  `--all-targets`. It now links to the same section instead of restating two
  commands.

## [1.0.0-rc.2] - 2026-09-17

### Fixed

- **The Actions cache threshold is no longer presented as a fixed ceiling.**
  10 GB is GitHub's *default included* threshold per repository: an
  authorized administrator can raise the real limit, and storage above it is
  billed. Eviction *to make room* starts only once a repository reaches its
  **configured** limit — a figure no endpoint exposes — and, independently of
  any limit, GitHub removes every cache entry that has not been accessed in
  over 7 days. The cache gauge's caveat now reads `(seuil inclus ; limite
  réelle non exposée par l'API)` in place of `(plafond GitHub, non exposé par
  l'API)`, and a gauge past 100 % warns `⚠ dépasse le seuil inclus : le
  stockage en excès est facturé ; l'éviction, elle, attend la limite
  configurée du dépôt` — billing and eviction as two separate facts, since
  billing does not depend on the configured limit and eviction does — instead
  of asserting that eviction was already under way.
  `gauges::CACHE_CEILING_BYTES` and `gauges::cache_over_ceiling` became
  `CACHE_INCLUDED_BYTES` and `cache_over_included`, so the mistake cannot be
  read back out of the code; the README and both landing pages say the same,
  and link GitHub's own [usage limits and eviction policy](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching#usage-limits-and-eviction-policy).
- **The cache gauge measures against the decimal 10 GB GitHub bills on**
  (`10_000_000_000`), not 10 GiB (`10_737_418_240`). The binary basis
  flattered a repository GitHub was already charging: one holding 10.5 GB
  read 98 % and went unflagged. Percentages move with it — the sample
  repository at 12.36 Go reads **124 %**, not 115 % — and the figures now
  read `12.4 Go / 10 Go` rather than `/ 10 Gio`, matching `model::human_size`,
  which has always formatted in decimal units because GitHub's own billing UI
  does. The repositories column's ⚠ moves with it too, marking from 10.0 Go
  instead of 10.7.
- **Only install channels that actually exist are advertised.** The README
  and both landing pages showed `yay -S bondebarras`, `brew install
  bondebarras` and `winget install bondebarras` as runnable commands. None
  of the three is published: for Homebrew and winget the release workflow
  skips the step on a pre-release tag, on purpose; for the AUR there is no
  job at all, and no package was ever submitted. `bondebarras update` no
  longer points at them either — on Arch it names the release's `PKGBUILD`
  and `makepkg -si`, and on macOS the release page, instead of printing
  `yay -S bondebarras` and `brew upgrade bondebarras`, neither of which could
  succeed. Installation is now given in two levels —
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
- **The pre-release status is stated where people actually look.** The
  README opens on a `v1.0.0-rc.1 — pre-release` badge and a note saying what
  that means — feature-complete and safe to run, but the CLI flags and the
  `scan --json` schema may still change before `v1.0.0` — and the
  Installation section says it again. The site hero carries the same version
  and note, in both languages. Nothing now implies a stable release exists.

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
  cache usage against GitHub's **default included** 10 GB per-repository
  threshold — not a ceiling, and the gauge says so: the real limit can be
  raised, storage above it is billed, and eviction to make room starts only
  at the repository's configured limit, which no endpoint exposes (any entry
  unread for over 7 days goes regardless) — and Actions minutes against the
  allowance of the organization's plan — `formule inconnue`, with no
  percentage, when the plan cannot be read. Neither is clamped past 100 %,
  since GitHub does not clamp there either.
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
  its row, and a ⚠ before a cache footprint past the included 10 GB (10.0 Go
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
