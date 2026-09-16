//! Column 3: the resources of the loaded repository, under its two gauges.

use crate::model::{Resource, ResourceKind, human_size};
use crate::refs::BranchClass;
use crate::safety::Safety;
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

/// The cells a row spends before its label: the checkbox with its safety
/// marker glued on (`"[x]⛑"`, 4) and the kind between two spaces
/// (`" cache "`, 7). A package version's class suffix, when it leaves the
/// row, starts this far in on the item's second line (`row_lines`).
const LABEL_COLUMN: usize = 4 + 7;

/// Every cell a row spends outside its label: `LABEL_COLUMN`, the size
/// between two spaces (`" {size:>5} "`, 7 — `compact_size` never takes more
/// than five) and the widest trailing classification (`"par défaut"`, or a
/// flag up to `"PR#99999 ⚑"`, 10).
///
/// Sized for `LABEL_FLOOR` (ruling R7-2, 2026-09-11): with the marker on a
/// cell and a space of its own, and `human_size`'s eight-cell sizes, the
/// same row left an 80-column terminal's resources column 5 label
/// characters.
const ROW_BESIDE_LABEL: usize = LABEL_COLUMN + 7 + 10;

/// The label characters every row keeps once the resources column has 40
/// inner cells, as it has at an 80-column terminal (ruling R7-2). Below 40
/// — 38 or 39 at a 100- or 101-column terminal, where three columns leave
/// this one at `MIN_WIDTH` — it is not promised.
///
/// Also the digest head a package version keeps beside its class suffix on
/// one line: with less room, the suffix takes a second line (`label_lines`).
const LABEL_FLOOR: usize = 12;

// The row budget leaves `LABEL_FLOOR` in 40 inner cells, or nothing builds.
const _: () = assert!(40 - ROW_BESIDE_LABEL >= LABEL_FLOOR);

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
/// as one head character and its `…` still fit beside it. A package version
/// reaches this only when its class suffix fits beside `LABEL_FLOOR` digest
/// characters: with less room, `label_lines` moves the suffix to a second
/// line first.
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

/// `bytes` in at most five cells, for a resource row's size: `467Mo`,
/// `1.1Go`, `12Go` — one decimal below ten of a unit, whole numbers from
/// ten, French decimal units, no space.
///
/// Resource rows alone use it (ruling R7-2). Their row must keep
/// `LABEL_FLOOR` label characters in a resources column of 40 inner cells,
/// and `human_size`'s `273.7 Mo` takes eight: the three cells this gives
/// back are what that budget lacked. The column's title, its gauges and the
/// orgs and repos columns have the room, and keep `human_size`.
///
/// Rounds to the nearest tenth or unit, in the unit whose rounded count
/// stays under a thousand — `999.5 Ko` reads `1.0Mo`, never `1000Ko`. In
/// integers throughout, so no boundary depends on how a float rounds; the
/// units run to `Eo`, which holds every `u64`.
fn compact_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["Ko", "Mo", "Go", "To", "Po", "Eo"];
    if bytes < 1000 {
        return format!("{bytes}o");
    }
    let bytes = u128::from(bytes);
    let rounded = |unit_bytes: u128| (bytes + unit_bytes / 2) / unit_bytes;
    let mut unit = 0;
    let mut unit_bytes: u128 = 1000;
    while unit + 1 < UNITS.len() && rounded(unit_bytes) >= 1000 {
        unit += 1;
        unit_bytes *= 1000;
    }
    let tenths = (bytes * 10 + unit_bytes / 2) / unit_bytes;
    if tenths < 100 {
        format!("{}.{}{}", tenths / 10, tenths % 10, UNITS[unit])
    } else {
        format!("{}{}", rounded(unit_bytes), UNITS[unit])
    }
}

/// A row's safety marker (spec §4), one cell glued to its checkbox (ruling
/// R7-2) so every kind and label starts in the same column: `⛑` for a safe
/// row, painted like the ⚑ flag — the safest thing on screen, never an
/// error; `•` for one to check, in the warning colour; a blank for one to
/// keep.
fn marker(safety: Safety) -> Span<'static> {
    match safety {
        Safety::Safe => Span::styled("⛑", theme::stale_style()),
        Safety::Check => Span::styled("•", theme::status_warn()),
        Safety::Keep => Span::styled(" ", theme::text_style()),
    }
}

/// `r`'s label fitted to `width` characters for its row, and the class
/// suffix a package version moves to its item's second line, if it does.
///
/// A package version's class — `(sans tag)`, `(attestation orpheline)`,
/// appended by `scan::version_label` — is what the row is offered for, and
/// it is never shortened (ruling R7-2). It stays on the row while its digest
/// keeps `LABEL_FLOOR` characters beside it; with less room, the whole
/// suffix goes to a second line and the digest has the row's label width to
/// itself. Every other label is fitted by `fit_label`, on its one line.
fn label_lines(r: &Resource, width: usize) -> (String, Option<String>) {
    if r.kind == ResourceKind::PackageVersion
        && r.label.chars().count() > width
        && r.label.ends_with(')')
        && let Some(start) = r.label.rfind(" (")
        && width < LABEL_FLOOR + r.label[start..].chars().count()
    {
        return (
            views::fit(&r.label[..start], width),
            Some(r.label[start + 1..].to_string()),
        );
    }
    (fit_label(&r.label, width), None)
}

