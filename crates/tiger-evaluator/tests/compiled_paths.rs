//! The compiled paths a compound assignment, an update, and a branch take.
//!
//! A compound assignment or an update through an index or a property resolves its
//! target once, reads it, then evaluates the right-hand side and stores back; a
//! comparison that only feeds a branch never becomes a `bool` value; and a
//! constant expression is folded when the program is compiled. Each of those is a
//! place where the compiled engine could drift from the tree-walker, so every case
//! here runs on both engines and must agree on the value, the diagnostic, and its
//! position.

mod common;

use common::Shared;
use tiger_evaluator::{Engine, Evaluator, Exit};

/// Runs `source` on one engine, returning its output and how it finished.
fn run_on(source: &str, engine: Engine) -> (String, Result<Exit, String>) {
    let source = source.to_string();
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let buffer = Shared::new();
            let program = tiger_parser::parse(&source).expect("the fixture must parse");
            let mut evaluator = Evaluator::new(Box::new(Shared(buffer.0.clone())));
            evaluator.filesystem = Some(Box::new(tiger_evaluator::MemoryFileSystem::default()));
            evaluator.engine = engine;
            // The two engines count steps differently, so no limit is set and only
            // the result is compared.
            evaluator.max_steps = 0;
            let outcome = evaluator.execute(&program);
            (buffer.text(), outcome)
        })
        .expect("the interpreter thread must start");
    handle.join().expect("the interpreter must not panic")
}

/// Runs `source` on both engines and requires them to agree, returning the
/// compiled engine's result.
#[track_caller]
fn both(source: &str) -> (String, Result<Exit, String>) {
    let compiled = run_on(source, Engine::Auto);
    let walked = run_on(source, Engine::Tree);
    assert_eq!(compiled.0, walked.0, "the engines disagree on the output of {:?}", source);
    assert_eq!(compiled.1, walked.1, "the engines disagree on the outcome of {:?}", source);
    compiled
}

/// Runs a program that must succeed on both engines, returning its output.
#[track_caller]
fn both_output(source: &str) -> String {
    match both(source) {
        (output, Ok(_)) => output,
        (_, Err(error)) => panic!("{:?} failed: {}", source, error),
    }
}

/// Checks a table of `(source, expected output)` pairs on both engines.
#[track_caller]
fn expect_both_outputs(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (source, expected) in cases {
        match both(source) {
            (output, Ok(_)) if output == *expected => {}
            (output, Ok(_)) => failures.push(format!("{:?}\n  want {:?}\n  got  {:?}", source, expected, output)),
            (_, Err(error)) => failures.push(format!("{:?}\n  want {:?}\n  got error {}", source, expected, error)),
        }
    }
    assert!(failures.is_empty(), "{} case(s) failed:\n{}", failures.len(), failures.join("\n"));
}

