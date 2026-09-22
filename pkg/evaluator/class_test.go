package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestClasses(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"spec example", `class Animal {
function init(this, name, species) { this.name = name; this.species = species; }
function speak(this) { return this.name + " makes a generic sound."; }
function describe(this) { return this.name + " is a " + this.species + "."; }
}
class Dog(Animal) {
function init(this, name, breed) { super.init(name, "Canine"); this.breed = breed; }
function speak(this) { return this.name + " barks!"; }
function fetch(this, item) { return this.name + " fetches the " + item + "."; }
}
const pet = Dog("Rex", "German Shepherd");
print(pet.describe()); print(pet.speak()); print(pet.fetch("ball"));`,
			"Rex is a Canine.\nRex barks!\nRex fetches the ball.\n"},
		{"independent fields", `class Box { function init(this, value) { this.value = value; this.items = []; } }
const first = Box(1); const second = Box(2); first.items = first.items + [3]; first.value = 7;
print(first.value, second.value, first.items, second.items, first == first, first == second);`, "7 2 [3] [] true false\n"},
		{"bound methods", `class Counter { function init(this) { this.value = 0; } function next(this, step) { this.value = this.value + step; return this.value; } }
const counter = Counter(); const saved = counter.next; print(saved(2), saved(3), counter.value, saved == counter.next);`, "2 5 5 true\n"},
		{"lexical super", `class Base { function init(this, value) { this.value = value; } function text(this) { return "B"; } }
class Middle(Base) { function text(this) { return super.text() + "M"; } }
class Leaf(Middle) { function text(this) { return super.text() + "L"; } }
class Inherited(Leaf) {}
const leaf = Inherited(9); print(leaf.text(), leaf.value);`, "BML 9\n"},
		{"dynamic dispatch", `class Base { function text(this) { return "base"; } function describe(this) { return this.text(); } }
class Child(Base) { function text(this) { return "child"; } }
print(Child().describe());`, "child\n"},
		{"captured this and super", `class Base { function text(this) { return this.name; } }
class Child(Base) { function init(this) { this.name = "captured"; } function callback(this) { function later() { return super.text() + this.name; } return later; } }
const callback = Child().callback(); print(callback());`, "capturedcaptured\n"},
		{"local classes", `function factory(prefix) { class Label { function text(this) { return prefix; } } return Label; }
const First = factory("one"); const Second = factory("two"); print(First().text(), Second().text());`, "one two\n"},
		{"chained properties", `class Box {} const outer = Box(); outer.child = Box(); outer.child.value = 3; outer.child.this = outer.child;
print([outer][0].child.value, outer.child.this == outer.child, Box, outer);`, "3 true <class Box> <Box instance>\n"},
		{"field shadows method", `class Item { function value(this) { return 1; } } const item = Item(); item.value = 9; print(item.value);`, "9\n"},
		{"initializer result", `class Item { function init(this) { this.value = 7; return 99; } } print(Item().value);`, "7\n"},
		{"method recursion", `class Math { function factorial(this, number) { if number < 2 { return 1; } return number * this.factorial(number - 1); } } print(Math().factorial(6));`, "720\n"},
		{"super ignores fields", `class Base { function value(this) { return 5; } } class Child(Base) { function parent(this) { return super.value(); } }
const item = Child(); item.value = 9; print(item.value, item.parent());`, "9 5\n"},
		{"nested class resets super", `class Base { function value(this) { return 1; } }
class Outer(Base) { function make(this) { class Local { function value(this) { return this; } } return Local(); } }
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
		{`const Parent = 1; class Child(Parent) {}`, "parent must be a class"},
		{`class Child(Missing) {}`, "undefined name"},
		{`class Empty {} Empty(1);`, "expects 0 arguments"},
		{`class Item { function init(this, value) {} } Item();`, "init expects 1 arguments"},
		{`class Item { function method(this, value) {} } Item().method();`, "method expects 1 arguments"},
		{`class Item {} print(Item().missing);`, "has no property"},
		{`print(1.name);`, "has no properties"},
		{`const value = 1; value.name = 2;`, "cannot assign a property"},
		{`super.method();`, "super is only available inside methods"},
		{`class Base { function method(this) { super.method(); } } Base().method();`, "super requires a parent class"},
		{`class Base {} class Child(Base) { function method(this) { super.missing(); } } Child().method();`, "has no method"},
		{`class Base {} class Child(Base) { function method(this) { super.value = 1; } } Child().method();`, "cannot assign a property"},
		{`class Item {} const item = Item(); item = Item();`, "cannot reassign const"},
		{`class Item {} class Item {}`, "already declared"},
		{`class Item { function forever(this) { this.forever(); } } Item().forever();`, "maximum evaluation depth"},
		{`class Base { function value(this) { return 1; } } class Outer(Base) { function make(this) { class Local { function value(this) { return super.value(); } } return Local(); } } Outer().make().value();`, "super requires a parent class"},
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
private function hidden(this) { return this.secret; }
function read(this) { return this.hidden(); }
function callback(this) { function later() { return ++this.secret; } return later; }
}
class Child(Base) {
function next(this) { return ++this.count; }
public function name(this) { return this.label; }
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
		{`class Box { private function hidden(this) {} } Box().hidden();`, "cannot access private"},
		{`class Base { private var value = 1; } class Child(Base) { function read(this) { return this.value; } } Child().read();`, "cannot access private"},
		{`class Base { private function hidden(this) {} } class Child(Base) { function read(this) { super.hidden(); } } Child().read();`, "cannot access private"},
		{`class Box { const value = 1; } Box().value++;`, "cannot reassign const field"},
		{`class Box { private function init(this) {} } Box();`, "cannot access private"},
		{`class Box { function old(self) {} }`, "must declare this"},
		{`class Base { var value = 1; } class Child(Base) { var value = 2; }`, "cannot redeclare inherited"},
	} {
		if err := Run(test.source, nil); err == nil || !strings.Contains(err.Error(), test.message) {
			t.Errorf("%s: expected %q, got %v", test.source, test.message, err)
		}
	}
}
