//! What a run measures, and how it is serialized.
//!
//! The protocol half is `tiger_conformance::protocol`, which is the same code the
//! threshold test runs, so a dashboard number is a number the gate would assert on.
//! The corpus half times the forty-four benchmark programs the way
//! `crates/tiger-conformance/tests/timing.rs` does, in process, with no process
//! startup in the numbers.
//!
//! Every measurement happens inside the run's own thread, because the interpreter's
//! values are `Rc`-based and not `Send`; what crosses back is plain data.

use std::path::PathBuf;
use std::time::Instant;

use tiger_conformance::protocol::{self, PythonSection, Section};
use tiger_conformance::{Engine, Files, RunOptions};

use crate::json;

/// What a run was asked to measure.
#[derive(Clone, Copy)]
pub struct Plan {
    /// How many times each program or section runs.
    pub iterations: u32,
    /// Whether the forty-four benchmark programs are timed at all.
    pub corpus: bool,
    /// Whether the corpus is timed a second time on the tree-walker, which is how
    /// the compiled engine's share of a workload is read without a profiler.
    pub engines: bool,
}

impl Default for Plan {
    fn default() -> Plan {
        Plan { iterations: 100, corpus: true, engines: false }
    }
}

/// One benchmark program's measurement, in milliseconds.
#[derive(Clone)]
pub struct Program {
    pub name: String,
    /// The fastest single run, which is the estimate to read: the noise on a
    /// sub-millisecond measurement is all upward.
    pub min: f64,
    /// The mean of every run.
    pub mean: f64,
    pub lines: usize,
    /// Why the program could not be timed, when it could not be.
    pub error: Option<String>,
}

/// One pass over the benchmark corpus.
#[derive(Clone)]
pub struct Corpus {
    pub programs: Vec<Program>,
    /// Which engine this pass ran on: `compiled` or `tree`.
    pub engine: &'static str,
    /// The sum of every program's best run.
    pub total_min: f64,
}

/// Everything one run produced.
#[derive(Clone)]
pub struct Report {
    pub plan: Plan,
    /// How long the whole run took, in seconds.
    pub seconds: f64,
    /// The paired protocol, when a CPython was found to compare against.
    pub protocol: Option<Vec<Section>>,
    /// Why the protocol is missing, when it is.
    pub protocol_note: Option<String>,
    pub corpus: Option<Corpus>,
    /// The tree-walker's pass over the same corpus, when it was asked for.
    pub tree: Option<Corpus>,
}

