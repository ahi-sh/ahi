//! `ahi tools`: write a shipped tool policy into the working directory and
//! describe the one that is there. The chat screen never writes
//! `ahi.tools.yaml`; this command is the operator's pen, run before any
//! model is involved, and it follows `ahi precommit migrate`: print first, write only
//! with `--write`, replace only with `--write --force`.
//!
//! The examples are the files under `examples/tools`, embedded at build
//! time so a `cargo install` has them. A test keeps the two in step.

use std::path::Path;

use crate::tool_policy::examples::{example, Example, DEFAULT_EXAMPLE, EXAMPLES};
use crate::tools::BUILTIN;
use std::process::ExitCode;

const DENY_BG: (u8, u8, u8) = (90, 35, 35);
const APPROVE_BG: (u8, u8, u8) = (35, 85, 45);
const ASK_BG: (u8, u8, u8) = (55, 55, 65);
use crate::error::Result;
use crate::tool_policy::{self, Effect};

/// The policy file name, `ahi.tools.yaml`.
pub const FILE_NAME: &str = crate::tool_policy::FILE_NAME;

/// `ahi tools examples list`: one line per example, name then summary,
/// and the commands that show them.
pub fn render_index() -> String {
    let width = EXAMPLES.iter().map(|e| e.name.len()).max().unwrap_or(0);
    let mut out = String::from(
        "Shipped tool policies (examples/tools, embedded in ahi). Each is a complete ahi.tools.yaml.\n\n",
    );
    for e in EXAMPLES {
        let default = if e.name == DEFAULT_EXAMPLE {
            " (default)"
        } else {
            ""
        };
        out.push_str(&format!("{:width$}  {}{default}\n", e.name, e.summary));
    }
    out.push_str(
        "\nEvery builtin tool works inside the directory `ahi tui` starts in: paths are \
         relative to it, and absolute paths, `..`, and symbolic links are refused.\n\
         This is not a sandbox. A program approved by a command rule runs on the host \
         with the operator's access: it can follow symbolic links, read files above the \
         working directory, and run other programs. Approve commands accordingly, or \
         run `ahi tui` inside `ahi dev run` for a container boundary.\n",
    );
    out.push_str(
        "\n`ahi tools examples show <name>` prints one.\n\
         `ahi tools examples show` prints all.\n\
         `ahi tools init --example <name> --write` copies one into the working directory.\n",
    );
    out
}

/// How `init` paints a rule, by its outcome for the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Approve,
    Ask,
    Deny,
}

impl Outcome {
    fn background(self) -> (u8, u8, u8) {
        match self {
            Outcome::Approve => APPROVE_BG,
            Outcome::Ask => ASK_BG,
            Outcome::Deny => DENY_BG,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Outcome::Approve => "approve",
            Outcome::Ask => "ask",
            Outcome::Deny => "deny",
        }
    }
}

/// Whether `line` starts a rule in a `rules:` list.
fn starts_rule(line: &str) -> bool {
    line.trim_start().starts_with("- id:")
}

/// Whether `line` continues the rule begun above it: deeper indentation,
/// not a comment at the list's own indentation.
fn continues_rule(line: &str) -> bool {
    let indent = line.len() - line.trim_start().len();
    !line.trim().is_empty() && indent >= 4 && !line.trim_start().starts_with('#')
}

/// Read a rule's outcome from its lines. A rule the parser would reject
/// still gets a color here; the parse is the chat screen's job.
fn outcome(block: &[&str]) -> Outcome {
    let has = |key: &str| {
        block
            .iter()
            .any(|line| line.trim_start().trim_start_matches("- ").starts_with(key))
    };
    if has("effect: deny") {
        Outcome::Deny
    } else if has("ask: true") {
        Outcome::Ask
    } else {
        Outcome::Approve
    }
}

fn paint(line: &str, width: usize, (r, g, b): (u8, u8, u8)) -> String {
    format!("\x1b[48;2;{r};{g};{b}m{line:<width$}\x1b[0m")
}

