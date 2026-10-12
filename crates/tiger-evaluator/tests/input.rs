//! `input()`: prompts, line endings, EOF, and reader failures.
//!
//! A port of `pkg/evaluator/input_test.go`.

mod common;

use common::{run, run_input, run_input_failing};

#[test]
fn input_reads_lines() {
    let cases: [(&str, &str, &str); 10] = [
        ("const name = input(\"Name: \"); print(\"Hello, \" + name + \"!\");", "Ada\n", "Name: Hello, Ada!\n"),
        ("print(input(), input());", "one\ntwo\n", "one two\n"),
        ("print(len(input()));", "abc\r\n", "3\n"),
        ("print(input(), input());", "first\nlast", "first last\n"),
        ("const line = input(); print(line == \"\", len(line));", "\n", "true 0\n"),
        ("input(42); print();", "x\n", "42\n"),
        ("const value = input(); print(value + value, str(1) + value);", "7\n", "77 17\n"),
        ("print(input().size());", "t\u{ed}gre\n", "5\n"),
        ("var total = 0; while true { try { total += len(input()); } catch (error) { break; } } print(total);", "ab\ncd\nefg\n", "7\n"),
        ("try { input(); } catch (error) { print(\"EOF\" in error); }", "", "true\n"),
    ];
    for (source, input, expected) in cases {
        match run_input(source, input) {
            Ok(text) if text == expected => {}
            Ok(text) => panic!("{}: want {:?}, got {:?}", source, expected, text),
            Err(error) => panic!("{}: want {:?}, got error {}", source, expected, error),
        }
    }
}

#[test]
fn input_errors() {
    for (source, input, message) in [
        ("input();", "", "EOF when reading a line"),
        ("input(\"a\", \"b\");", "x\n", "input expects at most 1 argument, got 2"),
        ("input(prompt=\"a\");", "x\n", "input does not accept keyword arguments"),
    ] {
        let error = run_input(source, input).unwrap_err();
        assert!(error.contains(message), "{}: expected {:?}, got {}", source, message, error);
    }
    // Without a stream at all, the built-in says so.
    let error = run("input();").unwrap_err();
    assert!(error.contains("no input stream is connected"), "{}", error);
    // A reader that fails reports its own error.
    let error = run_input_failing("input();").unwrap_err();
    assert!(error.contains("broken stdin"), "{}", error);
}
