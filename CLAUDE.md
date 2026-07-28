# bondebarras — project guide for AI agents & contributors

TUI/CLI to audit and clean up GitHub organization resources: Actions caches,
artifacts, and workflow runs, across every org a token can see. Rust
workspace: `bondebarras-core` (library: pure logic, the `api/` boundary, the
CLI parser and the TUI) + `bondebarras` (binary, thin shim).

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
  tool never promises otherwise — a Tier-1 confirmation before deletion, a
  per-item result after, is the whole safety model for v0.1.
- Required token scopes: `repo` and `read:org` cover everything v0.1 does.
  Repository *deletion* is out of scope, so `delete_repo` is never needed.
- User-facing strings (CLI/TUI output) may be in **French** (e.g.
  `Erreur : …`); code identifiers and documentation stay in English.

## Where to change what

| Need | File |
|------|------|
| Core types (`Resource`, `ResourceKind`, `RiskTier`, size formatting) | `crates/bondebarras-core/src/model.rs` |
| ⚑ stale-PR-cache detection | `crates/bondebarras-core/src/stale.rs` |
| Token resolution, OAuth scopes | `crates/bondebarras-core/src/auth.rs` |
| HTTP client, pagination, concurrency, delete retry | `crates/bondebarras-core/src/api/mod.rs` |
| Actions cache endpoints | `crates/bondebarras-core/src/api/caches.rs` |
| Artifact endpoints | `crates/bondebarras-core/src/api/artifacts.rs` |
| Workflow run endpoints | `crates/bondebarras-core/src/api/runs.rs` |
| Closed pull request listing | `crates/bondebarras-core/src/api/prs.rs` |
| Repository listing | `crates/bondebarras-core/src/api/repos.rs` |
| Two-stage scan orchestration | `crates/bondebarras-core/src/scan.rs` |
| Deletion planning & execution, progress events | `crates/bondebarras-core/src/clean.rs` |
| TUI event loop, terminal setup/teardown | `crates/bondebarras-core/src/tui/mod.rs` |
| TUI state: navigation, selection, sort, filter | `crates/bondebarras-core/src/tui/app.rs` |
| TUI palette and styles | `crates/bondebarras-core/src/tui/theme.rs` |
| Left pane (orgs / repos tree) | `crates/bondebarras-core/src/tui/views/orgs.rs` |
| Right pane (resource list) | `crates/bondebarras-core/src/tui/views/repo.rs` |
| Confirmation modal | `crates/bondebarras-core/src/tui/views/confirm.rs` |
| Overall layout (header/status/footer) | `crates/bondebarras-core/src/tui/views/mod.rs` |
| CLI parsing (`bondebarras scan`) | `crates/bondebarras-core/src/cli.rs` |
| Entry point / runtime wiring | `crates/bondebarras-core/src/lib.rs` |

## Quality gate

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```
