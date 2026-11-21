//! Floating transcript jump controls and their mouse hit targets.

use ahi_widgets::Paragraph;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui_core::layout::{Position, Rect};
use ratatui_core::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthStr;

use super::key_binding::KeyAction;

#[derive(Default)]
pub(super) struct ScrollNavigation {
    top: Option<Rect>,
    bottom: Option<Rect>,
    pointer: Option<Position>,
}

impl ScrollNavigation {
    pub(super) fn mouse(&mut self, event: MouseEvent) -> Option<KeyAction> {
        let position = Position::new(event.column, event.row);
        self.pointer = Some(position);
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return None;
        }
        if self.top.is_some_and(|area| area.contains(position)) {
            Some(KeyAction::JumpToTop)
        } else if self.bottom.is_some_and(|area| area.contains(position)) {
            Some(KeyAction::JumpToBottom)
        } else {
            None
        }
    }

    pub(super) fn draw(
        &mut self,
        frame: &mut ratatui_core::terminal::Frame,
        area: Rect,
        can_go_up: bool,
        can_go_down: bool,
    ) {
        self.top = None;
        self.bottom = None;
        if area.height == 0 || area.width < 8 {
            return;
        }
        for (show, top, label, compact) in [
            (
                can_go_up && (area.height > 1 || !can_go_down),
                true,
                " ↑ Top (Ctrl+K) ",
                " ↑ Top ",
            ),
            (can_go_down, false, " ↓ Bottom (Ctrl+L) ", " ↓ End "),
        ] {
            if !show {
                continue;
            }
            // These controls share a row with the transcript. Use only its
            // blank suffix, including both cells of a wide final grapheme.
            // The keyboard shortcuts still work when neither label fits.
            let row = if top { area.y } else { area.bottom() - 1 };
            let buffer = frame.buffer_mut();
            let used_end = (area.x..area.right())
                .rev()
                .find_map(|x| {
                    let symbol = buffer[(x, row)].symbol();
                    (symbol != " ").then(|| x.saturating_add(symbol.width() as u16))
                })
                .unwrap_or(area.x);
            let free = area.right().saturating_sub(used_end) as usize;
            let Some(text) = [label, compact].into_iter().find(|s| s.width() <= free) else {
                continue;
            };
            let width = text.width() as u16;
            let button = Rect::new(area.right() - width, row, width, 1);
            let hovered = self
                .pointer
                .is_some_and(|position| button.contains(position));
            let style = if hovered {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 40, 50))
            };
            frame.render_widget(Paragraph::new(text).style(style), button);
            if top {
                self.top = Some(button);
            } else {
                self.bottom = Some(button);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui_core::{backend::TestBackend, terminal::Terminal};

    #[test]
    fn jump_buttons_never_erase_text_when_the_corner_is_occupied() {
        let mut terminal = Terminal::new(TestBackend::new(20, 2)).unwrap();
        let mut nav = ScrollNavigation::default();
        terminal
            .draw(|frame| {
                let area = frame.area();
                frame.render_widget(Paragraph::new("123456789012345678界\n1234567890"), area);
                nav.draw(frame, area, true, true);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(18, 0)].symbol(), "界");
        assert_eq!(buffer[(9, 1)].symbol(), "0");
        assert_eq!(buffer[(16, 1)].symbol(), "E"); // Compact bottom button fits.
        assert!(nav.top.is_none());
    }
}
