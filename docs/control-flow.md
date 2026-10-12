# 7. Control Flow

[Previous: Lists and Dictionaries](collections.md) | [Guide index](README.md) | [Next: Functions and Closures](functions-and-closures.md)

## If, Elif, Else If, and Else

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

Conditions use [truthiness](types-and-operators.md), not just literal booleans. Branches are tested in order, and only the first matching branch executes. The header may be bare or parenthesized, so `if score >= 90` is the same as `if (score >= 90)`. A chain continues with `elif` or the C-style `else if`, and `else` is optional. Blocks always require braces; a body is never a bare statement.

```tg
function bucket(value) {
    if (value < 0) {
        return "negative";
    } else if (value == 0) {
        return "zero";
    } elif (value < 10) {
        return "small";
    } else {
        return "big";
    }
}
print(bucket(-1), bucket(0), bucket(5), bucket(50));
```

Output:

```text
negative zero small big
```

## Symbolic and Keyword Operators

`&&` and `and` are the same operator, as are `||` and `or`, and `!` and `not`. Each pair has identical precedence and may be used anywhere an expression may appear, so a condition, a clause header, or a plain expression can use either spelling.

```tg
print(true && false, true || false, !true, !false);
print(1 && 2 || 3, 0 || 2 && 3);
if (2 > 1 && !(3 > 4)) {
    print("truthy", not false and not not true);
}
```

Output:

```text
false true false true
2 3
truthy true
```

`&&` binds tighter than `||`, and both bind looser than comparisons, as in C. `!` is exactly `not`, so it also binds looser than comparison: `!a == b` means `!(a == b)`, and `(!a) == b` is the way to compare a negated value. Both operators short-circuit, so the right operand runs only when it can change the result.

## Conditional Expressions

A conditional expression chooses between two values: `value if condition else fallback`. Only the branch that is selected is evaluated.

```tg
const count = 7;
print("even" if count % 2 == 0 else "odd");
function grade(score) {
    return "A" if score >= 90 else "B" if score >= 80 else "C";
}
print(grade(95), grade(85), grade(70));
```

Output:

```text
odd
A B C
```

It binds looser than every operator, so `1 + 2 if flag else 3 * 4` means `(1 + 2) if flag else (3 * 4)`, and a chain such as `"a" if one else "b" if two else "c"` groups to the right. Use parentheses when a larger expression should be the condition. Both branches are required.

**Expected error:**

```tg
print(1 if true);
```

```error
expected "else"
```

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

## Do While

`do` runs its body first and checks the condition afterward, so the body always runs at least once.

```tg
var remaining = 3;
do {
    print(remaining);
    remaining = remaining - 1;
} while (remaining > 0);
var value = 10;
do {
    print("once");
} while (value < 0);
```

Output:

```text
3
2
1
once
```

The parentheses around the condition are optional. Unlike `while` and `for`, a `do...while` loop has no `else` clause; a `do...while` loop does accept a [label](#early-exit-and-labels).

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

Lists produce elements, strings produce Unicode code points, and dictionaries produce keys in insertion order. Numbers are not iterable. Each iteration binds its loop variable in a fresh scope; it does not overwrite an outer variable with the same name. See [iteration snapshots](collections.md) for mutation behavior. For the clause header `for (initializer; condition; update)`, see [Three-Clause For and Updates](#three-clause-for-and-updates).

## Early Exit and Labels

`break` exits the nearest loop or switch. `continue` skips to the next iteration of the nearest loop, including when written inside a switch. Neither can target a loop outside the current function. Use `return` to exit the entire function from nested loops.

Name a loop to leave it from an inner loop: write `name:` before the loop and `break name;` or `continue name;` inside it.

```tg
outer: for row in range(3) {
    for column in range(3) {
        if column == 1 {
            continue outer;
        }
        if row == 2 {
            break outer;
        }
        print(row, column);
    }
}
print("done");
```

Output:

```text
0 0
1 0
done
```

A label belongs to the loop it precedes and is visible only inside that loop's body, so an inner loop may reuse the same name and the innermost match wins. `while`, `for` (both header styles), and `do...while` all accept labels, and a labeled `break` skips the loop's `else` block just like an unlabeled one.

**Expected error:**

```tg
break missing;
```

```error
unknown label "missing"
```


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

## Three-Clause For and Updates

```tg
var total = 0;
for (var index = 0; index < 5; ++index) {
    if index == 2 { continue; }
    total = total + index;
} else { print(total); }
var count = 1;
print(count++, ++count, count--, --count);
var cursor = 0;
for cursor = 0; cursor < 3; cursor++ { total += cursor; }
for var step = 0; step < 4 && step * step < 10; step++ {
    print(step, "squared is", step * step);
}
print("total", total);
```

Output:

```text
8
1 3 3 1
0 squared is 0
1 squared is 1
2 squared is 4
3 squared is 9
total 11
```

`for (initializer; condition; update)` executes the initializer once, tests before each iteration, and runs the update after the body, including after `continue`. The parentheses are optional, so `for var index = 0; index < 3; index++` is the same header. Each clause may be empty; an omitted condition means true, so `for (;;) { break; }` is valid. The initializer may declare a variable with `var` or `const`, or assign one that already exists. The update may assign or evaluate an expression but cannot declare a variable. Header variables live in a loop-local scope visible to its body and `else`, not after the loop. Each iteration has a fresh child scope. A clause header that names a type instead of `var` is an error, because Tiger has no type annotations.

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

`range` returns a list with an exclusive stop; see [Built-ins](builtins.md). Every loop — `while`, both `for` header styles, and `do...while` — accepts an optional `else`. It runs on normal completion, including zero iterations, but not after a `break` that exits that loop, a `return`, or an escaping exception. `continue` does not suppress it. A `break` consumed by an inner switch or loop does not suppress an outer loop's `else`. A `for` iteration variable is not visible in its `else` block.

## Switch and Match

```tg
switch 2 {
    case 1: print("one"); break;
    case 2: print("two");
    case 3: print("fallthrough"); break;
    default: print("other");
}
match 9 {
    case 1: print("one");
    case _: print("other");
}
```

```text
two
fallthrough
other
```

`switch` and `match` are two spellings of one statement, and both accept an optional parenthesized selector, so `switch (value)` and `match value` are the same header. `case _:` is the wildcard spelling of `default:`; both mean "no case matched", and at most one of either may appear. The selector is evaluated once. Case expressions are checked in order with Tiger equality until a match; the default is used only if no case matches. Execution falls through later cases until `break`, return, or an exception, exactly as in C, whichever keyword was used.

Cases share one switch-local scope. `continue` inside a switch requires an enclosing loop and continues that loop.

## Try It

Use a `while` loop to sum the numbers from 1 through 10. Initialize both the counter and total before the loop. The result should be `55`.