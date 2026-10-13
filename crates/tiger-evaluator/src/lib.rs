//! Tree-walking interpreter for the Tiger language.
//!
//! A direct port of `pkg/evaluator`. Go's `panic`/`recover` control flow is
//! modelled with `Result<_, Signal>`: runtime errors, `throw`, `return`, and
//! `break`/`continue` all travel as signals, and `finally` runs on every path.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::rc::Rc;

use tiger_ast::{Base, Block, Expr, Stmt};
use tiger_object as object;
use tiger_object::{Env, Keyword, Module, RuntimeError, Value};

mod algo;
mod builtins;
mod compiler;
mod files;
mod methods;
mod modules;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod regex_engine;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod regex_stub;
mod regexmod;
mod vm;

pub use files::{parse_file_mode, FileMode, FileSystem, MemoryFileSystem, OsFileSystem};
pub use modules::{
    algo_module, json_module, math_module, random_module, regex_module, secrets_module, time_module, BundleLoader, FileLoader, ModuleLoader,
};

/// A non-local exit: a runtime error, a thrown value, or control flow.
pub enum Signal {
    Runtime { error: Rc<RuntimeError>, fatal: bool },
    Throw { value: Value, origin: String },
    Return(Value),
    Break(String),
    Continue(String),
}

/// Control flow produced by a statement.
#[derive(Clone)]
pub struct Flow {
    pub kind: FlowKind,
    pub label: String,
    pub value: Value,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FlowKind {
    Return,
    Break,
    Continue,
}

type Outcome = Result<Option<Flow>, Signal>;
type Eval = Result<Value, Signal>;

/// Classifies a diagnostic once, where it is raised.
const ERROR_KINDS: &[(&str, &str)] = &[
    ("division by zero", "zero division"),
    ("domain error", "math domain"),
    ("range error", "math range"),
    ("non-finite numeric result", "numeric range"),
    ("cannot reassign const", "constant reassignment"),
    ("already declared", "duplicate declaration"),
    ("undefined name", "undefined name"),
    ("index out of range", "index range"),
    ("index must be an integer", "index type"),
    ("dictionary key not found", "missing key"),
    ("pop: key not found", "missing key"),
    ("cannot be indexed", "not indexable"),
    ("is not supported for", "unsupported operands"),
    ("no such file", "missing file"),
    ("file does not exist", "missing file"),
];

fn error_kind(message: &str) -> &'static str {
    for (marker, kind) in ERROR_KINDS {
        if message.contains(marker) {
            return kind;
        }
    }
    "runtime error"
}

/// Builds a structured runtime error positioned at `base`.
pub fn new_runtime_error(base: &Base, message: String) -> Rc<RuntimeError> {
    Rc::new(RuntimeError { kind: error_kind(&message).to_string(), message, origin: base.location() })
}

pub(crate) fn runtime(base: &Base, message: String) -> Signal {
    Signal::Runtime { error: new_runtime_error(base, message), fatal: false }
}

fn fatal(base: &Base, message: String) -> Signal {
    Signal::Runtime { error: new_runtime_error(base, message), fatal: true }
}

/// Wraps a numeric result, rejecting a non-finite value the way the interpreter
/// does, so the arithmetic fast path stays a single call.
fn finite_number(base: &Base, value: f64) -> Eval {
    if value.is_infinite() || value.is_nan() {
        return Err(runtime(base, "non-finite numeric result".to_string()));
    }
    Ok(Value::Number(value))
}

/// Reports whether a block's own statements introduce a binding in its scope.
/// A body that only reads or assigns outer names can share one scope across
/// loop iterations, so a tight loop does not allocate a scope per iteration.
pub(crate) fn declares_in(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| match statement {
        Stmt::Import { .. } | Stmt::Destructure { .. } | Stmt::Function(_) | Stmt::Class(_) => true,
        Stmt::Assign { declaration, .. } => *declaration,
        _ => false,
    })
}

/// The scope a loop body runs in: a single shared scope when the body declares
/// nothing, or `None` to allocate a fresh scope per iteration.
fn loop_scope(body: &[Stmt], env: &Env) -> Option<Env> {
    if declares_in(body) {
        None
    } else {
        Some(object::Environment::new(Some(env.clone())))
    }
}

/// The scope a branch body runs in: the enclosing scope itself when the body
/// declares nothing (reads and writes reach it either way), so an `if`, `switch`,
/// or `try` pays no scope allocation in the common case.
fn branch_scope(body: &[Stmt], env: &Env) -> Env {
    if declares_in(body) {
        object::Environment::new(Some(env.clone()))
    } else {
        env.clone()
    }
}

/// Which execution engine runs a program.
///
/// `Auto` runs the compiled (linear) engine when a program lowers and falls back
/// to the tree-walking interpreter otherwise, so every program still runs.
/// `Tree` always walks the AST; the differential tests use it as the reference
/// the compiled engine must agree with.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Engine {
    #[default]
    Auto,
    Tree,
}

/// The step limit [`Evaluator::new`] starts with; `0` means unlimited.
pub const DEFAULT_MAX_STEPS: u64 = 1_000_000;

/// Which engine a run uses and how many steps it may take.
///
/// The two engines count steps differently — the compiled engine counts
/// instructions, the tree-walker counts statements — so the differential tests
/// run both with `max_steps` set to `0` and compare only the result.
#[derive(Clone, Copy)]
pub struct RunOptions {
    pub engine: Engine,
    pub max_steps: u64,
}

impl Default for RunOptions {
    fn default() -> RunOptions {
        RunOptions { engine: Engine::Auto, max_steps: DEFAULT_MAX_STEPS }
    }
}

/// The interpreter: host services, execution state, and module cache.
pub struct Evaluator {
    pub output: Box<dyn Write>,
    pub input: Option<Box<dyn BufRead>>,
    pub max_steps: u64,
    pub loader: Option<Rc<dyn ModuleLoader>>,
    pub source_path: String,
    /// Which engine a program runs on; `Auto` unless a caller forces one.
    pub engine: Engine,
    /// `None` means the operating system's filesystem, as in Go.
    pub filesystem: Option<Box<dyn FileSystem>>,
    modules: HashMap<String, Rc<Module>>,
    loading: Vec<String>,
    import_depth: usize,
    steps: u64,
    depth: usize,
    /// Call scopes freed by calls that did not capture them, reused to avoid a
    /// scope allocation per call.
    scope_pool: Vec<Env>,
    /// Value stacks freed by finished calls, reused so a call does not allocate.
    stack_pool: Vec<Vec<Value>>,
    /// Argument buffers freed by finished calls, reused so a call does not
    /// allocate a vector for the arguments it passes.
    args_pool: Vec<Vec<Value>>,
    keywords_pool: Vec<Vec<Keyword>>,
    /// Shared names for the bindings every method call creates, so binding them
    /// does not allocate.
    this_name: Rc<str>,
    super_name: Rc<str>,
}

impl Evaluator {
    pub fn new(output: Box<dyn Write>) -> Evaluator {
        Evaluator {
            output,
            input: None,
            max_steps: DEFAULT_MAX_STEPS,
            loader: None,
            source_path: String::new(),
            engine: Engine::Auto,
            filesystem: None,
            modules: HashMap::new(),
            loading: Vec::new(),
            import_depth: 0,
            steps: 0,
            depth: 0,
            scope_pool: Vec::new(),
            stack_pool: Vec::new(),
            args_pool: Vec::new(),
            keywords_pool: Vec::new(),
            this_name: Rc::from("this"),
            super_name: Rc::from("super"),
        }
    }

    /// Takes a scope for a call, reusing a pooled one when available.
    fn acquire_scope(&mut self, parent: Env) -> Env {
        match self.scope_pool.pop() {
            Some(scope) => {
                let mut borrowed = scope.borrow_mut();
                borrowed.parent = Some(parent);
                borrowed.access_class = None;
                borrowed.source_path.clear();
                drop(borrowed);
                scope
            }
            None => object::Environment::new(Some(parent)),
        }
    }

    /// Returns a scope to the pool when the call did not capture it, so the next
    /// call reuses it instead of allocating. A scope a closure captured (its `Rc`
    /// is shared) is left to the closure.
    fn release_scope(&mut self, scope: Env) {
        if Rc::strong_count(&scope) == 1 {
            scope.borrow_mut().bindings.clear();
            self.scope_pool.push(scope);
        }
    }

    /// Parses and executes a program, returning a formatted error on failure.
    pub fn execute(&mut self, program: &tiger_ast::Program) -> Result<(), String> {
        self.steps = 0;
        self.depth = 0;
        self.modules.clear();
        self.loading.clear();
        self.import_depth = 0;
        if !self.source_path.is_empty() {
            self.loading.push(self.source_path.clone());
        }
        let builtins = object::Environment::new(None);
        builtins::install(&builtins);
        let env = object::Environment::new(Some(builtins));
        env.borrow_mut().source_path = self.source_path.clone();
        // Run the linear engine when the program lowers, and fall back to the
        // tree-walking interpreter otherwise, so any program still runs. A caller
        // can force the tree-walker, which the differential tests use as the
        // reference for the compiled engine.
        let outcome: Result<(), Signal> = match self.engine {
            Engine::Tree => self.block(&program.statements, &env).map(|_| ()),
            Engine::Auto => match compiler::compile(&program.statements, &self.source_path) {
                Ok(chunk) => self.run_chunk(&chunk, &env).map(|_| ()),
                Err(compiler::Unsupported) => self.block(&program.statements, &env).map(|_| ()),
            },
        };
        match outcome {
            Ok(()) => Ok(()),
            Err(Signal::Runtime { error, .. }) => Err(error.text()),
            Err(Signal::Throw { value, origin }) => Err(format!("{}: uncaught throw: {}", origin, object::format(&value))),
            Err(_) => Ok(()),
        }
    }

    /// The active filesystem, defaulting to the operating system's one.
    fn files(&mut self) -> &mut dyn FileSystem {
        if self.filesystem.is_none() {
            self.filesystem = Some(Box::new(OsFileSystem));
        }
        self.filesystem.as_mut().unwrap().as_mut()
    }
}

/// Parses and executes a program with the host services a caller provides.
///
/// The Rust counterpart of Go's `evaluator.RunWithInput`: a `None` filesystem
/// means the operating system's, and a `None` loader rejects every `import`.
pub fn run_with_input(
    source: &str,
    filename: &str,
    loader: Option<Rc<dyn ModuleLoader>>,
    filesystem: Option<Box<dyn FileSystem>>,
    input: Option<Box<dyn BufRead>>,
    output: Box<dyn Write>,
) -> Result<(), String> {
    run_with_options(source, filename, loader, filesystem, input, output, RunOptions::default())
}

/// Parses and executes a program with the host services and options a caller
/// provides.
///
/// The counterpart of [`run_with_input`] that also pins the engine and the step
/// limit, which the differential tests use to run every fixture under
/// [`Engine::Tree`] with no step limit and compare it against the compiled
/// engine's result.
pub fn run_with_options(
    source: &str,
    filename: &str,
    loader: Option<Rc<dyn ModuleLoader>>,
    filesystem: Option<Box<dyn FileSystem>>,
    input: Option<Box<dyn BufRead>>,
    output: Box<dyn Write>,
    options: RunOptions,
) -> Result<(), String> {
    let program = tiger_parser::parse_source(source, filename)?;
    let mut evaluator = Evaluator::new(output);
    evaluator.source_path = filename.to_string();
    evaluator.loader = loader;
    evaluator.filesystem = filesystem;
    evaluator.input = input;
    evaluator.engine = options.engine;
    evaluator.max_steps = options.max_steps;
    evaluator.execute(&program)
}

impl object::Host for Evaluator {
    fn output(&mut self, text: &str) -> Result<(), String> {
        self.output.write_all(text.as_bytes()).map_err(|error| error.to_string())
    }

