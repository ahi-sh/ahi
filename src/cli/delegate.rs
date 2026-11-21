//! Operator-only local delegation commands. Not a model-facing tool, and
//! nothing here is advertised to a model.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Args, Subcommand};
use serde_json::json;

use crate::delegation::{self, Spec, DEFAULT_TIMEOUT_SECS};
use crate::directories::Directories;
use crate::terminal_host::Target;

#[derive(Args)]
pub(super) struct Options {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Reserve a scratch assignment and start its worker in the configured Rex session.
    ///
    /// The worker is the process in the block: it runs one turn on the exact
    /// gateway/model with no tools and publishes a structured result. No Rex
    /// client needs to be attached. The prompt file is kept privately beside
    /// the receipt, not in the scratch directory. A launch is issued once;
    /// an ambiguous reply is recorded, never retried.
    Launch(LaunchArgs),
    /// Show the receipt, the worker's claim, and its result if published.
    Status {
        /// Assignment id printed by `launch`.
        id: String,
        /// Poll until a result is published or this many seconds pass.
        #[arg(long, value_name = "SECS")]
        wait: Option<u64>,
    },
    /// Record a cancel; the worker honors it at its next checkpoint. The block is not touched.
    Cancel {
        /// Assignment id printed by `launch`.
        id: String,
    },
}

#[derive(Args)]
struct LaunchArgs {
    /// Gateway and model as gateway/model-id, curated in models.json. Bound exactly; never inherited later.
    #[arg(long, value_name = "GATEWAY/MODEL")]
    model: String,
    /// The task, read once (UTF-8, at most 1 MiB).
    #[arg(long, value_name = "PATH")]
    prompt_file: PathBuf,
    /// Local Rex endpoint: unix:///absolute/socket or a loopback IP:port (no host labels).
    #[arg(long)]
    server: String,
    /// Rex full session:UUID that receives the worker's block; not an ahi session ID.
    #[arg(long)]
    host_session: String,
    /// Seconds for the whole child turn.
    #[arg(long, value_name = "SECS", default_value_t = DEFAULT_TIMEOUT_SECS)]
    timeout: u64,
    /// Reasoning effort for the child turn.
    #[arg(long, value_name = "LEVEL", default_value = "off", value_parser = ["off", "minimal", "low", "medium", "high"])]
    thinking: String,
    /// Validate the route, prompt, and target; print the plan; reserve and start nothing.
    #[arg(long)]
    dry_run: bool,
}

/// Arguments of the hidden `internal-worker` entry point.
#[derive(Args)]
pub(super) struct WorkerArgs {
    /// The assignment directory recorded at launch.
    #[arg(long, value_name = "DIR")]
    assignment_dir: PathBuf,
    /// The launch attempt this process was started for.
    #[arg(long, value_name = "ID")]
    attempt: String,
}

/// Exit codes: 0 ok, 1 other, 2 input, 3 configuration, 4 conflict
/// (duplicate or ambiguous), 5 launch not established, 6 unsettled wait.
pub(super) fn run(dirs: &Directories, options: &Options) -> u8 {
    match execute(dirs, options) {
        Ok(value) => {
            println!("{value:#}");
            0
        }
        Err(Failure::Delegation(err)) => {
            eprintln!("error: {err}");
            match err.kind {
                delegation::ErrorKind::Input => 2,
                delegation::ErrorKind::Conflict => 4,
                delegation::ErrorKind::Launch => 5,
                delegation::ErrorKind::Io => 1,
            }
        }
        Err(Failure::Other(code, message)) => {
            eprintln!("error: {message}");
            code
        }
    }
}

pub(super) fn worker(args: &WorkerArgs) -> u8 {
    delegation::run_worker(&args.assignment_dir, &args.attempt)
}

enum Failure {
    Delegation(delegation::Error),
    Other(u8, String),
}

impl From<delegation::Error> for Failure {
    fn from(err: delegation::Error) -> Self {
        Self::Delegation(err)
    }
}

