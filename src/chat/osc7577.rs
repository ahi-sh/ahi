//! OSC 7577 (the Program Resume Protocol): reports from the owning TUI,
//! never from the inference service, so a terminal can offer to reopen a
//! saved local chat.
use std::{
    env,
    io::{self, IsTerminal, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use super::Engine;
use crate::{directories::Directories, util::base64::encode_standard};

pub(super) struct Reporter {
    token: &'static str,
    cwd: String,
    argv_prefix: String,
    registered: Option<String>,
}

fn encoded(text: &str) -> Option<String> {
    (!text.chars().any(char::is_control)).then(|| encode_standard(text.as_bytes()))
}

impl Reporter {
    pub fn new(dirs: &Directories, engine: &Engine, actions: Option<&Path>) -> Option<Self> {
        if !io::stdout().is_terminal() {
            return None;
        }
        let token = crate::cli::TERMINAL_RESUME_TOKEN.get()?.as_str();
        if !(22..=64).contains(&token.len())
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return None;
        }
        let cwd = env::current_dir().ok()?;
        let mut args = vec!["ahi", "tui"];
        // Pin every resolved root. The restored shell may have different
        // XDG/AHI overrides; relative paths retain their meaning through cwd.
        args.extend([
            "--config-dir",
            dirs.config.to_str()?,
            "--data-dir",
            dirs.data.to_str()?,
            "--state-root",
            dirs.state.to_str()?,
            "--cache-dir",
            dirs.cache.to_str()?,
        ]);
        match engine {
            Engine::Standalone { record: true } => args.push("--standalone"),
            Engine::Standalone { record: false } => return None,
            Engine::Service(client) => args.extend(["--socket", client.socket()?.to_str()?]),
        }
        if let Some(actions) = actions {
            args.extend(["--tools", actions.to_str()?]);
        }
        args.push("--session");
        Some(Self {
            token,
            cwd: encoded(cwd.to_str()?)?,
            argv_prefix: args
                .into_iter()
                .map(encoded)
                .collect::<Option<Vec<_>>>()?
                .join(","),
            registered: None,
        })
    }

    pub fn update(&mut self, session: Option<&str>) {
        let session = session.filter(|id| crate::session::valid_id(id));
        if self.registered.as_deref() == session {
            return;
        }
        let fields = match session {
            Some(id) => format!(
                "argv={},{}:cwd={}:need=cwd",
                self.argv_prefix,
                encode_standard(id.as_bytes()),
                self.cwd
            ),
            None => "clear=1".into(),
        };
        let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return;
        };
        if now.as_secs() == 0 {
            return;
        }
        let body = format!("pid={}:time={}:{fields}", std::process::id(), now.as_secs());
        let mut out = io::stdout().lock();
        if out
            .write_all(frame(&body, self.token).as_bytes())
            .and_then(|()| out.flush())
            .is_ok()
        {
            self.registered = session.map(str::to_owned);
        }
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        self.update(None);
    }
}

fn frame(body: &str, token: &str) -> String {
    let mac = crate::session::sha256_hex(format!("{body}:{token}").as_bytes());
    format!("\x1b]7577;{body}:mac={mac}\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clear_matches_the_published_protocol_vector() {
        assert_eq!(frame("pid=74877:time=1795543200:clear=1", "dummy-token-replace-me"), "\x1b]7577;pid=74877:time=1795543200:clear=1:mac=c30603369f98fef581a4098be889175b0c74f0641e49f8b25f4326cb38f97c93\x1b\\");
    }
}
