package compiler

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"tiger"
)

func Build(source, filename, output string) error {
	modules, entry, err := collectModules(source, filename)
	if err != nil {
		return err
	}
	encoded, err := json.Marshal(modules)
	if err != nil {
		return err
	}
	goTool, err := exec.LookPath("go")
	if err != nil {
		return fmt.Errorf("building requires Go 1.22+ on PATH: %w", err)
	}
	destination, err := filepath.Abs(output)
	if err != nil {
		return err
	}
	workspace, err := os.MkdirTemp("", "tiger-build-*")
	if err != nil {
		return err
	}
	defer os.RemoveAll(workspace)
	err = fs.WalkDir(tiger.RuntimeSources, ".", func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		destination := filepath.Join(workspace, filepath.FromSlash(path))
		if entry.IsDir() {
			return os.MkdirAll(destination, 0755)
		}
		if strings.HasSuffix(path, "_test.go") {
			return nil
		}
		content, err := tiger.RuntimeSources.ReadFile(path)
		if err != nil {
			return err
		}
		return os.WriteFile(destination, content, 0644)
	})
	if err != nil {
		return fmt.Errorf("prepare runtime: %w", err)
	}
	main := `package main
import (
	"encoding/json"
    "fmt"
    "os"
    "tiger/pkg/evaluator"
)
func main() {
	var modules evaluator.BundleLoader
	if err := json.Unmarshal([]byte(` + strconv.Quote(string(encoded)) + `), &modules); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := evaluator.RunWithLoader(modules.Sources[` + strconv.Quote(entry) + `], ` + strconv.Quote(entry) + `, modules, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, err)
        os.Exit(1)
    }
}
`
	if err := os.WriteFile(filepath.Join(workspace, "main.go"), []byte(main), 0644); err != nil {
		return err
	}
	command := exec.Command(goTool, "build", "-trimpath", "-mod=readonly", "-o", destination, ".")
	command.Dir = workspace
	command.Env = append(os.Environ(), "GOWORK=off", "CGO_ENABLED=0")
	if output, err := command.CombinedOutput(); err != nil {
		return fmt.Errorf("Go build failed: %w\n%s", err, output)
	}
	return nil
}
