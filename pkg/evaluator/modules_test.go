package evaluator

import (
	"bytes"
	"fmt"
	"strings"
	"testing"
	"testing/fstest"
)

func TestModules(t *testing.T) {
	files := fstest.MapFS{
		"lib/counter.tg": {Data: []byte(`print("loaded"); var count = 0; function next() { return ++count; }
function nested() { import "nested/value.tg" as value; return value.number; }
class Box { var value = 9; }`)},
		"lib/nested/value.tg": {Data: []byte(`const number = 7;`)},
	}
	source := `const count = 99; import "lib/counter.tg" as first; import "lib/../lib/counter.tg" as second;
print(first == second, first.next(), second.next(), first.count, count);
const later = first.nested; print(later(), first.Box().value);`
	var output bytes.Buffer
	if err := RunWithLoader(source, "main.tg", FSLoader{FS: files}, &output); err != nil {
		t.Fatal(err)
	}
	if want := "loaded\ntrue 1 2 2 99\n7 9\n"; output.String() != want {
		t.Fatalf("got %q, want %q", output.String(), want)
	}
}

func TestModuleErrors(t *testing.T) {
	files := fstest.MapFS{
		"good.tg":     {Data: []byte(`var value = 1;`)},
		"cycle.tg":    {Data: []byte(`import "main.tg" as root;`)},
		"bad.tg":      {Data: []byte(`const value = ;`)},
		"runtime.tg":  {Data: []byte(`function bad() { print(f"{missing}"); }`)},
		"isolated.tg": {Data: []byte(`print(caller);`)},
		"throw.tg":    {Data: []byte(`throw "module failed";`)},
	}
	for _, test := range []struct{ source, message string }{
		{`import "missing.tg" as missing;`, "cannot import"},
		{`import "good.txt" as good;`, "must end in .tg"},
		{`import "../outside.tg" as outside;`, "escapes the module root"},
		{`import "cycle.tg" as cycle;`, "circular import"},
		{`import "bad.tg" as bad;`, "bad.tg:1:"},
		{`import "runtime.tg" as runtime; runtime.bad();`, "runtime.tg:1:"},
		{`const caller = 1; import "isolated.tg" as isolated;`, "undefined name"},
		{`import "good.tg" as good; print(good.print);`, "has no export"},
		{`import "good.tg" as good; good = 1;`, "cannot reassign const"},
		{`import "good.tg" as good; good.value = 2;`, "cannot assign a property of module"},
		{`import "good.tg" as good; good.value++;`, "cannot assign a property of module"},
		{`import "throw.tg" as bad;`, "throw.tg:1:1: uncaught throw"},
	} {
		t.Run(test.source, func(t *testing.T) {
			err := RunWithLoader(test.source, "main.tg", FSLoader{FS: files}, nil)
			if err == nil || !strings.Contains(err.Error(), test.message) {
				t.Fatalf("expected %q, got %v", test.message, err)
			}
		})
	}
	if err := Run(`import "good.tg" as good;`, nil); err == nil || !strings.Contains(err.Error(), "module loader") {
		t.Fatalf("expected missing loader error, got %v", err)
	}
}

func TestModuleFailureRetryAndLimits(t *testing.T) {
	files := fstest.MapFS{"fail.tg": {Data: []byte(`print("attempt"); throw "failed";`)}}
	var output bytes.Buffer
	source := `for index in range(2) { try { import "fail.tg" as failed; } catch (error) { print(error); } }`
	if err := RunWithLoader(source, "main.tg", FSLoader{FS: files}, &output); err != nil {
		t.Fatal(err)
	}
	if output.String() != "attempt\nfailed\nattempt\nfailed\n" {
		t.Fatalf("got %q", output.String())
	}
	for index := 0; index < 130; index++ {
		files[fmt.Sprintf("module%d.tg", index)] = &fstest.MapFile{Data: []byte(fmt.Sprintf(`import "module%d.tg" as next;`, index+1))}
	}
	if err := RunWithLoader(`import "module0.tg" as first;`, "main.tg", FSLoader{FS: files}, nil); err == nil || !strings.Contains(err.Error(), "import depth") {
		t.Fatalf("expected import depth limit, got %v", err)
	}
}
