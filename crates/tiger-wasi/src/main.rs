//! The WASI entry point: `tiger-wasi <file.tg>`.
//!
//! A port of `cmd/wasi/main.go`. It shares the CLI's program runner, so the
//! interpreter sees the same host filesystem, module loader, and diagnostics
//! the native `tiger run` provides.

use std::io::Write;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 1 || tiger_cli::extension(&arguments[0]) != ".tg" {
        let _ = writeln!(std::io::stderr(), "Usage: tiger-wasi <file.tg>");
        std::process::exit(2);
    }
    match tiger_cli::run_file(&arguments[0], Some(Box::new(std::io::stdin().lock())), Box::new(std::io::stdout())) {
        Ok(()) => {}
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "{}", error);
            std::process::exit(1);
        }
    }
}
