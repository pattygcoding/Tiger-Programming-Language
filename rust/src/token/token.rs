#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenType {
    Illegal,
    Eof,

    // Identifiers + literals
    Ident,
    Int,
    Float,
    String,

    // Operators
    Assign,
    Plus,
    Minus,
    Asterisk,
    Slash,
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,

    // Delimiters
    Comma,
    Semicolon,
    LParen,
    RParen,
    LBrace,
    RBrace,

    // Keywords
    Let,
    Print,
    If,
    Else,
    While,
    True,
    False,
    Func,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub token_type: TokenType,
    pub literal: String,
}

impl Token {
    pub fn new(token_type: TokenType, literal: &str) -> Self {
        Self {
            token_type,
            literal: literal.to_string(),
        }
    }
}

impl TokenType {
    pub fn lookup_ident(ident: &str) -> TokenType {
        match ident {
            "let" => TokenType::Let,
            "print" => TokenType::Print,
            "if" => TokenType::If,
            "else" => TokenType::Else,
            "while" => TokenType::While,
            "true" => TokenType::True,
            "false" => TokenType::False,
            "func" => TokenType::Func,
            _ => TokenType::Ident,
        }
    }
}
