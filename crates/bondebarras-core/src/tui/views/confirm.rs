//! Tier-1 and tier-2 confirmation modals.
//!
//! v0.1 only deleted regenerable resources, so a single `[y/N]` was the
//! right amount of friction — `RiskTier::Medium` existed from the start but
//! had nothing mapped to it. v0.3's package versions are the first, and the
//! difference is not decoration: a cache or an artifact comes back with a
//! re-run, a package version does not — the layer leaves the registry for
//! good. So tier 2 lists what will go and says plainly that it will not come
//! back, instead of the tier-1 line that would be false for it. Tier 3
//! exists and is deliberately unused: repository deletion, the operation it
//! was conceived for, is permanently out of scope for this tool — no
//! resource maps to tier 3 today, and that name-typing flow has no owner.

use crate::clean::Plan;
use crate::model::RiskTier;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

/// How much a confirmation modal must show before asking `[y/N]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    /// A bare `[y/N]`: every item in the plan is regenerable by a re-run.
    Simple,
    /// The itemised recap plus the irreversibility warning: at least one
    /// item in the plan does not come back once deleted.
    Itemised,
}

/// Which modal a plan's tier calls for.
///
/// Tier 3 has no resource mapped to it yet, and none is planned: the
/// operation it was conceived for, repository deletion, is permanently out
/// of scope for this tool. `commands::clean::run` already refuses this tier
/// outright, so this arm is never reached today. It falls back to the more
/// cautious `Itemised` rather than `Simple`, so a future tier-3 resource
/// added without updating this modal degrades to "too much friction," never
/// "too little."
pub fn modal_kind(tier: RiskTier) -> ModalKind {
    match tier {
        RiskTier::Low => ModalKind::Simple,
        RiskTier::Medium | RiskTier::Nuclear => ModalKind::Itemised,
    }
}

/// How many items the itemised recap names before collapsing the rest into
/// a count. A repo in this account's own fleet (`maxds-lyon`) carries 28
/// package versions — past a handful, naming every one would grow the modal
/// past the terminal instead of informing.
const MAX_RECAP_ITEMS: usize = 8;

/// A centred box within `area`, sized by `width`/`height` constraints.
///
/// Takes `Constraint` rather than a raw percentage so a caller can centre a
/// box sized from its content (`Constraint::Length`) just as easily as one
/// sized as a fraction of the frame (`Constraint::Percentage`) — the
/// centring itself does not care which.
fn centered(width: Constraint, height: Constraint, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Fill(1), height, Constraint::Fill(1)])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), width, Constraint::Fill(1)])
        .split(v[1])[1]
}

/// Flattened text of a line, ignoring styling — used only to estimate how
/// many terminal rows `Wrap { trim: false }` will need for it.
fn flatten(line: &Line<'static>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

/// How many rows a `Paragraph` wrapped with `Wrap { trim: false }` needs to
/// render `lines` at `width` columns.
///
/// Ratatui exposes `Paragraph::line_count` for exactly this, but only
/// behind the `unstable-rendered-line-info` cargo feature — explicitly
/// documented upstream as liable to change even in a patch release, not
/// worth pulling in for one call site. A plain greedy word-wrap — pack
/// whole words onto a row until the next one would overflow, then start a
/// new one — is simple enough to trust on its own.
fn wrapped_row_count(lines: &[Line<'static>], width: u16) -> u16 {
    let width = width.max(1) as usize;
    let total: usize = lines.iter().map(|l| wrap_rows(&flatten(l), width)).sum();
    total.try_into().unwrap_or(u16::MAX)
}

/// Greedy word-wrap row count for one logical line, matching `Wrap { trim:
/// false }`.
///
/// Leading whitespace is preserved by that wrap mode and counts against the
/// width budget like any other character — an earlier version of this
/// function used `text.split_whitespace()` directly, which silently drops
/// leading whitespace, undercounting the two-space-indented item lines
/// `itemised_body` builds by exactly one row whenever a label's length
/// alone (without its indent) still just fit. A single word wider than
/// `width` — a long cache key with no spaces in it, say — hard-wraps across
/// several rows of its own here, the same way ratatui's own `WordWrapper`
/// does, rather than being counted as a single row that quietly overflows.
///
/// Implemented as one column position (`pos`) unrolled across rows of
/// `width`, rather than tracked per-row: a word that does not fit what is
/// left of the current row, but does fit a fresh one, skips ahead to the
/// next row boundary; everything else just accumulates.
fn wrap_rows(text: &str, width: usize) -> usize {
    let width = width.max(1);
    let stripped = text.trim_start();
    let indent = text.chars().count() - stripped.chars().count();

    let mut pos = indent;
    let mut first = true;
    for word in stripped.split_whitespace() {
        let len = word.chars().count();
        let sep = usize::from(!first);
        first = false;

        let col = pos % width;
        if col != 0 && len <= width && col + sep + len > width {
            // The whole word moves to a fresh row rather than splitting —
            // ordinary word-wrap. A word longer than `width` skips this and
            // falls through to the `else`, letting `pos` carry it across as
            // many rows as it needs from wherever it already stood.
            pos += width - col;
            pos += len;
        } else {
            pos += sep + len;
        }
    }

    // A blank line (`pos == 0`) still costs one visible row.
    pos.div_ceil(width).max(1)
}

/// The lines every modal opens with: the recap and the target repository.
fn header(plan: &Plan) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(plan.summary(), theme::text_style())),
        Line::from(Span::styled(
            format!("{}/{}", plan.owner, plan.repo),
            theme::muted(),
        )),
        Line::from(""),
    ]
}

