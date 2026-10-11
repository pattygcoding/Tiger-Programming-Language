package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestPowerAndFloorDivision(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"power", `print(2 ** 10, 2 ** 0, 4 ** 0.5);`, "1024 1 2\n"},
		{"power right associative", `print(2 ** 3 ** 2, 2 ** 2 ** 3);`, "512 256\n"},
		{"power binds tighter than unary minus", `print(-2 ** 2, -(2 ** 2), 2 ** -1);`, "-4 -4 0.5\n"},
		{"power with arithmetic", `print(2 * 3 ** 2, (2 * 3) ** 2, 2 ** 3 + 1);`, "18 36 9\n"},
		{"floor division", `print(7 div 2, -7 div 2, 7 div -2, -7 div -2);`, "3 -4 -4 3\n"},
		{"floor division fractions", `print(7.5 div 2, 10 div 5 div 2, 2 + 6 div 3);`, "3 1 4\n"},
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

func TestBitwiseOperators(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"and or xor", `print(6 & 3, 6 | 3, 6 ^ 3);`, "2 7 5\n"},
		{"not", `print(~0, ~5, ~2.7);`, "-1 -6 -3\n"},
		{"shifts", `print(1 << 4, 256 >> 4, -8 >> 1, 1 << 40);`, "16 16 -4 1099511627776\n"},
		{"truncates operands", `print(6.9 & 3.9, 7.9 >> 1);`, "2 3\n"},
		{"arithmetic precedence", `print(1 + 2 << 3, 1 | 2 & 3, 2 ** 3 & 1);`, "24 3 0\n"},
		{"bitwise precedence", `print(255 & 15 | 16, 1 ^ 1, 5 & 3 == 1);`, "31 0 true\n"},
		{"large shift count", `print(1 << 100, -1 >> 100);`, "0 -1\n"},
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

func TestOperatorErrors(t *testing.T) {
	for _, test := range []struct{ source, message string }{
		{`print(1 div 0);`, "division by zero"},
		{`print(1 % 0);`, "division by zero"},
		{`print(0 ** -1);`, "non-finite numeric result"},
		{`print((-8) ** 0.5);`, "non-finite numeric result"},
		{`print(1 << -1);`, "shift count must not be negative"},
		{`print(1e300 & 1);`, "64-bit integer range"},
		{`print("a" ** 2);`, "not supported"},
		{`print("a" div 2);`, "not supported"},
		{`print("a" & 1);`, "not supported"},
		{`print(~"a");`, "requires a number"},
	} {
		err := Run(test.source, nil)
		if err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
	var output bytes.Buffer
	if err := Run(`try { print(1 div 0); } catch (error) { print("division by zero" in error); }`, &output); err != nil || output.String() != "true\n" {
		t.Fatalf("got %q, %v", output.String(), err)
	}
}
