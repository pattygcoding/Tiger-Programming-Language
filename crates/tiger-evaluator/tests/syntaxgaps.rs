//! Syntax: conditional expressions, slicing, labeled loops, and `do`/`while`.
//!
//! A port of `pkg/evaluator/syntaxgaps_test.go`.

mod common;

use common::{expect_errors, expect_outputs};

#[test]
fn conditional_expression() {
    expect_outputs(&[
        ("print(\"odd\" if 3 % 2 == 1 else \"even\");", "odd\n"),
        ("print(\"odd\" if 4 % 2 == 1 else \"even\");", "even\n"),
        ("function boom() { throw \"boom\"; } print(\"safe\" if true else boom());", "safe\n"),
        ("print(\"a\" if false else \"b\" if true else \"c\");", "b\n"),
        ("print(0 or 5 if false else 9);", "9\n"),
        ("print(1 + 2 if true else 3 * 4);", "3\n"),
        ("print(not true if true else \"x\");", "false\n"),
        ("print(\"hit\" if 1 < 2 == true else \"miss\");", "hit\n"),
        ("print(str(1 if true else 2), str(1 if false else 2));", "1 2\n"),
        ("const label = \"big\" if 10 > 5 else \"small\"; print(label);", "big\n"),
        ("print(\"yes\" if \"text\" else \"no\", \"yes\" if \"\" else \"no\");", "yes no\n"),
        ("print([1 if true else 9, 2]);", "[1, 2]\n"),
        ("function pick(value) { return \"yes\" if value else \"no\"; } print(pick(1), pick(0));", "yes no\n"),
        ("try { throw \"bad\"; } catch (error) if error == \"bad\" { print(\"caught\"); }", "caught\n"),
        ("try { throw \"bad\"; } catch (error) if (error if true else \"x\") == \"bad\" { print(\"caught\"); }", "caught\n"),
    ]);
    expect_errors(&[("print(1 if true);", "expected"), ("print(1 if true else);", "expected expression")]);
}

#[test]
fn slicing() {
    expect_outputs(&[
        ("print([1, 2, 3, 4, 5][1:3]);", "[2, 3]\n"),
        ("print([1, 2, 3][:2]);", "[1, 2]\n"),
        ("print([1, 2, 3][1:]);", "[2, 3]\n"),
        ("print([1, 2, 3][:]);", "[1, 2, 3]\n"),
        ("print([1, 2, 3, 4, 5][::2]);", "[1, 3, 5]\n"),
        ("print([1, 2, 3, 4, 5][::-1]);", "[5, 4, 3, 2, 1]\n"),
        ("print([1, 2, 3][-1:]);", "[3]\n"),
        ("print([1, 2, 3][:-1]);", "[1, 2]\n"),
        ("print([1, 2, 3, 4, 5][3:0:-1]);", "[4, 3, 2]\n"),
        ("print([1, 2, 3][1:99], [1, 2, 3][-99:2]);", "[2, 3] [1, 2]\n"),
        ("print([1, 2, 3][2:1], [1, 2, 3][0:0]);", "[] []\n"),
        ("print(\"tiger\"[1:4], \"tiger\"[::-1], \"tiger\"[::2]);", "ige regit tgr\n"),
        ("print(\"h\\u00e9llo\"[0:3]);", "h\u{e9}l\n"),
        ("const items = [1, 2, 3]; const copy = items[:]; copy[0] = 99; print(items, copy);", "[1, 2, 3] [99, 2, 3]\n"),
        ("print([1, 2, 3][:] == [1, 2, 3]);", "true\n"),
        ("const items = [10, 20, 30, 40]; print(items[1 - 1:4 - 2]);", "[10, 20]\n"),
        ("print(([1, 2, 3, 4][1:3])[0]);", "2\n"),
        ("print([1, 2, 3][1], \"tiger\"[0]);", "2 t\n"),
    ]);
    expect_errors(&[
        ("print([1, 2, 3][0:2:0]);", "slice step cannot be zero"),
        ("print([1, 2, 3][\"a\":1]);", "slice bounds must be integers"),
        ("print([1, 2, 3][0.5:1]);", "slice bounds must be integers"),
        ("print({\"a\": 1}[0:1]);", "dict cannot be sliced"),
        ("print(5[0:1]);", "number cannot be sliced"),
        ("var items = [1, 2, 3]; items[0:1] = [9];", "invalid assignment target"),
    ]);
}

