//! File operations: modes, the file-object API, and their errors.
//!
//! A port of `pkg/evaluator/file_test.go`.

mod common;

use common::run_memory;
use tiger_evaluator::{parse_file_mode, FileMode};

fn mode(read: bool, write: bool, append: bool, truncate: bool, create: bool, binary: bool) -> FileMode {
    FileMode { read, write, append, truncate, create, binary }
}

#[test]
fn parses_file_modes() {
    let valid = [
        ("r", mode(true, false, false, false, false, false)),
        ("rb", mode(true, false, false, false, false, true)),
        ("w", mode(false, true, false, true, true, false)),
        ("wb", mode(false, true, false, true, true, true)),
        ("a", mode(false, true, true, false, true, false)),
        ("ab", mode(false, true, true, false, true, true)),
        ("r+", mode(true, true, false, false, false, false)),
        ("r+b", mode(true, true, false, false, false, true)),
        ("rb+", mode(true, true, false, false, false, true)),
        ("w+", mode(true, true, false, true, true, false)),
        ("a+", mode(true, true, true, false, true, false)),
    ];
    for (text, expected) in valid {
        match parse_file_mode(text) {
            Ok(parsed) => assert_eq!(parsed, expected, "parse_file_mode({:?})", text),
            Err(error) => panic!("parse_file_mode({:?}): {}", text, error),
        }
    }
    for text in ["", "q", "rw", "r++", "rbb", "br"] {
        let error = parse_file_mode(text).unwrap_err();
        assert!(error.contains("invalid mode"), "parse_file_mode({:?}) = {}", text, error);
    }
}

#[test]
fn file_operations() {
    let source = "const PATH = \"data.txt\";\n\
        print(write_file(PATH, \"alpha\\nbeta\\n\"));\n\
        print(file_exists(PATH), file_exists(\"missing.txt\"));\n\
        print(append_file(PATH, \"gamma\\n\"));\n\
        print(read_file(PATH), end=\"\");\n\
        const reader = open(PATH, \"r\");\n\
        print(reader.name, reader.mode, reader.closed);\n\
        print(reader.readline(), end=\"\");\n\
        print(reader.readline(), end=\"\");\n\
        print(reader.readlines());\n\
        print(reader.closed);\n\
        reader.close();\n\
        print(reader.closed);\n\
        const probe = open(PATH, \"r\");\n\
        print(probe.tell());\n\
        print(probe.read(5));\n\
        print(probe.tell());\n\
        print(probe.seek(6));\n\
        print(probe.tell());\n\
        probe.close();\n\
        const writer = open(PATH, \"w\");\n\
        print(writer.write(\"fresh\\n\"));\n\
        writer.writelines([\"one\\n\", \"two\\n\"]);\n\
        writer.close();\n\
        print(read_file(PATH), end=\"\");\n\
        const update = open(PATH, \"r+\");\n\
        print(update.read(), end=\"\");\n\
        update.seek(0);\n\
        update.write(\"FRESH\\n\");\n\
        update.close();\n\
        print(read_file(PATH), end=\"\");\n\
        const binary = open(\"raw.bin\", \"wb\");\n\
        binary.write(\"PQRS\");\n\
        binary.close();\n\
        print(open(\"raw.bin\", \"rb\").read());\n\
        remove_file(\"raw.bin\");\n\
        print(file_exists(\"raw.bin\"));\n\
        remove_file(PATH);\n\
        print(file_exists(PATH));\n\
        print(open(PATH, \"w\").closed);\n";
    let expected = "11\n\
        true false\n\
        6\n\
        alpha\n\
        beta\n\
        gamma\n\
        data.txt r false\n\
        alpha\n\
        beta\n\
        [\"gamma\\n\"]\n\
        false\n\
        true\n\
        0\n\
        alpha\n\
        5\n\
        6\n\
        6\n\
        6\n\
        fresh\n\
        one\n\
        two\n\
        fresh\n\
        one\n\
        two\n\
        FRESH\n\
        one\n\
        two\n\
        PQRS\n\
        false\n\
        false\n\
        false\n";
    assert_eq!(run_memory(source).expect("file operations"), expected);
}

#[test]
fn file_errors() {
    // These run against the in-memory filesystem, so a path is always fresh.
    let cases: [(&str, &str); 15] = [
        ("read_file(\"missing.txt\");", "missing.txt"),
        ("open(\"f.txt\", \"q\");", "invalid mode"),
        ("write_file(\"f.txt\", \"x\"); open(\"f.txt\").write(\"x\");", "not open for writing"),
        ("write_file(\"f.txt\", \"x\"); open(\"f.txt\", \"w\").read();", "not open for reading"),
        ("write_file(\"f.txt\", \"x\"); const handle = open(\"f.txt\"); handle.close(); handle.read();", "closed file"),
        ("write_file(\"f.txt\", \"x\"); open(\"f.txt\").flush(); open(\"f.txt\").unknown();", "has no property"),
        ("read_file(1);", "expects a path string"),
        ("write_file(\"f.txt\");", "write_file expects 2 arguments"),
        ("open(\"f.txt\", \"r\", \"extra\");", "open expects 1 or 2 arguments"),
        ("write_file(\"f.txt\", \"x\"); open(\"f.txt\").read(1, 2);", "read expects 0 or 1 arguments"),
        ("write_file(\"f.txt\", \"x\"); open(\"f.txt\").read(\"all\");", "read size must be a safe integer"),
        ("write_file(\"f.txt\", \"x\"); open(\"f.txt\").seek(\"start\");", "seek offset must be a safe integer"),
        ("write_file(\"f.txt\", \"x\"); open(\"f.txt\").write(1);", "write expects a string"),
        ("remove_file(\"missing.txt\");", "missing.txt"),
        ("open(\"f.txt\", \"r\", mode=\"w\");", "does not accept keyword arguments"),
    ];
    let mut failures = Vec::new();
    for (source, message) in cases {
        match run_memory(source) {
            Ok(text) => failures.push(format!("{}\n  want error {:?}, got output {:?}", source, message, text)),
            Err(error) => {
                if !error.contains(message) || !error.contains(':') {
                    failures.push(format!("{}\n  want positioned {:?}, got {}", source, message, error));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} case(s) failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn file_errors_are_catchable() {
    let source = "try { read_file(\"missing.txt\"); } catch (error) { print(\"caught\", \"missing.txt\" in error); }\n\
        try { open(\"f.txt\", \"q\"); } catch (error) { print(\"caught\", \"invalid mode\" in error); }";
    assert_eq!(run_memory(source).expect("catchable"), "caught true\ncaught true\n");
}

#[test]
fn host_filesystem() {
    // No filesystem is supplied, so the built-ins reach the host disk.
    let directory = std::env::temp_dir().join(format!("tiger-files-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("temporary directory");
    let path = directory.join("report.txt");
    let source = format!(
        "const PATH = {};\n\
         print(write_file(PATH, \"first\\n\"));\n\
         print(append_file(PATH, \"second\\n\"));\n\
         print(read_file(PATH), end=\"\");\n\
         remove_file(PATH);\n\
         print(file_exists(PATH));\n",
        format!("{:?}", path.to_string_lossy())
    );
    let text = common::run(&source).expect("host filesystem");
    assert_eq!(text, "6\n7\nfirst\nsecond\nfalse\n");
    assert!(!path.exists(), "the program should have removed {}", path.display());
    let _ = std::fs::remove_dir_all(&directory);
}
