//! Tree-walking interpreter for the Tiger language.
//!
//! A direct port of `pkg/evaluator`. Go's `panic`/`recover` control flow is
//! modelled with `Result<_, Signal>`: runtime errors, `throw`, `return`, and
//! `break`/`continue` all travel as signals, and `finally` runs on every path.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::rc::Rc;

use tiger_ast::{Base, Block, ComprehensionBody, ComprehensionClause, Expr, Stmt};
use tiger_object as object;
use tiger_object::bytecode::{CallLayout, GenProto};
use tiger_object::{Env, Keyword, Module, RuntimeError, Value};

mod algo;
mod builtins;
mod compiler;
mod files;
mod methods;
mod modules;

#[cfg(feature = "profile")]
pub mod profile;
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod regex_engine;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod regex_stub;
mod regexmod;
mod vm;

pub use files::{parse_file_mode, FileMode, FileSystem, MemoryFileSystem, OsFileSystem};
pub use modules::{
    algo_module, json_module, math_module, random_module, regex_module, secrets_module, sys_module, time_module, BundleLoader, FileLoader,
    ModuleLoader,
};

/// How a program finished: the status `sys.exit` chose, and the message a
/// non-integer argument asked the host to print.
///
/// A program that runs to its end reports status 0 with no message, which is what
/// [`Default`] gives. A non-integer `sys.exit` argument behaves as it does in
/// Python: the value is printed on standard error and the status is 1.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Exit {
    /// The process status the program chose.
    pub code: i32,
    /// The message the host should print on standard error, if any.
    pub message: Option<String>,
}

/// A non-local exit: a runtime error, a thrown value, control flow, or `sys.exit`.
pub enum Signal {
    Runtime {
        error: Rc<RuntimeError>,
        fatal: bool,
    },
    Throw {
        value: Value,
        origin: String,
    },
    Return(Value),
    Break(String),
    Continue(String),
    /// `sys.exit` stopped the program. It unwinds like a `return`, runs every
    /// `finally` on the way out, and no `catch` clause takes it.
    Exit(Exit),
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
    if !value.is_finite() {
        return Err(runtime(base, "non-finite numeric result".to_string()));
    }
    Ok(Value::Number(value))
}

