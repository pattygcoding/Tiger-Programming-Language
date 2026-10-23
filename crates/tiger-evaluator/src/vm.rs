//! The linear (bytecode) execution engine.
//!
//! Runs a [`Chunk`](tiger_object::bytecode::Chunk) in a flat loop instead of
//! recursively walking the AST. Every operation delegates to the same runtime
//! helpers the interpreter uses, so the two engines agree exactly.

use std::rc::Rc;

use tiger_object as object;
use tiger_object::bytecode::{CachedProperty, Chunk, Instr, Op, TryProto};
use tiger_object::{Env, FunctionValue, Keyword, Value};

use crate::{runtime, Evaluator, Signal};

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
        Op::Mod if b != 0.0 => {
            let mut remainder = a % b;
            if remainder != 0.0 && (remainder < 0.0) != (b < 0.0) {
                remainder += b;
            }
            Some(remainder)
        }
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
        self.run_chunk_with(chunk, env, locals)
    }

    /// Takes a frame's flat locals from the pool, sized and cleared, so a call that
    /// declares locals allocates nothing once the pool is warm.
    pub(crate) fn take_locals(&mut self, count: usize) -> Vec<Value> {
        let mut locals = self.locals_pool.pop().unwrap_or_default();
        locals.clear();
        locals.resize(count, Value::Null);
        locals
    }

    /// Runs a chunk with the frame's flat locals already prepared, which is how a
    /// call binds its flat parameters before the body starts.
    pub(crate) fn run_chunk_with(&mut self, chunk: &Rc<Chunk>, env: &Env, mut locals: Vec<Value>) -> Vm {
        let result = self.run_frame(chunk, env, &mut locals);
        locals.clear();
        self.locals_pool.push(locals);
        result
    }

    /// Runs a chunk in a frame whose flat locals belong to the caller, so a frame
    /// small enough for the caller's own stack never touches the locals pool.
    ///
    /// This is the one call on the hot path that crosses a module boundary, so it
    /// asks to be inlined; the loop it runs cannot be, being re-entered by calls.
    #[inline]
    pub(crate) fn run_frame(&mut self, chunk: &Rc<Chunk>, env: &Env, locals: &mut [Value]) -> Vm {
        let mut stack = self.stack_pool.pop().unwrap_or_default();
        let result = self.run_loop(chunk, env, &mut stack, locals);
        stack.clear();
        self.stack_pool.push(stack);
        result
    }

    /// The instruction loop; `stack` is pooled by the caller so a call does not
    /// allocate a value stack.
    fn run_loop(&mut self, chunk: &Rc<Chunk>, env: &Env, stack: &mut Vec<Value>, locals: &mut [Value]) -> Vm {
        let root = env;
        let mut env = env.clone();
        let mut scopes: Vec<Env> = Vec::new();
        let mut iters: Vec<IterState> = Vec::new();
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
                    object::define(&env, &chunk.names[name as usize], value, constant)
                        .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                }
                Instr::DefineLocal { name, slot, constant } => {
                    let value = stack.pop().expect("stack");
                    object::define_at(&env, &chunk.names[name as usize], slot as usize, value, constant)
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
                    let value = self.cached_property(chunk, pc - 1, index as usize, &receiver, &env)?;
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
                    let value = self.cached_property(chunk, cache as usize, name as usize, &locals[value as usize], &env)?;
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
                                // A call whose value is dropped absorbed the `Pop` that
                                // would have discarded it, so it steps over that slot as
                                // well — after the position above is read, because the
                                // error a call raises is reported where the call is.
                                if discard {
                                    pc += 1;
                                } else {
                                    stack.push(value);
                                }
                                continue;
                            }
                        }
                    }
                    let mut named = if keyword_count == 0 { Vec::new() } else { self.keywords_pool.pop().unwrap_or_default() };
                    if keyword_count > 0 {
                        for pair in stack[at + positional as usize..].chunks(2) {
                            let Value::String(name) = &pair[0] else { unreachable!("keyword name") };
                            named.push(Keyword { name: name.to_string(), value: pair[1].clone() });
                        }
                    }
                    // A user function is the next most common callee after a built-in, and
                    // it is the one the general path's type dispatch and argument plumbing
                    // exists for the sake of everything else. Calling it here skips that
                    // match and the slice it would rebuild.
                    if keyword_count == 0 {
                        if let Value::Function(function) = &callee {
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
                    // A bound method is the other common callee, and its dispatch is one
                    // step: the receiver and the method already travel together. Calling
                    // it here skips the match and the slice it would rebuild.
                    if let Value::BoundMethod(bound) = &callee {
                        let value = self.invoke(
                            &chunk.base_at(chunk.position(pc - 1)),
                            &bound.method.function,
                            &mut stack[at..at + positional as usize],
                            &named,
                            Some((&bound.method, &bound.receiver)),
                        )?;
                        stack.truncate(at - 1);
                        if keyword_count > 0 {
                            named.clear();
                            self.keywords_pool.push(named);
                        }
                        if !discard {
                            stack.push(value);
                        } else {
                            pc += 1;
                        }
                        continue;
                    }
                    let value =
                        self.call(&chunk.base_at(chunk.position(pc - 1)), callee, &mut stack[at..at + positional as usize], &named, &env)?;
                    stack.truncate(at - 1);
                    if keyword_count > 0 {
                        named.clear();
                        self.keywords_pool.push(named);
                    }
                    if !discard {
                        stack.push(value);
                    } else {
                        pc += 1;
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
                    let value = self.cached_property(chunk, pc - 1, index as usize, &receiver, &env)?;
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
                    self.store_property(chunk, pc - 1, index as usize, receiver, value, &env)?;
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
                    self.store_property(chunk, pc - 1, index as usize, receiver, value, &env)?;
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
                    self.store_property(chunk, pc - 1, property as usize, receiver, Value::Number(updated), &env)?;
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
                    self.release_scopes(root, &mut env, &mut scopes);
                    return Err(Signal::Throw { value, origin: chunk.base_at(chunk.position(pc - 1)).location() });
                }
                Instr::Return => {
                    let value = stack.pop().unwrap_or(Value::Null);
                    self.release_scopes(root, &mut env, &mut scopes);
                    return Ok(value);
                }
                Instr::ReturnEscape => {
                    // A `return` in a try body, a catch body, or a `finally`
                    // belongs to the enclosing function, so it leaves this chunk:
                    // the try the chunk belongs to runs its `finally` as the value
                    // unwinds, and the enclosing function turns it back into its own
                    // result.
                    let value = stack.pop().unwrap_or(Value::Null);
                    self.release_scopes(root, &mut env, &mut scopes);
                    return Err(Signal::Return(value));
                }
                Instr::Try(index) => {
                    let proto = chunk.tries[index as usize].clone();
                    self.run_try(&chunk.base_at(chunk.position(pc - 1)), &proto, &env)?;
                }
                Instr::Comprehension(index) => {
                    let proto = &chunk.comprehensions[index as usize];
                    let value = self.comprehension(&chunk.base_at(chunk.position(pc - 1)), &proto.body, &proto.clauses, &env)?;
                    stack.push(value);
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
    fn cached_property(&mut self, chunk: &Rc<Chunk>, pc: usize, name: usize, receiver: &Value, env: &Env) -> Vm {
        let name = &chunk.names[name];
        // Only a receiver whose members are stable and whose identity is
        // meaningful is worth a cache slot. A string, a tuple, or a scalar has no
        // member of its own that could be remembered — its methods are rebuilt
        // per access — so those take the interpreter's own path with no cache
        // lookup at all, which is what keeps a method-heavy program from paying
        // for a slot it can never fill.
        if !matches!(receiver, Value::Dict(_) | Value::Module(_) | Value::Instance(_) | Value::List(_) | Value::Set(_)) {
            return self.property(&chunk.base_at(chunk.position(pc)), receiver, name, env);
        }
        {
            let cache = chunk.property_cache.borrow();
            match cache.get(pc).and_then(|hit| hit.as_ref()) {
                Some(CachedProperty::Value { receiver: cached, value }) => {
                    if same_receiver(cached, receiver) {
                        return Ok(value.clone());
                    }
                }
                Some(CachedProperty::Field { class, position }) => {
                    if let Value::Instance(instance) = receiver {
                        if Rc::ptr_eq(class, &instance.class) {
                            if let Some((_, value)) = instance.fields.borrow().get(*position as usize) {
                                return Ok(value.clone());
                            }
                        }
                    }
                }
                None => {}
            }
        }
        // A public instance field is remembered by class and position, which turns
        // every access to it into a plain index: no class-chain walk, no visibility
        // check, and no name comparison, whatever instance arrives.
        if let Value::Instance(instance) = receiver {
            if let Some((position, value, remember)) = self.instance_field(&chunk.base_at(chunk.position(pc)), instance, name, env)? {
                if remember {
                    let class = instance.class.clone();
                    if let Some(slot) = chunk.property_cache.borrow_mut().get_mut(pc) {
                        *slot = Some(CachedProperty::Field { class, position });
                    }
                }
                return Ok(value);
            }
        }
        let value = self.property(&chunk.base_at(chunk.position(pc)), receiver, name, env)?;
        if reusable_property(receiver, &value) {
            if let Some(slot) = chunk.property_cache.borrow_mut().get_mut(pc) {
                *slot = Some(CachedProperty::Value { receiver: receiver.clone(), value: value.clone() });
            }
        }
        Ok(value)
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

    /// Unwinds pushed scopes, returning them to the pool, without touching the
    /// frame's root scope, which the caller owns.
    fn release_scopes(&mut self, root: &Env, env: &mut Env, scopes: &mut Vec<Env>) {
        while let Some(parent) = scopes.pop() {
            let child = std::mem::replace(env, parent);
            if !Rc::ptr_eq(&child, root) {
                self.release_scope(child);
            }
        }
    }
}