    fn input_line(&mut self, prompt: Option<&str>) -> Result<String, String> {
        if let Some(prompt) = prompt {
            self.output(prompt)?;
        }
        let Some(reader) = self.input.as_mut() else {
            return Err("input is unavailable: no input stream is connected".to_string());
        };
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => Err("EOF when reading a line".to_string()),
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Ok(line)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn file_exists(&mut self, name: &str) -> Result<bool, String> {
        self.files().exists(name)
    }

    fn load_file(&mut self, name: &str) -> Result<Vec<u8>, String> {
        self.files().load(name)
    }

    fn persist_file(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        self.files().persist(name, data)
    }

    fn remove_file(&mut self, name: &str) -> Result<(), String> {
        self.files().remove(name)
    }

    fn list_dir(&mut self, name: &str) -> Result<Vec<String>, String> {
        self.files().list(name)
    }
}

/// A resolved assignment target, so reads and writes can happen separately.
enum Target {
    Name { base: Base, name: String },
    Property { base: Base, instance: Rc<object::Instance>, name: String },
    ListIndex { elements: Rc<std::cell::RefCell<Vec<Value>>>, index: usize },
    DictIndex { base: Base, dict: Rc<std::cell::RefCell<object::Dict>>, key: Value },
}

impl Evaluator {
    fn tick(&mut self, base: &Base) -> Result<(), Signal> {
        self.steps += 1;
        if self.max_steps > 0 && self.steps > self.max_steps {
            return Err(fatal(base, "execution step limit exceeded".to_string()));
        }
        Ok(())
    }

    fn block(&mut self, statements: &[Stmt], env: &Env) -> Outcome {
        for statement in statements {
            if let Some(flow) = self.statement(statement, env)? {
                return Ok(Some(flow));
            }
        }
        Ok(None)
    }

    fn statement(&mut self, statement: &Stmt, env: &Env) -> Outcome {
        self.steps += 1;
        if self.max_steps > 0 && self.steps > self.max_steps {
            return Err(fatal(statement.base(), "execution step limit exceeded".to_string()));
        }
        match statement {
            Stmt::Import { base, path, name } => {
                let module = self.import_module(base, path, env)?;
                object::define(env, name, Value::Module(module), true).map_err(|message| runtime(base, message))?;
            }
            Stmt::Throw { base, value } => {
                let value = self.expression(value, env)?;
                return Err(Signal::Throw { value, origin: base.location() });
            }
            Stmt::Try { .. } => return self.try_statement(statement, env),
            Stmt::Destructure { .. } => return self.destructure(statement, env),
            Stmt::Expression { value, .. } => {
                self.expression(value, env)?;
            }
            Stmt::Assign { base, target, value, operator, constant, declaration } => {
                if *declaration || *constant {
                    let computed = self.expression(value, env)?;
                    let Expr::Identifier { name, .. } = target else { unreachable!("declaration target") };
                    object::define(env, name, computed, *constant).map_err(|message| runtime(base, message))?;
                } else if operator != "=" {
                    let op = operator.trim_end_matches('=');
                    // An identifier target needs no `Target`, so a plain `x += 1`
                    // reads and writes the name without cloning it.
                    if let Expr::Identifier { name, .. } = target {
                        let left = object::get(env, name).map_err(|message| runtime(base, message))?;
                        let right = self.expression(value, env)?;
                        let combined = self.binary(base, op, left, right)?;
                        object::assign(env, name, combined).map_err(|message| runtime(base, message))?;
                    } else {
                        let reference = self.reference(target, env)?;
                        let left = self.read_target(&reference, env)?;
                        let right = self.expression(value, env)?;
                        let combined = self.binary(base, op, left, right)?;
                        self.write_target(&reference, combined, env)?;
                    }
                } else if let Expr::Identifier { name, .. } = target {
                    let computed = self.expression(value, env)?;
                    object::assign(env, name, computed).map_err(|message| runtime(base, message))?;
                } else {
                    let computed = self.expression(value, env)?;
                    let reference = self.reference(target, env)?;
                    self.write_target(&reference, computed, env)?;
                }
            }
            Stmt::Function(declaration) => {
                let function =
                    Value::Function(Rc::new(object::FunctionValue { declaration: declaration.clone(), env: env.clone(), code: None }));
                object::define(env, &declaration.name, function, false).map_err(|message| runtime(&declaration.base, message))?;
            }
            Stmt::Class(declaration) => {
                self.define_class(declaration, env)?;
            }
            Stmt::Return { value, .. } => {
                let value = match value {
                    Some(value) => self.expression(value, env)?,
                    None => Value::Null,
                };
                return Ok(Some(Flow { kind: FlowKind::Return, label: String::new(), value }));
            }
            Stmt::Control { kind, label, .. } => {
                let flow = if kind == "break" { FlowKind::Break } else { FlowKind::Continue };
                return Ok(Some(Flow { kind: flow, label: label.clone(), value: Value::Null }));
            }
            Stmt::If { branches, else_block, .. } => {
                for branch in branches {
                    if object::truthy(&self.expression(&branch.condition, env)?) {
                        return self.block(&branch.body, &branch_scope(&branch.body, env));
                    }
                }
                return self.block(else_block, &branch_scope(else_block, env));
            }
            Stmt::While { label, condition, body, else_block, .. } => {
                let shared = loop_scope(body, env);
                while object::truthy(&self.expression(condition, env)?) {
                    let scope = match &shared {
                        Some(scope) => scope.clone(),
                        None => object::Environment::new(Some(env.clone())),
                    };
                    if let Some(result) = self.block(body, &scope)? {
                        match loop_control(label, &result) {
                            (_, Some(propagate)) => return Ok(Some(propagate)),
                            (true, None) => return Ok(None),
                            (false, None) => {}
                        }
                    }
                }
                return self.block(else_block, &object::Environment::new(Some(env.clone())));
            }
            Stmt::For { base, label, name, iterable, body, else_block } => {
                let iterable = self.expression(iterable, env)?;
                let values = self.iterable_values(base, &iterable)?;
                for value in values {
                    self.tick(base)?;
                    // The loop variable lives in a pooled scope; the pool keeps it
                    // out of a captured scope, so per-iteration bindings still work.
                    let scope = self.acquire_scope(env.clone());
                    let outcome = match object::define(&scope, name, value, false) {
                        Ok(()) => self.block(body, &scope),
                        Err(message) => Err(runtime(base, message)),
                    };
                    self.release_scope(scope);
                    if let Some(result) = outcome? {
                        match loop_control(label, &result) {
                            (_, Some(propagate)) => return Ok(Some(propagate)),
                            (true, None) => return Ok(None),
                            (false, None) => {}
                        }
                    }
                }
                return self.block(else_block, &object::Environment::new(Some(env.clone())));
            }
            Stmt::CFor { base, label, initializer, condition, update, body, else_block } => {
                let scope = object::Environment::new(Some(env.clone()));
                if let Some(initializer) = initializer {
                    self.statement(initializer, &scope)?;
                }
                let shared = loop_scope(body, &scope);
                loop {
                    if let Some(condition) = condition {
                        if !object::truthy(&self.expression(condition, &scope)?) {
                            break;
                        }
                    }
                    self.tick(base)?;
                    let body_scope = match &shared {
                        Some(scope) => scope.clone(),
                        None => object::Environment::new(Some(scope.clone())),
                    };
                    if let Some(result) = self.block(body, &body_scope)? {
                        match loop_control(label, &result) {
                            (_, Some(propagate)) => return Ok(Some(propagate)),
                            (true, None) => return Ok(None),
                            (false, None) => {}
                        }
                    }
                    if let Some(update) = update {
                        self.statement(update, &scope)?;
                    }
                }
                return self.block(else_block, &object::Environment::new(Some(scope)));
            }
            Stmt::DoWhile { base, label, condition, body } => {
                let shared = loop_scope(body, env);
                loop {
                    self.tick(base)?;
                    let scope = match &shared {
                        Some(scope) => scope.clone(),
                        None => object::Environment::new(Some(env.clone())),
                    };
                    if let Some(result) = self.block(body, &scope)? {
                        match loop_control(label, &result) {
                            (_, Some(propagate)) => return Ok(Some(propagate)),
                            (true, None) => return Ok(None),
                            (false, None) => {}
                        }
                    }
                    if !object::truthy(&self.expression(condition, env)?) {
                        return Ok(None);
                    }
                }
            }
            Stmt::Switch { value, cases, .. } => {
                let value = self.expression(value, env)?;
                let mut start: Option<usize> = None;
                let mut fallback: Option<usize> = None;
                for (index, case) in cases.iter().enumerate() {
                    match &case.value {
                        None => fallback = Some(index),
                        Some(case_value) => {
                            if object::equal(&value, &self.expression(case_value, env)?) {
                                start = Some(index);
                                break;
                            }
                        }
                    }
                }
                let Some(start) = start.or(fallback) else { return Ok(None) };
                let declares = cases[start..].iter().any(|case| declares_in(&case.body));
                let scope = if declares { object::Environment::new(Some(env.clone())) } else { env.clone() };
                for case in &cases[start..] {
                    if let Some(result) = self.block(&case.body, &scope)? {
                        if result.kind == FlowKind::Break && result.label.is_empty() {
                            return Ok(None);
                        }
                        return Ok(Some(result));
                    }
                }
            }
        }
        Ok(None)
    }
}

/// Interprets a flow value that reached a loop body.
fn loop_control(label: &str, result: &Flow) -> (bool, Option<Flow>) {
    match result.kind {
        FlowKind::Break => {
            if result.label.is_empty() || result.label == label {
                (true, None)
            } else {
                (false, Some(result.clone()))
            }
        }
        FlowKind::Continue => {
            if result.label.is_empty() || result.label == label {
                (false, None)
            } else {
                (false, Some(result.clone()))
            }
        }
        FlowKind::Return => (false, Some(result.clone())),
    }
}

impl Evaluator {
    fn expression(&mut self, expression: &Expr, env: &Env) -> Eval {
        self.steps += 1;
        if self.max_steps > 0 && self.steps > self.max_steps {
            return Err(fatal(expression.base(), "execution step limit exceeded".to_string()));
        }
        // Identifiers and literals are the hottest nodes and cannot recurse, so
        // they skip the depth bookkeeping that guards the recursive walk.
        match expression {
            Expr::Identifier { base, name } => return object::get(env, name).map_err(|message| runtime(base, message)),
            Expr::Literal { value, .. } => {
                return Ok(match value {
                    tiger_ast::LiteralValue::Number(number) => Value::Number(*number),
                    tiger_ast::LiteralValue::String(text) => Value::string(text),
                    tiger_ast::LiteralValue::Bool(flag) => Value::Bool(*flag),
                    tiger_ast::LiteralValue::Null => Value::Null,
                });
            }
            _ => {}
        }
        if self.depth >= 512 {
            return Err(fatal(expression.base(), "maximum evaluation depth exceeded".to_string()));
        }
        self.depth += 1;
        let result = self.expression_body(expression, env);
        self.depth -= 1;
        result
    }

