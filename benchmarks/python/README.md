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

A representative run, with the compile removed from both sides. The ratios are
against the host's CPython, so this one was taken with 3.14 on `PATH`: the same
revision reads differently against another release, which is why the assertion is
asked for rather than assumed.

| Section | Tiger | CPython | Ratio |
| --- | --- | --- | --- |
| `all` | 1.22 ms | 0.80 ms | 1.52x |
| `loop` | 0.041 | 0.032 | 1.27 |
| `lists` | 0.102 | 0.075 | 1.37 |
| `dicts` | 0.165 | 0.124 | 1.33 |
| `strings` | 0.007 | 0.010 | 0.67 |
| `methods` | 0.030 | 0.020 | 1.53 |
| `recursion` | 0.888 | 0.534 | 1.66 |
| `calls` | 2.425 | 2.464 | 0.98 |

The `overhead` probe is on the same two sides: 0.005 ms of a Tiger run, 0.001 ms
of a CPython `exec`. A ratio moves by up to a fifth between runs on a machine that
is doing anything else, so read the column as a magnitude rather than a
hundredth.

So Tiger is slower than CPython on six of the eight asserted sections. `strings` is
the one it wins — the probe says why: 3 µs of work against CPython's 9, and most of
the 0.68 it reports is the 5 µs floor a run costs before it does anything — and
`calls` is where it now sits at parity, at 1.01. The heavy sections are the
interpreter itself: one pass of `calls` — a call, the add it feeds, and the loop
around them — is 49 ns against CPython's 49, and that is most of what `methods` is
made of. `recursion` is still the widest ratio, and what is left in it is the half
of a call that is not the frame: a flat frame's slots are the bottom of its own
region on the value stack, so a recursive call pays no vector, no fill, and no
argument copy any more (see
[performance.md](../../docs/performance.md#a-frames-slots-are-the-bottom-of-its-own-stack)).
What that leaves is about seven dispatched instructions per invocation against
CPython's own bytecode loop — a recursive call is where a tree of small frames
meets a register machine. The docs' per-operation costs in
[performance.md](../../docs/performance.md#where-the-time-goes) are the place to
read those from.

## Extending it

Keep the two files in step. A change to one is a change to the other, and the
checksums must stay equal or both runners stop. Add a section to both, list it in
`protocol.py`'s `SECTIONS` and in the protocol test's, and re-run.
