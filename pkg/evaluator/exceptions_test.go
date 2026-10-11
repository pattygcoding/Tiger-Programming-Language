package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestFinally(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"normal path", `var log = []; try { log = log + ["try"]; } finally { log = log + ["finally"]; } print(log);`, "[\"try\", \"finally\"]\n"},
		{"after catch", `var log = []; try { throw "boom"; } catch (error) { log = log + ["caught"]; } finally { log = log + ["finally"]; } print(log);`, "[\"caught\", \"finally\"]\n"},
		{"before a propagating error", `var log = []; try { try { throw "boom"; } finally { log = log + ["inner"]; } } catch (error) { log = log + ["outer"]; } print(log);`, "[\"inner\", \"outer\"]\n"},
		{"before a return", `var log = []; function run() { try { return "value"; } finally { log = log + ["finally"]; } } print(run(), log);`, "value [\"finally\"]\n"},
		{"return in finally overrides", `function run() { try { throw "boom"; } finally { return "overridden"; } } print(run());`, "overridden\n"},
		{"with continue", `var log = []; for index in [1, 2, 3] { try { if index == 2 { continue; } log = log + [index]; } finally { log = log + ["f"]; } } print(log);`, "[1, \"f\", \"f\", 3, \"f\"]\n"},
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

func TestStructuredErrors(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"fields", `try { print(1 / 0); } catch (error) { print(type(error), error["kind"], "division by zero" in error, isinstance(error, "error")); }`, "error zero division true true\n"},
		{"origin", `try { print([1, 2][9]); } catch (error) { print(error["kind"], error["origin"] == ""); }`, "index range false\n"},
		{"message", `try { print(1 / 0); } catch (error) { print(error["message"]); }`, "division by zero\n"},
		{"thrown values pass through", `try { throw {"k": 1}; } catch (error) { print(error["k"]); }`, "1\n"},
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

func TestTypedCatch(t *testing.T) {
	source := `function classify(value) {
    try {
        if value == 0 { throw "custom"; }
        return [10, 20][value];
    } catch (error) if isinstance(error, "error") and error["kind"] == "index range" {
        return "range";
    } catch (error) {
        return "other";
    }
}
print(classify(1), classify(5), classify(0));`
	var output bytes.Buffer
	if err := Run(source, &output); err != nil || output.String() != "20 range other\n" {
		t.Fatalf("got %q, %v", output.String(), err)
	}
}

func TestErrorHandlingErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`try { print(1); }`, "needs a catch or finally"},
		{`print("a" / 1);`, "not supported"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
	var output bytes.Buffer
	if err := Run(`try { throw "boom"; } catch (error) { print("uncaught"); }`, &output); err != nil || output.String() != "uncaught\n" {
		t.Fatalf("got %q, %v", output.String(), err)
	}
}
