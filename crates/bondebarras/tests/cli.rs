use assert_cmd::Command;
use predicates::str::contains;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

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
///
/// What it cannot see is a stale *stable* release — a page still naming
/// `1.0.0` when the crate ships `1.0.1` — because the shape it looks for is
/// anchored on the literal `rc.`. That limit is deliberate, and
/// `the_census_can_still_recognise_this_crates_own_version` below is what
/// keeps it from going quiet.
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

/// The census has two halves, and only one of them survives the 1.0.0
/// release on its own.
///
/// "The version pass never reached this page" works off `CARGO_PKG_VERSION`
/// and keeps working forever. "This page still names an older release" works
/// off `release_tokens`, which recognises exactly one shape:
/// `<x>.<y>.<z><sep>rc.<n>`. The day this crate ships `1.0.1`, that function
/// returns nothing for every page, every `stale` list is empty, every
/// `Names::Current(0)` passes for free, and a landing page still advertising
/// `1.0.0` sails through — silently, and exactly the way #57's first guard
/// failed before it was widened.
///
/// So the extinction is made loud rather than guessed at: this test fails on
/// the very release that retires the `rc.` shape, and says what to do.
///
/// Widening `release_tokens` to any bare `<x>.<y>.<z>` was tried and
/// rejected on the evidence. Measured against the documentation as it
/// stands, it matches `keepachangelog.com/en/1.1.0/` and
/// `semver.org/spec/v2.0.0.html` in `docs/releases.md`, the `0.5.9` /
/// `0.5.10` pair that same page uses to explain that ordering is numeric and
/// not lexical, the `v1.2.0` tag in `docs/tui.md`'s mock-up of the TUI, and
/// the promise carried by four pages that the `scan --json` schema may still
/// change "before `v1.0.0`". Seven of the thirteen censused pages would fail
/// on prose that is not a release this repository has ever shipped — and a
/// guard that cries wolf seven times is one nobody reads.
#[test]
fn the_census_can_still_recognise_this_crates_own_version() {
    const CURRENT: &str = env!("CARGO_PKG_VERSION");
    for spelling in release_spellings(CURRENT) {
        assert_eq!(
            release_tokens(&spelling),
            vec![spelling.as_str()],
            "`release_tokens` no longer recognises {spelling}, this crate's own version. \
             It only knows the `<x>.<y>.<z><sep>rc.<n>` shape, so the half of the census \
             that catches a *stale* release is now blind and passes everything. Teach it \
             the shape {CURRENT} is written in — deriving the tokens from the page's text, \
             as it does now, so that a page naming a previous release is still caught."
        );
    }
}

/// The repository root, from this test's own manifest directory.
///
/// `include_str!` is the sharper tool for a page named in the source, and
/// the guard that compares `docs/cli.md` to the binary keeps using it. It
/// cannot list a directory, though, and a census that cannot list one only
/// ever checks the pages somebody remembered to name — which is how
/// `docs/tui.md` and `docs/billing.md` arrived in #63 watched by nothing.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/bondebarras sits two directories under the repository root")
        .to_path_buf()
}

/// A repository-relative page, read as text — failing with its path rather
/// than with a bare `No such file or directory`.
fn read(root: &Path, page: &str) -> String {
    fs::read_to_string(root.join(page)).unwrap_or_else(|e| panic!("{page} cannot be read: {e}"))
}

