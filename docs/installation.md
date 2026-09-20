# Installation

How to install bondebarras, verify what you downloaded, build it from source,
remove it, and keep it up to date.

## Current status: pre-release

**The only published release is [`v1.0.0-rc.2`][rc], a pre-release**, tagged
2026-09-17. GitHub marks it *Pre-release* and does not serve it as *Latest*.
**No stable release exists yet.**

The release candidate is feature-complete and safe to run — every safety rule
described in the [safety model](safety.md) is in force — but the CLI flags and
the `scan --json` schema may still change before `v1.0.0`. See
[releases and versioning](releases.md).

[rc]: https://github.com/systm-d/bondebarras/releases/tag/v1.0.0-rc.2

## What is published, and what is not

States are explicit. **A channel with no tick is not a channel that "will
work anyway" — it is one that does not exist yet.**

| Platform / channel | Architecture | Format | State |
| --- | --- | --- | --- |
| Linux | x86-64 | `.tar.gz` | **Pre-release only** |
| Debian / Ubuntu | x86-64 | `.deb` | **Pre-release only** |
| Fedora / RHEL | x86-64 | `.rpm` | **Pre-release only** |
| macOS (Apple Silicon) | aarch64 | `.tar.gz` | **Pre-release only** |
| Windows | x86-64 | `.exe`, `.zip` | **Pre-release only** |
| From source (`cargo install --git`) | any Rust target | — | **Available** |
| Arch Linux (`PKGBUILD` attached to the release) | x86-64 | `PKGBUILD` | **Pre-release only** |
| Homebrew (`brew install`) | — | formula | **Planned** — the release workflow skips pre-release tags on purpose |
| winget (`winget install`) | x86-64 | manifests | **Planned** — same rule; submission to `winget-pkgs` is a separate manual step |
| AUR (`yay -S bondebarras`) | — | — | **Not published** — there is no AUR job, and no package was ever submitted |
| crates.io (`cargo install bondebarras`) | — | crate | **Not published** — publication is opt-in per repository variable |
| Linux aarch64 | aarch64 | — | **Not published** — not in the build matrix |
| macOS Intel | x86-64 | — | **Not published** — not in the build matrix |
| Windows aarch64 | aarch64 | — | **Not published** — not in the build matrix |

"Pre-release only" means the artifact is built and attached to
`v1.0.0-rc.2`, but no stable release has ever produced one. Everything under
**Planned** is wired in the release workflow and deliberately gated on a
stable tag.

## Verify what you downloaded

**Every binary artifact ships a `.sha256` sidecar** in `sha256sum` format
(`<hex>  <filename>`). Verifying is two commands, and it is worth the ten
seconds — `bondebarras update` refuses to install anything it cannot verify,
and you should hold a manual download to the same standard.

Linux:

```sh
curl -LO https://github.com/systm-d/bondebarras/releases/download/v1.0.0-rc.2/bondebarras-linux-x86_64.tar.gz
curl -LO https://github.com/systm-d/bondebarras/releases/download/v1.0.0-rc.2/bondebarras-linux-x86_64.tar.gz.sha256
sha256sum -c bondebarras-linux-x86_64.tar.gz.sha256
```

macOS:

```sh
shasum -a 256 -c bondebarras-macos-aarch64.tar.gz.sha256
```

Windows (PowerShell) — compare the two values yourself:

```powershell
Get-FileHash .\bondebarras-windows-x86_64.exe -Algorithm SHA256
Get-Content .\bondebarras-windows-x86_64.exe.sha256
```

A mismatch means the file is not the one that was published. Do not run it.

## Linux (generic)

```sh
curl -LO https://github.com/systm-d/bondebarras/releases/download/v1.0.0-rc.2/bondebarras-linux-x86_64.tar.gz
# verify, as above
tar xzf bondebarras-linux-x86_64.tar.gz
sudo install -m 755 bondebarras-linux-x86_64/bondebarras /usr/local/bin/bondebarras
bondebarras --version   # bondebarras 1.0.0-rc.2
```

The archive contains the binary plus `README.md` and both licence files.

## Debian / Ubuntu

The `.deb` is downloaded first, then installed from the file — `apt install`
here takes a path, not a package name from a repository:

```sh
curl -LO https://github.com/systm-d/bondebarras/releases/download/v1.0.0-rc.2/bondebarras_1.0.0.rc.2-1_amd64.deb
# verify, as above
sudo apt install ./bondebarras_1.0.0.rc.2-1_amd64.deb
```

## Fedora / RHEL

```sh
curl -LO https://github.com/systm-d/bondebarras/releases/download/v1.0.0-rc.2/bondebarras-1.0.0.rc.2-1.x86_64.rpm
# verify, as above
sudo dnf install ./bondebarras-1.0.0.rc.2-1.x86_64.rpm
```

RPM forbids a dash in a version number, so the package's own version reads
`1.0.0~rc.2` — the tilde is RPM's pre-release convention and sorts *before*
the eventual `1.0.0`, which is what makes the upgrade work.

## macOS (Apple Silicon)

```sh
curl -LO https://github.com/systm-d/bondebarras/releases/download/v1.0.0-rc.2/bondebarras-macos-aarch64.tar.gz
# verify, as above
tar xzf bondebarras-macos-aarch64.tar.gz
sudo install -m 755 bondebarras-macos-aarch64/bondebarras /usr/local/bin/bondebarras
```

