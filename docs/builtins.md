# Built-in Function Reference

[Guide index](README.md) | [Classes and Inheritance](classes-and-inheritance.md) | [Runtime and Errors](runtime-and-errors.md)

Tiger provides the general built-ins `print`, `input`, `str`, `int`, `float`, `bool`, `len`, and `range`, the [`math` module](math.md), the [`algo` module](algo.md), plus the file functions `open`, `read_file`, `write_file`, `append_file`, `file_exists`, and `remove_file` described in [Files](file-io.md). Their initial bindings are constant. Assignment cannot replace them, but an explicit declaration (at the top level or in an inner scope) or a parameter can shadow them; for example, `import "lib/math.tg" as math;` hides the built-in `math` in that file. Built-in signatures are fixed; only user-defined functions and methods may declare `*args` or `**kwargs` (see [Functions and Closures](functions-and-closures.md)).

## print(...values, end="\n")

Accepts zero or more positional arguments of any value type. Formats each argument, joins them with a single space, and writes the string supplied by the optional `end` keyword argument. `end` defaults to a newline and must be a string. It returns `null`.

```tg
print("score", 12, true, null);
print(["one", "two"], {"ready": true});
const result = print("written");
print(result);
print("loading", end="");
print("...", end=" done\n");
```

Output:

```text
score 12 true null
["one", "two"] {"ready": true}
written
null
loading... done
```

Zero arguments produce a blank line unless `end` is changed. `print` accepts no other keyword arguments. A failure writing to the configured output writer becomes a positioned runtime error.

## input(prompt)

Works like Python's `input()`. If a prompt is given, it is formatted like `str(prompt)` and written without a trailing newline. `input` then reads one line from standard input and returns it as a string without the line ending (`\n` or `\r\n`). The result is always a string; convert it with `int` or `float` when you need a number.

```tg
const name = input("Name: ");
const age = input("Age: ");
print("Hello, " + name + "! You entered age " + age + ".");
print(input() == "", len(name));
```

Input:

```input
Ada
36

```

Output:

```text
Name: Age: Hello, Ada! You entered age 36.
true 3
```

The prompt has no newline, so in a terminal the typed text appears on the same line; when input is redirected from a file, the prompt is still written but the input text is not echoed. A last line without a trailing newline is still returned. At end of input with nothing left to read, `input` raises a catchable `EOF when reading a line` error, so a `while` loop with `try`/`catch` can read until input runs out. `input` accepts at most one positional argument and no keyword arguments.

`tiger run`, standalone executables built with `tiger build`, and the WASI build read standard input. In the browser playground, `input` shows a field at the end of the output; type a line and press Enter to continue the program, or press Ctrl+D to send end of input. Embedding with `evaluator.Run` has no input stream, so `input` raises `input is unavailable: no input stream is connected`; use `evaluator.RunWithInput` or set `Evaluator.Input` to supply a reader.

**Expected error:**

```tg
input("Name: ");
```

```error
input is unavailable
```

## str(value)

Returns Tiger's text representation of a value; `str()` with no argument returns `""`. A string is returned as text without outer quotes. Strings nested inside lists or dictionaries are quoted and escaped. Dictionary entries retain insertion order when formatted.

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

## int(value), float(value), bool(value)

Python-style conversions. Tiger has a single number type, so `int` and `float` both return numbers: `int` truncates toward zero, and `float` keeps any fraction. Mixed arithmetic needs no conversion, so `int("2") + 0.5` is `2.5`.

```tg
print(int(3.9), int(-3.9), int(" 42 "), int(true));
print(float("2.5") * 2, float("1e3"), float("-.5"), float(false));
print(bool(0), bool(""), bool(null), bool([]), bool("no"), bool([0]));
print(int(), float(), bool(), str() == "");
print(int(input()) + int(input()));
```

Input:

```input
19
23
```

Output:

```text
3 -3 42 1
5 1000 -0.5 0
false false false false true true
0 0 false true
42
```

