//go:build js

package evaluator

import "errors"

// OSFileSystem is a stub for the browser, which has no host filesystem. The
// playground connects MemoryFileSystem instead, so these methods are never hit
// during normal execution.
type OSFileSystem struct{}

func (OSFileSystem) Open(name string, mode FileMode) (FileHandle, error) {
	return nil, errNoHostFileSystem
}

func (OSFileSystem) Remove(name string) error { return errNoHostFileSystem }

func (OSFileSystem) Exists(name string) (bool, error) {
	return false, errNoHostFileSystem
}

var errNoHostFileSystem = errors.New("the browser has no host filesystem; use MemoryFileSystem")
