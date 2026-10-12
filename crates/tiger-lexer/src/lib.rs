//! Position-aware lexer for the Tiger language.
//!
//! Mirrors `pkg/lexer`: keywords and punctuation use their own text as the token
//! kind (as in Go), f-strings nest to a depth of 512, and every diagnostic
//! carries a `line:column` position. `if`, `else`, and `for` cover both the
//! Python-style and the C-style header, so `cif`, `celse`, and `cfor` are gone,
//! and `match` is an extra keyword for `switch`.

use std::rc::Rc;

/// A token kind. Keywords and punctuation use their literal text as the kind,
/// matching the Go implementation's `Kind(string)`.
pub type Kind = &'static str;

pub const EOF: Kind = "EOF";
pub const IDENT: Kind = "identifier";
pub const NUMBER: Kind = "number";
pub const STRING: Kind = "string";
pub const FSTRING: Kind = "formatted string";
pub const INTERPOLATION: Kind = "interpolation";

/// Reserved words, promoted from identifiers to keyword kinds.
pub const KEYWORDS: &[&str] = &[
    "const",
    "var",
    "function",
    "class",
    "extends",
    "super",
    "this",
    "public",
    "private",
    "protected",
    "return",
    "if",
    "elif",
    "else",
    "while",
    "do",
    "for",
    "in",
    "true",
    "false",
    "null",
    "and",
    "or",
    "not",
    "div",
    "break",
    "continue",
    "switch",
    "match",
    "case",
    "default",
    "try",
    "catch",
    "throw",
    "import",
    "as",
];

/// One lexical token, including the sub-tokens of a formatted string.
#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    pub line: u32,
    pub column: u32,
    pub parts: Vec<Token>,
    pub source: Rc<str>,
}

impl Token {
    pub fn new(kind: Kind, line: u32, column: u32) -> Token {
        Token { kind, text: String::new(), line, column, parts: Vec::new(), source: Rc::from("") }
    }

    /// Renders the token position as `source:line:column`, or `line:column`
    /// when the token has no source name.
    pub fn location(&self) -> String {
        if self.source.is_empty() {
            format!("{}:{}", self.line, self.column)
        } else {
            format!("{}:{}:{}", self.source, self.line, self.column)
        }
    }

    /// Builds a positioned diagnostic from a plain message.
    pub fn errorf(&self, message: &str) -> String {
        format!("{}: {}", self.location(), message)
    }
}

/// Scans `source` without a source name.
pub fn scan(source: &str) -> Result<Vec<Token>, String> {
    scan_named(source, "")
}

/// Scans `source`, tagging every token (including nested f-string parts) with
/// `name` for diagnostics.
pub fn scan_named(source: &str, name: &str) -> Result<Vec<Token>, String> {
    let mut scanner = Scanner { source: source.chars().collect(), pos: 0, line: 1, column: 1, name: Rc::from(name) };
    scanner.tokens(false, 0)
}

struct Scanner {
    source: Vec<char>,
    pos: usize,
    line: u32,
    column: u32,
    name: Rc<str>,
}

impl Scanner {
    fn peek(&self, offset: usize) -> char {
        *self.source.get(self.pos + offset).unwrap_or(&'\0')
    }

    fn advance(&mut self) -> char {
        let ch = self.source[self.pos];
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        ch
    }

    fn here(&self) -> Token {
        Token { kind: EOF, text: String::new(), line: self.line, column: self.column, parts: Vec::new(), source: self.name.clone() }
    }

