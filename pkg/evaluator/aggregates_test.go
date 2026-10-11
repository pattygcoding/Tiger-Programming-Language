package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestAggregateBuiltins(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"sum", `print(sum([1, 2, 3]), sum([]), sum([4, 8, 15, 16, 23, 42]), sum([1.5, 2.5]));`, "6 0 108 4\n"},
		{"min max list", `print(min([4, 8, 15, 16, 23, 42]), max([4, 8, 15, 16, 23, 42]));`, "4 42\n"},
		{"min max arguments", `print(min(3, 1, 2), max(3, 1, 2), min([2.5, 2.25]));`, "1 3 2.25\n"},
		{"min max strings", `print(min(["pear", "apple"]), max(["pear", "apple"]));`, "apple pear\n"},
		{"any all", `print(any([0, "", 3]), any([0, ""]), all([1, "x", true]), all([1, 0]), any([]), all([]));`, "true false true false false true\n"},
		{"enumerate", `print(enumerate(["a", "b"]));`, "[[0, \"a\"], [1, \"b\"]]\n"},
		{"enumerate loop", `for pair in enumerate(["x", "y"]) { print(pair[0], pair[1]); }`, "0 x\n1 y\n"},
		{"zip", `print(zip([1, 2, 3], ["a", "b"]), zip([1, 2], [3, 4], [5, 6]));`, "[[1, \"a\"], [2, \"b\"]] [[1, 3, 5], [2, 4, 6]]\n"},
		{"map", `print(map([1, 2, 3], function(v) { return v * 2; }), map(["a", "bb"], len));`, "[2, 4, 6] [1, 2]\n"},
		{"filter", `print(filter([1, 2, 3, 4], function(v) { return v % 2 == 0; }), filter([0, "", 5, null, "x"], function(v) { return v; }));`, "[2, 4] [5, \"x\"]\n"},
		{"sorted leaves the original", `const values = [3, 1, 2]; const ordered = sorted(values); print(ordered, values);`, "[1, 2, 3] [3, 1, 2]\n"},
		{"sorted is a new list", `const a = [2, 1]; const b = sorted(a); b[0] = 99; print(a, b);`, "[2, 1] [99, 2]\n"},
		{"sorted reverse", `print(sorted([3, 1, 2], reverse=true), sorted(["pear", "apple", "fig"]));`, "[3, 2, 1] [\"apple\", \"fig\", \"pear\"]\n"},
		{"sorted key", `print(sorted(["ccc", "a", "bb"], key=len), sorted(["ccc", "a", "bb"], key=function(w) { return len(w); }, reverse=true));`, "[\"a\", \"bb\", \"ccc\"] [\"ccc\", \"bb\", \"a\"]\n"},
		{"builtins are values", `print(type(map), isinstance(sum, "builtin"), isinstance(sorted, "builtin"));`, "builtin true true\n"},
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

func TestAggregateBuiltinErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`sum(1);`, "expects a list"},
		{`sum([1, "x"]);`, "expects numbers"},
		{`min([]);`, "of an empty sequence"},
		{`min([1, "a"]);`, "cannot compare"},
		{`sorted([1, "a"]);`, "cannot compare"},
		{`any(1);`, "expects a list"},
		{`all();`, "expects 1 argument"},
		{`zip();`, "expects at least 1 argument"},
		{`zip([1], 2);`, "expects lists"},
		{`map([1]);`, "expects 2 arguments"},
		{`map([1], 2);`, "not callable"},
		{`filter("x", len);`, "expects a list"},
		{`sorted([1, 2], reverse=1.5);`, "must be a boolean or integer"},
		{`sorted([1, 2], bogus=1);`, "does not accept keyword argument"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}
