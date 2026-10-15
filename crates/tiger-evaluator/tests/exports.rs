//! Selective exports and the import forms.
//!
//! A file shares the declarations it marks with `export`; a named import binds the
//! members it lists, a wildcard import (or the alias form) binds the module, and
//! `export import` re-exports. Every case runs on both engines and must agree.

mod common;

use std::rc::Rc;

use common::Shared;
use tiger_evaluator::{BundleLoader, Engine, Evaluator, ModuleLoader};

/// Runs `source` with the modules given, on one engine.
///
/// Each module is reachable from every other under its own name, so `main.tg`
/// imports `"lib.tg"` and `lib.tg` imports `"inner.tg"` the same way.
fn run_on(source: &str, modules: &[(&str, &str)], engine: Engine) -> Result<String, String> {
    let source = source.to_string();
    let modules: Vec<(String, String)> = modules.iter().map(|(name, text)| (name.to_string(), text.to_string())).collect();
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let buffer = Shared::new();
            // Parse under the name the evaluator uses, so both engines position a
            // diagnostic the same way.
            let program = tiger_parser::parse_source(&source, "main.tg").expect("the fixture must parse");
            let mut evaluator = Evaluator::new(Box::new(Shared(buffer.0.clone())));
            evaluator.filesystem = Some(Box::new(tiger_evaluator::MemoryFileSystem::default()));
            evaluator.engine = engine;
            evaluator.source_path = "main.tg".to_string();
            let mut bundle = BundleLoader::default();
            let names: Vec<String> = modules.iter().map(|(name, _)| name.clone()).collect();
            for (name, text) in &modules {
                bundle.sources.insert(name.clone(), text.clone());
                bundle.paths.insert(name.clone(), names.iter().map(|other| (other.clone(), other.clone())).collect());
            }
            bundle.paths.insert("main.tg".to_string(), names.iter().map(|other| (other.clone(), other.clone())).collect());
            evaluator.loader = Some(Rc::new(bundle) as Rc<dyn ModuleLoader>);
            let outcome = evaluator.execute(&program);
            match outcome {
                Ok(_) => Ok(buffer.text()),
                Err(error) => Err(error),
            }
        })
        .expect("the interpreter thread must start");
    handle.join().expect("the interpreter must not panic")
}

/// Runs a program with no module loader at all, which rejects every import.
fn run_without_loader(source: &str) -> Result<String, String> {
    let source = source.to_string();
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let buffer = Shared::new();
            let program = tiger_parser::parse_source(&source, "main.tg").expect("the fixture must parse");
            let mut evaluator = Evaluator::new(Box::new(Shared(buffer.0.clone())));
            evaluator.source_path = "main.tg".to_string();
            match evaluator.execute(&program) {
                Ok(_) => Ok(buffer.text()),
                Err(error) => Err(error),
            }
        })
        .expect("the interpreter thread must start");
    handle.join().expect("the interpreter must not panic")
}

/// Runs `source` on both engines, requiring the same result.
#[track_caller]
fn both(source: &str, modules: &[(&str, &str)]) -> Result<String, String> {
    let compiled = run_on(source, modules, Engine::Auto);
    let walked = run_on(source, modules, Engine::Tree);
    assert_eq!(compiled, walked, "the engines disagree on {:?}", source);
    compiled
}

/// Runs a program that must succeed.
#[track_caller]
fn output(source: &str, modules: &[(&str, &str)]) -> String {
    match both(source, modules) {
        Ok(text) => text,
        Err(error) => panic!("{:?} failed: {}", source, error),
    }
}

/// Runs a program that must fail, returning its message.
#[track_caller]
fn failure(source: &str, modules: &[(&str, &str)]) -> String {
    match both(source, modules) {
        Ok(text) => panic!("{:?} should have failed, output was {:?}", source, text),
        Err(error) => error,
    }
}

/// A module with one mark of every kind, a private helper, and module state.
const TOOLBOX: &str = "export const VERSION = \"1.0\";\n\
                       var uses = 0;\n\
                       export var calls = 0;\n\
                       function helper() { return 1; }\n\
                       export function bump() { uses += 1; calls += 1; return uses; }\n\
                       export function total() { return uses + helper(); }\n\
                       export class Box { var size; Box(size) { this.size = size; } }\n\
                       export const [MIN, MAX] = [1, 9];\n";
/// The re-exported module, with a name of its own that stays private.
const INNER: &str = "export function doubled(value) { return value * 2; }\nconst secret = \"inner\";\n";

#[test]
fn a_module_shares_only_what_it_marks() {
    let modules = &[("lib.tg", TOOLBOX)];
    assert_eq!(
        output("import \"lib.tg\" as lib;\nprint(lib.VERSION, lib.calls, lib.MIN, lib.MAX, lib.bump(), lib.total());", modules),
        "1.0 0 1 9 1 2\n"
    );
    // The private helper is what `total` adds, so the module keeps using it while
    // nothing outside can reach it.
    let message = failure("import \"lib.tg\" as lib;\nprint(lib.helper());", modules);
    assert!(message.contains("has no export \"helper\""), "message was {}", message);
    // A file that marks nothing shares nothing, though its code still runs.
    let quiet = &[("quiet.tg", "const value = 1;\nprint(\"ran\");\n")];
    assert_eq!(output("import \"quiet.tg\" as quiet;\nprint(type(quiet));", quiet), "ran\nmodule\n");
    let message = failure("import \"quiet.tg\" as quiet;\nprint(quiet.value);", quiet);
    assert!(message.contains("has no export \"value\""), "message was {}", message);
}

