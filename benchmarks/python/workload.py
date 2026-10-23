"""The Python half of the interpreter workload in `workload.tg`.

The two programs do the same work and print the same checksum, so `compare.py`
can time them against each other without comparing number formatting. Keep the
two files in step: change one and change the other, then re-run the comparison.

`sys.argv[1]` selects one section by name, which is how `compare.py` reports
where the time goes; the default runs them all.
"""

import sys

SIZE = 600
ROUNDS = 40
CALLS = 50000

requested = sys.argv[1] if len(sys.argv) > 1 else "all"


def wants(name):
    return requested == "all" or requested == name


def fib(n):
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)


checksum = 0

# A counting loop.
if wants("loop"):
    for i in range(SIZE):
        checksum = checksum + i * 3 % 7

# List building, indexing, in-place stores, and a summing pass.
if wants("lists"):
    items = [i * i % 97 for i in range(SIZE)]
    for i in range(SIZE):
        items[i] = items[i] + 1
    total = 0
    for i in range(SIZE):
        total += items[i]
    checksum += total

# Dictionary insert, membership, and a read-modify-write.
if wants("dicts"):
    counts = {}
    for i in range(SIZE):
        key = "k" + str(i % 50)
        if key in counts:
            counts[key] += 1
        else:
            counts[key] = 1
    for i in range(50):
        checksum += counts["k" + str(i)]

# String building.
if wants("strings"):
    text = ""
    for _ in range(200):
        text += "ab"
    checksum += len(text)

# Method calls on strings.
if wants("methods"):
    words = ["alpha", "beta", "gamma", "delta"]
    for _ in range(ROUNDS):
        for i in range(len(words)):
            checksum += len(words[i].upper())

# Recursion.
if wants("recursion"):
    checksum += fib(20)


# Plain function calls: an empty body, called in a loop, which isolates the
# cost of a call from the work it does. It is a probe rather than part of the
# whole workload, so `all` skips it.
def nothing(value):
    return value


if requested == "calls":
    for _ in range(CALLS):
        checksum += nothing(1)

print(checksum)

