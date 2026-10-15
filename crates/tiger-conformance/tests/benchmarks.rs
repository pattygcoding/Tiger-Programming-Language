//! The 42 benchmark programs, compared against their `.txt` expectations.
//!
//! A port of `benchmarks/benchmarks_test.go`: the same 42/42 counts, the same
//! 100-200 line budget, the same orphan check, and the same CRLF normalization.
//!
//! A benchmark may import a helper from `benchmarks/support/`, which the
//! directory loader resolves from the program's own name; see `support/toolbox.tg`,
//! the module `42_module_exports.tg` uses.

use std::path::PathBuf;

use tiger_conformance::{normalize, run_in_directory, Files, RunOptions};

fn fixtures(extension: &str) -> Vec<PathBuf> {
    let directory = tiger_conformance::root().join("benchmarks");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("benchmarks directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().map(|extension_| extension_ == extension).unwrap_or(false))
        .collect();
    paths.sort();
    paths
}

fn stem(path: &PathBuf) -> String {
    path.file_stem().map(|name| name.to_string_lossy().to_string()).unwrap_or_default()
}

#[test]
fn corpus_is_complete() {
    let sources = fixtures("tg");
    let outputs = fixtures("txt");
    assert_eq!(sources.len(), 42, "want 42 Tiger programs, got {}", sources.len());
    assert_eq!(outputs.len(), 42, "want 42 expected outputs, got {}", outputs.len());
    for output in fixtures("txt").into_iter().chain(fixtures("in")) {
        let source = output.with_extension("tg");
        assert!(source.exists(), "orphan fixture {}", output.display());
    }
}

#[test]
fn programs_match_their_expected_output() {
    let sources = fixtures("tg");
    assert!(!sources.is_empty(), "no Tiger programs found");
    // `TIGER_BENCH_FILTER` narrows the run to programs whose name contains it,
    // which is how one workload or a family of them is checked in isolation.
    let filter = std::env::var("TIGER_BENCH_FILTER").ok().filter(|value| !value.is_empty());
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for path in &sources {
        let name = stem(path);
        if let Some(filter) = &filter {
            if !name.contains(filter.as_str()) {
                continue;
            }
        }
        checked += 1;
        let source = normalize(&std::fs::read_to_string(path).expect("read program"));
        let lines = source.trim_end_matches('\n').matches('\n').count() + 1;
        if !(100..=200).contains(&lines) {
            failures.push(format!("{} has {} lines; want 100-200", name, lines));
            continue;
        }
        let expected_path = path.with_extension("txt");
        let expected = normalize(&std::fs::read_to_string(&expected_path).expect("read expectation"));
        let input_path = path.with_extension("in");
        let input = std::fs::read_to_string(&input_path).ok().map(|text| normalize(&text));
        // The Go corpus embeds the fixtures, so programs are named by their bare
        // file name and diagnostics quote exactly that. Imports still resolve
        // inside the benchmarks directory, which is where `support/` lives.
        let directory = tiger_conformance::root().join("benchmarks");
        let outcome = run_in_directory(
            &source,
            &path.file_name().unwrap().to_string_lossy(),
            directory,
            Files::Memory,
            input.as_deref(),
            RunOptions::default(),
        );
        if let Some(error) = outcome.error {
            failures.push(format!("{}: {}", name, error));
            continue;
        }
        let actual = normalize(&outcome.output);
        if actual != expected {
            failures.push(format!("{}: output differs from {}\nwant:\n{}\ngot:\n{}", name, expected_path.display(), expected, actual));
            continue;
        }
        println!("{}: {} lines; output matches {}", name, lines, expected_path.display());
    }
    assert!(checked > 0, "TIGER_BENCH_FILTER matched no program");
    assert!(failures.is_empty(), "{} benchmark(s) failed:\n{}", failures.len(), failures.join("\n---\n"));
}
