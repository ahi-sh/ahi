//! Human-facing tool output, separate from the result sent to the model.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Why rich details were not retained. Display wording belongs to the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ToolOmissionReason {
    DetailTooLarge {},
    /// Preserve both the wording and wire shape of historical string reasons.
    #[serde(untagged)]
    Legacy(String),
}

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
                    reason: ToolOmissionReason::DetailTooLarge {},
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
        reason: ToolOmissionReason,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omission_reasons_restore_typed_and_legacy_records() {
        for (json, expected) in [
            (
                r#"{"kind":"omitted","reason":{"kind":"detail_too_large"}}"#,
                ToolOmissionReason::DetailTooLarge {},
            ),
            (
                r#"{"kind":"omitted","reason":"preview not retained"}"#,
                ToolOmissionReason::Legacy("preview not retained".into()),
            ),
        ] {
            let detail: ToolDetail = serde_json::from_str(json).unwrap();
            assert_eq!(detail, ToolDetail::Omitted { reason: expected });
            let stored = serde_json::to_string(&detail).unwrap();
            assert_eq!(stored, json);
            assert_eq!(serde_json::from_str::<ToolDetail>(&stored).unwrap(), detail);
        }
    }

    #[test]
    fn malformed_or_unknown_omission_reasons_are_rejected() {
        for reason in [
            "null",
            "42",
            "[]",
            "{}",
            r#"{"kind":"unknown"}"#,
            r#"{"kind":"detail_too_large","extra":true}"#,
            r#"{"kind":"legacy","message":42}"#,
        ] {
            let json = format!(r#"{{"kind":"omitted","reason":{reason}}}"#);
            assert!(serde_json::from_str::<ToolDetail>(&json).is_err(), "{json}");
        }
    }

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
        assert_eq!(
            serde_json::to_value(&presentation).unwrap(),
            serde_json::json!({
                "detail": {"kind": "omitted", "reason": {"kind": "detail_too_large"}},
                "elapsed": {"secs": 3, "nanos": 800_000_000}
            })
        );
        assert!(serde_json::to_vec(&presentation).unwrap().len() < 1024);
    }
}