/// A resource's list item, as lines: its row (`row_spans`), and under it —
/// for a package version whose class suffix left the row (`label_lines`) —
/// that suffix, starting in the label's column. One list item either way,
/// so the cursor's highlight and the tick cover both lines. That second line
/// takes `LABEL_COLUMN` plus the longest suffix, 23: 34 cells, and the
/// column never has fewer than 38 inside.
fn row_lines(r: &Resource, checked: bool, label_width: usize) -> Vec<Line<'static>> {
    let (label, suffix) = label_lines(r, label_width);
    let mut lines = vec![Line::from(spans_with_label(r, checked, label))];
    if let Some(suffix) = suffix {
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(LABEL_COLUMN)),
            Span::styled(suffix, theme::text_style()),
        ]));
    }
    lines
}

/// One row of the resource list, as styled spans, its label fitted to
/// `label_width` characters (`label_width_in` gives it for a real column):
/// the first line of its list item, when a package version's class suffix
/// takes a second (`row_lines`).
///
/// Split out from the widget so it can be asserted on without a terminal.
pub fn row_spans(r: &Resource, checked: bool, label_width: usize) -> Vec<Span<'static>> {
    spans_with_label(r, checked, label_lines(r, label_width).0)
}

/// `row_spans`, its label already fitted.
fn spans_with_label(r: &Resource, checked: bool, label: String) -> Vec<Span<'static>> {
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

    // A kind GitHub reports no size for shows `—` rather than a zero, which
    // would read as "empty" — the opposite of the truth. `has_known_size`
    // decides which kinds, here as in `model::size_display`, the headless
    // `clean` listing's `human_size` counterpart, so the two screens cannot
    // drift apart on it.
    let size = if r.kind.has_known_size() {
        compact_size(r.size_bytes)
    } else {
        "—".to_string()
    };

    let mut spans = vec![
        Span::styled(if checked { "[x]" } else { "[ ]" }, theme::text_style()),
        marker(r.safety),
        Span::styled(format!(" {kind} "), theme::muted()),
        Span::styled(label, theme::text_style()),
        Span::styled(format!(" {size:>5} "), theme::muted()),
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

/// Why some rows show `—` instead of a size, as the user reads it.
///
/// Not optional when the visible list holds at least one resource
/// `ResourceKind::has_known_size` says GitHub exposes no size for: a column
/// of `—` in a tool that shows bytes on every other screen reads as "these
/// are empty", which is the opposite of the truth.
const SIZELESS_WARNING: &str = "⚠ GitHub n'expose pas la taille de certaines ressources";

/// The resources column's title: the column's name, then the item count
/// and the size of the ticked rows (`App::selection_bytes`) — what `d` would
/// free — plus `SIZELESS_WARNING` when `with_warning` is true.
///
/// Smoke S3: that size stood bare, and `353 éléments · 0 o` over caches of
/// about 400 Mo each read as a listing weighing nothing. It says what it
/// counts: `cochés 0 o`.
///
/// Debt 2 of the v0.4 final review: the warning used to hang on
/// `has_packages`, fed by `render`'s own `kind ==
/// ResourceKind::PackageVersion` check — so a list whose only sizeless rows
/// were branches or tags carried the same `—` markers with no banner to
/// explain them. Generalised to whatever `has_known_size` calls sizeless,
/// the one place that already enumerates every such kind.
fn list_title(count: usize, ticked_bytes: u64, with_warning: bool) -> String {
    let ticked = human_size(ticked_bytes);
    if with_warning {
        format!(" RESSOURCES · {count} éléments · cochés {ticked} · {SIZELESS_WARNING} ")
    } else {
        format!(" RESSOURCES · {count} éléments · cochés {ticked} ")
    }
}

/// `list_title` without the word `éléments` nor the warning, for a column
/// too narrow for the whole title: its border clips from the right, and the
/// ticked size — the figure that matters before `d` — would go first. This
/// one takes 33 cells and the count's digits, so the column's narrowest
/// inside, 38 cells, holds it for up to 99 999 rows.
fn compact_title(count: usize, ticked_bytes: u64) -> String {
    format!(
        " RESSOURCES · {count} · cochés {} ",
        human_size(ticked_bytes)
    )
}

/// The column's title, and the lines to draw at the head of the column when
/// the title cannot hold `SIZELESS_WARNING` whole.
///
/// CLAUDE.md: a package version shows `—` "with a header line spelling out
/// why". The title sits on a top border `room` cells wide, and ratatui clips
/// whatever overflows it from the right — the warning, its last part, is
/// what went. So the warning rides in the title only when the whole title
/// fits; otherwise the title drops it and it becomes lines of its own inside
/// the column, broken at spaces (`views::wrap_words`) so every word stays on
/// screen. A title still too wide without it drops `éléments` too
/// (`compact_title`), so the ticked size is never the part clipped.
fn column_head(
    count: usize,
    bytes: u64,
    has_sizeless: bool,
    room: u16,
) -> (String, Vec<Line<'static>>) {
    let room = usize::from(room);
    let with_warning = list_title(count, bytes, true);
    if has_sizeless && views::cells(&with_warning) <= room {
        return (with_warning, Vec::new());
    }
    let full = list_title(count, bytes, false);
    let title = if views::cells(&full) <= room {
        full
    } else {
        compact_title(count, bytes)
    };
    let lines = if has_sizeless {
        views::wrap_words(SIZELESS_WARNING, room)
            .into_iter()
            .map(|line| Line::from(Span::styled(line, theme::status_warn())))
            .collect()
    } else {
        Vec::new()
    };
    (title, lines)
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
/// same reasoning as `App::take_plan`), as `(cache, minutes)`: two parts of
/// the column's head, each whole or absent (`head_within`). Both empty
/// before anything has loaded, or if the loaded repository has since left
/// its org's list (an org refresh, say): there is nothing to gauge yet.
fn repo_gauges(app: &App, width: u16) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let Some((org_login, repo_name)) = app.loaded.as_ref() else {
        return (Vec::new(), Vec::new());
    };
    let Some(org) = app.orgs.iter().find(|o| &o.login == org_login) else {
        return (Vec::new(), Vec::new());
    };
    let Some(repo) = org.repos.iter().find(|r| &r.name == repo_name) else {
        return (Vec::new(), Vec::new());
    };

    let cache = gauges::cache_gauge_line(repo.cache_bytes, width);
    let minutes_used = if repo.private {
        repo_minutes_used(org, &repo.name)
    } else {
        0
    };
    let minutes = gauges::minutes_gauge_line(
        minutes_used,
        !repo.private,
        crate::billing::included_minutes_for(org.plan.as_deref()),
        width,
    );
    (cache, minutes)
}

/// In place of the list when the column has too few lines for one of its
/// rows (final review I2).
const TOO_SHORT: &str = "(fenêtre trop basse)";

/// The column's title when it has no line inside its borders at all, for
/// `TOO_SHORT` to stand in on.
const TOO_SHORT_TITLE: &str = " RESSOURCES · fenêtre trop basse ";

/// One part of the column's head: its lines, and how late it yields them.
struct HeadPart {
    lines: Vec<Line<'static>>,
    /// The part with the lowest rank yields first.
    rank: u8,
}

/// The column's head within `budget` lines: its parts in the order given,
/// each whole or absent, the lowest-ranked dropped first until the rest
/// fits.
///
/// A part is never cut — ruling B for the size explanation, and the same
/// for a gauge (spec §5: its caveat, its warning, its reason) and for the
/// repository's name. A lower part never stays while a higher one went.
fn head_within(mut parts: Vec<HeadPart>, budget: usize) -> Vec<Line<'static>> {
    let lines = |parts: &[HeadPart]| parts.iter().map(|p| p.lines.len()).sum::<usize>();
    while lines(&parts) > budget {
        let Some(lowest) = parts
            .iter()
            .enumerate()
            .min_by_key(|(_, p)| p.rank)
            .map(|(i, _)| i)
        else {
            break;
        };
        parts.remove(lowest);
    }
    parts.into_iter().flat_map(|p| p.lines).collect()
}

