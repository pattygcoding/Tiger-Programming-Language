//! Runtime values, lexical environments, classes, and value formatting.
//!
//! Mirrors `pkg/object`: mutable aggregates are shared through `Rc<RefCell<..>>`
//! so aliasing behaves like Go pointers, `Dict` preserves insertion order with a
//! hash index, and `Format`/`Equal`/`Truthy` reproduce the Go semantics exactly.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use tiger_ast::{FieldDecl, FunctionDecl};

/// One evaluated keyword argument, in call order.
#[derive(Clone)]
pub struct Keyword {
    pub name: String,
    pub value: Value,
}

/// A runtime value.
#[derive(Clone)]
pub enum Value {
    Number(f64),
    String(Rc<str>),
    Bool(bool),
    Null,
    List(Rc<RefCell<Vec<Value>>>),
    Dict(Rc<RefCell<Dict>>),
    Function(Rc<FunctionValue>),
    Builtin(Rc<Builtin>),
    Class(Rc<Class>),
    Instance(Rc<Instance>),
    BoundMethod(Rc<BoundMethod>),
    Super(Rc<Super>),
    Module(Rc<Module>),
    File(Rc<RefCell<FileData>>),
    Error(Rc<RuntimeError>),
}

impl Value {
    /// The type name reported by `type(value)` and used in diagnostics.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::Null => "null",
            Value::List(_) => "list",
            Value::Dict(_) => "dict",
            Value::Function(_) => "function",
            Value::Builtin(_) => "builtin",
            Value::Class(_) => "class",
            Value::Instance(_) => "instance",
            Value::BoundMethod(_) => "bound method",
            Value::Super(_) => "super",
            Value::Module(_) => "module",
            Value::File(_) => "file",
            Value::Error(_) => "error",
        }
    }

    pub fn string(text: &str) -> Value {
        Value::String(Rc::from(text))
    }

    pub fn list(elements: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(elements)))
    }
}

/// A dictionary key: the value's type name plus its canonical text.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub kind: &'static str,
    pub text: String,
}

/// Builds the hash key for a value, rejecting non-scalar and non-finite keys.
pub fn hash(value: &Value) -> Result<Key, String> {
    match value {
        Value::Number(number) => {
            if number.is_nan() || number.is_infinite() {
                return Err("dictionary keys must be finite".to_string());
            }
            let normalized = if *number == 0.0 { 0.0 } else { *number };
            Ok(Key { kind: "number", text: tiger_format::number(normalized) })
        }
        Value::String(text) => Ok(Key { kind: "string", text: text.to_string() }),
        Value::Bool(flag) => Ok(Key { kind: "bool", text: flag.to_string() }),
        other => Err(format!("{} cannot be a dictionary key", other.type_name())),
    }
}

/// An insertion-ordered dictionary with a hash index for key lookup.
#[derive(Default)]
pub struct Dict {
    pub entries: Vec<(Value, Value)>,
    pub indices: HashMap<Key, usize>,
}

impl Dict {
    pub fn set(&mut self, key: Value, value: Value) -> Result<(), String> {
        let hashed = hash(&key)?;
        if let Some(index) = self.indices.get(&hashed).copied() {
            self.entries[index].1 = value;
            return Ok(());
        }
        self.indices.insert(hashed, self.entries.len());
        self.entries.push((key, value));
        Ok(())
    }

    pub fn get(&self, key: &Value) -> Result<Option<Value>, String> {
        let hashed = hash(key)?;
        match self.indices.get(&hashed) {
            Some(index) => Ok(Some(self.entries[*index].1.clone())),
            None => Ok(None),
        }
    }

    /// Removes the entry for `key`, preserving the order of the rest.
    pub fn delete(&mut self, key: &Value) -> Result<bool, String> {
        let hashed = hash(key)?;
        let Some(index) = self.indices.get(&hashed).copied() else {
            return Ok(false);
        };
        self.entries.remove(index);
        self.indices.remove(&hashed);
        for offset in index..self.entries.len() {
            if let Ok(moved) = hash(&self.entries[offset].0) {
                self.indices.insert(moved, offset);
            }
        }
        Ok(true)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.indices.clear();
    }
}

/// One binding in a lexical scope.
#[derive(Clone)]
pub struct Binding {
    pub value: Value,
    pub constant: bool,
}

/// A lexical environment; `Rc<RefCell<..>>` so closures share the same scope.
pub struct Environment {
    pub parent: Option<Env>,
    pub bindings: HashMap<String, Binding>,
    pub access_class: Option<Rc<Class>>,
    pub source_path: String,
}

