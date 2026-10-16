//! Lazy iterators: `range`, `map`, `filter`, generator expressions, and the
//! operations that accept or reject a value that has not produced its elements.

mod common;

use common::{expect_errors, expect_outputs};

#[test]
fn generator_expressions() {
    expect_outputs(&[
        ("print(list((x * x for x in range(1, 5))));", "[1, 4, 9, 16]\n"),
        ("print(list((x for x in [3, 1, 2] if x % 2 == 1)));", "[3, 1]\n"),
        ("print(list((b for a in [1, 2] for b in [a, a * 10])));", "[1, 10, 2, 20]\n"),
        ("var total = 0; for n in (n * n for n in range(1, 1000000)) { if n > 10 { break; } total += n; } print(total);", "14\n"),
        // A generator is a single-use stream: the second pass sees only what is left.
        ("const stream = (n for n in [1, 2, 3]); print(list(stream), list(stream));", "[1, 2, 3] []\n"),
        ("print(type((n for n in [1])), isinstance((n for n in [1]), \"iterator\"));", "iterator true\n"),
        // The clause variable lives in the generator's own scope and closes over
        // the name around it.
        ("var outer = 5; print(list((outer + n for n in [1, 2])), outer);", "[6, 7] 5\n"),
    ]);
}

#[test]
fn lazy_range_map_and_filter() {
    expect_outputs(&[
        ("print(list(range(4)), list(range(2, 7, 2)), list(range(5, 0, -2)), list(range(0)));", "[0, 1, 2, 3] [2, 4, 6] [5, 3, 1] []\n"),
        ("print(sum(range(5)), min(range(3, 0, -1)), max((n for n in [2, 9, 4])));", "10 1 9\n"),
        ("print(list(map([1, 2, 3], function(v) { return v * 2; })));", "[2, 4, 6]\n"),
        ("print(list(filter([1, 2, 3, 4], function(v) { return v % 2 == 0; })));", "[2, 4]\n"),
        // `sorted`, `zip`, and the constructors drain a lazy source.
        ("print(sorted(filter([3, 1, 2], function(v) { return v > 1; })));", "[2, 3]\n"),
        ("print(zip([1, 2], (n * 10 for n in [3, 4])));", "[[1, 30], [2, 40]]\n"),
        ("print(list(tuple((n for n in [1, 2]))), list(set((n for n in [1, 1, 2]))));", "[1, 2] [1, 2]\n"),
        // Membership stops at the first match.
        ("print(7 in (n for n in [1, 7, 3]), 9 in (n for n in [1, 2]));", "true false\n"),
    ]);
}

#[test]
fn iterator_errors() {
    expect_errors(&[
        ("len(range(3));", "len does not accept iterator"),
        ("range(3)[0];", "cannot be indexed"),
        ("range(3) + range(3);", "not supported"),
        ("for value in (n for n in 1) { print(value); }", "number is not iterable"),
        // `map` is lazy, so a bad callable is only rejected when the stream is drained.
        ("list(map([1], 2));", "not callable"),
        ("print(list((x for x in)));", "expected expression"),
    ]);
}
