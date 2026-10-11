//go:build js

package evaluator

import (
	"errors"
	"io"
)

// FileLoader is a browser stub. Use BundleLoader or a custom ModuleLoader with
// RunWithInput instead of reading files from disk.
type FileLoader struct{}

func (FileLoader) Resolve(importer, requested string) (string, error) {
	return "", errors.New("the browser has no host filesystem; import modules with a ModuleLoader")
}

func (FileLoader) Load(filename string) (string, error) {
	return "", errors.New("the browser has no host filesystem; import modules with a ModuleLoader")
}

// RunFile is unavailable in the browser.
func RunFile(filename string, output io.Writer) error {
	return errors.New("RunFile is unavailable in the browser; use RunWithInput")
}

// RunFileWithInput is unavailable in the browser.
func RunFileWithInput(filename string, input io.Reader, output io.Writer) error {
	return errors.New("RunFileWithInput is unavailable in the browser; use RunWithInput")
}
