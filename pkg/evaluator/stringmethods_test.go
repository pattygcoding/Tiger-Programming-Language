package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestStringMethods(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"split separator", `print("a,b,c".split(","));`, "[\"a\", \"b\", \"c\"]\n"},
		{"split whitespace", `print("  a \t b\nc  ".split());`, "[\"a\", \"b\", \"c\"]\n"},
		{"split keeps empty fields", `print("a,,b".split(","));`, "[\"a\", \"\", \"b\"]\n"},
		{"split empty text", `print("".split(","), "   ".split());`, "[\"\"] []\n"},
		{"case", `print("Tiger".upper(), "Tiger".lower());`, "TIGER tiger\n"},
		{"trim", `print("  padded \t".trim(), "x");`, "padded x\n"},
		{"replace", `print("a-b-a".replace("a", "o"), "aaa".replace("a", "").size());`, "o-b-o 0\n"},
		{"affixes", `print("Tiger".startswith("Ti"), "Tiger".endswith("er"), "Tiger".startswith("er"));`, "true true false\n"},
		{"find", `print("Tiger".find("g"), "Tiger".find("z"), "Tiger".find(""));`, "2 -1 0\n"},
		{"find unicode", `print("café".find("é"), "café".find("f"), "café".size());`, "3 2 4\n"},
		{"pad", `print("7".padStart(3, "0"), "7".padEnd(3, "."), "ab".padStart(2), "ab".padEnd(4, "-"));`, "007 7.. ab ab--\n"},
		{"pad unicode width", `print("é".padEnd(3, "."));`, "é..\n"},
		{"join", `print(["a", "b", "c"].join("-"), ["x"].join("") + "y", [].join(",").size());`, "a-b-c xy 0\n"},
		{"chained methods", `print("a,b".split(",").join("-"));`, "a-b\n"},
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

func TestStringMethodErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`"abc".upper(1);`, "upper expects 0 arguments, got 1"},
		{`"abc".upper(x=1);`, "upper does not accept keyword arguments"},
		{`"abc".split(1);`, "split separator must be a string, got number"},
		{`"abc".split("");`, "split: empty separator"},
		{`"abc".split(",", "-");`, "split expects at most 1 argument, got 2"},
		{`"abc".replace("a");`, "replace expects 2 arguments, got 1"},
		{`"abc".replace(1, "b");`, "replace old must be a string, got number"},
		{`"abc".startswith(1);`, "startswith prefix must be a string, got number"},
		{`"abc".endswith(1);`, "endswith suffix must be a string, got number"},
		{`"abc".find(1);`, "find argument must be a string, got number"},
		{`"abc".padStart("3");`, "padStart width must be an integer"},
		{`"abc".padStart(-1);`, "padStart width must not be negative"},
		{`"abc".padEnd(2000000);`, "padEnd width must be at most 1000000"},
		{`"abc".padStart(3, "ab");`, "padStart fill must be a single character"},
		{`"abc".padStart(3, 1);`, "padStart fill must be a string, got number"},
		{`[1, "a"].join(",");`, "join expects a list of strings, got number at index 0"},
		{`["a"].join(1);`, "join separator must be a string, got number"},
		{`["a"].join();`, "join expects 1 argument, got 0"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
	var output bytes.Buffer
	if err := Run(`try { "abc".split(""); } catch (error) { print("empty separator" in error); }`, &output); err != nil || output.String() != "true\n" {
		t.Fatalf("got %q, %v", output.String(), err)
	}
}
