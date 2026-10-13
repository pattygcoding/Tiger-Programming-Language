//! Lowers a parsed program to bytecode for the optional linear engine.
//!
//! The compiler lowers the whole program or reports [`Unsupported`], in which
//! case the interpreter runs it instead. That fallback keeps the engine always
//! correct: a program it cannot lower is simply not compiled.

use std::collections::HashMap;
use std::rc::Rc;

use tiger_ast::{Base, Block, Expr, FunctionDecl, LiteralValue, Stmt};
use tiger_object::bytecode::{CatchProto, Chunk, ClassProto, FunctionProto, Instr, Op, SliceProto, TryProto};
use tiger_object::Value;

use crate::declares_in;

/// The names a method body's frame scope starts with: the engine binds both
/// before any parameter, so its parameters begin at slot 2.
const METHOD_PREFIX: [&str; 2] = ["this", "super"];

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
    chunk_of(statements, Rc::from(source), &[], None, false)
}

/// Compiles a block into a chunk that ends by returning the block's value.
///
/// `prefix` and `declaration` describe the bindings the engine makes before the
/// body runs — a method's `this` and `super`, then the parameters, the variadic
/// list, and the `**kwargs` dictionary — so the compiler can seed the frame scope
/// with exactly those names and give every declaration in the body the slot the
/// engine will give it. `escape_return` marks a chunk the engine runs as an
/// isolated block rather than as a function body.
fn chunk_of(
    statements: &[Stmt],
    source: Rc<str>,
    prefix: &[&str],
    declaration: Option<&FunctionDecl>,
    escape_return: bool,
) -> Result<Rc<Chunk>, Unsupported> {
    let base = statements.first().map(Stmt::base).cloned().unwrap_or(Base { line: 1, column: 1, source: source.clone() });
    let mut compiler = Compiler::new(source, prefix, declaration, escape_return);
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
    /// A `switch` target, which only a `break` can use: an unlabeled `continue`
    /// inside a `switch` belongs to the loop around it.
    is_switch: bool,
}

/// The names one lexical scope of a chunk declares, in declaration order.
///
/// A name's slot is its index here, and the runtime binds a scope's names in the
/// same order, so the two agree with no bookkeeping at run time: the compiler
/// just has to count every declaration, in source order.
struct ScopeFrame {
    /// Whether the scope's bindings appear in a fixed order at run time, so a
    /// name can be addressed by slot. A `switch` scope is not: which of its cases
    /// ran depends on the subject, so its names are addressed by name instead.
    stable: bool,
    names: Vec<Rc<str>>,
}

impl Default for ScopeFrame {
    fn default() -> ScopeFrame {
        ScopeFrame { stable: true, names: Vec::new() }
    }
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
    /// The lexical scopes of this chunk, outermost first. `hops` counts up from
    /// the innermost, so the frame's own scope is at index 0 and is the bottom of
    /// the running engine's scope stack.
    scopes: Vec<ScopeFrame>,
    iter_depth: usize,
    loops: Vec<LoopTarget>,
    tries: Vec<Rc<TryProto>>,
    slices: Vec<Rc<SliceProto>>,
    escape_return: bool,
}

