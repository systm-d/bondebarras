//! Self-update helpers: decide whether a newer bondebarras exists on GitHub
//! Releases, which package fits the current install, and how to install it.
//!
//! Deliberately network-free so it stays unit-testable: the HTTP calls live
//! in `commands::update`, which feeds the JSON here via [`parse_release`].
//! bondebarras only reaches this module on an explicit `bondebarras update`
//! — never automatically when the TUI starts.

use std::cmp::Ordering;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

/// `owner/repo` published on GitHub Releases.
pub const REPO: &str = "systm-d/bondebarras";

/// GitHub REST endpoint for the latest published release.
pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/systm-d/bondebarras/releases/latest";

/// The version this binary was built from.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A published release, distilled from the GitHub API payload.
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub tag: String,
    pub version: String,
    pub html_url: String,
    pub notes: String,
    pub assets: Vec<Asset>,
}

/// A downloadable file attached to a release.
#[derive(Debug, Clone)]
pub struct Asset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
}

impl ReleaseInfo {
    /// The release asset matching a channel's package suffix *and* the
    /// platform it will run on, if any.
    ///
    /// `target` — an `<os>-<arch>` token, see [`current_target`] — is a
    /// parameter rather than something read from `std::env::consts` right
    /// here, and that is the whole of the fix for #49. The two *Unix*
    /// archives a release publishes both end in `.tar.gz` — Windows ships a
    /// `.zip`, served since #55 by its own channel and its own suffix — so
    /// matching on the suffix alone returned whichever of those two GitHub
    /// happened to list first: a macOS user was offered
    /// `bondebarras-linux-x86_64.tar.gz`, a download that passes its
    /// checksum and then cannot execute. Injecting the target is also
    /// what makes that testable — CI runs on six platforms since #55, so a
    /// test keyed on the host's own target would assert something different
    /// on each one.
    ///
    /// `None` is a named, testable outcome — not a panic — for a channel
    /// this release genuinely ships nothing for (Cargo never publishes a
    /// package asset; a release can be missing one platform's build). The
    /// caller refuses on it, and says which platform it looked for, rather
    /// than falling back on another one's archive — see
    /// [`no_asset_for_target`] and `commands::update::apply`.
    pub fn asset_for(&self, channel: InstallChannel, target: &str) -> Option<&Asset> {
        let suffix = channel.package_suffix()?;
        let wanted = if channel.asset_name_carries_target() {
            format!("-{target}{suffix}")
        } else {
            suffix.to_string()
        };
        self.assets.iter().find(|a| a.name.ends_with(&wanted))
    }

    /// The published `.sha256` companion for an asset, if one was uploaded.
    pub fn checksum_for(&self, asset: &Asset) -> Option<&Asset> {
        let name = format!("{}.sha256", asset.name);
        self.assets.iter().find(|a| a.name == name)
    }
}

/// Parse a GitHub "latest release" JSON payload into a [`ReleaseInfo`].
///
/// Indexes the parsed [`serde_json::Value`] by hand rather than deriving
/// `Deserialize` on a mirror struct, matching `api::releases::assets` — the
/// crate takes `serde_json` as a dependency but not `serde` itself.
pub fn parse_release(json: &str) -> Result<ReleaseInfo> {
    let v: serde_json::Value =
        serde_json::from_str(json).context("réponse de GitHub illisible (JSON inattendu)")?;
    let tag = v["tag_name"]
        .as_str()
        .context("réponse de GitHub sans tag_name")?
        .to_string();
    let version = tag.trim_start_matches('v').to_string();
    let html_url = v["html_url"].as_str().unwrap_or_default().to_string();
    let notes = v["body"].as_str().unwrap_or_default().to_string();
    let assets = v["assets"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|a| {
                    Some(Asset {
                        name: a["name"].as_str()?.to_string(),
                        download_url: a["browser_download_url"].as_str()?.to_string(),
                        size: a["size"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(ReleaseInfo {
        tag,
        version,
        html_url,
        notes,
        assets,
    })
}

// --- Version comparison ------------------------------------------------------

/// Where the local build sits relative to the latest published release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    /// Running the newest published version.
    UpToDate,
    /// A newer version is available (carries that version string).
    Available(String),
    /// Local build is newer than anything published — the state of a build
    /// from source before its first release, or a dev build ahead of trunk.
    /// This is not a downgrade offer: `update` stops here.
    Ahead,
}

/// A minimal, dependency-free `major.minor.patch[-pre]` version, just
/// expressive enough for GitHub release tags. Not a full semver
/// implementation (build metadata is dropped, numeric pre-release identifier
/// detection doesn't reject leading zeroes) — deliberately so: the only
/// permitted new dependencies for this feature are `ureq` and `sha2`, so
/// version comparison is hand-rolled rather than pulling in the `semver`
/// crate.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<PreIdent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreIdent {
    Numeric(u64),
    Alpha(String),
}

fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim().trim_start_matches('v');
    // Build metadata (`+...`) never affects precedence — drop it.
    let s = s.split('+').next().unwrap_or(s);
    let (core, pre) = match s.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (s, None),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None; // a fourth numeric segment isn't major.minor.patch
    }
    let pre = pre
        .map(|p| p.split('.').map(parse_pre_ident).collect())
        .unwrap_or_default();
    Some(Version {
        major,
        minor,
        patch,
        pre,
    })
}

fn parse_pre_ident(ident: &str) -> PreIdent {
    if !ident.is_empty()
        && ident.chars().all(|c| c.is_ascii_digit())
        && let Ok(n) = ident.parse::<u64>()
    {
        return PreIdent::Numeric(n);
    }
    PreIdent::Alpha(ident.to_string())
}

fn cmp_versions(a: &Version, b: &Version) -> Ordering {
    a.major
        .cmp(&b.major)
        .then(a.minor.cmp(&b.minor))
        .then(a.patch.cmp(&b.patch))
        .then_with(|| cmp_pre(&a.pre, &b.pre))
}

/// A version with no pre-release identifiers outranks one with — `1.0.0` is
/// newer than `1.0.0-rc.1`, per semver precedence rules.
fn cmp_pre(a: &[PreIdent], b: &[PreIdent]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| cmp_ident(x, y))
            .find(|o| *o != Ordering::Equal)
            .unwrap_or_else(|| a.len().cmp(&b.len())),
    }
}

fn cmp_ident(a: &PreIdent, b: &PreIdent) -> Ordering {
    match (a, b) {
        (PreIdent::Numeric(x), PreIdent::Numeric(y)) => x.cmp(y),
        (PreIdent::Alpha(x), PreIdent::Alpha(y)) => x.cmp(y),
        (PreIdent::Numeric(_), PreIdent::Alpha(_)) => Ordering::Less,
        (PreIdent::Alpha(_), PreIdent::Numeric(_)) => Ordering::Greater,
    }
}

/// Compare two version strings.
///
/// Falls back to an exact string comparison if either side doesn't parse as
/// `major.minor.patch[-pre]`, erring toward "up to date" only on an exact
/// match — never toward silently offering what might be a downgrade.
pub fn compare(current: &str, latest: &str) -> UpdateStatus {
    match (parse_version(current), parse_version(latest)) {
        (Some(cur), Some(new)) => match cmp_versions(&new, &cur) {
            Ordering::Greater => UpdateStatus::Available(latest.to_string()),
            Ordering::Equal => UpdateStatus::UpToDate,
            Ordering::Less => UpdateStatus::Ahead,
        },
        _ if current == latest => UpdateStatus::UpToDate,
        _ => UpdateStatus::Available(latest.to_string()),
    }
}

// --- Install channel detection ----------------------------------------------

/// How this binary was most likely installed — drives the update strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallChannel {
    Deb,
    Rpm,
    Pacman,
    Cargo,
    Homebrew,
    Nix,
    /// A winget-managed install on Windows. Detected but never driven, for
    /// the same reason as Homebrew and pacman: winget records which file it
    /// installed, and replacing that file by hand leaves its database
    /// claiming the old version.
    Winget,
    /// A binary the user installed by hand from the published `.tar.gz` —
    /// Linux and macOS.
    Tarball,
    /// The same thing on Windows, where the release publishes a `.zip`
    /// instead of a `.tar.gz` (#55). A separate variant rather than a
    /// target-dependent suffix on `Tarball`: the suffix is what
    /// [`ReleaseInfo::asset_for`] matches on, and making that one function
    /// read the platform twice — once for the suffix, once for the token —
    /// is how the two could drift apart.
    Zip,
    Unknown,
}

