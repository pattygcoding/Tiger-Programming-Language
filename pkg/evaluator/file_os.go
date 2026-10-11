//go:build !js

package evaluator

import (
	"errors"
	"io/fs"
	"os"
)

// OSFileSystem reads and writes the host operating system's files. It is
// available everywhere except the browser, which has no host filesystem.
type OSFileSystem struct{}

func (OSFileSystem) Open(name string, mode FileMode) (FileHandle, error) {
	flags := os.O_RDONLY
	switch {
	case mode.Read && mode.Write:
		flags = os.O_RDWR
	case mode.Write:
		flags = os.O_WRONLY
	}
	if mode.Create {
		flags |= os.O_CREATE
	}
	if mode.Truncate {
		flags |= os.O_TRUNC
	}
	if mode.Append {
		flags |= os.O_APPEND
	}
	handle, err := os.OpenFile(name, flags, 0644)
	if err != nil {
		return nil, err
	}
	return handle, nil
}

func (OSFileSystem) Remove(name string) error { return os.Remove(name) }

func (OSFileSystem) Exists(name string) (bool, error) {
	info, err := os.Stat(name)
	if err != nil {
		if errors.Is(err, fs.ErrNotExist) {
			return false, nil
		}
		return false, err
	}
	return !info.IsDir(), nil
}
