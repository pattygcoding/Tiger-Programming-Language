//! Prints what this machine's clocks can resolve.
//!
//! A CPU profiler built on instruction-count-driven sampling cannot tell two instructions
//! apart when they share a sampling interval, so what the flamegraph would show is a count
//! mix rather than a time mix. Whether the clocks here can do better is what this probe
//! answers: how long a read costs, and what the smallest non-zero delta is.

use std::time::Instant;

#[cfg(target_arch = "x86_64")]
fn tsc() -> u64 {
    // SAFETY: `rdtsc` reads the cycle counter and touches nothing else. It is not
    // serializing, which is why the deltas below are read as a distribution and not as
    // a measurement of any one instruction.
    unsafe { core::arch::x86_64::_rdtsc() }
}

fn main() {
    // What one `Instant::now` costs, and the smallest delta it can show.
    let mut small = u64::MAX;
    let start = Instant::now();
    let mut previous = Instant::now();
    for _ in 0..200_000 {
        let now = Instant::now();
        let delta = now.duration_since(previous).as_nanos() as u64;
        if delta > 0 {
            small = small.min(delta);
        }
        previous = now;
    }
    let elapsed = start.elapsed().as_nanos() as u64;
    println!("Instant: {} ns per read, smallest non-zero delta {} ns", elapsed / 200_000, small);
    println!("Instant as a pair around nothing:");

    // A read pair around an empty body, which is the floor per-instruction timing would pay.
    let mut best = u64::MAX;
    for _ in 0..20_000 {
        let before = Instant::now();
        let delta = before.elapsed().as_nanos() as u64;
        best = best.min(delta);
    }
    println!("  best {best} ns");

    #[cfg(target_arch = "x86_64")]
    {
        let mut small = u64::MAX;
        let start = tsc();
        for _ in 0..200_000 {
            let delta = tsc().wrapping_sub(start);
            if delta > 0 {
                small = small.min(delta);
            }
        }
        println!("rdtsc: smallest non-zero delta between consecutive reads {small} ticks");

        // A read pair around an empty body: what one instruction's timing would pay.
        let mut best = u64::MAX;
        let mut total = 0u64;
        for _ in 0..200_000 {
            let before = tsc();
            let delta = tsc().wrapping_sub(before);
            best = best.min(delta);
            total += delta;
        }
        println!("  rdtsc pair around nothing: best {best}, mean {} ticks", total / 200_000);
    }
}
