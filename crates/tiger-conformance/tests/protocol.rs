//! The in-process Tiger/Python protocol, and the ratio it has to hold.
//!
//! `benchmarks/python/compare.py` times two whole processes, so both numbers carry
//! their launcher: the workload is one to three milliseconds of work against a five
//! to eighteen millisecond startup floor, and subtracting the floor amplifies its
//! jitter past the signal. This test measures the interpreters instead, so no
//! process startup is in either number.
//!
//! Tiger is measured here, in process. CPython is measured by
//! `benchmarks/python/protocol.py`, also in process: it compiles `workload.py` once
//! and then runs one section per `exec`. Both sides run one pinned section at a
//! time, for the same number of iterations, report the minimum, and have their
//! checksums compared, so the two are demonstrably doing the same work.
//!
//! The assertion is the comparison's own number: Tiger's work against CPython's,
//! with the program's own compile removed from both sides. It fails while any
//! section is slower than CPython, which is what "2.6x slower" means — but it is
//! asked for rather than assumed, because that ratio is against whatever CPython the
//! host has on `PATH`. A bare `cargo test` reports the table and passes; `make
//! protocol` sets `TIGER_PROTOCOL_ASSERT` and is what fails.
//!
//! `TIGER_PROTOCOL_ITERATIONS` sets the iteration count (default 300).
//! `TIGER_PROTOCOL_RATIO` sets the ratio Tiger's work may not exceed, read as
//! `tiger / cpython` (default 1.0). `TIGER_PROTOCOL_ASSERT=1` makes the run the
//! threshold instead of a report. `TIGER_PYTHON` names the interpreter to run;
//! with none on `PATH` the test skips with a message, the way the browser tests
//! skip until their module has been built.

use std::time::Instant;

use tiger_conformance::{root, Shared};
use tiger_evaluator::{CompiledProgram, Engine, Evaluator, RunOptions};

/// The workload's sections, in the order both programs declare them, then the
/// probe: `overhead` names no section, so what it measures is what a run costs
/// before it does any work — the floor every light section is read against. A probe
/// is reported and not asserted.
const SECTIONS: &[&str] = &["all", "loop", "lists", "dicts", "strings", "methods", "recursion", "calls", "overhead"];

/// Sections the ratio is reported for but not required of: a probe answers no
/// section, so there is no work for another interpreter to be slower at.
const PROBES: &[&str] = &["overhead"];

/// The file name both sides run the workload under, which is what `sys.argv[0]`
/// reports and what a diagnostic would quote.
const WORKLOAD: &str = "workload.tg";

/// How many times each section runs. The two halves share it so they measure the
/// same thing; 300 makes the minimum of even the lightest section repeatable,
/// because a section is a fraction of a millisecond. A debug build is an order of
/// magnitude slower and its numbers are not the ones the ratio is about, so the
/// suite's own default is small there; `make protocol` runs it in release.
fn iteration_count() -> u32 {
    if let Some(count) = std::env::var("TIGER_PROTOCOL_ITERATIONS").ok().and_then(|value| value.parse().ok()) {
        return count;
    }
    if cfg!(debug_assertions) {
        5
    } else {
        300
    }
}

/// The ratio Tiger's work may not exceed, read as `tiger / cpython`.
fn target_ratio() -> f64 {
    std::env::var("TIGER_PROTOCOL_RATIO").ok().and_then(|value| value.parse().ok()).unwrap_or(1.0)
}

/// Whether this run is the threshold rather than a report.
///
/// The ratio is a comparison against whatever CPython is on `PATH`, which is not a
/// property of this repository: the same revision is under 1.00 against one release
/// and over 2.00 against another. So the assertion is asked for rather than assumed —
/// `make protocol` sets this, and a bare `cargo test` only reports the table.
fn asserting() -> bool {
    std::env::var("TIGER_PROTOCOL_ASSERT").is_ok_and(|value| value == "1")
}

/// Runs the workload `section` on the given engine, with the arguments the
/// program reads as `sys.argv`.
fn options(section: &str, engine: Engine) -> RunOptions {
    RunOptions { engine, max_steps: 0, arguments: vec![WORKLOAD.to_string(), section.to_string()] }
}

