//! The performance dashboard: it measures, serves the numbers, and opens a browser.
//!
//! ```sh
//! cd tools/perf-dashboard
//! cargo run --release
//! ```
//!
//! `make dashboard` runs the same thing from the repository root. It is **not** a
//! member of the repository's workspace, so `cargo test` at the root never builds
//! it and the interpreter's dependency closure is untouched.
//!
//! The numbers it shows are the numbers the suites measure. The protocol half is
//! `tiger_conformance::protocol`, which is what the threshold test in
//! `crates/tiger-conformance/tests/protocol.rs` runs, so a dashboard row is a row
//! the gate would assert on. The corpus half times the forty-four benchmark programs
//! the way the timing harness does, in process, with no process startup in the
//! numbers.
//!
//! `--port <n>` chooses the address (default 127.0.0.1:7171, falling back to a free
//! port when busy), `--iterations <n>` sets the batch size, `--no-corpus` measures
//! only the protocol, `--engines` adds the tree-walker's pass over the corpus,
//! `--no-open` skips the browser, and `--run-once` measures, prints, and exits
//! without serving anything.

mod json;
mod open;
mod report;
mod server;

use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;

use report::Plan;
use server::Dashboard;

/// The address the dashboard prefers; it falls back to a free port when busy.
const DEFAULT_ADDRESS: &str = "127.0.0.1:7171";

fn main() {
    let mut address = DEFAULT_ADDRESS.to_string();
    let mut plan = Plan::default();
    let mut launch_browser = true;
    let mut once = false;

    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < arguments.len() {
        // The closure takes `arguments` and not `index`, so the arms below are free
        // to move `index` past the option they just read.
        let value = |at: usize, name: &str| -> String {
            arguments.get(at).cloned().unwrap_or_else(|| {
                eprintln!("tiger-perf-dashboard: {name} needs a value");
                std::process::exit(2);
            })
        };
        match arguments[index].as_str() {
            "--port" | "-addr" => {
                address = value(index + 1, "--port");
                index += 2;
            }
            "--iterations" => {
                let given = value(index + 1, "--iterations");
                plan.iterations = given.parse().unwrap_or_else(|_| {
                    eprintln!("tiger-perf-dashboard: --iterations needs a whole number, not {given:?}");
                    std::process::exit(2);
                });
                if plan.iterations == 0 {
                    eprintln!("tiger-perf-dashboard: --iterations needs at least one run");
                    std::process::exit(2);
                }
                index += 2;
            }
            "--no-corpus" => {
                plan.corpus = false;
                index += 1;
            }
            "--engines" => {
                plan.engines = true;
                index += 1;
            }
            "--no-open" => {
                launch_browser = false;
                index += 1;
            }
            "--run-once" => {
                once = true;
                index += 1;
            }
            "--help" | "-h" => {
                usage();
                return;
            }
            other => {
                eprintln!("tiger-perf-dashboard: unknown argument {other:?}\n");
                usage();
                std::process::exit(2);
            }
        }
    }

    if once {
        std::process::exit(measure_once(plan));
    }

    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dashboard");
    let dashboard = Dashboard::new(assets, plan);
    let (listener, bound) = bind(&address);
    let url = format!("http://{bound}/");
    println!("Tiger performance dashboard: {url}");
    println!(
        "Play measures {} iterations{}; one run at a time.",
        plan.iterations,
        if plan.corpus { " of every benchmark program and the paired protocol" } else { " of the paired protocol" }
    );
    if launch_browser {
        if let Err(error) = open::browser(&url) {
            eprintln!("tiger-perf-dashboard: {error}; open {url} yourself");
        }
    }
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let dashboard = Arc::clone(&dashboard);
                std::thread::spawn(move || {
                    let _ = dashboard.handle(stream);
                });
            }
            Err(error) => eprintln!("tiger-perf-dashboard: {error}"),
        }
    }
}

/// Binds the preferred address, falling back to a free port when it is busy.
fn bind(address: &str) -> (TcpListener, String) {
    match TcpListener::bind(address) {
        Ok(listener) => (listener, address.to_string()),
        Err(error) if address == DEFAULT_ADDRESS => {
            println!("{DEFAULT_ADDRESS} is busy ({error}); using a free port instead");
            match TcpListener::bind("127.0.0.1:0") {
                Ok(listener) => {
                    let bound = listener.local_addr().map(|value| value.to_string()).unwrap_or_default();
                    (listener, bound)
                }
                Err(error) => {
                    eprintln!("tiger-perf-dashboard: {error}");
                    std::process::exit(1);
                }
            }
        }
        Err(error) => {
            eprintln!("tiger-perf-dashboard: {error}");
            std::process::exit(1);
        }
    }
}

/// Measures without serving, printing what the dashboard would draw.
///
/// This is the headless path: it is what `--run-once` runs, and what a script or a
/// machine with no browser uses instead of the page.
fn measure_once(plan: Plan) -> i32 {
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let mut emit = |progress: report::Progress| match progress {
                report::Progress::Phase(phase) => println!("== {phase}"),
                report::Progress::Note(text) => println!("note: {text}"),
                report::Progress::Program { name, min, engine } => println!("{engine:<6} {name:<34} {min:>8.3} ms"),
                _ => {}
            };
            report::run(plan, &mut emit)
        })
        .expect("the run thread must start");
    let report = handle.join().expect("the run thread must not panic");
    println!();
    match &report.protocol {
        Some(sections) => {
            print!("{}", tiger_conformance::protocol::format_header());
            print!("{}", tiger_conformance::protocol::format_table(sections));
        }
        None => println!("no protocol: {}", report.protocol_note.as_deref().unwrap_or("unknown reason")),
    }
    for corpus in [&report.corpus, &report.tree].into_iter().flatten() {
        println!();
        println!("corpus on the {} engine: {:.3} ms for {} programs", corpus.engine, corpus.total_min, corpus.programs.len());
    }
    println!();
    println!("measured in {:.2} s", report.seconds);
    0
}

/// The command line, for `--help`.
fn usage() {
    println!("tiger-perf-dashboard [--port <address>] [--iterations <n>] [--no-corpus] [--engines] [--no-open] [--run-once]");
    println!();
    println!("  --port <address>   Where to serve, default {DEFAULT_ADDRESS}, a free port when busy");
    println!("  --iterations <n>   How many times each program or section runs, default 100");
    println!("  --no-corpus        Measure the paired protocol only");
    println!("  --engines          Add the tree-walker's pass over the corpus");
    println!("  --no-open          Do not open a browser");
    println!("  --run-once         Measure, print, and exit without serving");
}
