//! Classes: fields, constructors, inheritance, `super`, visibility, and their errors.
//!
//! A port of `pkg/evaluator/class_test.go`.

mod common;

use common::{expect_errors, expect_outputs, output};

#[test]
fn classes() {
    expect_outputs(&[
        (
            "class Animal {\n\
             var name; var species;\n\
             Animal(name, species) { this.name = name; this.species = species; }\n\
             function speak() { return this.name + \" makes a generic sound.\"; }\n\
             function describe() { return this.name + \" is a \" + this.species + \".\"; }\n\
             }\n\
             class Dog extends Animal {\n\
             var breed;\n\
             Dog(name, breed) { super(name, \"Canine\"); this.breed = breed; }\n\
             function speak() { return this.name + \" barks!\"; }\n\
             function fetch(item) { return this.name + \" fetches the \" + item + \".\"; }\n\
             }\n\
             const pet = Dog(\"Rex\", \"German Shepherd\");\n\
             print(pet.describe()); print(pet.speak()); print(pet.fetch(\"ball\"));",
            "Rex is a Canine.\nRex barks!\nRex fetches the ball.\n",
        ),
        (
            "class Box { var value; var items = []; Box(value) { this.value = value; } }\n\
             const first = Box(1); const second = Box(2); first.items = first.items + [3]; first.value = 7;\n\
             print(first.value, second.value, first.items, second.items, first == first, first == second);",
            "7 2 [3] [] true false\n",
        ),
        (
            "class Counter { var value = 0; function next(step) { this.value = this.value + step; return this.value; } }\n\
             const counter = Counter(); const saved = counter.next; print(saved(2), saved(3), counter.value, saved == counter.next);",
            "2 5 5 true\n",
        ),
        (
            "class Base { var value; Base(value) { this.value = value; } function text() { return \"B\"; } }\n\
             class Middle extends Base { function text() { return super.text() + \"M\"; } }\n\
             class Leaf extends Middle { function text() { return super.text() + \"L\"; } }\n\
             class Inherited extends Leaf {}\n\
             const leaf = Inherited(9); print(leaf.text(), leaf.value);",
            "BML 9\n",
        ),
        (
            "class Base { function text() { return \"base\"; } function describe() { return this.text(); } }\n\
             class Child extends Base { function text() { return \"child\"; } }\n\
             print(Child().describe());",
            "child\n",
        ),
        (
            "class Base { function text() { return this.name; } }\n\
             class Child extends Base { var name; Child() { this.name = \"captured\"; } function callback() { function later() { return super.text() + this.name; } return later; } }\n\
             const callback = Child().callback(); print(callback());",
            "capturedcaptured\n",
        ),
        (
            "function factory(prefix) { class Label { function text() { return prefix; } } return Label; }\n\
             const First = factory(\"one\"); const Second = factory(\"two\"); print(First().text(), Second().text());",
            "one two\n",
        ),
        (
            "class Box { var child; var value; } const outer = Box(); outer.child = Box(); outer.child.value = 3;\n\
             print([outer][0].child.value, outer.value, Box, outer);",
            "3 null <class Box> <Box instance>\n",
        ),
        (
            "class Pair { var left; var right = left_default(); } function left_default() { return 2; } const pair = Pair(); print(pair.left, pair.right);",
            "null 2\n",
        ),
        (
            "class Base { var tag; } class Child extends Base { Child() { this.tag = \"child\"; } } print(Child().tag);",
            "child\n",
        ),
        ("class Item { var value; Item() { this.value = 7; return 99; } } print(Item().value);", "7\n"),
        ("class Item { function init(value) { return value * 2; } } print(Item().init(4));", "8\n"),
        (
            "class A { var trail; A(value) { this.trail = [\"A\" + str(value)]; } }\n\
             class B extends A { B(value) { super(value + 1); this.trail += [\"B\"]; } }\n\
             class C extends B { C() { super(1); this.trail += [\"C\"]; } }\n\
             class D extends C {}\n\
             print(D().trail);",
            "[\"A2\", \"B\", \"C\"]\n",
        ),
        (
            "class A {} class B extends A { var ok; B() { super(); this.ok = true; } } print(B().ok);",
            "true\n",
        ),
        (
            "class A { var tag; protected A() { this.tag = \"a\"; } } class B extends A { B() { super(); } } print(B().tag);",
            "a\n",
        ),
        (
            "class Math { function factorial(number) { if number < 2 { return 1; } return number * this.factorial(number - 1); } } print(Math().factorial(6));",
            "720\n",
        ),
        (
            "class Base { function value() { return 5; } } class Child extends Base { function value() { return 9; } function parent() { return super.value(); } }\n\
             const item = Child(); print(item.value(), item.parent());",
            "9 5\n",
        ),
        (
            "class Base { function value() { return 1; } }\n\
             class Outer extends Base { function make() { class Local { function value() { return this; } } return Local(); } }\n\
             print(Outer().make());",
            "<Local instance>\n",
        ),
    ]);
}

