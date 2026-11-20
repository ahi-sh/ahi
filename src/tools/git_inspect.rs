//! Fixed, read-only Git operations. libgit2 reads the repository directly:
//! no shell, git executable, pager, hooks, external filters or textconv command.
use super::*;
use git2::{DiffFormat, DiffOptions, Repository, StatusOptions};
use std::sync::atomic::Ordering;

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Operation {
    Status {},
    Diff {
        #[serde(default)]
        staged: bool,
    },
    Log {
        #[serde(default = "default_count")]
        count: usize,
    },
}
fn default_count() -> usize {
    10
}
pub(super) fn schema() -> ToolSchema {
    ToolSchema{name:"git_inspect",description:"Inspect this workspace repository without running commands. status lists branch and paths; diff shows staged or unstaged changes; log lists recent first-parent commits. Optional path is a literal workspace file or directory, not a glob.".into(), parameters:json!({"type":"object","additionalProperties":false,"required":["operation"],"properties":{"operation":{"type":"string","enum":["status","diff","log"]},"path":{"type":"string"},"staged":{"type":"boolean","description":"diff only"},"count":{"type":"integer","minimum":1,"maximum":50,"description":"log only"}}})}
}
pub(super) fn prepare(call: &ToolCall) -> Result<PreparedCall, &'static str> {
    let mut raw = call.arguments.clone();
    let path = match raw.remove("path") {
        None => String::new(),
        Some(Value::String(s)) => normalize_relative_path(&s).ok_or(INVALID)?,
        _ => return Err(INVALID),
    };
    let op = serde_json::from_value(Value::Object(raw)).map_err(|_| INVALID)?;
    if matches!(op, Operation::Log{count} if !(1..=50).contains(&count)) {
        return Err(INVALID);
    }
    Ok(PreparedCall {
        tool: "git_inspect",
        path,
        arguments: Arguments::Inspect(op),
    })
}
const CAP: usize = 32768;
pub(super) fn run(
    ws: &Workspace,
    path: &str,
    op: &Operation,
    cancel: &AtomicBool,
) -> Result<String, &'static str> {
    if ws.overlaps_state(path) {
        return Err("Git inspection overlaps ahi's private directories");
    }
    let repo = Repository::open(ws.root()).map_err(|_| "workspace is not a Git working tree")?;
    if repo
        .workdir()
        .and_then(|p| p.canonicalize().ok())
        .as_deref()
        != Some(ws.root())
    {
        return Err("Git working tree must be the workspace root");
    }
    let mut out = Vec::new();
    let mut cut = false;
    let mut append = |s: &[u8]| {
        let n = s.len().min(CAP - out.len());
        out.extend_from_slice(&s[..n]);
        cut |= n != s.len();
        !cut && !cancel.load(Ordering::Relaxed)
    };
    match op {
        Operation::Status {} => {
            let mut opts = StatusOptions::new();
            opts.no_refresh(true)
                .update_index(false)
                .include_untracked(true)
                .exclude_submodules(true)
                .disable_pathspec_match(true);
            if !path.is_empty() {
                opts.pathspec(path);
            }
            let head = repo.head().ok();
            append(
                format!(
                    "branch {}\n",
                    head.as_ref()
                        .and_then(|h| h.shorthand())
                        .unwrap_or("(unborn)")
                )
                .as_bytes(),
            );
            for entry in repo
                .statuses(Some(&mut opts))
                .map_err(|_| "cannot inspect Git status")?
                .iter()
            {
                if !append(
                    format!(
                        "{:?} {}\n",
                        entry.status(),
                        String::from_utf8_lossy(entry.path_bytes())
                    )
                    .as_bytes(),
                ) {
                    break;
                }
            }
        }
        Operation::Diff { staged } => {
            let mut opts = DiffOptions::new();
            opts.ignore_submodules(true).disable_pathspec_match(true);
            if !path.is_empty() {
                opts.pathspec(path);
            }
            let diff = if *staged {
                let tree = match repo.head() {
                    Ok(h) => Some(h.peel_to_tree().map_err(|_| "cannot read HEAD tree")?),
                    Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
                    Err(_) => return Err("cannot read HEAD"),
                };
                repo.diff_tree_to_index(tree.as_ref(), None, Some(&mut opts))
            } else {
                repo.diff_index_to_workdir(None, Some(&mut opts))
            }
            .map_err(|_| "cannot inspect Git diff")?;
            let printed = diff.print(DiffFormat::Patch, |_, _, line| {
                let c = line.origin();
                if matches!(c, '+' | '-' | ' ') && !append(&[c as u8]) {
                    return false;
                }
                append(line.content())
            });
            if printed.is_err() && !cut {
                return Err("Git diff stopped");
            }
        }
        Operation::Log { count } => {
            let mut commit = match repo.head() {
                Ok(h) => Some(h.peel_to_commit().map_err(|_| "cannot read HEAD commit")?),
                Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
                Err(_) => return Err("cannot read HEAD"),
            };
            let mut found = 0;
            for _ in 0..1000 {
                let Some(c) = commit else { break };
                let parent = if c.parent_count() > 0 {
                    Some(c.parent(0).map_err(|_| "cannot read commit parent")?)
                } else {
                    None
                };
                let oid = |c: &git2::Commit| {
                    c.tree()
                        .ok()?
                        .get_path(std::path::Path::new(path))
                        .ok()
                        .map(|e| e.id())
                };
                if path.is_empty() || oid(&c) != parent.as_ref().and_then(oid) {
                    found += 1;
                    if !append(format!("{} {}\n", c.id(), c.summary().unwrap_or("")).as_bytes())
                        || found == *count
                    {
                        break;
                    }
                }
                if cancel.load(Ordering::Relaxed) {
                    return Err("Git inspection cancelled");
                }
                commit = parent;
            }
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Git inspection cancelled");
    }
    let mut text = String::from_utf8_lossy(&out).into_owned();
    if cut {
        text.push_str("\n[output truncated]");
    }
    Ok(text)
}
