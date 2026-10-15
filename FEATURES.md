# Features Added Since Control Flow

A feature-by-feature record of everything added to Tiger **after the
`16_control_flow` benchmark landed**. That benchmark (together with
`17_access_exceptions`, which shipped in the same batch) is the cutoff:
everything below is present in the current language and arrived afterwards. Each
item names the benchmark program that exercises it when one exists.

For how to *use* these features, see the [guide](docs/README.md); for the gaps
that remain, see [Room for Improvement](docs/room-for-improvement.md).

`16_control_flow` itself introduced prefix/postfix `++`/`--`, the parenthesized
and bare `for`/`if` headers, `elif`/`else if`, `&&`/`||`/`!`, `switch` (spelled
`match`), `case _`, `range`, loop `else`, and `break`/`continue`; those are not
repeated below.

## At a glance

| Area | Benchmark |
| --- | --- |
| Access modifiers and exceptions | `17_access_exceptions` |
| Variadic functions and `*`/`**` unpacking | `18_variadics` |
| File I/O | `19_file_io` |
| List sorting | `20_list_sort` |
| `input()` | `21_input` |
| Conversions and truthiness | `22_conversions` |
| `math` module | `23_math` |
| `algo` module | `24_algo` … `31_algo_factorials` |
| `type` and `isinstance` | `32_type_inspection` |
| String methods | `33_string_methods` |
| List and dictionary methods | `34_collection_methods` |
| Operators (`**`, `div`, bitwise) | `35_operators` |
| Standard library (`random`, `secrets`, `json`, `time`, `regex`, `sys`, `list_dir`) | `36_stdlib` |
| Function ergonomics | `37_function_ergonomics` |
| Aggregate built-ins and comprehensions | `38_aggregates` |
| Text precision | `39_text_precision` |
| Syntax additions | `40_syntax_gaps` |
| Sets and tuples | `41_sets_and_tuples` |
| File imports and exports | `42_module_exports` |

## Language core

### Access modifiers and exceptions (`17_access_exceptions`)

- Members are **public by default**; optional `public`, `private`, and
  `protected` modifiers on methods and declared fields.
- `private` permits access only from the declaring class; `protected` also
  allows descendants. Checks cover reads, writes, `++`/`--`, method extraction,
  and `super` lookup, and nested functions keep their lexical access context.
- `try { ... } catch (name) { ... } finally { ... }` and `throw value;`, with
  `finally` running on every exit path (normal, error, `return`, `break`,
  `continue`).
- **Typed catch guards**: `catch (error) if <condition> { ... }`, tried in order.
- Caught runtime errors are **structured values**: `error["message"]`,
  `error["kind"]`, and `error["origin"]`; `"text" in error` still works, and
  `throw error;` rethrows. Kinds include `zero division`, `math domain`,
  `math range`, `numeric range`, `constant reassignment`, `duplicate
  declaration`, `undefined name`, `index range`, `index type`, `missing key`,
  `not indexable`, `unsupported operands`, and `missing file`.

```tg
class Counter {
    private var value = 0;
    function next() { return ++this.value; }
}
const counter = Counter();
try { print(counter.value); } catch (error) { print("private" in error); }
```

### Variadic functions and unpacking (`18_variadics`)

- A parameter list may end with `*args` (extra positional arguments collected
  into a list) and `**kwargs` (unmatched keyword arguments collected into a
  dictionary).
- A call may pass `name=value` keywords, `*list` to unpack a list positionally,
  and `**dict` to unpack a dictionary into keywords.
- The same rules apply to functions, methods, and constructors.

```tg
function describe(name, *tags, **options) {
    return f"{name} {tags} {options}";
}
print(describe("tiger", "wild", size="large"));
```

### Function ergonomics (`37_function_ergonomics`)

- **Default parameter values**: `function greet(name, greeting = "hello") { ... }`;
  the default expression evaluates in the defining environment.
- **Anonymous functions** (lambdas): `function(value) { return value * 2; }` used
  where a value is expected.
- **Multiple return values**: `return a, b;` produces the list `[a, b]`.
- **Destructuring**: `const [x, y] = pair();` and `const {name} = record;`.

### Aggregate built-ins (`38_aggregates`)

- `sum`, `min`, `max`, `any`, `all`, `enumerate`, `zip`, `map`, `filter`, and the
  non-mutating `sorted(list, key=..., reverse=...)`.
