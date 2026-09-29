# 8. Functions and Closures

[Previous: Control Flow](control-flow.md) | [Guide index](README.md) | [Next: Classes and Inheritance](classes-and-inheritance.md)

## Define and Call

```tg
function add(left, right) {
    return left + right;
}
function do_nothing() {
    return;
}
function no_return() {
    const local = 1;
}
print(add(3, 4), do_nothing(), no_return());
```

Output:

```text
7 null null
```

Use `function`, a required parenthesized parameter list, and a braced body. Named parameters fill positionally, and the number of arguments must match exactly unless the declaration is variadic. Duplicate parameter names are syntax errors. There are no default values or anonymous/lambda expressions; declare `*args` and `**kwargs` when a call needs optional arguments.

Arguments are evaluated left to right. Parameters are local mutable bindings. Calls may pass positional arguments, `name=value` keyword arguments, `*list` to unpack a list positionally, and `**dict` to unpack a dictionary into keyword arguments. A keyword argument binds to a parameter with the same name; when no such parameter exists it goes to `**kwargs`, or the call fails when the declaration has no `**kwargs`. Returning without a value, or reaching the end of the body, produces `null`. Functions are first-class values and compare by identity.

## Variadic and Keyword Parameters

A trailing `*name` collects extra positional arguments into a list, and a trailing `**name` collects unmatched keyword arguments into a dictionary. Both are optional, and `**kwargs` must come last.

```tg
function describe(name, *tags, **options) {
    return f"{name} tags={tags} options={options}";
}
print(describe("tiger", "wild", "striped", size="large"));
print(describe("cub", order="carnivore"));
```

Output:

```text
tiger tags=["wild", "striped"] options={"size": "large"}
cub tags=[] options={"order": "carnivore"}
```

`tags` is always a list, empty when no extra positional arguments are supplied. `options` is always a dictionary, and it preserves the order in which the caller wrote the keywords. A function may declare only `*args` or only `**kwargs`; `function total(*values) {}` and `function settings(**fields) {}` are both valid.

## Unpacking Calls

An argument prefixed with `*` expands a list, and an argument prefixed with `**` expands a dictionary. Keywords bind to parameters by name before falling through to `**kwargs`.

```tg
function total(first, *rest) {
    var sum = first;
    for value in rest {
        sum = sum + value;
    }
    return sum;
}
function point(x, y) {
    return [x, y];
}
const numbers = [1, 2, 3];
const center = {"x": 4, "y": 5};
print(total(*numbers), total(10, *[20, 30]));
print(point(1, 2), point(y=9, x=8), point(*[3, 4]), point(**center));
```

Output:

```text
6 60
[1, 2] [8, 9] [3, 4] [4, 5]
```

Unpacking composes with forwarding, so a wrapper can relay every argument it received. A `*` argument must expand to a list and a `**` argument must expand to a dictionary; any other value is a runtime error. A `**` dictionary must use string keys. Duplicate keyword names, whether written directly or produced by a `**` expansion, are errors.

```tg
function log(level, *messages, **fields) {
    return f"{level}: {messages} {fields}";
}
function forward(*args, **kwargs) {
    return log("info", *args, **kwargs);
}
print(forward("started", user="ada"));
```

Output:

```text
info: ["started"] {"user": "ada"}
```

The same rules apply to methods and constructors: a method still declares `this` first, then any named parameters, then `*rest` and `**options`.

## Recursion

```tg
function factorial(number) {
    if number <= 1 {
        return 1;
    }
    return number * factorial(number - 1);
}
print(factorial(0), factorial(5));
```

Output:

```text
1 120
```

A function retains its defining environment, allowing it to resolve its own name. Function declarations execute in program order; a called name must be available when the call runs. There is no tail-call optimization, and deeply nested evaluation reaches a runtime depth limit.

## Callbacks

```tg
function double(value) {
    return value * 2;
}
function map_values(values, transform) {
    var result = [];
    for value in values {
        result = result + [transform(value)];
    }
    return result;
}
print(map_values([1, 2, 3], double));
```

Output:

```text
[2, 4, 6]
```

Pass `double`, not `double(...)`, when the receiving function should call it later. Functions can also be stored in lists, dictionaries, and instance fields.

## Closures Retain State

```tg
function make_counter(start) {
    var current = start;
    function next() {
        current = current + 1;
        return current;
    }
    return next;
}
const first = make_counter(0);
const second = make_counter(10);
print(first(), first(), second(), first());
```

Output:

```text
1 2 11 3
```

Each call to `make_counter` creates a new environment. Its returned function keeps that environment alive. Assignment in `next` updates the captured binding because it is the nearest existing `current`.

An important consequence: ordinary assignment updates the nearest existing binding and fails if no binding exists. Declare private state with `var` or `const` in the function's scope, even if an outer name matches. Parameters provide fresh local mutable bindings; `var` and `const` provide fresh local mutable and immutable bindings, respectively.

## Per-Iteration Capture

```tg
var callbacks = [];
for factor in [2, 3, 4] {
    function multiply(value) {
        return value * factor;
    }
    callbacks = callbacks + [multiply];
}
print(callbacks[0](5), callbacks[1](5), callbacks[2](5));
```

Output:

```text
10 15 20
```

Each loop iteration has a fresh binding for `factor`. The callbacks retain those distinct environments rather than all seeing the last value.

## Try It

Write `make_multiplier(factor)` that returns a function. Create independent doubling and tripling functions, then pass one to `map_values`. Then write `total(*values)` that sums any number of numbers and call it with `total(*[1, 2, 3])`.