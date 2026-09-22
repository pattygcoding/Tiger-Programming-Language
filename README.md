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

Import other Tiger files with `import "modules/math.tg" as math;`, then call `math.square(9)`. Imports resolve relative to the importing file and execute once per run in an isolated namespace. Standalone builds bundle their dependencies. See [Importing Tiger Files](docs/modules.md) and the [imports example](examples/imports.tg).

Formatted strings support Python-style interpolation: `print(f"Hello, {name}! Count: {len(items)}");`. Use `{{` and `}}` for literal braces. See [Strings](docs/strings.md) for details and supported syntax.

Functions and methods use `function`. Declare variables with `const` for immutable bindings or `var` for mutable bindings; later assignments update an existing binding and cannot introduce a new name. `const` prevents rebinding, not mutation of a referenced collection or instance. Parameters and `for name in ...` headers declare their own local bindings. Comments use `//` or `/* ... */`; legacy `def` definitions and `#` comments are no longer supported.

Methods explicitly receive `this`, replacing `self`. Class methods and declared fields are public by default; optional `public`, `private`, and `protected` modifiers control access. Tiger also supports prefix/postfix `++`/`--`, `cfor (var index = 0; index < limit; ++index)`, `range`, loop `else`, fall-through `switch`, `break`/`continue`, and `try`/`catch`/`throw`. See the [highlighting reference](docs/syntax-highlighting.md) for editor integration, the [control-flow example](examples/control_flow.tg), and the [batch queue](portfolio-features/batch_queue.tg) and [account audit](portfolio-features/account_audit.tg) portfolio programs.

```tg
class Scoreboard {
    function init(this, name) {
        this.name = name;
        this.points = 0;
    }
    function add(this, points) {
        this.points = this.points + points;
        return this.name + ": " + str(this.points);
    }
}

class DoubleScore(Scoreboard) {
    function add(this, points) { return super.add(points * 2); }
}

const score = DoubleScore("Tigers").add;
const rounds = [{"won": true, "points": 3}, {"won": true, "points": 4}];
for round in rounds {
    if round["won"] {
        print(score(round["points"]));
    }
}
```

Output:

```text
Tigers: 6
Tigers: 14
```

Explore the [Tiger tutorial and language reference](docs/README.md) for step-by-step lessons, runnable examples, and detailed semantics.

## Regression Benchmarks

Run all seventeen reference-output programs, including OOP, control-flow, and exception workloads:

```sh
go test ./benchmarks -count=1 -v
```

`make benchmarks` runs the same checks; `go test ./...` includes them too, along with execution checks for every example and portfolio-feature program. See [benchmarks/README.md](benchmarks/README.md) for coverage and filtering commands.

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
docs/           Tutorial and language reference with tested examples
benchmarks/     100-200-line reference-output regression programs
bundle.go       Embedded runtime sources for standalone builds
```

`make build`, `make run`, `make test`, `make wasm`, and `make web` wrap the commands above. `make wasi` uses POSIX environment-variable syntax. Direct Go commands work without Make.