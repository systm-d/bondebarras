# bondebarras — project guide for AI agents & contributors

TUI/CLI to audit and clean up GitHub organization resources: Actions caches,
artifacts, workflow runs, container package versions, merged branches, tags,
release assets, and (since v0.5) repository archiving, across every org a
token can see. Rust workspace: `bondebarras-core` (library: pure logic, the
`api/` boundary, the CLI parser and the TUI) + `bondebarras` (binary, thin
shim).

## Read first

1. [CONVENTIONS.md](CONVENTIONS.md) — shared standards (edition, fmt, lints,
   commits, license, language policy).
2. [docs/superpowers/specs/](docs/superpowers/specs/) — feature specs and
   design docs for each phase.
3. [README.md](README.md) — features, installation, usage.

## Product rules

- **Multi-platform**: Linux, Windows, macOS. bondebarras only talks to the
  GitHub REST API over HTTPS — nothing may assume a Linux-only environment.
- **`api/` is the only module that knows `octocrab`.** Some endpoints (cache
  usage, cache list/delete) have no typed surface in octocrab and are read
  through `Client::get_json` / `Client::delete` instead; the rest of the
  crate never learns which responses are typed and which are deserialized by
  hand.
- **The risk tier is carried by the type, not the UI.** `model::risk_tier`
  is an exhaustive `match` over `ResourceKind`: a destructive kind added
  without a tier assigned does not compile.
- **No trash, no undo — except archiving, which the tool says is different in
  as many words.** Nothing GitHub lets us *delete* is reversible, so the tool
  never promises otherwise. Tier 1 (caches, artifacts, workflow runs) is
  regenerable by a re-run and gets a bare confirmation; Tier 2 (package
  versions since v0.3; merged branches, tags, and release assets since v0.4)
  does not come back, so its modal lists the items and says so plainly — see
  `tui::views::confirm::modal_kind`. Repository archiving (v0.5) is also
  Tier 2, but for the opposite reason: it is the one operation this tool
  performs that *is* reversible on GitHub's side (un-archiving restores it),
  which is exactly what keeps it off Tier 3 — see `model::risk_tier`'s own
  doc comment. Its confirmation modal never reuses the deletion wording
  (`tui::views::confirm::archive_warning_line`, `prompt_line`'s `is_archive`
  flag): claiming a reversible action is permanent would be as much a lie as
  the reverse.
- **Repository archiving frees no bytes, and that is stated plainly
  everywhere it matters — not glossed over.** It earns its place because an
  archived repository has its Actions disabled, so it stops *producing* the
  caches, artifacts and workflow runs every other family here cleans up:
  closing the tap rather than mopping the floor forever. `ResourceKind::
  Repository.has_known_size()` is `false`, same as a package version, a
  branch or a tag.
- **A repository is never auto-selected, and headless never archives one, at
  all.** `pushed_at` alone is not proof of abandonment, so unlike every
  other family, archiving has **no preselection path whatsoever**:
  `tui::app::App::select_safe` (`[A]`) and `select_safe_and_check` (`[V]`)
  both exclude `ResourceKind::Repository` explicitly, and
  `commands::clean::select` returns `false` for it unconditionally — no
  `--archive` flag exists, or is planned. This is the first time the product
  refuses an entire resource *family* headlessly, not just a tier or a
  single protected instance. An already-archived repository, or one this
  token cannot administer, is not individually tickable either
  (`tui::app::App::toggle_repo_selected`) — a harder refusal than a
  protected tag or a live branch, which stay tickable one row at a time.
- **The repository is the one candidate that lives in its own column, not
  the resource list.** `model::RepoSummary` carries its own `age_days` and
  `repos::RepoClass` (`Archivable` / `AlreadyArchived` / `NoAdminRights`),
  rendered by `tui::views::repos::repo_row_spans` as either an age (`"775
  j"`) or the class name — the same "classification replaces the age" shape
  a `Branch` row already has.
- **A release is never deletable — only its assets are, permanently out of
  scope beyond that.** `ResourceKind` has no `Release` variant and never
  will: a release is a point in the repository's history (a tag, notes, a
  date), and its weight is entirely in whatever binaries are attached to it.
  `api::releases::assets` flattens every release's `assets[]` into rows,
  carrying the release's tag along in the label since the release itself
  never becomes a row of its own.
- **Package versions carry no size, ever.** GitHub's API exposes no size
  field for a package version, under any name, and no billing SKU covers
  package storage either. `Resource.size_bytes` is hardcoded to `0` for
  `ResourceKind::PackageVersion`; never estimate or extrapolate one. The TUI
  shows `—` instead of formatting that zero, with a header line spelling out
  why — see `tui::views::repo::column_head`.
- **A purge's progress messages are tagged with the repository its plan
  concerns, recorded at launch.** `tui::spawn_purge` labels every message it
  forwards on the purge channel with `(plan.owner, plan.repo)`, since
  `clean::Progress::Finished` itself names no repository and the cursor can
  be anywhere by the time it lands. `apply_progress` reads that tag, not the
  cursor, to decide whose cached listing a finished purge forgets
  (`App::purge_ended`) — `clean.rs` stays unaware that columns or a cache
  exist at all.
- **`Resource.protected` is refused in bulk, unconditionally.** A tagged
  package version (`latest`, and any other real tag), a live branch (the
  default one, GitHub-protected, or simply with no merged PR behind it), and
  every tag all set it; every other family always sets `false`.
  `commands::clean::select` filters it out before any other rule, headless
  or not — the guard lives on the resource, not in a caller's discipline,
  because a cron has no human to notice a broken deployment. Individual
  selection (`espace`, in the TUI) is unaffected: the spec only ever asked
  for a human looking at that one row.
