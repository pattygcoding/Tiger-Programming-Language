//! The `sys` module: the program's arguments, its environment, and its exit status.
//!
//! A host passes the program's arguments in, so `sys.argv` is the program's own
//! path first and then each argument, exactly as Python's `sys.argv` is. `sys.exit`
//! stops the program with the status Python's would use, unwinding like a `return`:
//! every `finally` on the way out runs, and no `catch` clause takes it. Every case
//! here runs on both engines and must agree.

mod common;

use std::collections::HashMap;
use std::rc::Rc;

use common::{expect_errors, Shared};
use tiger_evaluator::{BundleLoader, Engine, Evaluator, Exit, ModuleLoader};

/// Runs `source` with `arguments` and an optional module loader on one engine,
/// returning its output and how it finished.
fn run_on(source: &str, arguments: &[&str], engine: Engine, loader: Option<BundleLoader>) -> (String, Result<Exit, String>) {
    let source = source.to_string();
    let arguments: Vec<String> = arguments.iter().map(|value| value.to_string()).collect();
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let buffer = Shared::new();
            let program = tiger_parser::parse(&source).expect("the fixture must parse");
            let mut evaluator = Evaluator::new(Box::new(Shared(buffer.0.clone())));
            evaluator.filesystem = Some(Box::new(tiger_evaluator::MemoryFileSystem::default()));
            evaluator.engine = engine;
            evaluator.arguments = arguments;
            evaluator.loader = loader.map(|bundle| Rc::new(bundle) as Rc<dyn ModuleLoader>);
            let outcome = evaluator.execute(&program);
            (buffer.text(), outcome)
        })
        .expect("the interpreter thread must start");
    handle.join().expect("the interpreter must not panic")
}

/// Runs `source` on both engines, requiring the same output and outcome.
#[track_caller]
fn both(source: &str, arguments: &[&str]) -> (String, Result<Exit, String>) {
    let compiled = run_on(source, arguments, Engine::Auto, None);
    let walked = run_on(source, arguments, Engine::Tree, None);
    assert_eq!(compiled.0, walked.0, "the engines disagree on the output of {:?}", source);
    assert_eq!(compiled.1, walked.1, "the engines disagree on the outcome of {:?}", source);
    compiled
}

#[test]
fn argv_holds_the_program_then_its_arguments() {
    let (output, outcome) = both("print(sys.argv, len(sys.argv), type(sys.argv));", &["greet.tg", "Ada", "Grace"]);
    assert_eq!(output, "[\"greet.tg\", \"Ada\", \"Grace\"] 3 list\n");
    assert_eq!(outcome, Ok(Exit::default()));

    // A host that runs a program without a name of its own leaves the list empty.
    let (output, outcome) = both("print(sys.argv, sys.argv[1:]);", &[]);
    assert_eq!(output, "[] []\n");
    assert_eq!(outcome, Ok(Exit::default()));

    // `sys.argv` is an ordinary list, so a program may read and change it.
    let (output, _) = both("sys.argv[0] = \"renamed\"; print(sys.argv[0], sys.argv[1:]);", &["greet.tg", "Ada"]);
    assert_eq!(output, "renamed [\"Ada\"]\n");
}

#[test]
fn argv_reaches_an_imported_module() {
    let bundle = arguments_bundle();
    let source = "import \"lib.tg\" as lib;\nprint(lib.arguments());";
    let compiled = run_on(source, &["main.tg", "one", "two"], Engine::Auto, Some(bundle));
    let walked = run_on(source, &["main.tg", "one", "two"], Engine::Tree, Some(arguments_bundle()));
    assert_eq!(compiled.0, "[\"one\", \"two\"]\n");
    assert_eq!(compiled, walked, "the engines disagree on an imported module's arguments");
}

/// The bundle for the imported-module test: one module that reads `sys.argv`.
fn arguments_bundle() -> BundleLoader {
    let mut bundle = BundleLoader::default();
    // The runner leaves `source_path` empty, so the importer's name is empty too.
    bundle.sources.insert("lib.tg".to_string(), "export function arguments() { return sys.argv[1:]; }".to_string());
    bundle.paths.insert(String::new(), HashMap::from([("lib.tg".to_string(), "lib.tg".to_string())]));
    bundle
}

