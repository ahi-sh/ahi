use serde_json::{json, Value};

use super::{ActionKind, ActionPolicy, Effect, ExecutionMode, ToolRule};

const MAX_SUMMARY_BYTES: usize = 32 * 1024;

impl ActionPolicy {
    pub(crate) fn capability_details(
        &self,
        mode: &ExecutionMode,
        advertised: &[&str],
        visible_skills: &[String],
    ) -> String {
        if advertised.is_empty() {
            return String::new();
        }
        let mut details = String::from("\n\nRuntime policy: matching rules apply in this order: deny, approve, ask. Unmatched calls are denied. Path prefixes match whole components. Argv prefixes match exact tokens. An omitted selector adds no restriction.");
        if advertised
            .iter()
            .any(|name| matches!(*name, "write_file" | "edit_file"))
        {
            details.push_str(&format!(
                "\nFile writes exclude protected names {}, Git metadata, skill roots and ahi state.",
                json!(crate::tools::PROTECTED),
            ));
        }
        if mode.host_paths() {
            details.push_str("\nFile tools accept absolute host paths. Command working directories stay inside the workspace.");
        }
        if advertised.contains(&"run_command") {
            details.push_str("\nCommands run argv directly. The working directory does not confine a program's filesystem access.");
        }
        if advertised.contains(&super::DELEGATE_TOOL) {
            details.push_str(
                "\nDelegation requires a configured connector or an authorized local profile.",
            );
        }
        let preamble_length = details.len();
        for rule in &self.rules {
            let tools: Vec<_> = rule
                .tools
                .iter()
                .map(String::as_str)
                .filter(|tool| advertised.contains(tool))
                .collect();
            if tools.is_empty() || !rule_visible(rule, visible_skills) {
                continue;
            }
            let line = describe_rule(rule, &tools).to_string();
            if details.len() + line.len() + 1 > MAX_SUMMARY_BYTES {
                details.truncate(preamble_length);
                details.push_str("\nRule details exceed the summary limit.");
                break;
            }
            details.push('\n');
            details.push_str(&line);
        }
        details
    }
}

fn rule_visible(rule: &ToolRule, visible_skills: &[String]) -> bool {
    let selector = match rule.action {
        ActionKind::Skill => rule.argv.as_ref().and_then(|argv| argv.first()),
        ActionKind::SkillCommand => rule.skill_script.as_ref().map(|(skill, _)| skill),
        _ => return true,
    };
    selector.is_some_and(|selector| {
        if let Some(scope) = selector.strip_suffix('*') {
            visible_skills.iter().any(|id| id.starts_with(scope))
        } else {
            visible_skills.iter().any(|id| {
                id == selector || (rule.effect == Effect::Deny && id.eq_ignore_ascii_case(selector))
            })
        }
    })
}

fn describe_rule(rule: &ToolRule, tools: &[&str]) -> Value {
    let effect = match (rule.effect, rule.ask) {
        (Effect::Deny, _) => "deny",
        (Effect::Approve, true) => "ask",
        (Effect::Approve, false) => "approve",
    };
    let mut description = json!({"effect": effect, "tools": tools});
    if let Some(prefix) = &rule.path {
        let field = match rule.action {
            ActionKind::Command | ActionKind::SkillCommand => "working_directory_prefix",
            _ => "path_prefix",
        };
        description[field] = json!(prefix);
    }
    if let Some(prefix) = &rule.argv {
        match rule.action {
            ActionKind::Skill => description["skill_selector"] = json!(prefix.first()),
            ActionKind::Delegate => description["worker_or_local_profile_prefix"] = json!(prefix),
            ActionKind::SkillCommand => description["script_args_prefix"] = json!(prefix),
            _ => description["argv_prefix"] = json!(prefix),
        }
    }
    match rule.action {
        ActionKind::Command => description["action"] = json!("command"),
        ActionKind::SkillCommand => {
            if let Some((skill, script)) = &rule.skill_script {
                description["skill_selector"] = json!(skill);
                description["script_selector"] = json!(script);
            }
        }
        _ => {}
    }
    description
}
