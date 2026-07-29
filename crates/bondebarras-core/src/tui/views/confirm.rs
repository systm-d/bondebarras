//! Tier-1 and tier-2 confirmation modals.
//!
//! v0.1 only deleted regenerable resources, so a single `[y/N]` was the
//! right amount of friction — `RiskTier::Medium` existed from the start but
//! had nothing mapped to it. v0.3's package versions are the first, and the
//! difference is not decoration: a cache or an artifact comes back with a
//! re-run, a package version does not — the layer leaves the registry for
//! good. So tier 2 lists what will go and says plainly that it will not come
//! back, instead of the tier-1 line that would be false for it. Tier 3
//! (repository deletion) lands in v0.5 with its own name-typing flow.

use crate::clean::Plan;
use crate::model::RiskTier;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

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
/// Tier 3 has no resource mapped to it yet — repository deletion is out of
/// scope until v0.5, and `commands::clean::run` already refuses that tier
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

/// A centred box, sized as a percentage of the frame.
fn centered(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(v[1])[1]
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
    lines.push(Line::from(Span::styled(
        "Une version sans tag peut être une couche d'une image multi-architecture : la \
         supprimer casserait son manifeste parent. GitHub ne permet pas de le vérifier.",
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
    // The itemised body carries a recap list plus two extra warning lines:
    // it needs more room than the bare tier-1 box.
    let (percent_x, percent_y) = match modal_kind(plan.tier()) {
        ModalKind::Simple => (60, 22),
        ModalKind::Itemised => (70, 55),
    };
    let zone = centered(percent_x, percent_y, area);
    f.render_widget(Clear, zone);

    f.render_widget(
        Paragraph::new(body(plan)).block(
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
}
