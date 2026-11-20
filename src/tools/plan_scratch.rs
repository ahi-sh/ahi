//! A session's single private draft. No workspace-relative or absolute paths.
use super::*;
use std::{
    fs::File,
    io::{Read, Seek, Write},
    sync::{Arc, Mutex},
};
#[derive(Clone, Default, Debug)]
pub(crate) struct Scratch(Arc<Mutex<Option<File>>>);
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum Operation {
    Read {},
    Write { content: String },
}
pub(super) fn schema() -> ToolSchema {
    ToolSchema{name:"plan_scratch",description:"Read or replace this live session's private temporary draft (64 KiB). Never changes workspace files. Put the final plan in chat for durable history.".into(),parameters:json!({"type":"object","additionalProperties":false,"required":["operation"],"properties":{"operation":{"type":"string","enum":["read","write"]},"content":{"type":"string","description":"Required only for write"}}})}
}
pub(super) fn prepare(call: &ToolCall) -> Result<PreparedCall, &'static str> {
    let op = serde_json::from_value(Value::Object(call.arguments.clone())).map_err(|_| INVALID)?;
    if matches!(&op,Operation::Write{content} if content.len()>65536) {
        return Err("scratch draft exceeds 64 KiB");
    }
    Ok(PreparedCall {
        tool: "plan_scratch",
        path: String::new(),
        arguments: Arguments::Scratch(op),
    })
}
impl Scratch {
    pub(super) fn run(&self, op: &Operation) -> Result<String, &'static str> {
        let mut slot = self.0.lock().map_err(|_| "scratch unavailable")?;
        match op {
            Operation::Write { content } => {
                let mut f = tempfile::tempfile().map_err(|_| "cannot create session scratch")?;
                f.write_all(content.as_bytes())
                    .map_err(|_| "cannot write session scratch")?;
                *slot = Some(f);
                Ok("Session scratch updated; no workspace file changed.".into())
            }
            Operation::Read {} => {
                let mut text = String::new();
                if let Some(f) = slot.as_mut() {
                    f.rewind()
                        .and_then(|_| f.read_to_string(&mut text))
                        .map_err(|_| "cannot read session scratch")?;
                }
                Ok(text)
            }
        }
    }
}
