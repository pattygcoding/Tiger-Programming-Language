# Room for Improvement

[Guide index](README.md) | [Project README](../README.md)

This page records the gaps that are most likely to matter to people writing
larger Tiger programs. It is a planning document, not a promise or a roadmap of
committed work: everything here is **absent today**, and the code blocks show
the workarounds that the implemented language forces on you. The
[guide](README.md) teaches the language as it is; this page describes what it is
not yet.

Each entry lists the missing capability, why it matters, a runnable workaround,
a suggested direction, and any constraints that the addition would face.

## 1. Sets and tuples

**Missing:** a set type and a tuple type.

**Why it matters:** these are the last two collection shapes. Without a set,
membership and de-duplication go through `algo.removeDuplicates`, dictionary
keys, or hand-written loops; without a tuple, the fixed-length groups that
`enumerate`, `zip`, and `return a, b;` produce stay ordinary lists, so nothing
distinguishes a pair from a two-element list. See [Collections](collections.md)
and [Built-ins](builtins.md).

**Suggested direction:** both add a runtime value type, so they touch equality,
formatting, hashing, iteration, and `len`. A set can reuse the dictionary's key
hashing for `add`, `remove`, `has`, and the union/intersection/difference
operations; the parser change is a `{1, 2}` literal that is told apart from a
dictionary by the first pair, keeping `{}` a dictionary as Python does. A tuple
can reuse the list implementation with structural equality, no mutation, no
`append`/`sort`, and `(1, 2)` syntax with a trailing comma for the one-element
form.

## Constraints from the Wasm build

Any addition must keep `cmd/wasm` free of the packages listed under "Wasm size
guardrails" in [`AGENTS.md`](../AGENTS.md). In practice that means:

- The `random` and `json` modules are Wasm-safe: they use integer math and an
  in-repo parser instead of `math/rand` or `encoding/json`.
- `time` and `regex` are native/WASI-only behind `!js` build tags, with small
  browser stubs, because `time` and `regexp` would inflate `web/tiger.wasm`.
- Sort-based helpers must keep using the in-repo `stableSort` rather than the
  standard `sort` package.

See [Running and Building](running-and-building.md) for the size budget and the
browser and WASI entry points.

## Definition of done for a new feature

Every language or runtime change should ship with, per
[`AGENTS.md`](../AGENTS.md):

- a paired `benchmarks/NN_name.tg` / `NN_name.txt` program that exercises the
  feature, with counts updated in `benchmarks/`, `README.md`,
  [`docs/README.md`](README.md), and
  [`running-and-building.md`](running-and-building.md);
- documentation in the relevant guide, with a runnable example and its expected
  output, so that `docs/docs_test.go` checks it;
- unit tests next to the changed package, including error cases;
- successful `GOOS=js GOARCH=wasm` and `GOOS=wasip1 GOARCH=wasm` builds, and a
  passing `go test ./...` including `pkg/wasmcheck`.

## Deliberate non-goals

Some of the "missing" pieces are choices, not oversights, and should stay out
unless the design changes: arbitrary-precision integers (numbers are float64),
a package manager (imports are file-based), asynchronous syntax and generators
(the runtime is a synchronous tree-walker), and network access in the sandboxed
browser and WASI targets.

[Guide index](README.md) | [Project README](../README.md)

