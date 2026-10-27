//! The in-process Tiger/Python protocol: the measurement itself.
//!
//! Both sides run the same workload, one pinned section at a time, for the same
//! number of iterations, report the minimum, and have their checksums compared,
//! so the two are demonstrably doing the same work. No process startup is in
//! either number.
//!
//! This lives in the library rather than in the test so that the threshold test
//! (`tests/protocol.rs`) and the performance dashboard (`tools/perf-dashboard`)
//! read the *same* numbers: what a dashboard shows is what the gate asserts.
//!
//! Tiger is measured here, in process. CPython is measured by
//! `benchmarks/python/protocol.py`, also in process: it compiles `workload.py`
//! once and then runs one section per `exec`.

use std::path::PathBuf;
use std::time::Instant;

use tiger_evaluator::{CompiledProgram, Engine, Evaluator, RunOptions};

use crate::{root, Shared};

/// The workload's sections, in the order both programs declare them, then the
/// probe: `overhead` names no section, so what it measures is what a run costs
/// before it does any work — the floor every light section is read against. A
/// probe is reported and not asserted.
pub const SECTIONS: &[&str] = &["all", "loop", "lists", "dicts", "strings", "methods", "recursion", "calls", "overhead"];

/// Sections the ratio is reported for but not required of: a probe answers no
/// section, so there is no work for another interpreter to be slower at.
pub const PROBES: &[&str] = &["overhead"];

/// The file name both sides run the workload under, which is what `sys.argv[0]`
/// reports and what a diagnostic would quote.
pub const WORKLOAD: &str = "workload.tg";

/// The paired workload Tiger runs, `benchmarks/python/workload.tg`.
pub fn workload_path() -> PathBuf {
    root().join("benchmarks").join("python").join(WORKLOAD)
}

/// The workload's source text.
pub fn workload_text() -> Result<String, String> {
    let path = workload_path();
    std::fs::read_to_string(&path).map_err(|error| format!("read {}: {}", path.display(), error))
}

