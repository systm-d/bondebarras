# Conventions

The written source of truth for the standards shared across this project (and the
other CLIs built from the same `rust-cli-template` mood). Governance files link here
instead of restating these rules.

## Language & Edition

- Rust **edition 2024**, MSRV **1.88** (pinned via `Cargo.toml`'s
  `[workspace.package] rust-version = "1.88"`). MSRV is driven by the dependency
  graph, not chosen up front: `ratatui 0.30` and `ratatui-core` require 1.88.0.
- Formatting: `rustfmt` with `max_width = 100`, edition 2024 (`rustfmt.toml`).
  `cargo fmt --check` must pass.
- Lints: `unsafe_code = "forbid"`; clippy `all = { level = "warn", priority = -1 }`,
  inherited per crate via `[lints] workspace = true`.
  `cargo clippy --workspace --all-targets -- -D warnings` must pass.

## Project shape

- Workspace: `bondebarras-core` (library: pure logic, `api/` — the only module
  that knows `octocrab` —, `scan.rs`, `clean.rs`, `stale.rs`, `auth.rs`,
  `cli.rs` and `tui/*`) + `bondebarras` (binary, thin shim — `fn main() ->
  ExitCode { bondebarras_core::run() }`).
- Module discipline: business logic, CLI parsing/dispatch, and the TUI all live in
  `bondebarras-core`; the `bondebarras` binary carries no logic of its own.
- **Multi-platform** by design (Linux, Windows, macOS): bondebarras only talks to
  the GitHub REST API over HTTPS. Nothing in the codebase may assume a
  Linux-only environment (no systemd, no libnotify, no `/sys` paths).

## Language of text

- Documentation (README, this file, governance) is in **English**.
- User-facing strings — CLI and TUI output — may be in **French** (e.g. error
  messages such as `Erreur : …`). Never `ERROR`/`FATAL`/`PANIC` in user-facing
  text.
- Code identifiers are in English.

## Git & releases

- **Conventional Commits** (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`,
  `test:`, `build:`, `style:`).
- **Keep a Changelog** format in `CHANGELOG.md`; **Semantic Versioning**.
- Dual license: **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE`).

## Quality gate (run before every PR)

The single source of truth for what must pass. `CONTRIBUTING.md` links here
rather than restating it — the two copies had already drifted once (#28).

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

`.github/workflows/ci.yml` is what actually gates a merge, and it runs the
same checks with two differences worth knowing before opening a PR:

- it runs `cargo test --workspace **--locked**`, across a five-target matrix
  (Ubuntu 22.04/24.04, Fedora 40/41, macOS) — so a `Cargo.lock` left
  unstaged after a dependency change fails CI while passing locally;
- it does **not** build the release binary; `release.yml` does, and not with
  the gate's command either — it builds one binary per target
  (`cargo build --release --locked --bin bondebarras --target <triple>`),
  so the workspace-wide release build below is a check nothing in CI ever
  runs. Keep `cargo build --release` in the local gate for exactly that
  reason: a broken release build is not something to discover at tag time.

CI also runs `cargo audit` and `cargo deny check` (the `security` job), and
an informational `cargo tarpaulin` coverage run that cannot fail the build.
