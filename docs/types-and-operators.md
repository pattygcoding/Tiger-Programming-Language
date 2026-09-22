# 4. Types and Operators

[Previous: Variables and Scope](variables-and-scope.md) | [Guide index](README.md) | [Next: Strings](strings.md)

## Value Types

| Type | Examples or source |
| --- | --- |
| Number | `12`, `3.5`, `1e2`; unary `-` makes a negative value |
| String | `"hello"`, `'hello'` |
| Boolean | `true`, `false` |
| Null | `null`; also a function's default return value |
| List | `[1, "two", true]` |
| Dictionary | `{"port": 8080}` |
| Function | A `function` declaration, including a closure |
| Class | A `class` declaration |
| Instance | Calling a class |
| Bound method | Accessing a method through an instance |

Numbers are finite IEEE 754 float64 values, not arbitrary-precision integers. Integer arithmetic is exact only within float64's precision range; not every integer beyond 2^53 can be represented. Decimal fractions such as `0.1` are generally approximate. Arithmetic producing infinity or NaN raises an error. There are no user-visible integer, byte, or decimal types.

## Arithmetic

```tg
print(2 + 3 * 4, (2 + 3) * 4);
print(7 / 2, 17 % 5, -17 % 5, 17 % -5);
print(1e2 + 2, -(-3));
print("Tiger" + "!", [1] + [2, 3]);
var total = 10;
total += 5;
total -= 2;
total *= 3;
total %= 8;
print(total);
```

Output:

```text
14 20
3.5 2 3 -3
102 3
Tiger! [1, 2, 3]
7
```

`+`, `-`, `*`, `/`, and `%` accept numbers. Division always produces a number, so `7 / 2` is `3.5`. Modulo follows the divisor's sign. Division and modulo by zero are errors. Unary `+` and `-` require a number.

`+` also concatenates two strings or two lists. Mixed-type arithmetic does not implicitly convert values. Use `str` when constructing text. `//` begins a comment; it is not integer division. Exponentiation is not implemented.

`+=`, `-=`, `*=`, and `%=` read an assignable target, apply the matching arithmetic operator, and store the result. They work with the same operand types as `+`, `-`, `*`, and `%`; for example, `items += [next]` appends by list concatenation. Variables, instance fields, and list/dictionary entries are valid targets. The receiver and index expressions are evaluated once. Prefix and postfix `++`/`--` update numeric assignable targets; see [Control Flow](control-flow.md).

## Comparison and Equality

```tg
print(3 < 4, "apple" < "pear", 5 >= 5);
print([1, {"x": 2}] == [1, {"x": 2}]);
print({"a": 1, "b": 2} == {"b": 2, "a": 1});
print(true == 1, null == null, "1" != 1);
print(2 in [1, 2], "port" in {"port": 80}, "ig" in "Tiger");
```

Output:

```text
true true true
true
true
false true true
true true true
```

Ordering operators `<`, `<=`, `>`, and `>=` work between two numbers or two strings. Strings use lexicographic comparison, not locale-aware collation. Equality does not coerce types. Lists and dictionaries compare structurally with cycle protection; dictionary insertion order does not affect equality. Classes, instances, and functions use identity. Bound methods compare by both receiver and method identity.

`in` checks a list's elements, a dictionary's keys, or a substring. The left operand of string membership must be a string.

## Truthiness and Short-Circuiting

```tg
print(not null, not [], not {}, not "", not 0);
print("" or "fallback", 0 and "unused", [1] and "present");
print(false and missing_name, true or missing_name);
```

Output:

```text
true true true true true
fallback 0 present
false true
```

Falsey values are `false`, `null`, zero, and empty strings, lists, and dictionaries. Everything else is truthy, including all instances. `not` returns a boolean. `and` and `or` return one of their operands and only evaluate the right operand when needed.

## Precedence Reference

Highest precedence first:

| Operators | Behavior |
| --- | --- |
| Postfix `++`, `--` | Update a target and return its old value |
| Calls `()`, indexing `[]`, member access `.` | Chain left to right |
| Unary `+`, `-`, prefix `++`, `--` | Bind before multiplication; prefix updates return the new value |
| `*`, `/`, `%` | Left-associative |
| `+`, `-` | Left-associative |
| `==`, `!=`, `<`, `<=`, `>`, `>=`, `in` | Same precedence, left-associative |
| `not` | Binds less tightly than comparisons |
| `and` | Short-circuit |
| `or` | Short-circuit |

Use parentheses to override precedence. Python-style chained comparisons are not supported: write `lower < value and value < upper`, not `lower < value < upper`. Write `not (item in values)` for negated membership; there is no separate `not in` operator.

## Try It

Replace `""` with `"0"` in the fallback example. The nonempty string is truthy, so the result is `"0"`, not `"fallback"`.