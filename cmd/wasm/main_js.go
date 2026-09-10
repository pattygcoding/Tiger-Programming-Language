package main

import (
	"bytes"
	"fmt"
	"syscall/js"
	"tiger/pkg/evaluator"
)

type limitedOutput struct{ bytes.Buffer }

func (output *limitedOutput) Write(content []byte) (int, error) {
	if output.Len()+len(content) > 1<<20 {
		return 0, fmt.Errorf("output limit exceeded (1 MiB)")
	}
	return output.Buffer.Write(content)
}

var run js.Func

func main() {
	run = js.FuncOf(func(this js.Value, args []js.Value) any {
		if len(args) != 1 || args[0].Type() != js.TypeString {
			return map[string]any{"output": "", "error": "tigerRun expects one source string"}
		}
		var output limitedOutput
		err := evaluator.Run(args[0].String(), &output)
		message := ""
		if err != nil {
			message = err.Error()
		}
		return map[string]any{"output": output.String(), "error": message}
	})
	js.Global().Set("tigerRun", run)
	select {}
}
