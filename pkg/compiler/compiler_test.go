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
	testStandalone(t, `def square(number) { return number * number; } print(square(9));`, "81\n")
}

func TestStandaloneOOP(t *testing.T) {
	source, err := os.ReadFile("../../examples/oop.tg")
	if err != nil {
		t.Fatal(err)
	}
	testStandalone(t, string(source), "Rex is a Canine.\nRex barks!\nRex fetches the ball.\n")
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
