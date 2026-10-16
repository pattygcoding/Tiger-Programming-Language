//! Sets and tuples: the two collection types the roadmap listed as missing.
//!
//! A set is an insertion-ordered collection of hashable scalars (`{1, 2}`), so
//! membership and de-duplication are built in; a tuple is a fixed-length, read-only
//! group (`(1, 2)`), so a pair is distinct from a two-element list. Both are
//! iterable, structural in equality, and report their length through `len`,
//! `size()`, and `length()`.

mod common;

use common::{expect_errors, expect_outputs};

#[test]
fn set_literals_and_membership() {
    expect_outputs(&[
        ("print({1, 2, 3}, len({1, 2, 3}), {1, 2}.size(), {1, 2}.length());", "{1, 2, 3} 3 2 2\n"),
        // A repeat is absorbed, and the order the elements were written in is kept.
        ("print({3, 1, 3, 2});", "{3, 1, 2}\n"),
        ("print(type({1}), type({}), type(set()), isinstance({1}, \"set\"));", "set dict set true\n"),
        ("print(2 in {1, 2}, 9 in {1, 2}, {1, 2} == {2, 1}, {1} == {1, 2});", "true false true false\n"),
        ("print(set([1, 2, 2]), set((1, 2)), set(\"ab\"), set({\"a\": 1}), set({3, 1}));", "{1, 2} {1, 2} {\"a\", \"b\"} {\"a\"} {3, 1}\n"),
        ("print(bool(set()), bool({0}), bool({}), bool({0: 1}));", "false true false true\n"),
        ("print(len({1.5, 2.5}), 2.5 in {1.5, 2.5});", "2 true\n"),
    ]);
}

#[test]
fn set_methods() {
    expect_outputs(&[
        ("var s = {1, 2}; s.add(3); s.add(1); print(s, s.has(1), s.has(9));", "{1, 2, 3} true false\n"),
        ("var s = {1, 2}; print(s.remove(2), s.remove(2), s);", "true false {1}\n"),
        ("var s = {1, 2}; s.clear(); print(s, s.has(1));", "set() false\n"),
        ("print({1, 2}.union([3, 1]), {1, 2}.intersection({2, 9}), {1, 2}.difference({2}));", "{1, 2, 3} {2} {1}\n"),
        ("print({1, 2}.union((9, 1)), {1, 2, 3}.intersection(\"ab\"), {1}.difference([]));", "{1, 2, 9} set() {1}\n"),
        ("print({1, 2}.union(set()), {1, 2}.intersection({1, 2}), {1, 2}.difference({1, 2}));", "{1, 2} {1, 2} set()\n"),
        // The algebra returns a fresh set, leaving the receiver alone.
        ("var a = {1, 2}; var b = a.union({3}); b.add(4); print(a, b);", "{1, 2} {1, 2, 3, 4}\n"),
        // Iteration follows insertion order, which is what `list` and `union` use.
        ("for value in {3, 1, 2} { print(value); }", "3\n1\n2\n"),
        ("print(list({3, 1}), tuple({3, 1})[0], sum(list({1, 2, 3})), sorted(list({3, 1, 2})));", "[3, 1] 3 6 [1, 2, 3]\n"),
        ("var total = 0; for value in {1, 2, 3} { total += value; } print(total);", "6\n"),
    ]);
}