- `zip` cuts to the shortest list; `enumerate` produces `[[0, first], [1, second], ...]`.

### Conversions and truthiness (`22_conversions`)

- `int(value)`, `float(value)`, `str(value)`, and `bool(value)`.
- Conversion methods on every value, including class instances:
  `.ToString()`, `.ToInt()`, `.ToFloat()`, and `.ToBool()`, with user overrides.

### Type inspection (`32_type_inspection`)

- `type(value)` returns a name such as `number`, `string`, `bool`, `null`,
  `list`, `dict`, `function`, `builtin`, `class`, `instance`, `bound method`,
  `super`, `module`, `file`, or `error`.
- `isinstance(value, name)` and `isinstance(value, [names])`.

### Operators (`35_operators`)

- `**` exponentiation (right-associative, binds tighter than unary minus) and
  `div` floor division.
- Bitwise `&`, `|`, `^`, `~`, `<<`, `>>` over 64-bit two's-complement integers,
  with truncation of fractional operands and 64-bit shift handling.

## Collections and text

### Sets and tuples (`41_sets_and_tuples`)

- A set is an insertion-ordered collection of unique scalars: `{1, 2, 3}`, with
  `set()` for the empty set because `{}` stays an empty dictionary. `add`,
  `remove`, `has`, and `clear` mutate it; `union`, `intersection`, and
  `difference` take any iterable and return a new set. Membership (`in`, `has`),
  iteration, equality (order-insensitive), `len`/`size()`/`length()`, and `type`
  work as they do for the other collections.
- A tuple is a fixed-length, immutable group: `(1, 2)`, `(1,)`, and `()`. It
  supports indexing, slicing, `+`, membership, iteration, destructuring, and
  structural equality, and the sequence built-ins (`sum`, `sorted`, `zip`, `map`,
  `filter`, `min`, `max`, `any`, `all`, `enumerate`) accept it wherever they
  accept a list. It has no `append`, no `sort`, and no assignment to an index.
- `list(value)`, `tuple(value)`, and `set(value)` convert any iterable, and
  `json.stringify` writes a tuple as a JSON array.

### Comprehensions (`38_aggregates`)

- A list comprehension builds a list in one expression:
  `[x * x for x in range(1, 5)]`, with any number of `if` filters —
  `[x for x in values if x > 0 if x < 10]` — and one or more nested clauses, which
  run left to right: `[b for a in [1, 2] for b in [a, a * 10]]`.
- A dictionary comprehension uses the brace form: `{word: len(word) for word in
  words}`, with the same filters and clauses. A repeated key keeps its first
  position and takes the last value.
- Any value a `for` loop can walk may be iterated, and the clause variables live
  in a scope of their own, so they neither leak nor disturb an outer name. The
  runtime is eager: there are no generator expressions, and a set comprehension is
  reported with the workaround `set([x for x in items])`.

### List and dictionary methods (`34_collection_methods`)

- Lists: `pop()`, `pop(index)`, `remove(value)`, `insert(index, value)`,
  `index(value)`, `extend(other)`, `reverse()`, `clear()`, and `join(separator)`,
  plus `size()`/`length()` as aliases for `len(items)`.
- Dictionaries: `keys()`, `values()`, `items()`, `get(key, default?)`,
  `pop(key, default?)`, `delete(key)`, and `clear()`.

### List sorting (`20_list_sort`)

- In-place `items.sort()` accepting `key=` (a function called once per element)
  and `reverse=`; the sort is stable, including when reversed. Elements or keys
  must be all numbers or all strings.

### String methods (`33_string_methods`)

- `split()`, `split(separator)`, `upper()`, `lower()`, `trim()`, `replace(old, new)`,
  `startswith(prefix)`, `endswith(suffix)`, `find(substring)`,
  `padStart(width, fill)`, and `padEnd(width, fill)`, plus `items.join(separator)`.

### Text precision (`39_text_precision`)

- **f-string format specifications**: `f"{value:.2f}"`, `f"{value:>8}"`, zero
  padding and alignment, with the shape `[<|>|^][0][width][.Nf]`.
- **Escapes**: `\xHH`, `\uXXXX`, and `\UXXXXXXXX` name characters.
- **Raw strings**: `r"\d+"` keeps backslashes literal (no escapes or interpolation).
- **`graphemes(text)`** groups text into approximate user-perceived characters
  (combining marks, emoji modifiers, flags, and ZWJ sequences).

