//go:build js

package evaluator

import "tiger/pkg/format"

// The browser build omits regexp, which would pull sort and bytes into
// web/tiger.wasm. These stubs keep the module visible with a clear error.
var errNoRegexp = format.Errorf("the regex module is not available in the browser build; use the native CLI or the WASI target")

func regexMatchString(pattern, text string) (bool, error)            { return false, errNoRegexp }
func regexFindString(pattern, text string) (string, bool, error)     { return "", false, errNoRegexp }
func regexFindAllStrings(pattern, text string) ([]string, error)     { return nil, errNoRegexp }
func regexGroupsString(pattern, text string) ([]string, bool, error) { return nil, false, errNoRegexp }
func regexReplaceString(pattern, text, replacement string) (string, error) {
	return "", errNoRegexp
}
func regexSplitString(pattern, text string) ([]string, error) { return nil, errNoRegexp }