/// The process status a number names, or `None` for a number Python would not
/// accept as an exit status: a fractional, infinite, or NaN one, or one outside
/// the range a status can hold.
fn exit_code(number: f64) -> Option<i32> {
    if !number.is_finite() || number.fract() != 0.0 {
        return None;
    }
    let code = number as i64;
    if (i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(&code) {
        Some(code as i32)
    } else {
        None
    }
}

/// The values `for name in value` walks, or the type name of a value that cannot
/// be iterated.
///
/// A list, a tuple, a set, a string's characters, and a dictionary's keys all
/// iterate, so the collection constructors and the set operations accept exactly
/// what a `for` loop accepts.
pub(crate) fn iterable_elements(value: &Value) -> Result<Vec<Value>, &'static str> {
    match value {
        Value::List(elements) => Ok(elements.borrow().clone()),
        Value::Tuple(elements) => Ok(elements.to_vec()),
        Value::Set(set) => Ok(set.borrow().entries.clone()),
        Value::String(text) => Ok(text.chars().map(|ch| Value::string(&ch.to_string())).collect()),
        Value::Dict(dict) => Ok(dict.borrow().entries.iter().map(|(key, _)| key.clone()).collect()),
        other => Err(other.type_name()),
    }
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

/// Which engine a run uses, how many steps it may take, and the arguments the
/// program sees as `sys.argv`.
///
/// The two engines count steps differently — the compiled engine counts
/// instructions, the tree-walker counts statements — so the differential tests
/// run both with `max_steps` set to `0` and compare only the result.
#[derive(Clone)]
pub struct RunOptions {
    pub engine: Engine,
    pub max_steps: u64,
    /// The program's arguments, with the program's own path first as in Python.
    /// A host that runs a program without one leaves this empty, so `sys.argv` is
    /// empty too.
    pub arguments: Vec<String>,
}

impl Default for RunOptions {
    fn default() -> RunOptions {
        RunOptions { engine: Engine::Auto, max_steps: DEFAULT_MAX_STEPS, arguments: Vec::new() }
    }
}

/// A program lowered to the linear engine's instruction list, ready to run.
///
/// This is what [`Evaluator::compile`] produces and [`Evaluator::execute_compiled`]
/// runs: the compile half of a run, kept so a host that runs the same program many
/// times pays for it once. A benchmark that compares the interpreter against another
/// engine or language times this and the run, not the parse and the lowering, which
/// is what makes the comparison one between interpreters rather than compilers.
pub struct CompiledProgram(Rc<tiger_object::bytecode::Chunk>);

/// The interpreter: host services, execution state, and module cache.
pub struct Evaluator {
    pub output: Box<dyn Write>,
    pub input: Option<Box<dyn BufRead>>,
    pub max_steps: u64,
    pub loader: Option<Rc<dyn ModuleLoader>>,
    pub source_path: String,
    /// The program's arguments, with its own path first as in Python, which
    /// `sys.argv` reports.
    pub arguments: Vec<String>,
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
    /// Flat local frames freed by finished calls, reused the same way.
    locals_pool: Vec<Vec<Value>>,
    /// Argument buffers freed by finished calls, reused so a call does not
    /// allocate a vector for the arguments it passes.
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
            arguments: Vec::new(),
            engine: Engine::Auto,
            filesystem: None,
            modules: HashMap::new(),
            loading: Vec::new(),
            import_depth: 0,
            steps: 0,
            depth: 0,
            scope_pool: Vec::new(),
            stack_pool: Vec::new(),
            locals_pool: Vec::new(),

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

    /// Parses and executes a program, returning how it finished.
    ///
    /// The error string carries a positioned diagnostic; the [`Exit`] carries the
    /// status `sys.exit` chose.
    pub fn execute(&mut self, program: &tiger_ast::Program) -> Result<Exit, String> {
        self.begin();
        let env = self.global_scope();
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
        self.finish(outcome)
    }

    /// Lowers a program to the linear engine's instruction list.
    ///
    /// This is the compile half of a run, split out for a host that runs the same
    /// program more than once: [`CompiledProgram`] carries what
    /// [`Evaluator::execute_compiled`] then runs, so the compile is paid for once.
    /// `None` means the program uses a construct the compiler does not lower, which
    /// is the case [`Engine::Auto`] runs on the tree-walker instead.
    pub fn compile(program: &tiger_ast::Program, source_path: &str) -> Option<CompiledProgram> {
        compiler::compile(&program.statements, source_path).ok().map(CompiledProgram)
    }

    /// Runs a program that has already been lowered, without lowering it again.
    ///
    /// The linear engine's half of [`Evaluator::execute`], and the one a benchmark
    /// that times the interpreter rather than the compiler wants: the only work
    /// left here is the program's own global scope and running its instructions.
    /// A program that did not lower has no [`CompiledProgram`] to run, so the
    /// caller falls back to [`Evaluator::execute`] under [`Engine::Tree`].
    pub fn execute_compiled(&mut self, program: &CompiledProgram) -> Result<Exit, String> {
        self.begin();
        let env = self.global_scope();
        let outcome = self.run_chunk(&program.0, &env).map(|_| ());
        self.finish(outcome)
    }

    /// Clears the state a run owns, so a second run starts where a first one did.
    fn begin(&mut self) {
        self.steps = 0;
        self.depth = 0;
        self.modules.clear();
        self.loading.clear();
        self.import_depth = 0;
        if !self.source_path.is_empty() {
            self.loading.push(self.source_path.clone());
        }
    }

    /// The scope a program's globals are declared in: the built-ins, then a scope
    /// of its own that remembers the file the program came from.
    fn global_scope(&self) -> Env {
        let builtins = object::Environment::new(None);
        builtins::install(&builtins, &self.arguments);
        let env = object::Environment::new(Some(builtins));
        env.borrow_mut().source_path = self.source_path.clone();
        env
    }

    /// Turns a run's terminal signal into the result a caller of a run reports.
    fn finish(&self, outcome: Result<(), Signal>) -> Result<Exit, String> {
        match outcome {
            Ok(()) => Ok(Exit::default()),
            Err(Signal::Runtime { error, .. }) => Err(error.text()),
            Err(Signal::Throw { value, origin }) => Err(format!("{}: uncaught throw: {}", origin, object::format(&value))),
            Err(Signal::Exit(exit)) => Ok(exit),
            Err(_) => Ok(Exit::default()),
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
) -> Result<Exit, String> {
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
) -> Result<Exit, String> {
    let program = tiger_parser::parse_source(source, filename)?;
    let mut evaluator = Evaluator::new(output);
    evaluator.source_path = filename.to_string();
    evaluator.loader = loader;
    evaluator.filesystem = filesystem;
    evaluator.input = input;
    evaluator.engine = options.engine;
    evaluator.max_steps = options.max_steps;
    evaluator.arguments = options.arguments;
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

/// Where a comprehension collects what it builds while its clauses run.
enum ComprehensionResult {
    List(Vec<Value>),
    Dict(object::Dict),
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
            Stmt::Import { base, path, name, names } => {
                let module = self.import_module(base, path, env)?;
                if names.is_empty() {
                    object::define(env, name, Value::Module(module), true).map_err(|message| runtime(base, message))?;
                } else {
                    // A named import binds each member it asked for, as a constant
                    // like the module alias itself.
                    for imported in names {
                        let value = self.module_member(&imported.base, &module, &imported.name)?;
                        object::define(env, &imported.alias, value, true).map_err(|message| runtime(&imported.base, message))?;
                    }
                }
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
                // A lazy iterator is stepped one element at a time, so a generator
                // that is stopped early never builds the rest; anything else is
                // snapshotted first, exactly as before.
                let lazy = iterable.is_iterator();
                let values = if lazy { Vec::new() } else { self.iterable_values(base, &iterable)? };
                let mut index = 0usize;
                loop {
                    let value = if lazy {
                        match self.iterator_next(base, &iterable)? {
                            Some(value) => value,
                            None => break,
                        }
                    } else {
                        if index >= values.len() {
                            break;
                        }
                        let value = values[index].clone();
                        index += 1;
                        value
                    };
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
            Expr::Set { base, elements } => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    values.push(self.expression(element, env)?);
                }
                // Every element is hashed as it is added, so an unhashable one is
                // rejected here with the literal's own position.
                Value::set(values).map_err(|message| runtime(base, message))
            }
            Expr::Tuple { elements, .. } => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    values.push(self.expression(element, env)?);
                }
                Ok(Value::tuple(values))
            }
            Expr::Comprehension { base, body, clauses } => self.comprehension(base, body, clauses, env),
            Expr::Generator { body, clauses, .. } => {
                let proto = Rc::new(GenProto { body: (**body).clone(), clauses: clauses.clone() });
                self.generator(&proto, env)
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
                    if !updated.is_finite() {
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
                if !updated.is_finite() {
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
                    Value::Tuple(elements) => {
                        let index = self.index_at(base, &key, elements.len())?;
                        Ok(elements[index].clone())
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
                    return self.call(base, function, &mut buffer[..arguments.len()], &[], env);
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
                        Value::Tuple(elements) => positional.extend(elements.iter().cloned()),
                        other => {
                            return Err(runtime(base, format!("cannot unpack {} in a call; expected a list or tuple", other.type_name())));
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
                self.call(base, function, &mut positional, &named, env)
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
                let Some(field) = instance.class.find_field(name) else {
                    return Err(runtime(
                        base,
                        format!("{} has no field {:?}; declare it in the class with var or const", instance.class.name, name),
                    ));
                };
                instance.set_field(&field.name, value);
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
        iterable_elements(iterable).map_err(|kind| runtime(base, format!("{} is not iterable", kind)))
    }

    /// Drains any iterable into a list, stepping a lazy iterator to its end.
    ///
    /// This is the one materializing entry point, used where every element is
    /// genuinely needed — a constructor, an aggregate built-in, or `sorted` — and
    /// by the built-ins that accepted only a list or tuple before.
    fn materialize(&mut self, base: &Base, value: &Value) -> Result<Vec<Value>, Signal> {
        if value.is_iterator() {
            let mut values = Vec::new();
            while let Some(element) = self.iterator_next(base, value)? {
                values.push(element);
            }
            return Ok(values);
        }
        iterable_elements(value).map_err(|kind| runtime(base, format!("{} is not iterable", kind)))
    }

    /// Turns any iterable into a lazy iterator, leaving an iterator as it is.
    fn open_iterator(&mut self, base: &Base, value: Value) -> Result<Value, Signal> {
        if value.is_iterator() {
            return Ok(value);
        }
        if object::is_iterable(&value) {
            return Ok(Value::iterator(object::IteratorKind::Sequence { source: value, index: 0 }));
        }
        Err(runtime(base, format!("{} is not iterable", value.type_name())))
    }

    /// Produces the next element of a lazy iterator, or `None` when it ends.
    ///
    /// The kind is stepped out of the value and stored back, so no borrow is held
    /// while a `map`/`filter` callback or a generator body runs user code.
    fn iterator_next(&mut self, base: &Base, iterator: &Value) -> Result<Option<Value>, Signal> {
        let Value::Iterator(state) = iterator else {
            return Err(runtime(base, format!("{} is not an iterator", iterator.type_name())));
        };
        self.tick(base)?;
        // A range steps arithmetically and runs no user code, so it is stepped inside the borrow the
        // state is already under rather than swapped out and back. What that saves is a borrow and
        // two moves of the whole kind — larger than it looks, because the generator variant carries
        // two environments — for every element of every loop over a range, which is the shape
        // `for n in range(...)` has.
        {
            let mut borrowed = state.borrow_mut();
            if borrowed.done {
                return Ok(None);
            }
            let mut yielded: Option<Value> = None;
            let mut finished = false;
            if let object::IteratorKind::Range { current, stop, step } = &mut borrowed.kind {
                match range_step(*current, *stop, *step) {
                    Some((value, next)) => {
                        *current = next;
                        yielded = Some(Value::Number(value));
                    }
                    None => finished = true,
                }
            }
            if finished {
                borrowed.kind = object::IteratorKind::Done;
                borrowed.done = true;
                return Ok(None);
            }
            if yielded.is_some() {
                return Ok(yielded);
            }
        }
        let kind = {
            let mut borrowed = state.borrow_mut();
            std::mem::replace(&mut borrowed.kind, object::IteratorKind::Done)
        };
        let (result, restored) = match kind {
            object::IteratorKind::Done => (Ok(None), object::IteratorKind::Done),
            object::IteratorKind::Range { current, stop, step } => match range_step(current, stop, step) {
                Some((value, next)) => (Ok(Some(Value::Number(value))), object::IteratorKind::Range { current: next, stop, step }),
                None => (Ok(None), object::IteratorKind::Done),
            },
            object::IteratorKind::Sequence { source, index } => match object::sequence_element(&source, index) {
                Some(value) => (Ok(Some(value)), object::IteratorKind::Sequence { source, index: index + 1 }),
                None => (Ok(None), object::IteratorKind::Done),
            },
            object::IteratorKind::Map { source, function, env } => match self.iterator_next(base, &source)? {
                Some(element) => {
                    let mapped = self.call(base, function.clone(), &mut [element.clone()], &[], &env)?;
                    (Ok(Some(mapped)), object::IteratorKind::Map { source, function, env })
                }
                None => (Ok(None), object::IteratorKind::Done),
            },
            object::IteratorKind::Filter { source, function, env } => {
                let kept = loop {
                    match self.iterator_next(base, &source)? {
                        Some(element) => {
                            if object::truthy(&self.call(base, function.clone(), std::slice::from_mut(&mut element.clone()), &[], &env)?) {
                                break Some(element);
                            }
                        }
                        None => break None,
                    }
                };
                if kept.is_none() {
                    (Ok(None), object::IteratorKind::Done)
                } else {
                    (Ok(kept), object::IteratorKind::Filter { source, function, env })
                }
            }
            object::IteratorKind::Generator { proto, env, scope, mut cursors } => {
                match self.generator_next(base, &proto, &scope, &mut cursors)? {
                    Some(value) => (Ok(Some(value)), object::IteratorKind::Generator { proto, env, scope, cursors }),
                    None => (Ok(None), object::IteratorKind::Done),
                }
            }
        };
        let done = matches!(restored, object::IteratorKind::Done);
        {
            let mut borrowed = state.borrow_mut();
            borrowed.kind = restored;
            borrowed.done = done;
        }
        result
    }

    /// Builds a lazy generator iterator from a generator expression.
    ///
    /// Nothing is evaluated now: the clauses and element run in `scope`, the
    /// generator's own scope, only as elements are pulled. The scope lives as long
    /// as the iterator, so a clause name never leaks into the caller.
    fn generator(&self, proto: &Rc<GenProto>, env: &Env) -> Eval {
        let scope = object::Environment::new(Some(env.clone()));
        Ok(Value::iterator(object::IteratorKind::Generator { proto: proto.clone(), env: env.clone(), scope, cursors: Vec::new() }))
    }

    /// Pulls the next value a generator yields, advancing its clause cursors.
    fn generator_next(
        &mut self,
        base: &Base,
        proto: &Rc<GenProto>,
        scope: &Env,
        cursors: &mut Vec<Value>,
    ) -> Result<Option<Value>, Signal> {
        loop {
            if cursors.is_empty() {
                let iterable = self.expression(&proto.clauses[0].iterable, scope)?;
                cursors.push(self.open_iterator(base, iterable)?);
            }
            let level = cursors.len() - 1;
            let source = cursors[level].clone();
            let Some(value) = self.iterator_next(base, &source)? else {
                cursors.pop();
                if cursors.is_empty() {
                    return Ok(None);
                }
                continue;
            };
            let clause = &proto.clauses[level];
            // The clause variable is rebound in place, so a later clause can read
            // an earlier one; `define` succeeds once and `assign` afterwards.
            if object::define(scope, &clause.name, value.clone(), false).is_err() {
                object::assign(scope, &clause.name, value).map_err(|message| runtime(base, message))?;
            }
            let mut keep = true;
            for filter in &clause.filters {
                if !object::truthy(&self.expression(filter, scope)?) {
                    keep = false;
                    break;
                }
            }
            if !keep {
                continue;
            }
            if level + 1 < proto.clauses.len() {
                let iterable = self.expression(&proto.clauses[level + 1].iterable, scope)?;
                cursors.push(self.open_iterator(base, iterable)?);
                continue;
            }
            return Ok(Some(self.expression(&proto.body, scope)?));
        }
    }

    /// Evaluates a list or dictionary comprehension.
    ///
    /// The clauses nest left to right, so every iteration of one clause runs the
    /// whole of the next, and a clause's filters are checked before the next clause
    /// or the element. The clause variables live in the comprehension's own scope,
    /// which is dropped when it finishes, so they never leak into the enclosing one.
    /// Both engines come through here, so their results cannot drift apart.
    fn comprehension(&mut self, base: &Base, body: &ComprehensionBody, clauses: &[ComprehensionClause], env: &Env) -> Eval {
        let scope = self.acquire_scope(env.clone());
        let mut result = match body {
            ComprehensionBody::List(_) => ComprehensionResult::List(Vec::new()),
            ComprehensionBody::Dict(_) => ComprehensionResult::Dict(object::Dict::default()),
        };
        let outcome = self.comprehension_clause(base, body, clauses, 0, &scope, &mut result);
        self.release_scope(scope);
        outcome?;
        Ok(match result {
            ComprehensionResult::List(values) => Value::list(values),
            ComprehensionResult::Dict(dict) => Value::Dict(Rc::new(std::cell::RefCell::new(dict))),
        })
    }

    /// Runs one clause of a comprehension: its iterable, its filters, then either
    /// the next clause or the value that is collected.
    fn comprehension_clause(
        &mut self,
        base: &Base,
        body: &ComprehensionBody,
        clauses: &[ComprehensionClause],
        index: usize,
        scope: &Env,
        result: &mut ComprehensionResult,
    ) -> Result<(), Signal> {
        let clause = &clauses[index];
        let iterable = self.expression(&clause.iterable, scope)?;
        // A lazy iterator source is stepped one element at a time; anything else
        // is snapshotted first. Either way the clause body below is identical.
        let lazy = iterable.is_iterator();
        let values = if lazy { Vec::new() } else { self.iterable_values(clause.iterable.base(), &iterable)? };
        let mut position = 0usize;
        // Whether the clause variable has been declared in this scope yet. It is
        // declared once and written in place afterwards: declaring it on every pass
        // would make each pass after the first a duplicate declaration, which is a
        // diagnostic the binding path builds — a formatted string — and this one
        // throws away. A loop over a thousand elements used to build a thousand of
        // them.
        let mut declared = false;
        loop {
            let value = if lazy {
                match self.iterator_next(clause.iterable.base(), &iterable)? {
                    Some(value) => value,
                    None => break,
                }
            } else {
                if position >= values.len() {
                    break;
                }
                let value = values[position].clone();
                position += 1;
                value
            };
            self.tick(base)?;
            // The clause variable is rebound in place after the first pass, so a
            // later clause can still read the one before it. A name this scope
            // already holds — the same name in a repeated clause — is assigned
            // rather than declared, which is what the duplicate path did before.
            if declared {
                object::assign(scope, &clause.name, value).map_err(|message| runtime(base, message))?;
            } else {
                declared = true;
                if object::define(scope, &clause.name, value.clone(), false).is_err() {
                    object::assign(scope, &clause.name, value).map_err(|message| runtime(base, message))?;
                }
            }
            let mut keep = true;
            for filter in &clause.filters {
                if !object::truthy(&self.expression(filter, scope)?) {
                    keep = false;
                    break;
                }
            }
            if !keep {
                continue;
            }
            if index + 1 < clauses.len() {
                self.comprehension_clause(base, body, clauses, index + 1, scope, result)?;
                continue;
            }
            match body {
                ComprehensionBody::List(element) => {
                    let computed = self.expression(element, scope)?;
                    if let ComprehensionResult::List(values) = result {
                        values.push(computed);
                    }
                }
                ComprehensionBody::Dict(pair) => {
                    let key = self.expression(&pair.key, scope)?;
                    let value = self.expression(&pair.value, scope)?;
                    if let ComprehensionResult::Dict(dict) = result {
                        dict.set(key, value).map_err(|message| runtime(pair.key.base(), message))?;
                    }
                }
            }
        }
        Ok(())
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
            Value::Tuple(elements) => {
                let (start, stop, step) = self.slice_bounds(base, start, stop, step, elements.len(), env)?;
                let indices = slice_indices(start, stop, step);
                // Slicing a tuple gives another tuple, as in Python.
                Ok(Value::tuple(indices.iter().map(|index| elements[*index as usize].clone()).collect()))
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
        if !value.is_finite() || value < -LIMIT || value >= LIMIT {
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
    fn call(&mut self, base: &Base, function: Value, arguments: &mut [Value], keywords: &[Keyword], env: &Env) -> Eval {
        match function {
            Value::Builtin(builtin) => {
                if builtin.higher_order {
                    return self.call_aggregate(base, &builtin.name, builtin.payload.clone(), arguments, keywords, env);
                }
                // A built-in that walks its arguments gets any lazy iterator among
                // them drained first, so `list`, `sum`, `zip`, and their kin accept
                // a generator exactly as they accept a list. Built-ins that only
                // inspect a value — `type`, `len`, `str` — keep the iterator.
                if builtin.consumes_iterables && arguments.iter().any(Value::is_iterator) {
                    let mut materialized = Vec::with_capacity(arguments.len());
                    for argument in arguments {
                        if argument.is_iterator() {
                            materialized.push(Value::list(self.materialize(base, argument)?));
                        } else {
                            materialized.push(argument.clone());
                        }
                    }
                    return (builtin.call)(self, &materialized, keywords).map_err(|message| runtime(base, message));
                }
                (builtin.call)(self, arguments, keywords).map_err(|message| runtime(base, message))
            }
            Value::Function(function) => self.invoke(base, &function, arguments, keywords, None),
            Value::BoundMethod(bound) => {
                self.invoke(base, &bound.method.function, arguments, keywords, Some((&bound.method, &bound.receiver)))
            }
            Value::Super(super_value) => {
                if !super_value.constructor {
                    return Err(runtime(base, "super(...) can only be called inside a constructor".to_string()));
                }
                let Some(parent) = super_value.parent.clone() else {
                    return Err(runtime(base, "super requires a parent class".to_string()));
                };
                let constructor = parent.find_constructor();
                if let Some(constructor) = &constructor {
                    self.accessible(base, &constructor.function.declaration.access, &constructor.owner, env)?;
                }
                self.construct(base, &parent, constructor.as_ref(), &super_value.receiver, arguments, keywords)?;
                Ok(Value::Null)
            }
            Value::Class(class) => {
                let instance = Rc::new(object::Instance {
                    class: class.clone(),
                    fields: std::cell::RefCell::new(Vec::with_capacity(field_count(&class))),
                });
                // Found once and handed on: a class's constructor cannot change
                // between here and the call, and finding it walks the chain.
                let constructor = class.find_constructor();
                if let Some(constructor) = &constructor {
                    self.accessible(base, &constructor.function.declaration.access, &constructor.owner, env)?;
                }
                self.initialize_fields(&instance, &class)?;
                self.construct(base, &class, constructor.as_ref(), &instance, arguments, keywords)?;
                Ok(Value::Instance(instance))
            }
            other => Err(runtime(base, format!("{} is not callable", other.type_name()))),
        }
    }
}

/// How many fields an instance of this class ends up with, counting the whole
/// chain, so its storage can be sized once instead of grown field by field.
fn field_count(class: &Rc<object::Class>) -> usize {
    let mut total = 0;
    let mut current = Some(class);
    while let Some(level) = current {
        total += level.fields.borrow().len();
        current = level.parent.as_ref();
    }
    total
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

/// The flat slot a parameter binds to, when the compiler gave it one. A parameter
/// that reaches no nested function stays in the frame's own locals instead of the
/// shared environment, and the body's slot-based access expects the argument there.
fn parameter_slot(layout: Option<&CallLayout>, index: usize) -> Option<u32> {
    layout.and_then(|layout| layout.parameters.get(index).copied().flatten())
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

/// What a caller passed: the positional arguments and the keyword arguments. They
/// always travel together, and together they are what a callee binds.
///
/// The positional slice is mutable because binding *moves* each argument out of the
/// operand stack into the frame: the caller's copy is dead the moment the call is
/// made, so cloning it and then dropping the original is work with nothing to show.
struct Args<'a> {
    positional: &'a mut [Value],
    keywords: &'a [Keyword],
}

impl<'a> Args<'a> {
    fn new(positional: &'a mut [Value], keywords: &'a [Keyword]) -> Args<'a> {
        Args { positional, keywords }
    }
}

/// How many flat locals a function's frame may hold and still take the flat call path
/// — the shape the engine enters without re-entering itself.
///
/// The compiler folds this into [`Chunk::flat_callable`](tiger_object::bytecode::Chunk),
/// so the bound is decided once, where the declaration is lowered, rather than tested
/// at every call.
const FLAT_LOCALS: usize = 4;

/// The locals a call may reuse: the layout the compiler worked out for the call site,
/// and the frame itself when the site is a flat one. Both are `None` for a call the
/// compiler did not specialize, and the two always arrive together.
type Frame<'a> = Option<(&'a CallLayout, Option<&'a mut [Value]>)>;

/// A compiled frame a call has prepared but not entered.
///
/// This is everything a call does before its body runs: the arity check, the call's own
/// scope, the frame's flat locals, and the binding of `this`, `super`, the parameters,
/// the variadic list, the keywords, and the defaults. Keeping it apart from running the
/// body is what lets the instruction loop enter the frame itself instead of re-entering
/// the engine through `run_frame` — and one implementation serving both is what keeps a
/// parked call and an `invoke`d one from drifting apart in what they check or report.
pub(crate) struct Prepared<'a> {
    /// The body to run.
    pub(crate) code: &'a Rc<tiger_object::bytecode::Chunk>,
    /// The environment the body runs in: the call's own scope when the frame needs one,
    /// and the environment the function captured when it does not.
    pub(crate) env: Env,
    /// The frame's flat locals, with the arguments already bound.
    pub(crate) locals: Vec<Value>,
    /// The call's own scope, when it took one, for the frame's return to release.
    pub(crate) scope: Option<Env>,
}

impl Evaluator {
    fn invoke(
        &mut self,
        base: &Base,
        function: &Rc<object::FunctionValue>,
        arguments: &mut [Value],
        keywords: &[Keyword],
        bound: Option<(&Rc<object::Method>, &Rc<object::Instance>)>,
    ) -> Eval {
        let Some(prepared) = self.prepare_frame(base, function, arguments, keywords, bound)? else {
            // A function that was never lowered has no instructions to run, so the
            // tree-walking engine runs its body. Its frame is always a scope of its own:
            // the compiler proved nothing about a body it did not see.
            let scope = self.acquire_scope(function.env.clone());
            let result = self.run_call(base, function, arguments, keywords, bound, &scope);
            self.release_scope(scope);
            // A `return` inside a `try` leaves its sub-chunk as a `Signal::Return` and
            // unwinds to here, which is the function boundary: it becomes the result.
            return match result {
                Err(Signal::Return(value)) => Ok(value),
                other => other,
            };
        };
        let Prepared { code, env, locals, scope } = prepared;
        let result = self.run_frame(code, &env, locals);
        if let Some(scope) = scope {
            self.release_scope(scope);
        }
        self.depth -= 1;
        // A `return` inside a `try` leaves its sub-chunk as a `Signal::Return` and
        // unwinds to here, which is the function boundary: it becomes the result.
        match result {
            Err(Signal::Return(value)) => Ok(value),
            other => other,
        }
    }

    /// Checks a call's arity, takes the scope its frame needs, and binds the arguments
    /// into a frame the caller can then run — here, or in the instruction loop.
    ///
    /// `None` means the function has no compiled body, which is the shape the
    /// tree-walking engine runs: the arity has been checked, and no depth has been
    /// counted and no scope taken.
    pub(crate) fn prepare_frame<'a>(
        &mut self,
        base: &Base,
        function: &'a Rc<object::FunctionValue>,
        arguments: &mut [Value],
        keywords: &[Keyword],
        bound: Option<(&Rc<object::Method>, &Rc<object::Instance>)>,
    ) -> Result<Option<Prepared<'a>>, Signal> {
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
        let Some(code) = &function.code else { return Ok(None) };
        // A call scope is pooled and reused when the call does not capture it, so
        // a recursive or repeated call does not allocate a scope and its bindings
        // each time.
        //
        // A compiled function whose frame holds nothing the environment has to
        // carry — every name it binds is flat — needs no scope at all: the body
        // runs directly in the environment the function captured. A name that
        // resolves above the frame is a free name that walks the environment chain,
        // so skipping an empty frame leaves every slot-based access where the
        // compiler placed it.
        let scope = if code.frame_needed { Some(self.acquire_scope(function.env.clone())) } else { None };
        let env = match &scope {
            Some(scope) => scope.clone(),
            None => function.env.clone(),
        };
        self.depth += 1;
        if self.depth >= 512 {
            self.depth -= 1;
            if let Some(scope) = scope {
                self.release_scope(scope);
            }
            return Err(fatal(base, "maximum evaluation depth exceeded".to_string()));
        }
        // The frame's flat locals come from the pool, so a call allocates nothing once
        // the pool is warm. They are the call's, too: a parameter the compiler gave a
        // flat slot is written straight into it, so the body never looks a parameter up
        // in an environment.
        let mut locals = self.take_locals(code.flat_count);
        let binding =
            self.bind_call(base, function, Args::new(arguments, keywords), bound, &env, Some((&code.call_layout, Some(&mut locals[..]))));
        if let Err(signal) = binding {
            self.depth -= 1;
            if let Some(scope) = scope {
                self.release_scope(scope);
            }
            return Err(signal);
        }
        Ok(Some(Prepared { code, env, locals, scope }))
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
        args: Args<'_>,
        bound: Option<(&Rc<object::Method>, &Rc<object::Instance>)>,
        scope: &Env,
        frame: Frame<'_>,
    ) -> Result<(), Signal> {
        let Args { positional: arguments, keywords } = args;
        let (layout, mut locals) = match frame {
            Some((layout, locals)) => (Some(layout), locals),
            None => (None, None),
        };
        let declaration = &function.declaration;
        let parameters = &declaration.parameters;
        let fixed = parameters.len();
        let label = function_label(declaration);
        // The shape almost every call has: one positional argument per parameter, no
        // keywords, nothing variadic, and every binding landing in a flat slot. It
        // needs no lookup, no default, and no visibility decision beyond the class
        // context, so it is a straight copy into the frame's own locals.
        if keywords.is_empty()
            && declaration.variadic.is_empty()
            && declaration.keyword.is_empty()
            && arguments.len() == fixed
            && layout.is_some_and(|layout| !layout.super_used && (bound.is_none() || layout.this.is_some()))
        {
            let layout = layout.expect("the layout was just checked");
            if let Some(locals) = locals.as_deref_mut() {
                if layout.flat_parameters {
                    if let Some(bound) = bound {
                        scope.borrow_mut().access_class = Some(bound.0.owner.clone());
                        locals[layout.this.expect("checked") as usize] = Value::Instance(bound.1.clone());
                    }
                    for (index, slot) in layout.parameters.iter().enumerate() {
                        // The argument moves into the frame: the caller's copy on the
                        // operand stack is dropped when the call returns, so cloning it
                        // would be a copy that is thrown away.
                        locals[slot.expect("checked") as usize] = std::mem::replace(&mut arguments[index], Value::Null);
                    }
                    return Ok(());
                }
            }
        }
        // Binds one argument: into the frame's own flat slot when the compiler gave
        // it one, or into the shared environment when a nested function can reach it.
        let mut frame = locals;
        let mut bind = |slot: Option<u32>, name: &str, shared: Option<&Rc<str>>, value: Value| -> Result<(), Signal> {
            match (slot, frame.as_deref_mut()) {
                (Some(slot), Some(locals)) => locals[slot as usize] = value,
                _ => match shared {
                    Some(shared) => object::define_shared(scope, shared, value, false),
                    None => object::define(scope, name, value, false),
                }
                .map_err(|message| runtime(base, message))?,
            }
            Ok(())
        };
        if let Some(bound) = bound {
            scope.borrow_mut().access_class = Some(bound.0.owner.clone());
            bind(layout.and_then(|layout| layout.this), "this", Some(&self.this_name), Value::Instance(bound.1.clone()))?;
            // The tree-walking engine has no layout and always binds `super`, since
            // it resolves the name at run time.
            if layout.is_none_or(|layout| layout.super_used) {
                bind(
                    layout.and_then(|layout| layout.super_),
                    "super",
                    Some(&self.super_name),
                    Value::Super(Rc::new(object::Super {
                        parent: bound.0.owner.parent.clone(),
                        receiver: bound.1.clone(),
                        constructor: bound.0.constructor,
                    })),
                )?;
            }
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
                let value = arguments[index].clone();
                bind(parameter_slot(layout, index), parameter, Some(parameter), value)?;
            }
            for (index, parameter) in parameters.iter().enumerate().skip(bound) {
                let value = self.default_argument(base, function, index)?;
                bind(parameter_slot(layout, index), parameter, Some(parameter), value)?;
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
                bind(parameter_slot(layout, index), parameter, Some(parameter), value)?;
            }
        }
        if !declaration.variadic.is_empty() {
            let extra: Vec<Value> = if arguments.len() > fixed { arguments[fixed..].to_vec() } else { Vec::new() };
            let shared = Rc::from(declaration.variadic.as_str());
            bind(layout.and_then(|layout| layout.variadic), &declaration.variadic, Some(&shared), Value::list(extra))?;
        }
        if !declaration.keyword.is_empty() {
            let dict = collected.unwrap_or_else(|| Rc::new(std::cell::RefCell::new(object::Dict::default())));
            let shared = Rc::from(declaration.keyword.as_str());
            bind(layout.and_then(|layout| layout.keyword), &declaration.keyword, Some(&shared), Value::Dict(dict))?;
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
        arguments: &mut [Value],
        keywords: &[Keyword],
        bound: Option<(&Rc<object::Method>, &Rc<object::Instance>)>,
        scope: &Env,
    ) -> Eval {
        self.bind_call(base, function, Args::new(arguments, keywords), bound, scope, None)?;
        if let Some(flow) = self.block(&function.declaration.body, scope)? {
            return Ok(flow.value);
        }
        Ok(Value::Null)
    }

    fn construct(
        &mut self,
        base: &Base,
        class: &Rc<object::Class>,
        constructor: Option<&Rc<object::Method>>,
        instance: &Rc<object::Instance>,
        arguments: &mut [Value],
        keywords: &[Keyword],
    ) -> Result<(), Signal> {
        let Some(constructor) = constructor else {
            if !arguments.is_empty() || !keywords.is_empty() {
                return Err(runtime(base, format!("{} expects 0 arguments; it declares no constructor", class.name)));
            }
            return Ok(());
        };
        self.invoke(base, &constructor.function, arguments, keywords, Some((constructor, instance)))?;
        Ok(())
    }

    fn initialize_fields(&mut self, instance: &Rc<object::Instance>, class: &Rc<object::Class>) -> Result<(), Signal> {
        if let Some(parent) = &class.parent {
            self.initialize_fields(instance, parent)?;
        }
        // The class's own fields, fetched one at a time: a field with no initializer
        // needs neither a scope nor a borrow held across evaluation, so the whole
        // vector is not cloned for every instance built.
        let count = class.fields.borrow().len();
        for index in 0..count {
            let field = class.fields.borrow()[index].clone();
            // A field with no initializer is simply `null`, so there is no
            // expression to evaluate and no scope to build for it. This is the
            // common case, and skipping the scope — which exists only to give an
            // initializer `this` and `super` — is most of what a construction costs.
            // The instance's fields are pushed in declaration order, and a name is
            // never declared twice along the chain, so a field is always added.
            let Some(expression) = &field.declaration.value else {
                instance.fields.borrow_mut().push((field.name.clone(), Value::Null));
                continue;
            };
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
            let value = self.expression(expression, &scope)?;
            instance.fields.borrow_mut().push((field.name.clone(), value));
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
            class.fields.borrow_mut().push(Rc::new(object::Field {
                declaration: field.clone(),
                owner: class.clone(),
                env: env.clone(),
                name: Rc::from(field.name.as_str()),
            }));
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

    /// The value of `instance.name` when the class declares `name` as a field,
    /// along with the field's position in the instance's field vector and whether
    /// that position may be remembered.
    ///
    /// `None` means the name is not a field at all, so the caller can look for a
    /// method instead. The returned position is what lets the compiled engine turn
    /// a repeat access into a plain index.
    fn instance_field(
        &mut self,
        base: &Base,
        instance: &Rc<object::Instance>,
        name: &str,
        env: &Env,
    ) -> Result<Option<(u32, Value, bool)>, Signal> {
        let Some(field) = instance.class.find_field(name) else {
            return Ok(None);
        };
        self.accessible(base, &field.declaration.access, &field.owner, env)?;
        let fields = instance.fields.borrow();
        let Some(position) = fields.iter().position(|(declared, _)| *declared == field.name) else {
            return Ok(None);
        };
        let value = fields[position].1.clone();
        drop(fields);
        // A private or protected field is checked against the caller, which can
        // differ between two accesses of one receiver, so only a public one is
        // remembered.
        let remember = field.declaration.access.is_empty() || field.declaration.access == "public";
        Ok(Some((position as u32, value, remember)))
    }

    fn property(&mut self, base: &Base, receiver: &Value, name: &str, env: &Env) -> Eval {
        match receiver {
            Value::String(text) => {
                if name == "size" || name == "length" {
                    return Ok(self.length_method(name, receiver.clone()));
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
                    return Ok(self.length_method(name, receiver.clone()));
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
            Value::Tuple(_) => {
                if name == "size" || name == "length" {
                    return Ok(self.length_method(name, receiver.clone()));
                }
                if let Some(method) = methods::conversion_method(name, receiver) {
                    return Ok(method);
                }
                // A tuple is immutable, so it has no in-place methods at all.
                Err(runtime(base, format!("tuple has no property {:?}", name)))
            }
            Value::Set(set) => {
                if name == "size" || name == "length" {
                    return Ok(self.length_method(name, receiver.clone()));
                }
                if let Some(method) = methods::set_method(name, set.clone()) {
                    return Ok(method);
                }
                if let Some(method) = methods::conversion_method(name, receiver) {
                    return Ok(method);
                }
                Err(runtime(base, format!("set has no property {:?}", name)))
            }
            Value::Dict(dict) => {
                if name == "size" || name == "length" {
                    return Ok(self.length_method(name, receiver.clone()));
                }
                if let Some(method) = methods::dict_method(name, dict.clone()) {
                    return Ok(method);
                }
                if let Some(method) = methods::conversion_method(name, receiver) {
                    return Ok(method);
                }
                Err(runtime(base, format!("dict has no property {:?}", name)))
            }
            Value::Module(module) => self.module_member(base, module, name),
            Value::File(file) => self.file_property(base, file, name),
            Value::Instance(instance) => {
                // Look the member up once: a field and a method cannot share a
                // name, so at most one class-chain walk is needed, not the three
                // that a separate access check plus lookup would cost.
                if let Some((_, value, _)) = self.instance_field(base, instance, name, env)? {
                    return Ok(value);
                }
                if let Some(method) = instance.class.find_method(name) {
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
                // A value with no member namespace of its own still rejects the
                // length spellings the way `len` does, so all three agree on
                // every value rather than only on the collections.
                if name == "size" || name == "length" {
                    return Err(runtime(base, builtins::length_error(name, receiver)));
                }
                if let Some(method) = methods::conversion_method(name, receiver) {
                    return Ok(method);
                }
                Err(runtime(base, format!("{} has no properties", other.type_name())))
            }
        }
    }

    /// Builds `size`/`length` for a receiver.
    ///
    /// The method captures the receiver and reads it when it is called, exactly as
    /// `len` does, so `const read = items.size; items.pop();` still reports the
    /// current length. The three spellings share [`builtins::length_of`], so one
    /// rule decides what they accept and what they return.
    fn length_method(&self, name: &str, receiver: Value) -> Value {
        let label = name.to_string();
        Value::Builtin(object::Builtin::new(name, move |_host, arguments, keywords| {
            if !keywords.is_empty() {
                return Err(format!("{} does not accept keyword arguments", label));
            }
            if !arguments.is_empty() {
                return Err(format!("{} expects 0 arguments, got {}", label, arguments.len()));
            }
            match builtins::length_of(&receiver) {
                Some(length) => Ok(Value::Number(length)),
                None => Err(builtins::length_error(&label, &receiver)),
            }
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
            "sys.exit" => self.exit_builtin(base, arguments, keywords),
            _ => Err(runtime(base, format!("unknown aggregate built-in {:?}", name))),
        }
    }

    /// Implements `sys.exit(status)`, which stops the program.
    ///
    /// The status is Python's: an integer is the process status, `null` means
    /// success, and any other value is printed on standard error with status 1.
    /// The language has one number type, so an integral number is a status and a
    /// fractional one is a message.
    fn exit_builtin(&mut self, base: &Base, arguments: &[Value], keywords: &[Keyword]) -> Eval {
        if !keywords.is_empty() {
            return Err(runtime(base, "sys.exit does not accept keyword arguments".to_string()));
        }
        if arguments.len() > 1 {
            return Err(runtime(base, format!("sys.exit expects at most 1 argument, got {}", arguments.len())));
        }
        let exit = match arguments.first() {
            None | Some(Value::Null) => Exit::default(),
            Some(Value::Bool(flag)) => Exit { code: i32::from(*flag), message: None },
            Some(Value::Number(number)) => match exit_code(*number) {
                Some(code) => Exit { code, message: None },
                None => Exit { code: 1, message: Some(object::format(&arguments[0])) },
            },
            Some(other) => Exit { code: 1, message: Some(object::format(other)) },
        };
        Err(Signal::Exit(exit))
    }

    fn map_builtin(&mut self, base: &Base, arguments: &[Value], keywords: &[Keyword], env: &Env) -> Eval {
        // `map` is lazy: it captures the source and the callable and applies the
        // callable only as elements are pulled.
        let (input, function) = callable_source_arguments("map", arguments, keywords).map_err(|message| runtime(base, message))?;
        let source = self.open_iterator(base, input)?;
        Ok(Value::iterator(object::IteratorKind::Map { source, function, env: env.clone() }))
    }

    fn filter_builtin(&mut self, base: &Base, arguments: &[Value], keywords: &[Keyword], env: &Env) -> Eval {
        // `filter` is lazy for the same reason `map` is: the predicate runs once
        // per element, when the element is needed.
        let (input, function) = callable_source_arguments("filter", arguments, keywords).map_err(|message| runtime(base, message))?;
        let source = self.open_iterator(base, input)?;
        Ok(Value::iterator(object::IteratorKind::Filter { source, function, env: env.clone() }))
    }

    fn sorted_builtin(&mut self, base: &Base, arguments: &[Value], keywords: &[Keyword], env: &Env) -> Eval {
        if arguments.len() != 1 {
            return Err(runtime(base, format!("sorted expects 1 argument, got {}", arguments.len())));
        }
        let items = if arguments[0].is_iterator() {
            // A sort needs every element, so a lazy source is drained here.
            self.materialize(base, &arguments[0])?
        } else {
            let Some(items) = arguments[0].sequence() else {
                return Err(runtime(base, format!("sorted expects a list or tuple, got {}", arguments[0].type_name())));
            };
            items
        };
        let (key, reverse) = sort_options("sorted", keywords).map_err(|message| runtime(base, message))?;
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
                    keys.push(self.call(base, key.clone(), std::slice::from_mut(&mut item.clone()), &[], env)?);
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

/// The source and the callable of a `map` or `filter` call.
///
/// The source is returned as a value rather than a list, so a lazy iterator is
/// passed on untouched and stays lazy; a list or tuple is accepted exactly as
/// before. Any other type is rejected here, with the call's own name.
fn callable_source_arguments(name: &str, arguments: &[Value], keywords: &[Keyword]) -> Result<(Value, Value), String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.len() != 2 {
        return Err(format!("{} expects 2 arguments, got {}", name, arguments.len()));
    }
    match &arguments[0] {
        Value::List(_) | Value::Tuple(_) | Value::Iterator(_) => Ok((arguments[0].clone(), arguments[1].clone())),
        other => Err(format!("{} expects a list or tuple, got {}", name, other.type_name())),
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

/// One step of a `range` iterator: the value it yields, and where its next step starts. `None` once
/// the range is finished.
///
/// It is written once because two paths step a range — the general one, which swaps the kind out of
/// its state so that user code can run in between, and the in-place one for the kinds that run no
/// user code at all.
fn range_step(current: i64, stop: i64, step: i64) -> Option<(f64, i64)> {
    if if step > 0 { current < stop } else { current > stop } {
        Some((current as f64, current.checked_add(step).unwrap_or(stop)))
    } else {
        None
    }
}

/// `first % second`, with the sign following the divisor — the floored modulo the language
/// documents, which is Python's rather than C's.
///
/// It is written once here because three callers need it: the compiled engine's inlined
/// operators, the interpreter's numeric fast path, and its general arithmetic arm. They used
/// to hold three copies of this expression, which is a place for two engines to drift.
///
/// An integral operand pair is the case that looks worth special-casing, and it is not. `%` on
/// `f64` reaches the platform's `fmod`, which returns its dividend immediately when that is
/// smaller than the divisor and otherwise runs a short scaling loop — cheaper than the long
/// division an `i64` remainder costs. Replacing it with an integer remainder was measured and
/// made both shapes of a modulo loop slower; see "A modulo is not worth an integer path" in
/// `docs/performance.md`.
#[inline]
pub(crate) fn modulo(first: f64, second: f64) -> f64 {
    let mut remainder = first % second;
    if remainder == 0.0 {
        // A zero remainder takes the divisor's sign, which is where "the sign follows the
        // divisor" lands at zero: `-10 % 5` is `0` and `10 % -5` is `-0`. `fmod` gives the
        // dividend's sign instead, so this is the one place the two rules disagree, and it is
        // Python's float rule that the language documents.
        return if second < 0.0 { -0.0 } else { 0.0 };
    }
    if (remainder < 0.0) != (second < 0.0) {
        remainder += second;
    }
    remainder
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
                        _ => modulo(first, second),
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
                    Value::Tuple(elements) => {
                        let found = elements.iter().any(|element| object::equal(&left, element));
                        return Ok(Value::Bool(found));
                    }
                    Value::Set(set) => {
                        let exists = set.borrow().contains(&left).map_err(|message| runtime(base, message))?;
                        return Ok(Value::Bool(exists));
                    }
                    Value::Dict(dict) => {
                        let exists = dict.borrow().contains(&left).map_err(|message| runtime(base, message))?;
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
                    Value::Iterator(_) => {
                        // Membership walks a lazy iterator, stopping at the first
                        // match; like a `for` loop, it consumes what it reads.
                        let probe = right.clone();
                        loop {
                            match self.iterator_next(base, &probe)? {
                                Some(element) if object::equal(&left, &element) => return Ok(Value::Bool(true)),
                                Some(_) => continue,
                                None => return Ok(Value::Bool(false)),
                            }
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
                        _ => modulo(first, second),
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
            if !result.is_finite() {
                return Err(runtime(base, "non-finite numeric result".to_string()));
            }
            return Ok(Value::Number(result));
        }
        if let (Value::String(first), Value::String(second)) = (&left, &right) {
            match operator {
                // Sized once and filled, rather than built by `format!` and then
                // copied into a shared string: a concatenation allocates once.
                //
                // The two halves go into the buffer directly (`RcStr::concat`), because a
                // `String` built here and then copied into a shared string is that allocation
                // and that copy over again — and for a result short enough to live in the
                // handle, both of them.
                "+" => return Ok(Value::String(object::RcStr::concat(first, second))),
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
            if let (Value::Tuple(first), Value::Tuple(second)) = (&left, &right) {
                let mut items = first.to_vec();
                items.extend(second.iter().cloned());
                return Ok(Value::tuple(items));
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
            Value::Tuple(elements) => {
                let index = self.index_at(base, &key, elements.len())?;
                Ok(elements[index].clone())
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

    /// Stores `instance.name = value`, matching the interpreter's property target,
    /// and reports the field's position when that position may be remembered for the
    /// next store to the same receiver.
    ///
    /// A position is only worth remembering for a public, non-`const` field, because
    /// the check that a remembered store skips — visibility and `const` — is the one
    /// that can differ between two stores of one receiver.
    fn store_instance_field(
        &mut self,
        base: &Base,
        instance: &Rc<object::Instance>,
        name: &str,
        value: Value,
        env: &Env,
    ) -> Result<Option<u32>, Signal> {
        self.member_access(base, &instance.class, name, env, true)?;
        let Some(field) = instance.class.find_field(name) else {
            return Err(runtime(
                base,
                format!("{} has no field {:?}; declare it in the class with var or const", instance.class.name, name),
            ));
        };
        let position = instance.fields.borrow().iter().position(|(declared, _)| *declared == field.name);
        let Some(position) = position else {
            return Err(runtime(base, format!("{} has no field {:?}", instance.class.name, name)));
        };
        instance.fields.borrow_mut()[position].1 = value;
        let remember = (field.declaration.access.is_empty() || field.declaration.access == "public") && !field.declaration.constant;
        Ok(if remember { Some(position as u32) } else { None })
    }

    /// Applies `++`/`--` to a named variable, matching the interpreter's update.
    fn update_name(&mut self, base: &Base, env: &Env, name: &str, decrement: bool, prefix: bool) -> Eval {
        let previous = match object::get(env, name).map_err(|message| runtime(base, message))? {
            Value::Number(number) => number,
            _ => return Err(runtime(base, format!("{} requires a number", if decrement { "--" } else { "++" }))),
        };
        let updated = if decrement { previous - 1.0 } else { previous + 1.0 };
        if !updated.is_finite() {
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
    let fill: String = std::iter::repeat_n(pad, width - length).collect();
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
            let left_fill: String = std::iter::repeat_n(pad, left).collect();
            let right_fill: String = std::iter::repeat_n(pad, right).collect();
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
        let elements = match &value {
            Value::List(elements) => elements.borrow().clone(),
            Value::Tuple(elements) => elements.to_vec(),
            other => {
                return Err(runtime(base, format!("cannot destructure {} with [ ... ]; expected a list or tuple", other.type_name())));
            }
        };
        if elements.len() != names.len() {
            return Err(runtime(base, format!("cannot unpack {} values into {} names", elements.len(), names.len())));
        }
        for (index, name) in names.iter().enumerate() {
            object::define(env, name, elements[index].clone(), *constant).map_err(|message| runtime(base, message))?;
        }
        Ok(None)
    }

    /// Reads one member a module exposes, reporting a name it keeps private.
    ///
    /// A module shares only the names a file marked with `export`, so this is the
    /// single gate every member access goes through: `module.member` and each name a
    /// named import asked for.
    fn module_member(&self, base: &Base, module: &Rc<Module>, name: &str) -> Eval {
        match module.member(name) {
            Some(value) => Ok(value),
            None => Err(runtime(base, format!("module {:?} has no export {:?}", module.name, name))),
        }
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
        builtins::install(&builtins, &self.arguments);
        let scope = object::Environment::new(Some(builtins));
        scope.borrow_mut().source_path = filename.clone();
        let exports: Vec<String> = program.exports.iter().map(|exported| exported.name.clone()).collect();
        let module = Module::with_exports(filename.clone(), scope.clone(), exports);
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

    /// Prints the instruction list a program compiles to.
    ///
    /// The shape of that list is what the engine's cost is made of, so a fusion
    /// that stops matching is a performance change with no failing test; this makes
    /// it visible. Run with `--ignored --nocapture`:
    ///
    /// ```sh
    /// cargo test -p tiger-evaluator --lib -- --ignored --nocapture disassembles
    /// ```
    #[test]
    #[ignore = "prints a disassembly; run explicitly with --ignored --nocapture"]
    fn disassembles_the_hot_shapes() {
        println!(
            "sizes: Value {} Option<Value> {} Instr {} Token {} Expr {} Stmt {}",
            std::mem::size_of::<Value>(),
            std::mem::size_of::<Option<Value>>(),
            std::mem::size_of::<tiger_object::bytecode::Instr>(),
            std::mem::size_of::<tiger_lexer::Token>(),
            std::mem::size_of::<tiger_ast::Expr>(),
            std::mem::size_of::<tiger_ast::Stmt>(),
        );
        println!(
            "       Signal {} Eval {} RuntimeError {} Keyword {} Base {}",
            std::mem::size_of::<Signal>(),
            std::mem::size_of::<Eval>(),
            std::mem::size_of::<tiger_object::RuntimeError>(),
            std::mem::size_of::<tiger_object::Keyword>(),
            std::mem::size_of::<tiger_ast::Base>(),
        );
        for (name, source) in [
            ("loop/count", "var s = 0;\nfor (var i = 0; i < 200000; ++i) { s += 1; }\nprint(s);"),
            ("list/read", "const a = [1, 2, 3];\nvar s = 0;\nfor (var i = 0; i < 3; ++i) { s += a[i]; }\nprint(s);"),
            ("list/write", "const a = [1, 2, 3];\nfor (var i = 0; i < 3; ++i) { a[i] = i; }\nprint(a);"),
            ("string/concat", "var s = \"\";\nfor (var i = 0; i < 3; ++i) { s += \"ab\"; }\nprint(len(s));"),
            ("list/append", "const d = {\"a\": 1};\nvar ks = d.keys();\nks += [\"b\"];\nprint(d.keys(), ks);"),
            ("index/accumulate", "const d = {\"x\": 1, \"y\": 2};\nvar sum = 0;\nfor k in d.keys() { sum += d[k]; }\nprint(sum);"),
            ("call", "function f(v) { return v; }\nvar s = 0;\nfor (var i = 0; i < 3; ++i) { s += f(1); }\nprint(s);"),
            // The body the paired protocol's `recursion` section runs, where the ratio
            // against CPython is the widest: two calls and an add per frame.
            ("recursion", "function fib(n) { if (n < 2) { return n; } return fib(n - 1) + fib(n - 2); }\nprint(fib(3));"),
            ("method/call", "class Box { var v = 1; Box() {} function get() { return this.v; } }\nconst b = Box();\nvar s = 0;\nfor (var i = 0; i < 3; ++i) { s += b.get(); }\nprint(s);"),
            ("method/string", "const w = \"abc\";\nvar s = 0;\nfor (var i = 0; i < 3; ++i) { s += len(w.upper()); }\nprint(s);"),
            ("field", "class B { var v = 1; B() {} }\nconst b = B();\nvar s = 0;\nfor (var i = 0; i < 3; ++i) { s += b.v; }\nprint(s);"),
            ("dict", "const d = {};\nfor (var i = 0; i < 3; ++i) { d[\"k\" + str(i)] = i; }\nprint(len(d));"),
        ] {
            let program = tiger_parser::parse(source).expect("the fixture must parse");
            let Ok(chunk) = compiler::compile(&program.statements, "") else {
                println!("== {} (does not lower)", name);
                continue;
            };
            println!("== {} ({} instructions)", name, chunk.code.len());
            for (index, instr) in chunk.code.iter().enumerate() {
                println!("  {:>3}  {:?}", index, instr);
            }
        }
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
        assert_eq!(run_capture("print(list(map([1, 2, 3], function(x) { return x * 2; })));").unwrap(), "[2, 4, 6]\n");
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
