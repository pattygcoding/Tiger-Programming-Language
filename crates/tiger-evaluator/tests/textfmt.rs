//! Formatted strings, escapes, raw strings, and grapheme clusters.
//!
//! A port of `pkg/evaluator/textfmt_test.go`.

mod common;

use common::{expect_errors, expect_outputs};

#[test]
fn format_specs() {
    expect_outputs(&[
        ("print(f\"{1.23456:.2f}\", f\"{2:.0f}\", f\"{2.5:.3f}\");", "1.23 2 2.500\n"),
        ("print(f\"[{7:>4}]\", f\"[{7:<4}]\", f\"[{7:^5}]\", f\"[{7:4}]\");", "[   7] [7   ] [  7  ] [   7]\n"),
        ("print(f\"[{'ab':<4}]\", f\"[{'ab':4}]\", f\"[{'ab':>4}]\");", "[ab  ] [ab  ] [  ab]\n"),
        ("print(f\"{3.5:08.2f}\", f\"{-3.5:07.2f}\", f\"{42:04}\");", "00003.50 -003.50 0042\n"),
        ("print(f\"{255:>6}\", f\"{1.5:>8.2f}\", f\"{'x':^5}\");", "   255     1.50   x  \n"),
        ("print(f\"{1 + 1}\", f\"{'a'}\");", "2 a\n"),
        ("print(f\"{len('hello'):>3}\", f\"{[1, 2][1]:02}\");", "  5 02\n"),
    ]);
}

#[test]
fn string_escapes_and_raw_strings() {
    expect_outputs(&[
        ("print(\"\\u0041\\u00e9\\u4e2d\");", "A\u{e9}\u{4e2d}\n"),
        ("print(\"\\x41\\x42\");", "AB\n"),
        ("print(\"\\U0001F600\");", "\u{1F600}\n"),
        ("print(f\"A\\u0041[{1}]\");", "AA[1]\n"),
        ("print(r\"a\\nb\", len(r\"a\\nb\"));", "a\\nb 4\n"),
        ("print(regex.find(r\"\\d+\", \"order 42\"));", "42\n"),
        ("print(r'he said \"hi\"');", "he said \"hi\"\n"),
    ]);
}

#[test]
fn graphemes() {
    expect_outputs(&[
        ("print(graphemes(\"abc\"));", "[\"a\", \"b\", \"c\"]\n"),
        ("print(graphemes(\"e\\u0301\"), len(\"e\\u0301\"));", "[\"e\u{301}\"] 2\n"),
        ("print(graphemes(\"\\U0001F44D\\U0001F3FB\"));", "[\"\u{1F44D}\u{1F3FB}\"]\n"),
        ("print(graphemes(\"\\U0001F1FA\\U0001F1F8\"));", "[\"\u{1F1FA}\u{1F1F8}\"]\n"),
        ("print(graphemes(\"\\U0001F468\\u200D\\U0001F469\\u200D\\U0001F467\"));", "[\"\u{1F468}\\u200d\u{1F469}\\u200d\u{1F467}\"]\n"),
        ("print(len(graphemes(\"e\\u0301x\")), graphemes(\"\"));", "2 []\n"),
    ]);
}

#[test]
fn text_precision_errors() {
    expect_errors(&[
        ("print(f\"{1:.f}\");", "expected digits after '.'"),
        ("print(f\"{1:.2}\");", "only '.Nf' precision is supported"),
        ("print(f\"{1:.20f}\");", "at most 17"),
        ("print(f\"{'x':.2f}\");", "requires a number"),
        ("print(f\"{1:d}\");", "invalid format spec"),
        ("print(f\"{1:.2f.3}\");", "invalid format spec"),
        ("print(\"\\x4\");", "invalid hexadecimal escape"),
        ("print(\"\\uD800\");", "not a valid character"),
        ("print(r\"unterminated);", "unterminated raw string"),
        ("graphemes(1);", "expects a string"),
        ("graphemes();", "expects 1 argument"),
    ]);
}
