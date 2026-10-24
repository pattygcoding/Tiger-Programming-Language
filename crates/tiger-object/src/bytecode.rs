//! A linear bytecode form for the optional execution engine.
//!
//! The compiler in `tiger-evaluator` lowers a program to a [`Chunk`] of
//! instructions and a small virtual machine runs them in a flat loop instead of
//! recursively walking the AST. The instruction set reuses the runtime's own
//! operations (`Value`, environments, operators), so a compiled program has
//! exactly the semantics of the tree-walking interpreter.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::{Env, Value};

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
    StoreFlat { name: u32, slot: u32, constant: bool, reuse: bool },
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
    /// Resolve an index target, read it, and leave it below the value read.
    ///
    /// Pops `[collection, key]` and pushes `[collection, key, value]`. It validates
    /// exactly as the interpreter's assignment target does — the collection must be
    /// a list or a dictionary, and a list's index must be in range — so a compound
    /// assignment or an update reports the same diagnostic, at the same position, as
    /// the interpreter. Leaving the collection and key in place is what lets the
    /// store that follows reuse them instead of evaluating the target twice.
    ReadIndexTarget,
    /// Resolve a property target, read it, and leave it below the value read.
    ///
    /// Pops `[receiver]` and pushes `[receiver, value]`, rejecting a receiver that
    /// is not an instance exactly as the interpreter's assignment target does.
    ReadPropertyTarget(u32),
    /// Pop two values and push `left <op> right`. `op` selects an inlined fast
    /// path; `chunk.names[i]` is the operator for the fallback and diagnostics.
    /// `settled` is the adaptive state [`FlatBinaryFlat`](Instr::FlatBinaryFlat)
    /// describes.
    Binary { op: Op, name: u32, settled: bool },
    /// Push flat slot `a`, push flat slot `b`, then apply the tagged operator.
    ///
    /// This is the fused form of `LoadFlat(a)`, `LoadFlat(b)`, and
    /// `Binary { op, name }` in that order. The compiler rewrites the first of the
    /// three in place and the engine steps over the other two, so a pair of loads
    /// feeding one operator costs a single dispatch and touches the operand stack
    /// not at all.
    ///
    /// `settled` is the site's adaptive state: false while it is still counting the
    /// operand types it sees, and true once it has given up on specializing. Every
    /// fused numeric site carries one, and the engine reads it from the instruction it
    /// has already copied, so a site that has settled costs nothing to skip. See
    /// [`SPECIALIZE_AFTER`].
    FlatBinaryFlat { a: u32, b: u32, op: Op, name: u32, settled: bool },
    /// Push flat slot `a`, take constant `b`, then apply the tagged operator.
    ///
    /// The fused form of `LoadFlat(a)`, `Constant(b)`, and `Binary { op, name }`.
    FlatBinaryConstant { a: u32, b: u32, op: Op, name: u32, settled: bool },
    /// Apply the tagged operator to constant `b` and the value below it, pushing the
    /// result.
    ///
    /// The fused form of `Constant(b)` and `Binary { op, name }`, where the left
    /// operand is whatever the preceding instruction left on the stack. `settled` is the
    /// adaptive state [`FlatBinaryFlat`](Instr::FlatBinaryFlat) describes.
    BinaryConstant { b: u32, op: Op, name: u32, settled: bool },
    /// A [`Binary`](Instr::Binary) whose two operands are known to be numbers.
    ///
    /// The operands come off the value stack, so this is the form a numeric operator
    /// takes when neither side is a flat local — a sum of two call results, or of two
    /// index reads. The adaptive counterpart of `FlatBinaryFlatNumber`.
    BinaryNumber { op: Op, name: u32 },
    /// A [`BinaryConstant`](Instr::BinaryConstant) whose operands are known to be
    /// numbers: the value below the top of the stack, and a numeric constant.
    BinaryConstantNumber { b: u32, op: Op, name: u32 },
    /// A [`BinaryStore`](Instr::BinaryStore) whose two operands are known to be numbers.
    ///
    /// This is `total += items[index]` and `total += f(x)` where the tail has turned out
    /// to be numeric, and it steps over the string case's in-place append, which two
    /// numbers can never take. It keeps `reuse` so that a site which deoptimizes onto a
    /// pair of strings still gets that append.
    BinaryStoreNumber { op: Op, name: u32, out: u32, reuse: bool },
    /// A [`FlatBinaryFlat`](Instr::FlatBinaryFlat) whose two operands are known to be
    /// numbers.
    ///
    /// This instruction is never compiled. The engine writes it in place of the one
    /// it replaces once that site has run [`SPECIALIZE_AFTER`]
    /// times with both operands numbers, and it is what PEP 659 does in CPython: the
    /// generic form counts the types it sees, and mutates its own bytecode into a form
    /// that does not have to ask again. Its fields are the ones it was built from, so a
    /// site that later sees any other type can be put back exactly where it came from.
    ///
    /// What it saves is the type question — the two tag tests and the `Option` the
    /// general path wraps its answer in — not the operator, which the compiler already
    /// resolved.
    FlatBinaryFlatNumber { a: u32, b: u32, op: Op, name: u32 },
    /// A [`FlatBinaryConstant`](Instr::FlatBinaryConstant) whose operands are known to
    /// be numbers. The adaptive counterpart of `FlatBinaryFlatNumber`.
    FlatBinaryConstantNumber { a: u32, b: u32, op: Op, name: u32 },
    /// A [`FlatJumpUnlessFlat`](Instr::FlatJumpUnlessFlat) whose two operands are known
    /// to be numbers.
    ///
    /// A loop condition is where this pays most: `i < n` between two numbers is
    /// decided and branched in one instruction without ever building the `bool` the
    /// unfused form would.
    FlatJumpUnlessFlatNumber { a: u32, b: u32, op: Op, name: u32, target: u32 },
    /// A [`FlatJumpUnlessConstant`](Instr::FlatJumpUnlessConstant) whose operands are
    /// known to be numbers. The adaptive counterpart of `FlatJumpUnlessFlatNumber`.
    FlatJumpUnlessConstantNumber { a: u32, b: u32, op: Op, name: u32, target: u32 },
    /// A [`FlatBinaryFlatStore`](Instr::FlatBinaryFlatStore) whose two operands are
    /// known to be numbers.
    ///
    /// A numeric accumulator — `total += step` — is the shape this exists for, and it
    /// also steps over the string case's in-place append, which two numbers can never
    /// take.
    FlatBinaryFlatStoreNumber { a: u32, b: u32, op: Op, name: u32, out: u32 },
    /// A [`FlatBinaryConstantStore`](Instr::FlatBinaryConstantStore) whose operands are
    /// known to be numbers. The adaptive counterpart of `FlatBinaryFlatStoreNumber`.
    FlatBinaryConstantStoreNumber { a: u32, b: u32, op: Op, name: u32, out: u32 },
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
    /// Pop two values and jump to `target` when `left <op> right` is falsy.
    ///
    /// The fused form of `Binary { op, name }` and `JumpIfFalse`. A comparison
    /// whose result feeds only a branch — a loop condition, or an `if` header —
    /// never needs to become a `bool` value, so this skips building it and testing
    /// its truthiness.
    JumpUnless { op: Op, name: u32, target: u32 },
    /// Compare flat slots `a` and `b` and jump to `target` when the test is falsy.
    ///
    /// The fused form of `LoadFlat(a)`, `LoadFlat(b)`, `Binary { op, name }`, and
    /// `JumpIfFalse`: a counting loop's condition in one dispatch, with the operand
    /// stack untouched. `settled` is the adaptive state
    /// [`FlatBinaryFlat`](Instr::FlatBinaryFlat) describes.
    FlatJumpUnlessFlat { a: u32, b: u32, op: Op, name: u32, target: u32, settled: bool },
    /// Compare flat slot `a` with constant `b` and jump to `target` when falsy.
    ///
    /// The fused form of `LoadFlat(a)`, `Constant(b)`, `Binary { op, name }`, and
    /// `JumpIfFalse`, which is a loop against a literal bound. `settled` is the adaptive
    /// state [`FlatBinaryFlat`](Instr::FlatBinaryFlat) describes.
    FlatJumpUnlessConstant { a: u32, b: u32, op: Op, name: u32, target: u32, settled: bool },
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
    /// Pop a value into scratch slot `slot`, a flat local that no name addresses.
    ///
    /// A comprehension's accumulator lives in one: its loop body appends to it, and
    /// keeping it in the frame's own locals rather than on the operand stack is what
    /// lets the body use the stack normally.
    StoreScratch(u32),
    /// Pop a value and push it onto the list in scratch slot `slot`.
    Collect(u32),
    /// Pop a value and a key, then put the pair into the dictionary in scratch slot
    /// `slot`.
    ///
    /// A key the runtime rejects is reported where this instruction is, so the
    /// compiler places it at the key expression — which is where the interpreter's
    /// own comprehension reports it.
    CollectPair(u32),
    /// Push the comprehension `chunk.comprehensions[i]` builds.
    ///
    /// A comprehension whose clause variable nothing outside the frame reaches is
    /// lowered to its own loop instead (see `Compiler::comprehension`), so this is the
    /// shape that keeps the interpreter's own clause walker in use: the walker binds
    /// the clause variable by name, which is what a captured one needs.
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
    Call { positional: u32, keywords: u32, discard: bool },
    /// Call the member `name` of the receiver below the arguments: the stack holds
    /// `[receiver, positional.., name, value, ..]`.
    ///
    /// The fused form of `GetProperty(name)` and the `Call` that consumes it, which
    /// is what `receiver.name(...)` lowers to. It is also the one place a member is
    /// called without building the object a property read returns: a class method
    /// is invoked with the receiver, and a string's own method is dispatched by
    /// name, so neither builds a callable per access.
    ///
    /// `member` indexes [`Chunk::members`], which holds the position of the member
    /// name. Resolving a member reports from where the name is and a call that
    /// cannot be made reports from where the call is, and those differ, so the
    /// instruction's own position carries the second and this carries the first.
    ///
    /// The compiler only emits this when the arguments cannot be observed out of
    /// order: the member is resolved after they are evaluated, where the property
    /// read it replaces resolves it before, so an argument that could raise or do
    /// anything at all would move that work past the member's.
    CallMethod { name: u32, positional: u32, keywords: u32, member: u32, discard: bool },
    /// Pop a key and a collection, then push the indexed element.
    GetIndex,
    /// Increment or decrement the value in flat slot `slot`, then jump to `target`.
    ///
    /// The fused form of `UpdateFlat`, `Pop`, and `Jump`, which is what a `for`
    /// header's `++index` lowers to: the update's value is discarded, so this is a
    /// counting loop's whole step in one dispatch. A write to a `const` is left to
    /// the unfused form, which raises the error.
    UpdateFlatJump { name: u32, slot: u32, decrement: bool, target: u32 },
    /// Store `locals[out] = locals[a] <op> locals[b]`, computing an operator whose
    /// operands are both flat locals.
    ///
    /// The fused form of two loads, `Binary`, and `StoreFlat` — an assignment such as
    /// `total = total + value`. A store into a `const` is left to the unfused form.
    /// `settled` is the adaptive state [`FlatBinaryFlat`](Instr::FlatBinaryFlat)
    /// describes, and it is what a string accumulator does not need: a site that is not
    /// numeric keeps its instruction, because the in-place append that shape has is
    /// worth more than the type question specializing would save.
    FlatBinaryFlatStore { a: u32, b: u32, op: Op, name: u32, out: u32, settled: bool },
    /// Store `locals[out] = locals[a] <op> constants[b]`, computing an operator whose
    /// right operand is a constant.
    ///
    /// The fused form of a load, `Constant`, `Binary`, and `StoreFlat` — an
    /// assignment such as `total = total + 1`. `settled` is the adaptive state
    /// [`FlatBinaryFlat`](Instr::FlatBinaryFlat) describes.
    FlatBinaryConstantStore { a: u32, b: u32, op: Op, name: u32, out: u32, settled: bool },
    /// Store `locals[out] = a <op> b` for two operands taken from the value stack.
    ///
    /// The fused form of `Binary` and `StoreFlat` — the tail of any assignment whose
    /// value is computed from a call, an index, or a property, which is the one shape
    /// the both-flat forms above cannot reach. A store into a `const` is left to the
    /// unfused form, which raises the error.
    BinaryStore { op: Op, name: u32, out: u32, reuse: bool, settled: bool },
    /// Push `locals[a][locals[b]]`, reading an index whose collection and key are
    /// both flat locals.
    ///
    /// The fused form of `LoadFlat(a)`, `LoadFlat(b)`, and `GetIndex`, which is what
    /// `items[index]` lowers to: one dispatch instead of three, and the operand stack
    /// is untouched.
    FlatIndexFlat { a: u32, b: u32 },
    /// Push `locals[a][constants[b]]`, reading an index whose key is a constant.
    ///
    /// The fused form of `LoadFlat(a)`, `Constant(b)`, and `GetIndex`, which is what
    /// `items[0]` lowers to.
    FlatIndexConstant { a: u32, b: u32 },
    /// Store `locals[collection][locals[key]] = locals[value]`, writing an index
    /// whose value, collection, and key are all flat locals.
    ///
    /// The fused form of three loads and `SetIndex`, which is what `items[i] = n`
    /// lowers to.
    FlatStoreIndex { value: u32, collection: u32, key: u32 },
    /// Pop a receiver, then push its named member.
    GetProperty(u32),
    /// Pop a value, a key, and a collection, then store `collection[key] = value`.
    SetIndex,
    /// Pop a value and a receiver, then store the named member.
    SetProperty(u32),
    /// Pop a value, a key, and a collection, then store `collection[key] = value`,
    /// where the collection and key are already below the value.
    ///
    /// The tail of a read-modify-write: the target was resolved once and read, so
    /// `[collection, key, result]` is already on the stack and this consumes it
    /// without evaluating the target again.
    SetIndexKeep,
    /// Pop a value and a receiver, then store the named member, where the receiver
    /// is already below the value.
    ///
    /// The property counterpart of [`Instr::SetIndexKeep`].
    SetPropertyKeep(u32),
    /// Increment or decrement `collection[key]` in place, pushing it per `prefix`.
    ///
    /// Pops `[collection, key, value]`, requires a number, stores `value ± 1` back
    /// into the same place, and pushes the updated or the previous value. Resolving
    /// the target once is what makes `items[i]++` a single read and write, exactly
    /// as the interpreter's update target does it.
    UpdateIndex { name: u32, decrement: bool, prefix: bool },
    /// Increment or decrement `receiver.name` in place, pushing it per `prefix`.
    ///
    /// The property counterpart of [`Instr::UpdateIndex`]. `property` names the
    /// member to write back and `name` is the operator's spelling, which the
    /// "requires a number" diagnostic quotes exactly as the interpreter does.
    UpdateProperty { property: u32, name: u32, decrement: bool, prefix: bool },
    /// Increment or decrement the named variable, pushing it per `prefix`.
    UpdateName { name: u32, decrement: bool, prefix: bool },
    /// Increment or decrement the variable in slot `slot`, `hops` scopes up.
    UpdateLocal { name: u32, hops: u32, slot: u32, decrement: bool, prefix: bool },
    /// Increment or decrement the value in flat slot `slot`.
    ///
    /// A counting loop's counter is the name this matters for: a `for` header's
    /// `++index` reaches no nested function, so it lives in the frame's own locals
    /// and its update needs no scope to find first. `constant` is the binding's, so
    /// a write to a flat `const` is refused without reading a flag at run time.
    UpdateFlat { name: u32, slot: u32, constant: bool, decrement: bool, prefix: bool },
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