impl InstallChannel {
    /// The release-asset filename suffix to download for this channel, when
    /// the update can be driven from a downloaded package.
    fn package_suffix(self) -> Option<&'static str> {
        match self {
            InstallChannel::Deb => Some(".deb"),
            InstallChannel::Rpm => Some(".rpm"),
            InstallChannel::Tarball => Some(".tar.gz"),
            // The `.zip`, not the bare `.exe` the same release publishes:
            // it is the exact counterpart of the `.tar.gz` — the binary
            // plus `README.md` and both licence files — so the manual
            // install channel tells one story on all three platforms.
            InstallChannel::Zip => Some(".zip"),
            InstallChannel::Pacman
            | InstallChannel::Cargo
            | InstallChannel::Homebrew
            | InstallChannel::Nix
            | InstallChannel::Winget
            | InstallChannel::Unknown => None,
        }
    }

    /// Whether bondebarras downloads a release asset for this channel at
    /// all. The five hands-off channels never do — their update is a
    /// command the user runs, which [`install_plan`] spells out — so "no
    /// asset matched" means something else entirely for them than for the
    /// four that do, and must not borrow the same refusal.
    pub fn downloads_an_asset(self) -> bool {
        self.package_suffix().is_some()
    }

    /// Whether this channel's release assets carry the `<os>-<arch>` token
    /// in their filename. The `binaries` matrix in
    /// `.github/workflows/release.yml` packages each build as
    /// `bondebarras-<matrix.name>.tar.gz` (`.zip`/`.exe` on Windows), so an
    /// archive — `.tar.gz` or `.zip` — is only ever *this* machine's when
    /// that token matches [`current_target`].
    ///
    /// The `.deb` and `.rpm` are the exception, deliberately: their names
    /// are produced by `cargo-deb` and `cargo-generate-rpm`
    /// (`bondebarras_1.0.0-1_amd64.deb`, `bondebarras-1.0.0-1.x86_64.rpm`),
    /// in each tool's own architecture vocabulary rather than
    /// `matrix.name`'s, and the workflow builds exactly one of each, on
    /// `ubuntu-latest`. There is nothing to disambiguate, and no naming rule
    /// in the workflow to key on if there ever were: publishing an arm64
    /// `.deb` would mean flipping this to `true` *and* mapping
    /// `x86_64`/`aarch64` onto `amd64`/`arm64` first.
    ///
    /// Matched exhaustively, like `model::risk_tier`: a channel added
    /// without an answer here does not compile.
    fn asset_name_carries_target(self) -> bool {
        match self {
            InstallChannel::Tarball | InstallChannel::Zip => true,
            InstallChannel::Deb | InstallChannel::Rpm => false,
            InstallChannel::Pacman
            | InstallChannel::Cargo
            | InstallChannel::Homebrew
            | InstallChannel::Nix
            | InstallChannel::Winget
            | InstallChannel::Unknown => false,
        }
    }
}

/// The `<os>-<arch>` token the release workflow stamps into every archive
/// name — `matrix.name` in `.github/workflows/release.yml`, which builds
/// `linux-x86_64`, `windows-x86_64` and `macos-aarch64`, then packages each
/// as `bondebarras-<name>.tar.gz` / `.zip` / `.exe`.
///
/// That workflow spells both halves exactly as Rust spells them (`macos`,
/// not `darwin`; `x86_64`, not `amd64`; `aarch64`, not `arm64`), so the
/// correspondence needs no lookup table: `std::env::consts::OS` and
/// `std::env::consts::ARCH` *are* the two halves, resolved for the target
/// this binary was compiled for. A platform the workflow does not build —
/// a Linux aarch64 machine, say — composes a token no asset carries, and
/// [`ReleaseInfo::asset_for`] then matches nothing, which is the honest
/// answer rather than a usable-looking archive for another machine.
pub fn current_target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// `std::env::consts::OS` on Windows. Spelled once, so the branch that
/// reads it and the tests that drive it cannot drift apart.
const WINDOWS_OS: &str = "windows";

/// Best-effort detection of how the running binary was installed.
pub fn detect_channel() -> InstallChannel {
    let exe = std::env::current_exe().unwrap_or_default();
    detect_channel_for(&exe, std::env::consts::OS, package_owns)
}

/// The testable core of channel detection: the operating system *and* the
/// package-manager check are injected, so tests can simulate "dpkg claims
/// this file" without depending on the test machine's actual package
/// database, and can state what a Windows install resolves to from any
/// runner in the matrix. That second parameter is the same move
/// [`ReleaseInfo::asset_for`] makes with its target, for the same reason:
/// reading `std::env::consts::OS` in here would make the Windows branch
/// assertable only *on* Windows, which is the hole #55 was filed for.
///
/// Three signals, in order: the path first (`.cargo/` and winget's own
/// directories are unambiguous — no manager is ever consulted for them),
/// then the operating system, then the package manager asked directly about
/// the resolved path. A bare system path like `/usr/bin` proves nothing on
/// its own, so only the manager's answer decides there.
///
/// The path signal is itself platform-aware, which is why `os` reaches
/// [`channel_from_path`] instead of only gating the branch below it.
/// `linuxbrew`, `Cellar` and `/nix/store/` are Unix facts; a Windows path
/// that happens to contain one of them — `C:\Cellar\…` — is not a Homebrew
/// install, and reading those patterns ahead of the `windows` branch let a
/// spurious `Homebrew` outrank the `Zip` answer #55 exists to give. Cargo
/// and winget stay ungated: both spell their directories the same way
/// wherever they appear.
fn detect_channel_for(exe: &Path, os: &str, owns: impl Fn(&str, &Path) -> bool) -> InstallChannel {
    let path = exe.to_string_lossy();

    if let Some(channel) = channel_from_path(&path, os) {
        return channel;
    }
    // Windows has no `dpkg` to ask and no `/usr` to recognise: a path that
    // is neither cargo's nor winget's is a binary the user put there
    // themselves, which is exactly what the published `.zip` replaces.
    // Answering `Unknown` here — what this function did before #55 — told a
    // Windows user bondebarras could not work out how it had been
    // installed, while the release published an archive for their exact
    // machine. "I don't know" and "you installed it by hand" are the same
    // state on Windows, and the second one is the true one.
    if os == WINDOWS_OS {
        return InstallChannel::Zip;
    }
    if owns("dpkg", exe) {
        return InstallChannel::Deb;
    }
    if owns("rpm", exe) {
        return InstallChannel::Rpm;
    }
    if owns("pacman", exe) {
        return InstallChannel::Pacman;
    }
    // A system path with no package owner: a manual tarball copy.
    if path.starts_with("/usr/") || path.starts_with("/opt/") {
        return InstallChannel::Tarball;
    }
    InstallChannel::Unknown
}

/// winget's user-scope root, `%LOCALAPPDATA%\Microsoft\WinGet\`, lowercased
/// and spelled with `/` — the form [`channel_from_path`] normalises a path
/// into before matching. Matched wherever it appears: nothing but winget
/// puts a `WinGet` directory under a `Microsoft` one.
const WINGET_USER_ROOT: &str = "/microsoft/winget/";

/// The machine-scope pair. These two names carry nothing distinctive on
/// their own, so [`is_winget_path`] accepts them under a Program Files root
/// and nowhere else.
const WINGET_MACHINE_DIRECTORIES: [&str; 2] = ["/winget/packages/", "/winget/links/"];

/// What `%PROGRAMFILES%` widens to. The x86 form is a root of its own rather
/// than a prefix match: `C:\Program Files (x86)\WinGet\` holds an x86
/// package and is as much winget's as the other.
const PROGRAM_FILES_ROOTS: [&str; 2] = ["/program files", "/program files (x86)"];

/// Whether `lower` — already lowercased and spelled with `/` — sits in a
/// directory winget owns.
///
/// bondebarras' winget manifests declare `InstallerType: portable` (the
/// `winget` job in `.github/workflows/release.yml`), so the two directories
/// that matter are winget's portable package root and the `Links` directory
/// holding the shim that lands on `PATH`. Each exists once per install
/// scope:
///
/// | | user scope | machine scope |
/// | --- | --- | --- |
/// | package | `%LOCALAPPDATA%\Microsoft\WinGet\Packages\` | `%PROGRAMFILES%\WinGet\Packages\` |
/// | shim | `%LOCALAPPDATA%\Microsoft\WinGet\Links\` | `%PROGRAMFILES%\WinGet\Links\` |
///
/// **The machine-scope pair carries no `Microsoft\` segment.** winget's own
/// defaults say so: `portablePackageMachineRoot` is
/// `%PROGRAMFILES%/WinGet/Packages/` against `portablePackageUserRoot`'s
/// `%LOCALAPPDATA%/Microsoft/WinGet/Packages/`. Matching `Microsoft/WinGet/`
/// alone — all #55 first shipped — therefore sent every machine-scope
/// install to `Zip`, and that is the one misclassification here with a real
/// cost: `Zip` invites the user to swap the `.exe` by hand while winget's
/// database goes on describing the version it installed, which is the exact
/// desynchronisation detecting winget at all exists to prevent.
///
/// **So the machine-scope pair is anchored under Program Files, not searched
/// for anywhere in the path.** `winget\Links\` and `winget\Packages\` are
/// ordinary directory names — `…\Downloads\winget\Links\bondebarras.exe`,
/// a hand-unpacked copy sitting next to some downloaded manifests, answered
/// `Winget` unanchored, and would have been told to uninstall through a
/// winget that never installed it. The user-scope root needs no such anchor,
/// which is why the two are matched differently rather than uniformly.
///
/// Two layouts this deliberately does not catch, both read as a hand
/// install: a root redirected through winget's `settings.json`
/// (`portablePackageUserRoot` / `portablePackageMachineRoot` are settable),
/// and a binary sitting directly in `%PROGRAMFILES%\WinGet\` rather than in
/// its `Packages\` or `Links\` subdirectory. Recognising either would mean
/// reading winget's configuration — a second source of truth, on disk, to
/// keep in step — for a layout the default install never produces. It is
/// documented instead, in `docs/installation.md`'s *Updating* section, where
/// the reader it concerns is already standing.
fn is_winget_path(lower: &str) -> bool {
    if lower.contains(WINGET_USER_ROOT) {
        return true;
    }
    WINGET_MACHINE_DIRECTORIES.iter().any(|dir| {
        lower
            .split_once(dir)
            .is_some_and(|(above, _)| PROGRAM_FILES_ROOTS.iter().any(|root| above.ends_with(root)))
    })
}

