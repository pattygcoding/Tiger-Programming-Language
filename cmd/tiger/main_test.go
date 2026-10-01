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
	greet := filepath.Join(directory, "greet.tg")
	for path, source := range map[string]string{valid: `print("hello");`, invalid: `print("hello")`, greet: `print("hi " + input("name: "));`} {
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
		{[]string{"run", greet}, 0, "name: hi Ada\n", ""},
	}
	for _, test := range tests {
		t.Run(strings.Join(test.args, " "), func(t *testing.T) {
			var stdout, stderr bytes.Buffer
			code := run(test.args, strings.NewReader("Ada\n"), &stdout, &stderr)
			if code != test.code || !strings.Contains(stdout.String(), test.out) || !strings.Contains(stderr.String(), test.err) {
				t.Fatalf("code=%d stdout=%q stderr=%q", code, stdout.String(), stderr.String())
			}
		})
	}
}

func TestExamplePrograms(t *testing.T) {
	for _, directory := range []string{"examples", "portfolio-features"} {
		paths, err := filepath.Glob(filepath.Join("..", "..", directory, "*.tg"))
		if err != nil || len(paths) == 0 {
			t.Fatalf("find %s programs: %v", directory, err)
		}
		for _, path := range paths {
			t.Run(directory+"/"+filepath.Base(path), func(t *testing.T) {
				var stdout, stderr bytes.Buffer
				if code := run([]string{"run", path}, strings.NewReader(""), &stdout, &stderr); code != 0 {
					t.Fatalf("exit %d: %s", code, stderr.String())
				}
				if stdout.Len() == 0 || stderr.Len() != 0 {
					t.Fatalf("stdout=%q stderr=%q", stdout.String(), stderr.String())
				}
			})
		}
	}
}

func TestPortfolioInteractivePrograms(t *testing.T) {
	tests := []struct {
		program, input string
		want           []string
	}{
		{"interactive_quiz.tg", "", []string{"What is your name? Ada\n", "Ada scored 5/6 (83%), best streak 5, grade B\n"}},
		{"interactive_quiz.tg", "Bob\nvar\n", []string{"Welcome, Bob!", "Q2. What does 7 / 2 evaluate to? 3.5\n", "Bob scored 5/6 (83%)"}},
		{"connect_four.tg", "1\n2\n1\n2\n1\n2\n1\n", []string{"Player X wins!\n"}},
		{"connect_four.tg", "1\n1\n1\n1\n1\n1\n1\n0\n8\nabc\n", []string{"Column 1 is full.", "\"0\" is out of range", "\"8\" is out of range", "\"abc\" is not a whole number", "Input closed. Goodbye.\n"}},
	}
	for _, test := range tests {
		var stdout, stderr bytes.Buffer
		path := filepath.Join("..", "..", "portfolio-features", test.program)
		if code := run([]string{"run", path}, strings.NewReader(test.input), &stdout, &stderr); code != 0 {
			t.Fatalf("%s: exit %d: %s", test.program, code, stderr.String())
		}
		for _, want := range test.want {
			if !strings.Contains(stdout.String(), want) {
				t.Errorf("%s with input %q: missing %q in:\n%s", test.program, test.input, want, stdout.String())
			}
		}
	}
}

func TestCLIImports(t *testing.T) {
	directory := t.TempDir()
	if err := os.Mkdir(filepath.Join(directory, "lib"), 0700); err != nil {
		t.Fatal(err)
	}
	for name, source := range map[string]string{
		"main.tg":     `import "lib/math.tg" as math; print(math.square(9));`,
		"lib/math.tg": `function square(value) { return value * value; }`,
	} {
		if err := os.WriteFile(filepath.Join(directory, name), []byte(source), 0600); err != nil {
			t.Fatal(err)
		}
	}
	var stdout, stderr bytes.Buffer
	if code := run([]string{"run", filepath.Join(directory, "main.tg")}, strings.NewReader(""), &stdout, &stderr); code != 0 || stdout.String() != "81\n" {
		t.Fatalf("exit=%d output=%q error=%q", code, stdout.String(), stderr.String())
	}
}
