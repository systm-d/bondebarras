# Releases and versioning

How bondebarras is built, published and numbered, and what that does — and
does not — promise you.

Where to get each artifact, and how to verify it, is in
[installation](installation.md). Every released version is recorded in
[`CHANGELOG.md`](../CHANGELOG.md), in
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format.

## Stable releases and pre-releases

A release is produced by pushing a tag matching `v*`. **The tag itself decides
whether the release is a pre-release: one containing a `-` is published as
such.**

| Tag | GitHub status | Served as *Latest*? |
| --- | --- | --- |
| `v1.0.0-rc.2` | Pre-release | No |
| `v1.0.0-rc.3` | Pre-release | No |
| `v1.0.0` | Release | Yes |

This one rule drives everything downstream: a pre-release tag is published with
its binaries and packages, but the Homebrew and winget jobs are skipped, and
`bondebarras update` — which reads GitHub's `releases/latest` — does not see
it. That is deliberate. Committing a release candidate to the Homebrew tap
would serve it as *the* stable version to every `brew install`.

**Today there are two releases, `v1.0.0-rc.2` and `v1.0.0-rc.3`, and both
are pre-releases.** No stable release has ever been published.

## What the release workflow produces

On every `v*` tag:

| Artifact | Built from | Notes |
| --- | --- | --- |
| `bondebarras-linux-x86_64.tar.gz` | `x86_64-unknown-linux-gnu` | Binary + `README.md` + both licences |
| `bondebarras-macos-aarch64.tar.gz` | `aarch64-apple-darwin` | Same contents |
| `bondebarras-windows-x86_64.zip` | `x86_64-pc-windows-msvc` | Same contents |
| `bondebarras-windows-x86_64.exe` | same build | Standalone executable, for direct download and winget |
| `bondebarras_<version>-1_amd64.deb` | `cargo deb` | |
| `bondebarras-<version>-1.x86_64.rpm` | `cargo generate-rpm` | Version translated, see below |
| `<artifact>.sha256` | — | **One per binary artifact above** — not `PKGBUILD`, not `bondebarras.rb` — in `sha256sum` format |
| `PKGBUILD` | `packaging/aur/PKGBUILD` | Version and source checksum filled in |
| `bondebarras.rb` | `packaging/homebrew/bondebarras.rb` | Formula, rendered for this tag |

On a **stable** tag only, two more steps run:

- the Homebrew formula is committed to `Formula/bondebarras.rb` on the default
  branch;
- winget manifests are generated and attached as `winget-manifests.tar.gz`.

Publication to crates.io is opt-in: it runs only when the repository variable
`PUBLISH_CRATES` is set to `true`, and it is not currently enabled.

### Architectures

Three targets, and only three:

- `x86_64-unknown-linux-gnu`
- `x86_64-pc-windows-msvc`
- `aarch64-apple-darwin` (Apple Silicon)

