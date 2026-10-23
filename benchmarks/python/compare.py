"""Times the Tiger interpreter against CPython on one identical workload.

`workload.tg` and `workload.py` do the same work — a counting loop, list
indexing and stores, dictionary insert and lookup, string building, method
calls, and recursion — and print the same integer checksum. The runner refuses
to report a time unless the two checksums match, so the comparison can never
quietly compare two different programs.

Both languages pay a process-startup cost that has nothing to do with the
interpreter, so the runner measures each one's floor on an empty program
(`floor.tg`, and `python -c ""`) and reports the workload both ways: total, and
with the floor subtracted. The subtracted column is the one that compares the
interpreters.

Usage:

    python benchmarks/python/compare.py

`COMPARE_RUNS` sets how many times each program runs (default 15); the minimum
is reported, which is the least noisy estimate for a short workload.
"""

import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
EXE = "tiger.exe" if os.name == "nt" else "tiger"
TIGER = os.path.join(ROOT, "target", "release", EXE)
PYTHON = sys.executable
RUNS = int(os.environ.get("COMPARE_RUNS", "15"))

# The workload's sections, in the order the two programs declare them. Each is
# reported on its own so a slow path is visible rather than averaged away.
SECTIONS = ["loop", "lists", "dicts", "strings", "methods", "recursion", "calls"]


def run_once(command):
    """Runs one command to completion and returns (milliseconds, stdout)."""
    start = time.perf_counter()
    completed = subprocess.run(command, capture_output=True, text=True)
    elapsed = (time.perf_counter() - start) * 1000.0
    if completed.returncode != 0:
        raise SystemExit("{} failed:\n{}".format(" ".join(command), completed.stderr))
    return elapsed, completed.stdout


def best(command, runs=RUNS):
    """The fastest of `runs` completions of `command`, and its output."""
    fastest = float("inf")
    output = ""
    for _ in range(runs):
        elapsed, output = run_once(command)
        fastest = min(fastest, elapsed)
    return fastest, output


def main():
    if not os.path.exists(TIGER):
        raise SystemExit("build the release CLI first: cargo build --release")

    workload_tg = os.path.join(HERE, "workload.tg")
    workload_py = os.path.join(HERE, "workload.py")
    floor_tg = os.path.join(HERE, "floor.tg")

    def tiger(section=None):
        command = [TIGER, "run", workload_tg] + ([section] if section else [])
        return command

    def python(section=None):
        return [PYTHON, workload_py] + ([section] if section else [])

    # The startup each language pays for a process, which has nothing to do with
    # the interpreter and is what makes an end-to-end comparison misleading. It is
    # measured over four times as many runs, because it is subtracted from small
    # sections where its jitter would otherwise dominate the result.
    tiger_floor, _ = best([TIGER, "run", floor_tg], runs=RUNS * 4)
    python_floor, _ = best([PYTHON, "-c", ""], runs=RUNS * 4)

    print("minimum of {} runs per measurement".format(RUNS))
    print("startup floor: tiger {:.2f}ms, python {:.2f}ms".format(tiger_floor, python_floor))
    print()
    print("{:<12}{:>10}{:>10}{:>10}{:>10}{:>9}".format("section", "tiger", "python", "t.work", "p.work", "ratio"))
    print("-" * 61)

    sections = ["all"] + SECTIONS
    for section in sections:
        tiger_output = run_once(tiger(section))[1].strip()
        python_output = run_once(python(section))[1].strip()
        if tiger_output != python_output:
            raise SystemExit(
                "{}: the two programs disagree: tiger {!r}, python {!r}".format(section, tiger_output, python_output)
            )
        tiger_ms, _ = best(tiger(section))
        python_ms, _ = best(python(section))
        tiger_work = max(tiger_ms - tiger_floor, 0.001)
        python_work = max(python_ms - python_floor, 0.001)
        print(
            "{:<12}{:>9.2f}ms{:>9.2f}ms{:>9.2f}ms{:>9.2f}ms{:>8.2f}x".format(
                section, tiger_ms, python_ms, tiger_work, python_work, python_work / tiger_work
            )
        )

    print()
    print("t.work and p.work subtract each language's startup floor, so they compare")
    print("the interpreters. ratio is python over tiger: above 1.00 means tiger is faster.")
    print("A section whose work is smaller than the floor's jitter is not meaningful on")
    print("its own; `all`, `recursion`, and `calls` are the rows to read.")
    print("Every section's checksum matched between the two programs.")


if __name__ == "__main__":
    main()
