//! Operators: power, floor division, bitwise arithmetic, and their errors.
//!
//! A port of `pkg/evaluator/operators_test.go`.

mod common;

use common::{expect_errors, expect_outputs, output};

#[test]
fn power_and_floor_division() {
    expect_outputs(&[
        ("print(2 ** 10, 2 ** 0, 4 ** 0.5);", "1024 1 2\n"),
        ("print(2 ** 3 ** 2, 2 ** 2 ** 3);", "512 256\n"),
        ("print(-2 ** 2, -(2 ** 2), 2 ** -1);", "-4 -4 0.5\n"),
        ("print(2 * 3 ** 2, (2 * 3) ** 2, 2 ** 3 + 1);", "18 36 9\n"),
        ("print(7 div 2, -7 div 2, 7 div -2, -7 div -2);", "3 -4 -4 3\n"),
        ("print(7.5 div 2, 10 div 5 div 2, 2 + 6 div 3);", "3 1 4\n"),
    ]);
}

#[test]
fn bitwise_operators() {
    expect_outputs(&[
        ("print(6 & 3, 6 | 3, 6 ^ 3);", "2 7 5\n"),
        ("print(~0, ~5, ~2.7);", "-1 -6 -3\n"),
        ("print(1 << 4, 256 >> 4, -8 >> 1, 1 << 40);", "16 16 -4 1099511627776\n"),
        ("print(6.9 & 3.9, 7.9 >> 1);", "2 3\n"),
        ("print(1 + 2 << 3, 1 | 2 & 3, 2 ** 3 & 1);", "24 3 0\n"),
        ("print(255 & 15 | 16, 1 ^ 1, 5 & 3 == 1);", "31 0 true\n"),
        ("print(1 << 100, -1 >> 100);", "0 -1\n"),
    ]);
}

#[test]
fn logical_operators() {
    expect_outputs(&[
        ("print(true && false, true || false, !true, !false);", "false true false true\n"),
        ("print(1 && 2, 0 && 2, 1 || 0, 0 || 3);", "2 0 1 3\n"),
        ("print(!0, !1, !\"\", !\"x\", !null, ![], ![1]);", "true false true false true true false\n"),
        ("print(!!7, !!!0, !!!!0);", "true true false\n"),
        // Short circuit: the right operand must not run.
        ("var calls = 0; function hit() { calls++; return true; } print(false && hit(), true || hit(), calls);", "false true 0\n"),
        // `&&` and `||` are `and` and `or`, and bind tighter than `not`/`!`.
        ("print(true or false and false, (true or false) and false);", "true false\n"),
        ("print(not false and true, !false and true);", "true true\n"),
        ("print(!0 == true, not 0 == true);", "true true\n"),
    ]);
    expect_errors(&[("print(missing_flag && true);", "undefined name"), ("print(!missing_flag);", "undefined name")]);
}

#[test]
fn modulo_follows_the_divisor() {
    expect_outputs(&[
        // The sign follows the divisor, as Python's does, not the dividend as C's does.
        ("print(7 % 3, -7 % 3, 7 % -3, -7 % -3);", "1 2 -2 -1\n"),
        ("print(10 % 5, -10 % 5, 0 % 7, 5 % 5);", "0 0 0 0\n"),
        // A zero remainder takes the divisor's sign, as Python's float modulo does: `fmod`
        // would give the dividend's, which prints as `-0` where the divisor is positive.
        ("print(10 % -5, -10 % -5, -0.0 % 5, -0.0 % -5);", "-0 -0 0 -0\n"),
        // A fractional operand takes the float path, which corrects the same way.
        ("print(2.5 % 1, -2.5 % 1, 2.5 % -1, -2.5 % -1);", "0.5 0.5 -0.5 -0.5\n"),
        // Values large enough to matter are still exact: 1e9 is well inside what a float
        // holds as an integer, and the answer is the integer remainder either way.
        ("print(1e9 % 7, 123456 % 97, 10000 % 1000);", "6 72 0\n"),
        ("var total = 0; for (var i = 0; i < 12; ++i) { total += i * 3 % 7; } print(total);", "37\n"),
    ]);
}

#[test]
fn operator_errors() {
    expect_errors(&[
        ("print(1 div 0);", "division by zero"),
        ("print(1 % 0);", "division by zero"),
        ("print(0 ** -1);", "non-finite numeric result"),
        ("print((-8) ** 0.5);", "non-finite numeric result"),
        ("print(1 << -1);", "shift count must not be negative"),
        ("print(1e300 & 1);", "64-bit integer range"),
        ("print(\"a\" ** 2);", "not supported"),
        ("print(\"a\" div 2);", "not supported"),
        ("print(\"a\" & 1);", "not supported"),
        ("print(~\"a\");", "requires a number"),
    ]);
    assert_eq!(output("try { print(1 div 0); } catch (error) { print(\"division by zero\" in error); }"), "true\n");
}
