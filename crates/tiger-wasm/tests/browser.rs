//! Runs the browser runtime under Node, which is the only place the
//! `wasm32-unknown-unknown` build can be exercised without a browser.
//!
//! The module must already be built, because a nested `cargo build` cannot run
//! while `cargo test` holds the build lock:
//!
//! ```text
//! cargo build -p tiger-wasm --release --target wasm32-unknown-unknown
//! cargo test -p tiger-wasm
//! ```
//!
//! `cargo run -p tiger-web -- -build-only` does that build for you.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The compiled browser runtime, preferring the release build the playground ships.
fn module() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    for profile in ["release", "debug"] {
        let candidate = root.join("target").join("wasm32-unknown-unknown").join(profile).join("tiger_wasm.wasm");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn harness() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("runtime.mjs")
}

/// A fresh directory for one test's programs.
fn workspace(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("tiger-wasm-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("temporary directory");
    directory
}

/// Runs a program through the browser runtime, returning its output and exit code.
fn run(module: &Path, program: &Path, input: Option<&Path>) -> (String, i32) {
    let mut command = Command::new("node");
    command.arg(harness()).arg(module).arg(program);
    if let Some(input) = input {
        command.arg(input);
    }
    let finished = command.output().expect("run node");
    let mut text = String::from_utf8_lossy(&finished.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&finished.stderr));
    (text, finished.status.code().unwrap_or(-1))
}

/// Skips with a message when the browser runtime or Node is unavailable.
fn ready() -> Option<PathBuf> {
    let Some(module) = module() else {
        eprintln!("skipping: build the browser runtime first (cargo build -p tiger-wasm --release --target wasm32-unknown-unknown)");
        return None;
    };
    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("skipping: Node is not on PATH");
        return None;
    }
    Some(module)
}

#[test]
fn runs_language_features() {
    let Some(module) = ready() else { return };
    let directory = workspace("language");
    let program = directory.join("sample.tg");
    std::fs::write(
        &program,
        "class Shape { var name; Shape(name) { this.name = name; } function describe() { return f\"a {this.name}\"; } }\n\
         const shapes = [Shape(\"circle\"), Shape(\"square\")];\n\
         for shape in shapes { print(shape.describe()); }\n\
         print(map([1, 2, 3], function(x) { return x * x; }), \"abc\".upper(), len(\"héllo\"));\n\
         try { throw [1, 2]; } catch (error) { print(error, error[1]); }\n",
    )
    .expect("write program");
    let (text, code) = run(&module, &program, None);
    assert_eq!(code, 0, "{}", text);
    assert_eq!(text, "a circle\na square\n[1, 4, 9] ABC 5\n[1, 2] 2\n");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn reads_input_through_the_shared_buffer() {
    let Some(module) = ready() else { return };
    let directory = workspace("input");
    let program = directory.join("greet.tg");
    let input = directory.join("lines.txt");
    std::fs::write(&program, "const name = input(\"name: \");\nprint(\"hi \" + name + \"!\");\n").expect("write program");
    std::fs::write(&input, "Ada\n").expect("write input");
    let (text, code) = run(&module, &program, Some(&input));
    assert_eq!(code, 0, "{}", text);
    assert_eq!(text, "name: hi Ada!\n");

    // End of input surfaces as the same error the native runtime reports.
    let (text, code) = run(&module, &program, None);
    assert_eq!(code, 1, "{}", text);
    assert!(text.contains("EOF when reading a line"), "{}", text);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn loads_imported_modules() {
    let Some(module) = ready() else { return };
    let directory = workspace("imports");
    std::fs::create_dir_all(directory.join("lib")).expect("library directory");
    std::fs::write(directory.join("main.tg"), "import \"lib/math.tg\" as math; print(math.calculate(9));").expect("write entry");
    std::fs::write(
        directory.join("lib").join("math.tg"),
        "function calculate(value) { import \"factor.tg\" as factor; return value * factor.amount; }",
    )
    .expect("write library");
    std::fs::write(directory.join("lib").join("factor.tg"), "const amount = 3;").expect("write factor");
    let (text, code) = run(&module, &directory.join("main.tg"), None);
    assert_eq!(code, 0, "{}", text);
    assert_eq!(text, "27\n");
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn reports_the_modules_the_browser_omits() {
    let Some(module) = ready() else { return };
    let directory = workspace("omitted");
    for (name, source, expected) in [
        ("regex.tg", "print(regex.match(\"a\", \"a\"));", "the regex module is not available in the browser build"),
        ("time.tg", "print(time.now());", "the time module is not available in the browser build"),
    ] {
        let program = directory.join(name);
        std::fs::write(&program, source).expect("write program");
        let (text, code) = run(&module, &program, None);
        assert_eq!(code, 1, "{}", text);
        assert!(text.contains(expected), "{}", text);
    }
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn caps_output_like_the_go_runtime() {
    let Some(module) = ready() else { return };
    let directory = workspace("cap");
    let program = directory.join("big.tg");
    std::fs::write(&program, "var text = \"xxxxxxxxxx\";\nfor i in range(20) { text = text + text; }\nprint(text);\n")
        .expect("write program");
    let (text, code) = run(&module, &program, None);
    assert_eq!(code, 1, "{}", text);
    assert!(text.contains("output limit exceeded (1 MiB)"), "{}", text);
    let _ = std::fs::remove_dir_all(&directory);
}
