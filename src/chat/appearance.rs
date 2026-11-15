//! Operator-owned chat appearance. Files live in ahi's config directory,
//! never in the workspace and never in a conversation's model context.

use std::fs;
use std::path::Path;

use ratatui_core::style::{Color, Style};
use serde::{de::DeserializeOwned, Deserialize, Deserializer};

use super::transcript::TranscriptRole;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Padding {
    /// Terminal columns per side, not pixels or CSS ems.
    pub(crate) horizontal: u8,
    /// Terminal rows above and below a colored message.
    pub(crate) vertical: u8,
}

impl Default for Padding {
    fn default() -> Self {
        Appearance::DEFAULT.padding
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThemeColor(Color);

impl<'de> Deserialize<'de> for ThemeColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text == "default" {
            return Ok(Self(Color::Reset));
        }
        if let Some(hex) = text.strip_prefix('#') {
            if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                let value = u32::from_str_radix(hex, 16).map_err(serde::de::Error::custom)?;
                return Ok(Self(Color::Rgb(
                    (value >> 16) as u8,
                    (value >> 8) as u8,
                    value as u8,
                )));
            }
        }
        Err(serde::de::Error::custom("expected #RRGGBB or default"))
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Backgrounds {
    operator: ThemeColor,
    assistant: ThemeColor,
    tool_success: ThemeColor,
    tool_error: ThemeColor,
    alert: ThemeColor,
    notice: ThemeColor,
}

impl Default for Backgrounds {
    fn default() -> Self {
        Theme::DEFAULT.backgrounds
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Theme {
    backgrounds: Backgrounds,
    diff: DiffColors,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
struct DiffColors {
    added: ThemeColor,
    removed: ThemeColor,
    added_background: ThemeColor,
    removed_background: ThemeColor,
    added_emphasis: ThemeColor,
    removed_emphasis: ThemeColor,
}

impl Default for DiffColors {
    fn default() -> Self {
        Theme::DEFAULT.diff
    }
}

impl Theme {
    pub(crate) const DEFAULT: Self = Self {
        backgrounds: Backgrounds {
            operator: ThemeColor(Color::Rgb(30, 45, 60)),
            assistant: ThemeColor(Color::Reset),
            tool_success: ThemeColor(Color::Rgb(30, 60, 45)),
            tool_error: ThemeColor(Color::Rgb(60, 30, 30)),
            alert: ThemeColor(Color::Rgb(70, 50, 20)),
            notice: ThemeColor(Color::Reset),
        },
        diff: DiffColors {
            added: ThemeColor(Color::Rgb(160, 230, 170)),
            removed: ThemeColor(Color::Rgb(255, 160, 160)),
            added_background: ThemeColor(Color::Rgb(35, 85, 45)),
            removed_background: ThemeColor(Color::Rgb(90, 35, 35)),
            added_emphasis: ThemeColor(Color::Rgb(50, 130, 65)),
            removed_emphasis: ThemeColor(Color::Rgb(140, 50, 50)),
        },
    };

    pub(crate) fn diff_added(&self) -> Style {
        Style::default()
            .fg(self.diff.added.0)
            .bg(self.diff.added_background.0)
    }

    pub(crate) fn diff_removed(&self) -> Style {
        Style::default()
            .fg(self.diff.removed.0)
            .bg(self.diff.removed_background.0)
    }

    pub(crate) fn diff_added_emphasis(&self) -> Style {
        Style::default().bg(self.diff.added_emphasis.0)
    }

    pub(crate) fn diff_removed_emphasis(&self) -> Style {
        Style::default().bg(self.diff.removed_emphasis.0)
    }

    pub(crate) fn background(&self, role: TranscriptRole) -> Color {
        match role {
            TranscriptRole::Operator => self.backgrounds.operator.0,
            TranscriptRole::Assistant => self.backgrounds.assistant.0,
            TranscriptRole::Tool { exit_code: 0 } => self.backgrounds.tool_success.0,
            TranscriptRole::Tool { .. } => self.backgrounds.tool_error.0,
            TranscriptRole::Alert => self.backgrounds.alert.0,
            TranscriptRole::Notice => self.backgrounds.notice.0,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Appearance {
    pub(crate) padding: Padding,
    pub(crate) theme: Theme,
}

impl Appearance {
    pub(crate) const DEFAULT: Self = Self {
        padding: Padding {
            horizontal: 1,
            vertical: 1,
        },
        theme: Theme::DEFAULT,
    };

    pub(crate) fn load(config: &Path) -> Result<Self> {
        Ok(Self {
            padding: read(&config.join("padding.toml"))?,
            theme: read(&config.join("theme.toml"))?,
        })
    }
}

fn read<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match fs::read_to_string(path) {
        Ok(text) => {
            toml::from_str(&text).map_err(|err| Error::new(format!("{}: {err}", path.display())))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(err) => Err(Error::new(format!("cannot read {}: {err}", path.display()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_use_defaults_and_partial_files_override_only_named_values() {
        let dir = crate::fixture_directory::create_unique_fixture_directory("appearance");
        assert_eq!(Appearance::load(&dir).unwrap(), Appearance::DEFAULT);
        assert!(!dir.join("padding.toml").exists());
        fs::write(dir.join("padding.toml"), "vertical = 0\n").unwrap();
        fs::write(
            dir.join("theme.toml"),
            "[backgrounds]\noperator = '#112233'\nassistant = '#223344'\n",
        )
        .unwrap();
        let appearance = Appearance::load(&dir).unwrap();
        assert_eq!(
            appearance.padding,
            Padding {
                horizontal: 1,
                vertical: 0
            }
        );
        assert_eq!(
            appearance.theme.background(TranscriptRole::Operator),
            Color::Rgb(17, 34, 51)
        );
        assert_eq!(
            appearance.theme.background(TranscriptRole::Assistant),
            Color::Rgb(34, 51, 68)
        );
        assert_eq!(
            appearance
                .theme
                .background(TranscriptRole::Tool { exit_code: 1 }),
            Color::Rgb(60, 30, 30)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_files_name_the_path_and_reject_typos_and_bad_values() {
        let dir = crate::fixture_directory::create_unique_fixture_directory("appearance-errors");
        for value in [
            "vertical = -1",
            "vertical = 256",
            "vertcal = 1",
            "horizontal = 1.5",
        ] {
            fs::write(dir.join("padding.toml"), value).unwrap();
            assert!(Appearance::load(&dir)
                .unwrap_err()
                .to_string()
                .contains("padding.toml"));
        }
        fs::remove_file(dir.join("padding.toml")).unwrap();
        for value in [
            "[backgrounds]\noperator = 'red'",
            "[backgrounds]\noperator = '#fff'",
            "[backgrounds]\noperatr = '#112233'",
            "[backgounds]",
        ] {
            fs::write(dir.join("theme.toml"), value).unwrap();
            assert!(Appearance::load(&dir)
                .unwrap_err()
                .to_string()
                .contains("theme.toml"));
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
