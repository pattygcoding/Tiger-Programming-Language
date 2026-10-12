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

use std::path::PathBuf;
use std::time::Instant;

use tiger_conformance::{run, Files};

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
    let _ = tiger_evaluator::run_with_input(&source, &name, None, None, None, Box::new(std::io::sink()));
    let mut best = f64::INFINITY;
    let mut total = 0.0;
    for _ in 0..iterations {
        let start = Instant::now();
        tiger_evaluator::run_with_input(&source, &name, None, None, None, Box::new(std::io::sink())).expect("run");
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
