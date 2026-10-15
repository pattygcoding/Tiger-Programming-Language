# 6. Lists, Dictionaries, Sets, and Tuples

[Previous: Strings](strings.md) | [Guide index](README.md) | [Next: Control Flow](control-flow.md)

## Lists

A list is an ordered sequence that can mix value types. Indexing is zero-based; negative indices count backward from the end.

```tg
var items = [1, "apple", true];
items[0] = 9;
items = items + ["last"];
print(items);
print(items[-1], len(items), items.size(), items.length(), "apple" in items);
```

Output:

```text
[9, "apple", true, "last"]
last 4 4 4 true
```

Indexed assignment replaces an existing element. It cannot grow a list. Concatenation with `+` creates a new outer list; use `items += [value]` to add one element. `size()` and `length()` are aliases for `len(items)`, and a list of strings joins with `items.join(separator)`. There are no list comprehensions. Out-of-range and fractional indices are errors.

### Slicing

A slice reads a range of a list without changing it: `items[start:stop:step]`. Every part may be omitted, negative bounds count from the end, and out-of-range bounds clamp instead of failing.

```tg
const items = [10, 20, 30, 40, 50];
print(items[1:3]);
print(items[:2], items[2:], items[:]);
print(items[::2], items[::-1]);
print(items[-2:], items[1:99]);
```

Output:

```text
[20, 30]
[10, 20] [30, 40, 50] [10, 20, 30, 40, 50]
[10, 30, 50] [50, 40, 30, 20, 10]
[40, 50] [20, 30, 40, 50]
```