/// The path-only part of channel detection (no system calls) — unit-testable
/// on its own.
///
/// Separators are normalised before anything is matched: every pattern below
/// is written with `/`, and Windows spells them `\`. That one mismatch was
/// enough to send `C:\Users\…\.cargo\bin\bondebarras.exe` to `Unknown`
/// before #55 — a cargo install that bondebarras could see and still claimed
/// not to recognise.
///
/// `os` gates the patterns that are Unix facts rather than universal ones —
/// see [`detect_channel_for`] for why that gate belongs here and not after
/// the call.
fn channel_from_path(path: &str, os: &str) -> Option<InstallChannel> {
    let normalized = path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    // Case is folded where the filesystem folds it, and only there. Windows
    // compares paths case-insensitively, so `C:\Users\x\.CARGO\bin\` is
    // cargo's own directory and must answer `Cargo` — comparing it exactly
    // sent it to `Zip`, inviting a hand swap of a binary the next `cargo
    // install` overwrites anyway. Unix does not fold, so `/home/x/.CARGO/`
    // there is somebody else's directory and stays one.
    let path = if os == WINDOWS_OS {
        lower.as_str()
    } else {
        normalized.as_str()
    };
    if path.contains("/.cargo/") {
        Some(InstallChannel::Cargo)
    } else if is_winget_path(&lower) {
        Some(InstallChannel::Winget)
    } else if os != WINDOWS_OS && (path.contains("linuxbrew") || path.contains("/Cellar/")) {
        Some(InstallChannel::Homebrew)
    } else if os != WINDOWS_OS && path.starts_with("/nix/store/") {
        Some(InstallChannel::Nix)
    } else {
        None
    }
}

/// Ask a package manager whether it owns `exe`. Real subprocess calls live
/// only here, behind the seam `detect_channel_for` injects for tests.
fn package_owns(bin: &str, exe: &Path) -> bool {
    let args: &[&str] = match bin {
        "dpkg" => &["-S"],
        "rpm" => &["-qf"],
        "pacman" => &["-Qo"],
        _ => return false,
    };
    Command::new(bin)
        .args(args)
        .arg(exe)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// --- Install plan ------------------------------------------------------------

/// What to do once the channel is known and (maybe) a package is on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallPlan {
    /// Run this argv to install; the caller prepends `sudo` when `sudo` is true.
    Run { command: Vec<String>, sudo: bool },
    /// This channel is not one bondebarras drives on the user's behalf —
    /// show the message instead.
    Manual(String),
}

/// Decide how to install `package` for the detected `channel`.
///
/// Self-replace is the last resort, never the default: only `Tarball`,
/// `Zip` and `Unknown` fall through to it, and even then only as a pointer
/// at the already-downloaded, checksum-verified file — see the module docs
/// on why bondebarras doesn't extract the archive itself. That holds on
/// every platform, Windows included: bondebarras has never replaced its own
/// binary anywhere, so the Windows message asks for the same hand swap a
/// Linux user is asked for.
///
/// Both archive channels carry a caveat about swapping a binary that is
/// still running, worded per platform because the underlying fact is not the
/// same one. Windows refuses outright: an executable with a live image
/// cannot be overwritten at all. Unix refuses only the *overwrite* — a `cp`
/// over a running binary fails on `ETXTBSY`, « Text file busy » — while a
/// rename over it still succeeds. Reusing the Windows sentence on Unix would
/// be false, and `Tarball` saying nothing at all, which is what it did until
/// the #55 review, leaves that `cp` failure unexplained. Every channel owned
/// by a package manager (`Deb`, `Rpm`) or fenced off from one (`Pacman`,
/// `Homebrew`, `Nix`, `Cargo`, `Winget`) is handled without ever touching
/// the binary directly.
pub fn install_plan(channel: InstallChannel, package: &Path) -> InstallPlan {
    let pkg = package.display().to_string();
    match channel {
        InstallChannel::Deb => InstallPlan::Run {
            command: vec!["apt".into(), "install".into(), pkg],
            sudo: true,
        },
        InstallChannel::Rpm => InstallPlan::Run {
            command: vec!["dnf".into(), "install".into(), pkg],
            sudo: true,
        },
        // No AUR package is published (#20), so pointing at an AUR helper
        // would hand out a command that cannot resolve. The PKGBUILD shipped
        // with each release is the path the README documents.
        InstallChannel::Pacman => InstallPlan::Manual(format!(
            "Aucun paquet AUR n'est publié pour l'instant. Récupérez le PKGBUILD de la \
             dernière release sur https://github.com/{REPO}/releases/latest, puis \
             `makepkg -si`."
        )),
        // Likewise, the Homebrew tap only ever serves stable releases and
        // none has shipped yet: `brew upgrade` would find nothing.
        InstallChannel::Homebrew => InstallPlan::Manual(format!(
            "Aucune formule Homebrew n'est encore publiée. Récupérez la dernière version \
             sur https://github.com/{REPO}/releases/latest, ou installez depuis les \
             sources : `cargo install --git https://github.com/{REPO} bondebarras`."
        )),
        // /nix/store is read-only: nothing to install in place. The update
        // comes from the user's own configuration.
        InstallChannel::Nix => InstallPlan::Manual(
            "Sur Nix, la mise à jour passe par votre configuration : actualisez l'entrée \
             de flake `bondebarras` (ou votre canal), puis reconstruisez."
                .to_string(),
        ),
        InstallChannel::Cargo => InstallPlan::Manual(format!(
            "Via cargo : `cargo install --git https://github.com/{REPO} bondebarras`."
        )),
        // No winget package is published either: the manifests in
        // `release.yml` are rendered only on a stable tag, and submitting
        // them to `microsoft/winget-pkgs` is a manual step beyond that. So
        // `winget upgrade` is a command that would find nothing — the same
        // reason Pacman and Homebrew get a message rather than a command.
        InstallChannel::Winget => InstallPlan::Manual(format!(
            "Aucun paquet winget n'est encore publié : `winget upgrade` ne trouverait rien. \
             Récupérez la dernière version sur https://github.com/{REPO}/releases/latest, en \
             désinstallant d'abord (`winget uninstall systm-d.bondebarras`) — sans quoi la \
             base de winget resterait sur l'ancienne version."
        )),
        InstallChannel::Tarball if !pkg.is_empty() => InstallPlan::Manual(format!(
            "L'archive a été téléchargée et son empreinte vérifiée : {pkg}. Extrayez-la et \
             remplacez votre binaire par celui qu'elle contient — fermez-le d'abord si vous \
             l'écrasez sur place, un exécutable en cours d'exécution ne peut pas être réécrit \
             (« Text file busy »)."
        )),
        InstallChannel::Zip if !pkg.is_empty() => InstallPlan::Manual(format!(
            "L'archive a été téléchargée et son empreinte vérifiée : {pkg}. Extrayez-la et \
             remplacez votre bondebarras.exe par celui qu'elle contient — fermez-le d'abord, \
             Windows refuse d'écraser un exécutable en cours d'exécution."
        )),
        // The two empty-package arms below are unreachable from
        // `commands::update::apply`: it only reaches `install_plan` with an
        // empty path on a channel whose `downloads_an_asset()` is false, and
        // both archive channels download one. They are kept rather than
        // folded into `Unknown`, and pinned by tests of their own, because
        // the sentence in that last arm is true of exactly one channel. A
        // future caller that hands an archive channel an empty path must
        // come out with a message about an archive, not with a claim that
        // bondebarras cannot tell how it was installed — which it can, or it
        // would not be in this arm. `Tarball` shared that false sentence
        // until the #55 review; `Zip` never did, and now neither does.
        InstallChannel::Zip => InstallPlan::Manual(format!(
            "Récupérez la dernière archive Windows sur \
             https://github.com/{REPO}/releases/latest, extrayez-la et remplacez votre \
             bondebarras.exe."
        )),
        InstallChannel::Tarball => InstallPlan::Manual(format!(
            "Récupérez la dernière archive sur https://github.com/{REPO}/releases/latest, \
             extrayez-la et remplacez votre binaire."
        )),
        InstallChannel::Unknown => InstallPlan::Manual(format!(
            "Impossible de déterminer comment bondebarras a été installé. Récupérez la \
             dernière archive sur https://github.com/{REPO}/releases/latest et remplacez \
             votre binaire."
        )),
    }
}

