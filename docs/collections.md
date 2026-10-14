# 6. Lists and Dictionaries

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

`for` captures a snapshot of list elements or dictionary keys before iterating. Replacing a list slot or adding a key does not change that snapshot. The snapshot is shallow: objects referenced by its elements can still be mutated.

## Try It

Count how often each word appears in `["red", "blue", "red"]`. Start with an empty dictionary and check membership before incrementing a count. The result should be `{"red": 2, "blue": 1}`.