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

Input paths must end in `.tg`. Build accepts `-o` before or after the source path. There is no interactive REPL or standard-input source mode; standard input is passed to the program, where `input()` reads it (for example, `tiger run app.tg < answers.txt`). Standalone executables and the WASI build also read standard input; the browser playground asks for each line in the output pane. CLI exit codes are `0` for success, `1` for file/language/build errors, and `2` for invalid arguments.

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

The helper builds `web/tiger.wasm`, copies the installed toolchain's matching `wasm_exec.js`, and serves the editor at `http://127.0.0.1:7171`. If that port is busy, it picks a free port and prints the URL. Use `-addr 127.0.0.1:9000` for a specific port, or `-addr 127.0.0.1:0` to always select an available port.

Build artifacts without serving:

```sh
go run ./cmd/web -build-only
```

The underlying browser build on POSIX shells is:

```sh
GOOS=js GOARCH=wasm go build -trimpath -ldflags="-s -w" -o web/tiger.wasm ./cmd/wasm
```

### Wasm Size

The helper keeps `tiger.wasm` small in three ways:

- `-ldflags="-s -w"` drops the symbol table and DWARF debug data, and `-trimpath` drops local file paths.
- The interpreter avoids heavyweight standard-library packages such as `regexp` and `math/big`. Number parsing and `algo.isPrime` use small hand-written code instead, which removes roughly 700 KB from the binary.
- If Binaryen's `wasm-opt` is on `PATH`, the helper also runs `wasm-opt -Oz` with the post-MVP features Go emits enabled. This saves roughly another 7% before compression. Install it from your package manager (for example `brew install binaryen`) or the [Binaryen releases](https://github.com/WebAssembly/binaryen/releases); without it, the step is skipped.

The biggest saving at download time is compression. The stripped binary is about 3.5 MB but about 1 MB with gzip, and smaller still with Brotli. Most static hosts compress `.wasm` automatically; otherwise enable gzip or Brotli for the `application/wasm` content type. The local `go run ./cmd/web` server does not compress.

The Go support script must come from the same toolchain as the Wasm binary. It lives under `lib/wasm` on newer Go releases or `misc/wasm` on older ones; the helper handles both locations.

The browser editor supports examples, Run, Stop, Reset, Clear, source download, and local source persistence. Ctrl+Enter or Cmd+Enter runs the source. Code executes in a worker, where `tigerRun(source)` returns a promise for an object with `output` and `error` strings. While a program runs, output streams to the page as it is printed, and each `input()` call shows a field at the end of the output: type a line and press Enter (or Ctrl+D for end of input) and the program continues. The bridge is not a main-page global. Each call gets a fresh evaluator environment and a fresh in-memory filesystem, so file I/O in the playground never touches the host disk (see [Files](file-io.md)).

For static deployment, publish the contents of `web/` and copy the example scripts into an `examples/` subdirectory beside the HTML, and the `portfolio-features/` directory beside it for the Portfolio entries in the example menu. Serve over HTTP(S), not `file://`. No Node.js server or CDN is required. Rebuild the Wasm artifacts after changing the interpreter.

## WASI Preview 1

WASI is a different host interface from browser `syscall/js`. Use the separate entry point:

```sh
GOOS=wasip1 GOARCH=wasm go build -trimpath -ldflags="-s -w" -o bin/tiger-wasi.wasm ./cmd/wasi
wasmtime run --dir . bin/tiger-wasi.wasm examples/demo.tg
```

The WASI runtime must be installed separately and grant access to the source file. File input and output reach only directories the host exposes; `wasmtime run --dir .` grants the current directory for both reading and writing. The WASI entry point accepts a `.tg` file directly; it does not expose the native CLI's `run` and `build` subcommands.

PowerShell cross-compilation, preserving existing environment settings:

```powershell
$previousGOOS = $env:GOOS
$previousGOARCH = $env:GOARCH
try {
    $env:GOOS = "wasip1"
    $env:GOARCH = "wasm"
    go build -trimpath -ldflags="-s -w" -o bin/tiger-wasi.wasm ./cmd/wasi
} finally {
    $env:GOOS = $previousGOOS
    $env:GOARCH = $previousGOARCH
}
```

Use `GOOS=js` and `./cmd/wasm` for the browser target, or use the cross-platform browser helper instead.

## Embed Tiger in Go

Within this Go module, the simplest API is `evaluator.Run(source, writer)`. It parses and executes source, returns an error on failure, and writes `print` output to the supplied `io.Writer`. Passing `nil` discards output.

Use `evaluator.RunFile(filename, writer)` for filesystem imports, or `RunWithLoader` with an explicit loader for virtual sources. The plain `Run` API does not read files. See [Importing Tiger Files](modules.md). Standalone builds bundle all literal import dependencies, including imports inside functions; WASI imports require the corresponding host filesystem access.

`RunWithOptions(source, filename, loader, files, writer)` also selects a filesystem. Passing `nil` uses the host disk, `evaluator.NewMemoryFileSystem()` keeps file I/O in memory, and an `Evaluator` also accepts a custom `FileSystem` for embedding. See [Files](file-io.md).

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

The docs test checks Tiger examples against the output shown in Markdown. The benchmark suite checks thirty-one 100-200-line programs against fixed output files. Neither suite is a performance threshold test. Compiler tests build and execute standalone programs; `go test -short ./...` skips those standalone integration builds.

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