/// Every Markdown file below `start` — a repository-relative directory, or
/// the empty string for the repository itself — repository-relative, sorted,
/// minus whatever `skip` refuses. `skip` is handed each entry's
/// repository-relative path and its own file name; refusing a directory
/// prunes it whole.
fn markdown_below(root: &Path, start: &str, skip: impl Fn(&str, &str) -> bool) -> Vec<String> {
    let mut found = Vec::new();
    let mut directories = vec![start.to_string()];
    while let Some(directory) = directories.pop() {
        let entries = fs::read_dir(root.join(&directory))
            .unwrap_or_else(|e| panic!("{directory:?} cannot be listed: {e}"));
        for entry in entries {
            let entry = entry.expect("a readable directory entry");
            let name = entry.file_name().into_string().expect("a UTF-8 file name");
            let path = if directory.is_empty() {
                name.clone()
            } else {
                format!("{directory}/{name}")
            };
            if skip(&path, &name) {
                continue;
            }
            // Not `metadata()`: a symlink to a directory stays a symlink
            // here, so the walk cannot be sent round in a circle.
            if entry.file_type().expect("a readable file type").is_dir() {
                directories.push(path);
            } else if path.ends_with(".md") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// The two frozen subtrees of `docs/`: a published audit, and the design
/// history. Naming the release they were written against is their job — the
/// same reason `CHANGELOG.md` is out of the census — so no version pass has
/// to reach them.
const FROZEN: [&str; 2] = ["docs/audits", "docs/superpowers"];

/// The Markdown files under `dir`, **at any depth**, repository-relative and
/// sorted, minus the frozen subtrees.
///
/// At any depth, and that is a correction. This read one level only, which
/// made the claim below — a page added tomorrow is caught — true of
/// `docs/foo.md` and false of `docs/adr/0001.md`: a new subdirectory is the
/// likeliest shape for a batch of new pages, and it was the one shape the
/// census could not see. The exclusions are now named rather than implied by
/// a depth limit: a subtree escapes this test because it is on `FROZEN`, not
/// because of how deep it happens to sit.
fn markdown_in(root: &Path, dir: &str) -> Vec<String> {
    markdown_below(root, dir, |path, _| FROZEN.contains(&path))
}

/// The pages a version pass has to reach: the README, every page under
/// `docs/` bar the frozen subtrees, and the site's landing pages.
fn documented_pages(root: &Path) -> Vec<String> {
    let mut pages = vec!["README.md".to_string()];
    pages.extend(markdown_in(root, "docs"));
    pages.extend(markdown_in(root, "site/content"));
    pages
}

/// What a page is expected to say about the release it ships with.
#[derive(Clone, Copy, Debug)]
enum Names {
    /// The page advertises the current release, and every version pass has
    /// to reach it. The `usize` is how many mentions of an *older* release
    /// are deliberate.
    Current(usize),
    /// The page carries no version at all, and is expected to keep carrying
    /// none. Declared rather than inferred: the defect this guard exists for
    /// is a pass applied to some files and not others, so a page that
    /// *starts* naming a release has to be moved to `Current` by hand — one
    /// line, and the next pass then knows the page exists.
    Nothing,
}

/// Every page `documented_pages` finds, and what it says about the release.
///
/// Checked against the directory itself rather than trusted, so a page added
/// tomorrow fails this test until somebody classifies it here — which is the
/// half #33 asked for: `docs/tui.md` and `docs/billing.md` were recensed by
/// nothing at all, and a hand-written list is exactly what cannot notice
/// that.
const RELEASE_CENSUS: &[(&str, Names)] = &[
    ("README.md", Names::Current(0)),
    ("docs/README.md", Names::Nothing),
    ("docs/authentication.md", Names::Nothing),
    ("docs/billing.md", Names::Nothing),
    ("docs/cli.md", Names::Current(0)),
    ("docs/installation.md", Names::Current(0)),
    // Three mentions of an older release are this page's subject matter: the
    // table of published tags lists rc.2, the sentence below it enumerates
    // both pre-releases, and the versioning section cites rc.1's changelog
    // entry.
    ("docs/releases.md", Names::Current(3)),
    ("docs/resources.md", Names::Nothing),
    ("docs/safety.md", Names::Nothing),
    ("docs/troubleshooting.md", Names::Nothing),
    ("docs/tui.md", Names::Nothing),
    ("site/content/_index.fr.md", Names::Current(0)),
    ("site/content/_index.md", Names::Current(0)),
];

#[test]
fn every_document_that_names_a_release_names_this_one() {
    const CURRENT: &str = env!("CARGO_PKG_VERSION");
    let current = release_spellings(CURRENT);
    let root = repo_root();

    let censused: Vec<String> = RELEASE_CENSUS
        .iter()
        .map(|(page, _)| (*page).to_string())
        .collect();
    assert_eq!(
        documented_pages(&root),
        censused,
        "the documentation and `RELEASE_CENSUS` disagree about which pages exist; \
         classify the difference here — an unclassified page is one the next version \
         pass has no reason to visit"
    );

    for (page, names) in RELEASE_CENSUS {
        let text = read(&root, page);
        let tokens = release_tokens(&text);
        match names {
            Names::Current(deliberate) => {
                assert!(
                    current.iter().any(|s| text.contains(s.as_str())),
                    "{page} never names {CURRENT} — was the version pass applied to it?"
                );
                let stale: Vec<&str> = tokens
                    .into_iter()
                    .filter(|t| !current.iter().any(|s| s == t))
                    .collect();
                assert_eq!(
                    stale.len(),
                    *deliberate,
                    "{page} names {} older release(s) ({stale:?}), {deliberate} deliberate",
                    stale.len()
                );
            }
            Names::Nothing => assert!(
                tokens.is_empty(),
                "{page} now names {tokens:?}, and is censused as carrying no version. \
                 Move it to `Names::Current` so the next version pass visits it too."
            ),
        }
    }
}

/// `md`'s lines outside fenced code blocks, numbered from 1.
///
/// Every guard below reads a page through this, and none of them ever looks
/// inside a fence. What a fence holds was quoted from somewhere else —
/// `docs/cli.md`'s four `--help` blocks, the README's shell transcripts —
/// and answers to the thing it was quoted from rather than to this
/// repository's rules: the README's `# 2. Authenticate …` is a shell
/// comment, not a heading, and a URL printed by a command is not a link
/// anyone here promised to keep alive.
///
/// The same boundary is why no prose-wrapping formatter may ever be let
/// loose on this tree; the reason is written out where one would be added,
/// in `.github/workflows/ci.yml`'s `lint` job.
fn prose_lines(md: &str) -> Vec<(usize, &str)> {
    let mut lines = Vec::new();
    let mut fence: Option<&str> = None;
    for (n, line) in md.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"]
            .into_iter()
            .find(|marker| trimmed.starts_with(marker));
        match (fence, marker) {
            (None, None) => lines.push((n + 1, line)),
            (None, Some(open)) => fence = Some(open),
            (Some(open), Some(close)) if open == close => fence = None,
            _ => {}
        }
    }
    lines
}

/// The anchor GitHub gives a heading: lowercased, everything that is not a
/// letter, a digit, a hyphen or an underscore dropped, spaces turned into
/// hyphens. `### \`scan --json\`` becomes `scan---json`, which is what
/// `docs/billing.md` links to — three hyphens, two of them the flag's own.
///
/// A heading holding a Markdown link would slug its target along with its
/// label; none does. If one ever did, the links pointing at it would fail
/// here rather than quietly resolve to something else — noisy, not silent.
fn slug(heading: &str) -> String {
    let mut anchor = String::new();
    for c in heading.trim().chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            anchor.extend(c.to_lowercase());
        } else if c == ' ' {
            anchor.push('-');
        }
    }
    anchor
}

