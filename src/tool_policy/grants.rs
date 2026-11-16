//! Operator grants scoped to one process, workspace, tool, and destination.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// An operator approval that outlives one batch: this tool, under this
/// non-empty prefix, in this workspace, for this gateway at this destination.
/// Switching the gateway, reconfiguring its URL, or changing the working
/// directory needs a new approval, because the grant decides which files
/// leave the machine and where they go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionGrant {
    pub(crate) root: PathBuf,
    pub(crate) gateway: String,
    pub(crate) destination: String,
    pub(crate) tool: String,
    pub(crate) prefix: String,
    /// For `run_command`, the approving rule's argv prefix: the grant is
    /// exactly as wide as the rule that asked, never the full command.
    pub(crate) argv: Vec<String>,
}

/// Held by the backend for the life of the chat process.
pub(crate) type SessionGrants = Arc<Mutex<Vec<SessionGrant>>>;