/// The best of `iterations` runs, and the checksum the last one printed.
///
/// The minimum is the estimate to read here: the noise on a sub-millisecond
/// measurement is all upward — a descheduled thread is slower, never faster.
fn best(count: u32, mut run: impl FnMut() -> (f64, String)) -> (f64, String) {
    let mut fastest = f64::INFINITY;
    let mut output = String::new();
    for _ in 0..count {
        let (elapsed, text) = run();
        fastest = fastest.min(elapsed);
        output = text;
    }
    (fastest, output)
}

/// One run of the whole workload: parse, lower, and execute, which is what the
/// front of the comparison times.
fn tiger_total(text: &str, options: &RunOptions) -> (f64, String) {
    let buffer = Shared::new();
    let start = Instant::now();
    tiger_evaluator::run_with_options(text, WORKLOAD, None, None, None, Box::new(Shared(buffer.0.clone())), options.clone())
        .expect("the workload must run");
    (start.elapsed().as_secs_f64() * 1000.0, buffer.text())
}

/// One run of the workload with the compile removed: a program parsed and lowered
/// once, then executed here. This is the number the comparison is really about.
fn tiger_work(program: &CompiledProgram, options: &RunOptions) -> (f64, String) {
    let buffer = Shared::new();
    let mut evaluator = Evaluator::new(Box::new(Shared(buffer.0.clone())));
    evaluator.source_path = WORKLOAD.to_string();
    evaluator.engine = options.engine;
    evaluator.max_steps = options.max_steps;
    evaluator.arguments = options.arguments.clone();
    let start = Instant::now();
    evaluator.execute_compiled(program).expect("the workload must run");
    (start.elapsed().as_secs_f64() * 1000.0, buffer.text())
}

/// One section's Tiger measurement, in milliseconds.
struct Tiger {
    /// Parse, lower, and execute.
    total: f64,
    /// Execute alone, over a program parsed once.
    work: f64,
    checksum: String,
}

/// Measures every section on Tiger, in process.
fn tiger_sections(text: &str, count: u32, engine: Engine) -> Vec<(&'static str, Tiger)> {
    // The parse and the lowering are the compile, so both happen once here, outside
    // every timed region, for the measurement that has the compile removed.
    let parsed = tiger_parser::parse_source(text, WORKLOAD).expect("the workload must parse");
    let program = Evaluator::compile(&parsed, WORKLOAD).expect("the workload must lower");
    SECTIONS
        .iter()
        .map(|&section| {
            let options = options(section, engine);
            // One warm-up run of each shape, so the first section measured does not
            // pay for a cold instruction cache and an empty pool.
            let _ = tiger_total(text, &options);
            let _ = tiger_work(&program, &options);
            let (work, checksum) = best(count, || tiger_work(&program, &options));
            let (total, _) = best(count, || tiger_total(text, &options));
            (section, Tiger { total, work, checksum: checksum.trim().to_string() })
        })
        .collect()
}