/// A compiled comprehension that keeps the interpreter's clause walker: what it
/// builds and the clauses that drive it.
///
/// Both parts stay as AST because the engine evaluates them through the same helpers
/// the interpreter uses, which is what keeps the two engines' results — and their
/// diagnostics — identical.
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
    /// A class method a method call resolved.
    ///
    /// The class is the guard rather than the receiver, so every instance of one
    /// class takes the same hit: a loop over a list of instances calls each one's
    /// method without looking it up again, where a receiver-keyed slot would miss on
    /// every pass and build a bound method for each. A class's methods are fixed
    /// once it is defined, and a field cannot share a method's name, so no instance
    /// can shadow what this remembers. Only a public method is remembered — a private
    /// one's visibility is checked against the caller, which can differ between two
    /// calls — and only a `CallMethod` reads this.
    Method { class: Rc<crate::Class>, method: Rc<crate::Method> },
}

/// How many passes a numeric site runs before it specializes in place.
///
/// A site starts generic and counts the passes whose operands were both numbers. The
/// count is the whole guard against specializing on an atypical first pass — a branch
/// taken once, a call a program makes only at startup — and it is small because a loop
/// reaches it in microseconds while a site that never repeats never reaps the benefit.
/// A site that sees any other type inside the count stops counting and settles as a
/// general site instead, so a pair of strings or a dictionary operand costs the
/// counting once and never again.
pub const SPECIALIZE_AFTER: u16 = 10;