    fn expression_body(&mut self, expression: &Expr, env: &Env) -> Eval {
        match expression {
            Expr::FormattedString { base, parts } => {
                let mut text = String::new();
                for part in parts {
                    match part {
                        Expr::Interpolation { value, spec, .. } => {
                            let computed = self.expression(value, env)?;
                            text.push_str(&self.apply_format(base, spec, &computed)?);
                        }
                        other => {
                            let computed = self.expression(other, env)?;
                            text.push_str(&object::format(&computed));
                        }
                    }
                }
                Ok(Value::string(&text))
            }
            Expr::Literal { value, .. } => Ok(match value {
                tiger_ast::LiteralValue::Number(number) => Value::Number(*number),
                tiger_ast::LiteralValue::String(text) => Value::string(text),
                tiger_ast::LiteralValue::Bool(flag) => Value::Bool(*flag),
                tiger_ast::LiteralValue::Null => Value::Null,
            }),
            Expr::Identifier { base, name } => object::get(env, name).map_err(|message| runtime(base, message)),
            Expr::Super { base } => match object::get(env, "super") {
                Ok(value) => Ok(value),
                Err(_) => Err(runtime(base, "super is only available inside methods".to_string())),
            },
            Expr::Property { base, receiver, name } => {
                let receiver = self.expression(receiver, env)?;
                self.property(base, &receiver, name, env)
            }
            Expr::List { elements, .. } => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    values.push(self.expression(element, env)?);
                }
                Ok(Value::list(values))
            }
            Expr::Dict { base, pairs } => {
                let dict = Rc::new(std::cell::RefCell::new(object::Dict::default()));
                for pair in pairs {
                    let key = self.expression(&pair.key, env)?;
                    let value = self.expression(&pair.value, env)?;
                    dict.borrow_mut().set(key, value).map_err(|message| runtime(base, message))?;
                }
                Ok(Value::Dict(dict))
            }
            Expr::Function(declaration) => {
                Ok(Value::Function(Rc::new(object::FunctionValue { declaration: declaration.clone(), env: env.clone(), code: None })))
            }
            Expr::Unary { base, operator, right } => {
                let right = self.expression(right, env)?;
                if operator == "not" {
                    return Ok(Value::Bool(!object::truthy(&right)));
                }
                let Value::Number(number) = right else {
                    return Err(runtime(base, format!("operator {} requires a number", operator)));
                };
                if operator == "-" {
                    return Ok(Value::Number(-number));
                }
                if operator == "~" {
                    return Ok(Value::Number(!self.bitwise_integer(base, number)? as f64));
                }
                Ok(Value::Number(number))
            }
            Expr::Update { base, target, operator, prefix } => {
                // An identifier target needs no `Target`, so `++`/`--` updates the
                // name without cloning it.
                if let Expr::Identifier { name, .. } = &**target {
                    let previous = match object::get(env, name).map_err(|message| runtime(base, message))? {
                        Value::Number(number) => number,
                        _ => return Err(runtime(base, format!("{} requires a number", operator))),
                    };
                    let updated = if operator == "--" { previous - 1.0 } else { previous + 1.0 };
                    if updated.is_infinite() || updated.is_nan() {
                        return Err(runtime(base, "non-finite numeric result".to_string()));
                    }
                    object::assign(env, name, Value::Number(updated)).map_err(|message| runtime(base, message))?;
                    return Ok(if *prefix { Value::Number(updated) } else { Value::Number(previous) });
                }
                let reference = self.reference(target, env)?;
                let previous = match self.read_target(&reference, env)? {
                    Value::Number(number) => number,
                    _ => return Err(runtime(base, format!("{} requires a number", operator))),
                };
                let updated = if operator == "--" { previous - 1.0 } else { previous + 1.0 };
                if updated.is_infinite() || updated.is_nan() {
                    return Err(runtime(base, "non-finite numeric result".to_string()));
                }
                self.write_target(&reference, Value::Number(updated), env)?;
                if *prefix {
                    Ok(Value::Number(updated))
                } else {
                    Ok(Value::Number(previous))
                }
            }
            Expr::Binary { base, left, operator, right } => {
                let left = self.expression(left, env)?;
                if operator == "and" {
                    if !object::truthy(&left) {
                        return Ok(left);
                    }
                    return self.expression(right, env);
                }
                if operator == "or" {
                    if object::truthy(&left) {
                        return Ok(left);
                    }
                    return self.expression(right, env);
                }
                let right = self.expression(right, env)?;
                self.binary(base, operator, left, right)
            }
            Expr::Conditional { condition, then, otherwise, .. } => {
                if object::truthy(&self.expression(condition, env)?) {
                    self.expression(then, env)
                } else {
                    self.expression(otherwise, env)
                }
            }
            Expr::Interpolation { .. } => Err(runtime(expression.base(), "unsupported expression".to_string())),
            Expr::Slice { base, collection, start, stop, step } => {
                let collection = self.expression(collection, env)?;
                self.slice_value(base, collection, start, stop, step, env)
            }
            Expr::Index { base, collection, key } => {
                let collection = self.expression(collection, env)?;
                let key = self.expression(key, env)?;
                match &collection {
                    Value::List(elements) => {
                        let index = self.index_at(base, &key, elements.borrow().len())?;
                        Ok(elements.borrow()[index].clone())
                    }
                    Value::String(text) => {
                        if text.is_ascii() {
                            let index = self.index_at(base, &key, text.len())?;
                            Ok(Value::string(&text[index..index + 1]))
                        } else {
                            let runes: Vec<char> = text.chars().collect();
                            let index = self.index_at(base, &key, runes.len())?;
                            Ok(Value::string(&runes[index].to_string()))
                        }
                    }
                    Value::Dict(dict) => match dict.borrow().get(&key) {
                        Ok(Some(value)) => Ok(value),
                        Ok(None) => Err(runtime(base, format!("dictionary key not found: {}", object::format(&key)))),
                        Err(message) => Err(runtime(base, message)),
                    },
                    Value::Error(error) => self.error_field(base, error, &key),
                    other => Err(runtime(base, format!("{} cannot be indexed", other.type_name()))),
                }
            }
            Expr::Call { base, function, arguments, keywords } => {
                let function = self.expression(function, env)?;
                // The common call passes a few arguments with no keywords and no
                // spread, so evaluate them into a stack buffer instead of a heap
                // vector. The buffer stays local, so the callee's own calls cannot
                // invalidate it.
                if keywords.is_empty() && arguments.len() <= 4 && !arguments.iter().any(|argument| argument.spread) {
                    let mut buffer: [Value; 4] = std::array::from_fn(|_| Value::Null);
                    for (index, argument) in arguments.iter().enumerate() {
                        buffer[index] = self.expression(&argument.value, env)?;
                    }
                    return self.call(base, function, &buffer[..arguments.len()], &[], env);
                }
                let mut positional = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    let value = self.expression(&argument.value, env)?;
                    if !argument.spread {
                        positional.push(value);
                        continue;
                    }
                    match value {
                        Value::List(elements) => positional.extend(elements.borrow().iter().cloned()),
                        other => {
                            return Err(runtime(base, format!("cannot unpack {} in a call; expected a list", other.type_name())));
                        }
                    }
                }
                let mut named: Vec<Keyword> = Vec::with_capacity(keywords.len());
                let mut seen: Vec<String> = Vec::new();
                for entry in keywords {
                    let value = self.expression(&entry.value, env)?;
                    if !entry.spread {
                        if seen.contains(&entry.name) {
                            return Err(runtime(base, format!("duplicate keyword argument {:?}", entry.name)));
                        }
                        seen.push(entry.name.clone());
                        named.push(Keyword { name: entry.name.clone(), value });
                        continue;
                    }
                    match value {
                        Value::Dict(dict) => {
                            let entries: Vec<(Value, Value)> = dict.borrow().entries.clone();
                            for (key, item) in entries {
                                match key {
                                    Value::String(name) => {
                                        let name = name.to_string();
                                        if seen.contains(&name) {
                                            return Err(runtime(base, format!("duplicate keyword argument {:?}", name)));
                                        }
                                        seen.push(name.clone());
                                        named.push(Keyword { name, value: item });
                                    }
                                    other => {
                                        return Err(runtime(
                                            base,
                                            format!("keyword argument names must be strings, got {}", other.type_name()),
                                        ));
                                    }
                                }
                            }
                        }
                        other => {
                            return Err(runtime(base, format!("cannot unpack {} in a call; expected a dictionary", other.type_name())));
                        }
                    }
                }
                self.call(base, function, &positional, &named, env)
            }
        }
    }
}

impl Evaluator {
    fn reference(&mut self, target: &Expr, env: &Env) -> Result<Target, Signal> {
        match target {
            Expr::Identifier { base, name } => Ok(Target::Name { base: base.clone(), name: name.clone() }),
            Expr::Property { base, receiver, name } => {
                let receiver = self.expression(receiver, env)?;
                match receiver {
                    Value::Instance(instance) => Ok(Target::Property { base: base.clone(), instance, name: name.clone() }),
                    other => Err(runtime(base, format!("cannot assign a property of {}", other.type_name()))),
                }
            }
            Expr::Index { base, collection, key } => {
                let collection = self.expression(collection, env)?;
                let key = self.expression(key, env)?;
                match collection {
                    Value::List(elements) => {
                        let index = self.index_at(base, &key, elements.borrow().len())?;
                        Ok(Target::ListIndex { elements, index })
                    }
                    Value::Dict(dict) => Ok(Target::DictIndex { base: base.clone(), dict, key }),
                    other => Err(runtime(base, format!("cannot assign an index of {}", other.type_name()))),
                }
            }
            other => Err(runtime(other.base(), "invalid assignment target".to_string())),
        }
    }

    fn read_target(&mut self, target: &Target, env: &Env) -> Eval {
        match target {
            Target::Name { base, name } => object::get(env, name).map_err(|message| runtime(base, message)),
            Target::Property { base, instance, name } => self.property(base, &Value::Instance(instance.clone()), name, env),
            Target::ListIndex { elements, index } => Ok(elements.borrow()[*index].clone()),
            Target::DictIndex { base, dict, key } => match dict.borrow().get(key) {
                Ok(Some(value)) => Ok(value),
                Ok(None) => Err(runtime(base, format!("dictionary key not found: {}", object::format(key)))),
                Err(message) => Err(runtime(base, message)),
            },
        }
    }

    fn write_target(&mut self, target: &Target, value: Value, env: &Env) -> Result<(), Signal> {
        match target {
            Target::Name { base, name } => object::assign(env, name, value).map_err(|message| runtime(base, message)),
            Target::Property { base, instance, name } => {
                self.member_access(base, &instance.class, name, env, true)?;
                if instance.class.find_field(name).is_none() {
                    return Err(runtime(
                        base,
                        format!("{} has no field {:?}; declare it in the class with var or const", instance.class.name, name),
                    ));
                }
                instance.fields.borrow_mut().insert(name.clone(), value);
                Ok(())
            }
            Target::ListIndex { elements, index } => {
                elements.borrow_mut()[*index] = value;
                Ok(())
            }
            Target::DictIndex { base, dict, key } => dict.borrow_mut().set(key.clone(), value).map_err(|message| runtime(base, message)),
        }
    }

    fn error_field(&mut self, base: &Base, failure: &Rc<RuntimeError>, key: &Value) -> Eval {
        let Value::String(name) = key else {
            return Err(runtime(base, format!("error fields must be read with a string key, got {}", key.type_name())));
        };
        match name.as_ref() {
            "message" => Ok(Value::string(&failure.message)),
            "kind" => Ok(Value::string(&failure.kind)),
            "origin" => Ok(Value::string(&failure.origin)),
            other => Err(runtime(base, format!("error has no field {:?}", other))),
        }
    }

    fn iterable_values(&mut self, base: &Base, iterable: &Value) -> Result<Vec<Value>, Signal> {
        match iterable {
            Value::List(elements) => Ok(elements.borrow().clone()),
            Value::String(text) => Ok(text.chars().map(|ch| Value::string(&ch.to_string())).collect()),
            Value::Dict(dict) => Ok(dict.borrow().entries.iter().map(|(key, _)| key.clone()).collect()),
            other => Err(runtime(base, format!("{} is not iterable", other.type_name()))),
        }
    }

    fn index_at(&mut self, base: &Base, key: &Value, length: usize) -> Result<usize, Signal> {
        let Value::Number(number) = key else {
            return Err(runtime(base, "index must be an integer".to_string()));
        };
        if number.trunc() != *number {
            return Err(runtime(base, "index must be an integer".to_string()));
        }
        let mut number = *number;
        if number < 0.0 {
            number += length as f64;
        }
        if number < 0.0 || number >= length as f64 {
            return Err(runtime(base, "index out of range".to_string()));
        }
        Ok(number as usize)
    }