/// The interpreter to run the Python half with: `TIGER_PYTHON` when it is set,
/// otherwise the first of `python3` and `python` that answers.
pub fn python() -> Option<String> {
    let candidates: Vec<String> = match std::env::var("TIGER_PYTHON") {
        Ok(configured) => vec![configured],
        Err(_) => vec!["python3".to_string(), "python".to_string()],
    };
    candidates.into_iter().find(|candidate| {
        std::process::Command::new(candidate)
            .arg("-c")
            .arg("pass")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// One section's Tiger measurement, in milliseconds.
#[derive(Clone)]
pub struct TigerSection {
    /// The section's name, as both programs declare it.
    pub name: &'static str,
    /// Parse, lower, and execute.
    pub total: f64,
    /// Execute alone, over a program parsed once.
    pub work: f64,
    /// What the section's last run printed, which the two sides compare.
    pub checksum: String,
}

/// One section's CPython measurement, in milliseconds.
#[derive(Clone)]
pub struct PythonSection {
    /// The section's name, as the driver reported it.
    pub name: String,
    /// `compile()` plus `exec`.
    pub compile: f64,
    /// `exec` alone, over a code object compiled once.
    pub exec: f64,
    /// What the section's last run printed, which the two sides compare.
    pub checksum: String,
}

/// One section, measured on both sides.
#[derive(Clone)]
pub struct Section {
    /// The section's name.
    pub name: &'static str,
    /// Tiger: parse, lower, and execute.
    pub tiger_total: f64,
    /// Tiger: execute alone, over a program parsed once.
    pub tiger_work: f64,
    /// CPython: `compile()` plus `exec`.
    pub python_compile: f64,
    /// CPython: `exec` alone, over a code object compiled once.
    pub python_exec: f64,
    /// What Tiger's last run of the section printed.
    pub tiger_checksum: String,
    /// What CPython's last run of the section printed.
    pub python_checksum: String,
}

impl Section {
    /// The ratio the comparison is about: Tiger's work over CPython's `exec`,
    /// with each program's own compile removed from both sides.
    pub fn ratio(&self) -> f64 {
        if self.python_exec > 0.0 {
            self.tiger_work / self.python_exec
        } else {
            f64::INFINITY
        }
    }

    /// Whether the two sides agree that they did the same work.
    pub fn agrees(&self) -> bool {
        self.tiger_checksum == self.python_checksum
    }

    /// Whether this section is a probe, which is reported and never asserted.
    pub fn is_probe(&self) -> bool {
        PROBES.contains(&self.name)
    }
}

/// Runs the workload `section` on the given engine, with the arguments the
/// program reads as `sys.argv`.
fn options(section: &str, engine: Engine) -> RunOptions {
    RunOptions { engine, max_steps: 0, arguments: vec![WORKLOAD.to_string(), section.to_string()] }
}

/// The best of `iterations` runs, and the checksum the last one printed.
///
/// The minimum is the estimate to read here: the noise on a sub-millisecond
/// measurement is all upward — a descheduled thread is slower, never faster.
fn best(count: u32, mut run: impl FnMut() -> (f64, String)) -> (f64, String) {
    let mut fastest = f64::INFINITY;
    let mut output = String::new();
    for _ in 0..count {
        let (elapsed, text) = run();
        fastest = fastest.min(elapsed);
        output = text;
    }
    (fastest, output)
}

/// One run of the whole workload: parse, lower, and execute, which is what the
/// front of the comparison times.
fn tiger_total(text: &str, options: &RunOptions) -> (f64, String) {
    let buffer = Shared::new();
    let start = Instant::now();
    tiger_evaluator::run_with_options(text, WORKLOAD, None, None, None, Box::new(Shared(buffer.0.clone())), options.clone())
        .expect("the workload must run");
    (start.elapsed().as_secs_f64() * 1000.0, buffer.text())
}

/// One run of the workload with the compile removed: a program parsed and lowered
/// once, then executed here. This is the number the comparison is really about.
fn tiger_work(program: &CompiledProgram, options: &RunOptions) -> (f64, String) {
    let buffer = Shared::new();
    let mut evaluator = Evaluator::new(Box::new(Shared(buffer.0.clone())));
    evaluator.source_path = WORKLOAD.to_string();
    evaluator.engine = options.engine;
    evaluator.max_steps = options.max_steps;
    evaluator.arguments = options.arguments.clone();
    let start = Instant::now();
    evaluator.execute_compiled(program).expect("the workload must run");
    (start.elapsed().as_secs_f64() * 1000.0, buffer.text())
}

/// Measures every section on Tiger, in process.
///
/// `on_section` is called with each section as it finishes, which is how a
/// dashboard streams progress: a section's measurement is over by then, so the
/// number it reports is the number the run will end up with.
pub fn tiger_sections(
    text: &str,
    count: u32,
    engine: Engine,
    on_section: &mut dyn FnMut(&TigerSection),
) -> Result<Vec<TigerSection>, String> {
    // The parse and the lowering are the compile, so both happen once here, outside
    // every timed region, for the measurement that has the compile removed.
    let parsed = tiger_parser::parse_source(text, WORKLOAD).map_err(|error| format!("the workload must parse: {}", error))?;
    let program = Evaluator::compile(&parsed, WORKLOAD).ok_or("the workload must lower")?;
    let mut measured = Vec::with_capacity(SECTIONS.len());
    for &section in SECTIONS {
        let options = options(section, engine);
        // One warm-up run of each shape, so the first section measured does not
        // pay for a cold instruction cache and an empty pool.
        let _ = tiger_total(text, &options);
        let _ = tiger_work(&program, &options);
        let (work, checksum) = best(count, || tiger_work(&program, &options));
        let (total, _) = best(count, || tiger_total(text, &options));
        let measured_section = TigerSection { name: section, total, work, checksum: checksum.trim().to_string() };
        on_section(&measured_section);
        measured.push(measured_section);
    }
    Ok(measured)
}

/// Measures every section on CPython, in process, through the driver.
pub fn python_sections(python: &str, count: u32) -> Result<Vec<PythonSection>, String> {
    let driver = root().join("benchmarks").join("python").join("protocol.py");
    let output = std::process::Command::new(python)
        .arg(&driver)
        .env("TIGER_PROTOCOL_ITERATIONS", count.to_string())
        .output()
        .map_err(|error| format!("the Python protocol must run: {}", error))?;
    if !output.status.success() {
        return Err(format!("{} failed:\n{}", driver.display(), String::from_utf8_lossy(&output.stderr)));
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() != 4 {
                return Err(format!("unreadable protocol row {line:?}"));
            }
            let parse = |value: &str| value.parse::<f64>().map_err(|error| format!("unreadable protocol row {line:?}: {error}"));
            Ok(PythonSection {
                name: fields[0].to_string(),
                compile: parse(fields[2])?,
                exec: parse(fields[3])?,
                checksum: fields[1].to_string(),
            })
        })
        .collect()
}

/// Pairs the two sides up, section by section.
///
/// The two programs declare their sections independently, so a disagreement about
/// which sections exist is a failure of the comparison rather than a measurement.
pub fn combine(tiger: Vec<TigerSection>, python: Vec<PythonSection>) -> Result<Vec<Section>, String> {
    if tiger.len() != python.len() {
        return Err("the two sides disagree about their sections".to_string());
    }
    tiger
        .into_iter()
        .zip(python)
        .map(|(tiger, python)| {
            if tiger.name != python.name {
                return Err(format!("the two sides disagree about their sections: {} against {}", tiger.name, python.name));
            }
            Ok(Section {
                name: tiger.name,
                tiger_total: tiger.total,
                tiger_work: tiger.work,
                python_compile: python.compile,
                python_exec: python.exec,
                tiger_checksum: tiger.checksum,
                python_checksum: python.checksum,
            })
        })
        .collect()
}

/// Measures the whole protocol: every section on both sides, paired up.
pub fn measure(text: &str, python: &str, count: u32, engine: Engine) -> Result<Vec<Section>, String> {
    let tiger = tiger_sections(text, count, engine, &mut |_| {})?;
    let python = python_sections(python, count)?;
    combine(tiger, python)
}

/// The console table's column header, which both the test and the dashboard print.
pub fn format_header() -> String {
    let columns = format!("{:<10}{:>12}{:>12}{:>12}{:>12}{:>9}", "section", "tiger", "python", "p.exec", "tiger work", "ratio");
    format!("{columns}\n{}\n", "-".repeat(67))
}

/// One console table row, without its line ending.
pub fn format_row(section: &Section) -> String {
    format!(
        "{:<10}{:>12}{:>12}{:>12}{:>12}{:>8.2}x",
        section.name,
        format!("{:.3}ms", section.tiger_total),
        format!("{:.3}ms", section.python_compile),
        format!("{:.3}ms", section.python_exec),
        format!("{:.3}ms", section.tiger_work),
        section.ratio()
    )
}

/// The console table's rows, one line per section.
pub fn format_table(sections: &[Section]) -> String {
    let mut table = String::new();
    for section in sections {
        table.push_str(&format_row(section));
        table.push('\n');
    }
    table
}