/// Every anchor `md`'s headings answer to, deduplicated *almost* the way
/// GitHub deduplicates them: the second heading that slugs to `changed`
/// answers to `changed-1`, as `CHANGELOG.md`'s do.
///
/// Almost, and here is the gap. This counts occurrences per base slug;
/// GitHub also keeps a registry of the anchors it has already handed out,
/// and skips a candidate that is taken. On `Dup`, `Dup`, `Dup-1`, `Dup` this
/// yields `{dup, dup-1, dup-2}` while GitHub yields
/// `{dup, dup-1, dup-1-1, dup-2}`: the third heading is given `dup-1` here,
/// an anchor the second already holds, instead of stepping aside to
/// `dup-1-1`.
///
/// It is left alone on two grounds. The error points the noisy way: this set
/// is the smaller one, so the failure mode is rejecting a link that GitHub
/// would honour — an assertion somebody reads — and never accepting one that
/// leads nowhere. And it is unreachable today: every heading this test
/// reads, across the whole repository, gets the same anchor under both
/// algorithms, measured, with no divergence at all. A collision needs a
/// heading whose own text ends in `-1` sitting beside repeated siblings;
/// the day one is written, this rejects a valid link and the reader lands
/// here.
fn heading_anchors(md: &str) -> BTreeSet<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut anchors = BTreeSet::new();
    for (_, line) in prose_lines(md) {
        let depth = line.chars().take_while(|c| *c == '#').count();
        if depth == 0 || depth > 6 {
            continue;
        }
        let Some(text) = line[depth..].strip_prefix(' ') else {
            continue;
        };
        let base = slug(text);
        if base.is_empty() {
            continue;
        }
        let seen_before = seen.entry(base.clone()).or_insert(0);
        anchors.insert(if *seen_before == 0 {
            base
        } else {
            format!("{base}-{seen_before}")
        });
        *seen_before += 1;
    }
    anchors
}

