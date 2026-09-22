package parser

import (
	"strings"
	"testing"
	"tiger/pkg/ast"
)

func TestPrecedence(t *testing.T) {
	program, err := Parse("const answer = 1 + 2 * 3;")
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
function add(a, b) { return a + b; }
if PI > 3 { print(add(1, 2)); } elif PI == 3 { print("three"); } else { print(false); }
while false { const value = 1; }
for item in [1, 2, "apple", true] { print(item); }
const config = {"port": 8080, "host": "localhost"};
config["port"] = 80;
function empty() { return; };
`
	if _, err := Parse(source); err != nil {
		t.Fatal(err)
	}
}

func TestInvalidSyntax(t *testing.T) {
	for _, source := range []string{
		"value = 1", "print(1)", "function bad() { return 1 }", "if true: { print(1); }",
		"while true print(1);", "for item in [1] print(item);", "const value;",
		"function bad(a, a) {}", "return 1;", "1 = 2;", "if true {", "[1, 2;", "{1 2};",
		"var value;", "var value = ;", "const value = ;", "var items[0] = 1;", "const this.value = 1;",
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
	for _, source := range []string{
		strings.Repeat("switch 1 { case 1: ", 600) + strings.Repeat("}", 600),
		strings.Repeat(`f"{`, 600) + "1" + strings.Repeat(`}"`, 600) + ";",
	} {
		if _, err := Parse(source); err == nil {
			t.Fatal("expected nesting error")
		}
	}
}

func TestDeclarations(t *testing.T) {
	program, err := Parse("const limit = 3; var count = 0; count = 1;")
	if err != nil {
		t.Fatal(err)
	}
	for index, statement := range program.Statements {
		assignment := statement.(*ast.Assign)
		if assignment.Declaration != (index < 2) || assignment.Constant != (index == 0) {
			t.Fatalf("incorrect declaration flags at %d: %#v", index, assignment)
		}
	}
}

func TestControlSyntax(t *testing.T) {
	for _, source := range []string{
		`cfor (var count = 0; count < 3; ++count) { if count == 1 { continue; } } else { print("done"); }`,
		`cfor (;;) { break; }`,
		`while false {} else { print("empty"); } for item in [] {} else { print("empty"); }`,
		`switch 1 { case 1: print("one"); break; default: print("other"); }`,
		`var count = 0; print(++count, count++, --count, count--);`,
	} {
		if _, err := Parse(source); err != nil {
			t.Errorf("%s: %v", source, err)
		}
	}
	for _, source := range []string{
		`break;`, `continue;`, `switch 1 { case 1: continue; }`,
		`while true { function invalid() { break; } }`,
		`switch 1 { default: break; default: break; }`,
		`++1;`, `print(1)++;`, `cfor (; true; var value = 1) {}`,
		`try { print(1); }`, `try {} catch {}`, `try {} catch (error, other) {}`, `throw;`,
		`private var value = 1;`, `class Item { public private var value = 1; }`,
		`class Item { var value = 1; function value(this) {} }`,
	} {
		if _, err := Parse(source); err == nil {
			t.Errorf("expected error: %s", source)
		}
	}
}