    fn tokens(&mut self, interpolation: bool, depth: usize) -> Result<Vec<Token>, String> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut braces: i32 = 0;
        while self.pos < self.source.len() {
            let current = self.peek(0);
            if interpolation && current == '}' && braces == 0 {
                tokens.push(self.here());
                self.advance();
                return Ok(tokens);
            }
            if current.is_whitespace() {
                self.advance();
                continue;
            }
            if current == '/' && self.peek(1) == '/' {
                while self.pos < self.source.len() && self.peek(0) != '\n' {
                    self.advance();
                }
                continue;
            }
            let mut token = self.here();
            if current == '/' && self.peek(1) == '*' {
                self.advance();
                self.advance();
                while self.pos < self.source.len() && !(self.peek(0) == '*' && self.peek(1) == '/') {
                    self.advance();
                }
                if self.pos == self.source.len() {
                    return Err(token.errorf("unterminated block comment"));
                }
                self.advance();
                self.advance();
                continue;
            }
            let start = self.pos;
            match current {
                'f' | 'F' if self.peek(1) == '"' || self.peek(1) == '\'' => {
                    if depth >= 512 {
                        return Err(token.errorf("maximum formatted string nesting exceeded"));
                    }
                    token = self.formatted_string(token, depth + 1)?;
                }
                'r' | 'R' if self.peek(1) == '"' || self.peek(1) == '\'' => {
                    token = self.raw_string(token)?;
                }
                c if c.is_alphabetic() || c == '_' => {
                    while self.peek(0).is_alphabetic() || self.peek(0).is_numeric() || self.peek(0) == '_' {
                        self.advance();
                    }
                    token.text = self.source[start..self.pos].iter().copied().collect();
                    token.kind = IDENT;
                    if let Some(keyword) = KEYWORDS.iter().find(|k| **k == token.text.as_str()) {
                        token.kind = keyword;
                    }
                }
                c if c.is_ascii_digit() => {
                    while self.peek(0).is_ascii_digit() {
                        self.advance();
                    }
                    if self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
                        self.advance();
                        while self.peek(0).is_ascii_digit() {
                            self.advance();
                        }
                    }
                    if self.peek(0) == 'e' || self.peek(0) == 'E' {
                        self.advance();
                        if self.peek(0) == '+' || self.peek(0) == '-' {
                            self.advance();
                        }
                        while self.peek(0).is_ascii_digit() {
                            self.advance();
                        }
                    }
                    token.text = self.source[start..self.pos].iter().copied().collect();
                    token.kind = NUMBER;
                    if token.text.parse::<f64>().is_err() {
                        return Err(token.errorf(&format!("invalid number {:?}", token.text)));
                    }
                }
                '"' | '\'' => {
                    let quote = self.advance();
                    let mut value: Vec<char> = Vec::new();
                    while self.pos < self.source.len() && self.peek(0) != quote {
                        let mut ch = self.advance();
                        if ch == '\n' || ch == '\r' {
                            return Err(token.errorf("unterminated string"));
                        }
                        if ch == '\\' {
                            ch = self.escape(&token)?;
                        }
                        value.push(ch);
                    }
                    if self.pos == self.source.len() {
                        return Err(token.errorf("unterminated string"));
                    }
                    self.advance();
                    token.kind = STRING;
                    token.text = value.into_iter().collect();
                }
                _ => {
                    self.advance();
                    token.text = current.to_string();
                    match current {
                        '=' | '!' | '<' | '>' => {
                            if self.peek(0) == '=' {
                                token.text.push(self.advance());
                            } else if (current == '<' || current == '>') && self.peek(0) == current {
                                token.text.push(self.advance());
                            }
                        }
                        '+' | '-' => {
                            if self.peek(0) == current || self.peek(0) == '=' {
                                token.text.push(self.advance());
                            }
                        }
                        '*' => {
                            if self.peek(0) == '*' || self.peek(0) == '=' {
                                token.text.push(self.advance());
                            }
                        }
                        '%' => {
                            if self.peek(0) == '=' {
                                token.text.push(self.advance());
                            }
                        }
                        '&' | '|' => {
                            if self.peek(0) == current {
                                token.text.push(self.advance());
                            }
                        }
                        '/' | '(' | ')' | '[' | ']' | '{' | '}' | ':' | ',' | ';' | '.' | '^' | '~' => {}
                        _ => {
                            return Err(token.errorf(&format!("unexpected character {:?}", current)));
                        }
                    }
                    token.kind = operator_kind(&token.text)?;
                    if current == '{' {
                        braces += 1;
                    }
                    if current == '}' {
                        braces -= 1;
                    }
                }
            }
            tokens.push(token);
        }
        if interpolation {
            return Err(self.here().errorf("unterminated f-string interpolation"));
        }
        tokens.push(self.here());
        Ok(tokens)
    }

    fn formatted_string(&mut self, mut token: Token, depth: usize) -> Result<Token, String> {
        self.advance();
        let quote = self.advance();
        token.kind = FSTRING;
        let mut text: Vec<char> = Vec::new();
        while self.pos < self.source.len() {
            let mut ch = self.advance();
            if ch == quote {
                self.flush(&mut token, &mut text);
                return Ok(token);
            }
            if ch == '\n' || ch == '\r' {
                return Err(token.errorf("unterminated formatted string"));
            }
            if ch == '{' || ch == '}' {
                if self.peek(0) == ch {
                    self.advance();
                    text.push(ch);
                    continue;
                }
                if ch == '}' {
                    return Err(token.errorf("single '}' in formatted string; use '}}' for a literal brace"));
                }
                self.flush(&mut token, &mut text);
                let mut part = self.here();
                part.kind = INTERPOLATION;
                part.parts = self.tokens(true, depth)?;
                token.parts.push(part);
                continue;
            }
            if ch == '\\' {
                ch = self.escape(&token)?;
            }
            text.push(ch);
        }
        Err(token.errorf("unterminated formatted string"))
    }

    fn flush(&self, token: &mut Token, text: &mut Vec<char>) {
        if !text.is_empty() {
            let mut segment = Token::new(STRING, token.line, token.column);
            segment.source = self.name.clone();
            segment.text = text.iter().copied().collect();
            token.parts.push(segment);
            text.clear();
        }
    }

    fn raw_string(&mut self, mut token: Token) -> Result<Token, String> {
        self.advance();
        let quote = self.advance();
        let mut value: Vec<char> = Vec::new();
        while self.pos < self.source.len() && self.peek(0) != quote {
            let ch = self.advance();
            if ch == '\n' || ch == '\r' {
                return Err(token.errorf("unterminated raw string"));
            }
            value.push(ch);
        }
        if self.pos == self.source.len() {
            return Err(token.errorf("unterminated raw string"));
        }
        self.advance();
        token.kind = STRING;
        token.text = value.into_iter().collect();
        Ok(token)
    }

    fn escape(&mut self, token: &Token) -> Result<char, String> {
        if self.pos == self.source.len() {
            return Err(token.errorf("unterminated string"));
        }
        let ch = self.advance();
        Ok(match ch {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '\\' | '"' | '\'' => ch,
            'x' => self.hex_escape(2, token)?,
            'u' => self.hex_escape(4, token)?,
            'U' => self.hex_escape(8, token)?,
            _ => return Err(token.errorf(&format!("unknown escape \\{}", ch))),
        })
    }

    fn hex_escape(&mut self, digits: usize, token: &Token) -> Result<char, String> {
        let mut value: u32 = 0;
        for _ in 0..digits {
            let digit = hex_value(self.peek(0));
            if digit < 0 {
                return Err(token.errorf("invalid hexadecimal escape"));
            }
            value = value * 16 + digit as u32;
            self.advance();
        }
        if value > 0x10FFFF || (0xD800..=0xDFFF).contains(&value) {
            return Err(token.errorf(&format!("escape U+{:04X} is not a valid character", value)));
        }
        Ok(char::from_u32(value).expect("valid rune"))
    }
}

