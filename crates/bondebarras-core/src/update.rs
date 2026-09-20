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
    /// `.zip` and a bare `.exe`, which this suffix never matches — so
    /// matching on the suffix alone returned whichever of those two GitHub
    /// happened to list first: a macOS user was offered
    /// `bondebarras-linux-x86_64.tar.gz`, a download that passes its
    /// checksum and then cannot execute. Injecting the target is also
    /// what makes that testable — CI builds on five platforms, so a test
    /// keyed on the host's own target would assert something different on
    /// each one.
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
    Tarball,
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
            InstallChannel::Pacman
            | InstallChannel::Cargo
            | InstallChannel::Homebrew
            | InstallChannel::Nix
            | InstallChannel::Unknown => None,
        }
    }

    /// Whether bondebarras downloads a release asset for this channel at
    /// all. The four hands-off channels never do — their update is a
    /// command the user runs, which [`install_plan`] spells out — so "no
    /// asset matched" means something else entirely for them than for the
    /// three that do, and must not borrow the same refusal.
    pub fn downloads_an_asset(self) -> bool {
        self.package_suffix().is_some()
    }

    /// Whether this channel's release assets carry the `<os>-<arch>` token
    /// in their filename. The `binaries` matrix in
    /// `.github/workflows/release.yml` packages each build as
    /// `bondebarras-<matrix.name>.tar.gz` (`.zip`/`.exe` on Windows), so a
    /// `.tar.gz` is only ever *this* machine's archive when that token
    /// matches [`current_target`].
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
            InstallChannel::Tarball => true,
            InstallChannel::Deb | InstallChannel::Rpm => false,
            InstallChannel::Pacman
            | InstallChannel::Cargo
            | InstallChannel::Homebrew
            | InstallChannel::Nix
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

/// Best-effort detection of how the running binary was installed.
pub fn detect_channel() -> InstallChannel {
    let exe = std::env::current_exe().unwrap_or_default();
    detect_channel_for(&exe, package_owns)
}

