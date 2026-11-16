//! hc 0.1.21 OpenRPC contracts (`hc schema`). Its connect transport carries hc
//! JSON-RPC, never ahi's session protocol. Using the API preserves literal argv
//! even for a single executable containing spaces; `hc run` has shell shorthand.
use super::*;
use serde_json::json;

pub(super) fn prepare(session: &str, machine: Option<&str>, op: &Operation) -> Result<Vec<Call>> {
    let terminal = op.terminal().map(|t| t.id.as_str());
    let (method, params) = match op {
        Operation::Start { argv, cwd } => (
            "terminal.run",
            json!({
                "argv": argv, "cwd": cwd, "on": machine, "wait": false,
                "no_parent": true, "no_notify": true, "keep": false, "rm": false
            }),
        ),
        Operation::Inspect { .. } => ("terminal.get", json!({"terminal": terminal})),
        Operation::Capture { .. } => ("terminal.read", json!({"terminal": terminal, "lines": 0})),
        Operation::Text { .. } => {
            return Err(Error::new(
                ErrorKind::Unsupported,
                "hc's send API submits with Enter; raw text without submission is unsupported",
            ))
        }
        Operation::Submit { text, .. } => (
            "terminal.send",
            json!({"terminal": terminal, "text": text, "force": false, "wait": false}),
        ),
        Operation::Key { key, .. } => (
            "terminal.keys",
            json!({"terminal": terminal, "keys": [key.native()]}),
        ),
        Operation::Interrupt { .. } => (
            "terminal.interrupt",
            json!({"terminal": terminal, "timeout": 10}),
        ),
        Operation::Terminate { .. } => ("terminal.kill", json!({"terminal": terminal})),
    };
    Ok(vec![Call {
        argv: vec!["connect".into(), "--session".into(), session.into()],
        stdin: Some(Input::Json(
            json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
        )),
        response: Response::HcRpc,
    }])
}

#[derive(Deserialize)]
struct Terminal {
    address: String,
    state: String,
    exit_code: Option<i32>,
    signal: Option<i32>,
    reason: Option<String>,
}
impl Terminal {
    fn process(&self) -> ProcessState {
        match (self.state.as_str(), self.reason.as_deref()) {
            ("live", Some("spawn_pending")) => ProcessState::Pending,
            ("live", _) => ProcessState::Running,
            ("finished", Some("holder_lost" | "stopped_by_restart")) => ProcessState::Unknown {
                reason: self.reason.clone().unwrap_or_default(),
            },
            ("finished", _) => ProcessState::Exited {
                code: self.exit_code,
                signal: self.signal,
            },
            _ => ProcessState::Unknown {
                reason: self.state.clone(),
            },
        }
    }
}

pub(super) fn decode(target: &Target, op: &Operation, value: Value) -> Result<Reply> {
    if matches!(
        op,
        Operation::Start { .. } | Operation::Inspect { .. } | Operation::Terminate { .. }
    ) {
        let native: Terminal = super::decode(value)?;
        let terminal = Handle {
            target: target.clone(),
            id: native.address.clone(),
        };
        terminal.validate().map_err(|_| Error::protocol())?;
        if op.terminal().is_some_and(|expected| expected != &terminal) {
            return Err(Error::protocol());
        }
        return Ok(match op {
            Operation::Start { .. } => Reply::Started { terminal },
            Operation::Inspect { .. } => Reply::Inspected {
                terminal,
                process: native.process(),
            },
            Operation::Terminate { .. } => Reply::StopRequested {
                terminal,
                record_retained: true,
                process: Some(native.process()),
            },
            _ => unreachable!(),
        });
    }
    let terminal = op.terminal().ok_or_else(Error::protocol)?.clone();
    match op {
        Operation::Capture { .. } => {
            #[derive(Deserialize)]
            struct Snapshot {
                text: String,
                complete: bool,
                how: String,
            }
            let snapshot: Snapshot = super::decode(value)?;
            Ok(Reply::Captured {
                terminal,
                text: snapshot.text,
                extent: snapshot.how,
                complete: Some(snapshot.complete),
            })
        }
        Operation::Interrupt { .. } => {
            #[derive(Deserialize)]
            struct Interrupted {
                key: String,
                outcome: String,
            }
            let result: Interrupted = super::decode(value)?;
            Ok(Reply::Interrupted {
                terminal,
                key: result.key,
                observation: Some(result.outcome),
            })
        }
        Operation::Submit { .. } | Operation::Key { .. } => {
            #[derive(Deserialize)]
            struct Sent {
                turn: u64,
            }
            let Sent { turn: _turn } = super::decode(value)?;
            Ok(Reply::InputSent { terminal })
        }
        _ => Err(Error::protocol()),
    }
}
