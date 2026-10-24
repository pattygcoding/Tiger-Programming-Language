//! The linear (bytecode) execution engine.
//!
//! Runs a [`Chunk`](tiger_object::bytecode::Chunk) in a flat loop instead of
//! recursively walking the AST. Every operation delegates to the same runtime
//! helpers the interpreter uses, so the two engines agree exactly.

use std::rc::Rc;

use tiger_object as object;
use tiger_object::bytecode::{CachedProperty, Chunk, Instr, Op, TryProto};
use tiger_object::{Env, FunctionValue, Keyword, Value};

use crate::{builtins, methods, runtime, Evaluator, Signal};

/// A `for ... in` iteration in progress.
///
/// A lazy iterator is stepped one element at a time, so a generator stopped
/// early produces nothing beyond what the loop reads; anything else is
/// snapshotted when the loop opens, exactly as before.
enum IterState {
    Values(Vec<Value>, usize),
    Lazy(Value),
}

type Vm = Result<Value, Signal>;

/// Whether two values are the same receiver. Clones of one `Rc` share their
/// address, so a cache hit means this instruction resolved this very object before.
///
/// Every receiver whose members cannot change identity once it exists is listed:
/// a collection's methods, a module's bindings, and a string's methods are all
/// fixed, so one shared value's member is the same member every time. A number or
/// a boolean has no member namespace of its own and is not listed, because a
/// value like `1` is not a unique object to guard on.
/// Reads `collection[key]` when it is a list indexed by a non-negative integer in
/// range, which is the shape nearly every index read has.
///
/// `None` means the read needs the full path, with its own key check, its negative
/// index from the end, and its own diagnostics. Answering the common shape here
/// costs a borrow and an index rather than a call, a key check, and a second
/// borrow, and an index read is the operation a loop spends most of its time in.
/// Appends to a string in place when a fused `Add` targets its own left operand.
///
/// `s += text` in a loop is the shape this exists for: without it the whole
/// accumulator is copied on every iteration, which makes building a string cost the
/// square of its length. `RcStr::append` decides whether the buffer can really be
/// reused — a string another binding still holds is copied instead, so the alias keeps
/// its own text.
#[inline]
fn append_in_place(op: Op, targets_left: bool, target: &mut Value, more: &Value) -> bool {
    // The target's tag is the cheapest test and the one that rules out a numeric
    // accumulator, which is what most of these instructions carry.
    if !matches!(target, Value::String(_)) {
        return false;
    }
    if !targets_left || !matches!(op, Op::Add) {
        return false;
    }
    if let (Value::String(text), Value::String(more)) = (target, more) {
        text.append(more.as_str());
        return true;
    }
    false
}

#[inline]
fn fast_index(collection: &Value, key: &Value) -> Option<Value> {
    let Value::List(elements) = collection else { return None };
    let Value::Number(index) = key else { return None };
    let index = *index;
    if index < 0.0 || index.trunc() != index {
        return None;
    }
    let borrowed = elements.borrow();
    let index = index as usize;
    if index >= borrowed.len() {
        return None;
    }
    Some(borrowed[index].clone())
}

/// Stores `collection[key] = value` when it is a list indexed by a non-negative
/// integer in range, the shape nearly every index write has.
///
/// `false` means the store needs the full path. The element is dropped in place and
/// the new one cloned into it, which for a number allocates nothing.
#[inline]
fn fast_store_index(collection: &Value, key: &Value, value: &Value) -> bool {
    let Value::List(elements) = collection else { return false };
    let Value::Number(index) = key else { return false };
    let index = *index;
    if index < 0.0 || index.trunc() != index {
        return false;
    }
    let mut borrowed = elements.borrow_mut();
    let index = index as usize;
    if index >= borrowed.len() {
        return false;
    }
    borrowed[index] = value.clone();
    true
}

