package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestDefaultParameters(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"basic", `function greet(name, greeting = "hello") { return greeting + " " + name; } print(greet("world"), greet("world", "hi"), greet(name="ace"));`, "hello world hi world hello ace\n"},
		{"several", `function add(a, b = 10, c = 100) { return a + b + c; } print(add(1), add(1, 2), add(1, 2, 3), add(1, c=0));`, "111 103 6 11\n"},
		{"closure default", `const base = 5; function scale(value, factor = base) { return value * factor; } print(scale(3), scale(3, 2));`, "15 6\n"},
		{"method default", `class Counter { var step = 1; function bump(amount = 2) { return amount + this.step; } } print(Counter().bump(), Counter().bump(10));`, "3 11\n"},
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

func TestAnonymousFunctions(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"assigned", `const double = function(value) { return value * 2; }; print(double(4));`, "8\n"},
		{"sort key", `const words = ["ccc", "a", "bb"]; words.sort(key=function(word) { return len(word); }); print(words);`, "[\"a\", \"bb\", \"ccc\"]\n"},
		{"immediate", `print(function(a, b = 2) { return a + b; }(3));`, "5\n"},
		{"closure", `function make_adder(amount) { return function(value) { return value + amount; }; } const add5 = make_adder(5); print(add5(3));`, "8\n"},
		{"higher order", `function apply_all(values, action) { var result = []; for value in values { result = result + [action(value)]; } return result; } print(apply_all([1, 2, 3], function(value) { return value * value; }));`, "[1, 4, 9]\n"},
		{"formatting", `print(function() { return 1; });`, "<function anonymous>\n"},
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

func TestMultipleReturnsAndDestructuring(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"multiple return", `function min_max(values) { var low = values[0]; var high = values[0]; for value in values { if value < low { low = value; } if value > high { high = value; } } return low, high; } const [low, high] = min_max([3, 1, 4, 1, 5]); print(low, high, min_max([2, 9]));`, "1 5 [2, 9]\n"},
		{"swap", `function swap(a, b) { return b, a; } var [first, second] = swap(1, 2); print(first, second);`, "2 1\n"},
		{"list destructuring", `const [head, tail] = [1, 2]; print(head, tail);`, "1 2\n"},
		{"dict destructuring", `const record = {"name": "tiger", "legs": 4}; const {name, legs} = record; print(name, legs);`, "tiger 4\n"},
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

func TestFunctionErgonomicsErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`function f(a = 1, b) { return a; }`, "a required parameter cannot follow a defaulted parameter"},
		{`function f(a) { return a; } f();`, "expects 1 arguments, got 0"},
		{`print(function(a) { return a; }());`, "anonymous function expects 1 arguments, got 0"},
		{`var [a, b] = [1];`, "cannot unpack 1 values into 2 names"},
		{`var [a] = 5;`, "expected a list"},
		{`var {a} = [1];`, "expected a dictionary"},
		{`var {a} = {"b": 1};`, "missing key"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}
