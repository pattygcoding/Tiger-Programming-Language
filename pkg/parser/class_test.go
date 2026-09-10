package parser

import (
	"testing"
	"tiger/pkg/ast"
)

func TestClassSyntax(t *testing.T) {
	program, err := Parse(`class Animal {
    def init(self, name) { self.name = name; }
    def speak(self) { return self.name; }
}
class Dog(Animal) {
    def init(self, name) { super.init(name); }
    def speak(self) { return super.speak() + "!"; }
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
		`class Animal { def speak() {} }`, `class Animal { def speak(name, self) {} }`,
		`class Animal { def speak(self) {} def speak(self) {} }`,
		`class Animal { value = 1; }`, `class Animal { def init(self) { self.name = "Rex" } }`,
		`class Animal { def speak(self) { return 1 } }`, `class Animal {`,
		`pet.name = 1`, `pet.speak()`, `super;`, `pet.;`,
	} {
		t.Run(source, func(t *testing.T) {
			if _, err := Parse(source); err == nil {
				t.Fatal("expected syntax error")
			}
		})
	}
}
