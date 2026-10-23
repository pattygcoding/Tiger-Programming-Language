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

## Measuring

Two harnesses report timings, and neither is part of `cargo test`:

```sh
# A per-operation cost for the interpreter's hot paths, in process.
cargo test -p tiger-evaluator --test micro --release -- --ignored --nocapture

# Every benchmark program, in process, with no process startup in the numbers.
cargo test -p tiger-conformance --test timing --release -- --ignored --nocapture

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

## Where the time goes

The regression programs are small — under two milliseconds each in process — so
the number that matters for a single run is usually process startup, which is
where Tiger is far ahead of a CPython process. Within the interpreter, the
per-operation costs the micro harness reports are roughly:

| Operation | Cost |
| --- | --- |
| A loop iteration, counting | ~13 ns |
| A list read or write through a flat index | ~16–29 ns |
| A field read or write | ~26–31 ns |
| A built-in call | ~48 ns |
| A function call | ~80–100 ns |
| A dictionary insert or lookup | ~210–220 ns |

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
  Rust frames a call re-enters through. Parking a frame's state costs about what the
  frames it replaced did — the state is a `Chunk`, an environment, and three vectors
  — and the larger loop body made every nested entry more expensive. It cost 15% on
  `call/method` and gained nothing on `call/empty`.
- **`Value::String(Rc<String>)` with an in-place append**, so that `s += x` in a loop
  grows the accumulator instead of copying it. A growable inner type costs one more
  allocation than the `Rc<str>` it replaces, on every string the program builds, and
  the append does not even reach the accumulator through the fused store
  instructions, which borrow their operands. It cost 7% on dictionary operations and
  18% on the corpus's heaviest program. That measurement was also incomplete — it
  thinned `String` but left `Tuple` a fat pointer, so `Value` never actually shrank;
  see [a smaller `Value`](#a-smaller-value-needs-a-thinner-string) for the version
  that does shrink it.

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

Over the 39 timed benchmark programs — 351,563 instructions — the mix is:

| Category | Share | Cost each | Weighted |
| --- | --- | --- | --- |
| A flat local or a constant | 20.8% | ~5 ns | 1.0 |
| A name that walks the environment | 8.5% | ~6 | 0.5 |
| An index read | 5.1% | ~10 | 0.5 |
| A property read | 3.1% | ~10 | 0.3 |
| A flat store | 10.0% | ~6 | 0.6 |
| An environment store | 3.7% | ~15 | 0.6 |
| An operator | 12.1% | ~6 | 0.7 |
| A branch | 13.9% | ~5 | 0.7 |
| **A call** | **8.8%** | **~64** | **5.6** |
| Control, collections, and the rest | 13.9% | ~8 | 1.1 |

A call is one instruction in eleven and nearly half the time. Everything a call costs —
resolving the name, the operand-stack round trip for the callee and its arguments, the
dispatch, the scope, the frame, and the return — is charged to that one category, and it
is ~64 ns against ~5 ns for the load beside it. That is what "most of the time is not
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

### Dispatch is cheap; the arms are not

`cargo test -p tiger-evaluator --lib -- --ignored --nocapture disassembles` prints the
instruction list a program compiles to, which is the measurement that matters here.
A counting loop runs three instructions per iteration and takes ~13 ns, which reads
like 4.5 ns of dispatch each — but fusing one instruction out of a six-instruction
list read moved it by only 0.9 ns. Dispatch is about a nanosecond; **the work in the
handler is the other three or four.** A `Value` is 24 bytes, so every read and write
of a local or a stack slot moves 24 bytes and re-tests its tag, and an index read
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
measured slower when it was tried (session 2) because it makes every instruction read
its state through the frame instead of a register. The call path stays where it is
until something changes that trade.

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

## Adding to the corpus

A performance change needs evidence, not an assertion. Add a program to
`benchmarks/python/workload.tg` and its Python translation, keep the two printing
the same checksum, and run `compare.py` before and after. When a change affects
the compiled engine, add a case to `crates/tiger-evaluator/tests/compiled_paths.rs`
that runs on both engines and requires them to agree.

[Guide index](README.md) | [Project README](../README.md)
