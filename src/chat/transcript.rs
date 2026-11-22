//! Transcript data shared by the screen and completion backends.

pub(crate) mod diff;
pub(crate) mod format;
pub(crate) mod highlight;
#[cfg(test)]
mod layout_tests;
pub(crate) mod links;
pub(crate) mod markdown;
pub(crate) mod render;
pub(crate) mod theme;

use std::time::Duration;

use serde_json::Value;

use crate::harness::Protocol;
use crate::tools::ToolCall;
use ahi_durable::ToolDetail;

/// One transcript entry sent to a backend, or a local notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChatMessage {
    pub(crate) role: TranscriptRole,
    /// What the screen shows. For a tool result this is a bounded preview.
    pub(crate) text: String,
    /// Structured tool exchange replayed to the model on later requests.
    pub(crate) tool: Option<ToolPayload>,
    /// Screen-only structure of a tool result (diff texts, command streams,
    /// query). Never sent to a model.
    pub(crate) detail: Option<ToolDetail>,
    /// How long a tool call ran, for its header row.
    pub(crate) elapsed: Option<Duration>,
}

/// Provider-native output captured at a terminal event and replayed verbatim
/// to the protocol that produced it: Responses output items (reasoning with
/// encrypted content, message, function calls) or Anthropic thinking blocks.
/// Opaque data, never displayed, never interpreted, dropped on a protocol
/// change. Bounded by the stream caps that admitted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeReplay {
    pub(crate) protocol: Protocol,
    pub(crate) items: Vec<Value>,
}

/// The model-facing half of a tool exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolPayload {
    /// Calls requested by the assistant, or local loads requested explicitly
    /// by the operator. Native blocks are present only for provider output.
    Calls {
        calls: Vec<ToolCall>,
        native: Option<NativeReplay>,
    },
    /// The `role: "tool"` reply to one call.
    Result {
        call_id: String,
        name: String,
        content: String,
    },
}

/// Prefix of a *presentation* whose local capture exceeds the bytes passed
/// back to the model. Normal previews begin with the tool name, never data
/// from the result body. Kept in display text so old JSONL stays readable.
pub(crate) const MODEL_CUT_PREFIX: &str =
    "[model-facing tool result truncated by the turn budget]\n";

impl ChatMessage {
    pub(crate) fn model_change(gateway: &str, model: &str) -> Self {
        Self::new(
            TranscriptRole::Notice,
            format!("Switched model to {gateway}/{model}"),
        )
    }
    /// A command can be delivered successfully but exit unsuccessfully.
    pub(crate) fn display_role(&self) -> TranscriptRole {
        match (&self.role, &self.detail) {
            (
                TranscriptRole::Tool { exit_code: 0 },
                Some(ToolDetail::Command { exit_code, .. }),
            ) => TranscriptRole::Tool {
                exit_code: exit_code.unwrap_or(-1),
            },
            _ => self.role,
        }
    }

    pub(crate) fn new(role: TranscriptRole, text: impl Into<String>) -> Self {
        Self {
            role,
            text: text.into(),
            tool: None,
            detail: None,
            elapsed: None,
        }
    }
}

/// Who produced a transcript line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranscriptRole {
    Operator,
    Assistant,
    Notice,
    /// A notice about an interrupted or failed operation, drawn highlighted.
    Alert,
    Tool {
        exit_code: i32,
    },
}

impl TranscriptRole {
    /// Screen-only rows: never sent to a model.
    pub(crate) fn is_notice(self) -> bool {
        matches!(self, Self::Notice | Self::Alert)
    }
}
