//! Column 3: the resources of the loaded repository, under its two gauges.

use crate::model::{Resource, ResourceKind, human_size, size_display};
use crate::refs::BranchClass;
use crate::stale::pr_number_from_ref;
use crate::tui::app::{App, Focus};
use crate::tui::theme;
use crate::tui::views::{self, gauges};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};
use std::collections::HashSet;

/// Narrowest the resources column is drawn whenever it shares the screen.
/// `tui::views::columns_for` derives both of its thresholds from it: this is
/// the column deletion happens in, so a narrow terminal drops columns from
/// the left rather than squeeze this one below it.
pub(crate) const MIN_WIDTH: u16 = 40;

/// Widest a label renders as, however roomy the column.
///
/// v0.3 shipped a row whose `Span`s were correct in code and still
/// truncated on an actual terminal, because nothing reserved a bound for the
/// label column — a long one simply pushed every column after it, including
/// the size and the flag/age this task's branch and tag classification join,
/// past the edge of a realistic-width terminal (ratatui's `List` clips
/// rather than wraps). Bounding the label fixes that for every kind, not
/// just the one v0.3 happened to hit. Since the three columns, a narrower
/// column shortens it further (`label_width_in`).
///
/// Display-only, and safe as such: `clean::execute` deletes a branch or tag
/// by `Resource.label` itself, never by anything `fit_label` returns, so
/// shortening what's drawn cannot touch what GitHub is asked to delete — see
/// the task 3/4 report's own warning about decorating a branch/tag label.
const LABEL_WIDTH: usize = 40;

/// Every character a row spends outside its label: the checkbox (`"[ ] "`,
/// 4), the kind (`"cache  "`, 7), the size (`"{size:>10}  "`, 12) and the
/// widest trailing classification (`"par défaut"`, or a flag up to
/// `"PR#99999 ⚑"`, 10).
const ROW_BESIDE_LABEL: usize = 4 + 7 + 12 + 10;

/// How wide a label may be in a list `inner_width` cells wide: whatever the
/// rest of the row leaves, up to `LABEL_WIDTH`.
///
/// The label is the one part of a row that gives. Spec §2.1 as amended on
/// 2026-09-11: the size and the flag are never the part a narrow column
/// clips — the v0.3 defect again, a row correct in code and cut on screen,
/// would otherwise come back at every width where the resources column
/// shares the screen at its `MIN_WIDTH`.
fn label_width_in(inner_width: u16) -> usize {
    usize::from(inner_width)
        .saturating_sub(ROW_BESIDE_LABEL)
        .min(LABEL_WIDTH)
}

/// `label` fitted to `width` characters for a resource row.
///
/// Like `views::fit`, except for a label ending in a parenthesised suffix —
/// the release tag `scan::asset_resources` appends to an asset, the class
/// `scan::version_label` appends to a package version. That suffix is the
/// part that tells such rows apart, and cutting from the right drops it
/// first, so the head is elided instead and the suffix kept whole, as long
/// as one head character and its `…` still fit beside it.
fn fit_label(label: &str, width: usize) -> String {
    if label.chars().count() > width
        && label.ends_with(')')
        && let Some(start) = label.rfind(" (")
    {
        let suffix = &label[start..];
        let suffix_len = suffix.chars().count();
        if width >= suffix_len + 2 {
            return format!(
                "{}{suffix}",
                views::fit(&label[..start], width - suffix_len)
            );
        }
    }
    views::fit(label, width)
}

