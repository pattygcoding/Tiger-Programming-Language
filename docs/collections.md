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

Indexed assignment replaces an existing element. It cannot grow a list. Concatenation with `+` creates a new outer list; use `items += [value]` to add one element. `size()` and `length()` are aliases for `len(items)`. There are no `append`/`pop` methods, slices, or comprehensions. Out-of-range and fractional indices are errors.

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

Dictionaries retain insertion order. Repeated keys replace the value without moving the key; indexed assignment adds a missing key. Reading a missing key raises an error, so use `in` first when absence is possible. Dictionaries use bracket access, not instance-style dot access. There is no key-deletion operation.

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