/// What to tell a user whose platform this release publishes nothing for.
///
/// Names the platform that was detected instead of saying "votre
/// plateforme": it is the one fact the user cannot check for themselves,
/// and the one that explains an empty-handed `update` standing in front of
/// a release page visibly full of archives. Refusing here is the point of
/// #49 — a download that verifies its checksum and then cannot execute is a
/// worse outcome than a refusal that says why.
pub fn no_asset_for_target(channel: InstallChannel, target: &str, html_url: &str) -> String {
    // `parse_release` defaults `html_url` to empty when GitHub's payload
    // omits it; the message must still point somewhere.
    let page = if html_url.is_empty() {
        format!("https://github.com/{REPO}/releases/latest")
    } else {
        html_url.to_string()
    };
    // Naming the detected platform is only honest when the platform is what
    // ruled the asset out. A `.deb` or an `.rpm` carries no target in its
    // name (`asset_name_carries_target`), so the filter never looked at the
    // platform for those: landing here means the release published no such
    // package at all. Blaming the running platform would accuse it of a gap
    // it did not cause, and calling the missing file an "archive" would name
    // the wrong thing on top of that — a Fedora user reading "aucune archive
    // pour votre plateforme" would go hunting for a portability problem that
    // does not exist.
    // Since #55 this branch *is* reachable from Windows, and the sentence
    // was revisited for it rather than inherited. `Zip` downloads an asset
    // like `Tarball` does, so a release that published no
    // `bondebarras-<target>.zip` lands here — and "cette release ne publie
    // aucune archive pour elle" is then exactly true, the way a missing
    // `.tar.gz` makes it true on Linux and macOS. What the sentence never
    // claimed, and still must not, is that the release publishes nothing at
    // all: `asset_for_refuses_when_no_archive_matches_the_platform` pins
    // `Tarball + WINDOWS -> None` at the `asset_for` level, a narrower
    // claim — no *tarball* for Windows, true, and no longer what a Windows
    // user is ever asked for.
    if channel.asset_name_carries_target() {
        format!(
            "Plateforme détectée : {target}. Cette release ne publie aucune archive pour elle — \
             rien n'est téléchargé, une archive d'une autre plateforme ne s'exécuterait pas chez \
             vous. Consultez {page}"
        )
    } else {
        // `downloads_an_asset` is what gates the caller, and every channel
        // it lets through has a suffix — but the fallback keeps the sentence
        // grammatical rather than trusting that from a distance.
        let kind = channel.package_suffix().unwrap_or("installable");
        format!(
            "Cette release ne publie aucun paquet {kind} — rien n'est téléchargé : bondebarras \
             ne substitue pas un autre format à celui par lequel il a été installé. Consultez \
             {page}"
        )
    }
}

// --- Checksum ----------------------------------------------------------------

/// Compute the lowercase hex SHA-256 of a file.
pub fn sha256_hex(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file =
        std::fs::File::open(path).with_context(|| format!("ouverture de {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buf)
            .context("lecture du fichier pour la somme de contrôle")?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Extract the hex digest from a `sha256sum`-style line (`<hash>  <file>`).
pub fn parse_sha256_line(text: &str) -> Option<String> {
    text.split_whitespace().next().map(str::to_lowercase)
}

// --- Asset name validation ---------------------------------------------------

/// Why a release asset's name isn't safe to write into a local directory or
/// hand to a package manager's install command.
///
/// `asset.name` comes straight off the GitHub API response — an
/// adversary-controlled string, however unlikely that is in practice for a
/// repository we don't control the release process of end to end. The
/// caller joins it into a filesystem path (`dir.join(&asset.name)`) and, for
/// Deb/Rpm, passes it as an argv element to `sudo apt install` /
/// `sudo dnf install`. Refused outright rather than sanitised: a release
/// asset with a hostile name is not a case to paper over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsafeAssetName {
    /// Nothing to join, nothing to install.
    Empty,
    /// `.` or `..` — resolves to a directory, not a file to install.
    IsDotOrDotDot,
    /// A `/` or `\` would let the name escape the staging directory
    /// entirely (`../../etc/whatever`).
    ContainsPathSeparator,
    /// `apt`/`dnf` would read a leading `-` as an option, not a filename.
    StartsWithDash,
}

impl std::fmt::Display for UnsafeAssetName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            UnsafeAssetName::Empty => "le nom de l'asset est vide",
            UnsafeAssetName::IsDotOrDotDot => "le nom de l'asset est « . » ou « .. »",
            UnsafeAssetName::ContainsPathSeparator => {
                "le nom de l'asset contient un séparateur de chemin"
            }
            UnsafeAssetName::StartsWithDash => {
                "le nom de l'asset commence par « - » et serait lu comme une option"
            }
        })
    }
}

impl std::error::Error for UnsafeAssetName {}

