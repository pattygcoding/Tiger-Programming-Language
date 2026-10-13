//! The differential engine test: both engines must agree on every fixture.
//!
//! Every program the other suites cover — the 40 benchmarks, the `tg` fences in
//! `README.md` and `docs/*.md`, and the `examples/` and `portfolio-features/`
//! programs — is run twice: once on the tree-walking interpreter and once on the
//! compiled (linear) engine. The two must produce exactly the same output and
//! exactly the same error, including its position.
//!
//! This is the safety net for the compiled engine. A divergence in variable
//! resolution, scoping, argument binding, or control flow shows up here as a
//! mismatch, whole-program, without having to guess which construct is at fault.
//!
//! Both engines run with no step limit, because they count steps differently (the
//! compiled engine counts instructions, the tree-walker counts statements) and
//! the point of this test is semantics rather than limits.

use std::path::PathBuf;

use tiger_conformance::{fences, markdown_paths, normalize, run_engine, Engine, Files};

/// One program to run on both engines.
struct Fixture {
    /// How the fixture is named in a failure report.
    name: String,
    /// The source name the interpreter sees, which positioned errors carry.
    filename: String,
    source: String,
    files: Files,
    loader: bool,
    input: Option<String>,
}

/// The `benchmarks/*.tg` programs, each with its optional `.in` input file.
fn benchmarks() -> Vec<Fixture> {
    let directory = tiger_conformance::root().join("benchmarks");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("benchmarks directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().map(|extension| extension == "tg").unwrap_or(false))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path.file_stem().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
            Fixture {
                filename: name.clone(),
                name,
                source: normalize(&std::fs::read_to_string(&path).expect("read program")),
                files: Files::Memory,
                loader: false,
                input: std::fs::read_to_string(path.with_extension("in")).ok().map(|text| normalize(&text)),
            }
        })
        .collect()
}

/// The `tg` fences in `README.md` and `docs/*.md`, with any `input` fence.
fn documentation() -> Vec<Fixture> {
    let mut fixtures = Vec::new();
    for path in markdown_paths() {
        let content = std::fs::read_to_string(&path).expect("read document");
        let blocks = fences(&content);
        let directory = path.parent().expect("parent directory");
        let filename = directory.join("documentation.tg").to_string_lossy().to_string();
        let mut index = 0usize;
        while index < blocks.len() {
            if blocks[index].language != "tg" {
                index += 1;
                continue;
            }
            let block = &blocks[index];
            let mut input: Option<String> = None;
            let mut next = index + 1;
            if next < blocks.len() && blocks[next].language == "input" {
                input = Some(blocks[next].content.clone());
                next += 1;
            }
            // Only examples the documentation pins a result for are runnable.
            if next < blocks.len() && (blocks[next].language == "text" || blocks[next].language == "error") {
                fixtures.push(Fixture {
                    name: format!("{}:{}", path.display(), block.line),
                    filename: filename.clone(),
                    source: block.content.clone(),
                    files: Files::Memory,
                    loader: true,
                    input,
                });
            }
            index += 1;
        }
    }
    fixtures
}

/// The `examples/` and `portfolio-features/` programs.
///
/// These run against the in-memory filesystem rather than the host's, because the
/// point here is that the two engines agree, not that a program matches a golden
/// file, and other test binaries write to the host filesystem at the same time.
fn programs(directory: &str) -> Vec<Fixture> {
    let path = tiger_conformance::root().join(directory);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&path)
        .expect("program directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().map(|extension| extension == "tg").unwrap_or(false))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| Fixture {
            name: format!("{}/{}", directory, path.file_name().unwrap().to_string_lossy()),
            filename: path.to_string_lossy().to_string(),
            source: std::fs::read_to_string(&path).expect("read program"),
            files: Files::Memory,
            loader: true,
            input: Some(String::new()),
        })
        .collect()
}

/// Cases the fixture corpora may not happen to cover, chosen for the corners of
/// `try`/`catch`/`finally`: which exit paths run the `finally`, what a clause
/// condition sees, what happens when the `finally` itself leaves or fails, and
/// how the blocks scope their declarations.
fn semantics() -> Vec<Fixture> {
    const SOURCES: [&str; 12] = [
        // A `finally` runs on normal completion, on a caught throw, and on an
        // error that no clause takes.
        r#"try { print("body"); } finally { print("finally"); }"#,
        r#"try { throw "boom"; } catch (error) { print("caught", error); } finally { print("finally"); }"#,
        r#"try { try { throw "boom"; } finally { print("inner"); } } catch (error) { print("outer", error); }"#,
        // A `return` in the body or in a clause leaves through the `finally`.
        r#"function f() { try { return 1; } finally { print("finally"); } } print(f());"#,
        r#"function f() { try { throw "e"; } catch (error) { return "caught"; } finally { print("finally"); } } print(f());"#,
        // A `return` in a `finally` overrides what was in flight.
        r#"function f() { try { return 1; } finally { return 2; } } print(f());"#,
        // Clauses are tried in order, and a condition sees the caught value.
        r#"try { throw "boom"; } catch (error if error == "other") { print("wrong"); } catch (error) { print("right", error); }"#,
        r#"try { throw 7; } catch (error if error > 5) { print("big"); } catch (error) { print("small"); }"#,
        // An error from a `finally` replaces the one in flight; a rethrow from a
        // clause reaches the next handler out.
        r#"function f() { try { throw "first"; } finally { throw "second"; } } try { f(); } catch (error) { print(error); }"#,
        r#"try { try { throw "x"; } catch (error) { throw "y"; } } catch (error) { print(error); }"#,
        // A caught runtime error arrives as an error value.
        r#"try { const bad = 1 / 0; } catch (error) { print("zero division" in error, type(error)); }"#,
        // The body and each clause have their own scope.
        r#"var value = 1; try { var value = 2; print(value); } catch (error) { value = 9; } print(value);"#,
    ];
    SOURCES
        .iter()
        .enumerate()
        .map(|(index, source)| Fixture {
            name: format!("semantics[{}]", index),
            filename: "semantics.tg".to_string(),
            source: (*source).to_string(),
            files: Files::Memory,
            loader: false,
            input: None,
        })
        .collect()
}

#[test]
fn engines_agree_on_every_fixture() {
    let mut fixtures = benchmarks();
    fixtures.extend(documentation());
    fixtures.extend(programs("examples"));
    fixtures.extend(programs("portfolio-features"));
    fixtures.extend(semantics());
    assert!(fixtures.len() >= 200, "expected the whole corpus, found {} fixtures", fixtures.len());
    let mut failures: Vec<String> = Vec::new();
    for fixture in &fixtures {
        let tree = run_engine(&fixture.source, &fixture.filename, fixture.files, fixture.loader, fixture.input.as_deref(), Engine::Tree);
        let linear = run_engine(&fixture.source, &fixture.filename, fixture.files, fixture.loader, fixture.input.as_deref(), Engine::Auto);
        if tree.output == linear.output && tree.error == linear.error {
            continue;
        }
        failures.push(format!(
            "{}:\n  tree:   output {:?}\n          error  {:?}\n  linear: output {:?}\n          error  {:?}",
            fixture.name, tree.output, tree.error, linear.output, linear.error
        ));
    }
    assert!(
        failures.is_empty(),
        "{} of {} fixtures differ between the tree-walking and compiled engines:\n\n{}",
        failures.len(),
        fixtures.len(),
        failures.join("\n\n")
    );
}
