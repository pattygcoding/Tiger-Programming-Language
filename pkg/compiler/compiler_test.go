package compiler

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func TestStandaloneBuild(t *testing.T) {
	testStandalone(t, `function square(number) { return number * number; } print(square(9));`, "81\n")
}

func TestStandaloneOOP(t *testing.T) {
	source, err := os.ReadFile("../../examples/oop.tg")
	if err != nil {
		t.Fatal(err)
	}
	testStandalone(t, string(source), "Rex is a Canine.\nRex barks!\nRex fetches the ball.\n")
}

func TestStandaloneExtendedSyntax(t *testing.T) {
	testStandalone(t, `class Counter { private var count = 0; function next(this) { return ++this.count; } }
const counter = Counter(); cfor (var index = 0; index < 3; index++) { print(f"{counter.next()}"); }
try { throw range(3); } catch (error) { print(error); }`, "1\n2\n3\n[0, 1, 2]\n")
}

func TestStandaloneImports(t *testing.T) {
	if testing.Short() {
		t.Skip("standalone integration build")
	}
	directory := t.TempDir()
	sources := filepath.Join(directory, "sources")
	if err := os.MkdirAll(filepath.Join(sources, "lib"), 0700); err != nil {
		t.Fatal(err)
	}
	entry := `import "lib/math.tg" as math; print(math.calculate(9));`
	for name, source := range map[string]string{
		"main.tg":       entry,
		"lib/math.tg":   `function calculate(value) { import "factor.tg" as factor; return value * factor.amount; }`,
		"lib/factor.tg": `const amount = 3;`,
	} {
		if err := os.WriteFile(filepath.Join(sources, name), []byte(source), 0600); err != nil {
			t.Fatal(err)
		}
	}
	output := filepath.Join(directory, "standalone")
	if runtime.GOOS == "windows" {
		output += ".exe"
	}
	if err := Build(entry, filepath.Join(sources, "main.tg"), output); err != nil {
		t.Fatal(err)
	}
	if err := os.RemoveAll(sources); err != nil {
		t.Fatal(err)
	}
	command := exec.Command(output)
	command.Dir = directory
	command.Env = append(os.Environ(), "PATH=")
	actual, err := command.CombinedOutput()
	if err != nil || string(actual) != "27\n" {
		t.Fatalf("output=%q err=%v", actual, err)
	}
}

func testStandalone(t *testing.T, source, expected string) {
	t.Helper()
	if testing.Short() {
		t.Skip("standalone integration build")
	}
	directory := t.TempDir()
	output := filepath.Join(directory, "standalone")
	if runtime.GOOS == "windows" {
		output += ".exe"
	}
	if err := Build(source, "embedded.tg", output); err != nil {
		t.Fatal(err)
	}
	command := exec.Command(output)
	command.Dir = directory
	command.Env = append(os.Environ(), "PATH=")
	actual, err := command.CombinedOutput()
	if err != nil || string(actual) != expected {
		t.Fatalf("standalone output=%q err=%v", actual, err)
	}
}

func TestRejectInvalidSource(t *testing.T) {
	output := filepath.Join(t.TempDir(), "invalid")
	err := Build("print(1)", "invalid.tg", output)
	if err == nil || !strings.Contains(err.Error(), "invalid.tg:1:") {
		t.Fatalf("expected syntax error, got %v", err)
	}
	if _, err := os.Stat(output); !os.IsNotExist(err) {
		t.Fatal("invalid source created output")
	}
}