fn same_receiver(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Dict(a), Value::Dict(b)) => Rc::ptr_eq(a, b),
        (Value::Module(a), Value::Module(b)) => Rc::ptr_eq(a, b),
        (Value::Instance(a), Value::Instance(b)) => Rc::ptr_eq(a, b),
        (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
        (Value::Set(a), Value::Set(b)) => Rc::ptr_eq(a, b),
        (Value::Tuple(a), Value::Tuple(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}

/// Whether a resolved property may be reused for the same receiver.
///
/// A module's bindings, a class's methods, and a collection's methods are fixed
/// once the value exists, so those are remembered — the receiver itself is the
/// guard, and a hit means the very same receiver came back. An instance *field*
/// read is mutable state and is never remembered, and a private method's
/// visibility is checked against the caller — which can differ between two
/// accesses of one receiver — so neither is.
///
/// A string and a tuple are deliberately left out: their members are fixed too,
/// but a program builds most of them fresh, so remembering one would pay for a
/// cache write that almost never gets read back.
fn reusable_property(receiver: &Value, value: &Value) -> bool {
    match (receiver, value) {
        (Value::Dict(_), _) | (Value::Module(_), _) => true,
        (Value::Instance(_), Value::Builtin(_)) | (Value::List(_), Value::Builtin(_)) | (Value::Set(_), Value::Builtin(_)) => true,
        (Value::Instance(_), Value::BoundMethod(bound)) => {
            bound.method.function.declaration.access.is_empty() || bound.method.function.declaration.access == "public"
        }
        _ => false,
    }
}

/// The scope `hops` levels up the runtime chain: offset 0 is the current scope,
/// and the frame's own scope is the bottom of `scopes`.
///
/// `None` means the compiler named a scope this activation does not have, which
/// falls back to a name lookup rather than a panic.
fn scope_at<'a>(env: &'a Env, scopes: &'a [Env], hops: u32) -> Option<&'a Env> {
    if hops == 0 {
        Some(env)
    } else {
        scopes.len().checked_sub(hops as usize).map(|index| &scopes[index])
    }
}

/// Where one call's operands sit on the value stack, and which instruction is making
/// it.
///
/// The compiler leaves a call's arguments at the top of the stack — the positional
/// ones first, then a name/value pair per keyword — so every helper that runs a call
/// needs the same few things: the instruction, the base a failure reports from, where
/// the positional arguments start, and how many of each kind there are. Keeping them
/// together is what lets `Call` and `CallMethod` share the code that runs a call
/// without either passing half a dozen numbers to it.
struct CallSite<'a> {
    chunk: &'a Rc<Chunk>,
    /// The program counter of the instruction making the call.
    pc: usize,
    /// The index of the first positional argument.
    at: usize,
    positional: u32,
    keywords: u32,
}

impl<'a> CallSite<'a> {
    /// The call the instruction at `pc` is making, given the stack it left behind.
    fn new(chunk: &'a Rc<Chunk>, pc: usize, stack: &[Value], positional: u32, keywords: u32) -> CallSite<'a> {
        let arguments = positional as usize + 2 * keywords as usize;
        CallSite { chunk, pc, at: stack.len() - arguments, positional, keywords }
    }

    /// The positioned base a failure in this call reports from, which is where the
    /// call itself is.
    fn base(&self) -> tiger_ast::Base {
        self.chunk.base_at(self.chunk.position(self.pc))
    }

    /// The positional arguments, which sit contiguously at the top of the stack.
    fn arguments(&self) -> std::ops::Range<usize> {
        self.at..self.at + self.positional as usize
    }

    /// The keyword pairs, which sit above the positional arguments.
    fn keyword_pairs(&self) -> std::ops::Range<usize> {
        self.at + self.positional as usize..self.at + self.positional as usize + 2 * self.keywords as usize
    }

    /// What the member of this call resolved to, for the instruction that carries it.
    fn member(&self, member: u32) -> (u32, u32) {
        self.chunk.member(member)
    }
}

/// One frame a call parked inside the instruction loop.
///
/// A plain function call does not re-enter the loop. It saves the running frame
/// here and installs the callee's state in the loop's own variables. What is saved
/// is only what the caller needs when it resumes: where it was, what its operands
/// were resting on, where its result is to land, its flat locals, its environment,
/// and how tall its scopes and iterators were. Everything else the loop reads on the
/// instruction path — the chunk, the program counter, the operand stack — stays in a
/// variable of the loop and is never routed through this struct, which is the
/// difference between a frame stack that pays for itself and one that does not.
struct Parked {
    /// The chunk the caller was running.
    chunk: Rc<Chunk>,
    /// The instruction the caller resumes at: the one after its call, or after the
    /// `Pop` that a discarded result had fused into it.
    pc: usize,
    /// The caller's operand base on the shared value stack.
    base: usize,
    /// How tall the caller's scopes were when it was entered. A frame releases
    /// everything above its own mark, so a call's scopes are its own to unwind.
    scope_mark: usize,
    /// The same for the caller's iterators, which a `for` loop pushes and a return
    /// leaves behind.
    iter_mark: usize,
    /// Whether the callee's result is dropped rather than left on the stack.
    discard: bool,
    /// The caller's flat locals, which the frame leaves with.
    locals: Vec<Value>,
    /// The caller's environment.
    env: Env,
    /// A scope the callee's frame took for its own call, for the frame's return to
    /// release. A plain function whose body needs no scope has none; a method, or a
    /// function that captured a name, has the scope `bind_call` bound `this`, the
    /// parameters, and the keywords into.
    scope: Option<Env>,
}

/// The inline numeric path for a binary operator the compiler tagged.
///
/// `None` means the two values are not both numbers, or the operator keeps its
/// general meaning for them, so the caller falls back to the interpreter's own
/// operator. An `Err` is a result the interpreter rejects as well.
fn fast_binary(op: Op, left: &Value, right: &Value) -> Option<Result<Value, &'static str>> {
    let (Value::Number(a), Value::Number(b)) = (left, right) else {
        return None;
    };
    let (a, b) = (*a, *b);
    let computed = match op {
        Op::Add => Some(a + b),
        Op::Sub => Some(a - b),
        Op::Mul => Some(a * b),
        Op::Pow => Some(a.powf(b)),
        Op::Div if b != 0.0 => Some(a / b),
        Op::FloorDiv if b != 0.0 => Some((a / b).floor()),
        Op::Mod if b != 0.0 => Some(crate::modulo(a, b)),
        Op::Lt => return Some(Ok(Value::Bool(a < b))),
        Op::Le => return Some(Ok(Value::Bool(a <= b))),
        Op::Gt => return Some(Ok(Value::Bool(a > b))),
        Op::Ge => return Some(Ok(Value::Bool(a >= b))),
        Op::Eq => return Some(Ok(Value::Bool(a == b))),
        Op::Ne => return Some(Ok(Value::Bool(a != b))),
        _ => return None,
    };
    Some(match computed {
        Some(value) if !value.is_finite() => Err("non-finite numeric result"),
        Some(value) => Ok(Value::Number(value)),
        _ => return None,
    })
}

impl Evaluator {
    /// Runs a compiled chunk to completion, returning its value.
    pub(crate) fn run_chunk(&mut self, chunk: &Rc<Chunk>, env: &Env) -> Vm {
        let locals = self.take_locals(chunk.flat_count);
        self.run_frame(chunk, env, locals)
    }

    /// Takes a frame's flat locals from the pool.
    ///
    /// Every vector in the pool is at least as long as the largest frame that has used
    /// it and holds nothing but `Null`, so a frame that fits an existing one costs a
    /// pop and nothing else — no clearing pass and no re-initialization. The frame
    /// writes the slots it uses, and [`Evaluator::release_locals`] empties them again
    /// on the way out. That is what keeps a pooled frame as cheap to enter as the
    /// fixed array it replaced, which is what a call with a frame of its own — a
    /// method, or a call that passes keywords — is measured on.
    pub(crate) fn take_locals(&mut self, count: usize) -> Vec<Value> {
        match self.locals_pool.pop() {
            Some(mut locals) => {
                if locals.len() < count {
                    locals.resize(count, Value::Null);
                }
                locals
            }
            None => vec![Value::Null; count],
        }
    }

    /// Empties a frame's locals and returns them to the pool.
    ///
    /// Writing `Null` over every slot is what releases the values the frame left
    /// there. Doing it slot by slot, rather than with a `clear` followed by a
    /// `resize`, is what leaves the pool's invariant — every pooled vector all
    /// `Null` — standing without a second pass to rebuild it.
    pub(crate) fn release_locals(&mut self, mut locals: Vec<Value>) {
        locals.fill(Value::Null);
        self.locals_pool.push(locals);
    }

    /// Runs a chunk in a frame of its own, which is how a chunk that is not a plain
    /// function call — a script, a try body, a `finally`, a method still on the
    /// general call path — gets a frame.
    ///
    /// This is the one call on the hot path that crosses a module boundary, so it
    /// asks to be inlined.
    #[inline]
    pub(crate) fn run_frame(&mut self, chunk: &Rc<Chunk>, env: &Env, locals: Vec<Value>) -> Vm {
        // The depth a plain function call inside the loop adds is undone by the
        // `Return` that leaves its frame. An error that unwinds the whole loop leaves
        // those frames standing, so the count is restored here: a caught error must
        // not leave the recursion limit raised for the rest of the run.
        let entry = self.depth;
        let mut stack = self.stack_pool.pop().unwrap_or_default();
        let result = self.run_loop(chunk, env, &mut stack, locals);
        self.depth = entry;
        stack.clear();
        self.stack_pool.push(stack);
        result
    }

    /// Runs a plain function whose body needs no scope of its own and whose parameters all
    /// have flat slots, which is the shape most calls have.
    ///
    /// `None` means the callee is not that shape, and the caller takes the general call path.
    /// The arity checks that path makes cannot fail here: the fast path is only taken when the
    /// call passes exactly one argument per parameter, with nothing variadic and no keywords,
    /// which is the one case that is neither short nor long.
    ///
    /// This is one Rust activation where the general path is four: `invoke`, which decides
    /// whether the call needs a scope and pools one; `run_compiled`, which takes the frame's
    /// locals and holds the depth limit; `bind_call`, which reads the declaration's variadic,
    /// keyword, and default parameters only to find that none apply; and `run_frame`, which
    /// pools the operand stack. What is left here is the depth limit and the binding itself.
    ///
    /// A call reached from an instruction does not come here: `Instr::Call` enters a
    /// frame of this shape by parking one inside the loop, which is what keeps a
    /// plain call from re-entering the engine. This is the path the same call takes
    /// when it arrives through a member that resolved to a function, or through the
    /// general call path, where the callee has to be established first.
    ///
    /// A method never takes this path: a method's frame is never skipped, because its call sets
    /// the class context its body checks visibility against, so `frame_needed` is always true.
    pub(crate) fn call_flat_function(
        &mut self,
        base: &tiger_ast::Base,
        function: &Rc<FunctionValue>,
        arguments: &mut [Value],
    ) -> Result<Option<Value>, Signal> {
        let Some(code) = &function.code else { return Ok(None) };
        let layout = &code.call_layout;
        if !code.flat_callable || arguments.len() != layout.parameters.len() {
            return Ok(None);
        }
        self.depth += 1;
        if self.depth >= 512 {
            self.depth -= 1;
            return Err(crate::fatal(base, "maximum evaluation depth exceeded".to_string()));
        }
        // A frame whose flat locals fit a small fixed array keeps them on this activation's own
        // stack, as `run_compiled` does it, so a call allocates nothing.
        let mut locals = self.take_locals(code.flat_count);
        // The argument moves into the frame: the caller's copy is dropped when the call
        // returns, so cloning it would be a copy thrown away. A flat callable's parameters
        // own the frame's first slots, so one slice operation moves them all, and what is
        // left behind in the caller's slice is `Null` for it to drop.
        let fixed = layout.parameters.len();
        debug_assert!(
            layout.parameters.iter().enumerate().all(|(index, slot)| *slot == Some(index as u32)),
            "a flat call's parameters own the frame's first slots"
        );
        locals[..fixed].swap_with_slice(&mut arguments[..fixed]);
        let result = self.run_frame(code, &function.env, locals);
        self.depth -= 1;
        // A `return` inside the body leaves its chunk as a signal, and the function boundary is
        // where it becomes the result — the same place `invoke` turns it.
        match result {
            Err(Signal::Return(value)) => Ok(Some(value)),
            Ok(value) => Ok(Some(value)),
            Err(signal) => Err(signal),
        }
    }

    /// The instruction loop; `stack` and `locals` are pooled by the caller so a call
    /// allocates neither.
    ///
    /// A call to a plain function does not re-enter this loop. It parks the running
    /// frame in `frames`, loads the callee's state into this function's own variables,
    /// and carries on, and `Instr::Return` puts back what it parked. The state the
    /// dispatch path reads — the chunk, the program counter, the operand stack, the
    /// frame's locals — is therefore still a variable of this loop rather than a field
    /// of a frame, so only a call pays anything for the flattening.
    ///
    /// One value stack serves every frame in the loop, so a frame's operands start at
    /// `base` and a return truncates back to it instead of swapping a buffer in.
    fn run_loop(&mut self, chunk: &Rc<Chunk>, env: &Env, stack: &mut Vec<Value>, mut locals: Vec<Value>) -> Vm {
        // The running frame's state, which is exactly what a call parks and a return
        // restores.
        let mut chunk = chunk.clone();
        let mut env = env.clone();
        let mut scopes: Vec<Env> = Vec::new();
        let mut iters: Vec<IterState> = Vec::new();
        let mut frames: Vec<Parked> = Vec::new();
        // `base` is where this frame's operands start on the shared stack; the two
        // marks are how tall the scopes and iterators were when the frame was entered,
        // so a return unwinds exactly what this frame pushed and no more.
        let mut base = 0usize;
        let mut scope_mark = 0usize;
        let mut iter_mark = 0usize;
        let mut pc = 0usize;
        // The limit is read once, because it cannot change while a chunk runs and
        // this test is on every instruction.
        let limit = if self.max_steps == 0 { u64::MAX } else { self.max_steps };
        loop {
            // The step counter exists only to enforce `max_steps`, so a run with no
            // limit skips the accounting entirely: nothing else reads it, and a
            // counted-but-unread field would be a store on every instruction.
            if limit != u64::MAX {
                self.steps += 1;
                if self.steps > limit {
                    return Err(crate::fatal(&chunk.base(pc), "execution step limit exceeded".to_string()));
                }
            }
            let instr = chunk.code[pc];
            #[cfg(feature = "profile")]
            crate::profile::record(instr);
            pc += 1;
            match instr {
                Instr::Constant(index) => stack.push(chunk.constants[index as usize].clone()),
                Instr::Null => stack.push(Value::Null),
                Instr::LoadName(index) => {
                    // A global or built-in name read is the same read every pass
                    // through a loop, and resolving it hashes the name at every level
                    // of the scope chain, so the value it resolved to is remembered
                    // against the binding epoch — see `name_cache`.
                    let epoch = object::binding_epoch();
                    let remembered = {
                        let cache = chunk.name_cache.borrow();
                        match cache.get(pc - 1).and_then(|slot| slot.as_ref()) {
                            Some((scope, at, value)) if *at == epoch && Rc::ptr_eq(scope, &env) => Some(value.clone()),
                            _ => None,
                        }
                    };
                    let value = match remembered {
                        Some(value) => value,
                        None => {
                            let value = object::get(&env, &chunk.names[index as usize])
                                .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                            if let Some(slot) = chunk.name_cache.borrow_mut().get_mut(pc - 1) {
                                *slot = Some((env.clone(), epoch, value.clone()));
                            }
                            value
                        }
                    };
                    stack.push(value);
                }
                // A flat slot belongs to this frame alone, so this is an index into
                // `locals` with no borrow and no scope to find first.
                Instr::LoadFlat(slot) => stack.push(locals[slot as usize].clone()),
                Instr::StoreFlat { name, slot, constant, .. } => {
                    if constant {
                        let name = &chunk.names[name as usize];
                        return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), format!("cannot reassign const {:?}", name)));
                    }
                    let value = stack.pop().expect("stack");
                    locals[slot as usize] = value;
                }
                Instr::DefineFlat { name, slot, duplicate } => {
                    if duplicate {
                        let name = &chunk.names[name as usize];
                        return Err(runtime(
                            &chunk.base_at(chunk.position(pc - 1)),
                            format!("{:?} is already declared in this scope", name),
                        ));
                    }
                    let value = stack.pop().expect("stack");
                    locals[slot as usize] = value;
                }
                Instr::LoadLocal { name, hops, slot } => {
                    let name = &chunk.names[name as usize];
                    let slot = slot as usize;
                    let value = match scope_at(&env, &scopes, hops) {
                        Some(scope) => {
                            let borrowed = scope.borrow();
                            match borrowed.bindings.binding_at(slot) {
                                Some(binding) => {
                                    debug_assert_eq!(borrowed.bindings.name_at(slot), Some(&**name), "local slot");
                                    Some(binding.value.clone())
                                }
                                None => None,
                            }
                        }
                        None => None,
                    };
                    match value {
                        Some(value) => stack.push(value),
                        None => {
                            stack.push(object::get(&env, name).map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?)
                        }
                    }
                }
                Instr::LoadSuper(slot) => match slot {
                    Some(slot) => stack.push(locals[slot as usize].clone()),
                    None => match object::get(&env, "super") {
                        Ok(value) => stack.push(value),
                        Err(_) => {
                            return Err(runtime(
                                &chunk.base_at(chunk.position(pc - 1)),
                                "super is only available inside methods".to_string(),
                            ))
                        }
                    },
                },
                Instr::StoreName(index) => {
                    let value = stack.pop().expect("stack");
                    object::assign(&env, &chunk.names[index as usize], value)
                        .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                }
                Instr::StoreLocal { name, hops, slot } => {
                    let value = stack.pop().expect("stack");
                    let name = &chunk.names[name as usize];
                    let slot = slot as usize;
                    let stored = match scope_at(&env, &scopes, hops) {
                        Some(scope) => {
                            let mut borrowed = scope.borrow_mut();
                            if borrowed.bindings.binding_at(slot).is_some() {
                                debug_assert_eq!(borrowed.bindings.name_at(slot), Some(&**name), "local slot");
                                let stored = borrowed
                                    .bindings
                                    .set_at(slot, value)
                                    .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message));
                                drop(borrowed);
                                stored
                            } else {
                                drop(borrowed);
                                object::assign(scope, name, value)
                                    .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))
                            }
                        }
                        None => {
                            object::assign(&env, name, value).map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))
                        }
                    };
                    stored?;
                }
                Instr::DefineName { name, constant } => {
                    let value = stack.pop().expect("stack");
                    // `chunk.names[i]` is already a shared name, so the binding takes a
                    // clone of it rather than building a fresh one: a `for name in ...`
                    // loop declares its variable on every pass.
                    object::define_shared(&env, &chunk.names[name as usize], value, constant)
                        .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                }
                Instr::DefineLocal { name, slot, constant } => {
                    let value = stack.pop().expect("stack");
                    object::define_at_shared(&env, &chunk.names[name as usize], slot as usize, value, constant)
                        .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                }
                Instr::Equal => {
                    let right = stack.pop().expect("stack");
                    let left = stack.pop().expect("stack");
                    stack.push(Value::Bool(object::equal(&left, &right)));
                }
                Instr::Pop => {
                    stack.pop();
                }
                Instr::Dup => {
                    let value = stack.last().expect("stack").clone();
                    stack.push(value);
                }
                // Resolving a target and reading it are one step, because a compound
                // assignment and an update both need the target resolved once and
                // left in place for the store. The validation mirrors the
                // interpreter's `reference` and `read_target` exactly, so the
                // diagnostics and their positions match.
                Instr::ReadIndexTarget => {
                    let key = stack.pop().expect("stack");
                    let collection = stack.pop().expect("stack");
                    let base = chunk.base_at(chunk.position(pc - 1));
                    let value = match &collection {
                        Value::List(elements) => {
                            let index = self.index_at(&base, &key, elements.borrow().len())?;
                            elements.borrow()[index].clone()
                        }
                        Value::Dict(dict) => match dict.borrow().get(&key) {
                            Ok(Some(value)) => value,
                            Ok(None) => return Err(runtime(&base, format!("dictionary key not found: {}", object::format(&key)))),
                            Err(message) => return Err(runtime(&base, message)),
                        },
                        other => return Err(runtime(&base, format!("cannot assign an index of {}", other.type_name()))),
                    };
                    stack.push(collection);
                    stack.push(key);
                    stack.push(value);
                }
                Instr::ReadPropertyTarget(index) => {
                    let receiver = stack.pop().expect("stack");
                    if !matches!(receiver, Value::Instance(_)) {
                        let base = chunk.base_at(chunk.position(pc - 1));
                        return Err(runtime(&base, format!("cannot assign a property of {}", receiver.type_name())));
                    }
                    let value = self.cached_property(&chunk, pc - 1, index as usize, &receiver, chunk.position(pc - 1), &env)?;
                    stack.push(receiver);
                    stack.push(value);
                }
                // Inline the numeric operators the compiler tagged, so an addition
                // or comparison does not call out to the interpreter.
                Instr::Binary { op, name } => {
                    let right = stack.pop().expect("stack");
                    let left = stack.pop().expect("stack");
                    let value = match fast_binary(op, &left, &right) {
                        Some(Ok(value)) => value,
                        Some(Err(message)) => return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), message.to_string())),
                        None => self.binary(&chunk.base_at(chunk.position(pc - 1)), &chunk.names[name as usize], left, right)?,
                    };
                    stack.push(value);
                }
                // The three fused forms below replace a pair of loads or a constant
                // and an operator. Each steps over the instructions it replaced.
                Instr::FlatBinaryFlat { a, b, op, name } => {
                    let left = &locals[a as usize];
                    let right = &locals[b as usize];
                    let value = match fast_binary(op, left, right) {
                        Some(Ok(value)) => value,
                        Some(Err(message)) => return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), message.to_string())),
                        None => {
                            let (left, right) = (left.clone(), right.clone());
                            self.binary(&chunk.base_at(chunk.position(pc - 1)), &chunk.names[name as usize], left, right)?
                        }
                    };
                    stack.push(value);
                    pc += 2;
                }
                Instr::FlatBinaryConstant { a, b, op, name } => {
                    let left = &locals[a as usize];
                    let right = &chunk.constants[b as usize];
                    let value = match fast_binary(op, left, right) {
                        Some(Ok(value)) => value,
                        Some(Err(message)) => return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), message.to_string())),
                        None => {
                            let (left, right) = (left.clone(), right.clone());
                            self.binary(&chunk.base_at(chunk.position(pc - 1)), &chunk.names[name as usize], left, right)?
                        }
                    };
                    stack.push(value);
                    pc += 2;
                }
                // The fused property read: the receiver is a flat slot, and the cache
                // slot is the one the `GetProperty` it replaced would have used.
                Instr::FlatProperty { value, name, cache } => {
                    let slot = cache as usize;
                    let value = self.cached_property(&chunk, slot, name as usize, &locals[value as usize], chunk.position(slot), &env)?;
                    stack.push(value);
                    pc += 1;
                }
                // The `Call` after this one is the one this name was loaded for, so
                // this only has to put the callee where that call expects to find it.
                Instr::CallName { name } => {
                    let epoch = object::binding_epoch();
                    let remembered = {
                        let cache = chunk.name_cache.borrow();
                        match cache.get(pc - 1).and_then(|slot| slot.as_ref()) {
                            Some((scope, at, value)) if *at == epoch && Rc::ptr_eq(scope, &env) => Some(value.clone()),
                            _ => None,
                        }
                    };
                    let value = match remembered {
                        Some(value) => value,
                        None => {
                            let value = object::get(&env, &chunk.names[name as usize])
                                .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                            if let Some(slot) = chunk.name_cache.borrow_mut().get_mut(pc - 1) {
                                *slot = Some((env.clone(), epoch, value.clone()));
                            }
                            value
                        }
                    };
                    stack.push(value);
                }
                Instr::BinaryConstant { b, op, name } => {
                    let left = stack.pop().expect("stack");
                    let right = &chunk.constants[b as usize];
                    let value = match fast_binary(op, &left, right) {
                        Some(Ok(value)) => value,
                        Some(Err(message)) => return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), message.to_string())),
                        None => {
                            let right = right.clone();
                            self.binary(&chunk.base_at(chunk.position(pc - 1)), &chunk.names[name as usize], left, right)?
                        }
                    };
                    stack.push(value);
                    pc += 1;
                }
                Instr::Unary(index) => {
                    let value = stack.pop().expect("stack");
                    let value = self.unary(&chunk.base_at(chunk.position(pc - 1)), &chunk.names[index as usize], value)?;
                    stack.push(value);
                }
                Instr::Jump(target) => pc = target as usize,
                Instr::JumpIfFalse(target) => {
                    if !object::truthy(&stack.pop().expect("stack")) {
                        pc = target as usize;
                    }
                }
                // A comparison that only feeds a branch never has to become a `bool`
                // value, so these skip building it and testing its truthiness. The
                // fused forms step over the instructions they replaced.
                Instr::JumpUnless { op, name, target } => {
                    let right = stack.pop().expect("stack");
                    let left = stack.pop().expect("stack");
                    let holds = self.test(&chunk.base_at(chunk.position(pc - 1)), op, &chunk.names[name as usize], &left, &right)?;
                    // Step over the branch this replaced before taking its target.
                    pc += 1;
                    if !holds {
                        pc = target as usize;
                    }
                }
                Instr::FlatJumpUnlessFlat { a, b, op, name, target } => {
                    let holds = self.test(
                        &chunk.base_at(chunk.position(pc - 1)),
                        op,
                        &chunk.names[name as usize],
                        &locals[a as usize],
                        &locals[b as usize],
                    )?;
                    pc += 3;
                    if !holds {
                        pc = target as usize;
                    }
                }
                Instr::FlatJumpUnlessConstant { a, b, op, name, target } => {
                    let holds = self.test(
                        &chunk.base_at(chunk.position(pc - 1)),
                        op,
                        &chunk.names[name as usize],
                        &locals[a as usize],
                        &chunk.constants[b as usize],
                    )?;
                    pc += 3;
                    if !holds {
                        pc = target as usize;
                    }
                }
                Instr::JumpIfFalseOrPop(target) => {
                    if !object::truthy(stack.last().expect("stack")) {
                        pc = target as usize;
                    } else {
                        stack.pop();
                    }
                }
                Instr::JumpIfTrueOrPop(target) => {
                    if object::truthy(stack.last().expect("stack")) {
                        pc = target as usize;
                    } else {
                        stack.pop();
                    }
                }
                Instr::BuildList(count) => {
                    let at = stack.len() - count as usize;
                    let elements = stack.split_off(at);
                    stack.push(Value::list(elements));
                }
                Instr::BuildTuple(count) => {
                    let at = stack.len() - count as usize;
                    let elements = stack.split_off(at);
                    stack.push(Value::tuple(elements));
                }
                Instr::BuildSet(count) => {
                    let at = stack.len() - count as usize;
                    let elements = stack.split_off(at);
                    stack.push(Value::set(elements).map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?);
                }
                Instr::BuildDict(count) => {
                    let at = stack.len() - 2 * count as usize;
                    let entries = stack.split_off(at);
                    let mut dict = object::Dict::default();
                    for pair in entries.chunks(2) {
                        dict.set(pair[0].clone(), pair[1].clone())
                            .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                    }
                    stack.push(Value::Dict(Rc::new(std::cell::RefCell::new(dict))));
                }
                Instr::MakeFunction(index) => {
                    let proto = &chunk.functions[index as usize];
                    let function =
                        FunctionValue { declaration: proto.declaration.clone(), env: env.clone(), code: Some(proto.code.clone()) };
                    stack.push(Value::Function(Rc::new(function)));
                }
                Instr::Class(index) => {
                    self.define_class_code(&chunk.classes[index as usize], &env)?;
                }
                Instr::Call { positional, keywords, discard } => {
                    let keyword_count = keywords as usize;
                    let argument_count = positional as usize + 2 * keyword_count;
                    let at = stack.len() - argument_count;
                    // The positional arguments sit contiguously at the top of the value
                    // stack, so the call reads them straight out of it: no buffer and no
                    // copy. A call that passes keywords materializes those, which is the
                    // only call that allocates anything.
                    let callee = std::mem::replace(&mut stack[at - 1], Value::Null);
                    // A built-in called with positional arguments is the shape most of
                    // a standard-library-heavy program's work takes — `len`, `math.max`,
                    // `str` — so it is answered here rather than through the general
                    // call path's dispatch. The base a failure would need is built only
                    // if one happens, because building one clones the shared source name
                    // and that is a write to a refcount on every call.
                    if keyword_count == 0 {
                        if let Value::Builtin(builtin) = &callee {
                            if !builtin.higher_order && !builtin.consumes_iterables {
                                let value = (builtin.call)(self, &stack[at..at + positional as usize], &[])
                                    .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                                stack.truncate(at - 1);
                                if discard {
                                    pc += 1;
                                } else {
                                    stack.push(value);
                                }
                                continue;
                            }
                        }
                        // A user function is the next most common callee after a
                        // built-in, and it is the one the general path's type dispatch
                        // and argument plumbing exists for the sake of everything else.
                        // Calling it here skips that match and the slice it would
                        // rebuild — and, for a function whose parameters are all flat
                        // slots and whose body needs no scope, the frame it runs in as
                        // well: that shape is entered by parking this frame inside the
                        // loop instead of re-entering it, which is what takes the call
                        // off the Rust stack entirely.
                        if let Value::Function(function) = &callee {
                            // The shape the compiler proved about the declaration: every
                            // parameter in a flat slot, nothing variadic, no keywords,
                            // no `super`, and a body that needs no scope of its own.
                            // The arity the general path checks cannot be wrong for it,
                            // because the compiler gave every parameter a slot and a
                            // call of it passes one argument per parameter — which is
                            // what the count is compared against here.
                            let flat = match &function.code {
                                Some(code) if code.flat_callable && positional as usize == code.call_layout.parameters.len() => Some(code),
                                _ => None,
                            };
                            if let Some(code) = flat {
                                self.depth += 1;
                                if self.depth >= 512 {
                                    self.depth -= 1;
                                    let base = chunk.base_at(chunk.position(pc - 1));
                                    return Err(crate::fatal(&base, "maximum evaluation depth exceeded".to_string()));
                                }
                                let mut bound = self.take_locals(code.flat_count);
                                // The compiler seeds a flat callable's frame with its parameters
                                // first, so their slots are exactly `0..n`: one slice operation
                                // moves the arguments in and leaves `Null` where the truncate
                                // below discards them anyway, rather than a loop that reads an
                                // `Option`, checks it, and indexes twice per argument.
                                let fixed = code.call_layout.parameters.len();
                                debug_assert!(
                                    code.call_layout.parameters.iter().enumerate().all(|(index, slot)| *slot == Some(index as u32)),
                                    "a flat call's parameters own the frame's first slots"
                                );
                                bound[..fixed].swap_with_slice(&mut stack[at..at + fixed]);
                                // The callee's operands take the stack over from here,
                                // so the caller's arguments go; the frame's own base is
                                // where they were, and the result lands one slot below
                                // it, which is where the callee was.
                                stack.truncate(at);
                                frames.push(Parked {
                                    chunk: std::mem::replace(&mut chunk, code.clone()),
                                    // A call whose value is dropped had a `Pop` fused
                                    // out of it, and that instruction is still in the
                                    // code for the loop to step over — which is what the
                                    // general path's own `pc += 1` does for the same
                                    // reason.
                                    pc: if discard { pc + 1 } else { pc },
                                    base,
                                    scope_mark,
                                    iter_mark,
                                    discard,
                                    locals: std::mem::replace(&mut locals, bound),
                                    env: std::mem::replace(&mut env, function.env.clone()),
                                    scope: None,
                                });
                                scope_mark = scopes.len();
                                iter_mark = iters.len();
                                base = at;
                                pc = 0;
                                continue;
                            }
                            // Everything else a compiled function can be is entered below,
                            // by the preparation `invoke` itself uses, because it may need
                            // a scope of its own or may be passing keywords. A function
                            // with no chunk at all is the tree-walker's and has no frame to
                            // enter.
                            if function.code.is_none() {
                                let value = self.invoke(
                                    &chunk.base_at(chunk.position(pc - 1)),
                                    function,
                                    &mut stack[at..at + positional as usize],
                                    &[],
                                    None,
                                )?;
                                stack.truncate(at - 1);
                                if discard {
                                    pc += 1;
                                } else {
                                    stack.push(value);
                                }
                                continue;
                            }
                        }
                    }
                    // Everything else with a compiled body is entered by parking this
                    // frame above the one `prepare_frame` builds: a function whose body
                    // needs a scope of its own because it captured a name, one with more
                    // flat slots than a frame carries, a call that passes keywords, and a
                    // bound method, which is a method with its receiver already in hand.
                    // A function the tree-walker runs has no frame to enter and falls
                    // through to the path that runs it.
                    let target = match &callee {
                        Value::Function(function) if function.code.is_some() => Some((function, None)),
                        Value::BoundMethod(bound) if bound.method.function.code.is_some() => {
                            Some((&bound.method.function, Some((&bound.method, &bound.receiver))))
                        }
                        _ => None,
                    };
                    if let Some((function, bound)) = target {
                        let site = CallSite::new(&chunk, pc - 1, stack, positional, keywords);
                        let named = self.take_keywords(&site, stack);
                        let prepared = self.prepare_frame(&site.base(), function, &mut stack[at..at + positional as usize], &named, bound);
                        if keyword_count > 0 {
                            self.recycle_keywords(named);
                        }
                        if let Some(prepared) = prepared? {
                            stack.truncate(at);
                            frames.push(Parked {
                                chunk: std::mem::replace(&mut chunk, prepared.code.clone()),
                                pc: if discard { pc + 1 } else { pc },
                                base,
                                scope_mark,
                                iter_mark,
                                discard,
                                locals: std::mem::replace(&mut locals, prepared.locals),
                                env: std::mem::replace(&mut env, prepared.env),
                                scope: prepared.scope,
                            });
                            scope_mark = scopes.len();
                            iter_mark = iters.len();
                            base = at;
                            pc = 0;
                            continue;
                        }
                    }
                    let call = CallSite::new(&chunk, pc - 1, stack, positional, keywords);
                    let value = self.finish_call(&call, callee, stack, &env)?;
                    stack.truncate(at - 1);
                    if discard {
                        pc += 1;
                    } else {
                        stack.push(value);
                    }
                }
                // A method call: the member is resolved and the call made in one
                // dispatch, and no callable object is built for the shapes that would
                // otherwise build one. See `call_method`.
                Instr::CallMethod { name, positional, keywords, member, discard } => {
                    let keyword_count = keywords as usize;
                    let argument_count = positional as usize + 2 * keyword_count;
                    let at = stack.len() - argument_count;
                    let receiver = std::mem::replace(&mut stack[at - 1], Value::Null);
                    // A class method is entered as a frame of its own, the way a plain
                    // call is. The member is resolved here, which is what `call_method`
                    // would do next anyway, and the frame is prepared by the code
                    // `invoke` uses — so a plain call, a call through a member that
                    // resolved to a function, and a method call all bind their arguments
                    // in one place. The length spellings `call_method` answers before it
                    // can never match an instance, so resolving the method first cannot
                    // move another answer past this one.
                    let method = match &receiver {
                        Value::Instance(target) => {
                            let label = chunk.names[name as usize].clone();
                            self.method_of(&chunk, pc - 1, &label, chunk.member(member), target, &env)?
                        }
                        _ => None,
                    };
                    if let Some(method) = method {
                        let Value::Instance(target) = &receiver else { unreachable!("a method is found on an instance") };
                        let site = CallSite::new(&chunk, pc - 1, stack, positional, keywords);
                        let named = self.take_keywords(&site, stack);
                        let prepared = self.prepare_frame(
                            &site.base(),
                            &method.function,
                            &mut stack[at..at + positional as usize],
                            &named,
                            Some((&method, target)),
                        );
                        if keyword_count > 0 {
                            self.recycle_keywords(named);
                        }
                        if let Some(prepared) = prepared? {
                            stack.truncate(at);
                            frames.push(Parked {
                                chunk: std::mem::replace(&mut chunk, prepared.code.clone()),
                                pc: if discard { pc + 1 } else { pc },
                                base,
                                scope_mark,
                                iter_mark,
                                discard,
                                locals: std::mem::replace(&mut locals, prepared.locals),
                                env: std::mem::replace(&mut env, prepared.env),
                                scope: prepared.scope,
                            });
                            scope_mark = scopes.len();
                            iter_mark = iters.len();
                            base = at;
                            pc = 0;
                            continue;
                        }
                    }
                    let call = CallSite::new(&chunk, pc - 1, stack, positional, keywords);
                    let value = self.call_method(call, name as usize, member, receiver, stack, &env)?;
                    stack.truncate(at - 1);
                    if discard {
                        pc += 1;
                    } else {
                        stack.push(value);
                    }
                }
                // A counting loop's step: the update and the back edge in one
                // dispatch, with the value the update produced discarded.
                Instr::UpdateFlatJump { name, slot, decrement, target } => {
                    let previous = match &locals[slot as usize] {
                        Value::Number(number) => *number,
                        _ => {
                            return Err(runtime(
                                &chunk.base_at(chunk.position(pc - 1)),
                                format!("{} requires a number", chunk.names[name as usize]),
                            ))
                        }
                    };
                    let updated = if decrement { previous - 1.0 } else { previous + 1.0 };
                    if !updated.is_finite() {
                        return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), "non-finite numeric result".to_string()));
                    }
                    locals[slot as usize] = Value::Number(updated);
                    pc = target as usize;
                }
                // An assignment whose value is a computation over flat locals: the
                // operator and the store in one dispatch, with the result going
                // straight into its slot instead of through the operand stack.
                Instr::FlatBinaryFlatStore { a, b, op, name, out } => {
                    // `s += more` with both sides in the frame's own locals, where the
                    // accumulator is its own target: grow the string instead of copying
                    // it whole. `b != out` keeps the two borrows disjoint.
                    if a == out && b != out && matches!(op, Op::Add) {
                        let more = locals[b as usize].clone();
                        if append_in_place(op, true, &mut locals[out as usize], &more) {
                            pc += 3;
                            continue;
                        }
                    }
                    let left = &locals[a as usize];
                    let right = &locals[b as usize];
                    let value = match fast_binary(op, left, right) {
                        Some(Ok(value)) => value,
                        Some(Err(message)) => return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), message.to_string())),
                        None => {
                            let (left, right) = (left.clone(), right.clone());
                            self.binary(&chunk.base_at(chunk.position(pc - 1)), &chunk.names[name as usize], left, right)?
                        }
                    };
                    locals[out as usize] = value;
                    pc += 3;
                }
                Instr::FlatBinaryConstantStore { a, b, op, name, out } => {
                    // The same, with the right operand a constant: `s += "ab"` in a loop.
                    if a == out && append_in_place(op, true, &mut locals[out as usize], &chunk.constants[b as usize]) {
                        pc += 3;
                        continue;
                    }
                    let left = &locals[a as usize];
                    let right = &chunk.constants[b as usize];
                    let value = match fast_binary(op, left, right) {
                        Some(Ok(value)) => value,
                        Some(Err(message)) => return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), message.to_string())),
                        None => {
                            let (left, right) = (left.clone(), right.clone());
                            self.binary(&chunk.base_at(chunk.position(pc - 1)), &chunk.names[name as usize], left, right)?
                        }
                    };
                    locals[out as usize] = value;
                    pc += 3;
                }
                // The tail of an assignment whose value comes off the operand stack:
                // the operator and the store in one dispatch, so `total += items[i]`
                // and `total += f(x)` write their result straight into its slot.
                Instr::BinaryStore { op, name, out, reuse } => {
                    let right = stack.pop().expect("stack");
                    let left = stack.pop().expect("stack");
                    let value = match (left, right, reuse) {
                        // `x += items[i]`: the compiler proved the operand wrote none
                        // of the frame's own slots, so what is on the stack is still the
                        // value the target holds. Emptying the slot drops that handle,
                        // which leaves the string with one — and one is what lets it
                        // grow in place rather than being copied whole.
                        (Value::String(mut text), Value::String(more), true) if matches!(op, Op::Add) => {
                            locals[out as usize] = Value::Null;
                            text.append(more.as_str());
                            Value::String(text)
                        }
                        (left, right, _) => match fast_binary(op, &left, &right) {
                            Some(Ok(value)) => value,
                            Some(Err(message)) => return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), message.to_string())),
                            None => self.binary(&chunk.base_at(chunk.position(pc - 1)), &chunk.names[name as usize], left, right)?,
                        },
                    };
                    locals[out as usize] = value;
                    pc += 1;
                }
                Instr::GetIndex => {
                    let key = stack.pop().expect("stack");
                    let collection = stack.pop().expect("stack");
                    let value = match fast_index(&collection, &key) {
                        Some(value) => value,
                        None => self.index_value(&chunk.base_at(chunk.position(pc - 1)), collection, key)?,
                    };
                    stack.push(value);
                }
                // A flat index read and write: `items[index]` and `items[index] = n`
                // with both sides in the frame's own locals, so no operand is pushed
                // and popped and no operand stack is touched.
                Instr::FlatIndexFlat { a, b } => {
                    let value = match fast_index(&locals[a as usize], &locals[b as usize]) {
                        Some(value) => value,
                        None => {
                            let collection = locals[a as usize].clone();
                            let key = locals[b as usize].clone();
                            self.index_value(&chunk.base_at(chunk.position(pc - 1)), collection, key)?
                        }
                    };
                    stack.push(value);
                    pc += 2;
                }
                Instr::FlatIndexConstant { a, b } => {
                    let value = match fast_index(&locals[a as usize], &chunk.constants[b as usize]) {
                        Some(value) => value,
                        None => {
                            let collection = locals[a as usize].clone();
                            let key = chunk.constants[b as usize].clone();
                            self.index_value(&chunk.base_at(chunk.position(pc - 1)), collection, key)?
                        }
                    };
                    stack.push(value);
                    pc += 2;
                }
                Instr::FlatStoreIndex { value, collection, key } => {
                    if !fast_store_index(&locals[collection as usize], &locals[key as usize], &locals[value as usize]) {
                        let value = locals[value as usize].clone();
                        let collection = locals[collection as usize].clone();
                        let key = locals[key as usize].clone();
                        self.set_index(&chunk.base_at(chunk.position(pc - 1)), collection, key, value)?;
                    }
                    pc += 3;
                }
                Instr::GetProperty(index) => {
                    let receiver = stack.pop().expect("stack");
                    let value = self.cached_property(&chunk, pc - 1, index as usize, &receiver, chunk.position(pc - 1), &env)?;
                    stack.push(value);
                }
                Instr::SetIndex => {
                    let key = stack.pop().expect("stack");
                    let collection = stack.pop().expect("stack");
                    let value = stack.pop().expect("stack");
                    if !fast_store_index(&collection, &key, &value) {
                        self.set_index(&chunk.base_at(chunk.position(pc - 1)), collection, key, value)?;
                    }
                }
                Instr::SetProperty(index) => {
                    let receiver = stack.pop().expect("stack");
                    let value = stack.pop().expect("stack");
                    self.store_property(&chunk, pc - 1, index as usize, receiver, value, &env)?;
                }
                // The read-modify-write tails. Each target was resolved and read once,
                // so these reuse what is already below the value instead of evaluating
                // the target a second time.
                Instr::SetIndexKeep => {
                    let value = stack.pop().expect("stack");
                    let key = stack.pop().expect("stack");
                    let collection = stack.pop().expect("stack");
                    self.set_index(&chunk.base_at(chunk.position(pc - 1)), collection, key, value)?;
                }
                Instr::SetPropertyKeep(index) => {
                    let value = stack.pop().expect("stack");
                    let receiver = stack.pop().expect("stack");
                    self.store_property(&chunk, pc - 1, index as usize, receiver, value, &env)?;
                }
                Instr::UpdateIndex { name, decrement, prefix } => {
                    let value = stack.pop().expect("stack");
                    let key = stack.pop().expect("stack");
                    let collection = stack.pop().expect("stack");
                    let base = chunk.base_at(chunk.position(pc - 1));
                    let Value::Number(previous) = value else {
                        return Err(runtime(&base, format!("{} requires a number", chunk.names[name as usize])));
                    };
                    let updated = if decrement { previous - 1.0 } else { previous + 1.0 };
                    if !updated.is_finite() {
                        return Err(runtime(&base, "non-finite numeric result".to_string()));
                    }
                    self.set_index(&base, collection, key, Value::Number(updated))?;
                    stack.push(Value::Number(if prefix { updated } else { previous }));
                }
                Instr::UpdateProperty { property, name, decrement, prefix } => {
                    let value = stack.pop().expect("stack");
                    let receiver = stack.pop().expect("stack");
                    let base = chunk.base_at(chunk.position(pc - 1));
                    let Value::Number(previous) = value else {
                        return Err(runtime(&base, format!("{} requires a number", chunk.names[name as usize])));
                    };
                    let updated = if decrement { previous - 1.0 } else { previous + 1.0 };
                    if !updated.is_finite() {
                        return Err(runtime(&base, "non-finite numeric result".to_string()));
                    }
                    self.store_property(&chunk, pc - 1, property as usize, receiver, Value::Number(updated), &env)?;
                    stack.push(Value::Number(if prefix { updated } else { previous }));
                }
                // The flat form of an update, which is what a counting loop's
                // counter uses. The order is the interpreter's: read, require a
                // number, compute, require a finite result, then write — so a write
                // to a `const` is refused only after the value has been checked.
                Instr::UpdateFlat { name, slot, constant, decrement, prefix } => {
                    let previous = match &locals[slot as usize] {
                        Value::Number(number) => *number,
                        _ => {
                            return Err(runtime(
                                &chunk.base_at(chunk.position(pc - 1)),
                                format!("{} requires a number", chunk.names[name as usize]),
                            ))
                        }
                    };
                    let updated = if decrement { previous - 1.0 } else { previous + 1.0 };
                    if !updated.is_finite() {
                        return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), "non-finite numeric result".to_string()));
                    }
                    if constant {
                        return Err(runtime(
                            &chunk.base_at(chunk.position(pc - 1)),
                            format!("cannot reassign const {:?}", chunk.names[name as usize]),
                        ));
                    }
                    locals[slot as usize] = Value::Number(updated);
                    stack.push(Value::Number(if prefix { updated } else { previous }));
                }
                Instr::UpdateName { name, decrement, prefix } => {
                    let value =
                        self.update_name(&chunk.base_at(chunk.position(pc - 1)), &env, &chunk.names[name as usize], decrement, prefix)?;
                    stack.push(value);
                }
                Instr::UpdateLocal { name, hops, slot, decrement, prefix } => {
                    let name = &chunk.names[name as usize];
                    let slot = slot as usize;
                    let Some(scope) = scope_at(&env, &scopes, hops) else {
                        let value = self.update_name(&chunk.base_at(chunk.position(pc - 1)), &env, name, decrement, prefix)?;
                        stack.push(value);
                        continue;
                    };
                    let previous = {
                        let borrowed = scope.borrow();
                        match borrowed.bindings.binding_at(slot) {
                            Some(binding) => {
                                debug_assert_eq!(borrowed.bindings.name_at(slot), Some(&**name), "local slot");
                                match &binding.value {
                                    Value::Number(number) => Some(*number),
                                    _ => None,
                                }
                            }
                            None => None,
                        }
                    };
                    let value = match previous {
                        Some(previous) => {
                            let updated = if decrement { previous - 1.0 } else { previous + 1.0 };
                            if !updated.is_finite() {
                                return Err(runtime(&chunk.base_at(chunk.position(pc - 1)), "non-finite numeric result".to_string()));
                            }
                            let stored = scope.borrow_mut().bindings.set_at(slot, Value::Number(updated));
                            stored.map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                            Value::Number(if prefix { updated } else { previous })
                        }
                        None => self.update_name(&chunk.base_at(chunk.position(pc - 1)), &env, name, decrement, prefix)?,
                    };
                    stack.push(value);
                }
                Instr::ApplyFormat(index) => {
                    let value = stack.pop().expect("stack");
                    let text = self.apply_format(&chunk.base_at(chunk.position(pc - 1)), &chunk.names[index as usize], &value)?;
                    stack.push(Value::string(&text));
                }
                Instr::Format => {
                    let value = stack.pop().expect("stack");
                    stack.push(Value::string(&object::format(&value)));
                }
                Instr::PushScope => {
                    let child = self.acquire_scope(env.clone());
                    scopes.push(std::mem::replace(&mut env, child));
                }
                Instr::PopScope => {
                    let child = std::mem::replace(&mut env, scopes.pop().expect("scope"));
                    self.release_scope(child);
                }
                Instr::IterInit => {
                    let iterable = stack.pop().expect("stack");
                    if iterable.is_iterator() {
                        iters.push(IterState::Lazy(iterable));
                    } else {
                        let values = self.iterable_values(&chunk.base_at(chunk.position(pc - 1)), &iterable)?;
                        iters.push(IterState::Values(values, 0));
                    }
                }
                Instr::IterDrop => {
                    iters.pop();
                }
                Instr::IterNext(target) => {
                    let step = match iters.last_mut().expect("iterator") {
                        IterState::Values(values, index) => {
                            if *index >= values.len() {
                                None
                            } else {
                                let value = values[*index].clone();
                                *index += 1;
                                Some(value)
                            }
                        }
                        IterState::Lazy(source) => {
                            let source = source.clone();
                            self.iterator_next(&chunk.base_at(chunk.position(pc - 1)), &source)?
                        }
                    };
                    match step {
                        Some(value) => stack.push(value),
                        None => {
                            iters.pop();
                            pc = target as usize;
                        }
                    }
                }
                Instr::Import { path, name } => {
                    let module = self.import_module(&chunk.base_at(chunk.position(pc - 1)), &chunk.names[path as usize], &env)?;
                    object::define(&env, &chunk.names[name as usize], Value::Module(module), true)
                        .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                }
                Instr::Throw => {
                    let value = stack.pop().expect("stack");
                    // Only this frame's own scopes are unwound; the ones below it
                    // belong to the frames parked underneath.
                    self.release_scopes_to(&mut env, &mut scopes, scope_mark);
                    return Err(Signal::Throw { value, origin: chunk.base_at(chunk.position(pc - 1)).location() });
                }
                Instr::Return => {
                    let value = stack.pop().unwrap_or(Value::Null);
                    let Some(frame) = frames.pop() else {
                        // Nothing is parked above this frame, so it is the outermost:
                        // the value is what the chunk evaluates to.
                        self.release_scopes_to(&mut env, &mut scopes, scope_mark);
                        let mine = std::mem::take(&mut locals);
                        self.release_locals(mine);
                        return Ok(value);
                    };
                    // Leaving a frame a call entered. Its scopes and iterators are
                    // unwound, its locals go back to the pool, and the caller's parked
                    // state is loaded back. The result takes the place the call left for
                    // it, which is one slot below this frame's operand base — where the
                    // callee itself was on the caller's stack.
                    //
                    // A frame that pushed no scope and opened no iterator — every call
                    // whose body has no block and no `for` — has nothing to unwind, and the
                    // two tests are the loads those calls would make.
                    if scopes.len() != scope_mark {
                        self.release_scopes_to(&mut env, &mut scopes, scope_mark);
                    }
                    if iters.len() != iter_mark {
                        iters.truncate(iter_mark);
                    }
                    let mine = std::mem::replace(&mut locals, frame.locals);
                    self.release_locals(mine);
                    self.depth -= 1;
                    stack.truncate(base - 1);
                    if !frame.discard {
                        stack.push(value);
                    }
                    chunk = frame.chunk;
                    pc = frame.pc;
                    env = frame.env;
                    // The callee's own call scope, when it took one, goes back to the
                    // pool now that nothing here still refers to it.
                    if let Some(scope) = frame.scope {
                        self.release_scope(scope);
                    }
                    base = frame.base;
                    scope_mark = frame.scope_mark;
                    iter_mark = frame.iter_mark;
                }
                Instr::ReturnEscape => {
                    // A `return` in a try body, a catch body, or a `finally`
                    // belongs to the enclosing function, so it leaves this chunk:
                    // the try the chunk belongs to runs its `finally` as the value
                    // unwinds, and the enclosing function turns it back into its own
                    // result.
                    let value = stack.pop().unwrap_or(Value::Null);
                    self.release_scopes_to(&mut env, &mut scopes, scope_mark);
                    return Err(Signal::Return(value));
                }
                Instr::Try(index) => {
                    let proto = chunk.tries[index as usize].clone();
                    let outcome = self.run_try(&chunk.base_at(chunk.position(pc - 1)), &proto, &env);
                    let value = match outcome {
                        Ok(_) => continue,
                        Err(Signal::Return(value)) => value,
                        Err(signal) => return Err(signal),
                    };
                    // A `return` in a try body belongs to this function. It arrived as
                    // a signal because the try's sub-chunks run in frames of their own,
                    // and `Instr::ReturnEscape` is what a `return` in one of them
                    // leaves. A frame a call entered turns it back into its own result
                    // the same way `Instr::Return` does — which is the code below,
                    // repeated rather than shared because this is the one instruction
                    // that can reach a frame exit other than `Return` itself, and
                    // routing the hot path through a call to share it would cost every
                    // call in the program.
                    let Some(frame) = frames.pop() else {
                        // Nothing parked above this frame, so the signal belongs to the
                        // function boundary above this chunk, as it always did.
                        return Err(Signal::Return(value));
                    };
                    // The same two guards a plain return uses, for the same reason.
                    if scopes.len() != scope_mark {
                        self.release_scopes_to(&mut env, &mut scopes, scope_mark);
                    }
                    if iters.len() != iter_mark {
                        iters.truncate(iter_mark);
                    }
                    let mine = std::mem::replace(&mut locals, frame.locals);
                    self.release_locals(mine);
                    self.depth -= 1;
                    stack.truncate(base - 1);
                    if !frame.discard {
                        stack.push(value);
                    }
                    chunk = frame.chunk;
                    pc = frame.pc;
                    env = frame.env;
                    // The callee's own call scope, when it took one, goes back to the
                    // pool now that nothing here still refers to it.
                    if let Some(scope) = frame.scope {
                        self.release_scope(scope);
                    }
                    base = frame.base;
                    scope_mark = frame.scope_mark;
                    iter_mark = frame.iter_mark;
                }
                Instr::Comprehension(index) => {
                    let proto = &chunk.comprehensions[index as usize];
                    let value = self.comprehension(&chunk.base_at(chunk.position(pc - 1)), &proto.body, &proto.clauses, &env)?;
                    stack.push(value);
                }
                Instr::StoreScratch(slot) => {
                    let value = stack.pop().expect("stack");
                    locals[slot as usize] = value;
                }
                // The two collection steps of a comprehension: the value the body
                // computed goes straight into the collection the loop is building.
                Instr::Collect(slot) => {
                    let value = stack.pop().expect("stack");
                    let Value::List(elements) = &locals[slot as usize] else {
                        unreachable!("a list comprehension's accumulator is a list")
                    };
                    elements.borrow_mut().push(value);
                }
                Instr::CollectPair(slot) => {
                    let value = stack.pop().expect("stack");
                    let key = stack.pop().expect("stack");
                    let Value::Dict(dict) = &locals[slot as usize] else {
                        unreachable!("a dictionary comprehension's accumulator is a dictionary")
                    };
                    dict.borrow_mut().set(key, value).map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                }
                Instr::Generator(index) => {
                    let proto = chunk.generators[index as usize].clone();
                    let value = self.generator(&proto, &env)?;
                    stack.push(value);
                }
                Instr::Slice(index) => {
                    let collection = stack.pop().expect("stack");
                    let proto = &chunk.slices[index as usize];
                    let value =
                        self.slice_value(&chunk.base_at(chunk.position(pc - 1)), collection, &proto.start, &proto.stop, &proto.step, &env)?;
                    stack.push(value);
                }
            }
        }
    }

    /// Resolves `receiver.name` for one `GetProperty` instruction, remembering the
    /// answer for when the same instruction next sees the same receiver.
    ///
    /// Decides `left <op> right` exactly as the interpreter's operator decides it,
    /// without building the `bool` value a branch would then have to test.
    ///
    /// The numeric operators are inlined; anything else goes through
    /// [`Evaluator::binary`], so a fused comparison and an unfused one agree on
    /// every operand pair, errors included.
    fn test(&mut self, base: &tiger_ast::Base, op: Op, name: &str, left: &Value, right: &Value) -> Result<bool, Signal> {
        match fast_binary(op, left, right) {
            Some(Ok(value)) => Ok(object::truthy(&value)),
            Some(Err(message)) => Err(runtime(base, message.to_string())),
            None => {
                let value = self.binary(base, name, left.clone(), right.clone())?;
                Ok(object::truthy(&value))
            }
        }
    }

    /// This is a monomorphic inline cache, and it is worth it because a property
    /// access is a name match plus — for every receiver but an instance field — a
    /// freshly built method object. A loop over one receiver, which is the common
    /// shape, pays neither after its first pass.
    ///
    /// `position` is where a member that does not resolve reports from, which is
    /// the member name's own place rather than the instruction's: a `CallMethod`
    /// resolves a member for a call whose own position is somewhere else, so the
    /// two are passed separately instead of being read from `cache`.
    fn cached_property(&mut self, chunk: &Rc<Chunk>, cache: usize, name: usize, receiver: &Value, position: (u32, u32), env: &Env) -> Vm {
        let name = &chunk.names[name];
        // Only a receiver whose members are stable and whose identity is
        // meaningful is worth a cache slot. A string, a tuple, or a scalar has no
        // member of its own that could be remembered — its methods are rebuilt
        // per access — so those take the interpreter's own path with no cache
        // lookup at all, which is what keeps a method-heavy program from paying
        // for a slot it can never fill.
        if !matches!(receiver, Value::Dict(_) | Value::Module(_) | Value::Instance(_) | Value::List(_) | Value::Set(_)) {
            return self.property(&chunk.base_at(position), receiver, name, env);
        }
        {
            let slots = chunk.property_cache.borrow();
            match slots.get(cache).and_then(|hit| hit.as_ref()) {
                Some(CachedProperty::Value { receiver: cached, value }) => {
                    if same_receiver(cached, receiver) {
                        return Ok(value.clone());
                    }
                }
                Some(CachedProperty::Field { class, position: at }) => {
                    if let Value::Instance(instance) = receiver {
                        if Rc::ptr_eq(class, &instance.class) {
                            if let Some((_, value)) = instance.fields.borrow().get(*at as usize) {
                                return Ok(value.clone());
                            }
                        }
                    }
                }
                // A method a `CallMethod` remembered in this slot, which only that
                // instruction reads; a `GetProperty` on the same receiver is not it.
                Some(CachedProperty::Method { .. }) => {}
                None => {}
            }
        }
        // A public instance field is remembered by class and position, which turns
        // every access to it into a plain index: no class-chain walk, no visibility
        // check, and no name comparison, whatever instance arrives.
        if let Value::Instance(instance) = receiver {
            if let Some((at, value, remember)) = self.instance_field(&chunk.base_at(position), instance, name, env)? {
                if remember {
                    let class = instance.class.clone();
                    if let Some(slot) = chunk.property_cache.borrow_mut().get_mut(cache) {
                        *slot = Some(CachedProperty::Field { class, position: at });
                    }
                }
                return Ok(value);
            }
        }
        let value = self.property(&chunk.base_at(position), receiver, name, env)?;
        if reusable_property(receiver, &value) {
            if let Some(slot) = chunk.property_cache.borrow_mut().get_mut(cache) {
                *slot = Some(CachedProperty::Value { receiver: receiver.clone(), value: value.clone() });
            }
        }
        Ok(value)
    }

    /// Resolves the member `name` of `site`'s receiver and calls it, which is what one
    /// `CallMethod` instruction does.
    ///
    /// The fused form of a property read and the call that consumes it, and the one
    /// path that reaches a method without building the object a read returns: a class
    /// method is invoked with the receiver, a string's own method is dispatched by
    /// name, and a length spelling is answered outright. Everything else resolves the
    /// member exactly as `GetProperty` resolves it and calls the value it found, so a
    /// call and a read agree on every receiver, every result, and every diagnostic.
    ///
    /// The member is resolved here, after the arguments the caller has already
    /// evaluated, which is why the compiler emits this only where an argument cannot
    /// be observed out of order.
    ///
    /// Kept out of the instruction loop: it is a member lookup and a dispatch, and
    /// inlining it would put both in the loop's own frame for the sake of the one
    /// instruction that uses them.
    #[inline(never)]
    fn call_method(
        &mut self,
        site: CallSite<'_>,
        name: usize,
        member: u32,
        receiver: Value,
        stack: &mut [Value],
        env: &Env,
    ) -> Result<Value, Signal> {
        let call_position = site.chunk.position(site.pc);
        let member_position = site.member(member);
        let label: &str = &site.chunk.names[name];
        // A length spelling is answered here for every receiver that accepts one. The
        // built-in a read returns captures the receiver, so answering it directly is
        // what keeps `text.size()` from building one — and `length_of` accepts exactly
        // the receivers a read would return that built-in for.
        if label == "size" || label == "length" {
            if let Some(length) = builtins::length_of(&receiver) {
                // The arity the built-in would have reported, at the call.
                if site.keywords != 0 {
                    return Err(runtime(&site.chunk.base_at(call_position), format!("{} does not accept keyword arguments", label)));
                }
                if site.positional != 0 {
                    return Err(runtime(
                        &site.chunk.base_at(call_position),
                        format!("{} expects 0 arguments, got {}", label, site.positional),
                    ));
                }
                return Ok(Value::Number(length));
            }
        }
        // A class method: found in the class table and invoked with the receiver, so no
        // bound-method object is built.
        if let Value::Instance(instance) = &receiver {
            if let Some(method) = self.method_of(site.chunk, site.pc, label, member_position, instance, env)? {
                return self.call_bound_method(&site, &method, instance, stack);
            }
        }
        // A string's own method: dispatched by name, so no built-in is built. Every
        // other receiver's member read is remembered by the inline cache — a list's, a
        // dictionary's, an instance's — but a string's is deliberately never cached,
        // which makes it the one that would build a callable on every access.
        if site.keywords == 0 {
            if let Value::String(text) = &receiver {
                if let Some(result) = methods::call_string_method(label, text, &stack[site.arguments()], &[]) {
                    return result.map_err(|message| runtime(&site.chunk.base_at(call_position), message));
                }
            }
        }
        let callee = self.cached_property(site.chunk, site.pc, name, &receiver, member_position, env)?;
        self.finish_call(&site, callee, stack, env)
    }

    /// The class method `instance.name` resolves to, or `None` when the member is not
    /// one.
    ///
    /// A field of the same name shadows a method, so the field is looked for first —
    /// through the same helper a read uses, so a private field is refused in the same
    /// place — and a receiver that has one is left to the general path, which is where
    /// a call through a field goes. A method that is found is checked for visibility
    /// here, exactly as a read checks it.
    ///
    /// A public method is remembered against its class, so the next call — on this
    /// instance or any other of the same class — is a pointer comparison.
    fn method_of(
        &mut self,
        chunk: &Rc<Chunk>,
        cache: usize,
        name: &str,
        position: (u32, u32),
        instance: &Rc<object::Instance>,
        env: &Env,
    ) -> Result<Option<Rc<object::Method>>, Signal> {
        if let Some(CachedProperty::Method { class, method }) = chunk.property_cache.borrow().get(cache).and_then(|hit| hit.as_ref()) {
            if Rc::ptr_eq(class, &instance.class) {
                return Ok(Some(method.clone()));
            }
        }
        let base = chunk.base_at(position);
        if self.instance_field(&base, instance, name, env)?.is_some() {
            return Ok(None);
        }
        let Some(method) = instance.class.find_method(name) else { return Ok(None) };
        self.accessible(&base, &method.function.declaration.access, &method.owner, env)?;
        let access = &method.function.declaration.access;
        if access.is_empty() || access == "public" {
            let class = instance.class.clone();
            if let Some(slot) = chunk.property_cache.borrow_mut().get_mut(cache) {
                *slot = Some(CachedProperty::Method { class, method: method.clone() });
            }
        }
        Ok(Some(method))
    }

    /// Calls `callee` with `site`'s arguments, and returns its value.
    ///
    /// The tail of `Call` — keywords, a bound method, or any other callee — and the
    /// whole of `CallMethod`'s fallback, so a call of a value and a call of a member
    /// that resolved to a value reach a built-in, a user function, and a bound method
    /// through one path and cannot drift apart. The positional arguments are read out
    /// of the caller's operand stack rather than copied into a buffer, which is what
    /// keeps a call from allocating; the caller truncates the stack once it has the
    /// value.
    ///
    /// The two callees `Call` answers itself — a built-in called with positional
    /// arguments, and a user function — are answered here as well, because a member
    /// that resolved to one of them reaches this and has to take the same path.
    fn finish_call(&mut self, site: &CallSite<'_>, callee: Value, stack: &mut [Value], env: &Env) -> Result<Value, Signal> {
        if site.keywords == 0 {
            if let Value::Builtin(builtin) = &callee {
                if !builtin.higher_order && !builtin.consumes_iterables {
                    return (builtin.call)(self, &stack[site.arguments()], &[]).map_err(|message| runtime(&site.base(), message));
                }
            }
        }
        let base = site.base();
        let named = self.take_keywords(site, stack);
        let arguments = &mut stack[site.arguments()];
        let result = if site.keywords == 0 {
            // A user function is the next most common callee after a built-in, and it
            // is the one the general path's type dispatch and argument plumbing exists
            // for the sake of everything else. Calling it here skips that match and the
            // slice it would rebuild — and, for a function whose parameters are all flat
            // slots and whose body needs no scope, the three activations between this
            // instruction and the body as well.
            if let Value::Function(function) = &callee {
                match self.call_flat_function(&base, function, arguments)? {
                    Some(value) => Ok(value),
                    None => self.invoke(&base, function, arguments, &[], None),
                }
            } else if let Value::BoundMethod(bound) = &callee {
                self.invoke(&base, &bound.method.function, arguments, &named, Some((&bound.method, &bound.receiver)))
            } else {
                self.call(&base, callee, arguments, &named, env)
            }
        } else if let Value::BoundMethod(bound) = &callee {
            self.invoke(&base, &bound.method.function, arguments, &named, Some((&bound.method, &bound.receiver)))
        } else {
            self.call(&base, callee, arguments, &named, env)
        };
        if site.keywords > 0 {
            self.recycle_keywords(named);
        }
        result
    }

    /// Calls a class method with the receiver it was found on.
    ///
    /// The receiver and the method travel together, exactly as a bound method carries
    /// them, so this is the call `Call` makes for a `Value::BoundMethod` without the
    /// object that would have carried the pair.
    fn call_bound_method(
        &mut self,
        site: &CallSite<'_>,
        method: &Rc<object::Method>,
        instance: &Rc<object::Instance>,
        stack: &mut [Value],
    ) -> Result<Value, Signal> {
        let named = self.take_keywords(site, stack);
        let result = self.invoke(&site.base(), &method.function, &mut stack[site.arguments()], &named, Some((method, instance)));
        if site.keywords > 0 {
            self.recycle_keywords(named);
        }
        result
    }

    /// Materializes a call's keyword arguments, taking a pooled buffer.
    ///
    /// A call that passes no keywords — which is nearly every call — allocates nothing
    /// and touches no pool.
    fn take_keywords(&mut self, site: &CallSite<'_>, stack: &[Value]) -> Vec<Keyword> {
        if site.keywords == 0 {
            return Vec::new();
        }
        let mut named = self.keywords_pool.pop().unwrap_or_default();
        for pair in stack[site.keyword_pairs()].chunks(2) {
            let Value::String(name) = &pair[0] else { unreachable!("keyword name") };
            named.push(Keyword { name: name.to_string(), value: pair[1].clone() });
        }
        named
    }

    /// Returns a keyword buffer to the pool, cleared.
    fn recycle_keywords(&mut self, mut named: Vec<Keyword>) {
        named.clear();
        self.keywords_pool.push(named);
    }

    /// Stores `receiver.name = value` for one `SetProperty` instruction.
    ///
    /// A store into a public, non-`const` field is remembered by class and position,
    /// so every store into an instance of that class — the freshly built one a
    /// constructor writes included — is a plain index with no class-chain walk and
    /// no visibility check.
    fn store_property(
        &mut self,
        chunk: &Rc<Chunk>,
        pc: usize,
        name: usize,
        receiver: Value,
        value: Value,
        env: &Env,
    ) -> Result<(), Signal> {
        let name = &chunk.names[name];
        {
            let cache = chunk.property_cache.borrow();
            if let Some(CachedProperty::Field { class, position }) = cache.get(pc).and_then(|hit| hit.as_ref()) {
                if let Value::Instance(instance) = &receiver {
                    if Rc::ptr_eq(class, &instance.class) {
                        if let Some(slot) = instance.fields.borrow_mut().get_mut(*position as usize) {
                            slot.1 = value;
                            return Ok(());
                        }
                    }
                }
            }
        }
        let Value::Instance(instance) = &receiver else {
            let base = chunk.base_at(chunk.position(pc));
            return Err(runtime(&base, format!("cannot assign a property of {}", receiver.type_name())));
        };
        let position = self.store_instance_field(&chunk.base_at(chunk.position(pc)), instance, name, value, env)?;
        if let Some(position) = position {
            let class = instance.class.clone();
            if let Some(slot) = chunk.property_cache.borrow_mut().get_mut(pc) {
                *slot = Some(CachedProperty::Field { class, position });
            }
        }
        Ok(())
    }

    /// Runs one compiled `try`: its body, then the first catch clause that takes
    /// the error, then its `finally`. This mirrors the interpreter's
    /// `try_statement` and `try_handlers` step for step.
    fn run_try(&mut self, base: &tiger_ast::Base, proto: &Rc<TryProto>, env: &Env) -> Vm {
        let outcome = self.run_try_clauses(base, proto, env);
        // A resource-limit failure bypasses `finally` entirely, as it does in the
        // interpreter.
        if let Err(Signal::Runtime { fatal: true, .. }) = &outcome {
            return outcome;
        }
        let Some(finally) = &proto.finally else { return outcome };
        let scope = self.acquire_scope(env.clone());
        let result = self.run_chunk(finally, &scope);
        self.release_scope(scope);
        // A `finally` that completed leaves whatever was in flight standing; an
        // error or a `return` it raised replaces it.
        match result {
            Ok(_) => outcome,
            Err(failure) => Err(failure),
        }
    }

    /// Runs a try's body and then its catch clauses in order. The first clause
    /// that takes the error wins, and an unmatched error propagates unchanged.
    fn run_try_clauses(&mut self, base: &tiger_ast::Base, proto: &Rc<TryProto>, env: &Env) -> Vm {
        let scope = self.acquire_scope(env.clone());
        let outcome = self.run_chunk(&proto.body, &scope);
        self.release_scope(scope);
        let failure = match outcome {
            // The body completed, or left with a `return` or a `break`: no clause
            // takes those, and the outcome stands for the `finally` to see.
            Ok(_) => return Ok(Value::Null),
            Err(signal @ (Signal::Return(_) | Signal::Break(_) | Signal::Continue(_))) => return Err(signal),
            Err(signal @ Signal::Runtime { fatal: true, .. }) => return Err(signal),
            Err(signal) => signal,
        };
        let caught = match &failure {
            Signal::Runtime { error, .. } => Value::Error(error.clone()),
            Signal::Throw { value, .. } => value.clone(),
            _ => return Err(failure),
        };
        for clause in &proto.clauses {
            // The clause binds the caught value before its condition is tested, so
            // the condition can name it, and its body runs in the same scope.
            let scope = self.acquire_scope(env.clone());
            let outcome = match object::define(&scope, &clause.name, caught.clone(), false).map_err(|message| runtime(base, message)) {
                Err(signal) => Err(signal),
                Ok(()) => match &clause.condition {
                    None => self.run_chunk(&clause.body, &scope),
                    Some(condition) => match self.expression(condition, &scope) {
                        Ok(value) if object::truthy(&value) => self.run_chunk(&clause.body, &scope),
                        Ok(_) => {
                            self.release_scope(scope);
                            continue;
                        }
                        Err(signal) => Err(signal),
                    },
                },
            };
            self.release_scope(scope);
            return outcome;
        }
        Err(failure)
    }

    /// Unwinds the scopes a frame pushed, returning each to the pool, and leaves
    /// `env` standing at the frame's own root.
    ///
    /// `mark` is how tall `scopes` stood when the frame was entered, so everything
    /// above it was pushed by this frame and nothing below it is touched. That is what
    /// lets a call's scopes be unwound by its return without disturbing the frame it
    /// returns to: the frame's root is the environment at `mark`, and it is never the
    /// child being released.
    fn release_scopes_to(&mut self, env: &mut Env, scopes: &mut Vec<Env>, mark: usize) {
        while scopes.len() > mark {
            let parent = scopes.pop().expect("scopes above the frame's mark");
            let child = std::mem::replace(env, parent);
            self.release_scope(child);
        }
    }
}
