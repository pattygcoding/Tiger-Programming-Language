//! A linear bytecode form for the optional execution engine.
//!
//! The compiler in `tiger-evaluator` lowers a program to a [`Chunk`] of
//! instructions and a small virtual machine runs them in a flat loop instead of
//! recursively walking the AST. The instruction set reuses the runtime's own
//! operations (`Value`, environments, operators), so a compiled program has
//! exactly the semantics of the tree-walking interpreter.

use std::cell::RefCell;
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
    /// Push the value in slot `slot`, `hops` scopes up the chain.
    ///
    /// The compiler resolved the name to a slot in its own scope model, so this
    /// indexes straight into the scope instead of scanning it. `name` is kept for
    /// diagnostics and as the fallback when the slot is not there.
    LoadLocal { name: u32, hops: u32, slot: u32 },
    /// Push the value in flat slot `slot` of the current frame.
    ///
    /// A flat slot belongs to the frame alone — no nested function can reach it —
    /// so it is a plain index into the frame's locals with no borrow and no scope
    /// to find first.
    LoadFlat(u32),
    /// Push the `super` binding, with its dedicated error when unavailable.
    ///
    /// A method whose frame binds `super` flat carries its slot here; the engine
    /// then reads the frame's locals directly, exactly as [`Instr::LoadFlat`] does.
    /// Otherwise the binding is looked up by name, which is where a captured or
    /// absent `super` has to be found.
    LoadSuper(Option<u32>),
    /// Pop a value and reassign the existing variable `chunk.names[i]`.
    StoreName(u32),
    /// Pop a value into slot `slot`, `hops` scopes up the chain.
    StoreLocal { name: u32, hops: u32, slot: u32 },
    /// Pop a value into flat slot `slot`. `constant` is the binding's, which the
    /// compiler knows, so a write to a flat `const` is refused without a flag to
    /// read at run time.
    StoreFlat { name: u32, slot: u32, constant: bool },
    /// Pop a value and declare it as `chunk.names[i]` in the current scope.
    DefineName { name: u32, constant: bool },
    /// Pop a value and declare it at `slot` in the current scope.
    DefineLocal { name: u32, slot: u32, constant: bool },
    /// Pop a value and declare it in flat slot `slot`. `duplicate` is set when the
    /// compiler saw the name already declared in the same scope, which is an error
    /// raised when the declaration runs, exactly as the interpreter raises it.
    DefineFlat { name: u32, slot: u32, duplicate: bool },
    /// Pop two values and push whether they are equal, with the same rule the
    /// interpreter's `switch` uses, which never raises.
    Equal,
    /// Discard the top of the stack.
    Pop,
    /// Duplicate the top of the stack.
    Dup,
    /// Pop two values and push `left <op> right`. `op` selects an inlined fast
    /// path; `chunk.names[i]` is the operator for the fallback and diagnostics.
    Binary { op: Op, name: u32 },
    /// Push flat slot `a`, push flat slot `b`, then apply the tagged operator.
    ///
    /// This is the fused form of `LoadFlat(a)`, `LoadFlat(b)`, and
    /// `Binary { op, name }` in that order. The compiler rewrites the first of the
    /// three in place and the engine steps over the other two, so a pair of loads
    /// feeding one operator costs a single dispatch and touches the operand stack
    /// not at all.
    FlatBinaryFlat { a: u32, b: u32, op: Op, name: u32 },
    /// Push flat slot `a`, take constant `b`, then apply the tagged operator.
    ///
    /// The fused form of `LoadFlat(a)`, `Constant(b)`, and `Binary { op, name }`.
    FlatBinaryConstant { a: u32, b: u32, op: Op, name: u32 },
    /// Apply the tagged operator to constant `b` and the value below it, pushing the
    /// result.
    ///
    /// The fused form of `Constant(b)` and `Binary { op, name }`, where the left
    /// operand is whatever the preceding instruction left on the stack.
    BinaryConstant { b: u32, op: Op, name: u32 },
    /// Push the property `name` of flat slot `value`.
    ///
    /// The fused form of `LoadFlat(value)` and `GetProperty(name)`. `cache` is the
    /// inline-cache slot this access uses, which is the one the `GetProperty` it
    /// replaced would have used.
    FlatProperty { value: u32, name: u32, cache: u32 },
    /// Push the value of the free name `name`, leaving the `Call` that follows to run
    /// as it is.
    ///
    /// The fused form of `LoadName(name)` and the call that consumes it, so the two
    /// share one dispatch. The instruction after this one is that call.
    CallName { name: u32 },
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
    /// Pop `count` values and push a tuple containing them in order.
    BuildTuple(u32),
    /// Pop `count` values and push a set of them, rejecting an unhashable one.
    BuildSet(u32),
    /// Push the comprehension `chunk.comprehensions[i]` builds.
    Comprehension(u32),
    /// Push a lazy generator iterator for `chunk.generators[i]`.
    Generator(u32),
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
    /// Increment or decrement the variable in slot `slot`, `hops` scopes up.
    UpdateLocal { name: u32, hops: u32, slot: u32, decrement: bool, prefix: bool },
    /// Pop a value and push its f-string field rendered with `chunk.names[i]`.
    ApplyFormat(u32),
    /// Pop a value and push `format(value)`.
    Format,
    /// Return the top of the stack from the current function.
    Return,
    /// Leave an isolated block — a try body, a catch body, or a `finally` — with
    /// the top of the stack, as a `return` from the function the block belongs to.
    ReturnEscape,
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
    /// Runs `chunk.tries[i]`: the body, then its catch clauses, then its `finally`.
    Try(u32),
    /// Pop a collection and push `chunk.slices[i]` applied to it.
    Slice(u32),
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

