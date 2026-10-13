//! Lowers a parsed program to bytecode for the optional linear engine.
//!
//! The compiler lowers the whole program or reports [`Unsupported`], in which
//! case the interpreter runs it instead. That fallback keeps the engine always
//! correct: a program it cannot lower is simply not compiled.

use std::collections::HashMap;
use std::rc::Rc;

use tiger_ast::{Base, Block, Expr, FunctionDecl, LiteralValue, Stmt};
use tiger_object::bytecode::{Chunk, ClassProto, FunctionProto, Instr, Op};
use tiger_object::Value;

use crate::declares_in;

/// The fast-path code for a binary operator, or `Op::Other` for the fallback.
fn operator_code(text: &str) -> Op {
    match text {
        "+" => Op::Add,
        "-" => Op::Sub,
        "*" => Op::Mul,
        "/" => Op::Div,
        "div" => Op::FloorDiv,
        "%" => Op::Mod,
        "**" => Op::Pow,
        "==" => Op::Eq,
        "!=" => Op::Ne,
        "<" => Op::Lt,
        "<=" => Op::Le,
        ">" => Op::Gt,
        ">=" => Op::Ge,
        _ => Op::Other,
    }
}

/// Reported when the program uses a construct the compiler does not lower yet.
pub(crate) struct Unsupported;

type Compiled = Result<(), Unsupported>;

/// Compiles a whole program, or reports that it uses an unsupported construct.
pub(crate) fn compile(statements: &Block, source: &str) -> Result<Rc<Chunk>, Unsupported> {
    chunk_of(statements, Rc::from(source))
}

/// Compiles a block into a chunk that ends by returning the block's value.
fn chunk_of(statements: &[Stmt], source: Rc<str>) -> Result<Rc<Chunk>, Unsupported> {
    let base = statements.first().map(Stmt::base).cloned().unwrap_or(Base { line: 1, column: 1, source: source.clone() });
    let mut compiler = Compiler::new(source);
    compiler.block(statements)?;
    compiler.emit(Instr::Null, &base);
    compiler.emit(Instr::Return, &base);
    Ok(compiler.finish())
}

/// A loop a `break` or `continue` can target. `scope_depth` and `iter_depth` are
/// the depths just outside the loop, so a jump out can pop back to them.
struct LoopTarget {
    label: String,
    breaks: Vec<usize>,
    continues: Vec<usize>,
    scope_depth: usize,
    /// The scope depth a `continue` returns to; it differs from `break` inside a
    /// three-clause `for`, whose initializer scope must survive the `continue`.
    continue_scope_depth: usize,
    iter_depth: usize,
    for_in: bool,
}

struct Compiler {
    code: Vec<Instr>,
    constants: Vec<Value>,
    names: Vec<Rc<str>>,
    name_index: HashMap<String, u32>,
    functions: Vec<Rc<FunctionProto>>,
    classes: Vec<Rc<ClassProto>>,
    positions: Vec<(u32, u32)>,
    source: Rc<str>,
    scope_depth: usize,
    iter_depth: usize,
    loops: Vec<LoopTarget>,
}

impl Compiler {
    fn new(source: Rc<str>) -> Compiler {
        Compiler {
            code: Vec::new(),
            constants: Vec::new(),
            names: Vec::new(),
            name_index: HashMap::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            positions: Vec::new(),
            source,
            scope_depth: 0,
            iter_depth: 0,
            loops: Vec::new(),
        }
    }

    fn emit(&mut self, instr: Instr, base: &Base) {
        self.code.push(instr);
        self.positions.push((base.line, base.column));
    }

    fn here(&self) -> usize {
        self.code.len()
    }

    fn patch(&mut self, at: usize, target: usize) {
        let target = target as u32;
        self.code[at] = match self.code[at] {
            Instr::Jump(_) => Instr::Jump(target),
            Instr::JumpIfFalse(_) => Instr::JumpIfFalse(target),
            Instr::JumpIfFalseOrPop(_) => Instr::JumpIfFalseOrPop(target),
            Instr::JumpIfTrueOrPop(_) => Instr::JumpIfTrueOrPop(target),
            Instr::IterNext(_) => Instr::IterNext(target),
            other => other,
        };
    }