#[test]
fn every_import_form_binds_the_same_module() {
    let modules = &[("lib.tg", TOOLBOX)];
    assert_eq!(output("import \"lib.tg\" as box;\nprint(box.VERSION);", modules), "1.0\n");
    assert_eq!(output("import * as box from \"lib.tg\";\nprint(box.VERSION);", modules), "1.0\n");
    assert_eq!(
        output("import \"lib.tg\" as box;\nimport * as same from \"lib.tg\";\nprint(box == same, type(box));", modules),
        "true module\n"
    );
    // A named import binds the members directly, and `as` renames any of them.
    assert_eq!(
        output(
            "import { VERSION, bump as count, Box, MIN as lowest } from \"lib.tg\";\nprint(VERSION, count(), count(), lowest, Box(3).size);",
            modules,
        ),
        "1.0 1 2 1 3\n"
    );
    // A named import binds the value the member had; the module reads the one it
    // holds now, so only the module sees a later change.
    assert_eq!(
        output("import \"lib.tg\" as box;\nimport { calls } from \"lib.tg\";\nbox.bump();\nbox.bump();\nprint(calls, box.calls);", modules),
        "0 2\n"
    );
}

#[test]
fn re_exporting_forwards_a_module() {
    // A file re-exports another module by marking the import that binds it.
    let modules = &[("lib.tg", "export import \"inner.tg\" as inner;\n"), ("inner.tg", INNER)];
    assert_eq!(output("import \"lib.tg\" as lib;\nprint(lib.inner.doubled(21));", modules), "42\n");
    // The re-exported module keeps its own private names private.
    let message = failure("import \"lib.tg\" as lib;\nprint(lib.inner.secret);", modules);
    assert!(message.contains("has no export \"secret\""), "message was {}", message);
    // The re-exporting file shares nothing of its own beyond what it marks.
    let message = failure("import \"lib.tg\" as lib;\nprint(lib.doubled);", modules);
    assert!(message.contains("has no export \"doubled\""), "message was {}", message);
    // A named import can re-export a single name too.
    let single = &[("lib.tg", "export import { doubled, doubled as twice } from \"inner.tg\";\n"), ("inner.tg", INNER)];
    assert_eq!(output("import \"lib.tg\" as lib;\nprint(lib.doubled(3), lib.twice(4));", single), "6 8\n");
}

#[test]
fn exports_cover_every_declaration_form() {
    let modules = &[(
        "shapes.tg",
        "export class Point {\n    var x;\n    Point(x) { this.x = x; }\n}\n\
         export const [first, second] = [1, 2];\n\
         export var counter = 0;\n\
         export function step() { counter += 1; return counter; }\n",
    )];
    assert_eq!(
        output("import { Point, first, second, step } from \"shapes.tg\";\nprint(Point(7).x, first, second, step(), step());", modules),
        "7 1 2 1 2\n"
    );
    // A destructured export shares each name it declared, and nothing more.
    let message = failure("import { third } from \"shapes.tg\";", modules);
    assert!(message.contains("has no export \"third\""), "message was {}", message);
}

#[test]
fn import_errors_are_positioned() {
    let modules = &[("lib.tg", TOOLBOX), ("inner.tg", INNER)];
    let message = failure("import { missing } from \"lib.tg\";", modules);
    assert!(message.starts_with("main.tg:1:10"), "message was {}", message);
    assert!(message.contains("has no export \"missing\""), "message was {}", message);
    let message = failure("import { helper } from \"lib.tg\";", modules);
    assert!(message.contains("has no export \"helper\""), "message was {}", message);
    // A name already bound in the same scope is a duplicate declaration.
    let message = failure("const VERSION = 1;\nimport { VERSION } from \"lib.tg\";", modules);
    assert!(message.contains("already declared"), "message was {}", message);
    // A missing file reports the loader's failure, from the import's position.
    let message = failure("import \"nope.tg\" as nope;", modules);
    assert!(message.starts_with("main.tg:1:1"), "message was {}", message);
    assert!(message.contains("nope.tg"), "message was {}", message);
    // Without a loader, importing is an error of its own.
    let error = run_without_loader("import \"lib.tg\" as lib;").expect_err("a loader is required");
    assert!(error.contains("imports require a module loader"), "message was {}", error);
}

#[test]
fn a_module_runs_once_even_under_two_names() {
    let modules = &[("lib.tg", "export var hits = 0;\nexport function count() { hits += 1; return hits; }\nprint(\"loading\");\n")];
    // Both imports share one instance, so the module's own output appears once and
    // its state is one value.
    assert_eq!(
        output(
            "import \"lib.tg\" as box;\nimport * as again from \"lib.tg\";\nprint(box.count(), again.count(), box.hits, box == again);",
            modules,
        ),
        "loading\n1 2 2 true\n"
    );
    assert_eq!(output("import \"lib.tg\" as box;\nprint(box.hits);", modules), "loading\n0\n");
}
