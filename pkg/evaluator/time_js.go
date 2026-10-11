//go:build js

package evaluator

import (
	"tiger/pkg/format"
	"tiger/pkg/object"
)

// The browser build has no clock: importing time would pull the whole time
// package (and its os/io/fs dependencies) into web/tiger.wasm, which the size
// guardrails forbid. These stubs keep the module visible with a clear error.
var errNoClock = format.Errorf("the time module is not available in the browser build; use the native CLI or the WASI target")

func timeNow() (float64, error)                       { return 0, errNoClock }
func timeNowMillis() (float64, error)                 { return 0, errNoClock }
func timeSleep(seconds float64) error                 { return errNoClock }
func timeISO(seconds float64) (string, error)         { return "", errNoClock }
func timeParts(seconds float64) (*object.Dict, error) { return nil, errNoClock }