pub type Env = Rc<RefCell<Environment>>;

impl Environment {
    pub fn new(parent: Option<Env>) -> Env {
        Rc::new(RefCell::new(Environment { parent, bindings: HashMap::new(), access_class: None, source_path: String::new() }))
    }
}

/// The nearest `source_path` up the scope chain, used to resolve imports.
pub fn module_path(env: &Env) -> String {
    let mut current = Some(env.clone());
    while let Some(scope) = current {
        let next;
        {
            let borrowed = scope.borrow();
            if !borrowed.source_path.is_empty() {
                return borrowed.source_path.clone();
            }
            next = borrowed.parent.clone();
        }
        current = next;
    }
    String::new()
}

pub fn get_own(env: &Env, name: &str) -> Option<Value> {
    env.borrow().bindings.get(name).map(|binding| binding.value.clone())
}

/// The class whose members the current method body may access.
pub fn class_context(env: &Env) -> Option<Rc<Class>> {
    let mut current = Some(env.clone());
    while let Some(scope) = current {
        let next;
        {
            let borrowed = scope.borrow();
            if let Some(class) = &borrowed.access_class {
                return Some(class.clone());
            }
            next = borrowed.parent.clone();
        }
        current = next;
    }
    None
}

pub fn define(env: &Env, name: &str, value: Value, constant: bool) -> Result<(), String> {
    let mut scope = env.borrow_mut();
    if scope.bindings.contains_key(name) {
        return Err(format!("{:?} is already declared in this scope", name));
    }
    scope.bindings.insert(name.to_string(), Binding { value, constant });
    Ok(())
}

fn resolve(env: &Env, name: &str) -> Option<Env> {
    let mut current = Some(env.clone());
    while let Some(scope) = current {
        let next;
        {
            let borrowed = scope.borrow();
            if borrowed.bindings.contains_key(name) {
                return Some(scope.clone());
            }
            next = borrowed.parent.clone();
        }
        current = next;
    }
    None
}

pub fn get(env: &Env, name: &str) -> Result<Value, String> {
    match resolve(env, name) {
        Some(owner) => Ok(owner.borrow().bindings.get(name).map(|binding| binding.value.clone()).unwrap()),
        None => Err(format!("undefined name {:?}", name)),
    }
}

pub fn assign(env: &Env, name: &str, value: Value) -> Result<(), String> {
    let Some(owner) = resolve(env, name) else {
        return Err(format!("undefined name {:?}; declare it with const or var before assignment", name));
    };
    let mut scope = owner.borrow_mut();
    let constant = scope.bindings.get(name).map(|binding| binding.constant).unwrap_or(false);
    if constant {
        return Err(format!("cannot reassign const {:?}", name));
    }
    scope.bindings.insert(name.to_string(), Binding { value, constant: false });
    Ok(())
}

/// A class: name, optional parent, optional constructor, methods, and fields.
pub struct Class {
    pub name: String,
    pub parent: Option<Rc<Class>>,
    pub constructor: RefCell<Option<Rc<Method>>>,
    pub methods: RefCell<Vec<(String, Rc<Method>)>>,
    pub fields: RefCell<Vec<Rc<Field>>>,
}

impl Class {
    pub fn new(name: String, parent: Option<Rc<Class>>) -> Rc<Class> {
        Rc::new(Class {
            name,
            parent,
            constructor: RefCell::new(None),
            methods: RefCell::new(Vec::new()),
            fields: RefCell::new(Vec::new()),
        })
    }

    pub fn find_method(self: &Rc<Class>, name: &str) -> Option<Rc<Method>> {
        let mut current = Some(self.clone());
        while let Some(class) = current {
            let found = class.methods.borrow().iter().find(|(method_name, _)| method_name == name).map(|(_, method)| method.clone());
            if found.is_some() {
                return found;
            }
            current = class.parent.clone();
        }
        None
    }

    /// The nearest declared constructor; subclasses without one inherit it.
    pub fn find_constructor(self: &Rc<Class>) -> Option<Rc<Method>> {
        let mut current = Some(self.clone());
        while let Some(class) = current {
            let found = class.constructor.borrow().clone();
            if found.is_some() {
                return found;
            }
            current = class.parent.clone();
        }
        None
    }

    pub fn find_field(self: &Rc<Class>, name: &str) -> Option<Rc<Field>> {
        let mut current = Some(self.clone());
        while let Some(class) = current {
            let found = class.fields.borrow().iter().find(|field| field.declaration.name == name).map(|field| field.clone());
            if found.is_some() {
                return found;
            }
            current = class.parent.clone();
        }
        None
    }
}

