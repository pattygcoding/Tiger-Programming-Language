package main

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"tiger/pkg/compiler"
	"tiger/pkg/evaluator"
)

const usage = `Usage:
  tiger run <file.tg>
  tiger build <file.tg> [-o <executable>]
  tiger help
`

func main() { os.Exit(run(os.Args[1:], os.Stdout, os.Stderr)) }

func run(args []string, stdout, stderr io.Writer) int {
	if len(args) == 1 && (args[0] == "help" || args[0] == "-h" || args[0] == "--help") {
		fmt.Fprint(stdout, usage)
		return 0
	}
	if len(args) < 2 || args[0] != "run" && args[0] != "build" {
		fmt.Fprint(stderr, usage)
		return 2
	}
	var filename, output string
	for index := 1; index < len(args); index++ {
		arg := args[index]
		if args[0] == "build" && arg == "-o" {
			if output != "" || index+1 == len(args) {
				fmt.Fprintln(stderr, "build requires one output path after -o")
				return 2
			}
			index++
			output = args[index]
		} else if strings.HasPrefix(arg, "-") || filename != "" {
			fmt.Fprint(stderr, usage)
			return 2
		} else {
			filename = arg
		}
	}
	if filepath.Ext(filename) != ".tg" {
		fmt.Fprintln(stderr, "input must be a .tg file")
		return 2
	}
	if args[0] == "run" {
		if err := evaluator.RunFile(filename, stdout); err != nil {
			fmt.Fprintln(stderr, err)
			return 1
		}
		return 0
	}
	source, err := os.ReadFile(filename)
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}
	if output == "" {
		output = strings.TrimSuffix(filepath.Base(filename), ".tg")
		if runtime.GOOS == "windows" {
			output += ".exe"
		}
	}
	inputPath, inputErr := filepath.Abs(filename)
	outputPath, outputErr := filepath.Abs(output)
	if inputErr != nil || outputErr != nil || sameFile(inputPath, outputPath) {
		fmt.Fprintln(stderr, "output must be a valid path different from the input file")
		return 2
	}
	if err := compiler.Build(string(source), filename, output); err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}
	fmt.Fprintln(stdout, "Built", output)
	return 0
}

func sameFile(first, second string) bool {
	if first == second || runtime.GOOS == "windows" && strings.EqualFold(first, second) {
		return true
	}
	firstInfo, firstErr := os.Stat(first)
	secondInfo, secondErr := os.Stat(second)
	return firstErr == nil && secondErr == nil && os.SameFile(firstInfo, secondInfo)
}
