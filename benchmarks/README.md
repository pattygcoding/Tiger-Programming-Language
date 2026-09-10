# Tiger Benchmark Programs

Run all ten programs and compare their output with the corresponding checked-in `.txt` files:

```sh
go test ./benchmarks -count=1 -v
```

Run from the repository root. This works in PowerShell as well as POSIX shells and does not require a separately built Tiger executable. `make benchmarks` runs the same command. The fixtures also run as part of `go test ./...`.

Run one program's checks:

```sh
go test ./benchmarks -run TestPrograms/08_graphs -count=1 -v
```

| Program | Coverage |
| --- | --- |
| `01_arithmetic.tg` | Arithmetic, precedence, GCD, primes, numeric boundaries |
| `02_recursion.tg` | Recursive sequences, combinatorics, trees, strings |
| `03_sorting_search.tg` | Three sorting algorithms, binary search, duplicates |
| `04_matrices.tg` | Nested indexing, multiplication, transposition, matrix identities |
| `05_strings.tg` | Tokenization, frequency counts, Caesar cipher, run-length encoding |
| `06_inventory.tg` | Nested dictionary mutation, atomic orders, independent copies |
| `07_closures.tg` | Captured state, higher-order functions, per-iteration closures |
| `08_graphs.tg` | BFS, DFS, shortest paths, cycles, disconnected vertices |
| `09_dynamic_programming.tg` | Coin change, knapsack, grid paths, increasing subsequences |
| `10_language_semantics.tg` | Scope, truthiness, short-circuiting, returns, aliases, cyclic values |

Each program is 100-200 physical lines, including ordinary blank lines. The test suite enforces this range and checks that exactly ten source files and ten matching output files exist. Every program executes in a fresh evaluator with the normal execution limits.

Expected outputs are fixed reference results, not regenerated from interpreter output. Comparison preserves spaces and the final newline; only CRLF line endings are normalized to LF for cross-platform Git checkouts. Missing pairs, invalid lengths, syntax/runtime errors, and output mismatches fail the command with a nonzero exit status.

These are deterministic correctness workloads, not timing benchmarks. They do not enforce performance thresholds.