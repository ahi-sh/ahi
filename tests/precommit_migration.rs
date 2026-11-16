//! Public command migration: old spellings warn, but use the canonical runner.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "support/fixture_directory.rs"]
mod fixture_directory;

const CONFIG: &str = "repos:\n- repo: builtin\n  hooks:\n  - id: ahi/trailing-whitespace\n";

struct Repo(PathBuf);

impl Repo {
    fn new() -> Self {
        let root = fixture_directory::create_unique_fixture_directory("precommit-migration");
        let output = Command::new("git")
            .args(["init", "-q", "--template="])
            .arg(&root)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        fs::write(root.join(".pre-commit-config.yaml"), CONFIG).unwrap();
        fs::write(root.join("notes.txt"), "hello \n").unwrap();
        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["add", "notes.txt", ".pre-commit-config.yaml"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        Self(root)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ahi"))
            .arg("--state-dir")
            .arg(self.0.join("state"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }

    fn compare(
        &self,
        old: &[&str],
        new: &[&str],
        old_name: &str,
        new_name: &str,
        code: i32,
    ) -> Output {
        let legacy = self.run(old);
        let canonical = self.run(new);
        assert_eq!(legacy.status.code(), Some(code), "{legacy:?}");
        assert_eq!(canonical.status.code(), Some(code), "{canonical:?}");
        assert_eq!(legacy.stdout, canonical.stdout);
        let warning = format!(
            "warning: `ahi {old_name}` is deprecated; use `ahi precommit {new_name}` with the same arguments. See MIGRATING.md for the support window and migration steps.\n"
        );
        assert_eq!(
            legacy.stderr,
            [warning.as_bytes(), canonical.stderr.as_slice()].concat()
        );
        assert!(!String::from_utf8_lossy(&canonical.stderr).contains("deprecated"));
        canonical
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn aliases_preserve_reports_failures_and_global_options() {
    let repo = Repo::new();
    let result = repo.compare(
        &[
            "--config",
            ".pre-commit-config.yaml",
            "run",
            "--files",
            "notes.txt",
            "--format",
            "json",
        ],
        &[
            "precommit",
            "run",
            "--config",
            ".pre-commit-config.yaml",
            "--files",
            "notes.txt",
            "--format",
            "json",
        ],
        "run",
        "run",
        1,
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        report["hooks"],
        serde_json::json!([{
            "id": "ahi/trailing-whitespace",
            "name": "Trim Trailing Whitespace",
            "outcome": "failed",
            "findings": [{"path": "notes.txt", "line": 1, "column": null,
                          "message": "trailing whitespace", "severity": "error"}]
        }])
    );
    assert_eq!(fs::read(repo.0.join("notes.txt")).unwrap(), b"hello \n");

    let listed = repo.compare(
        &["list-checks", "--format", "json"],
        &["precommit", "list-checks", "--format", "json"],
        "list-checks",
        "list-checks",
        0,
    );
    let checks: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(checks[0]["id"], "ahi/trailing-whitespace");

    let explained = repo.compare(
        &["explain", "missing-check"],
        &["precommit", "explain", "missing-check"],
        "explain",
        "explain",
        1,
    );
    assert_eq!(
        explained.stderr,
        b"error: no builtin check `missing-check`\n"
    );

    for command in ["install", "uninstall"] {
        let result = repo.compare(
            &["hooks", command],
            &["precommit", command],
            &format!("hooks {command}"),
            command,
            1,
        );
        assert!(result.stdout.is_empty());
        assert!(String::from_utf8_lossy(&result.stderr).contains("not implemented"));
    }
}

#[test]
fn aliases_preserve_explicit_write_and_preview_semantics() {
    let repo = Repo::new();
    let preview = repo.compare(
        &["migrate"],
        &["precommit", "migrate"],
        "migrate",
        "migrate",
        0,
    );
    // Git and the OS can spell the same directory differently (notably on
    // Windows); the CLI then displays the full path rather than a relative one.
    assert!(
        String::from_utf8_lossy(&preview.stdout).lines().any(|line| {
            line.starts_with("Left ")
                && line.ends_with(".pre-commit-config.yaml unchanged. Rerun with --write --force to replace it with the suggestion above.")
        }),
        "{preview:?}"
    );
    assert_eq!(
        fs::read_to_string(repo.0.join(".pre-commit-config.yaml")).unwrap(),
        CONFIG
    );
    let refused = repo.compare(
        &["migrate", "--write"],
        &["precommit", "migrate", "--write"],
        "migrate",
        "migrate",
        1,
    );
    assert!(String::from_utf8_lossy(&refused.stderr).contains("refusing to overwrite"));
    assert_eq!(
        fs::read_to_string(repo.0.join(".pre-commit-config.yaml")).unwrap(),
        CONFIG
    );

    let mut fixed_outputs = Vec::new();
    for args in [
        vec!["run", "--files", "notes.txt", "--fix"],
        vec!["precommit", "run", "--files", "notes.txt", "--fix"],
    ] {
        fs::write(repo.0.join("notes.txt"), "hello \n").unwrap();
        let result = repo.run(&args);
        assert_eq!(result.status.code(), Some(1), "{result:?}");
        assert_eq!(fs::read(repo.0.join("notes.txt")).unwrap(), b"hello\n");
        fixed_outputs.push(result.stdout);
    }
    assert_eq!(fixed_outputs[0], fixed_outputs[1]);

    for args in [
        vec!["run", "--files", "notes.txt"],
        vec!["precommit", "run", "--files", "notes.txt"],
    ] {
        assert_eq!(repo.run(&args).status.code(), Some(0));
    }
    for args in [
        vec!["migrate", "--force"],
        vec!["precommit", "migrate", "--force"],
    ] {
        assert_eq!(repo.run(&args).status.code(), Some(2));
    }
}

#[cfg(unix)]
#[test]
fn shipped_git_shim_runs_the_canonical_command_and_preserves_failure() {
    use std::os::unix::fs::PermissionsExt;

    let repo = Repo::new();
    let hook = repo.0.join(".git/hooks/pre-commit");
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(&hook, include_str!("../githooks/pre-commit")).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let mut paths = vec![Path::new(env!("CARGO_BIN_EXE_ahi"))
        .parent()
        .unwrap()
        .to_path_buf()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(paths).unwrap();
    let commit = || {
        Command::new("git")
            .arg("-C")
            .arg(&repo.0)
            .args([
                "-c",
                "core.hooksPath=.git/hooks",
                "-c",
                "user.name=ahi",
                "-c",
                "user.email=ahi@example.com",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "fixture",
            ])
            .env("PATH", &path)
            .env("AHI_STATE_DIR", repo.0.join("state"))
            .output()
            .unwrap()
    };

    let failed = commit();
    assert!(!failed.status.success(), "{failed:?}");
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("trailing whitespace"),
        "{failed:?}"
    );
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("deprecated"));

    fs::write(repo.0.join("notes.txt"), "hello\n").unwrap();
    let staged = Command::new("git")
        .arg("-C")
        .arg(&repo.0)
        .args(["add", "notes.txt"])
        .output()
        .unwrap();
    assert!(staged.status.success());
    let passed = commit();
    assert!(passed.status.success(), "{passed:?}");
    assert!(!String::from_utf8_lossy(&passed.stderr).contains("deprecated"));
}

#[test]
fn canonical_help_is_available_without_configuration_or_side_effects() {
    let root = fixture_directory::create_unique_fixture_directory("precommit-help");
    for args in [
        vec!["--help"],
        vec!["precommit", "--help"],
        vec!["precommit", "run", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_ahi"))
            .args(&args)
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains("precommit"));
    }
    assert_eq!(fs::read_dir(Path::new(&root)).unwrap().count(), 0);
    fs::remove_dir_all(root).unwrap();
}
