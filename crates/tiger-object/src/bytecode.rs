//! A linear bytecode form for the optional execution engine.
//!
//! The compiler in `tiger-evaluator` lowers a program to a [`Chunk`] of
//! instructions and a small virtual machine runs them in a flat loop instead of
//! recursively walking the AST. The instruction set reuses the runtime's own
//! operations (`Value`, environments, operators), so a compiled program has
//! exactly the semantics of the tree-walking interpreter.

use std::rc::Rc;

use crate::Value;

/// A binary operator the engine can take a fast path on. `Other` falls back to
/// the general operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Other,
}

/// One bytecode instruction. All fields are plain integers so the enum is `Copy`
/// and dispatch is a cheap value copy.
#[derive(Clone, Copy, Debug)]
pub enum Instr {
    /// Push `chunk.constants[i]`.
    Constant(u32),
    /// Push the value of variable `chunk.names[i]`, walking the scope chain.
    LoadName(u32),
    /// Push the `super` binding, with its dedicated error when unavailable.
    LoadSuper,
    /// Pop a value and reassign the existing variable `chunk.names[i]`.
    StoreName(u32),
    /// Pop a value and declare it as `chunk.names[i]` in the current scope.
    DefineName { name: u32, constant: bool },
    /// Discard the top of the stack.
    Pop,
    /// Duplicate the top of the stack.
    Dup,
    /// Pop two values and push `left <op> right`. `op` selects an inlined fast
    /// path; `chunk.names[i]` is the operator for the fallback and diagnostics.
    Binary { op: Op, name: u32 },
    /// Pop one value and push `<op> value`; `chunk.names[i]` is the operator.
    Unary(u32),
    /// Unconditional jump to the absolute instruction index `target`.
    Jump(u32),
    /// Pop a value and jump to `target` when it is falsy.
    JumpIfFalse(u32),
    /// For `and`: jump (leaving the value) when falsy, otherwise pop and continue.
    JumpIfFalseOrPop(u32),
    /// For `or`: jump (leaving the value) when truthy, otherwise pop and continue.
    JumpIfTrueOrPop(u32),
    /// Pop `count` values and push a list containing them in order.
    BuildList(u32),
    /// Pop `count` key/value pairs and push a dictionary of them.
    BuildDict(u32),
    /// Push a closure over `chunk.functions[i]`, capturing the current scope.
    MakeFunction(u32),
    /// Define `chunk.classes[i]`, registering its compiled method bodies.
    Class(u32),
    /// Call a callable: the stack holds `[callee, positional.., name, value, ..]`.
    Call { positional: u32, keywords: u32 },
    /// Pop a key and a collection, then push the indexed element.
    GetIndex,
    /// Pop a receiver, then push its named member.
    GetProperty(u32),
    /// Pop a value, a key, and a collection, then store `collection[key] = value`.
    SetIndex,
    /// Pop a value and a receiver, then store the named member.
    SetProperty(u32),
    /// Increment or decrement the named variable, pushing it per `prefix`.
    UpdateName { name: u32, decrement: bool, prefix: bool },
    /// Pop a value and push its f-string field rendered with `chunk.names[i]`.
    ApplyFormat(u32),
    /// Pop a value and push `format(value)`.
    Format,
    /// Return the top of the stack from the current function.
    Return,
    /// Push `null`.
    Null,
    /// Enter a child scope.
    PushScope,
    /// Leave the current scope.
    PopScope,
    /// Pop an iterable and push an iterator over its snapshot.
    IterInit,
    /// Push the next iterated value, or jump to `target` when the iteration ends.
    IterNext(u32),
    /// Discard the innermost `for ... in` iterator, as `break` out of the loop does.
    IterDrop,
    /// Pop a value and raise it as a `throw`.
    Throw,
    /// Import `chunk.names[i]` and bind it as `chunk.names[j]`.
    Import { path: u32, name: u32 },
}

/// A compiled function prototype: a declaration and its compiled body.
pub struct FunctionProto {
    pub declaration: Rc<tiger_ast::FunctionDecl>,
    pub code: Rc<Chunk>,
}

/// A compiled class: its declaration plus the compiled constructor and method
/// bodies, so a class created by the VM has compiled methods rather than AST ones.
pub struct ClassProto {
    pub declaration: Rc<tiger_ast::ClassDecl>,
    pub constructor: Option<Rc<Chunk>>,
    pub methods: Vec<Rc<Chunk>>,
}

/// A compiled script or function body.
pub struct Chunk {
    pub code: Vec<Instr>,
    pub constants: Vec<Value>,
    pub names: Vec<Rc<str>>,
    pub functions: Vec<Rc<FunctionProto>>,
    pub classes: Vec<Rc<ClassProto>>,
    /// One `(line, column)` per instruction, indexed by program counter.
    pub positions: Vec<(u32, u32)>,
    /// The source name every position in this chunk belongs to.
    pub source: Rc<str>,
}

impl Chunk {
    /// The source position of instruction `pc`, for diagnostics.
    pub fn base(&self, pc: usize) -> tiger_ast::Base {
        let (line, column) = self.positions.get(pc).copied().unwrap_or((0, 0));
        tiger_ast::Base { line, column, source: self.source.clone() }
    }
}
