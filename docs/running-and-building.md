# Running and Building

[Guide index](README.md) | [Getting Started](getting-started.md) | [Runtime and Errors](runtime-and-errors.md)

All commands below run from the repository root unless stated otherwise.
Development requires a stable Rust toolchain; the workspace has one
third-party dependency (`regex`, used by the native and WASI `regex` module).

## Crates

| Crate | Purpose |
| --- | --- |
| `crates/tiger-format` | Number, string, and `%`-verb formatting |
| `crates/tiger-lexer` | Tokenizer |
| `crates/tiger-ast` | AST and literal import collection |
| `crates/tiger-parser` | Parser |
| `crates/tiger-object` | Values, environments, classes, modules |
| `crates/tiger-evaluator` | Tree-walking interpreter and standard library |
| `crates/tiger-cli` | The `tiger` binary: `run`, `build`, and `help` |
| `crates/tiger-wasi` | WASI Preview 1 entry point |
| `crates/tiger-wasm` | Browser runtime (`wasm32-unknown-unknown`) |
| `crates/tiger-web` | Playground build and server |
| `crates/tiger-conformance` | Benchmark, documentation, and example suites |

## Native CLI

```sh
cargo build --release
./target/release/tiger run examples/demo.tg
./target/release/tiger run examples/oop.tg
```

On Windows PowerShell the binary is `.\target\release\tiger.exe`.

| Command | Purpose |
| --- | --- |
| `tiger run <file.tg> [arguments...]` | Parse and execute a source file, passing the arguments to the program |
| `tiger build <file.tg> [-o <executable>]` | Produce a standalone executable |
| `tiger help`, `tiger -h`, `tiger --help` | Display usage |

