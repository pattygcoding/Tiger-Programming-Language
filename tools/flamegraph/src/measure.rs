//! What the tool measures, and how.
//!
//! Four numbers per unit — a benchmark program, or one section of the paired workload:
//!
//! - the **floor**: what a run costs before it does anything, which is `Evaluator::new`
//!   building a global scope and installing the built-ins into it;
//! - the **parse and compile**: the source read, lexed, parsed, and lowered;
//! - the **whole run**: all three, as a program that was launched would pay for them;
//! - the **profile**: the same program run with the interpreter's sampler on, which
//!   reports the time each instruction shape was charged and the time each native call
//!   took.
//!
//! Everything but the profile is a wall-clock minimum over repeated runs, so a unit's
//! width is a measurement rather than a model. The profile is repeated too, and divided
//! back down to one run; `coverage` is how much of that run's execute time the samples
//! account for, which is what says whether to trust the split inside a frame.

use std::path::Path;
use std::time::Instant;

use tiger_evaluator::{Engine, Evaluator, RunOptions};

/// One measured unit: a program, or a section of the paired workload.
pub struct Unit {
    /// What the frame is called.
    pub label: String,
    /// One line about the unit: where it came from, and what it runs on.
    pub detail: String,
    /// `Evaluator::new`, the cost of a run before it does anything. Measured once, on a
    /// program with nothing in it, and the same for every unit — which is what it is.
    pub floor_ns: u64,
    /// The unit's share of its execute time that goes to reading and lowering it.
    ///
    /// This is not the parse's own measurement but the parse's *share*: a program's parts
    /// are measured separately, and each of their minima can fall on a different run, so
    /// the parts do not add back up to the whole. What adds up is a whole run, divided by
    /// the ratio the separate measurements found — see [`measure`].
    pub parse_ns: u64,
    /// The rest of the run: everything after the source has been lowered.
    pub execute_ns: u64,
    /// Read, lex, parse, and lower, measured on its own.
    pub parse_raw_ns: u64,
    /// Execute alone, over a program lowered once, measured on its own.
    pub execute_raw_ns: u64,
    /// The whole run: read, lower, and execute, as running the file costs it.
    pub whole_ns: u64,
    /// How often each instruction shape ran, in one run, heaviest first.
    ///
    /// Counts, and nothing else: what a shape costs cannot be measured on this host — the
    /// interpreter's own module docs record the three attempts — so these are reported as a mix,
    /// which is exact, rather than turned into widths, which would not be.
    pub shape_counts: Vec<(&'static str, u64)>,
    /// Time charged to each native call, per run.
    pub natives: Vec<(String, u64)>,
    /// Instructions executed in the counted run.
    pub instructions: u64,
    /// Whether the program lowered to the compiled engine. A program that does not runs on the
    /// tree-walker, which the counting does not instrument, so its counts cover only the bodies
    /// that did lower.
    pub compiled: bool,
}

impl Unit {
    /// The time the native calls took, which the loop's own time excludes.
    pub fn native_ns(&self) -> u64 {
        self.natives.iter().map(|(_, ns)| *ns).sum()
    }

    /// The time the instruction loop took: the measured execute time less the natives.
    pub fn loop_ns(&self) -> u64 {
        self.execute_ns.saturating_sub(self.native_ns())
    }
}

/// The smallest number of nanoseconds any measurement may report, so a frame that took
/// less than a nanosecond is not drawn as if it took none.
const FLOOR: u64 = 1;

/// Times `body` `iterations` times and reports the minimum, in nanoseconds.
///
/// The minimum rather than the mean: a run is short enough that an interrupt or an
/// allocation elsewhere on the machine lands in one sample and not in the others, and the
/// minimum is the estimate of the work with the least of that noise in it. This is the
/// same choice the repository's own timing harness makes.
fn best(iterations: u32, mut body: impl FnMut()) -> u64 {
    let mut best = u64::MAX;
    for _ in 0..iterations {
        let start = Instant::now();
        body();
        best = best.min(start.elapsed().as_nanos() as u64);
    }
    best.max(FLOOR)
}

/// How a unit is run: the source, the name it is run under, and its `sys.argv`.
pub struct Job<'a> {
    label: String,
    detail: String,
    source: &'a str,
    filename: &'a str,
    arguments: Vec<String>,
}

impl Job<'_> {
    fn options(&self) -> RunOptions {
        RunOptions { engine: Engine::Auto, max_steps: 0, arguments: self.arguments.clone() }
    }
}