    fn constant(&mut self, value: Value) -> u32 {
        self.constants.push(value);
        (self.constants.len() - 1) as u32
    }

    fn name(&mut self, text: &str) -> u32 {
        if let Some(&index) = self.name_index.get(text) {
            return index;
        }
        let index = self.names.len() as u32;
        self.names.push(Rc::from(text));
        self.name_index.insert(text.to_string(), index);
        index
    }

    fn finish(self) -> Rc<Chunk> {
        Rc::new(Chunk {
            code: self.code,
            constants: self.constants,
            names: self.names,
            functions: self.functions,
            classes: self.classes,
            positions: self.positions,
            source: self.source,
        })
    }

    fn expression(&mut self, expression: &Expr) -> Compiled {
        match expression {
            Expr::Literal { base, value } => {
                let value = match value {
                    LiteralValue::Number(number) => Value::Number(*number),
                    LiteralValue::String(text) => Value::string(text),
                    LiteralValue::Bool(flag) => Value::Bool(*flag),
                    LiteralValue::Null => Value::Null,
                };
                let index = self.constant(value);
                self.emit(Instr::Constant(index), base);
            }
            Expr::Identifier { base, name } => {
                let index = self.name(name);
                self.emit(Instr::LoadName(index), base);
            }
            Expr::Super { base } => {
                self.emit(Instr::LoadSuper, base);
            }
            Expr::Binary { base, left, operator, right } => {
                if operator == "and" {
                    self.expression(left)?;
                    let jump = self.here();
                    self.emit(Instr::JumpIfFalseOrPop(0), base);
                    self.expression(right)?;
                    let end = self.here();
                    self.patch(jump, end);
                } else if operator == "or" {
                    self.expression(left)?;
                    let jump = self.here();
                    self.emit(Instr::JumpIfTrueOrPop(0), base);
                    self.expression(right)?;
                    let end = self.here();
                    self.patch(jump, end);
                } else {
                    self.expression(left)?;
                    self.expression(right)?;
                    let index = self.name(operator);
                    let op = operator_code(operator);
                    self.emit(Instr::Binary { op, name: index }, base);
                }
            }
            Expr::Unary { base, operator, right } => {
                self.expression(right)?;
                let index = self.name(operator);
                self.emit(Instr::Unary(index), base);
            }
            Expr::Conditional { base, condition, then, otherwise } => {
                self.expression(condition)?;
                let to_else = self.here();
                self.emit(Instr::JumpIfFalse(0), base);
                self.expression(then)?;
                let to_end = self.here();
                self.emit(Instr::Jump(0), base);
                let otherwise_start = self.here();
                self.patch(to_else, otherwise_start);
                self.expression(otherwise)?;
                let end = self.here();
                self.patch(to_end, end);
            }
            Expr::List { base, elements } => {
                for element in elements {
                    self.expression(element)?;
                }
                self.emit(Instr::BuildList(elements.len() as u32), base);
            }
            Expr::Dict { base, pairs } => {
                for pair in pairs {
                    self.expression(&pair.key)?;
                    self.expression(&pair.value)?;
                }
                self.emit(Instr::BuildDict(pairs.len() as u32), base);
            }
            Expr::Function(declaration) => {
                let proto = self.function_proto(declaration)?;
                self.emit(Instr::MakeFunction(proto), &declaration.base);
            }
            Expr::Call { base, function, arguments, keywords } => {
                self.expression(function)?;
                for argument in arguments {
                    if argument.spread {
                        return Err(Unsupported);
                    }
                    self.expression(&argument.value)?;
                }
                for entry in keywords {
                    if entry.spread {
                        return Err(Unsupported);
                    }
                    let name = self.constant(Value::string(&entry.name));
                    self.emit(Instr::Constant(name), base);
                    self.expression(&entry.value)?;
                }
                self.emit(Instr::Call { positional: arguments.len() as u32, keywords: keywords.len() as u32 }, base);
            }
            _ => return self.expression_tail(expression),
        }
        Ok(())
    }

