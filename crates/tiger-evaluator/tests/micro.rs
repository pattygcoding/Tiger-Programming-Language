//! In-process micro-benchmarks for the interpreter's hot paths.
//!
//! Each case runs a fixed number of operations and reports the best of several
//! runs, so a per-operation cost can be read off directly. Ignored by default, so
//! it never slows `cargo test`:
//!
//! ```sh
//! cargo test -p tiger-evaluator --test micro --release -- --ignored --nocapture
//! ```
//!
//! `TIGER_MICRO_ITERATIONS` sets how many times each case runs (default 25).
//! `TIGER_ENGINE=tree` runs every case on the tree-walker instead, which is how
//! the compiled engine's share of a workload is measured without a profiler.

mod common;

use std::time::Instant;

use common::Shared;
use tiger_evaluator::{Engine, Evaluator};

/// One case: a name, the program, and how many times its inner operation runs.
struct Case {
    name: &'static str,
    source: &'static str,
    operations: u64,
}

const CASES: &[Case] = &[
    // The baseline every other case is read against: a loop that does nothing but
    // count, with the same trip count as the call case.
    Case {
        name: "loop/count",
        source: "var s = 0;\nfor (var i = 0; i < 200000; ++i) { s += 1; }\nprint(s);",
        operations: 200_000,
    },
    // A call, which is the operation real programs spend most of their time in.
    Case {
        name: "call/empty",
        source: "function f(v) { return v; }\nvar s = 0;\nfor (var i = 0; i < 200000; ++i) { s += f(1); }\nprint(s);",
        operations: 200_000,
    },
    // A call that passes two arguments, so the marginal cost of an argument shows.
    Case {
        name: "call/two-args",
        source: "function f(a, b) { return a + b; }\nvar s = 0;\nfor (var i = 0; i < 200000; ++i) { s += f(1, 2); }\nprint(s);",
        operations: 200_000,
    },
    // A method call, which resolves a member before it calls.
    Case {
        name: "call/method",
        source: "class Box { var v = 1; Box() {} function get() { return this.v; } }\nconst b = Box();\nvar s = 0;\nfor (var i = 0; i < 200000; ++i) { s += b.get(); }\nprint(s);",
        operations: 200_000,
    },
    // A built-in call, which is a Rust function behind a closure.
    Case {
        name: "call/builtin",
        source: "var s = 0;\nfor (var i = 0; i < 200000; ++i) { s += len(\"abc\"); }\nprint(s);",
        operations: 200_000,
    },
    // The same call, but through a flat local alias, which skips the name lookup
    // entirely. Read against `call/builtin`, this is what a global name read costs —
    // which is the number the name cache is measured against.
    Case {
        name: "call/alias",
        source: "const l = len;\nvar s = 0;\nfor (var i = 0; i < 200000; ++i) { s += l(\"abc\"); }\nprint(s);",
        operations: 200_000,
    },
    // A call whose value is dropped, which is what a call written as a statement is.
    // The compiler folds the `Pop` into the call, so this is what that is worth.
    Case {
        name: "call/discard",
        source: "function f(v) { return v; }\nfor (var i = 0; i < 200000; ++i) { f(1); }\nprint(\"done\");",
        operations: 200_000,
    },
    // The same shape for a built-in, which takes the other fast path.
    Case {
        name: "call/discard-builtin",
        source: "for (var i = 0; i < 200000; ++i) { len(\"abc\"); }\nprint(\"done\");",
        operations: 200_000,
    },
    Case {
        name: "list/read",
        source: "const a = [n for n in range(1000)];\nvar s = 0;\nfor (var r = 0; r < 200; ++r) { for (var i = 0; i < 1000; ++i) { s += a[i]; } }\nprint(s);",
        operations: 200_000,
    },
    // The same read, but the comprehension binds the loop's own name. The
    // capture analysis is name-based, so this forces `i` into the environment and
    // costs the flat slot — which is what the two cases together measure.
    Case {
        name: "list/read-collide",
        source: "const a = [i for i in range(1000)];\nvar s = 0;\nfor (var r = 0; r < 200; ++r) { for (var i = 0; i < 1000; ++i) { s += a[i]; } }\nprint(s);",
        operations: 200_000,
    },
    Case {
        name: "list/write",
        source: "const a = [n for n in range(1000)];\nfor (var r = 0; r < 200; ++r) { for (var i = 0; i < 1000; ++i) { a[i] = i; } }\nprint(a[0]);",
        operations: 200_000,
    },
    Case {
        name: "list/write-collide",
        source: "const a = [i for i in range(1000)];\nfor (var r = 0; r < 200; ++r) { for (var i = 0; i < 1000; ++i) { a[i] = i; } }\nprint(a[0]);",
        operations: 200_000,
    },
    Case {
        name: "dict/set",
        source: "const d = {};\nfor (var i = 0; i < 200000; ++i) { d[\"k\" + str(i % 64)] = i; }\nprint(len(d));",
        operations: 200_000,
    },
    Case {
        name: "dict/get",
        source: "const d = {};\nfor (var i = 0; i < 64; ++i) { d[\"k\" + str(i)] = i; }\nvar s = 0;\nfor (var i = 0; i < 200000; ++i) { s += d[\"k\" + str(i % 64)]; }\nprint(s);",
        operations: 200_000,
    },
    Case {
        name: "string/concat",
        source: "var s = \"\";\nfor (var i = 0; i < 2000; ++i) { s += \"ab\"; }\nprint(len(s));",
        operations: 2_000,
    },
    Case {
        name: "field/read",
        source: "class Box { var v = 7; Box() {} }\nconst b = Box();\nvar s = 0;\nfor (var i = 0; i < 200000; ++i) { s += b.v; }\nprint(s);",
        operations: 200_000,
    },
    Case {
        name: "field/write",
        source: "class Box { var v = 7; Box() {} }\nconst b = Box();\nfor (var i = 0; i < 200000; ++i) { b.v = i; }\nprint(b.v);",
        operations: 200_000,
    },
    Case {
        name: "recursion/fib20",
        source: "function fib(n) { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); }\nprint(fib(20));",
        operations: 21_891,
    },
];

