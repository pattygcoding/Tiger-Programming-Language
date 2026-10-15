//! List and dictionary comprehensions: `[x * 2 for x in items if x > 0]`.
//!
//! The clauses nest left to right, filters belong to the clause they follow, and
//! the clause variables live in the comprehension's own scope, so they never leak
//! into the enclosing one.

mod common;

use common::{expect_errors, expect_outputs};

#[test]
fn list_comprehensions() {
    expect_outputs(&[
        ("print([x * x for x in range(1, 5)]);", "[1, 4, 9, 16]\n"),
        ("print([x for x in [3, 1, 2] if x % 2 == 1]);", "[3, 1]\n"),
        ("print([x for x in [1, 2, 3, 4] if x > 1 if x < 4]);", "[2, 3]\n"),
        ("print([x if x > 0 else -x for x in [-2, 3]]);", "[2, 3]\n"),
        ("print([word.upper() for word in \"ab\"]);", "[\"A\", \"B\"]\n"),
        // Any iterable works, exactly as a `for` statement accepts it.
        (
            "print([v for v in (1, 2)], [v for v in {3, 1}], [k for k in {\"a\": 1}], [c for c in \"xy\"]);",
            "[1, 2] [3, 1] [\"a\"] [\"x\", \"y\"]\n",
        ),
        // Clauses nest left to right: the second runs for every value of the first.
        ("print([b for a in [1, 2] for b in [a, a * 10]]);", "[1, 10, 2, 20]\n"),
        ("print([[a, b] for a in [1, 2] for b in [\"x\"]]);", "[[1, \"x\"], [2, \"x\"]]\n"),
        ("print([x for x in []], len([x for x in range(10)]));", "[] 10\n"),
        ("print([y for y in [x * 2 for x in [1, 2]] if y > 2]);", "[4]\n"),
        ("print([x for x in [1, 2, 3][0:2]]);", "[1, 2]\n"),
    ]);
}

#[test]
fn dictionary_comprehensions() {
    expect_outputs(&[
        ("print({word: len(word) for word in [\"a\", \"bb\", \"ccc\"]});", "{\"a\": 1, \"bb\": 2, \"ccc\": 3}\n"),
        ("print({x: x * x for x in range(1, 4) if x != 2});", "{1: 1, 3: 9}\n"),
        ("print({key: value for key in [\"a\"] for value in [1, 2]});", "{\"a\": 2}\n"),
        ("print(type({x: x for x in [1]}), {x: x for x in [1]} == {1: 1});", "dict true\n"),
        // A repeated key keeps its first position and takes the last value.
        ("print({k: len(k) for k in [\"a\", \"b\", \"a\"]});", "{\"a\": 1, \"b\": 1}\n"),
        ("print({c: c.upper() for c in \"ab\"});", "{\"a\": \"A\", \"b\": \"B\"}\n"),
        ("print({x: x for x in []});", "{}\n"),
    ]);
}

#[test]
fn comprehension_scope_and_order() {
    expect_outputs(&[
        // The clause variable is local to the comprehension and shadows nothing.
        ("var shadowed = 5; print([shadowed for shadowed in [1, 2]], shadowed);", "[1, 2] 5\n"),
        ("const items = [1, 2]; print([x for x in items], items);", "[1, 2] [1, 2]\n"),
        ("var outside = 10; print([x + outside for x in [1, 2]]);", "[11, 12]\n"),
        // Elements and values are evaluated in clause order, once per iteration.
        (
            "function noisy(value) { print(\"visit\", value); return value; } print([noisy(x) for x in [1, 2]]);",
            "visit 1\nvisit 2\n[1, 2]\n",
        ),
        (
            "function noisy(value) { print(\"visit\", value); return value; } print({noisy(x): noisy(0 - x) for x in [1]});",
            "visit 1\nvisit -1\n{1: -1}\n",
        ),
        // A later clause sees the variables of the ones before it.
        ("print([x + y for x in [1] for y in [x, x * 10]]);", "[2, 11]\n"),
        // A closure made by the element keeps the clause variable alive.
        ("const f = [function() { return x; } for x in [7]][0]; print(f());", "7\n"),
    ]);
}

#[test]
fn comprehension_errors() {
    expect_errors(&[
        ("[x for x in 1];", "number is not iterable"),
        ("{x: x for x in 1};", "number is not iterable"),
        ("{[x]: x for x in [1]};", "list cannot be a dictionary key"),
        ("[x for x in [1]]; print(x);", "undefined name"),
        ("[x for y in [1]];", "undefined name"),
        ("[x for x in [1] if x.nope];", "number has no properties"),
    ]);
}
