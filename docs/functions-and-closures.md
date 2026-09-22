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

Use `function`, a required parenthesized parameter list, and a braced body. Parameters are positional, and the number of arguments must match exactly. Duplicate parameter names are syntax errors. There are no default arguments, keyword arguments, variadic user functions, or anonymous/lambda expressions.

Arguments are evaluated left to right. Parameters are local mutable bindings. User-defined functions accept positional arguments only; keyword argument syntax is reserved for supported built-ins such as `print(..., end="")`. Returning without a value, or reaching the end of the body, produces `null`. Functions are first-class values and compare by identity.

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

Write `make_multiplier(factor)` that returns a function. Create independent doubling and tripling functions, then pass one to `map_values`.