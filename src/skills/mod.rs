//! Skills: named, bounded text the model may read on request, under an
//! `action: skill` rule. A skill is `SKILL.md` with a YAML frontmatter in a
//! directory named after it, under one of two roots:
//!
//! ```text
//! <workdir>/.agents/skills/<name>/SKILL.md repo skills, the repository's text
//! <home>/.agents/skills/<name>/SKILL.md    operator skills, outside every workdir
//! ```
//!
//! Both roots are enumerated, never fetched. Same-named skills have distinct
//! scoped IDs. A skill over a cap, with bad frontmatter, behind a symbolic
//! link, or whose directory name differs from its `name` is rejected with a
//! reason: not advertised, not loadable, listed by `ahi doctor`. The text a
//! skill carries has the authority of a file read with `read_file`: none.
//! `.agents/skills` is protected from the model's writes so a model cannot
//! author a skill it later loads.

mod files;
mod invocation;
pub(crate) use files::valid_reference;
pub(crate) use invocation::{parse_id, query_char, valid_id};

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// `SKILL.md` and each reference file at most this long.
pub(crate) const MAX_FILE_BYTES: u64 = 32 * 1024;
/// Skills per root; the rest, by name, are rejected.
pub(crate) const MAX_SKILLS: usize = 64;
/// References a skill may declare.
pub(crate) const MAX_REFERENCES: usize = 16;
/// Legacy `when` hints at most this many bytes; descriptions allow 1024 characters.
const MAX_LINE_BYTES: usize = 256;
/// The repo root, relative to the working directory.
pub(crate) const REPO_ROOT: &str = ".agents/skills";
/// The operator root, relative to the OS-resolved home directory.
pub(crate) const OPERATOR_ROOT: &str = ".agents/skills";
const SKILL_FILE: &str = "SKILL.md";

/// Which root answered for a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Root {
    Repo,
    Operator,
}

impl Root {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Repo => "repo",
            Self::Operator => "operator",
        }
    }
}

/// One valid skill. The body is read at load time, not held here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Skill {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) when: Option<String>,
    pub(crate) references: Vec<String>,
    pub(crate) disable_model_invocation: bool,
    /// Informational frontmatter, never interpreted as permission or sent in bulk.
    pub(crate) attributes: BTreeMap<String, serde_json::Value>,
    pub(crate) root: Root,
    /// The skill's directory, canonical.
    pub(crate) dir: PathBuf,
    /// Size of `SKILL.md` at enumeration.
    pub(crate) bytes: u64,
}

/// A skill directory that was refused, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rejected {
    pub(crate) name: String,
    pub(crate) root: Root,
    pub(crate) reason: String,
}

impl Skill {
    pub(crate) fn id(&self) -> String {
        format!("{}/{}", self.root.as_str(), self.name)
    }
}

/// Everything both roots hold, valid and refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Catalog {
    pub(crate) skills: Vec<Skill>,
    pub(crate) rejected: Vec<Rejected>,
}

impl Catalog {
    /// Resolve only a qualified identity, never a same-named substitute.
    pub(crate) fn get(&self, id: &str) -> Option<&Skill> {
        let (scope, name) = id.split_once('/')?;
        self.skills
            .iter()
            .find(|skill| skill.root.as_str() == scope && skill.name == name)
    }

