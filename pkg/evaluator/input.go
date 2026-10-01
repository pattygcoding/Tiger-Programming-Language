package evaluator

import (
	"bufio"
	"errors"
	"fmt"
	"io"
	"strings"
	"tiger/pkg/object"
)

// input mirrors Python's input([prompt]): it writes the prompt without a
// newline and returns one line of input without its line ending.
func (eval *Evaluator) input(args []object.Value, keywords map[string]object.Value) (object.Value, error) {
	if len(keywords) != 0 {
		return nil, fmt.Errorf("input does not accept keyword arguments")
	}
	if len(args) > 1 {
		return nil, fmt.Errorf("input expects at most 1 argument, got %d", len(args))
	}
	if len(args) == 1 {
		if _, err := io.WriteString(eval.Output, object.Format(args[0])); err != nil {
			return nil, err
		}
	}
	if eval.Input == nil {
		return nil, fmt.Errorf("input is unavailable: no input stream is connected")
	}
	if eval.reader == nil || eval.readerFrom != eval.Input {
		eval.reader, eval.readerFrom = bufio.NewReader(eval.Input), eval.Input
	}
	line, err := eval.reader.ReadString('\n')
	if errors.Is(err, io.EOF) {
		if line == "" {
			return nil, fmt.Errorf("EOF when reading a line")
		}
	} else if err != nil {
		return nil, err
	}
	line = strings.TrimSuffix(line, "\n")
	return object.String(strings.TrimSuffix(line, "\r")), nil
}
