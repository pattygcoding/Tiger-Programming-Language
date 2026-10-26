//! A dynamic count of what the instruction loop executes.
//!
//! The micro harness prices each operation and the disassembly shows a program's static
//! shape, but neither says how *often* a program runs each of them: a corpus program can
//! spend its time in a loop body that appears once in its listing. This counts what the
//! loop actually executes, by category, so the two can be multiplied together.
//!
//! It is only compiled with `--features profile`, so a release build has no trace of it:
//!
//! ```sh
//! cargo test -p tiger-evaluator --features profile --test profile -- --ignored --nocapture
//! ```
//!
//! The counters are atomic rather than thread-local because the harness runs a program on
//! a thread of its own, and the cost of counting is not the point — the counts are.
//!
//! # Where the time goes, as well as how often
//!
//! A count says how often an instruction runs, not what it costs, and the two are not
//! proportional: a call is one instruction in fifteen and about two fifths of the time.
//! Three ways of getting from a count to a cost were tried here, and only the last works
//! on a machine like this one.
//!
//! **Sampling the instruction that is running.** The host clock's resolution is a hundred
//! nanoseconds and a read costs twenty-five, so the clock can only be read every few hundred
//! instructions; an interval of a few hundred instructions charged to whichever instruction
//! the sample landed on says nothing about which instruction inside it was expensive. In a
//! loop of eight instructions every one of them ends up with an eighth of the time, whatever
//! it costs. That is a count mix wearing a clock's clothes.
//!
//! **Timing every instruction with the cycle counter.** `rdtsc` costs about fifteen times
//! what an instruction does, and on an out-of-order core the pipeline overlaps the work
//! around it: every instruction then reads within ten nanoseconds of every other, a call
//! included, because what is being measured is the counter's serialization and not the
//! instruction. Measured, and rejected: a loop of eight instructions reported eight costs
//! between 36 and 43 nanoseconds where the true spread is an order of magnitude.
//!
//! **What is left is the counts, and costs solved from them.** This module reports how many
//! times each shape ran — exact, and free of any clock — and the tool that draws the graph
//! solves for what each shape costs by fitting those counts against the times it measured
//! with the profiler switched off. Many programs with very different mixes, one unknown cost
//! per shape, and a least-squares solution: the call comes out an order of magnitude above a
//! flat load, and the fit reproduces the measured totals. Time not spent in the loop is
//! measured the other way, by [`sampled`], which times every native call and charges it to
//! the native's own frame.
//!
//! Everything expensive is behind [`ENABLED`], which [`enable`] turns on, so a timed run and
//! a counted run are the same program with one relaxed load per instruction between them.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use tiger_object::bytecode::Instr;

/// How many categories an instruction is sorted into.
pub const CATEGORIES: usize = 13;

/// What each category is, in the order [`category`] numbers them.
pub const NAMES: [&str; CATEGORIES] = [
    "flatload",  // a local, a constant
    "nameload",  // a name that walks the environment
    "index",     // reading through an index
    "property",  // reading a member
    "flatstore", // a local, including the fused operator-and-store forms
    "envstore",  // a name that walks the environment
    "setstore",  // writing through an index or a member
    "binary",    // an operator
    "jump",      // a branch
    "call",      // a call
    "make",      // making something callable
    "collect",   // building or slicing a collection
    "control",   // scope, iteration, and leaving
];

static COUNTS: [AtomicU64; CATEGORIES] = [const { AtomicU64::new(0) }; CATEGORIES];

/// Whether the counters are being taken. Off until [`enable`] says otherwise, so a run that
/// is being timed pays one relaxed load per instruction and nothing else.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// One native call in this many is timed, and the measurement scaled back up.
pub const NATIVE_ONE_IN: u64 = 64;

/// How many instructions have run since the last [`reset`].
static TICKS: AtomicU64 = AtomicU64::new(0);

/// How many native calls have been offered to [`sampled`] since the last [`reset`].
static NATIVE_TICKS: AtomicU64 = AtomicU64::new(0);

/// One entry per timed native call: what was called, and its time scaled up to the calls
/// the sampling stands for.
static NATIVES: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());

