package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestTypeBuiltin(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"scalars", `print(type(1), type("x"), type(true), type(null));`, "number string bool null\n"},
		{"collections", `print(type([]), type({}), type([1]));`, "list dict list\n"},
		{"callables", `print(type(print), type(math.sqrt)); function f() {} print(type(f));`, "builtin builtin\nfunction\n"},
		{"classes and instances", `class Box { function m() {} } print(type(Box), type(Box()), type(Box().m));`, "class instance bound method\n"},
		{"modules", `print(type(math), type(algo));`, "module module\n"},
		{"inside f-string", `print(f"a {type(3)} b");`, "a number b\n"},
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

func TestIsInstance(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"scalars", `print(isinstance(1, "number"), isinstance("x", "number"), isinstance(true, "bool"), isinstance(null, "null"));`, "true false true true\n"},
		{"collections", `print(isinstance([], "list"), isinstance({}, "dict"), isinstance("", "list"));`, "true true false\n"},
		{"list of names", `print(isinstance([], ["string", "list"]), isinstance(1, ["string", "dict"]), isinstance(null, []));`, "true false false\n"},
		{"callables and types", `class Box {} function f() {} print(isinstance(print, "builtin"), isinstance(f, "function"), isinstance(Box, "class"), isinstance(Box(), "instance"), isinstance(math, "module"));`, "true true true true true\n"},
		{"bound method", `class Box { function m() {} } print(isinstance(Box().m, "bound method"), isinstance(Box().m, "function"));`, "true false\n"},
		{"dispatch", `function label(value) { if isinstance(value, ["number", "bool"]) { return "numeric"; } if isinstance(value, "string") { return "text"; } return "other"; } print(label(1), label(true), label("x"), label([]), label(null));`, "numeric numeric text other other\n"},
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

func TestTypeInspectionErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`type();`, "type expects 1 argument, got 0"},
		{`type(1, 2);`, "type expects 1 argument, got 2"},
		{`type(value=1);`, "type does not accept keyword arguments"},
		{`isinstance(1);`, "isinstance expects 2 arguments, got 1"},
		{`isinstance(1, 2);`, "isinstance: expected a string or list of strings, got number"},
		{`isinstance(1, "integer");`, `isinstance: unknown type name "integer"`},
		{`isinstance(1, ["number", 2]);`, "isinstance: type name must be a string, got number"},
		{`isinstance(1, ["number", "integer"]);`, `isinstance: unknown type name "integer"`},
		{`isinstance(value=1);`, "isinstance does not accept keyword arguments"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
	var output bytes.Buffer
	if err := Run(`try { type(); } catch (error) { print("expects 1 argument" in error); }`, &output); err != nil || output.String() != "true\n" {
		t.Fatalf("got %q, %v", output.String(), err)
	}
}