**There is no Linux aarch64, no Intel macOS and no Windows aarch64 build.** On
any of those, [build from source](installation.md#building-from-source).

### The RPM version translation

RPM forbids a dash in a version number. The workflow translates it to a tilde
— `1.0.0-rc.3` becomes `1.0.0~rc.3` — which is RPM's own pre-release
convention and sorts *before* the final `1.0.0`, so upgrading from a release
candidate to the stable release works the way you would expect. Cargo's
version stays the source of truth; the workflow only translates it.

## Checksums

**Every binary artifact of every release gets a `.sha256` sidecar**, computed in
the artifact's own directory so the second field is a bare filename, in the
format `sha256sum -c` reads directly.

This is not decoration. `bondebarras update` verifies a downloaded asset
against its sidecar and **refuses to install on all three failure modes** —
no checksum published, a checksum that could not be fetched or parsed, and a
checksum that disagrees with the file. The three are kept as distinct states
with distinct messages rather than collapsed into one "proceed anyway",
because an attacker able to interfere with just the sidecar request would
otherwise look exactly like a release that never published one. Tests:
`verify_outcome_proceeds_only_when_verified`,
`verify_outcome_refuses_to_install_when_the_checksum_could_not_be_fetched`,
`the_three_refusal_messages_are_distinct`.

**The rendered packaging recipes get none.** `PKGBUILD` and `bondebarras.rb`
are written at the repository root *after* the step that checksums the
downloaded build artifacts, so they fall outside it: of the fourteen assets
on `v1.0.0-rc.3`, six are sidecars covering the six binary artifacts, and
those two have none. `winget-manifests.tar.gz`, attached by a separate job on
stable tags, would not have one either. Each recipe does embed the source
tarball's own `sha256` — which is what `makepkg` and `brew` verify — but the
recipe file itself is not covered, so read it before you run it.

Verification commands for a manual download are in
[installation](installation.md#verify-what-you-downloaded).

## Homebrew, the AUR and winget

| Channel | Policy | State today |
| --- | --- | --- |
| **Homebrew** | The formula is rendered from `packaging/homebrew/bondebarras.rb` and committed to the default branch — **on a stable tag only** | Planned. `brew install bondebarras` does not work yet |
| **winget** | Manifests are generated and attached to the release — **on a stable tag only**. The *first* publication additionally requires a pull request to `microsoft/winget-pkgs` | Planned. `winget install bondebarras` does not work yet |
| **AUR** | **No job exists**, and no package was ever submitted. A rendered `PKGBUILD` is attached to every release instead | Not published. `yay -S bondebarras` does not work, and never has |

`bondebarras update` reflects this honestly: on Arch it names the release's
`PKGBUILD` and `makepkg -si`, and on macOS it points at the release page — it
never prints a command that could not succeed.

## Versioning

bondebarras follows **[Semantic Versioning](https://semver.org/spec/v2.0.0.html)**
and records changes in **[Keep a Changelog](https://keepachangelog.com/en/1.1.0/)**
format, as stated in [CONVENTIONS.md](../CONVENTIONS.md). The workspace
version in `Cargo.toml` is the single source of truth: the binary's
`--version` asserts against it in a test rather than a literal, which is what
stops the two from drifting.

### What is guaranteed before `1.0.0`

**The `scan --json` schema is not stable, and neither are the CLI flags.** The
`1.0.0-rc.1` changelog entry says so in as many words: the release candidate
is feature-complete and safe to run, but both may still change before
`v1.0.0`.

Concretely, before the stable release:

- a JSON key may be added, renamed or removed;
- a flag may be added, renamed or removed;
- the French strings the CLI and TUI print may be reworded at any time, and
  are not an interface to parse.

If you automate against `scan --json` today, **pin the version you tested
against** and read the changelog before upgrading.

### What is guaranteed after `1.0.0`

From `1.0.0` onward, SemVer applies to the project as released: breaking
changes go in a major version.

> **What that covers has not been spelled out beyond SemVer itself.** The
> project has not published a statement of which surfaces — the JSON schema,
> the flag set, the library crate's public API — are part of the "public API"
> for versioning purposes, and this page will not invent one. Until it does,
> the safe reading is the narrow one: treat `scan --json` as stable within a
> major version because SemVer says so, and pin anyway if a break would hurt.

The safety rules themselves are a different matter, and they are not a
versioning question: they are enforced by the named tests cited throughout the
[safety model](safety.md). A change that let a bulk selection take a protected
resource would fail the suite, not merely bump a number.

### How `update` compares versions

`bondebarras update` parses `major.minor.patch[-pre]` itself rather than
pulling in a full SemVer implementation, and gets the cases that matter right:

- **numeric, not lexical**: `0.5.10` is newer than `0.5.9`;
- **a pre-release ranks below its stable version**: `1.0.0` is newer than
  `1.0.0-rc.3`, and a build of `1.0.0` is reported as *ahead* of a published
  `1.0.0-rc.3`;
- **a local build newer than anything published is reported as such**, never
  as "up to date" and never offered a downgrade — which is the normal state of
  a `cargo install --git` build;
- build metadata (`+…`) is dropped, as SemVer requires;
- anything that does not parse falls back to exact string comparison, erring
  toward "up to date" only on an exact match.

Tests: `compare_detects_every_ordering`,
`compare_ranks_a_prerelease_below_its_stable_version`,
`compare_reports_ahead_when_local_build_is_newer_than_the_latest_release`.

## Reporting a problem with a release

A broken artifact, a checksum that does not match, or a channel that claims to
exist and does not:
[open an issue](https://github.com/systm-d/bondebarras/issues). For a
vulnerability, follow [SECURITY.md](../SECURITY.md) instead.
