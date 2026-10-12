//! The `%` verbs the runtime uses, with the output Go's `fmt` produces.
//!
//! A port of `pkg/format/format_test.go`. Go's test compares against `fmt`
//! directly; the expectations here are the values that comparison locked in.

use tiger_format::{sprintf, Arg};

fn format(template: &str, args: &[Arg]) -> String {
    sprintf(template, args)
}

#[test]
fn verbs_match_fmt() {
    let cases: &[(&str, &[Arg], &str)] = &[
        ("%s", &[Arg::Str("hello")], "hello"),
        ("%s", &[Arg::Error("boom")], "boom"),
        ("%d", &[Arg::Int(42)], "42"),
        ("%d", &[Arg::Int(-7)], "-7"),
        ("%q", &[Arg::Str("hi\n")], "\"hi\\n\""),
        ("%q", &[Arg::Rune('A')], "'A'"),
        ("%q", &[Arg::Rune('\u{1F600}')], "'\u{1F600}'"),
        ("%c", &[Arg::Rune('x')], "x"),
        ("%c", &[Arg::Int(65)], "A"),
        ("%v", &[Arg::Error("bad")], "bad"),
        ("%v", &[Arg::Bool(true)], "true"),
        ("%v", &[Arg::Float(3.5)], "3.5"),
        ("%s-%d-%q", &[Arg::Str("a"), Arg::Int(2), Arg::Str("b")], "a-2-\"b\""),
        ("100%%", &[], "100%"),
        ("%d%%", &[Arg::Int(5)], "5%"),
    ];
    for (template, args, expected) in cases {
        assert_eq!(format(template, args), *expected, "sprintf({:?})", template);
    }
}

#[test]
fn missing_arguments_are_marked() {
    assert_eq!(format("%d", &[]), "%!d(MISSING)");
}
