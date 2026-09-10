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
def factorial(n) { if n <= 1 { return 1; } return n * factorial(n - 1); }
i = 1; while i <= MAX_RUNS { val = factorial(i); print("factorial(" + str(i) + ") = " + str(val)); i = i + 1; }`,
			"factorial(1) = 1\nfactorial(2) = 2\nfactorial(3) = 6\nfactorial(4) = 24\nfactorial(5) = 120\n"},
		{"branches", `for x in [-1, 0, 1] { if x > 0 { print("Positive"); } elif x == 0 { print("Zero"); } else { print("Negative"); } }`, "Negative\nZero\nPositive\n"},
		{"collections", `items = [1, 2, "apple", true]; items[0] = 9; config = {"port": 8080, "host": "localhost"}; config["port"] = 80; print(items[-1], config["port"], len(items)); for key in config { print(key); }`, "true 80 4\nport\nhost\n"},
		{"closures", `def counter() { count = 0; def next() { count = count + 1; return count; } return next; } next = counter(); print(next(), next());`, "1 2\n"},
		{"scope updates", `count = 0; while count < 3 { const local = 1; count = count + local; } for item in [1, 2] { count = count + item; } print(count);`, "6\n"},
		{"parameter scope", `const x = 3; def add(x) { x = x + 1; return x; } print(add(9), x);`, "10 3\n"},
		{"return propagation", `def find() { for item in [1, 2, 3] { while true { if item == 2 { return item; } return 1; } } } def empty() {} def explicit() { return; } print(find(), empty(), explicit());`, "1 null null\n"},
		{"operators", `print(1 + 2 * 3, (1 + 2) * 3, -5 % 3, 5 % -3, 7 / 2); print(not 1 == 2, false and missing, true or missing, "a" < "b");`, "7 9 1 -1 3.5\ntrue false true true\n"},
		{"membership", `print(2 in [1, 2], "port" in {"port": 80}, "ell" in "hello", [] == [], {"a": 1} == {"a": 1});`, "true true true true true\n"},
		{"cycles", `items = [null]; items[0] = items; other = [null]; other[0] = other; print(items, items == other); config = {}; config["self"] = config; print(config);`, "[[...]] true\n{\"self\": {...}}\n"},
		{"const binding", `const items = [1]; items[0] = 2; print(items);`, "[2]\n"},
		{"dict key types", `config = {1: "number", "1": "string", true: "bool", -0: "zero"}; print(config[1], config["1"], config[true], config[0]);`, "number string bool zero\n"},
		{"string iteration", `for char in "hi" { print(char); } print("hello"[-1], len("hello"));`, "h\ni\no 5\n"},
		{"list concat", `left = [1]; both = left + [2]; both[0] = 9; print(left, both);`, "[1] [9, 2]\n"},
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
		{`const PI = 3; def change() { PI = 4; } change();`, "cannot reassign const"},
		{`const PI = 3; const PI = 4;`, "already declared"},
		{`if true { local = 1; } print(local);`, "undefined name"},
		{`for item in [1] {} print(item);`, "undefined name"},
		{`def local() { hidden = 1; } local(); print(hidden);`, "undefined name"},
		{`print(1 / 0);`, "division by zero"},
		{`print(1 % 0);`, "division by zero"},
		{`print(1 + "x");`, "not supported"},
		{`def add(a, b) { return a + b; } add(1);`, "expects 2 arguments"},
		{`str();`, "expects 1 argument"},
		{`len(1);`, "does not accept number"},
		{`1();`, "not callable"},
		{`print([1][2]);`, "index out of range"},
		{`print([1][0.5]);`, "index must be an integer"},
		{`print({}["missing"]);`, "key not found"},
		{`config = {[]: 1};`, "cannot be a dictionary key"},
		{`for item in 1 {}`, "not iterable"},
		{`text = "hi"; text[0] = "a";`, "cannot assign an index"},
		{`print(1e308 * 1e308);`, "non-finite"},
		{`def forever() { forever(); } forever();`, "maximum evaluation depth"},
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

type brokenWriter struct{}

func (brokenWriter) Write([]byte) (int, error) { return 0, errors.New("output closed") }

func TestOutputError(t *testing.T) {
	if err := Run(`print("hello");`, brokenWriter{}); err == nil || !strings.Contains(err.Error(), "output closed") {
		t.Fatalf("expected output error, got %v", err)
	}
}
