# Room for Improvement

[Guide index](README.md) | [Project README](../README.md)

This page records the gaps that are most likely to matter to people writing
larger Tiger programs. It is a planning document, not a promise or a roadmap of
committed work: everything here is **absent today**, and the code blocks show
the workarounds that the implemented language forces on you. The
[guide](README.md) teaches the language as it is; this page describes what it is
not yet.

Each entry lists the missing capability, why it matters, a runnable workaround,
a suggested direction, and any constraints that the addition would face.

## 1. Lazy generator expressions and iterators

**Missing:** a lazy iterator: a value that produces its elements on demand, and
the generator-expression syntax that would build one.

**Why it matters:** every collection in Tiger is eager. `range`, `map`, `filter`,
and comprehensions all build the whole result before the caller sees the first
element, so a million-value sequence occupies a million slots even when the caller
stops after the third value, and a sequence without an end cannot be expressed at
all. See [Lists, Dictionaries, Sets, and Tuples](collections.md).

**Workaround:** build only the values that are needed, and stop the loop early.

```tg
const squares = [n * n for n in range(1, 6)];
var total = 0;
for value in squares {
    if value > 10 {
        break;
    }
    total += value;
}
print(squares[0], len(squares), total);
```

Output:

```text
1 5 14
```

**Suggested direction:** a lazy iterator is a runtime value type with a `next`
operation, so `for`, `in`, and the reshaping built-ins would have to accept it,
and `len`, indexing, and slicing would each have to decide what they mean for a
value that has not produced its elements yet. The syntax slot is already
reserved: `(x * 2 for x in items)` reports that generator expressions are not
supported, and points at the comprehension that does the same work eagerly. The
smaller first step is a runtime object behind `range` and `map` while every
`len`-like operation keeps requiring a list, a tuple, a set, or a dictionary.

## 2. Type annotations and optional static checking

**Missing:** a way to annotate parameters, variables, fields, and return values
with types, and any static checking that could use them.

**Why it matters:** annotations are documentation that a checker can verify, and
they power editor completion and refactoring. Tiger has none: a clause header
that names a type instead of `var` is an error, and every value is checked, if
at all, at run time with `type` and `isinstance`.

**Workaround:** guard at run time.

```tg
function area(width, height) {
    if not isinstance(width, "number") or not isinstance(height, "number") {
        throw "area expects numbers";
    }
    return width * height;
}
print(area(3, 4));
```

Output:

```text
12
```

**Suggested direction:** annotations can be parsed and ignored first, so they
document intent and editors can read them, before any checker exists. Because
the language is dynamically typed, a checker would be a separate tool rather
than a change to evaluation; the parser would need to accept a `: type` suffix
after a name and `-> type` after a parameter list without breaking `case` labels
or dictionary colons.

## 3. Context managers and deterministic cleanup

**Missing:** a `with` statement, or an equivalent, that runs cleanup when a
block ends, including on an error.

**Why it matters:** file objects are never closed automatically, and the
runtime has no destructors, so every `open` needs a matching `close` on every
exit path or a descriptor leaks until the process ends. The [Files](file-io.md)
guide warns about this directly on Windows.

**Workaround:** close in `finally`.

```tg
const file = open("notes.txt", "w");
try {
    file.write("alpha");
} finally {
    file.close();
}
print(file_exists("notes.txt"));
```

Output:

```text
true
```

**Suggested direction:** a `with open(path) as handle { ... }` statement that
desugars to the same `try`/`finally` is the smallest change and needs no new
value type. Generalizing it to user objects would need a protocol, an `enter`
and `exit` method pair, plus a way to bind the receiver, which the class system
can already express once the protocol is named.

## 4. Comparison and membership conveniences

**Missing:** chained comparisons (`lower < value < upper`), a `not in` operator,
and an identity operator (`is`).

**Why it matters:** Python's chained comparison reads as the math it mirrors,
and `is`/`is not` distinguishes identity from equality. Tiger has neither, so
the same tests become `and` chains, `not (...)`, and `==`; for classes,
functions, and instances `==` already means identity, so the gap shows mainly
when reading intent.

**Workaround:** spell the operators out.

```tg
const value = 7;
print(0 < value and value < 10);
print(not ("z" in ["a", "b"]));
```

Output:

```text
true
true
```

**Suggested direction:** chained comparison is a parser change that desugars to
the `and` chain and must evaluate the middle operand once. `not in` is a single
new operator that reuses the existing `in` evaluation and negates it. `is` would
need a defined identity notion for numbers and strings, which are values rather
than references, so it is the least clear of the three.

## 5. String and list repetition