/// A compiled script or function body.
pub struct Chunk {
    /// The instruction stream.
    ///
    /// Each slot is a [`Cell`] rather than a bare [`Instr`] because the running engine
    /// rewrites it: a site that has counted enough passes with the same operand types
    /// replaces itself with a specialized instruction, and a site that later sees any
    /// other type puts the general instruction back. Reading a slot is a copy of a
    /// `Copy` value either way, so the interior mutability costs the dispatch loop
    /// nothing; only the site that specializes writes.
    pub code: Vec<Cell<Instr>>,
    /// How many warmup passes each instruction has counted, indexed by program counter.
    ///
    /// An entry is read only from a generic arm that is still deciding, so a site that
    /// has settled — specialized, or settled as a general one — never touches its
    /// counter again, and a long loop pays nothing for the machinery.
    pub warmup: Vec<Cell<u16>>,
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
    /// Whether a call to this chunk can take the engine's flat fast path.
    ///
    /// The fast path applies when the body needs no frame of its own, every parameter has
    /// a flat slot, `super` is unused, neither a variadic nor a keyword parameter is
    /// declared, and the frame's locals fit the engine's inline array. All of those are
    /// facts about the declaration rather than about any call, so the compiler decides
    /// them once and a call site tests this flag instead of six fields. The one fact a
    /// call site still has to check is its own argument count.
    pub flat_callable: bool,
    /// One monomorphic inline cache slot per instruction: the receiver a property
    /// access last resolved, and the value it resolved to.
    ///
    /// The receiver is held rather than only its address, so the slot cannot be
    /// mistaken for a different object that later reuses that address. The engine
    /// fills and reads it; the compiler only makes room.
    pub property_cache: RefCell<Vec<Option<CachedProperty>>>,
    /// One remembered name read per instruction: the environment it resolved in,
    /// the value it resolved to, and the binding epoch it resolved at.
    ///
    /// A name read in a loop resolves the same way every pass, and resolving one
    /// walks the scope chain — a hash of the name at every level. Two things make
    /// remembering it sound, and both are needed. The environment is part of the
    /// key because a chunk is not tied to one environment: three closures built in
    /// a loop share a chunk and each carries its own, so the same instruction runs
    /// against a different scope chain per closure. The epoch is what covers the
    /// rest: a hit is only used while no binding anywhere has been written since
    /// the slot was filled, so a `var` that shadows the name, or a scope that
    /// starts holding one, cannot be missed.
    pub name_cache: RefCell<Vec<Option<(Env, u64, Value)>>>,
    /// One `(line, column)` per instruction, indexed by program counter.
    pub positions: Vec<(u32, u32)>,
    /// The `(line, column)` of each member name a [`Instr::CallMethod`] resolves,
    /// indexed by that instruction's `member` field.
    ///
    /// Only a method call needs a second position, so a chunk with no `CallMethod`
    /// in it has an empty vector rather than one entry per instruction.
    pub members: Vec<(u32, u32)>,
    /// The source name every position in this chunk belongs to.
    pub source: Rc<str>,
}

