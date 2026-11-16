//! Real piped CLI output must arrive while hooks are still blocked, not merely
//! appear in the right order after the process exits.
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[path = "support/fixture_directory.rs"]
mod fixture_directory;

struct Repo(PathBuf);
impl Repo {
    fn new() -> Self {
        let dir = fixture_directory::create_unique_fixture_directory("precommit-progress");
        assert!(Command::new("git")
            .args(["init", "-q", "--template="])
            .arg(&dir)
            .status()
            .unwrap()
            .success());
        Self(dir)
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ahi"));
        command
            .current_dir(&self.0)
            .env_remove("SKIP")
            .arg("--state-dir")
            .arg(self.0.join("state"));
        command
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "hook did not reach {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn stdout_is_flushed_before_and_between_hooks_with_real_exit_codes() {
    let repo = Repo::new();
    let exe = repo
        .0
        .join(format!("wait-hook{}", std::env::consts::EXE_SUFFIX));
    let compiled = Command::new("rustc")
        .args([
            "--edition=2021",
            "tests/support/precommit_wait_hook.rs",
            "-o",
        ])
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    // Entry is interpreted as argv without a shell, then matched exactly by policy.
    let program = exe.to_str().unwrap().replace('\\', "/");
    let first = format!("\"{program}\" first 0");
    let second = format!("\"{program}\" second 7");
    let quote = |s: &str| serde_json::to_string(s).unwrap(); // JSON strings are YAML scalars.
    fs::write(repo.0.join(".pre-commit-config.yaml"), format!("repos:\n- repo: builtin\n  hooks:\n  - id: ahi/check-json\n    files: ^never$\n- repo: local\n  hooks:\n  - id: first\n    name: First\n    language: system\n    entry: {}\n    always_run: true\n    pass_filenames: false\n    verbose: true\n  - id: second\n    name: Second\n    language: system\n    entry: {}\n    always_run: true\n    pass_filenames: false\n  - id: later\n    name: Later\n    language: system\n    entry: {}\n    always_run: true\n    pass_filenames: false\n", quote(&first), quote(&second), quote(&first))).unwrap();
    fs::write(repo.0.join("ahi.hooks.yaml"), format!("version: 1\nrules:\n- id: first\n  action: hook\n  match: {{id: first, entry: {}}}\n  effect: approve\n- id: second\n  action: hook\n  match: {{id: second, entry: {}}}\n  effect: approve\n",quote(&first),quote(&second))).unwrap();
    let mut child = Running(
        repo.command()
            .args(["precommit", "run", "--all-files", "--fail-fast"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let stdout = child.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line.unwrap()).is_err() {
                break;
            }
        }
    });
    let next = || {
        rx.recv_timeout(Duration::from_secs(10))
            .expect("progress must arrive before the hook is released")
    };
    assert_eq!(next(), "ahi precommit: starting checks");
    assert_eq!(next(), "Starting: Check JSON (ahi/check-json)");
    assert!(next().ends_with("Skipped (not run)"));
    assert_eq!(next(), "(no files to check)");
    assert_eq!(next(), "Starting: First (first)");
    wait_for(&repo.0.join("first.entered"));
    assert!(child.0.try_wait().unwrap().is_none());
    fs::write(repo.0.join("first.release"), b"go").unwrap();
    assert!(next().ends_with("Passed (exit code: 0)"));
    assert_eq!(next(), "first hook output");
    assert_eq!(next(), "Starting: Second (second)");
    wait_for(&repo.0.join("second.entered"));
    assert!(child.0.try_wait().unwrap().is_none());
    fs::write(repo.0.join("second.release"), b"go").unwrap();
    assert!(next().ends_with("Failed (exit code: 7)"));
    let mut rest = Vec::new();
    while let Ok(line) = rx.recv_timeout(Duration::from_secs(10)) {
        rest.push(line);
    }
    let status = child.0.wait().unwrap();
    let mut stderr = String::new();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert_eq!(status.code(), Some(1), "{stderr}");
    reader.join().unwrap();
    assert!(rest.iter().any(|line| line == "second hook output"));
    assert_eq!(
        rest.last().unwrap(),
        "ahi precommit: Failed (1 passed, 1 failed, 1 skipped; exit code: 1)"
    );
    assert!(!rest.iter().any(|line| line.contains("Later")));
}

#[test]
fn empty_run_finishes_and_machine_formats_remain_single_documents() {
    let repo = Repo::new();
    fs::write(repo.0.join(".pre-commit-config.yaml"), "repos: []\n").unwrap();
    let text = repo
        .command()
        .args(["precommit", "run", "--all-files"])
        .output()
        .unwrap();
    assert!(text.status.success());
    assert_eq!(String::from_utf8(text.stdout).unwrap(), "ahi precommit: starting checks\nahi precommit: Passed (0 passed, 0 failed, 0 skipped; exit code: 0)\n");
    for format in ["json", "sarif"] {
        let output = repo
            .command()
            .args(["precommit", "run", "--all-files", "--format", format])
            .output()
            .unwrap();
        assert!(output.status.success());
        let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        if format == "json" {
            assert_eq!(doc["result"], "passed");
        } else {
            assert_eq!(doc["version"], "2.1.0");
        }
    }
}
