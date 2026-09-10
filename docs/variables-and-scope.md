# 3. Variables and Scope

[Previous: Syntax](syntax.md) | [Guide index](README.md) | [Next: Types and Operators](types-and-operators.md)

## Assignment

No type declaration is needed. A mutable binding can hold values of different types over time.

```tg
value = 10;
print(value);
value = "ten";
print(value);
```

Output:

```text
10
ten
```

Assignment searches outward through lexical scopes. It updates the nearest existing binding; if none exists, it creates a new binding in the current scope. This applies inside functions as well as ordinary blocks.

## Constants Protect Bindings

```tg
const settings = {"port": 8080};
settings["port"] = 9090;
const items = [1, 2];
items[0] = 9;
print(settings, items);
```

Output:

```text
{"port": 9090} [9, 2]
```

`const` prevents reassignment of a name, not changes to the list, dictionary, or instance referenced by that name. It does not recursively freeze data.

**Expected error:**

```tg
const limit = 3;
limit = 4;
```

```error
cannot reassign const "limit"
```

`const` declares in the current scope. Declaring the same name twice in one scope is an error, even if the first binding was mutable.

## Block Scope

```tg
total = 0;
for amount in [2, 3] {
    const doubled = amount * 2;
    total = total + doubled;
}
if true {
    const total = 99;
    print("inner:", total);
}
print("outer:", total);
```

Output:

```text
inner: 99
outer: 10
```

Every branch and loop iteration gets a new scope. The loop variable is local to that iteration. New names created there do not escape the block. An explicit inner `const` declaration can shadow an outer name; an ordinary assignment instead updates the existing binding.

## Functions Can Update Outer State

```tg
count = 1;
def increment() {
    count = count + 1;
}
def local(count) {
    count = count + 10;
    return count;
}
increment();
print(count, local(5), count);
```

Output:

```text
2 15 2
```

Parameters always create local mutable bindings, even when an outer name is constant. Function and class declarations also create bindings in their current scope. Nested functions retain their defining scopes; see [Functions and Closures](functions-and-closures.md).

## References and Sharing

Assignment of a list, dictionary, instance, or function shares a reference; it does not clone the value. List concatenation creates a new outer list but shares any nested objects. See [Lists and Dictionaries](collections.md) for a working example.

## Try It

Add `print(amount);` after the loop example. It fails because `amount` belongs to the loop iteration, not the outer scope. To keep a result, initialize a separate variable before the loop and update it inside.