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
/// It catches exactly that failure and nothing more: a document that names a
/// release must name *this* one. It does not check dates, and it does not
/// check that a quoted `--help` block still matches the binary — both of
/// those went wrong in the rc.3 pass too, and both need their own guard.
#[test]
fn every_document_that_names_a_release_names_this_one() {
    const CURRENT: &str = env!("CARGO_PKG_VERSION");

    // The third field is how many mentions of an *older* release are
    // deliberate in that file. `docs/releases.md` has three: the table of
    // published tags lists rc.2, the sentence below it enumerates both
    // pre-releases, and the versioning section cites rc.1's changelog entry.
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
            text.contains(CURRENT),
            "{name} never names {CURRENT} — was the version pass applied to it?"
        );
        let stale = text
            .match_indices("1.0.0-rc.")
            .filter(|(i, _)| !text[*i..].starts_with(CURRENT))
            .count();
        assert_eq!(
            stale, *deliberate,
            "{name} names {stale} older release(s), {deliberate} deliberate"
        );
    }
}
