"""The Python half of the in-process Tiger/Python protocol.

`compare.py` times two whole processes, so both numbers carry their launcher:
the workload is one to three milliseconds of work against a five to eighteen
millisecond startup floor, and subtracting the floor amplifies its jitter past
the signal it is meant to remove. This driver measures the interpreter instead.

It compiles `workload.py` once, then runs one section per `exec` with `sys.argv`
pinned, and reports the minimum of `ITERATIONS` runs in milliseconds. Two numbers
come out of it: `compile_ms` is `compile()` plus `exec`, and `exec_ms` is `exec`
alone — the number that compares against Tiger with the program's own compile
removed. Nothing about process startup is in either one.

`crates/tiger-conformance/tests/protocol.rs` runs this driver and reads the rows
back:

    section checksum compile_ms exec_ms

`TIGER_PROTOCOL_ITERATIONS` sets how many times each section runs; the Rust half
passes the same count, so the two sides measure the same thing.
"""

import io
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
WORKLOAD = os.path.join(HERE, "workload.py")

# The workload's sections, in the order `workload.py` declares them, then the
# probe: `overhead` matches no section, so it measures what a run costs before it
# does any work, which is the floor every light section is read against.
SECTIONS = ["all", "loop", "lists", "dicts", "strings", "methods", "recursion", "calls", "overhead"]

ITERATIONS = int(os.environ.get("TIGER_PROTOCOL_ITERATIONS", "300"))


def time_section(source, section, iterations):
    """Runs one section `iterations` times and returns (checksum, compile_ms, exec_ms)."""
    # `sys.argv[1]` selects the section, so it is pinned before the loops, and the
    # program's own output is captured rather than printed.
    sys.argv = [WORKLOAD, section]
    capture = io.StringIO()
    saved = sys.stdout
    sys.stdout = capture
    try:
        best_compile = float("inf")
        for _ in range(iterations):
            start = time.perf_counter()
            code = compile(source, WORKLOAD, "exec")
            exec(code, {"__name__": "__main__"})
            best_compile = min(best_compile, (time.perf_counter() - start) * 1000.0)
        # The precompiled number: the same bytecode, executed again and again, so
        # what is left is the interpreter running the program.
        code = compile(source, WORKLOAD, "exec")
        best_exec = float("inf")
        for _ in range(iterations):
            start = time.perf_counter()
            exec(code, {"__name__": "__main__"})
            best_exec = min(best_exec, (time.perf_counter() - start) * 1000.0)
    finally:
        sys.stdout = saved
    # Every iteration of both loops has to have printed the same checksum: a
    # section that disagreed with itself would not be a measurement of anything,
    # and the two languages' checksums are compared besides.
    printed = capture.getvalue().split()
    if len(printed) != 2 * iterations:
        raise SystemExit("{}: {} checksums printed, want {}".format(section, len(printed), 2 * iterations))
    if len(set(printed)) != 1:
        raise SystemExit("{}: the iteration checksums disagree: {}".format(section, sorted(set(printed))))
    return printed[0], best_compile, best_exec


def main():
    with open(WORKLOAD, encoding="utf-8") as handle:
        source = handle.read()
    for section in SECTIONS:
        checksum, compile_ms, exec_ms = time_section(source, section, ITERATIONS)
        print("{} {} {:.6f} {:.6f}".format(section, checksum, compile_ms, exec_ms))


if __name__ == "__main__":
    main()
