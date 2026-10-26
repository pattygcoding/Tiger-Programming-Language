# Tiger Programming Language

Tiger is a dynamically typed language implemented in Rust. It uses Python-like expressions, explicit brace-delimited lexical scopes, and mandatory semicolons on simple statements. Source files use `.tg`.

The project includes a position-aware lexer, Pratt parser, typed AST, tree-walking interpreter, native CLI, standalone executable builder, browser playground, and WASI entry point. The workspace has one third-party dependency (`regex`, used by the native and WASI `regex` module).

## Native CLI

Run these commands from the repository root:

```sh
cargo test
cargo build --release
./target/release/tiger run examples/demo.tg
./target/release/tiger build examples/demo.tg -o bin/demo
./bin/demo
```

On Windows PowerShell:

```powershell
cargo build --release
.\target\release\tiger.exe run examples/demo.tg
.\target\release\tiger.exe build examples/demo.tg -o bin/demo.exe
.\bin\demo.exe
```

`tiger build <file.tg>` defaults to an executable named after the input in the current directory (`demo.exe` on Windows, `demo` elsewhere). `-o` can appear before or after the input. Building needs no compiler: it validates syntax, collects the script and every module it imports, and appends that bundle to a copy of the `tiger` binary. The resulting executable needs neither Rust, Tiger, this repository, nor the original script. This is a bundled interpreter, not an optimizing native-code compiler; runtime errors still occur when the executable runs.

Exit codes: `0` success, `1` file/syntax/runtime/build error, `2` invalid CLI arguments. Language diagnostics include `file:line:column`.

## Browser Playground

```sh
cargo run -p tiger-web
```

Open `http://127.0.0.1:7171` (the helper prints the URL and switches to a free port if 7171 is busy). Use `-addr 127.0.0.1:9000` to choose a specific port. The helper builds the browser runtime from `crates/tiger-wasm` before serving the playground and examples. No Node.js or external CDN is needed to run it.

The editor supports Run, Stop, Reset, example selection, source download, and local source persistence. Ctrl+Enter (Cmd+Enter on macOS) runs the current source. Programs execute in a Web Worker; stopping a program terminates that worker and creates a fresh runtime. Each run has a fresh environment.

### One-line Wasm build for your own website

```sh
cargo run -p tiger-web -- -build-only
```

(`make wasm` runs the same command.) This builds `web/tiger.wasm` from `crates/tiger-wasm`, no server required. Copy it plus `web/app.js`, `web/tiger.js`, `web/worker.js`, and the `examples/` and `portfolio-features/` directories into your own site.

The underlying Wasm build command is:

```sh
cargo build -p tiger-wasm --release --target wasm32-unknown-unknown
```

The release profile strips symbols, and if Binaryen's `wasm-opt` is on `PATH`, `cargo run -p tiger-web` also runs `wasm-opt -Oz --converge`. The browser build keeps `regex` and the host clock out, so `tiger.wasm` stays small. Serve it with gzip or Brotli: the binary is about 1,000 KB but about 300 KB compressed. See [Wasm Size](docs/running-and-building.md#wasm-size).

For static deployment, publish the contents of `web/` along with an `examples/` subdirectory containing the Tiger examples and a `portfolio-features/` subdirectory for the Portfolio menu entries. Serve over HTTP(S), not `file://`, and send `Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` so `input()` can wait for a line. `tigerRun(source, filename)` returns `{output, error}` inside the Wasm worker; output streams as it prints, and `input()` waits for a line typed into the output pane. Generated Wasm is ignored by Git.

## WASI

The browser target (`wasm32-unknown-unknown`) and WASI (`wasm32-wasip1`) have different host APIs. A separate entry point supports WASI preview 1:

```sh
cargo build -p tiger-wasi --release --target wasm32-wasip1
wasmtime run --dir . target/wasm32-wasip1/release/tiger-wasi.wasm examples/demo.tg
```