    /// Slices `collection`, with `start`/`stop`/`step` evaluated against its
    /// length. The interpreter's `Expr::Slice` case and the compiled engine's
    /// `Slice` instruction both come through here, so the two cannot drift apart.
    fn slice_value(
        &mut self,
        base: &Base,
        collection: Value,
        start: &Option<Box<Expr>>,
        stop: &Option<Box<Expr>>,
        step: &Option<Box<Expr>>,
        env: &Env,
    ) -> Eval {
        match &collection {
            Value::List(elements) => {
                let (start, stop, step) = self.slice_bounds(base, start, stop, step, elements.borrow().len(), env)?;
                let indices = slice_indices(start, stop, step);
                let borrowed = elements.borrow();
                Ok(Value::list(indices.iter().map(|index| borrowed[*index as usize].clone()).collect()))
            }
            Value::String(text) => {
                if text.is_ascii() {
                    let (start, stop, step) = self.slice_bounds(base, start, stop, step, text.len(), env)?;
                    let indices = slice_indices(start, stop, step);
                    let value: String = indices.iter().map(|index| text.as_bytes()[*index as usize] as char).collect();
                    Ok(Value::string(&value))
                } else {
                    let runes: Vec<char> = text.chars().collect();
                    let (start, stop, step) = self.slice_bounds(base, start, stop, step, runes.len(), env)?;
                    let indices = slice_indices(start, stop, step);
                    let value: String = indices.iter().map(|index| runes[*index as usize]).collect();
                    Ok(Value::string(&value))
                }
            }
            other => Err(runtime(base, format!("{} cannot be sliced", other.type_name()))),
        }
    }

    fn slice_bounds(
        &mut self,
        base: &Base,
        start: &Option<Box<Expr>>,
        stop: &Option<Box<Expr>>,
        step: &Option<Box<Expr>>,
        length: usize,
        env: &Env,
    ) -> Result<(i64, i64, i64), Signal> {
        let mut step_value: i64 = 1;
        if let Some(step) = step {
            let computed = self.expression(step, env)?;
            step_value = self.slice_integer(base, &computed)?;
            if step_value == 0 {
                return Err(runtime(base, "slice step cannot be zero".to_string()));
            }
        }
        let (mut low, mut high) = (0i64, length as i64);
        let mut start_value = 0i64;
        let mut stop_value = length as i64;
        if step_value < 0 {
            low = -1;
            high = length as i64 - 1;
            start_value = high;
            stop_value = low;
        }
        if let Some(start) = start {
            let computed = self.expression(start, env)?;
            let mut value = self.slice_integer(base, &computed)?;
            if value < 0 {
                value += length as i64;
            }
            start_value = value.clamp(low, high);
        }
        if let Some(stop) = stop {
            let computed = self.expression(stop, env)?;
            let mut value = self.slice_integer(base, &computed)?;
            if value < 0 {
                value += length as i64;
            }
            stop_value = value.clamp(low, high);
        }
        Ok((start_value, stop_value, step_value))
    }

    fn slice_integer(&mut self, base: &Base, value: &Value) -> Result<i64, Signal> {
        let Value::Number(number) = value else {
            return Err(runtime(base, "slice bounds must be integers".to_string()));
        };
        if number.trunc() != *number {
            return Err(runtime(base, "slice bounds must be integers".to_string()));
        }
        if *number <= -9.3e18 {
            return Ok(i64::MIN);
        }
        if *number >= 9.3e18 {
            return Ok(i64::MAX);
        }
        Ok(*number as i64)
    }

    fn bitwise_integer(&mut self, base: &Base, value: f64) -> Result<i64, Signal> {
        const LIMIT: f64 = 9223372036854775808.0;
        if value.is_nan() || value.is_infinite() || value < -LIMIT || value >= LIMIT {
            return Err(runtime(base, "bitwise operands must be within the 64-bit integer range".to_string()));
        }
        Ok(value as i64)
    }
}

fn slice_indices(start: i64, stop: i64, step: i64) -> Vec<i64> {
    let mut indices = Vec::new();
    if step > 0 {
        let mut index = start;
        while index < stop {
            indices.push(index);
            index += step;
        }
    } else {
        let mut index = start;
        while index > stop {
            indices.push(index);
            index += step;
        }
    }
    indices
}

impl Evaluator {
    fn call(&mut self, base: &Base, function: Value, arguments: &[Value], keywords: &[Keyword], env: &Env) -> Eval {
        match function {
            Value::Builtin(builtin) => {
                if builtin.higher_order {
                    return self.call_aggregate(base, &builtin.name, builtin.payload.clone(), arguments, keywords, env);
                }
                (builtin.call)(self, arguments, keywords).map_err(|message| runtime(base, message))
            }
            Value::Function(function) => self.invoke(base, &function, arguments, keywords, None),
            Value::BoundMethod(bound) => self.invoke(base, &bound.method.function, arguments, keywords, Some(&bound)),
            Value::Super(super_value) => {
                if !super_value.constructor {
                    return Err(runtime(base, "super(...) can only be called inside a constructor".to_string()));
                }
                let Some(parent) = super_value.parent.clone() else {
                    return Err(runtime(base, "super requires a parent class".to_string()));
                };
                if let Some(constructor) = parent.find_constructor() {
                    self.accessible(base, &constructor.function.declaration.access, &constructor.owner, env)?;
                }
                self.construct(base, &parent, &super_value.receiver, arguments, keywords)?;
                Ok(Value::Null)
            }
            Value::Class(class) => {
                let instance = Rc::new(object::Instance { class: class.clone(), fields: std::cell::RefCell::new(HashMap::new()) });
                if let Some(constructor) = class.find_constructor() {
                    self.accessible(base, &constructor.function.declaration.access, &constructor.owner, env)?;
                }
                self.initialize_fields(&instance, &class)?;
                self.construct(base, &class, &instance, arguments, keywords)?;
                Ok(Value::Instance(instance))
            }
            other => Err(runtime(base, format!("{} is not callable", other.type_name()))),
        }
    }
}

/// Names a function in diagnostics, covering anonymous functions. Borrows the
/// declaration's name so a call allocates nothing just to label an error.
fn function_label(declaration: &tiger_ast::FunctionDecl) -> &str {
    if declaration.name.is_empty() {
        "anonymous function"
    } else {
        &declaration.name
    }
}

/// Reports whether any parameter at or after `index` lacks a default.
fn missing_required(declaration: &tiger_ast::FunctionDecl, index: usize) -> bool {
    for position in index..declaration.parameters.len() {
        if position >= declaration.defaults.len() || declaration.defaults[position].is_none() {
            return true;
        }
    }
    false
}

impl Evaluator {
    fn invoke(
        &mut self,
        base: &Base,
        function: &Rc<object::FunctionValue>,
        arguments: &[Value],
        keywords: &[Keyword],
        bound: Option<&Rc<object::BoundMethod>>,
    ) -> Eval {
        let declaration = &function.declaration;
        let parameters = &declaration.parameters;
        let fixed = parameters.len();
        let label = function_label(declaration);
        if arguments.len() > fixed && declaration.variadic.is_empty() {
            return Err(runtime(base, format!("{} expects {} arguments, got {}", label, fixed, arguments.len())));
        }
        if arguments.len() < fixed
            && declaration.variadic.is_empty()
            && keywords.is_empty()
            && missing_required(declaration, arguments.len())
        {
            return Err(runtime(base, format!("{} expects {} arguments, got {}", label, fixed, arguments.len())));
        }
        // A call scope is pooled and reused when the call does not capture it, so
        // a recursive or repeated call does not allocate a scope and its bindings
        // each time.
        let scope = self.acquire_scope(function.env.clone());
        let result = match &function.code {
            Some(code) => self.run_compiled(base, function, arguments, keywords, bound, &scope, code.clone()),
            None => self.run_call(base, function, arguments, keywords, bound, &scope),
        };
        self.release_scope(scope);
        // A `return` inside a `try` leaves its sub-chunk as a `Signal::Return` and
        // unwinds to here, which is the function boundary: it becomes the result.
        match result {
            Err(Signal::Return(value)) => Ok(value),
            other => other,
        }
    }

    /// Binds a call's arguments and runs the body with the linear engine.
    fn run_compiled(
        &mut self,
        base: &Base,
        function: &Rc<object::FunctionValue>,
        arguments: &[Value],
        keywords: &[Keyword],
        bound: Option<&Rc<object::BoundMethod>>,
        scope: &Env,
        code: Rc<tiger_object::bytecode::Chunk>,
    ) -> Eval {
        self.depth += 1;
        if self.depth >= 512 {
            self.depth -= 1;
            return Err(fatal(base, "maximum evaluation depth exceeded".to_string()));
        }
        let result = self.bind_call(base, function, arguments, keywords, bound, scope).and_then(|()| self.run_chunk(&code, scope));
        self.depth -= 1;
        result
    }

    /// Binds the call's `this`/`super`, parameters, keywords, and defaults in
    /// `scope`, so either engine can then run the body.
    ///
    /// The bindings land in a fixed order — `this`, `super`, the parameters in
    /// declaration order, the variadic list, then the `**kwargs` dictionary — so a
    /// parameter's position in the scope is known when the body is compiled.
    fn bind_call(
        &mut self,
        base: &Base,
        function: &Rc<object::FunctionValue>,
        arguments: &[Value],
        keywords: &[Keyword],
        bound: Option<&Rc<object::BoundMethod>>,
        scope: &Env,
    ) -> Result<(), Signal> {
        let scope = scope.clone();
        let declaration = &function.declaration;
        let parameters = &declaration.parameters;
        let fixed = parameters.len();
        let label = function_label(declaration);
        if let Some(bound) = bound {
            scope.borrow_mut().access_class = Some(bound.method.owner.clone());
            object::define_shared(&scope, &self.this_name, Value::Instance(bound.receiver.clone()), true)
                .map_err(|message| runtime(base, message))?;
            object::define_shared(
                &scope,
                &self.super_name,
                Value::Super(Rc::new(object::Super {
                    parent: bound.method.owner.parent.clone(),
                    receiver: bound.receiver.clone(),
                    constructor: bound.method.constructor,
                })),
                true,
            )
            .map_err(|message| runtime(base, message))?;
        }
        // Resolve each parameter's value in declaration order and bind them in
        // that same order: a positional argument, then keyword, then default. A
        // fixed order is what lets the compiled engine address a parameter by
        // slot, since a parameter's slot is its binding's position in the scope.
        // Positional arguments fill their slots first, so a keyword argument never
        // moves an earlier parameter.
        //
        // Keyword arguments, and the `**kwargs` dictionary, are only materialized
        // when the call actually passes keywords, so the common call is a straight
        // pass over the parameters and allocates nothing.
        let mut collected: Option<Rc<std::cell::RefCell<object::Dict>>> = None;
        if keywords.is_empty() {
            // Positional arguments fill their slots first, then the remaining
            // parameters take their defaults, in declaration order.
            let bound = arguments.len().min(fixed);
            for (index, parameter) in parameters.iter().take(bound).enumerate() {
                object::define_shared(&scope, parameter, arguments[index].clone(), false).map_err(|message| runtime(base, message))?;
            }
            for (index, parameter) in parameters.iter().enumerate().skip(bound) {
                object::define_shared(&scope, parameter, self.default_argument(base, function, index)?, false)
                    .map_err(|message| runtime(base, message))?;
            }
        } else {
            // Resolve every parameter's value first — positional, then keyword,
            // then default — so the bindings land in declaration order however the
            // call orders its keywords.
            let mut values: Vec<Option<Value>> = (0..fixed).map(|index| arguments.get(index).cloned()).collect();
            for entry in keywords {
                if let Some(position) = parameters.iter().position(|parameter| &**parameter == entry.name.as_str()) {
                    if values[position].is_some() {
                        return Err(runtime(base, format!("{} got multiple values for argument {:?}", label, entry.name)));
                    }
                    values[position] = Some(entry.value.clone());
                    continue;
                }
                if declaration.keyword.is_empty() {
                    return Err(runtime(base, format!("{} got an unexpected keyword argument {:?}", label, entry.name)));
                }
                let dict = collected.get_or_insert_with(|| Rc::new(std::cell::RefCell::new(object::Dict::default())));
                let existing = dict.borrow().get(&Value::string(&entry.name)).map_err(|message| runtime(base, message))?;
                if existing.is_some() {
                    return Err(runtime(base, format!("{} got multiple values for keyword argument {:?}", label, entry.name)));
                }
                dict.borrow_mut().set(Value::string(&entry.name), entry.value.clone()).map_err(|message| runtime(base, message))?;
            }
            for (index, parameter) in parameters.iter().enumerate() {
                let value = match values[index].take() {
                    Some(value) => value,
                    None => self.default_argument(base, function, index)?,
                };
                object::define_shared(&scope, parameter, value, false).map_err(|message| runtime(base, message))?;
            }
        }
        if !declaration.variadic.is_empty() {
            let extra: Vec<Value> = if arguments.len() > fixed { arguments[fixed..].to_vec() } else { Vec::new() };
            object::define(&scope, &declaration.variadic, Value::list(extra), false).map_err(|message| runtime(base, message))?;
        }
        if !declaration.keyword.is_empty() {
            let dict = collected.unwrap_or_else(|| Rc::new(std::cell::RefCell::new(object::Dict::default())));
            object::define(&scope, &declaration.keyword, Value::Dict(dict), false).map_err(|message| runtime(base, message))?;
        }
        Ok(())
    }

