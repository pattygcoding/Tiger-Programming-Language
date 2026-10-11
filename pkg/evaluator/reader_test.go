package evaluator

import (
	"io"
	"strings"
	"testing"
)

func TestBufferedReaderReadString(t *testing.T) {
	reader := newBufferedReader(strings.NewReader("first\nsecond\nthird"))
	for _, want := range []string{"first\n", "second\n"} {
		got, err := reader.ReadString('\n')
		if err != nil || got != want {
			t.Fatalf("ReadString = %q, %v; want %q", got, err, want)
		}
	}
	got, err := reader.ReadString('\n')
	if got != "third" || err != io.EOF {
		t.Fatalf("final ReadString = %q, %v; want %q, EOF", got, err, "third")
	}
}

func TestBufferedReaderLongLine(t *testing.T) {
	long := strings.Repeat("x", 10_000)
	reader := newBufferedReader(strings.NewReader(long + "\nrest\n"))
	got, err := reader.ReadString('\n')
	if err != nil || got != long+"\n" {
		t.Fatalf("long ReadString len = %d, err = %v", len(got), err)
	}
	got, err = reader.ReadString('\n')
	if err != nil || got != "rest\n" {
		t.Fatalf("after long line: %q, %v", got, err)
	}
}

func TestBufferedReaderResetAndBuffered(t *testing.T) {
	reader := newBufferedReader(strings.NewReader("alpha\nbeta\n"))
	if _, err := reader.ReadString('\n'); err != nil {
		t.Fatal(err)
	}
	if reader.Buffered() == 0 {
		t.Fatal("expected buffered remainder")
	}
	reader.Reset(strings.NewReader("gamma\n"))
	if reader.Buffered() != 0 {
		t.Fatal("Reset should clear the buffer")
	}
	got, err := reader.ReadString('\n')
	if err != nil || got != "gamma\n" {
		t.Fatalf("after reset: %q, %v", got, err)
	}
}

func TestBufferedReaderReadAll(t *testing.T) {
	reader := newBufferedReader(strings.NewReader("keep\nthe rest"))
	if _, err := reader.ReadString('\n'); err != nil {
		t.Fatal(err)
	}
	rest, err := io.ReadAll(reader)
	if err != nil || string(rest) != "the rest" {
		t.Fatalf("ReadAll = %q, %v", rest, err)
	}
}
