// Standalone, std-only fake native CLI. Compiled by terminal_host's contract test.
// Never invokes a shell, backend, network service, or hosted program.
use std::io::{BufRead, Read, Write};
fn main() {
    let root = std::env::current_exe().unwrap().parent().unwrap().to_owned();
    let mode = std::fs::read_to_string(root.join("mode")).unwrap_or_default();
    if mode == "hang" { std::thread::sleep(std::time::Duration::from_secs(30)); return; }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut log = std::fs::OpenOptions::new().append(true).create(true).open(root.join("calls")).unwrap();
    writeln!(log, "{}", args.join("|")).unwrap();
    if mode == "fail-key" && args.iter().any(|arg| arg == "send-key") { std::process::exit(9); }
    let rpc = args.first().is_some_and(|arg| arg == "connect");
    let mut input = String::new();
    if rpc { std::io::stdin().lock().read_line(&mut input).unwrap(); }
    else { std::io::stdin().read_to_string(&mut input).unwrap(); }
    std::fs::write(root.join("input"), input).unwrap();
    if mode == "flood" { std::io::stdout().write_all(&vec![b'x'; 1024 * 1024 + 1]).unwrap(); return; }
    let reply = std::fs::read(root.join("reply")).unwrap();
    std::io::stdout().write_all(&reply).unwrap();
    std::io::stdout().flush().unwrap();
    if rpc { std::thread::sleep(std::time::Duration::from_secs(30)); }
}
