use crate::token::{Token, TokenType};

pub struct Lexer {
    input: Vec<char>,
    position: usize,
    read_position: usize,
    ch: Option<char>,
}

impl Lexer {
    pub fn new(input: String) -> Self {
        let mut l = Lexer {
            input: input.chars().collect(),
            position: 0,
            read_position: 0,
            ch: None,
        };
        l.read_char();
        l
    }

    fn read_char(&mut self) {
        self.ch = self.input.get(self.read_position).copied();
        self.position = self.read_position;
        self.read_position += 1;
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let tok = match self.ch {
            Some('=') => {
                if self.peek_char() == Some('=') {
                    self.read_char();
                    Token::new(TokenType::Eq, "==")
                } else {
                    Token::new(TokenType::Assign, "=")
                }
            }
            Some('+') => Token::new(TokenType::Plus, "+"),
            Some('-') => Token::new(TokenType::Minus, "-"),
            Some('*') => Token::new(TokenType::Asterisk, "*"),
            Some('/') => Token::new(TokenType::Slash, "/"),
            Some('<') => {
                if self.peek_char() == Some('=') {
                    self.read_char();
                    Token::new(TokenType::Lte, "<=")
                } else {
                    Token::new(TokenType::Lt, "<")
                }
            }
            Some('>') => {
                if self.peek_char() == Some('=') {
                    self.read_char();
                    Token::new(TokenType::Gte, ">=")
                } else {
                    Token::new(TokenType::Gt, ">")
                }
            }
            Some('!') => {
                if self.peek_char() == Some('=') {
                    self.read_char();
                    Token::new(TokenType::NotEq, "!=")
                } else {
                    Token::new(TokenType::Illegal, "!")
                }
            }
            Some(',') => Token::new(TokenType::Comma, ","),
            Some(';') => Token::new(TokenType::Semicolon, ";"),
            Some('(') => Token::new(TokenType::LParen, "("),
            Some(')') => Token::new(TokenType::RParen, ")"),
            Some('{') => Token::new(TokenType::LBrace, "{"),
            Some('}') => Token::new(TokenType::RBrace, "}"),
            Some('"') => {
                let literal = self.read_string();
                Token::new(TokenType::String, &literal)
            }
            Some(ch) if is_letter(ch) => {
                let literal = self.read_identifier();
                let token_type = TokenType::lookup_ident(&literal);
                return Token::new(token_type, &literal);
            }
            Some(ch) if ch.is_ascii_digit() => {
                let (literal, is_float) = self.read_number();
                let token_type = if is_float {
                    TokenType::Float
                } else {
                    TokenType::Int
                };
                return Token::new(token_type, &literal);
            }
            None => Token::new(TokenType::Eof, ""),
            Some(ch) => Token::new(TokenType::Illegal, &ch.to_string()),
        };

        self.read_char();
        tok
    }

    fn read_identifier(&mut self) -> String {
        let start = self.position;
        while let Some(ch) = self.ch {
            if is_letter(ch) || ch.is_ascii_digit() {
                self.read_char();
            } else {
                break;
            }
        }
        self.input[start..self.position].iter().collect()
    }

    fn read_number(&mut self) -> (String, bool) {
        let start = self.position;
        let mut is_float = false;

        while let Some(ch) = self.ch {
            if ch == '.' {
                if is_float {
                    break;
                }
                is_float = true;
            } else if !ch.is_ascii_digit() {
                break;
            }
            self.read_char();
        }

        (self.input[start..self.position].iter().collect(), is_float)
    }

    fn read_string(&mut self) -> String {
        self.read_char();
        let start = self.position;

        while let Some(ch) = self.ch {
            if ch == '"' || ch == '\0' {
                break;
            }
            self.read_char();
        }

        let result: String = self.input[start..self.position].iter().collect();
        self.read_char();
        result
    }

    fn peek_char(&self) -> Option<char> {
        self.input.get(self.read_position).copied()
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.ch {
            if ch.is_whitespace() {
                self.read_char();
            } else {
                break;
            }
        }
    }
}

fn is_letter(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}
