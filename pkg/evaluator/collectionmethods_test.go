package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestListMethods(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"pop last", `var items = [1, 2, 3]; print(items.pop(), items, len(items));`, "3 [1, 2] 2\n"},
		{"pop index and negative", `var items = [1, 2, 3]; print(items.pop(0), items.pop(-1), items);`, "1 3 [2]\n"},
		{"remove first equal", `var items = [1, 2, 3, 2]; items.remove(2); print(items);`, "[1, 3, 2]\n"},
		{"remove structural", `var items = [[1], [2]]; items.remove([1]); print(items);`, "[[2]]\n"},
		{"insert", `var items = [1, 3]; items.insert(1, 2); print(items);`, "[1, 2, 3]\n"},
		{"insert clamps", `var items = [1, 2]; items.insert(-5, 0); items.insert(99, 9); print(items);`, "[0, 1, 2, 9]\n"},
		{"index", `print([1, 2, 3].index(2), [1, 2, 3].index(9), ["a", "b"].index("b"));`, "1 -1 1\n"},
		{"extend", `var items = [1]; items.extend([2, 3]); print(items, len(items));`, "[1, 2, 3] 3\n"},
		{"reverse", `var items = [1, 2, 3]; items.reverse(); print(items);`, "[3, 2, 1]\n"},
		{"clear", `var items = [1, 2, 3]; items.clear(); print(items, len(items));`, "[] 0\n"},
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

func TestDictMethods(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"keys values items", `const d = {"a": 1, "b": 2}; print(d.keys(), d.values(), d.items());`, "[\"a\", \"b\"] [1, 2] [[\"a\", 1], [\"b\", 2]]\n"},
		{"get", `const d = {"a": 1}; print(d.get("a"), d.get("z"), d.get("z", 0));`, "1 null 0\n"},
		{"pop", `var d = {"a": 1, "b": 2}; print(d.pop("a"), d);`, "1 {\"b\": 2}\n"},
		{"pop default", `print({"a": 1}.pop("z", 99));`, "99\n"},
		{"delete", `var d = {"a": 1, "b": 2}; print(d.delete("a"), d.delete("z"), d);`, "true false {\"b\": 2}\n"},
		{"delete preserves order", `var d = {"a": 1, "b": 2, "c": 3}; d.delete("b"); d["d"] = 4; print(d);`, "{\"a\": 1, \"c\": 3, \"d\": 4}\n"},
		{"clear", `var d = {"a": 1}; d.clear(); print(d, len(d));`, "{} 0\n"},
		{"keys iterate", `const d = {"x": 1, "y": 2}; var sum = 0; for k in d.keys() { sum += d[k]; } print(sum);`, "3\n"},
		{"items iterate", `for pair in {"a": 1, "b": 2}.items() { print(pair[0], pair[1]); }`, "a 1\nb 2\n"},
		{"keys fresh list", `const d = {"a": 1}; var ks = d.keys(); ks += ["b"]; print(d.keys(), ks);`, "[\"a\"] [\"a\", \"b\"]\n"},
		{"numeric and bool keys", `var d = {1: "one", true: "yes"}; print(d.get(1), d.get(true), d.get(false, "no"));`, "one yes no\n"},
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

func TestCollectionMethodErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`[].pop();`, "pop from empty list"},
		{`[1].pop(5);`, "pop index out of range"},
		{`[1].pop(0.5);`, "pop index must be an integer"},
		{`[1, 2].pop(0, 1);`, "pop expects at most 1 argument, got 2"},
		{`[1].remove(9);`, "remove: value not in list"},
		{`[1].insert(0.5, 2);`, "insert index must be an integer"},
		{`[1].insert(0);`, "insert expects 2 arguments, got 1"},
		{`[1].extend("x");`, "extend expects a list, got string"},
		{`[1].index();`, "index expects 1 argument, got 0"},
		{`[1].clear(1);`, "clear expects 0 arguments, got 1"},
		{`"x".pop();`, `string has no property "pop"`},
		{`{}.keys(1);`, "keys expects 0 arguments, got 1"},
		{`{}.values(x=1);`, "values does not accept keyword arguments"},
		{`{}.get();`, "get expects 1 or 2 arguments, got 0"},
		{`{}.pop("z");`, "pop: key not found: z"},
		{`{}.delete();`, "delete expects 1 argument, got 0"},
		{`{}.delete([]);`, "list cannot be a dictionary key"},
		{`{}.rate();`, `dict has no property "rate"`},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
	var output bytes.Buffer
	if err := Run(`try { [].pop(); } catch (error) { print("empty list" in error); }`, &output); err != nil || output.String() != "true\n" {
		t.Fatalf("got %q, %v", output.String(), err)
	}
}
