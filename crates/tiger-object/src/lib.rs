//! Runtime values, lexical environments, classes, and value formatting.
//!
//! Mirrors `pkg/object`: mutable aggregates are shared through `Rc<RefCell<..>>`
//! so aliasing behaves like Go pointers, `Dict` preserves insertion order with a
//! hash index, and `Format`/`Equal`/`Truthy` reproduce the Go semantics exactly.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use tiger_ast::{FieldDecl, FunctionDecl};

pub mod bytecode;

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

/// A dictionary key: the scalar value itself, so lookups never re-format.
///
/// A number key keeps its raw bits (with `-0.0` folded to `0.0`). That is
/// equivalent to the previous text form because `tiger_format::number` is a
/// bijection on finite floats, so equal keys stay equal and distinct keys stay
/// distinct. A string key is a shared `Rc<str>` clone instead of a fresh copy.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    Number(u64),
    String(Rc<str>),
    Bool(bool),
}

/// Builds the hash key for a value, rejecting non-scalar and non-finite keys.
pub fn hash(value: &Value) -> Result<Key, String> {
    match value {
        Value::Number(number) => {
            if number.is_nan() || number.is_infinite() {
                return Err("dictionary keys must be finite".to_string());
            }
            let normalized = if *number == 0.0 { 0.0 } else { *number };
            Ok(Key::Number(normalized.to_bits()))
        }
        Value::String(text) => Ok(Key::String(text.clone())),
        Value::Bool(flag) => Ok(Key::Bool(*flag)),
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

/// The bindings of one scope: an insertion-ordered list of shared names that
/// promotes to a hash index once it grows past a small threshold.
///
/// Scopes almost always hold a handful of names, so a linear scan over the
/// `Rc<str>` keys beats hashing every lookup and keeps declaration order. The
/// built-in and module scopes hold dozens, so they promote to an index to keep
/// large lookups cheap.
#[derive(Default)]
pub struct Bindings {
    entries: Vec<(Rc<str>, Binding)>,
    index: Option<HashMap<Rc<str>, usize>>,
}

/// The number of bindings at which a scope gains a hash index.
const BINDINGS_INDEX_THRESHOLD: usize = 8;

impl Bindings {
    #[inline]
    fn position(&self, name: &str) -> Option<usize> {
        match &self.index {
            Some(index) => index.get(name).copied(),
            None => self.entries.iter().position(|(key, _)| &**key == name),
        }
    }

    #[inline]
    pub fn contains(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    #[inline]
    pub fn get(&self, name: &str) -> Option<&Binding> {
        self.position(name).map(|position| &self.entries[position].1)
    }

    #[inline]
    fn get_mut(&mut self, name: &str) -> Option<&mut Binding> {
        match self.position(name) {
            Some(position) => Some(&mut self.entries[position].1),
            None => None,
        }
    }

    /// The number of bindings in the scope.
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the scope is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The binding at `slot`, which is simply its position in the scope.
    ///
    /// A scope's names are declared in source order, so the compiled engine can
    /// address a binding by the position the compiler gave it and skip the name
    /// lookup entirely.
    #[inline]
    pub fn binding_at(&self, slot: usize) -> Option<&Binding> {
        self.entries.get(slot).map(|(_, binding)| binding)
    }

    /// The name declared at `slot`, which a slot-based access checks against.
    #[inline]
    pub fn name_at(&self, slot: usize) -> Option<&str> {
        self.entries.get(slot).map(|(key, _)| &**key)
    }

    /// Reassigns the binding at `slot`, with the same constant check [`assign`]
    /// makes and without looking the name up.
    #[inline]
    pub fn set_at(&mut self, slot: usize, value: Value) -> Result<(), String> {
        let entry = &mut self.entries[slot];
        if entry.1.constant {
            return Err(format!("cannot reassign const {:?}", &*entry.0));
        }
        entry.1.value = value;
        Ok(())
    }

    fn insert(&mut self, name: &str, binding: Binding) {
        self.insert_shared(Rc::from(name), binding);
    }

    /// Inserts a binding whose name is already a shared `Rc<str>`, so a repeated
    /// call with the same parameter name allocates nothing for the key.
    fn insert_shared(&mut self, key: Rc<str>, binding: Binding) {
        let position = self.entries.len();
        self.entries.push((key.clone(), binding));
        match &mut self.index {
            Some(index) => {
                index.insert(key, position);
            }
            None => {
                if self.entries.len() > BINDINGS_INDEX_THRESHOLD {
                    let mut index = HashMap::with_capacity(self.entries.len());
                    for (position, (key, _)) in self.entries.iter().enumerate() {
                        index.insert(key.clone(), position);
                    }
                    self.index = Some(index);
                }
            }
        }
    }

    /// Empties the scope, keeping the backing storage so a reused scope does
    /// not allocate again.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index = None;
    }

    /// The names this scope defines, in declaration order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(key, _)| &**key)
    }
}

/// A lexical environment; `Rc<RefCell<..>>` so closures share the same scope.
pub struct Environment {
    pub parent: Option<Env>,
    pub bindings: Bindings,
    pub access_class: Option<Rc<Class>>,
    pub source_path: String,
}

pub type Env = Rc<RefCell<Environment>>;

impl Environment {
    #[inline]
    pub fn new(parent: Option<Env>) -> Env {
        Rc::new(RefCell::new(Environment { parent, bindings: Bindings::default(), access_class: None, source_path: String::new() }))
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
    if scope.bindings.contains(name) {
        return Err(format!("{:?} is already declared in this scope", name));
    }
    scope.bindings.insert(name, Binding { value, constant });
    Ok(())
}

/// Defines a binding whose name is already a shared `Rc<str>`, so binding a
/// parameter on every call does not allocate the key again.
pub fn define_shared(env: &Env, name: &Rc<str>, value: Value, constant: bool) -> Result<(), String> {
    let mut scope = env.borrow_mut();
    if scope.bindings.contains(name) {
        return Err(format!("{:?} is already declared in this scope", name));
    }
    scope.bindings.insert_shared(name.clone(), Binding { value, constant });
    Ok(())
}

/// Defines a binding whose slot the compiler already knows, so a slot-based store
/// can follow without looking the name up.
///
/// The slot must be the next free position in the scope: a scope's names are
/// declared in source order, so a declaration's slot is its index. A debug build
/// checks that, which turns any drift between the compiler's scope model and the
/// runtime's into a loud failure in the test suite rather than a silent mistake.
pub fn define_at(env: &Env, name: &str, slot: usize, value: Value, constant: bool) -> Result<(), String> {
    let mut scope = env.borrow_mut();
    if scope.bindings.contains(name) {
        return Err(format!("{:?} is already declared in this scope", name));
    }
    debug_assert_eq!(slot, scope.bindings.len(), "declaration slot for {:?}", name);
    scope.bindings.insert(name, Binding { value, constant });
    Ok(())
}

/// Looks a name up the scope chain, borrowing each scope exactly once.
#[inline]
pub fn get(env: &Env, name: &str) -> Result<Value, String> {
    let mut current = Some(env.clone());
    while let Some(scope) = current {
        let next;
        {
            let borrowed = scope.borrow();
            if let Some(binding) = borrowed.bindings.get(name) {
                return Ok(binding.value.clone());
            }
            next = borrowed.parent.clone();
        }
        current = next;
    }
    Err(format!("undefined name {:?}", name))
}

/// Reassigns an existing binding, borrowing each scope exactly once.
#[inline]
pub fn assign(env: &Env, name: &str, value: Value) -> Result<(), String> {
    let mut current = Some(env.clone());
    while let Some(scope) = current {
        let next;
        {
            let mut borrowed = scope.borrow_mut();
            if let Some(binding) = borrowed.bindings.get_mut(name) {
                if binding.constant {
                    return Err(format!("cannot reassign const {:?}", name));
                }
                binding.value = value;
                return Ok(());
            }
            next = borrowed.parent.clone();
        }
        current = next;
    }
    Err(format!("undefined name {:?}; declare it with const or var before assignment", name))
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
    /// The compiled body, when the bytecode engine produced this function.
    pub code: Option<Rc<bytecode::Chunk>>,
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
#[inline]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn number(value: f64) -> Value {
        Value::Number(value)
    }

    fn dict_with(pairs: &[(Value, &str)]) -> Dict {
        let mut dict = Dict::default();
        for (key, value) in pairs {
            dict.set(key.clone(), Value::string(value)).expect("set");
        }
        dict
    }

    fn text(value: &Value) -> String {
        format(value)
    }

    #[test]
    fn number_keys_use_the_value_not_its_text() {
        // `1` and `1.0` are the same key, and `-0.0` folds into `0.0`.
        let dict = dict_with(&[(number(1.0), "one"), (number(0.0), "zero")]);
        assert_eq!(dict.get(&number(1.0)).unwrap().map(|value| text(&value)).as_deref(), Some("one"));
        assert_eq!(dict.get(&number(-0.0)).unwrap().map(|value| text(&value)).as_deref(), Some("zero"));
        // A number key and its string spelling stay distinct.
        assert!(dict.get(&Value::string("1")).unwrap().is_none());
        assert_ne!(hash(&Value::string("1")).unwrap(), hash(&number(1.0)).unwrap());
    }

    #[test]
    fn non_scalar_and_non_finite_keys_are_rejected() {
        assert!(hash(&number(f64::NAN)).is_err());
        assert!(hash(&number(f64::INFINITY)).is_err());
        assert!(hash(&Value::Null).is_err());
        assert!(hash(&Value::list(Vec::new())).is_err());
    }

    #[test]
    fn set_overwrites_an_existing_key_in_place() {
        let mut dict = dict_with(&[(number(1.0), "one"), (number(2.0), "two")]);
        dict.set(number(1.0), Value::string("uno")).expect("set");
        assert_eq!(dict.entries.len(), 2);
        assert_eq!(dict.get(&number(1.0)).unwrap().map(|value| text(&value)).as_deref(), Some("uno"));
    }

    #[test]
    fn bindings_keep_order_and_promote_to_an_index() {
        let mut bindings = Bindings::default();
        bindings.insert("a", Binding { value: number(1.0), constant: false });
        bindings.insert("b", Binding { value: number(2.0), constant: true });
        assert!(bindings.contains("a"));
        assert!(bindings.get("b").unwrap().constant);
        assert_eq!(bindings.keys().collect::<Vec<_>>(), vec!["a", "b"]);

        // Past the threshold the scope keeps working with its hash index.
        for index in 0..32 {
            bindings.insert(&format!("name{index}"), Binding { value: number(index as f64), constant: false });
        }
        assert_eq!(bindings.get("a").unwrap().value.type_name(), "number");
        assert_eq!(bindings.get("name0").unwrap().value.type_name(), "number");
        assert!(bindings.get("missing").is_none());
        *bindings.get_mut("b").unwrap() = Binding { value: number(9.0), constant: false };
        assert_eq!(bindings.get("b").unwrap().value.type_name(), "number");
    }

    #[test]
    fn environment_get_and_assign_walk_the_chain() {
        let outer = Environment::new(None);
        define(&outer, "x", number(1.0), false).expect("define");
        let inner = Environment::new(Some(outer.clone()));
        define(&inner, "y", number(2.0), false).expect("define");
        assert_eq!(get(&inner, "x").unwrap().type_name(), "number");
        assert!(get(&inner, "z").is_err());
        assign(&inner, "x", number(5.0)).expect("assign");
        assert!(matches!(get(&outer, "x").unwrap(), Value::Number(value) if value == 5.0));
        // Duplicate declaration in one scope is rejected.
        assert!(define(&inner, "y", number(0.0), false).is_err());
        // Reassigning a constant is rejected.
        let scope = Environment::new(None);
        define(&scope, "k", number(1.0), true).expect("define");
        assert!(assign(&scope, "k", number(2.0)).is_err());
    }
}
