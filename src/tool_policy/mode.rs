//! Per-turn restrictions are independent of workspace trust and presentation.
use super::{ActionKind, ActionPolicy, Effect, ToolRule};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ExecutionMode {
    #[default]
    Policy,
    #[serde(rename = "plan", alias = "read-only")]
    ReadOnly,
    #[serde(rename = "trusted", alias = "read-write")]
    ReadWrite,
    /// Trusted, plus file tools on absolute host paths. Built in, never the
    /// startup default, never remembered, never cycled into: the picker
    /// confirms it for this process only.
    #[serde(rename = "full-access")]
    FullAccess,
    #[serde(untagged)]
    Named(String),
}

impl ExecutionMode {
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Policy => "Policy",
            Self::ReadOnly => "Plan",
            Self::ReadWrite => "Trusted",
            Self::FullAccess => "Full Access",
            Self::Named(name) => name,
        }
    }
    /// Whether file tools may name absolute paths outside the working
    /// directory. Only Full Access does; every other mode keeps the
    /// workspace-relative contract.
    pub(crate) fn host_paths(&self) -> bool {
        matches!(self, Self::FullAccess)
    }
    pub(crate) fn allows(&self, tool: &str) -> bool {
        self != &Self::ReadOnly
            || matches!(
                tool,
                "read_file"
                    | "list_dir"
                    | "find_files"
                    | "search_files"
                    | "load_skill"
                    | "git_inspect"
                    | "plan_scratch"
            )
    }
}

impl ActionPolicy {
    pub(crate) fn plan() -> Self {
        let mut policy = Self::trusted();
        policy
            .rules
            .retain(|r| r.tools.iter().all(|t| ExecutionMode::ReadOnly.allows(t)));
        policy
    }
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
        Self {
            rules,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_access_is_a_distinct_built_in_mode_on_the_wire() {
        let mode: ExecutionMode = serde_json::from_value(serde_json::json!("full-access")).unwrap();
        assert_eq!(mode, ExecutionMode::FullAccess);
        assert_eq!(serde_json::to_value(&mode).unwrap(), "full-access");
        assert_eq!(mode.label(), "Full Access");
        assert!(mode.host_paths());
        assert!(mode.allows("write_file") && mode.allows("run_command"));
        for other in [
            ExecutionMode::Policy,
            ExecutionMode::ReadOnly,
            ExecutionMode::ReadWrite,
            ExecutionMode::Named("full_access".into()),
        ] {
            assert!(!other.host_paths(), "{other:?}");
        }
        // Legacy aliases never resolve to Full Access.
        for legacy in ["read-write", "trusted"] {
            let mode: ExecutionMode =
                serde_json::from_value(serde_json::Value::String(legacy.into())).unwrap();
            assert_eq!(mode, ExecutionMode::ReadWrite, "{legacy}");
        }
    }
}
