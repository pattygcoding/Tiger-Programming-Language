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
| Tuple | `(1, "two")`; `(1,)` for one element, `()` for none |
| Set | `{1, 2, 3}`; `set()` for the empty set |
| Dictionary | `{"port": 8080}` |
| Function | A `function` declaration, including a closure |
| Class | A `class` declaration |
| Instance | Calling a class |
| Bound method | Accessing a method through an instance |

Numbers are finite IEEE 754 float64 values, not arbitrary-precision integers. Integer arithmetic is exact only within float64's precision range; not every integer beyond 2^53 can be represented. Decimal fractions such as `0.1` are generally approximate. Arithmetic producing infinity or NaN raises an error. There are no separate integer, byte, or decimal types: integers and fractions share the number type, so mixed arithmetic such as `2 + 0.5` needs no promotion and yields `2.5`. `int(value)` truncates to a whole number and `float(value)` converts to a number; see [conversions](builtins.md#intvalue-floatvalue-boolvalue).

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
print(2 ** 10, 2 ** 3 ** 2, -2 ** 2, 2 ** -1);
print(7 div 2, -7 div 2, 7.5 div 2);
```

Output:

```text
14 20
3.5 2 3 -3
102 3
Tiger! [1, 2, 3]
7
1024 512 -4 0.5
3 -4 3
```

`+`, `-`, `*`, `/`, `%`, `**`, and `div` accept numbers. Division always produces a number, so `7 / 2` is `3.5`. `**` is exponentiation and `div` is floor division, so `7 div 2` is `3` and `-7 div 2` is `-4`. Modulo follows the divisor's sign. Division, floor division, and modulo by zero are errors. `**` is right-associative and binds tighter than unary minus, so `2 ** 3 ** 2` is `512` and `-2 ** 2` is `-4`. A power that overflows to infinity or NaN is an error. Unary `+` and `-` require a number.

`+` also concatenates two strings or two lists. Mixing numbers with strings, booleans, or other types does not implicitly convert values; use `str`, `int`, `float`, or the `.ToString()`-style methods. `//` begins a comment; it is not an operator. A bare `*` or `**` before a name unpacks a list or a dictionary in a call, or declares `*args`/`**kwargs` in a parameter list; `**` between two values is exponentiation. There is no compound `**=` or `div=` form.

`+=`, `-=`, `*=`, and `%=` read an assignable target, apply the matching arithmetic operator, and store the result. They work with the same operand types as `+`, `-`, `*`, and `%`; for example, `items += [next]` appends by list concatenation. Variables, instance fields, and list/dictionary entries are valid targets. The receiver and index expressions are evaluated once. Prefix and postfix `++`/`--` update numeric assignable targets; see [Control Flow](control-flow.md).

## Bitwise Operators

```tg
print(6 & 3, 6 | 3, 6 ^ 3, ~5);
print(1 << 4, 256 >> 4, -8 >> 1);
print(1 | 2 & 3, 1 + 2 << 3);
```

Output:

```text
2 7 5 -6
16 16 -4
3 24
```

`&`, `|`, `^`, `~`, `<<`, and `>>` operate on 64-bit two's-complement integers. Each operand is truncated toward zero first, so `6.9 & 3.9` is `2`. `~` is bitwise NOT (`~5` is `-6`), `^` is exclusive or, and `>>` is arithmetic, so `-8 >> 1` is `-4`. A shift count must not be negative, and an operand whose magnitude is too large for 64 bits is an error. Results are converted back to the number type, so a value beyond 2^53 loses precision. `&` binds tighter than `^`, which binds tighter than `|`; shifts bind looser than `+` and `-`, and the bitwise operators bind tighter than comparisons. `^` is not exponentiation; use `**`.

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

Ordering operators `<`, `<=`, `>`, and `>=` work between two numbers or two strings. Strings use lexicographic comparison, not locale-aware collation. Equality does not coerce types. Lists and tuples compare element by element and dictionaries compare by key and value, all with cycle protection; dictionary insertion order does not affect equality, and neither does set order. A tuple never equals a list of the same elements. Classes, instances, and functions use identity. Bound methods compare by both receiver and method identity.

`in` checks a list's or tuple's elements, a set's members, a dictionary's keys, or a substring. The left operand of string membership must be a string; the left operand of set membership must be a scalar the set could hold.

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

Falsey values are `false`, `null`, zero, and empty strings, lists, tuples, sets, and dictionaries. Everything else is truthy, including all instances. `bool(value)` and `value.ToBool()` return this truth value. `not` returns a boolean. `and` and `or` return one of their operands and only evaluate the right operand when needed.

## Precedence Reference

Highest precedence first:

| Operators | Behavior |
| --- | --- |
| Postfix `++`, `--` | Update a target and return its old value |
| Calls `()`, indexing `[]`, member access `.` | Chain left to right |
| `**` | Right-associative exponentiation |
| Unary `+`, `-`, `~`, prefix `++`, `--` | Bind before multiplication; prefix updates return the new value |
| `*`, `/`, `%`, `div` | Left-associative |
| `+`, `-` | Left-associative |
| `<<`, `>>` | Left-associative |
| `&`, `^`, `\|` | Each its own level, left-associative; `&` binds tighter than `^`, which binds tighter than `\|` |
| `==`, `!=`, `<`, `<=`, `>`, `>=`, `in` | Same precedence, left-associative |
| `not` | Binds less tightly than comparisons |
| `and` | Short-circuit |
| `or` | Short-circuit |

Use parentheses to override precedence. Python-style chained comparisons are not supported: write `lower < value and value < upper`, not `lower < value < upper`. Write `not (item in values)` for negated membership; there is no separate `not in` operator.

## Try It

Replace `""` with `"0"` in the fallback example. The nonempty string is truthy, so the result is `"0"`, not `"fallback"`.