/// Checks a table of `(source, expected error substring)` pairs on both engines.
#[track_caller]
fn expect_both_errors(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (source, expected) in cases {
        match both(source) {
            (_, Err(error)) if error.contains(*expected) => {}
            (_, Err(error)) => failures.push(format!("{:?}\n  want error {:?}\n  got {}", source, expected, error)),
            (output, Ok(_)) => failures.push(format!("{:?}\n  want error {:?}, got output {:?}", source, expected, output)),
        }
    }
    assert!(failures.is_empty(), "{} case(s) failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn compound_assignment_through_an_index() {
    expect_both_outputs(&[
        ("const items = [1, 2, 3]; items[1] += 10; print(items);", "[1, 12, 3]\n"),
        ("const items = [1, 2, 3]; items[1] -= 10; print(items);", "[1, -8, 3]\n"),
        ("const items = [2, 3]; items[0] *= 4; print(items);", "[8, 3]\n"),
        ("const items = [7]; items[0] %= 4; print(items);", "[3]\n"),
        ("const counts = {\"a\": 1}; counts[\"a\"] += 5; print(counts[\"a\"]);", "6\n"),
        ("const items = [1, 2, 3]; items[-1] += 1; print(items);", "[1, 2, 4]\n"),
        ("const d = {\"1\": 1, 1: 2}; d[1] += 1; d[\"1\"] += 10; print(d);", "{\"1\": 11, 1: 3}\n"),
        ("const rows = [[1], [2]]; rows[1][0] += 5; print(rows);", "[[1], [7]]\n"),
    ]);
}

#[test]
fn compound_assignment_resolves_its_target_once() {
    expect_both_outputs(&[
        (
            "var calls = 0;\n\
             function index() { calls += 1; return 0; }\n\
             const items = [5];\n\
             items[index()] += 2;\n\
             print(items[0], calls);",
            "7 1\n",
        ),
        (
            "var seen = [];\n\
             function note(text) { seen += [text]; return 1; }\n\
             const items = [10];\n\
             items[0] += note(\"rhs\");\n\
             print(items[0], seen);",
            "11 [\"rhs\"]\n",
        ),
        (
            "var seen = [];\n\
             function receiver() { seen += [\"target\"]; return store; }\n\
             function note() { seen += [\"rhs\"]; return 1; }\n\
             const store = {\"n\": 4};\n\
             receiver()[\"n\"] += note();\n\
             print(store[\"n\"], seen);",
            "5 [\"target\", \"rhs\"]\n",
        ),
    ]);
}

#[test]
fn compound_assignment_through_a_property() {
    expect_both_outputs(&[
        (
            "class Counter { var value = 0; Counter() {} }\n\
             const counter = Counter();\n\
             counter.value += 3; print(counter.value);\n\
             counter.value *= 2; print(counter.value);\n\
             counter.value -= 1; print(counter.value);",
            "3\n6\n5\n",
        ),
        (
            "var calls = 0;\n\
             class Box { var value = 0; Box() {} }\n\
             const box = Box();\n\
             function receiver() { calls += 1; return box; }\n\
             receiver().value += 7;\n\
             print(box.value, calls);",
            "7 1\n",
        ),
        (
            "class Sealed { private var hidden = 1; Sealed() {} function bump() { this.hidden += 4; return this.hidden; } }\n\
             const sealed = Sealed();\n\
             print(sealed.bump());",
            "5\n",
        ),
    ]);
}

#[test]
fn update_through_an_index() {
    expect_both_outputs(&[
        ("const items = [1, 2, 3]; print(items[0]++, items[0], ++items[0], items[0]);", "1 2 3 3\n"),
        ("const items = [5]; print(items[0]--, items[0]);", "5 4\n"),
        ("const counts = {\"a\": 1}; counts[\"a\"]++; print(counts[\"a\"]);", "2\n"),
        ("const counts = {\"a\": 1}; print(++counts[\"a\"], counts[\"a\"]);", "2 2\n"),
        ("const rows = [[1, 2]]; rows[0][1]++; print(rows);", "[[1, 3]]\n"),
        (
            "var calls = 0;\n\
             function index() { calls += 1; return 1; }\n\
             const items = [1, 2];\n\
             items[index()]++;\n\
             print(items, calls);",
            "[1, 3] 1\n",
        ),
    ]);
}

#[test]
fn update_through_a_property() {
    expect_both_outputs(&[
        (
            "class Counter { var value = 0; Counter() {} }\n\
             const counter = Counter();\n\
             counter.value++; counter.value++;\n\
             print(counter.value, counter.value++, counter.value);",
            "2 2 3\n",
        ),
        (
            "var calls = 0;\n\
             class Box { var value = 1; Box() {} }\n\
             const box = Box();\n\
             function receiver() { calls += 1; return box; }\n\
             receiver().value--;\n\
             print(box.value, calls);",
            "0 1\n",
        ),
    ]);
}

#[test]
fn comparisons_that_only_feed_a_branch() {
    expect_both_outputs(&[
        ("var n = 0; for (var i = 0; i < 5; ++i) { n += i; } print(n);", "10\n"),
        ("const limit = 4; var n = 0; for (var i = 0; i <= limit; ++i) { n += 1; } print(n);", "5\n"),
        ("var i = 3; var n = 0; while (i > 0) { n += i; i -= 1; } print(n);", "6\n"),
        ("const limit = 3; var i = 0; var n = 0; while i < limit { n += 1; i += 1; } print(n);", "3\n"),
        ("const value = 7; if value > 5 { print(\"big\"); } else { print(\"small\"); }", "big\n"),
        ("var n = 0; for (var i = 10; i > 1; i -= 3) { n += 1; } print(n);", "3\n"),
        ("print(3 < 5, 5 < 3, 3 == 3, 3 != 3);", "true false true false\n"),
        ("var flags = []; for (var i = 0; i < 3; ++i) { flags += [i < 2]; } print(flags);", "[true, true, false]\n"),
        // Short-circuiting keeps its own shape, so `and`/`or` must not become a
        // plain branch.
        (
            "var calls = 0; function yes() { calls += 1; return true; } function no() { calls += 1; return false; } print(no() and yes(), calls);",
            "false 1\n",
        ),
        (
            "var calls = 0; function yes() { calls += 1; return true; } function no() { calls += 1; return false; } print(yes() or no(), calls);",
            "true 1\n",
        ),
        // A comparison on strings and on mixed types still agrees.
        ("print(\"a\" < \"b\", \"b\" <= \"b\", 1 < 1.5);", "true true true\n"),
    ]);
}

#[test]
fn constant_expressions_are_folded() {
    expect_both_outputs(&[
        ("print(2 + 3 * 4);", "14\n"),
        ("print((2 + 3) * 4);", "20\n"),
        ("print(10 / 4, 10 div 4, 10 % 3);", "2.5 2 1\n"),
        ("print(2 ** 10);", "1024\n"),
        ("print(-5, +5, - -5);", "-5 5 5\n"),
        ("print(\"a\" + \"b\" + \"c\");", "abc\n"),
        ("print(\"a\" == \"a\", \"a\" != \"b\");", "true true\n"),
        ("print(1 < 2, 2 <= 2, 3 > 4, 4 >= 4);", "true true false true\n"),
        ("const size = 2 + 3; const items = [0, 0, 0, 0, 0]; items[size - 1] += 9; print(items);", "[0, 0, 0, 0, 9]\n"),
        ("var n = 0; for (var i = 0; i < 2 + 3; ++i) { n += 1; } print(n);", "5\n"),
    ]);
}

#[test]
fn compound_assignment_errors() {
    expect_both_errors(&[
        ("const items = [1]; items[5] += 1;", "index out of range"),
        ("const t = (1, 2); t[0] += 1;", "cannot assign an index of tuple"),
        ("const t = (1, 2); t[0]++;", "cannot assign an index of tuple"),
        ("const s = \"text\"; s[0] += 1;", "cannot assign an index of string"),
        ("const n = 5; n[0] += 1;", "cannot assign an index of number"),
        ("const n = 5; n.value += 1;", "cannot assign a property of number"),
        ("const n = 5; n.value++;", "cannot assign a property of number"),
        ("class Box { Box() {} } const box = Box(); box.value += 1;", "Box has no property \"value\""),
        ("const items = [\"a\"]; items[0]++;", "requires a number"),
        ("const d = {\"a\": \"x\"}; d[\"a\"]++;", "requires a number"),
        ("const items = [1e308]; items[0] += 1e308;", "non-finite numeric result"),
        ("const items = [1e308]; items[0] *= 2;", "non-finite numeric result"),
        ("const d = {\"a\": 1}; d[\"b\"] += 1;", "dictionary key not found"),
        ("const d = {\"a\": 1}; d[\"b\"]++;", "dictionary key not found"),
        ("const items = [1]; items[0.5] += 1;", "index must be an integer"),
    ]);
}

#[test]
fn folded_constants_keep_their_runtime_errors() {
    expect_both_errors(&[
        ("print(1 / 0);", "division by zero"),
        ("print(1 div 0);", "division by zero"),
        ("print(1 % 0);", "division by zero"),
        ("print(1e308 * 10);", "non-finite numeric result"),
        ("print(1e308 + 1e308);", "non-finite numeric result"),
        ("print(2 ** 10000);", "non-finite numeric result"),
    ]);
}

#[test]
fn loop_counters_use_a_flat_slot() {
    // A `for` header's counter reaches no nested function, so it lives in the
    // frame's own locals. The update still has to behave exactly as the
    // interpreter's does, for both spellings and both prefixes.
    expect_both_outputs(&[
        ("var n = 0; for (var i = 0; i < 4; ++i) { n += i; } print(n);", "6\n"),
        ("var n = 0; for (var i = 0; i < 4; i++) { n += i; } print(n);", "6\n"),
        ("var n = 0; for (var i = 4; i > 0; --i) { n += i; } print(n);", "10\n"),
        ("var n = 0; for (var i = 4; i > 0; i--) { n += i; } print(n);", "10\n"),
        ("var n = 0; for (var i = 0; i <= 4; i += 1) { n += i; } print(n);", "10\n"),
        // The value the update yields, in both prefixes. The body's own `i++`
        // advances the counter the loop header then advances again, so the loop
        // visits 0 and 2 rather than 0, 1, 2.
        ("var out = []; for (var i = 0; i < 3; ++i) { out += [i, i++, i]; } print(out);", "[0, 0, 1, 2, 2, 3]\n"),
        // A counter that a nested function reaches has to stay in the environment,
        // because the closure reads it through the scope chain. It is one variable,
        // so every closure sees the value the loop left behind.
        (
            "var seen = [];\n\
             for (var i = 0; i < 3; ++i) { seen += [function () { return i; }]; }\n\
             print(seen[0](), seen[1](), seen[2]());",
            "3 3 3\n",
        ),
        (
            "var total = 0;\n\
             for (var i = 0; i < 4; ++i) { const bump = function () { return i + 1; }; total += bump(); }\n\
             print(total);",
            "10\n",
        ),
        // A nested loop with two counters, and a counter read after the loop.
        ("var n = 0; for (var i = 0; i < 3; ++i) { for (var j = 0; j < 3; ++j) { n += i * j; } } print(n);", "9\n"),
        ("var last = 0; for (var i = 0; i < 5; ++i) { last = i; } print(last);", "4\n"),
        // `while` and `do...while` counters update through the same instruction.
        ("var i = 0; var n = 0; while (i < 4) { n += i; i++; } print(n);", "6\n"),
        ("var i = 0; var n = 0; do { n += i; ++i; } while (i < 4); print(n);", "6\n"),
        // A counter used as a list index in the same loop.
        ("const a = [10, 20, 30]; var n = 0; for (var i = 0; i < 3; ++i) { n += a[i]; } print(n);", "60\n"),
    ]);
}

#[test]
fn flat_index_reads_and_writes() {
    expect_both_outputs(&[
        ("const a = [1, 2, 3]; var i = 1; print(a[i], a[0], a[2]);", "2 1 3\n"),
        ("const a = [1, 2, 3]; var i = 1; a[i] = 9; print(a);", "[1, 9, 3]\n"),
        ("const a = [1, 2, 3]; a[0] = a[1]; print(a);", "[2, 2, 3]\n"),
        ("const a = [1, 2, 3]; var i = 0; var j = 2; a[i] = a[j]; print(a);", "[3, 2, 3]\n"),
        // A nested list, where the inner read is itself a flat index.
        ("const m = [[1, 2], [3, 4]]; var r = 1; var c = 0; print(m[r][c]);", "3\n"),
        ("const m = [[1, 2], [3, 4]]; var r = 1; var c = 1; m[r][c] = 9; print(m);", "[[1, 2], [3, 9]]\n"),
        // A dictionary indexed by a flat key.
        ("const d = {\"a\": 1, \"b\": 2}; const k = \"b\"; print(d[k]);", "2\n"),
        ("const d = {\"a\": 1}; const k = \"a\"; d[k] = 7; print(d);", "{\"a\": 7}\n"),
        // A string indexed by a flat key still reads one character.
        ("const text = \"tiger\"; var i = 2; print(text[i], text[0]);", "g t\n"),
    ]);
    // An error from a fused read or write is the same one the unfused form
    // raises, at the same position.
    expect_both_errors(&[
        ("const a = [1]; var i = 5; print(a[i]);", "index out of range"),
        ("const a = [1]; var i = 5; a[i] = 9;", "index out of range"),
        ("const t = (1, 2); var i = 0; t[i] = 9;", "cannot assign an index of tuple"),
        ("const n = 5; var i = 0; n[i] = 9;", "cannot assign an index of number"),
        ("const d = {\"a\": 1}; const k = \"b\"; print(d[k]);", "dictionary key not found"),
        ("const a = [1]; var i = 0.5; print(a[i]);", "index must be an integer"),
    ]);
}

#[test]
fn a_comprehension_variable_does_not_capture_an_outer_name() {
    // The capture analysis is name-based, so a comprehension binding a name the
    // enclosing chunk also uses must not force that outer name into the
    // environment. Both engines have to agree either way, and the values have to
    // be the same as when the names differ.
    expect_both_outputs(&[
        // The same program written with a colliding and a distinct name: the
        // results must match.
        ("const a = [i for i in range(4)];\nvar n = 0;\nfor (var i = 0; i < 4; ++i) { n += a[i]; }\nprint(n);", "6\n"),
        ("const a = [n for n in range(4)];\nvar s = 0;\nfor (var i = 0; i < 4; ++i) { s += a[i]; }\nprint(s);", "6\n"),
        // The comprehension's own variable is what its element sees.
        ("var i = 99; print([i for i in range(3)], i);", "[0, 1, 2] 99\n"),
        ("var x = 99; print([x * 2 for x in [1, 2]], x);", "[2, 4] 99\n"),
        // A dictionary comprehension, whose key and value both see the clause's
        // variable.
        ("var k = 9; print({k: k for k in [\"a\"]}, k);", "{\"a\": \"a\"} 9\n"),
        // A filter sees its own clause's variable.
        ("var v = 9; print([v for v in [1, 2, 3] if v > 1], v);", "[2, 3] 9\n"),
        // A filter that reads a name a *later* clause binds reads the outer one,
        // because the later clause has not bound it yet.
        ("const outer = 5;\nconst a = [1, 2, 3];\nconst b = [10];\nprint([x for x in a if outer > 0 for outer in b]);", "[1, 2, 3]\n"),
        // A generator expression's variable shadows the same way.
        ("var g = 9; print(list([g for g in range(3)]), g);", "[0, 1, 2] 9\n"),
        // A nested comprehension: each level binds its own variable.
        ("const rows = [[1, 2], [3]];\nvar i = 9;\nprint([[i for i in row] for row in rows], i);", "[[1, 2], [3]] 9\n"),
        // The clause's iterable is read in the enclosing scope, so a name it
        // mentions is the outer one.
        ("const items = [1, 2];\nvar n = 0;\nprint([n for n in items], n);", "[1, 2] 0\n"),
        // A name the comprehension genuinely reads from outside stays reachable,
        // including from inside a nested function in the element. The function
        // captures the comprehension's variable, which is one variable, so both
        // closures see its final value.
        ("const factor = 3;\nconst out = [function () { return v * factor; } for v in [1, 2]];\nprint(out[0](), out[1]());", "6 6\n"),
    ]);
}

#[test]
fn indexes_a_list_the_same_way_with_and_without_the_fast_path() {
    // The shape the fast path answers: a list, a non-negative integer key, in range.
    expect_both_outputs(&[
        ("const a = [10, 20, 30]; print(a[0], a[2]);", "10 30\n"),
        ("const a = [10, 20, 30]; a[1] = 99; print(a);", "[10, 99, 30]\n"),
        ("const a = [1]; for (var i = 0; i < 1; ++i) { a[i] = a[i] + 1; } print(a);", "[2]\n"),
    ]);
    // A negative index counts from the end, which the fast path declines, and a
    // tuple or a dictionary is read through its own path.
    expect_both_outputs(&[
        ("const a = [10, 20, 30]; print(a[-1], a[-3]);", "30 10\n"),
        ("const a = [10, 20, 30]; a[-1] = 7; print(a);", "[10, 20, 7]\n"),
        ("const t = (1, 2, 3); print(t[1], t[-1]);", "2 3\n"),
        ("const d = {\"k\": 1}; print(d[\"k\"]);", "1\n"),
    ]);
    // Everything the fast path declines keeps the diagnostic it had, on both the
    // read and the write.
    for (source, message) in [
        ("const a = [1]; print(a[5]);", "index out of range"),
        ("const a = [1]; print(a[-5]);", "index out of range"),
        ("const a = [1]; print(a[0.5]);", "index must be an integer"),
        ("const a = [1]; print(a[\"0\"]);", "index must be an integer"),
        ("const a = [1]; print(a[true]);", "index must be an integer"),
        ("const a = [1]; a[5] = 1;", "index out of range"),
        ("const a = [1]; a[-5] = 1;", "index out of range"),
        ("const a = [1]; a[0.5] = 1;", "index must be an integer"),
    ] {
        let (_, outcome) = both(source);
        let error = match outcome {
            Ok(_) => panic!("{:?} must fail", source),
            Err(error) => error,
        };
        assert!(error.contains(message), "{:?}: expected {:?}, got {}", source, message, error);
    }
}

#[test]
fn a_closure_in_a_for_in_loop_reads_its_own_binding() {
    // Three closures built in one loop share the loop's chunk and each carries its
    // own environment, so the same name read runs against a different scope chain
    // per closure. A read remembered for one of them must not answer for another,
    // which is why the environment is part of what a remembered read is keyed on.
    let source = "var transforms = [];\n\
                  for factor in [2, 3, 4] {\n\
                  transforms = transforms + [function (value) { return value * factor; }];\n\
                  }\n\
                  print(transforms[0](5), transforms[1](5), transforms[2](5));";
    assert_eq!(both_output(source), "10 15 20\n");
    // Two closures from two calls of one function are the same shape.
    let source = "function make(start) {\n\
                  var value = start;\n\
                  return function () { return value; };\n\
                  }\n\
                  const first = make(1);\n\
                  const second = make(2);\n\
                  print(first(), second());";
    assert_eq!(both_output(source), "1 2\n");
}

#[test]
fn the_same_work_compiles_and_walks_to_the_same_output() {
    // One program that uses every path this file covers, so the engines are
    // compared on their interaction rather than one at a time.
    let source = "class Tally { var total = 0; Tally() {}\n\
                  function add(step) { this.total += step; return this.total; } }\n\
                  const tally = Tally();\n\
                  const counts = {};\n\
                  const items = [3, 1, 2];\n\
                  var sum = 0;\n\
                  for (var i = 0; i < 4; ++i) {\n\
                  const key = \"k\" + str(i % 2);\n\
                  if key in counts { counts[key] += 1; } else { counts[key] = 1; }\n\
                  }\n\
                  for (var i = 0; i < 3; ++i) { items[i] *= 2; }\n\
                  items[0]++;\n\
                  for (var i = 0; i < 3; ++i) { sum += items[i]; }\n\
                  tally.add(sum + 2 * 3);\n\
                  print(counts, items, sum, tally.total);";
    assert_eq!(both_output(source), "{\"k0\": 2, \"k1\": 2} [7, 2, 4] 13 19\n");
}
