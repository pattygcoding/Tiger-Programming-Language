# 9. Classes and Inheritance

[Previous: Functions and Closures](functions-and-closures.md) | [Guide index](README.md) | [Next: Built-ins](builtins.md)

## Create an Instance

```tg
class Counter {
    def init(self, start) {
        self.value = start;
    }
    def add(self, amount) {
        self.value = self.value + amount;
        return self.value;
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

`class Name { ... }` declares a class in the current lexical scope. Class bodies contain only method definitions. Every method explicitly declares `self` as its first parameter; calls through an instance supply that receiver automatically.

Calling a class creates a fresh instance and invokes `init`. Fields are created by assignment through `self` or another instance reference. Field values are independent unless references are explicitly shared. `const` prevents rebinding `first`, not updating its fields.

## Inherit and Override

```tg
class Label {
    def init(self, name) {
        self.name = name;
    }
    def text(self) {
        return self.name;
    }
    def describe(self) {
        return "[" + self.text() + "]";
    }
}
class Badge(Label) {
    def init(self, name, rank) {
        super.init(name);
        self.rank = rank;
    }
    def text(self) {
        return super.text() + " #" + str(self.rank);
    }
}
const badge = Badge("Tiger", 7);
print(badge.describe());
```

Output:

```text
[Tiger #7]
```

`class Badge(Label)` declares single inheritance. The parent identifier must already resolve to a class. The inherited `describe` calls `self.text()`, which dispatches to the override on the actual instance. `super.init` explicitly initializes the parent portion of that same instance.

If a subclass has no `init`, it inherits the nearest ancestor's constructor. If no constructor exists anywhere in the hierarchy, the class accepts zero arguments. Constructor arity excludes `self`. Construction always returns the new instance, even when `init` explicitly returns another value; a direct call to `instance.init(...)` is an ordinary method call and returns that method's result.

## Super Is Lexical

```tg
class Base {
    def text(self) { return "base"; }
}
class Middle(Base) {
    def text(self) { return super.text() + "/middle"; }
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

Nested functions inside methods can capture both `self` and `super`. A locally declared class gets its own method receiver and parent context; it does not reuse the enclosing method's `super`.

## Save a Bound Method

```tg
class Accumulator {
    def init(self) { self.total = 0; }
    def add(self, amount) {
        self.total = self.total + amount;
        return self.total;
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

Reading a method through an instance produces a bound method. It retains its receiver when passed as a callback, returned, or stored in a collection. A plain function assigned to a field remains a plain function: calling that field does not inject `self`. A bound method stored on a different object retains its original receiver.

## Member and Identity Rules

- Reading a member checks instance fields first, then methods on its class and ancestors. A field can shadow a method. Calling a non-callable field is a runtime error.
- Unknown fields do not return `null`; they raise a positioned runtime error. Fields can be read and assigned from outside the class; there are no access modifiers.
- Classes and instances compare by identity. Bound methods compare by both receiver and method identity. All are truthy.
- `print` and `str` use stable descriptions such as `<class Counter>`, `<Counter instance>`, and `<bound method Counter.add>`. There are no user-defined conversion hooks.
- Class names are mutable bindings. A class's recorded parent reference does not change merely because the parent's name is later rebound.
- Duplicate methods, duplicate parameters, missing first `self`, and direct self-inheritance are syntax errors. Missing parents, non-class parents, invalid arity, and invalid `super` access are runtime errors.
- Static methods, class fields, decorators, properties with getters/setters, multiple parents, and operator overloading are not implemented. Class-level member access such as `Counter.add` is not supported.

## Try It

Derive `DoubleCounter` from the first example's `Counter`. Override `add` to call `super.add(amount * 2)` without defining a new constructor. `DoubleCounter(10).add(3)` should return `16`.

See [the full OOP example](../examples/oop.tg) and [OOP benchmarks](../benchmarks/README.md) for larger programs.