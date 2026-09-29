package evaluator

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestParseFileMode(t *testing.T) {
	valid := map[string]FileMode{
		"r":   {Read: true},
		"rb":  {Read: true, Binary: true},
		"w":   {Write: true, Truncate: true, Create: true},
		"wb":  {Write: true, Truncate: true, Create: true, Binary: true},
		"a":   {Write: true, Append: true, Create: true},
		"ab":  {Write: true, Append: true, Create: true, Binary: true},
		"r+":  {Read: true, Write: true},
		"r+b": {Read: true, Write: true, Binary: true},
		"rb+": {Read: true, Write: true, Binary: true},
		"w+":  {Read: true, Write: true, Truncate: true, Create: true},
		"a+":  {Read: true, Write: true, Append: true, Create: true},
	}
	for mode, want := range valid {
		got, err := ParseFileMode(mode)
		if err != nil {
			t.Fatalf("ParseFileMode(%q): %v", mode, err)
		}
		if got != want {
			t.Errorf("ParseFileMode(%q) = %+v, want %+v", mode, got, want)
		}
	}
	for _, mode := range []string{"", "q", "rw", "r++", "rbb", "br"} {
		if _, err := ParseFileMode(mode); err == nil {
			t.Errorf("ParseFileMode(%q) unexpectedly succeeded", mode)
		} else if !strings.Contains(err.Error(), "invalid mode") {
			t.Errorf("ParseFileMode(%q) = %v", mode, err)
		}
	}
}

func runFiles(t *testing.T, source string) string {
	t.Helper()
	var output bytes.Buffer
	if err := RunWithOptions(source, "main.tg", nil, NewMemoryFileSystem(), &output); err != nil {
		t.Fatal(err)
	}
	return output.String()
}

func TestFileOperations(t *testing.T) {
	source := `const PATH = "data.txt";
print(write_file(PATH, "alpha\nbeta\n"));
print(file_exists(PATH), file_exists("missing.txt"));
print(append_file(PATH, "gamma\n"));
print(read_file(PATH), end="");
const reader = open(PATH, "r");
print(reader.name, reader.mode, reader.closed);
print(reader.readline(), end="");
print(reader.readline(), end="");
print(reader.readlines());
print(reader.closed);
reader.close();
print(reader.closed);
const probe = open(PATH, "r");
print(probe.tell());
print(probe.read(5));
print(probe.tell());
print(probe.seek(6));
print(probe.tell());
probe.close();
const writer = open(PATH, "w");
print(writer.write("fresh\n"));
writer.writelines(["one\n", "two\n"]);
writer.close();
print(read_file(PATH), end="");
const update = open(PATH, "r+");
print(update.read(), end="");
update.seek(0);
update.write("FRESH\n");
update.close();
print(read_file(PATH), end="");
const binary = open("raw.bin", "wb");
binary.write("PQRS");
binary.close();
print(open("raw.bin", "rb").read());
remove_file("raw.bin");
print(file_exists("raw.bin"));
remove_file(PATH);
print(file_exists(PATH));
print(open(PATH, "w").closed);
`
	want := `11
true false
6
alpha
beta
gamma
data.txt r false
alpha
beta
["gamma\n"]
false
true
0
alpha
5
6
6
6
fresh
one
two
fresh
one
two
FRESH
one
two
PQRS
false
false
false
`
	if got := runFiles(t, source); got != want {
		t.Fatalf("got:\n%s\nwant:\n%s", got, want)
	}
}

func TestFileErrors(t *testing.T) {
	tests := []struct{ source, message string }{
		{`read_file("missing.txt");`, "missing.txt"},
		{`open("f.txt", "q");`, "invalid mode"},
		{`write_file("f.txt", "x"); open("f.txt").write("x");`, "not open for writing"},
		{`write_file("f.txt", "x"); open("f.txt", "w").read();`, "not open for reading"},
		{`write_file("f.txt", "x"); const handle = open("f.txt"); handle.close(); handle.read();`, "closed file"},
		{`write_file("f.txt", "x"); open("f.txt").flush(); open("f.txt").unknown();`, "has no property"},
		{`read_file(1);`, "expects a path string"},
		{`write_file("f.txt");`, "write_file expects 2 arguments"},
		{`open("f.txt", "r", "extra");`, "open expects 1 or 2 arguments"},
		{`write_file("f.txt", "x"); open("f.txt").read(1, 2);`, "read expects 0 or 1 arguments"},
		{`write_file("f.txt", "x"); open("f.txt").read("all");`, "read size must be a safe integer"},
		{`write_file("f.txt", "x"); open("f.txt").seek("start");`, "seek offset must be a safe integer"},
		{`write_file("f.txt", "x"); open("f.txt").write(1);`, "write expects a string"},
		{`remove_file("missing.txt");`, "missing.txt"},
		{`open("f.txt", "r", mode="w");`, "does not accept keyword arguments"},
	}
	for _, test := range tests {
		t.Run(test.source, func(t *testing.T) {
			err := RunWithOptions(test.source, "main.tg", nil, NewMemoryFileSystem(), nil)
			if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
				t.Fatalf("expected %q with position, got %v", test.message, err)
			}
		})
	}
}

func TestFileErrorsAreCatchable(t *testing.T) {
	source := `try { read_file("missing.txt"); } catch (error) { print("caught", "missing.txt" in error); }
try { open("f.txt", "q"); } catch (error) { print("caught", "invalid mode" in error); }`
	want := "caught true\ncaught true\n"
	if got := runFiles(t, source); got != want {
		t.Fatalf("got %q, want %q", got, want)
	}
}

func TestFileSystemOS(t *testing.T) {
	directory := t.TempDir()
	path := filepath.Join(directory, "report.txt")
	source := `const PATH = ` + strconvQuote(path) + `;
print(write_file(PATH, "first\n"));
print(append_file(PATH, "second\n"));
print(read_file(PATH), end="");
remove_file(PATH);
print(file_exists(PATH));
`
	// The default filesystem is the host disk, so no FileSystem is supplied.
	var output bytes.Buffer
	if err := RunWithOptions(source, "main.tg", nil, nil, &output); err != nil {
		t.Fatal(err)
	}
	want := "6\n7\nfirst\nsecond\nfalse\n"
	if output.String() != want {
		t.Fatalf("got %q, want %q", output.String(), want)
	}
	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Fatalf("expected %s to be removed, stat err = %v", path, err)
	}
}

func strconvQuote(text string) string {
	return `"` + strings.ReplaceAll(text, `\`, `\\`) + `"`
}