impl Compiler {
    fn new(source: Rc<str>, prefix: &[&str], declaration: Option<&FunctionDecl>, escape_return: bool) -> Compiler {
        // The engine binds a method's `this` and `super`, a catch clause's caught
        // value, then the parameters in declaration order, the variadic list, and
        // the `**kwargs` dictionary, all before the body runs. Seeding the frame
        // scope with exactly those names gives every declaration in the body the
        // slot the engine will give it, which is what a slot-based access relies on.
        let mut frame = ScopeFrame::default();
        frame.names.extend(prefix.iter().map(|name| Rc::from(*name)));
        if let Some(declaration) = declaration {
            frame.names.extend(declaration.parameters.iter().cloned());
            if !declaration.variadic.is_empty() {
                frame.names.push(Rc::from(declaration.variadic.as_str()));
            }
            if !declaration.keyword.is_empty() {
                frame.names.push(Rc::from(declaration.keyword.as_str()));
            }
        }
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
            scopes: vec![frame],
            iter_depth: 0,
            loops: Vec::new(),
            tries: Vec::new(),
            slices: Vec::new(),
            escape_return,
        }
    }

    /// Enters a child scope, for the `PushScope` this compiler emits.
    fn enter_scope(&mut self) {
        self.scopes.push(ScopeFrame::default());
        self.scope_depth += 1;
    }

    /// Leaves the current scope, for its matching `PopScope`.
    fn leave_scope(&mut self) {
        self.scopes.pop();
        self.scope_depth -= 1;
    }

    /// Resolves a name to `(hops, slot)` within this chunk, or `None` when the
    /// name is not a local of this chunk and must be looked up the chain instead.
    fn resolve(&self, name: &str) -> Option<(u32, u32)> {
        let depth = self.scopes.len();
        for (index, scope) in self.scopes.iter().enumerate().rev() {
            if !scope.stable {
                continue;
            }
            if let Some(slot) = scope.names.iter().position(|declared| &**declared == name) {
                return Some(((depth - 1 - index) as u32, slot as u32));
            }
        }
        None
    }

    /// Records a declaration in the current scope, for a construct whose own
    /// instruction binds the name — a `class` or an `import` — so that the
    /// declarations after it in the same scope get the slots the engine gives them.
    fn record(&mut self, name: &str) {
        let scope = self.scopes.last_mut().expect("a scope is always open");
        if scope.stable {
            scope.names.push(Rc::from(name));
        }
    }

    /// Records a declaration in the current scope and emits the instruction that
    /// binds it: a slot-based one where the scope's layout is fixed, or a
    /// name-based one where it is not.
    fn define(&mut self, name: &str, constant: bool, base: &Base) {
        let index = self.name(name);
        if !self.scopes.last().expect("a scope is always open").stable {
            self.emit(Instr::DefineName { name: index, constant }, base);
            return;
        }
        let slot = {
            let scope = self.scopes.last_mut().expect("a scope is always open");
            let slot = scope.names.len() as u32;
            scope.names.push(Rc::from(name));
            slot
        };
        self.emit(Instr::DefineLocal { name: index, slot, constant }, base);
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
        let slots = self.code.len();
        Rc::new(Chunk {
            property_cache: std::cell::RefCell::new(vec![None; slots]),
            code: self.code,
            constants: self.constants,
            names: self.names,
            functions: self.functions,
            classes: self.classes,
            tries: self.tries,
            slices: self.slices,
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
                match self.resolve(name) {
                    Some((hops, slot)) => self.emit(Instr::LoadLocal { name: index, hops, slot }, base),
                    None => self.emit(Instr::LoadName(index), base),
                }
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
            // The bounds stay as expressions: the engine evaluates them against
            // the collection's length, exactly as the interpreter does.
            Expr::Slice { base, collection, start, stop, step } => {
                self.expression(collection)?;
                let index = self.slices.len() as u32;
                self.slices.push(Rc::new(SliceProto { start: start.clone(), stop: stop.clone(), step: step.clone() }));
                self.emit(Instr::Slice(index), base);
            }
            Expr::Property { base, receiver, name } => {
                self.expression(receiver)?;
                let index = self.name(name);
                self.emit(Instr::GetProperty(index), base);
            }
            Expr::Update { base, target, operator, prefix } => {
                let Expr::Identifier { name, .. } = &**target else { return Err(Unsupported) };
                let index = self.name(name);
                let decrement = operator == "--";
                match self.resolve(name) {
                    Some((hops, slot)) => self.emit(Instr::UpdateLocal { name: index, hops, slot, decrement, prefix: *prefix }, base),
                    None => self.emit(Instr::UpdateName { name: index, decrement, prefix: *prefix }, base),
                }
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
        let code = chunk_of(&declaration.body, self.source.clone(), &[], Some(declaration), false)?;
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
            self.enter_scope();
        }
        self.block(body)?;
        if declares {
            self.emit(Instr::PopScope, base);
            self.leave_scope();
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
                self.define(&declaration.name, false, &declaration.base);
            }
            Stmt::Class(declaration) => {
                let index = self.class_proto(declaration)?;
                self.emit(Instr::Class(index), &declaration.base);
                // The `Class` instruction binds the name itself; record it here so
                // the declarations that follow in this scope get the right slots.
                self.record(&declaration.name);
            }
            Stmt::Return { base, value } => {
                match value {
                    Some(value) => self.expression(value)?,
                    None => self.emit(Instr::Null, base),
                }
                // A `return` in an isolated block belongs to the enclosing
                // function, so it leaves the block instead of ending it.
                if self.escape_return {
                    self.emit(Instr::ReturnEscape, base);
                } else {
                    self.emit(Instr::Return, base);
                }
            }
            Stmt::Throw { base, value } => {
                self.expression(value)?;
                self.emit(Instr::Throw, base);
            }
            Stmt::Import { base, path, name } => {
                // The `Import` instruction binds the name itself; record it here so
                // the declarations that follow in this scope get the right slots.
                self.record(name);
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
                    is_switch: false,
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
                self.enter_scope();
                self.define(name, false, base);
                self.loops.push(LoopTarget {
                    label: label.clone(),
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth: self.scope_depth - 1,
                    continue_scope_depth: self.scope_depth - 1,
                    iter_depth: self.iter_depth - 1,
                    for_in: true,
                    is_switch: false,
                });
                self.block(body)?;
                self.emit(Instr::PopScope, base);
                self.leave_scope();
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
                self.enter_scope();
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
                    is_switch: false,
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
                self.leave_scope();
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
                    is_switch: false,
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
            Stmt::Switch { value, cases, .. } => {
                let base = statement.base();
                // A `switch` scope is not slot-addressed: which of its cases ran
                // depends on the subject, so a case body's declarations are bound
                // by name, and a name no case defines still reads through to the
                // enclosing scope.
                self.emit(Instr::PushScope, base);
                self.scopes.push(ScopeFrame { stable: false, names: Vec::new() });
                self.scope_depth += 1;
                self.expression(value)?;
                // A ladder of equality tests. The subject stays on the stack and
                // each case value is evaluated in order, only until one matches.
                let mut enter: Vec<Option<usize>> = Vec::with_capacity(cases.len());
                for case in cases {
                    match &case.value {
                        None => enter.push(None),
                        Some(case_value) => {
                            self.emit(Instr::Dup, base);
                            self.expression(case_value)?;
                            self.emit(Instr::Equal, base);
                            let to_next = self.here();
                            self.emit(Instr::JumpIfFalse(0), base);
                            self.emit(Instr::Pop, base);
                            let to_body = self.here();
                            self.emit(Instr::Jump(0), base);
                            let next = self.here();
                            self.patch(to_next, next);
                            enter.push(Some(to_body));
                        }
                    }
                }
                // Nothing matched: drop the subject and enter the last `default`,
                // which then falls through like any other case.
                self.emit(Instr::Pop, base);
                let no_match = self.here();
                self.emit(Instr::Jump(0), base);
                // The bodies, in order, falling through into each other. An
                // unlabeled `break` leaves the whole `switch`, so its target is the
                // scope outside it, and its pops take the `switch` scope with them.
                self.loops.push(LoopTarget {
                    label: String::new(),
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth: self.scope_depth - 1,
                    continue_scope_depth: self.scope_depth - 1,
                    iter_depth: self.iter_depth,
                    for_in: false,
                    is_switch: true,
                });
                let mut starts = Vec::with_capacity(cases.len());
                for (index, case) in cases.iter().enumerate() {
                    if let Some(to_body) = enter[index] {
                        let body = self.here();
                        self.patch(to_body, body);
                    }
                    starts.push(self.here());
                    self.block(&case.body)?;
                }
                let target = self.loops.pop().expect("loop target");
                let default = cases.iter().enumerate().rev().find(|(_, case)| case.value.is_none()).map(|(index, _)| starts[index]);
                self.emit(Instr::PopScope, base);
                self.scopes.pop();
                self.scope_depth -= 1;
                let end = self.here();
                self.patch(no_match, default.unwrap_or(end));
                for position in target.breaks {
                    self.patch(position, end);
                }
            }
            Stmt::Try { base, body, catches, finally } => {
                // Every part of a `try` runs as its own chunk. That is what lets
                // the engine catch an error the body raises, and what lets a
                // `return` inside any of the three parts unwind through the
                // `finally` on its way out of the enclosing function. A `break` or
                // `continue` that would leave the try is refused by the sub-chunk's
                // own loop check, so a `try` containing one still falls back to the
                // interpreter rather than being lowered wrongly.
                let body = chunk_of(body, self.source.clone(), &[], None, true)?;
                let mut clauses = Vec::with_capacity(catches.len());
                for clause in catches {
                    let code = chunk_of(&clause.body, self.source.clone(), &[clause.name.as_str()], None, true)?;
                    clauses.push(CatchProto { name: Rc::from(clause.name.as_str()), condition: clause.condition.clone(), body: code });
                }
                let finally = if finally.is_empty() { None } else { Some(chunk_of(finally, self.source.clone(), &[], None, true)?) };
                let index = self.tries.len() as u32;
                self.tries.push(Rc::new(TryProto { body, clauses, finally }));
                self.emit(Instr::Try(index), base);
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
                    // An unlabeled `break` leaves the innermost loop or `switch`;
                    // an unlabeled `continue` targets the innermost loop, stepping
                    // over any `switch` in between.
                    self.loops.iter().rposition(|target| kind == "break" || !target.is_switch)
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
            self.define(name, constant, base);
            return Ok(());
        }
        if operator != "=" {
            let Expr::Identifier { name, .. } = target else { return Err(Unsupported) };
            let index = self.name(name);
            match self.resolve(name) {
                Some((hops, slot)) => self.emit(Instr::LoadLocal { name: index, hops, slot }, base),
                None => self.emit(Instr::LoadName(index), base),
            }
            self.expression(value)?;
            let trimmed = operator.trim_end_matches('=');
            let index_of_op = self.name(trimmed);
            let op = operator_code(trimmed);
            self.emit(Instr::Binary { op, name: index_of_op }, base);
            match self.resolve(name) {
                Some((hops, slot)) => self.emit(Instr::StoreLocal { name: index, hops, slot }, base),
                None => self.emit(Instr::StoreName(index), base),
            }
            return Ok(());
        }
        match target {
            Expr::Identifier { name, .. } => {
                self.expression(value)?;
                let index = self.name(name);
                match self.resolve(name) {
                    Some((hops, slot)) => self.emit(Instr::StoreLocal { name: index, hops, slot }, base),
                    None => self.emit(Instr::StoreName(index), base),
                }
            }
            // A property or index target reports its errors at the target's own
            // position (the member name, or the `[`), as the interpreter does,
            // rather than at the whole assignment statement.
            Expr::Property { base: target, receiver, name } => {
                self.expression(value)?;
                self.expression(receiver)?;
                let index = self.name(name);
                self.emit(Instr::SetProperty(index), target);
            }
            Expr::Index { base: target, collection, key } => {
                self.expression(value)?;
                self.expression(collection)?;
                self.expression(key)?;
                self.emit(Instr::SetIndex, target);
            }
            _ => return Err(Unsupported),
        }
        Ok(())
    }

    fn class_proto(&mut self, declaration: &Rc<tiger_ast::ClassDecl>) -> Result<u32, Unsupported> {
        let constructor = match &declaration.constructor {
            Some(constructor) => Some(chunk_of(&constructor.body, self.source.clone(), &METHOD_PREFIX, Some(constructor), false)?),
            None => None,
        };
        let mut methods = Vec::with_capacity(declaration.methods.len());
        for method in &declaration.methods {
            methods.push(chunk_of(&method.body, self.source.clone(), &METHOD_PREFIX, Some(method), false)?);
        }
        let index = self.classes.len() as u32;
        self.classes.push(Rc::new(ClassProto { declaration: declaration.clone(), constructor, methods }));
        Ok(index)
    }
}