    /// The value bound for parameter `index` when a call leaves it out: the
    /// parameter's default, evaluated in the function's own environment, or the
    /// "missing required argument" error.
    fn default_argument(&mut self, base: &Base, function: &Rc<object::FunctionValue>, index: usize) -> Result<Value, Signal> {
        let declaration = &function.declaration;
        if index < declaration.defaults.len() && declaration.defaults[index].is_some() {
            let default = declaration.defaults[index].as_ref().unwrap();
            return self.expression(default, &function.env);
        }
        Err(runtime(base, format!("{} missing required argument {:?}", function_label(declaration), declaration.parameters[index])))
    }

    /// Binds a call's arguments and runs the body with the tree-walking engine.
    fn run_call(
        &mut self,
        base: &Base,
        function: &Rc<object::FunctionValue>,
        arguments: &[Value],
        keywords: &[Keyword],
        bound: Option<&Rc<object::BoundMethod>>,
        scope: &Env,
    ) -> Eval {
        self.bind_call(base, function, arguments, keywords, bound, scope)?;
        if let Some(flow) = self.block(&function.declaration.body, scope)? {
            return Ok(flow.value);
        }
        Ok(Value::Null)
    }

    fn construct(
        &mut self,
        base: &Base,
        class: &Rc<object::Class>,
        instance: &Rc<object::Instance>,
        arguments: &[Value],
        keywords: &[Keyword],
    ) -> Result<(), Signal> {
        let Some(constructor) = class.find_constructor() else {
            if !arguments.is_empty() || !keywords.is_empty() {
                return Err(runtime(base, format!("{} expects 0 arguments; it declares no constructor", class.name)));
            }
            return Ok(());
        };
        let bound = Rc::new(object::BoundMethod { method: constructor.clone(), receiver: instance.clone() });
        self.invoke(base, &constructor.function, arguments, keywords, Some(&bound))?;
        Ok(())
    }

    fn initialize_fields(&mut self, instance: &Rc<object::Instance>, class: &Rc<object::Class>) -> Result<(), Signal> {
        if let Some(parent) = &class.parent {
            self.initialize_fields(instance, parent)?;
        }
        let fields: Vec<Rc<object::Field>> = class.fields.borrow().clone();
        for field in fields {
            let scope = object::Environment::new(Some(field.env.clone()));
            scope.borrow_mut().access_class = Some(class.clone());
            object::define(&scope, "this", Value::Instance(instance.clone()), true)
                .map_err(|message| runtime(&field.declaration.base, message))?;
            object::define(
                &scope,
                "super",
                Value::Super(Rc::new(object::Super { parent: class.parent.clone(), receiver: instance.clone(), constructor: false })),
                true,
            )
            .map_err(|message| runtime(&field.declaration.base, message))?;
            let value = match &field.declaration.value {
                Some(expression) => self.expression(expression, &scope)?,
                None => Value::Null,
            };
            instance.fields.borrow_mut().insert(field.declaration.name.clone(), value);
        }
        Ok(())
    }

    fn define_class(&mut self, declaration: &Rc<tiger_ast::ClassDecl>, env: &Env) -> Result<(), Signal> {
        self.define_class_with(declaration, env, None)
    }

    /// Defines a class produced by the bytecode engine, whose constructor and
    /// methods carry compiled bodies.
    fn define_class_code(&mut self, proto: &Rc<tiger_object::bytecode::ClassProto>, env: &Env) -> Result<(), Signal> {
        self.define_class_with(&proto.declaration, env, Some(proto))
    }

    fn define_class_with(
        &mut self,
        declaration: &Rc<tiger_ast::ClassDecl>,
        env: &Env,
        bodies: Option<&Rc<tiger_object::bytecode::ClassProto>>,
    ) -> Result<(), Signal> {
        let mut parent = None;
        if let (Some(parent_base), Some(parent_name)) = (&declaration.parent, &declaration.parent_name) {
            let value = object::get(env, parent_name).map_err(|message| runtime(parent_base, message))?;
            match value {
                Value::Class(class) => parent = Some(class),
                other => return Err(runtime(parent_base, format!("parent must be a class, got {}", other.type_name()))),
            }
        }
        let class = object::Class::new(declaration.name.clone(), parent);
        if let Some(constructor) = &declaration.constructor {
            let method = Rc::new(object::Method {
                function: Rc::new(object::FunctionValue {
                    declaration: constructor.clone(),
                    env: env.clone(),
                    code: bodies.and_then(|proto| proto.constructor.clone()),
                }),
                owner: class.clone(),
                constructor: true,
            });
            *class.constructor.borrow_mut() = Some(method);
        }
        for (index, method) in declaration.methods.iter().enumerate() {
            if let Some(parent) = &class.parent {
                if parent.find_field(&method.name).is_some() {
                    return Err(runtime(&method.base, format!("cannot override inherited field {:?}", method.name)));
                }
            }
            let bound = Rc::new(object::Method {
                function: Rc::new(object::FunctionValue {
                    declaration: method.clone(),
                    env: env.clone(),
                    code: bodies.map(|proto| proto.methods[index].clone()),
                }),
                owner: class.clone(),
                constructor: false,
            });
            class.methods.borrow_mut().push((method.name.clone(), bound));
        }
        for field in &declaration.fields {
            if let Some(parent) = &class.parent {
                if parent.find_field(&field.name).is_some() || parent.find_method(&field.name).is_some() {
                    return Err(runtime(&field.base, format!("cannot redeclare inherited member {:?}", field.name)));
                }
            }
            class.fields.borrow_mut().push(Rc::new(object::Field { declaration: field.clone(), owner: class.clone(), env: env.clone() }));
        }
        object::define(env, &declaration.name, Value::Class(class), false).map_err(|message| runtime(&declaration.base, message))?;
        Ok(())
    }

    fn member_access(&mut self, base: &Base, class: &Rc<object::Class>, name: &str, env: &Env, writing: bool) -> Result<(), Signal> {
        if let Some(field) = class.find_field(name) {
            self.accessible(base, &field.declaration.access, &field.owner, env)?;
            if writing && field.declaration.constant {
                return Err(runtime(base, format!("cannot reassign const field {:?}", name)));
            }
        } else if let Some(method) = class.find_method(name) {
            self.accessible(base, &method.function.declaration.access, &method.owner, env)?;
        }
        Ok(())
    }

    fn accessible(&mut self, base: &Base, access: &str, owner: &Rc<object::Class>, env: &Env) -> Result<(), Signal> {
        if access.is_empty() || access == "public" {
            return Ok(());
        }
        if let Some(context) = object::class_context(env) {
            if Rc::ptr_eq(&context, owner) {
                return Ok(());
            }
            if access == "protected" {
                let mut current = Some(context);
                while let Some(class) = current {
                    if Rc::ptr_eq(&class, owner) {
                        return Ok(());
                    }
                    current = class.parent.clone();
                }
            }
        }
        Err(runtime(base, format!("cannot access {} member of {}", access, owner.name)))
    }

    fn property(&mut self, base: &Base, receiver: &Value, name: &str, env: &Env) -> Eval {
        match receiver {
            Value::String(text) => {
                if name == "size" || name == "length" {
                    return Ok(self.length_method(name, (if text.is_ascii() { text.len() } else { text.chars().count() }) as f64));
                }
                if let Some(method) = methods::string_method(name, text.clone()) {
                    return Ok(method);
                }
                if let Some(method) = methods::conversion_method(name, receiver) {
                    return Ok(method);
                }
                Err(runtime(base, format!("string has no property {:?}", name)))
            }
            Value::List(elements) => {
                if name == "size" || name == "length" {
                    return Ok(self.length_method(name, elements.borrow().len() as f64));
                }
                if name == "sort" {
                    return Ok(Value::Builtin(object::Builtin::higher_order("list.sort", Some(receiver.clone()))));
                }
                if let Some(method) = methods::list_method(name, elements.clone()) {
                    return Ok(method);
                }
                if let Some(method) = methods::conversion_method(name, receiver) {
                    return Ok(method);
                }
                Err(runtime(base, format!("list has no property {:?}", name)))
            }
            Value::Dict(dict) => {
                if let Some(method) = methods::dict_method(name, dict.clone()) {
                    return Ok(method);
                }
                if let Some(method) = methods::conversion_method(name, receiver) {
                    return Ok(method);
                }
                Err(runtime(base, format!("dict has no property {:?}", name)))
            }
            Value::Module(module) => {
                if let Some(value) = object::get_own(&module.env, name) {
                    return Ok(value);
                }
                Err(runtime(base, format!("module {:?} has no export {:?}", module.name, name)))
            }
            Value::File(file) => self.file_property(base, file, name),
            Value::Instance(instance) => {
                // Look the member up once: a field and a method cannot share a
                // name, so at most one class-chain walk is needed, not the three
                // that a separate access check plus lookup would cost.
                if let Some(field) = instance.class.find_field(name) {
                    self.accessible(base, &field.declaration.access, &field.owner, env)?;
                    if let Some(value) = instance.fields.borrow().get(name) {
                        return Ok(value.clone());
                    }
                } else if let Some(method) = instance.class.find_method(name) {
                    self.accessible(base, &method.function.declaration.access, &method.owner, env)?;
                    return Ok(Value::BoundMethod(Rc::new(object::BoundMethod { method, receiver: instance.clone() })));
                }
                if let Some(method) = methods::conversion_method(name, receiver) {
                    return Ok(method);
                }
                Err(runtime(base, format!("{} has no property {:?}", instance.class.name, name)))
            }
            Value::Super(super_value) => {
                let Some(parent) = &super_value.parent else {
                    return Err(runtime(base, "super requires a parent class".to_string()));
                };
                if let Some(method) = parent.find_method(name) {
                    self.accessible(base, &method.function.declaration.access, &method.owner, env)?;
                    return Ok(Value::BoundMethod(Rc::new(object::BoundMethod { method, receiver: super_value.receiver.clone() })));
                }
                Err(runtime(base, format!("parent class {} has no method {:?}", parent.name, name)))
            }
            other => {
                if let Some(method) = methods::conversion_method(name, receiver) {
                    return Ok(method);
                }
                Err(runtime(base, format!("{} has no properties", other.type_name())))
            }
        }
    }

    fn length_method(&self, name: &str, length: f64) -> Value {
        let label = name.to_string();
        Value::Builtin(object::Builtin::new(name, move |_host, arguments, keywords| {
            if !keywords.is_empty() {
                return Err(format!("{} does not accept keyword arguments", label));
            }
            if !arguments.is_empty() {
                return Err(format!("{} expects 0 arguments, got {}", label, arguments.len()));
            }
            Ok(Value::Number(length))
        }))
    }

