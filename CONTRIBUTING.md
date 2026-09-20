# Contributing to bondebarras

Thanks for your interest in the project! This guide covers setting up your
environment, code conventions, and the contribution process.

---

## Prerequisites

- Rust stable ≥ 1.88
- `git`

---

## Build & test

The quality gate — formatting, lints, tests, release build — is defined once,
in **[CONVENTIONS.md § Quality gate](CONVENTIONS.md#quality-gate-run-before-every-pr)**.
Run it before every commit you intend to push.

This guide deliberately does not restate those commands. It used to, and the
two copies drifted: the clippy line here had lost `--all-targets`, so a
contributor following this file ran a narrower lint than CI does and than
CONVENTIONS.md asks for (#28). One source of truth is the fix; a second copy
kept in sync by hand is not.

> **Important:** this project applies `rustfmt` consistently. Run `cargo fmt`
> before every commit — CI checks `cargo fmt --check` and fails if the code
> isn't formatted.

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

3. Before opening a PR, run the whole quality gate from
   [CONVENTIONS.md](CONVENTIONS.md#quality-gate-run-before-every-pr) — every
   command in it, each one passing on its own.

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
