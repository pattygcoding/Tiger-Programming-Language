//! The playground highlighter and the VS Code grammar must list exactly the
//! members of the built-in modules.
//!
//! A port of `pkg/evaluator/highlight_sync_test.go`.

use std::path::PathBuf;

use tiger_evaluator::{algo_module, json_module, math_module, random_module, regex_module, time_module};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn read(path: &str) -> String {
    let full = root().join(path);
    std::fs::read_to_string(&full).unwrap_or_else(|error| panic!("read {}: {}", full.display(), error))
}

/// The names a module's environment defines, sorted.
fn members(module: &std::rc::Rc<tiger_object::Module>) -> Vec<String> {
    let mut names: Vec<String> = module.env.borrow().bindings.keys().cloned().collect();
    names.sort();
    names
}

/// The names `web/highlight.mjs` lists for a module.
fn highlighted(text: &str, module: &str) -> Vec<String> {
    let marker = format!("{}: new Set([", module);
    let start = text.find(&marker).unwrap_or_else(|| panic!("highlight.mjs has no {} set", module));
    let rest = &text[start + marker.len()..];
    let end = rest.find("])").expect("set terminator");
    let mut names: Vec<String> = rest[..end].split('"').skip(1).step_by(2).map(str::to_string).collect();
    names.sort();
    names
}

/// The names the VS Code grammar's `module-members` pattern lists for a module.
fn grammared(text: &str, module: &str) -> Vec<String> {
    let marker = format!("({})", module);
    let start = text.find(&marker).unwrap_or_else(|| panic!("tiger.tmLanguage.json has no {} pattern", module));
    let rest = &text[start..];
    let end = rest.find(")(?!").expect("member group terminator");
    let group = &rest[..end];
    let open = group.rfind('(').expect("member group start");
    let mut names: Vec<String> = group[open + 1..].split('|').map(str::to_string).collect();
    names.sort();
    names
}

#[test]
fn editors_list_module_members() {
    let playground = read("web/highlight.mjs");
    let grammar = read("editors/vscode/syntaxes/tiger.tmLanguage.json");
    for (name, module) in [
        ("math", math_module()),
        ("algo", algo_module()),
        ("random", random_module()),
        ("json", json_module()),
        ("time", time_module()),
        ("regex", regex_module()),
    ] {
        let expected = members(&module);
        assert_eq!(highlighted(&playground, name), expected, "highlight.mjs {} members", name);
        assert_eq!(grammared(&grammar, name), expected, "tiger.tmLanguage.json {} members", name);
    }
}
