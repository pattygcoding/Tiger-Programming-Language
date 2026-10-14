//! List and dictionary methods.
//!
//! A port of `pkg/evaluator/collectionmethods_test.go`.

mod common;

use common::{expect_errors, expect_outputs, output};

#[test]
fn list_methods() {
    expect_outputs(&[
        ("var items = [1, 2, 3]; print(items.pop(), items, len(items));", "3 [1, 2] 2\n"),
        ("var items = [1, 2, 3]; print(items.pop(0), items.pop(-1), items);", "1 3 [2]\n"),
        ("var items = [1, 2, 3, 2]; items.remove(2); print(items);", "[1, 3, 2]\n"),
        ("var items = [[1], [2]]; items.remove([1]); print(items);", "[[2]]\n"),
        ("var items = [1, 3]; items.insert(1, 2); print(items);", "[1, 2, 3]\n"),
        ("var items = [1, 2]; items.insert(-5, 0); items.insert(99, 9); print(items);", "[0, 1, 2, 9]\n"),
        ("print([1, 2, 3].index(2), [1, 2, 3].index(9), [\"a\", \"b\"].index(\"b\"));", "1 -1 1\n"),
        ("var items = [1]; items.extend([2, 3]); print(items, len(items));", "[1, 2, 3] 3\n"),
        ("var items = [1, 2, 3]; items.reverse(); print(items);", "[3, 2, 1]\n"),
        ("var items = [1, 2, 3]; items.clear(); print(items, len(items));", "[] 0\n"),
    ]);
}

#[test]
fn dict_methods() {
    expect_outputs(&[
        ("const d = {\"a\": 1, \"b\": 2}; print(d.keys(), d.values(), d.items());", "[\"a\", \"b\"] [1, 2] [[\"a\", 1], [\"b\", 2]]\n"),
        ("const d = {\"a\": 1}; print(d.get(\"a\"), d.get(\"z\"), d.get(\"z\", 0));", "1 null 0\n"),
        ("var d = {\"a\": 1, \"b\": 2}; print(d.pop(\"a\"), d);", "1 {\"b\": 2}\n"),
        ("print({\"a\": 1}.pop(\"z\", 99));", "99\n"),
        ("var d = {\"a\": 1, \"b\": 2}; print(d.delete(\"a\"), d.delete(\"z\"), d);", "true false {\"b\": 2}\n"),
        ("var d = {\"a\": 1, \"b\": 2, \"c\": 3}; d.delete(\"b\"); d[\"d\"] = 4; print(d);", "{\"a\": 1, \"c\": 3, \"d\": 4}\n"),
        ("var d = {\"a\": 1}; d.clear(); print(d, len(d));", "{} 0\n"),
        ("const d = {\"a\": 1, \"b\": 2}; print(d.size(), d.length(), len(d));", "2 2 2\n"),
        ("print({}.size(), {}.length(), len({}));", "0 0 0\n"),
        ("var d = {\"a\": 1}; const read = d.size; d[\"b\"] = 2; print(read(), len(d), d.length());", "2 2 2\n"),
        ("const d = {\"x\": 1, \"y\": 2}; var sum = 0; for k in d.keys() { sum += d[k]; } print(sum);", "3\n"),
        ("for pair in {\"a\": 1, \"b\": 2}.items() { print(pair[0], pair[1]); }", "a 1\nb 2\n"),
        ("const d = {\"a\": 1}; var ks = d.keys(); ks += [\"b\"]; print(d.keys(), ks);", "[\"a\"] [\"a\", \"b\"]\n"),
        ("var d = {1: \"one\", true: \"yes\"}; print(d.get(1), d.get(true), d.get(false, \"no\"));", "one yes no\n"),
    ]);
}

#[test]
fn collection_method_errors() {
    expect_errors(&[
        ("[].pop();", "pop from empty list"),
        ("[1].pop(5);", "pop index out of range"),
        ("[1].pop(0.5);", "pop index must be an integer"),
        ("[1, 2].pop(0, 1);", "pop expects at most 1 argument, got 2"),
        ("[1].remove(9);", "remove: value not in list"),
        ("[1].insert(0.5, 2);", "insert index must be an integer"),
        ("[1].insert(0);", "insert expects 2 arguments, got 1"),
        ("[1].extend(\"x\");", "extend expects a list, got string"),
        ("[1].index();", "index expects 1 argument, got 0"),
        ("[1].clear(1);", "clear expects 0 arguments, got 1"),
        ("[1].size(1);", "size expects 0 arguments, got 1"),
        ("[1].length(x=1);", "length does not accept keyword arguments"),
        ("{}.size(1);", "size expects 0 arguments, got 1"),
        ("7.size();", "size does not accept number"),
        ("7.length();", "length does not accept number"),
        ("null.size();", "size does not accept null"),
        ("true.length();", "length does not accept bool"),
        ("\"x\".pop();", "string has no property \"pop\""),
        ("{}.keys(1);", "keys expects 0 arguments, got 1"),
        ("{}.values(x=1);", "values does not accept keyword arguments"),
        ("{}.get();", "get expects 1 or 2 arguments, got 0"),
        ("{}.pop(\"z\");", "pop: key not found: z"),
        ("{}.delete();", "delete expects 1 argument, got 0"),
        ("{}.delete([]);", "list cannot be a dictionary key"),
        ("{}.rate();", "dict has no property \"rate\""),
    ]);
    assert_eq!(output("try { [].pop(); } catch (error) { print(\"empty list\" in error); }"), "true\n");
}
