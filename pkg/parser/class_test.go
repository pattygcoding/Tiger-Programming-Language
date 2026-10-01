package parser

import (
	"strings"
	"testing"
	"tiger/pkg/ast"
)

func TestClassSyntax(t *testing.T) {
	program, err := Parse(`class Animal {
    Animal(name) { this.name = name; }
    function speak() { return this.name; }
}
class Dog extends Animal {
    Dog(name) { super(name); }
    function speak() { return super.speak() + "!"; }
}
const pet = Dog("Rex");
pet.friend = Animal("Cat");
print(pet.friend.speak(), [pet][0].name);
`)
	if err != nil {
		t.Fatal(err)
	}
	derived := program.Statements[1].(*ast.Class)
	if derived.Parent.Name != "Animal" || len(derived.Methods) != 1 || derived.Constructor == nil || derived.Constructor.Name != "Dog" {
		t.Fatalf("incorrect derived class: %#v", derived)
	}
	assignment := program.Statements[3].(*ast.Assign)
	if assignment.Target.(*ast.Property).Name != "friend" {
		t.Fatal("incorrect property assignment")
	}
}

func TestInvalidClassSyntax(t *testing.T) {
	for _, source := range []string{
		`class Animal: {}`, `class Animal extends Parent, Other {}`, `class Animal extends Animal {}`,
		`class Animal(Parent) {}`, `class Animal extends {}`, `class Animal extends 1 {}`,
		`class Animal { function speak(this) {} }`, `class Animal { function speak(name, this) {} }`,
		`class Animal { function speak() {} function speak() {} }`,
		`class Animal { value = 1; }`, `class Animal { Animal() { this.name = "Rex" } }`,
		`class Animal { Animal(this) {} }`, `class Animal { Animal(name, this) {} }`,
		`class Animal { Animal() {} Animal(name) {} }`, `class Animal { function Animal() {} }`,
		`class Animal { Other() {} }`, `super.;`, `class Animal { const legs; }`, `class Animal { var legs }`, `class Animal { var legs = ; }`,
		`class Animal { function speak() { return 1 } }`, `class Animal {`,
		`pet.name = 1`, `pet.speak()`, `super;`, `pet.;`,
	} {
		t.Run(source, func(t *testing.T) {
			if _, err := Parse(source); err == nil {
				t.Fatal("expected syntax error")
			}
		})
	}
}

func TestParenthesizedInheritanceHint(t *testing.T) {
	_, err := Parse(`class Dog(Animal) {}`)
	if err == nil || !strings.Contains(err.Error(), "use 'class Dog extends Parent'") {
		t.Fatalf("expected extends hint, got %v", err)
	}
}