/// One row of the resource list, as styled spans, its label fitted to
/// `label_width` characters (`label_width_in` gives it for a real column).
///
/// Split out from the widget so it can be asserted on without a terminal.
pub fn row_spans(r: &Resource, checked: bool, label_width: usize) -> Vec<Span<'static>> {
    let kind = match r.kind {
        ResourceKind::Cache => "cache",
        ResourceKind::Artifact => "artif",
        ResourceKind::WorkflowRun => "run  ",
        ResourceKind::PackageVersion => "pkg  ",
        ResourceKind::Branch => "branc",
        ResourceKind::Tag => "tag  ",
        ResourceKind::ReleaseAsset => "asset",
        // Never actually reaches this list in production — a repository
        // lives in the repos column (`tui::views::repos`), not this resource
        // list (task 3) — but the match must stay exhaustive regardless, and
        // a real label costs nothing (same reasoning v0.3 and v0.4 gave
        // every other kind here: this function is pure display, with no
        // dependency on later tasks).
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
        Span::styled(fit_label(&r.label, label_width), theme::text_style()),
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

/// The resources column's title: the column's name, then item count and
/// byte tally, plus — when the visible list holds at least one resource
/// `ResourceKind::has_known_size` says GitHub exposes no size for — the
/// caveat that some rows' size is unknown.
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
            " RESSOURCES · {count} éléments · {} · ⚠ GitHub n'expose pas la taille de certaines ressources ",
            human_size(bytes)
        )
    } else {
        format!(" RESSOURCES · {count} éléments · {} ", human_size(bytes))
    }
}

/// Actions minutes this repository burnt in the most recent month its org's
/// usage report carries, in Linux-equivalent minutes.
///
/// Reuses `BillingReport::included_minutes` rather than recomputing the
/// per-SKU multiplier sum by hand — passing it a set containing only this
/// one repository turns the same org-wide allowance sum it already computes
/// (and is already unit-tested against) into a single repository's share.
/// `0` when there is nothing to read: no billing report at all (a 403 — this
/// token's owner is not an org owner), or no month in it yet — the same
/// absent-data-reads-as-zero the caller already applies to a public repo.
fn repo_minutes_used(org: &crate::model::OrgSummary, repo_name: &str) -> u64 {
    let Some(report) = &org.billing else {
        return 0;
    };
    let months = report.months();
    let Some(month) = months.last() else {
        return 0;
    };
    let mut only_this_repo = HashSet::new();
    only_this_repo.insert(repo_name.to_string());
    report.included_minutes(month, &only_this_repo)
}

/// The two gauges for the repository whose resources this column currently
/// shows (`app.loaded`, not wherever the repos cursor has since wandered —
/// same reasoning as `App::take_plan`). Empty before anything has loaded, or
/// if the loaded repository has since left its org's list (an org refresh,
/// say): there is nothing to gauge yet.
fn repo_gauge_lines(app: &App, width: u16) -> Vec<Line<'static>> {
    let Some((org_login, repo_name)) = app.loaded.as_ref() else {
        return Vec::new();
    };
    let Some(org) = app.orgs.iter().find(|o| &o.login == org_login) else {
        return Vec::new();
    };
    let Some(repo) = org.repos.iter().find(|r| &r.name == repo_name) else {
        return Vec::new();
    };

    let mut lines = gauges::cache_gauge_line(repo.cache_bytes, width);
    let minutes_used = if repo.private {
        repo_minutes_used(org, &repo.name)
    } else {
        0
    };
    lines.extend(gauges::minutes_gauge_line(
        minutes_used,
        !repo.private,
        width,
    ));
    lines
}

