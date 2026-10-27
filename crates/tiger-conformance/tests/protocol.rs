//! The in-process Tiger/Python protocol, and the ratio it has to hold.
//!
//! The measurement itself is `tiger_conformance::protocol`, so that the
//! performance dashboard in `tools/perf-dashboard` reads the same numbers this
//! test asserts on. What is here is the threshold: the table printed on every run,
//! and the failure reported when a section is slower than CPython and the run has
//! been asked for the assertion.
//!
//! `benchmarks/python/compare.py` times two whole processes, so both numbers carry
//! their launcher: the workload is one to three milliseconds of work against a five
//! to eighteen millisecond startup floor, and subtracting the floor amplifies its
//! jitter past the signal. This measures the interpreters instead, so no process
//! startup is in either number.
//!
//! Tiger is measured in process. CPython is measured by
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

use tiger_conformance::protocol::{combine, format_header, format_row, python, python_sections, tiger_sections, workload_text};
use tiger_conformance::Engine;

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

#[test]
fn tiger_beats_cpython_on_every_section() {
    let Some(python) = python() else {
        println!("no Python interpreter found; skipping the paired protocol. Set TIGER_PYTHON to name one.");
        return;
    };
    let count = iteration_count();
    let text = workload_text().expect("read the paired workload");
    // The program runs on a big-stack thread, as every harness in the repository
    // does, and the measurement happens inside it.
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let tiger = tiger_sections(&text, count, Engine::Auto, &mut |_| {}).expect("the workload must measure");
            let python = python_sections(&python, count).expect("the Python protocol must run");
            let sections = combine(tiger, python).expect("the two sides must agree about their sections");
            let target = target_ratio();
            println!("minimum of {count} runs per section, in process, no process startup in either number");
            print!("{}", format_header());
            let mut failures: Vec<String> = Vec::new();
            let mut table = String::new();
            for section in &sections {
                if !section.agrees() {
                    failures.push(format!(
                        "{}: the two programs disagree: tiger {}, python {}",
                        section.name, section.tiger_checksum, section.python_checksum
                    ));
                    continue;
                }
                let ratio = section.ratio();
                let line = format_row(section);
                println!("{line}");
                table.push_str(&line);
                table.push('\n');
                if ratio > target && !section.is_probe() {
                    failures.push(format!(
                        "{}: tiger {:.3}ms against python {:.3}ms ({ratio:.2}x)",
                        section.name, section.tiger_work, section.python_exec
                    ));
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
