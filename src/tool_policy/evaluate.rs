//! Evaluate declared rules without performing effects.

use super::{path_within, ActionKind, ActionPolicy, Effect, ToolRule, Verdict};

impl ToolRule {
    fn governs(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t == tool)
    }

    fn argv_matches(&self, argv: &[String]) -> bool {
        self.argv
            .as_deref()
            .is_none_or(|prefix| argv.len() >= prefix.len() && argv[..prefix.len()] == *prefix)
    }
}

impl ActionPolicy {
    /// Builtin tools that at least one rule could approve, in rule order.
    pub(crate) fn advertised_tools(&self) -> Vec<String> {
        let mut tools: Vec<String> = Vec::new();
        for rule in &self.rules {
            if rule.effect == Effect::Approve {
                for tool in &rule.tools {
                    if !tools.contains(tool) {
                        tools.push(tool.clone());
                    }
                }
            }
        }
        tools
    }

    pub(crate) fn rule(&self, id: &str) -> Option<&ToolRule> {
        self.rules.iter().find(|rule| rule.id == id)
    }

    /// Skill names at least one rule could approve, in rule order.
    pub(crate) fn approved_skills(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for rule in &self.rules {
            if rule.action == ActionKind::Skill && rule.effect == Effect::Approve {
                if let Some(name) = rule.argv.as_ref().and_then(|argv| argv.first()) {
                    if !names.contains(name) {
                        names.push(name.clone());
                    }
                }
            }
        }
        names
    }

    /// The verdict for loading the skill called `name`: a deny rule matches
    /// without regard to case (`Deploy` is `deploy` to a deny), an approval
    /// only exactly, and a name no rule approves is denied.
    pub(crate) fn evaluate_skill(&self, name: &str) -> Verdict {
        let skill_rules = || {
            self.rules
                .iter()
                .filter(|rule| rule.action == ActionKind::Skill)
        };
        let named = |rule: &ToolRule| rule.argv.as_ref().and_then(|argv| argv.first().cloned());
        if let Some(rule) = skill_rules().find(|rule| {
            rule.effect == Effect::Deny && named(rule).is_some_and(|n| n.eq_ignore_ascii_case(name))
        }) {
            return Verdict::Deny {
                rule_id: Some(rule.id.clone()),
            };
        }
        let approving = |rule: &&ToolRule| {
            rule.effect == Effect::Approve && named(rule).is_some_and(|n| n == name)
        };
        if let Some(rule) = skill_rules().find(|rule| approving(rule) && !rule.ask) {
            return Verdict::Approve {
                rule_id: rule.id.clone(),
            };
        }
        if let Some(rule) = skill_rules().find(approving) {
            return Verdict::Ask {
                rule_id: rule.id.clone(),
            };
        }
        Verdict::Deny { rule_id: None }
    }

    /// `path` is the file, directory, or working directory the call names;
    /// `argv` is empty for every tool but `run_command`.
    pub(crate) fn evaluate(&self, tool: &str, path: &str, argv: &[String]) -> Verdict {
        let matching = || {
            self.rules.iter().filter(move |rule| {
                rule.governs(tool)
                    && rule.argv_matches(argv)
                    && rule
                        .path
                        .as_deref()
                        .is_none_or(|prefix| path_within(prefix, path))
            })
        };
        // Deny rules match without regard to case: macOS and Windows
        // filesystems usually resolve `SRC/x` and `src/x` to the same file, and
        // canonicalization keeps the requested spelling. Approvals stay exact,
        // so a differently cased path falls through to ask or default deny.
        let folded = path.to_lowercase();
        if let Some(rule) = self.rules.iter().find(|rule| {
            rule.effect == Effect::Deny
                && rule.governs(tool)
                && rule.argv_matches(argv)
                && rule
                    .path
                    .as_deref()
                    .is_none_or(|prefix| path_within(&prefix.to_lowercase(), &folded))
        }) {
            return Verdict::Deny {
                rule_id: Some(rule.id.clone()),
            };
        }
        if let Some(rule) = matching().find(|rule| rule.effect == Effect::Approve && !rule.ask) {
            return Verdict::Approve {
                rule_id: rule.id.clone(),
            };
        }
        if let Some(rule) = matching().find(|rule| rule.effect == Effect::Approve) {
            return Verdict::Ask {
                rule_id: rule.id.clone(),
            };
        }
        Verdict::Deny { rule_id: None }
    }
}