/// The policy text for `init`. With `color`, every rule's lines carry a
/// background the transcript uses for the same outcome (deny like a removed
/// diff line, approve like an added one, ask in gray), padded to the widest
/// line so each rule reads as one block, and a legend follows. Comments and
/// the header keep the terminal's colors. Without `color` the text is the
/// file exactly.
pub fn render_policy(text: &str, color: bool) -> String {
    if !color {
        return text.to_string();
    }
    let lines: Vec<&str> = text.lines().collect();
    // Pad to the widest line so a rule reads as one block, but never to the
    // terminal's last column: a painted line that reaches the edge wraps and
    // shows as a blank row. Cut lines wider than the terminal are left as
    // they are; the terminal wraps those either way.
    let widest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let width = match crossterm::terminal::size() {
        Ok((columns, _)) if columns > 1 => widest.min(usize::from(columns) - 1),
        _ => widest,
    };
    let mut out = String::new();
    let mut index = 0;
    while index < lines.len() {
        if !starts_rule(lines[index]) {
            out.push_str(lines[index]);
            out.push('\n');
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < lines.len() && continues_rule(lines[index]) {
            index += 1;
        }
        let block = &lines[start..index];
        let background = outcome(block).background();
        for line in block {
            out.push_str(&paint(line, width, background));
            out.push('\n');
        }
    }
    out.push('\n');
    let legend: Vec<String> = [Outcome::Approve, Outcome::Ask, Outcome::Deny]
        .iter()
        .map(|o| paint(&format!(" {} ", o.label()), 0, o.background()))
        .collect();
    out.push_str(&format!(
        "{}  the model gets the call, is asked in chat first, or is refused\n",
        legend.join(" ")
    ));
    out
}

/// `ahi tools examples show [<name>]`: every example with a heading, or
/// one when `name` is given.
pub fn render_examples(name: Option<&str>, color: bool) -> std::result::Result<String, String> {
    let chosen: Vec<&Example> = match name {
        Some(name) => vec![example(name)?],
        None => EXAMPLES.iter().collect(),
    };
    let mut out = String::new();
    for (index, e) in chosen.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format!("== {} (examples/tools/{}.yaml)\n", e.name, e.name));
        out.push_str(&render_policy(e.text, color));
    }
    Ok(out)
}

