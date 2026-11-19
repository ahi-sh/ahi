//! Version 2 groups common grants. Expand into the same validator and evaluator as v1.
use super::*;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Modes {
    #[serde(rename = "version")]
    _version: u32,
    default_mode: String,
    modes: BTreeMap<String, Mode>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Mode {
    allow: Permissions,
    ask: Permissions,
    deny: Permissions,
    rules: Vec<RawRule>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Permissions {
    tools: Vec<String>,
    write_files: bool,
    skills: Vec<String>,
    skill_scripts: Vec<String>,
    commands: Vec<Commands>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Commands {
    programs: Vec<String>,
    #[serde(default)]
    toolchain_env: Vec<String>,
}

impl Modes {
    pub(super) fn compile(self, origin: &str, builtin: &[&str]) -> Result<ActionPolicy> {
        let mut selected = None;
        for (name, mode) in self.modes {
            if !crate::precommit::policy::valid_rule_id(&name)
                || matches!(name.as_str(), "read-only" | "read-write" | "policy")
            {
                return Err(Error::new(format!(
                    "{origin}: invalid or reserved mode name `{name}`"
                )));
            }
            let mut rules = mode.rules;
            for (label, effect, ask, grants) in [
                ("allow", Effect::Approve, false, mode.allow),
                ("ask", Effect::Approve, true, mode.ask),
                ("deny", Effect::Deny, false, mode.deny),
            ] {
                let mut push = |action: &str, matcher: RawMatch, env| {
                    let id = format!("{label}-{}", rules.len() + 1);
                    rules.push(RawRule {
                        id,
                        action: action.into(),
                        matcher,
                        effect,
                        ask,
                        sandbox: None,
                        toolchain_env: env,
                    });
                };
                for tool in grants.tools {
                    push(
                        "tool_call",
                        RawMatch {
                            tool: Some(tool),
                            ..Default::default()
                        },
                        None,
                    );
                }
                if grants.write_files {
                    push("file_write", RawMatch::default(), None);
                }
                for skill in grants.skills {
                    push(
                        "skill",
                        RawMatch {
                            skill: Some(skill),
                            ..Default::default()
                        },
                        None,
                    );
                }
                for skill in grants.skill_scripts {
                    push(
                        "skill_command",
                        RawMatch {
                            skill: Some(skill),
                            script: Some("scripts/**".into()),
                            ..Default::default()
                        },
                        None,
                    );
                }
                for commands in grants.commands {
                    if commands.programs.is_empty() {
                        return Err(Error::new(format!(
                            "{origin}: commands.programs cannot be empty"
                        )));
                    }
                    for program in commands.programs {
                        push(
                            "command",
                            RawMatch {
                                argv: Some(vec![program]),
                                ..Default::default()
                            },
                            Some(commands.toolchain_env.clone()),
                        );
                    }
                }
            }
            let policy = compile_rules(
                RawPolicy {
                    version: 1,
                    default: None,
                    rules,
                },
                &format!("{origin}: mode {name}"),
                builtin,
            )?;
            if name == self.default_mode {
                selected = Some(policy);
            }
        }
        selected.ok_or_else(|| {
            Error::new(format!(
                "{origin}: unknown default_mode `{}`",
                self.default_mode
            ))
        })
    }
}
