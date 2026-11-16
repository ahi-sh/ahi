//! Operator-only terminal hosting. This does not add a model-facing tool.
use crate::terminal_host::{Client, Handle, Key, Operation, Target};
use clap::{Args, Subcommand};

#[derive(Args)]
pub(super) struct Options {
    /// Select one backend. No automatic fallback, installation, or discovery.
    #[arg(long, global = true, default_value = "rex", value_parser = ["rex", "hc"])]
    backend: String,
    /// Rex host label/address. Required for Rex; not used by hc.
    #[arg(long, global = true)]
    server: Option<String>,
    /// Rex full session:UUID or explicit hc core session namespace; not an ahi session ID.
    #[arg(long, global = true)]
    host_session: Option<String>,
    /// hc machine name. Omit for this client's local core. Windows and WSL are separate targets.
    #[arg(long, global = true)]
    machine: Option<String>,
    /// Print native requests only; never start a process or contact a server.
    #[arg(long, global = true)]
    dry_run: bool,
    /// Bound the whole operation, including both steps of a Rex submission (1..120 seconds).
    #[arg(long, global = true, default_value_t = 15, value_parser = clap::value_parser!(u64).range(1..=120))]
    timeout: u64,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start exact argv in a persistent terminal. Requires an absolute program and cwd on the target.
    Start {
        #[arg(long)]
        cwd: String,
        #[arg(required = true, last = true)]
        argv: Vec<String>,
    },
    /// Observe process state; an unreachable host is not an exited program.
    Inspect { terminal: String },
    /// Return a bounded screen snapshot as JSON, not a stream or an agent reply.
    Capture { terminal: String },
    /// Send raw text without Enter (Rex only; hc returns unsupported).
    Text { terminal: String, text: String },
    /// Submit text. Rex writes then presses Enter; hc uses its draft-aware send API.
    Submit { terminal: String, text: String },
    /// Send one explicitly named key.
    Key {
        terminal: String,
        #[arg(value_name = "enter|escape|control-c")]
        key: Key,
    },
    /// Rex: send Ctrl+C. hc: use its observed stop-key protocol. Never escalate.
    Interrupt { terminal: String },
    /// Rex: close this block only. hc: end its program, retaining the terminal record.
    Terminate { terminal: String },
}

pub(super) fn run(options: &Options) -> u8 {
    match execute(options) {
        Ok(value) => {
            println!("{value}");
            0
        }
        Err(error) => {
            use crate::terminal_host::ErrorKind;
            eprintln!("error: {error}");
            match error.kind {
                ErrorKind::Input => 2,
                ErrorKind::Unsupported => 3,
                ErrorKind::Unavailable => 4,
                ErrorKind::Timeout => 5,
                ErrorKind::Limit => 6,
                ErrorKind::Protocol => 7,
                ErrorKind::Backend => 8,
            }
        }
    }
}

fn execute(options: &Options) -> crate::terminal_host::Result<serde_json::Value> {
    let invalid = |message: &str| crate::terminal_host::Error {
        kind: crate::terminal_host::ErrorKind::Input,
        effects_uncertain: false,
        message: message.into(),
    };
    let session = options.host_session.clone().ok_or_else(|| {
        invalid("--host-session must explicitly select a native session/core namespace")
    })?;
    let target = match options.backend.as_str() {
        "rex" if options.machine.is_none() => Target::Rex {
            server: options
                .server
                .clone()
                .ok_or_else(|| invalid("Rex requires --server to select an execution target"))?,
            session: session.clone(),
        },
        "hc" if options.server.is_none() => Target::Hc {
            session,
            machine: options.machine.clone(),
        },
        _ => return Err(invalid("--server belongs to Rex; --machine belongs to hc")),
    };
    let handle = |id: &str| Handle {
        target: target.clone(),
        id: id.into(),
    };
    let operation = match &options.command {
        Command::Start { argv, cwd } => Operation::Start {
            argv: argv.clone(),
            cwd: cwd.clone(),
        },
        Command::Inspect { terminal } => Operation::Inspect {
            terminal: handle(terminal),
        },
        Command::Capture { terminal } => Operation::Capture {
            terminal: handle(terminal),
        },
        Command::Text { terminal, text } => Operation::Text {
            terminal: handle(terminal),
            text: text.clone(),
        },
        Command::Submit { terminal, text } => Operation::Submit {
            terminal: handle(terminal),
            text: text.clone(),
        },
        Command::Key { terminal, key } => Operation::Key {
            terminal: handle(terminal),
            key: *key,
        },
        Command::Interrupt { terminal } => Operation::Interrupt {
            terminal: handle(terminal),
        },
        Command::Terminate { terminal } => Operation::Terminate {
            terminal: handle(terminal),
        },
    };
    let client =
        Client::new(target)?.with_timeout(std::time::Duration::from_secs(options.timeout))?;
    if options.dry_run {
        return client.preview(&operation);
    }
    serde_json::to_value(client.execute(&operation)?)
        .map_err(|_| invalid("could not encode terminal-host result"))
}
