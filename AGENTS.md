# Agent Instructions

## Every language or runtime change must include

- **Benchmarks:** add a paired `benchmarks/NN_name.tg` / `NN_name.txt` program (100-200 lines) covering the feature, or extend an existing one when it fits. Write expected output by hand, not by copying interpreter output. When adding a program, update the count in `benchmarks/benchmarks_test.go`, the table and counts in `benchmarks/README.md`, and the counts in `README.md`, `docs/README.md`, and `docs/running-and-building.md`.
- **Documentation:** document the feature in the relevant `docs/*.md` guide with a runnable `tg` example and its `text` output (checked by `docs/docs_test.go`), and mention it in `README.md` when it is user-facing.
- **Tests:** add unit tests, including error cases, next to the changed package.
- **Wasm compatibility:** all code must stay compilable to `wasm` (`GOOS=js GOARCH=wasm`, used by `cmd/wasm` and `cmd/web`) and `wasip1` (`GOOS=wasip1 GOARCH=wasm`, used by `cmd/wasi`). Avoid APIs unavailable under these targets (e.g. unsupported syscalls, OS-specific packages); gate any platform-specific code behind build tags.

## Validation

Run `go vet ./...` and `go test ./...` before finishing. When touching runtime/evaluator code, also verify `GOOS=js GOARCH=wasm go build ./...` and `GOOS=wasip1 GOARCH=wasm go build ./...` still succeed.