#[test]
fn labeled_loops() {
    expect_outputs(&[
        ("outer: for i in range(3) { for j in range(3) { if j == 1 { break outer; } print(i, j); } } print(\"done\");", "0 0\ndone\n"),
        (
            "outer: for i in range(3) { for j in range(3) { if j == 1 { continue outer; } print(i, j); } } print(\"done\");",
            "0 0\n1 0\n2 0\ndone\n",
        ),
        ("outer: while true { while true { break outer; } } print(\"after\");", "after\n"),
        ("outer: for (var i = 0; i < 3; i++) { for (var j = 0; j < 3; j++) { if i == 2 { break outer; } } } print(\"after\");", "after\n"),
        ("outer: do { do { break outer; } while (true); } while (true); print(\"after\");", "after\n"),
        (
            "outer: for i in range(2) { outer: for j in range(3) { if j == 1 { break outer; } print(i, j); } } print(\"done\");",
            "0 0\n1 0\ndone\n",
        ),
        ("for i in range(3) { if i == 1 { break; } print(i); } print(\"done\");", "0\ndone\n"),
        ("outer: for i in range(2) { for j in range(2) { break outer; } } else { print(\"else\"); } print(\"done\");", "done\n"),
        ("for i in range(2) { for j in range(2) { break; } } else { print(\"else\"); } print(\"done\");", "else\ndone\n"),
        ("var n = 0; outer: while n < 3 { n++; while true { continue outer; } } print(n);", "3\n"),
        (
            "outer: for i in range(3) { try { if i == 1 { break outer; } print(i); } finally { print(\"f\" + str(i)); } } print(\"done\");",
            "0\nf0\nf1\ndone\n",
        ),
    ]);
    for (source, message) in [
        ("break missing;", "unknown label"),
        ("while true { break missing; }", "unknown label"),
        ("outer: for i in range(2) { break; } break;", "break outside a loop"),
        ("for i in range(2) { continue; } continue;", "continue outside a loop"),
        ("outer: for i in range(2) { function inner() { break outer; } }", "unknown label"),
        ("for i in range(2) { continue 5; }", "expected"),
    ] {
        let error = common::failure(source);
        assert!(error.contains(message), "{}: expected {:?}, got {}", source, message, error);
    }
}

#[test]
fn do_while() {
    expect_outputs(&[
        ("var n = 0; do { print(n); n++; } while (n < 3);", "0\n1\n2\n"),
        ("var n = 5; do { print(\"once\"); } while (n < 3);", "once\n"),
        ("var n = 0; do { n++; if n == 2 { break; } } while (n < 5); print(n);", "2\n"),
        ("var n = 0; do { n++; } while n < 2; print(n);", "2\n"),
        ("var n = 0; do { n++; if n % 2 == 1 { continue; } print(n); } while (n < 4);", "2\n4\n"),
        (
            "var outer = 0; var inner = 0; do { inner = 0; do { inner++; } while (inner < 2); outer++; } while (outer < 2); print(outer, inner);",
            "2 2\n",
        ),
        ("outer: do { do { break outer; } while (true); } while (true); print(\"after\");", "after\n"),
    ]);
    for (source, message) in [
        ("do { print(1); } while;", "expected expression"),
        ("do { print(1); } while true", "expected"),
        ("break;", "break outside a loop"),
    ] {
        let error = common::failure(source);
        assert!(error.contains(message), "{}: expected {:?}, got {}", source, message, error);
    }
}
