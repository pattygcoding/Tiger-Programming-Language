//! Lowers a parsed program to bytecode for the optional linear engine.
//!
//! The compiler lowers the whole program or reports [`Unsupported`], in which
//! case the interpreter runs it instead. That fallback keeps the engine always
//! correct: a program it cannot lower is simply not compiled.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use tiger_ast::{Base, Block, ClassDecl, ComprehensionBody, ComprehensionClause, Expr, FunctionDecl, LiteralValue, Stmt};
use tiger_object::bytecode::{
    CallLayout, CatchProto, Chunk, ClassProto, CompProto, FunctionProto, GenProto, Instr, Op, SliceProto, TryProto,
};
use tiger_object::Value;

use crate::declares_in;

/// Folds a binary operation whose two operands are both literals, so a constant
/// expression is computed once, when the program is compiled, instead of on every
/// evaluation.
///
/// `None` means the operation is not folded — an operand that is not a literal, or
/// a result the runtime would reject — and the engine evaluates it as usual. That
/// is what keeps a folded program's diagnostics identical to an unfiltered one's:
/// `1 / 0` is left alone so the runtime still raises `division by zero`, and an
/// overflowing product is left alone so the runtime still raises
/// `non-finite numeric result`.
fn fold_binary(operator: &str, left: &Expr, right: &Expr) -> Option<Value> {
    let (Expr::Literal { value: left, .. }, Expr::Literal { value: right, .. }) = (left, right) else {
        return None;
    };
    match (left, right) {
        (LiteralValue::Number(a), LiteralValue::Number(b)) => {
            let (a, b) = (*a, *b);
            // A comparison on two literal numbers is exact whatever they are, so it
            // folds without a finiteness guard; an arithmetic result has to stay
            // finite or the runtime would have raised.
            let computed = match operator {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" if b != 0.0 => a / b,
                "div" if b != 0.0 => (a / b).floor(),
                "%" if b != 0.0 => {
                    let mut remainder = a % b;
                    if remainder != 0.0 && (remainder < 0.0) != (b < 0.0) {
                        remainder += b;
                    }
                    remainder
                }
                "**" => a.powf(b),
                "<" => return Some(Value::Bool(a < b)),
                "<=" => return Some(Value::Bool(a <= b)),
                ">" => return Some(Value::Bool(a > b)),
                ">=" => return Some(Value::Bool(a >= b)),
                "==" => return Some(Value::Bool(a == b)),
                "!=" => return Some(Value::Bool(a != b)),
                _ => return None,
            };
            if a.is_finite() && b.is_finite() && computed.is_finite() {
                Some(Value::Number(computed))
            } else {
                None
            }
        }
        (LiteralValue::String(a), LiteralValue::String(b)) => match operator {
            "+" => Some(Value::string(&format!("{}{}", a, b))),
            "==" => Some(Value::Bool(a == b)),
            "!=" => Some(Value::Bool(a != b)),
            _ => None,
        },
        _ => None,
    }
}

/// Folds a unary operator on a literal operand, with the same finiteness rule as
/// [`fold_binary`].
fn fold_unary(operator: &str, right: &Expr) -> Option<Value> {
    let Expr::Literal { value: LiteralValue::Number(number), .. } = right else {
        return None;
    };
    let computed = match operator {
        "-" => -*number,
        "+" => *number,
        "~" => {
            if !number.is_finite() || number.trunc() != *number {
                return None;
            }
            let truncated = *number as i64;
            !truncated as f64
        }
        _ => return None,
    };
    if computed.is_finite() {
        Some(Value::Number(computed))
    } else {
        None
    }
}

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
    let mut compiler = Compiler::new(source, prefix, declaration, escape_return, captured_by_nesting(statements));
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
    /// The names this scope keeps in the shared environment, in binding order, so
    /// each one's position is its slot.
    names: Vec<Rc<str>>,
    /// The names this scope keeps in the frame's own flat locals, with the slot
    /// each was given and whether its declaration was constant. A name lands here
    /// only when nothing outside the frame can reach it.
    flat: Vec<(Rc<str>, u32, bool)>,
}

impl Default for ScopeFrame {
    fn default() -> ScopeFrame {
        ScopeFrame { stable: true, names: Vec::new(), flat: Vec::new() }
    }
}

/// Where a name resolved to.
#[derive(Clone, Copy)]
enum Local {
    /// A slot in the shared environment, `hops` scopes up the chain.
    Env { hops: u32, slot: u32 },
    /// A slot in the current frame's own flat locals. Carrying `constant` here
    /// lets a store refuse a write to a flat `const` without reading a flag.
    Flat { slot: u32, constant: bool },
}

struct Compiler {
    code: Vec<Instr>,
    constants: Vec<Value>,
    names: Vec<Rc<str>>,
    name_index: HashMap<String, u32>,
    functions: Vec<Rc<FunctionProto>>,
    classes: Vec<Rc<ClassProto>>,
    positions: Vec<(u32, u32)>,
    /// One `(line, column)` per member name a `CallMethod` resolves, in the order
    /// the instructions were emitted; each instruction carries its own index.
    members: Vec<(u32, u32)>,
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
    /// The comprehensions that keep the interpreter's clause walker, because their
    /// clause variable has to be bound by name.
    comprehensions: Vec<Rc<CompProto>>,
    generators: Vec<Rc<GenProto>>,
    escape_return: bool,
    /// The names a nested function, class, `try`, or slice bound mentions, which
    /// therefore have to stay in the shared environment.
    captured: HashSet<String>,
    /// The next flat slot this frame will hand out.
    flat_slots: u32,
    /// Where the engine will bind a call's arguments, filled in as the frame scope
    /// is seeded with them.
    call_layout: CallLayout,
    /// Whether this chunk is a method body.
    ///
    /// A method's call sets the class context on its own scope, which member
    /// visibility is checked against, so a method always needs a frame to put it
    /// in — even when everything it binds is flat.
    is_method: bool,
    /// Whether the body compiled here named `super`.
    super_used: bool,
}

