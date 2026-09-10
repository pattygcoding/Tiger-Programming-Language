package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestCLI(t *testing.T) {
	directory := t.TempDir()
	valid := filepath.Join(directory, "valid.tg")
	invalid := filepath.Join(directory, "invalid.tg")
	for path, source := range map[string]string{valid: `print("hello");`, invalid: `print("hello")`} {
		if err := os.WriteFile(path, []byte(source), 0600); err != nil {
			t.Fatal(err)
		}
	}
	tests := []struct {
		args     []string
		code     int
		out, err string
	}{
		{[]string{"help"}, 0, "Usage:", ""},
		{nil, 2, "", "Usage:"},
		{[]string{"unknown", valid}, 2, "", "Usage:"},
		{[]string{"run", valid}, 0, "hello\n", ""},
		{[]string{"run", invalid}, 1, "", invalid + ":1:"},
		{[]string{"run", "missing.tg"}, 1, "", "missing.tg"},
		{[]string{"run", "wrong.py"}, 2, "", ".tg"},
		{[]string{"build", valid, "-o"}, 2, "", "output path"},
		{[]string{"build", valid, "-o", valid}, 2, "", "different from the input"},
		{[]string{"run", valid, "extra"}, 2, "", "Usage:"},
	}
	for _, test := range tests {
		t.Run(strings.Join(test.args, " "), func(t *testing.T) {
			var stdout, stderr bytes.Buffer
			code := run(test.args, &stdout, &stderr)
			if code != test.code || !strings.Contains(stdout.String(), test.out) || !strings.Contains(stderr.String(), test.err) {
				t.Fatalf("code=%d stdout=%q stderr=%q", code, stdout.String(), stderr.String())
			}
		})
	}
}
