//! Measures where a Tiger run spends its time, and draws it.
//!
//! ```sh
//! cd tools/flamegraph
//! cargo run --release
//! ```
//!
//! This writes `docs/flamegraph.svg` and prints the numbers behind it. It is **not** a
//! member of the repository's workspace: it builds the interpreter with its `profile`
//! feature on, and a crate inside the workspace that did that would turn the profiler on
//! for every other test binary built in the same invocation.
//!
//! Two panels, because the corpus and the paired workload answer different questions.
//! The workload's sections isolate language features — a section is a loop, or a
//! dictionary, or a call — so a frame there is one feature's cost. The corpus is what
//! real programs look like, and there the parse is what a small program mostly pays for.

mod measure;
mod svg;

use std::path::{Path, PathBuf};

use measure::Unit;
use svg::{folded, millis, nanos, Kind, Node, Panel};

fn main() {
    let mut iterations = 40u32;
    let mut profile_runs = 40u32;
    let mut out = None;
    let mut print_folded = false;
    let mut shapes = 0usize;
    let mut which = (true, true);
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < arguments.len() {
        let value = |at: usize| arguments.get(at).cloned().expect("the option needs a value");
        match arguments[index].as_str() {
            "--iterations" => {
                iterations = value(index + 1).parse().expect("--iterations needs a number");
                index += 2;
            }
            "--profile-runs" => {
                profile_runs = value(index + 1).parse().expect("--profile-runs needs a number");
                index += 2;
            }
            "--out" => {
                out = Some(PathBuf::from(value(index + 1)));
                index += 2;
            }
            "--folded" => {
                print_folded = true;
                index += 1;
            }
            "--shapes" => {
                shapes = value(index + 1).parse().expect("--shapes needs a number");
                index += 2;
            }
            "--sections-only" => {
                which = (true, false);
                index += 1;
            }
            "--programs-only" => {
                which = (false, true);
                index += 1;
            }
            other => panic!("unknown argument {other:?}"),
        }
    }

    let root = workspace_root();
    let mut sections: Vec<Unit> = Vec::new();
    let mut programs: Vec<Unit> = Vec::new();
    let floor = measure::floor_ns(iterations.max(50));
    println!("the floor of a run is {}", nanos(floor));

    // The paired workload, one section at a time.
    if which.0 {
        let workload = root.join("benchmarks").join("python").join("workload.tg");
        let workload = std::fs::read_to_string(&workload).expect("the paired workload");
        for section in measure::SECTIONS {
            println!("measuring the workload's {section} section");
            sections.push(measure::measure(&measure::section_job(&workload, section), floor, iterations, profile_runs));
        }
    }

    // The benchmark corpus.
    if which.1 {
        for (name, source) in measure::benchmark_jobs(&root) {
            println!("measuring {name}");
            programs.push(measure::measure(&measure::benchmark_job(&name, &source), floor, iterations, profile_runs));
        }
    }

    let mut units: Vec<Unit> = Vec::new();

    // The instruction mix is carried, not turned into widths. Three ways of pricing one shape
    // against another were tried on this host and all three failed measurably — a sampler that
    // reads the clock every few hundred instructions reports a count mix rather than a time mix,
    // a clock read per instruction measures the counter rather than the instruction, and a
    // least-squares solve for one cost per shape leaves a third of every program unexplained
    // because the engine specializes its operands as it runs. So the graph's widths are the
    // times that were measured, and the mix is reported as what it is, counts, where it can be
    // read without being mistaken for time. `docs/performance.md` records the three attempts.
    let shape_ranking = shape_ranking(&sections, &programs);
    let panels = build_panels(&sections, &programs);
    units.extend(sections);
    units.extend(programs);

    let document = svg::render(&panels, &notes(&units, &panels, &shape_ranking));
    let destination = out.unwrap_or_else(|| root.join("docs").join("flamegraph.svg"));
    std::fs::write(&destination, &document).expect("write the SVG");
    report(&units, &panels, &shape_ranking);
    if shapes > 0 {
        for unit in &units {
            println!("\n{} — {} instructions, {} of execute time", unit.label, unit.instructions, millis(unit.execute_ns));
            for (name, count) in unit.shape_counts.iter().take(shapes) {
                println!("    shape  {:<26} {:>12} runs", name, count);
            }
            for (name, ns) in unit.natives.iter().take(shapes) {
                println!("    native {:<26} {:>12}", name, nanos(*ns));
            }
        }
    }
    println!("\nwrote {} ({} bytes)", destination.display(), document.len());
    if print_folded {
        println!();
        for panel in &panels {
            print!("{}", folded(&panel.root));
        }
    }
}