    fn call_aggregate(
        &mut self,
        base: &Base,
        name: &str,
        payload: Option<Value>,
        arguments: &[Value],
        keywords: &[Keyword],
        env: &Env,
    ) -> Eval {
        match name {
            "map" => self.map_builtin(base, arguments, keywords, env),
            "filter" => self.filter_builtin(base, arguments, keywords, env),
            "sorted" => self.sorted_builtin(base, arguments, keywords, env),
            "list.sort" => match payload {
                Some(Value::List(list)) => self.sort_builtin(base, &list, arguments, keywords, env),
                _ => Err(runtime(base, "sort requires a list receiver".to_string())),
            },
            _ => Err(runtime(base, format!("unknown aggregate built-in {:?}", name))),
        }
    }

    fn map_builtin(&mut self, base: &Base, arguments: &[Value], keywords: &[Keyword], env: &Env) -> Eval {
        let (list, function) = callable_list_arguments("map", arguments, keywords).map_err(|message| runtime(base, message))?;
        let elements: Vec<Value> = list.borrow().clone();
        let mut mapped = Vec::with_capacity(elements.len());
        for element in elements {
            mapped.push(self.call(base, function.clone(), &[element], &[], env)?);
        }
        Ok(Value::list(mapped))
    }

    fn filter_builtin(&mut self, base: &Base, arguments: &[Value], keywords: &[Keyword], env: &Env) -> Eval {
        let (list, function) = callable_list_arguments("filter", arguments, keywords).map_err(|message| runtime(base, message))?;
        let elements: Vec<Value> = list.borrow().clone();
        let mut kept = Vec::new();
        for element in elements {
            if object::truthy(&self.call(base, function.clone(), &[element.clone()], &[], env)?) {
                kept.push(element);
            }
        }
        Ok(Value::list(kept))
    }

    fn sorted_builtin(&mut self, base: &Base, arguments: &[Value], keywords: &[Keyword], env: &Env) -> Eval {
        if arguments.len() != 1 {
            return Err(runtime(base, format!("sorted expects 1 argument, got {}", arguments.len())));
        }
        let Value::List(list) = &arguments[0] else {
            return Err(runtime(base, format!("sorted expects a list, got {}", arguments[0].type_name())));
        };
        let (key, reverse) = sort_options("sorted", keywords).map_err(|message| runtime(base, message))?;
        let items: Vec<Value> = list.borrow().clone();
        let keys = self.sort_keys(base, &items, &key, env)?;
        let sorted = self.apply_sort(base, &items, &keys, reverse, false)?;
        Ok(Value::list(sorted))
    }

    fn sort_builtin(
        &mut self,
        base: &Base,
        list: &Rc<std::cell::RefCell<Vec<Value>>>,
        arguments: &[Value],
        keywords: &[Keyword],
        env: &Env,
    ) -> Eval {
        if !arguments.is_empty() {
            return Err(runtime(base, "sort takes no positional arguments".to_string()));
        }
        let (key, reverse) = sort_options("sort", keywords).map_err(|message| runtime(base, message))?;
        let items: Vec<Value> = list.borrow().clone();
        let keys = self.sort_keys(base, &items, &key, env)?;
        let sorted = self.apply_sort(base, &items, &keys, reverse, true)?;
        *list.borrow_mut() = sorted;
        Ok(Value::Null)
    }

    fn sort_keys(&mut self, base: &Base, items: &[Value], key: &Option<Value>, env: &Env) -> Result<Vec<Value>, Signal> {
        match key {
            None => Ok(items.to_vec()),
            Some(key) => {
                let mut keys = Vec::with_capacity(items.len());
                for item in items {
                    keys.push(self.call(base, key.clone(), &[item.clone()], &[], env)?);
                }
                Ok(keys)
            }
        }
    }

    fn apply_sort(&mut self, base: &Base, items: &[Value], keys: &[Value], reverse: bool, sorting: bool) -> Result<Vec<Value>, Signal> {
        let mut order: Vec<usize> = (0..items.len()).collect();
        let mut failure: Option<String> = None;
        order.sort_by(|&left, &right| {
            let (first, second) = if reverse { (&keys[right], &keys[left]) } else { (&keys[left], &keys[right]) };
            match order_values(first, second, sorting) {
                Ok(ordering) => ordering,
                Err(message) => {
                    if failure.is_none() {
                        failure = Some(message);
                    }
                    std::cmp::Ordering::Equal
                }
            }
        });
        if let Some(message) = failure {
            return Err(runtime(base, message));
        }
        Ok(order.iter().map(|&index| items[index].clone()).collect())
    }
}

fn callable_list_arguments(
    name: &str,
    arguments: &[Value],
    keywords: &[Keyword],
) -> Result<(Rc<std::cell::RefCell<Vec<Value>>>, Value), String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.len() != 2 {
        return Err(format!("{} expects 2 arguments, got {}", name, arguments.len()));
    }
    match &arguments[0] {
        Value::List(list) => Ok((list.clone(), arguments[1].clone())),
        other => Err(format!("{} expects a list, got {}", name, other.type_name())),
    }
}

fn sort_options(name: &str, keywords: &[Keyword]) -> Result<(Option<Value>, bool), String> {
    let mut key = None;
    let mut reverse = false;
    for entry in keywords {
        match entry.name.as_str() {
            "key" => {
                key = if matches!(entry.value, Value::Null) { None } else { Some(entry.value.clone()) };
            }
            "reverse" => match &entry.value {
                Value::Bool(flag) => reverse = *flag,
                Value::Number(number) => {
                    if number.trunc() != *number {
                        return Err(format!("{} reverse must be a boolean or integer", name));
                    }
                    reverse = *number != 0.0;
                }
                other => return Err(format!("{} reverse must be a boolean or integer, got {}", name, other.type_name())),
            },
            other => return Err(format!("{} does not accept keyword argument {:?}", name, other)),
        }
    }
    Ok((key, reverse))
}

fn order_values(left: &Value, right: &Value, sorting: bool) -> Result<std::cmp::Ordering, String> {
    match (left, right) {
        (Value::Number(first), Value::Number(second)) => Ok(first.partial_cmp(second).unwrap_or(std::cmp::Ordering::Equal)),
        (Value::String(first), Value::String(second)) => Ok(first.cmp(second)),
        _ => {
            if sorting {
                Err(format!("cannot compare {} and {} while sorting", left.type_name(), right.type_name()))
            } else {
                Err(format!("cannot compare {} and {}", left.type_name(), right.type_name()))
            }
        }
    }
}

impl Evaluator {
    fn binary(&mut self, base: &Base, operator: &str, left: Value, right: Value) -> Eval {
        // Numeric fast path: arithmetic and comparison on two numbers is the hot
        // case in loops, so match it before the general equality/membership
        // dispatch and avoid several string comparisons per operation.
        if let (Value::Number(first), Value::Number(second)) = (&left, &right) {
            let (first, second) = (*first, *second);
            match operator {
                "+" => return finite_number(base, first + second),
                "-" => return finite_number(base, first - second),
                "*" => return finite_number(base, first * second),
                "<" => return Ok(Value::Bool(first < second)),
                "<=" => return Ok(Value::Bool(first <= second)),
                ">" => return Ok(Value::Bool(first > second)),
                ">=" => return Ok(Value::Bool(first >= second)),
                "==" => return Ok(Value::Bool(first == second)),
                "!=" => return Ok(Value::Bool(first != second)),
                "/" | "%" | "div" => {
                    if second == 0.0 {
                        return Err(runtime(base, "division by zero".to_string()));
                    }
                    let value = match operator {
                        "/" => first / second,
                        "div" => (first / second).floor(),
                        _ => {
                            let mut remainder = first % second;
                            if remainder != 0.0 && (remainder < 0.0) != (second < 0.0) {
                                remainder += second;
                            }
                            remainder
                        }
                    };
                    return finite_number(base, value);
                }
                "**" => return finite_number(base, first.powf(second)),
                _ => {}
            }
        }
        match operator {
            "==" => return Ok(Value::Bool(object::equal(&left, &right))),
            "!=" => return Ok(Value::Bool(!object::equal(&left, &right))),
            "in" => {
                match &right {
                    Value::List(elements) => {
                        let found = elements.borrow().iter().any(|element| object::equal(&left, element));
                        return Ok(Value::Bool(found));
                    }
                    Value::Dict(dict) => {
                        let exists = dict.borrow().get(&left).map_err(|message| runtime(base, message))?.is_some();
                        return Ok(Value::Bool(exists));
                    }
                    Value::String(text) => {
                        if let Value::String(needle) = &left {
                            return Ok(Value::Bool(text.contains(&**needle)));
                        }
                    }
                    Value::Error(error) => {
                        if let Value::String(needle) = &left {
                            return Ok(Value::Bool(error.text().contains(&**needle)));
                        }
                    }
                    _ => {}
                }
                return Err(runtime(base, format!("invalid operands for 'in': {} and {}", left.type_name(), right.type_name())));
            }
            _ => {}
        }
        if let (Value::Number(first), Value::Number(second)) = (&left, &right) {
            let (first, second) = (*first, *second);
            match operator {
                "<" => return Ok(Value::Bool(first < second)),
                "<=" => return Ok(Value::Bool(first <= second)),
                ">" => return Ok(Value::Bool(first > second)),
                ">=" => return Ok(Value::Bool(first >= second)),
                _ => {}
            }
            let result = match operator {
                "+" => first + second,
                "-" => first - second,
                "*" => first * second,
                "/" | "%" | "div" => {
                    if second == 0.0 {
                        return Err(runtime(base, "division by zero".to_string()));
                    }
                    match operator {
                        "/" => first / second,
                        "div" => (first / second).floor(),
                        _ => {
                            let mut remainder = first % second;
                            if remainder != 0.0 && (remainder < 0.0) != (second < 0.0) {
                                remainder += second;
                            }
                            remainder
                        }
                    }
                }
                "**" => first.powf(second),
                "&" | "|" | "^" | "<<" | ">>" => {
                    let left_integer = self.bitwise_integer(base, first)?;
                    let right_integer = self.bitwise_integer(base, second)?;
                    match operator {
                        "&" => (left_integer & right_integer) as f64,
                        "|" => (left_integer | right_integer) as f64,
                        "^" => (left_integer ^ right_integer) as f64,
                        _ => {
                            if right_integer < 0 {
                                return Err(runtime(base, "shift count must not be negative".to_string()));
                            }
                            let count = right_integer as u32;
                            let shifted = if count >= 64 {
                                if operator == "<<" || left_integer >= 0 {
                                    0i64
                                } else {
                                    -1i64
                                }
                            } else if operator == "<<" {
                                left_integer << count
                            } else {
                                left_integer >> count
                            };
                            shifted as f64
                        }
                    }
                }
                other => return Err(runtime(base, format!("unknown operator {}", other))),
            };
            if result.is_infinite() || result.is_nan() {
                return Err(runtime(base, "non-finite numeric result".to_string()));
            }
            return Ok(Value::Number(result));
        }
        if let (Value::String(first), Value::String(second)) = (&left, &right) {
            match operator {
                "+" => return Ok(Value::string(&format!("{}{}", first, second))),
                "<" => return Ok(Value::Bool(first < second)),
                "<=" => return Ok(Value::Bool(first <= second)),
                ">" => return Ok(Value::Bool(first > second)),
                ">=" => return Ok(Value::Bool(first >= second)),
                _ => {}
            }
        }
        if operator == "+" {
            if let (Value::List(first), Value::List(second)) = (&left, &right) {
                let mut items = first.borrow().clone();
                items.extend(second.borrow().iter().cloned());
                return Ok(Value::list(items));
            }
        }
        Err(runtime(base, format!("operator {} is not supported for {} and {}", operator, left.type_name(), right.type_name())))
    }
}

