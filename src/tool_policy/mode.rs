//! Per-turn restrictions are independent of workspace trust and presentation.
use super::{ActionKind, ActionPolicy, Effect, ToolRule};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ExecutionMode {
    #[default]
    Policy,
    ReadOnly,
    ReadWrite,
}

impl ExecutionMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Policy => "Policy",
            Self::ReadOnly => "Read-only",
            Self::ReadWrite => "Read-write",
        }
    }
    pub(crate) fn allows(self, tool: &str) -> bool {
        self != Self::ReadOnly
            || matches!(
                tool,
                "read_file" | "list_dir" | "find_files" | "search_files" | "load_skill"
            )
    }
}

impl ActionPolicy {
    /// Explicit interactive trust grants ordinary tools without an argv prefix.
    /// Existing policy files are still parsed before this choice can be used.
    pub(crate) fn trusted() -> Self {
        let mut rules = Vec::new();
        for tool in crate::tools::BUILTIN {
            if tool == "load_skill" {
                continue;
            }
            let action = match tool {
                "run_command" => ActionKind::Command,
                "delegate" => ActionKind::Delegate,
                "edit_file" | "write_file" => ActionKind::FileWrite,
                _ => ActionKind::ToolCall,
            };
            rules.push(ToolRule {
                id: format!("trusted-{tool}"),
                action,
                tools: vec![tool.into()],
                path: None,
                argv: None,
                skill_script: None,
                toolchain_env: Vec::new(),
                effect: Effect::Approve,
                ask: false,
            });
        }
        for scope in ["operator", "repo"] {
            rules.push(ToolRule {
                id: format!("trusted-{scope}-skills"),
                action: ActionKind::Skill,
                tools: vec!["load_skill".into()],
                path: None,
                argv: Some(vec![format!("{scope}/*")]),
                skill_script: None,
                toolchain_env: Vec::new(),
                effect: Effect::Approve,
                ask: false,
            });
            rules.push(ToolRule {
                id: format!("trusted-{scope}-scripts"),
                action: ActionKind::SkillCommand,
                tools: vec!["run_command".into()],
                path: None,
                argv: None,
                skill_script: Some((format!("{scope}/*"), "scripts/**".into())),
                toolchain_env: Vec::new(),
                effect: Effect::Approve,
                ask: false,
            });
        }
        Self { rules }
    }
}
