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
remaining = 3;
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
total = 0;
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

There is no `break` or `continue`. Put a search in a function and use `return` to exit from nested loops, or include a completion flag in a `while` condition.

```tg
def first_even(values) {
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

`return` exits the function, not just the innermost block. It is a syntax error outside a function or method. `range`, C-style counting `for` headers, loop `else`, and `switch` are not supported.

## Try It

Use a `while` loop to sum the numbers from 1 through 10. Initialize both the counter and total before the loop. The result should be `55`.