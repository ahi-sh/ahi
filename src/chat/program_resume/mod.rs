//! The Program Resume Protocol (OSC 7577) boundary of the owning TUI: the
//! only place ahi emits the sequences by which a terminal can offer to
//! reopen a saved local chat.

mod osc7577;

pub(super) use osc7577::Reporter;
