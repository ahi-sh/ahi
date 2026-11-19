//! Cursor-local skill completion; selecting a name edits the draft, never sends it.
use ahi_widgets::{Block, Borders, Clear, List, ListItem, ListState};
use ratatui_core::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::input::{EditorKey, InputEditor};

    #[test]
    fn completion_preserves_unicode_multiline_and_surrounding_text() {
        for (draft, left, expected) in [
            ("$rev", 0, "$repo/review"),
            ("前\nα$rev after", 6, "前\nα$repo/review after"),
            ("before $revafter", 5, "before $repo/reviewafter"),
        ] {
            let mut input = InputEditor::from_text(draft);
            for _ in 0..left {
                input.apply(EditorKey::Left, 80);
            }
            assert_eq!(input.skill_query().as_deref(), Some("rev"));
            input.complete_skill("repo/review");
            assert_eq!(input.contents(), expected);
            input.apply(EditorKey::Character('!'), 80);
            assert!(input.contents().contains("$repo/review!"));
            assert_eq!(input.selected_skills(), ["repo/review"]);
            input.apply(EditorKey::Left, 80);
            input.apply(EditorKey::Backspace, 80);
            assert!(input.selected_skills().is_empty());
        }
    }

    #[test]
    fn popup_renders_filtered_names_and_empty_state() {
        use ratatui_core::{backend::TestBackend, terminal::Terminal};
        let menu = SkillMenu {
            names: vec!["repo/review".into(), "repo/release".into()],
            dismissed: false,
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
        assert!(visible.contains("$repo/review"));
        assert!(!visible.contains("$repo/release"));
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
}

#[derive(Default, Clone, Copy)]
enum Scope {
    #[default]
    Both,
    Repo,
    Operator,
}

pub(super) struct SkillMenu {
    pub names: Vec<String>,
    pub descriptions: std::collections::BTreeMap<String, String>,
    pub dismissed: bool,
    pub selected: usize,
    scope: Scope,
}

impl Default for SkillMenu {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            descriptions: Default::default(),
            dismissed: true,
            selected: 0,
            scope: Scope::Both,
        }
    }
}

impl SkillMenu {
    pub fn reset(&mut self) {
        self.dismissed = false;
        self.selected = 0;
        self.scope = Scope::Both;
    }

    pub fn cycle_scope(&mut self) {
        self.scope = match self.scope {
            Scope::Both => Scope::Repo,
            Scope::Repo => Scope::Operator,
            Scope::Operator => Scope::Both,
        };
        self.selected = 0;
    }

    pub fn matching(&self, query: &str) -> Vec<&str> {
        let query = query.to_ascii_lowercase();
        let mut matches: Vec<_> = self
            .names
            .iter()
            .filter_map(|id| {
                let (scope, bare) = id.split_once('/')?;
                if matches!(self.scope, Scope::Repo) && scope != "repo"
                    || matches!(self.scope, Scope::Operator) && scope != "operator"
                {
                    return None;
                }
                let full = id.to_ascii_lowercase();
                let bare = bare.to_ascii_lowercase();
                let score = if full == query || bare == query {
                    0
                } else if bare.starts_with(&query) || full.starts_with(&query) {
                    1
                } else if bare.contains(&query) || full.contains(&query) {
                    2
                } else {
                    let mut chars = full.chars();
                    if !query
                        .chars()
                        .all(|wanted| chars.by_ref().any(|c| c == wanted))
                    {
                        return None;
                    }
                    3
                };
                Some((score, scope != "repo", id.as_str()))
            })
            .collect();
        matches.sort_unstable();
        matches.into_iter().map(|(_, _, id)| id).collect()
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
                .map(|name| match self.descriptions.get(*name) {
                    Some(description) => ListItem::new(format!("${name}  {description}")),
                    None => ListItem::new(format!("${name}")),
                })
                .collect()
        };
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(match self.scope {
                        Scope::Both => " Skills [Both] · Tab scope · Enter insert · Esc dismiss ",
                        Scope::Repo => " Skills [Repo] · Tab scope · Enter insert · Esc dismiss ",
                        Scope::Operator => {
                            " Skills [Operator] · Tab scope · Enter insert · Esc dismiss "
                        }
                    })
                    .title_bottom(Line::raw(" Inserts into draft · active mode governs use ")),
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
