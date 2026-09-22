package parser

import (
	"testing"
	"tiger/pkg/ast"
)

func TestClassSyntax(t *testing.T) {
	program, err := Parse(`class Animal {
    function init(this, name) { this.name = name; }
    function speak(this) { return this.name; }
}
class Dog(Animal) {
    function init(this, name) { super.init(name); }
    function speak(this) { return super.speak() + "!"; }
}
const pet = Dog("Rex");
pet.friend = Animal("Cat");
print(pet.friend.speak(), [pet][0].name);
`)
	if err != nil {
		t.Fatal(err)
	}
	derived := program.Statements[1].(*ast.Class)
	if derived.Parent.Name != "Animal" || len(derived.Methods) != 2 {
		t.Fatalf("incorrect derived class: %#v", derived)
	}
	assignment := program.Statements[3].(*ast.Assign)
	if assignment.Target.(*ast.Property).Name != "friend" {
		t.Fatal("incorrect property assignment")
	}
}

func TestInvalidClassSyntax(t *testing.T) {
	for _, source := range []string{
		`class Animal: {}`, `class Animal(Parent, Other) {}`, `class Animal(Animal) {}`,
		`class Animal { function speak() {} }`, `class Animal { function speak(name, this) {} }`,
		`class Animal { function speak(this) {} function speak(this) {} }`,
		`class Animal { value = 1; }`, `class Animal { function init(this) { this.name = "Rex" } }`,
		`class Animal { function speak(this) { return 1 } }`, `class Animal {`,
		`pet.name = 1`, `pet.speak()`, `super;`, `pet.;`,
	} {
		t.Run(source, func(t *testing.T) {
			if _, err := Parse(source); err == nil {
				t.Fatal("expected syntax error")
			}
		})
	}
}