/// `line` with its inline code spans removed, the survivors held apart by a
/// space so that nothing is glued into a `](` that was never written.
///
/// Three kinds of link get past everything downstream of here, and none of
/// them is a fenced block — the exclusion stated on `prose_lines`. Worth
/// naming, in a guard whose whole subject is claims nobody verified:
///
/// 1. **An unpaired backtick swallows the rest of its line.** The split
///    keeps the even-numbered halves, so an odd count leaves the tail on an
///    odd index and it is dropped — links written after it included.
/// 2. **Raw HTML is invisible.** `<a href="page.md">` carries no `](`, so no
///    target is ever read out of it.
/// 3. **Reference *usage* is unchecked, only reference *definitions* are.**
///    `[label]: target` is read by `reference_definition`; `[label][ref]`
///    and the shortcut `[ref]` are not, so a reference pointing at a label
///    that was never defined resolves to nothing and this test says nothing.
///
/// All three are silent, which is the wrong direction for this file — they
/// are listed rather than fixed because none has ever occurred here, and a
/// parser grown to catch them is a parser that itself needs a guard.
fn outside_code_spans(line: &str) -> String {
    line.split('`').step_by(2).collect::<Vec<_>>().join(" ")
}

/// The target of `[label]: target`, when `line` is a reference definition.
fn reference_definition(line: &str) -> Option<&str> {
    let (_, target) = line.strip_prefix('[')?.split_once("]:")?;
    target.split_whitespace().next()
}

/// Every link target in `md`, with the line it is written on: inline
/// `[label](target)` links and `[label]: target` definitions alike.
fn link_targets(md: &str) -> Vec<(usize, String)> {
    let mut targets = Vec::new();
    for (n, line) in prose_lines(md) {
        let line = outside_code_spans(line);
        if let Some(target) = reference_definition(&line) {
            targets.push((n, target.to_string()));
            continue;
        }
        let mut rest = line.as_str();
        while let Some(open) = rest.find("](") {
            rest = &rest[open + 2..];
            let Some(close) = rest.find(')') else { break };
            // A title after the target — `(page.md "Title")` — is not part
            // of it.
            let target = rest[..close].split_whitespace().next().unwrap_or_default();
            if !target.is_empty() {
                targets.push((n, target.to_string()));
            }
            rest = &rest[close + 1..];
        }
    }
    targets
}

/// Whether `target` leaves the repository. `mailto:` counts: there is
/// nothing on disk to check there either.
fn is_external(target: &str) -> bool {
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|scheme| target.starts_with(scheme))
}

