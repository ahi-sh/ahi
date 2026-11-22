//! Line editor for chat input and gateway URL entry.
//!
//! `key_binding` translates a terminal key into `KeyAction::Edit`. This module
//! does not read the terminal.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const ROW_LIMIT: usize = 8;

/// Soft-wrapped display only; the editor retains its original logical lines.
pub(crate) struct InputViewport {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_column: usize,
}

struct WrappedRow {
    text: String,
    line: usize,
    column: usize,
}

struct InputLayout {
    rows: Vec<WrappedRow>,
    cursor_row: usize,
    cursor_column: usize,
}

/// A key the line editor understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorKey {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Backspace,
    Delete,
    Character(char),
    Newline,
}

#[derive(Clone)]
pub(crate) struct InputEditor {
    lines: Vec<String>,
    line: usize,
    column: usize,
    skills: Vec<(std::ops::Range<usize>, String)>,
}

impl InputEditor {
    pub(crate) fn new() -> Self {
        Self {
            lines: vec![String::new()],
            line: 0,
            column: 0,
            skills: Vec::new(),
        }
    }

    pub(crate) fn from_text(text: &str) -> Self {
        let lines: Vec<String> = if text.is_empty() {
            vec![String::new()]
        } else {
            text.split('\n').map(str::to_string).collect()
        };
        let line = lines.len() - 1;
        let column = lines[line].chars().count();
        Self {
            lines,
            line,
            column,
            skills: Vec::new(),
        }
    }

    pub(crate) fn apply(&mut self, key: EditorKey, width: usize) {
        match key {
            EditorKey::Left => self.move_left(),
            EditorKey::Right => self.move_right(),
            EditorKey::Up | EditorKey::Down => {
                self.move_vertical(width, if key == EditorKey::Up { -1 } else { 1 });
            }
            EditorKey::Home => self.move_home(),
            EditorKey::End => self.move_end(),
            EditorKey::Backspace => self.backspace(),
            EditorKey::Delete => self.delete(),
            EditorKey::Character(character) => self.insert_char(character),
            EditorKey::Newline => self.insert_newline(),
        }
    }