fn hex_value(ch: char) -> i32 {
    match ch {
        '0'..='9' => (ch as i32) - ('0' as i32),
        'a'..='f' => (ch as i32) - ('a' as i32) + 10,
        'A'..='F' => (ch as i32) - ('A' as i32) + 10,
        _ => -1,
    }
}

fn operator_kind(text: &str) -> Result<Kind, String> {
    Ok(match text {
        "=" => "=",
        "==" => "==",
        "!=" => "!=",
        "!" => "!",
        "<" => "<",
        "<=" => "<=",
        ">" => ">",
        ">=" => ">=",
        "<<" => "<<",
        ">>" => ">>",
        "+" => "+",
        "++" => "++",
        "+=" => "+=",
        "-" => "-",
        "--" => "--",
        "-=" => "-=",
        "*" => "*",
        "**" => "**",
        "*=" => "*=",
        "%" => "%",
        "%=" => "%=",
        "&&" => "&&",
        "||" => "||",
        "&" => "&",
        "|" => "|",
        "/" => "/",
        "(" => "(",
        ")" => ")",
        "[" => "[",
        "]" => "]",
        "{" => "{",
        "}" => "}",
        ":" => ":",
        "," => ",",
        ";" => ";",
        "." => ".",
        "^" => "^",
        "~" => "~",
        _ => return Err(format!("unexpected operator {:?}", text)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<Kind> {
        scan(source).expect("scan").iter().map(|t| t.kind).collect()
    }

    #[test]
    fn scans_basic_source() {
        let tokens = scan("/* comment */\nconst answer = 1.5e2; // comment\nprint(\"hi\\n\", 'there', true != false);").unwrap();
        let want: Vec<Kind> =
            vec!["const", IDENT, "=", NUMBER, ";", IDENT, "(", STRING, ",", STRING, ",", "true", "!=", "false", ")", ";", EOF];
        assert_eq!(tokens.iter().map(|t| t.kind).collect::<Vec<_>>(), want);
        assert_eq!(tokens[0].line, 2);
        assert_eq!(tokens[0].column, 1);
        assert_eq!(tokens[7].text, "hi\n");
    }

    #[test]
    fn rejects_invalid_input() {
        for source in ["@", "1e;", "\"unfinished", "\"bad\\q\"", "\"line\nbreak\"", "\x00", "# old comment", "/* unfinished"] {
            let error = scan(source).expect_err("expected error");
            assert!(error.contains("1:1:"), "{source:?} => {error}");
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
            assert_eq!(kinds(source), vec![IDENT, EOF], "{source} is no longer a keyword");
        }
    }

    #[test]
    fn scans_block_comments() {
        let tokens = scan("/* first\nsecond */var value = 6/* inline */ / 2; // end").unwrap();
        assert_eq!(tokens.iter().map(|t| t.kind).collect::<Vec<_>>(), vec!["var", IDENT, "=", NUMBER, "/", NUMBER, ";", EOF]);
        assert_eq!(tokens[0].line, 2);
        assert_eq!(tokens[0].column, 10);
    }

    #[test]
    fn scans_compound_assignment_tokens() {
        assert_eq!(
            kinds("value += 1; value -= 1; value *= 2; value %= 3;"),
            vec![IDENT, "+=", NUMBER, ";", IDENT, "-=", NUMBER, ";", IDENT, "*=", NUMBER, ";", IDENT, "%=", NUMBER, ";", EOF]
        );
    }

    #[test]
    fn scans_star_and_double_star_tokens() {
        assert_eq!(
            kinds("function f(*rest, **options) { return rest * 2 * options; }"),
            vec!["function", IDENT, "(", "*", IDENT, ",", "**", IDENT, ")", "{", "return", IDENT, "*", NUMBER, "*", IDENT, ";", "}", EOF]
        );
        assert_eq!(
            kinds("collect(*items, **mapping, key=\"value\");"),
            vec![IDENT, "(", "*", IDENT, ",", "**", IDENT, ",", IDENT, "=", STRING, ")", ";", EOF]
        );
    }
}