fn regen_line() -> Line<'static> {
    Line::from(Span::styled(
        "Ces éléments sont régénérables par un re-run.",
        theme::muted(),
    ))
}

fn prompt_line() -> Line<'static> {
    Line::from(Span::styled("Supprimer ?   [y/N]", theme::title_style()))
}

fn warning_line() -> Line<'static> {
    Line::from(Span::styled(
        "Ces éléments ne reviendront pas : une fois supprimés, ils quittent le registre pour de bon.",
        theme::status_warn(),
    ))
}

/// Split across two `Line`s rather than one 154-character sentence: it
/// still wraps at a realistic terminal width either way (nothing here fits
/// unwrapped under ~225 columns), but wrapping mid-clause reads worse than
/// a deliberate break at the sentence boundary.
fn caveat_lines() -> [Line<'static>; 2] {
    [
        Line::from(Span::styled(
            "Une version sans tag peut être une couche d'une image multi-architecture : la \
             supprimer casserait son manifeste parent.",
            theme::muted(),
        )),
        Line::from(Span::styled(
            "GitHub ne permet pas de le vérifier.",
            theme::muted(),
        )),
    ]
}

/// The lines that must never be clipped: the confirmation prompt, and — for
/// a tier-2 plan — the irreversibility warning and the multi-arch caveat
/// that justify it. GitHub's API gives no way to check the multi-arch risk,
/// so the caveat has to live here, in the one place the user is guaranteed
/// to read it before confirming. A user who cannot see what is about to be
/// deleted can still refuse; a user who cannot see this cannot do either —
/// so unlike the recap, this is never truncated to fit a small frame.
///
/// `compact` drops the blank spacer lines — pure whitespace, no
/// information — the one concession this footer makes, and only once
/// `render` finds that even the full version does not fit the frame at
/// all. Nothing past that is negotiable: if the footer still does not fit
/// after that, the frame is simply too small for it, full stop.
fn footer(kind: ModalKind, compact: bool) -> Vec<Line<'static>> {
    match kind {
        ModalKind::Simple => vec![regen_line(), prompt_line()],
        ModalKind::Itemised => {
            let [caveat1, caveat2] = caveat_lines();
            if compact {
                vec![warning_line(), caveat1, caveat2, prompt_line()]
            } else {
                vec![
                    Line::from(""),
                    warning_line(),
                    caveat1,
                    caveat2,
                    Line::from(""),
                    prompt_line(),
                ]
            }
        }
    }
}

/// The recap: the plan summary, the target repository and — for a tier-2
/// plan — as many item labels as fit in `budget_rows`, capped at
/// [`MAX_RECAP_ITEMS`] regardless of how much room there is. This is what
/// gives way when the frame is tight, all the way down to nothing: the
/// footer must never lose a row to it, so the recap has to be able to give
/// up every one of its own rows, header included, rather than the other
/// way around.
fn recap(plan: &Plan, kind: ModalKind, budget_rows: u16, inner_width: u16) -> Vec<Line<'static>> {
    let hdr = header(plan);
    if kind == ModalKind::Simple {
        return hdr;
    }

    let hdr_rows = wrapped_row_count(&hdr, inner_width);
    if hdr_rows > budget_rows {
        return Vec::new();
    }

    let mut lines = hdr;
    // Reserve one row up front for a possible "… et N autre(s)" line: only
    // the loop below knows whether anything ends up hidden, and adding the
    // reservation retroactively could itself overflow the budget by the
    // row it was trying to make room for.
    let items_budget = budget_rows.saturating_sub(hdr_rows).saturating_sub(1);
    let mut item_rows_used = 0u16;
    let mut shown = 0usize;

    for item in plan.items.iter().take(MAX_RECAP_ITEMS) {
        let line = Line::from(Span::styled(format!("  {}", item.label), theme::muted()));
        let rows = wrap_rows(&flatten(&line), inner_width as usize) as u16;
        if item_rows_used + rows > items_budget {
            break;
        }
        item_rows_used += rows;
        shown += 1;
        lines.push(line);
    }

    if shown < plan.items.len() {
        lines.push(Line::from(Span::styled(
            format!("  … et {} autre(s)", plan.items.len() - shown),
            theme::muted(),
        )));
    }

    lines
}

