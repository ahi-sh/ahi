//! CLI adapter for the pre-commit-compatible runner.

use super::{directories, Cli};
use crate::precommit::audit::AuditEvent;
use crate::precommit::checks::{all_checks, builtin_by_id};
use crate::precommit::config::discover_config;
use crate::precommit::git::repo_root;
use crate::precommit::output::{render, render_hook_progress, render_progress_summary};
use crate::precommit::runner::{self, Selection};
use crate::precommit::{config, migrate, policy};
use crate::VERSION;
use std::{collections::BTreeSet, env, io::Write, process::ExitCode};

#[derive(clap::Args)]
pub(super) struct RunArgs {
    /// Run only this hook id or alias.
    #[arg(value_name = "HOOK")]
    hook: Option<String>,

    /// Check every tracked file instead of the staged set.
    #[arg(long, short = 'a', help_heading = "File selection")]
    all_files: bool,
    /// Check these paths instead of the staged set.
    #[arg(long, num_args = 1.., value_name = "PATH", help_heading = "File selection")]
    files: Vec<String>,
    /// With --to-ref, check the files changed between the two refs.
    #[arg(
        long,
        value_name = "REF",
        requires = "to_ref",
        help_heading = "File selection"
    )]
    from_ref: Option<String>,
    /// With --from-ref, check the files changed between the two refs.
    #[arg(
        long,
        value_name = "REF",
        requires = "from_ref",
        help_heading = "File selection"
    )]
    to_ref: Option<String>,
    /// Git hook stage whose hooks run.
    #[arg(
        long,
        default_value = "pre-commit",
        value_name = "STAGE",
        help_heading = "File selection"
    )]
    hook_stage: String,
    /// Commit message file, for the commit-msg and prepare-commit-msg stages.
    #[arg(long, value_name = "PATH", help_heading = "File selection")]
    commit_msg_filename: Option<String>,

    /// Output format. Text streams progress to stdout; JSON/SARIF emit only the final document.
    #[arg(long, default_value = "text", value_name = "FORMAT", value_parser = ["text", "json", "sarif"], help_heading = "Output")]
    format: String,
    /// Show the output of passing local hooks too. Failures always show output.
    #[arg(long, short = 'v', help_heading = "Output")]
    verbose: bool,

    /// Stop after the first failing hook.
    #[arg(long, help_heading = "Behavior")]
    fail_fast: bool,
    /// Rewrite files for end-of-file, trailing-whitespace, and mixed-line-ending; other checks stay report-only.
    ///
    /// A hook argument `--fix` enables the same three checks; `--fix=no` on
    /// mixed-line-ending keeps it report-only. Writes are atomic, do not follow
    /// symlinks, and skip files outside the selected set. A file with unstaged
    /// changes is left unchanged.
    #[arg(long, help_heading = "Behavior")]
    fix: bool,
}

// Canonical commands and deprecated aliases share these argument definitions.
#[derive(clap::Args)]
pub(super) struct ListChecksArgs {
    /// Output format.
    #[arg(long, default_value = "text", value_name = "FORMAT", value_parser = ["text", "json"])]
    pub(super) format: String,
}

#[derive(clap::Args)]
pub(super) struct ExplainArgs {
    /// Builtin id or alias, as `ahi precommit list-checks` prints it.
    #[arg(value_name = "ID")]
    pub(super) id: String,
}

#[derive(clap::Args)]
pub(super) struct MigrateArgs {
    /// Write the suggestion; requires --force to replace an existing file.
    #[arg(long)]
    pub(super) write: bool,
    /// With --write, replace an existing file.
    #[arg(long, requires = "write")]
    pub(super) force: bool,
}

