//! What the benchmark programs actually execute.
//!
//! The micro harness prices an operation and the disassembly shows a program's static
//! shape, but neither says how often a program runs each of them. This counts what the
//! instruction loop executes, so the two can be multiplied together. Only compiled with
//! the `profile` feature:
//!
//! ```sh
//! cargo test -p tiger-evaluator --features profile --test profile -- --ignored --nocapture
//! ```

#![cfg(feature = "profile")]

mod common;

use std::path::PathBuf;

use tiger_evaluator::profile;

/// The benchmark programs, in the order the corpus lists them.
fn benchmark_paths() -> Vec<PathBuf> {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("benchmarks");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(directory)
        .expect("benchmarks directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "tg"))
        .collect();
    paths.sort();
    paths
}

/// The programs the timing harness also leaves out: one needs input, one the host
/// filesystem, and two import a module the in-process runner has no loader for.
fn is_timed(name: &str) -> bool {
    !matches!(name, "19_file_io.tg" | "21_input.tg" | "36_stdlib.tg" | "42_module_exports.tg")
}

#[test]
#[ignore = "prints a dynamic instruction mix; run with --ignored --nocapture"]
fn where_the_benchmarks_spend_their_instructions() {
    let header: Vec<String> = profile::NAMES.iter().map(|name| format!("{:>6}", name)).collect();
    println!("{:<28}{:>12}   {}", "program", "instrs", header.join(""));
    let mut total = [0u64; profile::CATEGORIES];
    for path in benchmark_paths() {
        let name = path.file_name().expect("a name").to_string_lossy().to_string();
        if !is_timed(&name) {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("read program");
        profile::reset();
        common::run(&source).expect("run program");
        let counts = profile::counts();
        let instructions: u64 = counts.iter().sum();
        for (slot, count) in total.iter_mut().zip(counts.iter()) {
            *slot += count;
        }
        let shares: Vec<String> =
            counts.iter().map(|count| format!("{:>5.1}%", *count as f64 * 100.0 / instructions.max(1) as f64)).collect();
        println!("{:<28}{:>12}   {}", name, instructions, shares.join(""));
    }
    let instructions: u64 = total.iter().sum();
    let shares: Vec<String> = total.iter().map(|count| format!("{:>5.1}%", *count as f64 * 100.0 / instructions.max(1) as f64)).collect();
    println!("{:<28}{:>12}   {}", "ALL", instructions, shares.join(""));
}
