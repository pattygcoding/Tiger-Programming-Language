# Agent Instructions

## Every language or runtime change must include

- **Benchmarks:** add a paired `benchmarks/NN_name.tg` / `NN_name.txt` program (100-200 lines) covering the feature, or extend an existing one when it fits. Write expected output by hand, not by copying interpreter output. When adding a program, update the count in `benchmarks/benchmarks_test.go`, the table and counts in `benchmarks/README.md`, and the counts in `README.md`, `docs/README.md`, and `docs/running-and-building.md`.
- **Documentation:** document the feature in the relevant `docs/*.md` guide with a runnable `tg` example and its `text` output (checked by `docs/docs_test.go`), and mention it in `README.md` when it is user-facing.
- **Tests:** add unit tests, including error cases, next to the changed package.
- **Wasm compatibility:** all code must stay compilable to `wasm` (`GOOS=js GOARCH=wasm`, used by `cmd/wasm` and `cmd/web`) and `wasip1` (`GOOS=wasip1 GOARCH=wasm`, used by `cmd/wasi`). Avoid APIs unavailable under these targets (e.g. unsupported syscalls, OS-specific packages); gate any platform-specific code behind build tags.
- **Wasm size:** never make a package listed under "Wasm size guardrails" reachable from `cmd/wasm`. `go test ./...` runs `pkg/wasmcheck`, which fails when the `GOOS=js` dependency closure gains one of them.

## Wasm size guardrails

`web/tiger.wasm` bundles the whole interpreter, so every package linked into `cmd/wasm` ships to the browser. It currently builds to about 2.9 MB stripped (~0.8 MB gzip). Keep it there.

- **Never import `fmt` in code reachable from `cmd/wasm`.** `fmt` drags in `reflect`, `internal/fmtsort`, and `os`. The runtime packages (`pkg/lexer`, `pkg/parser`, `pkg/object`, `pkg/evaluator`, and anything they import) must use `tiger/pkg/format` for `Sprintf`/`Errorf`. `fmt` is allowed only in native-only code: `cmd/tiger`, `cmd/wasi`, `cmd/web`, and `pkg/compiler`.
- **Keep host-filesystem code behind `//go:build !js`.** `os`, `path/filepath`, `io/fs`, `time`, `internal/poll`, and non-js `syscall` must not be reachable from `cmd/wasm`. Follow the existing `file_os.go` / `file_js.go` and `modules_os.go` / `modules_js.go` pattern: real implementations under `!js`, small stubs under `js`.
- **Use the in-repo replacements instead of heavy standard-library packages:**
  - `bufio` (pulls `bytes`) -> `pkg/evaluator`'s `bufferedReader` (`reader.go`).
  - `sort` (pulls `slices` and `cmp`) -> `pkg/evaluator`'s `stableSort` (`sort.go`).
  - `container/heap` (imports `sort`) -> `pkg/evaluator`'s `mergeHeap` in `algomodule.go`.
  - `io/fs` error types -> `pkg/evaluator`'s `ErrNotExist` and `pathError` (`filesystem_error.go`).
- Already linked and safe to use: `errors`, `strconv`, `strings`, `unicode`, `unicode/utf8`, `math`, `math/bits`, `path`, and `syscall/js`. Prefer these over adding a new dependency.
- `pkg/format` supports only the verbs the runtime uses (`%s %d %q %c %v %t %w %%`). If you need another verb, add it there and extend `pkg/format/format_test.go`, which compares its output with `fmt`.
- Leave the build flags in `cmd/web/main.go` intact (`-trimpath -buildvcs=false -ldflags="-s -w -buildid="`), followed by `wasm-opt -Oz --converge` with the strip passes when Binaryen is on `PATH`.
- When the size changes, update the numbers in `README.md`, `docs/running-and-building.md`, and `web/README.md`, and adjust the size budget in `.github/workflows/ci.yml`.

## Validation

Run `go vet ./...` and `go test ./...` before finishing. `go test ./...` runs the unit tests, the documentation examples, the benchmark regression programs, the standalone-build integration tests, and the Wasm guard (`pkg/wasmcheck`). Use `go test ./benchmarks -count=1 -v` for the verbosely logged benchmark programs and `make wasm-check` for the guard alone.

When touching runtime/evaluator code, also verify `GOOS=js GOARCH=wasm go build ./...` and `GOOS=wasip1 GOARCH=wasm go build ./...` still succeed, and confirm the browser build still avoids the packages listed under "Wasm size guardrails".
