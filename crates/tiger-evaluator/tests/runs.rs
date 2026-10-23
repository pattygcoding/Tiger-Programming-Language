//! How one host runs many programs, which is what the shared built-ins and the
//! compile-once entry point have to keep true.
//!
//! The interpreter is embedded in hosts that run more than one program: these test
//! suites run thousands between them, the browser playground runs whatever is typed
//! into it, and the paired Tiger/Python protocol compiles one program and runs it
//! hundreds of times. Installing the built-ins is shared work between those runs, so
//! what a run may and may not inherit from the run before it is pinned here: the
//! values are shared, the bindings and the state are not.

mod common;

use common::Shared;
use tiger_evaluator::{CompiledProgram, Evaluator};

/// Runs a program in a fresh interpreter with the given `sys.argv`, and returns its
/// output.
fn run_as(source: &str, arguments: &[&str]) -> Result<String, String> {
    let buffer = Shared::new();
    let program = tiger_parser::parse(source)?;
    let mut evaluator = Evaluator::new(Box::new(Shared(buffer.0.clone())));
    evaluator.arguments = arguments.iter().map(|argument| argument.to_string()).collect();
    evaluator.execute(&program)?;
    Ok(buffer.text())
}

/// Runs a program with no arguments.
fn run(source: &str) -> Result<String, String> {
    run_as(source, &[])
}

/// Runs a program that has already been lowered, and returns its output.
fn run_compiled(program: &CompiledProgram, arguments: &[&str]) -> Result<String, String> {
    let buffer = Shared::new();
    let mut evaluator = Evaluator::new(Box::new(Shared(buffer.0.clone())));
    evaluator.source_path = "runs.tg".to_string();
    evaluator.arguments = arguments.iter().map(|argument| argument.to_string()).collect();
    evaluator.execute_compiled(program)?;
    Ok(buffer.text())
}

#[test]
fn a_run_owns_the_binding_it_declares() {
    // The built-in values are shared work between runs; the scopes a run declares into
    // are its own. A `const` that shadows a built-in is the run's own binding — and
    // the run after it must still find the built-in where it was.
    assert_eq!(run("const len = 7;\nprint(len);").expect("run"), "7\n");
    assert_eq!(run("print(len(\"abcd\"));").expect("run"), "4\n");
    // A built-in is bound as a `const`, so a program cannot reassign it and reach the
    // shared value every later run looks up.
    assert_eq!(run("len = 7;").expect_err("reassigning a built-in must be refused"), "1:1: cannot reassign const \"len\"");
}

#[test]
fn each_run_sees_its_own_arguments() {
    assert_eq!(run_as("print(sys.argv[1]);", &["runs.tg", "first"]).expect("run"), "first\n");
    assert_eq!(run_as("print(sys.argv[1]);", &["runs.tg", "second"]).expect("run"), "second\n");
    assert_eq!(run("print(len(sys.argv));").expect("run"), "0\n");
}

#[test]
fn sys_env_is_a_snapshot_each_run_owns() {
    // `sys.env` is documented as a snapshot, and the strings behind it are the shared
    // half. The dictionary itself is per run, so what one program writes into it does
    // not reach the next one.
    assert_eq!(run("sys.env[\"TIGER_RUNS_TEST\"] = \"first\";\nprint(sys.env[\"TIGER_RUNS_TEST\"]);").expect("run"), "first\n");
    assert_eq!(run("print(\"TIGER_RUNS_TEST\" in sys.env, sys.env.get(\"TIGER_RUNS_TEST\", \"unset\"));").expect("run"), "false unset\n");
}

#[test]
fn each_run_seeds_its_own_generator() {
    // The `random` module's generator belongs to the run, so an unseeded program
    // produces the same sequence however many programs ran before it.
    let first = run("print(random.random());").expect("run");
    let second = run("print(random.random());").expect("run");
    assert_eq!(first, second);
}

#[test]
fn a_program_compiled_once_runs_many_times() {
    // What the protocol times: the parse and the lowering are paid for once, and every
    // run over the same `CompiledProgram` starts where the first one did.
    let parsed = tiger_parser::parse("var total = 0;\nfor (var i = 0; i < 4; ++i) {\n    total += i;\n}\nprint(total);")
        .expect("the program must parse");
    let program = Evaluator::compile(&parsed, "runs.tg").expect("the program must lower");
    for _ in 0..3 {
        assert_eq!(run_compiled(&program, &[]).expect("run"), "6\n");
    }
}

#[test]
fn a_program_the_compiler_does_not_lower_has_no_compiled_form() {
    // A spread call is one of the shapes the linear engine does not cover, so the
    // compile reports none. That is the signal a host uses to run the program on the
    // tree-walker instead, which is what `execute` does for it.
    let source = "function pair(a, b) {\n    return a + b;\n}\nconst values = [1, 2];\nprint(pair(*values));";
    let parsed = tiger_parser::parse(source).expect("the program must parse");
    assert!(Evaluator::compile(&parsed, "runs.tg").is_none());
    assert_eq!(run(source).expect("run"), "3\n");
}
