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

Open `http://127.0.0.1:7171` (the helper prints the URL and switches to a free port if 7171 is busy). Use `-addr 127.0.0.1:9000` to choose a specific port. The helper builds Wasm and copies `wasm_exec.js` from the installed Go toolchain before serving the playground and examples. No Node.js or external CDN is needed.

The editor supports Run, Stop, Reset, example selection, source download, and local source persistence. Ctrl+Enter (Cmd+Enter on macOS) runs the current source. Programs execute in a Web Worker; stopping a program terminates that worker and creates a fresh runtime. Each run has a fresh environment.

### One-line Wasm build for your own website

```sh
go run ./cmd/web -build-only
```

(`make wasm` runs the same command.) This builds `web/tiger.wasm` and copies the matching `web/wasm_exec.js` from your installed Go toolchain, no server required. Copy both files, plus `web/app.js`, `web/worker.js`, and the `examples/` and `portfolio-features/` directories, into your own site.

The underlying Wasm build command on POSIX shells is:

```sh
GOOS=js GOARCH=wasm go build -trimpath -ldflags="-s -w" -o web/tiger.wasm ./cmd/wasm
```

`-ldflags="-s -w"` strips symbols and debug data. If Binaryen's `wasm-opt` is on `PATH`, `go run ./cmd/web` also runs `wasm-opt -Oz`. Serve `tiger.wasm` with gzip or Brotli: the binary is about 3.5 MB but about 1 MB compressed. See [Wasm Size](docs/running-and-building.md#wasm-size).

In PowerShell, set `$env:GOOS = "js"` and `$env:GOARCH = "wasm"` before the build, then remove them with `Remove-Item Env:GOOS, Env:GOARCH`. Always pair the generated Wasm with `wasm_exec.js` from the same Go toolchain (`lib/wasm` on newer Go releases, `misc/wasm` on older ones). The helper handles this automatically.

For static deployment, publish the contents of `web/` along with an `examples/` subdirectory containing the Tiger examples and a `portfolio-features/` subdirectory for the Portfolio menu entries. Serve over HTTP(S), not `file://`. `tigerRun(source)` returns a promise for `{output, error}` inside the Wasm worker; output streams as it prints, and `input()` waits for a line typed into the output pane. Generated Wasm and Go support files are ignored by Git.

## WASI

Browser Wasm (`GOOS=js`) and WASI (`GOOS=wasip1`) have different host APIs. A separate entry point supports WASI preview 1:

```sh
GOOS=wasip1 GOARCH=wasm go build -trimpath -ldflags="-s -w" -o bin/tiger-wasi.wasm ./cmd/wasi
wasmtime run --dir . bin/tiger-wasi.wasm examples/demo.tg
```

For PowerShell, set `GOOS` to `wasip1` and `GOARCH` to `wasm` as above, build, then remove the environment overrides. The WASI host must grant read access to the source file. The Go helper and browser runner do not require a WASI host.

## Language

Import other Tiger files with `import "modules/math.tg" as math;`, then call `math.square(9)`. Imports resolve relative to the importing file and execute once per run in an isolated namespace. Standalone builds bundle their dependencies. See [Importing Tiger Files](docs/modules.md) and the [imports example](examples/imports.tg).

Formatted strings support Python-style interpolation: `print(f"Hello, {name}! Count: {len(items)}");`. Use `{{` and `}}` for literal braces. See [Strings](docs/strings.md) for details and supported syntax.

Functions and methods use `function`. Declare variables with `const` for immutable bindings or `var` for mutable bindings; later assignments update an existing binding and cannot introduce a new name. Compound `+=`, `-=`, `*=`, and `%=` assignments are supported. `const` prevents rebinding, not mutation of a referenced collection or instance. Parameters and `for name in ...` headers declare their own local bindings. Comments use `//` or `/* ... */`; legacy `def` definitions and `#` comments are no longer supported.

`print` accepts an optional string ending, such as `print("working", end="");`. `input("Name: ")` reads a line from standard input like Python's `input()`; see [Built-ins](docs/builtins.md#inputprompt). Python-style `int()`, `float()`, `str()`, and `bool()` convert values, and every value (including class instances) has `.ToString()`, `.ToInt()`, `.ToFloat()`, and `.ToBool()` methods; see [conversions](docs/builtins.md#intvalue-floatvalue-boolvalue). The built-in `math` module needs no import and provides `math.ceil`, `math.floor`, `math.round`, `math.sqrt`, `math.abs`, `math.pow`, `math.log`, `math.sin`, `math.cos`, `math.tan`, `math.min`, and `math.max`; see [the math module](docs/math.md). The built-in `algo` module provides `algo.fibonacci(n)`, `algo.fibonacciList(n)`, `algo.factorial(n)`, `algo.factorialList(n)`, `algo.removeDuplicates(list)`, `algo.findMatches(X, Y)`, `algo.takeInventory(list)`, `algo.findPlace(list, place)`, `algo.isPrime(x)`, `algo.rainwater(heights)`, `algo.medianSorted(nums1, nums2)`, `algo.mergeKSorted(lists)`, `algo.editDistance(word1, word2)`, `algo.regexMatch(text, pattern)`, and sliding-window helpers (`slidingWindowMax`, `slidingWindowSumMax`, `slidingWindowMinLen`, `slidingWindowLongestUnique`, `slidingWindowMinSubstring`), and palindrome helpers (`palindromeValid`, `palindromeCanBeValid`, `palindromeLongest`, `palindromeCount`, `palindromeMinCuts`); see [the algo module](docs/algo.md). Lists and strings can report their length with `len(value)`, `value.size()`, or `value.length()`. Lists sort in place with Python-style `items.sort(key=..., reverse=...)`; see [Lists and Dictionaries](docs/collections.md#sorting). Read and write files with `open`, `read_file`, `write_file`, `append_file`, `file_exists`, and `remove_file`; see [Files](docs/file-io.md).

