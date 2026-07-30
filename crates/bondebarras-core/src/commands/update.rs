//! `bondebarras update` — check GitHub Releases and, with the caution the
//! detected install channel calls for, propose or apply the update.
//!
//! This is the only place bondebarras reaches this endpoint, and only on an
//! explicit `bondebarras update` — never automatically when the TUI starts
//! (see the design doc, §6: "un outil de nettoyage n'a pas à parler à un
//! serveur de release pendant qu'on lui demande de scanner des orgs"). No
//! GitHub token is required: the repository is public, and a version check
//! must never need authentication — see `bondebarras_core::run` for how this
//! command is dispatched before the token is resolved.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::update::{self, Asset, InstallPlan, ReleaseInfo, UpdateStatus};

const USER_AGENT: &str = concat!("bondebarras/", env!("CARGO_PKG_VERSION"));

/// Outcome of checking a downloaded asset against its published checksum.
/// Three distinct ways this can fail to come back `Verified`, each refused
/// on (see `verify_outcome`) — a security review of this command found that
/// collapsing the first two into one "proceed anyway" state was the gap: an
/// attacker able to interfere with just the sidecar request (not the release
/// JSON itself) would produce `ChecksumUnavailable`, and it must not degrade
/// to the same non-decision as a release that never published a checksum.
enum Integrity {
    /// The digests match.
    Verified,
    /// This release lists no `.sha256` asset for this file at all.
    NoChecksumPublished,
    /// A `.sha256` asset is listed, but fetching or parsing it failed.
    ChecksumUnavailable,
    /// A `.sha256` asset was fetched and parsed, and it disagrees with the
    /// downloaded file's own digest.
    Mismatch,
}

/// Run `bondebarras update` (or `--check`).
pub fn run(check_only: bool) -> Result<()> {
    let release = match fetch_latest(update::LATEST_RELEASE_URL) {
        Ok(Some(release)) => release,
        Ok(None) => {
            println!(
                "Aucune release publiée pour bondebarras sur GitHub pour l'instant — rien à comparer."
            );
            return Ok(());
        }
        Err(e) => {
            println!("Impossible de joindre GitHub pour vérifier les mises à jour.");
            println!("(détail : {e})");
            return Ok(());
        }
    };

    let current = update::current_version();
    match update::compare(current, &release.version) {
        UpdateStatus::UpToDate => println!("bondebarras {current} est déjà à jour."),
        UpdateStatus::Ahead => println!(
            "Votre build local ({current}) est plus récent que la dernière release publiée \
             ({}). Rien à faire.",
            release.version
        ),
        UpdateStatus::Available(version) => {
            println!("Une nouvelle version est disponible : {version} (vous avez {current}).");
            if !release.html_url.is_empty() {
                println!("Notes de version : {}", release.html_url);
            }
            if check_only {
                println!("Lancez `bondebarras update` pour l'installer.");
            } else {
                apply(&release)?;
            }
        }
    }
    Ok(())
}

