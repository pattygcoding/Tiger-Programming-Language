//! Shared helpers for the evaluator's integration tests.
//!
//! These tests are the port of the Go package's `_test.go` files, which are the
//! behavioural spec for the language. Each one runs a program and compares its
//! output, or checks that it fails with a positioned message.
//!
//! Every test target compiles this module, so it allows the helpers a given
//! target does not use.

#![allow(dead_code)]

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;

/// Collects written bytes so a test can compare them.
pub struct Shared(pub Rc<RefCell<Vec<u8>>>);

impl Shared {
    pub fn new() -> Shared {
        Shared(Rc::new(RefCell::new(Vec::new())))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow()).to_string()
    }
}

impl Write for Shared {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// How a program should be run.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Files {
    /// The host filesystem, which is what Go's `Run` uses.
    Host,
    /// A fresh in-memory filesystem, which is what Go's `RunWithOptions` uses.
    Memory,
}

/// Runs a program the way Go's `evaluator.Run` does.
pub fn run(source: &str) -> Result<String, String> {
    run_with(source, Files::Host, None)
}

/// Runs a program against a fresh in-memory filesystem.
pub fn run_memory(source: &str) -> Result<String, String> {
    run_with(source, Files::Memory, None)
}

/// Runs a program with standard input connected.
pub fn run_input(source: &str, input: &str) -> Result<String, String> {
    run_with(source, Files::Memory, Some(input))
}

/// Runs a program on a big-stack thread, as the CLI does.
pub fn run_with(source: &str, files: Files, input: Option<&str>) -> Result<String, String> {
    let source = source.to_string();
    let input = input.map(str::to_string);
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let buffer = Shared::new();
            let program = tiger_parser::parse(&source)?;
            let mut evaluator = tiger_evaluator::Evaluator::new(Box::new(Shared(buffer.0.clone())));
            evaluator.filesystem = match files {
                Files::Host => None,
                Files::Memory => Some(Box::new(tiger_evaluator::MemoryFileSystem::default())),
            };
            evaluator.input = input.map(|text| Box::new(std::io::Cursor::new(text.into_bytes())) as Box<dyn BufRead>);
            evaluator.execute(&program)?;
            Ok(buffer.text())
        })
        .expect("the interpreter thread must start");
    handle.join().expect("the interpreter must not panic")
}

/// Runs a program whose input reader always fails.
pub fn run_input_failing(source: &str) -> Result<String, String> {
    struct Broken;
    impl std::io::Read for Broken {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "broken stdin"))
        }
    }
    let source = source.to_string();
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let buffer = Shared::new();
            let program = tiger_parser::parse(&source)?;
            let mut evaluator = tiger_evaluator::Evaluator::new(Box::new(Shared(buffer.0.clone())));
            evaluator.filesystem = Some(Box::new(tiger_evaluator::MemoryFileSystem::default()));
            evaluator.input = Some(Box::new(std::io::BufReader::new(Broken)));
            evaluator.execute(&program)?;
            Ok(buffer.text())
        })
        .expect("the interpreter thread must start");
    handle.join().expect("the interpreter must not panic")
}

/// Runs a program with a small step budget, as Go's execution-limit tests do.
pub fn run_with_steps(source: &str, steps: u64) -> Result<String, String> {
    let source = source.to_string();
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let buffer = Shared::new();
            let program = tiger_parser::parse(&source)?;
            let mut evaluator = tiger_evaluator::Evaluator::new(Box::new(Shared(buffer.0.clone())));
            evaluator.max_steps = steps;
            evaluator.execute(&program)?;
            Ok(buffer.text())
        })
        .expect("the interpreter thread must start");
    handle.join().expect("the interpreter must not panic")
}

/// Runs a program whose output writer always fails.
pub fn run_failing_output(source: &str) -> Result<String, String> {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _data: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "output closed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let source = source.to_string();
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let program = tiger_parser::parse(&source)?;
            let mut evaluator = tiger_evaluator::Evaluator::new(Box::new(Broken));
            evaluator.execute(&program)?;
            Ok(String::new())
        })
        .expect("the interpreter thread must start");
    handle.join().expect("the interpreter must not panic")
}

/// Runs a program that must succeed, returning its output.
#[track_caller]
pub fn output(source: &str) -> String {
    match run(source) {
        Ok(text) => text,
        Err(error) => panic!("{} failed: {}", source, error),
    }
}

/// Runs a program that must fail, returning its message.
#[track_caller]
pub fn failure(source: &str) -> String {
    match run(source) {
        Ok(text) => panic!("{} unexpectedly printed {:?}", source, text),
        Err(error) => error,
    }
}

/// Checks a table of `(source, expected output)` pairs.
#[track_caller]
pub fn expect_outputs(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (source, expected) in cases {
        match run(source) {
            Ok(text) if text == *expected => {}
            Ok(text) => failures.push(format!("{}\n  want {:?}\n  got  {:?}", source, expected, text)),
            Err(error) => failures.push(format!("{}\n  want {:?}\n  got error {}", source, expected, error)),
        }
    }
    assert!(failures.is_empty(), "{} case(s) failed:\n{}", failures.len(), failures.join("\n"));
}

/// Checks a table of `(source, expected error substring)` pairs. Every message
/// must be positioned, as Go's tests require.
#[track_caller]
pub fn expect_errors(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (source, expected) in cases {
        match run(source) {
            Ok(text) => failures.push(format!("{}\n  want error {:?}, got output {:?}", source, expected, text)),
            Err(error) => {
                if !error.contains(expected) {
                    failures.push(format!("{}\n  want error {:?}\n  got {}", source, expected, error));
                } else if !error.contains(':') {
                    failures.push(format!("{}\n  want a positioned error, got {}", source, error));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} case(s) failed:\n{}", failures.len(), failures.join("\n"));
}
