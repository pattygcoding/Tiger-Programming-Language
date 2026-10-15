# Runtime and Errors

[Guide index](README.md) | [Built-ins](builtins.md) | [Running and Building](running-and-building.md)

## Read a Diagnostic

CLI language errors have the shape `file.tg:line:column: message`. Lines and columns start at one. The lexer tracks positions by Unicode code point. A syntax error can point at the token after a mistake, such as the closing brace where a semicolon was expected.

The browser displays the evaluator's `line:column` diagnostic without a source filename. The Go API returns an error to its caller. A program stops at its first error; output already written is not rolled back. Parsing finishes before execution begins, so a syntax error prevents any statements from running.

## Common Errors

| Message or symptom | Cause | Correction |
| --- | --- | --- |
| `expected ";"` | Missing statement terminator | End the simple statement with `;` |
| `generator expressions are not supported` | A `(x for x in items)` form was written | Use a list comprehension, or a function that returns a list |
| `set comprehensions are not supported` | A `{x for x in items}` form was written | Write `set([x for x in items])` |
| `is not iterable` | A `for` loop or comprehension walked a value that has no elements | Iterate a list, tuple, set, string, or dictionary |
| `sys.exit expects at most 1 argument` | `sys.exit` was given more than one argument | Pass one status, or none |
| `module "x" has no export "y"` | The member is private to the module, or misspelled | Mark it with `export` in the module, or read an exported name |
| `export is only allowed at the top level of a file` | An `export` marker was written inside a block or function | Move the declaration and its marker to the top level |
| A program stops early, with a status but no error | It called `sys.exit(status)`, which no `catch` takes | Look for the call; remove it, or branch on the status in the caller |
| `expected "{"` | Missing block braces or a colon-style header | Use `{ ... }` without a header colon |
| `undefined name` | No binding is visible | Define it before use; check block scope and spelling |
| `cannot reassign const` | Assignment targets an immutable binding | Keep the binding or use a mutable variable |
| `already declared in this scope` | Duplicate explicit declaration | Choose another name or another scope |
| `division by zero` | `/` or `%` has zero on the right | Validate the divisor |
| `index out of range` | List or string index exceeds bounds | Check `len` before indexing |
| `index must be an integer` | Index is fractional or not numeric | Use an integer-valued number |
| `dictionary key not found` | Missing dictionary entry | Check `key in dictionary` first |
| `pop from empty list` / `remove: value not in list` | No element to remove | Check `len` or `value in items` first |
| `pop: key not found` | Dictionary `pop` used a missing key | Use `get`/`delete`, or pass a default |
| `expects ... arguments` | Function, constructor, or method arity mismatch | Match the declared parameters; omit bound `this`; use `*args` for extra positional arguments |
| `missing required argument` | A variadic function did not receive a required named parameter | Pass the missing argument positionally or by keyword |
| `unexpected keyword argument` | A keyword argument has no matching parameter | Match a parameter name or declare `**kwargs` |
| `multiple values for argument` | One parameter received both a positional and a keyword argument | Pass each value once |
| `duplicate keyword argument` | A keyword name was supplied twice, directly or through `**` | Remove the repeated name |
| `type expects 1 argument` | `type` or `isinstance` was called with the wrong number of arguments | Pass the value, and for `isinstance` a type name |
| `isinstance: unknown type name` | `isinstance` name is not one that `type` returns | Use a name from the [`type` reference](builtins.md#typevalue-isinstancevalue-name) |
| `cannot unpack ... expected a list or tuple` | A `*` argument expanded to something else | Spread a list or a tuple |
| `cannot unpack ... expected a dictionary` | A `**` argument expanded to a non-dictionary | Spread a dictionary with string keys |
| `cannot be a set element` | A set literal or `add` was given a mutable or non-finite value | Use strings, numbers, or booleans; convert with `list(...)` |
| `set cannot be indexed` / `cannot be sliced` | A set has no positions | Convert with `list(my_set)`, or use `in` |
| `cannot assign an index of tuple` | A tuple is immutable | Build a new tuple with `+`, or use a list |
| `expects a list or tuple` | A sequence built-in received a set or another value | Convert with `list(value)` |
| `has no property` | Instance member does not exist | Initialize the field or check the method name |
| `super requires a parent class` | A parent method is requested in a class without a parent | Add inheritance or remove the delegation |
| `file ... is not open for reading` | Read attempted on a stream opened without read access | Open with `r`, `r+`, `w+`, or `a+` |
| `file ... is not open for writing` | Write attempted on a read-only stream | Open with `w`, `a`, `r+`, `w+`, or `a+` |
| `I/O operation on closed file` | A file object is used after `close()` | Close once, only when finished |
| `open ... no such file` | Path does not exist or is not readable | Check `file_exists` or catch the error |

## Error Examples

**Expected error: unknown name.**

```tg
if true {
    const local = 1;
}
print(local);
```

```error
undefined name "local"
```

**Expected error: missing dictionary key.**

```tg
const settings = {"port": 8080};
print(settings["host"]);
```

```error
dictionary key not found: host
```

**Expected error: explicit receiver parameter.**

```tg
class Invalid {
    function run(this) { return 1; }
}
```

```error
this is implicit in methods and constructors; remove it from the parameter list
```

## Try, Catch, and Throw

```tg
function require_positive(value) {
    if value <= 0 { throw {"message": "must be positive", "value": value}; }
    return value;
}
try { require_positive(-2); }
catch (error) { print(error["message"], error["value"]); }
try { print(1 / 0); } catch (error) { print("division by zero" in error); }
```

```text
must be positive -2
true
```

`throw expression;` raises any Tiger value, including `null`. `try { ... } catch (name) { ... }` catches thrown values unchanged. A caught runtime error is a structured `error` value: `error["message"]` is the bare diagnostic, `error["kind"]` is a stable category, and `error["origin"]` is the `file:line:column` (or `line:column`) position. `print(error)` and `str(error)` show the positioned diagnostic, and `"text" in error` checks whether that text contains a substring, so older code keeps working. Kinds include `zero division`, `math domain`, `math range`, `numeric range`, `constant reassignment`, `duplicate declaration`, `undefined name`, `index range`, `index type`, `missing key`, `not indexable`, `unsupported operands`, and `missing file`; anything unmatched is `runtime error`. The catch binding is mutable and local. Try and catch blocks have separate scopes. Rethrow with `throw name;`; bare `throw;` is not supported. Errors in a catch propagate to an outer handler. Returns, breaks, and continues pass through try/catch normally.

## Finally

```tg
var closed = false;
try {
    throw "boom";
} catch (error) {
    closed = true;
} finally {
    print("cleanup");
}
print("closed", closed);
```

```text
cleanup
closed true
```

`finally { ... }` runs on every exit path: normal completion, a caught error, an uncaught error, a `return`, and a `break` or `continue`. A `try` may have `finally` without `catch`, and may chain several `catch` clauses before `finally`. A `return`, `break`, or `continue` inside `finally` overrides any in-flight error, and an error raised in `finally` replaces the original. Resource-limit errors skip `finally`.

## Typed Catch

```tg
function classify(value) {
    try {
        return [10, 20][value];
    } catch (error) if isinstance(error, "error") and error["kind"] == "index range" {
        return "range";
    } catch (error) {
        return "other";
    }
}
print(classify(1), classify(5));
```

```text
20 range
```

Each `catch` clause may carry an `if condition` guard, evaluated with the binding in scope; the first clause whose guard is truthy, or that has no guard, runs. Combined with `error["kind"]`, this dispatches on the failure category instead of matching message text. Clauses are tried in order, so a plain `catch (error)` works as the fallback. An error that no clause matches propagates to an outer handler after any `finally` runs.

Uncaught throws stop execution and report `uncaught throw: ...` at the throw position. Parsing errors cannot be caught because parsing precedes execution. Execution-step and evaluation-depth limits cannot be caught. Output and mutations before an error are not rolled back.

## Resource Limits

| Limit | Default and meaning |
| --- | --- |
| Evaluation steps | 1,000,000 per execution; counts evaluated nodes and loop work, not source lines |
| Evaluation depth | 512 nested expression evaluations; not exactly 512 function calls |
| Syntax nesting | 512 parser nesting entries; protects deeply nested expressions and blocks |
| Browser output | 1 MiB of output bytes per run |

Embedded Go callers can set `Evaluator.MaxSteps` to a positive budget or `0` to disable the step limit. The CLI and standalone programs use the default. Evaluation-depth and syntax-depth limits are not configurable through Tiger source. Native execution has no corresponding built-in output-size cap.

These safeguards are not a memory limit, a wall-clock deadline, or a security guarantee for hostile programs. A single large collection or string operation can consume substantial memory. Browser execution occurs in a Web Worker; **Stop** terminates it and starts a new worker. Every run creates a fresh environment.

## Current Language Boundaries

Tiger borrows ideas from Python but is not a Python compatibility layer. It supports [file-based modules](modules.md), [file input and output](file-io.md), and variadic `*args`/`**kwargs` parameters with `*`/`**` call unpacking, but has no package manager, network API, asynchronous syntax, generators, tuples, sets, slicing, comprehensions, or type annotations. Numbers are float64, not arbitrary-precision integers.

`**` is exponentiation, `div` is floor division, and `&`, `|`, `^`, `~`, `<<`, and `>>` are bitwise operators over 64-bit integers. `+=`, `-=`, `*=`, `%=`, `++`/`--`, `break`, `continue`, `switch` (or `match`), loop `else`, both `for` header styles, and `if`/`elif`/`else if` are supported, and `&&`, `||`, and `!` are spellings of `and`, `or`, and `not`. `//` begins a comment; it is not floor division. Classes support one parent, implicit `this`, declared fields, public-by-default members, and `private`/`protected` restrictions; not static members, decorators, multiple parents, or operator overloading.

See [Running and Building](running-and-building.md) for CLI exit codes and host-level execution choices.