//! Execution records shared by ahi's tool executor and session service.
//!
//! This initial extraction preserves existing effect accounting. It does not
//! yet persist execution checkpoints or recover unfinished runs after a crash.

mod ledger;
mod tool_presentation;
mod tool_record;

pub use ledger::Ledger;
pub use tool_presentation::{
    ToolDetail, ToolOmissionReason, ToolPresentation, MAX_TOOL_DETAIL_BYTES,
};
pub use tool_record::ToolRecord;
