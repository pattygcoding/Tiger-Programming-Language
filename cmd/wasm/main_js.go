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

type browserLoader struct{}

func (browserLoader) Resolve(importer, requested string) (string, error) {
	result := js.Global().Call("tigerResolveModule", importer, requested)
	if message := result.Get("error").String(); message != "" {
		return "", fmt.Errorf("%s", message)
	}
	return result.Get("path").String(), nil
}

func (browserLoader) Load(filename string) (string, error) {
	result := js.Global().Call("tigerLoadModule", filename)
	if message := result.Get("error").String(); message != "" {
		return "", fmt.Errorf("%s", message)
	}
	return result.Get("source").String(), nil
}

func main() {
	run = js.FuncOf(func(this js.Value, args []js.Value) any {
		if len(args) < 1 || len(args) > 2 || args[0].Type() != js.TypeString || len(args) == 2 && args[1].Type() != js.TypeString {
			return map[string]any{"output": "", "error": "tigerRun expects source and an optional source path"}
		}
		var output limitedOutput
		filename := "examples/playground.tg"
		if len(args) == 2 {
			filename = args[1].String()
		}
		var err error
		if js.Global().Get("tigerResolveModule").Type() == js.TypeFunction && js.Global().Get("tigerLoadModule").Type() == js.TypeFunction {
			loader := browserLoader{}
			filename, err = loader.Resolve("", filename)
			if err == nil {
				err = evaluator.RunWithLoader(args[0].String(), filename, loader, &output)
			}
		} else {
			err = evaluator.Run(args[0].String(), &output)
		}
		message := ""
		if err != nil {
			message = err.Error()
		}
		return map[string]any{"output": output.String(), "error": message}
	})
	js.Global().Set("tigerRun", run)
	select {}
}
