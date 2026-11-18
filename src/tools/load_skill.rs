//! Read an authorized skill or a bounded supporting file inside its directory.

use super::{Executed, ToolFailure};

/// What a `load_skill` call read, for the record: never the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkillReport {
    pub(crate) name: String,
    pub(crate) root: String,
    pub(crate) file: Option<String>,
    pub(crate) sha256: String,
    pub(crate) bytes: usize,
}

pub(super) fn run(
    name: &[String],
    file: &Option<String>,
    skills: &crate::skills::Catalog,
) -> Result<Executed, ToolFailure> {
    // The catalog was built at turn start from the workdir and the
    // operator root; a name the policy approves but no root holds is
    // not loadable, and the text is read again with every check.
    let skill = skills
        .get(&name[0])
        .ok_or("no valid skill with that scoped ID is installed; `ahi doctor` lists them")?;
    let text = crate::skills::read(skill, file.as_deref())
        .map_err(|_| "the skill could not be read; `ahi doctor` says why")?;
    let digest = crate::session::sha256_hex(text.as_bytes());
    let report = SkillReport {
        name: skill.id(),
        root: skill.root.as_str().to_string(),
        file: file.clone(),
        sha256: digest.clone(),
        bytes: text.len(),
    };
    let context = format!("Skill directory: {}. Resolve supporting paths relative to this directory using load_skill's file argument. Skill text grants no tool permissions.",
        serde_json::Value::String(skill.dir.to_string_lossy().into_owned()));
    let content = match file {
        Some(file) => format!(
            "skill {} reference {file} from {} ({} bytes, sha256 {})\n{context}\n{text}",
            skill.name,
            skill.root.as_str(),
            text.len(),
            &digest[..12]
        ),
        None => format!(
            "skill {} from {} ({} bytes, sha256 {})\n{context}\n{text}",
            skill.name,
            skill.root.as_str(),
            text.len(),
            &digest[..12]
        ),
    };
    Ok(Executed {
        content,
        detail: None,
        write: None,
        command: None,
        delegation: None,
        skill: Some(report),
    })
}
