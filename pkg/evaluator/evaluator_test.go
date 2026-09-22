package evaluator

import (
	"bytes"
	"errors"
	"strings"
	"testing"
	"tiger/pkg/parser"
)

func TestPrograms(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"factorial", `const MAX_RUNS = 5;
function factorial(n) { if n <= 1 { return 1; } return n * factorial(n - 1); }
var i = 1; while i <= MAX_RUNS { const val = factorial(i); print("factorial(" + str(i) + ") = " + str(val)); i = i + 1; }`,
			"factorial(1) = 1\nfactorial(2) = 2\nfactorial(3) = 6\nfactorial(4) = 24\nfactorial(5) = 120\n"},
		{"branches", `for x in [-1, 0, 1] { if x > 0 { print("Positive"); } elif x == 0 { print("Zero"); } else { print("Negative"); } }`, "Negative\nZero\nPositive\n"},
		{"collections", `const items = [1, 2, "apple", true]; items[0] = 9; const config = {"port": 8080, "host": "localhost"}; config["port"] = 80; print(items[-1], config["port"], len(items)); for key in config { print(key); }`, "true 80 4\nport\nhost\n"},
		{"closures", `function counter() { var count = 0; function next() { count = count + 1; return count; } return next; } const next = counter(); print(next(), next());`, "1 2\n"},
		{"scope updates", `var count = 0; while count < 3 { const local = 1; count = count + local; } for item in [1, 2] { count = count + item; } print(count);`, "6\n"},
		{"parameter scope", `const x = 3; function add(x) { x = x + 1; return x; } print(add(9), x);`, "10 3\n"},
		{"return propagation", `function find() { for item in [1, 2, 3] { while true { if item == 2 { return item; } return 1; } } } function empty() {} function explicit() { return; } print(find(), empty(), explicit());`, "1 null null\n"},
		{"operators", `print(1 + 2 * 3, (1 + 2) * 3, -5 % 3, 5 % -3, 7 / 2); print(not 1 == 2, false and missing, true or missing, "a" < "b");`, "7 9 1 -1 3.5\ntrue false true true\n"},
		{"membership", `print(2 in [1, 2], "port" in {"port": 80}, "ell" in "hello", [] == [], {"a": 1} == {"a": 1});`, "true true true true true\n"},
		{"cycles", `const items = [null]; items[0] = items; const other = [null]; other[0] = other; print(items, items == other); const config = {}; config["self"] = config; print(config);`, "[[...]] true\n{\"self\": {...}}\n"},
		{"const binding", `const items = [1]; items[0] = 2; print(items);`, "[2]\n"},
		{"dict key types", `const config = {1: "number", "1": "string", true: "bool", -0: "zero"}; print(config[1], config["1"], config[true], config[0]);`, "number string bool zero\n"},
		{"string iteration", `for char in "hi" { print(char); } print("hello"[-1], len("hello"));`, "h\ni\no 5\n"},
		{"list concat", `const left = [1]; const both = left + [2]; both[0] = 9; print(left, both);`, "[1] [9, 2]\n"},
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

func TestRuntimeErrors(t *testing.T) {
	tests := []struct{ source, message string }{
		{`const PI = 3; PI = 4;`, "cannot reassign const"},
		{`const PI = 3; if true { PI = 4; }`, "cannot reassign const"},
		{`const PI = 3; function change() { PI = 4; } change();`, "cannot reassign const"},
		{`const PI = 3; const PI = 4;`, "already declared"},
		{`if true { const local = 1; } print(local);`, "undefined name"},
		{`for item in [1] {} print(item);`, "undefined name"},
		{`function local() { const hidden = 1; } local(); print(hidden);`, "undefined name"},
		{`print(1 / 0);`, "division by zero"},
		{`print(1 % 0);`, "division by zero"},
		{`print(1 + "x");`, "not supported"},
		{`function add(a, b) { return a + b; } add(1);`, "expects 2 arguments"},
		{`str();`, "expects 1 argument"},
		{`len(1);`, "does not accept number"},
		{`1();`, "not callable"},
		{`print([1][2]);`, "index out of range"},
		{`print([1][0.5]);`, "index must be an integer"},
		{`print({}["missing"]);`, "key not found"},
		{`const config = {[]: 1};`, "cannot be a dictionary key"},
		{`for item in 1 {}`, "not iterable"},
		{`const text = "hi"; text[0] = "a";`, "cannot assign an index"},
		{`print(1e308 * 1e308);`, "non-finite"},
		{`function forever() { forever(); } forever();`, "maximum evaluation depth"},
	}
	for _, test := range tests {
		t.Run(test.source, func(t *testing.T) {
			err := Run(test.source, nil)
			if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
				t.Fatalf("expected %q with position, got %v", test.message, err)
			}
		})
	}
}