impl Chunk {
    /// The instruction at `pc`.
    ///
    /// A copy out of the shared stream, which is all a read costs either way. The engine
    /// never holds a borrow across an instruction, so a site that rewrites itself is
    /// seen by the pass after the one that wrote it and never by that one.
    #[inline]
    pub fn at(&self, pc: usize) -> Instr {
        self.code[pc].get()
    }

    /// Replaces the instruction at `pc`, which is how a site specializes and how it is
    /// put back when it turns out not to be monomorphic after all.
    ///
    /// A `pc` past the end is ignored: a jump can only land inside the stream, so this
    /// cannot happen, and a caller that has one should not panic on it.
    #[inline]
    pub fn specialize(&self, pc: usize, instruction: Instr) {
        if let Some(slot) = self.code.get(pc) {
            slot.set(instruction);
        }
    }

    /// How many warmup passes `pc` has counted, which is
    /// [`SPECIALIZE_AFTER`] once it has settled.
    #[inline]
    pub fn warmup(&self, pc: usize) -> u16 {
        self.warmup.get(pc).map(Cell::get).unwrap_or(SPECIALIZE_AFTER)
    }

    /// Records how many warmup passes `pc` has counted.
    #[inline]
    pub fn set_warmup(&self, pc: usize, hits: u16) {
        if let Some(slot) = self.warmup.get(pc) {
            slot.set(hits);
        }
    }

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

    /// The `(line, column)` of the member name a [`Instr::CallMethod`] resolves.
    ///
    /// A plain copy, so the engine can carry it without building a base until a
    /// diagnostic actually needs one.
    pub fn member(&self, index: u32) -> (u32, u32) {
        self.members.get(index as usize).copied().unwrap_or((0, 0))
    }

    /// Turns a [`position`](Chunk::position) into a positioned diagnostic base.
    pub fn base_at(&self, position: (u32, u32)) -> tiger_ast::Base {
        tiger_ast::Base { line: position.0, column: position.1, source: self.source.clone() }
    }
}
