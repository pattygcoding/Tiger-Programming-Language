//! Integration tests for `tiger build`, mirroring `pkg/compiler`'s tests: the
//! produced executable must run with an empty `PATH` and without its sources.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn tiger() -> &'static str {
    env!("CARGO_BIN_EXE_tiger")
}

fn temporary(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("tiger-standalone-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("temporary directory");
    directory
}

fn build(directory: &Path, source: &str, name: &str) -> PathBuf {
    let entry = directory.join(name);
    if let Some(parent) = entry.parent() {
        std::fs::create_dir_all(parent).expect("entry directory");
    }
    std::fs::write(&entry, source).expect("write program");
    let output = directory.join(if cfg!(windows) { "standalone.exe" } else { "standalone" });
    let result = Command::new(tiger()).arg("build").arg(&entry).arg("-o").arg(&output).output().expect("run tiger build");
    assert!(result.status.success(), "build failed: {}", String::from_utf8_lossy(&result.stderr));
    assert!(String::from_utf8_lossy(&result.stdout).contains("Built "), "build printed {:?}", String::from_utf8_lossy(&result.stdout));
    output
}

fn execute(output: &Path, directory: &Path, input: &str) -> (String, i32) {
    execute_with(output, directory, input, &[], &[])
}

/// Runs a built executable with arguments and extra environment variables,
/// returning its combined output and its exit status.
fn execute_with(output: &Path, directory: &Path, input: &str, arguments: &[&str], environment: &[(&str, &str)]) -> (String, i32) {
    let mut command = Command::new(output);
    command.current_dir(directory).env("PATH", "").args(arguments).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("spawn the standalone executable");
    child.stdin.as_mut().expect("stdin").write_all(input.as_bytes()).expect("write stdin");
    let finished = child.wait_with_output().expect("wait for the standalone executable");
    let mut text = String::from_utf8_lossy(&finished.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&finished.stderr));
    (text, finished.status.code().unwrap_or(-1))
}

#[test]
fn builds_a_standalone_program() {
    let directory = temporary("basic");
    let output = build(&directory, "function square(number) { return number * number; }\nprint(square(9));", "embedded.tg");
    assert_eq!(execute(&output, &directory, ""), ("81\n".to_string(), 0));
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn builds_the_extended_syntax() {
    let directory = temporary("extended");
    let source = "class Counter { private var count = 0; function next() { return ++this.count; } }\n\
const counter = Counter(); for (var index = 0; index < 3; index++) { print(f\"{counter.next()}\"); }\n\
try { throw range(3); } catch (error) { print(error); }\n\
print(input(\"name? \") + \"!\");";
    let output = build(&directory, source, "embedded.tg");
    assert_eq!(execute(&output, &directory, "Tiger\n"), ("1\n2\n3\n[0, 1, 2]\nname? Tiger!\n".to_string(), 0));
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn built_executables_read_arguments_environment_and_status() {
    let directory = temporary("command-line");
    // A built executable is its own program, so `sys.argv[0]` is its path and the
    // rest are the arguments the caller passed; `sys.env` is the environment the
    // process inherited, and `sys.exit` is the status the caller sees.
    let source = "print(len(sys.argv), sys.argv[1:]);\n\
                  print(sys.env.get(\"TIGER_STANDALONE_PROBE\", \"unset\"));\n\
                  try {\n    sys.exit(5);\n} finally {\n    print(\"cleanup\");\n}\n\
                  print(\"not reached\");";
    let output = build(&directory, source, "command_line.tg");
    let (text, code) = execute_with(&output, &directory, "", &["one", "two"], &[("TIGER_STANDALONE_PROBE", "probe")]);
    assert_eq!((text.as_str(), code), ("3 [\"one\", \"two\"]\nprobe\ncleanup\n", 5));

    // Without arguments the list holds only the executable's own path.
    let (text, code) = execute_with(&output, &directory, "", &[], &[]);
    assert_eq!((text.as_str(), code), ("1 []\nunset\ncleanup\n", 5));
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn builds_a_program_with_imports() {
    let directory = temporary("imports");
    let sources = directory.join("sources");
    std::fs::create_dir_all(sources.join("lib")).expect("library directory");
    std::fs::write(
        sources.join("lib").join("math.tg"),
        "export function calculate(value) { import \"factor.tg\" as factor; return value * factor.amount; }",
    )
    .expect("write library");
    std::fs::write(sources.join("lib").join("factor.tg"), "export const amount = 3;").expect("write factor");
    let entry = sources.join("main.tg");
    std::fs::write(&entry, "import { calculate } from \"lib/math.tg\"; print(calculate(9));").expect("write entry");
    let output = directory.join(if cfg!(windows) { "standalone.exe" } else { "standalone" });
    let result = Command::new(tiger()).arg("build").arg(&entry).arg("-o").arg(&output).output().expect("run tiger build");
    assert!(result.status.success(), "build failed: {}", String::from_utf8_lossy(&result.stderr));
    std::fs::remove_dir_all(&sources).expect("remove the sources");
    assert_eq!(execute(&output, &directory, ""), ("27\n".to_string(), 0));
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn rejects_invalid_source() {
    let directory = temporary("invalid");
    let entry = directory.join("invalid.tg");
    std::fs::write(&entry, "print(1)").expect("write program");
    let output = directory.join("standalone");
    let result = Command::new(tiger()).arg("build").arg(&entry).arg("-o").arg(&output).output().expect("run tiger build");
    assert_eq!(result.status.code(), Some(1));
    let message = String::from_utf8_lossy(&result.stderr);
    assert!(message.contains("invalid.tg:1:"), "unexpected error {:?}", message);
    assert!(!output.exists(), "the failed build must not leave an output");
    let _ = std::fs::remove_dir_all(&directory);
}
