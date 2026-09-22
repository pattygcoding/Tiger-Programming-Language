# Built-in Function Reference

[Guide index](README.md) | [Classes and Inheritance](classes-and-inheritance.md) | [Runtime and Errors](runtime-and-errors.md)

Tiger provides four built-ins: `print`, `str`, `len`, and `range`. Their initial bindings are constant. Assignment cannot replace them, but an explicit declaration in an inner scope or a parameter can shadow them.

## print(...values)

Accepts zero or more arguments of any value type. Formats each argument, joins them with a single space, and writes a newline. It returns `null`. There are no keyword arguments for separators or line endings.

```tg
print("score", 12, true, null);
print(["one", "two"], {"ready": true});
const result = print("written");
print(result);
```

Output:

```text
score 12 true null
["one", "two"] {"ready": true}
written
null
```

Zero arguments produce a blank line. A failure writing to the configured output writer becomes a positioned runtime error.

## str(value)

Requires exactly one argument. Returns Tiger's text representation of that value. A string is returned as text without outer quotes. Strings nested inside lists or dictionaries are quoted and escaped. Dictionary entries retain insertion order when formatted.

```tg
print("total=" + str(42));
print(str(false), str(null), str([1, "two"]));
class Empty {}
print(str(Empty), str(Empty()));
const cycle = [null];
cycle[0] = cycle;
print(str(cycle));
```

Output:

```text
total=42
false null [1, "two"]
<class Empty> <Empty instance>
[[...]]
```

Numbers are formatted without unnecessary fractional zeros. Booleans are lowercase. A recursive list reference is represented by `[...]`; a recursive dictionary reference uses `{...}`. Functions print as `<function name>`, built-ins as `<builtin name>`, and bound methods as `<bound method Class.method>`. Formatting does not invoke user methods.

## len(value)

Requires exactly one string, list, or dictionary. Returns a number: Unicode code points for strings, element count for lists, or entry count for dictionaries.

```tg
print(len("Tiger"), len([1, 2, 3]), len({"a": 1, "b": 2}));
print(len(""), len([]), len({}));
```

Output:

```text
5 3 2
0 0 0
```

Instances do not acquire a custom length, even if they define a method named `len`.

**Expected error:**

```tg
len(7);
```

```error
len does not accept number
```

## What Is Not Built In

There is no `input`, `int`, `float`, `type`, `isinstance`, `sum`, `sorted`, general file API, or standard library. Share helper functions through [Tiger file imports](modules.md). See [sorting and searching](../benchmarks/03_sorting_search.tg) and [text processing](../benchmarks/05_strings.tg).

## range(stop), range(start, stop), range(start, stop, step)

Returns a new list starting at `start` (default `0`) and stopping before `stop`. The default step is `1`; negative steps count downward. A direction mismatch produces an empty list. Arguments must be finite integers with absolute value at most `9007199254740991`; step cannot be zero. A range may contain at most 1,000,000 elements. This is an eager list, not a lazy iterator.

```tg
print(range(4), range(2, 7, 2), range(5, 0, -2), range(0));
```

```text
[0, 1, 2, 3] [2, 4, 6] [5, 3, 1] []
```