/// The bounds of one slice expression, which the engine evaluates against the
/// collection's length exactly as the interpreter does.
pub struct SliceProto {
    pub start: Option<Box<tiger_ast::Expr>>,
    pub stop: Option<Box<tiger_ast::Expr>>,
    pub step: Option<Box<tiger_ast::Expr>>,
}

/// A compiled comprehension: what it builds and the clauses that drive it.
///
/// Both parts stay as AST because the engine evaluates them through the same
/// helpers the interpreter uses, which is what keeps the two engines' results and
/// diagnostics identical.
pub struct CompProto {
    pub body: tiger_ast::ComprehensionBody,
    pub clauses: Vec<tiger_ast::ComprehensionClause>,
}

/// A compiled generator expression: the element it yields and the clauses that
/// drive it.
///
/// Kept as AST for the same reason a comprehension is: the engine runs it
/// through the interpreter's own helpers, so the two engines agree. A lazy
/// iterator holds this prototype behind an `Rc`, so stepping it clones a pointer,
/// not the tree.
pub struct GenProto {
    pub body: tiger_ast::Expr,
    pub clauses: Vec<tiger_ast::ComprehensionClause>,
}

/// One catch clause of a `try`: the name the caught value binds to, an optional
/// condition that must be truthy for the clause to take the error, and the body.
pub struct CatchProto {
    pub name: Rc<str>,
    pub condition: Option<tiger_ast::Expr>,
    pub body: Rc<Chunk>,
}

/// A compiled `try`/`catch`/`finally`: the body, the catch clauses in order, and
/// the `finally` block when there is one.
///
/// Each part is its own chunk, so a `return` inside one has to leave that chunk;
/// see [`Chunk::escape_return`].
pub struct TryProto {
    pub body: Rc<Chunk>,
    pub clauses: Vec<CatchProto>,
    pub finally: Option<Rc<Chunk>>,
}

