//go:build !js

package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestTimeModule(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"iso epoch", `print(time.iso(0));`, "1970-01-01T00:00:00Z\n"},
		{"iso known", `print(time.iso(1600000000));`, "2020-09-13T12:26:40Z\n"},
		{"parts", `const p = time.parts(1600000000); print(p["year"], p["month"], p["day"], p["hour"], p["minute"], p["second"], p["weekday"], p["yearday"]);`, "2020 9 13 12 26 40 0 257\n"},
		{"clock", `print(time.now() > 1600000000, time.now_ms() > 1600000000000);`, "true true\n"},
		{"sleep", `time.sleep(0); print("slept");`, "slept\n"},
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

func TestTimeModuleErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`time.iso("x");`, "expects a number"},
		{`time.sleep(-1);`, "non-negative"},
		{`time.parts();`, "expects 1 argument"},
		{`time.now(1);`, "expects 0 arguments"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}