/// A function value: a declaration plus the environment it captured.
pub struct FunctionValue {
    pub declaration: Rc<FunctionDecl>,
    pub env: Env,
}

/// A method bound to its owning class.
pub struct Method {
    pub function: Rc<FunctionValue>,
    pub owner: Rc<Class>,
    pub constructor: bool,
}

/// A class field declaration bound to its owning class.
pub struct Field {
    pub declaration: FieldDecl,
    pub owner: Rc<Class>,
    pub env: Env,
}

/// An instance with its own field storage.
pub struct Instance {
    pub class: Rc<Class>,
    pub fields: RefCell<HashMap<String, Value>>,
}

/// A method bound to a receiver.
pub struct BoundMethod {
    pub method: Rc<Method>,
    pub receiver: Rc<Instance>,
}

/// The `super` view: the parent class plus the receiver.
pub struct Super {
    pub parent: Option<Rc<Class>>,
    pub receiver: Rc<Instance>,
    pub constructor: bool,
}

/// An imported module: a name plus its own environment.
pub struct Module {
    pub name: String,
    pub env: Env,
}

/// A structured runtime failure, classified by kind and positioned by origin.
pub struct RuntimeError {
    pub message: String,
    pub kind: String,
    pub origin: String,
}

impl RuntimeError {
    /// Renders the positioned diagnostic, e.g. `1:5: division by zero`.
    pub fn text(&self) -> String {
        if self.origin.is_empty() {
            self.message.clone()
        } else {
            format!("{}: {}", self.origin, self.message)
        }
    }
}

/// Host services a built-in may need: output, input, and a byte-level filesystem.
pub trait Host {
    fn output(&mut self, text: &str) -> Result<(), String>;
    fn input_line(&mut self, prompt: Option<&str>) -> Result<String, String>;
    fn file_exists(&mut self, name: &str) -> Result<bool, String>;
    fn load_file(&mut self, name: &str) -> Result<Vec<u8>, String>;
    fn persist_file(&mut self, name: &str, data: &[u8]) -> Result<(), String>;
    fn remove_file(&mut self, name: &str) -> Result<(), String>;
    fn list_dir(&mut self, name: &str) -> Result<Vec<String>, String>;
}

pub type BuiltinFn = dyn Fn(&mut dyn Host, &[Value], &[Keyword]) -> Result<Value, String>;

/// A built-in function. `higher_order` marks a callable the evaluator dispatches
/// itself so it can call back into user code (map, filter, sorted, list.sort);
/// `payload` carries the receiver for bound higher-order methods.
pub struct Builtin {
    pub name: String,
    pub higher_order: bool,
    pub payload: Option<Value>,
    pub call: Rc<BuiltinFn>,
}

impl Builtin {
    pub fn new<F>(name: &str, call: F) -> Rc<Builtin>
    where
        F: Fn(&mut dyn Host, &[Value], &[Keyword]) -> Result<Value, String> + 'static,
    {
        Rc::new(Builtin { name: name.to_string(), higher_order: false, payload: None, call: Rc::new(call) })
    }

    /// Builds a placeholder for a callable the evaluator dispatches itself.
    pub fn higher_order(name: &str, payload: Option<Value>) -> Rc<Builtin> {
        let label = name.to_string();
        Rc::new(Builtin {
            name: label.clone(),
            higher_order: true,
            payload,
            call: Rc::new(move |_host, _arguments, _keywords| Err(format!("{} is dispatched by the evaluator", label))),
        })
    }
}

/// One open file object's byte stream.
pub struct FileData {
    pub name: String,
    pub mode: String,
    pub binary: bool,
    pub data: Vec<u8>,
    pub offset: usize,
    pub append: bool,
    pub read: bool,
    pub write: bool,
    pub open: bool,
}

/// Formats a value for `print`, `str()`, and diagnostics.
pub fn format(value: &Value) -> String {
    format_value(value, &mut Vec::new(), false)
}