#[test]
fn env_is_a_dictionary_and_process_variables_are_separate() {
    // The shape is what every host agrees on; the variables themselves come from
    // the process, which the standalone tests check end to end.
    let (output, _) = both("print(type(sys.env), \"TIGER_NO_SUCH_VARIABLE\" in sys.env);", &[]);
    assert_eq!(output, "dict false\n");
    let (output, _) = both("print(sys.env.get(\"TIGER_NO_SUCH_VARIABLE\", \"unset\"));", &[]);
    assert_eq!(output, "unset\n");

    // It is an ordinary dictionary: a program may add to it without touching the
    // process's own environment, which the next run reads again.
    let (output, _) = both("sys.env[\"TIGER_NO_SUCH_VARIABLE\"] = \"local\";\nprint(sys.env[\"TIGER_NO_SUCH_VARIABLE\"]);", &[]);
    assert_eq!(output, "local\n");
}

#[test]
fn exit_sets_the_status_python_would() {
    let cases: [(&str, i32, Option<&str>); 9] = [
        ("sys.exit();", 0, None),
        ("sys.exit(null);", 0, None),
        ("sys.exit(0);", 0, None),
        ("sys.exit(7);", 7, None),
        ("sys.exit(-3);", -3, None),
        ("sys.exit(false);", 0, None),
        ("sys.exit(true);", 1, None),
        ("sys.exit(\"stopping\");", 1, Some("stopping")),
        ("sys.exit(2.5);", 1, Some("2.5")),
    ];
    for (source, code, message) in cases {
        let (_, outcome) = both(source, &[]);
        assert_eq!(outcome, Ok(Exit { code, message: message.map(str::to_string) }), "wrong exit for {:?}", source);
    }
    // Anything else is reported on standard error with status 1, as in Python.
    let (_, outcome) = both("sys.exit([1, 2]);", &[]);
    assert_eq!(outcome, Ok(Exit { code: 1, message: Some("[1, 2]".to_string()) }));
}

#[test]
fn exit_stops_where_it_stands_but_runs_finally() {
    let source = "function stop() {\n    try {\n        print(\"try\");\n        sys.exit(5);\n    } catch (error) {\n        print(\"caught\");\n    } finally {\n        print(\"finally\");\n    }\n}\nstop();\nprint(\"after\");";
    let (output, outcome) = both(source, &[]);
    assert_eq!(output, "try\nfinally\n");
    assert_eq!(outcome, Ok(Exit { code: 5, message: None }));

    // A `finally` that exits overrides the in-flight one, as Python's does.
    let (_, outcome) = both("try {\n    sys.exit(1);\n} finally {\n    sys.exit(2);\n}", &[]);
    assert_eq!(outcome, Ok(Exit { code: 2, message: None }));

    // A loop's `else` does not run when the body exits.
    let source = "for value in [1, 2] {\n    sys.exit(3);\n} else {\n    print(\"else\");\n}";
    let (output, outcome) = both(source, &[]);
    assert_eq!(output, "");
    assert_eq!(outcome, Ok(Exit { code: 3, message: None }));

    // `sys.exit` is an ordinary value, so it can be stored or passed around.
    let source = "const stop = sys.exit;\nprint(\"start\");\nstop(4);\nprint(\"after\");";
    let (output, outcome) = both(source, &[]);
    assert_eq!(output, "start\n");
    assert_eq!(outcome, Ok(Exit { code: 4, message: None }));
}

#[test]
fn exit_reports_its_own_argument_errors() {
    expect_errors(&[
        ("sys.exit(1, 2);", "sys.exit expects at most 1 argument"),
        ("sys.exit(1, \"two\");", "sys.exit expects at most 1 argument"),
        ("sys.exit(status=1);", "sys.exit does not accept keyword arguments"),
    ]);
    // A call the runtime rejects is an ordinary error, so `catch` takes it, while an
    // exit itself is not catchable.
    let source = "try {\n    sys.exit(1, 2);\n} catch (error) {\n    print(\"caught\", error);\n}";
    let (output, outcome) = both(source, &[]);
    assert!(output.starts_with("caught ") && output.contains("sys.exit expects at most 1 argument"), "output was {:?}", output);
    assert_eq!(outcome, Ok(Exit::default()));
}
