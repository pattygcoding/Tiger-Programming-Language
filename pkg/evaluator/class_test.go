package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestClasses(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"spec example", `class Animal {
def init(self, name, species) { self.name = name; self.species = species; }
def speak(self) { return self.name + " makes a generic sound."; }
def describe(self) { return self.name + " is a " + self.species + "."; }
}
class Dog(Animal) {
def init(self, name, breed) { super.init(name, "Canine"); self.breed = breed; }
def speak(self) { return self.name + " barks!"; }
def fetch(self, item) { return self.name + " fetches the " + item + "."; }
}
const pet = Dog("Rex", "German Shepherd");
print(pet.describe()); print(pet.speak()); print(pet.fetch("ball"));`,
			"Rex is a Canine.\nRex barks!\nRex fetches the ball.\n"},
		{"independent fields", `class Box { def init(self, value) { self.value = value; self.items = []; } }
const first = Box(1); const second = Box(2); first.items = first.items + [3]; first.value = 7;
print(first.value, second.value, first.items, second.items, first == first, first == second);`, "7 2 [3] [] true false\n"},
		{"bound methods", `class Counter { def init(self) { self.value = 0; } def next(self, step) { self.value = self.value + step; return self.value; } }
counter = Counter(); saved = counter.next; print(saved(2), saved(3), counter.value, saved == counter.next);`, "2 5 5 true\n"},
		{"lexical super", `class Base { def init(self, value) { self.value = value; } def text(self) { return "B"; } }
class Middle(Base) { def text(self) { return super.text() + "M"; } }
class Leaf(Middle) { def text(self) { return super.text() + "L"; } }
class Inherited(Leaf) {}
leaf = Inherited(9); print(leaf.text(), leaf.value);`, "BML 9\n"},
		{"dynamic dispatch", `class Base { def text(self) { return "base"; } def describe(self) { return self.text(); } }
class Child(Base) { def text(self) { return "child"; } }
print(Child().describe());`, "child\n"},
		{"captured self and super", `class Base { def text(self) { return self.name; } }
class Child(Base) { def init(self) { self.name = "captured"; } def callback(self) { def later() { return super.text() + self.name; } return later; } }
callback = Child().callback(); print(callback());`, "capturedcaptured\n"},
		{"local classes", `def factory(prefix) { class Label { def text(self) { return prefix; } } return Label; }
First = factory("one"); Second = factory("two"); print(First().text(), Second().text());`, "one two\n"},
		{"chained properties", `class Box {} const outer = Box(); outer.child = Box(); outer.child.value = 3; outer.child.self = outer.child;
print([outer][0].child.value, outer.child.self == outer.child, Box, outer);`, "3 true <class Box> <Box instance>\n"},
		{"field shadows method", `class Item { def value(self) { return 1; } } item = Item(); item.value = 9; print(item.value);`, "9\n"},
		{"initializer result", `class Item { def init(self) { self.value = 7; return 99; } } print(Item().value);`, "7\n"},
		{"method recursion", `class Math { def factorial(self, number) { if number < 2 { return 1; } return number * self.factorial(number - 1); } } print(Math().factorial(6));`, "720\n"},
		{"super ignores fields", `class Base { def value(self) { return 5; } } class Child(Base) { def parent(self) { return super.value(); } }
item = Child(); item.value = 9; print(item.value, item.parent());`, "9 5\n"},
		{"nested class resets super", `class Base { def value(self) { return 1; } }
class Outer(Base) { def make(self) { class Local { def value(self) { return self; } } return Local(); } }
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
		{`Parent = 1; class Child(Parent) {}`, "parent must be a class"},
		{`class Child(Missing) {}`, "undefined name"},
		{`class Empty {} Empty(1);`, "expects 0 arguments"},
		{`class Item { def init(self, value) {} } Item();`, "init expects 1 arguments"},
		{`class Item { def method(self, value) {} } Item().method();`, "method expects 1 arguments"},
		{`class Item {} print(Item().missing);`, "has no property"},
		{`print(1.name);`, "has no properties"},
		{`value = 1; value.name = 2;`, "cannot assign a property"},
		{`super.method();`, "super is only available inside methods"},
		{`class Base { def method(self) { super.method(); } } Base().method();`, "super requires a parent class"},
		{`class Base {} class Child(Base) { def method(self) { super.missing(); } } Child().method();`, "has no method"},
		{`class Base {} class Child(Base) { def method(self) { super.value = 1; } } Child().method();`, "cannot assign a property"},
		{`class Item {} const item = Item(); item = Item();`, "cannot reassign const"},
		{`class Item {} class Item {}`, "already declared"},
		{`class Item { def forever(self) { self.forever(); } } Item().forever();`, "maximum evaluation depth"},
		{`class Base { def value(self) { return 1; } } class Outer(Base) { def make(self) { class Local { def value(self) { return super.value(); } } return Local(); } } Outer().make().value();`, "super requires a parent class"},
	} {
		t.Run(test.source, func(t *testing.T) {
			err := Run(test.source, nil)
			if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
				t.Fatalf("expected positioned %q error, got %v", test.message, err)
			}
		})
	}
}