    /// `ahi doctor` lines: the valid names by root, then each refusal.
    pub(crate) fn doctor_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for root in [Root::Repo, Root::Operator] {
            let names: Vec<&str> = self
                .skills
                .iter()
                .filter(|skill| skill.root == root)
                .map(|skill| skill.name.as_str())
                .collect();
            lines.push(format!(
                "skills ({}): {}",
                root.as_str(),
                if names.is_empty() {
                    "none".to_string()
                } else {
                    names.join(", ")
                }
            ));
        }
        for skill in &self.skills {
            if skill.disable_model_invocation {
                lines.push(format!(
                    "skill manual-only ({}): {}",
                    skill.root.as_str(),
                    skill.name
                ));
            }
            if !skill.attributes.is_empty() {
                lines.push(format!(
                    "skill metadata ({}): {}: {}",
                    skill.root.as_str(),
                    skill.name,
                    skill
                        .attributes
                        .keys()
                        .map(|key| format!("{key:?}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        for rejected in &self.rejected {
            lines.push(format!(
                "skill rejected ({}): {}: {}",
                rejected.root.as_str(),
                rejected.name,
                rejected.reason
            ));
        }
        lines
    }
}

/// `ahi doctor`: the skills both roots hold, valid and refused.
pub fn doctor_lines(workdir: &Path, home: Option<&Path>) -> Vec<String> {
    let mut lines = enumerate(workdir, home).doctor_lines();
    if workdir.join(".ahi/skills").exists() {
        lines.push(
            "skills: .ahi/skills is deprecated and ignored; move skills to .agents/skills".into(),
        );
    }
    lines
}

/// Enumerate the repo root under `workdir` and, when given, the operator
/// root under `home`. Either root may be absent, which is no skills.
pub(crate) fn enumerate(workdir: &Path, home: Option<&Path>) -> Catalog {
    let mut catalog = Catalog::default();
    let mut operator: BTreeMap<String, Skill> = BTreeMap::new();
    if let Some(home) = home {
        scan_root(
            &home.join(OPERATOR_ROOT),
            Root::Operator,
            &mut operator,
            &mut catalog.rejected,
        );
    }
    let mut repo: BTreeMap<String, Skill> = BTreeMap::new();
    scan_root(
        &workdir.join(REPO_ROOT),
        Root::Repo,
        &mut repo,
        &mut catalog.rejected,
    );
    catalog.skills = repo.into_values().chain(operator.into_values()).collect();
    catalog
}

/// An exact script path below a skill's scripts directory, never a selector.
pub(crate) fn valid_script(script: &str) -> bool {
    script.starts_with("scripts/") && !script.contains('*') && valid_reference(script)
}

/// Resolve the script again immediately before dispatch. These are pathname
/// checks, not protection against concurrent replacement or a sandbox.
pub(crate) fn script_path(skill: &Skill, script: &str) -> Result<PathBuf, &'static str> {
    if !valid_script(script) {
        return Err("script must be a relative file below scripts/");
    }
    if !skill
        .dir
        .parent()
        .is_some_and(|root| no_link_chain(root).is_ok_and(|present| present))
    {
        return Err("the skill root is not a real directory");
    }
    files::checked_file(&skill.dir, script)
        .map_err(|_| "the skill script is not a regular, non-symlink file")
}

/// Read a skill's body (`file` `None`) or a supporting relative file, with the
/// same checks as at enumeration.
pub(crate) fn read(skill: &Skill, file: Option<&str>) -> Result<String, String> {
    let name = file.unwrap_or(SKILL_FILE);
    if file == Some(SKILL_FILE) {
        return Err("omit file to load the skill's instructions".into());
    }
    let text = files::read(&skill.dir, name)?;
    match file {
        None => Ok(frontmatter(&text)?.1.to_string()),
        Some(_) => Ok(text),
    }
}

/// The frontmatter of `SKILL.md`, between `---` fences at the very start.
#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: String,
    description: String,
    #[serde(default)]
    when: Option<String>,
    #[serde(default)]
    references: Option<Vec<String>>,
    #[serde(default, rename = "disable-model-invocation")]
    disable_model_invocation: bool,
    #[serde(flatten)]
    attributes: BTreeMap<String, serde_json::Value>,
}

fn scan_root(
    root: &Path,
    which: Root,
    into: &mut BTreeMap<String, Skill>,
    rejected: &mut Vec<Rejected>,
) {
    // The root and everything above it inside the checkout must be real
    // directories: a symbolic link at `.agents` or at `skills` would point the
    // whole root elsewhere.
    match no_link_chain(root) {
        Ok(false) => return,
        Ok(true) => {}
        Err(reason) => {
            rejected.push(Rejected {
                name: "*".into(),
                root: which,
                reason,
            });
            return;
        }
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    let mut folded: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for name in &names {
        folded
            .entry(name.to_ascii_lowercase())
            .or_default()
            .push(name.clone());
    }
    for (index, name) in names.iter().enumerate() {
        if folded[&name.to_ascii_lowercase()].len() > 1 {
            rejected.push(Rejected {
                name: name.clone(),
                root: which,
                reason: "its name collides with another skill's when case is ignored".into(),
            });
            continue;
        }
        if index >= MAX_SKILLS {
            rejected.push(Rejected {
                name: name.clone(),
                root: which,
                reason: format!("more than {MAX_SKILLS} skills in the root"),
            });
            continue;
        }
        match load(root, name, which) {
            Ok(skill) => {
                into.insert(name.clone(), skill);
            }
            Err(reason) => rejected.push(Rejected {
                name: name.clone(),
                root: which,
                reason,
            }),
        }
    }
}

fn load(root: &Path, name: &str, which: Root) -> Result<Skill, String> {
    if !crate::precommit::policy::valid_rule_id(name) {
        return Err(
            "the directory name is not a skill name (1-64 of letters, digits, '-', '_', '.')"
                .into(),
        );
    }
    let dir = root.join(name);
    let metadata = fs::symlink_metadata(&dir).map_err(|err| format!("cannot stat: {err}"))?;
    if metadata.file_type().is_symlink() && which != Root::Operator {
        return Err("the skill directory is a symbolic link".into());
    }
    // Resolve an operator-installed entry once, before reading any skill files.
    // Keep using this directory even if the installer later retargets the link.
    let dir = dir
        .canonicalize()
        .map_err(|err| format!("cannot resolve the skill directory: {err}"))?;
    if !dir.is_dir() {
        return Err("not a directory".into());
    }
    let text = files::read(&dir, SKILL_FILE)?;
    let (front, _) = frontmatter(&text)?;
    let front: Frontmatter = crate::yaml_safe::typed(front, &format!("{name}/{SKILL_FILE}"))
        .map_err(|err| format!("frontmatter: {err}"))?;
    if front.name != name {
        return Err(format!(
            "frontmatter name `{}` does not equal the directory name",
            front.name
        ));
    }
    if front.description.trim().is_empty() {
        return Err("description is empty".into());
    }
    if front.description.chars().count() > 1024 {
        return Err("description is longer than 1024 characters".into());
    }
    if front
        .description
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err("description contains control characters".into());
    }
    if let Some(when) = &front.when {
        one_line("when", when)?;
    }
    let references = front.references.unwrap_or_default();
    if references.len() > MAX_REFERENCES {
        return Err(format!("more than {MAX_REFERENCES} references"));
    }
    for reference in &references {
        if !valid_reference(reference) {
            return Err(format!(
                "reference `{reference}` must be a relative path inside the skill directory"
            ));
        }
        if reference == SKILL_FILE {
            return Err("SKILL.md is the skill itself, not a reference".into());
        }
        // Exists, regular, not a link, within the cap: checked again at read.
        files::read(&dir, reference)
            .map_err(|reason| format!("reference `{reference}`: {reason}"))?;
    }
    Ok(Skill {
        name: name.to_string(),
        description: front.description,
        when: front.when,
        references,
        disable_model_invocation: front.disable_model_invocation,
        attributes: front.attributes,
        root: which,
        dir,
        bytes: text.len() as u64,
    })
}

/// `Ok(true)` when `root` is a directory reached through no symbolic link
/// from its grandparent down (`.agents`, then `skills`); `Ok(false)` when it
/// does not exist.
fn no_link_chain(root: &Path) -> Result<bool, String> {
    let mut checked = Vec::new();
    if let Some(parent) = root.parent() {
        checked.push(parent.to_path_buf());
    }
    checked.push(root.to_path_buf());
    let mut exists = true;
    for path in checked {
        match fs::symlink_metadata(&path) {
            Err(_) => exists = false,
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "{} is a symbolic link; the skills root must be a real directory",
                    path.display()
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!("{} is not a directory", path.display()));
            }
            Ok(_) => {}
        }
    }
    Ok(exists)
}

