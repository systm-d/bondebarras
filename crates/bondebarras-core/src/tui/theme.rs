//! bondebarras TUI theme.
//!
//! The palette does not paint a full background: it inherits the terminal's,
//! the way claudine does. Only the accent, the text and the state colours are
//! ours.

use ratatui::style::{Color, Modifier, Style};

/// Primary — titles, active cursor.
pub const PRIMARY: Color = Color::Rgb(0xd9, 0x77, 0x57);
/// Deep tone for borders and chrome.
pub const BORDER: Color = Color::Rgb(0x7c, 0x3a, 0x00);
/// Base text.
pub const TEXT: Color = Color::Rgb(0xec, 0xe6, 0xe0);
/// Secondary text: sizes, ages, metadata.
pub const MUTED: Color = Color::Rgb(0xa8, 0x9e, 0x95);
/// A deletion that succeeded.
pub const SUCCESS: Color = Color::Rgb(0x9e, 0xc2, 0x7e);
/// Degraded state: an org that could not be read.
pub const WARNING: Color = Color::Rgb(0xc9, 0xa3, 0x5a);
/// A deletion that failed.
pub const ERROR: Color = Color::Rgb(0xc8, 0x70, 0x5c);
/// The ⚑ flag — a cache whose pull request is closed. Deliberately distinct
/// from ERROR: this is the safest thing on screen to delete, not a problem.
pub const STALE: Color = Color::Rgb(0x7e, 0xa8, 0xc2);
/// Text drawn on top of the primary colour.
pub const SEL_FG: Color = Color::Rgb(0x1a, 0x12, 0x0d);

pub fn title_style() -> Style {
    Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
}
pub fn border_style() -> Style {
    Style::default().fg(BORDER)
}
pub fn text_style() -> Style {
    Style::default().fg(TEXT)
}
pub fn muted() -> Style {
    Style::default().fg(MUTED)
}
pub fn status_warn() -> Style {
    Style::default().fg(WARNING)
}
pub fn status_error() -> Style {
    Style::default().fg(ERROR)
}
pub fn status_success() -> Style {
    Style::default().fg(SUCCESS)
}
pub fn stale_style() -> Style {
    Style::default().fg(STALE)
}
pub fn selection_style() -> Style {
    Style::default()
        .bg(PRIMARY)
        .fg(SEL_FG)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stale_flag_has_its_own_colour() {
        // ⚑ must not read as an error: it marks the safest thing to delete.
        assert_ne!(STALE, ERROR);
        assert_eq!(stale_style().fg, Some(STALE));
    }

    #[test]
    fn selection_inverts_the_primary_colour() {
        let s = selection_style();
        assert_eq!(s.bg, Some(PRIMARY));
    }
}
