//! Human-facing tool output, separate from the result sent to the model.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Maximum serialized detail retained in a session entry. This leaves room
/// for the tool's bounded text and model result in ahi's 2 MiB service frame.
pub const MAX_TOOL_DETAIL_BYTES: usize = 512 * 1024;

/// The facts needed to redraw a completed tool call after a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPresentation {
    pub detail: Option<ToolDetail>,
    /// Stored as seconds and nanoseconds, preserving the original duration.
    pub elapsed: Duration,
}

impl ToolPresentation {
    pub fn new(detail: Option<ToolDetail>, elapsed: Duration) -> Self {
        let detail = detail.map(|detail| {
            let encoded = serde_json::to_vec(&detail)
                .expect("ToolDetail contains only JSON-compatible fields");
            if encoded.len() > MAX_TOOL_DETAIL_BYTES {
                ToolDetail::Omitted {
                    reason: "Tool preview exceeded the 512 KiB session-detail limit; the result and duration were retained.".into(),
                }
            } else {
                detail
            }
        });
        Self { detail, elapsed }
    }
}

/// Structured presentation data. Never use this as model-facing tool content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ToolDetail {
    Write {
        /// `None` for a created file.
        before: Option<String>,
        after: String,
    },
    Command {
        argv: Vec<String>,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        stdout_cut: bool,
        stderr_cut: bool,
    },
    Search {
        pattern: String,
        literal: bool,
        glob: Option<String>,
        matches: usize,
    },
    Find {
        pattern: String,
        matches: usize,
    },
    Read {
        offset: Option<usize>,
        limit: Option<usize>,
        lines: usize,
    },
    Omitted {
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_details_are_explicitly_omitted_and_keep_the_duration() {
        let elapsed = Duration::from_millis(3800);
        let presentation = ToolPresentation::new(
            Some(ToolDetail::Write {
                before: None,
                after: "\0".repeat(MAX_TOOL_DETAIL_BYTES / 2),
            }),
            elapsed,
        );
        assert_eq!(presentation.elapsed, elapsed);
        assert!(
            matches!(presentation.detail, Some(ToolDetail::Omitted { ref reason })
            if reason == "Tool preview exceeded the 512 KiB session-detail limit; the result and duration were retained.")
        );
        assert!(serde_json::to_vec(&presentation).unwrap().len() < 1024);
    }
}