func TestExecutionLimit(t *testing.T) {
	program, err := parser.Parse("while true {}")
	if err != nil {
		t.Fatal(err)
	}
	eval := New(nil)
	eval.MaxSteps = 20
	if err := eval.Execute(program); err == nil || !strings.Contains(err.Error(), "step limit") {
		t.Fatalf("expected execution limit, got %v", err)
	}
}

func TestExplicitDeclarations(t *testing.T) {
	tests := []struct{ source, want, message string }{
		{`var count = 0; function next() { count = count + 1; } next(); print(count);`, "1\n", ""},
		{`const value = 1; if true { var value = 2; value = 3; print(value); } print(value);`, "3\n1\n", ""},
		{`const values = [1]; values[0] = 2; print(values);`, "[2]\n", ""},
		{`/* multiline
comment */ const text = "# // /* */"; print(text); // end`, "# // /* */\n", ""},
		{`missing = 1;`, "", "declare it with const or var"},
		{`function create() { missing = 1; } create();`, "", "declare it with const or var"},
		{`var value = 1; var value = 2;`, "", "already declared"},
		{`var value = 1; const value = 2;`, "", "already declared"},
		{`const value = 1; value = 2;`, "", "cannot reassign const"},
		{`def old() {}`, "", "expected"},
		{`# old comment`, "", "unexpected character"},
	}
	for _, test := range tests {
		t.Run(test.source, func(t *testing.T) {
			var output bytes.Buffer
			err := Run(test.source, &output)
			if test.message != "" {
				if err == nil || !strings.Contains(err.Error(), test.message) {
					t.Fatalf("expected %q, got %v", test.message, err)
				}
			} else if err != nil || output.String() != test.want {
				t.Fatalf("got %q, %v; want %q", output.String(), err, test.want)
			}
		})
	}
}

func TestExtendedControlFlow(t *testing.T) {
	tests := []struct{ source, want string }{
		{`var count = 1; print(count++, ++count, count--, --count, count);`, "1 3 3 1 1\n"},
		{`var calls = 0; function index() { calls++; return 0; } const items = [4]; print(items[index()]++, ++items[index()], calls, items);`, "4 6 2 [6]\n"},
		{`const values = {"n": 2}; print(--values["n"], values["n"]--, values["n"]);`, "1 1 0\n"},
		{`var total = 0; cfor (var index = 0; index < 5; ++index) { if index == 2 { continue; } total = total + index; } else { print(total); }`, "8\n"},
		{`cfor (;;) { break; } else { print("wrong"); } while false {} else { print("empty"); } for item in [] {} else { print("done"); }`, "empty\ndone\n"},
		{`for item in [1, 2] { if item == 1 { continue; } break; } else { print("wrong"); } print("done");`, "done\n"},
		{`switch 2 { case 1: print("wrong"); break; case 2: print("two"); case 3: print("three"); break; default: print("wrong"); }`, "two\nthree\n"},
		{`switch 9 { case 1: break; default: print("fallback"); case 2: print("fallthrough"); }`, "fallback\nfallthrough\n"},
		{`for item in [1, 2, 3] { switch item { case 1: continue; case 2: break; default: print("three"); } print(item); } else { print("done"); }`, "2\nthree\n3\ndone\n"},
		{`function find() { cfor (;;) { switch 1 { case 1: return 7; } } else { return 0; } } print(find());`, "7\n"},
	}
	for _, test := range tests {
		t.Run(test.source, func(t *testing.T) {
			var output bytes.Buffer
			if err := Run(test.source, &output); err != nil || output.String() != test.want {
				t.Fatalf("got %q, %v; want %q", output.String(), err, test.want)
			}
		})
	}
	for _, source := range []string{`const count = 1; count++;`, `var text = "one"; ++text;`, `missing++;`, `cfor (var index = 0; index < 1; ++index) {} print(index);`} {
		if err := Run(source, nil); err == nil {
			t.Errorf("expected error: %s", source)
		}
	}
}

