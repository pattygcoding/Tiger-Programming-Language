//! A wall-clock timing harness for the benchmark programs.
//!
//! This is not a correctness check and is ignored by default, so it never slows
//! `cargo test`. Run it explicitly to compare interpreter revisions in-process
//! (no process startup), for example:
//!
//! ```sh
//! cargo test -p tiger-conformance --test timing --release -- --ignored --nocapture
//! ```
//!
//! `TIGER_TIMING_ITERATIONS` sets how many times each program runs (default 10).
//! `TIGER_ENGINE=tree` runs `time_source_file`'s program on the tree-walking
//! interpreter instead of the compiled engine, which is how the compiled engine's
//! share of a workload is measured without a profiler. `TIGER_SHOW_OUTPUT=1` keeps
//! the program's own output, which a program that times itself needs in order to
//! report its numbers.

use std::path::PathBuf;
use std::time::Instant;

use tiger_conformance::{run, Engine, Files, RunOptions};

fn benchmark_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(tiger_conformance::root().join("benchmarks"))
        .expect("benchmarks directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().map(|extension| extension == "tg").unwrap_or(false))
        .collect();
    paths.sort();
    paths
}

#[test]
#[ignore = "times one program from TIGER_BENCH_FILE in-process; set TIGER_BENCH_ITERATIONS"]
fn time_source_file() {
    let path = std::env::var("TIGER_BENCH_FILE").expect("set TIGER_BENCH_FILE to a .tg file");
    let iterations: u32 = std::env::var("TIGER_BENCH_ITERATIONS").ok().and_then(|value| value.parse().ok()).unwrap_or(30);
    let source = std::fs::read_to_string(&path).expect("read program");
    let name = std::path::Path::new(&path).file_name().expect("file name").to_string_lossy().to_string();
    // `TIGER_ENGINE=tree` runs the same program on the tree-walker instead, which
    // is how the compiled engine's share of a workload is measured in-process.
    let mut options = RunOptions { max_steps: 0, ..RunOptions::default() };
    if std::env::var("TIGER_ENGINE").is_ok_and(|value| value == "tree") {
        options.engine = Engine::Tree;
    }
    // `TIGER_SHOW_OUTPUT=1` keeps the program's own output visible, which a
    // benchmark that times itself needs in order to report its numbers.
    let show = std::env::var_os("TIGER_SHOW_OUTPUT").is_some();
    let run_once = |source: &str, name: &str| {
        let writer: Box<dyn std::io::Write> = if show { Box::new(std::io::stdout()) } else { Box::new(std::io::sink()) };
        tiger_evaluator::run_with_options(source, name, None, None, None, writer, options).expect("run")
    };
    let _ = run_once(&source, &name);
    let mut best = f64::INFINITY;
    let mut total = 0.0;
    for _ in 0..iterations {
        let start = Instant::now();
        run_once(&source, &name);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        best = best.min(elapsed);
        total += elapsed;
    }
    println!("{name}: min {best:.3} ms, mean {:.3} ms ({iterations} iterations)", total / f64::from(iterations));
}

#[test]
#[ignore = "timing harness; run explicitly with --ignored --nocapture"]
fn time_benchmark_programs() {
    let iterations: u32 = std::env::var("TIGER_TIMING_ITERATIONS").ok().and_then(|value| value.parse().ok()).unwrap_or(10);
    let mut total = 0.0f64;
    for path in benchmark_paths() {
        let source = std::fs::read_to_string(&path).expect("read program");
        let input = std::fs::read_to_string(path.with_extension("in")).ok();
        let name = path.file_name().expect("file name").to_string_lossy().to_string();
        // One warm-up run, then time the batch.
        let _ = run(&source, &name, Files::Memory, false, input.as_deref());
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = run(&source, &name, Files::Memory, false, input.as_deref());
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        total += elapsed;
        println!("{:<34} {:>8.1} ms  ({iterations} iterations)", name, elapsed);
    }
    println!("{:<34} {:>8.1} ms  total", "TOTAL", total);
}
