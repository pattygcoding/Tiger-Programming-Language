# Runtime and Errors

[Guide index](README.md) | [Built-ins](builtins.md) | [Running and Building](running-and-building.md)

## Read a Diagnostic

CLI language errors have the shape `file.tg:line:column: message`. Lines and columns start at one. The lexer tracks positions by Unicode code point. A syntax error can point at the token after a mistake, such as the closing brace where a semicolon was expected.

The browser displays the evaluator's `line:column` diagnostic without a source filename. The Go API returns an error to its caller. A program stops at its first error; output already written is not rolled back. Parsing finishes before execution begins, so a syntax error prevents any statements from running.

## Common Errors

| Message or symptom | Cause | Correction |
| --- | --- | --- |
| `expected ";"` | Missing statement terminator | End the simple statement with `;` |
| `expected "{"` | Missing block braces or a colon-style header | Use `{ ... }` without a header colon |
| `undefined name` | No binding is visible | Define it before use; check block scope and spelling |
| `cannot reassign const` | Assignment targets an immutable binding | Keep the binding or use a mutable variable |
| `already declared in this scope` | Duplicate explicit declaration | Choose another name or another scope |
| `division by zero` | `/` or `%` has zero on the right | Validate the divisor |
| `index out of range` | List or string index exceeds bounds | Check `len` before indexing |
| `index must be an integer` | Index is fractional or not numeric | Use an integer-valued number |
| `dictionary key not found` | Missing dictionary entry | Check `key in dictionary` first |
| `expects ... arguments` | Function, constructor, or method arity mismatch | Match the declared parameters; omit bound `this`; use `*args` for extra positional arguments |
| `missing required argument` | A variadic function did not receive a required named parameter | Pass the missing argument positionally or by keyword |
| `unexpected keyword argument` | A keyword argument has no matching parameter | Match a parameter name or declare `**kwargs` |
| `multiple values for argument` | One parameter received both a positional and a keyword argument | Pass each value once |
| `duplicate keyword argument` | A keyword name was supplied twice, directly or through `**` | Remove the repeated name |
| `cannot unpack ... expected a list` | A `*` argument expanded to a non-list | Spread a list instead |
| `cannot unpack ... expected a dictionary` | A `**` argument expanded to a non-dictionary | Spread a dictionary with string keys |
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

`throw expression;` raises any Tiger value, including `null`. `try { ... } catch (name) { ... }` catches thrown values unchanged, or ordinary runtime errors as positioned message strings. The catch binding is mutable and local. Try and catch blocks have separate scopes. Rethrow with `throw name;`; bare `throw;` is not supported. Errors in a catch propagate to an outer handler. Returns, breaks, and continues pass through try/catch normally. There is no `finally`, typed catch, or `except`.

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

Tiger borrows ideas from Python but is not a Python compatibility layer. It supports [file-based modules](modules.md), [file input and output](file-io.md), and variadic `*args`/`**kwargs` parameters with `*`/`**` call unpacking, but has no package manager, network API, asynchronous syntax, generators, tuples, sets, slicing, comprehensions, type annotations, destructuring, or default parameter values. Numbers are float64, not arbitrary-precision integers.

There is no exponentiation. `+=`, `-=`, `*=`, `%=`, `++`/`--`, `break`, `continue`, `switch`, loop `else`, C-style `cfor`, and `cif`/`celse` (with `&&`/`||` in their headers) are supported. `//` is a comment marker, not integer division. Classes support one parent, implicit `this`, declared fields, public-by-default members, and `private`/`protected` restrictions; not static members, decorators, multiple parents, or operator overloading.

See [Running and Building](running-and-building.md) for CLI exit codes and host-level execution choices.