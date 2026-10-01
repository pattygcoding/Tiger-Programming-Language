# Tiger Benchmark Programs

Run all thirty-one programs and compare their output with the corresponding checked-in `.txt` files:

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
| `11_oop_shapes.tg` | Polymorphism, overrides, inherited methods, chained calls |
| `12_oop_accounts.tg` | Constructors, per-instance state, super calls, transactions |
| `13_oop_super.tg` | Deep inheritance, lexical super, inherited constructors, local classes |
| `14_oop_composition.tg` | Nested objects, recursive methods, shared references, fluent methods |
| `15_oop_callbacks.tg` | Detached bound methods, captured this/super, callable fields |
| `16_control_flow.tg` | Prefix/postfix updates, cfor, cif/celse, `&&`/`\|\|` headers, range, loop else, switch, break/continue |
| `17_access_exceptions.tg` | Public defaults, restricted fields/methods, this, try/catch/throw, recovery, f-string interpolation |
| `18_variadics.tg` | `*args`/`**kwargs` collection, keyword binding, `*`/`**` call unpacking, variadic methods and constructors |
| `19_file_io.tg` | File helpers, open modes, file-object methods, seek/tell, binary streams, catchable file errors |
| `20_list_sort.tg` | In-place `sort()`, `key`/`reverse`, stability, multi-pass ordering, bound/closure keys, sort errors |
| `21_input.tg` | `input()` prompts, line parsing, read-until-sentinel loops, whitespace handling, EOF detection (stdin from `21_input.in`) |
| `22_conversions.tg` | `int`/`float`/`str`/`bool`, `.ToString()`/`.ToInt()`/`.ToFloat()`/`.ToBool()`, user overrides, truthiness, mixed arithmetic, conversion errors |
| `23_math.tg` | built-in `math` module: `ceil`/`floor`/`round` (half to even), `sqrt`, `abs`, `pow`, `log`, `sin`/`cos`/`tan`, `min`/`max`, geometry and statistics, functions as values, domain/range errors, shadowing |
| `24_algo.tg` | built-in `algo` module: `fibonacci`/`fibonacciList`, the exact-result limit (index 78), cross-checks against recursive and iterative implementations, sequence identities, fresh mutable lists, `removeDuplicates` (order, `==` semantics, nested values, identity), `findMatches`, argument errors |
| `25_algo_inventory.tg` | `algo.takeInventory` frequency dictionaries (key order, `==` merging, restock and histogram reports, manual cross-check) and `algo.findPlace` rankings (duplicates, strings, floats, sort cross-check, out-of-range and type errors), and `algo.isPrime` (trial-division cross-check, large primes, non-integer errors) |
| `26_algo_rainwater.tg` | `algo.rainwater` trapping-rain-water totals cross-checked against brute-force and prefix-max Tiger implementations, ASCII water diagram, generated maps, valleys and square basins, fractional heights, argument errors |
| `27_algo_sorted.tg` | `algo.medianSorted` (odd/even totals, empty sides, duplicates, negatives, fractions) and `algo.mergeKSorted` (numbers, strings, empty lists, ties) cross-checked against sort-based Tiger versions, generated lists, a merged leaderboard, unsorted and type errors |
| `28_algo_strings.tg` | `algo.editDistance` and `algo.regexMatch` cross-checked against Tiger DP and recursive implementations, a spelling suggester, file-name filtering, generated strings and patterns, Unicode, invalid patterns |
| `29_algo_windows.tg` | the five `algo.slidingWindow*` functions cross-checked against brute-force Tiger versions: fixed-window maxima and sums, shortest run reaching a target, longest unique substring, minimum covering substring, generated inputs, argument errors |
| `30_algo_palindromes.tg` | the five `algo.palindrome*` functions: sentence checks ignoring case and punctuation, one-deletion checks, longest, counts, and minimum cuts cross-checked against brute-force Tiger versions, the 5,000-character limit, argument errors |
| `31_algo_factorials.tg` | `algo.factorial`/`factorialList` checked against recursion and term ratios, combinations and Pascal's row, word arrangements with `takeInventory`, trailing zeros, an approximation of e, the 18! limit, argument errors |

Run only the five OOP programs:

```sh
go test ./benchmarks -run 'TestPrograms/1[1-5]_' -count=1 -v
```

Each program is 100-200 physical lines, including ordinary blank lines. The test suite enforces this range and checks that exactly thirty-one source files and thirty-one matching output files exist. Every program executes in a fresh evaluator with the normal execution limits, and each run receives a fresh in-memory filesystem so file-I/O workloads stay hermetic. A program with a matching `.in` file reads it as standard input through `input()`; programs without one have no input stream.

Expected outputs are fixed reference results, not regenerated from interpreter output. Comparison preserves spaces and the final newline; only CRLF line endings are normalized to LF for cross-platform Git checkouts. Missing pairs, invalid lengths, syntax/runtime errors, and output mismatches fail the command with a nonzero exit status.

These are deterministic correctness workloads, not timing benchmarks. They do not enforce performance thresholds.