/// Renders the resources column: its block, the repository's two gauges
/// (`tui::views::gauges`) at its head (spec §5), and under them the resource
/// list, as a stateful list so ratatui scrolls to keep the selection
/// visible. On a 69-cache repo, an 80x24 terminal only fits about 19 rows
/// without this — the plain `render_widget` used before left most of them
/// unreachable.
pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Resources;

    // Everything below until `app.res_state` reads `app` through shared
    // borrows only; the items own their strings (`ListItem<'static>`), so
    // those borrows end once the items are built.
    let visible = app.visible_resources();
    let has_sizeless = visible.iter().any(|r| !r.kind.has_known_size());
    let title = list_title(visible.len(), app.selection_bytes(), has_sizeless);

    let block = views::column_block(title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let gauge_lines = repo_gauge_lines(app, inner.width);
    let list_area = if gauge_lines.is_empty() {
        inner
    } else {
        let [gauge_area, list_area] = Layout::vertical([
            Constraint::Length(gauge_lines.len() as u16),
            Constraint::Min(0),
        ])
        .areas(inner);
        f.render_widget(Paragraph::new(gauge_lines), gauge_area);
        list_area
    };

    let label_width = label_width_in(list_area.width);
    let items: Vec<ListItem<'static>> = visible
        .into_iter()
        .map(|r| {
            let checked = app.selected.contains(&(r.kind, r.id));
            ListItem::new(Line::from(row_spans(r, checked, label_width)))
        })
        .collect();

    app.res_state.select(if items.is_empty() {
        None
    } else {
        Some(app.res_cursor.min(items.len() - 1))
    });

    let list = List::new(items).highlight_style(views::cursor_style(focused));

    f.render_stateful_widget(list, list_area, &mut app.res_state);
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
    /// production — a repository lives in the repos column, never in
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
        let line = text(&row_spans(&r, false, LABEL_WIDTH));
        assert!(line.contains("repo"), "got: {line}");
    }

    /// An asset's label ends with its release tag (`scan::asset_resources`)
    /// and a package version's with its class (`scan::version_label`), both
    /// in parentheses — the one part of such a label that tells rows apart.
    /// Shortening a label to the column must elide the head and keep that
    /// suffix whole while at least one head character still fits; cutting
    /// from the right, as `views::fit` does, would drop the suffix first.
    #[test]
    fn a_shortened_label_keeps_its_parenthesised_suffix() {
        let asset = "claudine-linux-x86_64.tar.gz (v0.1.1)";
        assert_eq!(fit_label(asset, 20), "claudine-l… (v0.1.1)");
        // No room for even one head character beside the suffix: a plain cut.
        assert_eq!(fit_label(asset, 10), "claudine-…");
        // A label that fits is only padded, suffix or not.
        assert_eq!(fit_label("main", 6), "main  ");
        // No suffix to keep: a plain cut.
        assert_eq!(fit_label("v0-rust-coverage-Linux-x64", 8), "v0-rust…");
        // A column too narrow for any label at all: nothing, not a panic.
        assert_eq!(fit_label(asset, 0), "");
    }

    #[test]
    fn a_row_shows_the_checkbox_kind_label_and_size() {
        let line = text(&row_spans(&res(false), true, LABEL_WIDTH));
        assert!(line.contains("[x]"));
        assert!(line.contains("cache"));
        assert!(line.contains("v0-rust-coverage-Linux-x64"));
        assert!(line.contains("273.7 Mo"));
    }

    #[test]
    fn a_stale_row_carries_the_flag_and_its_pr_number() {
        let line = text(&row_spans(&res(true), false, LABEL_WIDTH));
        assert!(line.contains("[ ]"));
        assert!(line.contains("PR#32"));
        assert!(line.contains('⚑'));
    }

    #[test]
    fn a_fresh_row_carries_no_flag() {
        assert!(!text(&row_spans(&res(false), false, LABEL_WIDTH)).contains('⚑'));
    }

    /// The flag is the safest thing on screen to delete, so it must never be
    /// painted like an error. Task 11 locked `STALE != ERROR` in the theme;
    /// this locks the row actually reaching for the right one — asserting on
    /// content alone would let a swapped style through unnoticed.
    #[test]
    fn the_flag_is_painted_stale_not_error() {
        let spans = row_spans(&res(true), false, LABEL_WIDTH);
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
        let line = text(&row_spans(&package_resource(), false, LABEL_WIDTH));
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
            LABEL_WIDTH,
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
        let line = text(&row_spans(
            &branch("main", BranchClass::Default),
            false,
            LABEL_WIDTH,
        ));
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
            LABEL_WIDTH,
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
            LABEL_WIDTH,
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
        let line = text(&row_spans(&tag("v0.1.3"), false, LABEL_WIDTH));
        assert!(line.contains("protégé"), "got: {line}");
    }

    /// Same reasoning as `the_flag_is_painted_stale_not_error` above, for the
    /// branch classification this task adds: the safest thing on screen to
    /// delete must never be painted like a problem.
    #[test]
    fn the_dead_branch_flag_is_painted_stale_not_error() {
        let spans = row_spans(
            &branch("claude/landing-3jbqk4", BranchClass::Merged),
            false,
            LABEL_WIDTH,
        );
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
            LABEL_WIDTH,
        ));
        assert!(line.contains("v0.1.1"), "got: {line}");
        assert!(line.contains("2.4 Mo"), "got: {line}");
    }

    /// v0.3 shipped a row that was correct in `row_spans` — every `Span` it
    /// built carried the right text — and still truncated on an actual
    /// terminal, because nothing asserted on a rendered buffer, only on the
    /// spans themselves (see `a_package_row_survives_at_eighty_columns`
    /// above). A branch name can run much longer than a cache key — the
    /// design doc's own measured example is 42 characters — and a single
    /// sampled width can dodge a truncation the way the confirm modal's
    /// height sweep found one recurring at exactly one height per width.
    ///
    /// Since the three columns, read back from the resources column's real
    /// `Rect` (`views::testing::focused_column`) at every terminal width
    /// from 60 to 200, where that column is as narrow as `MIN_WIDTH` in the
    /// three- and two-column layouts. The label is the part that gives: the
    /// dead branch's `mergée ⚑`, its `—` size and the asset's size survive
    /// every width. The asset's release tag lives *inside* its label
    /// (`scan::asset_resources`), so it survives wherever `fit_label` has
    /// room to keep the label's ` (v0.1.1)` suffix whole beside one head
    /// character and its `…` — from a column of `ROW_BESIDE_LABEL` plus that
    /// much, plus its two borders; read off the real `Rect`, not assumed.
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
        let tag_floor = ROW_BESIDE_LABEL + " (v0.1.1)".len() + 2 + 2;

        for width in 60..=200u16 {
            let (rect, column) =
                views::testing::focused_column(&mut app, Focus::Resources, width, 12);

            assert!(
                column.contains("mergée ⚑"),
                "the dead branch's classification clipped at width {width}:\n{column}"
            );
            assert!(
                column.contains('—') && column.contains("2.4 Mo"),
                "a row's size clipped at width {width}:\n{column}"
            );
            if usize::from(rect.width) >= tag_floor {
                assert!(
                    column.contains("(v0.1.1)"),
                    "the asset's release tag lost at width {width} (column {}):\n{column}",
                    rect.width
                );
            }
        }
    }

    /// The merged/asset sweep above proves the longest branch label
    /// ("mergée ⚑") survives every width; it says nothing about the three
    /// shorter classifications this task adds — `par défaut` is in fact the
    /// widest of the four, the one `ROW_BESIDE_LABEL` is sized for. Swept,
    /// not sampled, over the resources column's real `Rect`, for the same
    /// reason as the sweep above.
    #[test]
    fn every_branch_classification_stays_legible_across_swept_widths() {
        let mut app = App::new(vec![]);
        app.resources = vec![
            branch("main", BranchClass::Default),
            branch("release/2.0", BranchClass::Protected),
            branch("claude/landing-3jbqk4", BranchClass::Merged),
            branch("feature/rejected", BranchClass::Live),
        ];

        for width in 60..=200u16 {
            let (_, column) = views::testing::focused_column(&mut app, Focus::Resources, width, 12);

            assert!(
                column.contains("par défaut"),
                "the default-branch label clipped at width {width}:\n{column}"
            );
            assert!(
                column.contains("protégée"),
                "the GitHub-protected label clipped at width {width}:\n{column}"
            );
            assert!(
                column.contains("mergée ⚑"),
                "the merged-branch classification clipped at width {width}:\n{column}"
            );
            assert!(
                column.contains("vivante"),
                "the live-branch label clipped at width {width}:\n{column}"
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

    /// A private repository, over its cache ceiling, with a nonzero minutes
    /// figure — the fixture exercises every line either gauge can print (the
    /// base line, plus the cache overshoot warning) at once, so a width that
    /// clips any one of them shows up here rather than in only one of two
    /// separate, narrower fixtures.
    fn org_with_gauged_repo() -> crate::model::OrgSummary {
        let report = crate::billing::BillingReport {
            items: vec![crate::billing::UsageItem {
                month: "2026-07".into(),
                product: "actions".into(),
                sku: "Actions Linux".into(),
                quantity: 1_000.0,
                unit_type: "Minutes".into(),
                gross: 6.0,
                discount: 0.0,
                net: 6.0,
                repo: "josephine".into(),
            }],
        };
        crate::model::OrgSummary {
            login: "systm-d".into(),
            cache_bytes: 12_360_000_000,
            cache_count: 3,
            repos: vec![crate::model::RepoSummary {
                name: "josephine".into(),
                cache_bytes: 12_360_000_000,
                cache_count: 3,
                private: true,
                age_days: 5,
                class: crate::repos::RepoClass::Archivable,
            }],
            billing: Some(report),
        }
    }

    /// Task 3: the two gauges (`tui::views::gauges`) are drawn at the head of
    /// the resources column, for the repository `app.loaded` names — not
    /// wherever the repos cursor sits, and not through some
    /// separately-rendered widget the real `render` never touches.
    ///
    /// Renders the whole real layout and reads back only the resources
    /// column's `Rect` (`views::testing::focused_column`), never `f.area()`
    /// stood in for it: `views::render` never hands this column the whole
    /// frame — it first carves off the header/status/footer rows, then the
    /// columns — and a defect that only bites once that real width is
    /// accounted for, this column's own border and gauge/list split
    /// included, would stay invisible to a test searching a roomier
    /// fabricated area. Swept at every terminal width from 60 to 200, where
    /// the column is as narrow as `MIN_WIDTH` in the three- and two-column
    /// layouts.
    ///
    /// The fixture is built so a wrong implementation cannot pass by
    /// accident: capping the cache percentage at 100 would drop "115" and
    /// the eviction warning; a percent helper that divides by a genuinely
    /// zero ceiling would show `u64::MAX` instead of "50"; never wiring the
    /// gauges into `render` at all would show neither "Cache" nor "Minutes".
    #[test]
    fn the_two_gauges_render_at_the_head_of_the_real_resource_pane_across_swept_widths() {
        let mut app = App::new(vec![]);
        app.orgs = vec![org_with_gauged_repo()];
        app.loaded = Some(("systm-d".to_string(), "josephine".to_string()));
        app.resources = vec![res(false)];

        for width in 60..=200u16 {
            // 13 rows: the 3 fixed header/status/footer rows this crate's
            // real layout always reserves, plus the same 10-row body height
            // the file's other sweep tests used before the columns.
            let (_, rendered) =
                views::testing::focused_column(&mut app, Focus::Resources, width, 13);

            assert!(
                rendered.contains("Cache"),
                "cache gauge missing at width {width}:\n{rendered}"
            );
            assert!(
                rendered.contains("Minutes"),
                "minutes gauge missing at width {width}:\n{rendered}"
            );
            assert!(
                rendered.contains("115"),
                "cache overshoot percent clipped at width {width}:\n{rendered}"
            );
            assert!(
                rendered.contains("évince"),
                "cache eviction warning clipped at width {width}:\n{rendered}"
            );
            assert!(
                rendered.contains("50"),
                "minutes percent clipped at width {width}:\n{rendered}"
            );
        }
    }
}