/// Where a compiled function's call bindings go.
///
/// `None` means the binding stays in the shared environment, which is where a
/// nested function will look for it; `Some(slot)` means the frame's own flat
/// locals, which the engine writes directly without touching a scope.
#[derive(Default)]
pub struct CallLayout {
    /// A method's `this` and `super`, which the engine binds before any parameter.
    pub this: Option<u32>,
    pub super_: Option<u32>,
    /// One entry per parameter, in declaration order.
    pub parameters: Vec<Option<u32>>,
    /// The `*rest` list and the `**options` dictionary, when the function has them.
    pub variadic: Option<u32>,
    pub keyword: Option<u32>,
    /// Whether every parameter went to a flat slot, which is the case a call
    /// binds without looking at the layout again.
    pub flat_parameters: bool,
    /// Whether the body — or a nested function inside it — can reach `super`.
    ///
    /// Binding `super` builds a fresh object, so a method that never mentions it
    /// skips that allocation. A name that reaches above the frame walks the
    /// environment chain, and the compiler knows whether anything in the body does.
    pub super_used: bool,
}

/// What an inline cache remembered for one property instruction.
#[derive(Clone)]
pub enum CachedProperty {
    /// A value that cannot change for one receiver: a module binding, a class
    /// method, or a builtin. The receiver is the guard.
    Value { receiver: Value, value: Value },
    /// A field of an instance of one class.
    ///
    /// The class is the guard rather than the receiver, because a field's position
    /// is fixed once its class is defined. A store into a freshly built instance is
    /// then a plain index as well, which is what a constructor does, and the value
    /// is still read fresh on every access.
    Field { class: Rc<crate::Class>, position: u32 },
}

/// A compiled script or function body.
pub struct Chunk {
    pub code: Vec<Instr>,
    pub constants: Vec<Value>,
    pub names: Vec<Rc<str>>,
    pub functions: Vec<Rc<FunctionProto>>,
    pub classes: Vec<Rc<ClassProto>>,
    pub tries: Vec<Rc<TryProto>>,
    pub slices: Vec<Rc<SliceProto>>,
    pub comprehensions: Vec<Rc<CompProto>>,
    pub generators: Vec<Rc<GenProto>>,
    /// How many flat locals this frame needs: the number of its declarations that
    /// no nested function can reach.
    pub flat_count: usize,
    /// Where a call's arguments bind, which is what lets the engine write a
    /// parameter straight into a flat slot.
    pub call_layout: CallLayout,
    /// Whether running this chunk needs an environment of its own. When every name
    /// it binds is flat, it does not, and the engine runs the body directly in the
    /// environment the function captured.
    pub frame_needed: bool,
    /// One monomorphic inline cache slot per instruction: the receiver a property
    /// access last resolved, and the value it resolved to.
    ///
    /// The receiver is held rather than only its address, so the slot cannot be
    /// mistaken for a different object that later reuses that address. The engine
    /// fills and reads it; the compiler only makes room.
    pub property_cache: RefCell<Vec<Option<CachedProperty>>>,
    /// One `(line, column)` per instruction, indexed by program counter.
    pub positions: Vec<(u32, u32)>,
    /// The source name every position in this chunk belongs to.
    pub source: Rc<str>,
}

impl Chunk {
    /// The source position of instruction `pc`, for diagnostics.
    pub fn base(&self, pc: usize) -> tiger_ast::Base {
        self.base_at(self.position(pc))
    }

    /// The `(line, column)` of instruction `pc`.
    ///
    /// A plain copy, so the engine loop can carry an instruction's position for
    /// free and only build a [`Base`](tiger_ast::Base) — which clones the shared
    /// source name — where a diagnostic actually needs one.
    pub fn position(&self, pc: usize) -> (u32, u32) {
        self.positions.get(pc).copied().unwrap_or((0, 0))
    }

    /// Turns a [`position`](Chunk::position) into a positioned diagnostic base.
    pub fn base_at(&self, position: (u32, u32)) -> tiger_ast::Base {
        tiger_ast::Base { line: position.0, column: position.1, source: self.source.clone() }
    }
}
