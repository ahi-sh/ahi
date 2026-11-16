//! Builtin syntax and chrome styles for transcript formatting.
//! Configurable message backgrounds and edit-diff colors live in `appearance`.

use ratatui_core::style::{Color, Modifier, Style};

pub(crate) const ACCENT: Color = Color::Rgb(120, 170, 220);
const DIM: Color = Color::Rgb(130, 130, 130);

pub(crate) fn dim() -> Style {
    Style::default().fg(DIM)
}
pub(crate) fn heading() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}
pub(crate) fn bold() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}
pub(crate) fn italic() -> Style {
    Style::default().add_modifier(Modifier::ITALIC)
}
pub(crate) fn strike() -> Style {
    Style::default().add_modifier(Modifier::CROSSED_OUT)
}
pub(crate) fn inline_code() -> Style {
    Style::default()
        .fg(Color::Rgb(230, 200, 140))
        .bg(Color::Rgb(45, 45, 55))
}
/// Fenced block body and header background.
pub(crate) fn code_block() -> Style {
    Style::default().bg(Color::Rgb(38, 38, 48))
}
pub(crate) fn code_header() -> Style {
    Style::default().fg(DIM).bg(Color::Rgb(38, 38, 48))
}
pub(crate) fn link() -> Style {
    Style::default()
        .fg(ACCENT)
        .add_modifier(Modifier::UNDERLINED)
}
pub(crate) fn file_ref() -> Style {
    Style::default().add_modifier(Modifier::UNDERLINED)
}
pub(crate) fn quote_bar() -> Style {
    Style::default().fg(ACCENT)
}
pub(crate) fn list_marker() -> Style {
    Style::default().fg(ACCENT)
}
pub(crate) fn rule() -> Style {
    dim()
}
pub(crate) fn tool_header() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}
pub(crate) fn stream_label() -> Style {
    dim()
}
pub(crate) fn diff_hunk() -> Style {
    Style::default().fg(Color::Rgb(150, 160, 200))
}
pub(crate) fn gutter() -> Style {
    dim()
}

/// Syntax token colors.
pub(crate) fn token(kind: super::highlight::Token) -> Style {
    use super::highlight::Token;
    let color = match kind {
        Token::Keyword => Color::Rgb(200, 130, 200),
        Token::Type => Color::Rgb(130, 200, 200),
        Token::String => Color::Rgb(160, 200, 130),
        Token::Number => Color::Rgb(230, 170, 120),
        Token::Comment => DIM,
        Token::Punct => Color::Rgb(180, 180, 180),
        Token::Function => Color::Rgb(130, 170, 230),
        Token::Variable => Color::Rgb(220, 160, 160),
        Token::Attribute => Color::Rgb(200, 190, 130),
        Token::Plain => return Style::default(),
    };
    Style::default().fg(color)
}

/// `0.4s`, `3.8s`, `12s`, `1m 12s`, `1h 02m`.
pub(crate) fn duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 10 {
        format!("{:.1}s", d.as_secs_f64())
    } else if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn durations_read_like_a_stopwatch() {
        assert_eq!(duration(Duration::from_millis(400)), "0.4s");
        assert_eq!(duration(Duration::from_millis(3800)), "3.8s");
        assert_eq!(duration(Duration::from_secs(12)), "12s");
        assert_eq!(duration(Duration::from_secs(72)), "1m 12s");
        assert_eq!(duration(Duration::from_secs(3720)), "1h 02m");
    }
}
