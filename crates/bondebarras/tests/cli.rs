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

/// The line every quoted help block is keyed on, and aligned at.
const USAGE: &str = "Usage: bondebarras";

/// The width `--help` is captured at. Not what makes the comparison
/// deterministic — see `the_quoted_help_is_the_same_at_any_terminal_width`.
const HELP_COLUMNS: &str = "100";

/// The four help screens `docs/cli.md` quotes, named as their `Usage:` lines
/// name them (the empty string for the top-level one).
const HELP_SCREENS: [&str; 4] = ["", "scan", "clean", "update"];

fn help_args(sub: &str) -> Vec<&str> {
    if sub.is_empty() {
        vec!["--help"]
    } else {
        vec![sub, "--help"]
    }
}

/// #61's guard, and the reason it exists: `docs/cli.md` opens by claiming
/// that every flag it lists is taken from the binary's own `--help`, and the
/// claim was false. The `clean --help` block truncated four descriptions —
/// `--packages`, `--branches`, `--tags`, `--assets` — each losing the second
/// sentence, which is the one carrying a product guarantee. No terminal width
/// reproduced that block: it had simply never been re-captured. A page that
/// asserts its own provenance and cannot prove it is worse than one that
/// asserts nothing, because it discourages the reader from checking.
///
/// Its scope, as plainly as #57's guard above states its own: it compares the
/// four quoted `--help` blocks to the binary, line for line, and nothing
/// else. The prose around them — the flag tables, the exit codes, the cron
/// advice, the test names cited inline — is still only as true as its last
/// reader.
///
/// `bondebarras <args>`'s stdout, with `COLUMNS` pinned to `columns`.
fn help_output(args: &[&str], columns: &str) -> String {
    let assert = Command::cargo_bin("bondebarras")
        .unwrap()
        .args(args)
        .env("COLUMNS", columns)
        .assert()
        .success();
    String::from_utf8(assert.get_output().stdout.clone()).expect("--help is UTF-8")
}

/// Every fenced block of `md` that quotes a help screen, paired with the
/// subcommand its `Usage:` line names.
///
/// Keyed off `Usage:` rather than off a hardcoded list of headings, so a
/// fifth block added to the page tomorrow is compared too, instead of going
/// unchecked for as long as nobody thinks to extend this test.
fn quoted_help_blocks(md: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in md.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with("```") {
            match current.take() {
                None => current = Some(String::new()),
                Some(block) => {
                    if let Some(usage) = block.lines().find(|l| l.starts_with(USAGE)) {
                        let named = usage[USAGE.len()..].split_whitespace().next().unwrap_or("");
                        let sub = if named.starts_with(['[', '-']) {
                            ""
                        } else {
                            named
                        };
                        blocks.push((sub.to_string(), block));
                    }
                }
            }
        } else if let Some(block) = current.as_mut() {
            block.push_str(line);
            block.push('\n');
        }
    }
    blocks
}

/// `actual` split at its first `Usage:` line: what clap prints above it, and
/// the rest.
fn split_at_usage_line(actual: &str) -> (&str, &str) {
    let mut offset = 0;
    for line in actual.lines() {
        if line.starts_with(USAGE) {
            return actual.split_at(offset);
        }
        offset += line.len() + 1; // clap ends every line with exactly one '\n'
    }
    ("", actual) // no `Usage:` at all: compare the whole thing and fail on it
}

/// Compares one quoted block to the real output, line by line, failing on the
/// first divergence with both lines side by side. A whole-block `assert_eq!`
/// on two twenty-line strings reports *that* they differ and leaves finding
/// *where* to the reader — which is how four truncated descriptions survived
/// a re-read in the first place.
///
/// The three subcommand blocks start at `Usage:`, dropping the description
/// line clap prints above it and the blank line after it. That is a constant
/// editorial choice across the three, not an omission, and the dropped
/// sentence is not lost: it is also in the top-level block's `Commands:`
/// table, which this same test compares verbatim. So the comparison starts
/// where the page starts, rather than forcing the page to carry a line it has
/// no use for — while still checking that what it skips is only ever those
/// two lines.
fn assert_block_is_verbatim(label: &str, quoted: &str, actual: &str) {
    let expected = if quoted.starts_with(USAGE) {
        let (dropped, from_usage) = split_at_usage_line(actual);
        assert!(
            dropped.lines().count() <= 2,
            "`bondebarras {label}` now prints {} lines before `Usage:`; docs/cli.md \
             drops the description line and the blank line after it, nothing more",
            dropped.lines().count()
        );
        from_usage
    } else {
        actual
    };
    let binary: Vec<&str> = expected.lines().collect();
    let page: Vec<&str> = quoted.lines().collect();
    for n in 0..binary.len().max(page.len()) {
        assert_eq!(
            page.get(n),
            binary.get(n),
            "docs/cli.md's `bondebarras {label}` block is not what the binary prints, \
             at line {}: the page says {:?}, the binary says {:?}. Re-capture the block \
             — the page claims it is the binary's own output.",
            n + 1,
            page.get(n).unwrap_or(&"<end of block>"),
            binary.get(n).unwrap_or(&"<end of output>"),
        );
    }
}

#[test]
fn the_quoted_help_is_the_same_at_any_terminal_width() {
    // clap re-wraps `--help` only through its `wrap_help` feature, which this
    // workspace does not enable: `Cargo.lock` carries no `terminal_size`, and
    // the long flag descriptions come out on one line however narrow the
    // terminal claims to be. That is what lets `docs/cli.md` quote a single
    // rendering and call it *the* output. `COLUMNS` is pinned in the guard
    // below anyway — one line, and one environment variable fewer in an
    // otherwise exact comparison across five CI targets — but pinning is not
    // what makes it deterministic: this is, and if the premise ever stops
    // holding, this test says so instead of leaving the next reader to guess
    // which width the page was captured at.
    for sub in HELP_SCREENS {
        let args = help_args(sub);
        assert_eq!(
            help_output(&args, "40"),
            help_output(&args, "400"),
            "`bondebarras {}` wraps to the terminal: docs/cli.md can no longer quote \
             one rendering as the output",
            args.join(" ")
        );
    }
}

#[test]
fn docs_cli_md_quotes_the_binarys_own_help() {
    const PAGE: &str = include_str!("../../../docs/cli.md");

    let quoted = quoted_help_blocks(PAGE);
    let screens: Vec<&str> = quoted.iter().map(|(sub, _)| sub.as_str()).collect();
    assert_eq!(
        screens, HELP_SCREENS,
        "docs/cli.md does not quote the four help screens it says it quotes"
    );

    for (sub, block) in &quoted {
        let args = help_args(sub);
        let actual = help_output(&args, HELP_COLUMNS);
        assert_block_is_verbatim(&args.join(" "), block, &actual);
    }
}