fn execute(dirs: &Directories, options: &Options) -> Result<serde_json::Value, Failure> {
    match &options.command {
        Command::Launch(args) => launch(dirs, args),
        Command::Status { id, wait } => {
            let deadline = wait.map(|secs| Instant::now() + Duration::from_secs(secs));
            loop {
                let status = delegation::status(dirs, id)?;
                let settled = status["settled"] == true;
                match deadline {
                    Some(deadline) if !settled && Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(250));
                    }
                    Some(_) if !settled => {
                        return Err(Failure::Other(
                            6,
                            format!(
                                "assignment {id} has no result yet (state {})",
                                status["state"]
                            ),
                        ))
                    }
                    _ => return Ok(status),
                }
            }
        }
        Command::Cancel { id } => {
            let mut receipt = delegation::load(dirs, id)?;
            delegation::cancel(dirs, &mut receipt)?;
            Ok(delegation::status(dirs, id)?)
        }
    }
}

fn launch(dirs: &Directories, args: &LaunchArgs) -> Result<serde_json::Value, Failure> {
    let selection = crate::chat::backend::parse_selection(&args.model);
    if selection.gateway.is_empty() || selection.model.is_empty() {
        return Err(Failure::Other(2, "--model takes gateway/model-id".into()));
    }
    let prompt = read_prompt(&args.prompt_file)?;
    let target = Target::Rex {
        server: args.server.clone(),
        session: args.host_session.clone(),
    };
    let spec = Spec {
        gateway: selection.gateway.clone(),
        model: selection.model.clone(),
        thinking: args.thinking.clone(),
        prompt,
        timeout_secs: args.timeout,
        target: target.clone(),
    };
    delegation::preview(dirs, &spec)?;
    // Bind the exact route now: an unknown gateway or model, or a missing
    // credential source, fails here with zero reservations and zero
    // provider requests. Resolution reads local configuration only.
    crate::harness::completion_connection(dirs, &selection.gateway, &selection.model)
        .map_err(|err| Failure::Other(3, err.to_string()))?;
    let executable = std::env::current_exe()
        .ok()
        .filter(|path| path.is_absolute())
        .ok_or_else(|| {
            Failure::Other(3, "cannot determine this executable's absolute path".into())
        })?;
    if args.dry_run {
        return Ok(json!({
            "dry_run": true,
            "gateway": spec.gateway,
            "model": spec.model,
            "thinking": spec.thinking,
            "prompt_sha256": crate::session::sha256_hex(spec.prompt.as_bytes()),
            "prompt_bytes": spec.prompt.len(),
            "timeout_secs": spec.timeout_secs,
            "target": target,
            "worker_executable": executable,
            "placement": "scratch",
            "tools": [],
            "reserved": false,
        }));
    }
    let mut receipt = delegation::reserve(dirs, spec)?;
    let launched = delegation::launch(dirs, &mut receipt, &executable, None);
    let status = delegation::status(dirs, &receipt.id)?;
    match launched {
        Ok(()) => Ok(status),
        Err(err) => {
            // The reservation and its phase are on disk; the operator needs
            // the id to reconcile, so it goes to stderr with the error.
            eprintln!("assignment {} is {}", receipt.id, receipt.phase.name());
            Err(err.into())
        }
    }
}

fn read_prompt(path: &Path) -> Result<String, Failure> {
    let usage = |message: String| Failure::Other(2, message);
    let metadata = std::fs::metadata(path)
        .map_err(|err| usage(format!("cannot read {}: {err}", path.display())))?;
    if !metadata.is_file() {
        return Err(usage(format!("{} is not a file", path.display())));
    }
    if metadata.len() > delegation::MAX_PROMPT_BYTES as u64 {
        return Err(usage(format!(
            "{} is larger than the {} byte prompt cap",
            path.display(),
            delegation::MAX_PROMPT_BYTES
        )));
    }
    let bytes = std::fs::read(path)
        .map_err(|err| usage(format!("cannot read {}: {err}", path.display())))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| usage(format!("{} is not UTF-8 text", path.display())))?;
    if text.trim().is_empty() {
        return Err(usage("the prompt is empty".into()));
    }
    Ok(text)
}
