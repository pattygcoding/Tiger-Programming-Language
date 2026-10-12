//! Function ergonomics: defaults, anonymous functions, multiple returns, and
//! destructuring.
//!
//! A port of `pkg/evaluator/ergonomics_test.go`.

mod common;

use common::{expect_errors, expect_outputs};

#[test]
fn default_parameters() {
    expect_outputs(&[
        (
            "function greet(name, greeting = \"hello\") { return greeting + \" \" + name; } print(greet(\"world\"), greet(\"world\", \"hi\"), greet(name=\"ace\"));",
            "hello world hi world hello ace\n",
        ),
        (
            "function add(a, b = 10, c = 100) { return a + b + c; } print(add(1), add(1, 2), add(1, 2, 3), add(1, c=0));",
            "111 103 6 11\n",
        ),
        (
            "const base = 5; function scale(value, factor = base) { return value * factor; } print(scale(3), scale(3, 2));",
            "15 6\n",
        ),
        (
            "class Counter { var step = 1; function bump(amount = 2) { return amount + this.step; } } print(Counter().bump(), Counter().bump(10));",
            "3 11\n",
        ),
    ]);
}

#[test]
fn anonymous_functions() {
    expect_outputs(&[
        ("const double = function(value) { return value * 2; }; print(double(4));", "8\n"),
        (
            "const words = [\"ccc\", \"a\", \"bb\"]; words.sort(key=function(word) { return len(word); }); print(words);",
            "[\"a\", \"bb\", \"ccc\"]\n",
        ),
        ("print(function(a, b = 2) { return a + b; }(3));", "5\n"),
        (
            "function make_adder(amount) { return function(value) { return value + amount; }; } const add5 = make_adder(5); print(add5(3));",
            "8\n",
        ),
        (
            "function apply_all(values, action) { var result = []; for value in values { result = result + [action(value)]; } return result; } print(apply_all([1, 2, 3], function(value) { return value * value; }));",
            "[1, 4, 9]\n",
        ),
        ("print(function() { return 1; });", "<function anonymous>\n"),
    ]);
}

#[test]
fn multiple_returns_and_destructuring() {
    expect_outputs(&[
        (
            "function min_max(values) { var low = values[0]; var high = values[0]; for value in values { if value < low { low = value; } if value > high { high = value; } } return low, high; } const [low, high] = min_max([3, 1, 4, 1, 5]); print(low, high, min_max([2, 9]));",
            "1 5 [2, 9]\n",
        ),
        (
            "function swap(a, b) { return b, a; } var [first, second] = swap(1, 2); print(first, second);",
            "2 1\n",
        ),
        ("const [head, tail] = [1, 2]; print(head, tail);", "1 2\n"),
        (
            "const record = {\"name\": \"tiger\", \"legs\": 4}; const {name, legs} = record; print(name, legs);",
            "tiger 4\n",
        ),
    ]);
}

#[test]
fn function_ergonomics_errors() {
    expect_errors(&[
        ("function f(a = 1, b) { return a; }", "a required parameter cannot follow a defaulted parameter"),
        ("function f(a) { return a; } f();", "expects 1 arguments, got 0"),
        ("print(function(a) { return a; }());", "anonymous function expects 1 arguments, got 0"),
        ("var [a, b] = [1];", "cannot unpack 1 values into 2 names"),
        ("var [a] = 5;", "expected a list"),
        ("var {a} = [1];", "expected a dictionary"),
        ("var {a} = {\"b\": 1};", "missing key"),
    ]);
}
