package main

import (
	"fmt"
	"syscall/js"
	"tiger/go/eval"
	"tiger/go/lexer"
	"tiger/go/parser"
)

func evalTiger(this js.Value, args []js.Value) interface{} {
	code := args[0].String()
	l := lexer.New(code)
	p := parser.New(l)
	program := p.ParseProgram()
	env := eval.NewEnvironment()
	result := eval.Eval(program, env)
	return js.ValueOf(result)
}

func main() {
	fmt.Println("Tiger interpreter started")
	js.Global().Set("evalTiger", js.FuncOf(evalTiger))
	select {}
}
