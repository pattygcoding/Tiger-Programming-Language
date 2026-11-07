//! Enumeration declarations.
//!
//! An `enum` is a class whose members are read-only static attributes: each member
//! is a singleton instance carrying its own `name` and `value`, so `switch` can be
//! exhaustive over the closed set and `isinstance` can test a member against its
//! enum. These tests cover the declaration, member access, explicit and
//! auto-incrementing values, hashing, switching, and the error paths.

mod common;

use common::{expect_errors, expect_outputs, output};

#[test]
fn members() {
    expect_outputs(&[
        ("enum Color { RED, GREEN, BLUE } print(Color.RED.name, Color.GREEN.name, Color.BLUE.name);", "RED GREEN BLUE\n"),
        ("enum Color { RED, GREEN, BLUE } print(Color.RED.value, Color.GREEN.value, Color.BLUE.value);", "0 1 2\n"),
        ("enum Color { RED, GREEN } print(type(Color), type(Color.RED));", "class instance\n"),
        ("enum Color { RED, GREEN } print(Color.RED == Color.RED, Color.RED == Color.GREEN);", "true false\n"),
        ("enum Color { RED, GREEN } print(isinstance(Color.RED, Color), isinstance(Color.GREEN, Color));", "true true\n"),
        ("enum Color { RED } print(isinstance(Color.RED, \"instance\"), isinstance(Color.RED, \"class\"));", "true false\n"),
    ]);
}

#[test]
fn explicit_and_implicit_values() {
    expect_outputs(&[
        ("enum Color { RED = 1, GREEN = 2, BLUE = 4 } print(Color.RED.value, Color.GREEN.value, Color.BLUE.value);", "1 2 4\n"),
        ("enum Color { RED = 5, GREEN, BLUE } print(Color.RED.value, Color.GREEN.value, Color.BLUE.value);", "5 6 7\n"),
        ("enum Color { RED = -1, GREEN = 0, BLUE } print(Color.RED.value, Color.GREEN.value, Color.BLUE.value);", "-1 0 1\n"),
        ("enum Color { RED } print(Color.RED.value);", "0\n"),
        ("enum Color { RED = 2 + 2 } print(Color.RED.value);", "4\n"),
        ("enum Color { RED } print(Color.RED.value, Color.RED.value);", "0 0\n"),
    ]);
}

#[test]
fn switching() {
    let source = r#"enum Color { RED, GREEN, BLUE }
function label(color) {
    switch color {
        case Color.RED: return "red";
        case Color.GREEN: return "green";
        case Color.BLUE: return "blue";
    }
    return "unknown";
}
print(label(Color.RED), label(Color.GREEN), label(Color.BLUE));"#;
    assert_eq!(output(source), "red green blue\n");
}

#[test]
fn switch_falls_through_to_break() {
    let source = r#"enum Color { RED, GREEN, BLUE }
var seen = "";
switch (Color.GREEN) {
    case Color.RED: seen += "r";
    case Color.GREEN: seen += "g";
    case Color.BLUE: seen += "b"; break;
}
print(seen);"#;
    assert_eq!(output(source), "gb\n");
}

#[test]
fn hashing() {
    let source = r#"enum Color { RED, GREEN, BLUE }
const names = {Color.RED: "red", Color.GREEN: "green"};
print(names[Color.RED], names[Color.GREEN]);
const colors = {Color.RED, Color.GREEN};
print(len(colors), Color.RED in colors, Color.BLUE in colors);
enum A { X }
enum B { X }
const n = {A.X: 1, B.X: 2};
print(n[A.X], n[B.X]);"#;
    assert_eq!(output(source), "red green\n2 true false\n1 2\n");
}

#[test]
fn enum_in_scope_and_rebinding() {
    expect_outputs(&[
        ("function make() { enum Color { RED, GREEN } return Color.GREEN; } print(make().name);", "GREEN\n"),
        ("enum Color { RED } const copy = Color.RED; print(copy.name, copy == Color.RED);", "RED true\n"),
    ]);
}

#[test]
fn errors() {
    expect_errors(&[
        ("enum Color { RED, RED }", "duplicate enum member \"RED\""),
        ("enum Color {}", "enum Color must declare at least one member"),
        ("enum Color { RED } print(Color.NOPE);", "class Color has no member \"NOPE\""),
        ("enum Color { RED } Color.RED = 5;", "cannot assign to read-only member \"RED\" of class Color"),
        ("enum Color { RED } Color.RED.name = \"BLUE\";", "cannot reassign const field \"name\""),
        ("enum Color { RED } Color.RED.value = 9;", "cannot reassign const field \"value\""),
        ("enum { RED }", "expected \"identifier\""),
        ("enum Color", "expected \"{\""),
        ("enum Color { RED } enum Color { BLUE }", "\"Color\" is already declared in this scope"),
    ]);
}