/// Fetch and parse the latest release, distinguishing "nothing published
/// yet" (HTTP 404 — this repository's real state as of writing this command)
/// from a genuine network or parsing failure. `url` is injected so tests can
/// point this at a wiremock server, mirroring `api::Client::with_base`.
fn fetch_latest(url: &str) -> Result<Option<ReleaseInfo>> {
    match ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .config()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .call()
    {
        Ok(mut resp) => {
            let body = resp
                .body_mut()
                .read_to_string()
                .context("lecture de la réponse de GitHub")?;
            update::parse_release(&body).map(Some)
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(e).context("requête vers l'API GitHub"),
    }
}

/// Propose or apply the update, once we know a newer version exists.
fn apply(release: &ReleaseInfo) -> Result<()> {
    let channel = update::detect_channel();

    // Pacman/Homebrew/Nix/Cargo always land here (no package_suffix at all —
    // bondebarras never touches those binaries itself). Deb/Rpm/Tarball land
    // here too, but only when this specific release genuinely ships nothing
    // for this platform — worth a distinct message rather than the generic
    // "no asset" case those channels never hit for a well-formed release.
    let Some(asset) = release.asset_for(channel) else {
        match update::install_plan(channel, Path::new("")) {
            InstallPlan::Manual(message) => println!("{message}"),
            InstallPlan::Run { .. } => println!(
                "Cette release ne publie pas de paquet pour votre plateforme. Consultez {}",
                release.html_url
            ),
        }
        return Ok(());
    };

    // `dir` is a `tempfile::TempDir` guard: dropping it deletes the
    // directory (and the package inside it) from disk. It is bound here, at
    // the top of `apply`, and never moved into a narrower scope — Rust drops
    // an owned local at the end of its *enclosing scope*, regardless of when
    // it was last read (that rule doesn't move for NLL, which only affects
    // reference liveness, not destructor timing), so `dir` stays alive for
    // the rest of this function. That includes `run_install` below, which
    // needs the `.deb`/`.rpm` file to still exist on disk while `apt`/`dnf`
    // read it — dropping `dir` any earlier would delete the package out from
    // under them mid-install.
    let dir = staging_dir()?;
    println!("Téléchargement de {} …", asset.name);
    let package = download(asset, dir.path())?;

    match verify_outcome(&verify(release, asset, &package)) {
        VerifyOutcome::Refuse(message) => {
            println!("{message}");
            return Ok(());
        }
        VerifyOutcome::Proceed => println!("Intégrité vérifiée ✓"),
    }

    match update::install_plan(channel, &package) {
        InstallPlan::Run { command, sudo } => {
            println!("Installation de la nouvelle version — votre mot de passe peut être demandé.");
            let status = run_install(&command, sudo)?;
            if status.success() {
                println!("bondebarras mis à jour vers {}.", release.version);
            } else {
                println!(
                    "L'installation ne s'est pas terminée. Le paquet reste prêt ici : {}",
                    package.display()
                );
            }
        }
        InstallPlan::Manual(message) => println!("{message}"),
    }
    Ok(())
}

/// A fresh, single-use staging directory with a random, non-guessable
/// suffix, created with `O_EXCL` semantics (an existing path is an error,
/// never silently reused).
///
/// Security review finding (HIGH): the previous version joined the process
/// id onto a fixed prefix under a shared, world-writable temp directory —
/// a PID is small and guessable, and `create_dir_all` succeeds on a
/// directory that already exists. A local attacker could pre-create
/// `/tmp/bondebarras-update-<pid>` (or race the window between `verify` and
/// `run_install`) and swap the package `sudo apt install` was about to run.
/// `tempfile` closes both: the random suffix defeats guessing, and creation
/// fails outright on a collision instead of reusing whatever is already
/// there.
fn staging_dir() -> Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix("bondebarras-update-")
        .tempdir()
        .context("création du dossier de préparation")
}

/// Download `asset` into `dir`, refusing first if its name isn't a plain,
/// single-component filename — see `update::validate_asset_name` for why
/// (security review finding, HIGH: an unvalidated name could escape `dir`
/// via `../`, or be read as a flag by `apt`/`dnf` if it started with `-`).
/// Checked before any network call, so a hostile name never even reaches
/// the download.
fn download(asset: &Asset, dir: &Path) -> Result<PathBuf> {
    update::validate_asset_name(&asset.name)
        .with_context(|| format!("nom d'asset refusé : « {} »", asset.name))?;
    let dest = dir.join(&asset.name);

    let mut reader = ureq::get(&asset.download_url)
        .header("User-Agent", USER_AGENT)
        .call()
        .context("téléchargement du paquet")?
        .into_body()
        .into_reader();
    let mut file =
        std::fs::File::create(&dest).with_context(|| format!("création de {}", dest.display()))?;
    io::copy(&mut reader, &mut file).context("écriture du paquet téléchargé")?;

    // World-readable so a sandboxed installer user (e.g. apt's `_apt`) can
    // read the package during install. Unix-only: `PermissionsExt` doesn't
    // exist on other platforms, and Windows installers don't need this.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o644))
            .with_context(|| format!("réglage des permissions de {}", dest.display()))?;
    }
    Ok(dest)
}

