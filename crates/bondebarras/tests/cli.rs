use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn help_lists_the_subcommands() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("scan"));
}

#[test]
fn version_is_reported() {
    // Asserts against the crate's own `CARGO_PKG_VERSION`, not a literal —
    // a hardcoded "0.1.0" is exactly what let the workspace version drift
    // two releases behind `CHANGELOG.md` without a test ever catching it.
    Command::cargo_bin("bondebarras")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn scan_json_is_documented_in_help() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .args(["scan", "--help"])
        .assert()
        .success()
        .stdout(contains("--json"));
}

#[test]
fn clean_requires_yes_to_delete() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .args(["clean", "--help"])
        .assert()
        .success()
        .stdout(contains("--yes"))
        .stdout(contains("--stale-pr"));
}

#[test]
fn clean_help_documents_the_packages_flag() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .args(["clean", "--help"])
        .assert()
        .success()
        .stdout(contains("--packages"));
}

#[test]
fn clean_help_documents_the_v04_family_flags() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .args(["clean", "--help"])
        .assert()
        .success()
        .stdout(contains("--branches"))
        .stdout(contains("--tags"))
        .stdout(contains("--assets"));
}

#[test]
fn update_is_listed_and_documents_its_check_flag() {
    Command::cargo_bin("bondebarras")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("update"));

    Command::cargo_bin("bondebarras")
        .unwrap()
        .args(["update", "--help"])
        .assert()
        .success()
        .stdout(contains("--check"));
}

/// #57's guard, and the reason it exists: the rc.1 → rc.2 pass was applied
/// to some files and not others, and the half that stayed behind pointed at
/// a tag that had been deleted — dead links on the landing page and in the
/// README, found by a reader rather than by CI.
///
/// It catches exactly that failure and nothing more. It does **not** check
/// dates, and it does not check that a quoted `--help` block still matches
/// the binary — both went wrong in the rc.3 pass too, and the second is
/// tracked in #61.
///
/// Three files that name a release are deliberately out of scope, because
/// naming an *old* one is their job: `CHANGELOG.md` is a history,
/// `docs/audits/` holds a frozen copy of an audit, and `release.yml`'s
/// mentions are comments illustrating the tilde rule.
fn release_spellings(version: &str) -> Vec<String> {
    // One version, four spellings across this repo: canonical, RPM's tilde,
    // the dot GitHub substitutes into an asset name, and the double dash a
    // shields.io badge needs. The first guard only knew the canonical one,
    // so re-introducing a stale `1.0.0.rc.2` or `1.0.0--rc.2` sailed past it
    // — half a guard against a half-applied pass.
    match version.split_once('-') {
        Some((base, pre)) => ["-", "~", ".", "--"]
            .iter()
            .map(|sep| format!("{base}{sep}{pre}"))
            .collect(),
        None => vec![version.to_string()],
    }
}

/// Every `<x>.<y>.<z><sep>rc.<n>` token in `text`, whatever the separator.
///
/// Hand-written rather than pulled from a regex crate: the shapes are few,
/// and a test that guards against drift should not itself drift behind a
/// dependency. Derived from the text rather than from the current version,
/// so it still sees a stale `1.0.0-rc.3` once the crate is at `1.0.1`.
fn release_tokens(text: &str) -> Vec<&str> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    for (i, _) in text.match_indices("rc.") {
        let mut end = i + 3;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        if end == i + 3 {
            continue; // "rc." with no number behind it is prose, not a version
        }
        let mut start = i;
        while start > 0 && matches!(b[start - 1], b'-' | b'~' | b'.') {
            start -= 1;
        }
        if start == i {
            continue; // no separator: not a version either
        }
        while start > 0 && (b[start - 1].is_ascii_digit() || b[start - 1] == b'.') {
            start -= 1;
        }
        if b[start].is_ascii_digit() {
            out.push(&text[start..end]);
        }
    }
    out
}

#[test]
fn every_document_that_names_a_release_names_this_one() {
    const CURRENT: &str = env!("CARGO_PKG_VERSION");
    let current = release_spellings(CURRENT);

    // The third field is how many mentions of an *older* release are
    // deliberate. `docs/releases.md` has three: the table of published tags
    // lists rc.2, the sentence below it enumerates both pre-releases, and
    // the versioning section cites rc.1's changelog entry.
    let docs: &[(&str, &str, usize)] = &[
        ("README.md", include_str!("../../../README.md"), 0),
        (
            "docs/installation.md",
            include_str!("../../../docs/installation.md"),
            0,
        ),
        ("docs/cli.md", include_str!("../../../docs/cli.md"), 0),
        (
            "docs/releases.md",
            include_str!("../../../docs/releases.md"),
            3,
        ),
        (
            "site/content/_index.md",
            include_str!("../../../site/content/_index.md"),
            0,
        ),
        (
            "site/content/_index.fr.md",
            include_str!("../../../site/content/_index.fr.md"),
            0,
        ),
    ];

    for (name, text, deliberate) in docs {
        assert!(
            current.iter().any(|s| text.contains(s.as_str())),
            "{name} never names {CURRENT} — was the version pass applied to it?"
        );
        let stale: Vec<&str> = release_tokens(text)
            .into_iter()
            .filter(|t| !current.iter().any(|s| s == t))
            .collect();
        assert_eq!(
            stale.len(),
            *deliberate,
            "{name} names {} older release(s) ({stale:?}), {deliberate} deliberate",
            stale.len()
        );
    }
}
