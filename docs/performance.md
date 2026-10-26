# Performance

[Guide index](README.md) | [Project README](../README.md)

Tiger runs a program on one of two engines. This page describes what each one
does, which one a program gets, the shapes the compiled engine recognizes, how
to measure a change, and where the remaining time goes.

## Two engines

- The **compiled engine** lowers a program to a flat instruction list and runs it
  in one loop. It is the default.
- The **tree-walking interpreter** evaluates the syntax tree directly. It is the
  reference the compiled engine has to agree with, and the fallback for a program
  the compiler does not lower.

Both engines delegate to the same runtime operations, so they produce the same
output and the same diagnostics, positions included. The differential test runs
every fixture in the repository on both engines and fails if they differ.

## Which engine a program gets

A program runs on the compiled engine when the whole of it lowers. It falls back
to the tree-walker when it uses a construct the compiler does not cover yet:

- destructuring, such as `const [a, b] = pair();`
- a spread call, such as `f(*args)` or `f(**options)`
- a `break` or a `continue` that would leave a `try`

The fallback is per program, not per statement, so one spread call anywhere puts
the whole program on the tree-walker. `Engine::Tree` forces the tree-walker, which
is how the two are compared.

## What the compiled engine recognizes

The compiler works on the flat locals it can prove no nested function reaches, so
most of what it does is replacing a name lookup with a slot index. On top of that
it folds and fuses.

**Constant folding.** An operation whose operands are both literals is computed
once, when the program is compiled. A result the runtime would reject is left
alone, so `1 / 0` still raises where it is written.

```tg
print(2 + 3 * 4, "a" + "b" + "c");
```

```text
14 abc
```

**Fused comparison and branch.** A comparison whose only consumer is a branch — a
loop condition, or an `if` header — never becomes a `bool` value. Two flat locals,
or a flat local and a constant, are compared and branched in a single
instruction.

```tg
var total = 0;
for (var index = 0; index < 5; ++index) {
    total += index * 2;
}
print(total);
```

```text
20
```

**A loop's step and an accumulating assignment are single instructions.** The
update a `for` header ends with discards the value it produces, so `++index`
followed by the loop's back edge becomes one instruction that increments the slot
and jumps. An assignment whose value is a computation over flat locals is fused the
same way, so `total += index` and `total = total + index` are each one instruction
that writes its result straight into the slot and never touches the operand stack.

```tg
var total = 0;
for (var index = 0; index < 4; ++index) {
    total = total + index;
}
print(total);
```

```text
6
```

**The tail of an assignment is one instruction.** `total += items[index]` and
`total += f(x)` compute a value that a store then takes off the operand stack; the
operator and the store are fused into one instruction that writes the result
straight into its slot. Only the both-flat forms are longer, so those still win
when they match.

```tg
const items = [3, 1, 2];
var total = 0;
for (var index = 0; index < 3; ++index) {
    total += items[index] + 1;
}
print(total);
```

```text
9
```

**An index read or write answers the shape a loop has.** `a[i]` is read directly
when the collection is a list and the key is a non-negative integer in range, which
is what nearly every index is: a borrow and an index, rather than a call, a key
check, and a second borrow. Everything else — a negative index counting from the end,
a key that is not an integer, an index out of range, a string, a tuple, a dictionary —
falls through to the path that owns the diagnostics, so the messages and their
positions are unchanged. A store takes the same shape.

```tg
const items = [3, 1, 2];
var total = 0;
for (var index = 0; index < 3; ++index) {
    items[index] += 10;
    total += items[index];
}
print(items, total, items[-1]);
```

```text
[13, 11, 12] 36 12
```

**A read-modify-write resolves its target once.** A compound assignment or an
update through an index or a property evaluates the target's own expressions once,
reads the current value, then evaluates the right-hand side, combines, and stores
back into the same place.

```tg
const counts = {"a": 0};
counts["a"] += 1;
counts["a"] += 4;
print(counts["a"]);
```

```text
5
```

```tg
class Counter {
    var value = 0;
    Counter() {}
    function bump() {
        this.value++;
        return this.value;
    }
}
const counter = Counter();
counter.bump();
print(counter.value);
```

```text
1
```

**Flat locals.** A name that no nested function reaches lives in the frame's own
locals instead of the shared environment, so reading or writing it is a slot index
rather than a lookup. That covers a `for` header's counter — `++index` is a plain
slot write — and the collection and key of an index, so `items[index]` and
`items[index] = value` are each a single instruction with no operand stack traffic.

```tg
var total = 0;
for (var index = 0; index < 4; ++index) {
    total += index;
}
print(total);
```

```text
6
```

A counter that a nested function *does* reach stays in the environment, because
the closure reads it through the scope chain. It is one variable, so a closure
made in the loop body sees the value the loop left behind:

```tg
var readers = [];
for (var index = 0; index < 3; ++index) {
    readers += [function () { return index; }];
}
print(readers[0](), readers[1](), readers[2]());
```

```text
3 3 3
```

**A comprehension's own variables shadow rather than capture.** Which names have
to stay in the environment is decided from the names a nested construct mentions,
and a comprehension, a generator, or a slice bound is evaluated separately — so
the names it reads from outside must be reachable. Its *own* clause variables are
not: they are bound in the scope the comprehension opens, so a reference to one
never reaches the enclosing scope. Scanning the clauses in order, and forgetting
each clause's variable from the point it is bound, is what keeps a loop counter
from being forced into the environment just because a comprehension elsewhere
uses the same name.

```tg
var index = 99;
const squares = [index * index for index in [1, 2, 3]];
print(squares, index);
```

```text
[1, 4, 9] 99
```

**Inline caches for members.** A property access remembers the receiver it last
resolved and the value it produced, so a member read in a loop — a module binding,
a class method, a dictionary entry — resolves once. An instance *field* read is
remembered as a position in the class's field layout rather than as a value,
because a field is mutable state. A receiver whose members cannot be remembered
cheaply, such as a string or a tuple, skips the cache entirely.

## A site learns its operand types

The compiled engine changes its own bytecode while it runs. A numeric site counts the passes
whose two operands were both numbers, and once it has counted enough of them it replaces
itself, in place, with an instruction that does not have to ask again. This is PEP 659's
specializing adaptive interpreter, applied to the sites the compiler had already fused:

- `Binary`, `BinaryConstant`, `FlatBinaryFlat`, `FlatBinaryConstant` — an operator on two
  operands, which becomes its `Number` form and reaches the arithmetic directly.
- `FlatJumpUnlessFlat` and `FlatJumpUnlessConstant` — a fused comparison and branch, which
  is a loop's condition and the most-executed instruction in any loop.
- `FlatBinaryFlatStore`, `FlatBinaryConstantStore`, `BinaryStore` — an accumulator, whose
  `Number` form also steps over the string case's in-place append, which two numbers can
  never take.

The count is [`SPECIALIZE_AFTER`](../crates/tiger-object/src/bytecode.rs), ten passes. A
site that sees anything else inside the count stops counting and settles as a general site —
with its operator left exactly as the compiler wrote it, so the numeric fast path and the
in-place string append both survive. Either way the site is left with an instruction the
engine will not count again, which is why the machinery costs a long loop nothing: the state
lives in the instruction, and reading it is a field of an instruction the dispatch had
already copied.

A site specializes, and a site can then be put back. Ten passes of numbers and then a string
is exactly that, and the answer is the interpreter's either way:

```tg
function fold(values) {
    var total = values[0];
    for (var index = 1; index < len(values); ++index) {
        total = total + values[index];
    }
    return total;
}
print(fold([1, 2, 3, 4]));
print(fold(["a", "b", "c"]));
```

```text
10
abc
```

The second call runs the instruction sites the first call specialized, meets two values that
are not numbers, and puts the general instruction back — settled, not removed. The
comparison is a loop's own: a condition on two numbers is decided and branched without ever
building the `bool` the unfused form would, and the branch is taken on the result.

`benchmarks/01_arithmetic.tg` carries the same shape — a `fold` that is called with numbers
and then with strings, and a `countdown` whose bound decides whether its sites see enough
passes to settle at all — and the differential test runs it on both engines.

### What specializing is worth

A micro case at this scale moves more between two builds of the *same* source than this
change is worth, so a before-and-after across builds cannot read it. Two measurements can.
Each is one build against another that differs in one constant, which is the only way to know
that what moved is the specialization:

- With the count set out of reach — every instruction and guard present, nothing ever
  specializing — the protocol's section times are the same as with the count at ten. That is
  what says the section deltas a build-to-build comparison reports are layout rather than
  work.
- The micro cases then read as the count's own contribution. Best of 60, in release:

| micro case | never settles | settles | what the site is |
| --- | --- | --- | --- |
| `loop/count` | 12.0 ns | **9.8 ns** | a loop's condition and its accumulator |
| `list/write-collide` | 14.5 | **11.7** | a condition and an index store |
| `list/write` | 14.3 | **11.7** | |
| `string/concat` | 18.8 | **12.9** | the condition only; the accumulator settles general |
| `operator/mod-small` | 21.8 | **20.0** | a condition, a product, and a remainder |
| `list/read` | 28.5 | **25.1** | a condition and an index read |
| `field/read` | 30.1 | **26.7** | |
| `call/alias` | 38.8 | **35.3** | a condition, with a built-in call beside it |
| `recursion/fib20` | 51.1 | **46.7** | two calls and an add per frame |
| `dict/set` | 111.7 | **110.1** | a condition, which is a fraction of the work |
| `dict/read-modify-write` | 239.6 | **238.3** | |

Every case is faster and none is slower. What the count buys is the type question the general
path asks on every pass: a fused comparison and branch replaces a call that built a `bool`
only to test it, and an accumulator replaces the two tag tests and the `Option` its answer
came wrapped in. What it does not buy is the operator, which the compiler had already
resolved, or the `Value` moves, which are still 16 bytes each — and that is why the gain is
5–30% of a site's cost rather than a step change in the engine. A number is already inline
in the `Value` that carries it, so there is no pointer to chase and nothing for a packed
representation to remove here; that is what the `disassembles` test reports `Value` at 16
bytes for, and `Instr` at 20 — the same 20 the adaptive state fits inside, because every
field it added landed in padding the instruction already had.

## Measuring

Three harnesses report timings. The first two are diagnostics; the third is a
threshold, and it is the only performance test in the repository:

```sh
# A per-operation cost for the interpreter's hot paths, in process.
cargo test -p tiger-evaluator --test micro --release -- --ignored --nocapture

# Every benchmark program, in process, with no process startup in the numbers.
cargo test -p tiger-conformance --test timing --release -- --ignored --nocapture

# The paired Tiger/Python workload, in process on both sides, asserted. The assertion
# is asked for, because the ratio is against whatever CPython the host has on `PATH`:
# without the flag the same run reports its table and passes.
TIGER_PROTOCOL_ASSERT=1 cargo test -p tiger-conformance --test protocol --release -- --nocapture

# One program, on either engine.
TIGER_BENCH_FILE=benchmarks/24_algo.tg TIGER_BENCH_ITERATIONS=200 \
  cargo test -p tiger-conformance --test timing --release -- --ignored --nocapture time_source_file
TIGER_ENGINE=tree TIGER_BENCH_FILE=benchmarks/24_algo.tg \
  cargo test -p tiger-conformance --test timing --release -- --ignored --nocapture time_source_file
```

The micro harness runs a fixed number of operations per case and reports the best
of several runs, so a change can be read as nanoseconds per call, per loop
iteration, or per dictionary operation. `TIGER_MICRO_ITERATIONS` sets the batch
size and `TIGER_ENGINE=tree` runs every case on the tree-walker instead.

`TIGER_ENGINE=tree` runs the same program on the tree-walker, which is how the
compiled engine's share of a workload is measured without a profiler.

