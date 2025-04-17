use crate::lexer::Lexer;
use crate::token::{Token, TokenType};
use crate::ast::{Program, Statement, Expression, LetStatement, PrintStatement, Identifier, StringLiteral};

pub struct Parser<'a> {
    lexer: &'a mut Lexer,
    cur_token: Token,
    peek_token: Token,
}

impl<'a> Parser<'a> {
    pub fn new(lexer: &'a mut Lexer) -> Self {
        let mut parser = Parser {
            lexer,
            cur_token: Token::new(TokenType::Illegal, ""),
            peek_token: Token::new(TokenType::Illegal, ""),
        };
        parser.next_token();
        parser.next_token();
        parser
    }

    fn next_token(&mut self) {
        self.cur_token = std::mem::replace(&mut self.peek_token, self.lexer.next_token());
    }

    pub fn parse_program(&mut self) -> Program {
        let mut program = Program::new();

        while self.cur_token.token_type != TokenType::Eof {
            if let Some(stmt) = self.parse_statement() {
                program.statements.push(stmt);
            }
            self.next_token();
        }

        program
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        match self.cur_token.token_type {
            TokenType::Let => self.parse_let_statement(),
            TokenType::Print => self.parse_print_statement(),
            _ => None,
        }
    }

    fn parse_let_statement(&mut self) -> Option<Statement> {
        self.next_token(); // identifier
        let name = Identifier {
            value: self.cur_token.literal.clone(),
        };

        self.next_token(); // '='
        self.next_token(); // value
        let value = Expression::StringLiteral(StringLiteral {
            value: self.cur_token.literal.clone(),
        });

        Some(Statement::Let(LetStatement {
            name,
            value,
        }))
    }

    fn parse_print_statement(&mut self) -> Option<Statement> {
        self.next_token(); // expression
        let value = Expression::Identifier(Identifier {
            value: self.cur_token.literal.clone(),
        });

        Some(Statement::Print(PrintStatement { value }))
    }
}