/// The value-producing operations the linear engine shares with the interpreter.
impl Evaluator {
    /// `operator value` for `not`, `-`, and `~`.
    fn unary(&mut self, base: &Base, operator: &str, right: Value) -> Eval {
        if operator == "not" {
            return Ok(Value::Bool(!object::truthy(&right)));
        }
        let Value::Number(number) = right else {
            return Err(runtime(base, format!("operator {} requires a number", operator)));
        };
        if operator == "-" {
            return Ok(Value::Number(-number));
        }
        if operator == "~" {
            return Ok(Value::Number(!self.bitwise_integer(base, number)? as f64));
        }
        Ok(Value::Number(number))
    }

    /// `collection[key]` for a list, string, dictionary, or error value.
    fn index_value(&mut self, base: &Base, collection: Value, key: Value) -> Eval {
        match &collection {
            Value::List(elements) => {
                let index = self.index_at(base, &key, elements.borrow().len())?;
                Ok(elements.borrow()[index].clone())
            }
            Value::String(text) => {
                if text.is_ascii() {
                    let index = self.index_at(base, &key, text.len())?;
                    Ok(Value::string(&text[index..index + 1]))
                } else {
                    let runes: Vec<char> = text.chars().collect();
                    let index = self.index_at(base, &key, runes.len())?;
                    Ok(Value::string(&runes[index].to_string()))
                }
            }
            Value::Dict(dict) => match dict.borrow().get(&key) {
                Ok(Some(value)) => Ok(value),
                Ok(None) => Err(runtime(base, format!("dictionary key not found: {}", object::format(&key)))),
                Err(message) => Err(runtime(base, message)),
            },
            Value::Error(error) => self.error_field(base, error, &key),
            other => Err(runtime(base, format!("{} cannot be indexed", other.type_name()))),
        }
    }

    /// Stores `collection[key] = value`, matching the interpreter's index target.
    fn set_index(&mut self, base: &Base, collection: Value, key: Value, value: Value) -> Result<(), Signal> {
        match collection {
            Value::List(elements) => {
                let index = self.index_at(base, &key, elements.borrow().len())?;
                elements.borrow_mut()[index] = value;
                Ok(())
            }
            Value::Dict(dict) => dict.borrow_mut().set(key, value).map_err(|message| runtime(base, message)),
            other => Err(runtime(base, format!("cannot assign an index of {}", other.type_name()))),
        }
    }

    /// Stores `receiver.name = value`, matching the interpreter's property target.
    fn set_property(&mut self, base: &Base, receiver: Value, name: &str, value: Value, env: &Env) -> Result<(), Signal> {
        let Value::Instance(instance) = receiver else {
            return Err(runtime(base, format!("cannot assign a property of {}", receiver.type_name())));
        };
        self.member_access(base, &instance.class, name, env, true)?;
        if instance.class.find_field(name).is_none() {
            return Err(runtime(
                base,
                format!("{} has no field {:?}; declare it in the class with var or const", instance.class.name, name),
            ));
        }
        instance.fields.borrow_mut().insert(name.to_string(), value);
        Ok(())
    }

    /// Applies `++`/`--` to a named variable, matching the interpreter's update.
    fn update_name(&mut self, base: &Base, env: &Env, name: &str, decrement: bool, prefix: bool) -> Eval {
        let previous = match object::get(env, name).map_err(|message| runtime(base, message))? {
            Value::Number(number) => number,
            _ => return Err(runtime(base, format!("{} requires a number", if decrement { "--" } else { "++" }))),
        };
        let updated = if decrement { previous - 1.0 } else { previous + 1.0 };
        if updated.is_infinite() || updated.is_nan() {
            return Err(runtime(base, "non-finite numeric result".to_string()));
        }
        object::assign(env, name, Value::Number(updated)).map_err(|message| runtime(base, message))?;
        Ok(if prefix { Value::Number(updated) } else { Value::Number(previous) })
    }
}

impl Evaluator {
    /// Renders one f-string field with a small format spec: `[<|>|^][0][width][.Nf]`.
    fn apply_format(&mut self, base: &Base, spec: &str, value: &Value) -> Result<String, Signal> {
        let mut text = object::format(value);
        if spec.is_empty() {
            return Ok(text);
        }
        let bytes = spec.as_bytes();
        let mut index = 0usize;
        let mut align = 0u8;
        let mut pad = ' ';
        if index < bytes.len() && (bytes[index] == b'<' || bytes[index] == b'>' || bytes[index] == b'^') {
            align = bytes[index];
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b'0' {
            pad = '0';
            index += 1;
        }
        let mut width = 0usize;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            width = width * 10 + (bytes[index] - b'0') as usize;
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
            if index >= bytes.len() || !bytes[index].is_ascii_digit() {
                return Err(runtime(base, format!("invalid format spec {:?}: expected digits after '.'", spec)));
            }
            let mut precision = 0usize;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                precision = precision * 10 + (bytes[index] - b'0') as usize;
                index += 1;
            }
            if index >= bytes.len() || bytes[index] != b'f' {
                return Err(runtime(base, format!("invalid format spec {:?}: only '.Nf' precision is supported", spec)));
            }
            index += 1;
            if precision > 17 {
                return Err(runtime(base, format!("invalid format spec {:?}: precision must be at most 17", spec)));
            }
            let Value::Number(number) = value else {
                return Err(runtime(base, format!("format spec {:?} requires a number, got {}", spec, value.type_name())));
            };
            text = tiger_format::number_precision(*number, precision);
        }
        if index != bytes.len() {
            return Err(runtime(base, format!("invalid format spec {:?}", spec)));
        }
        if align == 0 {
            align = if matches!(value, Value::String(_)) { b'<' } else { b'>' };
        }
        Ok(pad_text(&text, align, pad, width))
    }
}

/// Pads text to at least `width` code points using the given alignment.
fn pad_text(text: &str, align: u8, pad: char, width: usize) -> String {
    let length = text.chars().count();
    if width <= length {
        return text.to_string();
    }
    let fill: String = std::iter::repeat(pad).take(width - length).collect();
    match align {
        b'>' => {
            if pad == '0' && !text.is_empty() && (text.starts_with('-') || text.starts_with('+')) {
                format!("{}{}{}", &text[..1], fill, &text[1..])
            } else {
                format!("{}{}", fill, text)
            }
        }
        b'^' => {
            let left = (width - length) / 2;
            let right = width - length - left;
            let left_fill: String = std::iter::repeat(pad).take(left).collect();
            let right_fill: String = std::iter::repeat(pad).take(right).collect();
            format!("{}{}{}", left_fill, text, right_fill)
        }
        _ => format!("{}{}", text, fill),
    }
}

impl Evaluator {
    /// Runs try/catch/finally. A `finally` block runs on every exit path; a
    /// control-flow statement inside it overrides an in-flight error, and an
    /// error raised by `finally` replaces the original. Resource-limit failures
    /// bypass `finally` entirely.
    fn try_statement(&mut self, statement: &Stmt, env: &Env) -> Outcome {
        let Stmt::Try { base, body, catches, finally } = statement else { unreachable!("try statement") };
        if finally.is_empty() {
            return self.try_handlers(base, body, catches, env);
        }
        match self.try_handlers(base, body, catches, env) {
            Ok(flow) => {
                let final_result = self.block(finally, &branch_scope(finally, env))?;
                if final_result.is_some() {
                    return Ok(final_result);
                }
                Ok(flow)
            }
            Err(fatal @ Signal::Runtime { fatal: true, .. }) => Err(fatal),
            Err(failure) => {
                let final_result = self.block(finally, &branch_scope(finally, env));
                match final_result {
                    Ok(Some(flow)) => Ok(Some(flow)),
                    Ok(None) => Err(failure),
                    Err(final_failure) => Err(final_failure),
                }
            }
        }
    }

    /// Runs the body and then each catch clause in order; the first matching
    /// clause wins, and an unmatched error propagates unchanged.
    fn try_handlers(&mut self, base: &Base, body: &Block, catches: &[tiger_ast::Catch], env: &Env) -> Outcome {
        let failure = match self.block(body, &branch_scope(body, env)) {
            Ok(flow) => return Ok(flow),
            Err(fatal @ Signal::Runtime { fatal: true, .. }) => return Err(fatal),
            Err(signal) => signal,
        };
        let caught = match &failure {
            Signal::Runtime { error, .. } => Value::Error(error.clone()),
            Signal::Throw { value, .. } => value.clone(),
            _ => return Err(failure),
        };
        for clause in catches {
            let scope = object::Environment::new(Some(env.clone()));
            object::define(&scope, &clause.name, caught.clone(), false).map_err(|message| runtime(base, message))?;
            if let Some(condition) = &clause.condition {
                if !object::truthy(&self.expression(condition, &scope)?) {
                    continue;
                }
            }
            return self.block(&clause.body, &scope);
        }
        Err(failure)
    }

    /// Binds several names from a list or dictionary value.
    fn destructure(&mut self, statement: &Stmt, env: &Env) -> Outcome {
        let Stmt::Destructure { base, names, constant, value, lookup } = statement else { unreachable!("destructure") };
        let value = self.expression(value, env)?;
        if *lookup {
            let Value::Dict(dict) = &value else {
                return Err(runtime(base, format!("cannot destructure {} with {{ ... }}; expected a dictionary", value.type_name())));
            };
            for name in names {
                match dict.borrow().get(&Value::string(name)) {
                    Ok(Some(entry)) => object::define(env, name, entry, *constant).map_err(|message| runtime(base, message))?,
                    Ok(None) => return Err(runtime(base, format!("dictionary is missing key {:?}", name))),
                    Err(message) => return Err(runtime(base, message)),
                }
            }
            return Ok(None);
        }
        let Value::List(elements) = &value else {
            return Err(runtime(base, format!("cannot destructure {} with [ ... ]; expected a list", value.type_name())));
        };
        let elements = elements.borrow();
        if elements.len() != names.len() {
            return Err(runtime(base, format!("cannot unpack {} values into {} names", elements.len(), names.len())));
        }
        for (index, name) in names.iter().enumerate() {
            object::define(env, name, elements[index].clone(), *constant).map_err(|message| runtime(base, message))?;
        }
        Ok(None)
    }

