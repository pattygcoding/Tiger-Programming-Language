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
    for source in ["@", "1e;", "\"unfinished", "\"bad\\q\"", "\"line\nbreak\"", "\u{0}", "# old comment", "/* unfinished"] {
        let error = scan(source).unwrap_err();
        assert!(error.contains("1:1:"), "{:?}: expected a positioned error, got {}", source, error);
    }
}

#[test]
fn scans_symbolic_logical_operators() {
    assert_eq!(
        kinds("if (a && b || c) {} else {}"),
        vec!["if", "(", IDENT, "&&", IDENT, "||", IDENT, ")", "{", "}", "else", "{", "}", EOF]
    );
    assert_eq!(kinds("!a != b;"), vec!["!", IDENT, "!=", IDENT, ";", EOF]);
}

#[test]
fn scans_match_and_the_removed_c_style_words() {
    assert_eq!(kinds("match value { case _: }"), vec!["match", IDENT, "{", "case", IDENT, ":", "}", EOF]);
    for source in ["cif", "celse", "cfor"] {
        assert_eq!(kinds(source), vec![IDENT, EOF], "{} is no longer a keyword", source);
    }
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
fn a_punctuation_token_spells_its_kind_and_builds_no_text() {
    // A punctuation mark's kind is its spelling, so the lexer never allocates a
    // `String` for one. `spelling` reads the kind back.
    let tokens = scan("a + b == c;").expect("scan");
    let spelled: Vec<(&str, &str, bool)> = tokens.iter().map(|token| (token.kind, token.spelling(), token.text.is_empty())).collect();
    assert_eq!(
        spelled,
        vec![
            (IDENT, "a", false),
            ("+", "+", true),
            (IDENT, "b", false),
            ("==", "==", true),
            (IDENT, "c", false),
            (";", ";", true),
            (EOF, "", true),
        ]
    );
}

#[test]
fn every_two_character_operator_scans_as_one_token() {
    for (source, kind) in [
        ("a == b", "=="),
        ("a != b", "!="),
        ("a <= b", "<="),
        ("a >= b", ">="),
        ("a << b", "<<"),
        ("a >> b", ">>"),
        ("a ++", "++"),
        ("a --", "--"),
        ("a += b", "+="),
        ("a -= b", "-="),
        ("a *= b", "*="),
        ("a %= b", "%="),
        ("a && b", "&&"),
        ("a || b", "||"),
        ("a ** b", "**"),
    ] {
        let tokens = scan(source).expect("scan");
        let found = tokens.iter().find(|token| token.kind == kind).unwrap_or_else(|| panic!("{:?} did not scan {:?}", source, kind));
        assert_eq!(found.spelling(), kind);
        assert!(found.text.is_empty(), "{:?} built a text for {:?}", source, kind);
    }
    // A single character that could start a pair still scans as one token.
    for (source, kind) in [("a = b", "="), ("a < b", "<"), ("a > b", ">"), ("a % b", "%"), ("a & b", "&"), ("a | b", "|")] {
        let tokens = scan(source).expect("scan");
        assert!(tokens.iter().any(|token| token.kind == kind), "{:?} did not scan {:?}", source, kind);
    }
}

#[test]
fn a_keyword_and_a_string_spell_what_they_say() {
    let tokens = scan("const name = \"\"; const other = 'x';").expect("scan");
    assert_eq!((tokens[0].kind, tokens[0].spelling()), ("const", "const"));
    assert_eq!(tokens[1].spelling(), "name");
    assert_eq!(tokens[3].kind, STRING);
    // An empty string still spells itself, which is why `spelling` reads the kind
    // only for a token that has no text of its own.
    assert_eq!(tokens[3].spelling(), "");
    assert_eq!(tokens[8].spelling(), "x");
}

#[test]
fn identifiers_may_be_unicode() {
    // The ASCII-first test in the identifier scan must not stop a non-ASCII letter
    // from continuing one.
    let tokens = scan("const café = 1; const naïve = café + 1;").expect("scan");
    assert_eq!(tokens[1].spelling(), "café");
    assert_eq!(tokens[6].spelling(), "naïve");
    assert_eq!(tokens[8].spelling(), "café");
    assert_eq!(tokens[1].kind, IDENT);
    // A digit and an underscore continue an identifier, and a non-ASCII digit does
    // too — while it cannot start one, because a leading digit is a number.
    let tokens = scan("const a_1 = 2; const a٣ = 4;").expect("scan");
    assert_eq!(tokens[1].spelling(), "a_1");
    assert_eq!(tokens[6].spelling(), "a٣");
    let error = scan("const ٣ = 4;").unwrap_err();
    assert!(error.contains("unexpected character"), "a leading non-ASCII digit is not an identifier: {}", error);
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
