//! Private to one open chat. Entries are editor snapshots, not transcript messages:
//! a stashed draft retains its cursor and explicit skill selections.
use super::input::InputEditor;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use std::collections::VecDeque;

#[derive(Default)]
struct ReverseSearch {
    query: String,
    found: Option<usize>,
}

#[derive(Default)]
pub(super) struct InputHistory {
    search: Option<ReverseSearch>,
    entries: VecDeque<InputEditor>,         // oldest first
    browsing: Option<(usize, InputEditor)>, // index and unsent draft to return to
}

impl InputHistory {
    pub(super) const MAX_BYTES: usize = 64 * 1024;

    pub(super) fn remember(&mut self, entry: InputEditor) {
        if entry.contents().len() <= Self::MAX_BYTES {
            if self.entries.len() == 64 {
                self.entries.pop_front();
            }
            self.entries.push_back(entry);
        }
    }

    /// Take the selected input for submission or stashing, leaving the draft
    /// the operator was composing before recall (or an empty editor).
    pub(super) fn take(&mut self, input: &mut InputEditor) -> InputEditor {
        let draft = self
            .browsing
            .take()
            .map_or_else(InputEditor::new, |(_, draft)| draft);
        std::mem::replace(input, draft)
    }

    pub(super) fn searching(&self) -> bool {
        self.search.is_some()
    }

    pub(super) fn start_search(&mut self) {
        self.search = Some(ReverseSearch {
            query: String::new(),
            found: self.entries.len().checked_sub(1),
        });
    }

    /// The editor itself is untouched until Enter accepts a match. Paste is
    /// query text only, never a key sequence or an implicit submit.
    pub(super) fn search_event(&mut self, event: Event, input: &mut InputEditor) -> bool {
        let Some(search) = self.search.as_mut() else {
            return false;
        };
        let mut end = self.entries.len();
        let mut older = false;
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match (key.code, key.modifiers) {
                (KeyCode::Esc, _) | (KeyCode::Char('c' | 'g'), KeyModifiers::CONTROL) => {
                    self.search = None;
                    return false;
                }
                (KeyCode::Enter, KeyModifiers::NONE) => {
                    if let Some(index) = search.found {
                        self.select(input, index);
                        self.search = None;
                        return true;
                    }
                    return false;
                }
                (KeyCode::Char('r'), KeyModifiers::CONTROL) => {
                    end = search.found.unwrap_or(0);
                    older = true;
                }
                (KeyCode::Backspace, _) => {
                    search.query.pop();
                }
                (KeyCode::Char(ch), mods)
                    if !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && !ch.is_control()
                        && search.query.len() + ch.len_utf8() <= 1024 =>
                {
                    search.query.push(ch)
                }
                _ => return false,
            },
            Event::Paste(text) => {
                for ch in text.chars().filter(|c| !c.is_control()) {
                    if search.query.len() + ch.len_utf8() > 1024 {
                        break;
                    }
                    search.query.push(ch);
                }
            }
            _ => return false,
        }
        let found = (0..end)
            .rev()
            .find(|&i| self.entries[i].contents().contains(&search.query));
        if !older || found.is_some() {
            search.found = found;
        }
        false
    }

    pub(super) fn search_preview(&self) -> Option<InputEditor> {
        let search = self.search.as_ref()?;
        let preview = search.found.map_or_else(
            || "[no match]".into(),
            |i| {
                let text = self.entries[i].contents();
                // Show the hit even late in a multiline prompt, rather than
                // presenting identical first lines for different matches.
                let hit = text.find(&search.query).unwrap_or(0);
                let start = text[..hit]
                    .char_indices()
                    .rev()
                    .nth(23)
                    .map_or(0, |(offset, _)| offset);
                let mut chars = text[start..].chars();
                let mut line = if start == 0 {
                    String::new()
                } else {
                    "…".into()
                };
                line.extend(
                    chars
                        .by_ref()
                        .take(120)
                        .map(|c| if c.is_control() { '↵' } else { c }),
                );
                if chars.next().is_some() {
                    line.push('…');
                }
                line
            },
        );
        Some(InputEditor::from_text(&format!(
            "{preview}\nreverse-i-search (Ctrl+R older · Enter edit · Esc cancel): {}",
            search.query
        )))
    }

    fn select(&mut self, input: &mut InputEditor, next: usize) {
        let previous = std::mem::replace(input, self.entries[next].clone());
        if let Some((index, _)) = &mut self.browsing {
            *index = next;
        } else {
            self.browsing = Some((next, previous));
        }
    }

    pub(super) fn navigate(&mut self, input: &mut InputEditor, older: bool) {
        let len = self.entries.len();
        let index = self.browsing.as_ref().map_or(len, |(index, _)| *index);
        if older && index > 0 {
            self.select(input, index - 1);
        } else if !older && index < len {
            if index + 1 == len {
                self.take(input);
            } else {
                self.select(input, index + 1);
            }
        }
    }
}
