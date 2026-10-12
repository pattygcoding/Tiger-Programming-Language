//! Sorting stability, which Go tests against `sort.Ints`.
//!
//! A port of `pkg/evaluator/sort_test.go`, expressed through the language
//! because the port uses the standard library's stable sort rather than an
//! in-repo one.

mod common;

use common::{expect_outputs, output};

#[test]
fn sorts_like_the_standard_library() {
    expect_outputs(&[
        (
            "const values = [5, 3, 8, 1, 9, 2, 7, 4, 6, 0, 15, 13, 18, 11, 19, 12, 17, 14, 16, 10]; values.sort(); print(values);",
            "[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19]\n",
        ),
        ("const empty = []; empty.sort(); print(empty);", "[]\n"),
        ("const single = [7]; single.sort(); print(single);", "[7]\n"),
        ("const equal = [4, 4, 4, 4]; equal.sort(); print(equal);", "[4, 4, 4, 4]\n"),
        ("const two = [2, 1]; two.sort(); print(two);", "[1, 2]\n"),
        (
            "const many = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]; many.sort(); print(many);",
            "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]\n",
        ),
    ]);
}

#[test]
fn sort_is_stable() {
    // Equal keys keep their original order, as Go's stableSort guarantees.
    assert_eq!(
        output(
            "const items = [{\"key\": 2, \"tag\": 0}, {\"key\": 1, \"tag\": 1}, {\"key\": 2, \"tag\": 2}, {\"key\": 1, \"tag\": 3}, {\"key\": 2, \"tag\": 4}, {\"key\": 1, \"tag\": 5}];\n\
             items.sort(key=function(item) { return item[\"key\"]; });\n\
             var tags = []; for item in items { tags += [item[\"tag\"]]; } print(tags);"
        ),
        "[1, 3, 5, 0, 2, 4]\n"
    );
}

#[test]
fn sort_descending() {
    assert_eq!(
        output("const values = [3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5]; values.sort(reverse=true); print(values);"),
        "[9, 6, 5, 5, 5, 4, 3, 3, 2, 1, 1]\n"
    );
}