The WASI host must grant read access to the source file. The playground server and browser runner do not require a WASI host.

## Language

Import other Tiger files with `import "modules/math.tg" as math;`, `import * as math from "modules/math.tg";`, or the selective `import { square, circle_area as area } from "modules/math.tg";`, then use `math.square(9)` or the bound names directly. A file shares the declarations it marks with `export` and keeps the rest private, and `export import` re-exports a module. Imports resolve relative to the importing file and execute once per run in an isolated namespace. Standalone builds bundle their dependencies. See [Importing Tiger Files](docs/modules.md) and the [imports example](examples/imports.tg).

Formatted strings support Python-style interpolation: `print(f"Hello, {name}! Count: {len(items)}");`, and a field may carry a format spec such as `f"{value:.2f}"` or `f"{value:>8}"`. Use `{{` and `}}` for literal braces. `r"..."` strings keep backslashes literal, `\x`/`\u`/`\U` escapes name characters, and `graphemes(text)` groups text into approximate characters. See [Strings](docs/strings.md) for details and supported syntax.

Functions and methods use `function`. Declare variables with `const` for immutable bindings or `var` for mutable bindings; later assignments update an existing binding and cannot introduce a new name. Compound `+=`, `-=`, `*=`, and `%=` assignments are supported. The operator set also includes `**` exponentiation, `div` floor division, and the bitwise operators `&`, `|`, `^`, `~`, `<<`, and `>>`; see [Types and Operators](docs/types-and-operators.md). `const` prevents rebinding, not mutation of a referenced collection or instance. Parameters and `for name in ...` headers declare their own local bindings. Comments use `//` or `/* ... */`; legacy `def` definitions and `#` comments are no longer supported.