impl Compiler {
    fn new(
        source: Rc<str>,
        prefix: &[&str],
        declaration: Option<&FunctionDecl>,
        escape_return: bool,
        captured: HashSet<String>,
    ) -> Compiler {
        // The engine binds a method's `this` and `super`, a catch clause's caught
        // value, then the parameters in declaration order, the variadic list, and
        // the `**kwargs` dictionary, all before the body runs. Seeding the frame
        // scope with exactly those names gives every declaration in the body the
        // slot the engine will give it, which is what a slot-based access relies on.
        let is_method = prefix.first() == Some(&"this");
        let mut frame = ScopeFrame::default();
        let mut flat_slots = 0u32;
        let mut call_layout = CallLayout::default();
        for (index, name) in prefix.iter().enumerate() {
            // A prefix outside a method is a catch clause's caught name, which stays
            // in the environment however little the body does with it: the clause's
            // condition is an un-compiled expression that the tree-walking engine
            // resolves by name, so the name has to be there for both engines.
            let slot = if declaration.is_none() {
                frame.names.push(Rc::from(*name));
                None
            } else {
                seed_name(&mut frame, &mut flat_slots, name, true, &captured)
            };
            if index == 0 {
                call_layout.this = slot;
            } else {
                call_layout.super_ = slot;
            }
        }
        if let Some(declaration) = declaration {
            for parameter in &declaration.parameters {
                let slot = seed_name(&mut frame, &mut flat_slots, parameter, false, &captured);
                call_layout.parameters.push(slot);
            }
            if !declaration.variadic.is_empty() {
                call_layout.variadic = seed_name(&mut frame, &mut flat_slots, &declaration.variadic, false, &captured);
            }
            if !declaration.keyword.is_empty() {
                call_layout.keyword = seed_name(&mut frame, &mut flat_slots, &declaration.keyword, false, &captured);
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
            members: Vec::new(),
            source,
            scope_depth: 0,
            scopes: vec![frame],
            iter_depth: 0,
            loops: Vec::new(),
            tries: Vec::new(),
            slices: Vec::new(),
            comprehensions: Vec::new(),
            generators: Vec::new(),
            escape_return,
            captured,
            flat_slots,
            call_layout,
            is_method,
            super_used: false,
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

    /// Whether evaluating `expression` can do nothing at all: neither fail nor run
    /// anything a caller could observe.
    ///
    /// A literal and a read of a frame-local are the two shapes that qualify. Both
    /// always succeed — a flat slot belongs to the frame and holds a value — and
    /// neither can run user code, which is what [`Instr::CallMethod`] needs: it
    /// resolves the member after the arguments are evaluated, where the property
    /// read it replaces resolves it before, so an argument that could raise would
    /// raise in the wrong order.
    fn side_effect_free(&self, expression: &Expr) -> bool {
        match expression {
            Expr::Literal { .. } => true,
            Expr::Identifier { name, .. } => matches!(self.resolve(name), Some(Local::Flat { .. })),
            _ => false,
        }
    }

    /// Resolves a name within this chunk, or `None` when it is not a local of this
    /// chunk and must be looked up the chain instead.
    fn resolve(&self, name: &str) -> Option<Local> {
        let depth = self.scopes.len();
        for (index, scope) in self.scopes.iter().enumerate().rev() {
            if !scope.stable {
                continue;
            }
            if let Some((_, slot, constant)) = scope.flat.iter().find(|(declared, _, _)| &**declared == name) {
                return Some(Local::Flat { slot: *slot, constant: *constant });
            }
            if let Some(slot) = scope.names.iter().position(|declared| &**declared == name) {
                return Some(Local::Env { hops: (depth - 1 - index) as u32, slot: slot as u32 });
            }
        }
        None
    }

    /// Emits the load for a name: its flat slot, a slot in a scope up the chain, or
    /// a lookup by name when the compiler cannot place it.
    fn emit_load(&mut self, name: &str, base: &Base) {
        let index = self.name(name);
        match self.resolve(name) {
            Some(Local::Flat { slot, .. }) => self.emit(Instr::LoadFlat(slot), base),
            Some(Local::Env { hops, slot }) => self.emit(Instr::LoadLocal { name: index, hops, slot }, base),
            None => self.emit(Instr::LoadName(index), base),
        }
    }

    /// Emits the store for a name, with the same resolution [`Compiler::emit_load`]
    /// uses.
    fn emit_store(&mut self, name: &str, base: &Base) {
        self.emit_store_reusable(name, base, false)
    }

    /// The same, with `reuse` set when the value being stored was computed from this
    /// slot's own value and the operand could not have written it. That is what lets a
    /// string grow in place rather than being copied whole; see `BinaryStore`.
    fn emit_store_reusable(&mut self, name: &str, base: &Base, reuse: bool) {
        let index = self.name(name);
        match self.resolve(name) {
            Some(Local::Flat { slot, constant }) => self.emit(Instr::StoreFlat { name: index, slot, constant, reuse }, base),
            Some(Local::Env { hops, slot }) => self.emit(Instr::StoreLocal { name: index, hops, slot }, base),
            None => self.emit(Instr::StoreName(index), base),
        }
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
    /// binds it.
    ///
    /// A name that nothing outside the frame can reach goes into the frame's own
    /// flat slots, where reading and writing it needs no borrow and no scope to
    /// find first. Anything a nested function, class, `try`, or slice bound mentions
    /// stays in the shared environment instead, since that is where it will be
    /// looked for by name. A `switch` scope is addressed by name throughout, because
    /// which of its cases ran decides what it holds.
    fn define(&mut self, name: &str, constant: bool, base: &Base) {
        let index = self.name(name);
        if !self.scopes.last().expect("a scope is always open").stable {
            self.emit(Instr::DefineName { name: index, constant }, base);
            return;
        }
        let duplicate = {
            let scope = self.scopes.last().expect("a scope is always open");
            scope.names.iter().any(|declared| &**declared == name) || scope.flat.iter().any(|(declared, _, _)| &**declared == name)
        };
        if self.captured.contains(name) {
            let slot = {
                let scope = self.scopes.last_mut().expect("a scope is always open");
                let slot = scope.names.len() as u32;
                scope.names.push(Rc::from(name));
                slot
            };
            self.emit(Instr::DefineLocal { name: index, slot, constant }, base);
            return;
        }
        let slot = self.flat_slots;
        self.flat_slots += 1;
        self.scopes.last_mut().expect("a scope is always open").flat.push((Rc::from(name), slot, constant));
        self.emit(Instr::DefineFlat { name: index, slot, duplicate }, base);
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

    /// Records the position of a member name a `CallMethod` resolves, and returns
    /// the index that instruction carries.
    ///
    /// A method call has two positions and the instruction's own slot holds only
    /// one of them, so the member name's is kept here.
    fn member(&mut self, base: &Base) -> u32 {
        self.members.push((base.line, base.column));
        (self.members.len() - 1) as u32
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

    /// Fuses a pair of flat loads — or a flat load and a constant, or just a
    /// constant — with the operator that consumes them, so the engine runs one
    /// instruction where it would otherwise run two or three.
    ///
    /// A group keeps its slots: the fused instruction replaces the first and the
    /// engine steps over the rest, which stay where they were. Nothing moves, so
    /// every jump target still points at what it did, and the only check needed is
    /// that nothing jumps into the middle of a group.
    fn fuse(&mut self) {
        if self.code.len() < 2 {
            return;
        }
        let mut targets = vec![false; self.code.len()];
        for instruction in &self.code {
            if let Some(target) = jump_target(instruction) {
                if let Some(slot) = targets.get_mut(target as usize) {
                    *slot = true;
                }
            }
        }
        let mut index = 0;
        while index < self.code.len() {
            let room = self.code.len() - index;
            // Each fused instruction takes the position of the operation it
            // performs, not of the load it replaced, so a diagnostic raised by that
            // operation points at the operator exactly where the interpreter's does.
            // A group's own slots stay where they were, so nothing moves.
            if room >= 4 && !targets[index + 1] && !targets[index + 2] && !targets[index + 3] {
                let fused = match (&self.code[index], &self.code[index + 1], &self.code[index + 2], &self.code[index + 3]) {
                    (Instr::LoadFlat(a), Instr::LoadFlat(b), Instr::Binary { op, name }, Instr::JumpIfFalse(target)) => {
                        Some((Instr::FlatJumpUnlessFlat { a: *a, b: *b, op: *op, name: *name, target: *target }, index + 2))
                    }
                    (Instr::LoadFlat(a), Instr::Constant(b), Instr::Binary { op, name }, Instr::JumpIfFalse(target)) => {
                        Some((Instr::FlatJumpUnlessConstant { a: *a, b: *b, op: *op, name: *name, target: *target }, index + 2))
                    }
                    // `items[i] = value` with all three in the frame's own locals.
                    (Instr::LoadFlat(value), Instr::LoadFlat(collection), Instr::LoadFlat(key), Instr::SetIndex) => {
                        Some((Instr::FlatStoreIndex { value: *value, collection: *collection, key: *key }, index + 3))
                    }
                    // An assignment whose value is a computation over flat locals:
                    // `total = total + value`, and `total = total + 1`. A store into
                    // a `const` is left alone, so the unfused form still raises the
                    // error for it.
                    (
                        Instr::LoadFlat(a),
                        Instr::LoadFlat(b),
                        Instr::Binary { op, name },
                        Instr::StoreFlat { slot, constant: false, .. },
                    ) => Some((Instr::FlatBinaryFlatStore { a: *a, b: *b, op: *op, name: *name, out: *slot }, index + 2)),
                    (
                        Instr::LoadFlat(a),
                        Instr::Constant(b),
                        Instr::Binary { op, name },
                        Instr::StoreFlat { slot, constant: false, .. },
                    ) => Some((Instr::FlatBinaryConstantStore { a: *a, b: *b, op: *op, name: *name, out: *slot }, index + 2)),
                    _ => None,
                };
                if let Some((fused, position)) = fused {
                    self.code[index] = fused;
                    self.positions[index] = self.positions[position];
                    index += 4;
                    continue;
                }
            }
            if room >= 3 && !targets[index + 1] && !targets[index + 2] {
                let fused = match (&self.code[index], &self.code[index + 1], &self.code[index + 2]) {
                    (Instr::LoadFlat(a), Instr::LoadFlat(b), Instr::Binary { op, name }) => {
                        Some((Instr::FlatBinaryFlat { a: *a, b: *b, op: *op, name: *name }, index + 2))
                    }
                    (Instr::LoadFlat(a), Instr::Constant(b), Instr::Binary { op, name }) => {
                        Some((Instr::FlatBinaryConstant { a: *a, b: *b, op: *op, name: *name }, index + 2))
                    }
                    // `items[index]` and `items[0]`, with the collection in the
                    // frame's own locals.
                    (Instr::LoadFlat(a), Instr::LoadFlat(b), Instr::GetIndex) => Some((Instr::FlatIndexFlat { a: *a, b: *b }, index + 2)),
                    (Instr::LoadFlat(a), Instr::Constant(b), Instr::GetIndex) => {
                        Some((Instr::FlatIndexConstant { a: *a, b: *b }, index + 2))
                    }
                    // A counting loop's step: the update, the discard of its value,
                    // and the back edge, in one instruction. A `const` target is left
                    // alone so the unfused form still raises the error for it.
                    (Instr::UpdateFlat { name, slot, constant: false, decrement, .. }, Instr::Pop, Instr::Jump(target)) => {
                        Some((Instr::UpdateFlatJump { name: *name, slot: *slot, decrement: *decrement, target: *target }, index))
                    }
                    _ => None,
                };
                if let Some((fused, position)) = fused {
                    self.code[index] = fused;
                    self.positions[index] = self.positions[position];
                    index += 3;
                    continue;
                }
            }
            if room >= 2 && !targets[index + 1] {
                let fused = match (&self.code[index], &self.code[index + 1]) {
                    (Instr::Constant(b), Instr::Binary { op, name }) => {
                        Some((Instr::BinaryConstant { b: *b, op: *op, name: *name }, index + 1, 2))
                    }
                    (Instr::LoadFlat(a), Instr::GetProperty(name)) => {
                        Some((Instr::FlatProperty { value: *a, name: *name, cache: (index + 1) as u32 }, index + 1, 2))
                    }
                    // `CallName` is the name load itself, so it keeps the name's
                    // position: an undefined name is reported where the name is, not
                    // where the call that consumes it begins. It renames the load and
                    // leaves the `Call` where it stands, so the pass can still see what
                    // follows the call.
                    (Instr::LoadName(name), Instr::Call { .. }) => Some((Instr::CallName { name: *name }, index, 1)),
                    // A call whose value is dropped — `items.append(x);`, `print(y);`.
                    // The result is never pushed, and the `Pop` that would have discarded
                    // it goes away with it. `Pop` reports nothing, so the fused
                    // instruction can keep the call's position.
                    (Instr::Call { positional, keywords, discard: false }, Instr::Pop) => {
                        Some((Instr::Call { positional: *positional, keywords: *keywords, discard: true }, index, 2))
                    }
                    // The same for a method call written as a statement:
                    // `items.insert(0, x);`. The member name's own position is carried
                    // through, because resolving it still reports from there.
                    (Instr::CallMethod { name, positional, keywords, member, discard: false }, Instr::Pop) => Some((
                        Instr::CallMethod { name: *name, positional: *positional, keywords: *keywords, member: *member, discard: true },
                        index,
                        2,
                    )),
                    // A comparison whose only consumer is the branch: the operand
                    // stack is left alone and the truthiness test is skipped.
                    (Instr::Binary { op, name }, Instr::JumpIfFalse(target)) => {
                        Some((Instr::JumpUnless { op: *op, name: *name, target: *target }, index, 2))
                    }
                    // The tail of an assignment whose value comes off the operand
                    // stack: the operator and the store in one dispatch, with the
                    // result going straight into its slot. The both-flat forms are
                    // longer, so they have already been tried and lost by here.
                    (Instr::Binary { op, name }, Instr::StoreFlat { slot, constant: false, reuse, .. }) => {
                        Some((Instr::BinaryStore { op: *op, name: *name, out: *slot, reuse: *reuse }, index, 2))
                    }
                    _ => None,
                };
                if let Some((fused, position, advance)) = fused {
                    self.code[index] = fused;
                    self.positions[index] = self.positions[position];
                    index += advance;
                    continue;
                }
            }
            index += 1;
        }
    }

    fn finish(mut self) -> Rc<Chunk> {
        self.fuse();
        let slots = self.code.len();
        // A method that never names `super` — neither itself nor through a nested
        // function, which the capture set already accounts for — does not get one
        // bound, because building the binding allocates.
        let call_layout = CallLayout {
            flat_parameters: self.call_layout.parameters.iter().all(|slot| slot.is_some()),
            super_used: self.super_used || self.captured.contains("super"),
            ..self.call_layout
        };
        let frame_needed = !self.scopes[0].names.is_empty() || self.is_method;
        // Every fact the flat call path tests is a fact about this declaration, not about the
        // call: no frame of its own, flat parameters, no `super`, neither variadic nor keyword
        // parameters, and locals few enough to fit a frame the engine enters directly. The call
        // site tests this one flag instead; only its own argument count is left to it.
        let flat_callable = !frame_needed
            && call_layout.flat_parameters
            && !call_layout.super_used
            && call_layout.variadic.is_none()
            && call_layout.keyword.is_none()
            && self.flat_slots as usize <= crate::FLAT_LOCALS;
        Rc::new(Chunk {
            property_cache: std::cell::RefCell::new(vec![None; slots]),
            name_cache: std::cell::RefCell::new(vec![None; slots]),
            code: self.code,
            constants: self.constants,
            names: self.names,
            functions: self.functions,
            classes: self.classes,
            tries: self.tries,
            slices: self.slices,
            comprehensions: self.comprehensions,
            generators: self.generators,
            flat_count: self.flat_slots as usize,
            call_layout,
            frame_needed,
            flat_callable,
            positions: self.positions,
            members: self.members,
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
            Expr::Identifier { base, name } => self.emit_load(name, base),
            Expr::Super { base } => {
                self.super_used = true;
                // A method whose frame binds `super` flat reads it from the frame's
                // own locals; every other `super` — a captured one, or one outside a
                // method — is looked up by name at run time.
                let slot = match self.resolve("super") {
                    Some(Local::Flat { slot, .. }) => Some(slot),
                    _ => None,
                };
                self.emit(Instr::LoadSuper(slot), base);
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
                    // A constant expression is computed here, once, instead of on
                    // every evaluation.
                    if let Some(folded) = fold_binary(operator, left, right) {
                        let index = self.constant(folded);
                        self.emit(Instr::Constant(index), base);
                        return Ok(());
                    }
                    self.expression(left)?;
                    self.expression(right)?;
                    let index = self.name(operator);
                    let op = operator_code(operator);
                    self.emit(Instr::Binary { op, name: index }, base);
                }
            }
            Expr::Unary { base, operator, right } => {
                if let Some(folded) = fold_unary(operator, right) {
                    let index = self.constant(folded);
                    self.emit(Instr::Constant(index), base);
                    return Ok(());
                }
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
            Expr::Tuple { base, elements } => {
                for element in elements {
                    self.expression(element)?;
                }
                self.emit(Instr::BuildTuple(elements.len() as u32), base);
            }
            Expr::Set { base, elements } => {
                for element in elements {
                    self.expression(element)?;
                }
                self.emit(Instr::BuildSet(elements.len() as u32), base);
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
                // `receiver.name(...)` is one instruction when the arguments cannot
                // be observed out of order. A property read resolves the member
                // before the arguments are evaluated and `CallMethod` resolves it
                // after, so an argument that could raise or run anything at all would
                // move that work past the member's — a literal and a read of a
                // frame-local are the two shapes that can do neither.
                if let Expr::Property { base: member, receiver, name } = &**function {
                    if keywords.is_empty() && arguments.iter().all(|argument| !argument.spread && self.side_effect_free(&argument.value)) {
                        self.expression(receiver)?;
                        for argument in arguments {
                            self.expression(&argument.value)?;
                        }
                        let index = self.name(name);
                        let member = self.member(member);
                        self.emit(
                            Instr::CallMethod { name: index, positional: arguments.len() as u32, keywords: 0, member, discard: false },
                            base,
                        );
                        return Ok(());
                    }
                }
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
                self.emit(Instr::Call { positional: arguments.len() as u32, keywords: keywords.len() as u32, discard: false }, base);
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
            Expr::Comprehension { base, body, clauses } => {
                // A clause variable mentioned inside a function, a class, or a generator
                // has to stay in the shared environment: those run later, out of the frame
                // that is running this loop, and reach a name through the environment — so
                // the interpreter's own clause walker binds it, which is the same thing the
                // tree-walking engine does and what makes the two agree. Everything else is
                // lowered to a loop of this chunk, whose clause variables live in the frame's
                // own flat slots.
                if clause_reaches_out(body, clauses) {
                    let index = self.comprehensions.len() as u32;
                    self.comprehensions.push(Rc::new(CompProto { body: body.clone(), clauses: clauses.clone() }));
                    self.emit(Instr::Comprehension(index), base);
                } else {
                    self.comprehension(body, clauses, base)?;
                }
            }
            Expr::Generator { base, body, clauses } => {
                // Like a comprehension, the clauses and element stay as AST for the
                // engine's shared generator helper; the engine only builds the
                // iterator here, and nothing is evaluated until it is walked.
                let index = self.generators.len() as u32;
                self.generators.push(Rc::new(GenProto { body: (**body).clone(), clauses: clauses.clone() }));
                self.emit(Instr::Generator(index), base);
            }
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
                let decrement = operator == "--";
                // The operator's own spelling is what the "requires a number"
                // diagnostic quotes, so an index or property update carries it too.
                let op_index = self.name(operator);
                match &**target {
                    Expr::Identifier { name, .. } => {
                        let index = self.name(name);
                        // A counter that reaches no nested function lives in the
                        // frame's own locals, so its update is a plain slot write.
                        match self.resolve(name) {
                            Some(Local::Flat { slot, constant }) => {
                                self.emit(Instr::UpdateFlat { name: index, slot, constant, decrement, prefix: *prefix }, base)
                            }
                            Some(Local::Env { hops, slot }) => {
                                self.emit(Instr::UpdateLocal { name: index, hops, slot, decrement, prefix: *prefix }, base)
                            }
                            None => self.emit(Instr::UpdateName { name: index, decrement, prefix: *prefix }, base),
                        }
                    }
                    // An index or property target is resolved once and updated in
                    // place, so `items[i]++` reads and writes one element and the
                    // target's own expressions run once, as the interpreter does.
                    Expr::Index { base: target_base, collection, key } => {
                        self.expression(collection)?;
                        self.expression(key)?;
                        self.emit(Instr::ReadIndexTarget, target_base);
                        self.emit(Instr::UpdateIndex { name: op_index, decrement, prefix: *prefix }, base);
                    }
                    Expr::Property { base: target_base, receiver, name } => {
                        self.expression(receiver)?;
                        let property = self.name(name);
                        self.emit(Instr::ReadPropertyTarget(property), target_base);
                        self.emit(Instr::UpdateProperty { property, name: op_index, decrement, prefix: *prefix }, base);
                    }
                    _ => return Err(Unsupported),
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

    /// Lowers a comprehension to the loop the `for ... in` statement already uses.
    ///
    /// The clause variables are flat slots of this frame, so the scope the comprehension
    /// opens holds nothing: it is there to place them, which is what keeps a clause variable
    /// from leaking into the enclosing scope and what lets a nested comprehension shadow a
    /// name an outer one binds. The instructions are the statement's own, so a source is
    /// treated exactly as a `for` loop treats it — a lazy iterator is stepped one element at
    /// a time and anything else is snapshotted when the loop opens — and nothing here
    /// re-enters the interpreter, which is what a comprehension used to cost per element.
    ///
    /// A comprehension whose clause variable a deferred construct reaches is not lowered at
    /// all: it keeps the interpreter's clause walker, which binds the variable by name in the
    /// scope the construct captured. See [`clause_reaches_out`].
    fn comprehension(&mut self, body: &ComprehensionBody, clauses: &[ComprehensionClause], base: &Base) -> Compiled {
        // The accumulator is a flat local this frame hands out no name for, so the body
        // can append to it while using the operand stack normally.
        let accumulator = self.flat_slots;
        self.flat_slots += 1;
        // A comprehension opens a scope of its own: that is what keeps its clause
        // variables from leaking into the enclosing one, and it is why a nested
        // comprehension can shadow a name the outer one binds.
        self.emit(Instr::PushScope, base);
        self.enter_scope();
        match body {
            ComprehensionBody::List(_) => self.emit(Instr::BuildList(0), base),
            ComprehensionBody::Dict(_) => self.emit(Instr::BuildDict(0), base),
        }
        self.emit(Instr::StoreScratch(accumulator), base);
        self.comprehension_clauses(body, clauses, 0, accumulator, base)?;
        self.emit(Instr::PopScope, base);
        self.leave_scope();
        // The collection the loop left in the scratch slot is the comprehension's value.
        self.emit(Instr::LoadFlat(accumulator), base);
        Ok(())
    }

    /// One clause of a comprehension: its iterable, its filters, and then either the
    /// next clause or the value that is collected.
    fn comprehension_clauses(
        &mut self,
        body: &ComprehensionBody,
        clauses: &[ComprehensionClause],
        index: usize,
        accumulator: u32,
        base: &Base,
    ) -> Compiled {
        let clause = &clauses[index];
        self.expression(&clause.iterable)?;
        self.emit(Instr::IterInit, clause.iterable.base());
        // The clause variable gets a flat slot of this frame and nothing binds it: the
        // loop's own store writes that slot before anything can read it, and a name the
        // compiler has not recorded yet does not resolve at all — which is what keeps a
        // clause variable read before its own loop has run an `undefined name` error
        // rather than a silent `null`. A clause that reuses a name an earlier clause
        // bound writes that same binding instead of taking a slot of its own.
        self.declare_clause(&clause.name);
        let start = self.here();
        self.emit(Instr::IterNext(0), clause.iterable.base());
        self.emit_store(&clause.name, clause.iterable.base());
        // A filter that does not hold skips to the next element of this clause, which is
        // its own `IterNext`. They are tested in order, so the first that fails ends the
        // pass.
        for filter in &clause.filters {
            self.expression(filter)?;
            let skip = self.here();
            self.emit(Instr::JumpIfFalse(0), filter.base());
            self.patch(skip, start);
        }
        if index + 1 < clauses.len() {
            // A later clause runs once per element of this one, so its iterable is
            // evaluated here rather than once for the whole comprehension.
            self.comprehension_clauses(body, clauses, index + 1, accumulator, base)?;
        } else {
            match body {
                ComprehensionBody::List(element) => {
                    self.expression(element)?;
                    self.emit(Instr::Collect(accumulator), element.base());
                }
                ComprehensionBody::Dict(pair) => {
                    self.expression(&pair.key)?;
                    self.expression(&pair.value)?;
                    // The instruction carries the key's position, so a key the runtime
                    // cannot hash is reported where the key is written.
                    self.emit(Instr::CollectPair(accumulator), pair.key.base());
                }
            }
        }
        self.emit(Instr::Jump(start as u32), base);
        let end = self.here();
        self.patch(start, end);
        Ok(())
    }

    /// Records a comprehension's clause variable in the scope the comprehension opened,
    /// and gives it a flat slot of this frame.
    ///
    /// No instruction binds it. The loop's own store writes the slot before the element
    /// or a later clause can read it, so a declaration would only be a store of `null`
    /// that the first element overwrites; and leaving the name unrecorded until its
    /// clause is compiled is what makes a read of it *before* its loop an `undefined
    /// name` rather than a silent `null`.
    fn declare_clause(&mut self, name: &str) {
        if self.declared_here(name) {
            return;
        }
        let slot = self.flat_slots;
        self.flat_slots += 1;
        self.scopes.last_mut().expect("a scope is always open").flat.push((Rc::from(name), slot, false));
    }

    /// Whether the scope the compiler is in already declares `name`.
    ///
    /// A comprehension's clause variables are the case this exists for: a second clause
    /// under the same name is a write to the binding the first one made, so it must not
    /// take a slot of its own.
    fn declared_here(&self, name: &str) -> bool {
        let Some(scope) = self.scopes.last() else { return false };
        scope.flat.iter().any(|(declared, _, _)| &**declared == name) || scope.names.iter().any(|declared| &**declared == name)
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
            Stmt::Import { base, path, name, names } => {
                // The `Import` instruction binds the name itself; record it here so
                // the declarations that follow in this scope get the right slots.
                if names.is_empty() {
                    self.record(name);
                    let path = self.name(path);
                    let name = self.name(name);
                    self.emit(Instr::Import { path, name }, base);
                } else {
                    // A named import lands the module in a name no source can spell,
                    // reads each member from it, and declares the name it asked for.
                    let hidden_name = format!("\u{0}import {} at {}", path, self.here());
                    self.record(&hidden_name);
                    let hidden = self.name(&hidden_name);
                    let path = self.name(path);
                    self.emit(Instr::Import { path, name: hidden }, base);
                    for imported in names {
                        // Each member read needs the module on the stack, so it is
                        // loaded again for every name rather than held across them.
                        self.emit_load(&hidden_name, base);
                        let member = self.name(&imported.name);
                        self.emit(Instr::GetProperty(member), &imported.base);
                        self.define(&imported.alias, true, &imported.base);
                    }
                }
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
                    self.emit(Instr::JumpIfFalse(0), branch.condition.base());
                    self.branch_body(&branch.body, branch.condition.base())?;
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
                self.scopes.push(ScopeFrame { stable: false, names: Vec::new(), flat: Vec::new() });
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
            let trimmed = operator.trim_end_matches('=');
            let index_of_op = self.name(trimmed);
            let op = operator_code(trimmed);
            match target {
                Expr::Identifier { name, .. } => {
                    self.emit_load(name, base);
                    let operand_start = self.code.len();
                    self.expression(value)?;
                    // `x += items[i]`: when the operand wrote none of the frame's own
                    // slots, the value the load pushed is still the one in the target,
                    // so the store can take it back by value and let a string grow in
                    // place instead of being copied whole. The scan is what makes that
                    // sound — see `BinaryStore`'s `reuse` in the VM — and it is
                    // conservative: any slot write in the operand disables it.
                    let wrote_flat = self.code[operand_start..].iter().any(|instruction| {
                        matches!(
                            instruction,
                            Instr::StoreFlat { .. }
                                | Instr::DefineFlat { .. }
                                | Instr::UpdateFlat { .. }
                                | Instr::UpdateFlatJump { .. }
                                | Instr::FlatBinaryFlatStore { .. }
                                | Instr::FlatBinaryConstantStore { .. }
                                | Instr::BinaryStore { .. }
                        )
                    });
                    self.emit(Instr::Binary { op, name: index_of_op }, base);
                    self.emit_store_reusable(name, base, !wrote_flat);
                }
                // A property or index target is resolved once and read, then the
                // right-hand side is evaluated, combined, and stored back into the
                // same place — the order the interpreter's compound assignment uses.
                Expr::Property { base: target, receiver, name } => {
                    self.expression(receiver)?;
                    let index = self.name(name);
                    self.emit(Instr::ReadPropertyTarget(index), target);
                    self.expression(value)?;
                    self.emit(Instr::Binary { op, name: index_of_op }, base);
                    self.emit(Instr::SetPropertyKeep(index), target);
                }
                Expr::Index { base: target, collection, key } => {
                    self.expression(collection)?;
                    self.expression(key)?;
                    self.emit(Instr::ReadIndexTarget, target);
                    self.expression(value)?;
                    self.emit(Instr::Binary { op, name: index_of_op }, base);
                    self.emit(Instr::SetIndexKeep, target);
                }
                _ => return Err(Unsupported),
            }
            return Ok(());
        }
        match target {
            Expr::Identifier { name, .. } => {
                self.expression(value)?;
                self.emit_store(name, base);
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

/// The absolute instruction index an instruction jumps to, when it jumps.
///
/// Every instruction the fusion pass consults for a branch target goes through
/// here, so a variant that carries one has to be listed or a group could be fused
/// across it.
fn jump_target(instruction: &Instr) -> Option<u32> {
    match instruction {
        Instr::Jump(target)
        | Instr::JumpIfFalse(target)
        | Instr::JumpIfFalseOrPop(target)
        | Instr::JumpIfTrueOrPop(target)
        | Instr::IterNext(target) => Some(*target),
        Instr::JumpUnless { target, .. } | Instr::FlatJumpUnlessFlat { target, .. } | Instr::FlatJumpUnlessConstant { target, .. } => {
            Some(*target)
        }
        Instr::UpdateFlatJump { target, .. } => Some(*target),
        _ => None,
    }
}

/// Seeds one name the engine binds before a body runs, returning its flat slot when
/// it gets one: a name no nested function can reach goes into the frame's own
/// locals, and anything else stays in the shared environment.
fn seed_name(frame: &mut ScopeFrame, flat_slots: &mut u32, name: &str, constant: bool, captured: &HashSet<String>) -> Option<u32> {
    if captured.contains(name) {
        frame.names.push(Rc::from(name));
        return None;
    }
    let slot = *flat_slots;
    *flat_slots += 1;
    frame.flat.push((Rc::from(name), slot, constant));
    Some(slot)
}

/// The names one chunk declares that something outside its frame may read.
///
/// A local can live in the frame's own flat slot only when nothing outside the
/// frame will ever ask for it by name. That is true unless one of these reaches it,
/// and every one of them evaluates somewhere the frame is not:
///
/// * a nested function body, and its default argument expressions, which run in the
///   environment the function captured;
/// * a nested class's field initializers, constructor, and methods;
/// * a `try`, `catch`, or `finally` block, and a catch condition, which compile into
///   their own chunks and so reach outwards by name;
/// * a slice bound, which the engine evaluates as an expression against the scope.
///
/// The walk is deliberately generous: it gathers every identifier it meets in those
/// places, so a name is only ever treated as flat when nothing mentions it there.
/// Whether a clause variable of a comprehension is mentioned inside a construct that runs
/// later: a function, a class, or a generator expression.
///
/// The capture scan answers this already, because what it collects *is* the set of names a
/// deferred construct mentions — so asking it with `inside` false leaves a name in the set
/// only when something nested mentioned it. A comprehension that mentions a clause variable
/// that way keeps the interpreter's clause walker, which binds the variable by name in the
/// scope the construct captured; one that does not is lowered to a loop whose clause
/// variables are flat slots of the frame running it.
fn clause_reaches_out(body: &ComprehensionBody, clauses: &[ComprehensionClause]) -> bool {
    let mut deferred: HashSet<String> = HashSet::new();
    for clause in clauses {
        scan_expr(&clause.iterable, false, &mut deferred);
        for filter in &clause.filters {
            scan_expr(filter, false, &mut deferred);
        }
    }
    match body {
        ComprehensionBody::List(element) => scan_expr(element, false, &mut deferred),
        ComprehensionBody::Dict(pair) => {
            scan_expr(&pair.key, false, &mut deferred);
            scan_expr(&pair.value, false, &mut deferred);
        }
    }
    clauses.iter().any(|clause| deferred.contains(&clause.name))
}

fn captured_by_nesting(statements: &[Stmt]) -> HashSet<String> {
    let mut found = HashSet::new();
    scan_block(statements, false, &mut found);
    found
}

fn scan_block(statements: &[Stmt], inside: bool, out: &mut HashSet<String>) {
    for statement in statements {
        scan_stmt(statement, inside, out);
    }
}

fn scan_stmt(statement: &Stmt, inside: bool, out: &mut HashSet<String>) {
    match statement {
        Stmt::Expression { value, .. } => scan_expr(value, inside, out),
        Stmt::Assign { target, value, .. } => {
            scan_expr(target, inside, out);
            scan_expr(value, inside, out);
        }
        Stmt::Throw { value, .. } => scan_expr(value, inside, out),
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                scan_expr(value, inside, out);
            }
        }
        Stmt::Destructure { value, .. } => scan_expr(value, inside, out),
        Stmt::Import { .. } | Stmt::Control { .. } => {}
        Stmt::Try { body, catches, finally, .. } => {
            scan_block(body, true, out);
            for clause in catches {
                if let Some(condition) = &clause.condition {
                    scan_expr(condition, true, out);
                }
                scan_block(&clause.body, true, out);
            }
            scan_block(finally, true, out);
        }
        Stmt::Function(declaration) => scan_function(declaration, out),
        Stmt::Class(declaration) => scan_class(declaration, out),
        Stmt::If { branches, else_block, .. } => {
            for branch in branches {
                scan_expr(&branch.condition, inside, out);
                scan_block(&branch.body, inside, out);
            }
            scan_block(else_block, inside, out);
        }
        Stmt::While { condition, body, else_block, .. } => {
            scan_expr(condition, inside, out);
            scan_block(body, inside, out);
            scan_block(else_block, inside, out);
        }
        Stmt::For { iterable, body, else_block, .. } => {
            scan_expr(iterable, inside, out);
            scan_block(body, inside, out);
            scan_block(else_block, inside, out);
        }
        Stmt::CFor { initializer, condition, update, body, else_block, .. } => {
            if let Some(initializer) = initializer {
                scan_stmt(initializer, inside, out);
            }
            if let Some(condition) = condition {
                scan_expr(condition, inside, out);
            }
            if let Some(update) = update {
                scan_stmt(update, inside, out);
            }
            scan_block(body, inside, out);
            scan_block(else_block, inside, out);
        }
        Stmt::DoWhile { condition, body, .. } => {
            scan_block(body, inside, out);
            scan_expr(condition, inside, out);
        }
        Stmt::Switch { value, cases, .. } => {
            scan_expr(value, inside, out);
            for case in cases {
                if let Some(case_value) = &case.value {
                    scan_expr(case_value, inside, out);
                }
                scan_block(&case.body, inside, out);
            }
        }
    }
}

fn scan_function(declaration: &FunctionDecl, out: &mut HashSet<String>) {
    for default in declaration.defaults.iter().flatten() {
        scan_expr(default, true, out);
    }
    scan_block(&declaration.body, true, out);
}

fn scan_class(declaration: &ClassDecl, out: &mut HashSet<String>) {
    // The parent is found by name at run time, so it has to stay in the
    // environment even though nothing in the class body mentions it.
    if let Some(parent) = &declaration.parent_name {
        out.insert(parent.clone());
    }
    for field in &declaration.fields {
        if let Some(value) = &field.value {
            scan_expr(value, true, out);
        }
    }
    if let Some(constructor) = &declaration.constructor {
        scan_function(constructor, out);
    }
    for method in &declaration.methods {
        scan_function(method, out);
    }
}

/// Scans `expression` and then forgets the names the enclosing construct binds.
///
/// A comprehension's own variables are bound in the scope it opens, so a reference
/// to one of them never reaches the enclosing scope and must not force a same-named
/// variable *there* into the environment. `bound` is the set of names bound so far,
/// and a name this scan was the only one to add is dropped again — while a name an
/// earlier part of the program genuinely captured stays captured.
fn scan_shadowed(expression: &Expr, out: &mut HashSet<String>, bound: &HashSet<String>) {
    if bound.is_empty() {
        scan_expr(expression, true, out);
        return;
    }
    let before: HashSet<String> = out.clone();
    scan_expr(expression, true, out);
    for name in bound {
        if !before.contains(name) {
            out.remove(name);
        }
    }
}

/// Scans a comprehension's or generator's clauses, returning the names they bind.
///
/// A clause's iterable runs before that clause binds its name and after the earlier
/// clauses have bound theirs, so the set grows as the clauses are scanned. Scanning
/// in that order is what keeps `[x for x in a if y > 0 for y in b]` sound: the
/// filter's `y` is read before the second clause binds it, so it is an outer
/// reference and stays captured.
fn scan_clauses(clauses: &[ComprehensionClause], out: &mut HashSet<String>) -> HashSet<String> {
    let mut bound: HashSet<String> = HashSet::new();
    for clause in clauses {
        scan_shadowed(&clause.iterable, out, &bound);
        bound.insert(clause.name.clone());
        for filter in &clause.filters {
            scan_shadowed(filter, out, &bound);
        }
    }
    bound
}

fn scan_expr(expression: &Expr, inside: bool, out: &mut HashSet<String>) {
    match expression {
        Expr::Literal { .. } => {}
        // `super` has its own syntax but is bound under its own name, and a nested
        // function reaches it through the environment, so it is captured like any
        // other name the frame has to keep.
        Expr::Super { .. } => {
            if inside {
                out.insert("super".to_string());
            }
        }
        Expr::Identifier { name, .. } => {
            if inside {
                out.insert(name.clone());
            }
        }
        Expr::FormattedString { parts, .. } => {
            for part in parts {
                scan_expr(part, inside, out);
            }
        }
        Expr::Interpolation { value, .. } => scan_expr(value, inside, out),
        Expr::Conditional { condition, then, otherwise, .. } => {
            scan_expr(condition, inside, out);
            scan_expr(then, inside, out);
            scan_expr(otherwise, inside, out);
        }
        Expr::Slice { collection, start, stop, step, .. } => {
            scan_expr(collection, inside, out);
            for bound in [start, stop, step].into_iter().flatten() {
                scan_expr(bound, true, out);
            }
        }
        Expr::Unary { right, .. } => scan_expr(right, inside, out),
        // An update target may stay flat: the engine has an update for a flat slot,
        // and the analysis marks the name captured only when a nested function
        // actually reaches it — which is when the environment has to carry it.
        Expr::Update { target, .. } => scan_expr(target, inside, out),
        Expr::Binary { left, right, .. } => {
            scan_expr(left, inside, out);
            scan_expr(right, inside, out);
        }
        Expr::Call { function, arguments, keywords, .. } => {
            scan_expr(function, inside, out);
            for argument in arguments {
                scan_expr(&argument.value, inside, out);
            }
            for entry in keywords {
                scan_expr(&entry.value, inside, out);
            }
        }
        Expr::Index { collection, key, .. } => {
            scan_expr(collection, inside, out);
            scan_expr(key, inside, out);
        }
        Expr::Property { receiver, .. } => scan_expr(receiver, inside, out),
        Expr::List { elements, .. } => {
            for element in elements {
                scan_expr(element, inside, out);
            }
        }
        Expr::Tuple { elements, .. } | Expr::Set { elements, .. } => {
            for element in elements {
                scan_expr(element, inside, out);
            }
        }
        Expr::Comprehension { body, clauses, .. } => {
            // The clauses are scanned before the element, because the element runs
            // with every clause's variable already bound.
            let bound = scan_clauses(clauses, out);
            match body {
                ComprehensionBody::List(element) => scan_shadowed(element, out, &bound),
                ComprehensionBody::Dict(pair) => {
                    scan_shadowed(&pair.key, out, &bound);
                    scan_shadowed(&pair.value, out, &bound);
                }
            }
        }
        Expr::Generator { body, clauses, .. } => {
            // A generator body and its clause expressions run later, in the scope
            // the generator captured, so any name they mention has to stay
            // reachable through the environment rather than a flat slot — except the
            // generator's own variables, which its own scope carries.
            let bound = scan_clauses(clauses, out);
            scan_shadowed(body, out, &bound);
        }
        Expr::Dict { pairs, .. } => {
            for pair in pairs {
                scan_expr(&pair.key, inside, out);
                scan_expr(&pair.value, inside, out);
            }
        }
        Expr::Function(declaration) => scan_function(declaration, out),
    }
}
