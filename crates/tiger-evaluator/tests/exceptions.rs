//! Exceptions: `finally`, structured error values, and typed catches.
//!
//! A port of `pkg/evaluator/exceptions_test.go`.

mod common;

use common::{expect_errors, expect_outputs, output};

#[test]
fn finally_blocks() {
    expect_outputs(&[
        (
            "var log = []; try { log = log + [\"try\"]; } finally { log = log + [\"finally\"]; } print(log);",
            "[\"try\", \"finally\"]\n",
        ),
        (
            "var log = []; try { throw \"boom\"; } catch (error) { log = log + [\"caught\"]; } finally { log = log + [\"finally\"]; } print(log);",
            "[\"caught\", \"finally\"]\n",
        ),
        (
            "var log = []; try { try { throw \"boom\"; } finally { log = log + [\"inner\"]; } } catch (error) { log = log + [\"outer\"]; } print(log);",
            "[\"inner\", \"outer\"]\n",
        ),
        (
            "var log = []; function run() { try { return \"value\"; } finally { log = log + [\"finally\"]; } } print(run(), log);",
            "value [\"finally\"]\n",
        ),
        (
            "function run() { try { throw \"boom\"; } finally { return \"overridden\"; } } print(run());",
            "overridden\n",
        ),
        (
            "var log = []; for index in [1, 2, 3] { try { if index == 2 { continue; } log = log + [index]; } finally { log = log + [\"f\"]; } } print(log);",
            "[1, \"f\", \"f\", 3, \"f\"]\n",
        ),
        // A `finally` that fails replaces the error in flight, after running.
        (
            "var log = []; function run() { try { throw \"first\"; } finally { log = log + [\"f\"]; throw \"second\"; } } try { run(); } catch (error) { print(error, log); }",
            "second [\"f\"]\n",
        ),
        // A clause that throws leaves through the `finally` and reaches the next
        // handler out.
        (
            "var log = []; try { try { throw \"a\"; } catch (error) { log = log + [\"c\"]; throw \"b\"; } finally { log = log + [\"f\"]; } } catch (error) { print(error, log); }",
            "b [\"c\", \"f\"]\n",
        ),
    ]);
}

#[test]
fn structured_errors() {
    expect_outputs(&[
        (
            "try { print(1 / 0); } catch (error) { print(type(error), error[\"kind\"], \"division by zero\" in error, isinstance(error, \"error\")); }",
            "error zero division true true\n",
        ),
        (
            "try { print([1, 2][9]); } catch (error) { print(error[\"kind\"], error[\"origin\"] == \"\"); }",
            "index range false\n",
        ),
        ("try { print(1 / 0); } catch (error) { print(error[\"message\"]); }", "division by zero\n"),
        ("try { throw {\"k\": 1}; } catch (error) { print(error[\"k\"]); }", "1\n"),
    ]);
}

#[test]
fn typed_catch() {
    let source = "function classify(value) {\n\
        try {\n\
        if value == 0 { throw \"custom\"; }\n\
        return [10, 20][value];\n\
        } catch (error) if isinstance(error, \"error\") and error[\"kind\"] == \"index range\" {\n\
        return \"range\";\n\
        } catch (error) {\n\
        return \"other\";\n\
        }\n\
        }\n\
        print(classify(1), classify(5), classify(0));";
    assert_eq!(output(source), "20 range other\n");
}

#[test]
fn error_handling_errors() {
    expect_errors(&[("try { print(1); }", "needs a catch or finally"), ("print(\"a\" / 1);", "not supported")]);
    assert_eq!(output("try { throw \"boom\"; } catch (error) { print(\"uncaught\"); }"), "uncaught\n");
}
