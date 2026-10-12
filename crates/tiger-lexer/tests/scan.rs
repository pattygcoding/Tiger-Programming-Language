//! The scanner: kinds, positions, escapes, comments, and invalid input.
//!
//! A port of `pkg/lexer/lexer_test.go`.

use tiger_lexer::{scan, Kind, EOF, IDENT, NUMBER, STRING};

fn kinds(source: &str) -> Vec<Kind> {
    scan(source).unwrap_or_else(|error| panic!("scan({:?}): {}", source, error)).into_iter().map(|token| token.kind).collect()
}

#[test]
fn scans_a_program() {
    let tokens = scan("/* comment */\nconst answer = 1.5e2; // comment\nprint(\"hi\\n\", 'there', true != false);").expect("scan");
    let expected: Vec<Kind> =
        vec!["const", IDENT, "=", NUMBER, ";", IDENT, "(", STRING, ",", STRING, ",", "true", "!=", "false", ")", ";", EOF];
    assert_eq!(tokens.iter().map(|token| token.kind).collect::<Vec<Kind>>(), expected);
    assert_eq!(tokens[0].line, 2);
    assert_eq!(tokens[0].column, 1);
    assert_eq!(tokens[7].text, "hi\n");
}

#[test]
fn rejects_invalid_input() {
    for source in ["@", "1e;", "\"unfinished", "\"bad\\q\"", "\"line\nbreak\"", "!true", "\u{0}", "# old comment", "/* unfinished"] {
        let error = scan(source).unwrap_err();
        assert!(error.contains("1:1:"), "{:?}: expected a positioned error, got {}", source, error);
    }
}

#[test]
fn scans_c_style_tokens() {
    assert_eq!(
        kinds("cif (a && b || c) {} celse {}"),
        vec!["cif", "(", IDENT, "&&", IDENT, "||", IDENT, ")", "{", "}", "celse", "{", "}", EOF]
    );
}

#[test]
fn scans_block_comments() {
    let tokens = scan("/* first\nsecond */var value = 6/* inline */ / 2; // end").expect("scan");
    assert_eq!(tokens.iter().map(|token| token.kind).collect::<Vec<Kind>>(), vec!["var", IDENT, "=", NUMBER, "/", NUMBER, ";", EOF]);
    assert_eq!(tokens[0].line, 2);
    assert_eq!(tokens[0].column, 10);
}

#[test]
fn scans_compound_assignments() {
    assert_eq!(
        kinds("value += 1; value -= 1; value *= 2; value %= 3;"),
        vec![IDENT, "+=", NUMBER, ";", IDENT, "-=", NUMBER, ";", IDENT, "*=", NUMBER, ";", IDENT, "%=", NUMBER, ";", EOF]
    );
}

#[test]
fn scans_star_and_double_star() {
    assert_eq!(
        kinds("function f(*rest, **options) { return rest * 2 * options; }"),
        vec!["function", IDENT, "(", "*", IDENT, ",", "**", IDENT, ")", "{", "return", IDENT, "*", NUMBER, "*", IDENT, ";", "}", EOF]
    );
    assert_eq!(
        kinds("collect(*items, **mapping, key=\"value\");"),
        vec![IDENT, "(", "*", IDENT, ",", "**", IDENT, ",", IDENT, "=", STRING, ")", ";", EOF]
    );
}
