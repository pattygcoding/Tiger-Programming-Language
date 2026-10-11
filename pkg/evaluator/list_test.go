package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func runList(t *testing.T, source string) string {
	t.Helper()
	var output bytes.Buffer
	if err := RunWithOptions(source, "main.tg", nil, NewMemoryFileSystem(), &output); err != nil {
		t.Fatal(err)
	}
	return output.String()
}

func TestListDir(t *testing.T) {
	source := `write_file("data/a.txt", "1");
write_file("data/b.txt", "2");
write_file("root.txt", "3");
print(list_dir("data"));
print(list_dir("."));`
	if got, want := runList(t, source), "[\"a.txt\", \"b.txt\"]\n[\"root.txt\"]\n"; got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestListDirErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`list_dir("missing");`, "file does not exist"},
		{`list_dir();`, "expects 1 argument"},
		{`list_dir(1);`, "expects a path string"},
	} {
		err := RunWithOptions(test.source, "main.tg", nil, NewMemoryFileSystem(), nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}
