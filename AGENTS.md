# Agent Instructions

Tiger is written in Rust. `crates/` holds the implementation; `web/`,
`benchmarks/`, `docs/`, `examples/`, and `portfolio-features/` hold the assets
it serves and is checked against.

## Every language or runtime change must include

- **Benchmarks:** add a paired `benchmarks/NN_name.tg` / `NN_name.txt` program (100-200 lines) covering the feature, or extend an existing one when it fits. Write expected output by hand, not by copying interpreter output. When adding a program, update the count in `crates/tiger-conformance/tests/benchmarks.rs`, the table and counts in `benchmarks/README.md`, and the counts in `README.md`, `docs/README.md`, and `docs/running-and-building.md`.
- **Documentation:** document the feature in the relevant `docs/*.md` guide with a runnable `tg` example and its `text` output (checked by `crates/tiger-conformance/tests/documentation.rs`), and mention it in `README.md` when it is user-facing.
- **Tests:** add unit tests, including error cases, next to the changed crate. The ported suites live in `crates/tiger-evaluator/tests/` and follow the helpers in `tests/common/mod.rs`.
- **Wasm compatibility:** all runtime code must stay compilable to `wasm32-unknown-unknown` (the browser runtime, `crates/tiger-wasm`) and `wasm32-wasip1` (the WASI entry point, `crates/tiger-wasi`). Avoid APIs that are missing or that panic under these targets; gate platform-specific code behind `#[cfg]`, as the `time` and `regex` modules do.
- **Wasm size:** never make a crate reachable from `crates/tiger-wasm` that the browser build must not carry. The guardrail is a `#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]` split with a stub, as in `crates/tiger-evaluator/src/regexmod.rs` and the `time` module.

## Wasm size guardrails

`web/tiger.wasm` bundles the whole interpreter, so every crate linked into
`crates/tiger-wasm` ships to the browser. It currently builds to about 660 KB
stripped (~250 KB gzip). Keep it there.

- **Keep the browser runtime free of `regex`, the host clock, and host entropy.** They live behind the `wasm32-unknown-unknown` cfg with stubs that report the documented error, because the `regex` crate would bloat the download and `SystemTime::now` panics on that target. `std::fs` is linked but never reached, because the browser always runs on the in-memory filesystem.
- **Never add a dependency without checking the browser closure.** `cargo tree -p tiger-evaluator --target wasm32-unknown-unknown` must not gain a crate, and the same command for `--target wasm32-wasip1` must still show the ones WASI keeps.
- **Keep the release profile stripping.** `[profile.release]` sets `opt-level = 3` and `strip = "symbols"`; `crates/tiger-web` copies the release artifact and then runs `wasm-opt -Oz --converge` when Binaryen is on `PATH`.
- **Update the numbers when the size changes:** `README.md`, `docs/running-and-building.md`, `web/README.md`, and the budget in `.github/workflows/ci.yml`.

## Validation

Run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` before
finishing. `cargo test` runs the unit tests, the language suites, the 40
benchmark programs, the 157 documentation examples, the example and portfolio
programs, the standalone-build integration tests, the differential test that runs
every one of those programs on both engines and checks that they agree, and the
browser runtime under Node. The browser tests skip with a message until `cargo
build -p tiger-wasm
--release --target wasm32-unknown-unknown` has run.

When touching runtime or evaluator code, also verify:

```sh
cargo build -p tiger-evaluator --target wasm32-unknown-unknown
cargo build -p tiger-evaluator --target wasm32-wasip1
cargo build -p tiger-wasi --target wasm32-wasip1
cargo build -p tiger-wasm --release --target wasm32-unknown-unknown
```

and confirm the browser closure still avoids the crates listed above.