/// `ahi tools list`: the working directory's `ahi.tools.yaml`, painted by
/// outcome, followed by the doctor summary. Without the file, the doctor
/// line alone, which names `ahi tools init`. An invalid file is still
/// shown, with its parse error after it, so the operator sees what to fix.
pub fn render_current(root: &Path, color: bool) -> String {
    let path = root.join(FILE_NAME);
    let mut out = String::new();
    if let Ok(text) = std::fs::read_to_string(&path) {
        out.push_str(&format!("== {}\n", path.display()));
        out.push_str(&render_policy(&text, color));
        out.push('\n');
    }
    for line in doctor_lines(root) {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// What the policy in `root` approves, for `ahi doctor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub approve: usize,
    pub ask: usize,
    pub deny: usize,
    /// Builtin tools at least one rule could approve, in rule order.
    pub advertised: Vec<String>,
    /// Skill names at least one rule could approve, in rule order.
    pub skills: Vec<String>,
}

/// Read `root/ahi.tools.yaml`. `Ok(None)` when it is absent; an error when
/// it is present and invalid, or when the retired name is present.
pub fn summary(root: &Path) -> Result<Option<Summary>> {
    let Some(policy) = tool_policy::load(root, &BUILTIN)? else {
        return Ok(None);
    };
    let mut out = Summary {
        approve: 0,
        ask: 0,
        deny: 0,
        advertised: policy.advertised_tools(),
        skills: policy.approved_skills(),
    };
    for rule in &policy.rules {
        match (rule.effect, rule.ask) {
            (Effect::Deny, _) => out.deny += 1,
            (Effect::Approve, true) => out.ask += 1,
            (Effect::Approve, false) => out.approve += 1,
        }
    }
    Ok(Some(out))
}

/// Doctor lines for `root`, the same text whether or not a file exists.
pub fn doctor_lines(root: &Path) -> Vec<String> {
    let mut lines = policy_lines(root);
    lines.push(delegate_via_line());
    lines
}

/// The connect command `delegate` would run, parsed the way it will be.
fn delegate_via_line() -> String {
    let Some(template) = crate::tools::delegate::via_template() else {
        return "delegate via: unconfigured; set AHI_DELEGATE_VIA to an ahi session-protocol connector (for example, ssh {worker} ahi connect); hc connect speaks a different protocol".into();
    };
    let source = "AHI_DELEGATE_VIA";
    match crate::service::via::split(&template) {
        Ok(argv) => format!(
            "delegate via: {} ({source}; `{{worker}}` is the label)",
            argv.join(" ")
        ),
        Err(err) => format!("delegate via: {err} ({source}: {template})"),
    }
}

fn policy_lines(root: &Path) -> Vec<String> {
    let path = root.join(FILE_NAME);
    match summary(root) {
        Ok(Some(summary)) => {
            let advertised = if summary.advertised.is_empty() {
                "none (no approve rule)".to_string()
            } else {
                summary.advertised.join(", ")
            };
            let skills = if summary.skills.is_empty() {
                "none (no skill rule)".to_string()
            } else {
                summary.skills.join(", ")
            };
            vec![
                format!("tools policy: {}", path.display()),
                format!(
                    "tool rules: {} approve, {} ask, {} deny",
                    summary.approve, summary.ask, summary.deny
                ),
                format!("tools advertised: {advertised}"),
                format!("skills approved: {skills}"),
            ]
        }
        Ok(None) => vec![
            "tools policy: absent (tools off; `ahi tools init --write` creates a read-only one)"
                .to_string(),
        ],
        Err(err) => vec![format!("tools policy: {err}")],
    }
}

fn choose_example(
    input: &mut impl std::io::BufRead,
    output: &mut impl std::io::Write,
) -> std::io::Result<Option<&'static Example>> {
    writeln!(
        output,
        "Choose a tool policy (nothing is written without --write):"
    )?;
    for (index, mode) in EXAMPLES.iter().enumerate() {
        writeln!(output, "  {}. {} — {}", index + 1, mode.name, mode.summary)?;
    }
    write!(output, "Mode name or number [read-all], q to cancel: ")?;
    output.flush()?;
    let mut answer = String::new();
    if input.read_line(&mut answer)? == 0 || answer.trim() == "q" {
        return Ok(None);
    }
    let answer = answer.trim();
    let selected = if answer.is_empty() {
        example(DEFAULT_EXAMPLE).ok()
    } else if let Ok(number) = answer.parse::<usize>() {
        number.checked_sub(1).and_then(|index| EXAMPLES.get(index))
    } else {
        example(answer).ok()
    };
    let selected = selected.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "unknown mode; use a listed name or number",
        )
    })?;
    if selected.name == "permissive" {
        writeln!(output, "WARNING: shell commands run without approval with your host account's access, outside builtin file protections. This is not a sandbox.")?;
        write!(
            output,
            "Type permissive to confirm, or anything else to cancel: "
        )?;
        output.flush()?;
        let mut confirmation = String::new();
        input.read_line(&mut confirmation)?;
        Ok((confirmation.trim() == "permissive").then_some(selected))
    } else {
        Ok(Some(selected))
    }
}

