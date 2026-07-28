//! The Billing tab: strictly diagnostic, no destructive action.
//!
//! Minutes cannot be reclaimed retroactively, so the only useful thing this
//! view can do is name the repository burning them.

use crate::billing::FREE_MINUTES_PER_MONTH;
use crate::tui::app::App;
use crate::tui::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

/// One line summarising allowance consumption.
///
/// Deliberately not clamped at 100 %: an org at 818 % of its included minutes
/// is exactly the situation the tab exists to surface, and a full bar would
/// say nothing.
pub fn gauge_line(used: u64, allowance: u64) -> String {
    let percent = if allowance == 0 {
        0
    } else {
        (used as f64 / allowance as f64 * 100.0).round() as u64
    };
    let filled = (percent as usize / 10).min(20);
    format!(
        "{} / {}   {}  {} %",
        thousands(used),
        thousands(allowance),
        "█".repeat(filled),
        percent
    )
}

/// Groups digits with a narrow space, as French convention wants.
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

pub fn render(app: &mut App, f: &mut Frame, area: Rect) {
    let Some(org) = app.orgs.get(app.org_cursor) else {
        f.render_widget(
            Paragraph::new(Span::styled("Aucune organisation.", theme::muted())),
            area,
        );
        return;
    };

    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        org.login.clone(),
        theme::title_style(),
    ))];

    match &org.billing {
        None => lines.push(Line::from(Span::styled(
            "⚠ facturation illisible — vous n'êtes pas propriétaire de cette organisation",
            theme::status_warn(),
        ))),
        Some(report) => {
            let months = report.months();
            let month = months
                .get(app.month_cursor.min(months.len().saturating_sub(1)))
                .cloned()
                .unwrap_or_default();

            lines.push(Line::from(Span::styled(month.clone(), theme::muted())));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Minutes équivalent-inclus",
                theme::text_style(),
            )));
            lines.push(Line::from(Span::styled(
                gauge_line(report.included_minutes(&month), FREE_MINUTES_PER_MONTH),
                theme::text_style(),
            )));

            let (gross, covered, billed) = report.cost(&month);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!(
                    "Coûts   brut {gross:.2} €   couvert {covered:.2} €   facturé {billed:.2} €"
                ),
                if billed > 0.0 {
                    theme::status_warn()
                } else {
                    theme::muted()
                },
            )));

            for sku in report.unknown_skus(&month) {
                lines.push(Line::from(Span::styled(
                    format!("⚠ SKU inconnu, compté ×1 : {sku}"),
                    theme::status_warn(),
                )));
            }
        }
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Billing ")
                .borders(Borders::ALL)
                .border_style(theme::border_style()),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gauge_reports_overshoot_rather_than_capping_at_full() {
        // 818 % is the real figure measured on systm-d in July 2026. Clamping
        // it to 100 % would hide exactly the thing the tab exists to show.
        let line = gauge_line(16_369, 2_000);
        assert!(line.contains("818"), "got: {line}");
        assert!(
            line.contains("16 369") || line.contains("16369"),
            "got: {line}"
        );
    }

    #[test]
    fn a_zero_allowance_does_not_divide_by_zero() {
        let line = gauge_line(100, 0);
        assert!(!line.contains("NaN"), "got: {line}");
        assert!(!line.contains("inf"), "got: {line}");
    }

    #[test]
    fn an_unused_month_reads_zero_percent() {
        assert!(gauge_line(0, 2_000).contains('0'));
    }
}
