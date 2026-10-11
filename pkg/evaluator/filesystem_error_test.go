package evaluator

import (
	"errors"
	"testing"
)

func TestPathErrorMessageMatchesIOFS(t *testing.T) {
	failure := &pathError{Op: "open", Path: "missing.txt", Err: ErrNotExist}
	if got, want := failure.Error(), "open missing.txt: file does not exist"; got != want {
		t.Fatalf("pathError message = %q; want %q", got, want)
	}
	if !errors.Is(failure, ErrNotExist) {
		t.Fatal("errors.Is did not unwrap pathError to ErrNotExist")
	}
}

func TestMemoryFileSystemMissingFile(t *testing.T) {
	_, err := NewMemoryFileSystem().Open("missing.txt", FileMode{Read: true})
	if !errors.Is(err, ErrNotExist) {
		t.Fatalf("Open error = %v; want ErrNotExist", err)
	}
}
