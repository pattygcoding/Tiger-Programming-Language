# 9. Classes and Inheritance

[Previous: Functions and Closures](functions-and-closures.md) | [Guide index](README.md) | [Next: Built-ins](builtins.md)

## Create an Instance

```tg
class Counter {
    function init(this, start) {
        this.value = start;
    }
    function add(this, amount) {
        this.value = this.value + amount;
        return this.value;
    }
}
const first = Counter(0);
const second = Counter(10);
print(first.add(3), second.add(2), first.value);
first.label = "primary";
print(first.label);
```

Output:

```text
3 12 3
primary
```

`class Name { ... }` declares a class in the current lexical scope. Class bodies contain methods and `var`/`const` field declarations. Every method explicitly declares `this` as its first parameter; calls through an instance supply it automatically. `this` is a reserved receiver name and cannot be rebound inside a method.

Calling a class creates a fresh instance, evaluates declared field initializers from ancestors to descendants in source order, then invokes `init`. Initializers run once per instance, may refer to `this`, and have their declaring class's access context. Dynamic public fields can still be created by assignment through an instance reference. `const` on an instance variable prevents rebinding that variable, not updating its mutable fields.

## Access Modifiers and Declared Fields

```tg
class Counter {
    var label = "counter";
    private var value = 0;
    protected var calls = 0;
    public const category = "numeric";
    function next(this) { this.calls++; return ++this.value; }
}
class TrackedCounter(Counter) {
    function count(this) { return this.calls; }
}
const counter = TrackedCounter();
print(counter.label, counter.next(), counter.count(), counter.category);
try { print(counter.value); } catch (error) { print("private" in error); }
```

```text
counter 1 1 numeric
true
```

Methods and fields are **public by default**; `public` is optional. `private` permits access only from the declaring class's lexical method/initializer context. `protected` also permits descendant classes. Checks apply to reads, writes, increments, method extraction, and `super` lookup. Nested functions retain their lexical access context; unrelated global helpers do not gain access from their callers. An authorized method may intentionally return a private bound method as a callable capability.

Declared `const` fields require initializers and cannot be rebound, even in `init`; referenced collections remain mutable. Inherited fields cannot be redeclared, and field/method declaration collisions are rejected. Methods may override inherited methods. Restricted constructors obey the same access rules as methods.

## Inherit and Override

```tg
class Label {
    function init(this, name) {
        this.name = name;
    }
    function text(this) {
        return this.name;
    }
    function describe(this) {
        return "[" + this.text() + "]";
    }
}
class Badge(Label) {
    function init(this, name, rank) {
        super.init(name);
        this.rank = rank;
    }
    function text(this) {
        return super.text() + " #" + str(this.rank);
    }
}
const badge = Badge("Tiger", 7);
print(badge.describe());
```

Output:

```text
[Tiger #7]
```

`class Badge(Label)` declares single inheritance. The parent identifier must already resolve to a class. The inherited `describe` calls `this.text()`, which dispatches to the override on the actual instance. `super.init` explicitly initializes the parent portion of that same instance.

If a subclass has no `init`, it inherits the nearest ancestor's constructor. If no constructor exists anywhere in the hierarchy, the class accepts zero arguments. Constructor arity excludes `this`. Construction always returns the new instance, even when `init` explicitly returns another value; a direct call to `instance.init(...)` is an ordinary method call and returns that method's result.

## Super Is Lexical

```tg
class Base {
    function text(this) { return "base"; }
}
class Middle(Base) {
    function text(this) { return super.text() + "/middle"; }
}
class Leaf(Middle) {
}
print(Leaf().text());
```

Output:

```text
base/middle
```

The inherited method was defined in `Middle`, so its `super` starts lookup at `Base`, even when the receiver is a `Leaf`. Lookup continues up the single-parent chain and retains the same receiver. This avoids accidentally calling `Middle.text` again. `super` only accesses parent methods, not instance fields, and it cannot be used as an assignment target.

Nested functions inside methods can capture both `this` and `super`. A locally declared class gets its own method receiver and parent context; it does not reuse the enclosing method's `super`.

## Save a Bound Method

```tg
class Accumulator {
    function init(this) { this.total = 0; }
    function add(this, amount) {
        this.total = this.total + amount;
        return this.total;
    }
}
const counter = Accumulator();
const add = counter.add;
print(add(2), add(5), counter.total);
print(add == counter.add);
```

Output:

```text
2 7 7
true
```

Reading a method through an instance produces a bound method. It retains its receiver when passed as a callback, returned, or stored in a collection. A plain function assigned to a field remains a plain function: calling that field does not inject `this`. A bound method stored on a different object retains its original receiver.

## Member and Identity Rules

- Reading a member checks instance fields first, then methods on its class and ancestors. A field can shadow a method. Calling a non-callable field is a runtime error.
- Unknown fields raise a positioned runtime error. Public fields can be accessed from outside the class; restricted fields require an authorized class context.
- Classes and instances compare by identity. Bound methods compare by both receiver and method identity. All are truthy.
- `print` and `str` use stable descriptions such as `<class Counter>`, `<Counter instance>`, and `<bound method Counter.add>`. There are no user-defined conversion hooks.
- Class names are mutable bindings. A class's recorded parent reference does not change merely because the parent's name is later rebound.
- Duplicate members, duplicate parameters, missing first `this`, and direct self-inheritance are syntax errors. Missing parents, non-class parents, invalid arity, and invalid `super` access are runtime errors.
- Static methods, static fields, decorators, getters/setters, multiple parents, and operator overloading are not implemented. Declared fields belong to instances. Class-level member access such as `Counter.add` is not supported.

## Try It

Derive `DoubleCounter` from the first example's `Counter`. Override `add` to call `super.add(amount * 2)` without defining a new constructor. `DoubleCounter(10).add(3)` should return `16`.

See [the full OOP example](../examples/oop.tg) and [OOP benchmarks](../benchmarks/README.md) for larger programs.