    /// Insert clipboard text as edits, never as submit keys.
    pub(crate) fn insert_paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let byte = byte_index(&self.lines[self.line], self.column);
        if text.is_empty() {
            return;
        }
        self.track_line_edit(byte, 0, text.len());
        let right = self.lines[self.line].split_off(byte);
        let mut parts = text.split('\n');
        let first = parts.next().unwrap_or_default();
        self.lines[self.line].push_str(first);
        self.column += first.chars().count();
        let mut added: Vec<String> = parts.map(str::to_owned).collect();
        if let Some(last) = added.last_mut() {
            self.column = last.chars().count();
            last.push_str(&right);
            let count = added.len();
            self.lines.splice(self.line + 1..self.line + 1, added);
            self.line += count;
        } else {
            self.lines[self.line].push_str(&right);
        }
    }

    /// The `$` token under the cursor, independent of its position in the draft.
    fn skill_range(&self) -> Option<std::ops::Range<usize>> {
        let text = &self.lines[self.line];
        let cursor = byte_index(text, self.column);
        let start = text[..cursor].rfind('$')?;
        if text[..start]
            .bytes()
            .rev()
            .take_while(|b| *b == b'\\')
            .count()
            % 2
            == 1
        {
            return None;
        }
        if !text[start + 1..cursor]
            .chars()
            .all(crate::skills::query_char)
        {
            return None;
        }
        Some(start..cursor)
    }

    pub(crate) fn skill_query(&self) -> Option<String> {
        let range = self.skill_range()?;
        Some(self.lines[self.line][range.start + 1..range.end].to_string())
    }

    pub(crate) fn complete_skill(&mut self, name: &str) {
        let Some(range) = self.skill_range() else {
            return;
        };
        let prefix_columns = self.lines[self.line][..range.start].chars().count();
        let start = self.line_offset() + range.start;
        let len = name.len() + 1;
        self.track_edit(start, range.len(), len);
        self.lines[self.line].replace_range(range, &format!("${name}"));
        self.column = prefix_columns + name.chars().count() + 1;
        self.skills.push((start..start + len, name.to_owned()));
        self.skills.sort_by_key(|(span, _)| span.start);
    }

    pub(crate) fn selected_skills(&self) -> Vec<String> {
        self.skills.iter().map(|(_, id)| id.clone()).collect()
    }

    fn line_offset(&self) -> usize {
        self.lines[..self.line]
            .iter()
            .map(|line| line.len() + 1)
            .sum()
    }

    fn track_line_edit(&mut self, byte: usize, removed: usize, inserted: usize) {
        if !self.skills.is_empty() {
            self.track_edit(self.line_offset() + byte, removed, inserted);
        }
    }

    // Edits within a reference remove selection intent, even if the operator
    // later restores the text. Edits beside it move the intact reference.
    fn track_edit(&mut self, start: usize, removed: usize, inserted: usize) {
        self.skills.retain_mut(|(span, _)| {
            if span.end <= start {
                true
            } else if span.start >= start + removed {
                span.start = span.start - removed + inserted;
                span.end = span.end - removed + inserted;
                true
            } else {
                false
            }
        });
    }

    pub(crate) fn contents(&self) -> String {
        self.lines.join("\n")
    }

    pub(crate) fn is_blank(&self) -> bool {
        self.contents().trim().is_empty()
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::new();
    }

    pub(crate) fn viewport(&self, width: usize, height: usize) -> InputViewport {
        if width == 0 || height == 0 {
            return InputViewport {
                lines: Vec::new(),
                cursor_row: 0,
                cursor_column: 0,
            };
        }
        let layout = self.layout(width);
        let count = layout.rows.len().min(height).min(ROW_LIMIT);
        let start = layout
            .cursor_row
            .saturating_sub(count - 1)
            .min(layout.rows.len() - count);
        InputViewport {
            lines: layout.rows[start..start + count]
                .iter()
                .map(|row| row.text.clone())
                .collect(),
            cursor_row: layout.cursor_row - start,
            cursor_column: layout.cursor_column,
        }
    }

    fn layout(&self, width: usize) -> InputLayout {
        let width = width.max(1);
        let mut layout = InputLayout {
            rows: Vec::new(),
            cursor_row: 0,
            cursor_column: 0,
        };
        for (line, text) in self.lines.iter().enumerate() {
            let mut row = WrappedRow {
                text: String::new(),
                line,
                column: 0,
            };
            let mut cells = 0;
            let mut column = 0;
            let cursor_byte = byte_index(text, self.column);
            for (byte, grapheme) in text.grapheme_indices(true) {
                let grapheme_width = grapheme.width();
                if cells > 0 && cells + grapheme_width > width {
                    layout.rows.push(row);
                    row = WrappedRow {
                        text: String::new(),
                        line,
                        column,
                    };
                    cells = 0;
                }
                if line == self.line && (byte..byte + grapheme.len()).contains(&cursor_byte) {
                    layout.cursor_row = layout.rows.len();
                    layout.cursor_column = cells;
                }
                row.text.push_str(grapheme);
                cells += grapheme_width;
                column += grapheme.chars().count();
            }
            if line == self.line && cursor_byte == text.len() {
                // An insertion cursor after a completely full row needs its
                // own visual row; it must never be clamped over the last glyph.
                if cells >= width {
                    layout.rows.push(row);
                    row = WrappedRow {
                        text: String::new(),
                        line,
                        column,
                    };
                    cells = 0;
                }
                layout.cursor_row = layout.rows.len();
                layout.cursor_column = cells;
            }
            layout.rows.push(row);
        }
        layout
    }

    fn insert_char(&mut self, character: char) {
        let column = self.column.min(self.lines[self.line].chars().count());
        let byte = byte_index(&self.lines[self.line], column);
        self.track_line_edit(byte, 0, character.len_utf8());
        self.lines[self.line].insert(byte, character);
        self.column = column + 1;
    }

    fn insert_newline(&mut self) {
        let column = self.column.min(self.lines[self.line].chars().count());
        let byte = byte_index(&self.lines[self.line], column);
        self.track_line_edit(byte, 0, 1);
        let right = self.lines[self.line].split_off(byte);
        self.lines.insert(self.line + 1, right);
        self.line += 1;
        self.column = 0;
    }

    fn backspace(&mut self) {
        if self.column > 0 {
            let end = byte_index(&self.lines[self.line], self.column);
            let start = byte_index(&self.lines[self.line], self.column - 1);
            self.track_line_edit(start, end - start, 0);
            self.lines[self.line].replace_range(start..end, "");
            self.column -= 1;
            return;
        }
        if self.line == 0 {
            return;
        }
        if !self.skills.is_empty() {
            self.track_edit(self.line_offset() - 1, 1, 0);
        }
        let current = self.lines.remove(self.line);
        self.line -= 1;
        self.column = self.lines[self.line].chars().count();
        self.lines[self.line].push_str(&current);
    }

    fn delete(&mut self) {
        let length = self.lines[self.line].chars().count();
        if self.column < length {
            let start = byte_index(&self.lines[self.line], self.column);
            let end = byte_index(&self.lines[self.line], self.column + 1);
            self.track_line_edit(start, end - start, 0);
            self.lines[self.line].replace_range(start..end, "");
            return;
        }
        if self.line + 1 < self.lines.len() {
            self.track_line_edit(self.lines[self.line].len(), 1, 0);
            let next = self.lines.remove(self.line + 1);
            self.lines[self.line].push_str(&next);
        }
    }

    fn move_left(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        } else if self.line > 0 {
            self.line -= 1;
            self.column = self.lines[self.line].chars().count();
        }
    }

    fn move_right(&mut self) {
        let length = self.lines[self.line].chars().count();
        if self.column < length {
            self.column += 1;
        } else if self.line + 1 < self.lines.len() {
            self.line += 1;
            self.column = 0;
        }
    }

    /// False means the cursor was already on the requested visual edge.
    pub(super) fn move_vertical(&mut self, width: usize, delta: isize) -> bool {
        let layout = self.layout(width);
        let target = layout
            .cursor_row
            .saturating_add_signed(delta)
            .min(layout.rows.len() - 1);
        if target == layout.cursor_row {
            return false;
        }
        let row = &layout.rows[target];
        let mut cells = 0;
        let mut column = row.column;
        for grapheme in row.text.graphemes(true) {
            if cells + grapheme.width() > layout.cursor_column {
                break;
            }
            cells += grapheme.width();
            column += grapheme.chars().count();
        }
        self.line = row.line;
        self.column = column;
        true
    }

    fn move_home(&mut self) {
        self.column = 0;
    }

    fn move_end(&mut self) {
        self.column = self.lines[self.line].chars().count();
    }
}

