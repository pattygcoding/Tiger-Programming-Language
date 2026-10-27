# Performance Dashboard

A local app that measures the interpreter and draws what it measured. It is the
watchable form of the numbers the test suites already report.

```sh
make dashboard
```

That builds the tool, serves it on `http://127.0.0.1:7171` (a free port when that
one is busy), and opens the browser. Press **Play** on the page to measure; the bars
fill in as each section and each program finishes rather than after the run is over.

From the tool's own directory:

```sh
cd tools/perf-dashboard
cargo run --release
```

It is **not** a member of the repository's workspace, so `cargo test` at the root
never builds it and the interpreter's dependency closure is untouched — the same
arrangement `tools/flamegraph` uses, and for the same reason.

## What it measures

| Panel | What it is | Where the code lives |
| --- | --- | --- |
| Tiger vs CPython | One fixed workload, one pinned section at a time, in process on both sides, each program's own compile removed. The bar is Tiger's work over CPython's `exec`; the rule marks 1.00x. | `tiger_conformance::protocol`, the same module the threshold test runs |
| The corpus | The forty-four benchmark programs, timed in process with one warm-up run each, reporting the best of the batch. | `src/report.rs`, the way `tests/timing.rs` does it |
| Compiled engine vs the tree-walker | The same corpus a second time on `Engine::Tree`, which is how the compiled engine's share of a workload is read without a profiler. Opt-in, because it doubles the wait. | the same |

The protocol half is shared with `crates/tiger-conformance/tests/protocol.rs` rather
than reimplemented, so a row the dashboard draws is a row `make protocol` would
assert on. The corpus half is the measurement `make timing` reports.

Nothing here is a threshold. `make protocol` is the gate; this is what says where to
point it.

## Controls

- **Play** measures with the controls as they stand, and is disabled while a run is
  in flight. The server refuses a second concurrent run with `409`, because two
  runs would measure each other.
- **Iterations** sets how many times each program or section runs. 25 is a quick
  look, 300 is what the gate uses.
- **Benchmark corpus** and **Compare engines** choose which passes run.
- **Copy JSON** puts the whole report on the clipboard, which is the way to get a
  run into a script or an issue.

## Command line

```sh
cargo run --release -- [--port <address>] [--iterations <n>] [--no-corpus] [--engines] [--no-open] [--run-once]
```

`--run-once` measures, prints the same console table the protocol test prints, and
exits without serving anything, which is the headless path.

## Endpoints

The server is plain `std`, the way `crates/tiger-web` serves the playground.

| Route | What it does |
| --- | --- |
| `GET /` | The page |
| `GET /api/state` | Whether a run is in flight, the plan, and the last completed report |
| `GET /api/events` | A server-sent event stream: `start`, `phase`, `tiger`, `python`, `program`, `note`, `report` |
| `POST /api/play` | Starts a run; the body is `{"iterations":100,"corpus":true,"engines":false}`, and any field left out keeps its previous value |

A run holds the interpreter, whose values are `Rc`-based and not `Send`, so every
measurement happens on the run's own thread and only strings cross back. That thread
gets a 64 MiB stack, which is the arrangement every harness in the repository uses.

## Tests

```sh
cd tools/perf-dashboard
cargo test
```

The unit tests cover the JSON the report is written as, the small request parser,
and the plan a `POST /api/play` body asks for — including the fields it must ignore.
