fn main() -> std::process::ExitCode {
    // This is the single-threaded entry point. Take the terminal capability
    // out of the environment before any worker or subprocess can inherit it.
    let token = std::env::var("OSC7577_TOKEN").ok();
    std::env::remove_var("OSC7577_TOKEN");
    ahi::cli::run_with_terminal_token(token)
}