pub fn render(plan: &Plan, f: &mut Frame, area: Rect) {
    let kind = modal_kind(plan.tier());

    // Width: itemised carries a recap list plus the multi-arch caveat, and
    // gets more of the frame than the bare tier-1 box so that caveat wraps
    // into fewer, more readable rows.
    let percent_x: u32 = match kind {
        ModalKind::Simple => 60,
        ModalKind::Itemised => 90,
    };
    let width = ((u32::from(area.width) * percent_x / 100).max(1) as u16).min(area.width.max(1));
    let inner_width = width.saturating_sub(2).max(1);
    let available = area.height.max(1).saturating_sub(2);

    // The footer is reserved first, and only it is allowed to claim it is
    // never clippable — see its own doc comment. The recap gets whatever
    // rows are left over, and truncates its item list (down to nothing, if
    // it must) to fit rather than the other way around, which is what let
    // an ordinary ten-cache-plus-one-package-version plan clip the warning,
    // the caveat and the prompt off an 80x24 terminal even after the
    // content-sized `height` fix: that fix sized the *box* from the
    // content, but a single `Paragraph` still clips *within* the box from
    // the bottom — exactly where the footer lives — whenever the content
    // does not fit the box after all.
    let mut footer_lines = footer(kind, false);
    let mut footer_rows = wrapped_row_count(&footer_lines, inner_width);
    if footer_rows > available {
        footer_lines = footer(kind, true);
        footer_rows = wrapped_row_count(&footer_lines, inner_width);
    }

    let recap_lines = recap(
        plan,
        kind,
        available.saturating_sub(footer_rows),
        inner_width,
    );
    let recap_rows = wrapped_row_count(&recap_lines, inner_width);

    let mut lines = recap_lines;
    lines.extend(footer_lines);

    let height = (2 + recap_rows + footer_rows).min(area.height.max(1));

    let zone = centered(Constraint::Length(width), Constraint::Length(height), area);
    f.render_widget(Clear, zone);

    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(" Confirmation ")
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        ),
        zone,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Resource, ResourceKind, RiskTier};

    fn item(kind: ResourceKind, id: u64, label: &str) -> Resource {
        Resource {
            kind,
            id,
            label: label.to_string(),
            size_bytes: 100,
            age_days: 10,
            git_ref: None,
            stale_pr: false,
            protected: false,
        }
    }

    fn plan(items: Vec<Resource>) -> Plan {
        Plan {
            items,
            owner: "systm-d".into(),
            repo: "repolens".into(),
        }
    }

    /// Flattens a rendered body to plain text for substring assertions.
    fn text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The modal's full, unconstrained content — every line `render` might
    /// show, with no width or row budget applied (`u16::MAX` both ways, so
    /// nothing here ever wraps or gets truncated for space). A seam for the
    /// tests below that assert on content rather than on a rendered buffer:
    /// `render` itself never calls this, since unconstrained content is
    /// exactly what clipped the prompt off an 80x24 terminal in the first
    /// place — the point of this whole review round.
    fn body(plan: &Plan) -> Vec<Line<'static>> {
        let kind = modal_kind(plan.tier());
        let mut lines = recap(plan, kind, u16::MAX, u16::MAX);
        lines.extend(footer(kind, false));
        lines
    }

    #[test]
    fn the_modal_matches_the_plans_tier() {
        // Tier 1 stays a bare [y/N]; tier 2 must add the itemised recap and
        // the irreversibility warning. A plan mixing both takes the higher.
        assert_eq!(modal_kind(RiskTier::Low), ModalKind::Simple);
        assert_eq!(modal_kind(RiskTier::Medium), ModalKind::Itemised);
    }

    #[test]
    fn a_tier_1_plan_gets_the_bare_body_with_no_irreversibility_warning() {
        // A wrong implementation that always renders the itemised body would
        // pass a test that only checked "some text renders" — this checks
        // the tier-1-only and tier-2-only lines are mutually exclusive.
        let p = plan(vec![item(ResourceKind::Cache, 1, "coverage-linux")]);
        let t = text(&body(&p));
        assert!(t.contains("régénérables"), "got: {t}");
        assert!(!t.contains("ne reviendront pas"), "got: {t}");
        assert!(!t.to_lowercase().contains("multi-arch"), "got: {t}");
    }

    #[test]
    fn a_tier_2_plan_lists_its_items_and_warns_they_wont_come_back() {
        // The false claim from tier 1 ("régénérables") must not leak into
        // tier 2: a package version does not come back once deleted.
        let p = plan(vec![item(
            ResourceKind::PackageVersion,
            9,
            "sha256:9a26c7080… (sans tag)",
        )]);
        let t = text(&body(&p));
        assert!(t.contains("sha256:9a26c7080… (sans tag)"), "got: {t}");
        assert!(t.contains("ne reviendront pas"), "got: {t}");
        assert!(!t.contains("régénérables"), "got: {t}");
    }

    #[test]
    fn a_tier_2_plan_names_the_multi_arch_risk() {
        // GitHub's API gives no way to check whether an untagged version is
        // still a layer of a multi-arch manifest, so the modal has to say so
        // — this is the one place the user will actually read it.
        let p = plan(vec![item(
            ResourceKind::PackageVersion,
            9,
            "sha256:9a26c7080… (sans tag)",
        )]);
        let t = text(&body(&p)).to_lowercase();
        assert!(t.contains("multi-arch"), "got: {t}");
        assert!(t.contains("manifeste"), "got: {t}");
    }

    #[test]
    fn a_mixed_plan_takes_the_higher_tier() {
        let p = plan(vec![
            item(ResourceKind::Cache, 1, "coverage-linux"),
            item(ResourceKind::PackageVersion, 9, "sha256:9a26c7080…"),
        ]);
        assert_eq!(modal_kind(p.tier()), ModalKind::Itemised);
        assert!(text(&body(&p)).contains("ne reviendront pas"));
    }

    #[test]
    fn a_long_plan_is_capped_with_a_count_of_the_rest() {
        // maxds-lyon alone carries 28 versions in one repo — a plan that
        // size must not grow the modal past the terminal, nor silently drop
        // the count of what it hid.
        let items: Vec<Resource> = (0..12)
            .map(|i| item(ResourceKind::PackageVersion, i, &format!("v{i}")))
            .collect();
        let t = text(&body(&plan(items)));
        assert!(t.contains("v0"), "got: {t}");
        assert!(!t.contains("v11"), "got: {t}");
        assert!(t.contains("autre"), "got: {t}");
    }

    #[test]
    fn the_tier_two_modal_shows_its_prompt_and_caveat_at_eighty_columns() {
        // Rendered, not stringly: every other test here asserts on the lines
        // this modal is built from, which is why a version of it that clipped
        // both the caveat and the [y/N] prompt off an 80x24 terminal passed
        // eleven reviews.
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let items: Vec<Resource> = (0..20)
            .map(|i| item(ResourceKind::PackageVersion, i, &format!("v{i}")))
            .collect();
        let p = plan(items);
        terminal.draw(|f| render(&p, f, f.area())).unwrap();

        let rendered: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(rendered.contains("[y/N]"), "the prompt must be on screen");
        assert!(
            rendered.contains("multi-architecture"),
            "the caveat must be on screen"
        );
    }

    #[test]
    fn the_prompt_survives_a_plan_whose_items_overflow_the_frame() {
        // Ten 71-char cache keys plus a package version: an everyday tier-2
        // plan, and the shape that clipped the prompt off an 80x24 terminal
        // after the first fix — the content-sized box still handed the
        // whole thing to one `Paragraph`, which clips from the bottom, and
        // the bottom is where the warning, the caveat and the prompt live.
        // The prompt and the warning are never clippable; items are what
        // gets dropped.
        let mut items: Vec<Resource> = (0..10)
            .map(|i| {
                item(
                    ResourceKind::Cache,
                    i,
                    &format!("Linux-x64-cargo-registry-{i:0>46}"),
                )
            })
            .collect();
        items.push(item(
            ResourceKind::PackageVersion,
            100,
            "sha256:9a26c7080… (sans tag)",
        ));
        let p = plan(items);

        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| render(&p, f, f.area())).unwrap();

        let rendered: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            rendered.contains("[y/N]"),
            "the prompt must be on screen: {rendered}"
        );
        assert!(
            rendered.contains("ne reviendront pas"),
            "the irreversibility warning must be on screen: {rendered}"
        );
        assert!(
            rendered.contains("multi-architecture"),
            "the caveat must be on screen: {rendered}"
        );
    }

    #[test]
    fn wrap_rows_accounts_for_an_items_two_space_indent() {
        // The label alone is exactly `width - 1` characters — one shy of
        // the available width — but `itemised_body`'s two-space indent,
        // preserved by `Wrap { trim: false }`, pushes the full rendered
        // line one character past it. `split_whitespace` alone would drop
        // the indent and miscount this as a single row.
        let width = 40usize;
        let label = "x".repeat(width - 1);
        let line = format!("  {label}");
        assert_eq!(
            wrap_rows(&line, width),
            2,
            "a {}-character line at width {width} must need two rows",
            line.chars().count()
        );
    }
}