    fn import_module(&mut self, base: &Base, path: &str, env: &Env) -> Result<Rc<Module>, Signal> {
        let Some(loader) = self.loader.clone() else {
            return Err(runtime(base, "imports require a module loader; use RunFile or configure Evaluator.Loader".to_string()));
        };
        if !path.ends_with(".tg") {
            return Err(runtime(base, "import path must end in .tg".to_string()));
        }
        let filename = loader.resolve(&object::module_path(env), path).map_err(|message| runtime(base, message))?;
        if let Some(module) = self.modules.get(&filename) {
            return Ok(module.clone());
        }
        if self.loading.contains(&filename) {
            return Err(runtime(base, format!("circular import of {:?}", filename)));
        }
        if self.import_depth >= 128 {
            return Err(runtime(base, "maximum import depth exceeded".to_string()));
        }
        let source = loader.load(&filename).map_err(|message| runtime(base, format!("cannot import {:?}: {}", filename, message)))?;
        let program = tiger_parser::parse_source(&source, &filename).map_err(|message| runtime(base, message))?;
        self.loading.push(filename.clone());
        self.import_depth += 1;
        let builtins = object::Environment::new(None);
        builtins::install(&builtins);
        let scope = object::Environment::new(Some(builtins));
        scope.borrow_mut().source_path = filename.clone();
        let module = Rc::new(Module { name: filename.clone(), env: scope.clone() });
        let result = self.block(&program.statements, &scope);
        self.loading.pop();
        self.import_depth -= 1;
        result?;
        self.modules.insert(filename, module.clone());
        Ok(module)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct Shared(Rc<std::cell::RefCell<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn run_capture(source: &str) -> Result<String, String> {
        let buffer = Rc::new(std::cell::RefCell::new(Vec::new()));
        let program = tiger_parser::parse(source)?;
        let mut evaluator = Evaluator::new(Box::new(Shared(buffer.clone())));
        evaluator.execute(&program)?;
        let bytes = buffer.borrow().clone();
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    #[test]
    fn evaluates_basics() {
        assert_eq!(run_capture("print(1 + 2 * 3);").unwrap(), "7\n");
        assert_eq!(run_capture("print(\"a\" + \"b\");").unwrap(), "ab\n");
        assert_eq!(run_capture("print(-5 % 3, 5 % -3, 7 / 2, 2 ** 10);").unwrap(), "1 -1 3.5 1024\n");
        assert_eq!(run_capture("print(1 | 2 & 3, 1 << 4, ~0);").unwrap(), "3 16 -1\n");
        assert_eq!(run_capture("print(2 in [1, 2], \"port\" in {\"port\": 80});").unwrap(), "true true\n");
    }

    #[test]
    fn evaluates_collections_and_functions() {
        assert_eq!(run_capture("const xs = [3, 1, 2]; xs.sort(); print(xs);").unwrap(), "[1, 2, 3]\n");
        assert_eq!(run_capture("const d = {}; d[\"a\"] = 1; print(d, len(d));").unwrap(), "{\"a\": 1} 1\n");
        assert_eq!(run_capture("function f(a, b = 2) { return a + b; } print(f(1));").unwrap(), "3\n");
        assert_eq!(run_capture("print(map([1, 2, 3], function(x) { return x * 2; }));").unwrap(), "[2, 4, 6]\n");
        assert_eq!(run_capture("print(sorted([3, 1, 2], reverse=true));").unwrap(), "[3, 2, 1]\n");
        assert_eq!(run_capture("print([1, 2, 3][0:2], \"hello\"[1:3]);").unwrap(), "[1, 2] el\n");
        assert_eq!(run_capture("print(1 if false else 2);").unwrap(), "2\n");
    }

    #[test]
    fn evaluates_classes_and_errors() {
        assert_eq!(run_capture("class A { var v = 1; function get() { return this.v; } } print(A().get());").unwrap(), "1\n");
        assert_eq!(
            run_capture("class A { var n; A(n) { this.n = n; } function get() { return this.n; } } class B extends A { B(n) { super(n + 1); } } print(B(1).get());")
                .unwrap(),
            "2\n"
        );
        assert_eq!(run_capture("try { throw 1; } catch (e) { print(e); }").unwrap(), "1\n");
        assert_eq!(run_capture("try { print(1 / 0); } catch (e) { print(e[\"kind\"]); }").unwrap(), "zero division\n");
        assert_eq!(run_capture("for x in [1, 2, 3] { print(x, end=\"\"); }").unwrap(), "123");
        assert_eq!(run_capture("print(f\"{2.5:.2f}\", f\"{42:05}\");").unwrap(), "2.50 00042\n");
        assert_eq!(run_capture("print(math.sqrt(16), math.floor(2.7));").unwrap(), "4 2\n");
        assert_eq!(run_capture("print(json.stringify({\"a\": [1, 2]}));").unwrap(), "{\"a\":[1,2]}\n");
    }

    #[test]
    fn evaluates_algo_module() {
        assert_eq!(run_capture("print(algo.fibonacci(6), algo.fibonacciList(6));").unwrap(), "8 [0, 1, 1, 2, 3, 5, 8]\n");
        assert_eq!(run_capture("print(algo.fibonacci(78));").unwrap(), "8944394323791464\n");
        assert_eq!(run_capture("print(algo.factorial(0), algo.factorial(5), algo.factorial(18));").unwrap(), "1 120 6402373705728000\n");
        assert_eq!(
            run_capture("print(algo.palindromeValid(\"A man, a plan, a canal: Panama\"), algo.palindromeValid(\"race a car\"));").unwrap(),
            "true false\n"
        );
        assert_eq!(
            run_capture("print(algo.palindromeCanBeValid(\"abca\"), algo.palindromeLongest(\"babad\"), algo.palindromeCount(\"aaa\"), algo.palindromeMinCuts(\"banana\"));").unwrap(),
            "true bab 6 1\n"
        );
        assert_eq!(run_capture("print(algo.slidingWindowMax([1, 3, -1, -3, 5, 3, 6, 7], 3));").unwrap(), "[3, 3, 5, 5, 6, 7]\n");
        assert_eq!(
            run_capture("print(algo.slidingWindowSumMax([1, 12, -5, -6, 50, 3], 4), algo.slidingWindowMinLen([2, 3, 1, 2, 4, 3], 7));")
                .unwrap(),
            "51 2\n"
        );
        assert_eq!(
            run_capture("print(algo.slidingWindowLongestUnique(\"pwwkew\"), algo.slidingWindowMinSubstring(\"ADOBECODEBANC\", \"ABC\"));")
                .unwrap(),
            "3 BANC\n"
        );
        assert_eq!(
            run_capture("print(algo.editDistance(\"kitten\", \"sitting\"), algo.regexMatch(\"aab\", \"c*a*b\"));").unwrap(),
            "3 true\n"
        );
        assert_eq!(
            run_capture("print(algo.rainwater([4, 2, 0, 3, 2, 5]), algo.isPrime(7919), algo.isPrime(561));").unwrap(),
            "9 true false\n"
        );
        assert_eq!(run_capture("print(algo.medianSorted([1, 3], [2]), algo.mergeKSorted([[1, 4], [2, 3]]));").unwrap(), "2 [1, 2, 3, 4]\n");
        assert_eq!(
            run_capture(
                "print(algo.takeInventory([\"a\", \"b\", \"a\"]), algo.removeDuplicates([3, 1, 3]), algo.findMatches([1, 2, 3], [3, 1]));"
            )
            .unwrap(),
            "{\"a\": 2, \"b\": 1} [3, 1] [1, 3]\n"
        );
        assert_eq!(run_capture("print(algo.findPlace([70, 95, 88, 60], 2));").unwrap(), "88\n");
        assert_eq!(run_capture("print(algo, algo.fibonacci);").unwrap(), "<module algo> <builtin algo.fibonacci>\n");
    }

    #[test]
    fn rejects_bad_algo_calls() {
        for (source, message) in [
            ("algo.fibonacci(-1);", "algo.fibonacci expects a non-negative integer, got -1"),
            ("algo.fibonacci(79);", "algo.fibonacci: index 79 is too large; the maximum is 78"),
            ("algo.fibonacci(\"6\");", "algo.fibonacci expects a number, got string"),
            ("algo.fibonacci(1, 2);", "algo.fibonacci expects 1 argument, got 2"),
            ("algo.fibonacci(n=3);", "algo.fibonacci does not accept keyword arguments"),
            ("algo.removeDuplicates(\"aab\");", "algo.removeDuplicates expects a list, got string"),
            ("algo.findMatches([1], \"1\");", "algo.findMatches expects a list, got string"),
            ("algo.takeInventory([1, [2]]);", "algo.takeInventory: list cannot be a dictionary key"),
            ("algo.findPlace([3, 1, 2], 4);", "algo.findPlace: place 4 is out of range; the list has 3 distinct values"),
            ("algo.findPlace([2, 2], 2);", "algo.findPlace: place 2 is out of range; the list has 1 distinct value"),
            ("algo.findPlace([1, 2], 0);", "algo.findPlace: place must be a positive whole number, got 0"),
            ("algo.findPlace([1, \"2\"], 1);", "algo.findPlace: cannot compare number and string"),
            ("algo.findPlace([[1], [2]], 1);", "algo.findPlace: cannot rank list values; use numbers or strings"),
            ("algo.isPrime(2.5);", "algo.isPrime expects an integer, got 2.5"),
            ("algo.isPrime(\"7\");", "algo.isPrime expects an integer, got string"),
            ("algo.isPrime(9007199254740992);", "algo.isPrime: 9007199254740992 is too large; the maximum is 9007199254740991"),
            ("algo.medianSorted([2, 1], [3]);", "algo.medianSorted: first list is not sorted: 1 at index 1 comes after 2"),
            ("algo.medianSorted([], []);", "algo.medianSorted: both lists are empty"),
            ("algo.mergeKSorted([[1], [\"a\"]]);", "algo.mergeKSorted: cannot compare number and string (list 1, index 0)"),
            ("algo.mergeKSorted([1]);", "algo.mergeKSorted: expects a list of lists, got number at index 0"),
            ("algo.rainwater([-1]);", "algo.rainwater: heights cannot be negative, got -1 at index 0"),
            ("algo.rainwater([\"a\"]);", "algo.rainwater: heights must be numbers, got string at index 0"),
            (
                "algo.slidingWindowMax([1, 2], 3);",
                "algo.slidingWindowMax: k must be a whole number from 1 to 2 (the length of nums), got 3",
            ),
            ("algo.slidingWindowMax([1], \"2\");", "algo.slidingWindowMax: k must be a number, got string"),
            ("algo.slidingWindowMinLen([-1], 1);", "algo.slidingWindowMinLen: nums cannot contain negative numbers, got -1 at index 0"),
            ("algo.slidingWindowMinLen([1], 0);", "algo.slidingWindowMinLen: target must be positive, got 0"),
            ("algo.palindromeCount(121);", "algo.palindromeCount expects a string, got number"),
            ("algo.editDistance(\"a\");", "algo.editDistance expects 2 arguments, got 1"),
            ("algo.regexMatch(\"a\", \"**\");", "algo.regexMatch: * at index 0 must follow a character or ."),
        ] {
            let error = run_capture(source).unwrap_err();
            assert!(error.contains(message), "{source}: expected {message:?}, got {error:?}");
        }
    }

    #[test]
    fn evaluates_regex_module() {
        assert_eq!(run_capture("print(regex.match(\"^a.c$\", \"abc\"), regex.match(\"z\", \"abc\"));").unwrap(), "true false\n");
        assert_eq!(run_capture("print(regex.find(\"\\\\d+\", \"ab 123 cd\"), regex.find(\"z\", \"abc\"));").unwrap(), "123 null\n");
        assert_eq!(run_capture("print(regex.findAll(\"\\\\d+\", \"1 22 333\"));").unwrap(), "[\"1\", \"22\", \"333\"]\n");
        assert_eq!(
            run_capture("print(regex.groups(\"(\\\\w+)@(\\\\w+)\", \"user@host\"), regex.groups(\"z\", \"abc\"));").unwrap(),
            "[\"user\", \"host\"] null\n"
        );
        assert_eq!(run_capture("print(regex.replace(\"\\\\d\", \"a1b2\", \"#\"));").unwrap(), "a#b#\n");
        assert_eq!(run_capture("print(regex.split(\"\\\\s+\", \"a b  c\"));").unwrap(), "[\"a\", \"b\", \"c\"]\n");
        assert_eq!(
            run_capture("print(regex.findAll(\"\\\\w+\", regex.replace(\" \", \"a b c\", \"-\")));").unwrap(),
            "[\"a\", \"b\", \"c\"]\n"
        );
        assert_eq!(run_capture("print(regex, regex.match);").unwrap(), "<module regex> <builtin regex.match>\n");
    }

    #[test]
    fn rejects_bad_regex_calls() {
        for (source, message) in [
            ("regex.match(\"[\", \"x\");", "regex: invalid pattern \"[\""),
            ("regex.match(1, \"x\");", "regex.match expects string arguments, got number at position 1"),
            ("regex.match(\"a\");", "regex.match expects 2 arguments, got 1"),
            ("regex.replace(\"a\", \"b\");", "regex.replace expects 3 arguments, got 2"),
            ("regex.split(\"a\", \"b\", \"c\");", "regex.split expects 2 arguments, got 3"),
            ("regex.match(\"a\", \"b\", end=1);", "regex.match does not accept keyword arguments"),
        ] {
            let error = run_capture(source).unwrap_err();
            assert!(error.contains(message), "{source}: expected {message:?}, got {error:?}");
        }
    }
}
