package parser

import (
	"strings"
	"testing"
	"tiger/pkg/ast"
)

func TestPrecedence(t *testing.T) {
	program, err := Parse("answer = 1 + 2 * 3;")
	if err != nil {
		t.Fatal(err)
	}
	addition := program.Statements[0].(*ast.Assign).Value.(*ast.Binary)
	if addition.Operator != "+" || addition.Right.(*ast.Binary).Operator != "*" {
		t.Fatalf("incorrect precedence: %#v", addition)
	}
}

func TestSyntax(t *testing.T) {
	source := `const PI = 3.14;
def add(a, b) { return a + b; }
if PI > 3 { print(add(1, 2)); } elif PI == 3 { print("three"); } else { print(false); }
while false { value = 1; }
for item in [1, 2, "apple", true] { print(item); }
config = {"port": 8080, "host": "localhost"};
config["port"] = 80;
def empty() { return; };
`
	if _, err := Parse(source); err != nil {
		t.Fatal(err)
	}
}

func TestInvalidSyntax(t *testing.T) {
	for _, source := range []string{
		"value = 1", "print(1)", "def bad() { return 1 }", "if true: { print(1); }",
		"while true print(1);", "for item in [1] print(item);", "const value;",
		"def bad(a, a) {}", "return 1;", "1 = 2;", "if true {", "[1, 2;", "{1 2};",
	} {
		t.Run(source, func(t *testing.T) {
			if _, err := Parse(source); err == nil {
				t.Fatal("expected syntax error")
			}
		})
	}
}

func TestNestingLimit(t *testing.T) {
	if _, err := Parse(strings.Repeat("(", 600) + "1" + strings.Repeat(")", 600) + ";"); err == nil {
		t.Fatal("expected nesting error")
	}
}
