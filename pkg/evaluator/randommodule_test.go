package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestRandomModule(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"single point int", `print(random.int(4, 4), random.int(-1, -1));`, "4 -1\n"},
		{"fraction range", `var value = random.random(); print(value >= 0 and value < 1);`, "true\n"},
		{"uniform range", `var value = random.uniform(2, 3); print(value >= 2 and value < 3);`, "true\n"},
		{"choice", `const items = ["a", "b", "c"]; print(random.choice(items) in items);`, "true\n"},
		{"seeded reproducibility", `random.seed(7); var first = [random.int(0, 1000), random.int(0, 1000)]; random.seed(7); var second = [random.int(0, 1000), random.int(0, 1000)]; print(first == second);`, "true\n"},
		{"shuffle keeps a multiset", `random.seed(1); var items = [1, 2, 3, 4, 5]; random.shuffle(items); items.sort(); print(items);`, "[1, 2, 3, 4, 5]\n"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var output bytes.Buffer
			if err := Run(test.source, &output); err != nil || output.String() != test.want {
				t.Fatalf("got %q, %v; want %q", output.String(), err, test.want)
			}
		})
	}
}

func TestRandomDefaultSeedIsReproducible(t *testing.T) {
	var first, second bytes.Buffer
	if err := Run(`print(random.int(0, 1000000), random.random());`, &first); err != nil {
		t.Fatal(err)
	}
	if err := Run(`print(random.int(0, 1000000), random.random());`, &second); err != nil {
		t.Fatal(err)
	}
	if first.String() != second.String() {
		t.Fatalf("default seed is not reproducible: %q versus %q", first.String(), second.String())
	}
}

func TestRandomModuleErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`random.int(5, 1);`, "expects low <= high"},
		{`random.int(0.5, 1);`, "must be a safe integer"},
		{`random.int("a", 1);`, "must be a number"},
		{`random.seed();`, "expects 1 argument"},
		{`random.choice([]);`, "empty list"},
		{`random.choice("x");`, "expects a list"},
		{`random.shuffle(1);`, "expects a list"},
		{`random.random(1);`, "expects 0 arguments"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}
