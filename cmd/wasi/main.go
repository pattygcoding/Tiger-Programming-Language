package main

import (
	"fmt"
	"os"
	"path/filepath"
	"tiger/pkg/evaluator"
)

func main() {
	if len(os.Args) != 2 || filepath.Ext(os.Args[1]) != ".tg" {
		fmt.Fprintln(os.Stderr, "Usage: tiger-wasi <file.tg>")
		os.Exit(2)
	}
	filename := os.Args[1]
	source, err := os.ReadFile(filename)
	if err == nil {
		err = evaluator.Run(string(source), os.Stdout)
	}
	if err != nil {
		fmt.Fprintf(os.Stderr, "%s:%v\n", filename, err)
		os.Exit(1)
	}
}
