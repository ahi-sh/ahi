//! Operator-owned workspace trust and a small, draft-preserving mode chooser.
use crate::error::{Error, Result};
use crate::tool_policy::ExecutionMode;
use ahi_widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui_core::layout::Rect;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub(super) enum Dialog {
    Trust,
    Choose,
    Bypass,
}

pub(super) struct Modes {
    pub selected: ExecutionMode,
    pub active: Option<ExecutionMode>,
    pub dialog: Option<Dialog>,
    pub has_policy: bool,
    pub trusted: bool,
    pub bypass: bool,
}
impl Default for Modes {
    fn default() -> Self {
        Self {
            selected: ExecutionMode::Policy,
            active: None,
            dialog: None,
            has_policy: false,
            trusted: false,
            bypass: false,
        }
    }
}
impl Modes {
    pub fn startup(root: &Path, state: &Path, policy: Option<&Path>) -> Result<Self> {
        let installed = match policy {
            Some(path) => Some(crate::tool_policy::load_file(path, &crate::tools::BUILTIN)?),
            None => crate::tool_policy::load(root, &crate::tools::BUILTIN)?,
        };
        let has_policy = installed.is_some();
        let trusted = std::fs::read_to_string(trust_path(state, root))
            .is_ok_and(|text| text == root.to_string_lossy());
        Ok(Self {
            has_policy,
            trusted,
            selected: if has_policy {
                ExecutionMode::Policy
            } else if trusted {
                ExecutionMode::ReadWrite
            } else {
                ExecutionMode::ReadOnly
            },
            dialog: (!has_policy && !trusted).then_some(Dialog::Trust),
            ..Self::default()
        })
    }
    pub fn cycle(&mut self) {
        use ExecutionMode::*;
        self.selected = match self.selected {
            Policy => ReadOnly,
            ReadOnly if self.has_policy && !self.bypass => Policy,
            ReadOnly if self.trusted || self.bypass => ReadWrite,
            ReadOnly => {
                self.dialog = Some(Dialog::Trust);
                ReadOnly
            }
            ReadWrite if self.has_policy => Policy,
            ReadWrite => ReadOnly,
        };
    }
    pub fn status(&self) -> String {
        match self.active {
            Some(active) if active != self.selected => {
                format!("{} → {} next turn", active.label(), self.selected.label())
            }
            _ => self.selected.label().to_string(),
        }
    }
    pub fn draw(&self, frame: &mut ratatui_core::terminal::Frame, root: &Path, policy: &Path) {
        let Some(dialog) = self.dialog else {
            return;
        };
        let area = frame.area();
        let popup = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        let (title, body) = match dialog {
            Dialog::Trust => (" Trust this workspace? ", format!("{}\n\nRead, edit, run commands, delegate to configured workers, and use repo/operator skills and bundled scripts. Commands run with your account's access. This is NOT a sandbox.\n\n[t] Trust and continue (remember this workspace)\n[r] Continue read-only (no commands or writes)\n[Esc] Cancel\n\nTrust is stored outside the checkout; /mode revoke removes it.", root.display())),
            Dialog::Choose => (" Execution mode ", format!("Current: {}\nPolicy: {}\n\n[1] Read-only: no writes, commands or delegation\n[2] Read-write: trusted host access\n[3] Policy: use the policy file\n\nShift+Tab cycles available modes. Changes apply next turn; running commands are not stopped.\n/mode revoke forgets workspace trust. Esc closes without changing the draft.", self.status(), policy.display())),
            Dialog::Bypass => (" Bypass tool policy for this session? ", "Read-write ignores your custom policy, including denies and approval prompts. Commands run with your account's access, not in a sandbox. The policy file is unchanged.\n\n[t] Confirm session-only bypass\n[Esc] Keep current mode".to_string()),
        };
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL).title(title)),
            popup,
        );
    }
}
fn trust_path(state: &Path, root: &Path) -> PathBuf {
    state
        .join("workspace-trust")
        .join(crate::session::sha256_hex(
            root.to_string_lossy().as_bytes(),
        ))
}
pub(super) fn remember(state: &Path, root: &Path, trust: bool) -> Result<()> {
    let path = trust_path(state, root);
    if !trust {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::new(e.to_string())),
        };
    }
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| Error::new(e.to_string()))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    match options.open(&path) {
        Ok(mut file) => file
            .write_all(root.to_string_lossy().as_bytes())
            .map_err(|e| Error::new(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read_to_string(path).is_ok_and(|text| text == root.to_string_lossy()) {
                Ok(())
            } else {
                Err(Error::new(
                    "invalid workspace trust record; revoke it before trusting again",
                ))
            }
        }
        Err(e) => Err(Error::new(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trust_is_outside_checkout_and_policy_always_takes_precedence() {
        let dir = crate::fixture_directory::create_unique_fixture_directory("workspace-trust");
        let root = dir.join("work");
        let state = dir.join("state");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        assert!(matches!(
            Modes::startup(&root, &state, None).unwrap().dialog,
            Some(Dialog::Trust)
        ));
        remember(&state, &root, true).unwrap();
        let mut modes = Modes::startup(&root, &state, None).unwrap();
        assert_eq!(modes.selected, ExecutionMode::ReadWrite);
        assert!(modes.dialog.is_none());
        modes.cycle();
        assert_eq!(modes.selected, ExecutionMode::ReadOnly);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::write(root.join("ahi.tools.yaml"), "version: 1\nrules: []\n").unwrap();
        let mut modes = Modes::startup(&root, &state, None).unwrap();
        assert_eq!(modes.selected, ExecutionMode::Policy);
        modes.cycle();
        modes.cycle();
        assert_eq!(modes.selected, ExecutionMode::Policy);
        std::fs::write(root.join("ahi.tools.yaml"), "version: broken\n").unwrap();
        assert!(Modes::startup(&root, &state, None).is_err());
        remember(&state, &root, false).unwrap();
        std::fs::remove_file(root.join("ahi.tools.yaml")).unwrap();
        assert!(matches!(
            Modes::startup(&root, &state, None).unwrap().dialog,
            Some(Dialog::Trust)
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