Functions and methods may declare `*args` to collect extra positional arguments and `**kwargs` to collect unmatched keyword arguments. A call may unpack a list with `*items`, unpack a dictionary with `**mapping`, or pass `name=value` keywords that bind to parameters by name. See [Functions and Closures](docs/functions-and-closures.md).

Methods and constructors receive `this` automatically, replacing Python's explicit `self` parameter. Constructors are declared Java-style with the class name (`Scoreboard(name) { ... }`), and subclasses use `class Child extends Parent` and call `super(...)` to run the parent constructor. As in Java, instance fields must be declared with `var`/`const` in the class before they are assigned. Class methods and declared fields are public by default; optional `public`, `private`, and `protected` modifiers control access. Tiger also supports prefix/postfix `++`/`--`, `cfor (var index = 0; index < limit; ++index)`, C-style `cif (...) { } celse cif (...) { } celse { }` with `&&`/`||` in `cif` and `cfor` headers, `range`, loop `else`, fall-through `switch`, `break`/`continue`, and `try`/`catch`/`throw`. See the [highlighting reference](docs/syntax-highlighting.md) for editor integration and the bundled [VS Code extension](editors/vscode) that colors `.tg` files and ` ```tg ` Markdown blocks, the [control-flow example](examples/control_flow.tg), and the [vending machine](portfolio-features/vending_machine.tg) and [bank ledger](portfolio-features/bank_ledger.tg) portfolio programs.

```tg
class Scoreboard {
    var name;
    var points = 0;
    Scoreboard(name) {
        this.name = name;
    }
    function add(points) {
        this.points = this.points + points;
        return this.name + ": " + str(this.points);
    }
}

class DoubleScore extends Scoreboard {
    function add(points) { return super.add(points * 2); }
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

## Portfolio Programs

The [portfolio-features](portfolio-features/) directory holds complete programs, each 100-150 lines, that exercise the language end to end. Run any of them with `tiger run`:

| Program | Highlights |
| --- | --- |
| [bank_ledger.tg](portfolio-features/bank_ledger.tg) | three-level inheritance, `super(...)`, private/protected members, exception objects |
| [shape_gallery.tg](portfolio-features/shape_gallery.tg) | polymorphism, the `math` module, sorting by key, a text bar chart |
| [functional_toolkit.tg](portfolio-features/functional_toolkit.tg) | closures, compose, memoization, an event emitter, `*args`/`**kwargs` unpacking |
| [text_studio.tg](portfolio-features/text_studio.tg) | hand-built string utilities, word statistics, ciphers, word wrap, f-strings |
| [matrix_lab.tg](portfolio-features/matrix_lab.tg) | a variadic `Matrix` class, recursive determinants, matrix powers, rotations |
| [vending_machine.tg](portfolio-features/vending_machine.tg) | a state machine with fall-through `switch`, `cif`/`celse`, `&&`/`||`, loop `else` |
| [inventory_report.tg](portfolio-features/inventory_report.tg) | CSV parsing with conversions, grouping, stable multi-key sorts, aligned tables |
| [game_of_life.tg](portfolio-features/game_of_life.tg) | grid simulation, structural equality, cycle detection |
| [file_journal.tg](portfolio-features/file_journal.tg) | every file API: helpers, `open` modes, `readline`, `seek`/`tell`, in-place edits |
| [interactive_quiz.tg](portfolio-features/interactive_quiz.tg) | `input()` with validation and retries, and a scripted fallback without a keyboard |
| [task_scheduler.tg](portfolio-features/task_scheduler.tg) | a binary-heap priority queue, dependency ordering, a multi-worker simulation |
| [expression_calculator.tg](portfolio-features/expression_calculator.tg) | a tokenizer, a recursive-descent parser, and an evaluator with variables |
| [turing_machine.tg](portfolio-features/turing_machine.tg) | a two-way tape class, rule tables passed as `*rules`, traced runs, busy beavers, halting limits |
| [connect_four.tg](portfolio-features/connect_four.tg) | a playable two-player console game: `input()` validation, re-prompting, win and tie detection |

## Regression Benchmarks

Run all thirty-one reference-output programs, including OOP, control-flow, exception, variadic-call, file-I/O, list-sort, input, conversion, math, and algo workloads:

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