To compare Tiger against another language on identical work, run the paired
workload:

```sh
python benchmarks/python/compare.py
```

It runs `benchmarks/python/workload.tg` and `benchmarks/python/workload.py`,
refuses to report a time unless both print the same checksum, and subtracts each
language's process-startup floor so the remaining column compares the
interpreters rather than the launchers. See
[benchmarks/python/README.md](../benchmarks/python/README.md).

That end-to-end comparison is the wrong measurement for the interpreters, though:
the workload is one to three milliseconds of work against a five to eighteen
millisecond startup floor, so the subtraction mostly amplifies the floor's own
jitter. `cargo test -p tiger-conformance --test protocol` — `make protocol` — is
the measurement to read instead. It runs the same workload **in process on both
sides**: Tiger through `Evaluator::compile` and `Evaluator::execute_compiled`
(the compile once, the run many times), CPython through
`benchmarks/python/protocol.py`, which compiles `workload.py` once and executes it
the same number of times. One pinned section at a time in both files, 300
iterations each, the minimum reported, and every checksum compared, so the two are
demonstrably doing the same work. It **fails while any section is slower than
CPython** with the program's own compile removed from both sides, which is what
"2.6x slower" means. A debug build reports the table and asserts nothing, because
the ratio a debug build measures is not a ratio anything is about.

## Where the time goes

The regression programs are small — under two milliseconds each in process — so
the number that matters for a single run is usually process startup, which is
where Tiger is far ahead of a CPython process. Within the interpreter, the
per-operation costs the micro harness reports are roughly:

| Operation | Cost |
| --- | --- |
| A loop iteration, counting | ~10 ns |
| A list read or write through a flat index | ~12–25 ns |
| A field read or write | ~22–27 ns |
| A built-in call | ~40 ns |
| A function call | ~50 ns |
| A string method call (`len(w.upper())` over the call it sits on) | ~39 ns |
| A small integer through `str()` | ~71 ns |
| A comprehension element over a range | ~51 ns |
| A modulo, by the shape of its operands | ~9–16 ns |
| A dictionary insert or lookup | ~119–129 ns |
| A read-modify-write through a dictionary | ~255 ns |