fn format_value(value: &Value, seen: &mut Vec<usize>, nested: bool) -> String {
    match value {
        Value::Number(number) => tiger_format::number(*number),
        Value::String(text) => {
            if nested {
                tiger_format::quote(text)
            } else {
                text.to_string()
            }
        }
        Value::Bool(flag) => flag.to_string(),
        Value::Null => "null".to_string(),
        Value::List(elements) => {
            let pointer = Rc::as_ptr(elements) as usize;
            if seen.contains(&pointer) {
                return "[...]".to_string();
            }
            seen.push(pointer);
            let parts: Vec<String> = {
                let borrowed = elements.borrow();
                borrowed.iter().map(|element| format_value(element, seen, true)).collect()
            };
            seen.pop();
            format!("[{}]", parts.join(", "))
        }
        Value::Dict(dict) => {
            let pointer = Rc::as_ptr(dict) as usize;
            if seen.contains(&pointer) {
                return "{...}".to_string();
            }
            seen.push(pointer);
            let parts: Vec<String> = {
                let borrowed = dict.borrow();
                borrowed
                    .entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", format_value(key, seen, true), format_value(value, seen, true)))
                    .collect()
            };
            seen.pop();
            format!("{{{}}}", parts.join(", "))
        }
        Value::Function(function) => {
            let name: &str = if function.declaration.name.is_empty() { "anonymous" } else { &function.declaration.name };
            format!("<function {}>", name)
        }
        Value::Module(module) => format!("<module {}>", module.name),
        Value::File(file) => format!("<file {}>", tiger_format::quote(&file.borrow().name)),
        Value::Class(class) => format!("<class {}>", class.name),
        Value::Instance(instance) => format!("<{} instance>", instance.class.name),
        Value::BoundMethod(bound) => {
            format!("<bound method {}.{}>", bound.method.owner.name, bound.method.function.declaration.name)
        }
        Value::Super(_) => "<super>".to_string(),
        Value::Builtin(builtin) => format!("<builtin {}>", builtin.name),
        Value::Error(error) => error.text(),
    }
}

/// Reports whether a value is truthy, matching the language's rules.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => *number != 0.0,
        Value::String(text) => !text.is_empty(),
        Value::List(elements) => !elements.borrow().is_empty(),
        Value::Dict(dict) => !dict.borrow().entries.is_empty(),
        _ => true,
    }
}

/// Structural equality with cycle detection, mirroring `object.Equal`.
pub fn equal(left: &Value, right: &Value) -> bool {
    equal_inner(left, right, &mut Vec::new())
}

fn equal_inner(left: &Value, right: &Value, seen: &mut Vec<(usize, usize)>) -> bool {
    if std::mem::discriminant(left) != std::mem::discriminant(right) {
        return false;
    }
    match (left, right) {
        (Value::Number(first), Value::Number(second)) => first == second,
        (Value::String(first), Value::String(second)) => first == second,
        (Value::Bool(first), Value::Bool(second)) => first == second,
        (Value::Null, Value::Null) => true,
        (Value::BoundMethod(first), Value::BoundMethod(second)) => {
            Rc::ptr_eq(&first.method, &second.method) && Rc::ptr_eq(&first.receiver, &second.receiver)
        }
        (Value::List(first), Value::List(second)) => {
            let pair = (Rc::as_ptr(first) as usize, Rc::as_ptr(second) as usize);
            if seen.contains(&pair) {
                return true;
            }
            seen.push(pair);
            let first = first.borrow();
            let second = second.borrow();
            if first.len() != second.len() {
                return false;
            }
            first.iter().zip(second.iter()).all(|(a, b)| equal_inner(a, b, seen))
        }
        (Value::Dict(first), Value::Dict(second)) => {
            let pair = (Rc::as_ptr(first) as usize, Rc::as_ptr(second) as usize);
            if seen.contains(&pair) {
                return true;
            }
            seen.push(pair);
            let first = first.borrow();
            let second = second.borrow();
            if first.entries.len() != second.entries.len() {
                return false;
            }
            for (key, value) in &first.entries {
                match second.get(key) {
                    Ok(Some(other)) => {
                        if !equal_inner(value, &other, seen) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
            true
        }
        (Value::Function(first), Value::Function(second)) => Rc::ptr_eq(first, second),
        (Value::Builtin(first), Value::Builtin(second)) => Rc::ptr_eq(first, second),
        (Value::Class(first), Value::Class(second)) => Rc::ptr_eq(first, second),
        (Value::Instance(first), Value::Instance(second)) => Rc::ptr_eq(first, second),
        (Value::Super(first), Value::Super(second)) => Rc::ptr_eq(first, second),
        (Value::Module(first), Value::Module(second)) => Rc::ptr_eq(first, second),
        (Value::File(first), Value::File(second)) => Rc::ptr_eq(first, second),
        (Value::Error(first), Value::Error(second)) => Rc::ptr_eq(first, second),
        _ => false,
    }
}
