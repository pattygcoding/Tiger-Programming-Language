package wasmcheck

import (
	"os"
	"os/exec"
	"strings"
	"testing"
)

// forbiddenPackages must never be reachable from cmd/wasm under GOOS=js. Each
// entry re-inflates the browser binary far beyond its runtime value; see
// AGENTS.md, "Wasm size guardrails", for the intended replacement of each.
var forbiddenPackages = []string{
	"fmt",              // pulls reflect, internal/fmtsort, os
	"reflect",          // no longer needed once fmt is gone
	"os",               // host filesystem
	"path/filepath",    // pulls os
	"io/fs",            // pulls time
	"time",             // only reached through os/io/fs
	"syscall",          // non-js syscalls
	"sort",             // pulls slices and cmp
	"container/heap",   // imports sort
	"slices",           // only reached through sort
	"cmp",              // only reached through slices
	"bufio",            // pulls bytes
	"bytes",            // only reached through bufio
	"internal/fmtsort", // only reached through fmt
	"internal/poll",    // only reached through os
}

// TestBrowserBuildHasNoBloatingDependencies fails if the GOOS=js dependency
// closure contains a package that must stay out of the browser build.
func TestBrowserBuildHasNoBloatingDependencies(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping dependency scan in short mode")
	}
	goTool, err := exec.LookPath("go")
	if err != nil {
		t.Skipf("go tool unavailable: %v", err)
	}
	command := exec.Command(goTool, "list", "-deps", "tiger/cmd/wasm")
	command.Env = append(os.Environ(), "GOOS=js", "GOARCH=wasm", "CGO_ENABLED=0")
	output, err := command.CombinedOutput()
	if err != nil {
		t.Fatalf("go list -deps tiger/cmd/wasm failed: %v\n%s", err, output)
	}
	linked := map[string]bool{}
	for _, line := range strings.Split(strings.TrimSpace(string(output)), "\n") {
		linked[strings.TrimSpace(line)] = true
	}
	for _, name := range forbiddenPackages {
		if linked[name] {
			t.Errorf("%s is linked into the browser Wasm build; "+
				"see AGENTS.md (Wasm size guardrails) for the intended replacement", name)
		}
	}
}