#[test]
fn class_errors() {
    expect_errors(&[
        ("const Parent = 1; class Child extends Parent {}", "parent must be a class"),
        ("class Child extends Missing {}", "undefined name"),
        ("class Empty {} Empty(1);", "expects 0 arguments"),
        ("class Item { Item(value) {} } Item();", "Item expects 1 arguments"),
        ("class Item { function init(value) {} } Item(1);", "declares no constructor"),
        ("class Item { function run() { super(); } } Item().run();", "only be called inside a constructor"),
        ("class Item { Item() { super(); } } Item();", "super requires a parent class"),
        ("class A {} class B extends A { B() { super(1); } } B();", "A expects 0 arguments"),
        ("class A { A(x) {} } class B extends A { B() { super(); } } B();", "A expects 1 arguments"),
        ("class A { private A() {} } class B extends A { B() { super(); } } B();", "cannot access private"),
        ("class Item { Item() {} } Item().Item;", "has no property"),
        ("super(1);", "super is only available inside methods"),
        ("class Item { function method(value) {} } Item().method();", "method expects 1 arguments"),
        ("class Item {} print(Item().missing);", "has no property"),
        ("class Item { Item() { this.value = 1; } } Item();", "Item has no field \"value\""),
        ("class Item { function set() { this.value = 1; } } Item().set();", "Item has no field \"value\""),
        ("class Item {} const item = Item(); item.value = 1;", "Item has no field \"value\""),
        (
            "class Item { function value() { return 1; } } const item = Item(); item.value = 9;",
            "Item has no field \"value\"",
        ),
        ("class Item {} Item().count++;", "Item has no property \"count\""),
        ("print(1.name);", "has no properties"),
        ("const value = 1; value.name = 2;", "cannot assign a property"),
        ("super.method();", "super is only available inside methods"),
        ("class Base { function method() { super.method(); } } Base().method();", "super requires a parent class"),
        (
            "class Base {} class Child extends Base { function method() { super.missing(); } } Child().method();",
            "has no method",
        ),
        (
            "class Base {} class Child extends Base { function method() { super.value = 1; } } Child().method();",
            "cannot assign a property",
        ),
        ("class Item {} const item = Item(); item = Item();", "cannot reassign const"),
        ("class Item {} class Item {}", "already declared"),
        ("class Item { function forever() { this.forever(); } } Item().forever();", "maximum evaluation depth"),
        (
            "class Base { function value() { return 1; } } class Outer extends Base { function make() { class Local { function value() { return super.value(); } } return Local(); } } Outer().make().value();",
            "super requires a parent class",
        ),
    ]);
}

#[test]
fn member_access() {
    let source = "class Base {\n\
        var visible = 1;\n\
        public const label = \"base\";\n\
        private var secret = 2;\n\
        protected var count = 3;\n\
        var items = [];\n\
        private function hidden() { return this.secret; }\n\
        function read() { return this.hidden(); }\n\
        function callback() { function later() { return ++this.secret; } return later; }\n\
        }\n\
        class Child extends Base {\n\
        function next() { return ++this.count; }\n\
        public function name() { return this.label; }\n\
        }\n\
        const first = Child(); const second = Child(); first.visible++; first.items = [9];\n\
        const callback = first.callback();\n\
        print(first.visible, second.visible, first.read(), callback(), first.next(), first.name(), second.items);";
    assert_eq!(output(source), "2 1 2 3 4 base []\n");

    // Go checks these without requiring a position, so neither does this.
    for (source, message) in [
        ("class Box { private var value = 1; } print(Box().value);", "cannot access private"),
        ("class Box { private var value = 1; } const box = Box(); box.value = 2;", "cannot access private"),
        ("class Box { protected var value = 1; } Box().value++;", "cannot access protected"),
        ("class Box { private function hidden() {} } Box().hidden();", "cannot access private"),
        (
            "class Base { private var value = 1; } class Child extends Base { function read() { return this.value; } } Child().read();",
            "cannot access private",
        ),
        (
            "class Base { private function hidden() {} } class Child extends Base { function read() { super.hidden(); } } Child().read();",
            "cannot access private",
        ),
        ("class Box { const value = 1; } Box().value++;", "cannot reassign const field"),
        ("class Box { private Box() {} } Box();", "cannot access private"),
        ("class Box { function old(this) {} }", "this is implicit"),
        ("function helper(this) {}", "this is implicit"),
        ("class Base { var value = 1; } class Child extends Base { var value = 2; }", "cannot redeclare inherited"),
    ] {
        let error = common::failure(source);
        assert!(error.contains(message), "{}: expected {:?}, got {}", source, message, error);
    }
}