/// Split `SKILL.md` into its frontmatter YAML and the body after the
/// closing fence.
fn frontmatter(text: &str) -> Result<(&str, &str), String> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .ok_or("SKILL.md must start with a `---` frontmatter fence")?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Ok((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    Err("the frontmatter has no closing `---` fence".into())
}

fn one_line(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} is empty"));
    }
    if value.len() > MAX_LINE_BYTES {
        return Err(format!("{field} is longer than {MAX_LINE_BYTES} bytes"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} must be one line of printable text"));
    }
    Ok(())
}

/// Whether a normalized workspace-relative path may never be written by a
/// tool because it would author or replace a skill: the root directories and
/// everything under `.agents/skills` and the retired `.ahi/skills`.
pub(crate) fn protected_write(path: &str) -> bool {
    path == ".agents"
        || path == REPO_ROOT
        || path.starts_with(".agents/skills/")
        || path == ".ahi"
        || path == ".ahi/skills"
        || path.starts_with(".ahi/skills/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(dir: &Path, name: &str, front: &str, body: &str) {
        fs::create_dir_all(dir.join(name)).unwrap();
        fs::write(
            dir.join(name).join(SKILL_FILE),
            format!("---\n{front}---\n{body}"),
        )
        .unwrap();
    }

    #[test]
    fn both_roots_keep_same_named_skills_independently_addressable() {
        let base = crate::fixture_directory::create_unique_fixture_directory("skills");
        let work = base.join("work");
        let config = base.join("config");
        let repo = work.join(REPO_ROOT);
        let operator = config.join(OPERATOR_ROOT);
        skill(
            &repo,
            "rust-review",
            "name: rust-review\ndescription: Review Rust changes\nwhen: before a PR\nreferences: [checklist.md]\n",
            "# Review\nLook twice.\n",
        );
        fs::write(repo.join("rust-review/checklist.md"), "- clippy\n").unwrap();
        skill(
            &repo,
            "release",
            "name: release\ndescription: Cut a release\n",
            "repo release\n",
        );
        skill(
            &operator,
            "release",
            "name: release\ndescription: The operator's release notes\nversion: '2'\n",
            "operator release\n",
        );
        let catalog = enumerate(&work, Some(&config));
        let names: Vec<(&str, Root)> = catalog
            .skills
            .iter()
            .map(|s| (s.name.as_str(), s.root))
            .collect();
        assert_eq!(
            names,
            [
                ("release", Root::Repo),
                ("rust-review", Root::Repo),
                ("release", Root::Operator)
            ]
        );
        assert!(catalog.rejected.is_empty());
        assert!(catalog.get("release").is_none());
        let review = catalog.get("repo/rust-review").unwrap();
        assert_eq!(review.when.as_deref(), Some("before a PR"));
        assert_eq!(review.references, ["checklist.md"]);
        assert_eq!(read(review, None).unwrap(), "# Review\nLook twice.\n");
        assert_eq!(read(review, Some("checklist.md")).unwrap(), "- clippy\n");
        assert!(read(review, Some("SKILL.md"))
            .unwrap_err()
            .contains("omit file"));
        assert!(read(review, Some("../x.md"))
            .unwrap_err()
            .contains("relative path"));
        assert_eq!(
            read(catalog.get("repo/release").unwrap(), None).unwrap(),
            "repo release\n"
        );
        let lines = catalog.doctor_lines();
        assert_eq!(lines[0], "skills (repo): release, rust-review");
        assert_eq!(lines[1], "skills (operator): release");
        assert_eq!(
            read(catalog.get("operator/release").unwrap(), None).unwrap(),
            "operator release\n"
        );
        // No roots at all is no skills and no complaint.
        let empty = enumerate(&base.join("nowhere"), None);
        assert_eq!(empty, Catalog::default());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn every_refusal_has_a_reason_and_leaves_the_rest_standing() {
        let base = crate::fixture_directory::create_unique_fixture_directory("skills-bad");
        let work = base.join("work");
        let repo = work.join(REPO_ROOT);
        skill(&repo, "good", "name: good\ndescription: fine\n", "ok\n");
        skill(&repo, "wrong-name", "name: other\ndescription: x\n", "");
        skill(&repo, "no-fence", "", "");
        fs::write(repo.join("no-fence/SKILL.md"), "just text\n").unwrap();
        skill(
            &repo,
            "unknown-key",
            "name: unknown-key\ndescription: x\ntools: [sh]\n",
            "",
        );
        skill(&repo, "no-description", "name: no-description\n", "");
        skill(
            &repo,
            "long-description",
            &format!(
                "name: long-description\ndescription: {}\n",
                "d".repeat(1025)
            ),
            "",
        );
        skill(
            &repo,
            "bad-ref",
            "name: bad-ref\ndescription: x\nreferences: ['../secret.md']\n",
            "",
        );
        skill(
            &repo,
            "missing-ref",
            "name: missing-ref\ndescription: x\nreferences: [gone.md]\n",
            "",
        );
        skill(
            &repo,
            "exe-ref",
            "name: exe-ref\ndescription: x\nreferences: [run.sh]\n",
            "",
        );
        skill(
            &repo,
            "big",
            "name: big\ndescription: x\n",
            &"b".repeat(MAX_FILE_BYTES as usize),
        );
        // Two names that differ only by case: only a case-sensitive
        // filesystem can hold both directories.
        #[cfg(target_os = "linux")]
        {
            skill(&repo, "Case", "name: Case\ndescription: x\n", "");
            skill(&repo, "case", "name: case\ndescription: x\n", "");
        }
        fs::create_dir_all(repo.join("bad name")).unwrap();
        fs::write(repo.join("stray.txt"), "").unwrap();
        #[cfg(unix)]
        {
            fs::create_dir_all(base.join("elsewhere")).unwrap();
            fs::write(
                base.join("elsewhere/SKILL.md"),
                "---\nname: linked\ndescription: x\n---\n",
            )
            .unwrap();
            std::os::unix::fs::symlink(base.join("elsewhere"), repo.join("linked")).unwrap();
            skill(
                &repo,
                "linked-file",
                "name: linked-file\ndescription: x\n",
                "",
            );
            fs::remove_file(repo.join("linked-file/SKILL.md")).unwrap();
            std::os::unix::fs::symlink(
                base.join("elsewhere/SKILL.md"),
                repo.join("linked-file/SKILL.md"),
            )
            .unwrap();
        }
        let catalog = enumerate(&work, None);
        let names: Vec<&str> = catalog.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["good", "unknown-key"]);
        assert_eq!(
            catalog.get("repo/unknown-key").unwrap().attributes["tools"],
            serde_json::json!(["sh"])
        );
        let reason = |name: &str| {
            catalog
                .rejected
                .iter()
                .find(|r| r.name == name)
                .unwrap_or_else(|| panic!("{name} not rejected: {:?}", catalog.rejected))
                .reason
                .clone()
        };
        assert!(reason("wrong-name").contains("does not equal the directory name"));
        assert!(reason("no-fence").contains("frontmatter fence"));
        assert!(reason("no-description").contains("description"));
        assert!(reason("long-description").contains("longer than"));
        assert!(reason("bad-ref").contains("relative path"));
        assert!(reason("missing-ref").contains("is missing"));
        assert!(reason("exe-ref").contains("is missing"));
        assert!(reason("big").contains("the cap is"));
        #[cfg(target_os = "linux")]
        {
            assert!(reason("Case").contains("collides"));
            assert!(reason("case").contains("collides"));
        }
        assert!(reason("bad name").contains("not a skill name"));
        assert!(reason("stray.txt").contains("not a directory"));
        #[cfg(unix)]
        {
            assert!(reason("linked").contains("symbolic link"));
            assert!(reason("linked-file").contains("symbolic link"));
        }
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_skills_root_is_refused_whole() {
        let base = crate::fixture_directory::create_unique_fixture_directory("skills-root");
        let work = base.join("work");
        fs::create_dir_all(work.join(".agents")).unwrap();
        fs::create_dir_all(base.join("outside/x")).unwrap();
        fs::write(
            base.join("outside/x/SKILL.md"),
            "---\nname: x\ndescription: x\n---\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(base.join("outside"), work.join(".agents/skills")).unwrap();
        let catalog = enumerate(&work, None);
        assert!(catalog.skills.is_empty());
        assert_eq!(catalog.rejected.len(), 1);
        assert!(catalog.rejected[0].reason.contains("symbolic link"));
        // The same when `.agents` itself is the link.
        let work2 = base.join("work2");
        fs::create_dir_all(&work2).unwrap();
        std::os::unix::fs::symlink(base.join("outside"), work2.join(".agents")).unwrap();
        let catalog = enumerate(&work2, None);
        assert!(catalog.skills.is_empty());
        assert!(catalog.rejected[0].reason.contains("symbolic link"));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn portable_metadata_and_nested_undeclared_supporting_files_load() {
        let base = crate::fixture_directory::create_unique_fixture_directory("portable-skills");
        let repo = base.join(REPO_ROOT);
        let description = "界".repeat(1024);
        skill(&repo, "portable", &format!("name: portable\ndescription: {description}\nlicense: MIT\ncompatibility: Requires Rust\nmetadata: {{author: example, version: '1'}}\nallowed-tools: 'Bash(*)'\ndisable-model-invocation: true\nx-extra: {{nested: [a, b]}}\n"), "Read references/nested/guide.md.\n");
        let support = repo.join("portable/references/nested");
        fs::create_dir_all(&support).unwrap();
        fs::write(support.join("guide.md"), "Nested guidance.\n").unwrap();
        let catalog = enumerate(&base, None);
        let portable = catalog.get("repo/portable").unwrap();
        assert_eq!(portable.description, description);
        assert!(portable.disable_model_invocation);
        assert_eq!(portable.attributes["license"], "MIT");
        assert_eq!(portable.attributes["metadata"]["author"], "example");
        assert_eq!(portable.attributes["allowed-tools"], "Bash(*)");
        let diagnostics = catalog.doctor_lines().join("\n");
        assert!(diagnostics.contains("skill manual-only (repo): portable"));
        assert!(diagnostics.contains("\"metadata\""));
        assert!(!diagnostics.contains("Bash(*)"));
        assert_eq!(
            read(portable, None).unwrap(),
            "Read references/nested/guide.md.\n"
        );
        assert_eq!(
            read(portable, Some("references/nested/guide.md")).unwrap(),
            "Nested guidance.\n"
        );
        for path in [
            "../secret",
            "/etc/passwd",
            "references/../SKILL.md",
            "C:/secret",
            "references\\guide.md",
            "references//guide.md",
            ".. /secret",
            "references./guide.md",
            "references/guide.md ",
        ] {
            assert!(
                read(portable, Some(path))
                    .unwrap_err()
                    .contains("relative path"),
                "{path}"
            );
        }
        fs::write(
            support.join("large.md"),
            vec![b'x'; MAX_FILE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(read(portable, Some("references/nested/large.md"))
            .unwrap_err()
            .contains("cap"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&support, repo.join("portable/linked")).unwrap();
            assert!(read(portable, Some("linked/guide.md"))
                .unwrap_err()
                .contains("symbolic link"));
        }
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn legacy_repo_skills_are_ignored_with_migration_guidance() {
        let base = crate::fixture_directory::create_unique_fixture_directory("legacy-skills");
        fs::create_dir_all(base.join(".ahi/skills/old")).unwrap();
        fs::write(
            base.join(".ahi/skills/old/SKILL.md"),
            "---\nname: old\ndescription: legacy\n---\n",
        )
        .unwrap();
        assert!(enumerate(&base, None).skills.is_empty());
        assert_eq!(
            doctor_lines(&base, None).last().unwrap(),
            "skills: .ahi/skills is deprecated and ignored; move skills to .agents/skills"
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn the_skills_root_is_protected_from_writes() {
        assert!(protected_write(".agents"));
        assert!(protected_write(".agents/skills"));
        assert!(protected_write(".agents/skills/x/SKILL.md"));
        assert!(!protected_write(".agents/other.txt"));
        assert!(protected_write(".ahi"));
        assert!(protected_write(".ahi/skills"));
        assert!(protected_write(".ahi/skills/x/SKILL.md"));
        assert!(!protected_write(".ahi/other.txt"));
        assert!(!protected_write("src/.ahi/skills/x"));
        assert!(!protected_write("ahi/skills"));
    }
}
