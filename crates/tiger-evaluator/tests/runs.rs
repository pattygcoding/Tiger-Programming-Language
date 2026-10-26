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

#[test]
fn a_module_that_is_built_on_first_use_behaves_as_one_built_up_front() {
    // `random`, `secrets`, and `sys` are filled the first time a program reads a member,
    // not when the run starts. Nothing a program can observe may depend on when.
    let program = "print(type(sys), type(random), type(secrets));\nprint(len(sys.argv), type(sys.env), type(sys.exit));\nrandom.seed(7);\nconst first = random.int(0, 1000);\nrandom.seed(7);\nprint(first == random.int(0, 1000));\nprint(secrets.int(0, 0), len(secrets.choice([\"a\"])));";
    assert_eq!(run(program).expect("run"), "module module module\n0 dict builtin\ntrue\n0 1\n");
    // A module nobody reads is never built, and the run is the same as one that does not name it.
    assert_eq!(run("print(1 + 1);").expect("run"), "2\n");
}

#[test]
fn a_lazy_module_reports_the_same_diagnostics() {
    // The member that is not there is reported exactly as a module that was built up front would
    // report it, which means the fill happened before the lookup failed.
    let error = run("print(sys.nope);").expect_err("a missing member must fail");
    assert!(error.contains("nope"), "{error}");
    let error = run("random.seed();").expect_err("a bad call must fail");
    assert!(error.contains("random.seed expects 1 argument, got 0"), "{error}");
    // A built-in module is shared by reference once read: two reads see one object, so state a
    // program keeps in it is the state the next read finds.
    assert_eq!(
        run("sys.env[\"TIGER_LAZY_TEST\"] = \"x\";\nprint(sys.env[\"TIGER_LAZY_TEST\"], sys.env == sys.env);").expect("run"),
        "x true\n"
    );
}

#[test]
fn a_lazy_module_is_still_owned_by_its_run() {
    // Seeding in one run must not reach the next, and each run's arguments are its own, even
    // though the module is filled long after the run was set up.
    assert_eq!(
        run("random.seed(3);\nprint(random.int(0, 1000000));").expect("run"),
        run("random.seed(3);\nprint(random.int(0, 1000000));").expect("run")
    );
    assert_eq!(run_as("print(sys.argv);", &["a.tg", "one"]).expect("run"), "[\"a.tg\", \"one\"]\n");
    assert_eq!(run_as("print(sys.argv);", &["b.tg"]).expect("run"), "[\"b.tg\"]\n");
    // A module imported by name is the same lazily-built one.
    assert_eq!(run("const kind = type(random);\nprint(kind);").expect("run"), "module\n");
}

#[test]
fn a_member_built_on_demand_is_built_once_and_stays_a_constant() {
    // `sys.env` is the one member that is built when it is named, not when its module is
    // filled. Reading `argv` first must not change what `env` is, two reads must see one
    // dictionary, and it must refuse assignment as `argv` does.
    assert_eq!(
        run("print(len(sys.argv));\nsys.env[\"TIGER_ON_DEMAND\"] = \"x\";\nprint(sys.env[\"TIGER_ON_DEMAND\"], sys.env == sys.env, len(sys.argv));").expect("run"),
        "0\nx true 0\n"
    );
    let argv_error = run("sys.argv = [];").expect_err("a constant member cannot be assigned");
    let env_error = run("sys.env = {};").expect_err("a constant member cannot be assigned");
    assert_eq!(argv_error.replace("argv", "env"), env_error);
    // The member is built per run: a write in this run is gone in the next.
    assert_eq!(run("print(\"TIGER_ON_DEMAND\" in sys.env);").expect("run"), "false\n");
}

#[test]
fn a_program_can_shadow_a_built_in_module_name() {
    // The modules live in a scope of their own in front of the shared built-ins, so a program's
    // own binding of the same name has to win over both, and the next run must not see it.
    assert_eq!(run("var sys = 5;\nprint(sys + 1);").expect("run"), "6\n");
    assert_eq!(run("function random() { return 4; }\nprint(random());").expect("run"), "4\n");
    assert_eq!(run("print(type(sys), type(random));").expect("run"), "module module\n");
}
