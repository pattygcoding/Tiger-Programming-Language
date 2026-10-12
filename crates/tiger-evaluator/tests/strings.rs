//! String methods: `split`, `upper`, `lower`, `trim`, `replace`, `startswith`,
//! `endswith`, `find`, `padStart`, `padEnd`, and `join`.
//!
//! A port of `pkg/evaluator/stringmethods_test.go`.

mod common;

use common::{expect_errors, expect_outputs, output};

#[test]
fn string_methods() {
    expect_outputs(&[
        ("print(\"a,b,c\".split(\",\"));", "[\"a\", \"b\", \"c\"]\n"),
        ("print(\"  a \\t b\\nc  \".split());", "[\"a\", \"b\", \"c\"]\n"),
        ("print(\"a,,b\".split(\",\"));", "[\"a\", \"\", \"b\"]\n"),
        ("print(\"\".split(\",\"), \"   \".split());", "[\"\"] []\n"),
        ("print(\"Tiger\".upper(), \"Tiger\".lower());", "TIGER tiger\n"),
        ("print(\"  padded \\t\".trim(), \"x\");", "padded x\n"),
        ("print(\"a-b-a\".replace(\"a\", \"o\"), \"aaa\".replace(\"a\", \"\").size());", "o-b-o 0\n"),
        ("print(\"Tiger\".startswith(\"Ti\"), \"Tiger\".endswith(\"er\"), \"Tiger\".startswith(\"er\"));", "true true false\n"),
        ("print(\"Tiger\".find(\"g\"), \"Tiger\".find(\"z\"), \"Tiger\".find(\"\"));", "2 -1 0\n"),
        ("print(\"café\".find(\"é\"), \"café\".find(\"f\"), \"café\".size());", "3 2 4\n"),
        ("print(\"7\".padStart(3, \"0\"), \"7\".padEnd(3, \".\"), \"ab\".padStart(2), \"ab\".padEnd(4, \"-\"));", "007 7.. ab ab--\n"),
        ("print(\"é\".padEnd(3, \".\"));", "é..\n"),
        ("print([\"a\", \"b\", \"c\"].join(\"-\"), [\"x\"].join(\"\") + \"y\", [].join(\",\").size());", "a-b-c xy 0\n"),
        ("print(\"a,b\".split(\",\").join(\"-\"));", "a-b\n"),
    ]);
}

#[test]
fn string_method_errors() {
    expect_errors(&[
        ("\"abc\".upper(1);", "upper expects 0 arguments, got 1"),
        ("\"abc\".upper(x=1);", "upper does not accept keyword arguments"),
        ("\"abc\".split(1);", "split separator must be a string, got number"),
        ("\"abc\".split(\"\");", "split: empty separator"),
        ("\"abc\".split(\",\", \"-\");", "split expects at most 1 argument, got 2"),
        ("\"abc\".replace(\"a\");", "replace expects 2 arguments, got 1"),
        ("\"abc\".replace(1, \"b\");", "replace old must be a string, got number"),
        ("\"abc\".startswith(1);", "startswith prefix must be a string, got number"),
        ("\"abc\".endswith(1);", "endswith suffix must be a string, got number"),
        ("\"abc\".find(1);", "find argument must be a string, got number"),
        ("\"abc\".padStart(\"3\");", "padStart width must be an integer"),
        ("\"abc\".padStart(-1);", "padStart width must not be negative"),
        ("\"abc\".padEnd(2000000);", "padEnd width must be at most 1000000"),
        ("\"abc\".padStart(3, \"ab\");", "padStart fill must be a single character"),
        ("\"abc\".padStart(3, 1);", "padStart fill must be a string, got number"),
        ("[1, \"a\"].join(\",\");", "join expects a list of strings, got number at index 0"),
        ("[\"a\"].join(1);", "join separator must be a string, got number"),
        ("[\"a\"].join();", "join expects 1 argument, got 0"),
    ]);
    assert_eq!(output("try { \"abc\".split(\"\"); } catch (error) { print(\"empty separator\" in error); }"), "true\n");
}
