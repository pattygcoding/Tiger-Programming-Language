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
	if err := evaluator.RunFile(filename, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