**Missing:** sequence repetition with `*`, such as `"ab" * 3` or `[0] * 5`.

**Why it matters:** repeating a character, a separator, or a template is a
common one-liner (`"-" * 40` for a rule), and it composes with `len` and `join`.
In Tiger `*` accepts only numbers, so the same result needs a loop.

**Workaround:** build the repetition with a loop.

```tg
var banner = "";
for step in range(3) {
    banner = banner + "ab";
}
var zeros = [];
for step in range(4) {
    zeros = zeros + [0];
}
print(banner, zeros);
```

Output:

```text
ababab [0, 0, 0, 0]
```

**Suggested direction:** teach the binary operator path to accept a string or
list on the left and a non-negative whole number on the right, mirroring the
existing `+` handling for those types. The count must be range-checked against
the same 1,000,000-element ceiling that `range` uses. There is likewise no `**=`
or `div=` compound assignment, though those are only spelling.

## 6. Standard-library collections and algorithms

**Missing:** ready-made collection types and algorithms, such as a deque, a
binary heap, a counter, and binary search.

**Why it matters:** the [algo](algo.md) module covers named interview problems,
but everyday work still hand-builds the data structures those problems assume.
The [task scheduler](../portfolio-features/task_scheduler.tg) writes a binary
heap by hand, and counting or de-duplicating goes through `algo.takeInventory`
or `algo.removeDuplicates` rather than a `Counter` or a set.

**Workaround:** use a dictionary as a counter.

```tg
const counts = {};
for word in ["red", "blue", "red"] {
    counts[word] = counts.get(word, 0) + 1;
}
print(counts);
```

Output:

```text
{"red": 2, "blue": 1}
```

**Suggested direction:** each addition is a new built-in module rather than
syntax, so it can reuse the existing value types: a counter is a dictionary
wrapper, a heap is a list with `push`/`pop` helpers, and binary search operates
on a sorted list. Keep them in-repo and free of new dependencies so the browser
build stays within its size budget.

## 7. Introspection and reflection

**Missing:** reflective access such as `getattr`, `setattr`, `hasattr`, and
`dir`, and a `hash` function.

**Why it matters:** dynamic languages let a program inspect and drive objects by
name, which powers serialization, plugins, and generic helpers. Tiger exposes
only `type` and `isinstance`, so a generic routine must hard-code the members it
touches. Field access is also stricter than in Python: assigning an undeclared
field is an error, so `setattr` has no natural target.

**Workaround:** dispatch on `type` and `isinstance`.

```tg
class Point {
    var x = 0;
    var y = 0;
}
const point = Point();
print(type(point), isinstance(point, "instance"));
```

Output:

```text
instance true
```

**Suggested direction:** the runtime already stores declared fields and methods
per class, so `hasattr` and a name-based read are small additions. `setattr`
would have to respect the declared-field rule, which is a deliberate design
choice, so a read-only reflection surface is the consistent first step.

## 8. Assertions

**Missing:** an `assert` statement, or function, for invariants and quick
tests.

**Why it matters:** assertions document what a program believes and fail loudly
at the point the belief breaks, and they are the usual way to write the small
checks inside a script. Tiger has only `throw`, which is catchable and carries
no standard "assertion" kind.

**Workaround:** wrap a throw in a helper.

```tg
function check(condition, message) {
    if not condition {
        throw message;
    }
}
try {
    check(1 > 2, "expected failure");
} catch (error) {
    print("caught");
}
```

Output:

```text
caught
```

**Suggested direction:** `assert condition, message;` is a statement that throws
a structured error with its own `kind` when the condition is falsey, so existing
`try`/`catch` and typed guards keep working. Empty blocks are already allowed
(`if true { }` parses), so no separate no-op statement is needed.

## 9. Exception ergonomics

**Missing:** stack traces, a built-in error hierarchy, and exception chaining
(`raise ... from`).

**Why it matters:** a single `file:line:column` position and a `kind` string
identify where an error started but not the call path that reached it, and there
is no way to say that one failure was caused by another when re-throwing. Users
also cannot define their own error types; `throw` accepts any value.

**Workaround:** catch and inspect the structured fields.

```tg
try {
    [1][5];
} catch (error) {
    print(error["kind"]);
}
```

Output:

```text
index range
```

**Suggested direction:** a call stack can be recorded on the runtime error the
evaluator already builds, without changing the value model. A hierarchy needs
classes that the standard error object can extend, and chaining needs a field on
that object, so those two depend on deciding what a user-defined error class is.

## 10. Class-system breadth

**Missing:** static methods and fields, properties (getters and setters),
operator overloading, interfaces or abstract classes, and multiple inheritance.

