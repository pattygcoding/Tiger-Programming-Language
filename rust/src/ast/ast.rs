#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
}

impl Program {
    pub fn new() -> Self {
        Program {
            statements: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Statement {
    Let(LetStatement),
    Print(PrintStatement),
}

#[derive(Debug, Clone)]
pub struct LetStatement {
    pub name: Identifier,
    pub value: Expression,
}

#[derive(Debug, Clone)]
pub struct PrintStatement {
    pub value: Expression,
}

#[derive(Debug, Clone)]
pub enum Expression {
    Identifier(Identifier),
    StringLiteral(StringLiteral),
}

#[derive(Debug, Clone)]
pub struct Identifier {
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct StringLiteral {
    pub value: String,
}
