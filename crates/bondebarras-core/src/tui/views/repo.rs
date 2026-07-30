//! Right pane: the resources of the selected repository.

use crate::model::{Resource, ResourceKind, human_size, size_display};
use crate::refs::BranchClass;
use crate::stale::pr_number_from_ref;
use crate::tui::app::App;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

/// Widest a label renders as before this truncates it with a trailing `…`.
///
/// v0.3 shipped a row whose `Span`s were correct in code and still
/// truncated on an actual terminal, because nothing reserved a bound for the
/// label column — a long one simply pushed every column after it, including
/// the size and the flag/age this task's branch and tag classification join,
/// past the edge of a realistic-width terminal (ratatui's `List` clips
/// rather than wraps). Bounding the label fixes that for every kind, not
/// just the one v0.3 happened to hit.
///
/// Display-only, and safe as such: `clean::execute` deletes a branch or tag
/// by `Resource.label` itself, never by anything this function returns, so
/// shortening what's drawn cannot touch what GitHub is asked to delete — see
/// the task 3/4 report's own warning about decorating a branch/tag label.
const LABEL_WIDTH: usize = 40;

fn display_label(label: &str) -> String {
    let chars: Vec<char> = label.chars().collect();
    if chars.len() > LABEL_WIDTH {
        let head: String = chars[..LABEL_WIDTH - 1].iter().collect();
        format!("{head}…")
    } else {
        let width = LABEL_WIDTH;
        format!("{label:<width$}")
    }
}

/// One row of the resource list, as styled spans.
///
/// Split out from the widget so it can be asserted on without a terminal.
pub fn row_spans(r: &Resource, checked: bool) -> Vec<Span<'static>> {
    let kind = match r.kind {
        ResourceKind::Cache => "cache",
        ResourceKind::Artifact => "artif",
        ResourceKind::WorkflowRun => "run  ",
        ResourceKind::PackageVersion => "pkg  ",
        ResourceKind::Branch => "branc",
        ResourceKind::Tag => "tag  ",
        ResourceKind::ReleaseAsset => "asset",
        // Never actually reaches this list in production — a repository
        // lives in the left tree (`tui::views::orgs`), not this right-pane
        // resource list (task 3) — but the match must stay exhaustive
        // regardless, and a real label costs nothing (same reasoning v0.3
        // and v0.4 gave every other kind here: this function is pure
        // display, with no dependency on later tasks).
        ResourceKind::Repository => "repo ",
    };

    // `size_display` shows `—` rather than "0 o" for a package version:
    // GitHub exposes no size for that family, and a bare 0 here would read
    // as "empty" — the opposite of the truth. Shared with the headless
    // `clean` dry-run listing so the two screens cannot drift apart.
    let size = size_display(r);

    let mut spans = vec![
        Span::styled(
            if checked { "[x] " } else { "[ ] " }.to_string(),
            theme::text_style(),
        ),
        Span::styled(format!("{kind}  "), theme::muted()),
        Span::styled(display_label(&r.label), theme::text_style()),
        Span::styled(format!("{size:>10}  "), theme::muted()),
    ];

    // Branches and tags carry no PR ref (`mark_stale` leaves `stale_pr`
    // false for both, on purpose — see `scan::branch_resources`), so the
    // stale-PR arm below never fires for them. They earn their own
    // classification instead. A branch reads its real `BranchClass` (Finding
    // 2 of the v0.4 final review): `mergée ⚑` only for one a merged PR
    // proved dead, `par défaut`/`protégée` only for the two GitHub itself
    // refuses to let go, and `vivante` for one that is merely unmerged —
    // `protégée` used to cover all three of the latter, which claimed a
    // protection GitHub does not provide for a branch nobody has decided
    // anything about. A tag is always `protected: true`, unconditionally,
    // so it always reads "protégé".
    match r.kind {
        ResourceKind::Branch => {
            let (text, style) = match r.branch_class {
                Some(BranchClass::Merged) => ("mergée ⚑".to_string(), theme::stale_style()),
                Some(BranchClass::Default) => ("par défaut".to_string(), theme::muted()),
                Some(BranchClass::Protected) => ("protégée".to_string(), theme::muted()),
                // `None` should not happen in production — `scan::
                // branch_resources` always sets it for a `Branch` row — but
                // falls back to the least alarming, least presumptuous
                // label rather than panicking or claiming a protection
                // nothing has confirmed.
                Some(BranchClass::Live) | None => ("vivante".to_string(), theme::muted()),
            };
            spans.push(Span::styled(text, style));
        }
        ResourceKind::Tag => spans.push(Span::styled("protégé".to_string(), theme::muted())),
        // A stale row earns its own colour and the PR that made it dead weight.
        _ => match r.git_ref.as_deref().and_then(pr_number_from_ref) {
            Some(n) if r.stale_pr => {
                spans.push(Span::styled(format!("PR#{n} ⚑"), theme::stale_style()))
            }
            _ => spans.push(Span::styled(format!("{}j", r.age_days), theme::muted())),
        },
    }
    spans
}

