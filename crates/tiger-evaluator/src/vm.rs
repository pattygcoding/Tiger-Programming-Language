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
struct IterState {
    values: Vec<Value>,
    index: usize,
}

type Vm = Result<Value, Signal>;

/// Whether two values are the same receiver. Clones of one `Rc` share their
/// address, so a cache hit means this instruction resolved this very object before.
fn same_receiver(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Dict(a), Value::Dict(b)) => Rc::ptr_eq(a, b),
        (Value::Module(a), Value::Module(b)) => Rc::ptr_eq(a, b),
        (Value::Instance(a), Value::Instance(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}

/// Whether a resolved property may be reused for the same receiver.
///
/// A module's bindings and a class's methods are fixed once defined, so those are
/// remembered. An instance *field* read is mutable state and is never remembered,
/// and a private method's visibility is checked against the caller — which can
/// differ between two accesses of one receiver — so neither is.
fn reusable_property(receiver: &Value, value: &Value) -> bool {
    match (receiver, value) {
        (Value::Dict(_), _) | (Value::Module(_), _) => true,
        (Value::Instance(_), Value::Builtin(_)) => true,
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
        Some(value) if value.is_infinite() || value.is_nan() => Err("non-finite numeric result"),
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
            self.steps += 1;
            if self.steps > limit {
                return Err(crate::fatal(&chunk.base(pc), "execution step limit exceeded".to_string()));
            }
            let instr = chunk.code[pc];
            pc += 1;
            match instr {
                Instr::Constant(index) => stack.push(chunk.constants[index as usize].clone()),
                Instr::Null => stack.push(Value::Null),
                Instr::LoadName(index) => {
                    let value = object::get(&env, &chunk.names[index as usize])
                        .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
                    stack.push(value);
                }
                // A flat slot belongs to this frame alone, so this is an index into
                // `locals` with no borrow and no scope to find first.
                Instr::LoadFlat(slot) => stack.push(locals[slot as usize].clone()),
                Instr::StoreFlat { name, slot, constant } => {
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
                    let value = object::get(&env, &chunk.names[name as usize])
                        .map_err(|message| runtime(&chunk.base_at(chunk.position(pc - 1)), message))?;
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
                Instr::Call { positional, keywords } => {
                    let keyword_count = keywords as usize;
                    let argument_count = positional as usize + 2 * keyword_count;
                    let at = stack.len() - argument_count;
                    // The positional arguments sit contiguously at the top of the value
                    // stack, so the call reads them straight out of it: no buffer and no
                    // copy. A call that passes keywords materializes those, which is the
                    // only call that allocates anything.
                    let callee = std::mem::replace(&mut stack[at - 1], Value::Null);
                    let mut named = if keyword_count == 0 { Vec::new() } else { self.keywords_pool.pop().unwrap_or_default() };
                    if keyword_count > 0 {
                        for pair in stack[at + positional as usize..].chunks(2) {
                            let Value::String(name) = &pair[0] else { unreachable!("keyword name") };
                            named.push(Keyword { name: name.to_string(), value: pair[1].clone() });
                        }
                    }
                    let value =
                        self.call(&chunk.base_at(chunk.position(pc - 1)), callee, &stack[at..at + positional as usize], &named, &env)?;
                    stack.truncate(at - 1);
                    if keyword_count > 0 {
                        named.clear();
                        self.keywords_pool.push(named);
                    }
                    stack.push(value);
                }
                Instr::GetIndex => {
                    let key = stack.pop().expect("stack");
                    let collection = stack.pop().expect("stack");
                    let value = self.index_value(&chunk.base_at(chunk.position(pc - 1)), collection, key)?;
                    stack.push(value);
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
                    self.set_index(&chunk.base_at(chunk.position(pc - 1)), collection, key, value)?;
                }
                Instr::SetProperty(index) => {
                    let receiver = stack.pop().expect("stack");
                    let value = stack.pop().expect("stack");
                    self.store_property(chunk, pc - 1, index as usize, receiver, value, &env)?;
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
                            if updated.is_infinite() || updated.is_nan() {
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
                    let values = self.iterable_values(&chunk.base_at(chunk.position(pc - 1)), &iterable)?;
                    iters.push(IterState { values, index: 0 });
                }
                Instr::IterDrop => {
                    iters.pop();
                }
                Instr::IterNext(target) => {
                    let state = iters.last_mut().expect("iterator");
                    if state.index >= state.values.len() {
                        iters.pop();
                        pc = target as usize;
                    } else {
                        let value = state.values[state.index].clone();
                        state.index += 1;
                        stack.push(value);
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
    /// This is a monomorphic inline cache, and it is worth it because a property
    /// access is a name match plus — for every receiver but an instance field — a
    /// freshly built method object. A loop over one receiver, which is the common
    /// shape, pays neither after its first pass.
    fn cached_property(&mut self, chunk: &Rc<Chunk>, pc: usize, name: usize, receiver: &Value, env: &Env) -> Vm {
        let name = &chunk.names[name];
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
