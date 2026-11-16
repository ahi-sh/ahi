// Disposable executable for streaming progress tests. No shell or network.
use std::path::Path;
use std::time::{Duration, Instant};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let name = &args[1];
    std::fs::write(format!("{name}.entered"), b"running").unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !Path::new(&format!("{name}.release")).exists() {
        if Instant::now() >= deadline { std::process::exit(99); }
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("{name} hook output");
    std::process::exit(args[2].parse().unwrap());
}
