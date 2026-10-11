package evaluator

import (
	"bytes"
	"strings"
	"testing"
)

func TestConditionalExpression(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"basic", `print("odd" if 3 % 2 == 1 else "even");`, "odd\n"},
		{"falsy", `print("odd" if 4 % 2 == 1 else "even");`, "even\n"},
		{"lazy", `function boom() { throw "boom"; } print("safe" if true else boom());`, "safe\n"},
		{"nested", `print("a" if false else "b" if true else "c");`, "b\n"},
		{"low precedence", `print(0 or 5 if false else 9);`, "9\n"},
		{"whole left side", `print(1 + 2 if true else 3 * 4);`, "3\n"},
		{"not binds tighter", `print(not true if true else "x");`, "false\n"},
		{"comparison is the condition", `print("hit" if 1 < 2 == true else "miss");`, "hit\n"},
		{"call arguments", `print(str(1 if true else 2), str(1 if false else 2));`, "1 2\n"},
		{"declaration", `const label = "big" if 10 > 5 else "small"; print(label);`, "big\n"},
		{"truthiness", `print("yes" if "text" else "no", "yes" if "" else "no");`, "yes no\n"},
		{"list element", `print([1 if true else 9, 2]);`, "[1, 2]\n"},
		{"return", `function pick(value) { return "yes" if value else "no"; } print(pick(1), pick(0));`, "yes no\n"},
		{"catch guard", `try { throw "bad"; } catch (error) if error == "bad" { print("caught"); }`, "caught\n"},
		{"guard with ternary", `try { throw "bad"; } catch (error) if (error if true else "x") == "bad" { print("caught"); }`, "caught\n"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var output bytes.Buffer
			if err := Run(test.source, &output); err != nil || output.String() != test.want {
				t.Fatalf("got %q, %v; want %q", output.String(), err, test.want)
			}
		})
	}
	for _, test := range []struct{ source, message string }{
		{`print(1 if true);`, "expected"},
		{`print(1 if true else);`, "expected expression"},
	} {
		if err := Run(test.source, nil); err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}

func TestSlicing(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"list range", `print([1, 2, 3, 4, 5][1:3]);`, "[2, 3]\n"},
		{"open start", `print([1, 2, 3][:2]);`, "[1, 2]\n"},
		{"open stop", `print([1, 2, 3][1:]);`, "[2, 3]\n"},
		{"copy", `print([1, 2, 3][:]);`, "[1, 2, 3]\n"},
		{"step", `print([1, 2, 3, 4, 5][::2]);`, "[1, 3, 5]\n"},
		{"reverse", `print([1, 2, 3, 4, 5][::-1]);`, "[5, 4, 3, 2, 1]\n"},
		{"negative start", `print([1, 2, 3][-1:]);`, "[3]\n"},
		{"negative stop", `print([1, 2, 3][:-1]);`, "[1, 2]\n"},
		{"mixed", `print([1, 2, 3, 4, 5][3:0:-1]);`, "[4, 3, 2]\n"},
		{"clamped", `print([1, 2, 3][1:99], [1, 2, 3][-99:2]);`, "[2, 3] [1, 2]\n"},
		{"empty", `print([1, 2, 3][2:1], [1, 2, 3][0:0]);`, "[] []\n"},
		{"string", `print("tiger"[1:4], "tiger"[::-1], "tiger"[::2]);`, "ige regit tgr\n"},
		{"unicode", `print("h\u00e9llo"[0:3]);`, "h\u00e9l\n"},
		{"copy is independent", `const items = [1, 2, 3]; const copy = items[:]; copy[0] = 99; print(items, copy);`, "[1, 2, 3] [99, 2, 3]\n"},
		{"structural equality", `print([1, 2, 3][:] == [1, 2, 3]);`, "true\n"},
		{"expressions", `const items = [10, 20, 30, 40]; print(items[1 - 1:4 - 2]);`, "[10, 20]\n"},
		{"nested access", `print(([1, 2, 3, 4][1:3])[0]);`, "2\n"},
		{"index still works", `print([1, 2, 3][1], "tiger"[0]);`, "2 t\n"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var output bytes.Buffer
			if err := Run(test.source, &output); err != nil || output.String() != test.want {
				t.Fatalf("got %q, %v; want %q", output.String(), err, test.want)
			}
		})
	}
	for _, test := range []struct{ source, message string }{
		{`print([1, 2, 3][0:2:0]);`, "slice step cannot be zero"},
		{`print([1, 2, 3]["a":1]);`, "slice bounds must be integers"},
		{`print([1, 2, 3][0.5:1]);`, "slice bounds must be integers"},
		{`print({"a": 1}[0:1]);`, "dict cannot be sliced"},
		{`print(5[0:1]);`, "number cannot be sliced"},
		{`var items = [1, 2, 3]; items[0:1] = [9];`, "invalid assignment target"},
	} {
		if err := Run(test.source, nil); err == nil || !strings.Contains(err.Error(), test.message) || !strings.Contains(err.Error(), "1:") {
			t.Errorf("%s: expected positioned %q, got %v", test.source, test.message, err)
		}
	}
}

