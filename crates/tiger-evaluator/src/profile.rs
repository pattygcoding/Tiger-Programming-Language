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

use std::sync::atomic::{AtomicU64, Ordering};

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

/// Counts one executed instruction.
#[inline]
pub fn record(instruction: Instr) {
    COUNTS[category(&instruction)].fetch_add(1, Ordering::Relaxed);
}

/// The counts so far, in [`NAMES`] order.
pub fn counts() -> [u64; CATEGORIES] {
    let mut counts = [0; CATEGORIES];
    for (slot, count) in counts.iter_mut().zip(COUNTS.iter()) {
        *slot = count.load(Ordering::Relaxed);
    }
    counts
}

/// Clears the counts.
pub fn reset() {
    for count in &COUNTS {
        count.store(0, Ordering::Relaxed);
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
        | Instr::FlatBinaryFlatStore { .. }
        | Instr::FlatBinaryConstantStore { .. }
        | Instr::FlatStoreIndex { .. }
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
        | Instr::BinaryConstant { .. }
        | Instr::FlatBinaryFlat { .. }
        | Instr::FlatBinaryConstant { .. }
        | Instr::Unary(_)
        | Instr::Equal => 7,

        Instr::Jump(_)
        | Instr::JumpIfFalse(_)
        | Instr::JumpUnless { .. }
        | Instr::JumpIfFalseOrPop(_)
        | Instr::JumpIfTrueOrPop(_)
        | Instr::FlatJumpUnlessFlat { .. }
        | Instr::FlatJumpUnlessConstant { .. } => 8,

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