macOS will refuse to run an unsigned, unnotarized binary downloaded with a
browser until you clear the quarantine attribute:

```sh
xattr -d com.apple.quarantine /usr/local/bin/bondebarras
```

**There is no Intel (x86-64) macOS build.** On an Intel Mac, build from source.

## Windows (x86-64)

Two forms, same binary:

```powershell
# Standalone executable
curl.exe -LO https://github.com/systm-d/bondebarras/releases/download/v1.0.0-rc.2/bondebarras-windows-x86_64.exe

# Or the archive, which also carries the README and licences
curl.exe -LO https://github.com/systm-d/bondebarras/releases/download/v1.0.0-rc.2/bondebarras-windows-x86_64.zip
Expand-Archive .\bondebarras-windows-x86_64.zip -DestinationPath .
```

Put the `.exe` somewhere on your `PATH`. bondebarras is a terminal
application: Windows Terminal renders its box-drawing and symbols correctly;
the legacy console host may not.

## Arch Linux

There is **no AUR package**. Each release attaches a rendered `PKGBUILD` with
the correct version and source checksum filled in:

```sh
curl -LO https://github.com/systm-d/bondebarras/releases/download/v1.0.0-rc.2/PKGBUILD
makepkg -si
```

This builds from the release tarball, so it needs a Rust toolchain.

## Building from source

Requirements: **Rust 1.88** (edition 2024) and a C compiler.

> octocrab wants a JWT crypto backend even though bondebarras never signs a
> JWT, and its default one pulls `rsa`, which carries an unfixed advisory
> (RUSTSEC-2023-0071). bondebarras selects `aws-lc-rs` instead, which is what
> needs the C compiler. Precompiled packages are unaffected.

Install straight from the repository:

```sh
cargo install --git https://github.com/systm-d/bondebarras bondebarras
```

Or clone and build, which is also how you run the test suite:

```sh
git clone https://github.com/systm-d/bondebarras
cd bondebarras
cargo build --release          # target/release/bondebarras
cargo test --workspace
```

`cargo install --git` tracks the default branch, which may be ahead of the
latest release. `bondebarras update` will correctly report such a build as
*ahead* of anything published rather than offering you a downgrade.

## Channels that are not available yet

Three commands you may expect **do not work today**, and bondebarras itself
never suggests them:

- **`brew install bondebarras`** — the formula is generated and committed only
  on a *stable* tag. Committing a release candidate to the tap would serve it
  as the stable version to everyone.
- **`winget install bondebarras`** — the manifests are generated only on a
  stable tag, and the first publication additionally requires a pull request
  to `microsoft/winget-pkgs`.
- **`yay -S bondebarras`** — no AUR package exists, and none was ever
  submitted. Use the attached `PKGBUILD`.

`cargo install bondebarras` from crates.io is likewise not published:
publication is opt-in per repository and is not currently enabled.

## Uninstalling

```sh
sudo apt remove bondebarras        # Debian / Ubuntu
sudo dnf remove bondebarras        # Fedora / RHEL
sudo rm /usr/local/bin/bondebarras # tarball install
cargo uninstall bondebarras        # cargo install
```

**There is nothing else to clean up.** bondebarras writes no configuration
file, no cache and no state anywhere on disk: everything it learns lives in
memory for the length of one session. It never stores your token either — the
token comes from `gh`'s own credential store or from `$GITHUB_TOKEN` each
time. Removing the binary removes bondebarras.

## Updating

```sh
bondebarras update --check   # report only, install nothing
bondebarras update           # verify and install, according to how you installed it
```

`update` needs no token, queries GitHub only when you ask it to — never at
startup — and acts on the install channel it detects rather than overwriting
the binary blindly:

| How you installed | What `update` does |
| --- | --- |
| `.deb` | Downloads, verifies, then `sudo apt install <file>` |
| `.rpm` | Downloads, verifies, then `sudo dnf install <file>` |
| Homebrew, AUR / pacman, Nix, `cargo install` | Prints the right instruction and touches nothing — overwriting a package manager's file would desynchronize its database |
| A manually installed binary | Downloads and verifies the archive, then tells you where it is for you to replace the binary yourself |

**Verification fails closed.** Nothing is installed unless the published
checksum matches: a release with no checksum, a checksum that could not be
fetched or parsed, and a checksum that disagrees are three distinct refusals,
each with its own message, and **all three refuse**.

Two limitations worth knowing before you rely on it:

> **`update` exits 0 even when it gives up** — when it could not reach GitHub,
> or refused a checksum. A download or an installation that fails outright does
> exit `1`. Read its output; do not test its exit code.

> **On a manually installed binary, `update` picks the first `.tar.gz` asset
> of the release, without considering your platform.** The current release
> publishes two — Linux and macOS — so the archive it downloads and verifies
> may not be the one for the machine you are on. Check the filename it prints
> before replacing your binary, or download the archive yourself from the
> [release page](https://github.com/systm-d/bondebarras/releases/latest). The
> `.deb` and `.rpm` channels are unaffected: each release publishes exactly one
> of each.

Once installed, start with [authentication](authentication.md) — bondebarras
needs a GitHub token before it can show you anything.