**Why it matters:** these are how object-oriented code shares behavior and
enforces contracts. Tiger's classes support a single parent, implicit `this`,
declared fields, and `public`/`private`/`protected`, but a class is only a
constructor plus instance methods and fields. See
[Classes and Inheritance](classes-and-inheritance.md).

**Workaround:** use module-level functions and an explicit conversion method.

```tg
class Vec {
    var x = 0;
    var y = 0;
    function ToString() {
        return "Vec(" + str(this.x) + ", " + str(this.y) + ")";
    }
}
print(Vec().ToString());
```

Output:

```text
Vec(0, 0)
```

**Suggested direction:** static members are members resolved on the class rather
than an instance, which the existing class object can hold. Properties need a
call-on-read rule, and operator overloading needs named hooks that the operator
and member-access paths consult; both are larger than they look because they
change how those paths resolve. Interfaces and multiple inheritance would change
how method lookup walks parents.

## 11. String and number helper breadth

**Missing:** `printf`-style formatting (`"%d" % value`), a `format` method, and
the smaller string helpers (`lstrip`, `rstrip`, `count`, `capitalize`, `title`),
plus top-level numeric functions (`abs`, `round`, `pow`, `divmod`, `chr`, `ord`,
`hex`, `bin`) and constants such as `math.pi`.

**Why it matters:** these are the small conveniences that keep code short. Tiger
has f-strings with format specs, `trim`, `find`, `replace`, `padStart`, and
`padEnd`, but `%` on a string is an operator error, `abs`/`round`/`pow` exist
only as `math.abs`/`math.round`/`math.pow`, and `math` has no constants. See
[Strings](strings.md) and [the math module](math.md).

**Workaround:** use f-strings and `math`.

```tg
const name = "Tiger";
print(f"{name}: {3.14159:.2f}");
print("  x  ".trim());
```

Output:

```text
Tiger: 3.14
x
```

**Suggested direction:** the f-string formatter already handles width, padding,
and precision, so most of the string helpers are one-line additions next to the
existing methods, and the missing built-ins can delegate to the `math` module.
`%` formatting is the only large piece, and it overlaps f-strings, so it is the
weakest candidate.

## 12. Enumerations

**Missing:** an `enum` type for a fixed set of named constants.

**Why it matters:** enumerations give a closed set of names a shared type, so a
`switch` can be exhaustive and a value can be checked with `isinstance`. Tiger
uses loose constants, which can collide and are just numbers or strings.

**Workaround:** a constant dictionary of names.

```tg
const Color = {"RED": 0, "GREEN": 1, "BLUE": 2};
print(Color["GREEN"]);
print(Color["RED"] < Color["BLUE"]);
```

Output:

```text
1
true
```

**Suggested direction:** an enum is a class whose members are read-only
attributes, so it can be built from the existing class machinery once member
access can resolve class-level names. Until then a `const` dictionary documents
intent and keeps the names in one place.

## Constraints from the Wasm build

Any addition must keep `crates/tiger-wasm` free of the packages listed under "Wasm size
guardrails" in [`AGENTS.md`](../AGENTS.md). In practice that means:

- The `random` and `json` modules are Wasm-safe: they use integer math and an
  in-repo parser instead of `math/rand` or `encoding/json`.
- `time`, `regex`, and `secrets` are native/WASI-only behind `!js` build tags,
  with small browser stubs, because `time`, `regexp`, and the host entropy source
  are unavailable or costly in `web/tiger.wasm`.
- Sort-based helpers must keep using the in-repo `stableSort` rather than the
  standard `sort` package.

See [Running and Building](running-and-building.md) for the size budget and the
browser and WASI entry points.

## Definition of done for a new feature

Every language or runtime change should ship with, per
[`AGENTS.md`](../AGENTS.md):

- a paired `benchmarks/NN_name.tg` / `NN_name.txt` program that exercises the
  feature, with counts updated in `benchmarks/`, `README.md`,
  [`docs/README.md`](README.md), and
  [`running-and-building.md`](running-and-building.md);
- documentation in the relevant guide, with a runnable example and its expected
  output, so that `docs/docs_test.go` checks it;
- unit tests next to the changed package, including error cases;
- successful `wasm32-unknown-unknown` and `wasm32-wasip1` builds, and a
  passing `cargo test` including the closure guard in CI.

## Deliberate non-goals

Some of the "missing" pieces are choices, not oversights, and should stay out
unless the design changes: arbitrary-precision integers (numbers are float64),
a package manager (imports are file-based), asynchronous syntax and generators
(the runtime is a synchronous tree-walker), and network access in the sandboxed
browser and WASI targets.

[Guide index](README.md) | [Project README](../README.md)

