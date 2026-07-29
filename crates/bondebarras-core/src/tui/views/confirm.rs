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
/// worth pulling in for one call site. This modal has exactly one line long
/// enough to actually wrap (the multi-arch caveat), so a plain greedy
/// word-wrap — pack whole words onto a row until the next one would
/// overflow, then start a new one — is simple enough to trust on its own.
fn wrapped_row_count(lines: &[Line<'static>], width: u16) -> u16 {
    let width = width.max(1) as usize;
    let total: usize = lines.iter().map(|l| wrap_rows(&flatten(l), width)).sum();
    total.try_into().unwrap_or(u16::MAX)
}

/// Greedy word-wrap row count for one logical line. A blank line still
/// costs one row: it is a visible gap, not zero rows.
fn wrap_rows(text: &str, width: usize) -> usize {
    let mut rows = 1usize;
    let mut col = 0usize;
    for word in text.split_whitespace() {
        let len = word.chars().count();
        let sep = if col == 0 { 0 } else { 1 };
        if col > 0 && col + sep + len > width {
            rows += 1;
            col = len;
        } else {
            col += sep + len;
        }
    }
    rows
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

/// Tier 1: every item is regenerable, so a re-run undoes any mistake.
fn simple_body(plan: &Plan) -> Vec<Line<'static>> {
    let mut lines = header(plan);
    lines.push(Line::from(Span::styled(
        "Ces éléments sont régénérables par un re-run.",
        theme::muted(),
    )));
    lines.push(Line::from(Span::styled(
        "Supprimer ?   [y/N]",
        theme::title_style(),
    )));
    lines
}

/// Tier 2: at least one item is gone for good once deleted, so the modal
/// names what will go and says so plainly — the tier-1 "régénérable" claim
/// would be false here.
fn itemised_body(plan: &Plan) -> Vec<Line<'static>> {
    let mut lines = header(plan);

    for item in plan.items.iter().take(MAX_RECAP_ITEMS) {
        lines.push(Line::from(Span::styled(
            format!("  {}", item.label),
            theme::muted(),
        )));
    }
    if plan.items.len() > MAX_RECAP_ITEMS {
        lines.push(Line::from(Span::styled(
            format!("  … et {} autre(s)", plan.items.len() - MAX_RECAP_ITEMS),
            theme::muted(),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Ces éléments ne reviendront pas : une fois supprimés, ils quittent le registre pour de bon.",
        theme::status_warn(),
    )));
    // GitHub's API gives no way to check this — the caveat has to live here,
    // in the one place the user is guaranteed to read before confirming.
    // Split across two `Line`s rather than one 154-character sentence: it
    // still wraps at a realistic terminal width either way (nothing here fits
    // unwrapped under ~225 columns), but wrapping mid-clause reads worse than
    // a deliberate break at the sentence boundary.
    lines.push(Line::from(Span::styled(
        "Une version sans tag peut être une couche d'une image multi-architecture : la \
         supprimer casserait son manifeste parent.",
        theme::muted(),
    )));
    lines.push(Line::from(Span::styled(
        "GitHub ne permet pas de le vérifier.",
        theme::muted(),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Supprimer ?   [y/N]",
        theme::title_style(),
    )));
    lines
}

/// The modal's body, chosen by the plan's tier — never decided by the caller.
fn body(plan: &Plan) -> Vec<Line<'static>> {
    match modal_kind(plan.tier()) {
        ModalKind::Simple => simple_body(plan),
        ModalKind::Itemised => itemised_body(plan),
    }
}

pub fn render(plan: &Plan, f: &mut Frame, area: Rect) {
    let lines = body(plan);

    // Width: itemised carries a recap list plus the multi-arch caveat, and
    // gets more of the frame than the bare tier-1 box so that caveat wraps
    // into fewer, more readable rows.
    let percent_x: u32 = match modal_kind(plan.tier()) {
        ModalKind::Simple => 60,
        ModalKind::Itemised => 90,
    };
    let width = ((u32::from(area.width) * percent_x / 100).max(1) as u16).min(area.width.max(1));
    let inner_width = width.saturating_sub(2).max(1);

    // Height: sized from the content, not guessed as a percentage. A
    // percentage silently clips whatever does not fit — the eight-item recap,
    // the irreversibility warning, the multi-arch caveat and the `[y/N]`
    // prompt itself all fell off an 80x24 terminal this way — and `Paragraph`
    // clips rather than scrolls, so getting this number wrong is exactly as
    // bad as the percentage it replaces. `wrapped_row_count` accounts for
    // `Wrap { trim: false }` reflowing the caveat, still clamped to what the
    // frame actually has in case content ever outgrows even that.
    let height = (wrapped_row_count(&lines, inner_width) + 2).min(area.height.max(1));

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
}