/// Refuse an asset name that isn't a plain, single-component filename.
pub fn validate_asset_name(name: &str) -> Result<(), UnsafeAssetName> {
    if name.is_empty() {
        return Err(UnsafeAssetName::Empty);
    }
    if name == "." || name == ".." {
        return Err(UnsafeAssetName::IsDotOrDotDot);
    }
    if name.contains('/') || name.contains('\\') {
        return Err(UnsafeAssetName::ContainsPathSeparator);
    }
    if name.starts_with('-') {
        return Err(UnsafeAssetName::StartsWithDash);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // A real release's asset list: one `.deb`, one `.rpm`, and one archive
    // per platform the `binaries` matrix builds — each with its `.sha256`
    // sidecar. The macOS archive is listed *before* the Linux one on
    // purpose: picking the first `.tar.gz`, which is what `asset_for` used
    // to do (#49), then hands a Linux user the macOS build, so a test can
    // fail on the ordering alone.
    //
    // Windows contributes both of the forms `release.yml` publishes for it
    // — the `.zip` and the bare `.exe` — because #55 had to choose between
    // them, and a sample carrying only the chosen one would let a wrong
    // suffix look right.
    const SAMPLE: &str = r#"{
        "tag_name": "v0.6.0",
        "html_url": "https://github.com/systm-d/bondebarras/releases/tag/v0.6.0",
        "body": "Notes",
        "assets": [
            {"name": "bondebarras_0.6.0-1_amd64.deb", "browser_download_url": "https://example/deb", "size": 10},
            {"name": "bondebarras-0.6.0-1.x86_64.rpm", "browser_download_url": "https://example/rpm", "size": 20},
            {"name": "bondebarras-macos-aarch64.tar.gz", "browser_download_url": "https://example/mac-tgz", "size": 25},
            {"name": "bondebarras-macos-aarch64.tar.gz.sha256", "browser_download_url": "https://example/mac-tgz.sha256", "size": 1},
            {"name": "bondebarras-linux-x86_64.tar.gz", "browser_download_url": "https://example/tgz", "size": 30},
            {"name": "bondebarras-linux-x86_64.tar.gz.sha256", "browser_download_url": "https://example/tgz.sha256", "size": 1},
            {"name": "bondebarras-windows-x86_64.zip", "browser_download_url": "https://example/zip", "size": 35},
            {"name": "bondebarras-windows-x86_64.zip.sha256", "browser_download_url": "https://example/zip.sha256", "size": 1},
            {"name": "bondebarras-windows-x86_64.exe", "browser_download_url": "https://example/exe", "size": 40},
            {"name": "bondebarras-windows-x86_64.exe.sha256", "browser_download_url": "https://example/exe.sha256", "size": 1}
        ]
    }"#;

    // The three `matrix.name` values in `.github/workflows/release.yml`,
    // spelled as it spells them — and as `current_target` composes them.
    const LINUX: &str = "linux-x86_64";
    const MACOS: &str = "macos-aarch64";
    const WINDOWS: &str = "windows-x86_64";

    // `std::env::consts::OS` for the platforms whose detection is asserted
    // here. `WINDOWS_OS` comes from the module itself — the branch and the
    // tests must read the same spelling.
    const LINUX_OS: &str = "linux";
    const MACOS_OS: &str = "macos";

    #[test]
    fn parse_release_extracts_version_and_assets() {
        let r = parse_release(SAMPLE).unwrap();
        assert_eq!(r.tag, "v0.6.0");
        assert_eq!(r.version, "0.6.0");
        assert_eq!(r.assets.len(), 10);
    }

    // The test that matters most: this machine's real state today is 0.5.0
    // built from source with nothing published yet. A wrong implementation
    // that only distinguishes "newer" from "up to date" would either claim
    // up-to-date (if it maps Less to Equal) or offer a downgrade (if it maps
    // Less to Available) — both wrong. Only a dedicated `Ahead` variant is
    // correct here.
    #[test]
    fn compare_reports_ahead_when_local_build_is_newer_than_the_latest_release() {
        assert_eq!(compare("0.5.0", "0.4.0"), UpdateStatus::Ahead);
    }

    #[test]
    fn compare_detects_every_ordering() {
        assert_eq!(compare("0.5.0", "0.5.0"), UpdateStatus::UpToDate);
        assert_eq!(
            compare("0.5.0", "0.6.0"),
            UpdateStatus::Available("0.6.0".into())
        );
        // Numeric, not lexical: 0.5.9 must lose to 0.5.10.
        assert_eq!(
            compare("0.5.9", "0.5.10"),
            UpdateStatus::Available("0.5.10".into())
        );
        assert_eq!(compare("0.6.0", "0.5.0"), UpdateStatus::Ahead);
    }

    // A pre-release must never look newer than the stable version it leads
    // up to — GitHub's `releases/latest` shouldn't return one, but the
    // comparator must still get this right defensively.
    #[test]
    fn compare_ranks_a_prerelease_below_its_stable_version() {
        assert_eq!(
            compare("0.6.0-rc.1", "0.6.0"),
            UpdateStatus::Available("0.6.0".into())
        );
        assert_eq!(compare("0.6.0", "0.6.0-rc.1"), UpdateStatus::Ahead);
    }

    #[test]
    fn compare_falls_back_to_string_equality_for_unparsable_versions() {
        assert_eq!(compare("garbage", "garbage"), UpdateStatus::UpToDate);
        assert_eq!(
            compare("garbage", "other"),
            UpdateStatus::Available("other".into())
        );
    }

    #[test]
    fn asset_for_matches_the_suffix_and_the_platform() {
        let r = parse_release(SAMPLE).unwrap();
        assert_eq!(
            r.asset_for(InstallChannel::Deb, LINUX).unwrap().name,
            "bondebarras_0.6.0-1_amd64.deb"
        );
        assert_eq!(
            r.asset_for(InstallChannel::Rpm, LINUX).unwrap().name,
            "bondebarras-0.6.0-1.x86_64.rpm"
        );
        assert_eq!(
            r.asset_for(InstallChannel::Tarball, LINUX).unwrap().name,
            "bondebarras-linux-x86_64.tar.gz"
        );
        // Named outcome, not a panic: a release genuinely missing this
        // platform's asset (Cargo never ships one) must resolve to `None`,
        // distinguishable from "the release is unparsable".
        assert!(r.asset_for(InstallChannel::Cargo, LINUX).is_none());
    }

    // The regression test for #49. Both archives end in `.tar.gz`, so the
    // suffix alone cannot tell them apart, and taking the first match — what
    // `asset_for` used to do — offers a Linux user the macOS build: a
    // download that passes its checksum and then refuses to execute. The
    // target is a parameter precisely so this can name a platform other than
    // the one CI happens to run on, of which there are six since #55.
    #[test]
    fn asset_for_picks_the_archive_of_the_running_platform_not_the_first_listed() {
        let r = parse_release(SAMPLE).unwrap();
        for (target, expected) in [
            (LINUX, "bondebarras-linux-x86_64.tar.gz"),
            (MACOS, "bondebarras-macos-aarch64.tar.gz"),
        ] {
            assert_eq!(
                r.asset_for(InstallChannel::Tarball, target).unwrap().name,
                expected,
                "target {target}"
            );
        }
    }

    // The other half of the fix: when nothing matches, refuse. A `None`,
    // not a panic and not a substitute — this sample publishes no Windows
    // archive, and an `aarch64` Linux machine is a platform the workflow
    // does not build at all.
    #[test]
    fn asset_for_refuses_when_no_archive_matches_the_platform() {
        let r = parse_release(SAMPLE).unwrap();
        assert!(r.asset_for(InstallChannel::Tarball, WINDOWS).is_none());
        assert!(
            r.asset_for(InstallChannel::Tarball, "linux-aarch64")
                .is_none()
        );
    }

    // `.deb` and `.rpm` names carry no `<os>-<arch>` token (see
    // `asset_name_carries_target`), so the platform filter must not reach
    // them: one of each is published, and filtering on a token their names
    // never contain would refuse every single one.
    #[test]
    fn deb_and_rpm_are_not_filtered_on_the_platform_token() {
        let r = parse_release(SAMPLE).unwrap();
        for target in [LINUX, MACOS, WINDOWS] {
            assert!(
                r.asset_for(InstallChannel::Deb, target).is_some(),
                "deb, target {target}"
            );
            assert!(
                r.asset_for(InstallChannel::Rpm, target).is_some(),
                "rpm, target {target}"
            );
        }
    }

    #[test]
    fn deb_asset_is_not_its_own_sha256_sidecar() {
        let r = parse_release(SAMPLE).unwrap();
        let tgz = r.asset_for(InstallChannel::Tarball, LINUX).unwrap();
        assert!(!tgz.name.ends_with(".sha256"));
        assert_eq!(
            r.checksum_for(tgz).unwrap().name,
            "bondebarras-linux-x86_64.tar.gz.sha256"
        );
    }

    // The sidecar follows the archive by full name, so it inherits the
    // platform fix rather than needing its own: picking the right `.tar.gz`
    // would be undone by checking it against the other platform's digest,
    // which fails closed (`Integrity::Mismatch`) and installs nothing.
    #[test]
    fn the_checksum_follows_the_platform_of_its_own_archive() {
        let r = parse_release(SAMPLE).unwrap();
        // Keyed on LINUX, not MACOS: `SAMPLE` lists the macOS archive
        // first, so a version of `asset_for` that took the first `.tar.gz`
        // would answer this correctly for MACOS and the test would pass
        // against the very defect it illustrates (#49, review finding).
        let linux = r.asset_for(InstallChannel::Tarball, LINUX).unwrap();
        assert_eq!(
            r.checksum_for(linux).unwrap().name,
            "bondebarras-linux-x86_64.tar.gz.sha256"
        );
    }

    #[test]
    fn checksum_for_is_none_when_no_sidecar_was_published() {
        let r = parse_release(SAMPLE).unwrap();
        let deb = r.asset_for(InstallChannel::Deb, LINUX).unwrap();
        assert!(r.checksum_for(deb).is_none());
    }

    // Whichever of the six CI platforms this runs on, the token must be
    // spelled the way `release.yml` spells its `matrix.name` — that identity
    // is the entire mapping, and the reason no lookup table exists.
    #[test]
    fn current_target_spells_the_platform_as_the_release_workflow_does() {
        let t = current_target();
        if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            assert_eq!(t, LINUX);
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(t, MACOS);
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            assert_eq!(t, WINDOWS);
        } else {
            // A host the workflow builds nothing for still composes an
            // `os-arch` token; it simply matches no published archive.
            assert!(t.contains('-'), "expected an os-arch token, got {t}");
        }
    }

    // Only the four downloading channels can be told "nothing for your
    // platform"; the other six never download anything, and get their own
    // install plan instead — `commands::update::apply` branches on this.
    #[test]
    fn only_the_downloading_channels_ever_claim_an_asset() {
        for channel in [
            InstallChannel::Deb,
            InstallChannel::Rpm,
            InstallChannel::Tarball,
            InstallChannel::Zip,
        ] {
            assert!(channel.downloads_an_asset(), "{channel:?}");
        }
        for channel in [
            InstallChannel::Pacman,
            InstallChannel::Homebrew,
            InstallChannel::Nix,
            InstallChannel::Cargo,
            InstallChannel::Winget,
            InstallChannel::Unknown,
        ] {
            assert!(!channel.downloads_an_asset(), "{channel:?}");
        }
    }

    // The refusal names the platform it detected: "aucun paquet pour votre
    // plateforme" leaves the user guessing which platform bondebarras thinks
    // they are on, and that guess is exactly what was wrong (#49).
    #[test]
    fn the_refusal_message_names_the_detected_platform() {
        let msg = no_asset_for_target(InstallChannel::Tarball, MACOS, "https://example/release");
        assert!(msg.contains(MACOS), "{msg}");
        assert!(msg.contains("https://example/release"), "{msg}");
    }

    // `html_url` is empty whenever GitHub's payload omitted it
    // (`parse_release` defaults it), and the message must still point
    // somewhere rather than trail off after "Consultez ".
    #[test]
    fn the_refusal_message_falls_back_on_the_releases_page() {
        let msg = no_asset_for_target(InstallChannel::Tarball, MACOS, "");
        assert!(
            msg.contains("github.com/systm-d/bondebarras/releases/latest"),
            "{msg}"
        );
    }

    // A `.deb` or `.rpm` name carries no target, so the platform filter
    // never ruled it out: the refusal must not pin the gap on the platform,
    // nor call a missing package an "archive".
    #[test]
    fn the_refusal_blames_the_release_not_the_platform_for_a_package() {
        for (channel, kind) in [(InstallChannel::Deb, ".deb"), (InstallChannel::Rpm, ".rpm")] {
            let msg = no_asset_for_target(channel, LINUX, "https://example/release");
            assert!(msg.contains(kind), "{msg}");
            assert!(!msg.contains("Plateforme détectée"), "{msg}");
            assert!(!msg.contains("archive"), "{msg}");
            assert!(msg.contains("https://example/release"), "{msg}");
        }
    }

    #[test]
    fn channel_from_path_spots_cargo_brew_and_nix() {
        assert_eq!(
            channel_from_path("/home/x/.cargo/bin/bondebarras", LINUX_OS),
            Some(InstallChannel::Cargo)
        );
        assert_eq!(
            channel_from_path("/home/linuxbrew/.linuxbrew/bin/bondebarras", LINUX_OS),
            Some(InstallChannel::Homebrew)
        );
        assert_eq!(
            channel_from_path(
                "/opt/homebrew/Cellar/bondebarras/0.5.0/bin/bondebarras",
                MACOS_OS
            ),
            Some(InstallChannel::Homebrew)
        );
        assert_eq!(
            channel_from_path(
                "/nix/store/abcd1234-bondebarras-0.5.0/bin/bondebarras",
                LINUX_OS
            ),
            Some(InstallChannel::Nix)
        );
        // A bare system path proves nothing on its own — this is the seam
        // where channel detection must fall through to asking a package
        // manager instead of guessing from the path alone.
        assert_eq!(channel_from_path("/usr/bin/bondebarras", LINUX_OS), None);
    }

    // The test that matters most, alongside `Ahead`: a wrong implementation
    // could plausibly guess a channel from the path alone (e.g. "/usr/bin"
    // therefore Tarball) without ever consulting the package manager. That
    // would be exactly the claudine mistake transplanted into channel
    // detection: touching a manager-owned binary as if it were unmanaged.
    // This test injects a fake "the manager owns it" answer for a path that,
    // by prefix alone, would otherwise fall through to `Tarball` — proving
    // the manager's answer overrides the path guess rather than merely
    // supplementing it.
    // The OS is passed in, not read: with a Windows runner in the matrix
    // since #55, a version of this test that let `detect_channel_for` read
    // `std::env::consts::OS` would assert a Linux classification on a
    // machine that answers `windows`, and fail there for a reason that has
    // nothing to do with package managers.
    #[test]
    fn package_manager_ownership_overrides_the_bare_path_guess() {
        let exe = Path::new("/usr/bin/bondebarras");

        // No manager claims it: path alone can't tell us anything better
        // than "sitting under /usr with no owner" — a manual tarball copy.
        assert_eq!(
            detect_channel_for(exe, LINUX_OS, |_bin, _exe| false),
            InstallChannel::Tarball
        );

        // rpm claims it: the manager's answer must win over the path guess.
        assert_eq!(
            detect_channel_for(exe, LINUX_OS, |bin, _exe| bin == "rpm"),
            InstallChannel::Rpm
        );
        // dpkg claims it instead.
        assert_eq!(
            detect_channel_for(exe, LINUX_OS, |bin, _exe| bin == "dpkg"),
            InstallChannel::Deb
        );
        // pacman claims it instead.
        assert_eq!(
            detect_channel_for(exe, LINUX_OS, |bin, _exe| bin == "pacman"),
            InstallChannel::Pacman
        );
    }

    #[test]
    fn cargo_and_nix_paths_are_never_second_guessed_by_a_package_manager() {
        // These two locations are unambiguous from the path alone, so the
        // manager must never even be asked — a manager returning `true` for
        // everything must not override an unambiguous cargo/nix path.
        let exe = Path::new("/home/x/.cargo/bin/bondebarras");
        assert_eq!(
            detect_channel_for(exe, LINUX_OS, |_bin, _exe| true),
            InstallChannel::Cargo
        );
    }

    // --- Windows (#55) -------------------------------------------------

    // The heart of #55. Every Windows path used to fall through to
    // `Unknown`, so `downloads_an_asset` was false and the user read
    // "Impossible de déterminer comment bondebarras a été installé" — in
    // front of a release publishing an archive for their exact machine.
    // The OS is injected, so this states the Windows outcome from any of
    // the six runners rather than only from the Windows one.
    #[test]
    fn a_windows_install_is_recognised_and_served_the_archive_the_release_publishes() {
        let exe = Path::new(r"C:\Program Files\bondebarras\bondebarras.exe");
        let channel = detect_channel_for(exe, WINDOWS_OS, |_bin, _exe| false);
        assert_eq!(channel, InstallChannel::Zip);
        assert!(channel.downloads_an_asset(), "{channel:?}");

        let r = parse_release(SAMPLE).unwrap();
        assert_eq!(
            r.asset_for(channel, WINDOWS).unwrap().name,
            "bondebarras-windows-x86_64.zip"
        );
    }

    // The sentence #55 exists to delete, pinned on the outcome rather than
    // on one path: whatever a Windows user is told, it is never that
    // bondebarras cannot work out how it was installed — and it is never a
    // `Run` plan either, since nothing here is driven on their behalf.
    #[test]
    fn no_windows_path_is_ever_told_bondebarras_cannot_tell_how_it_was_installed() {
        let package = Path::new(r"C:\Temp\bondebarras-update-a1b2\bondebarras-windows-x86_64.zip");
        for path in [
            r"C:\Program Files\bondebarras\bondebarras.exe",
            r"C:\Users\x\.cargo\bin\bondebarras.exe",
            r"C:\Users\x\AppData\Local\Microsoft\WinGet\Links\bondebarras.exe",
            r"D:\tools\bondebarras.exe",
        ] {
            let channel = detect_channel_for(Path::new(path), WINDOWS_OS, |_bin, _exe| false);
            assert_ne!(channel, InstallChannel::Unknown, "{path}");
            let InstallPlan::Manual(msg) = install_plan(channel, package) else {
                panic!("{path}: Windows must never produce a Run plan");
            };
            assert!(!msg.contains("Impossible de déterminer"), "{path}: {msg}");
        }
    }

    // A cargo install spells its path with backslashes on Windows and
    // differs in nothing else. Matching `/.cargo/` alone sent it to
    // `Unknown`: bondebarras could see `.cargo` in its own path and still
    // claimed to have no idea. The OS branch must not overrule it either —
    // cargo's path is unambiguous on every platform, which is why it is
    // tested against a manager that claims everything.
    #[test]
    fn a_windows_cargo_install_is_still_a_cargo_install() {
        let exe = Path::new(r"C:\Users\x\.cargo\bin\bondebarras.exe");
        assert_eq!(
            channel_from_path(&exe.to_string_lossy(), WINDOWS_OS),
            Some(InstallChannel::Cargo)
        );
        assert_eq!(
            detect_channel_for(exe, WINDOWS_OS, |_bin, _exe| true),
            InstallChannel::Cargo
        );
    }

    // winget records the file it installed, so it is detected to be
    // refused, not to be driven — and it downloads nothing, which is what
    // keeps `apply` on the install-plan branch instead of the asset one.
    #[test]
    fn a_winget_install_is_detected_and_never_downloads_anything() {
        let packages = Path::new(
            r"C:\Users\x\AppData\Local\Microsoft\WinGet\Packages\systm-d.bondebarras\bondebarras.exe",
        );
        assert_eq!(
            detect_channel_for(packages, WINDOWS_OS, |_bin, _exe| false),
            InstallChannel::Winget
        );
        // The shim that actually lands on `PATH` sits one directory over,
        // under the same parent — `current_exe` may resolve to either.
        assert_eq!(
            channel_from_path(
                r"C:\Users\x\AppData\Local\Microsoft\WinGet\Links\bondebarras.exe",
                WINDOWS_OS
            ),
            Some(InstallChannel::Winget)
        );
        assert!(!InstallChannel::Winget.downloads_an_asset());
    }

    // Machine-scope winget (#55, review finding). `%PROGRAMFILES%\WinGet\`
    // carries no `Microsoft\` segment — winget's own `portablePackage`
    // `MachineRoot` default is `%PROGRAMFILES%/WinGet/Packages/`, against
    // `%LOCALAPPDATA%/Microsoft/WinGet/Packages/` for the user scope — so
    // matching `Microsoft/WinGet/` alone classified every machine-wide
    // install as `Zip`. That invited a hand swap of the `.exe` while
    // winget's database went on naming the version it installed: the very
    // desynchronisation this detection exists to prevent, reached from the
    // other side.
    #[test]
    fn a_machine_scope_winget_install_is_detected_like_a_user_scope_one() {
        for path in [
            r"C:\Program Files\WinGet\Packages\systm-d.bondebarras\bondebarras.exe",
            r"C:\Program Files\WinGet\Links\bondebarras.exe",
            // An x86 package widens `%PROGRAMFILES%` and changes nothing else.
            r"C:\Program Files (x86)\WinGet\Packages\systm-d.bondebarras\bondebarras.exe",
        ] {
            assert_eq!(
                detect_channel_for(Path::new(path), WINDOWS_OS, |_bin, _exe| false),
                InstallChannel::Winget,
                "{path}"
            );
        }
    }

    // Case is folded where the filesystem folds it (#55, second review).
    // Windows compares paths case-insensitively, so `.CARGO` is `.cargo`
    // there; comparing it exactly answered `Zip`, which invites a hand swap
    // of a binary the next `cargo install` overwrites anyway. Unix folds
    // nothing, and a `/home/x/.CARGO/` of somebody else's making must not
    // become a cargo install on the way past.
    #[test]
    fn a_windows_cargo_install_is_spotted_whatever_its_case() {
        assert_eq!(
            channel_from_path(r"C:\Users\x\.CARGO\bin\bondebarras.exe", WINDOWS_OS),
            Some(InstallChannel::Cargo)
        );
        assert_eq!(
            detect_channel_for(
                Path::new(r"C:\Users\x\.Cargo\Bin\bondebarras.exe"),
                WINDOWS_OS,
                |_bin, _exe| false
            ),
            InstallChannel::Cargo
        );
        for os in [LINUX_OS, MACOS_OS] {
            assert_eq!(
                channel_from_path("/home/x/.CARGO/bin/bondebarras", os),
                None,
                "{os}"
            );
        }
    }

    // The machine-scope pair is anchored under Program Files (#55, second
    // review). `winget\Links\` and `winget\Packages\` are ordinary directory
    // names: matched anywhere in a path, a hand-unpacked copy sitting beside
    // some downloaded manifests answered `Winget`, and its owner was told to
    // uninstall through a winget that never installed it — the mirror image
    // of the machine-scope miss, and the reason the two scopes are matched
    // differently rather than uniformly.
    #[test]
    fn a_directory_merely_named_winget_is_not_a_winget_install() {
        for path in [
            r"C:\Users\x\Downloads\winget\Links\bondebarras.exe",
            r"C:\Users\x\Downloads\winget\Packages\bondebarras.exe",
            r"D:\winget\Links\bondebarras.exe",
        ] {
            assert_eq!(
                detect_channel_for(Path::new(path), WINDOWS_OS, |_bin, _exe| false),
                InstallChannel::Zip,
                "{path}"
            );
        }
        // Under a real Program Files root the same two directories are
        // winget's, and the user-scope root needs no anchor at all: nobody
        // else spells `Microsoft\WinGet\`.
        assert_eq!(
            channel_from_path(r"C:\Program Files\WinGet\Links\bondebarras.exe", WINDOWS_OS),
            Some(InstallChannel::Winget)
        );
        assert_eq!(
            channel_from_path(
                r"D:\AppData\Local\Microsoft\WinGet\Links\bondebarras.exe",
                WINDOWS_OS
            ),
            Some(InstallChannel::Winget)
        );
    }

    // Detection order (#55, review finding). `linuxbrew` and `Cellar` are
    // Unix facts, and `channel_from_path` ran before the `windows` branch,
    // so `C:\Cellar\…` answered `Homebrew` — implausible, but inverted with
    // respect to the intent, and it would have handed a Windows user a note
    // about a tap that publishes nothing instead of the archive waiting for
    // them. Gating those two patterns on the OS is what fixes the order
    // without moving cargo and winget, which are unambiguous everywhere.
    #[test]
    fn a_windows_path_is_never_classified_by_a_unix_only_pattern() {
        for path in [
            r"C:\Cellar\bondebarras\bondebarras.exe",
            r"C:\tools\linuxbrew\bondebarras.exe",
        ] {
            assert_eq!(
                detect_channel_for(Path::new(path), WINDOWS_OS, |_bin, _exe| false),
                InstallChannel::Zip,
                "{path}"
            );
        }
        // And the same shapes still mean Homebrew where they mean anything.
        assert_eq!(
            channel_from_path(
                "/opt/homebrew/Cellar/bondebarras/1.0.0/bin/bondebarras",
                MACOS_OS
            ),
            Some(InstallChannel::Homebrew)
        );
    }

    // --- The messages, not just the classification (#55, review) --------
    //
    // The proof shipped with #55 covered which channel a path resolves to
    // and never a word of what the user is then told — and the text was the
    // lie #55 existed to correct. The review demonstrated the hole by
    // rewriting the winget message into « Lancez `winget upgrade
    // systm-d.bondebarras` », a command that cannot succeed, and watching
    // every test stay green. Each test below is keyed on the claim its
    // message must keep making, so that mutation and the two beside it now
    // fail.

    // Every variant of `InstallChannel`. Kept complete by hand: the
    // module's own exhaustive `match`es are what refuse to compile for a
    // channel with no plan, and this list is what stops one escaping the
    // text assertions — the two are updated together.
    const EVERY_CHANNEL: [InstallChannel; 10] = [
        InstallChannel::Deb,
        InstallChannel::Rpm,
        InstallChannel::Pacman,
        InstallChannel::Cargo,
        InstallChannel::Homebrew,
        InstallChannel::Nix,
        InstallChannel::Winget,
        InstallChannel::Tarball,
        InstallChannel::Zip,
        InstallChannel::Unknown,
    ];

    // No manifest has reached `microsoft/winget-pkgs`, and `release.yml`
    // renders them on a stable tag only, so `winget upgrade` finds nothing.
    // The message has to say that rather than hand out the command — the
    // exact mutation the review planted and nothing caught.
    #[test]
    fn the_winget_message_says_winget_upgrade_would_find_nothing() {
        let InstallPlan::Manual(msg) = install_plan(InstallChannel::Winget, Path::new("")) else {
            panic!("expected Manual");
        };
        assert!(
            msg.contains("Aucun paquet winget n'est encore publié"),
            "{msg}"
        );
        assert!(msg.contains("ne trouverait rien"), "{msg}");
        // The one command it does name *is* reachable: this branch is taken
        // only when winget installed bondebarras, so winget's database holds
        // the identifier `release.yml` writes into the manifests.
        assert!(
            msg.contains("winget uninstall systm-d.bondebarras"),
            "{msg}"
        );
        assert!(msg.contains("releases/latest"), "{msg}");
    }

    // Arch, the first of the three channels `docs/releases.md` promises for.
    // No AUR package was ever submitted (#20), so `yay -S bondebarras` and
    // `pacman -S bondebarras` are precisely the commands that cannot
    // succeed; the PKGBUILD attached to every release is what can.
    #[test]
    fn the_arch_message_names_the_pkgbuild_rather_than_an_aur_helper() {
        let InstallPlan::Manual(msg) = install_plan(InstallChannel::Pacman, Path::new("")) else {
            panic!("expected Manual");
        };
        assert!(msg.contains("PKGBUILD"), "{msg}");
        assert!(msg.contains("makepkg -si"), "{msg}");
        assert!(msg.contains("releases/latest"), "{msg}");
        for helper in ["yay ", "paru ", "pacman -S"] {
            assert!(!msg.contains(helper), "names {helper}: {msg}");
        }
    }

    // macOS, the second. The tap is committed on a stable tag only and none
    // has shipped, so `brew upgrade bondebarras` finds nothing — and that is
    // the exact sentence the second review wrote into this arm, watching the
    // whole suite stay green. `docs/releases.md` made the promise for three
    // channels while one test held it up.
    #[test]
    fn the_homebrew_message_says_no_formula_is_published_yet() {
        let InstallPlan::Manual(msg) = install_plan(InstallChannel::Homebrew, Path::new("")) else {
            panic!("expected Manual");
        };
        assert!(msg.contains("Aucune formule Homebrew"), "{msg}");
        assert!(msg.contains("releases/latest"), "{msg}");
        for command in ["brew upgrade", "brew install", "brew reinstall"] {
            assert!(!msg.contains(command), "names {command}: {msg}");
        }
    }

    // Nix is deliberately left out of the three above rather than forgotten.
    // Its message names no command: it points at the reader's own flake
    // input or channel, so it makes no claim about a published package that
    // could come to be false, and `/nix/store` being read-only is not a fact
    // that rots. The remaining hands-off channel, cargo, is pinned by
    // `cargo_manual_message_names_this_repository` — which fails on `cargo
    // install bondebarras`, the crates.io form that is not published either.

    // The empty-package `Tarball` arm, unreachable from
    // `commands::update::apply` exactly as its `Zip` twin is, and pinned for
    // the same reason — plus one of its own: the second review rewrote it
    // into « Votre bondebarras est déjà à jour » and nothing failed. An arm
    // whose whole job is to point at a newer release told the reader there
    // was none.
    #[test]
    fn the_tarball_plan_without_a_package_still_names_the_archive() {
        let InstallPlan::Manual(msg) = install_plan(InstallChannel::Tarball, Path::new("")) else {
            panic!("expected Manual");
        };
        assert!(msg.contains("archive"), "{msg}");
        assert!(msg.contains("releases/latest"), "{msg}");
        assert!(!msg.contains("à jour"), "{msg}");
    }

    // The reserve the review deleted and watched nothing notice. It is the
    // one thing that genuinely differs on Windows, and dropping it leaves
    // an instruction the user cannot carry out: the swap fails, with no
    // hint as to why.
    #[test]
    fn the_windows_plan_keeps_the_reserve_about_a_running_executable() {
        let package = Path::new(r"C:\Temp\bondebarras-update-a1b2\bondebarras-windows-x86_64.zip");
        let InstallPlan::Manual(msg) = install_plan(InstallChannel::Zip, package) else {
            panic!("expected Manual");
        };
        assert!(msg.contains("fermez-le d'abord"), "{msg}");
        assert!(msg.contains("en cours d'exécution"), "{msg}");
    }

    // The same reserve on the Unix side, worded for the fact that actually
    // holds there: a `cp` over a running binary fails on `ETXTBSY`, while a
    // rename over it succeeds. `Tarball` said nothing at all until the #55
    // review — the real gap the Windows reserve exposed by contrast.
    #[test]
    fn the_tarball_plan_warns_about_overwriting_a_running_binary() {
        let package = Path::new("/tmp/bondebarras-update-a1b2/bondebarras-linux-x86_64.tar.gz");
        let InstallPlan::Manual(msg) = install_plan(InstallChannel::Tarball, package) else {
            panic!("expected Manual");
        };
        assert!(msg.contains("fermez-le d'abord"), "{msg}");
        assert!(msg.contains("Text file busy"), "{msg}");
        // Windows's absolute refusal must not be transplanted onto Unix,
        // where a rename over the file still works.
        assert!(!msg.contains("Windows"), "{msg}");
    }

    // The sentence #55 exists to stop saying, pinned over every channel and
    // both shapes of package path at once. The review rewrote the
    // empty-package `Zip` arm into exactly this sentence and nothing
    // failed. Stated as an equivalence rather than a spot check, no arm can
    // borrow it back — and `Tarball` with an empty package, which really
    // did borrow it, is what this test took it away from.
    #[test]
    fn only_an_unknown_install_is_told_bondebarras_cannot_tell_how_it_was_installed() {
        for channel in EVERY_CHANNEL {
            for package in [Path::new(""), Path::new("/tmp/staging/bondebarras.pkg")] {
                let text = match install_plan(channel, package) {
                    InstallPlan::Manual(msg) => msg,
                    InstallPlan::Run { command, .. } => command.join(" "),
                };
                assert_eq!(
                    text.contains("Impossible de déterminer"),
                    channel == InstallChannel::Unknown,
                    "{channel:?} with {package:?}: {text}"
                );
            }
        }
    }

    // The empty-package `Zip` arm, unreachable from
    // `commands::update::apply` — which reaches `install_plan` with an
    // empty path only on a channel that downloads nothing, and `Zip`
    // downloads the `.zip`. Pinned rather than deleted: deleting it folds
    // `Zip` into the `Unknown` arm and hands a Windows user the one
    // sentence #55 removed, so the guard belongs on the arm rather than on
    // a caller's discipline.
    #[test]
    fn the_windows_plan_without_a_package_still_names_the_windows_archive() {
        let InstallPlan::Manual(msg) = install_plan(InstallChannel::Zip, Path::new("")) else {
            panic!("expected Manual");
        };
        assert!(msg.contains("archive Windows"), "{msg}");
        assert!(msg.contains("bondebarras.exe"), "{msg}");
        assert!(msg.contains("releases/latest"), "{msg}");
    }

    // The `.zip` carries its platform token exactly as the `.tar.gz` does,
    // so #49's refusal covers Windows rather than being bypassed by it: a
    // `windows-aarch64` machine, which the release workflow builds nothing
    // for, comes back empty-handed instead of being handed the x86-64
    // archive. The sidecar follows the archive by full name, so the
    // fail-closed verification is inherited, not re-implemented.
    #[test]
    fn the_windows_archive_is_platform_matched_and_carries_its_own_checksum() {
        let r = parse_release(SAMPLE).unwrap();
        assert!(
            r.asset_for(InstallChannel::Zip, "windows-aarch64")
                .is_none()
        );
        let zip = r.asset_for(InstallChannel::Zip, WINDOWS).unwrap();
        assert_eq!(
            r.checksum_for(zip).unwrap().name,
            "bondebarras-windows-x86_64.zip.sha256"
        );
    }

    // The Windows plan names the verified file and the file to replace, and
    // the one thing that genuinely differs on Windows — a running `.exe`
    // cannot be overwritten in place. It must not suggest bondebarras
    // replaces the binary itself anywhere else: it never has, on any
    // platform.
    #[test]
    fn the_windows_plan_points_at_the_verified_archive_and_the_exe_to_replace() {
        let package = Path::new(r"C:\Temp\bondebarras-update-a1b2\bondebarras-windows-x86_64.zip");
        let InstallPlan::Manual(msg) = install_plan(InstallChannel::Zip, package) else {
            panic!("expected Manual");
        };
        assert!(msg.contains(&package.display().to_string()), "{msg}");
        assert!(msg.contains("bondebarras.exe"), "{msg}");
        assert!(msg.contains("vérifiée"), "{msg}");
    }

    // The Windows branch must stay a Windows branch. A Linux or macOS path
    // with no package owner is still a tarball copy, and a home-directory
    // binary with nothing to say about it is still honestly `Unknown` —
    // that state did not become dishonest, it became Windows-free.
    #[test]
    fn the_windows_branch_does_not_leak_onto_the_other_platforms() {
        for os in [LINUX_OS, MACOS_OS] {
            assert_eq!(
                detect_channel_for(Path::new("/usr/bin/bondebarras"), os, |_bin, _exe| false),
                InstallChannel::Tarball,
                "{os}"
            );
            assert_eq!(
                detect_channel_for(Path::new("/home/x/bin/bondebarras"), os, |_bin, _exe| false),
                InstallChannel::Unknown,
                "{os}"
            );
        }
    }

    #[test]
    fn install_plan_uses_apt_for_deb() {
        let plan = install_plan(InstallChannel::Deb, Path::new("/tmp/b.deb"));
        assert_eq!(
            plan,
            InstallPlan::Run {
                command: vec!["apt".into(), "install".into(), "/tmp/b.deb".into()],
                sudo: true,
            }
        );
    }

    #[test]
    fn install_plan_uses_dnf_for_rpm() {
        let plan = install_plan(InstallChannel::Rpm, Path::new("/tmp/b.rpm"));
        assert_eq!(
            plan,
            InstallPlan::Run {
                command: vec!["dnf".into(), "install".into(), "/tmp/b.rpm".into()],
                sudo: true,
            }
        );
    }

    // The five channels this project must never touch on its own behalf —
    // this is the direct test of "self-replace is the last resort, not the
    // default": each of these must resolve to `Manual`, never `Run`.
    // `Winget` joined them in #55 for the reason Homebrew and pacman are
    // there: it records the file it installed, and a hand swap would leave
    // its database describing a version that is no longer on disk.
    #[test]
    fn the_five_hands_off_channels_are_always_manual() {
        for channel in [
            InstallChannel::Pacman,
            InstallChannel::Homebrew,
            InstallChannel::Nix,
            InstallChannel::Cargo,
            InstallChannel::Winget,
        ] {
            assert!(
                matches!(install_plan(channel, Path::new("")), InstallPlan::Manual(_)),
                "{channel:?} must never produce a Run plan"
            );
        }
    }

    #[test]
    fn cargo_manual_message_names_this_repository() {
        let InstallPlan::Manual(msg) = install_plan(InstallChannel::Cargo, Path::new("")) else {
            panic!("expected Manual");
        };
        assert!(msg.contains("cargo install"));
        assert!(msg.contains("systm-d/bondebarras"));
    }

    #[test]
    fn sha256_line_takes_first_field() {
        assert_eq!(
            parse_sha256_line("abc123  bondebarras.deb\n").as_deref(),
            Some("abc123")
        );
    }

    fn temp_file_with(tag: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "bondebarras-sha256-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, bytes).expect("writing the temp file");
        path
    }

    #[test]
    fn sha256_hex_matches_the_known_vector() {
        let path = temp_file_with("known", b"hello");
        let digest = sha256_hex(&path).expect("hashing");
        std::fs::remove_file(&path).ok();

        // `printf 'hello' | sha256sum`
        assert_eq!(
            digest,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn sha256_hex_of_an_empty_file_is_the_empty_digest() {
        let path = temp_file_with("empty", b"");
        let digest = sha256_hex(&path).expect("hashing");
        std::fs::remove_file(&path).ok();

        assert_eq!(
            digest,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// The file is read through a 64 KiB buffer, so anything larger exercises
    /// the loop's chunk boundaries — where an off-by-one would hide.
    #[test]
    fn sha256_hex_spans_several_buffer_fills() {
        use sha2::{Digest, Sha256};

        let bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let path = temp_file_with("large", &bytes);
        let from_file = sha256_hex(&path).expect("hashing");
        std::fs::remove_file(&path).ok();

        let one_shot = Sha256::digest(&bytes);
        let expected: String = one_shot.iter().map(|b| format!("{b:02x}")).collect();

        assert_eq!(from_file, expected);
    }

    // Security review finding (HIGH): `asset.name` comes straight off the
    // GitHub API response and used to get joined into a filesystem path
    // (`dir.join(&asset.name)`) and, for Deb/Rpm, handed to `sudo apt
    // install`/`sudo dnf install` as an argv element. A wrong implementation
    // that only checks emptiness, or that "sanitises" by stripping instead
    // of refusing, would still let `../../etc/whatever` escape the staging
    // directory or let `-x` be read as a flag by the package manager.
    #[test]
    fn validate_asset_name_refuses_a_path_separator() {
        assert_eq!(
            validate_asset_name("../../etc/passwd"),
            Err(UnsafeAssetName::ContainsPathSeparator)
        );
        assert_eq!(
            validate_asset_name("sub/dir.deb"),
            Err(UnsafeAssetName::ContainsPathSeparator)
        );
        assert_eq!(
            validate_asset_name("sub\\dir.deb"),
            Err(UnsafeAssetName::ContainsPathSeparator)
        );
    }

    #[test]
    fn validate_asset_name_refuses_dot_and_dot_dot() {
        assert_eq!(
            validate_asset_name("."),
            Err(UnsafeAssetName::IsDotOrDotDot)
        );
        assert_eq!(
            validate_asset_name(".."),
            Err(UnsafeAssetName::IsDotOrDotDot)
        );
    }

    #[test]
    fn validate_asset_name_refuses_a_leading_dash() {
        // `apt install -x` / `dnf install --reinstall` — a name starting
        // with `-` would be read as an option, not a filename.
        assert_eq!(
            validate_asset_name("-x.deb"),
            Err(UnsafeAssetName::StartsWithDash)
        );
    }

    #[test]
    fn validate_asset_name_refuses_an_empty_name() {
        assert_eq!(validate_asset_name(""), Err(UnsafeAssetName::Empty));
    }

    #[test]
    fn validate_asset_name_accepts_an_ordinary_release_filename() {
        assert_eq!(
            validate_asset_name("bondebarras-linux-x86_64.tar.gz"),
            Ok(())
        );
        assert_eq!(validate_asset_name("bondebarras_0.6.0-1_amd64.deb"), Ok(()));
    }

    #[test]
    fn sha256_hex_rejects_a_mismatched_expectation() {
        // Not a real function under test on its own — this documents the
        // property the caller (commands::update::verify) relies on: two
        // different files must not hash equal, or "refuse on mismatch" would
        // never trigger.
        let a = temp_file_with("mismatch-a", b"hello");
        let b = temp_file_with("mismatch-b", b"hellp");
        let da = sha256_hex(&a).unwrap();
        let db = sha256_hex(&b).unwrap();
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
        assert_ne!(da, db);
    }
}
