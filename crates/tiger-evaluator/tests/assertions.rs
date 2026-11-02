//! Assertions: `assert condition, message;`.
//!
//! A failed assertion is an ordinary catchable runtime error with its own
//! `kind`, so typed catch guards, `finally`, and the structured error fields
//! behave exactly as they do for every other runtime failure.

mod common;

use common::{expect_errors, expect_outputs, failure, output};

#[test]
fn a_passing_assert_prints_nothing() {
    expect_outputs(&[
        ("assert true; print(\"done\");", "done\n"),
        ("assert 1 < 2, \"one is less\"; print(\"done\");", "done\n"),
        // The message is evaluated only when the assertion fails.
        ("function note() { print(\"evaluated\"); return \"why\"; } assert 1 < 2, note(); print(\"done\");", "done\n"),
        // An assertion in a method sees `this`, and one in a nested function sees
        // the names it captured.
        (
            "class Box { var value = 0; Box(value) { this.value = value; } function checked() { assert this.value > 0, \"empty box\"; return this.value; } } print(Box(2).checked());",
            "2\n",
        ),
        (
            "function outer(limit) { function check(value) { assert value < limit, \"too large\"; return value; } return check; } print(outer(3)(2));",
            "2\n",
        ),
    ]);
}

#[test]
fn a_failed_assert_is_a_structured_error() {
    expect_outputs(&[
        (
            "try { assert 1 > 2, \"one is not greater\"; } catch (error) { print(type(error), error[\"kind\"], error[\"message\"], isinstance(error, \"error\")); }",
            "error assertion failed assertion failed: one is not greater true\n",
        ),
        (
            "try { assert 1 > 2; } catch (error) { print(error[\"kind\"], error[\"message\"]); }",
            "assertion failed assertion failed\n",
        ),
        // The message is rendered with `str`, exactly as `print` renders it.
        (
            "try { assert 1 > 2, [\"rows\", 3]; } catch (error) { print(error[\"message\"]); }",
            "assertion failed: [\"rows\", 3]\n",
        ),
        ("try { assert 1 > 2, 42; } catch (error) { print(error[\"message\"]); }", "assertion failed: 42\n"),
        // An empty message reports the bare failure rather than a trailing colon.
        (
            "try { assert 1 > 2, \"\"; } catch (error) { print(error[\"message\"]); }",
            "assertion failed\n",
        ),
        // The kind is the assertion's own even when the message quotes another
        // diagnostic's wording.
        (
            "try { assert 1 > 2, \"division by zero\"; } catch (error) { print(error[\"kind\"]); }",
            "assertion failed\n",
        ),
    ]);
}

#[test]
fn a_typed_guard_tells_an_assertion_apart() {
    let source = "function classify(value) {\n\
        try {\n\
        assert value > 0, \"value must be positive\";\n\
        return [10, 20][value];\n\
        } catch (error) if isinstance(error, \"error\") and error[\"kind\"] == \"assertion failed\" {\n\
        return \"assertion\";\n\
        } catch (error) {\n\
        return \"other\";\n\
        }\n\
        }\n\
        print(classify(1), classify(-1), classify(9));";
    assert_eq!(output(source), "20 assertion other\n");
}

#[test]
fn assertions_use_truthiness() {
    expect_outputs(&[
        // Zero, empty text, an empty list, and null are falsey.
        (
            "for value in [0, \"\", [], null] { try { assert value; print(\"passed\"); } catch (error) { print(error[\"kind\"]); } }",
            "assertion failed\nassertion failed\nassertion failed\nassertion failed\n",
        ),
        // A non-zero number, non-empty text, and a populated list are truthy.
        ("for value in [1, \"0\", [0]] { assert value; } print(\"passed\");", "passed\n"),
    ]);
}

#[test]
fn a_finally_still_runs() {
    let source = "var log = [];\n\
        function verify(limit) {\n\
        try {\n\
        assert limit < 3, f\"limit {limit} is too large\";\n\
        return \"accepted\";\n\
        } finally {\n\
        log = log + [limit];\n\
        }\n\
        }\n\
        print(verify(2));\n\
        try { verify(5); } catch (error) { print(error[\"kind\"]); }\n\
        print(log);";
    assert_eq!(output(source), "accepted\nassertion failed\n[2, 5]\n");
}

#[test]
fn an_uncaught_assertion_stops_the_program() {
    let error = failure("print(\"before\");\nassert 1 > 2, \"one is not greater\";\nprint(\"after\");");
    assert!(error.contains("2:1: assertion failed: one is not greater"), "{error}");
}

#[test]
fn assertion_errors() {
    expect_errors(&[
        ("assert;", "expected expression"),
        ("assert 1 < 2", "expected \";\""),
        ("assert 1 < 2, ;", "expected expression"),
        // `assert` is reserved, so it cannot name a binding.
        ("const assert = 1;", "expected \"identifier\""),
    ]);
}
