//! Text, JSON, and SARIF 2.1.0 renderers.

use serde_json::{json, Value};

use crate::precommit::checks::all_checks;
use crate::precommit::runner::{HookReport, Outcome, Report};
use crate::VERSION;

pub fn render_text(report: &Report) -> String {
    let mut out = String::new();
    for warning in &report.warnings {
        out.push_str("warning: ");
        out.push_str(warning);
        out.push('\n');
    }
    for hook in &report.hooks {
        out.push_str(&render_hook(hook, false));
    }
    out
}

/// One completed hook for the live text CLI. Skips have no process exit code;
/// builtins and policy refusals use their equivalent runner result (0 or 1).
pub fn render_hook_progress(hook: &HookReport) -> String {
    render_hook(hook, true)
}

fn render_hook(hook: &HookReport, show_exit_code: bool) -> String {
    let mut out = String::new();
    let label = match hook.outcome {
        Outcome::Passed => "Passed",
        Outcome::Failed => "Failed",
        Outcome::Skipped => "Skipped",
    };
    out.push_str(&dot_line(&hook.name, label));
    if show_exit_code {
        match hook.outcome {
            Outcome::Skipped => out.push_str(" (not run)"),
            Outcome::Passed => out.push_str(" (exit code: 0)"),
            Outcome::Failed => {
                out.push_str(&format!(" (exit code: {})", hook.exit_code.unwrap_or(1)))
            }
        }
    }
    out.push('\n');
    if hook.outcome == Outcome::Failed {
        let code = hook.exit_code.unwrap_or(1);
        out.push_str(&format!("- hook id: {}\n- exit code: {code}\n\n", hook.id));
        for finding in &hook.findings {
            out.push_str(&format_finding(
                finding.path.as_deref(),
                finding.line,
                &finding.message,
            ));
            out.push('\n');
        }
        if let Some(detail) = &hook.detail {
            push_detail(&mut out, detail);
        }
    } else if let Some(detail) = &hook.detail {
        push_detail(&mut out, detail);
    } else if let Some(reason) = &hook.skip_reason {
        if reason.starts_with('(') {
            out.push_str(reason);
            out.push('\n');
        }
    }
    out
}

/// Final text summary without repeating hooks already presented live.
pub fn render_progress_summary(report: &Report) -> String {
    let mut out = String::new();
    for warning in &report.warnings {
        out.push_str(&format!("warning: {warning}\n"));
    }
    let count = |outcome| {
        report
            .hooks
            .iter()
            .filter(|hook| hook.outcome == outcome)
            .count()
    };
    let (label, code) = if report.result == Outcome::Failed {
        ("Failed", 1)
    } else {
        ("Passed", 0)
    };
    out.push_str(&format!(
        "ahi precommit: {label} ({} passed, {} failed, {} skipped; exit code: {code})\n",
        count(Outcome::Passed),
        count(Outcome::Failed),
        count(Outcome::Skipped)
    ));
    out
}

pub fn render_json(report: &Report) -> String {
    let hooks: Vec<Value> = report
        .hooks
        .iter()
        .map(|hook| {
            let mut value = json!({
                "id": hook.id,
                "name": hook.name,
                "outcome": outcome_name(hook.outcome),
                "findings": hook.findings.iter().map(|finding| {
                    json!({
                        "path": finding.path,
                        "line": finding.line,
                        "column": finding.column,
                        "severity": "error",
                        "message": finding.message,
                    })
                }).collect::<Vec<_>>(),
            });
            if let Some(detail) = &hook.detail {
                value["output"] = json!(detail);
            }
            value
        })
        .collect();
    let mut value = json!({
        "ahi_version": VERSION,
        "result": outcome_name(report.result),
        "warnings": report.warnings,
        "hooks": hooks,
    });
    value.sort_all_objects();
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()) + "\n"
}

pub fn render_sarif(report: &Report) -> String {
    let rules: Vec<Value> = all_checks()
        .iter()
        .map(|check| {
            json!({
                "id": check.id(),
                "shortDescription": {"text": check.name()},
                "fullDescription": {"text": check.description()},
            })
        })
        .collect();
    let mut results = Vec::new();
    for hook in &report.hooks {
        for finding in &hook.findings {
            let mut result = json!({
                "ruleId": hook.id,
                "level": "error",
                "message": {"text": finding.message},
            });
            if let Some(path) = &finding.path {
                let mut region = json!({});
                if let Some(line) = finding.line {
                    region = json!({"startLine": line});
                }
                result["locations"] = json!([{
                    "physicalLocation": {
                        "artifactLocation": {"uri": path},
                        "region": region,
                    }
                }]);
            }
            results.push(result);
        }
    }
    let mut value = json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "ahi",
                    "version": VERSION,
                    "rules": rules,
                }
            },
            "results": results,
        }]
    });
    value.sort_all_objects();
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()) + "\n"
}

pub fn render(report: &Report, format: &str) -> Result<String, String> {
    match format {
        "text" => Ok(render_text(report)),
        "json" => Ok(render_json(report)),
        "sarif" => Ok(render_sarif(report)),
        other => Err(format!(
            "unknown format `{other}` (expected text, json, or sarif)"
        )),
    }
}

fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Passed => "passed",
        Outcome::Failed => "failed",
        Outcome::Skipped => "skipped",
    }
}

fn dot_line(name: &str, label: &str) -> String {
    let width = 79usize;
    let dots = width.saturating_sub(name.len() + label.len()).max(1);
    format!("{name}{}{label}", ".".repeat(dots))
}

fn push_detail(out: &mut String, detail: &str) {
    out.push_str(detail);
    if !detail.ends_with('\n') {
        out.push('\n');
    }
}

fn format_finding(path: Option<&str>, line: Option<u32>, message: &str) -> String {
    match (path, line) {
        (Some(path), Some(line)) => format!("{path}:{line}: {message}"),
        (Some(path), None) => format!("{path}: {message}"),
        (None, _) => message.to_string(),
    }
}