`print` accepts an optional string ending, such as `print("working", end="");`. `input("Name: ")` reads a line from standard input like Python's `input()`; see [Built-ins](docs/builtins.md#inputprompt). Python-style `int()`, `float()`, `str()`, and `bool()` convert values, and every value (including class instances) has `.ToString()`, `.ToInt()`, `.ToFloat()`, and `.ToBool()` methods; see [conversions](docs/builtins.md#intvalue-floatvalue-boolvalue). Use `type(value)` to read a value's type name and `isinstance(value, name)` to test it; see [the type built-ins](docs/builtins.md#typevalue-isinstancevalue-name). The built-in `math` module needs no import and provides `math.ceil`, `math.floor`, `math.round`, `math.sqrt`, `math.abs`, `math.pow`, `math.log`, `math.sin`, `math.cos`, `math.tan`, `math.min`, and `math.max`; see [the math module](docs/math.md). The built-in `algo` module provides `algo.fibonacci(n)`, `algo.fibonacciList(n)`, `algo.factorial(n)`, `algo.factorialList(n)`, `algo.removeDuplicates(list)`, `algo.findMatches(X, Y)`, `algo.takeInventory(list)`, `algo.findPlace(list, place)`, `algo.isPrime(x)`, `algo.rainwater(heights)`, `algo.medianSorted(nums1, nums2)`, `algo.mergeKSorted(lists)`, `algo.editDistance(word1, word2)`, `algo.regexMatch(text, pattern)`, and sliding-window helpers (`slidingWindowMax`, `slidingWindowSumMax`, `slidingWindowMinLen`, `slidingWindowLongestUnique`, `slidingWindowMinSubstring`), and palindrome helpers (`palindromeValid`, `palindromeCanBeValid`, `palindromeLongest`, `palindromeCount`, `palindromeMinCuts`); see [the algo module](docs/algo.md). The `random` module provides `random.seed`, `random.random`, `random.int`, `random.uniform`, `random.choice`, and `random.shuffle`; the native/WASI-only `secrets` module adds non-reproducible randomness (`secrets.random`, `secrets.int`, `secrets.uniform`, `secrets.choice`, `secrets.shuffle`) that needs no seeding; `json.parse` and `json.stringify` convert JSON; the native/WASI-only `time` and `regex` modules add clock and regular-expression helpers; `sys.argv`, `sys.env`, and `sys.exit(status)` report the program's own context — its arguments, its environment, and the status it exits with — so a script can read its command line; and `list_dir(path)` lists a directory. See [Standard Library Additions](docs/standard-library.md). Lists, tuples, sets, and dictionaries can report their length with `len(value)`, `value.size()`, or `value.length()`, which are the same operation under three spellings. A set (`{1, 2}`) keeps unique scalars with fast membership and `add`/`remove`/`has`/`union`/`intersection`/`difference`; a tuple (`(1, 2)`) is a fixed-length, immutable group that the sequence built-ins accept in place of a list, and `list(value)`, `tuple(value)`, and `set(value)` convert between them; see [Sets and Tuples](docs/collections.md#sets). Lists sort in place with Python-style `items.sort(key=..., reverse=...)`; see [Lists and Dictionaries](docs/collections.md#sorting). Reductions and reshaping are built in: `sum`, `min`, `max`, `any`, `all`, `enumerate`, `zip`, `map`, `filter`, and the non-mutating `sorted(key=..., reverse=...)`; see [Aggregate Built-ins](docs/builtins.md#aggregate-built-ins). A [comprehension](docs/collections.md#comprehensions) writes the same mapping and filtering as one expression: `[value * 2 for value in values if value > 0]` builds a list, and `{word: len(word) for word in words}` builds a dictionary. A [lazy iterator](docs/collections.md#lazy-iterators-and-generator-expressions) from `range`, `map`, `filter`, or a generator expression `(value * 2 for value in values)` produces its elements on demand, so a loop that stops early never builds the rest. Read and write files with `open`, `read_file`, `write_file`, `append_file`, `file_exists`, `remove_file`, and `list_dir`; see [Files](docs/file-io.md).

Functions and methods may declare `*args` to collect extra positional arguments and `**kwargs` to collect unmatched keyword arguments. A call may unpack a list with `*items`, unpack a dictionary with `**mapping`, or pass `name=value` keywords that bind to parameters by name. A parameter may declare a default (`greeting = "hello"`), `function(params) { ... }` used as a value is an anonymous function, `return a, b;` returns the list `[a, b]`, and `const [x, y] = pair();` or `const {name} = record;` destructures a value into bindings. See [Functions and Closures](docs/functions-and-closures.md).

Methods and constructors receive `this` automatically, replacing Python's explicit `self` parameter. Constructors are declared Java-style with the class name (`Scoreboard(name) { ... }`), and subclasses use `class Child extends Parent` and call `super(...)` to run the parent constructor. As in Java, instance fields must be declared with `var`/`const` in the class before they are assigned. Class methods and declared fields are public by default; optional `public`, `private`, and `protected` modifiers control access. Tiger also supports prefix/postfix `++`/`--`, the clause header `for (var index = 0; index < limit; ++index)` with optional parentheses, `if`/`elif`/`else if` headers with or without parentheses and `&&`, `||`, and `!` as spellings of `and`, `or`, and `not` anywhere, `range`, loop `else`, `do...while`, labeled `break`/`continue` (`outer: for ... { break outer; }`), slicing (`items[1:3]`, `text[::-1]`), conditional expressions (`"big" if size > 10 else "small"`), fall-through `switch` (also spelled `match`, with `case _:` as the wildcard), and `try`/`catch`/`finally`/`throw` (caught runtime errors are structured objects with `message`, `kind`, and `origin`). See the [highlighting reference](docs/syntax-highlighting.md) for editor integration and the bundled [VS Code extension](editors/vscode) that colors `.tg` files and ` ```tg ` Markdown blocks, the [control-flow example](examples/control_flow.tg), and the [vending machine](portfolio-features/vending_machine.tg) and [bank ledger](portfolio-features/bank_ledger.tg) portfolio programs.

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
| [vending_machine.tg](portfolio-features/vending_machine.tg) | a state machine with fall-through `switch`, `match`/`case _`, `if`/`else if`, `&&`/`||`, loop `else` |
| [inventory_report.tg](portfolio-features/inventory_report.tg) | CSV parsing with conversions, grouping, stable multi-key sorts, aligned tables |
| [game_of_life.tg](portfolio-features/game_of_life.tg) | grid simulation, structural equality, cycle detection |
| [file_journal.tg](portfolio-features/file_journal.tg) | every file API: helpers, `open` modes, `readline`, `seek`/`tell`, in-place edits |
| [interactive_quiz.tg](portfolio-features/interactive_quiz.tg) | `input()` with validation and retries, and a scripted fallback without a keyboard |
| [task_scheduler.tg](portfolio-features/task_scheduler.tg) | a binary-heap priority queue, dependency ordering, a multi-worker simulation |
| [expression_calculator.tg](portfolio-features/expression_calculator.tg) | a tokenizer, a recursive-descent parser, and an evaluator with variables |
| [turing_machine.tg](portfolio-features/turing_machine.tg) | a two-way tape class, rule tables passed as `*rules`, traced runs, busy beavers, halting limits |
| [connect_four.tg](portfolio-features/connect_four.tg) | a playable two-player console game: `input()` validation, re-prompting, win and tie detection |

## Regression Benchmarks

Run all forty-four reference-output programs, including OOP, control-flow, exception, variadic-call, function-ergonomics, aggregate/comprehension, lazy-iterator, text-precision, syntax-gap, file-I/O, list-sort, input, conversion, type-inspection, string-method, collection-method, set-and-tuple, module-export, operator, standard-library, math, and algo workloads:

```sh
cargo test -p tiger-conformance --test benchmarks -- --nocapture
```

`make benchmarks` runs the same checks; `cargo test` includes them too, along with execution checks for every example and portfolio-feature program. See [benchmarks/README.md](benchmarks/README.md) for coverage and filtering commands.

For timings, `make timing` reports each program in process, with no
process startup in the numbers, `make compare` times a paired Tiger/Python
workload on identical work, and `make protocol` times the same workload in
process on both sides and fails while any section is slower than CPython. See
the [performance guide](docs/performance.md) for the two engines, what the
compiler recognizes, what a site learns about its operands as the program runs,
and how a change is measured. [`docs/flamegraph.svg`](docs/flamegraph.svg) is a
measured flamegraph of the corpus and of the paired workload, generated by the
tool in `tools/flamegraph`, which is its own Cargo project and not a member of
the workspace.

## Layout

```text
crates/tiger-cli/          Native CLI: run, build, help
crates/tiger-wasm/         Browser runtime (wasm32-unknown-unknown)
crates/tiger-wasi/         WASI interpreter (wasm32-wasip1)
crates/tiger-web/          Playground build and HTTP helper
crates/tiger-format/       Number, string, and %-verb formatting
crates/tiger-lexer/        Tokens and scanning
crates/tiger-parser/       Pratt parser and syntax validation
crates/tiger-ast/          AST declarations and import collection
crates/tiger-object/       Runtime values and lexical environments
crates/tiger-evaluator/    Interpreter and built-in modules
crates/tiger-conformance/  Benchmark, documentation, and example suites
web/                       Browser editor, worker, and runtime glue
examples/                  Runnable .tg programs
docs/                      Tutorial and language reference with tested examples
benchmarks/                100-200-line reference-output regression programs
benchmarks/python/         Paired Tiger/Python workload and its comparison runner

```

`make build`, `make run`, `make test`, `make benchmarks`, `make wasm`, `make wasm-check`, `make startup`, `make web`, and `make wasi` wrap the commands above. Direct `cargo` commands work without Make.