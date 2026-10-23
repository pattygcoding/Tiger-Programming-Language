# Paired Tiger / Python Workload

`workload.tg` and `workload.py` do the same work and print the same integer
checksum, so the two languages can be compared on time without comparing number
formatting. `compare.py` refuses to report a time unless the checksums match.

```sh
cargo build --release
python benchmarks/python/compare.py
```

`make compare` does both. `COMPARE_RUNS` sets how many times each program runs
(default 15); the minimum is reported, which is the least noisy estimate for a
short workload.

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

`sys.argv[1]` selects one section, and `all` (the default) runs everything except
`calls`, which is a probe rather than part of the workload.

## Reading the result

Both languages pay a process-startup cost that has nothing to do with the
interpreter, and CPython's is more than three times Tiger's, so an end-to-end
comparison mostly measures the launcher. The runner therefore measures each
language's floor on an empty program (`floor.tg`, and `python -c ""`) and reports
the workload twice: as a total, and with the floor subtracted. The subtracted
column is the one that compares the interpreters.

A section whose work is smaller than the floor's jitter is not meaningful on its
own, so read `all`, `recursion`, and `calls`. Everything else is a hint about
where the time goes.

## Extending it

Keep the two files in step. A change to one is a change to the other, and the
checksums must stay equal or the runner stops. Add a section to both, list it in
`compare.py`'s `SECTIONS`, and re-run.
