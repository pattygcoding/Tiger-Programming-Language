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
