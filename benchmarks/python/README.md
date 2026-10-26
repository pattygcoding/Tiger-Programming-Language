# Paired Tiger / Python Workload

`workload.tg` and `workload.py` do the same work and print the same integer
checksum, so the two languages can be compared on time without comparing number
formatting. Both runners refuse to report a time unless the checksums match.

```sh
# The measurement: both interpreters in process, no process startup in either
# number, one pinned section at a time, 300 iterations, the minimum reported.
# `TIGER_PROTOCOL_ASSERT` is what makes it the threshold and fails while any section
# is slower than CPython; without it the same run reports its table and passes, so a
# bare `cargo test` never fails on a host whose CPython is faster than the one the
# ratio was read on.
TIGER_PROTOCOL_ASSERT=1 cargo test -p tiger-conformance --test protocol --release -- --nocapture

# The end-to-end check: one process per run, with each language's startup floor
# subtracted. Its numbers carry the launcher; see "Reading the result".
cargo build --release
python benchmarks/python/compare.py
```

`make protocol` and `make compare` do those two. `TIGER_PROTOCOL_ITERATIONS` sets
how many times each section runs in the protocol (default 300; 5 in a debug
build), `TIGER_PROTOCOL_RATIO` sets the ratio the protocol requires (default
1.00, read as `tiger / cpython` — raising it makes the threshold pass without
anything getting faster, so read it as the bar and not as a knob), and
`TIGER_PYTHON` names the interpreter to run the Python half with. `COMPARE_RUNS`
does the same job for `compare.py`.

## What it measures

| Section | Work |
| --- | --- |
| `loop` | A counting loop: the fused comparison and branch |
| `lists` | List building, indexing, in-place stores, and a summing pass |
| `dicts` | Dictionary insert, membership, and a read-modify-write |
| `strings` | String building in a loop |
| `methods` | String method calls |
| `recursion` | `fib(20)`: recursive calls |
| `calls` | An empty function called 50,000 times, which isolates a call |
| `overhead` | A probe: a name no section answers, so it measures a run's own setup |

`sys.argv[1]` selects one section, and `all` (the default) runs everything except
`calls`, which is a probe rather than part of the workload. `overhead` is the
other probe: both programs skip every section for it, so what is left is what a
run costs before it does any work — the floor the light sections are read
against. A probe's ratio is reported and not asserted, because there is no work
for the other interpreter to be slower at.

## Reading the result

`python benchmarks/python/compare.py` times two whole processes, and the workload
is one to three milliseconds of work against a five to eighteen millisecond
startup floor, so subtracting the floor mostly amplifies its jitter. `protocol.py`
is the runner to read instead: it compiles `workload.py` once and executes it
`TIGER_PROTOCOL_ITERATIONS` times, while the Rust half of the protocol does the
same for Tiger through `Evaluator::compile` and `Evaluator::execute_compiled`.
Both sides report the minimum, and both have the program's own compile removed,
which is what makes the comparison one between interpreters.

A representative reading: the best of twelve runs of each side, which is the method
the [performance guide](../../docs/performance.md#getting-every-protocol-section-under-cpython)
states its own paired table with, because one run's minimum moves with whatever else
the host is doing. The ratios are against the host's CPython, so this one was taken
with 3.14 on `PATH`: the same revision reads differently against another release,
which is why the assertion is asked for rather than assumed.

| Section | Tiger work | CPython `p.exec` | Ratio |
| --- | --- | --- | --- |
| `all` | 0.748 ms | 0.808 ms | 0.93x |
| `loop` | 0.022 ms | 0.031 ms | 0.71 |
| `lists` | 0.060 ms | 0.075 ms | 0.80 |
| `dicts` | 0.110 ms | 0.124 ms | 0.89 |
| `strings` | 0.005 ms | 0.013 ms | 0.38 |
| `methods` | 0.018 ms | 0.021 ms | 0.86 |
| `recursion` | 0.505 ms | 0.540 ms | 0.94 |
| `calls` | 1.709 ms | 2.428 ms | 0.70 |

Tiger is at or ahead of CPython on all eight asserted sections. The four it wins by
the widest margin are the ones whose work is large enough to read above the host's
own jitter: `strings` at 0.38x, `calls` at 0.70, `loop` at 0.71 and `lists` at 0.80.
`recursion` (0.94) and `dicts` (0.89) are under the line but nearer to it —
`recursion` is `fib(20)`'s 21,891 calls, and what is left in it once the frame work
is done is about seven dispatched instructions per invocation against CPython's own
bytecode loop, which the performance guide measures under
[a return is one dispatch too](../../docs/performance.md#a-return-is-one-dispatch-too)
— and `methods` is the section with the least margin: 0.018 ms of Tiger work against
0.021 ms of CPython's, with no dominant cost left in its profile to remove.

The `overhead` probe is on the same two sides: 0.002 ms of a Tiger run, 0.002 ms of
a CPython `exec`. A ratio moves by up to a fifth between runs on a machine that is
doing anything else, so read the column as a magnitude rather than a hundredth: the
four rows with real margin are `strings`, `calls`, `loop` and `lists`, and the rest
sit close enough to 1.00 that a change to them has to be confirmed over several runs
before it counts. The light sections are the ones that jitter can carry over the
line: read on a busy host, `make protocol` passes on most runs and not on every
one. The reading above came from four passing runs out of twelve; the run that
produced the performance guide's table passed in 11 of 14 asserted runs, and each of
its three failures was a light section over the line on that one run.

## Extending it

Keep the two files in step. A change to one is a change to the other, and the
checksums must stay equal or both runners stop. Add a section to both, list it in
`protocol.py`'s `SECTIONS` and in the protocol test's, and re-run.