/// `target`, resolved against the directory `page` sits in, as a
/// repository-relative `/`-separated path — the way a reader's browser
/// resolves it. `None` when it climbs out of the repository entirely.
fn resolve(page: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = page.split('/').collect();
    parts.pop()?; // the page's own file name
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// Every Markdown file in the repository, repository-relative and sorted.
///
/// This walks the **working tree**, not the tracked tree, so it also meets
/// whatever a build has left lying about. Two kinds of directory are stepped
/// over, and the distinction is the point.
///
/// **By name, at any depth**: four directories written by tools rather than
/// by authors. `.git` and `target` have always been here; `vendor` and
/// `node_modules` are the correction. `cargo vendor` is a legitimate way to
/// build offline, and it writes one `README.md` per dependency whose links
/// are somebody else's to keep — so the local gate went red over a third
/// party's prose, and a guard that does that is a guard that gets switched
/// off. By name rather than by path because neither belongs to a fixed
/// place: `cargo vendor` takes a directory argument, and a workspace can
/// hold more than one `target`.
///
/// **By path**: three that belong to this repository but not to this check.
/// `.worktrees` holds other checkouts of this same repository, whose pages
/// answer to their own branch. `site/public` is whatever Zola last rendered.
/// And `docs/audits/` is a frozen record that quotes the prose it recommends
/// for *other* files: its `[Billing and GitHub limits](docs/billing.md)` is
/// a line proposed for the README, correct from the README and meaningless
/// from where it is quoted. Checking those links would be checking them
/// against the wrong base, and the fix would be to edit an audit — which
/// would make it stop being one; the exclusion is on the directory, so a
/// *future* audit filed there is skipped for that same reason without
/// anybody having to remember this.
///
/// A list, deliberately, rather than asking git what it ignores — and this
/// repository's own `.gitignore` is the argument. It names `/target`,
/// `**/*.rs.bk` and `site/public/`: not `.worktrees`, which this walk has
/// always had to skip anyway, and not `vendor`, which `cargo vendor` does
/// not add for you. "What git ignores" would have fixed neither of the two
/// cases that actually bite. It would also tie the local gate to a git
/// repository, when this tree is read from a release tarball and a
/// downloaded zip as well. The cost is the honest one, and it is the smaller
/// one: this list ages, and the day a tool writes somewhere new, somebody
/// adds a line to it.
/// The census and the link checker walk the tree with different rules, on
/// purpose: the census freezes whole subtrees (`FROZEN`), the link checker
/// skips tool directories by name at any depth (`vendor`, `node_modules`,
/// …). Nothing made the two agree, and a page could fall between them — a
/// `docs/vendor/guide.md` was required by the census and silently exempt
/// from link checking, so a dead link in it went unseen while the suite
/// stayed green.
///
/// This does not merge the two rules; they answer different questions. It
/// makes their disagreement impossible to reach without being told: a page
/// the census insists exists must also be one whose links are read.
#[test]
fn every_censused_page_is_also_link_checked() {
    let root = repo_root();
    let mut censused = markdown_in(&root, "docs");
    censused.extend(markdown_in(&root, "site/content"));
    let checked = all_markdown(&root);

    let unread: Vec<&String> = censused.iter().filter(|p| !checked.contains(p)).collect();
    assert!(
        unread.is_empty(),
        "the census requires {unread:?}, and the link checker never reads \
         them — one of the two filters has to change, because a page that \
         must exist and whose links nobody checks is the worst of both"
    );
}

fn all_markdown(root: &Path) -> Vec<String> {
    const SKIPPED_NAMES: [&str; 4] = [".git", "target", "vendor", "node_modules"];
    const SKIPPED_PATHS: [&str; 4] = [".worktrees", "site/public", "site/themes", "docs/audits"];
    markdown_below(root, "", |path, name| {
        SKIPPED_NAMES.contains(&name) || SKIPPED_PATHS.contains(&path)
    })
}

/// #33's guard, and the reason it exists: the links and anchors of three
/// successive documentation passes were checked by hand — 55 of them in #48,
/// 26 in #58, 24 in #63 — which is three times the same work, and one
/// distraction is all it takes to publish a dead one. That already happened
/// at the rc.1 → rc.2 pass.
///
/// Its scope, stated as plainly as the two guards above state theirs.
///
/// **Relative links are checked**, because those break for reasons that are
/// ours: a page renamed, a section retitled, a file moved.
///
/// **Anchors are checked too**, both `#section` inside a page and
/// `page.md#section` across two, because they are the half that actually
/// breaks during a rewrite. Retitling a heading leaves every link to it
/// pointing at a file that still exists, so a checker that only stats files
/// would have found nothing in three passes of manual work.
///
/// **External links are not fetched.** They break for reasons that are not
/// ours, and a build that goes red because a third-party site is down is a
/// build people learn to ignore — which costs more than the dead link the
/// check was bought for. The one class of external link that breaks *because
/// of an edit made here* is the one carrying our own version number, which
/// is exactly what the rc.1 → rc.2 incident was; those are read offline, on
/// every page, by `every_document_that_names_a_release_names_this_one` above.
/// So nothing here touches the network: this runs in the local gate, before
/// CI, and on a train.
#[test]
fn every_relative_link_and_anchor_in_the_documentation_resolves() {
    let root = repo_root();
    let mut anchors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut dead: Vec<String> = Vec::new();

    for page in all_markdown(&root) {
        for (line, target) in link_targets(&read(&root, &page)) {
            let here = format!("{page}:{line}: `{target}`");
            if is_external(&target) {
                continue;
            }
            if target.starts_with('/') {
                dead.push(format!(
                    "{here} — an absolute path is resolved against github.com, not against \
                     the repository; write it relative to {page}"
                ));
                continue;
            }
            let (path, fragment) = match target.split_once('#') {
                Some((path, fragment)) => (path, Some(fragment)),
                None => (target.as_str(), None),
            };
            // A bare `#section` points inside the page it is written on.
            let destination = if path.is_empty() {
                page.clone()
            } else {
                match resolve(&page, path) {
                    Some(destination) => destination,
                    None => {
                        dead.push(format!("{here} — climbs out of the repository"));
                        continue;
                    }
                }
            };
            if !root.join(&destination).exists() {
                dead.push(format!("{here} — {destination} does not exist"));
                continue;
            }
            let Some(fragment) = fragment else { continue };
            if !destination.ends_with(".md") {
                dead.push(format!(
                    "{here} — {destination} is not Markdown, so it has no headings to \
                     anchor to"
                ));
                continue;
            }
            let known = anchors
                .entry(destination.clone())
                .or_insert_with(|| heading_anchors(&read(&root, &destination)));
            if !known.contains(fragment) {
                dead.push(format!(
                    "{here} — {destination} has no heading whose anchor is `#{fragment}`. \
                     It answers to: {}",
                    known.iter().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
        }
    }

    assert!(
        dead.is_empty(),
        "{} dead link(s) or anchor(s) in the documentation:\n{}",
        dead.len(),
        dead.join("\n")
    );
}

/// The line every quoted help block is keyed on, and aligned at.
///
/// It reads the same on the three platforms because `cli.rs` pins clap's
/// `bin_name`. Left to its default, clap names the binary after `argv[0]`,
/// which Windows spells `bondebarras.exe`. This test carried a normalisation
/// for that suffix until a second reading of #55 showed what it cost: the
/// replacement was global and anchored on nothing, so forcing the suffix to
/// `" [COMMAND]"` and deleting that token from the page left the test green —
/// a real divergence, swallowed. The fix belongs where the name is chosen,
/// not in the guard that checks it.
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
/// fifth block added to the page tomorrow is *found* — and then fails this
/// test until `HELP_SCREENS` names it, rather than sitting unchecked for as
/// long as nobody thinks to extend it. Noisy, not silent.
///
/// An earlier wording of this comment promised the new block would simply
/// be compared, which is not what the code does. Worth correcting in a test
/// whose whole purpose is to punish claims nothing verifies.
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
    // otherwise exact comparison across six CI targets — but pinning is not
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