fn byte_index(text: &str, column: usize) -> usize {
    text.chars().take(column).map(char::len_utf8).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_grows_to_eight_rows_and_no_further() {
        let mut editor = InputEditor::from_text("a");
        assert_eq!(editor.viewport(20, 20).lines, ["a"]);
        for _ in 0..7 {
            editor.apply(EditorKey::Newline, 20);
        }
        assert_eq!(editor.viewport(20, 20).lines.len(), 8);
        editor.apply(EditorKey::Newline, 20);
        assert_eq!(editor.contents(), "a\n\n\n\n\n\n\n\n");
        assert_eq!(editor.viewport(20, 20).lines.len(), 8);
        assert_eq!(editor.viewport(20, 20).cursor_row, 7);
    }

    #[test]
    fn soft_wrap_and_resize_preserve_draft_and_cursor() {
        let mut editor = InputEditor::from_text("abc");
        let view = editor.viewport(3, 8);
        assert_eq!(view.lines, ["abc", ""]);
        assert_eq!((view.cursor_row, view.cursor_column), (1, 0));
        editor.apply(EditorKey::Character('d'), 3);
        assert_eq!(editor.viewport(3, 8).lines, ["abc", "d"]);
        let view = editor.viewport(6, 8);
        assert_eq!(view.lines, ["abcd"]);
        assert_eq!((view.cursor_row, view.cursor_column), (0, 4));
        assert_eq!(editor.contents(), "abcd");
        assert!(editor.viewport(0, 8).lines.is_empty());
        assert!(editor.viewport(6, 0).lines.is_empty());
    }

    #[test]
    fn unicode_cell_widths_and_visual_up_down_preserve_logical_text() {
        let mut editor = InputEditor::from_text("a界e\u{301}🙂b");
        let view = editor.viewport(4, 8);
        assert_eq!(view.lines, ["a界e\u{301}", "🙂b"]);
        assert_eq!((view.cursor_row, view.cursor_column), (1, 3));
        editor.apply(EditorKey::Up, 4);
        let view = editor.viewport(4, 8);
        assert_eq!((view.cursor_row, view.cursor_column), (0, 3));
        editor.apply(EditorKey::Down, 4);
        editor.apply(EditorKey::Character('!'), 4);
        assert_eq!(editor.contents(), "a界e\u{301}🙂b!");
        editor.apply(EditorKey::Backspace, 4);
        editor.apply(EditorKey::Up, 4);
        editor.apply(EditorKey::Character('X'), 4);
        assert_eq!(editor.contents(), "a界Xe\u{301}🙂b");
    }

    #[test]
    fn long_draft_scrolls_to_cursor_within_row_and_terminal_limits() {
        let mut editor = InputEditor::from_text("abcdefghijklmnopqrstuvwxyz0123456789");
        let view = editor.viewport(4, 20);
        assert_eq!(
            view.lines,
            ["ijkl", "mnop", "qrst", "uvwx", "yz01", "2345", "6789", ""]
        );
        assert_eq!((view.cursor_row, view.cursor_column), (7, 0));
        let view = editor.viewport(4, 2);
        assert_eq!(view.lines, ["6789", ""]);
        assert_eq!((view.cursor_row, view.cursor_column), (1, 0));
        editor.apply(EditorKey::Home, 4);
        let view = editor.viewport(4, 20);
        assert_eq!(
            view.lines,
            ["abcd", "efgh", "ijkl", "mnop", "qrst", "uvwx", "yz01", "2345"]
        );
        assert_eq!((view.cursor_row, view.cursor_column), (0, 0));
        assert_eq!(editor.contents(), "abcdefghijklmnopqrstuvwxyz0123456789");
    }
}
