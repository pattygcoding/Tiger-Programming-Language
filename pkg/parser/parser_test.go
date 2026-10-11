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

func TestSyntaxGaps(t *testing.T) {
	program, err := Parse(`const pick = "a" if flag else "b";
outer: for item in [1, 2] {
    if item == 1 { continue outer; }
    break outer;
}
do { print(1); } while (false);
const window = [1, 2, 3][0:2];`)
	if err != nil {
		t.Fatal(err)
	}
	conditional, ok := program.Statements[0].(*ast.Assign).Value.(*ast.Conditional)
	if !ok || conditional.Then == nil || conditional.Condition == nil || conditional.Else == nil {
		t.Fatalf("conditional expression did not parse: %#v", program.Statements[0])
	}
	loop, ok := program.Statements[1].(*ast.For)
	if !ok || loop.Label != "outer" {
		t.Fatalf("labeled loop did not parse: %#v", program.Statements[1])
	}
	branch := loop.Body[0].(*ast.If).Branches[0].Body[0].(*ast.Control)
	if branch.Kind != "continue" || branch.Label != "outer" {
		t.Fatalf("labeled continue did not parse: %#v", branch)
	}
	exit := loop.Body[1].(*ast.Control)
	if exit.Kind != "break" || exit.Label != "outer" {
		t.Fatalf("labeled break did not parse: %#v", exit)
	}
	if _, ok := program.Statements[2].(*ast.DoWhile); !ok {
		t.Fatalf("do...while did not parse: %#v", program.Statements[2])
	}
	slice, ok := program.Statements[3].(*ast.Assign).Value.(*ast.Slice)
	if !ok || slice.Start == nil || slice.Stop == nil || slice.Step != nil {
		t.Fatalf("slice did not parse: %#v", program.Statements[3])
	}
}