/// What a run reports while it is going, so a dashboard can draw it live.
///
/// The two halves stream at different granularities for a reason rather than for
/// convenience: Tiger's sections are measured one at a time here, while CPython's
/// are all measured by one driver process, so its rows can only arrive together.
pub enum Progress {
    /// A phase is starting.
    Phase(&'static str),
    /// One Tiger section finished; its CPython counterpart is not measured yet.
    TigerSection { name: &'static str, total: f64, work: f64 },
    /// The CPython driver returned every section at once.
    PythonSections(Vec<PythonSection>),
    /// One benchmark program finished, on the named engine.
    Program { name: String, min: f64, engine: &'static str },
    /// Something worth putting on screen: a missing interpreter, a failed program.
    Note(String),
}

/// The benchmark programs, sorted by file name, the way the suites read them.
fn benchmark_paths() -> Vec<PathBuf> {
    let directory = tiger_conformance::root().join("benchmarks");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("benchmarks directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().map(|extension| extension == "tg").unwrap_or(false))
        .collect();
    paths.sort();
    paths
}

/// Times one program `iterations` times and reports its best and mean run.
///
/// One warm-up run comes first, so the first timed run does not pay for a cold
/// instruction cache and an empty pool, exactly as the timing harness does it.
fn time_program(
    source: &str,
    name: &str,
    directory: PathBuf,
    input: Option<&str>,
    engine: Engine,
    iterations: u32,
) -> Result<(f64, f64), String> {
    let options = RunOptions { engine, max_steps: 0, arguments: Vec::new() };
    let run_once = || tiger_conformance::run_in_directory(source, name, directory.clone(), Files::Memory, input, options.clone());
    if let Some(error) = run_once().error {
        return Err(error);
    }
    let mut best = f64::INFINITY;
    let mut total = 0.0;
    for _ in 0..iterations {
        let start = Instant::now();
        let outcome = run_once();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        if let Some(error) = outcome.error {
            return Err(error);
        }
        best = best.min(elapsed);
        total += elapsed;
    }
    Ok((best, total / f64::from(iterations)))
}

/// Times every benchmark program once, reporting each as it finishes.
fn time_corpus(engine: Engine, label: &'static str, iterations: u32, progress: &mut dyn FnMut(Progress)) -> Corpus {
    let directory = tiger_conformance::root().join("benchmarks");
    let mut programs = Vec::with_capacity(44);
    for path in benchmark_paths() {
        let name = path.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
        let source = tiger_conformance::normalize(&std::fs::read_to_string(&path).expect("read program"));
        let input = std::fs::read_to_string(path.with_extension("in")).ok().map(|text| tiger_conformance::normalize(&text));
        let lines = source.trim_end_matches('\n').matches('\n').count() + 1;
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        let measured = time_program(&source, &file, directory.clone(), input.as_deref(), engine, iterations);
        let program = match measured {
            Ok((min, mean)) => Program { name, min, mean, lines, error: None },
            Err(error) => {
                progress(Progress::Note(format!("{name} could not be timed: {error}")));
                Program { name, min: f64::NAN, mean: f64::NAN, lines, error: Some(error) }
            }
        };
        progress(Progress::Program { name: program.name.clone(), min: program.min, engine: label });
        programs.push(program);
    }
    let total_min = programs.iter().filter(|program| program.min.is_finite()).map(|program| program.min).sum();
    Corpus { programs, engine: label, total_min }
}

/// Measures the paired protocol, streaming each half as it arrives.
fn measure_protocol(iterations: u32, progress: &mut dyn FnMut(Progress)) -> (Option<Vec<Section>>, Option<String>) {
    let Some(python) = protocol::python() else {
        let note = "no Python interpreter found; set TIGER_PYTHON to name one".to_string();
        progress(Progress::Note(note.clone()));
        return (None, Some(note));
    };
    let text = match protocol::workload_text() {
        Ok(text) => text,
        Err(error) => {
            progress(Progress::Note(error.clone()));
            return (None, Some(error));
        }
    };
    // Each measurement is bound before it is matched: a closure created in a `match`
    // scrutinee lives until the end of the match, and that would keep `progress`
    // borrowed in the arms that report the failure.
    let measured = protocol::tiger_sections(&text, iterations, Engine::Auto, &mut |section| {
        progress(Progress::TigerSection { name: section.name, total: section.total, work: section.work });
    });
    let tiger = match measured {
        Ok(sections) => sections,
        Err(error) => {
            progress(Progress::Note(error.clone()));
            return (None, Some(error));
        }
    };
    let python_sections = match protocol::python_sections(&python, iterations) {
        Ok(sections) => sections,
        Err(error) => {
            progress(Progress::Note(error.clone()));
            return (None, Some(error));
        }
    };
    progress(Progress::PythonSections(python_sections.clone()));
    match protocol::combine(tiger, python_sections) {
        Ok(sections) => (Some(sections), None),
        Err(error) => {
            progress(Progress::Note(error.clone()));
            (None, Some(error))
        }
    }
}

/// Measures everything the plan asks for, reporting progress as it goes.
pub fn run(plan: Plan, progress: &mut dyn FnMut(Progress)) -> Report {
    let start = Instant::now();
    progress(Progress::Phase("protocol"));
    let (protocol, protocol_note) = measure_protocol(plan.iterations, progress);
    let corpus = if plan.corpus {
        progress(Progress::Phase("corpus"));
        Some(time_corpus(Engine::Auto, "auto", plan.iterations, progress))
    } else {
        None
    };
    let tree = if plan.corpus && plan.engines {
        progress(Progress::Phase("engines"));
        Some(time_corpus(Engine::Tree, "tree", plan.iterations, progress))
    } else {
        None
    };
    progress(Progress::Phase("done"));
    Report { plan, seconds: start.elapsed().as_secs_f64(), protocol, protocol_note, corpus, tree }
}

/// The plan, as JSON.
pub fn plan_json(plan: Plan) -> String {
    format!("{{\"iterations\":{},\"corpus\":{},\"engines\":{}}}", plan.iterations, json::boolean(plan.corpus), json::boolean(plan.engines))
}

/// One protocol section, as JSON.
fn section_json(section: &Section) -> String {
    format!(
        "{{\"name\":{},\"tigerTotal\":{},\"tigerWork\":{},\"pythonCompile\":{},\"pythonExec\":{},\"ratio\":{},\"probe\":{},\"agrees\":{}}}",
        json::quote(section.name),
        json::number(section.tiger_total, 3),
        json::number(section.tiger_work, 3),
        json::number(section.python_compile, 3),
        json::number(section.python_exec, 3),
        json::number(section.ratio(), 2),
        json::boolean(section.is_probe()),
        json::boolean(section.agrees())
    )
}

/// One benchmark program, as JSON.
///
/// A program that could not be timed carries a null time and the reason, so the
/// dashboard can show it as a gap rather than as a zero, which would read as a very
/// fast program.
fn program_json(program: &Program) -> String {
    let error = program.error.as_ref().map_or_else(|| "null".to_string(), |error| json::quote(error));
    format!(
        "{{\"name\":{},\"min\":{},\"mean\":{},\"lines\":{},\"error\":{}}}",
        json::quote(&program.name),
        json::number(program.min, 3),
        json::number(program.mean, 3),
        program.lines,
        error
    )
}

/// One corpus pass, as JSON.
fn corpus_json(corpus: &Corpus) -> String {
    let programs: Vec<String> = corpus.programs.iter().map(program_json).collect();
    format!(
        "{{\"engine\":{},\"totalMin\":{},\"programs\":[{}]}}",
        json::quote(corpus.engine),
        json::number(corpus.total_min, 3),
        programs.join(",")
    )
}

impl Report {
    /// The whole report, as one JSON document.
    pub fn to_json(&self) -> String {
        let protocol = match &self.protocol {
            Some(sections) => {
                let sections: Vec<String> = sections.iter().map(section_json).collect();
                format!("{{\"available\":true,\"sections\":[{}]}}", sections.join(","))
            }
            None => "{\"available\":false,\"sections\":[]}".to_string(),
        };
        let note = self.protocol_note.as_ref().map_or_else(|| "null".to_string(), |note| json::quote(note));
        let corpus = self.corpus.as_ref().map_or_else(|| "null".to_string(), corpus_json);
        let tree = self.tree.as_ref().map_or_else(|| "null".to_string(), corpus_json);
        format!(
            "{{\"plan\":{},\"seconds\":{},\"protocol\":{},\"protocolNote\":{},\"corpus\":{},\"tree\":{}}}",
            plan_json(self.plan),
            json::number(self.seconds, 2),
            protocol,
            note,
            corpus,
            tree
        )
    }
}

impl Progress {
    /// One progress event, as JSON, for the dashboard's event stream.
    pub fn to_json(&self) -> String {
        match self {
            Progress::Phase(phase) => format!("{{\"type\":\"phase\",\"phase\":{}}}", json::quote(phase)),
            Progress::TigerSection { name, total, work } => format!(
                "{{\"type\":\"tiger\",\"name\":{},\"total\":{},\"work\":{}}}",
                json::quote(name),
                json::number(*total, 3),
                json::number(*work, 3)
            ),
            Progress::PythonSections(sections) => {
                let rows: Vec<String> = sections
                    .iter()
                    .map(|section| {
                        format!(
                            "{{\"name\":{},\"compile\":{},\"exec\":{},\"checksum\":{}}}",
                            json::quote(&section.name),
                            json::number(section.compile, 3),
                            json::number(section.exec, 3),
                            json::quote(&section.checksum)
                        )
                    })
                    .collect();
                format!("{{\"type\":\"python\",\"sections\":[{}]}}", rows.join(","))
            }
            Progress::Program { name, min, engine } => format!(
                "{{\"type\":\"program\",\"name\":{},\"min\":{},\"engine\":{}}}",
                json::quote(name),
                json::number(*min, 3),
                json::quote(engine)
            ),
            Progress::Note(text) => format!("{{\"type\":\"note\",\"text\":{}}}", json::quote(text)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        Report {
            plan: Plan::default(),
            seconds: 1.5,
            protocol: Some(vec![Section {
                name: "loop",
                tiger_total: 0.117,
                tiger_work: 0.028,
                python_compile: 0.329,
                python_exec: 0.037,
                tiger_checksum: "1801".to_string(),
                python_checksum: "1801".to_string(),
            }]),
            protocol_note: None,
            corpus: Some(Corpus {
                programs: vec![
                    Program { name: "01_basics".to_string(), min: 0.5, mean: 0.6, lines: 120, error: None },
                    Program { name: "02_broken".to_string(), min: f64::NAN, mean: f64::NAN, lines: 110, error: Some("boom".to_string()) },
                ],
                engine: "auto",
                total_min: 0.5,
            }),
            tree: None,
        }
    }

    /// The cheapest check that a hand-built document is not malformed.
    fn assert_balanced(document: &str) {
        let mut depth = 0i32;
        for character in document.chars() {
            match character {
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                _ => {}
            }
            assert!(depth >= 0, "unbalanced {document}");
        }
        assert_eq!(depth, 0, "unbalanced {document}");
    }

    #[test]
    fn a_report_serializes_as_one_document() {
        let document = report().to_json();
        assert_balanced(&document);
        assert!(document.contains("\"tigerWork\":0.028"), "{document}");
        assert!(document.contains("\"ratio\":0.76"), "{document}");
        assert!(document.contains("\"probe\":false"), "{document}");
    }

    #[test]
    fn a_program_that_could_not_be_timed_has_no_number() {
        let document = report().to_json();
        assert!(document.contains("\"name\":\"02_broken\",\"min\":null"), "{document}");
        assert!(document.contains("\"error\":\"boom\""), "{document}");
    }

    #[test]
    fn a_missing_protocol_is_still_a_document() {
        let mut report = report();
        report.protocol = None;
        report.protocol_note = Some("no Python interpreter found".to_string());
        let document = report.to_json();
        assert_balanced(&document);
        assert!(document.contains("\"available\":false"), "{document}");
        assert!(document.contains("\"protocolNote\":\"no Python interpreter found\""), "{document}");
    }

    #[test]
    fn progress_events_carry_their_own_type() {
        assert_eq!(Progress::Phase("corpus").to_json(), "{\"type\":\"phase\",\"phase\":\"corpus\"}");
        assert_eq!(
            Progress::TigerSection { name: "loop", total: 0.117, work: 0.028 }.to_json(),
            "{\"type\":\"tiger\",\"name\":\"loop\",\"total\":0.117,\"work\":0.028}"
        );
        assert_eq!(
            Progress::Program { name: "01_basics".to_string(), min: 0.5, engine: "auto" }.to_json(),
            "{\"type\":\"program\",\"name\":\"01_basics\",\"min\":0.500,\"engine\":\"auto\"}"
        );
    }

    #[test]
    fn the_default_plan_is_a_short_demo_run() {
        let plan = Plan::default();
        assert!(plan.corpus, "the corpus is the panel a demo is for");
        assert!(!plan.engines, "the second pass doubles the wait, so it is opt-in");
        assert!(plan.iterations > 0);
    }
}
