# Tiger Programming Language

Tiger is a dynamically typed language implemented in Go 1.22+. It uses Python-like expressions, explicit brace-delimited lexical scopes, and mandatory semicolons on simple statements. Source files use `.tg`.

The project includes a position-aware lexer, Pratt parser, typed AST, tree-walking interpreter, native CLI, standalone executable builder, browser playground, and WASI entry point. It has no third-party Go dependencies.

## Native CLI

Run these commands from the repository root:

```sh
go test ./...
go build -o bin/tiger ./cmd/tiger
./bin/tiger run examples/demo.tg
./bin/tiger build examples/demo.tg -o bin/demo
./bin/demo
```

On Windows PowerShell:

```powershell
go build -o bin/tiger.exe ./cmd/tiger
.\bin\tiger.exe run examples/demo.tg
.\bin\tiger.exe build examples/demo.tg -o bin/demo.exe
.\bin\demo.exe
```

`tiger build <file.tg>` defaults to an executable named after the input in the current directory (`demo.exe` on Windows, `demo` elsewhere). `-o` can appear before or after the input. Building requires Go 1.22+ on `PATH`. It validates syntax, embeds the script and interpreter sources in a temporary Go module, and invokes `go build`. The resulting executable needs neither Go, Tiger, this repository, nor the original script. This is a bundled interpreter, not an optimizing native-code compiler; runtime errors still occur when the executable runs.

Exit codes: `0` success, `1` file/syntax/runtime/build error, `2` invalid CLI arguments. Language diagnostics include `file:line:column`.

## Browser Playground

```sh
go run ./cmd/web
```

Open `http://127.0.0.1:8080`. Use `-addr 127.0.0.1:8081` to choose a different port. The helper builds Wasm and copies `wasm_exec.js` from the installed Go toolchain before serving the playground and examples. No Node.js or external CDN is needed.

The editor supports Run, Stop, Reset, example selection, source download, and local source persistence. Ctrl+Enter (Cmd+Enter on macOS) runs the current source. Programs execute in a Web Worker; stopping a program terminates that worker and creates a fresh runtime. Each run has a fresh environment.

Build browser artifacts without starting a server:

```sh
go run ./cmd/web -build-only
```

The underlying Wasm build command on POSIX shells is:

```sh
GOOS=js GOARCH=wasm go build -o web/tiger.wasm ./cmd/wasm
```

In PowerShell, set `$env:GOOS = "js"` and `$env:GOARCH = "wasm"` before the build, then remove them with `Remove-Item Env:GOOS, Env:GOARCH`. Always pair the generated Wasm with `wasm_exec.js` from the same Go toolchain (`lib/wasm` on newer Go releases, `misc/wasm` on older ones). The helper handles this automatically.

For static deployment, publish the contents of `web/` along with an `examples/` subdirectory containing the Tiger examples. Serve over HTTP(S), not `file://`. `tigerRun(source)` returns `{output, error}` inside the Wasm worker. Generated Wasm and Go support files are ignored by Git.

## WASI

Browser Wasm (`GOOS=js`) and WASI (`GOOS=wasip1`) have different host APIs. A separate entry point supports WASI preview 1:

```sh
GOOS=wasip1 GOARCH=wasm go build -o bin/tiger-wasi.wasm ./cmd/wasi
wasmtime run --dir . bin/tiger-wasi.wasm examples/demo.tg
```

For PowerShell, set `GOOS` to `wasip1` and `GOARCH` to `wasm` as above, build, then remove the environment overrides. The WASI host must grant read access to the source file. The Go helper and browser runner do not require a WASI host.

## Language

```tg
const MAX_RUNS = 5;

def factorial(n) {
    if n <= 1 {
        return 1;
    }
    return n * factorial(n - 1);
}

for item in [1, 2, 3, 4, 5] {
    print("factorial(" + str(item) + ") = " + str(factorial(item)));
}
```

- Assignments, constant declarations, returns, and expression statements end in `;`. Newlines never substitute for semicolons.
- Functions, `if`/`elif`/`else`, `while`, and `for` use `{ ... }`, never colon-and-indent syntax. As in the specification's examples, a closing block brace does not require a semicolon; an optional one is accepted.
- Assignment updates the nearest existing binding; a new name belongs to the current block. New names do not escape functions, branches, or loop iterations. Each loop iteration gets a new block scope. Loop variables and parameters are local bindings. Closures retain lexical environments and support recursion.
- `const` declares a binding in the current scope and rejects reassignment, including from nested scopes. Explicit declarations and parameters may shadow outer bindings. Constants prevent rebinding, not mutation of a referenced list or dictionary. Duplicate declarations in the same scope are errors.
- Values: finite 64-bit floating-point numbers, single- or double-quoted strings, `true`, `false`, `null`, lists, dictionaries, and functions. Numbers follow float64 precision, not Python's arbitrary-precision integers.
- Arithmetic: `+ - * / %`; `%` follows the divisor's sign. Comparisons: `== != < <= > >=`. Boolean operators: `and or not`, with short-circuit operand-returning `and`/`or`. Comparison chains are not Python-style chains; write `a < b and b < c`.
- `+` adds numbers or concatenates two strings or two lists. Mixed-type arithmetic is an error; use `str` for conversion. List/dictionary equality is structural, with cycle protection.
- Falsey values: `false`, `null`, zero, and empty strings/lists/dictionaries. All other values are truthy.
- Lists and strings support integer indexing, including negative indices. String indexing and `len` count Unicode code points. Lists and dictionaries support indexed assignment; strings are immutable.
- Dictionary keys may be strings, numbers, or booleans; these are distinct key types. Dictionaries preserve insertion order. Repeated keys replace the value without changing order. Missing keys and invalid indices are errors.
- `for` iterates a snapshot of list elements, string code points, or dictionary keys. `in` tests list membership, dictionary keys, or substrings.
- Built-ins: `print(...values)` writes space-separated values and a newline; `str(value)` converts to text; `len(value)` returns the length of a string, list, or dictionary. Built-in bindings are constant.
- Comments start with `#` or `//`. String escapes: `\n`, `\r`, `\t`, `\\`, `\"`, `\'`. Whitespace outside strings is stylistic.
- Bare `return;` and functions without a return produce `null`. A return outside a function is a syntax error.

The runtime defaults to 1,000,000 evaluation steps and a depth limit of 512. Embedded callers may set `Evaluator.MaxSteps` (`0` disables the step limit). The browser additionally caps output at 1 MiB. These limits catch common runaway programs; they are not a security or memory-isolation guarantee for hostile code.

## Layout

```text
cmd/tiger/       Native CLI
cmd/wasm/        Browser syscall/js bridge
cmd/wasi/        WASI interpreter
cmd/web/         Cross-platform Wasm build and HTTP helper
pkg/lexer/      Tokens and scanning
pkg/parser/     Pratt parser and syntax validation
pkg/ast/        AST declarations
pkg/object/     Runtime values and lexical environments
pkg/evaluator/  Interpreter and built-ins
pkg/compiler/   Standalone executable builder
web/            Browser editor and worker
examples/       Runnable .tg programs
bundle.go       Embedded runtime sources for standalone builds
```

`make build`, `make run`, `make test`, `make wasm`, and `make web` wrap the commands above. `make wasi` uses POSIX environment-variable syntax. Direct Go commands work without Make.