/// Starts or stops the counting.
///
/// A run that is being *timed* has the counters off, because two atomic adds on every
/// instruction is a fifth of what a flat read costs; a run that is being *counted* has them
/// on and does not care what it costs.
pub fn enable(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Counts one executed instruction, when the counters are on.
#[inline]
pub fn record(instruction: &Instr) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    COUNTS[category(instruction)].fetch_add(1, Ordering::Relaxed);
    SHAPE_COUNTS[shape_index(instruction)].fetch_add(1, Ordering::Relaxed);
    TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Runs a native call, timing one in [`NATIVE_ONE_IN`] of them.
///
/// The time is multiplied back up, so a native's entry is an estimate of the calls the
/// sample stands for rather than a reading of one call. The samples are taken whether or not
/// the counters are on, because a native's time is not something a count can reconstruct: the
/// two atomic adds a sampled call costs are nothing beside a call that costs forty
/// nanoseconds, and the unsampled sixty-three pay one relaxed load each.
#[inline]
pub fn sampled<R>(name: &str, body: impl FnOnce() -> R) -> R {
    if !ENABLED.load(Ordering::Relaxed) {
        return body();
    }
    let timed = NATIVE_TICKS.fetch_add(1, Ordering::Relaxed) % NATIVE_ONE_IN == 0;
    let start = Instant::now();
    let result = body();
    if timed {
        let spent = start.elapsed().as_nanos() as u64;
        if let Ok(mut natives) = NATIVES.lock() {
            natives.push((name.to_string(), spent.saturating_mul(NATIVE_ONE_IN)));
        }
    }
    result
}

/// One instruction shape's measurement: what it is, and how often it ran.
pub struct ShapeCount {
    pub name: &'static str,
    pub count: u64,
}

/// Every shape that ran, heaviest first.
pub fn shape_costs() -> Vec<ShapeCount> {
    let mut costs: Vec<ShapeCount> = SHAPE_NAMES
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            let count = SHAPE_COUNTS[index].load(Ordering::Relaxed);
            if count == 0 {
                return None;
            }
            Some(ShapeCount { name, count })
        })
        .collect();
    costs.sort_by(|left, right| right.count.cmp(&left.count).then_with(|| left.name.cmp(right.name)));
    costs
}

/// The native calls timed so far, each scaled up by [`NATIVE_ONE_IN`].
pub fn native_samples() -> Vec<(String, u64)> {
    NATIVES.lock().map(|natives| natives.clone()).unwrap_or_default()
}

/// The counts so far, in [`NAMES`] order.
pub fn counts() -> [u64; CATEGORIES] {
    let mut counts = [0; CATEGORIES];
    for (slot, count) in counts.iter_mut().zip(COUNTS.iter()) {
        *slot = count.load(Ordering::Relaxed);
    }
    counts
}

/// How many instructions have been run since the last [`reset`].
pub fn instructions() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Clears the counts and the native samples.
pub fn reset() {
    for count in &COUNTS {
        count.store(0, Ordering::Relaxed);
    }
    for count in &SHAPE_COUNTS {
        count.store(0, Ordering::Relaxed);
    }
    TICKS.store(0, Ordering::Relaxed);
    NATIVE_TICKS.store(0, Ordering::Relaxed);
    if let Ok(mut natives) = NATIVES.lock() {
        natives.clear();
    }
}

/// Which category an instruction belongs to.
fn category(instruction: &Instr) -> usize {
    match instruction {
        Instr::Constant(_) | Instr::LoadLocal { .. } | Instr::LoadFlat(_) | Instr::LoadSuper(_) => 0,
        Instr::LoadName(_) => 1,
        Instr::FlatIndexFlat { .. } | Instr::FlatIndexConstant { .. } | Instr::GetIndex => 2,
        Instr::FlatProperty { .. } | Instr::GetProperty(_) | Instr::ReadPropertyTarget(_) => 3,

        Instr::StoreFlat { .. }
        | Instr::DefineFlat { .. }
        | Instr::StoreScratch(_)
        | Instr::BinaryStore { .. }
        | Instr::BinaryStoreNumber { .. }
        | Instr::FlatBinaryFlatStore { .. }
        | Instr::FlatBinaryFlatStoreNumber { .. }
        | Instr::FlatBinaryConstantStore { .. }
        | Instr::FlatBinaryConstantStoreNumber { .. }
        | Instr::FlatStoreIndex { .. }
        | Instr::StoreIndexFlat { .. }
        | Instr::UpdateFlat { .. }
        | Instr::UpdateFlatJump { .. }
        | Instr::ReadIndexTarget => 4,

        Instr::StoreLocal { .. }
        | Instr::StoreName(_)
        | Instr::DefineLocal { .. }
        | Instr::DefineName { .. }
        | Instr::UpdateLocal { .. }
        | Instr::UpdateName { .. } => 5,

        Instr::SetIndex
        | Instr::SetIndexKeep
        | Instr::UpdateIndex { .. }
        | Instr::SetProperty(_)
        | Instr::SetPropertyKeep(_)
        | Instr::UpdateProperty { .. } => 6,

        Instr::Binary { .. }
        | Instr::BinaryNumber { .. }
        | Instr::BinaryConstant { .. }
        | Instr::BinaryConstantNumber { .. }
        | Instr::FlatBinaryFlat { .. }
        | Instr::FlatBinaryFlatNumber { .. }
        | Instr::FlatBinaryConstant { .. }
        | Instr::FlatBinaryConstantNumber { .. }
        | Instr::Unary(_)
        | Instr::Equal => 7,

        Instr::Jump(_)
        | Instr::JumpIfFalse(_)
        | Instr::JumpUnless { .. }
        | Instr::JumpIfFalseOrPop(_)
        | Instr::JumpIfTrueOrPop(_)
        | Instr::FlatJumpUnlessFlat { .. }
        | Instr::FlatJumpUnlessFlatNumber { .. }
        | Instr::FlatJumpUnlessConstant { .. }
        | Instr::FlatJumpUnlessConstantNumber { .. } => 8,

        Instr::Call { .. } | Instr::CallName { .. } | Instr::CallMethod { .. } => 9,

        Instr::MakeFunction(_) | Instr::Class(_) | Instr::Import { .. } | Instr::ApplyFormat(_) | Instr::Format => 10,

        Instr::BuildList(_)
        | Instr::BuildTuple(_)
        | Instr::BuildSet(_)
        | Instr::BuildDict(_)
        | Instr::Comprehension(_)
        | Instr::Collect(_)
        | Instr::CollectPair(_)
        | Instr::Generator(_)
        | Instr::Slice(_) => 11,

        _ => 12,
    }
}

