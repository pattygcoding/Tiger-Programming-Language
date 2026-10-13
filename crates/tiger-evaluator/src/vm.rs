//! The linear (bytecode) execution engine.
//!
//! Runs a [`Chunk`](tiger_object::bytecode::Chunk) in a flat loop instead of
//! recursively walking the AST. Every operation delegates to the same runtime
//! helpers the interpreter uses, so the two engines agree exactly.

use std::rc::Rc;

use tiger_object as object;
use tiger_object::bytecode::{Chunk, Instr, Op};
use tiger_object::{Env, FunctionValue, Keyword, Value};

use crate::{runtime, Evaluator, Signal};

/// A `for ... in` iteration in progress.
struct IterState {
    values: Vec<Value>,
    index: usize,
}

type Vm = Result<Value, Signal>;

impl Evaluator {
    /// Runs a compiled chunk to completion, returning its value.
    pub(crate) fn run_chunk(&mut self, chunk: &Rc<Chunk>, env: &Env) -> Vm {
        let mut stack = self.stack_pool.pop().unwrap_or_default();
        let result = self.run_loop(chunk, env, &mut stack);
        stack.clear();
        self.stack_pool.push(stack);
        result
    }

    /// The instruction loop; `stack` is pooled by the caller so a call does not
    /// allocate a value stack.
    fn run_loop(&mut self, chunk: &Rc<Chunk>, env: &Env, stack: &mut Vec<Value>) -> Vm {
        let root = env.clone();
        let mut env = env.clone();
        let mut scopes: Vec<Env> = Vec::new();
        let mut iters: Vec<IterState> = Vec::new();
        let mut pc = 0usize;
        loop {
            self.steps += 1;
            if self.max_steps > 0 && self.steps > self.max_steps {
                return Err(crate::fatal(&chunk.base(pc), "execution step limit exceeded".to_string()));
            }
            let instr = chunk.code[pc];
            let position = chunk.base(pc);
            pc += 1;
            match instr {
                Instr::Constant(index) => stack.push(chunk.constants[index as usize].clone()),
                Instr::Null => stack.push(Value::Null),
                Instr::LoadName(index) => {
                    let value = object::get(&env, &chunk.names[index as usize]).map_err(|message| runtime(&position, message))?;
                    stack.push(value);
                }
                Instr::LoadSuper => match object::get(&env, "super") {
                    Ok(value) => stack.push(value),
                    Err(_) => return Err(runtime(&position, "super is only available inside methods".to_string())),
                },
                Instr::StoreName(index) => {
                    let value = stack.pop().expect("stack");
                    object::assign(&env, &chunk.names[index as usize], value).map_err(|message| runtime(&position, message))?;
                }
                Instr::DefineName { name, constant } => {
                    let value = stack.pop().expect("stack");
                    object::define(&env, &chunk.names[name as usize], value, constant).map_err(|message| runtime(&position, message))?;
                }
                Instr::Pop => {
                    stack.pop();
                }
                Instr::Dup => {
                    let value = stack.last().expect("stack").clone();
                    stack.push(value);
                }
                Instr::Binary { op, name } => {
                    let right = stack.pop().expect("stack");
                    let left = stack.pop().expect("stack");
                    // Inline the numeric operators the compiler tagged, so an
                    // addition or comparison does not call out to the interpreter.
                    if let (Value::Number(a), Value::Number(b)) = (&left, &right) {
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
                            Op::Lt => {
                                stack.push(Value::Bool(a < b));
                                continue;
                            }
                            Op::Le => {
                                stack.push(Value::Bool(a <= b));
                                continue;
                            }
                            Op::Gt => {
                                stack.push(Value::Bool(a > b));
                                continue;
                            }
                            Op::Ge => {
                                stack.push(Value::Bool(a >= b));
                                continue;
                            }
                            Op::Eq => {
                                stack.push(Value::Bool(a == b));
                                continue;
                            }
                            Op::Ne => {
                                stack.push(Value::Bool(a != b));
                                continue;
                            }
                            _ => None,
                        };
                        if let Some(value) = computed {
                            if value.is_infinite() || value.is_nan() {
                                return Err(runtime(&position, "non-finite numeric result".to_string()));
                            }
                            stack.push(Value::Number(value));
                            continue;
                        }
                    }
                    let value = self.binary(&position, &chunk.names[name as usize], left, right)?;
                    stack.push(value);
                }
                Instr::Unary(index) => {
                    let value = stack.pop().expect("stack");
                    let value = self.unary(&position, &chunk.names[index as usize], value)?;
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
                        dict.set(pair[0].clone(), pair[1].clone()).map_err(|message| runtime(&position, message))?;
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
                    let callee = stack[at - 1].clone();
                    let positional_arguments: Vec<Value> = stack[at..at + positional as usize].to_vec();
                    let mut named = Vec::with_capacity(keyword_count);
                    for pair in stack[at + positional as usize..].chunks(2) {
                        let Value::String(name) = &pair[0] else { unreachable!("keyword name") };
                        named.push(Keyword { name: name.to_string(), value: pair[1].clone() });
                    }
                    stack.truncate(at - 1);
                    let value = self.call(&position, callee, &positional_arguments, &named, &env)?;
                    stack.push(value);
                }
                Instr::GetIndex => {
                    let key = stack.pop().expect("stack");
                    let collection = stack.pop().expect("stack");
                    let value = self.index_value(&position, collection, key)?;
                    stack.push(value);
                }
                Instr::GetProperty(index) => {
                    let receiver = stack.pop().expect("stack");
                    let value = self.property(&position, &receiver, &chunk.names[index as usize], &env)?;
                    stack.push(value);
                }
                Instr::SetIndex => {
                    let key = stack.pop().expect("stack");
                    let collection = stack.pop().expect("stack");
                    let value = stack.pop().expect("stack");
                    self.set_index(&position, collection, key, value)?;
                }
                Instr::SetProperty(index) => {
                    let receiver = stack.pop().expect("stack");
                    let value = stack.pop().expect("stack");
                    self.set_property(&position, receiver, &chunk.names[index as usize], value, &env)?;
                }
                Instr::UpdateName { name, decrement, prefix } => {
                    let value = self.update_name(&position, &env, &chunk.names[name as usize], decrement, prefix)?;
                    stack.push(value);
                }
                Instr::ApplyFormat(index) => {
                    let value = stack.pop().expect("stack");
                    let text = self.apply_format(&position, &chunk.names[index as usize], &value)?;
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
                    let values = self.iterable_values(&position, &iterable)?;
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
                    let module = self.import_module(&position, &chunk.names[path as usize], &env)?;
                    object::define(&env, &chunk.names[name as usize], Value::Module(module), true)
                        .map_err(|message| runtime(&position, message))?;
                }
                Instr::Throw => {
                    let value = stack.pop().expect("stack");
                    self.release_scopes(&root, &mut env, &mut scopes);
                    return Err(Signal::Throw { value, origin: position.location() });
                }
                Instr::Return => {
                    let value = stack.pop().unwrap_or(Value::Null);
                    self.release_scopes(&root, &mut env, &mut scopes);
                    return Ok(value);
                }
            }
        }
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
