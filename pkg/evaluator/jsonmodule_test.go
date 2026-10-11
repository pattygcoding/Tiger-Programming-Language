package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestJSONModule(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"parse object", `const value = json.parse("{\"a\": 1, \"b\": [true, null, \"x\"]}"); print(value["a"], value["b"]);`, "1 [true, null, \"x\"]\n"},
		{"stringify object", `print(json.stringify(json.parse("{\"n\": 1.5, \"ok\": true}")));`, "{\"n\":1.5,\"ok\":true}\n"},
		{"stringify numbers", `print(json.stringify(json.parse("[1, 2.5, -3, 1e2]")));`, "[1,2.5,-3,100]\n"},
		{"nested literal", `print(json.stringify({"k": [1, {"n": null}]}));`, "{\"k\":[1,{\"n\":null}]}\n"},
		{"empty containers", `print(json.stringify(json.parse("{}")), json.stringify(json.parse("[]")));`, "{} []\n"},
		{"escaping", `print(json.stringify("line\nbreak"));`, "\"line\\nbreak\"\n"},
		{"unicode escape", `print(json.parse("\"\\u0041\\u00e9\""));`, "A\u00e9\n"},
		{"emoji", `print(json.stringify("😀"));`, "\"😀\"\n"},
		{"scalars", `print(json.parse("true"), json.parse("false"), json.parse("null"), json.parse("42"));`, "true false null 42\n"},
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

func TestJSONModuleErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`json.parse("{bad}");`, "json.parse"},
		{`json.parse("[1,]");`, "json.parse"},
		{`json.parse("1 2");`, "trailing content"},
		{`json.parse("");`, "unexpected end of input"},
		{`json.parse(1);`, "expects a string"},
		{`json.parse();`, "expects 1 argument"},
		{`json.stringify(print);`, "cannot serialize"},
		{`json.stringify({1: "x"});`, "requires string keys"},
		{`const a = [null]; a[0] = a; json.stringify(a);`, "cyclic structure"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}