func TestRangeAndExceptions(t *testing.T) {
	tests := []struct{ source, want string }{
		{`print(range(4), range(2, 5), range(5, -1, -2), range(0), range(5, 1), range(1, 5, -1));`, "[0, 1, 2, 3] [2, 3, 4] [5, 3, 1] [] [] []\n"},
		{`var total = 0; for value in range(5) { total = total + value; } else { print(total); }`, "10\n"},
		{`function reject() { throw {"code": 7}; } try { reject(); } catch (error) { print(error["code"]); } print("resumed");`, "7\nresumed\n"},
		{`try { try { throw null; } catch (inner) { print(inner); throw "again"; } } catch (outer) { print(outer); }`, "null\nagain\n"},
		{`try { print(1 / 0); } catch (error) { print("division by zero" in error); }`, "true\n"},
		{`function result() { try { return 3; } catch (error) { return 0; } } print(result());`, "3\n"},
		{`function result() { try { throw false; } catch (error) { return error; } } print(result());`, "false\n"},
		{`for value in range(3) { try { if value == 1 { continue; } if value == 2 { break; } print(value); } catch (error) { print("wrong"); } } else { print("wrong"); }`, "0\n"},
		{`var count = 0; while count < 3 { try { throw count++; } catch (error) { if error == 1 { continue; } print(error); } } else { print("done"); }`, "0\n2\ndone\n"},
		{`try { missing = 1; } catch (error) { print("const or var" in error); }`, "true\n"},
	}
	for _, test := range tests {
		var output bytes.Buffer
		if err := Run(test.source, &output); err != nil || output.String() != test.want {
			t.Errorf("%s: got %q, %v; want %q", test.source, output.String(), err, test.want)
		}
	}
	for _, source := range []string{`range();`, `range(1, 2, 3, 4);`, `range(1.5);`, `range("3");`, `range(0, 3, 0);`, `range(1e20);`, `range(1000001);`, `throw "unhandled";`, `try { throw 1; } catch (error) {} print(error);`} {
		if err := Run(source, nil); err == nil {
			t.Errorf("expected error: %s", source)
		}
	}
	program, err := parser.Parse(`try { cfor (;;) {} } catch (error) { print("cannot catch limits"); }`)
	if err != nil {
		t.Fatal(err)
	}
	eval := New(nil)
	eval.MaxSteps = 20
	if err := eval.Execute(program); err == nil || !strings.Contains(err.Error(), "step limit") {
		t.Fatalf("expected uncatchable execution limit, got %v", err)
	}
}

func TestFormattedStrings(t *testing.T) {
	tests := []struct{ source, want string }{
		{`const name = "Tiger"; print(f"Hello, {name}! {2 + 3}");`, "Hello, Tiger! 5\n"},
		{`print(F'{{braces}} {true} {null} {range(3)}');`, "{braces} true null [0, 1, 2]\n"},
		{`var count = 0; print(f"{count++}:{++count}:{count}");`, "0:2:2\n"},
		{`const data = {"value": 7}; print(f"{data["value"]} {{ { {"key": 2}["key"] } }}");`, "7 { 2 }\n"},
		{`print(f"outer {f'inner {1 + 2}'}", f"", f'line\nnext');`, "outer inner 3  line\nnext\n"},
		{`function message(value) { return f"value={value}"; } const result = message(9); print(result + "!");`, "value=9!\n"},
		{`class Label { var name = "item"; function text(this) { return f"{this.name}"; } } print(Label().text());`, "item\n"},
		{`try { print(f"{missing}"); } catch (error) { print("undefined name" in error); }`, "true\n"},
		{`print(f"# // /* {1 /* } ignored */ + 2} */");`, "# // /* 3 */\n"},
	}
	for _, test := range tests {
		var output bytes.Buffer
		if err := Run(test.source, &output); err != nil || output.String() != test.want {
			t.Errorf("%s: got %q, %v; want %q", test.source, output.String(), err, test.want)
		}
	}
	for _, source := range []string{`print(f"{}");`, `print(f"}");`, `print(f"{1");`, `print(f"unterminated);`, `print(f"{1; 2}");`, `print(f"{1:.2f}");`, `print(f"{1!r}");`} {
		if err := Run(source, nil); err == nil {
			t.Errorf("expected error: %s", source)
		}
	}
	if err := Run("\nprint(f\"value {missing}\");", nil); err == nil || !strings.Contains(err.Error(), "2:16:") {
		t.Fatalf("expected interpolation source position, got %v", err)
	}
}

type brokenWriter struct{}

func (brokenWriter) Write([]byte) (int, error) { return 0, errors.New("output closed") }

func TestOutputError(t *testing.T) {
	if err := Run(`print("hello");`, brokenWriter{}); err == nil || !strings.Contains(err.Error(), "output closed") {
		t.Fatalf("expected output error, got %v", err)
	}
}