/// Preview an operator policy; create or replace it only with explicit flags.
pub(super) fn init(
    name: &str,
    interactive: bool,
    write: bool,
    force: bool,
    color: bool,
) -> ExitCode {
    if force && !write {
        eprintln!("error: --force does not write a file; pass --write --force to replace one");
        return ExitCode::from(2);
    }
    let selected;
    let name = if interactive {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            eprintln!("error: --interactive requires a terminal; use --example NAME instead");
            return ExitCode::from(2);
        }
        selected = match choose_example(&mut std::io::stdin().lock(), &mut std::io::stderr().lock())
        {
            Ok(Some(mode)) => mode,
            Ok(None) => {
                eprintln!("Cancelled; policy unchanged.");
                return ExitCode::from(0);
            }
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::from(2);
            }
        };
        selected.name
    } else {
        name
    };
    let example = match example(name) {
        Ok(example) => example,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };
    let path = std::path::Path::new(FILE_NAME);
    print!("{}", render_policy(example.text, color));
    println!();
    if !write {
        println!(
            "Left {} unchanged. Rerun with --example {} --write to create it; rule paths are relative to this directory.",
            path.display(),
            example.name
        );
        return ExitCode::from(0);
    }
    if path.exists() && !force {
        eprintln!(
            "error: refusing to overwrite {}; pass --write --force to replace it",
            path.display()
        );
        return ExitCode::from(1);
    }
    if let Err(err) = std::fs::write(path, example.text) {
        eprintln!("error: cannot write {}: {err}", path.display());
        return ExitCode::from(1);
    }
    println!(
        "Wrote {} from `{}`. `ahi tui` in this directory advertises the tools it approves; `ahi doctor` lists them.",
        path.display(),
        example.name
    );
    ExitCode::from(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn every_example_parses_and_matches_the_directory() {
        for e in EXAMPLES {
            tool_policy::parse(e.text, e.name, &BUILTIN)
                .unwrap_or_else(|err| panic!("{}: {err}", e.name));
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/tools");
        let mut on_disk: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter_map(|name| name.strip_suffix(".yaml").map(str::to_string))
            .filter(|name| name != "invalid-version")
            .collect();
        on_disk.sort();
        let mut embedded: Vec<String> = EXAMPLES.iter().map(|e| e.name.to_string()).collect();
        embedded.sort();
        assert_eq!(
            embedded, on_disk,
            "EXAMPLES must list every file in examples/tools"
        );
        assert!(example(DEFAULT_EXAMPLE).is_ok());
        assert!(example("nope").unwrap_err().contains("read-all"));
    }

    #[test]
    fn render_policy_paints_each_rule_by_outcome_and_leaves_comments_plain() {
        let text = example("commands").unwrap().text;
        assert_eq!(render_policy(text, false), text);
        let painted = render_policy(text, true);
        let bg = |(r, g, b): (u8, u8, u8)| format!("\x1b[48;2;{r};{g};{b}m");
        let line_with = |needle: &str| {
            painted
                .lines()
                .find(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("{needle}"))
                .to_string()
        };
        assert!(line_with("id: no-env").starts_with(&bg(DENY_BG)));
        assert!(line_with("match: {tool: read_file, path: .env}").starts_with(&bg(DENY_BG)));
        assert!(line_with("id: cargo-check").starts_with(&bg(APPROVE_BG)));
        assert!(line_with("toolchain_env: [CARGO_TARGET_DIR]").starts_with(&bg(APPROVE_BG)));
        assert!(line_with("id: cargo-other").starts_with(&bg(ASK_BG)));
        assert!(line_with("id: no-publish").starts_with(&bg(DENY_BG)));
        assert!(line_with("# Reads anywhere").starts_with("# Reads"));
        assert!(line_with("version: 1").starts_with("version: 1"));
        assert!(painted.lines().last().unwrap().contains("approve"));
        let index = render_index();
        assert!(index.starts_with("Shipped tool policies"), "{index}");
        assert!(index.contains("\nread-all "), "{index}");
        assert!(index.contains("examples show <name>"), "{index}");
        let all = render_examples(None, false).unwrap();
        assert!(all.starts_with("== read-all"), "{all}");
        for e in EXAMPLES {
            assert!(all.contains(&format!("== {} (examples/tools/{}.yaml)", e.name, e.name)));
            assert!(all.contains(e.text));
        }
        let one = render_examples(Some("deny-all"), false).unwrap();
        assert!(one.starts_with("== deny-all"), "{one}");
        assert!(!one.contains("== read-all"));
        assert!(render_examples(Some("nope"), false).is_err());
        // Section comments between rules stay unpainted; the following
        // rule still gets its own outcome.
        let text = include_str!("fixtures/tools-rendering.yaml");
        tool_policy::parse(text, "tools-rendering.yaml", &BUILTIN).unwrap();
        let root = render_policy(text, true);
        let section = root
            .lines()
            .find(|l| l.contains("Repository commands"))
            .unwrap();
        assert!(section.starts_with("  #"), "{section}");
        assert!(root
            .lines()
            .find(|l| l.contains("id: git-workflow"))
            .unwrap()
            .starts_with(&bg(ASK_BG)));
    }

    #[test]
    fn interactive_modes_default_cancel_and_confirm_host_access() {
        let select = |input: &str| {
            choose_example(&mut input.as_bytes(), &mut Vec::new())
                .unwrap()
                .map(|mode| mode.name)
        };
        assert_eq!(select("\n"), Some("read-all"));
        assert_eq!(select("1\n"), Some("read-all"));
        assert_eq!(select("deny-all\n"), Some("deny-all"));
        assert_eq!(select("q\n"), None);
        assert_eq!(select(""), None);
        assert_eq!(select("permissive\nno\n"), None);
        assert_eq!(select("permissive\n"), None);
        let mut output = Vec::new();
        let mode = choose_example(&mut "permissive\npermissive\n".as_bytes(), &mut output)
            .unwrap()
            .unwrap();
        assert_eq!(mode.name, "permissive");
        assert!(String::from_utf8(output)
            .unwrap()
            .contains("This is not a sandbox."));
        assert!(choose_example(&mut "unknown\n".as_bytes(), &mut Vec::new()).is_err());
    }

    #[test]
    fn summary_counts_rules_and_advertised_tools() {
        let dir = crate::fixture_directory::create_unique_fixture_directory("tools-summary");
        assert_eq!(summary(&dir).unwrap(), None);
        assert!(doctor_lines(&dir)[0].contains("absent"));
        fs::write(dir.join(FILE_NAME), example("commands").unwrap().text).unwrap();
        let got = summary(&dir).unwrap().unwrap();
        assert_eq!((got.approve, got.ask, got.deny), (6, 2, 3));
        assert_eq!(
            got.advertised,
            [
                "read_file",
                "list_dir",
                "find_files",
                "search_files",
                "run_command"
            ]
        );
        let lines = doctor_lines(&dir);
        assert_eq!(lines[1], "tool rules: 6 approve, 2 ask, 3 deny");
        assert_eq!(lines[3], "skills approved: none (no skill rule)");
        fs::write(dir.join(FILE_NAME), example("skills").unwrap().text).unwrap();
        let got = summary(&dir).unwrap().unwrap();
        assert_eq!(got.skills, ["repo/rust-review", "repo/release"]);
        assert!(got.advertised.iter().any(|tool| tool == "load_skill"));
        assert_eq!(
            doctor_lines(&dir)[3],
            "skills approved: repo/rust-review, repo/release"
        );
        fs::write(dir.join(FILE_NAME), example("deny-all").unwrap().text).unwrap();
        assert!(doctor_lines(&dir)[2].ends_with("none (no approve rule)"));
        fs::write(dir.join(FILE_NAME), "version: 2\n").unwrap();
        assert!(doctor_lines(&dir)[0].starts_with("tools policy: "));
        let current = render_current(&dir, false);
        assert!(current.starts_with("== "), "{current}");
        assert!(current.contains("version: 2"), "{current}");
        assert!(current.contains("tools policy: "), "{current}");
        fs::remove_file(dir.join(FILE_NAME)).unwrap();
        assert!(render_current(&dir, false).starts_with("tools policy: absent"));
        let _ = fs::remove_dir_all(&dir);
    }
}