| Function | Accepts | Result |
| --- | --- | --- |
| `int(value)` | number, bool, or a string of optional sign and decimal digits | Whole number, truncated toward zero; `true` is `1`, `false` is `0` |
| `float(value)` | number, bool, or a decimal string such as `"2.5"`, `".5"`, `"4."`, `"1e3"` | Number |
| `str(value)` | any value | Text, as above |
| `bool(value)` | any value | The value's [truthiness](types-and-operators.md#truthiness-and-short-circuiting) |

Surrounding whitespace in strings is ignored. `int("3.5")`, `int("1e3")`, `float("inf")`, `float("nan")`, hexadecimal text, and any other malformed text raise catchable errors, as do lists, dictionaries, `null`, and instances passed to `int` or `float`. A string that overflows float64, such as `"1e400"`, is also an error because Tiger numbers must be finite. Each function returns its zero value when called with no argument and accepts no keyword arguments.

**Expected error:**

```tg
int("3.5");
```

```error
int: invalid integer literal "3.5"
```

## Conversion Methods: ToString, ToInt, ToFloat, ToBool

Every value, including class instances, has zero-argument conversion methods equivalent to the built-ins: `value.ToString()` is `str(value)`, `value.ToInt()` is `int(value)`, `value.ToFloat()` is `float(value)`, and `value.ToBool()` is `bool(value)`.

```tg
class Point { var x = 1; }
class Money {
    var cents = 250;
    function ToString() { return "$" + str(this.cents / 100); }
}
const count = "12".ToInt();
print(count + 1, 9.75.ToInt(), "2.5".ToFloat() * 2, true.ToInt());
print(42.ToString() + "!", null.ToString(), [1, 2].ToString());
print(0.ToBool(), "".ToBool(), {"a": 1}.ToBool(), Point().ToBool());
print(Point().ToString(), Money().ToString(), str(Money()));
```

Output:

```text
13 9 5 1
42! null [1, 2]
false false true true
<Point instance> $2.5 <Money instance>
```

A class may define its own method with one of these names; the user method takes precedence when called through the instance. `str`, `print`, and f-strings still use the built-in formatting and never call user methods. Conversion methods can be extracted like other methods: `const read = "7".ToInt;` stores a callable. Conversion failures produce the same errors as the built-ins, prefixed with the method name.

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

Lists and strings also provide equivalent zero-argument `size()` and `length()` methods:

```tg
const items = [1, 2, 3];
print(items.size(), items.length(), "Tiger".size(), "Tiger".length());
```

```text
3 3 5 5
```

Lists also have an in-place, Python-style `sort(key=..., reverse=...)` method; see [Sorting](collections.md#sorting).

**Expected error:**

```tg
len(7);
```

```error
len does not accept number
```

## Built-in Modules

Two read-only modules are available without an import:

- [`math`](math.md): `ceil`, `floor`, `round`, `sqrt`, `abs`, `pow`, `log`, `sin`, `cos`, `tan`, `min`, and `max`.
- [`algo`](algo.md): `fibonacci`, `fibonacciList`, `factorial`, `factorialList`, `removeDuplicates`, `findMatches`, `takeInventory`, `findPlace`, `isPrime`, `rainwater`, `medianSorted`, `mergeKSorted`, `editDistance`, `regexMatch`, and the `slidingWindow*` and `palindrome*` functions.

```tg
print(math.sqrt(16), algo.findMatches([1, 2, 3], [3, 1]));
```

```text
4 [1, 3]
```

## What Is Not Built In

There is no `type`, `isinstance`, `sum`, `sorted`, network API, or standard library beyond `math` and `algo`. File input and output use the helpers and file objects in [Files](file-io.md). Share other helper functions through [Tiger file imports](modules.md). See [sorting and searching](../benchmarks/03_sorting_search.tg) and [text processing](../benchmarks/05_strings.tg).

## range(stop), range(start, stop), range(start, stop, step)

Returns a new list starting at `start` (default `0`) and stopping before `stop`. The default step is `1`; negative steps count downward. A direction mismatch produces an empty list. Arguments must be finite integers with absolute value at most `9007199254740991`; step cannot be zero. A range may contain at most 1,000,000 elements. This is an eager list, not a lazy iterator.

```tg
print(range(4), range(2, 7, 2), range(5, 0, -2), range(0));
```

```text
[0, 1, 2, 3] [2, 4, 6] [5, 3, 1] []
```