/// Renders the resources column: its block, at its head the name of the
/// repository it shows (`tui::views::shown`), that repository's two gauges
/// (`tui::views::gauges`, spec §5) and then — when the title cannot hold it
/// — the sizeless warning (`column_head`), and under them the resource list,
/// as a stateful list so ratatui scrolls to keep the selection visible. On a
/// 69-cache repo, an 80x24 terminal only fits about 19 rows without this —
/// the plain `render_widget` used before left most of them unreachable.
///
/// The list comes first (final review I2): the head takes only the lines
/// left once the list has room for its tallest row — a package version's
/// row takes two, and ratatui draws nothing of a row taller than its area.
/// When the height runs short the head yields whole parts, lowest first: the
/// minutes gauge, then the cache gauge, then the size explanation, then the
/// repository's name (`head_within`). With too few lines for even one row,
/// the column draws no list and says so (`TOO_SHORT`), and records it in
/// `App::resources_too_short`, which keeps the list keys and `d` from acting
/// on rows no frame shows.
///
/// While that repository's listing is on its way, or failed, the column
/// draws no list, no count and no gauge: `tui::views::shown` says which.
pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let focused = app.focus == Focus::Resources;
    let shown = app.shown();
    let listing = shown.draws_resources();

    // The column's inside: the title does not change it.
    let inner = views::column_block("", focused).inner(area);

    // Everything below until `app.resources_too_short` reads `app` through
    // shared borrows only; the rows own their strings (`Line<'static>`), so
    // those borrows end once the rows are built.
    let visible = if listing {
        app.visible_resources()
    } else {
        Vec::new()
    };
    let has_sizeless = visible.iter().any(|r| !r.kind.has_known_size());
    let label_width = label_width_in(inner.width);
    let rows: Vec<Vec<Line<'static>>> = visible
        .iter()
        .map(|r| row_lines(r, app.selected.contains(&(r.kind, r.id)), label_width))
        .collect();
    let tallest = rows.iter().map(Vec::len).max().unwrap_or(1);
    // The title's room is the top border between its two corners.
    let (title, warning_lines) = if listing {
        column_head(
            visible.len(),
            app.selection_bytes(),
            has_sizeless,
            inner.width,
        )
    } else {
        (views::shown::BARE_TITLE.to_string(), Vec::new())
    };
    let head = if listing {
        let (cache, minutes) = repo_gauges(app, inner.width);
        let part = |lines, rank| HeadPart { lines, rank };
        head_within(
            vec![
                part(views::shown::head_lines(&shown, inner.width), 3),
                part(cache, 1),
                part(minutes, 0),
                part(warning_lines, 2),
            ],
            usize::from(inner.height).saturating_sub(tallest),
        )
    } else {
        views::shown::head_lines(&shown, inner.width)
    };

    let too_short = listing && usize::from(inner.height) < tallest;
    app.resources_too_short = too_short;
    let title = if too_short && inner.height == 0 {
        TOO_SHORT_TITLE.to_string()
    } else {
        title
    };
    f.render_widget(views::column_block(title, focused), area);
    if too_short {
        f.render_widget(
            Paragraph::new(Span::styled(TOO_SHORT, theme::muted())),
            inner,
        );
        return;
    }

    let list_area = if head.is_empty() {
        inner
    } else {
        let [head_area, list_area] =
            Layout::vertical([Constraint::Length(head.len() as u16), Constraint::Min(0)])
                .areas(inner);
        f.render_widget(Paragraph::new(head), head_area);
        list_area
    };

    let items: Vec<ListItem<'static>> = rows.into_iter().map(ListItem::new).collect();

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

    /// Ruling R7-2: a resource row's size in at most five cells, French
    /// units, no space — `467Mo`, `1.1Go`, `12Go`: one decimal below ten of
    /// a unit, whole numbers from ten, the next unit up once rounding would
    /// reach a thousand. Each boundary from both sides.
    #[test]
    fn compact_size_reads_in_french_units_across_each_boundary() {
        for (bytes, shown) in [
            (0, "0o"),
            (999, "999o"),
            (1_000, "1.0Ko"),
            (1_049, "1.0Ko"),
            (1_050, "1.1Ko"),
            (9_949, "9.9Ko"),
            (9_950, "10Ko"),
            (999_499, "999Ko"),
            (999_500, "1.0Mo"),
            (273_678_336, "274Mo"),
            (467_000_000, "467Mo"),
            (1_100_000_000, "1.1Go"),
            (12_000_000_000, "12Go"),
            (999_500_000_000, "1.0To"),
            (u64::MAX, "18Eo"),
        ] {
            assert_eq!(compact_size(bytes), shown, "for {bytes} bytes");
        }
    }

    /// The row budget counts five cells for a size, whatever the size.
    /// Swept over every magnitude a `u64` holds: each unit's rounding
    /// boundaries — 9.95, 99.95, 999.5 of it — one byte either side, then a
    /// geometric walk from one byte to `u64::MAX`.
    #[test]
    fn compact_size_never_takes_more_than_five_cells() {
        let mut sizes = vec![0, u64::MAX];
        for power in 0..=6u32 {
            let unit = 1000u128.pow(power);
            for thousandths in [
                1_000u128, 9_949, 9_950, 9_999, 10_000, 99_949, 99_950, 999_499, 999_500, 999_999,
            ] {
                if let Ok(bytes) = u64::try_from(unit * thousandths / 1000) {
                    sizes.extend([bytes.saturating_sub(1), bytes, bytes.saturating_add(1)]);
                }
            }
        }
        let mut walk = 1.0_f64;
        while walk < u64::MAX as f64 {
            sizes.push(walk as u64);
            walk *= 1.01;
        }

        for bytes in sizes {
            let shown = compact_size(bytes);
            assert!(
                views::cells(&shown) <= 5 && shown.ends_with('o'),
                "{bytes} bytes read {shown:?}"
            );
        }
    }

    #[test]
    fn a_row_shows_the_checkbox_kind_label_and_size() {
        let line = text(&row_spans(&res(false), true, LABEL_WIDTH));
        assert!(line.contains("[x]"));
        assert!(line.contains("cache"));
        assert!(line.contains("v0-rust-coverage-Linux-x64"));
        assert!(line.contains("274Mo"));
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
        // Every other screen shows bytes. A bare zero here — "0 o", or the
        // row's compact "0o" — would read as "empty", which is the opposite
        // of the truth.
        let line = text(&row_spans(&package_resource(), false, LABEL_WIDTH));
        assert!(!line.contains("0 o") && !line.contains("0o"), "got: {line}");
        assert!(line.contains('—'), "got: {line}");
    }

    /// `package_resource`'s sibling for the other offered class: an orphaned
    /// attestation. Built through the real `scan::version_label` from a full
    /// digest, like its sibling; its label is the longest a package version
    /// gets — `sha256:1d7018e56… (attestation orpheline)`, 41 characters.
    fn attestation_resource() -> Resource {
        let v = crate::packages::PackageVersion {
            id: 10,
            digest: "sha256:1d7018e5672547cced06883706367832e5f1be5fa90bc2038ad308e19958e80e"
                .into(),
            tags: vec![],
            age_days: 5,
        };
        Resource {
            kind: ResourceKind::PackageVersion,
            id: v.id,
            label: crate::scan::version_label(
                &v,
                crate::packages::VersionClass::OrphanedAttestation,
            ),
            size_bytes: 0,
            age_days: v.age_days,
            git_ref: None,
            stale_pr: false,
            protected: false,
            branch_class: None,
            safety: crate::safety::Safety::Safe,
        }
    }

    /// The row of `column` holding `needle`, without the column's left
    /// border: the line as its list item draws it.
    fn row_holding<'a>(column: &'a str, needle: &str) -> &'a str {
        column
            .lines()
            .find(|line| line.contains(needle))
            .and_then(|line| line.strip_prefix('│'))
            .unwrap_or_else(|| panic!("no row holds {needle}:\n{column}"))
    }

    /// Whether `suffix` sits on the row holding `head`, or on the line right
    /// under it — the second line of the same list item.
    fn suffix_follows(column: &str, head: &str, suffix: &str) -> bool {
        let lines: Vec<&str> = column.lines().collect();
        lines.iter().enumerate().any(|(i, line)| {
            line.contains(head)
                && (line.contains(suffix)
                    || lines.get(i + 1).is_some_and(|next| next.contains(suffix)))
        })
    }

    /// Rendered, not stringly: `row_spans` alone cannot show what actually
    /// reaches the screen. Before `version_label` elided its digest, this
    /// row's label alone ran to 82 characters — past column 80 before the
    /// checkbox and kind columns even get counted — pushing the `—` size
    /// marker and the `(sans tag)` class suffix off the visible buffer
    /// entirely, with no assertion here able to see it, since every other
    /// test in this module asserts on the spans, not on what a terminal
    /// would actually show.
    ///
    /// Read back from the resources column's real `Rect`
    /// (`views::testing::focused_column`); the name stays because `scan.rs`
    /// cites it. Task 7 gives its name back its guarantee (ruling R7-2): at
    /// an 80-column terminal — a 42-cell column, 40 inner cells, 12 of them
    /// label — both offered classes show whole, `(sans tag)` and the longest,
    /// `(attestation orpheline)`, though neither fits beside its digest
    /// there. A class suffix is never shortened: it moves to a second line of
    /// the same list item. Each suffix is asserted on its own digest's row or
    /// on the line right under it, so two swapped suffixes fail as well as a
    /// cut one; each digest's head must show for that. The same holds at
    /// every width from 60 to 200, with the `—` size marker.
    #[test]
    fn a_package_row_survives_at_eighty_columns() {
        let mut app = App::new(vec![]);
        app.resources = vec![package_resource(), attestation_resource()];
        let classes = [
            ("sha256:9a", "(sans tag)"),
            ("sha256:1d", "(attestation orpheline)"),
        ];

        let (rect, column) = views::testing::focused_column(&mut app, Focus::Resources, 80, 16);
        assert_eq!(
            rect.width, 42,
            "the resources column at an 80-column terminal"
        );
        for (head, suffix) in classes {
            assert!(
                suffix_follows(&column, head, suffix),
                "{suffix} is not whole under {head} at 80 columns:\n{column}"
            );
        }

        for width in 60..=200u16 {
            let (_, column) = views::testing::focused_column(&mut app, Focus::Resources, width, 16);
            assert!(
                column.contains('—'),
                "the size marker must survive at width {width}:\n{column}"
            );
            for (head, suffix) in classes {
                assert!(
                    suffix_follows(&column, head, suffix),
                    "{suffix} is not whole under {head} at width {width}:\n{column}"
                );
            }
        }
    }

    /// One cache per safety level, labels short enough to stay whole at
    /// every width swept: `sure1` is ⛑, `chk2` is •, `keep3` carries no
    /// marker.
    fn one_row_per_level() -> Vec<Resource> {
        [
            ("sure1", Safety::Safe),
            ("chk2", Safety::Check),
            ("keep3", Safety::Keep),
        ]
        .into_iter()
        .zip(1u64..)
        .map(|((label, safety), id)| Resource {
            id,
            label: label.into(),
            safety,
            ..res(false)
        })
        .collect()
    }

    /// Spec §4's marker, as ruled for the row budget (R7-2): `⛑` for a safe
    /// row, `•` for one to check, a blank for one to keep, glued to the
    /// checkbox — `[x]⛑`, `[ ]•`, `[ ] ` — so every kind and label starts in
    /// the same column. Read off the resources column's real `Rect` at every
    /// width from 60 to 200, row by row: a marker on the wrong row, or
    /// anywhere but right after its checkbox, fails, where a search of the
    /// whole column for `⛑` and `•` would not.
    #[test]
    fn each_row_carries_its_safety_marker_glued_to_its_checkbox_across_swept_widths() {
        let mut app = App::new(vec![]);
        app.resources = one_row_per_level();
        app.selected.insert((ResourceKind::Cache, 1));

        for width in 60..=200u16 {
            let (_, column) = views::testing::focused_column(&mut app, Focus::Resources, width, 12);
            for (label, start) in [
                ("sure1", "[x]⛑ cache "),
                ("chk2", "[ ]• cache "),
                ("keep3", "[ ]  cache "),
            ] {
                let row = row_holding(&column, label);
                assert!(
                    row.starts_with(start),
                    "the {label} row starts {row:?} at width {width}, not {start:?}"
                );
            }
        }
    }

    /// `⛑` says "safe to delete": painted like the ⚑ flag, never like an
    /// error — `the_flag_is_painted_stale_not_error`'s reason. `•` says "look
    /// first": the warning colour, not an error either.
    #[test]
    fn the_safety_markers_are_painted_safe_and_warning_never_error() {
        let rows = one_row_per_level();
        let painted = |r: &Resource, glyph: &str| {
            row_spans(r, false, LABEL_WIDTH)
                .into_iter()
                .find(|span| span.content == glyph)
                .map(|span| span.style)
        };
        assert_eq!(painted(&rows[0], "⛑"), Some(theme::stale_style()));
        assert_eq!(painted(&rows[1], "•"), Some(theme::status_warn()));
        assert_ne!(painted(&rows[0], "⛑"), Some(theme::status_error()));
        assert_ne!(painted(&rows[1], "•"), Some(theme::status_error()));
    }

    /// Rows whose fixed fields are each as wide as they get — a five-cell
    /// size beside a `PR#99999 ⚑` flag, a `par défaut` classification —
    /// with labels longer than any column holds and no parenthesised suffix,
    /// so each one renders cut and its head can be measured up to its `…`;
    /// plus the two package versions, whose digest ends in `…` of its own.
    fn widest_rows() -> Vec<Resource> {
        let row = |kind, id, label: &str| Resource {
            kind,
            id,
            label: label.into(),
            size_bytes: 0,
            age_days: 40,
            git_ref: None,
            stale_pr: false,
            protected: false,
            branch_class: None,
            safety: Safety::Keep,
        };
        vec![
            Resource {
                size_bytes: 467_000_000,
                git_ref: Some("refs/pull/99999/merge".into()),
                stale_pr: true,
                safety: Safety::Safe,
                ..row(
                    ResourceKind::Cache,
                    1,
                    "cache-v0-rust-coverage-Linux-x64-0123456789abcdef",
                )
            },
            Resource {
                protected: true,
                branch_class: Some(BranchClass::Default),
                ..row(
                    ResourceKind::Branch,
                    2,
                    "branch-release/2026-a-very-long-default-branch-name",
                )
            },
            Resource {
                size_bytes: 1_100_000,
                safety: Safety::Check,
                ..row(
                    ResourceKind::Artifact,
                    3,
                    "artifact-github-pages-deployment-bundle-for-the-docs",
                )
            },
            Resource {
                protected: true,
                ..row(
                    ResourceKind::Tag,
                    4,
                    "tag-v2026.09.11-release-candidate-with-annotations",
                )
            },
            package_resource(),
            attestation_resource(),
        ]
    }

    /// How each of `widest_rows` reads on screen: a head of its label, its
    /// size, its trailing field. No head is longer than 8 characters — all a
    /// 9-cell label shows before its `…`, one cell under the 10 the column's
    /// narrowest inner width leaves — so a label cut one cell short still
    /// finds its row, and fails on the 12-character measurement itself.
    const WIDEST_ROWS_READ_AS: [(&str, &str, &str); 6] = [
        ("cache-v", "467Mo", "PR#99999 ⚑"),
        ("branch-", "—", "par défaut"),
        ("artifac", "1.1Mo", "40j"),
        ("tag-v20", "—", "protégé"),
        ("sha256:9", "—", "5j"),
        ("sha256:1", "—", "5j"),
    ];

    /// How many characters of the label starting with `needle` `row` shows,
    /// up to and including the `…` that ends it.
    fn rendered_head(row: &str, needle: &str) -> usize {
        let rest = &row[row.find(needle).expect("the row holds the needle")..];
        let end = rest
            .find('…')
            .unwrap_or_else(|| panic!("the label starting {needle} is not cut: {row:?}"));
        rest[..end].chars().count() + 1
    }

    /// Ruling R7-2's guarantee, stated on the column's inner width: once the
    /// resources column has at least 40 inner cells — as it has at an
    /// 80-column terminal — every row keeps at least 12 label characters,
    /// even beside its widest fixed fields (`widest_rows`). At 38 or 39
    /// inner cells — a 100- or 101-column terminal, where three columns
    /// leave this one at its `MIN_WIDTH` — it is not promised. At every width
    /// from 60 to 200 each row keeps its size and trailing field whole: the
    /// label is the part that gives. Measured on the resources column's real
    /// `Rect`, row by row.
    #[test]
    fn every_row_keeps_twelve_label_characters_once_the_column_has_forty_inner_cells() {
        let mut app = App::new(vec![]);
        app.resources = widest_rows();

        for width in 60..=200u16 {
            let (rect, column) =
                views::testing::focused_column(&mut app, Focus::Resources, width, 24);
            let inner = rect.width - 2;
            for (needle, size, trailing) in WIDEST_ROWS_READ_AS {
                let row = row_holding(&column, needle);
                assert!(
                    row.contains(size) && row.contains(trailing),
                    "the {needle} row lost its size or trailing field at width {width}: {row:?}"
                );
                if inner >= 40 {
                    let head = rendered_head(row, needle);
                    assert!(
                        head >= 12,
                        "the {needle} row keeps {head} label characters in {inner} inner cells \
                         at width {width}: {row:?}"
                    );
                }
            }
        }
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
        assert!(line.contains("2.4Mo"), "got: {line}");
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
                column.contains('—') && column.contains("2.4Mo"),
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
    ///
    /// Read back from the resources column's real `Rect`
    /// (`views::testing::focused_column`) at every terminal width from 60 to
    /// 200, and asserting the whole explanation rather than one word of it:
    /// a title clipped at its border can keep "GitHub" and lose the rest.
    #[test]
    fn the_title_warns_when_the_visible_list_holds_a_branch_with_no_package_present() {
        let mut app = App::new(vec![]);
        app.resources = vec![branch("claude/landing-3jbqk4", BranchClass::Merged)];

        for width in 60..=200u16 {
            let (_, column) = views::testing::focused_column(&mut app, Focus::Resources, width, 12);
            assert!(
                views::testing::unwrapped(&column).contains(EXPLANATION),
                "a branch-only list must still warn that its size is unknown at width \
                 {width}:\n{column}"
            );
        }
    }

    /// The explanation for `—`, word for word as the user reads it — typed
    /// out here rather than read from the production constant, so a change
    /// to what the screen says cannot pass unnoticed.
    const EXPLANATION: &str = "⚠ GitHub n'expose pas la taille de certaines ressources";

    /// CLAUDE.md: a package version shows `—` "with a header line spelling
    /// out why". That explanation rode in the column's title, and a title
    /// that overflows its border is clipped from the right — the
    /// explanation, its last part, is what went, at every width where the
    /// column is narrower than the whole title. Swept over the resources
    /// column's real `Rect` from 60 to 200 with a package version, asserting
    /// the whole explanation is on screen in order: in the title where it
    /// fits, otherwise on rows of its own inside the column, which
    /// `views::testing::unwrapped` joins back together.
    #[test]
    fn the_sizeless_explanation_is_never_clipped_across_swept_widths() {
        let mut app = App::new(vec![]);
        app.resources = vec![package_resource()];

        for width in 60..=200u16 {
            let (_, column) = views::testing::focused_column(&mut app, Focus::Resources, width, 12);
            assert!(
                views::testing::unwrapped(&column).contains(EXPLANATION),
                "the explanation for `—` is clipped at width {width}:\n{column}"
            );
        }
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
            plan: Some("free".into()),
            budgets: None,
            retention: None,
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
            // 30 rows: tall enough for the whole head at every width. The
            // explanations wrap onto rows of their own in a narrow column,
            // and a head without the room yields whole gauges to the list
            // (`the_column_head_keeps_each_part_whole_or_drops_it_across_swept_heights`).
            let (_, rendered) =
                views::testing::focused_column(&mut app, Focus::Resources, width, 30);

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

    /// `org_with_gauged_repo`'s repository, `private` or not, holding
    /// `cache_bytes` of cache, loaded, listing a cache and a package version
    /// — the version's `—` brings the size explanation into the head too.
    fn gauged_app(private: bool, cache_bytes: u64) -> App {
        let mut org = org_with_gauged_repo();
        org.repos[0].private = private;
        org.repos[0].cache_bytes = cache_bytes;
        let mut app = App::new(vec![org]);
        app.loaded = Some(("systm-d".to_string(), "josephine".to_string()));
        app.resources = vec![res(false), package_resource()];
        app
    }

    /// What the gauges say, word for word as the user reads them — typed out
    /// here rather than read from the production strings, so a change to
    /// what the screen says cannot pass unnoticed.
    const CACHE_CAVEAT: &str = "(plafond GitHub, non exposé par l'API)";
    const EVICTION: &str = "⚠ évince : GitHub supprime déjà les caches les moins récemment lus, \
                            y compris ceux de la branche par défaut, au profit des PR fermées";
    const PUBLIC_REASON: &str =
        "0 % (dépôt public : minutes Actions gratuites et illimitées, hors plafond)";
    const UNKNOWN_PLAN: &str = "1000 min · formule inconnue, pas de quota";

    /// Final review I4 and smoke S1: spec §5 wants the hardcoded ceiling
    /// said ("la jauge le dit") and a public repository's 0 % given with its
    /// reason; §7 wants nothing cut. Each gauge was one line, clipped at the
    /// column's border: at 80 columns `12.4 Go / 10` lost its unit — reading
    /// as 124 % — and the caveat, the eviction warning and the public reason
    /// went mid-word.
    ///
    /// Swept over every terminal width from 60 to 200 on the resources
    /// column's real rect, for a public repository over its ceiling, a
    /// private one under it, and that private one in an org whose plan was
    /// not read (#11): each gauge's percentage and figures, with their
    /// units, stand whole on one row; the caveat, the eviction warning, the
    /// public reason and the unknown plan's explanation read whole once the
    /// column's rows are joined — on the gauge's row where they fit, on rows
    /// of their own otherwise — and the unknown plan's minutes row carries
    /// no percentage at all.
    #[test]
    fn every_gauge_figure_and_explanation_stays_whole_across_swept_widths() {
        let mut public = gauged_app(false, 12_360_000_000);
        let mut private = gauged_app(true, 4_000_000_000);
        let mut unknown_plan = gauged_app(true, 4_000_000_000);
        unknown_plan.orgs[0].plan = None;
        for width in 60..=200u16 {
            let (_, column) =
                views::testing::focused_column(&mut public, Focus::Resources, width, 40);
            let prose = views::testing::unwrapped(&column);
            let cache = row_holding(&column, "115 %");
            assert!(
                cache.contains("12.4 Go / 10 Gio"),
                "the cache figures are cut at width {width}: {cache:?}\n{column}"
            );
            for phrase in [CACHE_CAVEAT, EVICTION, PUBLIC_REASON] {
                assert!(
                    prose.contains(phrase),
                    "{phrase:?} is not whole at width {width}:\n{column}"
                );
            }

            let (_, column) =
                views::testing::focused_column(&mut private, Focus::Resources, width, 40);
            let prose = views::testing::unwrapped(&column);
            let cache = row_holding(&column, "37 %");
            assert!(
                cache.contains("4.0 Go / 10 Gio"),
                "the cache figures are cut at width {width}: {cache:?}\n{column}"
            );
            let minutes = row_holding(&column, "50 %");
            assert!(
                minutes.contains("1000 / 2000"),
                "the minutes figures are cut at width {width}: {minutes:?}\n{column}"
            );
            assert!(
                prose.contains(CACHE_CAVEAT),
                "the ceiling caveat is not whole at width {width}:\n{column}"
            );
            assert!(
                !column.contains("évince"),
                "a cache under its ceiling warns of eviction at width {width}:\n{column}"
            );

            let (_, column) =
                views::testing::focused_column(&mut unknown_plan, Focus::Resources, width, 40);
            let prose = views::testing::unwrapped(&column);
            let minutes = row_holding(&column, "Minutes");
            assert!(
                minutes.contains("1000 min") && !minutes.contains('%'),
                "the unknown plan's minutes row is cut or has a percentage at width \
                 {width}: {minutes:?}\n{column}"
            );
            assert!(
                prose.contains(UNKNOWN_PLAN),
                "the unknown plan's explanation is not whole at width {width}:\n{column}"
            );
        }
    }

    /// Final review I2 and I4 together: on a short terminal the head gives
    /// its lines to the list, and what it keeps it keeps whole — a gauge
    /// with its figures and every line of its explanation, the size
    /// explanation in full (ruling B), or none of them. Swept over every
    /// height from 3 to 40 at widths across the three layouts, public
    /// repository over its ceiling: whatever part of the head is on screen
    /// reads whole, and no explanation stays on screen without its gauge.
    ///
    /// Task 5's review (T5-m3): the same sweep for a private repository in
    /// an org whose plan was not read (#11), the taller unknown-plan minutes
    /// gauge — its `formule inconnue, pas de quota` explanation stays with
    /// its figures, whole, or goes with them.
    #[test]
    fn the_column_head_keeps_each_part_whole_or_drops_it_across_swept_heights() {
        for width in [60u16, 78, 80, 99, 100, 101, 120, 160, 200] {
            for height in 3u16..=40 {
                let mut unknown_plan = gauged_app(true, 4_000_000_000);
                unknown_plan.orgs[0].plan = None;
                let (_, column) = views::testing::focused_column(
                    &mut unknown_plan,
                    Focus::Resources,
                    width,
                    height,
                );
                let at = format!("at {width}x{height}, no plan read:\n{column}");
                if column.contains("Minutes") {
                    assert!(
                        views::testing::unwrapped(&column).contains(UNKNOWN_PLAN),
                        "the unknown plan's minutes gauge shows without all of its \
                         explanation {at}"
                    );
                } else {
                    assert!(
                        !column.contains("formule inconnue"),
                        "the unknown plan's explanation shows without its gauge {at}"
                    );
                }

                let mut app = gauged_app(false, 12_360_000_000);
                let (_, column) =
                    views::testing::focused_column(&mut app, Focus::Resources, width, height);
                let prose = views::testing::unwrapped(&column);
                let at = format!("at {width}x{height}:\n{column}");

                let cache_shown = column.contains("Cache");
                if cache_shown {
                    let cache = row_holding(&column, "Cache");
                    assert!(
                        cache.contains("115 %") && cache.contains("12.4 Go / 10 Gio"),
                        "the cache gauge's figures are cut {at}"
                    );
                    assert!(
                        prose.contains(CACHE_CAVEAT) && prose.contains(EVICTION),
                        "the cache gauge shows without all of its explanation {at}"
                    );
                } else {
                    assert!(
                        !column.contains("plafond") && !column.contains("évince"),
                        "a cache gauge's explanation shows without its gauge {at}"
                    );
                }

                if column.contains("Minutes") {
                    assert!(
                        prose.contains(PUBLIC_REASON),
                        "the minutes gauge shows without its reason {at}"
                    );
                } else {
                    assert!(
                        !column.contains("dépôt public"),
                        "the public reason shows without its gauge {at}"
                    );
                }

                if column.contains("n'expose") {
                    assert!(
                        prose.contains(EXPLANATION),
                        "the size explanation is cut {at}"
                    );
                }
            }
        }
    }

    /// Smoke S3: the title read `RESSOURCES · 353 éléments · 0 o` over caches
    /// of about 400 Mo each. That figure is the size of the ticked rows
    /// (`App::selection_bytes`) — what `d` would free — not the listing's
    /// total, and unlabelled it read as "these 353 rows weigh nothing".
    ///
    /// Swept over every width from 60 to 200 on the resources column's real
    /// rect, with sized caches, nothing ticked and then the biggest ticked:
    /// the title's row names the figure as what is ticked, whole — `cochés
    /// 0 o`, then `cochés 467.0 Mo` — and never shows a bare `éléments · 0 o`.
    #[test]
    fn the_title_says_its_size_is_the_ticked_rows_across_swept_widths() {
        let mut app = App::new(vec![]);
        app.resources = vec![
            Resource {
                id: 1,
                size_bytes: 467_000_000,
                ..res(false)
            },
            Resource {
                id: 2,
                size_bytes: 12_000_000,
                ..res(false)
            },
        ];
        for width in 60..=200u16 {
            for (ticked, figure, bare) in [
                (false, "cochés 0 o", "éléments · 0 o"),
                (true, "cochés 467.0 Mo", "éléments · 467.0 Mo"),
            ] {
                app.selected.clear();
                if ticked {
                    app.selected.insert((ResourceKind::Cache, 1));
                }
                let (_, column) =
                    views::testing::focused_column(&mut app, Focus::Resources, width, 12);
                let title = column.lines().next().unwrap_or_default();
                assert!(
                    title.contains("RESSOURCES") && title.contains(figure),
                    "the title does not say {figure:?} whole at width {width}: {title:?}"
                );
                assert!(
                    !title.contains(bare),
                    "the title shows an unlabelled size at width {width}: {title:?}"
                );
            }
        }
    }
}
