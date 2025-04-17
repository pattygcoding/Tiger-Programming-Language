use wasm_bindgen::prelude::*;

pub mod token;
pub mod lexer;
pub mod parser;
pub mod ast;
pub mod eval;

#[wasm_bindgen]
pub fn eval_tiger(code: &str) -> String {
    let mut lexer = lexer::Lexer::new(code.to_string());
    let mut parser = parser::Parser::new(&mut lexer);
    let program = parser.parse_program();

    let mut env = eval::Environment::new();
    eval::eval(&program, &mut env)
}
