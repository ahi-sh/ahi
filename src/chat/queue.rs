//! Session-local pending prompt UI. Opening the manager suspends dispatch.
use super::*;
use ahi_widgets::{Block, Borders, Clear};
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

impl ChatScreen {
    pub(super) fn queue_event(&mut self, event: Event) {
        let Some(view) = self.queue_view.as_mut() else {
            return;
        };
        if let Some((id, editor)) = view.editing.as_mut() {
            match event {
                Event::Paste(text) => editor.insert_paste(&text),
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match key_binding::translate(key) {
                        KeyAction::Cancel => view.editing = None,
                        KeyAction::Submit => {
                            let text = editor.contents();
                            if !text.trim().is_empty()
                                && !text.trim_start().starts_with('/')
                                && text.len() <= 64 * 1024
                            {
                                if let Some(item) =
                                    self.prompt_queue.iter_mut().find(|item| item.id == *id)
                                {
                                    item.text = text;
                                }
                                view.editing = None;
                            }
                        }
                        KeyAction::Edit(action) => {
                            editor.apply(action, self.transcript_width.max(1))
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            return;
        }
        let Event::Key(key) = event else { return };
        if key.kind != KeyEventKind::Press {
            return;
        }
        if view.confirm_clear {
            if key.code == KeyCode::Char('y') {
                self.prompt_queue.clear();
                view.selected = 0;
            }
            view.confirm_clear = false;
            return;
        }
        match key.code {
            KeyCode::Esc => self.queue_view = None,
            KeyCode::Up => view.selected = view.selected.saturating_sub(1),
            KeyCode::Down => {
                view.selected = (view.selected + 1).min(self.prompt_queue.len().saturating_sub(1))
            }
            KeyCode::Delete => {
                self.prompt_queue.remove(view.selected);
                view.selected = view.selected.min(self.prompt_queue.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                if let Some(item) = self.prompt_queue.get(view.selected) {
                    let editor = if key.modifiers.contains(KeyModifiers::ALT) {
                        InputEditor::new()
                    } else {
                        InputEditor::from_text(&item.text)
                    };
                    view.editing = Some((item.id, editor));
                }
            }
            KeyCode::Char('c') => view.confirm_clear = true,
            KeyCode::Char('p') => self.prompt_queue_held = true,
            KeyCode::Char('r') => {
                self.prompt_queue_held = false;
                self.resend_available = false;
                self.queue_view = None;
            }
            _ => {}
        }
    }

    pub(super) fn draw_pending_queue(&self, frame: &mut ratatui_core::terminal::Frame, area: Rect) {
        if self.prompt_queue.is_empty() {
            return;
        }
        let mut rows = vec![format!(
            "Queue {} · {} pending · Alt+Q manage · session-local",
            if self.prompt_queue_held {
                "paused"
            } else {
                "ready"
            },
            self.prompt_queue.len()
        )];
        rows.extend(
            self.prompt_queue
                .iter()
                .take(3)
                .enumerate()
                .map(|(i, item)| {
                    format!(
                        "  {}  {}",
                        i + 1,
                        item.text.lines().next().unwrap_or_default()
                    )
                }),
        );
        if self.prompt_queue.len() > 3 {
            rows.push(format!("  … {} more", self.prompt_queue.len() - 3));
        }
        frame.render_widget(Paragraph::new(rows.join("\n")), area);
    }

    pub(super) fn draw_queue_manager(
        &self,
        frame: &mut ratatui_core::terminal::Frame,
        bottom: u16,
    ) {
        let Some(view) = &self.queue_view else { return };
        let screen = frame.area();
        let available = bottom.saturating_sub(screen.y);
        let inset = u16::from(screen.width >= 40);
        let width = screen.width.saturating_sub(inset * 2);
        let desired = if let Some((_, editor)) = &view.editing {
            editor
                .viewport(width.saturating_sub(2) as usize, 8)
                .lines
                .len()
                + 4
        } else {
            self.prompt_queue.len() + 5 + usize::from(view.confirm_clear)
        };
        let height = (desired.min(u16::MAX as usize) as u16)
            .min((screen.height / 2).max(1))
            .min(available);
        let popup = Rect::new(
            screen.x + inset,
            bottom.saturating_sub(height),
            width,
            height,
        );
        if popup.width < 3 || popup.height < 3 {
            return;
        }
        let block = Block::default().borders(Borders::ALL);
        let area = block.inner(popup);
        frame.render_widget(Clear, popup);
        frame.render_widget(block, popup);
        if let Some((_, editor)) = &view.editing {
            let input =
                editor.viewport(area.width as usize, area.height.saturating_sub(2) as usize);
            frame.render_widget(Paragraph::new(format!("Edit queued prompt · Enter save · Esc discard\nSkills retained · nonblank plain text, Shift+Enter newline · max 64 KiB\n{}", input.lines.join("\n"))), area);
            if area.height > 2 && area.width > 0 {
                frame.set_cursor_position((
                    area.x + (input.cursor_column as u16).min(area.width - 1),
                    area.y + 2 + (input.cursor_row as u16).min(area.height - 3),
                ));
            }
            return;
        }
        let mut rows = vec![
            format!(
                "Queue {} · {} pending · dispatch suspended while open",
                if self.prompt_queue_held {
                    "paused"
                } else {
                    "ready"
                },
                self.prompt_queue.len()
            ),
            "↑/↓ select · Enter edit · Alt/Option+Enter replace · Delete remove · c clear all".into(),
            "p Pause queue · r Resume queue · Esc close · not saved on exit".into(),
        ];
        if view.confirm_clear {
            rows.push("Remove all pending prompts? y confirms; any other key cancels".into());
        }
        let count = area.height.saturating_sub(rows.len() as u16) as usize;
        let start = view.selected.saturating_sub(count.saturating_sub(1));
        rows.extend(
            self.prompt_queue
                .iter()
                .enumerate()
                .skip(start)
                .take(count)
                .map(|(i, item)| {
                    format!(
                        "{} {}  {}",
                        if i == view.selected { "›" } else { " " },
                        i + 1,
                        item.text.lines().next().unwrap_or_default()
                    )
                }),
        );
        frame.render_widget(Paragraph::new(rows.join("\n")), area);
        // Keep keyboard focus visibly in the overlay, not on the inactive draft.
        frame.set_cursor_position((area.x, area.y));
    }
}
