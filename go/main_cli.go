//go:build !js

package main

import (
	"bufio"
	"fmt"
	"io/ioutil"
	"os"
	"strings"
	"tiger/go/eval"
	"tiger/go/lexer"
	"tiger/go/parser"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Println("Tiger Programming Language CLI")
		fmt.Println("Usage:")
		fmt.Println("  tiger run <file.tg>  - Run a Tiger file")
		fmt.Println("  tiger repl          - Start interactive REPL")
		return
	}

	command := os.Args[1]
	
	switch command {
	case "run":
		if len(os.Args) < 3 {
			fmt.Println("Error: Please specify a file to run")
			fmt.Println("Usage: tiger run <file.tg>")
			return
		}
		runFile(os.Args[2])
	case "repl":
		runRepl()
	default:
		fmt.Printf("Unknown command: %s\n", command)
		fmt.Println("Available commands: run, repl")
	}
}

func runFile(filename string) {
	content, err := ioutil.ReadFile(filename)
	if err != nil {
		fmt.Printf("Error reading file %s: %v\n", filename, err)
		return
	}

	code := string(content)
	result := executeCode(code)
	fmt.Print(result)
}

func runRepl() {
	fmt.Println("🐯 Tiger Language REPL")
	fmt.Println("Type 'exit' to quit")

	scanner := bufio.NewScanner(os.Stdin)
	env := eval.NewEnvironment()

	for {
		fmt.Print(">>> ")
		if !scanner.Scan() {
			break
		}

		input := strings.TrimSpace(scanner.Text())
		if input == "exit" {
			break
		}

		if input == "" {
			continue
		}

		l := lexer.New(input)
		p := parser.New(l)
		program := p.ParseProgram()
		result := eval.Eval(program, env)
		
		if result != "" {
			fmt.Println(result)
		}
	}

	fmt.Println("Goodbye!")
}

func executeCode(code string) string {
	l := lexer.New(code)
	p := parser.New(l)
	program := p.ParseProgram()
	env := eval.NewEnvironment()
	return eval.Eval(program, env)
}