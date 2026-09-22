# 7. Control Flow

[Previous: Lists and Dictionaries](collections.md) | [Guide index](README.md) | [Next: Functions and Closures](functions-and-closures.md)

## If, Elif, and Else

```tg
for score in [42, 75, 95] {
    if score >= 90 {
        print("excellent");
    } elif score >= 60 {
        print("pass");
    } else {
        print("retry");
    }
}
```

Output:

```text
retry
pass
excellent
```

Conditions use [truthiness](types-and-operators.md), not just literal booleans. Branches are tested in order, and only the first matching branch executes. `else` is optional. Blocks require braces; parentheses around conditions are optional.

## While

```tg
var remaining = 3;
while remaining > 0 {
    print(remaining);
    remaining = remaining - 1;
}
print("go");
```

Output:

```text
3
2
1
go
```

The condition is checked before each iteration, so the body can run zero times. Initialize persistent loop state outside the body; names first created inside an iteration do not survive into the next one. The runtime's execution limit catches many infinite loops, but programs should still have a deliberate termination condition.

## For

```tg
var total = 0;
for number in [2, 4, 6] {
    total = total + number;
}
print(total);
for character in "go" {
    print(character);
}
for key in {"first": 1, "second": 2} {
    print(key);
}
```

Output:

```text
12
g
o
first
second
```

Lists produce elements, strings produce Unicode code points, and dictionaries produce keys in insertion order. Numbers are not iterable. Each iteration binds its loop variable in a fresh scope; it does not overwrite an outer variable with the same name. See [iteration snapshots](collections.md) for mutation behavior.

## Early Exit with Return

`break` exits the nearest loop or switch. `continue` skips to the next iteration of the nearest loop, including when written inside a switch. Neither can target a loop outside the current function. Use `return` to exit the entire function from nested loops.

```tg
function first_even(values) {
    for value in values {
        if value % 2 == 0 {
            return value;
        }
    }
    return null;
}
print(first_even([1, 3, 8, 10]), first_even([1, 3]));
```

Output:

```text
8 null
```

`return` exits the function, not just the innermost block. It is a syntax error outside a function or method.

## C-Style Loops and Updates

```tg
var total = 0;
cfor (var index = 0; index < 5; ++index) {
    if index == 2 { continue; }
    total = total + index;
} else { print(total); }
var count = 1;
print(count++, ++count, count--, --count);
```

```text
8
1 3 3 1
```

`cfor (initializer; condition; update)` executes the initializer once, tests before each iteration, and runs the update after the body, including after `continue`. Each clause may be empty; an omitted condition means true. `cfor (;;) { break; }` is valid. The initializer may declare a variable; the update may assign or evaluate an expression but cannot declare one. Header variables live in a loop-local scope visible to its body and `else`, not after the loop. Each iteration has a fresh child scope.

Prefix `++value` and `--value` return the updated number. Postfix `value++` and `value--` return the old number. Targets may be mutable variables, fields, or list/dictionary entries; receivers and index expressions are evaluated once. Constants and nonnumeric targets fail. Unlike C's unspecified expression-order cases, Tiger evaluates operands and arguments left to right.

## Range and Loop Else

```tg
for number in range(3, 0, -1) { print(number); }
for number in range(0) { print("unreachable"); }
else { print("empty range completed"); }
```

```text
3
2
1
empty range completed
```

`range` returns a list with an exclusive stop; see [Built-ins](builtins.md). `while`, `for`, and `cfor` accept an optional `else`. It runs on normal completion, including zero iterations, but not after a `break` that exits that loop, a `return`, or an escaping exception. `continue` does not suppress it. A `break` consumed by an inner switch or loop does not suppress an outer loop's `else`. A `for` iteration variable is not visible in its `else` block.

## Switch

```tg
switch 2 {
    case 1: print("one"); break;
    case 2: print("two");
    case 3: print("fallthrough"); break;
    default: print("other");
}
```

```text
two
fallthrough
```

The selector is evaluated once. Case expressions are checked in order with Tiger equality until a match; `default` is used only if no case matches. Execution falls through later cases until `break`, return, or an exception. There may be at most one `default`. Cases share one switch-local scope. `continue` inside a switch requires an enclosing loop and continues that loop.

## Try It

Use a `while` loop to sum the numbers from 1 through 10. Initialize both the counter and total before the loop. The result should be `55`.