#[test]
fn tuple_literals_and_indexing() {
    expect_outputs(&[
        ("print((1, 2), (1,), (), len((1, 2, 3)));", "(1, 2) (1,) () 3\n"),
        // Parentheses alone group; only a comma makes a tuple.
        ("print(type((1, 2)), type((1)), type(()), (1, 2).size(), (1, 2).length());", "tuple number tuple 2 2\n"),
        ("print((1, 2, 3)[0], (1, 2, 3)[-1], (1, 2, 3)[1:], (1, 2, 3)[::-1], (1, 2, 3)[::2]);", "1 3 (2, 3) (3, 2, 1) (1, 3)\n"),
        ("print((1, 2) == (1, 2), (1, 2) == (2, 1), (1, 2) == [1, 2], 2 in (1, 2));", "true false false true\n"),
        ("print((1, 2) + (3, 4), tuple([1]), tuple(\"ab\"), bool(()), bool((0,)));", "(1, 2, 3, 4) (1,) (\"a\", \"b\") false true\n"),
        ("const [a, b] = (1, 2); print(a, b);", "1 2\n"),
        ("print((1, (2, 3)).ToString(), json.stringify((1, (2, 3))), json.stringify([(1, 2)]));", "(1, (2, 3)) [1,[2,3]] [[1,2]]\n"),
        ("print(type(sorted((2, 1))), sorted((2, 1)), sum((1, 2, 3)), min((3, 1)), enumerate((7, 8)));", "list [1, 2] 6 1 [[0, 7], [1, 8]]\n"),
        (
            "print(zip((1, 2), (\"a\", \"b\")), list(map((1, 2), function(v) { return v * 10; })), list(filter((1, 2, 3), function(v) { return v != 2; })));",
            "[[1, \"a\"], [2, \"b\"]] [10, 20] [1, 3]\n",
        ),
        ("print(isinstance((1,), \"tuple\"), isinstance((1,), [\"tuple\", \"list\"]), isinstance((1,), \"list\"));", "true true false\n"),
        // A tuple can be unpacked into a call like a list.
        ("function add(a, b) { return a + b; } print(add(*(1, 2)));", "3\n"),
        // Nesting keeps the two collection shapes distinct in formatting.
        ("print((1, {2, 3}), [{1, 2}, (3,)]);", "(1, {2, 3}) [{1, 2}, (3,)]\n"),
    ]);
}

#[test]
fn set_and_tuple_errors() {
    expect_errors(&[
        // Only the scalars a dictionary accepts as keys can be elements.
        ("{[1]};", "list cannot be a set element"),
        ("{{1, 2}};", "set cannot be a set element"),
        ("{null};", "null cannot be a set element"),
        ("{(1, 2): 3};", "tuple cannot be a dictionary key"),
        ("{{1}: 2};", "set cannot be a dictionary key"),
        // A set has no positions, and a tuple has no in-place operations.
        ("{1, 2}[0];", "set cannot be indexed"),
        ("{1, 2}[0:1];", "set cannot be sliced"),
        ("{1, 2}[0] = 9;", "cannot assign an index of set"),
        ("const items = (1, 2); items[0] = 9;", "cannot assign an index of tuple"),
        ("(1, 2).append(3);", "tuple has no property \"append\""),
        ("(1, 2).sort();", "tuple has no property \"sort\""),
        ("(1, 2).missing;", "tuple has no property \"missing\""),
        ("{1}.append(2);", "set has no property \"append\""),
        ("(1, 2) + [3];", "operator + is not supported for tuple and list"),
        ("{1} + {2};", "operator + is not supported for set and set"),
        ("(1, 2)[0.5];", "index must be an integer"),
        ("(1, 2)[9];", "index out of range"),
        ("for value in 1 { }", "number is not iterable"),
        // The constructors reject a non-iterable and an extra argument.
        ("set(1);", "set expects an iterable, got number"),
        ("list(1);", "list expects an iterable, got number"),
        ("tuple(x=1);", "tuple does not accept keyword arguments"),
        ("set([1], [2]);", "set expects at most 1 argument, got 2"),
        ("{1, 2}.add();", "add expects 1 argument, got 0"),
        ("{1, 2}.add([3]);", "list cannot be a set element"),
        ("{1, 2}.has();", "has expects 1 argument, got 0"),
        ("{1, 2}.union(1);", "union expects an iterable, got number"),
        // A set is unordered, so the sequence built-ins ask for a list or tuple.
        ("sum({1, 2});", "expects a list or tuple, got set"),
        ("sorted({1});", "sorted expects a list or tuple, got set"),
        ("map({1}, len);", "map expects a list or tuple, got set"),
        ("zip((1,), 2);", "zip expects lists or tuples"),
        ("json.stringify({1});", "json.stringify cannot serialize set"),
    ]);
}
