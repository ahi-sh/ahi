//! CLI contracts run without Rex/hc on PATH and never contact external services.
use serde_json::{json, Value};
use std::process::{Command, Output};
const SESSION: &str = "session:11111111-1111-4111-8111-111111111111";
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ahi"))
        .env("PATH", "")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn help_and_preview_need_no_backend_and_keep_json_stdout() {
    let help = run(&["terminal-host", "--help"]);
    assert!(
        help.status.success(),
        "{}",
        String::from_utf8_lossy(&help.stderr)
    );
    assert!(String::from_utf8_lossy(&help.stdout).contains("--dry-run"));
    let preview = run(&[
        "terminal-host",
        "--server",
        "work-wsl",
        "--host-session",
        SESSION,
        "--dry-run",
        "start",
        "--cwd",
        "/workspace",
        "--",
        "/usr/bin/env",
        "literal space; $HOME",
    ]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    assert!(preview.stderr.is_empty());
    let value: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(
        value["target"],
        json!({"backend":"rex", "server":"work-wsl", "session":SESSION})
    );
    assert_eq!(
        value["calls"][0]["stdin"]["layout"]["block"]["options"]["command"],
        json!(["/usr/bin/env", "literal space; $HOME"])
    );
}

#[test]
fn hc_preview_keeps_windows_argv_literal_and_target_explicit() {
    let preview = run(&[
        "terminal-host",
        "--backend",
        "hc",
        "--host-session",
        "work-core",
        "--machine",
        "work-windows",
        "--dry-run",
        "start",
        "--cwd",
        "C:\\work",
        "--",
        "C:\\Program Files\\Example\\app.exe",
    ]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let value: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(
        value["calls"][0]["stdin"]["params"]["argv"],
        json!(["C:\\Program Files\\Example\\app.exe"])
    );
    assert_eq!(value["calls"][0]["stdin"]["params"]["on"], "work-windows");
}

#[test]
fn unsupported_input_and_invalid_target_fail_before_launch() {
    let result = run(&[
        "terminal-host",
        "--backend",
        "hc",
        "--host-session",
        "core",
        "text",
        "t7",
        "draft",
    ]);
    assert_eq!(result.status.code(), Some(3));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("submits with Enter"));
    let result = run(&[
        "terminal-host",
        "--host-session",
        SESSION,
        "inspect",
        "logs",
    ]);
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--server"));
    let result = run(&["terminal-host", "--server", "work", "inspect", "logs"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--host-session"));
}

#[test]
fn missing_backend_is_not_silently_replaced() {
    let result = run(&[
        "terminal-host",
        "--server",
        "work",
        "--host-session",
        SESSION,
        "start",
        "--cwd",
        "/workspace",
        "--",
        "/usr/bin/env",
    ]);
    assert_eq!(result.status.code(), Some(4));
    assert!(result.stdout.is_empty());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(error.contains("could not be started"));
    assert!(!error.contains("effects are uncertain"));
}