- **A branch is only ever offered dead because a pull request merged it —
  never from a per-branch `compare` call.** `refs::branch_is_dead` reads
  `head.ref`/`merged_at` off the same closed-PR listing `api::prs::
  closed_prs` already fetches for the caches' ⚑ flag (one call, two uses —
  zero marginal requests). A PR closed *without* merging leaves its branch
  alone: `a_branch_with_no_merged_pr_is_alive` is the test that guards this,
  and the negative case is the one that matters — a classifier keying on
  "closed" alone would offer to delete work someone meant to resume.
- Required token scopes: `repo`, `read:org`, `read:packages`, and
  `delete:packages` cover everything bondebarras does, including the Billing
  tab's usage report (a 403 there just means the token's owner isn't an org
  owner). Branches, tags, and release assets (v0.4) need no scope beyond
  `repo`, already in that list — neither does repository archiving (v0.5):
  same `repo`-scoped endpoint, gated by the token's admin rights on that one
  repository rather than a scope to grant. Repository *deletion* is
  permanently out of scope, so `delete_repo` is never needed.
- User-facing strings (CLI/TUI output) may be in **French** (e.g.
  `Erreur : …`); code identifiers and documentation stay in English.

## Where to change what

| Need | File |
|------|------|
| Core types (`Resource`, `ResourceKind`, `RiskTier`, size formatting) | `crates/bondebarras-core/src/model.rs` |
| Billing aggregation: SKU multipliers, allowance math, per-repo/-month rollups | `crates/bondebarras-core/src/billing.rs` |
| ⚑ stale-PR-cache detection | `crates/bondebarras-core/src/stale.rs` |
| Token resolution, OAuth scopes | `crates/bondebarras-core/src/auth.rs` |
| HTTP client, pagination, concurrency, delete retry | `crates/bondebarras-core/src/api/mod.rs` |
| Actions cache endpoints | `crates/bondebarras-core/src/api/caches.rs` |
| Artifact endpoints | `crates/bondebarras-core/src/api/artifacts.rs` |
| Workflow run endpoints | `crates/bondebarras-core/src/api/runs.rs` |
| Closed pull request listing (also carries merged `head.ref`s, for dead branches) | `crates/bondebarras-core/src/api/prs.rs` |
| Repository listing (name, `private`, `archived`, admin rights, `pushed_at` age) | `crates/bondebarras-core/src/api/repos.rs` |
| Repository archiving endpoint (`PATCH .../repos/{owner}/{repo}`) | `crates/bondebarras-core/src/api/archive.rs` |
| Repository archiving classification (pure): archivable / already-archived / no admin rights | `crates/bondebarras-core/src/repos.rs` |
| Billing usage-report fetch (403 degrades to `None`, not an error) | `crates/bondebarras-core/src/api/billing.rs` |
| Package version endpoints (list, delete) | `crates/bondebarras-core/src/api/packages.rs` |
| Package version classification: untagged, orphaned attestation, tagged (pure) | `crates/bondebarras-core/src/packages.rs` |
| Branch/tag endpoints (list, delete), default-branch lookup | `crates/bondebarras-core/src/api/refs.rs` |
| Release-asset endpoints (list, delete) — no endpoint for deleting a release itself | `crates/bondebarras-core/src/api/releases.rs` |
| Dead-branch classification (pure): default/protected/no-merged-PR exclusions | `crates/bondebarras-core/src/refs.rs` |
| Two-stage scan orchestration | `crates/bondebarras-core/src/scan.rs` |
| Resource safety classification (`Safety::Safe`/`Check`/`Keep`, `RepoContext`) | `crates/bondebarras-core/src/safety.rs` |
| Deletion planning & execution, progress events | `crates/bondebarras-core/src/clean.rs` |
| TUI event loop, terminal setup/teardown, tagged purge/load channels | `crates/bondebarras-core/src/tui/mod.rs` |
| TUI state: navigation, selection, sort, filter, quit guard, load-after-pause cache | `crates/bondebarras-core/src/tui/app.rs` |
| TUI palette and styles | `crates/bondebarras-core/src/tui/theme.rs` |
| Column 1: organizations | `crates/bondebarras-core/src/tui/views/orgs.rs` |
| Column 2: repositories of the org under the cursor | `crates/bondebarras-core/src/tui/views/repos.rs` |
| Column 3: resources of the loaded repository | `crates/bondebarras-core/src/tui/views/repo.rs` |
| Column 3's subject line and its `(chargement…)`/`(échec du chargement)` stand-ins | `crates/bondebarras-core/src/tui/views/shown.rs` |
| Per-repository cache/minutes gauges, at the head of column 3 | `crates/bondebarras-core/src/tui/views/gauges.rs` |
| Progress row (purge/archive/load bar, between the status line and the footer) | `crates/bondebarras-core/src/tui/views/progress.rs` |
| Confirmation modal | `crates/bondebarras-core/src/tui/views/confirm.rs` |
| Billing tab rendering | `crates/bondebarras-core/src/tui/views/billing.rs` |
| Overall layout (header/status/footer, column widths & folding thresholds) | `crates/bondebarras-core/src/tui/views/mod.rs` |
| CLI parsing (`scan`, `clean`) | `crates/bondebarras-core/src/cli.rs` |
| Headless `scan --json` / `clean` command bodies | `crates/bondebarras-core/src/commands/{scan,clean}.rs` |
| Entry point / runtime wiring | `crates/bondebarras-core/src/lib.rs` |

## Quality gate

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```