/// The interpreter to run the Python half with: `TIGER_PYTHON` when it is set,
/// otherwise the first of `python3` and `python` that answers.
fn python() -> Option<String> {
    let candidates: Vec<String> = match std::env::var("TIGER_PYTHON") {
        Ok(configured) => vec![configured],
        Err(_) => vec!["python3".to_string(), "python".to_string()],
    };
    candidates.into_iter().find(|candidate| {
        std::process::Command::new(candidate)
            .arg("-c")
            .arg("pass")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// One section's CPython measurement, in milliseconds.
struct Cpython {
    /// `compile()` plus `exec`.
    compile: f64,
    /// `exec` alone, over a code object compiled once.
    precompiled: f64,
    checksum: String,
}

/// Measures every section on CPython, in process, through the driver.
fn cpython_sections(python: &str, count: u32) -> Vec<(String, Cpython)> {
    let driver = root().join("benchmarks").join("python").join("protocol.py");
    let output = std::process::Command::new(python)
        .arg(&driver)
        .env("TIGER_PROTOCOL_ITERATIONS", count.to_string())
        .output()
        .expect("the Python protocol must run");
    assert!(output.status.success(), "{} failed:\n{}", driver.display(), String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(fields.len(), 4, "unreadable protocol row {line:?}");
            let row = Cpython {
                compile: fields[2].parse().expect("compile time"),
                precompiled: fields[3].parse().expect("precompiled time"),
                checksum: fields[1].to_string(),
            };
            (fields[0].to_string(), row)
        })
        .collect()
}

#[test]
fn tiger_beats_cpython_on_every_section() {
    let Some(python) = python() else {
        println!("no Python interpreter found; skipping the paired protocol. Set TIGER_PYTHON to name one.");
        return;
    };
    let count = iteration_count();
    let workload = root().join("benchmarks").join("python").join(WORKLOAD);
    let text = std::fs::read_to_string(&workload).expect("read the paired workload");
    // The program runs on a big-stack thread, as every harness in the repository
    // does, and the measurement happens inside it.
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let tiger = tiger_sections(&text, count, Engine::Auto);
            let cpython = cpython_sections(&python, count);
            assert_eq!(tiger.len(), cpython.len(), "the two sides disagree about their sections");
            let target = target_ratio();
            println!("minimum of {count} runs per section, in process, no process startup in either number");
            println!("{:<10}{:>12}{:>12}{:>12}{:>12}{:>9}", "section", "tiger", "python", "p.exec", "tiger work", "ratio");
            println!("{}", "-".repeat(67));
            let mut failures: Vec<String> = Vec::new();
            let mut table = String::new();
            for ((section, tiger), (name, cpython)) in tiger.iter().zip(cpython.iter()) {
                assert_eq!(*section, name, "the two sides disagree about their sections");
                if tiger.checksum != cpython.checksum {
                    failures.push(format!("{section}: the two programs disagree: tiger {}, python {}", tiger.checksum, cpython.checksum));
                    continue;
                }
                let ratio = tiger.work / cpython.precompiled;
                let line = format!(
                    "{:<10}{:>12}{:>12}{:>12}{:>12}{:>8.2}x",
                    section,
                    format!("{:.3}ms", tiger.total),
                    format!("{:.3}ms", cpython.compile),
                    format!("{:.3}ms", cpython.precompiled),
                    format!("{:.3}ms", tiger.work),
                    ratio
                );
                println!("{line}");
                table.push_str(&line);
                table.push('\n');
                if ratio > target && !PROBES.contains(section) {
                    failures
                        .push(format!("{section}: tiger {:.3}ms against python {:.3}ms ({ratio:.2}x)", tiger.work, cpython.precompiled));
                }
            }
            println!();
            println!("tiger is parse+lower+execute; python is compile+exec; p.exec is the same");
            println!("bytecode executed again, and tiger work is execute alone over a program");
            println!("parsed once, so tiger work and p.exec are the two that compare. ratio is");
            println!("tiger work over p.exec, and must not exceed {target:.2}.");
            (table, failures)
        })
        .expect("the protocol thread must start");
    // The assertion is made out here rather than in the thread, so a failure reports the
    // protocol's own table instead of a panic inside a thread that panicked.
    let (table, failures) = handle.join().expect("the protocol thread must not panic");
    // A debug build runs the interpreter several times slower and CPython exactly as
    // fast, so the ratio it reports is not the one the comparison is about; the
    // assertion belongs to a release build. And even there it is asked for rather than
    // assumed, so that a correctness run on a host with a faster CPython than the one
    // the ratio was read on reports the table without failing the build.
    if cfg!(debug_assertions) {
        println!("this is a debug build, so the ratio is reported and not asserted.");
        return;
    }
    if !asserting() {
        println!("this run reports the ratio and asserts nothing; `make protocol` is what fails.");
        return;
    }
    assert!(failures.is_empty(), "{} section(s) are not faster than CPython:\n{}\n\n{table}", failures.len(), failures.join("\n"));
}