Input paths must end in `.tg`. Build accepts `-o` before or after the source path. Everything after the source path belongs to the program: `tiger run greet.tg Ada Grace` makes `sys.argv` the list `["greet.tg", "Ada", "Grace"]`, and a built executable does the same with its own path first. There is no interactive REPL or standard-input source mode; standard input is passed to the program, where `input()` reads it (for example, `tiger run app.tg < answers.txt`). Standalone executables and the WASI build also read standard input; the browser playground asks for each line in the output pane. CLI exit codes are `0` for success, `1` for file/language/build errors, `2` for invalid arguments, and otherwise the status the program chose with `sys.exit`, whose message (when the argument is not an integer) is printed on standard error first. See [`sys`](standard-library.md#sys).

## Standalone Executables

```powershell
.\target\release\tiger.exe build examples\oop.tg -o bin\oop.exe
.\bin\oop.exe
```

On POSIX shells, use `./target/release/tiger build examples/oop.tg -o bin/oop` and then `./bin/oop`. Without `-o`, the output uses the source basename in the current directory: `oop.exe` on Windows or `oop` elsewhere.

Building needs no compiler: `tiger build` validates the syntax, collects the
program and every module it imports, and appends that bundle to a copy of the
`tiger` binary. The result is a self-contained executable that needs neither
Rust, Tiger, the repository, nor the original source file, and it targets the
platform the copy was taken on. It bundles a tree-walking interpreter; it is not
an optimizing translation of Tiger into native instructions. Runtime errors
remain runtime errors, even if the build succeeds. Building does not execute the
Tiger program.
The output keeps the permissions of the `tiger` binary it was copied from, so on
POSIX systems it is executable as soon as the build writes it; on Windows the
`.exe` suffix is what makes it runnable.


## Browser WebAssembly

```sh
cargo run -p tiger-web
```

The helper builds `crates/tiger-wasm` for `wasm32-unknown-unknown`, copies the
result to `web/tiger.wasm`, and serves the editor at `http://127.0.0.1:7171`. If
that port is busy, it picks a free port and prints the URL. Use `-addr
127.0.0.1:9000` for a specific port, or `-addr 127.0.0.1:0` to always select an
available port. The server sends `Cross-Origin-Opener-Policy` and
`Cross-Origin-Embedder-Policy`, which the runtime needs to wait for input.

Build the artifact without serving:

```sh
cargo run -p tiger-web -- -build-only
```

(`make wasm` runs the same command.) The underlying build is:

```sh
cargo build -p tiger-wasm --release --target wasm32-unknown-unknown
```

`web/tiger.js` instantiates `web/tiger.wasm` directly through its `extern "C"`
ABI — there is no `wasm_exec.js` and no bindgen step — and `web/worker.js` drives
it in a module worker. The browser build omits the `regex`, `time`, and `secrets`
modules and raises the documented error for each; everything else, including the
in-memory filesystem, is available.

### Wasm Size

The runtime stays small because of the release profile and the module split:

- `[profile.release]` sets `opt-level = 3`, `lto = "fat"`, and
  `codegen-units = 1`, and strips the symbol table and debug data with
  `strip = "symbols"`. Link-time optimization and a single codegen unit trade a
  longer build for a faster binary, and they also make `tiger.wasm` smaller.
- `regex`, the host clock, and the host entropy source live behind a
  `#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]` split with stubs
  that report the documented error, so neither the `regex` crate, `SystemTime`,
  nor the entropy source reaches the browser. The same split covers `time.iso`,
  `time.parts`, and the whole `secrets` module, which the native build computes
  itself.
- If Binaryen's `wasm-opt` is on `PATH`, the build also runs `wasm-opt -Oz
  --converge` with the debug and producer sections stripped. Install it from your
  package manager (for example `brew install binaryen`) or the [Binaryen
  releases](https://github.com/WebAssembly/binaryen/releases); without it, the
  step is skipped.

The stripped binary is about 915 KB, about 287 KB with gzip, and smaller still
with Brotli. Most static hosts compress `.wasm` automatically; otherwise enable
gzip or Brotli for the `application/wasm` content type. The local `cargo run -p
tiger-web` server does not compress.

The browser editor supports examples, Run, Stop, Reset, Clear, source download, and local source persistence. Ctrl+Enter or Cmd+Enter runs the source. Code executes in a worker, where `tigerRun(source, filename)` returns an object with `output` and `error` strings. While a program runs, output streams to the page as it is printed, and each `input()` call shows a field at the end of the output: type a line and press Enter (or Ctrl+D for end of input) and the program continues. Each call gets a fresh evaluator environment and a fresh in-memory filesystem, so file I/O in the playground never touches the host disk (see [Files](file-io.md)).

For static deployment, publish the contents of `web/` and copy the example scripts into an `examples/` subdirectory beside the HTML, and the `portfolio-features/` directory beside it for the Portfolio entries in the example menu. Serve over HTTP(S), not `file://`, and send the two isolation headers above. Rebuild the Wasm artifact after changing the interpreter.


## WASI Preview 1

WASI is a different host interface from the browser target. Use the separate entry point:

```sh
cargo build -p tiger-wasi --release --target wasm32-wasip1
wasmtime run --dir . target/wasm32-wasip1/release/tiger-wasi.wasm examples/demo.tg
```

The WASI runtime must be installed separately and grant access to the source file. File input and output reach only directories the host exposes; `wasmtime run --dir .` grants the current directory for both reading and writing. The WASI entry point accepts a `.tg` file directly; it does not expose the native CLI's `run` and `build` subcommands. Unlike the browser build, it keeps the `regex` and `time` modules.

## Embedding Tiger in Rust

The simplest API is `tiger_evaluator::run_with_input`, which parses and executes
source and writes `print` output to the writer you supply:

```rust
let mut output = Vec::new();
tiger_evaluator::run_with_input(
    "print(6 * 7);",
    "embedded.tg",
    None,                            // a module loader, for `import`
    None,                            // the host filesystem, or a MemoryFileSystem
    None,                            // standard input
    Box::new(&mut output),
)
.expect("the program runs");
assert_eq!(String::from_utf8(output).unwrap(), "42\n");
```

Pass `Some(Rc::new(FileLoader))` to let the program `import` from disk, and
`Some(Box::new(MemoryFileSystem::default()))` to keep file I/O in memory, as the
playground does. See [Files](file-io.md) and [Importing Tiger Files](modules.md).

For a configurable step budget, parse once and execute explicitly:

```rust
let program = tiger_parser::parse("print(6 * 7);").expect("parses");
let mut engine = tiger_evaluator::Evaluator::new(Box::new(std::io::stdout()));
engine.max_steps = 100_000;
engine.execute(&program).expect("the program runs");
```

`execute` resets the step counter and creates a fresh environment each time. An
evaluator holds mutable execution state, so do not share one between threads, and
there is no API for persisting Tiger bindings across separate executions.

A program runs on the compiled engine when it lowers, and falls back to the
tree-walking interpreter for the few constructs the compiler does not cover yet:
destructuring, spread calls, and a `break` or `continue` that would leave a `try`.
The fallback is per program, so one such construct anywhere puts the whole program
on the tree-walker. A compound assignment or an update through an index or a
property — `x[i] += 1`, `x[i]++`, `this.count -= 1` — is compiled, and resolves its
target once so the two engines agree on evaluation order and on the position of
every diagnostic. `try`/`catch`/`finally`, `switch`/`match`, slices, and constant
folding are all compiled, including a `return` from any block of a `try`, which
unwinds through the `finally` on its way out of the enclosing function. See the
[performance guide](performance.md) for what else the compiler recognizes.
`tiger_evaluator::run_with_options` pins both the engine and the step limit, which
is how the differential test runs every fixture on both engines and checks that
they agree. `Engine::Tree` forces the tree-walker, and `max_steps` counts
statements there but instructions on the compiled engine, so `0` — no limit — is
the useful setting when comparing them:

```rust
use tiger_evaluator::{Engine, RunOptions};
let options = RunOptions { engine: Engine::Tree, max_steps: 0 };
tiger_evaluator::run_with_options(
    "print(6 * 7);",
    "embedded.tg",
    None,
    None,
    None,
    Box::new(std::io::sink()),
    options,
)
.expect("the program runs");
```

## Tests and Make Targets

```sh
cargo test
cargo test -p tiger-conformance --test benchmarks -- --nocapture
cargo test -p tiger-conformance --test documentation -- --nocapture
cargo fmt --check
cargo clippy --all-targets
```

For wall-clock timings, `cargo test -p tiger-conformance --test timing --release
-- --ignored --nocapture` reports a minimum and mean for every benchmark program
in-process. `TIGER_BENCH_FILE` times a single program, `TIGER_TIMING_ITERATIONS`
sets the batch size, `TIGER_ENGINE=tree` runs the same program on the tree-walking
interpreter instead of the compiled engine, which is how the compiled engine's
share of a workload is measured without a profiler, and `TIGER_SHOW_OUTPUT=1`
keeps the program's own output for a program that times itself.

`make compare` times a paired Tiger/Python workload on identical work and reports
the ratio, subtracting each language's process-startup floor so the numbers
compare the interpreters. The two programs print the same checksum, and the runner
refuses to report a time if they ever disagree. See the
[performance guide](performance.md) and
[benchmarks/python/README.md](../benchmarks/python/README.md).

`cargo test` checks the unit tests, the language suites in
`crates/tiger-evaluator/tests/`, the forty-three 100-200-line benchmark programs
against fixed output files, the 176 documentation examples, every example and
portfolio-feature program, the standalone-build integration tests, the
differential test that runs every one of those programs on both engines and
checks that the results match, and the
browser runtime under Node. The documentation suite checks Tiger examples against
the output shown in Markdown, and the benchmark suite checks the programs against
their `.txt` files; neither is a performance threshold test. The standalone
integration tests build and execute real executables.

`make build`, `make run`, `make test`, `make benchmarks`, `make timing`,
`make compare`, `make wasm`, `make wasm-check`, `make startup`, `make web`, and
`make wasi` wrap the commands
above. `make startup` boots the served playground over HTTP with Node and runs a
program on it, which is the same path a browser takes.

## Continuous Integration

`.github/workflows/ci.yml` runs two jobs:

- **Tests** on Linux and Windows: `cargo fmt --check`, a release build,
  `cargo test --release`, and the verbose benchmark and documentation suites.
- **Wasm builds, closure guard, and size budget**: builds the browser runtime and
  both WASI artifacts, fails if `cargo tree -p tiger-evaluator --target
  wasm32-unknown-unknown` gains `regex` (or if the WASI closure loses it), and
  fails if `tiger.wasm` grows past the size budget.
