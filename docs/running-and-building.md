# Running and Building

[Guide index](README.md) | [Getting Started](getting-started.md) | [Runtime and Errors](runtime-and-errors.md)

All commands below run from the repository root unless stated otherwise. Development requires Go 1.22 or newer. The Go module has no third-party dependencies.

## Native CLI

On POSIX shells:

```sh
go build -o bin/tiger ./cmd/tiger
./bin/tiger run examples/demo.tg
./bin/tiger run examples/oop.tg
```

On Windows PowerShell:

```powershell
go build -o bin/tiger.exe ./cmd/tiger
.\bin\tiger.exe run examples/demo.tg
.\bin\tiger.exe run examples/oop.tg
```

| Command | Purpose |
| --- | --- |
| `tiger run <file.tg>` | Parse and execute a source file |
| `tiger build <file.tg> [-o <executable>]` | Produce a standalone executable |
| `tiger help`, `tiger -h`, `tiger --help` | Display usage |

Input paths must end in `.tg`. Build accepts `-o` before or after the source path. There is no interactive REPL or standard-input source mode. CLI exit codes are `0` for success, `1` for file/language/build errors, and `2` for invalid arguments.

## Standalone Executables

```powershell
.\bin\tiger.exe build examples/oop.tg -o bin/oop.exe
.\bin\oop.exe
```

On POSIX shells, use `./bin/tiger build examples/oop.tg -o bin/oop` and then `./bin/oop`. Without `-o`, the output uses the source basename in the current directory: `oop.exe` on Windows or `oop` elsewhere.

Building requires Go on `PATH`. Tiger validates syntax, copies its embedded runtime sources and source program into a temporary Go module, and invokes `go build`. The executable bundles a tree-walking interpreter; it is not an optimizing translation of Tiger into native instructions. Runtime errors remain runtime errors, even if the build succeeds.

The result needs neither Go, Tiger, the repository, nor the original source file when executed. It targets the platform selected by the Go build environment. The temporary module is removed after building. Building does not execute the Tiger program.

## Browser WebAssembly

```sh
go run ./cmd/web
```

The helper builds `web/tiger.wasm`, copies the installed toolchain's matching `wasm_exec.js`, and serves the editor at `http://127.0.0.1:8080`. Use `-addr 127.0.0.1:8081` for another port, or `-addr 127.0.0.1:0` to select an available port and print its URL.

Build artifacts without serving:

```sh
go run ./cmd/web -build-only
```

The underlying browser build on POSIX shells is:

```sh
GOOS=js GOARCH=wasm go build -o web/tiger.wasm ./cmd/wasm
```

The Go support script must come from the same toolchain as the Wasm binary. It lives under `lib/wasm` on newer Go releases or `misc/wasm` on older ones; the helper handles both locations.

The browser editor supports examples, Run, Stop, Reset, Clear, source download, and local source persistence. Ctrl+Enter or Cmd+Enter runs the source. Code executes in a worker, where `tigerRun(source)` returns an object with `output` and `error` strings. The bridge is not a main-page global. Each call gets a fresh evaluator environment.

For static deployment, publish the contents of `web/` and copy the example scripts into an `examples/` subdirectory beside the HTML. Serve over HTTP(S), not `file://`. No Node.js server or CDN is required. Rebuild the Wasm artifacts after changing the interpreter.

## WASI Preview 1

WASI is a different host interface from browser `syscall/js`. Use the separate entry point:

```sh
GOOS=wasip1 GOARCH=wasm go build -o bin/tiger-wasi.wasm ./cmd/wasi
wasmtime run --dir . bin/tiger-wasi.wasm examples/demo.tg
```

The WASI runtime must be installed separately and grant access to the source file. The WASI entry point accepts a `.tg` file directly; it does not expose the native CLI's `run` and `build` subcommands.

PowerShell cross-compilation, preserving existing environment settings:

```powershell
$previousGOOS = $env:GOOS
$previousGOARCH = $env:GOARCH
try {
    $env:GOOS = "wasip1"
    $env:GOARCH = "wasm"
    go build -o bin/tiger-wasi.wasm ./cmd/wasi
} finally {
    $env:GOOS = $previousGOOS
    $env:GOARCH = $previousGOARCH
}
```

Use `GOOS=js` and `./cmd/wasm` for the browser target, or use the cross-platform browser helper instead.

## Embed Tiger in Go

Within this Go module, the simplest API is `evaluator.Run(source, writer)`. It parses and executes source, returns an error on failure, and writes `print` output to the supplied `io.Writer`. Passing `nil` discards output.

For a configurable step budget, parse once and execute explicitly:

```go
package main

import (
    "log"
    "os"
    "tiger/pkg/evaluator"
    "tiger/pkg/parser"
)

func main() {
    program, err := parser.Parse(`print(6 * 7);`)
    if err != nil {
        log.Fatal(err)
    }
    engine := evaluator.New(os.Stdout)
    engine.MaxSteps = 100_000
    if err := engine.Execute(program); err != nil {
        log.Fatal(err)
    }
}
```

`Execute` resets the step counter and creates a fresh environment each time. An evaluator has mutable execution state; do not share one concurrently. There is no API here for persisting Tiger bindings across separate executions.

## Tests and Make Targets

```sh
go test ./...
go test ./docs -count=1 -v
go test ./benchmarks -count=1 -v
go vet ./...
```

The docs test checks Tiger examples against the output shown in Markdown. The benchmark suite checks fifteen 100-200-line programs against fixed output files. Neither suite is a performance threshold test. Compiler tests build and execute standalone programs; `go test -short ./...` skips those standalone integration builds.

| Make target | Purpose |
| --- | --- |
| `make build` | Native CLI |
| `make run` | Build CLI and run the factorial demo |
| `make test` | Full Go test suite |
| `make benchmarks` | Verbose, uncached reference-output checks |
| `make wasm` | Build browser Wasm and support script |
| `make web` | Build and serve the browser playground |
| `make wasi` | Build WASI using POSIX environment syntax |

Direct Go commands work without Make. See the [project layout](../README.md) for implementation packages.