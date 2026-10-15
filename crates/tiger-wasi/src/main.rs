//! The WASI entry point: `tiger-wasi <file.tg> [arguments...]`.
//!
//! A port of `cmd/wasi/main.go`. It shares the CLI's program runner, so the
//! interpreter sees the same host filesystem, module loader, and diagnostics
//! the native `tiger run` provides.

use std::io::Write;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.is_empty() || tiger_cli::extension(&arguments[0]) != ".tg" {
        let _ = writeln!(std::io::stderr(), "Usage: tiger-wasi <file.tg> [arguments...]");
        std::process::exit(2);
    }
    // Everything after the program's path belongs to the program, so `sys.argv` is
    // its path first and then each argument, as it is for `tiger run`.
    match tiger_cli::run_file(&arguments[0], &arguments, Some(Box::new(std::io::stdin().lock())), Box::new(std::io::stdout())) {
        Ok(exit) => {
            if let Some(message) = &exit.message {
                let _ = writeln!(std::io::stderr(), "{}", message);
            }
            std::process::exit(exit.code);
        }
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "{}", error);
            std::process::exit(1);
        }
    }
}
