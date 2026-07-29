# bondebarras — project guide for AI agents & contributors

TUI/CLI to audit and clean up GitHub organization resources: Actions caches,
artifacts, workflow runs, and container package versions, across every org a
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
- **No trash, no undo.** Nothing GitHub lets us delete is reversible, so the
  tool never promises otherwise. Tier 1 (caches, artifacts, workflow runs) is
  regenerable by a re-run and gets a bare confirmation; Tier 2 (package
  versions, since v0.3) does not come back, so its modal lists the items and
  says so plainly — see `tui::views::confirm::modal_kind`.
- **Package versions carry no size, ever.** GitHub's API exposes no size
  field for a package version, under any name, and no billing SKU covers
  package storage either. `Resource.size_bytes` is hardcoded to `0` for
  `ResourceKind::PackageVersion`; never estimate or extrapolate one. The TUI
  shows `—` instead of formatting that zero, with a header line spelling out
  why — see `tui::views::repo::list_title`.
- Required token scopes: `repo`, `read:org`, `read:packages`, and
  `delete:packages` cover everything bondebarras does, including the Billing
  tab's usage report (a 403 there just means the token's owner isn't an org
  owner). Repository *deletion* is permanently out of scope, so
  `delete_repo` is never needed.
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
| Closed pull request listing | `crates/bondebarras-core/src/api/prs.rs` |
| Repository listing | `crates/bondebarras-core/src/api/repos.rs` |
| Billing usage-report fetch (403 degrades to `None`, not an error) | `crates/bondebarras-core/src/api/billing.rs` |
| Package version endpoints (list, delete) | `crates/bondebarras-core/src/api/packages.rs` |
| Package version classification: untagged, orphaned attestation, tagged (pure) | `crates/bondebarras-core/src/packages.rs` |
| Two-stage scan orchestration | `crates/bondebarras-core/src/scan.rs` |
| Deletion planning & execution, progress events | `crates/bondebarras-core/src/clean.rs` |
| TUI event loop, terminal setup/teardown | `crates/bondebarras-core/src/tui/mod.rs` |
| TUI state: navigation, selection, sort, filter, quit guard | `crates/bondebarras-core/src/tui/app.rs` |
| TUI palette and styles | `crates/bondebarras-core/src/tui/theme.rs` |
| Left pane (orgs / repos tree) | `crates/bondebarras-core/src/tui/views/orgs.rs` |
| Right pane (resource list) | `crates/bondebarras-core/src/tui/views/repo.rs` |
| Confirmation modal | `crates/bondebarras-core/src/tui/views/confirm.rs` |
| Billing tab rendering | `crates/bondebarras-core/src/tui/views/billing.rs` |
| Overall layout (header/status/footer) | `crates/bondebarras-core/src/tui/views/mod.rs` |
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
