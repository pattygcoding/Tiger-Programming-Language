use std::collections::HashMap;

use crate::ast::{Program, Statement, Expression, Identifier, StringLiteral};

#[derive(Default)]
pub struct Environment {
    store: HashMap<String, String>,
}

impl Environment {
    pub fn new() -> Self {
        Environment {
            store: HashMap::new(),
        }
    }

    pub fn set(&mut self, name: String, value: String) {
        self.store.insert(name, value);
    }

    pub fn get(&self, name: &str) -> Option<&String> {
        self.store.get(name)
    }
}

pub fn eval(program: &Program, env: &mut Environment) -> String {
    let mut output = String::new();

    for stmt in &program.statements {
        match stmt {
            Statement::Let(let_stmt) => {
                let val = eval_expression(&let_stmt.value, env);
                env.set(let_stmt.name.value.clone(), val);
            }
            Statement::Print(print_stmt) => {
                let val = eval_expression(&print_stmt.value, env);
                output.push_str(&val);
                output.push('\n');
            }
        }
    }

    output.trim_end().to_string()
}

fn eval_expression(expr: &Expression, env: &Environment) -> String {
    match expr {
        Expression::StringLiteral(StringLiteral { value }) => value.clone(),
        Expression::Identifier(Identifier { value }) => {
            env.get(value).cloned().unwrap_or_else(|| format!("[undefined variable: {}]", value))
        }
    }
}
