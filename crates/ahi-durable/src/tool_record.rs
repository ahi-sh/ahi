/// The audit-log facts of one effectful call, as a caller's ledger records
/// them: paths and hashes, argv and exit codes. Never contents or output.
/// The audit line is derived from this record, so a ledger and `audit.log`
/// agree line for line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolRecord {
    Write {
        tool: String,
        /// Workspace-relative path.
        path: String,
        /// `ok`, or the tool's error message.
        outcome: String,
        /// SHA-256 hex of the file before and after; `None` when absent.
        before: Option<String>,
        after: Option<String>,
        lines_added: usize,
        lines_removed: usize,
    },
    Command {
        argv: Vec<String>,
        /// Workspace-relative working directory; empty is the root.
        cwd: String,
        outcome: String,
        /// Whether a process ran: `Some(true)` it did (even if killed),
        /// `Some(false)` it never started, `None` unknown.
        ran: Option<bool>,
        exit_code: Option<i32>,
        duration_ms: u128,
    },
    /// A skill the model read: which, from which root, which file, and the
    /// hash and size of what it saw. Never the text.
    Skill {
        name: String,
        /// `repo` or `operator`; empty when the load failed before a root
        /// answered.
        root: String,
        /// A declared reference, or `None` for `SKILL.md`.
        file: Option<String>,
        /// `ok`, or the tool's error message.
        outcome: String,
        sha256: String,
        bytes: usize,
    },
    /// A task handed to a worker: which one, the worker's session and run,
    /// how it ended, and what the worker says it changed.
    Delegate {
        /// The connect command that was run, resolved for the label.
        via: Vec<String>,
        worker: String,
        session: String,
        run_id: String,
        outcome: String,
        status: String,
        /// The worker's `effects`: `none`, `some`, `unknown`.
        effects: String,
        duration_ms: u128,
    },
}

impl ToolRecord {
    /// Whether this call changed the working directory or ran a program.
    /// `None` when that cannot be known from the outcome. For a delegation
    /// it is the worker's word on its own directory, not this one.
    pub fn effect(&self) -> Option<bool> {
        match self {
            Self::Write { outcome, .. } => Some(outcome == "ok"),
            Self::Command { ran, .. } => *ran,
            // A read: nothing in the working directory changes.
            Self::Skill { .. } => Some(false),
            Self::Delegate { effects, .. } => match effects.as_str() {
                "none" => Some(false),
                "some" => Some(true),
                _ => None,
            },
        }
    }
}
