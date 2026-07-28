# Contributing to bondebarras

Thanks for your interest in the project! This guide covers setting up your
environment, code conventions, and the contribution process.

---

## Prerequisites

- Rust stable ≥ 1.88
- `git`

---

## Build & test

```sh
# Build the workspace
cargo build --workspace

# Run all tests
cargo test --workspace

# Lint (zero warnings required)
cargo clippy --workspace --all-targets -- -D warnings
```

> **Important:** this project applies `rustfmt` consistently.
> Run `cargo fmt` before every commit — CI checks `cargo fmt --check`
> and fails if the code isn't formatted.

---

## Commit conventions (Conventional Commits)

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/).
Examples observed in the project:

```
feat(tui): rendu split-pane et modale de confirmation palier 1
fix(api): ne reessaie un 403 que s'il porte un Retry-After
feat(scan): orchestration a deux etages avec tolerance aux orgs en echec
docs(plan): pagine closed_numbers, plafond silencieux a 100 PR
```

General format: `<type>(<scope>): <short description>`

Common types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `build`.

---

## Pull request workflow

1. Create a branch from `main`:

   ```sh
   git switch main
   git switch -c feat/my-feature
   ```

2. Develop test-first where possible (`bondebarras-core` is the UI-less
   library — business logic, `api/`, `scan.rs`, `clean.rs`, `stale.rs`, and
   the TUI itself, all unit- and integration-tested; `bondebarras` is the
   thin binary shim plus the CLI integration tests).

3. Before opening a PR, verify:

   ```sh
   cargo test --workspace                    # all tests pass
   cargo clippy --workspace -- -D warnings   # zero warnings
   ```

4. Open a PR against `main`. Describe the change, its motivation, and the
   tests added.

---

## Project structure

```
crates/
  bondebarras-core/   # pure lib (no binary of its own), unit-tested
  bondebarras/        # binary: thin shim + CLI integration tests
docs/
  superpowers/        # specs and plans for each phase
```

---

## License

By contributing, you agree that your contributions will be published under
the project's dual **MIT OR Apache-2.0** license.
