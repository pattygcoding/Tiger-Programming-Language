//! The `tiger` executable: a CLI that also runs the programs it built.
//!
//! `tiger build` copies this binary and appends a module bundle, so the result
//! is self-contained. At startup the trailer decides which mode to run, and
//! every mode executes on a thread with a large stack so deeply recursive Tiger
//! programs do not overflow the default one.

use std::io::{Read, Write};

/// The stack every program gets, matching the conformance harness.
const STACK: usize = 64 * 1024 * 1024;

fn main() {
    let code = match tiger_cli::find_payload(&own_bytes()) {
        Some(payload) => {
            let payload = payload.to_vec();
            // A built executable is its own program, so the process's arguments are
            // the program's, with the executable's path first as in Python.
            let arguments: Vec<String> = std::env::args().collect();
            spawn(move || {
                tiger_cli::run_embedded(
                    &payload,
                    &arguments,
                    Some(Box::new(std::io::stdin().lock())),
                    Box::new(std::io::stdout()),
                    Box::new(std::io::stderr()),
                )
            })
        }
        None => {
            let args: Vec<String> = std::env::args().skip(1).collect();
            spawn(move || {
                tiger_cli::run(&args, Some(Box::new(std::io::stdin().lock())), Box::new(std::io::stdout()), Box::new(std::io::stderr()))
            })
        }
    };
    std::process::exit(code);
}

/// Reads this executable so the trailer can be inspected.
fn own_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    if let Ok(mut file) = std::fs::File::open(std::env::current_exe().unwrap_or_default()) {
        let _ = file.read_to_end(&mut bytes);
    }
    bytes
}

/// Runs `body` on a big-stack thread, falling back to this thread if it cannot
/// be started.
fn spawn<F>(body: F) -> i32
where
    F: FnOnce() -> i32 + Send + 'static,
{
    match std::thread::Builder::new().stack_size(STACK).spawn(body) {
        Ok(handle) => match handle.join() {
            Ok(code) => code,
            Err(_) => {
                let _ = writeln!(std::io::stderr(), "tiger: the interpreter panicked");
                1
            }
        },
        Err(_) => 1,
    }
}