    /// The expression cases that follow the common ones: access, update, and
    /// f-strings. Everything else is unsupported.
    fn expression_tail(&mut self, expression: &Expr) -> Compiled {
        match expression {
            Expr::Index { base, collection, key } => {
                self.expression(collection)?;
                self.expression(key)?;
                self.emit(Instr::GetIndex, base);
            }
            Expr::Property { base, receiver, name } => {
                self.expression(receiver)?;
                let index = self.name(name);
                self.emit(Instr::GetProperty(index), base);
            }
            Expr::Update { base, target, operator, prefix } => {
                let Expr::Identifier { name, .. } = &**target else { return Err(Unsupported) };
                let index = self.name(name);
                self.emit(Instr::UpdateName { name: index, decrement: operator == "--", prefix: *prefix }, base);
            }
            Expr::FormattedString { base, parts } => {
                if parts.is_empty() {
                    let index = self.constant(Value::string(""));
                    self.emit(Instr::Constant(index), base);
                }
                for (position, part) in parts.iter().enumerate() {
                    match part {
                        Expr::Interpolation { value, spec, .. } => {
                            self.expression(value)?;
                            let index = self.name(spec);
                            self.emit(Instr::ApplyFormat(index), part.base());
                        }
                        other => {
                            self.expression(other)?;
                            self.emit(Instr::Format, other.base());
                        }
                    }
                    if position > 0 {
                        let plus = self.name("+");
                        self.emit(Instr::Binary { op: Op::Add, name: plus }, base);
                    }
                }
            }
            _ => return Err(Unsupported),
        }
        Ok(())
    }

    fn function_proto(&mut self, declaration: &Rc<FunctionDecl>) -> Result<u32, Unsupported> {
        let code = chunk_of(&declaration.body, self.source.clone())?;
        let index = self.functions.len() as u32;
        self.functions.push(Rc::new(FunctionProto { declaration: declaration.clone(), code }));
        Ok(index)
    }

    fn block(&mut self, statements: &[Stmt]) -> Compiled {
        for statement in statements {
            self.statement(statement)?;
        }
        Ok(())
    }

    /// Compiles a branch body: a child scope only when the body declares
    /// something, matching the interpreter, so a simple branch pays no scope.
    fn branch_body(&mut self, body: &[Stmt], base: &Base) -> Compiled {
        let declares = declares_in(body);
        if declares {
            self.emit(Instr::PushScope, base);
            self.scope_depth += 1;
        }
        self.block(body)?;
        if declares {
            self.emit(Instr::PopScope, base);
            self.scope_depth -= 1;
        }
        Ok(())
    }

    fn statement(&mut self, statement: &Stmt) -> Compiled {
        match statement {
            Stmt::Expression { base, value } => {
                self.expression(value)?;
                self.emit(Instr::Pop, base);
            }
            Stmt::Assign { base, target, value, operator, constant, declaration } => {
                self.assign(base, target, value, operator, *constant, *declaration)?;
            }
            Stmt::Function(declaration) => {
                let proto = self.function_proto(declaration)?;
                self.emit(Instr::MakeFunction(proto), &declaration.base);
                let index = self.name(&declaration.name);
                self.emit(Instr::DefineName { name: index, constant: false }, &declaration.base);
            }
            Stmt::Class(declaration) => {
                let index = self.class_proto(declaration)?;
                self.emit(Instr::Class(index), &declaration.base);
            }
            Stmt::Return { base, value } => {
                match value {
                    Some(value) => self.expression(value)?,
                    None => self.emit(Instr::Null, base),
                }
                self.emit(Instr::Return, base);
            }
            Stmt::Throw { base, value } => {
                self.expression(value)?;
                self.emit(Instr::Throw, base);
            }
            Stmt::Import { base, path, name } => {
                let path = self.name(path);
                let name = self.name(name);
                self.emit(Instr::Import { path, name }, base);
            }
            _ => return self.statement_tail(statement),
        }
        Ok(())
    }

