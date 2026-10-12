//! The `examples/` and `portfolio-features/` programs.
//!
//! A port of `cmd/tiger/main_test.go`'s `TestExamplePrograms` and
//! `TestPortfolioInteractivePrograms`: every program must exit cleanly with
//! output and no error, and the interactive ones must react to their input.

use std::path::PathBuf;

use tiger_conformance::{run, Files};

fn programs(directory: &str) -> Vec<PathBuf> {
    let path = tiger_conformance::root().join(directory);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&path)
        .expect("program directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().map(|extension| extension == "tg").unwrap_or(false))
        .collect();
    paths.sort();
    paths
}

#[test]
fn example_programs_run() {
    let mut failures: Vec<String> = Vec::new();
    for directory in ["examples", "portfolio-features"] {
        let paths = programs(directory);
        assert!(!paths.is_empty(), "find {} programs", directory);
        for path in paths {
            let source = std::fs::read_to_string(&path).expect("read program");
            let outcome = run(&source, &path.to_string_lossy(), Files::Host, true, Some(""));
            let name = format!("{}/{}", directory, path.file_name().unwrap().to_string_lossy());
            if let Some(error) = outcome.error {
                failures.push(format!("{}: {}", name, error));
            } else if outcome.output.is_empty() {
                failures.push(format!("{}: produced no output", name));
            }
        }
    }
    assert!(failures.is_empty(), "{} program(s) failed:\n{}", failures.len(), failures.join("\n---\n"));
}

#[test]
fn portfolio_interactive_programs() {
    let scenarios: [(&str, &str, &[&str]); 4] = [
        ("interactive_quiz.tg", "", &["What is your name? Ada\n", "Ada scored 5/6 (83%), best streak 5, grade B\n"]),
        ("interactive_quiz.tg", "Bob\nvar\n", &["Welcome, Bob!", "Q2. What does 7 / 2 evaluate to? 3.5\n", "Bob scored 5/6 (83%)"]),
        ("connect_four.tg", "1\n2\n1\n2\n1\n2\n1\n", &["Player X wins!\n"]),
        (
            "connect_four.tg",
            "1\n1\n1\n1\n1\n1\n1\n0\n8\nabc\n",
            &[
                "Column 1 is full.",
                "\"0\" is out of range",
                "\"8\" is out of range",
                "\"abc\" is not a whole number",
                "Input closed. Goodbye.\n",
            ],
        ),
    ];
    let mut failures: Vec<String> = Vec::new();
    for (program, input, wanted) in scenarios {
        let path = tiger_conformance::root().join("portfolio-features").join(program);
        let source = std::fs::read_to_string(&path).expect("read program");
        let outcome = run(&source, &path.to_string_lossy(), Files::Host, true, Some(input));
        if let Some(error) = outcome.error {
            failures.push(format!("{} with input {:?}: {}", program, input, error));
            continue;
        }
        for want in wanted {
            if !outcome.output.contains(want) {
                failures.push(format!("{} with input {:?}: missing {:?} in:\n{}", program, input, want, outcome.output));
            }
        }
    }
    assert!(failures.is_empty(), "{} scenario(s) failed:\n{}", failures.len(), failures.join("\n---\n"));
}
