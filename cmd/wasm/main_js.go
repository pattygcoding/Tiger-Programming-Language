package main

import (
	"bytes"
	"fmt"
	"io"
	"syscall/js"
	"tiger/pkg/evaluator"
)

// limitedOutput caps output at 1 MiB and streams each write to the optional tigerOutput hook.
type limitedOutput struct{ bytes.Buffer }

func (output *limitedOutput) Write(content []byte) (int, error) {
	if output.Len()+len(content) > 1<<20 {
		return 0, fmt.Errorf("output limit exceeded (1 MiB)")
	}
	if hook := js.Global().Get("tigerOutput"); hook.Type() == js.TypeFunction {
		hook.Invoke(string(content))
	}
	return output.Buffer.Write(content)
}

// WriteString shadows bytes.Buffer.WriteString, which io.WriteString would otherwise call directly.
func (output *limitedOutput) WriteString(text string) (int, error) {
	return output.Write([]byte(text))
}

// browserInput blocks the running program until tigerProvideInput delivers a line; null means end of input.
type browserInput struct {
	lines   chan *string
	pending []byte
}

func (input *browserInput) Read(buffer []byte) (int, error) {
	if len(input.pending) == 0 {
		js.Global().Call("tigerRequestInput")
		line := <-input.lines
		if line == nil {
			return 0, io.EOF
		}
		input.pending = []byte(*line + "\n")
	}
	count := copy(buffer, input.pending)
	input.pending = input.pending[count:]
	return count, nil
}

var currentInput *browserInput

var run, provideInput js.Func

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

func execute(source, filename string) map[string]any {
	var output limitedOutput
	var input io.Reader
	if js.Global().Get("tigerRequestInput").Type() == js.TypeFunction {
		currentInput = &browserInput{lines: make(chan *string, 64)}
		input = currentInput
	}
	var loader evaluator.ModuleLoader
	var err error
	if js.Global().Get("tigerResolveModule").Type() == js.TypeFunction && js.Global().Get("tigerLoadModule").Type() == js.TypeFunction {
		loader = browserLoader{}
		filename, err = loader.Resolve("", filename)
	}
	if err == nil {
		err = evaluator.RunWithInput(source, filename, loader, evaluator.NewMemoryFileSystem(), input, &output)
	}
	currentInput = nil
	message := ""
	if err != nil {
		message = err.Error()
	}
	return map[string]any{"output": output.String(), "error": message}
}

func main() {
	// tigerRun returns a Promise so a program can wait for input while the worker keeps receiving messages.
	run = js.FuncOf(func(this js.Value, args []js.Value) any {
		if len(args) < 1 || len(args) > 2 || args[0].Type() != js.TypeString || len(args) == 2 && args[1].Type() != js.TypeString {
			return js.Global().Get("Promise").Call("resolve", map[string]any{"output": "", "error": "tigerRun expects source and an optional source path"})
		}
		source := args[0].String()
		filename := "examples/playground.tg"
		if len(args) == 2 {
			filename = args[1].String()
		}
		var executor js.Func
		executor = js.FuncOf(func(this js.Value, callbacks []js.Value) any {
			resolve := callbacks[0]
			go func() {
				resolve.Invoke(execute(source, filename))
				executor.Release()
			}()
			return nil
		})
		return js.Global().Get("Promise").New(executor)
	})
	provideInput = js.FuncOf(func(this js.Value, args []js.Value) any {
		if currentInput == nil {
			return false
		}
		var line *string
		if len(args) > 0 && args[0].Type() == js.TypeString {
			text := args[0].String()
			line = &text
		}
		select {
		case currentInput.lines <- line:
			return true
		default:
			return false
		}
	})
	js.Global().Set("tigerRun", run)
	js.Global().Set("tigerProvideInput", provideInput)
	select {}
}