/// Runs a unit's source once, discarding its output.
///
/// The whole run is measured through the same entry point a host uses — parse, evaluate,
/// and the evaluator's own construction — so what a frame shows is what running the file
/// costs rather than what one internal phase of it costs.
fn run_once(job: &Job<'_>) {
    let options = job.options();
    let result = tiger_evaluator::run_with_options(job.source, job.filename, None, None, None, Box::new(std::io::sink()), options);
    if let Err(message) = result {
        panic!("{} failed to run: {message}", job.label);
    }
}

/// Measures the floor of a run: what a program with nothing in it costs beyond its parse.
///
/// An empty program is the one program whose whole cost is the evaluator being built and
/// the engine being entered, so the floor can be read off it directly: measure the whole
/// run, measure the parse, and what is left is what a run costs before it does anything.
/// It is the same for every program, because building the evaluator does not depend on
/// what is going to run in it.
pub fn floor_ns(iterations: u32) -> u64 {
    let path = "floor.tg";
    let parse_ns = best(iterations, || {
        let program = tiger_parser::parse_source("", path).expect("an empty program must parse");
        std::hint::black_box(Evaluator::compile(&program, path));
    });
    let whole_ns = best(iterations, || {
        let exit = tiger_evaluator::run_with_options(
            "",
            path,
            None,
            None,
            None,
            Box::new(std::io::sink()),
            RunOptions { max_steps: 0, ..RunOptions::default() },
        );
        std::hint::black_box(exit).expect("an empty program must run");
    });
    whole_ns.saturating_sub(parse_ns).max(FLOOR)
}

