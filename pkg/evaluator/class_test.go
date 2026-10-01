package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestClasses(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"spec example", `class Animal {
var name; var species;
Animal(name, species) { this.name = name; this.species = species; }
function speak() { return this.name + " makes a generic sound."; }
function describe() { return this.name + " is a " + this.species + "."; }
}
class Dog extends Animal {
var breed;
Dog(name, breed) { super(name, "Canine"); this.breed = breed; }
function speak() { return this.name + " barks!"; }
function fetch(item) { return this.name + " fetches the " + item + "."; }
}
const pet = Dog("Rex", "German Shepherd");
print(pet.describe()); print(pet.speak()); print(pet.fetch("ball"));`,
			"Rex is a Canine.\nRex barks!\nRex fetches the ball.\n"},
		{"independent fields", `class Box { var value; var items = []; Box(value) { this.value = value; } }
const first = Box(1); const second = Box(2); first.items = first.items + [3]; first.value = 7;
print(first.value, second.value, first.items, second.items, first == first, first == second);`, "7 2 [3] [] true false\n"},
		{"bound methods", `class Counter { var value = 0; function next(step) { this.value = this.value + step; return this.value; } }
const counter = Counter(); const saved = counter.next; print(saved(2), saved(3), counter.value, saved == counter.next);`, "2 5 5 true\n"},
		{"lexical super", `class Base { var value; Base(value) { this.value = value; } function text() { return "B"; } }
class Middle extends Base { function text() { return super.text() + "M"; } }
class Leaf extends Middle { function text() { return super.text() + "L"; } }
class Inherited extends Leaf {}
const leaf = Inherited(9); print(leaf.text(), leaf.value);`, "BML 9\n"},
		{"dynamic dispatch", `class Base { function text() { return "base"; } function describe() { return this.text(); } }
class Child extends Base { function text() { return "child"; } }
print(Child().describe());`, "child\n"},
		{"captured this and super", `class Base { function text() { return this.name; } }
class Child extends Base { var name; Child() { this.name = "captured"; } function callback() { function later() { return super.text() + this.name; } return later; } }
const callback = Child().callback(); print(callback());`, "capturedcaptured\n"},
		{"local classes", `function factory(prefix) { class Label { function text() { return prefix; } } return Label; }
const First = factory("one"); const Second = factory("two"); print(First().text(), Second().text());`, "one two\n"},
		{"chained properties", `class Box { var child; var value; } const outer = Box(); outer.child = Box(); outer.child.value = 3;
print([outer][0].child.value, outer.value, Box, outer);`, "3 null <class Box> <Box instance>\n"},
		{"uninitialized fields", `class Pair { var left; var right = left_default(); } function left_default() { return 2; } const pair = Pair(); print(pair.left, pair.right);`, "null 2\n"},
		{"inherited field assignment", `class Base { var tag; } class Child extends Base { Child() { this.tag = "child"; } } print(Child().tag);`, "child\n"},
		{"initializer result", `class Item { var value; Item() { this.value = 7; return 99; } } print(Item().value);`, "7\n"},
		{"init is ordinary", `class Item { function init(value) { return value * 2; } } print(Item().init(4));`, "8\n"},
		{"super constructor chain", `class A { var trail; A(value) { this.trail = ["A" + str(value)]; } }
class B extends A { B(value) { super(value + 1); this.trail += ["B"]; } }
class C extends B { C() { super(1); this.trail += ["C"]; } }
class D extends C {}
print(D().trail);`, "[\"A2\", \"B\", \"C\"]\n"},
		{"super without parent constructor", `class A {} class B extends A { var ok; B() { super(); this.ok = true; } } print(B().ok);`, "true\n"},
		{"protected constructor via super", `class A { var tag; protected A() { this.tag = "a"; } } class B extends A { B() { super(); } } print(B().tag);`, "a\n"},
		{"method recursion", `class Math { function factorial(number) { if number < 2 { return 1; } return number * this.factorial(number - 1); } } print(Math().factorial(6));`, "720\n"},
		{"super calls parent method", `class Base { function value() { return 5; } } class Child extends Base { function value() { return 9; } function parent() { return super.value(); } }
const item = Child(); print(item.value(), item.parent());`, "9 5\n"},
		{"nested class resets super", `class Base { function value() { return 1; } }
class Outer extends Base { function make() { class Local { function value() { return this; } } return Local(); } }
print(Outer().make());`, "<Local instance>\n"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var output bytes.Buffer
			if err := Run(test.source, &output); err != nil {
				t.Fatal(err)
			}
			if output.String() != test.want {
				t.Fatalf("got %q, want %q", output.String(), test.want)
			}
		})
	}
}

func TestClassErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`const Parent = 1; class Child extends Parent {}`, "parent must be a class"},
		{`class Child extends Missing {}`, "undefined name"},
		{`class Empty {} Empty(1);`, "expects 0 arguments"},
		{`class Item { Item(value) {} } Item();`, "Item expects 1 arguments"},
		{`class Item { function init(value) {} } Item(1);`, "declares no constructor"},
		{`class Item { function run() { super(); } } Item().run();`, "only be called inside a constructor"},
		{`class Item { Item() { super(); } } Item();`, "super requires a parent class"},
		{`class A {} class B extends A { B() { super(1); } } B();`, "A expects 0 arguments"},
		{`class A { A(x) {} } class B extends A { B() { super(); } } B();`, "A expects 1 arguments"},
		{`class A { private A() {} } class B extends A { B() { super(); } } B();`, "cannot access private"},
		{`class Item { Item() {} } Item().Item;`, "has no property"},
		{`super(1);`, "super is only available inside methods"},
		{`class Item { function method(value) {} } Item().method();`, "method expects 1 arguments"},
		{`class Item {} print(Item().missing);`, "has no property"},
		{`class Item { Item() { this.value = 1; } } Item();`, `Item has no field "value"`},
		{`class Item { function set() { this.value = 1; } } Item().set();`, `Item has no field "value"`},
		{`class Item {} const item = Item(); item.value = 1;`, `Item has no field "value"`},
		{`class Item { function value() { return 1; } } const item = Item(); item.value = 9;`, `Item has no field "value"`},
		{`class Item {} Item().count++;`, `Item has no property "count"`},
		{`print(1.name);`, "has no properties"},
		{`const value = 1; value.name = 2;`, "cannot assign a property"},
		{`super.method();`, "super is only available inside methods"},
		{`class Base { function method() { super.method(); } } Base().method();`, "super requires a parent class"},
		{`class Base {} class Child extends Base { function method() { super.missing(); } } Child().method();`, "has no method"},
		{`class Base {} class Child extends Base { function method() { super.value = 1; } } Child().method();`, "cannot assign a property"},
		{`class Item {} const item = Item(); item = Item();`, "cannot reassign const"},
		{`class Item {} class Item {}`, "already declared"},
		{`class Item { function forever() { this.forever(); } } Item().forever();`, "maximum evaluation depth"},
		{`class Base { function value() { return 1; } } class Outer extends Base { function make() { class Local { function value() { return super.value(); } } return Local(); } } Outer().make().value();`, "super requires a parent class"},
	} {
		t.Run(test.source, func(t *testing.T) {
			err := Run(test.source, nil)
			if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
				t.Fatalf("expected positioned %q error, got %v", test.message, err)
			}
		})
	}
}

func TestMemberAccess(t *testing.T) {
	const source = `class Base {
var visible = 1;
public const label = "base";
private var secret = 2;
protected var count = 3;
var items = [];
private function hidden() { return this.secret; }
function read() { return this.hidden(); }
function callback() { function later() { return ++this.secret; } return later; }
}
class Child extends Base {
function next() { return ++this.count; }
public function name() { return this.label; }
}
const first = Child(); const second = Child(); first.visible++; first.items = [9];
const callback = first.callback();
print(first.visible, second.visible, first.read(), callback(), first.next(), first.name(), second.items);`
	var output bytes.Buffer
	if err := Run(source, &output); err != nil || output.String() != "2 1 2 3 4 base []\n" {
		t.Fatalf("got %q, %v", output.String(), err)
	}
	for _, test := range []struct{ source, message string }{
		{`class Box { private var value = 1; } print(Box().value);`, "cannot access private"},
		{`class Box { private var value = 1; } const box = Box(); box.value = 2;`, "cannot access private"},
		{`class Box { protected var value = 1; } Box().value++;`, "cannot access protected"},
		{`class Box { private function hidden() {} } Box().hidden();`, "cannot access private"},
		{`class Base { private var value = 1; } class Child extends Base { function read() { return this.value; } } Child().read();`, "cannot access private"},
		{`class Base { private function hidden() {} } class Child extends Base { function read() { super.hidden(); } } Child().read();`, "cannot access private"},
		{`class Box { const value = 1; } Box().value++;`, "cannot reassign const field"},
		{`class Box { private Box() {} } Box();`, "cannot access private"},
		{`class Box { function old(this) {} }`, "this is implicit"},
		{`function helper(this) {}`, "this is implicit"},
		{`class Base { var value = 1; } class Child extends Base { var value = 2; }`, "cannot redeclare inherited"},
	} {
		if err := Run(test.source, nil); err == nil || !strings.Contains(err.Error(), test.message) {
			t.Errorf("%s: expected %q, got %v", test.source, test.message, err)
		}
	}
}
