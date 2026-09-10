package benchmarks_test

import (
	"bytes"
	"embed"
	"io/fs"
	"strings"
	"testing"
	"tiger/pkg/evaluator"
)

//go:embed *.tg *.txt
var fixtures embed.FS

func TestCorpus(t *testing.T) {
	sources, err := fs.Glob(fixtures, "*.tg")
	if err != nil {
		t.Fatal(err)
	}
	outputs, err := fs.Glob(fixtures, "*.txt")
	if err != nil {
		t.Fatal(err)
	}
	if len(sources) != 10 || len(outputs) != 10 {
		t.Fatalf("want 10 Tiger programs and 10 expected outputs, got %d and %d", len(sources), len(outputs))
	}
	for _, output := range outputs {
		if _, err := fixtures.ReadFile(strings.TrimSuffix(output, ".txt") + ".tg"); err != nil {
			t.Errorf("orphan expected output %s: %v", output, err)
		}
	}
}

func TestPrograms(t *testing.T) {
	paths, err := fs.Glob(fixtures, "*.tg")
	if err != nil {
		t.Fatal(err)
	}
	if len(paths) == 0 {
		t.Fatal("no Tiger programs found")
	}
	for _, path := range paths {
		t.Run(strings.TrimSuffix(path, ".tg"), func(t *testing.T) {
			source, err := fixtures.ReadFile(path)
			if err != nil {
				t.Fatal(err)
			}
			text := normalize(string(source))
			lines := strings.Count(strings.TrimSuffix(text, "\n"), "\n") + 1
			if lines < 100 || lines > 200 {
				t.Fatalf("%s has %d lines; want 100-200", path, lines)
			}
			expectedPath := strings.TrimSuffix(path, ".tg") + ".txt"
			expected, err := fixtures.ReadFile(expectedPath)
			if err != nil {
				t.Fatal(err)
			}
			var output bytes.Buffer
			if err := evaluator.Run(text, &output); err != nil {
				t.Fatalf("%s:%v", path, err)
			}
			if actual, want := normalize(output.String()), normalize(string(expected)); actual != want {
				t.Fatalf("output differs from %s\nwant:\n%s\ngot:\n%s", expectedPath, want, actual)
			}
			t.Logf("%d lines; output matches %s", lines, expectedPath)
		})
	}
}

func normalize(text string) string {
	return strings.ReplaceAll(text, "\r\n", "\n")
}
