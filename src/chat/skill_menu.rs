//! Cursor-local skill completion; selecting a name edits the draft, never sends it.
use ahi_widgets::{Block, Borders, Clear, List, ListItem, ListState};
use ratatui_core::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
};

pub(super) fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

/// A bare dollar or dollar amount stays literal; named references are explicit.
pub(super) fn expand(text: &str, approved: &[String]) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('$') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let len = tail
            .chars()
            .take_while(|c| name_char(*c))
            .map(char::len_utf8)
            .sum::<usize>();
        let name = &tail[..len];
        if name.is_empty() || name.chars().all(|c| c.is_ascii_digit() || c == '.') {
            out.push('$');
            rest = tail;
            continue;
        }
        if !approved.iter().any(|entry| entry == name) {
            return Err(format!(
                "no {} rule approves the skill \"{name}\"; the draft was kept",
                crate::tool_policy::FILE_NAME
            ));
        }
        out.push_str(&format!("Use the skill \"{name}\"."));
        rest = &tail[len..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::input::{EditorKey, InputEditor};

    #[test]
    fn completion_preserves_unicode_multiline_and_surrounding_text() {
        for (draft, left, expected) in [
            ("$rev", 0, "$review"),
            ("前\nα$rev after", 6, "前\nα$review after"),
            ("before $review after", 9, "before $review after"),
        ] {
            let mut input = InputEditor::from_text(draft);
            for _ in 0..left {
                input.apply(EditorKey::Left, 80);
            }
            assert_eq!(input.skill_query().as_deref(), Some("rev"));
            input.complete_skill("review");
            assert_eq!(input.contents(), expected);
            input.apply(EditorKey::Character('!'), 80);
            assert!(input.contents().contains("$review!"));
        }
    }

    #[test]
    fn popup_renders_filtered_names_and_empty_state() {
        use ratatui_core::{backend::TestBackend, terminal::Terminal};
        let menu = SkillMenu {
            names: vec!["review".into(), "release".into()],
            ..Default::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
        terminal
            .draw(|frame| menu.draw(frame, frame.area(), "VIEW"))
            .unwrap();
        let visible: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(visible.contains("$review"));
        assert!(!visible.contains("$release"));
        terminal
            .draw(|frame| menu.draw(frame, frame.area(), "missing"))
            .unwrap();
        let visible: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(visible.contains("No matching skills"));
    }

    #[test]
    fn invocation_expands_multiple_inline_names_and_keeps_literal_dollars() {
        let approved = vec!["review".into(), "test".into()];
        assert_eq!(
            expand("先$review then\n$test please", &approved).unwrap(),
            "先Use the skill \"review\". then\nUse the skill \"test\". please"
        );
        assert_eq!(
            expand("cost $20.50 and $", &approved).unwrap(),
            "cost $20.50 and $"
        );
        assert_eq!(
            expand("before $missing after", &approved).unwrap_err(),
            "no ahi.tools.yaml rule approves the skill \"missing\"; the draft was kept"
        );
    }
}

#[derive(Default)]
pub(super) struct SkillMenu {
    pub names: Vec<String>,
    pub dismissed: bool,
    pub selected: usize,
}

impl SkillMenu {
    pub fn reset(&mut self) {
        self.dismissed = false;
        self.selected = 0;
    }

    pub fn matching(&self, query: &str) -> Vec<&str> {
        let query = query.to_ascii_lowercase();
        self.names
            .iter()
            .filter(|name| name.to_ascii_lowercase().contains(&query))
            .map(String::as_str)
            .collect()
    }

    pub fn draw(&self, frame: &mut ratatui_core::terminal::Frame, area: Rect, query: &str) {
        if self.dismissed || area.width < 4 || area.height < 3 {
            return;
        }
        let names = self.matching(query);
        let height = (names.len().max(1) as u16 + 2).min(area.height).min(8);
        let popup = Rect::new(area.x, area.bottom() - height, area.width.min(80), height);
        let items: Vec<ListItem> = if names.is_empty() {
            vec![ListItem::new("No matching skills")]
        } else {
            names
                .iter()
                .map(|name| ListItem::new(format!("${name}")))
                .collect()
        };
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Skills · ↑/↓ select · Enter insert · Esc dismiss ")
                    .title_bottom(Line::raw(" Tool-policy approval required to invoke ")),
            )
            .highlight_style(Style::default().bg(Color::Rgb(45, 65, 85)))
            .highlight_symbol("› ");
        let mut state = ListState::default().with_selected(if names.is_empty() {
            None
        } else {
            Some(self.selected.min(names.len() - 1))
        });
        frame.render_widget(Clear, popup);
        frame.render_stateful_widget(list, popup, &mut state);
    }
}
