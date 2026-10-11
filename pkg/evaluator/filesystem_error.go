package evaluator

import "errors"

// ErrNotExist is returned by a FileSystem when a path does not exist. It
// replaces io/fs.ErrNotExist so the in-memory filesystem does not pull io/fs
// (and its time dependency) into the browser build.
var ErrNotExist = errors.New("file does not exist")

// pathError mirrors io/fs.PathError's message without importing io/fs.
type pathError struct {
	Op   string
	Path string
	Err  error
}

func (failure *pathError) Error() string {
	return failure.Op + " " + failure.Path + ": " + failure.Err.Error()
}

func (failure *pathError) Unwrap() error { return failure.Err }
