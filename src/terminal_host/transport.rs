//! Bounded one-shot CLI calls. No shell, mutation retries, or backend discovery.
use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Instant;

const OUTPUT_LIMIT: u64 = 1024 * 1024;
const ERROR_LIMIT: u64 = 16 * 1024;

struct Running {
    child: Child,
    release_input: Option<mpsc::Sender<()>>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.release_input.take();
        // Stop only the connector, never the hosted program or external server.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

enum Event {
    Output(std::io::Result<Vec<u8>>),
    Diagnostics(std::io::Result<Vec<u8>>),
    Input(std::io::Result<()>),
}

pub(super) fn call(executable: &Path, call: &Call, timeout: Duration) -> Result<Value> {
    if timeout.is_zero() {
        return Err(Error::new(
            ErrorKind::Timeout,
            "terminal-host operation timed out",
        ));
    }
    let deadline = Instant::now() + timeout;
    let mut command = Command::new(executable);
    command
        .args(&call.argv)
        .stdin(if call.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in [
        "REX_SESSION",
        "REX_BLOCK",
        "REX_CLIENT",
        "REX_SERVER",
        "HC_TERMINAL_ID",
        "HC_SESSION",
        "HC_SOCKET",
    ] {
        command.env_remove(name);
    }
    let child = command.spawn().map_err(|_| {
        Error::new(
            ErrorKind::Unavailable,
            "terminal-host executable could not be started",
        )
    })?;
    let (release, released) = mpsc::channel();
    let mut running = Running {
        child,
        release_input: Some(release),
    };
    let (events, receiver) = mpsc::channel();
    let stdout = running.child.stdout.take().ok_or_else(Error::protocol)?;
    let stderr = running.child.stderr.take().ok_or_else(Error::protocol)?;
    let rpc = matches!(call.response, Response::HcRpc);
    let output_events = events.clone();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut reader = BufReader::new(stdout).take(OUTPUT_LIMIT + 1);
        let result = if rpc {
            reader.read_until(b'\n', &mut bytes)
        } else {
            reader.read_to_end(&mut bytes)
        };
        let _ = output_events.send(Event::Output(result.map(|_| bytes)));
    });
    let diagnostic_events = events.clone();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stderr.take(ERROR_LIMIT + 1).read_to_end(&mut bytes);
        let _ = diagnostic_events.send(Event::Diagnostics(result.map(|_| bytes)));
    });
    if let Some(payload) = &call.stdin {
        let mut input = running.child.stdin.take().ok_or_else(Error::protocol)?;
        let bytes = match payload {
            Input::Json(value) => {
                let mut bytes = serde_json::to_vec(value).map_err(|_| Error::protocol())?;
                bytes.push(b'\n');
                bytes
            }
            // Text must not acquire an implicit Enter or JSON quoting.
            Input::Text(text) => text.as_bytes().to_vec(),
        };
        std::thread::spawn(move || {
            let result = input.write_all(&bytes).and_then(|()| input.flush());
            let failed = result.is_err();
            let _ = events.send(Event::Input(result));
            // hc connect is a persistent bidirectional transport. Keep its write
            // half open until the response; Rex's stdin-file convention needs EOF.
            if rpc && !failed {
                let _ = released.recv();
            }
        });
    }
    let mut output = None;
    let mut diagnostics_done = false;
    loop {
        if Instant::now() >= deadline {
            return Err(Error::new(
                ErrorKind::Timeout,
                "terminal-host operation timed out",
            ));
        }
        match receiver.recv_timeout(Duration::from_millis(5)) {
            Ok(Event::Output(result)) => {
                let bytes = result.map_err(|_| Error::protocol())?;
                if bytes.len() as u64 > OUTPUT_LIMIT {
                    return Err(Error::new(
                        ErrorKind::Limit,
                        "terminal-host output exceeds 1 MiB",
                    ));
                }
                if rpc {
                    return rpc_response(&bytes);
                }
                output = Some(bytes);
            }
            Ok(Event::Diagnostics(result)) => {
                if result.map_err(|_| Error::protocol())?.len() as u64 > ERROR_LIMIT {
                    return Err(Error::new(
                        ErrorKind::Limit,
                        "terminal-host diagnostics exceed 16 KiB",
                    ));
                }
                diagnostics_done = true;
            }
            Ok(Event::Input(result)) => {
                result.map_err(|_| Error::protocol())?;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) if output.is_none() => {
                return Err(Error::protocol())
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_millis(5))
            }
        }
        if let Some(status) = running.child.try_wait().map_err(|_| Error::protocol())? {
            if !status.success() {
                return Err(Error::new(
                    ErrorKind::Backend,
                    "terminal-host command failed (native diagnostic text withheld)",
                ));
            }
            if diagnostics_done {
                if let Some(bytes) = output.take() {
                    return match call.response {
                        Response::Json => {
                            let value: Value =
                                serde_json::from_slice(&bytes).map_err(|_| Error::protocol())?;
                            if !value.is_object() || value.get("error").is_some() {
                                return Err(Error::protocol());
                            }
                            Ok(value)
                        }
                        Response::Text => String::from_utf8(bytes)
                            .map(Value::String)
                            .map_err(|_| Error::protocol()),
                        Response::HcRpc => Err(Error::protocol()),
                    };
                }
            }
        }
    }
}

fn rpc_response(bytes: &[u8]) -> Result<Value> {
    let mut value: Value = serde_json::from_slice(bytes).map_err(|_| Error::protocol())?;
    if value["jsonrpc"] != "2.0" || value["id"] != 1 || !value.is_object() {
        return Err(Error::protocol());
    }
    if value.get("error").is_some() {
        return Err(Error::new(
            ErrorKind::Backend,
            "hc rejected the native request (native diagnostic text withheld)",
        ));
    }
    value
        .as_object_mut()
        .and_then(|object| object.remove("result"))
        .ok_or_else(Error::protocol)
}