/// The list block's title: item count and byte tally, plus — when the
/// visible list holds at least one resource `ResourceKind::has_known_size`
/// says GitHub exposes no size for — the caveat that some rows' size is
/// unknown.
///
/// Not optional when `has_sizeless` is true: a column of `—` in a tool that
/// shows bytes on every other screen reads as "these are empty", which is
/// the opposite of the truth. Debt 2 of the v0.4 final review: this used to
/// take `has_packages`, fed by `render`'s own `kind ==
/// ResourceKind::PackageVersion` check — so a list whose only sizeless rows
/// were branches or tags carried the same `—` markers with no banner to
/// explain them. Generalised to whatever `has_known_size` calls sizeless,
/// the one place that already enumerates every such kind.
fn list_title(count: usize, bytes: u64, has_sizeless: bool) -> String {
    if has_sizeless {
        format!(
            " {count} éléments · {} · ⚠ GitHub n'expose pas la taille de certaines ressources ",
            human_size(bytes)
        )
    } else {
        format!(" {count} éléments · {} ", human_size(bytes))
    }
}

/// Renders the resource list as a stateful list so ratatui scrolls to keep
/// the selection visible. On a 69-cache repo, an 80x24 terminal only fits
/// about 19 rows without this — the plain `render_widget` used before left
/// most of them unreachable.
pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    // Read before `items` is built, from the same shared borrow, so both can
    // draw from `app.visible_resources()` before anything is borrowed
    // mutably below.
    let has_sizeless = app
        .visible_resources()
        .iter()
        .any(|r| !r.kind.has_known_size());

    // Built first, from a shared borrow of `app` only: the items own their
    // strings (`ListItem<'static>`), so the borrow ends here, before
    // `app.res_state` is borrowed mutably below.
    let items: Vec<ListItem<'static>> = app
        .visible_resources()
        .into_iter()
        .map(|r| {
            let spans = row_spans(r, app.selected.contains(&(r.kind, r.id)));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = list_title(items.len(), app.selection_bytes(), has_sizeless);

    app.res_state.select(if items.is_empty() {
        None
    } else {
        Some(app.res_cursor.min(items.len() - 1))
    });

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        )
        .highlight_style(theme::selection_style());

    f.render_stateful_widget(list, area, &mut app.res_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ResourceKind;
    use crate::refs::BranchClass;

    fn res(stale: bool) -> Resource {
        Resource {
            kind: ResourceKind::Cache,
            id: 1,
            label: "v0-rust-coverage-Linux-x64".into(),
            size_bytes: 273_678_336,
            age_days: 40,
            git_ref: Some("refs/pull/32/merge".into()),
            stale_pr: stale,
            protected: false,
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        }
    }

    /// Builds its label through the real `scan::version_label`, from a
    /// full, unelided 71-character digest — the shape production actually
    /// produces. A fixture that instead hand-types an already-elided string
    /// would still read "sha256:9a26c7080… (sans tag)" even if
    /// `version_label` regressed back to emitting the full digest: nothing
    /// in that string flows through the function under test.
    fn package_resource() -> Resource {
        let v = crate::packages::PackageVersion {
            id: 9,
            digest: "sha256:9a26c70801010123223adb5e73ff703aca86c15e19b30124ede5628a1e185826"
                .into(),
            tags: vec![],
            age_days: 5,
        };
        Resource {
            kind: ResourceKind::PackageVersion,
            id: v.id,
            label: crate::scan::version_label(&v, crate::packages::VersionClass::Untagged),
            size_bytes: 0,
            age_days: v.age_days,
            git_ref: None,
            stale_pr: false,
            protected: false,
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        }
    }

    /// `protected: class != BranchClass::Merged` and `branch_class:
    /// Some(class)` are exactly what `scan::branch_resources` sets — this
    /// fixture mirrors production rather than inventing its own shape.
    fn branch(label: &str, class: BranchClass) -> Resource {
        Resource {
            kind: ResourceKind::Branch,
            id: 1,
            label: label.to_string(),
            size_bytes: 0,
            age_days: 0,
            git_ref: None,
            stale_pr: false,
            protected: class != BranchClass::Merged,
            branch_class: Some(class),
            safety: crate::safety::Safety::Keep,
        }
    }

    /// A tag is always `protected: true` — `scan::tag_resources` never sets
    /// it any other way, so no fixture parameter for it exists here.
    fn tag(label: &str) -> Resource {
        Resource {
            kind: ResourceKind::Tag,
            id: 1,
            label: label.to_string(),
            size_bytes: 0,
            age_days: 0,
            git_ref: None,
            stale_pr: false,
            protected: true,
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        }
    }

    /// `scan::asset_resources` bakes the release tag into the label itself
    /// (`"{name} ({release_tag})"`) — there is nowhere else for it to
    /// survive to, since the release itself never becomes its own row. This
    /// fixture takes the already-composed label, the same shape production
    /// hands `row_spans`.
    fn asset(label: &str, size: u64, age: i64) -> Resource {
        Resource {
            kind: ResourceKind::ReleaseAsset,
            id: 1,
            label: label.to_string(),
            size_bytes: size,
            age_days: age,
            git_ref: None,
            stale_pr: false,
            protected: false,
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        }
    }

    fn text(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// `ResourceKind::Repository` never actually reaches `row_spans` in
    /// production — task 3 keeps a repository in the left tree, never in
    /// `app.resources` — but the match on `r.kind` must stay exhaustive
    /// regardless, and this locks the label it was given rather than leaving
    /// it unverified.
    #[test]
    fn a_repository_row_shows_its_kind_label() {
        let r = Resource {
            kind: ResourceKind::Repository,
            id: 1,
            label: "claudine".into(),
            size_bytes: 0,
            age_days: 0,
            git_ref: None,
            stale_pr: false,
            protected: false,
            branch_class: None,
            safety: crate::safety::Safety::Keep,
        };
        let line = text(&row_spans(&r, false));
        assert!(line.contains("repo"), "got: {line}");
    }

    #[test]
    fn a_row_shows_the_checkbox_kind_label_and_size() {
        let line = text(&row_spans(&res(false), true));
        assert!(line.contains("[x]"));
        assert!(line.contains("cache"));
        assert!(line.contains("v0-rust-coverage-Linux-x64"));
        assert!(line.contains("273.7 Mo"));
    }

    #[test]
    fn a_stale_row_carries_the_flag_and_its_pr_number() {
        let line = text(&row_spans(&res(true), false));
        assert!(line.contains("[ ]"));
        assert!(line.contains("PR#32"));
        assert!(line.contains('⚑'));
    }

    #[test]
    fn a_fresh_row_carries_no_flag() {
        assert!(!text(&row_spans(&res(false), false)).contains('⚑'));
    }

    /// The flag is the safest thing on screen to delete, so it must never be
    /// painted like an error. Task 11 locked `STALE != ERROR` in the theme;
    /// this locks the row actually reaching for the right one — asserting on
    /// content alone would let a swapped style through unnoticed.
    #[test]
    fn the_flag_is_painted_stale_not_error() {
        let spans = row_spans(&res(true), false);
        let flag = spans
            .last()
            .expect("a row always ends with a flag or an age");
        assert_eq!(flag.style, theme::stale_style());
        assert_ne!(flag.style, theme::status_error());
    }

    #[test]
    fn a_package_row_says_its_size_is_unknown_not_zero() {
        // Every other screen shows bytes. A bare "0 o" here would read as
        // "empty", which is the opposite of the truth.
        let line = text(&row_spans(&package_resource(), false));
        assert!(!line.contains("0 o"), "got: {line}");
        assert!(line.contains('—'), "got: {line}");
    }

    /// Rendered, not stringly: `row_spans` alone cannot show what actually
    /// reaches the screen. Before `version_label` elided its digest, this
    /// row's label alone ran to 82 characters — past column 80 before the
    /// checkbox and kind columns even get counted — pushing the `—` size
    /// marker and the `(sans tag)` class suffix off the visible buffer
    /// entirely, with no assertion here able to see it, since every other
    /// test in this module asserts on the spans, not on what a terminal
    /// would actually show.
    #[test]
    fn a_package_row_survives_at_eighty_columns() {
        let mut app = App::new(vec![]);
        app.resources = vec![package_resource()];

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&mut app, f, f.area())).unwrap();

        let rendered: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            rendered.contains('—'),
            "the size marker must survive: {rendered}"
        );
        assert!(
            rendered.contains("sans tag"),
            "the class suffix must survive: {rendered}"
        );
    }

    /// The dead-branch classification the design doc's mockup (§5) calls
    /// `mergée #31 ⚑` — no PR number is carried this far (`branch_resources`
    /// discards it once `classify_branch` has used it, and `Resource` has
    /// nowhere to keep it), so the row names what it can prove: that the
    /// branch is dead because a PR merged it, and flags it the same way a
    /// stale cache is.
    #[test]
    fn a_dead_branch_is_labelled_merged_and_flagged() {
        let line = text(&row_spans(
            &branch("claude/landing-3jbqk4", BranchClass::Merged),
            true,
        ));
        assert!(line.contains("mergée"), "got: {line}");
        assert!(line.contains('⚑'), "got: {line}");
    }

    /// Finding 2: `protected: true` used to be the *only* signal a live
    /// branch had, and it was shared by three different reasons — the
    /// default branch, one GitHub protects directly, and one that is simply
    /// unmerged — all rendering the same word, "protégée", a claim GitHub
    /// backs for only the first two. These three tests feed one
    /// `BranchClass` each and assert all four possible words
    /// (`mergée`/`par défaut`/`protégée`/`vivante`) are mutually exclusive:
    /// a row_spans that collapsed any two of the non-merged classes back
    /// together (e.g. always printing "protégée" for both `Default` and
    /// `Live`) would pass a test that only checked the word it expects
    /// present, but fail the sibling test that checks the same word is
    /// *absent* for a different class.
    #[test]
    fn a_default_branch_is_labelled_par_defaut() {
        let line = text(&row_spans(&branch("main", BranchClass::Default), false));
        assert!(line.contains("par défaut"), "got: {line}");
        assert!(!line.contains("mergée"), "got: {line}");
        assert!(!line.contains("vivante"), "got: {line}");
        assert!(!line.contains('⚑'), "got: {line}");
    }

    #[test]
    fn a_github_protected_branch_is_labelled_protegee() {
        let line = text(&row_spans(
            &branch("release/2.0", BranchClass::Protected),
            false,
        ));
        assert!(line.contains("protégée"), "got: {line}");
        assert!(!line.contains("par défaut"), "got: {line}");
        assert!(!line.contains("mergée"), "got: {line}");
        assert!(!line.contains("vivante"), "got: {line}");
        assert!(!line.contains('⚑'), "got: {line}");
    }

    #[test]
    fn a_live_unmerged_branch_is_labelled_vivante() {
        let line = text(&row_spans(
            &branch("feature/rejected", BranchClass::Live),
            false,
        ));
        assert!(line.contains("vivante"), "got: {line}");
        assert!(!line.contains("protégée"), "got: {line}");
        assert!(!line.contains("par défaut"), "got: {line}");
        assert!(!line.contains("mergée"), "got: {line}");
        assert!(!line.contains('⚑'), "got: {line}");
    }

    /// `scan::tag_resources` sets `protected: true` unconditionally — there
    /// is no "dead tag" the way there is a dead branch — so this needs no
    /// negative counterpart the way the branch tests above do.
    #[test]
    fn a_tag_is_always_labelled_protected() {
        let line = text(&row_spans(&tag("v0.1.3"), false));
        assert!(line.contains("protégé"), "got: {line}");
    }

    /// Same reasoning as `the_flag_is_painted_stale_not_error` above, for the
    /// branch classification this task adds: the safest thing on screen to
    /// delete must never be painted like a problem.
    #[test]
    fn the_dead_branch_flag_is_painted_stale_not_error() {
        let spans = row_spans(&branch("claude/landing-3jbqk4", BranchClass::Merged), false);
        let flag = spans
            .last()
            .expect("a branch row always ends with a classification");
        assert_eq!(flag.style, theme::stale_style());
        assert_ne!(flag.style, theme::status_error());
    }

    /// An asset's release tag already survives on screen — `scan::
    /// asset_resources` bakes it into the label itself, since the release
    /// never becomes its own row. This just locks that the label column
    /// still carries it once rendered through `row_spans`, the same
    /// guarantee `a_package_row_says_its_size_is_unknown_not_zero` gives the
    /// `—` size marker for packages.
    #[test]
    fn an_asset_row_carries_its_release_tag() {
        let line = text(&row_spans(
            &asset("claudine-linux-x86_64.tar.gz (v0.1.1)", 2_400_000, 40),
            false,
        ));
        assert!(line.contains("v0.1.1"), "got: {line}");
        assert!(line.contains("2.4 Mo"), "got: {line}");
    }

    /// v0.3 shipped a row that was correct in `row_spans` — every `Span` it
    /// built carried the right text — and still truncated on an actual
    /// terminal, because nothing asserted on a rendered buffer, only on the
    /// spans themselves (see `a_package_row_survives_at_eighty_columns`
    /// above). That fix covered one resource at one width. A branch name can
    /// run much longer than a cache key — the design doc's own measured
    /// example is 42 characters — and a single sampled width can dodge a
    /// truncation the way the confirm modal's height sweep found one
    /// recurring at exactly one height per width. Sweep widths instead of
    /// sampling them, for both a dead branch and a release asset.
    #[test]
    fn a_branch_and_an_asset_row_stay_legible_across_swept_widths() {
        let mut app = App::new(vec![]);
        app.resources = vec![
            branch(
                "claude/claudine-landing-positioning-3jbqk4",
                BranchClass::Merged,
            ),
            asset("claudine-linux-x86_64.tar.gz (v0.1.1)", 2_400_000, 40),
        ];

        // Floor: the smallest width at which both rows' full classification
        // (the branch's "mergée ⚑", the asset's release tag) is on screen at
        // all, determined empirically by probing every width from 40 to 200
        // with this exact fixture and recording the first one both markers
        // appeared at — not computed by hand, since the checkbox/kind/label/
        // size columns plus the border are several small `format!`s away
        // from an easy mental sum. 72 (one below) was checked separately and
        // clips the branch flag, confirming this is the real floor, not
        // just a width that happens to work.
        const FLOOR: u16 = 73;
        for width in FLOOR..=200 {
            let backend = ratatui::backend::TestBackend::new(width, 10);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| render(&mut app, f, f.area())).unwrap();

            let rendered: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect();

            assert!(
                rendered.contains("mergée") && rendered.contains('⚑'),
                "the dead branch's classification clipped at width {width}: {rendered}"
            );
            assert!(
                rendered.contains("v0.1.1"),
                "the asset's release tag clipped at width {width}: {rendered}"
            );
        }
    }

    /// The merged/asset sweep above proves the longest branch label
    /// ("mergée ⚑") survives from width 73; it says nothing about the three
    /// shorter classifications this task adds. Swept, not sampled, for the
    /// same reason as the sweep above — a single sampled width could dodge a
    /// truncation the way the confirm modal's height sweep found one
    /// recurring at exactly one height per width.
    #[test]
    fn every_branch_classification_stays_legible_across_swept_widths() {
        let mut app = App::new(vec![]);
        app.resources = vec![
            branch("main", BranchClass::Default),
            branch("release/2.0", BranchClass::Protected),
            branch("claude/landing-3jbqk4", BranchClass::Merged),
            branch("feature/rejected", BranchClass::Live),
        ];

        // Floor: the smallest width at which all four classification words
        // are on screen at once, determined empirically the same way as the
        // sweep above — probing every width from 40 to 200 and recording the
        // first one all five markers ("par défaut", "protégée", "mergée",
        // "⚑", "vivante") appeared at, with no gap above it up to 200. 74
        // (one below) was checked separately and clips "par défaut" to "par
        // défau" — confirming this is the real floor.
        const FLOOR: u16 = 75;
        for width in FLOOR..=200 {
            let backend = ratatui::backend::TestBackend::new(width, 10);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| render(&mut app, f, f.area())).unwrap();

            let rendered: String = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|c| c.symbol())
                .collect();

            assert!(
                rendered.contains("par défaut"),
                "the default-branch label clipped at width {width}: {rendered}"
            );
            assert!(
                rendered.contains("protégée"),
                "the GitHub-protected label clipped at width {width}: {rendered}"
            );
            assert!(
                rendered.contains("mergée") && rendered.contains('⚑'),
                "the merged-branch classification clipped at width {width}: {rendered}"
            );
            assert!(
                rendered.contains("vivante"),
                "the live-branch label clipped at width {width}: {rendered}"
            );
        }
    }

    #[test]
    fn the_title_warns_when_the_list_holds_a_package_version() {
        // A wrong implementation that never surfaces the caveat would leave
        // the "—" size column reading as "empty" instead of "unmeasured".
        let title = list_title(3, 0, true);
        assert!(title.contains("GitHub"), "got: {title}");
        assert!(title.to_lowercase().contains("taille"), "got: {title}");
    }

    #[test]
    fn the_title_carries_no_warning_without_a_package_version() {
        // A wrong implementation that always shows the caveat would clutter
        // every ordinary cache/artifact/run listing with an irrelevant line.
        let title = list_title(3, 100, false);
        assert!(!title.contains("GitHub"), "got: {title}");
        assert!(title.contains("100 o"), "got: {title}");
    }

    /// Debt 2 of the v0.4 final review: `render` computed the flag it hands
    /// `list_title` from `r.kind == ResourceKind::PackageVersion` alone, so a
    /// repository whose only sizeless rows were branches or tags — no
    /// package version anywhere — rendered a column of `—` with no banner
    /// explaining it, the exact "reads as empty" defect this whole title
    /// exists to prevent. `has_known_size` is the one place that already
    /// knows every sizeless kind; `render`'s predicate must ask it, not
    /// re-derive its own narrower list. Exercised through `render` itself,
    /// not `list_title` in isolation: `list_title` only takes an
    /// already-computed bool and cannot prove which kinds fed it — a
    /// regression back to `kind == PackageVersion` would still pass every
    /// `list_title` unit test above unchanged.
    #[test]
    fn the_title_warns_when_the_visible_list_holds_a_branch_with_no_package_present() {
        let mut app = App::new(vec![]);
        app.resources = vec![branch("claude/landing-3jbqk4", BranchClass::Merged)];

        let backend = ratatui::backend::TestBackend::new(100, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&mut app, f, f.area())).unwrap();

        let rendered: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            rendered.contains("GitHub"),
            "a branch-only list must still warn that its size is unknown: {rendered}"
        );
    }
}