func TestSyntaxGapErrors(t *testing.T) {
	for _, source := range []string{
		"print(1 if true);",
		"print(1 if true else);",
		"break missing;",
		"outer: for item in [1] { continue missing; }",
		"outer: while true { function inner() { break outer; } }",
		"do { print(1); } while;",
		"do { print(1); } while true",
		"for item in [1] { continue 5; }",
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
	program, err := Parse("const limit = 3; var count = 0; count = 1; count += 2;")
	if err != nil {
		t.Fatal(err)
	}
	for index, statement := range program.Statements {
		assignment := statement.(*ast.Assign)
		if assignment.Declaration != (index < 2) || assignment.Constant != (index == 0) {
			t.Fatalf("incorrect declaration flags at %d: %#v", index, assignment)
		}
	}
	if assignment := program.Statements[3].(*ast.Assign); assignment.Operator != "+=" {
		t.Fatalf("incorrect compound operator: %#v", assignment)
	}
}

func TestKeywordArguments(t *testing.T) {
	program, err := Parse(`print("working", end="");`)
	if err != nil {
		t.Fatal(err)
	}
	call := program.Statements[0].(*ast.ExpressionStmt).Value.(*ast.Call)
	if len(call.Arguments) != 1 || len(call.Keywords) != 1 || call.Keywords[0].Name != "end" {
		t.Fatalf("incorrect call arguments: %#v", call)
	}
	for _, source := range []string{`print(end="", 1);`, `print(end="", end="!");`} {
		if _, err := Parse(source); err == nil {
			t.Errorf("expected syntax error: %s", source)
		}
	}
}

func TestVariadicAndSpreadSyntax(t *testing.T) {
	for _, source := range []string{
		`function collect(first, *rest, **options) { return rest; }`,
		`function only(*rest) { return rest; }`,
		`function only(**options) { return options; }`,
		`function keyword_first(**options) {}`,
		`function mixed(a, b, *rest, **options) {}`,
		`class Item { function method(*rest, **options) {} }`,
		`const numbers = [1, 2]; collect(*numbers);`,
		`collect(**{"a": 1});`,
		`collect(1, *numbers, key="value", **{"other": 2});`,
	} {
		if _, err := Parse(source); err != nil {
			t.Errorf("%s: %v", source, err)
		}
	}
	for _, source := range []string{
		`function bad(*rest, *more) {}`,
		`function bad(**options, **more) {}`,
		`function bad(**options, *rest) {}`,
		`function bad(first, *first) {}`,
		`function bad(a, *rest, extra) {}`,
		`function bad(*) {}`,
		`function bad(**) {}`,
		`class Item { function method(this, *rest) {} }`,
		`collect(*numbers, key="value", 2);`,
		`collect(key="value", *numbers);`,
	} {
		if _, err := Parse(source); err == nil {
			t.Errorf("expected syntax error: %s", source)
		}
	}
}

func TestImports(t *testing.T) {
	program, err := Parse(`import "lib/math.tg" as math; print(math.square(3));`)
	if err != nil {
		t.Fatal(err)
	}
	declaration := program.Statements[0].(*ast.Import)
	if declaration.Path != "lib/math.tg" || declaration.Name != "math" {
		t.Fatalf("unexpected import: %#v", declaration)
	}
	for _, source := range []string{`import "math.tg";`, `import math as math;`, `import "math.tg" as;`, `import f"math.tg" as math;`, `import "math.tg" as math`} {
		if _, err := Parse(source); err == nil {
			t.Errorf("expected syntax error: %s", source)
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
		`class Item { var value = 1; function value() {} }`,
	} {
		if _, err := Parse(source); err == nil {
			t.Errorf("expected error: %s", source)
		}
	}
}

func TestCStyleConditions(t *testing.T) {
	program, err := Parse(`cif (a && b || c) { print(1); } celse cif (d) {} celse { print(2); }`)
	if err != nil {
		t.Fatal(err)
	}
	conditional := program.Statements[0].(*ast.If)
	if len(conditional.Branches) != 2 || len(conditional.Else) != 1 {
		t.Fatalf("incorrect branches: %#v", conditional)
	}
	or := conditional.Branches[0].Condition.(*ast.Binary)
	if or.Operator != "or" || or.Left.(*ast.Binary).Operator != "and" {
		t.Fatalf("&& must bind tighter than ||: %#v", or)
	}
	for source, message := range map[string]string{
		`print(a && b);`:               "&& is only allowed in cif and cfor headers; use and",
		`if a || b {}`:                 "|| is only allowed in cif and cfor headers; use or",
		`cif (a) {} else {}`:           "use celse or celse cif after a cif block",
		`if a {} celse {}`:             "use elif or else after an if block",
		`cif (a) { print(a && b); }`:   "&& is only allowed",
		`cfor (;;) { print(a || b); }`: "|| is only allowed",
	} {
		if _, err := Parse(source); err == nil || !strings.Contains(err.Error(), message) {
			t.Errorf("%s: expected %q, got %v", source, message, err)
		}
	}
}

func TestPowerAndBitwisePrecedence(t *testing.T) {
	value := func(source string) ast.Expr {
		program, err := Parse(source)
		if err != nil {
			t.Fatal(err)
		}
		return program.Statements[0].(*ast.Assign).Value
	}

	power := value("const power = 2 ** 3 ** 2;").(*ast.Binary)
	if power.Operator != "**" || power.Right.(*ast.Binary).Operator != "**" {
		t.Fatalf("** must be right-associative: %#v", power)
	}

	unary := value("const unary = -2 ** 2;").(*ast.Unary)
	if unary.Operator != "-" || unary.Right.(*ast.Binary).Operator != "**" {
		t.Fatalf("unary minus must bind looser than **: %#v", unary)
	}

	or := value("const bits = 1 | 2 & 3;").(*ast.Binary)
	if or.Operator != "|" || or.Right.(*ast.Binary).Operator != "&" {
		t.Fatalf("& must bind tighter than |: %#v", or)
	}

	shift := value("const shift = 1 + 2 << 3;").(*ast.Binary)
	if shift.Operator != "<<" || shift.Left.(*ast.Binary).Operator != "+" {
		t.Fatalf("+ must bind tighter than <<: %#v", shift)
	}

	comparison := value("const compare = 1 | 2 == 3;").(*ast.Binary)
	if comparison.Operator != "==" || comparison.Left.(*ast.Binary).Operator != "|" {
		t.Fatalf("| must bind tighter than ==: %#v", comparison)
	}

	floor := value("const floor = 6 div 3 * 2;").(*ast.Binary)
	if floor.Operator != "*" || floor.Left.(*ast.Binary).Operator != "div" {
		t.Fatalf("div must be left-associative with *: %#v", floor)
	}
}

func TestFunctionErgonomicsParsing(t *testing.T) {
	named, err := Parse(`function f(a, b = 2) { return a; }`)
	if err != nil {
		t.Fatal(err)
	}
	declaration := named.Statements[0].(*ast.Function)
	if len(declaration.Parameters) != 2 || declaration.Defaults[0] != nil || declaration.Defaults[1] == nil {
		t.Fatalf("unexpected defaults: %#v", declaration.Defaults)
	}

	anonymous, err := Parse(`const f = function(x) { return x; };`)
	if err != nil {
		t.Fatal(err)
	}
	if value := anonymous.Statements[0].(*ast.Assign).Value.(*ast.Function); value.Name != "" {
		t.Fatalf("anonymous function has a name: %#v", value)
	}

	multiple, err := Parse(`function f() { return 1, 2; }`)
	if err != nil {
		t.Fatal(err)
	}
	body := multiple.Statements[0].(*ast.Function).Body[0].(*ast.Return).Value.(*ast.List)
	if len(body.Elements) != 2 {
		t.Fatalf("expected a two-element list return: %#v", body)
	}

	destructured, err := Parse(`const [a, b] = pair();`)
	if err != nil {
		t.Fatal(err)
	}
	statement := destructured.Statements[0].(*ast.Destructure)
	if statement.Lookup || len(statement.Names) != 2 || !statement.Constant {
		t.Fatalf("unexpected destructure: %#v", statement)
	}

	record, err := Parse(`var {x, y} = record;`)
	if err != nil {
		t.Fatal(err)
	}
	if lookup := record.Statements[0].(*ast.Destructure); !lookup.Lookup || lookup.Constant {
		t.Fatalf("unexpected dictionary destructure: %#v", lookup)
	}

	if _, err := Parse(`function bad(a = 1, b) { return a; }`); err == nil {
		t.Fatal("expected an error for a required parameter after a defaulted one")
	}
}

func TestTryCatchesAndFinally(t *testing.T) {
	program, err := Parse(`try { print(1); } catch (error) if isinstance(error, "error") { print(2); } finally { print(3); }`)
	if err != nil {
		t.Fatal(err)
	}
	statement := program.Statements[0].(*ast.Try)
	if len(statement.Catches) != 1 || statement.Catches[0].Condition == nil || len(statement.Finally) == 0 {
		t.Fatalf("unexpected try: %#v", statement)
	}
	if _, err := Parse(`try { } finally { }`); err != nil {
		t.Fatalf("try/finally must parse: %v", err)
	}
	if _, err := Parse(`try { }`); err == nil {
		t.Fatal("expected an error for a try without catch or finally")
	}
	chained, err := Parse(`try { } catch (error) if error["kind"] == "x" { } catch (other) { }`)
	if err != nil || len(chained.Statements[0].(*ast.Try).Catches) != 2 {
		t.Fatalf("expected two catch clauses: %v", err)
	}
}