/// Runs `source` once on `engine` and returns how long it took, in milliseconds.
fn run_once(source: &str, engine: Engine) -> f64 {
    let program = tiger_parser::parse(source).expect("the case must parse");
    let buffer = Shared::new();
    let mut evaluator = Evaluator::new(Box::new(Shared(buffer.0.clone())));
    evaluator.engine = engine;
    // The cases are long enough to pass the default limit, and this measures the
    // interpreter rather than the guard.
    evaluator.max_steps = 0;
    let start = Instant::now();
    evaluator.execute(&program).expect("the case must run");
    start.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "micro-benchmarks; run explicitly with --ignored --nocapture"]
fn where_a_program_spends_its_time() {
    // Splits each benchmark program's in-process cost into parsing and everything
    // else, which is what says whether a change to the engine or to the parser is
    // the one that matters.
    let iterations: u32 = std::env::var("TIGER_MICRO_ITERATIONS").ok().and_then(|value| value.parse().ok()).unwrap_or(25);
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("benchmarks");
    let mut sources: Vec<(String, String)> = std::fs::read_dir(&directory)
        .expect("benchmarks directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().map(|extension| extension == "tg").unwrap_or(false))
        .map(|path| {
            let name = path.file_name().expect("file name").to_string_lossy().to_string();
            (name, std::fs::read_to_string(&path).expect("read program"))
        })
        .collect();
    sources.sort();
    // A program that needs standard input or a module loader cannot run here. Its
    // failure is caught and reported as such, so the default hook is silenced while
    // the measurements run rather than printing a backtrace per iteration.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    println!("{:<30}{:>10}{:>10}{:>10}{:>10}", "program", "lex ms", "parse ms", "run ms", "parse %");
    println!("{}", "-".repeat(70));
    let mut parse_total = 0.0f64;
    let mut lex_total = 0.0f64;
    let mut run_total = 0.0f64;
    for (name, source) in &sources {
        let _ = tiger_parser::parse(source);
        let mut best_lex = f64::INFINITY;
        for _ in 0..iterations {
            let start = Instant::now();
            let _ = tiger_lexer::scan(source).expect("scan");
            best_lex = best_lex.min(start.elapsed().as_secs_f64() * 1000.0);
        }
        let mut best_parse = f64::INFINITY;
        for _ in 0..iterations {
            let start = Instant::now();
            let _ = tiger_parser::parse(source).expect("parse");
            best_parse = best_parse.min(start.elapsed().as_secs_f64() * 1000.0);
        }
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_once(source, Engine::Auto)));
        let mut best_run = f64::INFINITY;
        for _ in 0..iterations {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_once(source, Engine::Auto))) {
                Ok(elapsed) => best_run = best_run.min(elapsed),
                // A program that needs standard input cannot run here; it is left
                // out of the totals rather than failing the harness.
                Err(_) => best_run = f64::NAN,
            }
        }
        if best_run.is_nan() {
            println!("{:<30}{:>9.4}ms{:>9.4}ms{:>10}{:>10}", name, best_lex, best_parse, "needs input", "-");
            continue;
        }
        lex_total += best_lex;
        parse_total += best_parse;
        run_total += best_run;
        println!("{:<30}{:>9.4}ms{:>9.4}ms{:>9.4}ms{:>9.0}%", name, best_lex, best_parse, best_run, best_parse / best_run * 100.0);
    }
    println!("{}", "-".repeat(70));
    println!("{:<30}{:>9.4}ms{:>9.4}ms{:>9.4}ms{:>9.0}%", "TOTAL", lex_total, parse_total, run_total, parse_total / run_total * 100.0);
    println!(
        "lexer is {:.0}% of the parse; the parser is the other {:.0}%",
        lex_total / parse_total * 100.0,
        (parse_total - lex_total) / parse_total * 100.0
    );
    std::panic::set_hook(previous_hook);
}

#[test]
#[ignore = "micro-benchmarks; run explicitly with --ignored --nocapture"]
fn hot_paths() {
    let iterations: u32 = std::env::var("TIGER_MICRO_ITERATIONS").ok().and_then(|value| value.parse().ok()).unwrap_or(25);
    let engine = match std::env::var("TIGER_ENGINE").as_deref() {
        Ok("tree") => Engine::Tree,
        _ => Engine::Auto,
    };
    println!("{:<18}{:>12}{:>14}", "case", "ms", "ns/op");
    println!("{}", "-".repeat(44));
    for case in CASES {
        // One warm-up run, then the best of `iterations`.
        let _ = run_once(case.source, engine);
        let mut best = f64::INFINITY;
        for _ in 0..iterations {
            best = best.min(run_once(case.source, engine));
        }
        println!("{:<18}{:>11.3}ms{:>13.1}", case.name, best, best * 1_000_000.0 / case.operations as f64);
    }
}
