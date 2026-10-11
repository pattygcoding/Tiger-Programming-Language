//go:build !js

package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestRegexModule(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"match", `print(regex.match("^a.c$", "abc"), regex.match("z", "abc"));`, "true false\n"},
		{"find", `print(regex.find("\\d+", "ab 123 cd"), regex.find("z", "abc"));`, "123 null\n"},
		{"findAll", `print(regex.findAll("\\d+", "1 22 333"));`, "[\"1\", \"22\", \"333\"]\n"},
		{"groups", `print(regex.groups("(\\w+)@(\\w+)", "user@host"));`, "[\"user\", \"host\"]\n"},
		{"replace", `print(regex.replace("\\d", "a1b2", "#"));`, "a#b#\n"},
		{"split", `print(regex.split("\\s+", "a b  c"));`, "[\"a\", \"b\", \"c\"]\n"},
		{"no match groups", `print(regex.groups("z", "abc"));`, "null\n"},
		{"chained", `print(regex.findAll("\\w+", regex.replace(" ", "a b c", "-")));`, "[\"a\", \"b\", \"c\"]\n"},
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

func TestRegexModuleErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`regex.match("[", "x");`, "invalid pattern"},
		{`regex.match(1, "x");`, "string arguments"},
		{`regex.match("a");`, "expects 2 arguments"},
		{`regex.replace("a", "b");`, "expects 3 arguments"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}
