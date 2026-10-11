package format

import (
	"errors"
	"fmt"
	"testing"
)

// TestSprintfParity locks the supported verbs to fmt's output.
func TestSprintfParity(t *testing.T) {
	cases := []struct {
		name   string
		format string
		args   []any
	}{
		{"string", "%s", []any{"hello"}},
		{"error", "%s", []any{errors.New("boom")}},
		{"decimal", "%d", []any{42}},
		{"negative", "%d", []any{-7}},
		{"quoted string", "%q", []any{"hi\n"}},
		{"quoted rune", "%q", []any{'A'}},
		{"quoted wide rune", "%q", []any{rune(0x1F600)}},
		{"character rune", "%c", []any{'x'}},
		{"character code", "%c", []any{65}},
		{"value error", "%v", []any{errors.New("bad")}},
		{"value bool", "%v", []any{true}},
		{"value float", "%v", []any{3.5}},
		{"mixed", "%s-%d-%q", []any{"a", 2, "b"}},
		{"percent", "100%%", nil},
		{"trailing percent", "%d%%", []any{5}},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			if got, want := Sprintf(test.format, test.args...), fmt.Sprintf(test.format, test.args...); got != want {
				t.Fatalf("Sprintf(%q) = %q; want %q", test.format, got, want)
			}
		})
	}
}

// TestErrorfParity verifies messages match fmt.Errorf when no %w is present.
func TestErrorfParity(t *testing.T) {
	got := Errorf("%s: %d", "value", 3).Error()
	want := fmt.Errorf("%s: %d", "value", 3).Error()
	if got != want {
		t.Fatalf("Errorf message = %q; want %q", got, want)
	}
}

// TestErrorfWraps verifies %w keeps errors.Is working.
func TestErrorfWraps(t *testing.T) {
	root := errors.New("root cause")
	err := Errorf("context: %w", root)
	if err.Error() != "context: root cause" {
		t.Fatalf("message = %q", err.Error())
	}
	if !errors.Is(err, root) {
		t.Fatal("errors.Is did not find the wrapped error")
	}
}

// TestErrorfWrapsMultiple verifies several %w verbs are all reachable.
func TestErrorfWrapsMultiple(t *testing.T) {
	first, second := errors.New("first"), errors.New("second")
	err := Errorf("%w and %w", first, second)
	if !errors.Is(err, first) || !errors.Is(err, second) {
		t.Fatalf("errors.Is failed for multi-wrap: %v", err)
	}
}

// TestSprintfMissingArgument documents the missing-argument marker.
func TestSprintfMissingArgument(t *testing.T) {
	if got, want := Sprintf("%d", []any{}...), "%!d(MISSING)"; got != want {
		t.Fatalf("got %q; want %q", got, want)
	}
}
