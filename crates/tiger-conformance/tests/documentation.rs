//! The `tg`/`text`/`error`/`input` fences in `README.md` and `docs/*.md`.
//!
//! A port of `docs/docs_test.go`: each `tg` fence is followed by an optional
//! `input` fence and then a `text` or `error` fence that pins its result.

use tiger_conformance::{fences, markdown_paths, run, Files};

#[test]
fn documentation_examples() {
    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();
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
            let mut next = index + 1;
            let mut input: Option<String> = None;
            if next < blocks.len() && blocks[next].language == "input" {
                input = Some(blocks[next].content.clone());
                next += 1;
            }
            if next == blocks.len() || (blocks[next].language != "text" && blocks[next].language != "error") {
                failures.push(format!(
                    "{}:{}: each Tiger example must be followed by a text output or error expectation",
                    path.display(),
                    block.line
                ));
                index += 1;
                continue;
            }
            checked += 1;
            let expected = &blocks[next];
            let outcome = run(&block.content, &filename, Files::Memory, true, input.as_deref());
            if expected.language == "error" {
                let message = expected.content.trim();
                match &outcome.error {
                    Some(error) if !message.is_empty() && error.contains(message) => {}
                    other => failures.push(format!(
                        "{}:{}: expected error containing {:?}, got {:?}",
                        path.display(),
                        block.line,
                        message,
                        other
                    )),
                }
            } else if let Some(error) = &outcome.error {
                failures.push(format!("{}:{}: {}", path.display(), block.line, error));
            } else if outcome.output != expected.content {
                failures.push(format!(
                    "{}:{}: output mismatch\nwant: {:?}\ngot:  {:?}",
                    path.display(),
                    block.line,
                    expected.content,
                    outcome.output
                ));
            }
            index = next + 1;
        }
    }
    assert!(checked > 0, "no Tiger documentation examples found");
    println!("checked {} Tiger documentation examples", checked);
    assert!(failures.is_empty(), "{} documentation example(s) failed:\n{}", failures.len(), failures.join("\n---\n"));
}

#[test]
fn documentation_links_resolve() {
    let mut failures = Vec::new();
    for path in markdown_paths() {
        let content = std::fs::read_to_string(&path).expect("read document");
        let directory = path.parent().expect("parent directory");
        for target in link_targets(&content) {
            let file = target.split('#').next().unwrap_or_default();
            if file.is_empty() || file.contains("://") {
                continue;
            }
            let resolved = directory.join(file.replace('/', std::path::MAIN_SEPARATOR_STR));
            if !resolved.exists() {
                failures.push(format!("{}: broken link {:?}", path.display(), target));
            }
        }
    }
    assert!(failures.is_empty(), "{} broken link(s):\n{}", failures.len(), failures.join("\n"));
}

/// The `[text](target)` targets in a document's prose, skipping code fences.
fn link_targets(content: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut in_code = false;
    for line in content.split('\n') {
        if line.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let mut rest = line;
        while let Some(open) = rest.find("](") {
            let after = &rest[open + 2..];
            let Some(close) = after.find(')') else { break };
            targets.push(after[..close].to_string());
            rest = &after[close..];
        }
    }
    targets
}