#[derive(clap::Subcommand)]
pub(super) enum Command {
    /// Run configured checks on staged files; local commands require ahi.hooks.yaml approval.
    Run(RunArgs),
    /// List builtin checks and their aliases.
    ListChecks(ListChecksArgs),
    /// Describe a builtin check, its defaults, and the upstream hook it replaces.
    Explain(ExplainArgs),
    /// Install the Git hook shim (not implemented yet; exits 1).
    Install,
    /// Remove the Git hook shim (not implemented yet; exits 1).
    Uninstall,
    /// Preview a builtin configuration migration; write only with --write.
    ///
    /// Best effort: does not guarantee feature parity or the same file scope.
    /// Remote repositories are not fetched. This migrates configuration, not CLI invocations.
    Migrate(MigrateArgs),
}

pub(super) fn dispatch(cli: &Cli, command: &Command) -> ExitCode {
    match command {
        Command::Run(args) => run_precommit(cli, args),
        Command::ListChecks(args) => list_checks(&args.format),
        Command::Explain(args) => explain(&args.id),
        Command::Install | Command::Uninstall => {
            super::unimplemented_until("the hook shim phase (after hook execution exists)")
        }
        Command::Migrate(args) => migrate_config(cli.config.as_deref(), args.write, args.force),
    }
}

pub(super) fn warn_deprecated(command: &super::Command) {
    let (old, new) = match command {
        super::Command::Run(_) => ("run", "run"),
        super::Command::ListChecks(_) => ("list-checks", "list-checks"),
        super::Command::Explain(_) => ("explain", "explain"),
        super::Command::Migrate(_) => ("migrate", "migrate"),
        super::Command::Hooks {
            command: super::HooksCommand::Install,
        } => ("hooks install", "install"),
        super::Command::Hooks {
            command: super::HooksCommand::Uninstall,
        } => ("hooks uninstall", "uninstall"),
        _ => return,
    };
    eprintln!("warning: `ahi {old}` is deprecated; use `ahi precommit {new}` with the same arguments. See MIGRATING.md for the support window and migration steps.");
}

