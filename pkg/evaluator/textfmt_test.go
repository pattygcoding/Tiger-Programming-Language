package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestFormatSpecs(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"precision", `print(f"{1.23456:.2f}", f"{2:.0f}", f"{2.5:.3f}");`, "1.23 2 2.500\n"},
		{"width and alignment", `print(f"[{7:>4}]", f"[{7:<4}]", f"[{7:^5}]", f"[{7:4}]");`, "[   7] [7   ] [  7  ] [   7]\n"},
		{"string default alignment", `print(f"[{'ab':<4}]", f"[{'ab':4}]", f"[{'ab':>4}]");`, "[ab  ] [ab  ] [  ab]\n"},
		{"zero padding", `print(f"{3.5:08.2f}", f"{-3.5:07.2f}", f"{42:04}");`, "00003.50 -003.50 0042\n"},
		{"combined", `print(f"{255:>6}", f"{1.5:>8.2f}", f"{'x':^5}");`, "   255     1.50   x  \n"},
		{"plain field", `print(f"{1 + 1}", f"{'a'}");`, "2 a\n"},
		{"nested expression", `print(f"{len('hello'):>3}", f"{[1, 2][1]:02}");`, "  5 02\n"},
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

func TestStringEscapesAndRawStrings(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"unicode escape", `print("\u0041\u00e9\u4e2d");`, "A\u00e9\u4e2d\n"},
		{"hex escape", `print("\x41\x42");`, "AB\n"},
		{"long escape", `print("\U0001F600");`, "\U0001F600\n"},
		{"escape in f-string", `print(f"A\u0041[{1}]");`, "AA[1]\n"},
		{"raw string", `print(r"a\nb", len(r"a\nb"));`, "a\\nb 4\n"},
		{"raw regex pattern", `print(regex.find(r"\d+", "order 42"));`, "42\n"},
		{"raw keeps quotes", `print(r'he said "hi"');`, "he said \"hi\"\n"},
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

func TestGraphemes(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"ascii", `print(graphemes("abc"));`, "[\"a\", \"b\", \"c\"]\n"},
		{"combining accent", `print(graphemes("e\u0301"), len("e\u0301"));`, "[\"e\u0301\"] 2\n"},
		{"emoji with modifier", `print(graphemes("\U0001F44D\U0001F3FB"));`, "[\"\U0001F44D\U0001F3FB\"]\n"},
		{"flag", `print(graphemes("\U0001F1FA\U0001F1F8"));`, "[\"\U0001F1FA\U0001F1F8\"]\n"},
		{"zwj sequence", `print(graphemes("\U0001F468\u200D\U0001F469\u200D\U0001F467"));`, "[\"\U0001F468\\u200d\U0001F469\\u200d\U0001F467\"]\n"},
		{"mixed", `print(len(graphemes("e\u0301x")), graphemes(""));`, "2 []\n"},
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

func TestTextPrecisionErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`print(f"{1:.f}");`, "expected digits after '.'"},
		{`print(f"{1:.2}");`, "only '.Nf' precision is supported"},
		{`print(f"{1:.20f}");`, "at most 17"},
		{`print(f"{'x':.2f}");`, "requires a number"},
		{`print(f"{1:d}");`, "invalid format spec"},
		{`print(f"{1:.2f.3}");`, "invalid format spec"},
		{`print("\x4");`, "invalid hexadecimal escape"},
		{`print("\uD800");`, "not a valid character"},
		{`print(r"unterminated);`, "unterminated raw string"},
		{`graphemes(1);`, "expects a string"},
		{`graphemes();`, "expects 1 argument"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}