func TestLabeledLoops(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"break label", `outer: for i in range(3) { for j in range(3) { if j == 1 { break outer; } print(i, j); } } print("done");`, "0 0\ndone\n"},
		{"continue label", `outer: for i in range(3) { for j in range(3) { if j == 1 { continue outer; } print(i, j); } } print("done");`, "0 0\n1 0\n2 0\ndone\n"},
		{"while label", `outer: while true { while true { break outer; } } print("after");`, "after\n"},
		{"cfor label", `outer: cfor (var i = 0; i < 3; i++) { cfor (var j = 0; j < 3; j++) { if i == 2 { break outer; } } } print("after");`, "after\n"},
		{"do while label", `outer: do { do { break outer; } while (true); } while (true); print("after");`, "after\n"},
		{"inner label shadows", `outer: for i in range(2) { outer: for j in range(3) { if j == 1 { break outer; } print(i, j); } } print("done");`, "0 0\n1 0\ndone\n"},
		{"plain break still works", `for i in range(3) { if i == 1 { break; } print(i); } print("done");`, "0\ndone\n"},
		{"labeled break skips else", `outer: for i in range(2) { for j in range(2) { break outer; } } else { print("else"); } print("done");`, "done\n"},
		{"inner break keeps else", `for i in range(2) { for j in range(2) { break; } } else { print("else"); } print("done");`, "else\ndone\n"},
		{"continue label in while", `var n = 0; outer: while n < 3 { n++; while true { continue outer; } } print(n);`, "3\n"},
		{"label through try", `outer: for i in range(3) { try { if i == 1 { break outer; } print(i); } finally { print("f" + str(i)); } } print("done");`, "0\nf0\nf1\ndone\n"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var output bytes.Buffer
			if err := Run(test.source, &output); err != nil || output.String() != test.want {
				t.Fatalf("got %q, %v; want %q", output.String(), err, test.want)
			}
		})
	}
	for _, test := range []struct{ source, message string }{
		{`break missing;`, "unknown label"},
		{`while true { break missing; }`, "unknown label"},
		{`outer: for i in range(2) { break; } break;`, "break outside a loop"},
		{`for i in range(2) { continue; } continue;`, "continue outside a loop"},
		{`outer: for i in range(2) { function inner() { break outer; } }`, "unknown label"},
		{`for i in range(2) { continue 5; }`, "expected"},
	} {
		if err := Run(test.source, nil); err == nil || !strings.Contains(err.Error(), test.message) {
			t.Errorf("%s: expected %q, got %v", test.source, test.message, err)
		}
	}
}

func TestDoWhile(t *testing.T) {
	tests := []struct{ name, source, want string }{
		{"runs until false", `var n = 0; do { print(n); n++; } while (n < 3);`, "0\n1\n2\n"},
		{"always once", `var n = 5; do { print("once"); } while (n < 3);`, "once\n"},
		{"break", `var n = 0; do { n++; if n == 2 { break; } } while (n < 5); print(n);`, "2\n"},
		{"no parentheses", `var n = 0; do { n++; } while n < 2; print(n);`, "2\n"},
		{"continue", `var n = 0; do { n++; if n % 2 == 1 { continue; } print(n); } while (n < 4);`, "2\n4\n"},
		{"nested", `var outer = 0; var inner = 0; do { inner = 0; do { inner++; } while (inner < 2); outer++; } while (outer < 2); print(outer, inner);`, "2 2\n"},
		{"label", `outer: do { do { break outer; } while (true); } while (true); print("after");`, "after\n"},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			var output bytes.Buffer
			if err := Run(test.source, &output); err != nil || output.String() != test.want {
				t.Fatalf("got %q, %v; want %q", output.String(), err, test.want)
			}
		})
	}
	for _, test := range []struct{ source, message string }{
		{`do { print(1); } while;`, "expected expression"},
		{`do { print(1); } while true`, "expected"},
		{`break;`, "break outside a loop"},
	} {
		if err := Run(test.source, nil); err == nil || !strings.Contains(err.Error(), test.message) {
			t.Errorf("%s: expected %q, got %v", test.source, test.message, err)
		}
	}
}