pub(super) fn run_precommit(cli: &Cli, args: &RunArgs) -> ExitCode {
    let modes = [
        args.all_files,
        !args.files.is_empty(),
        args.from_ref.is_some() || args.to_ref.is_some(),
    ]
    .into_iter()
    .filter(|on| *on)
    .count();
    if modes > 1 {
        eprintln!("error: use only one of --all-files, --files, or --from-ref/--to-ref");
        return ExitCode::from(2);
    }
    if args.from_ref.is_some() != args.to_ref.is_some() {
        eprintln!("error: Specify both --from-ref and --to-ref.");
        return ExitCode::from(1);
    }
    let live = args.format == "text";
    if live {
        if let Err(err) = write_run_output("ahi precommit: starting checks\n") {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    }
    let cwd = match env::current_dir() {
        Ok(cwd) => cwd,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    let root = match repo_root(&cwd) {
        Ok(root) => root,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    let config_path = match discover_config(&root, cli.config.as_deref()) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    let config = match config::load_config(&config_path) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    let selection = if args.all_files {
        Selection::All
    } else if !args.files.is_empty() {
        match relativize_files(&root, &cwd, &args.files) {
            Ok(files) => Selection::Files(files),
            Err(err) => {
                eprintln!("error: {err}");
                return ExitCode::from(1);
            }
        }
    } else if let (Some(from), Some(to)) = (&args.from_ref, &args.to_ref) {
        Selection::Range {
            from: from.clone(),
            to: to.clone(),
        }
    } else {
        Selection::Staged
    };
    let stage_fixes = matches!(selection, Selection::Staged);
    let mut skip = BTreeSet::new();
    if let Ok(raw) = env::var("SKIP") {
        for part in raw.split(',') {
            let part = part.trim();
            if !part.is_empty() {
                skip.insert(part.to_string());
            }
        }
    }
    let request = runner::RunRequest {
        root: &root,
        config: &config,
        selection,
        stage: &args.hook_stage,
        only_hook: args.hook.as_deref(),
        fail_fast: args.fail_fast,
        skip,
        commit_msg_filename: args.commit_msg_filename.as_deref(),
        fix: args.fix,
        stage_fixes,
        verbose: args.verbose,
    };
    let report = match runner::run_with_progress(&request, |event| {
        if !live {
            return Ok(());
        }
        let text = match event {
            runner::Progress::Started { id, name } => format!("Starting: {name} ({id})\n"),
            runner::Progress::Finished(hook) => render_hook_progress(hook),
        };
        write_run_output(&text)
    }) {
        Ok(report) => report,
        Err(err) => {
            if live {
                let _ = write_run_output("ahi precommit: aborted (exit code: 1)\n");
            }
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    let rendered = match if live {
        Ok(render_progress_summary(&report))
    } else {
        render(&report, &args.format)
    } {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };
    {
        let ids: Vec<String> = report.hooks.iter().map(|hook| hook.id.clone()).collect();
        let findings = report.hooks.iter().map(|hook| hook.findings.len()).sum();
        let outcome = match report.result {
            runner::Outcome::Passed => "passed",
            runner::Outcome::Failed => "failed",
            runner::Outcome::Skipped => "skipped",
        };
        let local_env: Vec<crate::precommit::audit::HookEnv<'_>> = report
            .local_env
            .iter()
            .map(|item| crate::precommit::audit::HookEnv {
                hook: &item.hook,
                names: &item.names,
            })
            .collect();
        // The log is persistent state, not cache; a missing location only warns.
        let appended = directories(cli).and_then(|dirs| {
            crate::precommit::audit::append(
                &dirs.state,
                &AuditEvent {
                    command: "run",
                    repo: &root,
                    hook_ids: &ids,
                    outcome,
                    finding_count: findings,
                    local_hooks: &report.local_hooks,
                    local_env: &local_env,
                },
            )
        });
        if let Err(err) = appended {
            eprintln!("warning: audit log not written: {err}");
        }
    }
    if let Err(err) = write_run_output(&rendered) {
        eprintln!("error: {err}");
        return ExitCode::from(1);
    }
    if report.result == runner::Outcome::Failed {
        ExitCode::from(1)
    } else {
        ExitCode::from(0)
    }
}

/// Flush each progress event even when stdout is a pipe (Git hooks and CI).
fn write_run_output(text: &str) -> crate::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(text.as_bytes())
        .and_then(|()| stdout.flush())
        .map_err(|err| crate::Error::new(format!("cannot write precommit output: {err}")))
}

pub(super) fn list_checks(format: &str) -> ExitCode {
    match format {
        "text" => {
            for check in all_checks() {
                println!("{:<32} {}", check.id(), check.name());
                for alias in check.aliases() {
                    println!("  alias: {alias}");
                }
            }
            ExitCode::from(0)
        }
        "json" => {
            let checks: Vec<_> = all_checks()
                .iter()
                .map(|check| {
                    serde_json::json!({
                        "id": check.id(),
                        "name": check.name(),
                        "aliases": check.aliases(),
                        "description": check.description(),
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&checks).unwrap_or_else(|_| "[]".into())
            );
            ExitCode::from(0)
        }
        other => {
            eprintln!("error: unknown format `{other}`");
            ExitCode::from(2)
        }
    }
}

pub(super) fn explain(id: &str) -> ExitCode {
    let Some(check) = builtin_by_id(id) else {
        eprintln!("error: no builtin check `{id}`");
        return ExitCode::from(1);
    };
    println!("{}", check.id());
    println!("{}", check.name());
    println!();
    println!("{}", check.description());
    println!();
    println!(
        "default types: {}",
        if check.default_types().is_empty() {
            "(none)".to_string()
        } else {
            check.default_types().join(", ")
        }
    );
    if !check.default_files().is_empty() {
        println!("default files: {}", check.default_files());
    }
    if !check.aliases().is_empty() {
        println!("aliases: {}", check.aliases().join(", "));
    }
    if let Some(upstream) = check.upstream() {
        println!("upstream hook: {upstream}");
    }
    if check.fixes() {
        println!(
            "fixer: report-only unless `ahi precommit run --fix` or a hook argument `--fix` is set. The rewrite is atomic, does not follow symlinks, and only touches files in the selected set. ahi refuses the rewrite when that file has unstaged changes. There is no stash."
        );
    }
    if check.default_always_run() {
        println!("always_run: true when the config omits always_run");
    }
    if !check.default_pass_filenames() {
        println!("pass_filenames: false when the config omits pass_filenames");
    }
    println!("version: ahi {VERSION}");
    println!();
    let loaded = env::current_dir().ok().and_then(|cwd| {
        repo_root(&cwd)
            .ok()
            .and_then(|root| policy::load_policy(&root).ok())
    });
    print_local_env(loaded.as_ref());
    ExitCode::from(0)
}

pub(super) fn print_local_env(policy: Option<&crate::precommit::policy::Policy>) {
    println!(
        "local hook environment (names only, if set): {}",
        crate::base_env_names().join(", ")
    );
    println!(
        "toolchain pass-through (default, if set): {}",
        crate::default_toolchain_names().join(", ")
    );
    println!(
        "toolchain pass-through adds names a tool needs to find an install that is already there. ahi does not install, update, or cache a toolchain. Credential names (TOKEN, SECRET, PASSWORD, KEY, AWS_*, AZURE_*, GITHUB_*) are refused."
    );
    let Some(policy) = policy else {
        return;
    };
    if policy.toolchain_env.is_empty() {
        println!("toolchain pass-through (policy): (none)");
    } else {
        println!(
            "toolchain pass-through (policy): {}",
            policy.toolchain_env.join(", ")
        );
    }
    for rule in &policy.rules {
        if rule.toolchain_env.is_empty() {
            continue;
        }
        println!(
            "toolchain pass-through (rule {}): {}",
            rule.id,
            rule.toolchain_env.join(", ")
        );
    }
}

pub(super) fn migrate_config(
    config: Option<&std::path::Path>,
    write: bool,
    force: bool,
) -> ExitCode {
    if force && !write {
        eprintln!("error: --force does not write a file; pass --write --force to replace one");
        return ExitCode::from(2);
    }
    let cwd = match env::current_dir() {
        Ok(cwd) => cwd,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    let root = match repo_root(&cwd) {
        Ok(root) => root,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    let path = match discover_config(&root, config) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    let original = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    let shown = path
        .strip_prefix(&cwd)
        .unwrap_or(&path)
        .display()
        .to_string();
    let suggestion = match migrate::suggest(&original, &shown) {
        Ok(suggestion) => suggestion,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(1);
        }
    };
    print!("{}", suggestion.report);
    print!("{}", suggestion.suggested);
    println!();
    if !write {
        println!("Left {shown} unchanged. Rerun with --write --force to replace it with the suggestion above.");
        return ExitCode::from(0);
    }
    if path.exists() && !force {
        eprintln!("error: refusing to overwrite {shown}; pass --write --force to replace it");
        return ExitCode::from(1);
    }
    if let Err(err) = std::fs::write(&path, &suggestion.suggested) {
        eprintln!("error: {err}");
        return ExitCode::from(1);
    }
    println!("Wrote {shown}.");
    ExitCode::from(0)
}

fn relativize_files(
    root: &std::path::Path,
    cwd: &std::path::Path,
    files: &[String],
) -> crate::Result<Vec<String>> {
    let mut out = Vec::new();
    for file in files {
        let path = std::path::Path::new(file);
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        let absolute = absolute.canonicalize().unwrap_or(absolute);
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        match absolute.strip_prefix(&root) {
            Ok(rel) => out.push(rel.to_string_lossy().replace('\\', "/")),
            Err(_) => {
                return Err(crate::Error::new(format!(
                    "path `{file}` is outside the repository"
                )));
            }
        }
    }
    Ok(out)
}