/// The repository root, which is two directories up from this tool.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").canonicalize().expect("the repository root")
}

/// The instruction mix over every unit: how often each shape ran, heaviest first.
///
/// Counts, and reported as counts. They are the one thing about the instruction loop that can be
/// measured exactly on this host, and they answer a different question from the graph's widths:
/// not what a shape cost, but what the loop was made of.
fn shape_ranking(sections: &[Unit], programs: &[Unit]) -> Vec<(&'static str, u64)> {
    let mut totals: Vec<(&'static str, u64)> = Vec::new();
    for unit in sections.iter().chain(programs.iter()) {
        for (name, count) in &unit.shape_counts {
            match totals.iter_mut().find(|(seen, _)| *seen == *name) {
                Some((_, total)) => *total += *count,
                None => totals.push((*name, *count)),
            }
        }
    }
    totals.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
    totals
}

/// The measurement's two panels.
///
/// Within a panel the units are laid out heaviest first, so the frames worth looking at
/// are the wide ones on the left, and a program that costs a fifth of the corpus is a
/// fifth of the panel rather than a sliver somewhere in the middle.
fn build_panels(sections: &[Unit], programs: &[Unit]) -> Vec<Panel> {
    let mut panels: Vec<Panel> = Vec::new();
    if !sections.is_empty() {
        let mut ordered: Vec<&Unit> = sections.iter().collect();
        ordered.sort_by(|left, right| right.whole_ns.cmp(&left.whole_ns).then_with(|| left.label.cmp(&right.label)));
        let total: u64 = sections.iter().map(|unit| unit.whole_ns).sum();
        panels.push(Panel {
            title: format!("The paired workload, one section at a time — {} sections", sections.len()),
            subtitle: "Each section is a language feature on its own: a counting loop, list indexing and in-place stores, dictionary insert and lookup, string building, method calls, recursion, and plain calls. `workload.tg <section>` runs one of them."
                .to_string(),
            root: Node::new(
                Kind::Root,
                format!("paired workload, {} sections", sections.len()),
                total,
                "the measured sum of one run of each section, in process, with the program's own compile counted into each",
            )
            .with(ordered.iter().map(|unit| unit_tree(unit)).collect()),
        });
    }
    if !programs.is_empty() {
        let mut ordered: Vec<&Unit> = programs.iter().collect();
        ordered.sort_by(|left, right| right.whole_ns.cmp(&left.whole_ns).then_with(|| left.label.cmp(&right.label)));
        let total: u64 = programs.iter().map(|unit| unit.whole_ns).sum();
        panels.push(Panel {
            title: format!("The benchmark corpus — {} programs", programs.len()),
            subtitle: "Every reference program the in-process runner can run, one run each. The four that need standard input, the host filesystem, or a module loader are left out, exactly as the repository's own profiler leaves them out."
                .to_string(),
            root: Node::new(
                Kind::Root,
                format!("{} benchmark programs", programs.len()),
                total,
                "the measured sum of one run of each program, startup and parse included",
            )
            .with(ordered.iter().map(|unit| unit_tree(unit)).collect()),
        });
    }
    panels
}

/// The tree of one measured unit: what a run costs, and then what the run did.
fn unit_tree(unit: &Unit) -> Node {
    let children = vec![
        Node::new(
            Kind::Measured,
            format!("floor: Evaluator::new {}", nanos(unit.floor_ns)),
            unit.floor_ns,
            "the cost of a run before it does anything: a global scope built, the built-ins installed into it, the host services set up",
        ),
        Node::new(
            Kind::Measured,
            format!("parse + compile {}", nanos(unit.parse_ns)),
            unit.parse_ns,
            format!(
                "read, lex, parse, and lower to instructions — what a program pays before its first statement runs. Measured alone, the parse and the lowering are {}.",
                nanos(unit.parse_raw_ns)
            ),
        ),
        execute_node(unit),
    ];
    Node::new(
        Kind::Unit,
        format!("{} {}", unit.label, millis(unit.whole_ns)),
        unit.whole_ns,
        format!(
            "{} — {} instructions, {} of execution{}",
            unit.detail,
            unit.instructions,
            millis(unit.execute_ns),
            if unit.compiled { "" } else { ", on the tree-walker, whose dispatch the counting does not instrument" }
        ),
    )
    .with(children)
}

/// The `execute` frame: the measured execute time, split into natives and the loop.
///
/// These two add up to it exactly, because the loop's width is what is left after the natives
/// were measured — so there is no `(unattributed)` remainder here to explain. What the loop was
/// *made of* is in its tooltip as counts, which is the only thing about it that can be measured
/// exactly on this host; a width per instruction shape would be a model, not a measurement.
fn execute_node(unit: &Unit) -> Node {
    let mut inside: Vec<Node> = Vec::new();
    let native_ns = unit.native_ns();
    if native_ns > 0 {
        inside.push(
            Node::new(
                Kind::Native,
                format!("native calls {}", nanos(native_ns)),
                native_ns,
                format!(
                    "built-ins and string methods, each timed as it ran: {} of the execute time. One call in 64 is timed and scaled back up, which is an estimate of the group its sample stands for.",
                    percent(native_ns, unit.execute_ns)
                ),
            )
            .with(
                unit.natives
                    .iter()
                    .map(|(name, ns)| {
                        Node::new(
                            Kind::Native,
                            name,
                            *ns,
                            format!("the native call `{name}`, {} of the execute time", percent(*ns, unit.execute_ns)),
                        )
                    })
                    .collect(),
            ),
        );
    }
    let loop_ns = unit.loop_ns();
    let mix: Vec<String> = unit.shape_counts.iter().take(6).map(|(name, count)| format!("{name} {}", count)).collect();
    inside.push(Node::new(
        Kind::Loop,
        format!("the engine's instructions {}", nanos(loop_ns)),
        loop_ns,
        format!(
            "the instruction loop: the execute time less every measured native call. {} instructions ran{} — {}. What each of those shapes costs is not measurable here: see the notes.",
            unit.instructions,
            if unit.compiled { "" } else { ", on the tree-walker, which the counting does not instrument" },
            mix.join(", ")
        ),
    ));
    inside.sort_by(|left, right| right.ns.cmp(&left.ns).then_with(|| left.name.cmp(&right.name)));
    Node::new(Kind::Measured, format!("execute {}", nanos(unit.execute_ns)), unit.execute_ns, "everything after the source was lowered")
        .with(inside)
}

/// A share as a percentage with one decimal place.
fn percent(part: u64, whole: u64) -> String {
    format!("{:.1}%", part as f64 * 100.0 / whole.max(1) as f64)
}

/// Everything the graph's footer should say, built from what was measured.
fn notes(units: &[Unit], panels: &[Panel], mix: &[(&'static str, u64)]) -> Vec<String> {
    let measured: u64 = panels.iter().map(|panel| panel.root.ns).sum();
    let mut natives: Vec<(String, u64)> = Vec::new();
    for unit in units {
        for (name, ns) in &unit.natives {
            match natives.iter_mut().find(|(seen, _)| *seen == *name) {
                Some((_, total)) => *total += *ns,
                None => natives.push((name.clone(), *ns)),
            }
        }
    }
    natives.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let heaviest: Vec<String> = natives.iter().take(8).map(|(name, ns)| format!("{name} {}", millis(*ns))).collect();
    let instructions: u64 = mix.iter().map(|(_, count)| *count).sum();
    let commonest: Vec<String> =
        mix.iter().take(8).map(|(name, count)| format!("{name} {:.1}%", *count as f64 * 100.0 / instructions.max(1) as f64)).collect();
    let parsed: u64 = units.iter().map(|unit| unit.parse_ns).sum();
    let ran: u64 = units.iter().map(|unit| unit.execute_ns).sum();
    let tree_walked: Vec<&str> = units.iter().filter(|unit| !unit.compiled).map(|unit| unit.label.as_str()).collect();

    let mut notes = Vec::new();
    notes.push(format!(
        "The heaviest native calls, measured as they ran: {}. These are the places a change can be measured against, because each one was timed rather than modelled.",
        heaviest.join(", ")
    ));
    notes.push(format!(
        "The loop is made of what these counts say: {}. Over the {} units drawn, {} instructions ran; the graph has no frame per shape because what each shape costs is not measurable on this host — three attempts are recorded in `docs/performance.md`, and all three failed measurably.",
        commonest.join(", "),
        units.len(),
        instructions
    ));
    notes.push(format!(
        "Reading the two levels that are fully measured: {} was spent parsing and lowering (the first child of every unit) against {} of execution. For a light program the parse is most of the run; for a heavy one it is a few percent.",
        millis(parsed),
        millis(ran)
    ));
    notes.push(format!(
        "The floor of a run — building an evaluator and installing the built-ins — is {} and is the same for every program, which is why it is a frame of its own in each unit rather than folded into the execute time.",
        nanos(units.iter().map(|unit| unit.floor_ns).min().unwrap_or(1))
    ));
    if tree_walked.is_empty() {
        notes.push("Every unit lowered to the compiled engine, so every unit has an instruction count.".to_string());
    } else {
        notes.push(format!(
            "These units do not lower as a whole and run on the tree-walker, which the counting does not instrument: {}. Their widths are measured like everything else's; their instruction counts cover only the bodies that did lower.",
            tree_walked.join(", ")
        ));
    }
    notes.push(format!(
        "Both panels together measured {}. To regenerate: `cd tools/flamegraph && cargo run --release`. The tool sits outside the workspace, so `cargo test` at the root never builds the profiler into anything else.",
        millis(measured)
    ));
    notes
}

/// Prints the numbers behind the picture: what was measured, and what the loop was made of.
fn report(units: &[Unit], panels: &[Panel], mix: &[(&'static str, u64)]) {
    println!();
    println!(
        "{:<22} {:>10} {:>9} {:>9} {:>9} {:>9} {:>9} {:>11}",
        "unit", "whole", "parse*", "execute*", "floor", "parse", "execute", "instrs"
    );
    for unit in units {
        println!(
            "{:<22} {:>10} {:>9} {:>9} {:>9} {:>9} {:>9} {:>11}",
            unit.label,
            millis(unit.whole_ns),
            millis(unit.parse_raw_ns),
            millis(unit.execute_raw_ns),
            nanos(unit.floor_ns),
            millis(unit.parse_ns),
            millis(unit.execute_ns),
            unit.instructions
        );
    }
    println!(
        "`parse*` and `execute*` are each measured on their own; `parse` and `execute` are the measured whole run split by their ratio."
    );
    let measured: u64 = panels.iter().map(|panel| panel.root.ns).sum();
    println!("{:<22} {:>10}", "all panels", millis(measured));

    // The natives, which are the one inner level that is measured rather than counted.
    let mut natives: Vec<(String, u64)> = Vec::new();
    for unit in units {
        for (name, ns) in &unit.natives {
            match natives.iter_mut().find(|(seen, _)| *seen == *name) {
                Some((_, total)) => *total += *ns,
                None => natives.push((name.clone(), *ns)),
            }
        }
    }
    natives.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    println!("\nthe native calls, measured as they ran:");
    for (name, ns) in natives.iter().take(15) {
        println!("  {:<26} {:>10}", name, millis(*ns));
    }

    // The mix, which is a count and not a time, and is reported as a count.
    let instructions: u64 = mix.iter().map(|(_, count)| *count).sum();
    println!("\nthe instruction loop's mix, by count ({} instructions):", instructions);
    for (name, count) in mix.iter().take(15) {
        println!("  {:<26} {:>12}  {:>5.1}%", name, count, *count as f64 * 100.0 / instructions.max(1) as f64);
    }
}