The result is a new list, so changing it does not affect the original; `items[:]` is a shallow copy. A step of zero, a non-integer bound, and slicing a dictionary are errors. Strings slice the same way; see [Strings](strings.md#slicing).

### List Methods

The [list methods](#list-methods) below change a list in place and return `null`, except `pop`, which returns the removed element, and `index`, which returns a position:

```tg
var queue = [1, 2, 3];
queue.insert(0, 0);
queue.extend([4, 5]);
print(queue, queue.index(4), queue.index(9));
print(queue.pop(), queue);
queue.remove(0);
queue.reverse();
print(queue);
queue.clear();
print(queue, len(queue));
```

```text
[0, 1, 2, 3, 4, 5] 4 -1
5 [0, 1, 2, 3, 4]
[4, 3, 2, 1]
[] 0
```

| Method | Result |
| --- | --- |
| `items.pop()` | Removes and returns the last element; an empty list is an error |
| `items.pop(index)` | Removes and returns the element at a possibly negative index; out of range is an error |
| `items.remove(value)` | Removes the first element equal to `value`; a missing value is an error |
| `items.insert(index, value)` | Inserts `value` before `index`, clamping out-of-range indexes like Python |
| `items.index(value)` | First index equal to `value`, or `-1` |
| `items.extend(other)` | Appends every element of another list |
| `items.reverse()` | Reverses the list in place |
| `items.clear()` | Removes every element |
| `items.join(separator)` | Joins a list of strings into one string |

Comparisons are structural, so `remove([1])` removes a nested list equal to `[1]`, and `index` uses the same `==` rules as `in`. `index` returns `-1` when the value is absent, matching string `find`, rather than raising.

### Sorting

`items.sort()` sorts a list in place and returns `null`, like Python's `list.sort()`. It accepts only the keyword arguments `key` (a function called once per element, or `null`) and `reverse` (default `false`). The sort is stable, including when reversed. Elements or keys must all be numbers or all be strings; mixed types are an error.

```tg
const scores = [72, 95, 88];
scores.sort();
print(scores);
const names = ["Mia", "Al", "Bo", "Kate"];
function size(name) { return len(name); }
names.sort(key=size, reverse=true);
print(names);
```

Output:

```text
[72, 88, 95]
["Kate", "Mia", "Al", "Bo"]
```

## Dictionaries

```tg
const config = {"port": 8080, "host": "localhost"};
config["port"] = 9090;
config["debug"] = false;
print(config["host"], len(config));
for key in config {
    print(key, config[key]);
}
print("debug" in config, "missing" in config);
```

Output:

```text
localhost 3
port 9090
host localhost
debug false
true false
```

Keys may be strings, numbers, or booleans. These are distinct types: `1`, `"1"`, and `true` are different keys. Lists, dictionaries, instances, and `null` cannot be keys. Numeric `0` and `-0` address the same key.

Dictionaries retain insertion order. Repeated keys replace the value without moving the key; indexed assignment adds a missing key. Reading a missing key raises an error, so use `in` first when absence is possible. Dictionaries use bracket access, not instance-style dot access. `len(config)`, `config.size()`, and `config.length()` all report the entry count, the same three spellings lists and strings accept.

### Dictionary Methods

```tg
var scores = {"ada": 36, "alan": 41, "grace": 45};
print(scores.keys(), scores.values());
scores["edsger"] = 72;
print(scores.items());
print(scores.get("ada"), scores.get("nobody"), scores.get("nobody", 0));
print(scores.delete("alan"), scores.delete("nobody"), scores);
print(scores.pop("ada"), scores);
print(scores.size(), scores.length(), len(scores));
scores.clear();
print(scores, len(scores));
```

```text
["ada", "alan", "grace"] [36, 41, 45]
[["ada", 36], ["alan", 41], ["grace", 45], ["edsger", 72]]
36 null 0
true false {"ada": 36, "grace": 45, "edsger": 72}
36 {"grace": 45, "edsger": 72}
2 2 2
{} 0
```

| Method | Result |
| --- | --- |
| `d.keys()` | New list of keys in insertion order |
| `d.values()` | New list of values in insertion order |
| `d.items()` | New list of `[key, value]` lists in insertion order |
| `d.get(key)`, `d.get(key, default)` | The value, or `default` (else `null`) when the key is absent |
| `d.pop(key)`, `d.pop(key, default)` | Removes and returns the value; without a default, a missing key is an error |
| `d.delete(key)` | Removes the key and returns whether it was present |
| `d.clear()` | Removes every entry |
| `d.size()`, `d.length()` | The entry count, the same as `len(d)` |

`keys`, `values`, and `items` return fresh lists, so adding to them does not change the dictionary. Deleting a key preserves the insertion order of the remaining entries, and a later assignment to a deleted key appends it at the end.

## Sets

A set is an insertion-ordered collection of unique values: `{1, 2, 3}`. Membership is a hash lookup, so `in` and `has` stay fast as the set grows, and a value that is already there keeps its place. Only the scalars a dictionary accepts as keys — strings, numbers, and booleans — can be elements. `{}` remains an empty *dictionary*, so an empty set is written `set()`.

```tg
const digits = {3, 1, 4, 1, 5};
print(digits, len(digits), digits.size(), digits.length());
print(2 in digits, 4 in digits, digits.has(5), type(digits));
print(set(), set([1, 1, 2]), set("abc"), set({"a": 1}));
```

Output:

```text
{3, 1, 4, 5} 4 4 4
false true true set
set() {1, 2} {"a", "b", "c"} {"a"}
```

`add(value)` inserts, `remove(value)` removes and reports whether it was there, `has(value)` tests membership, and `clear()` empties the set. `union`, `intersection`, and `difference` take any iterable — a list, a tuple, another set, a string's characters, or a dictionary's keys — and return a new set, leaving the receiver alone:

```tg
var vowels = set("aeiou");
vowels.add("y");
vowels.add("a");
print(vowels, len(vowels));
print(vowels.remove("y"), vowels.remove("y"));
print(vowels);
const odds = {1, 3, 5};
print(odds.union([2, 1]), odds.intersection((3, 9)), odds.difference({3}));
print(odds, odds.union({2}));
```

Output:

```text
{"a", "e", "i", "o", "u", "y"} 6
true false
{"a", "e", "i", "o", "u"}
{1, 3, 5, 2} {3} {1, 5}
{1, 3, 5} {1, 3, 5, 2}
```

Because a `print` argument holding a set is rendered when the line is written, a mutation made by an earlier argument is visible in it: `print(items, items.remove(1))` shows the set without `1`. Print or copy the value before mutating it in the same call.

Sets iterate in insertion order, and mutating a set does not invalidate the copy a `for` loop walks. The usual shape for de-duplication is a set for the membership test plus a list for the order:

```tg
var unique = [];
const seen = set();
for word in ["red", "blue", "red"] {
    if !seen.has(word) {
        seen.add(word);
        unique += [word];
    }
}
print(unique, len(seen), sorted(list(seen)));
```

Output:

```text
["red", "blue"] 2 ["blue", "red"]
```

A set is unordered, so the sequence built-ins (`sum`, `min`, `sorted`, `map`, `filter`, `zip`, `enumerate`) take a list or a tuple: write `list(my_set)` when a positional view is needed. Set equality ignores insertion order, `{1, 2} == {2, 1}` is `true`, and a set never equals a list of the same elements.

## Tuples

A tuple is a fixed-length group: `(1, 2)`, with `(1,)` for one element and `()` for none. Parentheses on their own still group an expression, so `(1)` is the number `1` and only a comma makes a tuple. A tuple is immutable: it supports indexing, slicing, `+` concatenation, membership, iteration, and destructuring, but it has no `append`, no `sort`, and no assignment to an index.

```tg
const pair = (10, 20);
print(pair, len(pair), pair.size(), pair.length(), type((1, 2)), type((1)));
print(pair[0], pair[-1], (1, 2, 3, 4)[1:3], (1, 2, 3, 4)[::-1], (1, 2) + (3,));
print((1, 2) == (2, 1), (1, 2) == [1, 2], 2 in (1, 2), bool(()), bool((0,)));
const [first, second] = pair;
print(first, second);
```

Output:

```text
(10, 20) 2 2 2 tuple number
10 20 (2, 3) (4, 3, 2, 1) (1, 2, 3)
false false true false true
10 20
```

Equality is structural and a tuple never equals a list of the same elements: `(1, 2) == [1, 2]` is `false`. The sequence built-ins accept a tuple wherever they accept a list — `sum((1, 2, 3))`, `sorted((3, 1))`, `zip((1, 2), ("a", "b"))`, `min((3, 1))`, and `enumerate(("a", "b"))` all work — and `list(tuple)` makes a mutable copy. `enumerate` and `zip` still return lists of lists, and `dict.items()` still returns `[key, value]` lists, so existing programs keep the shape they had; write `tuple(pair)` where a fixed-length group reads better.

## Comprehensions

A comprehension builds a list or a dictionary from an iterated expression in one line. `[x * x for x in items]` is the list form and `{word: len(word) for word in words}` is the dictionary form. Every clause is `for name in iterable`, and any number of `if condition` filters may follow it:

```tg
print([x * x for x in range(1, 5)]);
print([x for x in [3, 1, 2] if x % 2 == 1]);
print([x for x in [1, 2, 3, 4] if x > 1 if x < 4]);
print([word.upper() for word in ["charlie", "bravo"] if len(word) > 5]);
print({word: len(word) for word in ["a", "bb", "ccc"]});
print({x: x * x for x in range(1, 4) if x != 2});
```

Output:

```text
[1, 4, 9, 16]
[3, 1]
[2, 3]
["CHARLIE"]
{"a": 1, "bb": 2, "ccc": 3}
{1: 1, 3: 9}
```

Any value a `for` loop can walk may be iterated: a list, a tuple, a set, a string's characters, or a dictionary's keys. Clauses nest left to right, so the second runs once for every value of the first, and a filter belongs to the clause it follows:

```tg
print([b for a in [1, 2] for b in [a, a * 10]]);
print([[row, sum(row)] for row in [[1, 2], [3, 4]]]);
print([value for value in {3, 1}], [key for key in {"a": 1}], [char for char in "ab"]);
```

```text
[1, 10, 2, 20]
[[[1, 2], 3], [[3, 4], 7]]
[3, 1] ["a"] ["a", "b"]
```

The clause variables live in a scope of their own: a comprehension reads the names around it, shadows them in its element and filters, and its variables never leak into the enclosing scope. The element and the filters are evaluated in clause order, once per iteration, and a later clause can read the variables of the clauses before it:

```tg
var shadowed = 5;
print([shadowed for shadowed in [1, 2]], shadowed);
print([x + y for x in [1] for y in [x, x * 10]]);
```

```text
[1, 2] 5
[2, 11]
```

A repeated key in a dictionary comprehension keeps its first position and takes the last value, exactly as a dictionary literal does. The runtime is eager, so a comprehension builds its whole result immediately: there are no generator expressions, and a set comprehension is a syntax error — write `set([x for x in items])`. Inside a clause header `if` starts a filter, so a conditional expression there needs parentheses: `[x for x in (a if flag else b)]`.

**Expected error:**

```tg
[x for x in 1];
```

```error
number is not iterable
```

## Sharing and Shallow Copies

```tg
const original = [[1], [2]];
const alias = original;
const copy = original + [];
copy[0][0] = 9;
copy[1] = [8];
print(original, alias, copy);
print(original == alias, original == copy);
```

Output:

```text
[[9], [2]] [[9], [2]] [[9], [8]]
true false
```

`alias` shares the entire list. `copy` has a distinct outer list, but its initial elements reference the same nested lists. Replacing `copy[1]` does not replace `original[1]`; modifying the shared `copy[0][0]` changes what both outer lists see.

Equality compares list elements and dictionary key/value contents recursively, rather than checking collection identity. Dictionary order does not affect equality. Cyclic collections are supported by equality and formatting; recursive references print as `[...]` or `{...}` markers.

## Iteration Snapshots

```tg
const values = [1, 2];
var seen = [];
for value in values {
    seen = seen + [value];
    values[1] = 9;
}
print(seen, values);

const mapping = {"first": 1};
var keys = [];
for key in mapping {
    keys = keys + [key];
    mapping["second"] = 2;
}
print(keys, mapping);
```

Output:

```text
[1, 2] [1, 9]
["first"] {"first": 1, "second": 2}
```

`for` captures a snapshot of list or tuple elements, set members, or dictionary keys before iterating. Replacing a list slot or adding a key does not change that snapshot. The snapshot is shallow: objects referenced by its elements can still be mutated.

## Try It

Count how often each word appears in `["red", "blue", "red"]`. Start with an empty dictionary and check membership before incrementing a count. The result should be `{"red": 2, "blue": 1}`.