//go:build !js

package evaluator

import (
	"io"
	"io/fs"
	"os"
	"path"
	"path/filepath"
	"strings"

	"tiger/pkg/format"
)

type FileLoader struct{}

func (FileLoader) Resolve(importer, requested string) (string, error) {
	if filepath.Ext(requested) != ".tg" {
		return "", format.Errorf("import path must end in .tg")
	}
	filename := filepath.FromSlash(requested)
	if !filepath.IsAbs(filename) {
		filename = filepath.Join(filepath.Dir(importer), filename)
	}
	filename, err := filepath.Abs(filename)
	if err != nil {
		return "", err
	}
	if canonical, err := filepath.EvalSymlinks(filename); err == nil {
		filename = canonical
	}
	return filename, nil
}

func (FileLoader) Load(filename string) (string, error) {
	content, err := os.ReadFile(filename)
	return string(content), err
}

type FSLoader struct{ FS fs.FS }

func (FSLoader) Resolve(importer, requested string) (string, error) {
	if path.Ext(requested) != ".tg" || strings.Contains(requested, "\\") || path.IsAbs(requested) {
		return "", format.Errorf("import path must be a relative .tg path using forward slashes")
	}
	filename := path.Join(path.Dir(importer), requested)
	if !fs.ValidPath(filename) {
		return "", format.Errorf("import path escapes the module root")
	}
	return filename, nil
}

func (loader FSLoader) Load(filename string) (string, error) {
	content, err := fs.ReadFile(loader.FS, filename)
	return string(content), err
}

func RunFile(filename string, output io.Writer) error {
	return RunFileWithInput(filename, nil, output)
}

// RunFileWithInput runs a file from disk; input feeds the input() built-in.
func RunFileWithInput(filename string, input io.Reader, output io.Writer) error {
	loader := FileLoader{}
	canonical, err := loader.Resolve("", filename)
	if err != nil {
		return err
	}
	source, err := loader.Load(canonical)
	if err != nil {
		return err
	}
	return RunWithInput(source, canonical, loader, nil, input, output)
}
