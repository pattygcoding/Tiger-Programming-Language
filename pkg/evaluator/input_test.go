package evaluator

import (
	"bytes"
	"errors"
	"strings"
	"testing"
)

func runInput(source, input string) (string, error) {
	var output bytes.Buffer
	err := RunWithInput(source, "main.tg", nil, NewMemoryFileSystem(), strings.NewReader(input), &output)
	return output.String(), err
}

func TestInput(t *testing.T) {
	tests := []struct{ name, source, input, want string }{
		{"prompt and line", `const name = input("Name: "); print("Hello, " + name + "!");`, "Ada\n", "Name: Hello, Ada!\n"},
		{"no prompt", `print(input(), input());`, "one\ntwo\n", "one two\n"},
		{"crlf stripped", `print(len(input()));`, "abc\r\n", "3\n"},
		{"last line without newline", `print(input(), input());`, "first\nlast", "first last\n"},
		{"empty line", `const line = input(); print(line == "", len(line));`, "\n", "true 0\n"},
		{"non-string prompt", `input(42); print();`, "x\n", "42\n"},
		{"returns string", `const value = input(); print(value + value, str(1) + value);`, "7\n", "77 17\n"},
		{"unicode", `print(input().size());`, "t\u00edgre\n", "5\n"},
		{"read loop", `var total = 0; while true { try { total += len(input()); } catch (error) { break; } } print(total);`, "ab\ncd\nefg\n", "7\n"},
		{"eof catchable", `try { input(); } catch (error) { print("EOF" in error); }`, "", "true\n"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := runInput(test.source, test.input)
			if err != nil || got != test.want {
				t.Fatalf("got %q, %v; want %q", got, err, test.want)
			}
		})
	}
}

type failingReader struct{}

func (failingReader) Read([]byte) (int, error) { return 0, errors.New("broken stdin") }

func TestInputErrors(t *testing.T) {
	for _, test := range []struct{ source, input, message string }{
		{`input();`, "", "EOF when reading a line"},
		{`input("a", "b");`, "x\n", "input expects at most 1 argument, got 2"},
		{`input(prompt="a");`, "x\n", "input does not accept keyword arguments"},
	} {
		if _, err := runInput(test.source, test.input); err == nil || !strings.Contains(err.Error(), test.message) {
			t.Errorf("%s: expected %q, got %v", test.source, test.message, err)
		}
	}
	if err := Run(`input();`, nil); err == nil || !strings.Contains(err.Error(), "no input stream is connected") {
		t.Errorf("expected missing input stream error, got %v", err)
	}
	var output bytes.Buffer
	err := RunWithInput(`input();`, "main.tg", nil, nil, failingReader{}, &output)
	if err == nil || !strings.Contains(err.Error(), "broken stdin") {
		t.Errorf("expected reader error, got %v", err)
	}
}
