//! `type` and `isinstance`.
//!
//! A port of `pkg/evaluator/typecheck_test.go`.

mod common;

use common::{expect_errors, expect_outputs, output};

#[test]
fn type_builtin() {
    expect_outputs(&[
        ("print(type(1), type(\"x\"), type(true), type(null));", "number string bool null\n"),
        ("print(type([]), type({}), type([1]));", "list dict list\n"),
        ("print(type(print), type(math.sqrt)); function f() {} print(type(f));", "builtin builtin\nfunction\n"),
        ("class Box { function m() {} } print(type(Box), type(Box()), type(Box().m));", "class instance bound method\n"),
        ("print(type(math), type(algo));", "module module\n"),
        ("print(f\"a {type(3)} b\");", "a number b\n"),
    ]);
}

#[test]
fn is_instance() {
    expect_outputs(&[
        (
            "print(isinstance(1, \"number\"), isinstance(\"x\", \"number\"), isinstance(true, \"bool\"), isinstance(null, \"null\"));",
            "true false true true\n",
        ),
        ("print(isinstance([], \"list\"), isinstance({}, \"dict\"), isinstance(\"\", \"list\"));", "true true false\n"),
        (
            "print(isinstance([], [\"string\", \"list\"]), isinstance(1, [\"string\", \"dict\"]), isinstance(null, []));",
            "true false false\n",
        ),
        (
            "class Box {} function f() {} print(isinstance(print, \"builtin\"), isinstance(f, \"function\"), isinstance(Box, \"class\"), isinstance(Box(), \"instance\"), isinstance(math, \"module\"));",
            "true true true true true\n",
        ),
        (
            "class Box { function m() {} } print(isinstance(Box().m, \"bound method\"), isinstance(Box().m, \"function\"));",
            "true false\n",
        ),
        (
            "function label(value) { if isinstance(value, [\"number\", \"bool\"]) { return \"numeric\"; } if isinstance(value, \"string\") { return \"text\"; } return \"other\"; } print(label(1), label(true), label(\"x\"), label([]), label(null));",
            "numeric numeric text other other\n",
        ),
    ]);
}

#[test]
fn type_inspection_errors() {
    expect_errors(&[
        ("type();", "type expects 1 argument, got 0"),
        ("type(1, 2);", "type expects 1 argument, got 2"),
        ("type(value=1);", "type does not accept keyword arguments"),
        ("isinstance(1);", "isinstance expects 2 arguments, got 1"),
        ("isinstance(1, 2);", "isinstance: expected a string or list of strings, got number"),
        ("isinstance(1, \"integer\");", "isinstance: unknown type name \"integer\""),
        ("isinstance(1, [\"number\", 2]);", "isinstance: type name must be a string, got number"),
        ("isinstance(1, [\"number\", \"integer\"]);", "isinstance: unknown type name \"integer\""),
        ("isinstance(value=1);", "isinstance does not accept keyword arguments"),
    ]);
    assert_eq!(output("try { type(); } catch (error) { print(\"expects 1 argument\" in error); }"), "true\n");
}
