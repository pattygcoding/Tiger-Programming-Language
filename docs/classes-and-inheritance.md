# 9. Classes and Inheritance

[Previous: Functions and Closures](functions-and-closures.md) | [Guide index](README.md) | [Next: Files](file-io.md)

## Create an Instance

```tg
class Counter {
    var value;
    var label = "counter";
    Counter(start) {
        this.value = start;
    }
    function add(amount) {
        this.value = this.value + amount;
        return this.value;
    }
}
const first = Counter(0);
const second = Counter(10);
print(first.add(3), second.add(2), first.value);
first.label = "primary";
print(first.label, second.label);
```

Output:

```text
3 12 3
primary counter
```

`class Name { ... }` declares a class in the current lexical scope. Class bodies contain an optional constructor, methods, and `var`/`const` field declarations. A constructor is written like a Java constructor: the class name followed by a parameter list and body, with no `function` keyword, as in `Counter(start) { ... }`. A class declares at most one constructor. Inside constructors and methods, `this` is available automatically and must not be listed as a parameter; the parameter list contains only the caller's arguments, optionally followed by `*rest` and `**options` (see [Functions and Closures](functions-and-closures.md)). `this` is a reserved receiver name and cannot be rebound inside a method.

Calling a class creates a fresh instance, evaluates declared field initializers from ancestors to descendants in source order, then runs the constructor. Initializers run once per instance, may refer to `this`, and have their declaring class's access context. `const` on an instance variable prevents rebinding that variable, not updating its mutable fields.

Like Java, every instance field must be declared in the class (or an ancestor) before it is assigned. `var name;` declares a field that starts as `null`; `var name = value;` gives it an initial value. Assigning an undeclared field, whether in a constructor, a method, or from outside the class, is a runtime error.

**Expected error:**

```tg
class Point {
    var x;
    Point(x, y) {
        this.x = x;
        this.y = y;
    }
}
Point(1, 2);
```

```error
Point has no field "y"; declare it in the class with var or const
```

## Access Modifiers and Declared Fields

```tg
class Counter {
    var label = "counter";
    private var value = 0;
    protected var calls = 0;
    public const category = "numeric";
    function next() { this.calls++; return ++this.value; }
}
class TrackedCounter extends Counter {
    function count() { return this.calls; }
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

Declared `const` fields require initializers and cannot be rebound, even in the constructor; referenced collections remain mutable. Inherited fields cannot be redeclared, and field/method declaration collisions are rejected. Methods may override inherited methods. Restricted constructors obey the same access rules as methods: a `private Name() { ... }` constructor can only be called from inside the class.

## Inherit and Override

```tg
class Label {
    var name;
    Label(name) {
        this.name = name;
    }
    function text() {
        return this.name;
    }
    function describe() {
        return "[" + this.text() + "]";
    }
}
class Badge extends Label {
    var rank;
    Badge(name, rank) {
        super(name);
        this.rank = rank;
    }
    function text() {
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

`class Badge extends Label` declares single inheritance. `extends` is a reserved keyword and accepts exactly one parent name; the older parenthesized form `class Badge(Label)` is a syntax error.

**Expected error:**

```tg
class Base {}
class Child(Base) {}
```

```error
use 'class Child extends Parent' to declare inheritance
```

The parent identifier must already resolve to a class. The inherited `describe` calls `this.text()`, which dispatches to the override on the actual instance. `super(name)` runs the parent constructor on that same instance.

`super(...)` may only be called inside a constructor, and it is not implicit: a subclass constructor that never calls `super(...)` skips the parent constructor. If a subclass declares no constructor, it inherits the nearest ancestor's constructor. If no constructor exists anywhere in the hierarchy, the class accepts zero arguments. Construction always returns the new instance, even when the constructor explicitly returns another value. Constructors are not methods: `instance.Badge` is not a member, and `function Badge() { ... }` inside `class Badge` is a syntax error. `init` has no special meaning; a method named `init` is an ordinary method.

**Expected error:**

```tg
class Base {
    function helper() { super(); }
}
Base().helper();
```

```error
super(...) can only be called inside a constructor
```

## Super Is Lexical

```tg
class Base {
    function text() { return "base"; }
}
class Middle extends Base {
    function text() { return super.text() + "/middle"; }
}
class Leaf extends Middle {
}
print(Leaf().text());
```

Output:

```text
base/middle
```

The inherited method was defined in `Middle`, so its `super` starts lookup at `Base`, even when the receiver is a `Leaf`. Lookup continues up the single-parent chain and retains the same receiver. This avoids accidentally calling `Middle.text` again. `super.name` only accesses parent methods, not instance fields, and it cannot be used as an assignment target.

Nested functions inside methods can capture both `this` and `super`. A locally declared class gets its own method receiver and parent context; it does not reuse the enclosing method's `super`.

## Save a Bound Method

```tg
class Accumulator {
    var total = 0;
    function add(amount) {
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

Reading a method through an instance produces a bound method. It retains its receiver when passed as a callback, returned, or stored in a collection. A plain function stored in a declared field remains a plain function: calling that field does not inject `this`. A bound method stored on a different object retains its original receiver.

## Member and Identity Rules

- Reading a member checks instance fields first, then methods on its class and ancestors. Fields and methods cannot share a name. Calling a non-callable field is a runtime error.
- Reading or assigning an undeclared field raises a positioned runtime error. Public fields can be accessed from outside the class; restricted fields require an authorized class context.
- Classes and instances compare by identity. Bound methods compare by both receiver and method identity. All are truthy.
- `print` and `str` use stable descriptions such as `<class Counter>`, `<Counter instance>`, and `<bound method Counter.add>`. There are no user-defined conversion hooks for `print` and `str`; a class can define its own `ToString()` method for explicit calls (see [conversion methods](builtins.md#conversion-methods-tostring-toint-tofloat-tobool)).
- Class names are mutable bindings. A class's recorded parent reference does not change merely because the parent's name is later rebound.
- Duplicate members, duplicate constructors, duplicate parameters, a `this` parameter, and direct self-inheritance are syntax errors. Missing parents, non-class parents, invalid arity, and invalid `super` access are runtime errors.
- Static methods, static fields, decorators, getters/setters, multiple parents, and operator overloading are not implemented. Declared fields belong to instances. Class-level member access such as `Counter.add` is not supported.

## Try It

Derive `DoubleCounter` from the first example's `Counter`. Override `add` to call `super.add(amount * 2)` without defining a new constructor. `DoubleCounter(10).add(3)` should return `16`.

See [the full OOP example](../examples/oop.tg) and [OOP benchmarks](../benchmarks/README.md) for larger programs.