    /// The control-flow statements.
    fn statement_tail(&mut self, statement: &Stmt) -> Compiled {
        match statement {
            Stmt::If { branches, else_block, .. } => {
                let base = statement.base();
                let mut ends = Vec::new();
                for branch in branches {
                    self.expression(&branch.condition)?;
                    let to_next = self.here();
                    self.emit(Instr::JumpIfFalse(0), &branch.condition.base());
                    self.branch_body(&branch.body, &branch.condition.base())?;
                    ends.push(self.here());
                    self.emit(Instr::Jump(0), base);
                    let next = self.here();
                    self.patch(to_next, next);
                }
                self.branch_body(else_block, base)?;
                let end = self.here();
                for position in ends {
                    self.patch(position, end);
                }
            }
            Stmt::While { base, label, condition, body, else_block, .. } => {
                let start = self.here();
                self.expression(condition)?;
                let to_else = self.here();
                self.emit(Instr::JumpIfFalse(0), base);
                self.loops.push(LoopTarget {
                    label: label.clone(),
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth: self.scope_depth,
                    continue_scope_depth: self.scope_depth,
                    iter_depth: self.iter_depth,
                    for_in: false,
                });
                self.branch_body(body, base)?;
                self.emit(Instr::Jump(start as u32), base);
                let else_start = self.here();
                self.patch(to_else, else_start);
                let target = self.loops.pop().expect("loop target");
                self.branch_body(else_block, base)?;
                let end = self.here();
                for position in target.breaks {
                    self.patch(position, end);
                }
                for position in target.continues {
                    self.patch(position, start);
                }
            }
            _ => return self.statement_tail_loops(statement),
        }
        Ok(())
    }

    /// The three `for`-family loops.
    fn statement_tail_loops(&mut self, statement: &Stmt) -> Compiled {
        match statement {
            Stmt::For { base, label, name, iterable, body, else_block } => {
                self.expression(iterable)?;
                self.emit(Instr::IterInit, base);
                self.iter_depth += 1;
                let start = self.here();
                self.emit(Instr::IterNext(0), base);
                self.emit(Instr::PushScope, base);
                self.scope_depth += 1;
                let var = self.name(name);
                self.emit(Instr::DefineName { name: var, constant: false }, base);
                self.loops.push(LoopTarget {
                    label: label.clone(),
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth: self.scope_depth - 1,
                    continue_scope_depth: self.scope_depth - 1,
                    iter_depth: self.iter_depth - 1,
                    for_in: true,
                });
                self.block(body)?;
                self.emit(Instr::PopScope, base);
                self.scope_depth -= 1;
                self.emit(Instr::Jump(start as u32), base);
                let else_start = self.here();
                self.patch(start, else_start);
                self.iter_depth -= 1;
                let target = self.loops.pop().expect("loop target");
                self.branch_body(else_block, base)?;
                let end = self.here();
                for position in target.breaks {
                    self.patch(position, end);
                }
                for position in target.continues {
                    self.patch(position, start);
                }
            }
            Stmt::CFor { base, label, initializer, condition, update, body, else_block } => {
                let outer_depth = self.scope_depth;
                self.emit(Instr::PushScope, base);
                self.scope_depth += 1;
                if let Some(initializer) = initializer {
                    self.statement(initializer)?;
                }
                let start = self.here();
                let to_else = if let Some(condition) = condition {
                    self.expression(condition)?;
                    let jump = self.here();
                    self.emit(Instr::JumpIfFalse(0), base);
                    Some(jump)
                } else {
                    None
                };
                self.loops.push(LoopTarget {
                    label: label.clone(),
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth: outer_depth,
                    continue_scope_depth: outer_depth + 1,
                    iter_depth: self.iter_depth,
                    for_in: false,
                });
                self.branch_body(body, base)?;
                let update_start = self.here();
                if let Some(update) = update {
                    self.statement(update)?;
                }
                self.emit(Instr::Jump(start as u32), base);
                let else_start = self.here();
                if let Some(jump) = to_else {
                    self.patch(jump, else_start);
                }
                let target = self.loops.pop().expect("loop target");
                self.branch_body(else_block, base)?;
                self.emit(Instr::PopScope, base);
                self.scope_depth -= 1;
                let end = self.here();
                for position in target.breaks {
                    self.patch(position, end);
                }
                for position in target.continues {
                    self.patch(position, update_start);
                }
            }
            Stmt::DoWhile { base, label, condition, body } => {
                let start = self.here();
                self.loops.push(LoopTarget {
                    label: label.clone(),
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth: self.scope_depth,
                    continue_scope_depth: self.scope_depth,
                    iter_depth: self.iter_depth,
                    for_in: false,
                });
                self.branch_body(body, base)?;
                let condition_start = self.here();
                self.expression(condition)?;
                let to_end = self.here();
                self.emit(Instr::JumpIfFalse(0), base);
                self.emit(Instr::Jump(start as u32), base);
                let end = self.here();
                self.patch(to_end, end);
                let target = self.loops.pop().expect("loop target");
                for position in target.breaks {
                    self.patch(position, end);
                }
                for position in target.continues {
                    self.patch(position, condition_start);
                }
            }
            _ => return self.statement_tail_control(statement),
        }
        Ok(())
    }