/// Measures one unit: the parse, the execute, the whole run, and the profile.
///
/// Each phase is measured on its own and then composed: the whole run is the frame's
/// width, and the parse and the execute are two parts of it, each bounded by what it
/// measured alone. See the split in the body for why it is done that way — a minimum of a
/// part is not a part of a minimum, so the separate measurements cannot simply be added.
pub fn measure(job: &Job<'_>, floor_ns: u64, iterations: u32, profile_runs: u32) -> Unit {
    let parsed = tiger_parser::parse_source(job.source, job.filename).expect("the unit must parse");
    let compiled = Evaluator::compile(&parsed, job.filename);

    // Warmed up before anything is measured, and in the order a run does it: the first
    // unit measured in a process would otherwise pay for code paths, allocator pages, and
    // an instruction cache that every later unit gets for free, and the first unit in a
    // panel is the same program as the others — its parse should cost what theirs does.
    let warm = || {
        let program = tiger_parser::parse_source(job.source, job.filename).expect("the unit must parse");
        std::hint::black_box(Evaluator::compile(&program, job.filename));
    };
    warm();
    run_once(job);

    // Read, lex, parse, and lower, on their own.
    let parse_raw_ns = best(iterations, warm);

    // Execute alone, over a program lowered once, through the engine the program gets.
    let execute_raw_ns = best(iterations, || {
        let mut evaluator = Evaluator::new(Box::new(std::io::sink()));
        evaluator.source_path = job.filename.to_string();
        evaluator.arguments = job.arguments.clone();
        evaluator.max_steps = 0;
        let exit = match &compiled {
            Some(program) => evaluator.execute_compiled(program),
            None => evaluator.execute(&parsed),
        };
        std::hint::black_box(exit).expect("the unit must run");
    });

    // The whole run, as a host that was handed a file pays for it.
    let whole_ns = best(iterations, || run_once(job));

    // The measured whole, split between the parse and the execute.
    //
    // The whole run is what a frame is: it is the one number that has every phase of a run
    // inside it. The floor comes off it, and what is left is divided by the two separate
    // measurements — but the execute may not be given less than it measured on its own,
    // because that measurement is a minimum of exactly this work and the whole run cannot
    // contradict it. So the parse takes the remainder, which is never more than its own
    // measurement either. Both parts are then measurements, and they add up to the run.
    let available = whole_ns.saturating_sub(floor_ns).max(2);
    let execute_ns = available.saturating_sub(parse_raw_ns).max(execute_raw_ns).min(available).max(FLOOR);
    let parse_ns = available.saturating_sub(execute_ns);

    // The profile: the counters on, run a few times. The first run warms the paths the counting
    // itself takes; the rest are read. The counts come from the last run — they are exact, so
    // one run is as good as ten — and the native samples are averaged over all of them, because
    // a native is timed one call in sixty-four and one run of a light program gives few samples.
    // There is no sample size to grow on the instruction side, which is why the caller's
    // `profile_runs` is only taken as a floor here.
    let runs = profile_runs.clamp(3, 20);
    let mut instructions = 0u64;
    let mut shape_counts: Vec<(&'static str, u64)> = Vec::new();
    let mut raw_natives: Vec<(String, u64)> = Vec::new();
    for run in 0..runs {
        profile_run(job, &parsed, &compiled, true);
        if run + 1 == runs {
            instructions = tiger_evaluator::profile::instructions();
            shape_counts = tiger_evaluator::profile::shape_costs().into_iter().map(|shape| (shape.name, shape.count)).collect();
        }
        raw_natives.extend(tiger_evaluator::profile::native_samples());
    }
    let per_run = |ns: u64| ns / u64::from(runs);
    let mut natives = collapse(raw_natives.into_iter().map(|(name, ns)| (name, per_run(ns))));
    natives.retain(|(_, ns)| *ns > 0);

    Unit {
        label: job.label.clone(),
        detail: job.detail.clone(),
        floor_ns,
        parse_ns,
        execute_ns,
        parse_raw_ns,
        execute_raw_ns,
        whole_ns,
        shape_counts,
        natives,
        instructions,
        compiled: compiled.is_some(),
    }
}

/// The profile: two runs with the counters on, the first a warm-up.
///
/// The counters are off for every timed run — [`tiger_evaluator::profile::enable`] — so the
/// times this unit reports are times of a program with nothing added to it, and on for the
/// counted run, whose own cost nobody reads.
fn profile_run(job: &Job<'_>, parsed: &tiger_ast::Program, compiled: &Option<tiger_evaluator::CompiledProgram>, enable: bool) {
    let mut evaluator = Evaluator::new(Box::new(std::io::sink()));
    evaluator.source_path = job.filename.to_string();
    evaluator.arguments = job.arguments.clone();
    evaluator.max_steps = 0;
    tiger_evaluator::profile::reset();
    tiger_evaluator::profile::enable(enable);
    if let Some(program) = compiled {
        evaluator.execute_compiled(program).expect("the unit must run");
    } else {
        evaluator.execute(parsed).expect("the unit must run");
    }
    tiger_evaluator::profile::enable(false);
}

/// Adds up the entries that share a name, keeping the heaviest first.
fn collapse<K: Ord>(entries: impl Iterator<Item = (K, u64)>) -> Vec<(K, u64)> {
    let mut totals: Vec<(K, u64)> = Vec::new();
    for (key, ns) in entries {
        match totals.iter_mut().find(|(seen, _)| *seen == key) {
            Some((_, total)) => *total += ns,
            None => totals.push((key, ns)),
        }
    }
    totals.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    totals
}

/// The paired workload's sections, in the order both sides declare them.
pub const SECTIONS: &[&str] = &["loop", "lists", "dicts", "strings", "methods", "recursion", "calls"];

/// Builds the job that runs one section of the paired workload.
///
/// `sys.argv[1]` is the section's name, which is how both sides of the protocol select
/// one; a section is `wants`-guarded in the source, so nothing else runs.
pub fn section_job<'a>(workload: &'a str, section: &str) -> Job<'a> {
    Job {
        label: section.to_string(),
        detail: format!("benchmarks/python/workload.tg {section}"),
        source: workload,
        filename: "workload.tg",
        arguments: vec!["workload.tg".to_string(), section.to_string()],
    }
}

/// The benchmark programs, in corpus order, minus the four the repository's own
/// profiler leaves out.
///
/// Those four are left out here for the same reason: one needs standard input, one the
/// host filesystem, and two import a module the in-process runner has no loader for.
pub fn benchmark_jobs(root: &Path) -> Vec<(String, String)> {
    let directory = root.join("benchmarks");
    let mut entries: Vec<_> = std::fs::read_dir(&directory).expect("the benchmarks directory").filter_map(|entry| entry.ok()).collect();
    entries.sort_by_key(|entry| entry.path());
    let mut jobs: Vec<(String, String)> = Vec::new();
    for entry in entries {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "tg") {
            continue;
        }
        let name = path.file_name().expect("a file name").to_string_lossy().to_string();
        if matches!(name.as_str(), "19_file_io.tg" | "21_input.tg" | "36_stdlib.tg" | "42_module_exports.tg") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a benchmark program");
        jobs.push((name, source));
    }
    jobs
}

/// Builds the job that runs one benchmark program.
pub fn benchmark_job<'a>(name: &'a str, source: &'a str) -> Job<'a> {
    Job { label: name.to_string(), detail: format!("benchmarks/{name}"), source, filename: name, arguments: vec![name.to_string()] }
}
