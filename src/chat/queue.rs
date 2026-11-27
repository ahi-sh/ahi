//! Session-local pending prompt UI. Opening the manager suspends dispatch.
use super::*;
use ahi_widgets::{Block, Borders, Clear};
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

const QUEUE_HELP: &[&str] = &[
    "p: Ready → Paused; turn continues.",
    "r: Paused → Ready; closes panel.",
    "Esc: close; keep Ready/Paused.",
    "Next sends after turn completes.",
    "Close panel to allow next send.",
    "Failure/cancel → Paused.",
    "↑/↓ select · Home/End first/last",
    "PgUp/PgDn move one visible page",
    "Alt+↑/↓ move prompt earlier/later",
    "Enter: edit the saved text.",
    "Option/Alt+Enter: empty editor.",
    "Enter saves; Esc keeps saved text.",
    "Delete remove · c then y clear all",
    "Esc close · ? show/hide help",
    "Edits keep the prompt's skills.",
    "Tool calls finish before next send.",
    "If idle, closing sends the next.",
    "Pending prompts are lost on exit.",
];

impl ChatScreen {
    fn queue_dispatch_hint(&self) -> &'static str {
        if self.prompt_queue.is_empty() {
            "No pending prompts to send."
        } else if self.prompt_queue_held || self.resend_available {
            if self.queue_view.is_some() {
                if self.response_in_progress {
                    "Paused: active turn continues."
                } else {
                    "Paused: pending prompts will wait."
                }
            } else {
                "Paused: Alt+Q, then r to resume."
            }
        } else if self.response_in_progress {
            if self.queue_view.is_some() {
                "Next: turn ends and panel closes."
            } else {
                "Next: after current turn completes."
            }
        } else if self.modes.dialog.is_some() {
            "Next: after mode selection closes."
        } else if self.queue_view.is_some() {
            "Next: when this panel closes."
        } else {
            "Next: when chat is idle."
        }
    }

    pub(super) fn queue_event(&mut self, event: Event) {
        let Some(view) = self.queue_view.as_mut() else {
            return;
        };
        if let Some((id, editor)) = view.editing.as_mut() {
            match event {
                Event::Paste(text) => {
                    editor.insert_paste(&text);
                    view.edit_error = None;
                }
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match key_binding::translate(key) {
                        KeyAction::Cancel => {
                            view.editing = None;
                            view.edit_error = None;
                        }
                        KeyAction::Submit => {
                            let text = editor.contents();
                            view.edit_error = if text.trim().is_empty() {
                                Some("Prompt must contain text.")
                            } else if text.trim_start().starts_with('/') {
                                Some("Slash commands cannot be queued.")
                            } else if text.len() > 64 * 1024 {
                                Some("Prompt exceeds 64 KiB; shorten it.")
                            } else {
                                None
                            };
                            if view.edit_error.is_none() {
                                if let Some(item) =
                                    self.prompt_queue.iter_mut().find(|item| item.id == *id)
                                {
                                    item.text = text;
                                }
                                view.editing = None;
                            }
                        }
                        KeyAction::Edit(action) => {
                            editor.apply(action, view.editor_width.max(1));
                            view.edit_error = None;
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
        let plain = !key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL);
        if view.confirm_clear {
            if plain && key.code == KeyCode::Char('y') {
                self.prompt_queue.clear();
                view.selected = 0;
            }
            view.confirm_clear = false;
            return;
        }
        if plain && key.code == KeyCode::Char('p') {
            self.prompt_queue_held = true;
            return;
        }
        if plain && key.code == KeyCode::Char('r') {
            self.prompt_queue_held = false;
            self.resend_available = false;
            self.queue_view = None;
            return;
        }
        if view.help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') if plain => view.help = false,
                KeyCode::Up if plain => view.help_offset = view.help_offset.saturating_sub(1),
                KeyCode::Down if plain => view.help_offset += 1,
                KeyCode::PageUp if plain => {
                    view.help_offset = view.help_offset.saturating_sub(view.visible_rows)
                }
                KeyCode::PageDown if plain => view.help_offset += view.visible_rows,
                KeyCode::Home if plain => view.help_offset = 0,
                KeyCode::End if plain => view.help_offset = QUEUE_HELP.len(),
                _ => {}
            }
            view.help_offset = view
                .help_offset
                .min(QUEUE_HELP.len().saturating_sub(view.visible_rows));
            return;
        }
        let last = self.prompt_queue.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc if plain => self.queue_view = None,
            KeyCode::Char('q') if key.modifiers == KeyModifiers::ALT => self.queue_view = None,
            KeyCode::Up | KeyCode::Down if key.modifiers == KeyModifiers::ALT => {
                let target = if key.code == KeyCode::Up {
                    view.selected.saturating_sub(1)
                } else {
                    (view.selected + 1).min(last)
                };
                if target != view.selected && view.selected < self.prompt_queue.len() {
                    self.prompt_queue.swap(view.selected, target);
                    view.selected = target;
                }
            }
            KeyCode::Up if plain => view.selected = view.selected.saturating_sub(1),
            KeyCode::Down if plain => view.selected = (view.selected + 1).min(last),
            KeyCode::Home if plain => view.selected = 0,
            KeyCode::End if plain => view.selected = last,
            KeyCode::PageUp if plain => {
                view.selected = view.selected.saturating_sub(view.visible_rows)
            }
            KeyCode::PageDown if plain => {
                view.selected = (view.selected + view.visible_rows).min(last)
            }
            KeyCode::Delete if plain => {
                self.prompt_queue.remove(view.selected);
                view.selected = view.selected.min(self.prompt_queue.len().saturating_sub(1));
            }
            KeyCode::Enter if plain || key.modifiers == KeyModifiers::ALT => {
                if let Some(item) = self.prompt_queue.get(view.selected) {
                    let editor = if key.modifiers.contains(KeyModifiers::ALT) {
                        InputEditor::new()
                    } else {
                        InputEditor::from_text(&item.text)
                    };
                    view.editing = Some((item.id, editor));
                    view.edit_error = None;
                }
            }
            KeyCode::Char('?') if plain => {
                view.help = true;
                view.help_offset = 0;
            }
            KeyCode::Char('c') if plain && !self.prompt_queue.is_empty() => {
                view.confirm_clear = true
            }
            _ => {}
        }
    }

    pub(super) fn draw_pending_queue(&self, frame: &mut ratatui_core::terminal::Frame, area: Rect) {
        if self.prompt_queue.is_empty() {
            return;
        }
        let mut rows = vec![
            format!(
                "Queue {} · {} pending · Alt+Q manage",
                if self.prompt_queue_held {
                    "paused"
                } else {
                    "ready"
                },
                self.prompt_queue.len()
            ),
            self.queue_dispatch_hint().into(),
        ];
        rows.extend(
            self.prompt_queue
                .iter()
                .take(3)
                .enumerate()
                .map(|(index, item)| format!("  {}  {}", index + 1, prompt_preview(&item.text))),
        );
        if self.prompt_queue.len() > 3 {
            rows.push(format!("  … {} more", self.prompt_queue.len() - 3));
        }
        frame.render_widget(Paragraph::new(rows.join("\n")), area);
    }

    pub(super) fn draw_queue_manager(
        &mut self,
        frame: &mut ratatui_core::terminal::Frame,
        bottom: u16,
    ) {
        let dispatch_hint = self.queue_dispatch_hint();
        let Some(view) = &mut self.queue_view else {
            return;
        };
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
        } else if view.help {
            QUEUE_HELP.len() + 3
        } else {
            self.prompt_queue.len().max(1) + 4
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
        let title = if view.help {
            format!(
                " Queue controls · {} ",
                if self.prompt_queue_held {
                    "paused"
                } else {
                    "ready"
                }
            )
        } else if view.editing.is_some() {
            " Queue editor ".to_owned()
        } else {
            format!(
                " Queue {} · {} pending · {}/{} ",
                if self.prompt_queue_held {
                    "paused"
                } else {
                    "ready"
                },
                self.prompt_queue.len(),
                if self.prompt_queue.is_empty() {
                    0
                } else {
                    view.selected + 1
                },
                self.prompt_queue.len()
            )
        };
        let mut block = Block::default().borders(Borders::ALL).title(title);
        if view.editing.is_none() && !view.confirm_clear {
            let controls = if popup.width >= 42 {
                " p Pause · r Resume + close · Esc Close "
            } else {
                "p Pause · r Resume + close · Esc"
            };
            block = block.title_bottom(Line::styled(
                controls,
                Style::default().add_modifier(Modifier::BOLD),
            ));
        }
        let area = block.inner(popup);
        frame.render_widget(
            Clear,
            Rect::new(screen.x, popup.y, screen.width, popup.height),
        );
        frame.render_widget(block, popup);
        view.editor_width = area.width as usize;
        if let Some((_, editor)) = &view.editing {
            let header_rows = usize::from(area.height > 1) + usize::from(area.height > 2);
            let input = editor.viewport(area.width as usize, area.height as usize - header_rows);
            let mut rows = Vec::new();
            if header_rows > 0 {
                rows.push(Line::from(format!(
                    "Edit queued prompt · {} bytes",
                    editor.contents().len()
                )));
            }
            if header_rows > 1 {
                rows.push(match view.edit_error {
                    Some(error) => Line::styled(error, Style::default().fg(Color::Red)),
                    None => Line::from(if area.width >= 40 {
                        "Enter save · Esc discard · skills kept"
                    } else {
                        "Enter save · Esc discard"
                    }),
                });
            }
            rows.extend(input.lines.into_iter().map(Line::from));
            frame.render_widget(Paragraph::new(Text::from(rows)), area);
            frame.set_cursor_position((
                area.x + (input.cursor_column as u16).min(area.width - 1),
                area.y + header_rows as u16 + input.cursor_row as u16,
            ));
            return;
        }
        if view.confirm_clear && area.height == 1 {
            frame.render_widget(Paragraph::new("y clear all · other key cancel"), area);
            frame.set_cursor_position((area.x, area.y));
            return;
        }
        let footer_rows = usize::from(area.height > 1);
        let status_rows = usize::from(!view.help && area.height > 2 && !view.confirm_clear);
        view.visible_rows = (area.height as usize - footer_rows - status_rows).max(1);
        let mut rows = Vec::new();
        if status_rows > 0 {
            rows.push(Line::from(dispatch_hint));
        }
        if view.help {
            view.help_offset = view
                .help_offset
                .min(QUEUE_HELP.len().saturating_sub(view.visible_rows));
            rows.extend(
                QUEUE_HELP
                    .iter()
                    .skip(view.help_offset)
                    .take(view.visible_rows)
                    .map(|text| Line::from(*text)),
            );
        } else if self.prompt_queue.is_empty() {
            rows.push(Line::from("No pending prompts"));
        } else {
            let start = view.selected.saturating_sub(view.visible_rows - 1);
            rows.extend(
                self.prompt_queue
                    .iter()
                    .enumerate()
                    .skip(start)
                    .take(view.visible_rows)
                    .map(|(index, item)| {
                        let selected = index == view.selected;
                        let text = format!(
                            "{} {}  {}",
                            if selected { "›" } else { " " },
                            index + 1,
                            prompt_preview(&item.text)
                        );
                        if selected {
                            Line::styled(text, Style::default().add_modifier(Modifier::REVERSED))
                        } else {
                            Line::from(text)
                        }
                    }),
            );
        }
        frame.render_widget(Paragraph::new(Text::from(rows)), area);
        if footer_rows > 0 {
            let footer = if view.confirm_clear {
                "Clear all? y confirm · any key cancel"
            } else if view.help {
                "↑/↓ scroll · ?/Esc back"
            } else if area.width >= 64 {
                "Enter edit · Option/Alt+Enter empty editor · ? help · Esc close"
            } else {
                "Option/Alt+Enter empty · ? help"
            };
            frame.render_widget(
                Paragraph::new(footer),
                Rect::new(area.x, area.bottom() - 1, area.width, 1),
            );
        }
        frame.set_cursor_position((area.x, area.y));
    }
}

fn prompt_preview(text: &str) -> &str {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim()
}