/// How many instruction shapes there are to tell apart.
pub const SHAPE_COUNT: usize = 86;

/// What each shape is, in the order [`shape_index`] numbers them.
///
/// The two lists are one list: an instruction's index is the position of its name here,
/// so a shape added to the enum has to be added to both. The names are pinned against the
/// instructions they claim to be by the `profile` test.
pub const SHAPE_NAMES: [&str; SHAPE_COUNT] = [
    "Constant",
    "LoadName",
    "LoadLocal",
    "LoadFlat",
    "LoadSuper",
    "StoreName",
    "StoreLocal",
    "StoreFlat",
    "DefineName",
    "DefineLocal",
    "DefineFlat",
    "Equal",
    "Pop",
    "Dup",
    "ReadIndexTarget",
    "ReadPropertyTarget",
    "Binary",
    "FlatBinaryFlat",
    "FlatBinaryConstant",
    "BinaryConstant",
    "BinaryNumber",
    "BinaryConstantNumber",
    "BinaryStoreNumber",
    "FlatBinaryFlatNumber",
    "FlatBinaryConstantNumber",
    "FlatJumpUnlessFlatNumber",
    "FlatJumpUnlessConstantNumber",
    "FlatBinaryFlatStoreNumber",
    "FlatBinaryConstantStoreNumber",
    "FlatProperty",
    "CallName",
    "ReturnFused",
    "Unary",
    "Jump",
    "JumpIfFalse",
    "JumpUnless",
    "FlatJumpUnlessFlat",
    "FlatJumpUnlessConstant",
    "JumpIfFalseOrPop",
    "JumpIfTrueOrPop",
    "BuildList",
    "BuildTuple",
    "BuildSet",
    "StoreScratch",
    "Collect",
    "CollectPair",
    "Comprehension",
    "Generator",
    "BuildDict",
    "MakeFunction",
    "Class",
    "Call",
    "CallMethod",
    "GetIndex",
    "UpdateFlatJump",
    "FlatBinaryFlatStore",
    "FlatBinaryConstantStore",
    "BinaryStore",
    "FlatIndexFlat",
    "FlatIndexConstant",
    "FlatStoreIndex",
    "GetProperty",
    "SetIndex",
    "SetProperty",
    "SetIndexKeep",
    "SetPropertyKeep",
    "UpdateIndex",
    "UpdateProperty",
    "UpdateName",
    "UpdateLocal",
    "UpdateFlat",
    "ApplyFormat",
    "Format",
    "Return",
    "ReturnEscape",
    "Null",
    "PushScope",
    "PopScope",
    "IterInit",
    "IterNext",
    "IterDrop",
    "Throw",
    "Import",
    "Try",
    "Slice",
    "StoreIndexFlat",
];

/// How many times each shape has run.
static SHAPE_COUNTS: [AtomicU64; SHAPE_COUNT] = [const { AtomicU64::new(0) }; SHAPE_COUNT];

/// How often each shape has run.
pub fn shape_counts() -> [u64; SHAPE_COUNT] {
    let mut counts = [0; SHAPE_COUNT];
    for (slot, count) in counts.iter_mut().zip(SHAPE_COUNTS.iter()) {
        *slot = count.load(Ordering::Relaxed);
    }
    counts
}