/// Compare a downloaded asset against its published `.sha256`, distinguishing
/// *why* it isn't `Verified` — the caller (`verify_outcome`) refuses on all
/// three other cases, but the message differs.
fn verify(release: &ReleaseInfo, asset: &Asset, package: &Path) -> Integrity {
    let Some(sum_asset) = release.checksum_for(asset) else {
        return Integrity::NoChecksumPublished;
    };
    let Some(body) = ureq::get(&sum_asset.download_url)
        .header("User-Agent", USER_AGENT)
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_to_string().ok())
    else {
        return Integrity::ChecksumUnavailable;
    };
    let Some(expected) = update::parse_sha256_line(&body) else {
        return Integrity::ChecksumUnavailable;
    };
    // Hashing our own just-downloaded file failing is a local I/O problem,
    // not something to report as a network-shaped message — but the safe
    // posture is the same refusal, so it folds into the same state.
    let Ok(actual) = update::sha256_hex(package) else {
        return Integrity::ChecksumUnavailable;
    };

    if expected == actual {
        Integrity::Verified
    } else {
        Integrity::Mismatch
    }
}

/// What `apply` does with a verification outcome. Only `Verified` proceeds —
/// every other state refuses to install, full stop. This is the direct fix
/// for the security review's MEDIUM finding: verification used to fail
/// *open* (a missing or unfetchable checksum still installed); now it fails
/// *closed*. Concretely, this means `bondebarras update` installs nothing at
/// all until a release publishes a matching checksum — see the `release.yml`
/// change in this same branch, which is what makes that achievable rather
/// than a permanent no-op.
enum VerifyOutcome {
    Proceed,
    Refuse(&'static str),
}

fn verify_outcome(integrity: &Integrity) -> VerifyOutcome {
    match integrity {
        Integrity::Verified => VerifyOutcome::Proceed,
        Integrity::NoChecksumPublished => VerifyOutcome::Refuse(
            "⚠ Cette release ne publie aucune empreinte pour ce fichier. Par prudence, rien \
             n'est installé.",
        ),
        Integrity::ChecksumUnavailable => VerifyOutcome::Refuse(
            "⚠ Impossible de vérifier l'empreinte de ce fichier (somme injoignable ou \
             illisible). Par prudence, rien n'est installé — réessayez plus tard.",
        ),
        Integrity::Mismatch => VerifyOutcome::Refuse(
            "⚠ L'empreinte du fichier téléchargé ne correspond pas à celle publiée. Par \
             prudence, rien n'est installé — vérifiez votre connexion et réessayez.",
        ),
    }
}

/// Prepend `sudo` to `command` when required. Pure and unit-testable —
/// spawning the process itself isn't.
fn install_argv(command: &[String], sudo: bool) -> Vec<String> {
    if !sudo {
        return command.to_vec();
    }
    let mut argv = Vec::with_capacity(command.len() + 1);
    argv.push("sudo".to_string());
    argv.extend(command.iter().cloned());
    argv
}

fn run_install(command: &[String], sudo: bool) -> Result<ExitStatus> {
    let argv = install_argv(command, sudo);
    let (program, args) = argv
        .split_first()
        .expect("install_plan ne produit jamais de commande vide");
    // Inherit stdio so the user can type their sudo password.
    Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("lancement de `{}`", argv.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // The scenario this repository is actually in as of writing this
    // command: nothing has been tagged yet, so GitHub answers 404. A wrong
    // implementation that treats any non-2xx the same as a network failure,
    // or that silently defaults to "up to date", would fail this test.
    #[tokio::test]
    async fn fetch_latest_treats_a_404_as_no_release_yet_not_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/bondebarras/releases/latest"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let url = format!("{}/repos/systm-d/bondebarras/releases/latest", server.uri());
        let result = fetch_latest(&url).expect("a 404 must not surface as Err");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn fetch_latest_parses_a_real_payload() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/bondebarras/releases/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tag_name": "v0.6.0",
                "html_url": "https://github.com/systm-d/bondebarras/releases/tag/v0.6.0",
                "body": "Notes",
                "assets": []
            })))
            .mount(&server)
            .await;

        let url = format!("{}/repos/systm-d/bondebarras/releases/latest", server.uri());
        let release = fetch_latest(&url)
            .unwrap()
            .expect("a 200 must parse to Some");
        assert_eq!(release.version, "0.6.0");
    }

    #[tokio::test]
    async fn fetch_latest_surfaces_a_genuine_server_error_distinctly_from_a_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/bondebarras/releases/latest"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let url = format!("{}/repos/systm-d/bondebarras/releases/latest", server.uri());
        // Must be an Err, not the Ok(None) a 404 produces — otherwise a real
        // outage would be indistinguishable from "no release published yet".
        assert!(fetch_latest(&url).is_err());
    }

    fn asset(name: &str, url: &str) -> Asset {
        Asset {
            name: name.to_string(),
            download_url: url.to_string(),
            size: 0,
        }
    }

    fn release_with(assets: Vec<Asset>) -> ReleaseInfo {
        ReleaseInfo {
            tag: "v0.6.0".into(),
            version: "0.6.0".into(),
            html_url: String::new(),
            notes: String::new(),
            assets,
        }
    }

    #[tokio::test]
    async fn verify_matches_a_correct_published_checksum() {
        let server = MockServer::start().await;
        let bytes = b"the package contents";
        let digest = {
            use sha2::{Digest, Sha256};
            let d = Sha256::digest(bytes);
            d.iter().map(|b| format!("{b:02x}")).collect::<String>()
        };
        Mock::given(method("GET"))
            .and(path("/pkg.tar.gz.sha256"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(format!("{digest}  pkg.tar.gz")),
            )
            .mount(&server)
            .await;

        let dir = std::env::temp_dir().join(format!(
            "bondebarras-verify-ok-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let package = dir.join("pkg.tar.gz");
        std::fs::write(&package, bytes).unwrap();

        let a = asset("pkg.tar.gz", "unused");
        let sum = asset(
            "pkg.tar.gz.sha256",
            &format!("{}/pkg.tar.gz.sha256", server.uri()),
        );
        let release = release_with(vec![a.clone(), sum]);

        assert!(matches!(
            verify(&release, &a, &package),
            Integrity::Verified
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    // The other half of "refuse on mismatch": a checksum that doesn't match
    // the downloaded bytes must come back `Mismatch`, not `Verified` or
    // silently `Unverified` — either of those would let `apply` install a
    // tampered or corrupted package.
    #[tokio::test]
    async fn verify_flags_a_checksum_that_does_not_match_the_file() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pkg.tar.gz.sha256"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "0000000000000000000000000000000000000000000000000000000000000000  pkg.tar.gz",
            ))
            .mount(&server)
            .await;

        let dir = std::env::temp_dir().join(format!(
            "bondebarras-verify-bad-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let package = dir.join("pkg.tar.gz");
        std::fs::write(&package, b"the package contents").unwrap();

        let a = asset("pkg.tar.gz", "unused");
        let sum = asset(
            "pkg.tar.gz.sha256",
            &format!("{}/pkg.tar.gz.sha256", server.uri()),
        );
        let release = release_with(vec![a.clone(), sum]);

        assert!(matches!(
            verify(&release, &a, &package),
            Integrity::Mismatch
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    // A release that simply never listed a `.sha256` asset — distinct from
    // one that listed it but the fetch failed (see the next test). Security
    // review finding (MEDIUM): these two used to collapse into the same
    // `Unverified` state and both fell through to installing anyway.
    #[test]
    fn verify_reports_no_checksum_published_when_the_release_has_no_sidecar() {
        let dir = std::env::temp_dir().join(format!(
            "bondebarras-verify-none-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let package = dir.join("pkg.tar.gz");
        std::fs::write(&package, b"anything").unwrap();

        let a = asset("pkg.tar.gz", "unused");
        let release = release_with(vec![a.clone()]);

        assert!(matches!(
            verify(&release, &a, &package),
            Integrity::NoChecksumPublished
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    // The MEDIUM finding itself: a `.sha256` asset IS listed in the release,
    // but fetching it fails (404 here — could as easily be a network
    // hiccup). An attacker able to interfere with just the sidecar request,
    // without touching the release JSON itself, produces exactly this. It
    // must not degrade to the same "proceed anyway" as a release that never
    // published a checksum at all — both are named states, and both refuse.
    #[tokio::test]
    async fn verify_reports_checksum_unavailable_when_the_sidecar_fetch_fails() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pkg.tar.gz.sha256"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let dir = std::env::temp_dir().join(format!(
            "bondebarras-verify-unavailable-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let package = dir.join("pkg.tar.gz");
        std::fs::write(&package, b"anything").unwrap();

        let a = asset("pkg.tar.gz", "unused");
        let sum = asset(
            "pkg.tar.gz.sha256",
            &format!("{}/pkg.tar.gz.sha256", server.uri()),
        );
        let release = release_with(vec![a.clone(), sum]);

        assert!(matches!(
            verify(&release, &a, &package),
            Integrity::ChecksumUnavailable
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    // Same state, different cause: the sidecar fetch succeeds but the body
    // isn't a `sha256sum`-shaped line at all (empty here). Also refuses.
    #[tokio::test]
    async fn verify_reports_checksum_unavailable_when_the_sidecar_body_is_unparsable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pkg.tar.gz.sha256"))
            .respond_with(ResponseTemplate::new(200).set_body_string(""))
            .mount(&server)
            .await;

        let dir = std::env::temp_dir().join(format!(
            "bondebarras-verify-unparsable-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let package = dir.join("pkg.tar.gz");
        std::fs::write(&package, b"anything").unwrap();

        let a = asset("pkg.tar.gz", "unused");
        let sum = asset(
            "pkg.tar.gz.sha256",
            &format!("{}/pkg.tar.gz.sha256", server.uri()),
        );
        let release = release_with(vec![a.clone(), sum]);

        assert!(matches!(
            verify(&release, &a, &package),
            Integrity::ChecksumUnavailable
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    // The property that matters: `update` must refuse to install anything it
    // could not check — a missing or unfetchable checksum is not a lesser
    // case than a mismatched one, it is the same case. A wrong
    // implementation that lets any non-`Verified` state fall through to
    // installing (what this command did before this test existed, and what
    // josephine still does for a missing sidecar) would pass every other
    // test in this file while failing this one.
    #[test]
    fn verify_outcome_refuses_to_install_when_no_checksum_was_published() {
        assert!(matches!(
            verify_outcome(&Integrity::NoChecksumPublished),
            VerifyOutcome::Refuse(_)
        ));
    }

    #[test]
    fn verify_outcome_refuses_to_install_when_the_checksum_could_not_be_fetched() {
        assert!(matches!(
            verify_outcome(&Integrity::ChecksumUnavailable),
            VerifyOutcome::Refuse(_)
        ));
    }

    #[test]
    fn verify_outcome_refuses_to_install_on_a_mismatch() {
        assert!(matches!(
            verify_outcome(&Integrity::Mismatch),
            VerifyOutcome::Refuse(_)
        ));
    }

    #[test]
    fn verify_outcome_proceeds_only_when_verified() {
        assert!(matches!(
            verify_outcome(&Integrity::Verified),
            VerifyOutcome::Proceed
        ));
    }

    // The three refusal messages must actually differ — a single generic
    // "refused" string would leave the user unable to tell "this release
    // never shipped a checksum" from "your network ate the checksum
    // request" from "someone tampered with the download".
    #[test]
    fn the_three_refusal_messages_are_distinct() {
        let msg = |i: Integrity| match verify_outcome(&i) {
            VerifyOutcome::Refuse(m) => m,
            VerifyOutcome::Proceed => panic!("expected Refuse"),
        };
        let no_checksum = msg(Integrity::NoChecksumPublished);
        let unavailable = msg(Integrity::ChecksumUnavailable);
        let mismatch = msg(Integrity::Mismatch);
        assert_ne!(no_checksum, unavailable);
        assert_ne!(no_checksum, mismatch);
        assert_ne!(unavailable, mismatch);
    }

    // Security review finding (HIGH): a release asset named e.g.
    // `../../etc/passwd` must never reach `dir.join(&asset.name)`. Validated
    // before any network call, so this needs no mock server — a wrong
    // implementation that validates only after downloading (or not at all)
    // would still fail this, since the download URL here is unreachable and
    // would surface as a different error if `download` ever tried it.
    #[test]
    fn download_refuses_an_asset_name_that_escapes_the_staging_directory() {
        let a = asset("../../etc/passwd", "http://unused.invalid/x");
        let dir = std::env::temp_dir();
        assert!(download(&a, &dir).is_err());
    }

    #[test]
    fn install_argv_prefixes_sudo_only_when_required() {
        let cmd = vec![
            "apt".to_string(),
            "install".to_string(),
            "/tmp/x.deb".to_string(),
        ];
        assert_eq!(
            install_argv(&cmd, true),
            vec!["sudo", "apt", "install", "/tmp/x.deb"]
        );
        assert_eq!(install_argv(&cmd, false), cmd);
    }
}