## Syntax additions (`40_syntax_gaps`)

- **Conditional expressions**: `"even" if count % 2 == 0 else "odd"`, including
  right-grouping chains.
- **Slicing**: `items[1:3]`, `items[::-1]`, `text[:2]`, with negatives and
  clamping, for lists and strings.
- **Labeled `break`/`continue`**: `outer: for i in range(3) { break outer; }`
  across nested loops.
- **`do...while`**: `do { ... } while (condition);`, with `continue` re-checking
  the condition.

## Input and files

### `input()` (`21_input`)

- `input(prompt)` writes the prompt (no newline) and reads one line, returning it
  without the line ending; at end of input it raises a catchable
  `EOF when reading a line` error.

### File I/O (`19_file_io`)

- `open(path, mode)` with modes `r`, `w`, `a`, `r+`, `w+`, `a+` (append `b` for a
  binary stream), returning a file object.
- File-object members: `read(size?)`, `readline()`, `readlines()`, `write(text)`,
  `writelines(list)`, `flush()`, `close()`, `seek(offset, origin?)`, `tell()`,
  and the `closed`, `name`, and `mode` properties.
- Helpers: `read_file`, `write_file`, `append_file`, `file_exists`,
  `remove_file`, and `list_dir`.

## Modules and the standard library

### File imports and exports (`docs/modules.md`)

- `import "path/to/file.tg" as name;` and `import * as name from "path/to/file.tg";`
  for a whole module, with isolated per-file namespaces, once-per-run module
  caching, paths relative to the importing file, and standalone-executable
  bundling.
- `import { member, member as alias } from "path/to/file.tg";` binds selected
  names directly, as constants.
- `export` in front of a declaration shares it; everything else in the file stays
  private while its own functions keep using it. `export import` re-exports a
  module under its alias.
- Exported classes can be constructed and extended, and imported
  collections/instances stay mutable.

### `math` module (`23_math`)

- `math.ceil`, `floor`, `round` (half to even), `sqrt`, `abs`, `pow`, `log`,
  `sin`, `cos`, `tan`, `min`, and `max`, with domain and range errors.

### `algo` module (`24_algo` … `31_algo_factorials`)

- `algo.fibonacci`, `fibonacciList`, `factorial`, `factorialList`,
  `removeDuplicates`, `findMatches`, `takeInventory`, `findPlace`, `isPrime`,
  `rainwater`, `medianSorted`, `mergeKSorted`, `editDistance`, `regexMatch`, the
  `slidingWindow*` helpers (`slidingWindowMax`, `slidingWindowSumMax`,
  `slidingWindowMinLen`, `slidingWindowLongestUnique`, `slidingWindowMinSubstring`),
  and the `palindrome*` helpers (`palindromeValid`, `palindromeCanBeValid`,
  `palindromeLongest`, `palindromeCount`, `palindromeMinCuts`).

### Standard-library additions (`36_stdlib`)

- `random`: a deterministic xorshift64* PRNG — `seed`, `random`, `int`,
  `uniform`, `choice`, `shuffle`.
- `secrets`: non-reproducible randomness that needs no seeding — `random`, `int`,
  `uniform`, `choice`, `shuffle`; native and WASI only.
- `json`: `parse` and `stringify`.
- `time`: `now`, `now_ms`, `sleep`, `iso`, `parts`; native and WASI only.
- `regex`: `match`, `find`, `findAll`, `groups`, `replace`, `split`; native and
  WASI only.
- `sys`: `argv` (the program's path first, then its arguments), `env` (a dictionary
  of the inherited environment), and `exit(status)`, which stops the program with
  Python's status rules and reaches the caller.
- `list_dir(path)`: a sorted list of a directory's immediate entries.

## Documentation added since the milestone

- New guides: `docs/modules.md`, `docs/math.md`, `docs/algo.md`,
  `docs/standard-library.md`, `docs/file-io.md`, and
  `docs/room-for-improvement.md`.
- Expanded guides: `docs/builtins.md`, `docs/collections.md`, `docs/strings.md`,
  `docs/functions-and-closures.md`, `docs/types-and-operators.md`, and
  `docs/control-flow.md`.

## Still missing

See [Room for Improvement](docs/room-for-improvement.md) for the gaps that
remain (lazy iterators, type annotations, context managers, and more), each with
a runnable workaround and a suggested direction.