Those are the micro harness's own cases, best of its runs, and the last five rows are new in this
session's work. The dictionary row is what moved most: it was 210–220 ns when this page was written,
and the two hundred and fifty nanoseconds above it are the shape `counts[key] += 1` has — key
building, a membership check, a read and a store. The string-method, `str` and dictionary rows were
re-measured, best of 15, after the two changes recorded under "A method call builds no callable" and
"A small integer becomes a string without a buffer"; the other rows are as the earlier pass left them.
The loop-iteration, index, field, call, and modulo rows were then re-measured, best of 60, with the
builds described under [a site learns its operand types](#a-site-learns-its-operand-types); the loop
row is the one that change moved, and the rest are this machine's reading of the same cases.

The per-run floor is 4 µs, and two measurements say what it is **not**. Removing a run's copy of the
process environment takes it to 3 µs; stubbing the `random` and `secrets` module objects leaves it at
4 µs, unchanged. So the environment copy is one microsecond of four, the module objects are free, and
the rest is `Evaluator::new` building a fresh top-level scope and installing the built-ins into it.
Deferring a module member would buy nothing, which is why the floor is not where a light section's
remaining time will come from — and why neither experiment was kept.

What the two tightest sections actually execute is worth writing down, because it decides which
instruction to build next. `methods` is `checksum += len(words[i].upper())` in a loop — an index read,
two `len` calls, and the method call — and the method call is about half of that section, at ~104 ns
against ~41 for a call beside it. `dicts` is a membership test and a read-modify-write, but it also
builds its key every iteration with `"k" + str(i % 50)`, so a built-in call and a concatenation are as
much of that section as the dictionary probes are. The method call was the one operation either section
turned on, and it is what `CallMethod` below was built for; `str(i % 50)`, the other thing both sections
pay for, is what the integer fast path below answers.

The flat call path's prologue was trimmed in the last push of this session. Six of the seven facts it
tested — no frame of its own, flat parameters, no `super`, neither variadic nor keyword parameters,
and locals that fit the inline array — are facts about the *declaration*, not about the call, so the
compiler now folds them into one flag on the chunk and a call site tests that and its own argument
count. Measured, best of 40: `call/empty` 72.7 → 69.1 ns, `call/discard` 59.5 → 55.5, and `fib(20)`
64–71 → 62.3 ns per call — while `call/builtin`, which does not take that path, stayed at 40.5, which
is what says the move is the flag and not the noise floor. The sections moved with the micro cases:
`recursion` 1.419–1.477 → 1.375 ms, `all` 1.78–1.84 → 1.749 ms. It is the first change this session
to move the call path at all, and it moves it by removing work from the prologue rather than by
removing Rust frames — which is the third measurement saying that frames are not where the call's
time is.

One line more of the same: the array a flat frame keeps its locals in was cut from eight slots to
four (`INLINE_LOCALS`). A frame initializes every slot to `Null` and drops every slot on the way out,
and most frames — `fib`'s, any one-parameter function — use one. Measured on the protocol's own
300-run minimums: `recursion` 1.375 → 1.254 ms, `calls` 3.324 → 3.240 ms, and `all` 1.749 → 1.600 ms,
the best `all` this session has seen. Frames with five to eight flat locals now take the pooled vector
path instead of an inline array, which is what makes this a trade rather than a free win; the sections
say it comes out ahead, and an array sized per frame — a slot or two for most, eight for the rest —
would keep both halves of it.

A note on the micro harness, which cost this session a wrong reading. Its cases are not equally
repeatable. Running the same build twice in a row moved `recursion/fib20` by 0.2% and `call/discard` by
0.5%, but `call/str-method` by 14% and `call/method-index` by 200%. `call/empty` moved 11% *between*
the two builds, and the section rows then said its section had improved — so layout effects between
builds are real at this scale, and a micro case has to be shown stable before its delta is read.

A global name read used to be about 13 ns of a built-in call's 62: calling `len`
through a flat local alias instead of through the built-in name measured 48 ns
against 61. A binding can be reassigned, so remembering one needs an invalidation
the property cache cannot give it, and that is what `Chunk::name_cache` is: the
resolved name is remembered per instruction, keyed on the environment, a global
binding epoch that every write to a binding bumps, and the value itself. It took the
built-in call from 61 ns to 48, and `call/alias` — the same loop through a flat local
alias — now measures 43, so a name read costs 5 ns where it cost 13.

Two changes that look like the obvious next step were implemented and measured
*slower*, and are recorded here so they are not tried again:

- **An explicit frame stack inside the loop**, the design Lua uses, in place of the
  Rust frames a call re-enters through — tried the way it is usually written, where
  every instruction reads the chunk, the program counter, and the stack out of the
  current frame. Parking a frame's state costs about what the frames it replaced did —
  the state is a `Chunk`, an environment, and three vectors — and the larger loop body
  made every nested entry more expensive. It cost 15% on `call/method` and gained
  nothing on `call/empty`. The version that keeps the instruction state in the loop's
  own variables is in the engine now, and it wins: see
  [a call stops re-entering the engine](#a-call-stops-re-entering-the-engine).
- **`Value::String(Rc<String>)` with an in-place append**, so that `s += x` in a loop
  grows the accumulator instead of copying it. A growable inner type costs one more
  allocation than the `Rc<str>` it replaces, on every string the program builds, and
  the append does not even reach the accumulator through the fused store
  instructions, which borrow their operands. It cost 7% on dictionary operations and
  18% on the corpus's heaviest program. That measurement was also incomplete — it
  thinned `String` but left `Tuple` a fat pointer, so `Value` never actually shrank;
  see [a smaller `Value`](#a-smaller-value-needs-a-thinner-string) for the version
  that does shrink it.
- **Fusing the whole call site into one instruction** — the name's load, the
  instructions that compute the arguments, and the `Call` — with a descriptor the engine
  reads for the name and the arguments, and hands to the `Call` below for the shapes it
  does not enter as a frame itself. The compiler emitted it for a callee that is a free
  name and arguments that are reads (a frame-local, a constant, or one of those combined
  by an operator), and it cut the instructions executed by 28% on the paired workload —
  33% on its `recursion` section, where `fib` went from six instructions per invocation
  to four. It was still **slower**: two binaries below alternate, best of 25, and a call
  with the callee in a name costs 4 ns more than the same call with the callee in a flat
  slot, where the descriptor version should have gained the two dispatches it removed.
  What the arm added was a second interpretation of the same work — a descriptor read per
  call, an argument loop, and a `Result` per argument — and each of those costs more than
  the dispatch it replaced. The lesson is the one this page keeps meeting: an arm that
  does one thing with one instruction's worth of state is cheaper than a general one, so
  a fused instruction has to *remove work*, not read a table that says what to do.

## What a run costs before it does anything

The protocol's `overhead` row is the measurement that explains the light sections.
It names a section neither program answers, so each runs its whole program and does
none of any section's work; what is left is what a run costs before it starts.

| Run | Before |
| --- | --- |
| A Tiger run over a compiled program | 52 µs |
| CPython's `exec` of a compiled code object | 1 µs |

That is where `strings`' 5.6x came from: 200 appends are 3 µs of work, and a 52 µs
floor is more than fifteen times that. The floor was 93% of `strings`, 60% of
`loop`, 55% of `methods`, 27% of `lists`, and 20% of `dicts` — five of the eight
sections were reporting a run's setup rather than the interpreter.

Nearly all of it was `builtins::install`, which rebuilt every built-in, every module
table, and the `sys.env` snapshot of the whole process environment on every run.
None of that is program state. What is stateless is now built once per thread and
copied into a run's scope as an `Rc` clone and a name insert (`builtins::shared`),
and the process environment is read once per thread (`environment_variables`) with
each run getting its own copy of the dictionary. The three that do belong to a run —
`sys`, whose `argv` is the run's, and `random` and `secrets`, whose generators a
program seeds for itself — are still built per run, and
`crates/tiger-evaluator/tests/runs.rs` pins what a run may and may not inherit from
the one before it.

| Measurement | Before | After |
| --- | --- | --- |
| A run's own setup (`overhead`) | 52 µs | 13 µs |
| `loop` | 0.086 ms | 0.047 |
| `lists` | 0.192 | 0.151 |
| `dicts` | 0.262 | 0.218 |
| `strings` | 0.056 | 0.016 |
| `methods` | 0.094 | 0.054 |
| `all`, the whole workload | 2.027 | 1.974 |

The 13 µs that is left is the three per-run modules, about 110 name inserts, and
the hundred-or-so-entry `sys.env` dictionary — the last of which a run has to own,
because a program may write into it. On the CLI's clock that is 13 µs of a five
millisecond process, which is not what makes a process fast or slow; it is what a
host that runs many programs pays per program, and it is the floor the protocol's
light sections are read against.

**The protocol test failed on all eight sections when this was written**, and the floor was
no longer most of the reason. Take it off both sides, since it is a run's own cost and not
the section's, and `strings` is 3 µs of work against CPython's 9 — faster — with
the floor making the ratio 1.56; `loop` is 1.2; `lists`, `dicts`, and `calls` are
1.5 to 2.1; and `methods` (2.2) and `recursion` (2.9) are calls and the work around
them, which is where the rest of this page's remaining cost sits.

### A second pass at the floor

The 13 µs above was three things, and two of them were still doing per-entry work for a scope
that takes all its names at once.

**The install was one name insert per binding.** About a hundred of them, and each cost a
borrow of the scope's cell, a hash of the name to check it was not already there, and an atomic
bump of the binding epoch. `Bindings::extend_shared` takes the batch instead: one bump, one
index build, and the entries pushed straight in, because a fresh scope has nothing for them to
collide with — which is what the debug assertion on it states, and what both callers
(`builtins::install` and `modules::module`) do.

**And `sys.env` was rebuilt entry by entry.** The process environment was already read once per
thread, but each run inserted its hundred variables into a fresh dictionary one at a time: a
hash of each name, a `borrow_mut` of the cell for each, and a rehash of the whole index at every
doubling of its growth. A run has to *own* its copy — a program may write into `sys.env`, and
`tests/runs.rs` pins that the next run must not see it — but owning a copy is not the same as
building one: the snapshot is now a dictionary built once per thread, index and all, and a run
clones it. Two allocations instead of a hundred inserts and six rehashes.

| Measurement | Before | After |
| --- | --- | --- |
| A run's own setup (`overhead`) | 13 µs | **5 µs** |
| `overhead` against CPython | 16.2x | **4.6x** |
| `strings` | 0.020 ms | **0.008** |
| `strings` against CPython | 1.91x | **0.71x** |
| `loop` | 0.050 | 0.042 |
| `lists` | 0.112 | 0.100 |
| `dicts` | 0.224 | 0.214 |
| `methods` | 0.056 | 0.045 |
| `recursion` | 1.431 | 1.408 |
| `calls` | 3.461 | 3.420 |

`strings` is the first section the protocol passes: 200 appends are about 9 µs of work, and the
floor that made that 1.91x is now under the work rather than over it. The sections still over the
ratio are the ones the floor was never hiding — `recursion`, `calls`, and `methods` are calls,
`dicts` is dictionary operations, and `loop` and `lists` are the index and step machinery the
compiled engine shares with comprehensions.

## Reading the corpus number

`time_benchmark_programs` runs each program through `tiger_conformance::run`, which
runs a program **on a big-stack thread** — a thread per run. At roughly 0.17 ms of
spawn and join per program, that is about 0.17 ms × 43 ≈ 7 ms of the 25 ms a full
corpus pass reports, so a fifth of the number is the harness rather than the
interpreter. To measure one program without it, use `time_source_file`, which calls
`run_with_options` directly:

```sh
# Parse and lex alone, and the split between them, for every benchmark program.
cargo test -p tiger-evaluator --test micro --release -- --ignored --nocapture where_a_program
```

## What the heavy programs spend their time on

The corpus total says where the time is not. Eight programs hold about 46% of it, and
the micro harness says nothing about how often each of its operations runs. Running the
same program on both engines separates the two: the tree-walker and the compiled engine
share the built-ins, the collections, the strings, and the allocations, and differ only
in how they walk the program.

```sh
TIGER_BENCH_FILE=benchmarks/26_algo_rainwater.tg TIGER_ENGINE=tree \
  cargo test -p tiger-conformance --test timing --release -- --ignored --nocapture time_source_file
```

| Program | Compiled | Tree-walker | Ratio |
| --- | --- | --- | --- |
| `24_algo` | 1.098 ms | 1.709 ms | 1.56 |
| `26_algo_rainwater` | 1.349 | 2.647 | 1.96 |
| `30_algo_palindromes` | 1.679 | 2.377 | 1.42 |
| `32_type_inspection` | 1.248 | **1.138** | **0.91** |

Two things follow. The compiled engine is only one and a half to two times the
tree-walker, not the many times a bytecode loop is usually worth — so most of what these
programs do is *not* dispatch, it is the runtime helpers both engines call: a built-in, a
dictionary operation, a string, an allocation. A change that only makes the loop tighter
cannot reach it. And `32_type_inspection` is *slower* compiled, which is worth knowing:
it is a program about diagnostics, so its time is in raising and catching errors, and the
compiled engine's error path is the more expensive of the two.

The same measurement rules out two changes that look obvious. `len` is the most common
built-in — 283 calls across the corpus, examples, and portfolio programs, many of them
inside loops — and answering it inside the dispatch instead of through its own function
changed nothing measurable: the name lookup that finds it costs about what the call did,
and the extra test is paid by every *other* built-in. And the hasher for dictionary and
set keys is the standard one on purpose, which the comment on `NameHasher` says: those
keys are program data, and a program that indexes untrusted input should not be
floodable. The browser build has no host entropy, so a random seed cannot buy that back
there.

### Where the instructions go

That conclusion needed a number the harness could not give: how often a program runs each
*kind* of instruction. `crates/tiger-evaluator/src/profile.rs` counts exactly that, behind
the `profile` feature so a release build has no trace of it:

```sh
cargo test -p tiger-evaluator --features profile --test profile -- --ignored --nocapture
```

Over the 44 benchmark programs — 780,348 instructions — the mix is:

| Category | Share | Cost each | Weighted |
| --- | --- | --- | --- |
| A flat local or a constant | 12.3% | ~5 ns | 0.6 |
| A name that walks the environment | 6.4% | ~3 | 0.2 |
| An index read | 2.3% | ~13 | 0.3 |
| A property read | 1.4% | ~15 | 0.2 |
| A flat store | 15.0% | ~3 | 0.5 |
| An environment store | 1.7% | ~15 | 0.3 |
| An operator | 15.7% | ~6 | 0.9 |
| A branch | 14.2% | ~4 | 0.6 |
| **A call** | **6.6%** | **~58** | **3.8** |
| Control, collections, and the rest | 24.5% | ~8 | 2.0 |
| **All** | 100% | — | **~9.4** |

The corpus is not the same one this table first described: it holds 44 programs now, and the
comprehension program added in this session's work is over half of its instructions, which is what
moved locals down and control, collections, and the rest up. The conclusion the table was written for
survives it — a call is one instruction in fifteen and about 40% of the time, because the *weighted*
column is what a change has to move.

A call is one instruction in fifteen and about 40% of the weighted time. Everything a call costs —
resolving the name, the operand-stack round trip for the callee and its arguments, the
dispatch, the scope, the frame, and the return — is charged to that one category, and it
is ~58 ns against ~5 ns for the load beside it. That is what "most of the time is not
dispatch" turns out to mean: not the loop, the *calls*.

Two slices of it came off cheaply. `Instr::Call` already answered a built-in directly; it
now calls a plain function and a bound method directly too, skipping the general
dispatch's type match and the argument slice it would rebuild. Each dispatch is one match
arm, so a fast path is exactly what the general path does for that callee type — which the
differential test holds it to:

| Operation | Before | After |
| --- | --- | --- |
| A call, empty body | 78.1 ns | **73.0 ns** |
| A call, two arguments | 88.6 | **83.2** |
| A method call | 100.7 | **89.5** |
| `fib(20)` | 81.3 | **74.9** |

The corpus moves about a percent, which is the harness's own spread, so the micro is where
this reads. What is left in a call is a dozen steps of a few nanoseconds each — sessions 2
and 9 both went at it directly and found the same — and the next real cut has to remove
several at once, which is a redesign rather than a fast path.

### A call whose value is dropped

`Pop` reports no errors, which the position rule allows a fusion to absorb — and the
shape it appears in most is a call written as a statement: `items.append(x);`,
`print(y);`. `Instr::Call` carries a `discard` flag that the peephole sets when a `Pop`
follows it, so the call never pushes its result and steps over the slot the `Pop` left
behind. The `Pop` was `control`'s largest single contributor, and the corpus loses 1,230
of its 350,333 instructions to it:

| Operation | Value used | Value dropped |
| --- | --- | --- |
| A function call | 74.9 ns | **63.6 ns** |
| A built-in call | 40.5 | **28.7** |

The corpus barely moves, because only about one call in twenty-five has its value
dropped. The micro is where this reads.

Two things went wrong on the way, and both are worth recording, because the test suite
caught neither by itself. The first version set the flag but did not step over the
absorbed slot, so the `Pop` still ran — and popped a value the call no longer pushed.
Every test passed, because the program's *output* was unchanged; the profile tool caught
it, because the instruction count did not drop. The fix for that then broke a benchmark's
error column: stepping `pc` before the handler reads `chunk.position(pc - 1)` reports a
call's error at the `Pop`'s position instead of the call's. `19_file_io` compares an error
message character by character, which is what caught that one. The step now happens after
the position is read, on each path out of the arm.

### The parse is allocation-bound, not copy-bound

The sizes the diagnostic prints say which way to go, and they are easy to misread.
`Token` is 88 bytes, `Base` 24, `Expr` 80, and `Stmt` 216 — `Stmt::Assign` holds two
`Expr`s inline. That is a lot to move, so the obvious move is to shrink them. It would
not pay: the parse's cost is the *allocations* its shape implies, one per boxed node, not
the bytes those nodes carry, and a smaller struct is still an allocation. `Base`'s
`Rc<str>` source name is the one part that is neither — every node in a file clones the
same handle, which is a refcount write per node — and removing it means threading the
source name through the evaluator instead of storing it on every node.

## What dominates the corpus

The corpus total hides how much of it is the engine, because the harness it runs
through adds a thread per program. Measured one program at a time, without that
thread, the compiled engine is **1.4× to 1.9× faster than the tree-walker** on the
heaviest programs — `26_algo_rainwater` 1.55 ms against 2.89, `30_algo_palindromes`
1.66 against 2.46, `24_algo` 1.17 against 1.68 — and about even on one whose time is
inside a built-in, such as `20_list_sort`. So roughly 40% of a heavy program is the
instruction loop and the operations it performs; the rest is the Rust built-ins those
programs call, and the parse.

That makes the per-operation costs the thing to move, and an index read is the
operation a loop spends most of its time in. `a[i]` is answered directly when the
collection is a list and the key is a non-negative integer in range — a borrow and an
index, rather than a call, a key check, and a second borrow — and everything else, a
negative index included, falls through to the path that owns the diagnostics. The
same shape is answered for `a[i] = value`. That took a list read from 38 ns to 30 and
a list write from 25 ns to 17.

The same trick was tried for a field read and **rejected on measurement**. Answering
a cached instance field directly in the handler improved the isolated case by 6%, and
made three real object-heavy programs slower (0.191 against 0.188 ms, 0.201 against
0.196, 0.207 against 0.203): the extra code in the handler costs more than the call it
saves. `cached_property` is left as it is.

### A built-in call is the single biggest item

Measured the same way, a built-in call costs **49 ns beyond the loop it sits in**
(`s += len(a)` at 62.2 ns an iteration against a bare `s += n` loop at 13.2), and a
user function call 68.6. That matters more than any of the above, because the
heaviest programs are *made* of built-in calls: `26_algo_rainwater`'s inner loop
evaluates `j < len(heights)` and `math.max(left_max, heights[i])` on every pass, so
one run of it makes about 7,500 calls — roughly a third of that program.

So `Instr::Call` answers a built-in called with positional arguments itself, without
the general call path's dispatch, and builds the diagnostic `Base` a failure would
need *only if one happens* — building one clones the shared source name, which is a
write to a refcount on every call. That took `call/builtin` from 60.5 ns to 55.7 and
the call cost above from 49.1 ns to 43.6.

The largest part of what is left was the name lookup, so a name read is remembered
too. `Chunk::name_cache` holds, per instruction, the environment the read resolved
in, the value it resolved to, and the binding epoch it resolved at; `call/builtin`
fell again, to 47.4 ns, and `26_algo_rainwater` — whose inner loop reads `len` and
`math.max` by name on every pass — by 8.5%.

**Both halves of that key are needed, and the first version had only one.** The
binding epoch alone looks sufficient, and is not: a chunk is not tied to one
environment. Three closures built in a loop share the loop's chunk and each carries
its own environment, so the same instruction runs against a different scope chain per
closure, and a read remembered for the first answers wrongly for the second. The
benchmark corpus caught it — `07_closures` reported `loop captures: 10 10 10` where it
should report `10 15 20` — and
`compiled_paths.rs::a_closure_in_a_for_in_loop_reads_its_own_binding` now pins the
case on both engines. The epoch is what covers the other half: a hit is only used
while no binding anywhere has been written since the slot was filled, so a `var` that
shadows the name, or a scope that starts holding one, cannot be missed. That guard
has to be *complete*, which is why it is bumped inside `Bindings`' own mutators —
`insert_shared`, `get_mut`, `set_at`, `clear` — rather than at the call sites: the
first attempt missed `set_at`, the slot-based store the compiled engine uses for a
captured local, and the same benchmark caught that too.

What is left is diffuse rather than concentrated: about ten steps of two to five
nanoseconds each — an `Rc` deref and two indirect calls through vtables for the
built-in itself, a 24-byte move of the callee out of the stack and another of the
result back in, the argument slice's bounds check, the remembered read's own key
check, and the drops of the arguments. None of them is large enough to be worth a
change on its own, which is why this is where the remaining time sits.


### A smaller `Value` needs a thinner string

`disassembles_the_hot_shapes` prints the sizes too, which is how this became clear.
`Value` was 24 bytes with a 16-byte payload, because exactly two variants are fat
pointers: `String(Rc<str>)` and `Tuple(Rc<[Value]>)`. Every other variant is a thin
`Rc`. Thinning the tuple alone changes nothing, and thinning the string to
`Rc<String>` — a growable inner type — does make `Value` 16 bytes but gives most of it
back: `Rc<str>` keeps the bytes in the allocation it counts, while `Rc<String>` adds a
second allocation's worth of teardown and a second indirection to every read of those
bytes, which costs about 20 ns for every string a program builds. That version was
measured and rejected — the corpus came out 2.3% *slower*, 18.34 ms against 17.92,
with `30_algo_palindromes` losing 1.485 → 1.604 ms.

What the 16-byte `Value` needs is a string that is thin *and* keeps its bytes in their
own allocation, which is what `Rc<str>` already is internally: a pointer to a
length-prefixed buffer, with the length read back from the header. Rust has no safe way
to spell that, so `tiger-object` has one: `RcStr`, in
[`crates/tiger-object/src/rcstr.rs`](../crates/tiger-object/src/rcstr.rs) — a pointer
to the bytes, with the count and the length in a header in front of them. That is the
same single allocation and the same single indirection as `Rc<str>`, in eight bytes
instead of sixteen. It is the one place in this runtime where `unsafe` is
load-bearing, so it is a module of its own with its own tests, and `refs()` is exposed
to them so the accounting is checked directly rather than inferred. Its `Clone` and
`Drop` are `#[inline]`: without that the derived `Value::clone` grows past the
inliner's limit and a list read gets 15% *slower*, which is how the first version of
this measured.

With it, `Value` is 16 bytes and every operation in the micro harness improves:

| Operation | Before | After |
| --- | --- | --- |
| A loop iteration, counting | 14.4 ns | 13.8 ns |
| A list read | 30.2 | 27.8 |
| A list write | 17.1 | 16.5 |
| A field read | 31.0 | 28.7 |
| A built-in call | 48.6 | 42.7 |
| A dictionary insert | 207.7 | 196.9 |
| A string concatenation | 219.8 | 222.8 |

The corpus is 2.2% faster — 17.38 ms against 17.77 over the 39 programs that run
without input — which is much less than the micro harness suggests, because that total
is mostly parsing, and the parse does not care how wide a `Value` is. The browser build
pays 6 KB for it: 891 KB to 898 KB.

### A string that fits in its handle

The handle's eight bytes are a pointer, and allocations are eight-aligned, so the low
bit is always clear — which leaves a way to say "the text is in here". `RcStr` sets it
for any string of seven bytes or fewer: the first byte holds the length and the rest
holds the text. Such a handle is copied on clone and does nothing on drop, because the
text is part of it rather than shared behind a count, and `append` writes it in place
and keeps it inline while the result still fits.

That is where most strings live. A dictionary key, a character read out of a string,
the result of `str(1)`, a field name — programs build far more tiny strings than large
ones, and every one of them used to be an allocation and a free. The dictionary cases
in the micro harness, which build two strings per operation, are the clearest measure:

| Operation | Before | After |
| --- | --- | --- |
| A dictionary insert | 203.4 ns | **164.3 ns** |
| A dictionary lookup | 210.3 | **174.6** |
| A built-in call | 42.8 | 40.9 |
| A method call | 96.1 | 90.6 |

A dictionary operation is the most expensive row in the table above, so a fifth off it
is worth more than the raw percentages suggest, and nothing in the harness regressed.

The corpus does not resolve the change: its rounds land anywhere between 17.30 ms and
17.66 ms either way, and the harness's own spread is about a percent. The browser build
pays 14 KB for it — 899 KB to 913 KB — which is the cost of the inline path's extra
branching in `len`, `as_str`, `clone`, and `drop`.


### Building a string in a loop

`s += text` is the one operation whose cost is not proportional to its work: with an
immutable string it copies the whole accumulator on every iteration, so building a
string of `n` pieces costs the square of its length. `RcStr` grows the buffer instead
when it can — when it is the only handle and there is room — and copies when the string
is shared, which is what keeps an alias of the accumulator independent of it.

The two fused shapes a flat accumulator produces, `s += "literal"` and `s += other`,
take that path. A loop appending a constant goes from **222.8 ns to 44.2 ns** per
iteration, five times faster, and every other operation in the harness is unmoved.

The corpus does not move, and the reason is worth recording rather than guessing at:
its string-building loops are `rest += s[i]`, an *index* operand, which compiles to
`BinaryStore` with the operand on the operand stack. That load clones the accumulator,
so the target still holds a second handle when the store runs and the buffer cannot be
reused. That is now handled: the emitter scans the instructions the operand produced
and, when none of them writes one of the frame's own slots, marks the store `reuse`.
The store then empties the target's slot before appending, which drops that second
handle and leaves the string with one — so `x += items[i]` grows in place too.

The check has to be made where the operand's instructions are known, and the
peephole cannot make it: `fuse` replaces an instruction in place and leaves the
instructions it absorbed in the array, so looking two instructions back from a
`Binary` lands on a stale slot as often as on the load it wants. Emitting the fused
store directly from the emitter instead of letting the peephole build it is *not* an
option either — that changes what the peephole sees and unbalances the operand stack.
The `reuse` flag rides on `StoreFlat` and is carried into `BinaryStore` by the
existing fusion, which leaves the emitted instruction stream byte-for-byte what it
was.

It is worth being straight about the result: the flag fires (`reuse: true` on
`sum += d[k]`, `rest += s[i]`, and the rest) and the tests agree on both engines, but
the corpus does not move. `30_algo_palindromes` costs what it did before, so the loop
this was aimed at is not where its time goes. The change removes a quadratic copy for
programs that build a string by appending indexed elements, and that is what it should
be judged on — the corpus simply does not contain one.

### A string key's hash is not what a dictionary operation pays for

A dictionary operation with a string key costs about 20 ns more than the same operation with an
integer key — 88 against 69 in the micro harness, for a read of a key that is already built — and
the obvious reading is the hash: `Hash for RcStr` hands the hasher the bytes and a terminator,
while a number hands it one word. An inline string makes that look avoidable, because its handle
already *is* its text (a length byte, the bytes, then zeros), so equal text means equal handles and
one `write_usize` could stand in for a `write` plus a terminator. Equality could short-circuit the
same way, comparing two handles as one word.

Both were implemented, and neither moved anything:

| micro case | Before | After |
| --- | --- | --- |
| `dict/set` | 149.8 ns | 146.5 |
| `dict/get` | 158.9 | 158.4 |
| `dict/get-held` | 89.2 | 88.3 |
| `dict/set-int` (control) | 57.1 | 57.2 |
| `dict/get-int` (control) | 69.1 | 68.8 |
| `list/read` (control) | 26.7 | 26.6 |

The controls are in the same runs, so the comparison holds. The fixed cost of hashing dominates
either form — a three-byte `write` and a `write_usize` cost the same inside a `SipHash` round — and
what is left of those 20 ns is the probe and the key's own plumbing rather than the bytes.
**Reverted**, with the invariant the idea rests on left documented where it is created
(`RcStr::with_capacity`) and pinned by a test, so the next attempt starts from it.

The 20 ns is real, though, and the protocol's `dicts` section is three probes deep in it:
`counts[key] += 1` reads through the index, writes through it, and checks membership before that.
Cutting it means fewer probes or a remembered slot, not a cheaper hash — which is what the next
section is.

### The hash *function* was the untried half

The section above changed what the hash is *fed* and left the hash itself alone. Replacing the
function is the other half, and the language already had the pieces: `NameHasher` is the FxHash
mixing a scope's names are hashed with, under a fixed seed, because a scope holds the program's own
identifiers.

Dictionary and set keys use that mixing now, under a seed read once per process from the standard
hasher's own source of randomness — so a program that indexes values a caller supplied still cannot
be made to collide them all, which is the only reason the standard hasher was there.

| micro case | before | after |
| --- | --- | --- |
| `dict/set-int` | 55.7 ns | **51.3** |
| `dict/get-int` | 69.5 | **63.7** |
| `dict/get-held` | 89.7 | **87.6** |
| `dict/set` | 115.4 | **111.1** |
| `dict/read-modify-write` | 252.2 | **237.2** |
| `list/read` (control) | 25.1 | 26.5 |

The integer cases gained the most, and that is the honest reading of it: SipHash's round was worth
about 4 ns of an integer-key operation and under a nanosecond of a string-key one. So the 20 ns a
string key costs over an integer key is not the hash either, which leaves the key's own plumbing —
an `RcStr` clone, the enum, the `RefCell` borrow, the probe, and the evaluator's dispatch around the
read. Both halves of the hash are measured now, from both directions, and they agree: a dictionary
operation's remaining cost is the probe and the plumbing around it, and the next attempt at one
starts there.

### A case conversion needs no string of its own

`upper` and `lower` ran `str::to_uppercase`, which builds a `String`, and then copied that
into an `RcStr` — two allocations and two copies for one answer, on the path a loop over
`len(word.upper())` takes on every iteration. An ASCII string changes case byte for byte,
so the common case now converts in a stack buffer of 64 bytes and builds the `RcStr`
straight from it: no `String`, and no copy after one. A string that is not ASCII, or is
longer than the buffer, takes the general path unchanged — which is what keeps a mapping
that can change a character's length (`ß` upper-casing to `SS`) exactly where it was.

| micro case | before | after |
| --- | --- | --- |
| `call/str-method`, `s += len(w.upper())` | 81.3 ns | **72.0** |
| `call/method-index` | 84.7 | **74.6** |
| `string/concat` (control) | 13.0 | 13.0 |

Eleven percent on the shape, which for a five-byte word is the allocator and the second
copy and nothing else. It is the same reading the dictionary's two hash measurements gave:
what a short string costs is what happens *around* the text.

### A range steps in place

`IterNext` reaches a lazy source through `iterator_next`, which swaps the iterator's kind out of its
state and stores it back. That is necessary for `map`, `filter`, and a generator body, because they
run user code in between and no borrow may be held across it. A range runs no user code at all, so it
was paying for that safety anyway: one `RefCell` borrow and two moves of the whole kind per element —
and the kind is bigger than it looks, since the generator variant carries two environments and a
cursor list.

A range is now stepped inside the borrow the state is already under, and its arithmetic lives in one
function both paths call, so the fast path adds no second copy of the rule.

| micro case | Before | After |
| --- | --- | --- |
| `comprehension/range`, 2,000 elements over a range | 63.6 ns | **50.9** |
| `for/loop-var-captured`, a `for` over a range with a closure | 125.1 | 118.8 |
| `loop/count` (control) | 13.5 | 13.3 |

Eleven nanoseconds an element, on the shape every `for n in range(...)` in the corpus has. The
protocol's `lists` — a 600-element comprehension over a range plus 1,800 loop passes — goes 0.101 ms
to **0.097**, and `loop` reaches parity with CPython (0.96x on the run that measured it).

### A dictionary remembers where its last lookup landed

The line above is the fix. `Dict` keeps the position its most recent `get` resolved, so a `set` for
that same key writes the entry instead of hashing the key and probing the index again. That is
exactly what `counts[key] += 1` is — a read through the index and a store back through it — and what
every `d[k] = d[k] + 1` in a counting loop is.

An insert appends, so it cannot move an entry a lookup found. The two operations that *can* move one
are `delete`, which shifts every entry after the one removed, and `clear`, which removes them all,
and both drop what was remembered. That is the whole soundness argument, and `tiger-object`'s tests
pin it step by step: a store after a read, after an insert, after a delete, and after a clear, with
entry positions and key order checked at each.

| micro case | Cache off | Cache on |
| --- | --- | --- |
| `dict/read-modify-write`: a membership check, a read and a store | 310.9 ns | **286.1** |
| `loop/count` (control) | 13.1 | 13.6 |
| `dict/set` (control: a fresh key every time) | 149.4 | 149.4 |
| `dict/get` (control: reads, never stores) | 159.4 | 159.4 |

Twenty-three nanoseconds an operation, read as an A/B rather than against a previous run, because the
case did not exist before it. The protocol's `dicts` section goes 0.200 ms to **0.192**: the section
is also key building and a membership check, so an operation's 7.5% is a section's 4%.

Two of the three probes are still there for this loop — the membership check's and the read's.
Removing those means the *compiler* fusing a membership test, a read and a store into one instruction
that resolves the entry once, which is the next step here.

The same cache was then tried on the *read* side, where the membership check before a read-modify-write
asks the same question two instructions earlier. **It is not a win.** `dict/read-modify-write` went
286.1 to 279.0, but `dict/get` went 158.4 to **162.8** and `dict/get-held` 89.0 to **91.0** on a run
whose control (`loop/count`) was 3% *faster* than the one before it — a program that reads many
distinct keys pays for the check on every read and hits it on none. Reverted. The write side is the
shape that repeats a key, and the read side is not.

### A method access built four things

`words[i].upper()` measured **184 ns** against a plain built-in call at 40.5, and the method itself
was not the cost. `methods::string_method` builds a fresh callable on *every* access: `transform`
calls `Builtin::new`, which allocates the `Rc<Builtin>`, boxes the closure, and — with
`name: String` — allocates the name; then `transform` allocates a second copy of that name as the
label its diagnostics quote. Two of those four are avoidable and are gone: `Builtin.name` is a
shared string, which for a name like `upper` lives in the handle and allocates nothing, and the
labels are shared the same way. The six `let label = name.to_string()` sites in `methods.rs` were
the rest of it.

| micro case | Before | After |
| --- | --- | --- |
| `call/str-method`, `s += len(w.upper())` | 183.9 ns | **144.8** |
| `call/method-index`, the same through `words[0]` | 190.1 | **153.4** |
| `call/builtin`, the floor a call sits on | 40.5 | 41.3 |
| `call/empty` / `call/method` | 72.0 / 91.3 | 71.1 / 91.7 |

The name was not particular to strings: a built-in is built for every `list.sort`, `sorted`, `map`,
`filter`, and conversion-method access too. The protocol's `methods` section — four string-method
calls per round — goes 0.046 ms to **0.040**, and its ratio 2.27x to **2.02x**.

What is left of the 144 ns is the two structural allocations (the callable and its closure), the
method's own work (`to_uppercase` returns a `String`, which is then copied into a shared string),
and the call at ~27 ns. Removing the last two means not building a callable at all — passing the
receiver to the call as a first argument, which is what a `CallMethod` instruction would do, and
what a bound method costs in a class today.

### A concatenation copied its result twice

`a + b` on two strings sized a `String`, filled it, and then copied those bytes into the shared
string it returned — two allocations and two copies of the same text. The comment on it said
"allocates once", which was true of the `String` and not of the answer.

`RcStr::concat` fills the shared string's own buffer instead: the pair is sized up front, the first
half is copied into the allocation, and the second is appended into it. A result short enough to
live in the handle — `"k" + str(n)` is three characters — never reaches the allocator at all, where
the `String` did and was then thrown away.

| micro case | Before | After |
| --- | --- | --- |
| `dict/set`, a string key built per operation | 166.7 ns | **149.8** |
| `dict/get`, the same | 177.9 | **158.9** |
| `dict/get-held`, a key already built | 89.0 | 89.2 |
| `dict/set-int`, an integer key | 57.2 | 57.1 |
| `string/concat`, appending to an accumulator | 16.6 | 16.2 |
| `string/concat-fresh`, `"k" + "x"` | — | 60.9 |
| `string/concat-long`, twenty characters | — | 80.4 |

The cases that build no key are the check: a dictionary operation handed its key does not move, and
neither does the in-place append path, which was already writing into a buffer it owned. What moved
is the key construction, and building a key is about a quarter of the protocol's `dicts` section —
`i % 50`, then `str`, then `"k" +` — which is why that section goes 0.211 ms to **0.201** and its
ratio 1.69x to **1.57x** while `strings` stays where it was.

Two cases were added for the fresh concatenation, one on each side of the handle's limit, because
what it costs is not the same either side of that line.

### A comprehension declared its variable on every pass

A comprehension's clause is a loop with a scope of its own, and it did two things per
element that cost more than the element did.

**It declared the clause variable on every pass**, and treated the refusal as the signal to
assign:

```rust
if object::define(scope, &clause.name, value.clone(), false).is_err() {
    object::assign(scope, &clause.name, value)?;
}
```

`define` refuses a name the scope already holds, and the refusal is a diagnostic — a
`format!`-ed `String`. From the second pass on the clause variable *is* already there, so the
loop took that path deliberately, built a string saying so, discarded it, and assigned. A
comprehension over a thousand elements built a thousand of them and threw every one away.
The variable is now declared once and written in place after that, which is what the
duplicate path was already doing.

**And the clause loop itself was interpreted.** A comprehension whose clause variable nothing
deferred reaches is now lowered to the instructions the `for ... in` statement already uses:
an `IterInit` for the source, an `IterNext` per element, the clause variable stored into a
flat slot of the frame, each filter a branch back to that `IterNext`, and the element
collected into a list or dictionary the loop builds in a scratch slot of the frame. Nothing
re-enters the interpreter, so an element costs what a loop pass costs.

The compiler will not lower a comprehension whose clause variable a function, a class, or a
generator expression mentions. Those run later, out of the frame running the loop, and reach
a name through the environment — so that shape keeps `comprehension_clause`, which binds the
variable by name in the scope the construct captured. `clause_reaches_out` is the test, and it
is the capture scan itself: what that collects *is* the set of names a deferred construct
mentions. A comprehension is a construct the compiler folds into the enclosing chunk, not a
chunk of its own, so lowering it also removes a per-comprehension scope with nothing to hold:
a clause variable that is a flat slot needs no scope at all. What is left in the frame is one
scratch slot for the collection and one flat slot per clause variable.

The same per-pass declaration shape sat in `for ... in`: it declares its variable at the top of
every pass, under a name the chunk already holds as a shared `Rc<str>`, and the declaration was
rebuilding that name from the `&str` it was handed — an allocation per iteration. `DefineName`
and `DefineLocal` now bind through the shared name, with `object::define_shared` and
`object::define_at_shared`.

| Measurement | Before | After |
| --- | --- | --- |
| `comprehension/range` (micro), per element | 92–100 ns | **64 ns** |
| `lists` (protocol): a 600-element comprehension plus 1,800 loop passes | 0.155 ms | **0.112 ms** (−28%) |
| `lists` ratio against CPython | 2.21x | **1.44x** |
| `all` (protocol) | 1.975 ms | 1.914 ms |
| `loop`, `dicts`, `strings`, `methods`, `recursion` | unchanged | unchanged |

The sections holding no comprehension are unmoved inside the harness's own spread — `dicts`
0.222 against 0.227, `strings` 0.019 against 0.019, `methods` 0.057 against 0.059 — which is
the check that this did nothing outside the path it was aimed at. Two cases were added to the
micro harness so the shape is measured from here on: `comprehension/range`, which builds 2,000
elements, and `for/loop-var-captured`, a loop whose variable a closure made in the body
reaches, at about 126 ns per pass including the closure.

What is left of the 64 ns is the `IterNext` step — stepping a `range` goes through
`Value::Iterator(Rc<RefCell<..>>)`, and a `for` loop over a range pays that too — plus a
six-instruction body and the collect. An element now costs about twice what a hand-written
loop pass costs instead of seven times, and the gap that is left is the machinery a `for` loop
shares, not something a comprehension adds.

### Dispatch is cheap; the arms are not

`cargo test -p tiger-evaluator --lib -- --ignored --nocapture disassembles` prints the
instruction list a program compiles to, which is the measurement that matters here.
A counting loop runs three instructions per iteration and takes ~13 ns, which reads
like 4.5 ns of dispatch each — but fusing one instruction out of a six-instruction
list read moved it by only 0.9 ns. Dispatch is about a nanosecond; **the work in the
handler is the other three or four.** A `Value` is 16 bytes, so every read and write
of a local or a stack slot moves 16 bytes and re-tests its tag, and an index read
borrows a `RefCell` twice. That is what the remaining time is, and it is why fusing
instructions is close to exhausted as a strategy.

An `Instr` is 20 bytes, and the same measurement says shrinking it is not worth much
either: the fetch is about a nanosecond of an instruction that costs four. The two
bytes over a 16-byte instruction are the operator's name index, which could move to a
chunk-level table the same way a constant does, but the arms would not get shorter for
it — the work is in the `Value` moves, not the instruction.

Fusing also has a hard limit that is easy to miss: **a fused instruction carries one
position**, so it can only absorb instructions that do not report their own errors.
`Binary` and `StoreFlat` can be fused (the store raises nothing), and `GetIndex` and
`Binary` cannot, because `a[5] += 1` has to report an out-of-range index where the
index is written and an operator error where the operator is — and the differential
test requires the compiled engine to agree with the tree-walker on both.

The rest of the corpus's cost is spread across built-in calls at ~60 ns each, a global
lookup at ~13 ns of that, and a parse that is about a fifth of a whole corpus pass and
**46–99% of a single light program**. The lexer is about half of that parse, which is
why it no longer builds a `String` for a punctuation token: a punctuation mark's kind
already *is* its spelling, so `Token::spelling` reads it back from the kind, and only a
token with text of its own — an identifier, a number, a string — carries one. The
identifier scan tests ASCII first and consults the Unicode tables only for a character
outside ASCII, a string literal is built in one allocation rather than two, and the
token list is sized once instead of grown by doubling.

The other half of the parse is the tree builder, and what it spent its time on was
copying tokens. A parser only ever moves forward, so it *drains* its token list
rather than indexing it: `take` hands the token over — text and all — instead of
cloning it and leaving the copy in the list. That removes a `String` allocation per
identifier, number, and string in the source, and it is why the parser has no
`pos` field and no placeholder to leave behind. Two smaller copies went with it: an
f-string field's tokens are moved into the nested parser rather than copied, and a
member name asks `lexer::is_keyword` — a `match` — instead of scanning the
thirty-six-entry keyword table.

Together those cut the parse by about 28% and a whole corpus pass by about 7%. What
is left is mostly the AST itself: every node is a boxed enum carrying a `Base`, so
the remaining cost is the allocations that shape implies.

### A modulo is not worth an integer path

`%` on two numbers is the operator the benchmarks use most inside a loop — `i * 3 % 7`,
`i * i % 97`, `i % 50` — and on `f64` it reaches the platform's `fmod`. An integral operand pair is
the case that looks worth pulling out of it: `f64` holds every integer below 2^53 exactly, so an
integer remainder would be exact, and it needs no scaling loop. It was implemented, and it is
**slower**:

| micro case | `fmod` | integer remainder |
| --- | --- | --- |
| `operator/mod-large`: `s * 31 % 1000003` | **15.1 ns** above the control | 17.1 |
| `operator/mod-small`: `s % 251`, `s < 251` | **8.3** | 13.0 |

Both shapes, three repeats each, read as a difference against `loop/count` in the same run — the
control, not an absolute, because the two builds were measured in machine states that differed by
17% on it. The direction held in every repeat of both shapes.

The reason is two-sided. `fmod` is not the long function it looks like: it returns its dividend
immediately when that is *smaller* than the divisor, which is precisely what the second case
measures, and for a larger dividend it scales rather than divides. And what replaced it is an `i64`
division, whose latency on this machine is worse than either. Both cases stay in the harness, one
per shape, because a modulo's cost depends on its operands and a change to the operator has to be
read against both.

What came out of the attempt and is kept is the *refactor*: the compiled engine's inlined
operators, the interpreter's numeric fast path, and its general arithmetic arm held three copies of
this expression, which is a place for two engines to drift, and they now call one `modulo`. The
tests that came with it pin semantics rather than cost — including the case the measurement
uncovered, below.

### A zero remainder takes the divisor's sign

`print(-10 % 5)` printed `-0`. `fmod` gives a zero remainder the *dividend's* sign and the
correction around it only fires for a non-zero remainder, so the sign of that zero came from the
wrong operand — where the language documents that the sign follows the divisor. Python's float
modulo is the rule:

| | `-10 % 5` | `-0.0 % 5` | `-0.0 % -5` |
| --- | --- | --- | --- |
| Python `f64` | `0.0` | `0.0` | `-0.0` |
| Tiger, before | `-0` | `-0` | `-0` |
| Tiger, now | `0` | `0` | `-0` |

A zero remainder now takes the divisor's sign, which is where "the sign follows the divisor" lands
at zero. Nothing in the corpus, the documentation, or the examples prints such a value, so no
expectation moved: `tests/operators.rs` and the both-engines table in
`tests/compiled_paths.rs` pin all three columns.

### A method call builds no callable

`obj.method(...)` compiled to a property read and then a call. The read resolved the member
and returned what a read returns — a `BoundMethod` for an instance method, a `Builtin` for a
string's or a collection's own method — and the call then invoked that object. So every
method call built a value nothing else could see: `Rc::new` of a `BoundMethod` per instance
method, and for the built-in methods an `Rc<Builtin>` with a boxed closure capturing the
receiver. On a string it could not even be amortized: `cached_property` deliberately never
remembers a string's member, because a program builds most strings fresh, which made a
string method the one shape that allocated a callable on *every* access.

`CallMethod` is one instruction in place of those two. It leaves the receiver where the
call's callee would have been and resolves the member itself:

- a **class method** is found in the class table and invoked with the receiver, so no
  `BoundMethod` is built — and the lookup is remembered against the *class*, so a loop over
  a list of instances hits on every pass where the receiver-keyed slot `cached_property`
  keeps could only ever hit for a repeated receiver;
- a **string's own method** is dispatched by name, so no `Builtin` is built. The body is
  shared with the one a property read builds, so the two run the same code;
- a **length spelling** — `size`, `length` — is answered outright, for exactly the receivers a
  read would answer it for;
- anything else — a list's method, a module's, a field holding a callable, `super` — resolves
  the member exactly as `GetProperty` did and calls the value it found.

Three things had to hold for that to be the same language:

*Two positions.* Resolving a member reports at the member's own column and a call that cannot
be made reports at the call's: `[1].unknown()` reports at `unknown` and `"x".length(1)`
reports at `(`. One instruction has one position, so the member name's is kept beside it, in
`Chunk::members`, and the instruction's own carries the call's.

*The order.* A property read resolves the member *before* the arguments are evaluated and a
`CallMethod` resolves it *after*, because by then the arguments are already on the stack. An
argument that could raise would therefore raise in the wrong order — `9.size(nope)` has to
report `size does not accept number` and never evaluate `nope` — so the compiler emits the
fused instruction only when every argument is a literal or a read of a frame-local. Those are
the two shapes that can neither fail nor run anything. Everything else keeps the property
read and the call.

*The field.* A field shadows a method of the same name, so the method path looks for a field
first, through the same helper a read uses — which is also where a private field is refused.

| micro case | before | after |
| --- | --- | --- |
| `call/str-method`, `s += len(w.upper())` | 138.5 ns | **83.2** |
| `call/method-index`, the same through `words[0]` | 149.6 | **83.9** |
| `call/method`, a class method on one instance | 94.3 | **89.3** |
| `recursion/fib20` (control) | 60.1 | 60.1 |
| `operator/mod-small` (control) | 21.6 | 21.6 |
| `call/builtin` (control) | 39.9 | 44.6 |
| `call/alias` (control) | 38.7 | 41.4 |

Back to back in one directory, best of 15. The two cases that share nothing with this change
are identical to a tenth of a nanosecond, and the two that also share nothing with it moved by
the 7–12% this harness's noise floor is — the spread the note above records. The method cases
moved by 6% to 44%, both string shapes by about 40%.

The protocol's `methods` section — four string-method calls per round — goes from **2.00x to
1.44–1.57x** against CPython. Its `dicts` and `recursion` sections are unmoved by this, which is
the check that it changed the method call and nothing else.

`all` does not move, and that is arithmetic rather than a missing win: the section is 94% of
its own time inside `recursion` (1.64 ms of 1.71), and a plain function call takes the `Call`
path, which this does not touch. What is left of `methods` is the index read, two `len` calls,
and the call the method sits on; what is left of `dicts` is the membership probe and the
read-modify-write, which is the next thing to fuse.

### A small integer becomes a string without a buffer

`str(n)` is how a counting loop builds a dictionary key — `"k" + str(i % 50)` — and it went
through `object::format`, which renders into a `String` and then copies that text into a
shared string. For an integer that is two passes over text that never had to reach the heap:
a key like `"k7"` fits in the handle `RcStr` keeps in its own eight bytes, and so do the
digits it is built from.

`RcStr::from_integer` writes those digits straight into a handle, for the range the handle
holds — seven digits, or a sign and six. It is the same rendering `tiger_format::number`
gives an integral value, and a `#[test]` beside it pins the two against each other on both
sides of the boundary. Everything else — a fraction, `1e21`, `2^53`, a NaN — returns `None`
and goes to the general formatter, which is the only place that knows how those print.
`str` and `ToString` both go through it, so `1.ToString()` gets it too.

The membership test beside it stopped cloning what it found as well. `key in dict` read the
entry through `Dict::get`, which clones the value for a caller that throws it away; it now
probes through `Dict::contains`, which leaves the entry it resolved remembered for the store
that follows, exactly as a read does.

| micro case | before | after |
| --- | --- | --- |
| `string/str-int`, `s += len(str(i % 50))` | 110.9 ns | **70.9** |
| `dict/set`, a string key built per operation | 148.2 | **119.3** |
| `dict/get`, the same | 173.7 | **128.9** |
| `dict/read-modify-write`, a check, a read and a store | 319.7 | **255.3** |
| `string/concat-fresh` (control: no `str`) | 62.3 | 61.0 |

The protocol's `dicts` section goes from **1.62x to 1.29–1.38x** against CPython. With
`CallMethod` switched off for the comparison it is 1.35–1.40x, so this is the whole of that
move — and `methods` is unmoved by it, at 1.89–1.90x with the fast path in and `CallMethod`
out. The two halves of the section's work are therefore accounted for separately, which is
what the switched-off run is for.

## Binding an argument moves it

A call's arguments are pushed on the operand stack, and the frame that receives them
used to *clone* each one into its parameter slot while the caller's copy stayed on the
stack until `truncate` dropped it. That is a refcount bump and a refcount drop per
argument, spent on a value that is dead the moment the call is made.

`Args` now carries `&mut [Value]`, and `bind_call` moves each argument into its slot
with `mem::replace(..., Value::Null)`, leaving a `Null` for the caller's `truncate` to
drop for nothing. Only a user function binds, so the built-in paths — which take
`&[Value]` and are allowed to look at the arguments — never see the moved state.

| micro case | before | after |
| --- | --- | --- |
| `call/empty` | 74.9 ns | **71.8 ns** |
| `call/two-args` | 85.4 ns | **78.4 ns** |
| `call/discard` | 63.6 ns | **61.0 ns** |
| `recursion/fib20` | 74.6 ns | **72.9 ns** |
| `call/builtin` | 40.5 ns | 40.4 ns |
| `call/discard-builtin` | 28.7 ns | 28.4 ns |

The per-argument cost — `call/two-args` minus `call/empty` — falls from 10.5 ns to
6.6 ns, a 37% cut, and the base drops about 3 ns as well, because the caller's copies
no longer have to be dropped. The two built-in shapes are unchanged, which is the
check that matters: they must not have started moving.

### What is left in a call

Subtracting the built-in shape from the user-function shape isolates the frame
machinery: `call/empty` minus `call/alias` is 34 ns, and the profile says calls are
8.8% of instructions but about half of the interpreter's time. That 34 ns is spread
over a dozen steps of two to five nanoseconds each — the operand-stack pool, the
scope push, the inline locals array, the `Signal::Return` hop back out through five
nested calls — and every one of them measured individually is smaller than this
machine's noise floor of roughly 2%. What is left is not a fast path; it is the
recursive shape of `run_loop` itself. The alternative, a flat frame stack, was
measured slower when it was first tried because it makes every instruction read its
state through the frame instead of a register — and that trade changed once the operand
stack was shared between frames rather than pooled per call, which is the change
[below](#a-call-stops-re-entering-the-engine).

### The activations were not the cost

The section above says the remaining 34 ns of a call is spread so thin that no single step is
worth a change, and that the next cut has to remove several at once. That was measured
directly. `Instr::Call` now runs a plain function whose parameters are all flat slots and whose
body needs no scope straight from the call instruction, skipping `invoke` (which decides
whether the call needs a scope and pools one), `run_compiled` (which takes the frame's locals
and holds the depth limit), and `bind_call` (which reads the declaration's variadic, keyword,
and default parameters only to find that none apply). Three of the five Rust activations
between the instruction and the body are gone for the most common call shape, and the arity
checks those functions make cannot fail in it: the call passes exactly one argument per
parameter, with nothing variadic and no keywords, which is the one case that is neither short
nor long.

| micro case | Before | After |
| --- | --- | --- |
| `call/empty` | 74.3 ns | **73.0 ns** |
| `call/two-args` | 81.0 | **77.2** |
| `call/discard` | 62.7 | **59.8** |
| `recursion/fib20` | 71.1 | **69.6** |
| `call/method` | 92.2 | 93.9 (unmoved — a bound method does not take the path) |

Two to five percent, which settles the question: the activations were not where the time was,
and neither is the operand-stack pool, which is warm by the second call at any given depth. What
is left in a call — about 55 ns against a 13 ns loop pass, for a body of two instructions — is
the *frame*: the locals array, the state `run_loop` builds for itself, and the return hop. A
change that removes Rust frames from between calls does not touch any of it; only a change that
stops building a frame per call does, which is the arena the section above calls "the design".
The fast path is cheap and stays, but it is not the road to a faster call — what is, and
what it cost to get it, is the change below.

### A call stops re-entering the engine

`Instr::Call` used to call `run_frame`, which took a value stack from a pool and called
`run_loop` again. Every plain call paid a Rust activation, a pool round trip, and the
state a fresh loop entry builds, and the return crossed back out through the same
activations. Nothing in the sections above had moved any of it: the fast path had taken
3 ns off the prologue, and the remaining 34 ns of a call — the subtraction in "What is
left in a call" — had been read as out of reach.

A call now parks the running frame inside the loop and installs the callee's state in
the loop's own variables.

- **A frame is data, not an activation.** `Parked` holds the chunk, the program
  counter, the operand base, the two marks, the discarded-result flag, the flat locals,
  and the environment, and nothing else.
- **The instruction path never reads a frame.** The chunk, the program counter, the
  value stack, and the locals stay variables of `run_loop`, so dispatch is unchanged
  and only entering and leaving a call touches `Parked`. That is the difference between
  this and the attempt recorded under "Two changes that look the obvious next step":
  parking that state cost about the same then, but every instruction paid for it.
- **One value stack serves every frame.** A frame's operands start at `base` and a
  return truncates back to it, so a call no longer takes a `Vec` from `stack_pool` and
  hands it back — which is what leaves the state a frame parks small enough to be
  cheaper than the frames it replaced.
- **A frame unwinds its own scopes and iterators.** Each frame records how tall the two
  stacks were when it was entered, so a return releases exactly what it pushed, and a
  `for` loop's iterator still belongs to the frame that opened it.
- **The depth a call counts is put back, not leaked.** An error abandons the frames
  below it rather than returning through them, so `run_frame` restores the depth it
  found: one caught error cannot raise the recursion limit for the rest of the run.
- **The `Pop` a discarded call fused away is still stepped over**, which is what keeps
  `f(x);` from leaving a value the next instruction would read.

A method call, and a call that passes keywords, are entered the same way. Their frame
needs a scope of its own for `this` and for bindings a keyword can move, so the two
shapes share one preparation step with `invoke`: `prepare_frame` checks the arity, takes
the scope, and binds `this`, `super`, the parameters, the variadics, the keywords, and
the defaults, and hands back a frame the loop can park its own above. One implementation
serving both is what keeps a parked call and an `invoke`d one from drifting apart in
what they check or what they report.

What still re-enters is a `try` body, a `catch` body, and a `finally` — each is a chunk
of its own, run a level down — and a function the tree-walker runs, which has no chunk
to enter at all. Neither is on a call's path: a try costs one loop entry per *try*, not
per call, and no section moves by either.

Two binaries, one from each revision, run alternately, best of 25:

| micro case | before | after |
| --- | --- | --- |
| `call/empty` | 66.1 ns | **56.9 ns** |
| `call/two-args` | 73.3 | **65.4** |
| `call/discard` | 53.1 | **45.6** |
| `recursion/fib20` | 58.7 | **51.0** |
| `call/method` | 88.5 | 97.4 (up — a bound method does not take the path) |
| `call/builtin`, `call/alias` (controls) | 42.1, 40.9 | 42.5, 41.9 (unmoved) |

The controls are the check that matters: neither a built-in call nor a name read moved,
so the delta is the frame and not the build. The protocol's sections move with them,
best of 150, also alternating:

| section | before | after |
| --- | --- | --- |
| `all` | 1.610 ms | **1.503 ms** |
| `recursion` | 1.269 | **1.179** |
| `calls` | 3.231 | **3.031** |
| `loop`, `lists`, `dicts`, `strings`, `methods` | — | unmoved, within the noise of their own size |

So the largest section moves 7% and the whole workload 6%, off one change: a call no
longer pays for a frame it does not need. `fib(20)` is 21,891 calls, so the micro
case's per-call figure is 51 ns where it was 58.7 — against 24 ns for CPython on the
same workload.

The shape that carried the first step's price is now a frame the loop enters too: a class
method, a bound method, and a call that passes keywords all take their frame from
`prepare_frame`, so they get the flattening as well as the locals indirection that made
`call/method` 10% slower. That the micro case is back where it started has not been read
cleanly — the runs after this step were taken on a loaded machine, where the controls
themselves moved ±5% and `call/method`'s own spread was 20% — so the claim here stops at
what the sections say: nothing else paid for it. `all` 1.503 → 1.504 ms, `recursion`
1.179 → 1.125, `calls` 3.031 → 2.973, and the remaining shapes — a `try`'s chunks and a
function the tree-walker runs — move no section at all.

What is left of the locals indirection is the `Vec` the loop indexes rather than a slice
it keeps in two registers, which is what the next change — a frame whose locals come from
the same shared stack as its operands — is for.

### Binding an argument is one slice operation

A parked call binds its arguments slot by slot, and those slots are exactly `0..n`: a
flat callable's frame is seeded with its parameters first, so nothing can take a slot
below them. The loop that did it read an `Option` out of the layout, checked it, and
indexed two vectors per argument. It is now one `swap_with_slice`, which moves the
arguments into the frame and leaves `Null` where the truncate that follows discards
them anyway. The invariant it rests on is a `debug_assert`, and the language suites and
the whole corpus run with debug assertions on, so a call shape that broke it would fail
there rather than quietly bind the wrong slot.

A frame's return took the same treatment. `release_scopes_to` and `iters.truncate` now
sit behind length tests, because a body with no block and no `for` pushed nothing for
them to unwind, and both calls were two loads and a loop guard to discover that.

| micro case | before | after |
| --- | --- | --- |
| `call/two-args` − `call/empty`: the marginal argument | 9.3, 9.3, 8.3 ns | **6.2, 6.8, 6.6 ns** |
| `call/discard` | 46.6 | **44.8** |
| `call/empty` | 58.1 | **57.3** |
| `recursion/fib20` | 49.5 | **48.3** |
| controls: `call/builtin`, `call/alias`, `loop/count` | 44.0, 42.7, 14.7 | 41.4, 41.3, 13.9 |

Two binaries, one from each revision, run alternately, best of 25, three rounds, each
round's three measurements shown in order. The controls are the check that matters, and
here they moved by 2-6%: the whole-call rows are therefore not separable from this
machine's build-to-build noise. What the change is worth is the difference between two
cases *in the same binary* — the marginal argument — and that is 9 ns down to 6.5, and
it held in every round of both shapes.

The protocol's sections are where a call-dominated program is measured. One binary per
revision, alternating, 200 iterations, best of three: `calls` 2.853 → 2.777 ms,
`recursion` 1.088 → 1.053, `all` 1.415 → 1.372 — about 3% — while every section the
change does not touch (`loop`, `lists`, `dicts`, `methods`, `strings`) holds to the
digit. Against CPython that is 1.22x → 1.19x on `calls` and 2.02x → 1.98x on
`recursion` — the ratio those same two rounds reported. The deficit these sections
carry is not in the binding: it is the frame, the dispatch, and the call protocol,
which is what the sections above left.

## A frame's slots are the bottom of its own stack

Where a call's locals used to be a `Vec` the engine took from a pool and handed back —
moved in and out of the parked frame, filled with `Null` on its way out — a frame's slots
are now the bottom of its region on the one shared value stack, with its operands above
them. A call is entered at the first argument it pushed and left by dropping back to the
slot below its own. Three things follow, and every call takes all three:

- **A flat call copies no arguments.** A flat callable's parameters own the frame's first
  slots and a call of it passes one argument per parameter, so the arguments are already
  sitting in the slots they belong to. What is left is a `resize` for the slots above the
  parameters — and that is usually none of them: `fib` declares one local, its parameter,
  so its frame is entered by writing the parked frame's fields and nothing else.
- **No vector leaves the loop and none returns to it.** `Parked` records where the
  caller's slots start, a `usize` where it used to hold a `Vec`, and a return takes the
  callee's slots off the stack along with its operands. The pool round trip, the fill, and
  the two moves into and out of the frame are gone.
- **A frame that is prepared rather than flat** — a method, a call that passes keywords, a
  body that needs a scope of its own — still binds its arguments into a vector first, and
  that vector's values are moved onto the stack where the receiver and arguments were. It
  is the one shape that still pays a copy, and its vector goes back to the pool empty.

The protocol's `recursion` section is where this reads, because `fib(20)` is 21,891 calls
of a function that declares exactly one local. Mean of five runs, Tiger work against
CPython's `p.exec`; the CPython column had not moved:

| section | before | after | against CPython |
| --- | --- | --- | --- |
| `recursion` | 1.012 ms | **0.888 ms** | 1.91x → **1.66x** |
| `calls` | 2.726 | **2.425** | 1.13x → **0.98x** |
| `methods` | 0.032 | **0.030** | 1.64x → **1.53x** |
| `strings` | 0.008 | **0.007** | 0.74x → **0.67x** |
| `all` | 1.331 | **1.219** | 1.67x → **1.52x** |
| `loop`, `lists`, `dicts` | — | within their own noise (1.14–1.46x on this host) | — |

Two binaries, one per revision, best of 60, single-threaded, same machine:

| micro case | before | after |
| --- | --- | --- |
| `recursion/fib20` | 53.1 ns | **41.2** |
| `call/empty` | 63.7 | **50.1** |
| `call/two-args` | 64.5 | **55.4** |
| `call/discard` | 43.6 | **36.8** |
| `call/method` | 100.2 | 100.7 (a prepared frame pays the one copy it always did) |
| `call/builtin`, `call/alias` (controls) | 40.2, 36.6 | 43.7, 39.3 |
| `loop/count`, `list/read`, `dict/set` | 10.8, 25.1, 118.8 | 10.5, 25.8, 115.0 |

The controls are the check that matters: a built-in call enters no frame at all, and this
change did not touch it — the two rows moved by the 7–12% this harness's own spread is,
which is why a row has to be stable before a delta is read off it.

That a call's *slots* were not the only cost is now the clearer measurement. `fib` executes
about seven instructions per invocation and this machine dispatches one for ~3.6 ns — the
figure `loop/count` gives when its three fused instructions are divided out of its 10.5 ns.
So of the ~41 ns a recursive call now costs, roughly 25 ns is the instruction loop reading
its own bytecode, and the frame is the other half. `recursion` against CPython needs both
halves moved:

```tg
function fib(n) {
    if n < 2 {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}
print(fib(10));
```

```text
55
```

### A return is one dispatch too

The last instruction of a body is one of two shapes — `LoadFlat(slot)` and `Return`, or a
`Binary` and `Return` — and both end the frame. `ReturnFused` is that pair in one
dispatch: the slot form hands the frame's own slot back, the operator form computes it
from the two operands below, and either way the frame's exit happens in the same arm
rather than a dispatch later.

It is the fusion the measurements above say should pay: nothing about it is general, and
what it removes is real work — a dispatch, the push of the value the `LoadFlat` or the
operator produced, and the pop the `Return` would have taken it back with. What it does
*not* do is read a descriptor that says what to compute; see
[the fused call site](#two-changes-that-look-like-the-obvious-next-step-were-implemented-and-measured-slower)
for what happens when a fused instruction does that instead.

The instruction counts are the reliable half of this measurement, because a loaded
machine moves a time without moving a count. The paired workload executes **164,326
instructions for `all` before and 142,435 after** — 13% fewer — and its `recursion`
section, which is where a body's return is the whole of the work, **131,449 → 109,558**,
which is six instructions per `fib` invocation down to five.

## Getting every protocol section under CPython

`make protocol` asserts that Tiger's work is no more than CPython's in every section. At the
start of this round `recursion` (1.50x), `dicts` (1.36x), `methods` (1.44x), `loop` and `lists`
were over the line. Every change below was measured one at a time, on a micro case and then
on the protocol table, and only the ones that moved a number were kept. The light sections
spread about ±30% from run to run, so a section was judged on at least ten runs.

### Where the time was

A sampling profiler (`SuspendThread` and `GetThreadContext` on the interpreter thread,
symbolized with `llvm-symbolizer`, counting only samples inside `execute_compiled`) said
what the instruction counts could not:

- **Pushes stalled on store forwarding.** `Vec::push` tests the capacity after its argument
  exists, so the compiler kept a half-built `Value` — a tag byte and a payload, stored
  separately — in a stack temporary across the call to grow, then copied it into the vector
  with one 16-byte load. A load that straddles two smaller stores cannot be forwarded from
  them, so the push waited for the stores to reach the cache. About a third of a recursive
  call's samples sat on those copies.
- **Name reads went through an `Option<Value>`**, the same shape, on every `LoadName` hit.
- **Modules nobody read were built.** A run built `random`, `secrets` and `sys` — including a
  copy of the process environment — before its first instruction, and the sections that run
  in tens of microseconds paid for it every time.

### What was kept

| change | measured |
| --- | --- |
| `room()` makes capacity first, `put()` writes unchecked, `copy_of()` copies plain data bitwise, in `LoadFlat`, `Constant`, `LoadName`, `FlatBinaryConstantNumber` and the call-result tails | `recursion/fib20` 35.9 → 24.7 ns (CPython about 24 ns) |
| a name-read hit pushes the remembered value and continues; the cache holds a `Weak` scope | with the line above, `recursion` 1.50x → about 1.0x |
| a frame parked for a callee that shares the caller's chunk or scope stores `None` for them | a count up and down per recursive call |
| a flat or built-in callee stays in its stack slot and the return's `truncate` drops it | moving it out wrote a `Null` built as one byte and read back as sixteen |
| `Parked` keeps its five indices as `u32` | 72 → 48 bytes written per call; `recursion` 1.01x → **0.95x** paired, nothing else in the table moved |
| clearing an empty scope no longer bumps the binding epoch; a pooled scope reused under a different parent calls `invalidate_name_reads()` | the name cache stopped being flushed by every call that returned |
| the capture scan skips comprehensions that stay in the chunk | `SIZE` keeps its flat slot, so the `lists` loops need no name lookup |
| `random`, `secrets` and `sys` are built on the first member read, and `sys.env` only when `env` is named | allocations per run 92 → 48 |
| the built-ins are one thread-local scope indexed once, with a run's own modules in a scope in front | the per-run index rebuild is gone |
| `upper` and `lower` convert ASCII in place | `methods` 1.44x → 1.28x → 1.11x, then 1.00x with the lazy modules |
| `BinaryStoreNumber` reads its operands in place; `StoreIndexFlat` fuses `items[i] = value` | `list/write` moved about 2% — borderline, kept for the section, worth re-checking |


The paired table at the end, best of 12 runs, Tiger work against CPython's `p.exec`:

| section | start | now |
| --- | --- | --- |
| `all` | over | **0.94x** |
| `loop` | 1.26–1.64x | **0.76x** |
| `lists` | 1.04–1.5x | **0.88x** |
| `dicts` | 1.36x | **0.91x** |
| `strings` | under | **0.40x** |
| `methods` | 1.44x | **1.00x** |
| `recursion` | 1.50x | **0.95x** |
| `calls` | 0.95x | **0.72x** |

`make protocol` passed in 11 of 14 asserted runs. Each of the three failures was a light
section over the line on that run: `lists` at 1.06x and at 1.28x, and `loop` at 1.06x
together with `methods` at 1.01x. Those sections take 18–60 microseconds and one run's
minimum moves with the host. `methods` is the section with no margin: 0.018 ms of Tiger work
against 0.018 ms of CPython's, and the profile no longer has one dominant cost left to
remove. What remains is `CallMethod` dispatch, and about 15% of its samples inside `ntdll`.
The 0.95x aim was reached for `recursion`, `dicts`, `lists`, `loop` and `calls`, and not for
`methods`. So the line holds on most runs but not on every one, on this host.

### What was tried and not kept

- **`CallName` for a callee that is not adjacent to its arguments.** It saves nothing: the
  arm is a copy of `LoadName`. Resolving the callee after the arguments instead would change
  the order of evaluation and the diagnostics.
- **Removing the depth counter and the return guards.** `fib20` got slower.
- **`frames.reserve` inline.** No change.
- **A `len` fast path (`measures_length`).** No gain; reverted.
- **`copy_of` on the `borrowed[index]` clone.** Reverted.
- **An in-place compare in `JumpUnless`.** Reverted.
- **A `u32` fast path in `modulo`.** Reverted.
- **`room`/`put` across every arithmetic and index arm.** Mixed: `put` on the call-result
  tails helped and was kept, the `GetIndex` and `FlatIndex*` conversions were reverted.

### Reading the micro harness after these

`TIGER_MICRO_CASE` now runs one case by name, and `recursion/fib20-flat-callee`,
`scope/block-local` and `scope/block-none` price a name load in a call and a block scope in
a loop. Two builds that differ only in code layout can move a case, so a micro delta has to
be confirmed on the protocol table, over at least ten runs, before it counts.


## The biggest program was not the interpreter

A per-program sweep is worth running before trusting an instruction profile. Summing
one thread-free run of every benchmark showed that the corpus is not shaped the way
the instruction counts suggest:

| program | min ms |
| --- | --- |
| `19_file_io` | 6.63 |
| `30_algo_palindromes` | 1.77 |
| `26_algo_rainwater` | 1.29 |
| `32_type_inspection` | 1.22 |
| `24_algo` | 1.16 |

A quarter of the corpus was one file-I/O program, and none of that quarter was the
interpreter: `19_file_io` moves about 23 bytes through roughly 45 host calls, and
every one of them is a real syscall. Three sessions of call-path tuning had been
aimed at the other three quarters.

Reading the file layer showed why the count was as high as it was.

- `open_file` asked `file_exists` and then `load_file`, two syscalls for one answer,
  and `append_file` did the same. `load_or_empty` now does one read and reports
  whether the file was there, asking the host a second time only on the failure
  path, where the old distinction between missing and unreadable is kept exactly.
- `close` persisted unconditionally, so closing a handle that had only *read* a file
  wrote the whole thing back. That is a syscall spent changing nothing, and it can
  clobber what another writer put there in the meantime. A `dirty` flag on `FileData`
  now records whether the handle changed the file, and `close` writes back only then.
- `flush` left that flag set, so a flush followed by a close wrote the same bytes
  twice. A flush clears it.

The subtlety is which opens count as dirty. A truncating mode (`"w"`, `"w+"`) has
already changed the file, so its close must land that truncation even if nothing was
written to it, and a creating mode that found no file must bring it into being. Every
other open leaves the file as it found it. `tests/files.rs` pins all four cases,
including the truncating open with no writes, which is the one that would break first.

| measurement | before | after |
| --- | --- | --- |
| `19_file_io` | 6.452 ms | **4.320 ms** (−33%) |
| `01_arithmetic` | 0.182 ms | 0.186 ms |
| `11_oop_shapes` | 0.211 ms | 0.207 ms |
| corpus, 40 programs | 26.19 ms | **24.47 ms** (−6.6%) |

The interpreter-bound programs did not move, which is the point: this was not an
interpreter change, and `web/tiger.wasm` is byte-for-byte the same size afterwards.
What is left in `19_file_io` is the 25-odd syscalls the program actually asks for.

```sh
# One program, on either engine, without the harness's thread.
TIGER_BENCH_FILE=benchmarks/26_algo_rainwater.tg TIGER_BENCH_ITERATIONS=200 \
  cargo test -p tiger-conformance --test timing --release -- --ignored --nocapture time_source_file
```

The build is tuned for it: `[profile.release]` sets `opt-level = 3`, `lto = "fat"`,
and `codegen-units = 1`. The last two trade a longer build for a faster binary,
and they also shrink `web/tiger.wasm`.

## Drawing it

The graph is at [`flamegraph.svg`](flamegraph.svg), and it is generated rather than
drawn: `tools/flamegraph` measures this machine and writes the file. It is not a
member of the workspace, so `cargo test` at the root never builds the profiler into
anything else.

```sh
cd tools/flamegraph
cargo run --release
```

Two panels, because the corpus and the paired workload answer different questions.
The workload's seven sections are one language feature each — a counting loop, list
indexing, dictionary insert and lookup, string building, method calls, recursion,
plain calls — so a frame there is one feature's cost. The corpus is what real
programs look like, and it is where the parse shows up as a share of a *run* rather
than of a program.

The tree under a program is built from two kinds of number, and each frame says
which kind it is when the pointer rests on it.

**Measured.** A unit's width is its whole run — `run_with_options`, which is what a
host pays — and its children are the floor (an evaluator built, `Evaluator::new`,
about five microseconds and the same for every program), the parse and the lowering,
and the execute. The parse and the execute are each measured on their own and then
composed: a minimum of a part is not a part of a minimum, so the two separate minima
do not add back up to the whole run, and what the frames are is the measured whole
run split by the ratio those two measurements found, with the execute never given
less than its own measurement. Inside execute, every native call — a built-in, or a
string method — is timed as it runs and charged to the call's own frame, one call in
sixty-four timed and scaled back up; what is left after them is the instruction
loop's own time, exactly.

**Counted.** The instruction loop's *mix* — how many times each of the eighty-five
instruction shapes ran — with the counting in
`crates/tiger-evaluator/src/profile.rs` behind its `profile` feature, off by default
so a release build has no trace of it:

```sh
cargo test -p tiger-evaluator --features profile --test profile -- --ignored --nocapture
```

**There is no frame per instruction shape, and that is a measurement rather than an
omission.** Three ways of pricing one shape against another were tried on this
machine, and all three failed measurably:

- **Sampling the running instruction.** The host clock resolves to a hundred
  nanoseconds and a read costs twenty-five, so the clock can be read at best every
  few hundred instructions. An interval of a few hundred instructions charged to
  whichever instruction the sample landed on says nothing about which instruction
  inside it was expensive: in a loop of eight instructions every one of them came out
  at an eighth of the time. That is the count mix again, wearing a clock's clothes.
- **Timing every instruction with the cycle counter.** `rdtsc` costs about fifteen
  times what an instruction does, and on an out-of-order core the pipeline overlaps
  the work around it. Measured on the loop above, the eight shapes came out between
  36 and 43 nanoseconds each — a call included — where the true spread is an order of
  magnitude. What was measured was the counter's serialization, not the instruction.
  `tools/flamegraph/src/bin/clock.rs` is the probe that established the resolution and
  the cost of each clock (`cargo run --release --bin clock`).
- **Solving for one cost per shape.** Counts times costs equals time, so a few dozen
  programs with different mixes are a few dozen equations, and a non-negative
  least-squares solve gives the costs. It leaves a third of every program
  unexplained (33% mean error): the engine *specializes* sites as it runs, so one
  shape is not one cost — a `Binary` that has settled onto numbers is a different
  instruction from the one that has not, and `Call` covers a built-in, a flat user
  function, and a bound method.

What that leaves is the two levels above, both fully measured, and the mix as counts.
For the corpus that is enough to read the shape of the thing: parsing and lowering is
5.3 ms of the 21.1 ms the forty programs and seven sections take, one program
(`44_comprehensions`) is 23% of it, and the heaviest single native in the whole
measurement is `str` at 0.81 ms. `make timing`, `make compare`, `make protocol`, and
the micro harness are the measurements to act on; the graph is what says where to
point them.

## Adding to the corpus

A performance change needs evidence, not an assertion. Add a program to
`benchmarks/python/workload.tg` and its Python translation, keep the two printing
the same checksum, and run `compare.py` before and after. When a change affects
the compiled engine, add a case to `crates/tiger-evaluator/tests/compiled_paths.rs`
that runs on both engines and requires them to agree.

[Guide index](README.md) | [Project README](../README.md)