/// The testable core of channel detection: the package-manager check is
/// injected so tests can simulate "dpkg claims this file" without depending
/// on the test machine's actual package database.
///
/// Two signals, in order: the path first (`.cargo/`, `linuxbrew`/`Cellar`,
/// `/nix/store/` are unambiguous — no manager is ever consulted for them),
/// then the package manager asked directly about the resolved path. A bare
/// system path like `/usr/bin` proves nothing on its own, so only the
/// manager's answer decides there.
fn detect_channel_for(exe: &Path, owns: impl Fn(&str, &Path) -> bool) -> InstallChannel {
    let path = exe.to_string_lossy();

    if let Some(channel) = channel_from_path(&path) {
        return channel;
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

/// The path-only part of channel detection (no system calls) — unit-testable
/// on its own.
fn channel_from_path(path: &str) -> Option<InstallChannel> {
    if path.contains("/.cargo/") {
        Some(InstallChannel::Cargo)
    } else if path.contains("linuxbrew") || path.contains("/Cellar/") {
        Some(InstallChannel::Homebrew)
    } else if path.starts_with("/nix/store/") {
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
/// Self-replace is the last resort, never the default: only `Tarball` and
/// `Unknown` fall through to it, and even then only as a pointer at the
/// already-downloaded, checksum-verified file — see the module docs on why
/// bondebarras doesn't extract the archive itself. Every channel owned by a
/// package manager (`Deb`, `Rpm`) or fenced off from one (`Pacman`,
/// `Homebrew`, `Nix`, `Cargo`) is handled without ever touching the binary
/// directly.
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
        InstallChannel::Tarball if !pkg.is_empty() => InstallPlan::Manual(format!(
            "L'archive a été téléchargée et son empreinte vérifiée : {pkg}. \
             Extrayez-la et remplacez votre binaire par celui qu'elle contient."
        )),
        InstallChannel::Tarball | InstallChannel::Unknown => InstallPlan::Manual(format!(
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
            "Cette release ne publie aucun paquet {kind} — rien n'est téléchargé, et bondebarras \
             ne remplace pas un paquet par un autre format. Consultez {page}"
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
            {"name": "bondebarras-linux-x86_64.tar.gz.sha256", "browser_download_url": "https://example/tgz.sha256", "size": 1}
        ]
    }"#;

    // The three `matrix.name` values in `.github/workflows/release.yml`,
    // spelled as it spells them — and as `current_target` composes them.
    const LINUX: &str = "linux-x86_64";
    const MACOS: &str = "macos-aarch64";
    const WINDOWS: &str = "windows-x86_64";

    #[test]
    fn parse_release_extracts_version_and_assets() {
        let r = parse_release(SAMPLE).unwrap();
        assert_eq!(r.tag, "v0.6.0");
        assert_eq!(r.version, "0.6.0");
        assert_eq!(r.assets.len(), 6);
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
    // the one CI happens to run on, of which there are five.
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

    // Whichever of the five CI platforms this runs on, the token must be
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

    // Only the three downloading channels can be told "nothing for your
    // platform"; the other five never download anything, and get their own
    // install plan instead — `commands::update::apply` branches on this.
    #[test]
    fn only_the_downloading_channels_ever_claim_an_asset() {
        for channel in [
            InstallChannel::Deb,
            InstallChannel::Rpm,
            InstallChannel::Tarball,
        ] {
            assert!(channel.downloads_an_asset(), "{channel:?}");
        }
        for channel in [
            InstallChannel::Pacman,
            InstallChannel::Homebrew,
            InstallChannel::Nix,
            InstallChannel::Cargo,
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
            channel_from_path("/home/x/.cargo/bin/bondebarras"),
            Some(InstallChannel::Cargo)
        );
        assert_eq!(
            channel_from_path("/home/linuxbrew/.linuxbrew/bin/bondebarras"),
            Some(InstallChannel::Homebrew)
        );
        assert_eq!(
            channel_from_path("/opt/homebrew/Cellar/bondebarras/0.5.0/bin/bondebarras"),
            Some(InstallChannel::Homebrew)
        );
        assert_eq!(
            channel_from_path("/nix/store/abcd1234-bondebarras-0.5.0/bin/bondebarras"),
            Some(InstallChannel::Nix)
        );
        // A bare system path proves nothing on its own — this is the seam
        // where channel detection must fall through to asking a package
        // manager instead of guessing from the path alone.
        assert_eq!(channel_from_path("/usr/bin/bondebarras"), None);
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
    #[test]
    fn package_manager_ownership_overrides_the_bare_path_guess() {
        let exe = Path::new("/usr/bin/bondebarras");

        // No manager claims it: path alone can't tell us anything better
        // than "sitting under /usr with no owner" — a manual tarball copy.
        assert_eq!(
            detect_channel_for(exe, |_bin, _exe| false),
            InstallChannel::Tarball
        );

        // rpm claims it: the manager's answer must win over the path guess.
        assert_eq!(
            detect_channel_for(exe, |bin, _exe| bin == "rpm"),
            InstallChannel::Rpm
        );
        // dpkg claims it instead.
        assert_eq!(
            detect_channel_for(exe, |bin, _exe| bin == "dpkg"),
            InstallChannel::Deb
        );
        // pacman claims it instead.
        assert_eq!(
            detect_channel_for(exe, |bin, _exe| bin == "pacman"),
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
            detect_channel_for(exe, |_bin, _exe| true),
            InstallChannel::Cargo
        );
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

    // The four channels this project must never touch on its own behalf —
    // this is the direct test of "self-replace is the last resort, not the
    // default": each of these must resolve to `Manual`, never `Run`.
    #[test]
    fn the_four_hands_off_channels_are_always_manual() {
        for channel in [
            InstallChannel::Pacman,
            InstallChannel::Homebrew,
            InstallChannel::Nix,
            InstallChannel::Cargo,
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
