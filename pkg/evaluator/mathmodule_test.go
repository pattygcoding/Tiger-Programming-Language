package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestMathModule(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"ceil", `print(math.ceil(2.1), math.ceil(-2.9), math.ceil(-0.5), math.ceil(4));`, "3 -2 0 4\n"},
		{"floor", `print(math.floor(2.9), math.floor(-2.1), math.floor(7));`, "2 -3 7\n"},
		{"round half to even", `print(math.round(2.5), math.round(3.5), math.round(-2.5), math.round(-0.4), math.round(2.6), math.round(1.49));`, "2 4 -2 0 3 1\n"},
		{"sqrt", `print(math.sqrt(16), math.sqrt(2), math.sqrt(0));`, "4 1.4142135623730951 0\n"},
		{"abs", `print(math.abs(-3.5), math.abs(4), math.abs(-0));`, "3.5 4 0\n"},
		{"pow", `print(math.pow(2, 10), math.pow(9, 0.5), math.pow(2, -1), math.pow(0, 0), math.pow(-8, 3));`, "1024 3 0.5 1 -512\n"},
		{"log", `print(math.log(1), math.log(2), math.round(math.log(1000) / math.log(10)));`, "0 0.6931471805599453 3\n"},
		{"trig", `print(math.sin(0), math.cos(0), math.tan(0), math.sin(1));`, "0 1 0 0.8414709848078965\n"},
		{"min max", `print(math.min(3, -2), math.max(3, -2), math.min(1.5, 1.5), math.max(-1, -0.5));`, "-2 3 1.5 -0.5\n"},
		{"module value", `print(math);`, "<module math>\n"},
		{"functions are values", `const f = math.sqrt; print([1, 4, 9].length(), f(81));`, "3 9\n"},
		{"available in functions", `function hyp(a, b) { return math.sqrt(a * a + b * b); } print(hyp(3, 4));`, "5\n"},
		{"shadowed by user binding", `const math = {"pi": 3}; print(math["pi"]);`, "3\n"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var output bytes.Buffer
			if err := Run(test.source, &output); err != nil || output.String() != test.want {
				t.Fatalf("got %q, %v; want %q", output.String(), err, test.want)
			}
		})
	}
}

func TestMathModuleErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`math.sqrt(-1);`, "math.sqrt: math domain error"},
		{`math.log(0);`, "math.log: math domain error"},
		{`math.log(-2);`, "math.log: math domain error"},
		{`math.pow(0, -1);`, "math.pow: math domain error"},
		{`math.pow(-8, 0.5);`, "math.pow: math domain error"},
		{`math.pow(10, 400);`, "math.pow: math range error"},
		{`math.sqrt("4");`, "math.sqrt expects a number, got string"},
		{`math.max(1, true);`, "math.max expects a number, got bool"},
		{`math.abs();`, "math.abs expects 1 argument, got 0"},
		{`math.min(1);`, "math.min expects 2 arguments, got 1"},
		{`math.floor(1.5, 2);`, "math.floor expects 1 argument, got 2"},
		{`math.round(x=1.5);`, "math.round does not accept keyword arguments"},
		{`math.pi;`, `module "math" has no export "pi"`},
		{`math.sqrt = 1;`, "cannot assign a property of module"},
		{`math = 1;`, "math"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}