/// Which shape an instruction is, as a position in [`SHAPE_NAMES`].
///
/// [`category`] groups instructions that cost about the same; a cost wants the shape
/// itself, because two instructions in one category can differ by an order of magnitude —
/// `Jump` and `Call` are both "control" to a count and nothing like each other to a clock.
///
/// The match is exhaustive on purpose: a new instruction has to be given a shape here, and
/// the compiler says so. The index it returns is the name's position in [`SHAPE_NAMES`],
/// which the `profile` test holds it to.
#[inline]
pub fn shape_index(instruction: &Instr) -> usize {
    match instruction {
        Instr::Constant(_) => 0,
        Instr::LoadName(_) => 1,
        Instr::LoadLocal { .. } => 2,
        Instr::LoadFlat(_) => 3,
        Instr::LoadSuper(_) => 4,
        Instr::StoreName(_) => 5,
        Instr::StoreLocal { .. } => 6,
        Instr::StoreFlat { .. } => 7,
        Instr::DefineName { .. } => 8,
        Instr::DefineLocal { .. } => 9,
        Instr::DefineFlat { .. } => 10,
        Instr::Equal => 11,
        Instr::Pop => 12,
        Instr::Dup => 13,
        Instr::ReadIndexTarget => 14,
        Instr::ReadPropertyTarget(_) => 15,
        Instr::Binary { .. } => 16,
        Instr::FlatBinaryFlat { .. } => 17,
        Instr::FlatBinaryConstant { .. } => 18,
        Instr::BinaryConstant { .. } => 19,
        Instr::BinaryNumber { .. } => 20,
        Instr::BinaryConstantNumber { .. } => 21,
        Instr::BinaryStoreNumber { .. } => 22,
        Instr::FlatBinaryFlatNumber { .. } => 23,
        Instr::FlatBinaryConstantNumber { .. } => 24,
        Instr::FlatJumpUnlessFlatNumber { .. } => 25,
        Instr::FlatJumpUnlessConstantNumber { .. } => 26,
        Instr::FlatBinaryFlatStoreNumber { .. } => 27,
        Instr::FlatBinaryConstantStoreNumber { .. } => 28,
        Instr::FlatProperty { .. } => 29,
        Instr::CallName { .. } => 30,
        Instr::ReturnFused { .. } => 31,
        Instr::Unary(_) => 32,
        Instr::Jump(_) => 33,
        Instr::JumpIfFalse(_) => 34,
        Instr::JumpUnless { .. } => 35,
        Instr::FlatJumpUnlessFlat { .. } => 36,
        Instr::FlatJumpUnlessConstant { .. } => 37,
        Instr::JumpIfFalseOrPop(_) => 38,
        Instr::JumpIfTrueOrPop(_) => 39,
        Instr::BuildList(_) => 40,
        Instr::BuildTuple(_) => 41,
        Instr::BuildSet(_) => 42,
        Instr::StoreScratch(_) => 43,
        Instr::Collect(_) => 44,
        Instr::CollectPair(_) => 45,
        Instr::Comprehension(_) => 46,
        Instr::Generator(_) => 47,
        Instr::BuildDict(_) => 48,
        Instr::MakeFunction(_) => 49,
        Instr::Class(_) => 50,
        Instr::Call { .. } => 51,
        Instr::CallMethod { .. } => 52,
        Instr::GetIndex => 53,
        Instr::UpdateFlatJump { .. } => 54,
        Instr::FlatBinaryFlatStore { .. } => 55,
        Instr::FlatBinaryConstantStore { .. } => 56,
        Instr::BinaryStore { .. } => 57,
        Instr::FlatIndexFlat { .. } => 58,
        Instr::FlatIndexConstant { .. } => 59,
        Instr::FlatStoreIndex { .. } => 60,
        Instr::GetProperty(_) => 61,
        Instr::SetIndex => 62,
        Instr::SetProperty(_) => 63,
        Instr::SetIndexKeep => 64,
        Instr::SetPropertyKeep(_) => 65,
        Instr::UpdateIndex { .. } => 66,
        Instr::UpdateProperty { .. } => 67,
        Instr::UpdateName { .. } => 68,
        Instr::UpdateLocal { .. } => 69,
        Instr::UpdateFlat { .. } => 70,
        Instr::ApplyFormat(_) => 71,
        Instr::Format => 72,
        Instr::Return { .. } => 73,
        Instr::ReturnEscape => 74,
        Instr::Null => 75,
        Instr::PushScope => 76,
        Instr::PopScope => 77,
        Instr::IterInit { .. } => 78,
        Instr::IterNext { .. } => 79,
        Instr::IterDrop => 80,
        Instr::Throw => 81,
        Instr::Import { .. } => 82,
        Instr::Try { .. } => 83,
        Instr::Slice(_) => 84,
        Instr::StoreIndexFlat { .. } => 85,
    }
}

/// What an instruction's shape is called.
pub fn shape(instruction: &Instr) -> &'static str {
    SHAPE_NAMES[shape_index(instruction)]
}