    /// `break`/`continue`, and the constructs the compiler does not lower.
    fn statement_tail_control(&mut self, statement: &Stmt) -> Compiled {
        match statement {
            Stmt::Control { base, kind, label, .. } => {
                let index = if label.is_empty() {
                    self.loops.len().checked_sub(1)
                } else {
                    self.loops.iter().rposition(|target| &target.label == label)
                };
                let Some(index) = index else { return Err(Unsupported) };
                let is_break = kind == "break";
                let target = &self.loops[index];
                let (scope_depth, continue_depth, iter_depth, for_in) =
                    (target.scope_depth, target.continue_scope_depth, target.iter_depth, target.for_in);
                // A `break` discards the loop's own iterator; a `continue` keeps it.
                let drop_iterators = self.iter_depth - iter_depth - usize::from(!is_break && for_in);
                let pop_scopes = self.scope_depth - if is_break { scope_depth } else { continue_depth };
                for _ in 0..drop_iterators {
                    self.emit(Instr::IterDrop, base);
                }
                for _ in 0..pop_scopes {
                    self.emit(Instr::PopScope, base);
                }
                let position = self.here();
                self.emit(Instr::Jump(0), base);
                if is_break {
                    self.loops[index].breaks.push(position);
                } else {
                    self.loops[index].continues.push(position);
                }
            }
            _ => return Err(Unsupported),
        }
        Ok(())
    }

    /// Compiles an assignment, a compound assignment, or a declaration.
    fn assign(&mut self, base: &Base, target: &Expr, value: &Expr, operator: &str, constant: bool, declaration: bool) -> Compiled {
        if declaration || constant {
            let Expr::Identifier { name, .. } = target else { return Err(Unsupported) };
            self.expression(value)?;
            let index = self.name(name);
            self.emit(Instr::DefineName { name: index, constant }, base);
            return Ok(());
        }
        if operator != "=" {
            let Expr::Identifier { name, .. } = target else { return Err(Unsupported) };
            let index = self.name(name);
            self.emit(Instr::LoadName(index), base);
            self.expression(value)?;
            let trimmed = operator.trim_end_matches('=');
            let index_of_op = self.name(trimmed);
            let op = operator_code(trimmed);
            self.emit(Instr::Binary { op, name: index_of_op }, base);
            self.emit(Instr::StoreName(index), base);
            return Ok(());
        }
        match target {
            Expr::Identifier { name, .. } => {
                self.expression(value)?;
                let index = self.name(name);
                self.emit(Instr::StoreName(index), base);
            }
            Expr::Property { receiver, name, .. } => {
                self.expression(value)?;
                self.expression(receiver)?;
                let index = self.name(name);
                self.emit(Instr::SetProperty(index), base);
            }
            Expr::Index { collection, key, .. } => {
                self.expression(value)?;
                self.expression(collection)?;
                self.expression(key)?;
                self.emit(Instr::SetIndex, base);
            }
            _ => return Err(Unsupported),
        }
        Ok(())
    }

    fn class_proto(&mut self, declaration: &Rc<tiger_ast::ClassDecl>) -> Result<u32, Unsupported> {
        let constructor = match &declaration.constructor {
            Some(constructor) => Some(chunk_of(&constructor.body, self.source.clone())?),
            None => None,
        };
        let mut methods = Vec::with_capacity(declaration.methods.len());
        for method in &declaration.methods {
            methods.push(chunk_of(&method.body, self.source.clone())?);
        }
        let index = self.classes.len() as u32;
        self.classes.push(Rc::new(ClassProto { declaration: declaration.clone(), constructor, methods }));
        Ok(index)
    }
}
