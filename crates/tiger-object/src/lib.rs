//! Runtime values, lexical environments, classes, and value formatting.
//!
//! Mirrors `pkg/object`: mutable aggregates are shared through `Rc<RefCell<..>>`
//! so aliasing behaves like Go pointers, `Dict` preserves insertion order with a
//! hash index, and `Format`/`Equal`/`Truthy` reproduce the Go semantics exactly.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use tiger_ast::{FieldDecl, FunctionDecl};

pub mod bytecode;
mod rcstr;

pub use rcstr::RcStr;

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
    String(RcStr),
    Bool(bool),
    Null,
    List(Rc<RefCell<Vec<Value>>>),
    /// A fixed-length, immutable group: `(1, 2)`. Shared like a string, because
    /// nothing can mutate it in place.
    Tuple(Rc<Vec<Value>>),
    /// An insertion-ordered set of hashable scalars: `{1, 2}`.
    Set(Rc<RefCell<Set>>),
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
    /// A lazy iterator: a value that produces its elements on demand. The
    /// evaluator drives every step, because a `map` or generator body may call
    /// back into user code; see [`IteratorValue`].
    Iterator(Rc<RefCell<IteratorValue>>),
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
            Value::Tuple(_) => "tuple",
            Value::Set(_) => "set",
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
            Value::Iterator(_) => "iterator",
        }
    }

    pub fn string(text: &str) -> Value {
        Value::String(RcStr::new(text))
    }

    pub fn list(elements: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(elements)))
    }

    pub fn tuple(elements: Vec<Value>) -> Value {
        Value::Tuple(Rc::new(elements))
    }

    pub fn set(elements: Vec<Value>) -> Result<Value, String> {
        let mut set = Set::default();
        for element in elements {
            set.add(element)?;
        }
        Ok(Value::Set(Rc::new(RefCell::new(set))))
    }

    /// A fresh lazy iterator with the given state.
    pub fn iterator(kind: IteratorKind) -> Value {
        Value::Iterator(Rc::new(RefCell::new(IteratorValue { kind, done: false })))
    }

    /// Whether this value is a lazy iterator.
    pub fn is_iterator(&self) -> bool {
        matches!(self, Value::Iterator(_))
    }

    /// The elements of an ordered collection, or `None` for any other value.
    ///
    /// Lists and tuples are the language's ordered sequences, so the operations
    /// that need positions — indexing, slicing, and the sequence built-ins — share
    /// this one test.
    pub fn sequence(&self) -> Option<Vec<Value>> {
        match self {
            Value::List(elements) => Some(elements.borrow().clone()),
            Value::Tuple(elements) => Some(elements.to_vec()),
            _ => None,
        }
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
    String(RcStr),
    Bool(bool),
}

/// Builds the hash key for a value, rejecting non-scalar and non-finite keys.
pub fn hash(value: &Value) -> Result<Key, String> {
    key_for(value, "dictionary key", "dictionary keys must be finite")
}

/// Builds the hash key for a set element.
///
/// A set holds the same scalars a dictionary accepts as keys, so its members can
/// be looked up without scanning, and a set can never contain a mutable value
/// that would change out from under the index.
pub fn element_key(value: &Value) -> Result<Key, String> {
    key_for(value, "set element", "set elements must be finite")
}

fn key_for(value: &Value, role: &str, non_finite: &str) -> Result<Key, String> {
    match value {
        Value::Number(number) => {
            if !number.is_finite() {
                return Err(non_finite.to_string());
            }
            let normalized = if *number == 0.0 { 0.0 } else { *number };
            Ok(Key::Number(normalized.to_bits()))
        }
        Value::String(text) => Ok(Key::String(text.clone())),
        Value::Bool(flag) => Ok(Key::Bool(*flag)),
        other => Err(format!("{} cannot be a {}", other.type_name(), role)),
    }
}

/// An insertion-ordered dictionary with a hash index for key lookup.
///
/// `last` remembers where the most recent lookup found its entry. A read-modify-write through a
/// dictionary — `counts[key] += 1` — reads the entry and then stores back into it, and without this
/// the store would hash the key and probe the index all over again. An insert appends, so it never
/// moves an entry a lookup found; a `delete` shifts the entries after it and a `clear` removes them
/// all, and both drop this.
#[derive(Default, Clone)]
pub struct Dict {
    pub entries: Vec<(Value, Value)>,
    pub indices: HashMap<Key, usize, KeyHasherBuilder>,
    last: RefCell<Option<(Key, usize)>>,
}

impl Dict {
    /// An empty dictionary with room for `count` entries, so filling it does not rehash the
    /// index as it grows.
    pub fn with_capacity(count: usize) -> Dict {
        Dict {
            entries: Vec::with_capacity(count),
            indices: HashMap::with_capacity_and_hasher(count, KeyHasherBuilder::default()),
            last: RefCell::new(None),
        }
    }

    pub fn set(&mut self, key: Value, value: Value) -> Result<(), String> {
        let hashed = hash(&key)?;
        // A value that was read out of this dictionary and is being stored back for the same key —
        // which is what a compound assignment through an index is — finds its entry here rather
        // than hashing the key and probing the index a second time.
        let remembered = self.last.borrow().as_ref().and_then(|(found, position)| (found == &hashed).then_some(*position));
        if let Some(position) = remembered {
            self.entries[position].1 = value;
            return Ok(());
        }
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
        match self.indices.get(&hashed).copied() {
            Some(index) => {
                // Remember where this resolved, so a store for the same key is an array write.
                *self.last.borrow_mut() = Some((hashed, index));
                Ok(Some(self.entries[index].1.clone()))
            }
            None => Ok(None),
        }
    }

    /// Whether `key` is present, without reading what it holds.
    ///
    /// A membership test wants the entry, not its value, and going through
    /// [`Dict::get`] would clone a value the caller throws away — for a key that
    /// holds a list or a string that is a refcount per test. The entry the probe
    /// resolved is remembered exactly as a read remembers it, so the store or the
    /// read that follows the test for the same key is still an array write.
    pub fn contains(&self, key: &Value) -> Result<bool, String> {
        let hashed = hash(key)?;
        match self.indices.get(&hashed).copied() {
            Some(index) => {
                *self.last.borrow_mut() = Some((hashed, index));
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Removes the entry for `key`, preserving the order of the rest.
    pub fn delete(&mut self, key: &Value) -> Result<bool, String> {
        let hashed = hash(key)?;
        let Some(index) = self.indices.get(&hashed).copied() else {
            return Ok(false);
        };
        self.entries.remove(index);
        // The entries after the one removed have shifted down, so what the last lookup found is not
        // where it was any more.
        *self.last.borrow_mut() = None;
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
        *self.last.borrow_mut() = None;
    }
}

/// An insertion-ordered set of hashable scalars with a hash index for membership.
///
/// The same shape as [`Dict`] without the values: elements keep insertion order,
/// repeats are absorbed, and removal preserves the order of the rest.
#[derive(Default)]
pub struct Set {
    pub entries: Vec<Value>,
    pub indices: HashMap<Key, usize, KeyHasherBuilder>,
}

impl Set {
    /// Adds `value`; an element already present keeps its place.
    pub fn add(&mut self, value: Value) -> Result<(), String> {
        let hashed = element_key(&value)?;
        if self.indices.contains_key(&hashed) {
            return Ok(());
        }
        self.indices.insert(hashed, self.entries.len());
        self.entries.push(value);
        Ok(())
    }

    pub fn contains(&self, value: &Value) -> Result<bool, String> {
        Ok(self.indices.contains_key(&element_key(value)?))
    }

    /// Removes `value`, preserving the order of the rest, and reports whether it
    /// was there.
    pub fn remove(&mut self, value: &Value) -> Result<bool, String> {
        let hashed = element_key(value)?;
        let Some(index) = self.indices.get(&hashed).copied() else {
            return Ok(false);
        };
        self.entries.remove(index);
        self.indices.remove(&hashed);
        for offset in index..self.entries.len() {
            if let Ok(moved) = element_key(&self.entries[offset]) {
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

/// A lazy iterator's state, driven one step at a time by the evaluator.
///
/// Nothing here evaluates user code: the evaluator reads the state, does the
/// step, and stores it back. That keeps this crate free of the interpreter while
/// still letting a generator body or a `map` callback run.
pub struct IteratorValue {
    pub kind: IteratorKind,
    pub done: bool,
}

/// What a lazy iterator is walking.
pub enum IteratorKind {
    /// Exhausted. The evaluator also swaps this in while it steps a real kind
    /// out, so the state can be replaced without holding a borrow across a call.
    Done,
    /// `range(start, stop, step)`, advanced arithmetically.
    Range { current: i64, stop: i64, step: i64 },
    /// A one-shot walk over a collection already in memory, by position.
    Sequence { source: Value, index: usize },
    /// `map(source, function)`: `function` is called for each element of `source`.
    Map { source: Value, function: Value, env: Env },
    /// `filter(source, function)`: keeps the elements `function` reports truthy.
    Filter { source: Value, function: Value, env: Env },
    /// A generator expression. `cursors` holds one open source iterator per
    /// active clause, innermost last.
    Generator { proto: Rc<bytecode::GenProto>, env: Env, scope: Env, cursors: Vec<Value> },
}

/// The element at `index` of a collection already in memory, or `None` past its
/// end. A dictionary contributes its keys, exactly as a `for` loop iterates it.
pub fn sequence_element(value: &Value, index: usize) -> Option<Value> {
    match value {
        Value::List(elements) => elements.borrow().get(index).cloned(),
        Value::Tuple(elements) => elements.get(index).cloned(),
        Value::Set(set) => set.borrow().entries.get(index).cloned(),
        Value::String(text) => text.chars().nth(index).map(|ch| Value::string(&ch.to_string())),
        Value::Dict(dict) => dict.borrow().entries.get(index).map(|(key, _)| key.clone()),
        _ => None,
    }
}

/// Whether a value can be walked by a `for` loop, a comprehension, or a
/// reshaping built-in: the ordered collections, a string, a dictionary's keys,
/// and a lazy iterator.
pub fn is_iterable(value: &Value) -> bool {
    matches!(value, Value::List(_) | Value::Tuple(_) | Value::Set(_) | Value::Dict(_) | Value::String(_) | Value::Iterator(_))
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
    index: Option<HashMap<Rc<str>, usize, BuildHasherDefault<NameHasher>>>,
}

/// The number of bindings at which a scope gains a hash index.
const BINDINGS_INDEX_THRESHOLD: usize = 8;

/// A count of every write to any binding, anywhere.
///
/// A global name read may be remembered against this count, and remembering one is
/// only sound while no binding has changed since: a later `var` can shadow the name
/// the read resolved to, and a scope can be entered that holds one. Bumping this in
/// the two places a binding is written — the scope's own insert and its mutable
/// accessor, which every definition and assignment goes through — gives the cache a
/// guard that no write can forget. An over-eager bump costs a cache refill; a missed
/// one would be a wrong answer.
static BINDING_EPOCH: AtomicU64 = AtomicU64::new(0);

/// The current binding epoch; see [`BINDING_EPOCH`].
#[inline]
pub fn binding_epoch() -> u64 {
    BINDING_EPOCH.load(Ordering::Relaxed)
}

/// Moves the binding epoch on, which forgets every remembered name read.
///
/// A write to a binding does this itself. What this is for is a change to *where a name
/// resolves* that is not a write to one: a pooled scope handed out with a different parent
/// reaches different bindings through the same environment object, and a read remembered
/// against that object would answer for the parent it used to have.
#[inline]
pub fn invalidate_name_reads() {
    BINDING_EPOCH.fetch_add(1, Ordering::Relaxed);
}

/// A fast hasher for the names in a lexical scope.
///
/// A scope's names are the program's own identifiers, not data a caller supplies,
/// so the flooding resistance a randomized hasher buys is not needed here — while
/// its cost is paid on every global and built-in reference, because those resolve
/// through a scope with enough names to have been indexed. The mixing is FxHash's:
/// a rotate, an xor, and a multiply per word.
///
/// Dictionary and set keys use [`KeyHasher`] instead: those are program data, so the seed
/// that keeps them from being floodable lives there rather than here.

#[derive(Default)]
pub struct NameHasher(u64);

/// FxHash's 64-bit constant.
const NAME_HASH_SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl Hasher for NameHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut state = self.0;
        for chunk in bytes.chunks(8) {
            let mut word = 0u64;
            for (index, byte) in chunk.iter().enumerate() {
                word |= u64::from(*byte) << (index * 8);
            }
            state = (state.rotate_left(5) ^ word).wrapping_mul(NAME_HASH_SEED);
        }
        // The length term keeps a name from colliding with its own extension.
        self.0 = (state.rotate_left(5) ^ bytes.len() as u64).wrapping_mul(NAME_HASH_SEED);
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&[value]);
    }
}

/// A fast hasher for dictionary and set keys, seeded once per process.
///
/// [`NameHasher`] can start from nothing because a lexical scope holds the program's own
/// identifiers. A dictionary holds whatever the program was handed, so a blind flooding
/// attack must not be able to make every key collide: the seed is what stops that, and it
/// comes from the same source the standard hasher's does.
///
/// What the seed does not buy back is SipHash's round over every key, which is the whole
/// of this hasher's reason to exist. A key is hashed on every probe, an insert hashes it
/// again, and `counts[key] += 1` is three probes for one key, so the fixed cost of the
/// round is paid three times on every iteration of a dictionary loop. The mixing here is
/// [`NameHasher`]'s — a rotate, an xor, and a multiply per word — and the seed is folded
/// in before the first one.
#[derive(Clone)]
pub struct KeyHasherBuilder {
    seed: u64,
}

impl KeyHasherBuilder {
    /// The state every hasher this builds starts from.
    ///
    /// Read once per process, so two runs of a program do not collide the same keys and a
    /// program that indexes values a caller supplied cannot be made to collide them all.
    fn seed() -> u64 {
        static SEED: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
        // The seed is odd so that a zero-length key has not hashed to nothing before the
        // first word is folded in.
        *SEED.get_or_init(|| {
            let mut source = std::collections::hash_map::RandomState::new().build_hasher();
            source.write_u8(0);
            source.finish() | 1
        })
    }
}

impl Default for KeyHasherBuilder {
    fn default() -> KeyHasherBuilder {
        KeyHasherBuilder { seed: KeyHasherBuilder::seed() }
    }
}

impl BuildHasher for KeyHasherBuilder {
    type Hasher = KeyHasher;

    fn build_hasher(&self) -> KeyHasher {
        KeyHasher(self.seed)
    }
}

/// The hasher [`KeyHasherBuilder`] builds.
pub struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut state = self.0;
        for chunk in bytes.chunks(8) {
            let mut word = 0u64;
            for (index, byte) in chunk.iter().enumerate() {
                word |= u64::from(*byte) << (index * 8);
            }
            state = (state.rotate_left(5) ^ word).wrapping_mul(NAME_HASH_SEED);
        }
        // The length term keeps a key from colliding with its own extension.
        self.0 = (state.rotate_left(5) ^ bytes.len() as u64).wrapping_mul(NAME_HASH_SEED);
    }

    fn write_u8(&mut self, value: u8) {
        self.write(&[value]);
    }
}

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
            Some(position) => {
                BINDING_EPOCH.fetch_add(1, Ordering::Relaxed);
                Some(&mut self.entries[position].1)
            }
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
        BINDING_EPOCH.fetch_add(1, Ordering::Relaxed);
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

    /// Inserts a batch of bindings into an *empty* scope, in order.
    ///
    /// This is the installer's case: a fresh top-level scope takes every built-in and every
    /// module at once. One at a time, each entry costs a hash of its name, an atomic bump of
    /// the binding epoch, and a borrow of the scope's cell — about a hundred of each, before
    /// a program runs its first instruction. A batch costs one bump and one index build for
    /// the lot.
    ///
    /// Nothing is checked against what the scope already holds, which is what the scope being
    /// empty means: the debug assertion is the statement of that contract, and it holds for
    /// every caller — a run's top-level scope is built here and nowhere else.
    pub fn extend_shared(&mut self, entries: Vec<(Rc<str>, Binding)>) {
        debug_assert!(self.entries.is_empty(), "a batch insert fills an empty scope");
        BINDING_EPOCH.fetch_add(1, Ordering::Relaxed);
        self.entries = entries;
        self.index = None;
        self.ensure_index();
    }

    /// Gives the scope a hash index once it holds enough names to want one.
    fn ensure_index(&mut self) {
        if self.entries.len() <= BINDINGS_INDEX_THRESHOLD {
            self.index = None;
            return;
        }
        let mut index = HashMap::with_capacity_and_hasher(self.entries.len(), BuildHasherDefault::<NameHasher>::default());
        for (position, (key, _)) in self.entries.iter().enumerate() {
            index.insert(key.clone(), position);
        }
        self.index = Some(index);
    }

    /// Inserts a binding whose name is already a shared `Rc<str>`, so a repeated
    /// call with the same parameter name allocates nothing for the key.
    fn insert_shared(&mut self, key: Rc<str>, binding: Binding) {
        BINDING_EPOCH.fetch_add(1, Ordering::Relaxed);
        let position = self.entries.len();
        self.entries.push((key.clone(), binding));
        match &mut self.index {
            Some(index) => {
                index.insert(key, position);
            }
            None => {
                if self.entries.len() > BINDINGS_INDEX_THRESHOLD {
                    self.ensure_index();
                }
            }
        }
    }

    /// Empties the scope, keeping the backing storage so a reused scope does
    /// not allocate again.
    pub fn clear(&mut self) {
        // A scope that is emptied can take bindings a remembered name read
        // resolved through, so the epoch moves here too. It is a coarse guard — a
        // released scope invalidates every remembered read, not only the ones that
        // went through it.
        //
        // A scope with nothing in it has nothing to take away, so clearing one moves
        // nothing. That is the block a loop body opens when it declares only flat locals:
        // it is pushed and released on every pass, and moving the epoch each time made
        // every global name read in the body — len, str — walk the scope chain again,
        // which was most of what such a body cost. What a reused scope *can* change is
        // where its parent leads, and that is what invalidate_name_reads is for.
        if self.entries.is_empty() {
            self.index = None;
            return;
        }
        BINDING_EPOCH.fetch_add(1, Ordering::Relaxed);
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

/// Defines a binding whose name is already a shared `Rc<str>` and whose slot the
/// compiler already knows, so a loop that declares its variable on every pass
/// allocates neither the key nor a lookup.
pub fn define_at_shared(env: &Env, name: &Rc<str>, slot: usize, value: Value, constant: bool) -> Result<(), String> {
    let mut scope = env.borrow_mut();
    if scope.bindings.contains(name) {
        return Err(format!("{:?} is already declared in this scope", name));
    }
    debug_assert_eq!(slot, scope.bindings.len(), "declaration slot for {:?}", name);
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
            let found = class.fields.borrow().iter().find(|field| field.declaration.name == name).cloned();
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
    /// The field's name, interned once when the class is defined. An instance
    /// stores this handle rather than a fresh string, so building one allocates
    /// nothing for its field names.
    pub name: Rc<str>,
}

/// An instance with its own field storage.
pub struct Instance {
    pub class: Rc<Class>,
    /// The declared fields in declaration order, each with the name its class
    /// interned.
    ///
    /// A class declares a handful of fields, so an ordered vector beats a map: a
    /// lookup is a short scan of short string comparisons instead of a hash of the
    /// name, and it never allocates.
    pub fields: RefCell<Vec<(Rc<str>, Value)>>,
}

impl Instance {
    /// Sets the field named `name` to `value`.
    pub fn set_field(&self, name: &Rc<str>, value: Value) {
        let mut fields = self.fields.borrow_mut();
        match fields.iter_mut().find(|(field, _)| field == name) {
            Some(slot) => slot.1 = value,
            None => fields.push((name.clone(), value)),
        }
    }
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
    /// The names other files may reach, in source order. `None` exposes everything
    /// the environment holds, which is how the built-in modules are built.
    pub exports: Option<Vec<String>>,
    /// What fills `env` the first time something asks the module for a member, for a
    /// module built with [`Module::lazy`]; `None` once it has run, and from the start for
    /// every other module.
    pending: RefCell<Option<LazyBindings>>,
    /// Whether `pending` has nothing left to run, so a member read is one flag test.
    ready: std::cell::Cell<bool>,
    /// Members that are built only when a program reads them by name, for a lazy module that
    /// has one too costly to build for a program that never mentions it.
    late: RefCell<Vec<(&'static str, LateValue)>>,
}

/// What builds one deferred member of a lazy module.
type LateValue = Box<dyn FnOnce() -> Value>;

/// The bindings a lazy module fills its environment with when it is first read.
type LazyBindings = Box<dyn FnOnce() -> Vec<(Rc<str>, Binding)>>;

impl Module {
    /// A module that exposes everything its environment holds.
    ///
    /// The built-in modules are built this way: every name they install is public.
    pub fn new(name: String, env: Env) -> Rc<Module> {
        Rc::new(Module {
            name,
            env,
            exports: None,
            pending: RefCell::new(None),
            ready: std::cell::Cell::new(true),
            late: RefCell::new(Vec::new()),
        })
    }

    /// A module whose members are built the first time one is read.
    ///
    /// A built-in module that belongs to a run — one with a generator a program seeds, or
    /// the run's own arguments — has to be built per run, and most programs never name it.
    /// Building it where it is installed made every run pay for the members of modules it
    /// never touched; this builds it where it is used. Everything else about it is the
    /// ordinary module: it exposes all of its bindings, and the run still gets its own.
    pub fn lazy(name: String, build: impl FnOnce() -> Vec<(Rc<str>, Binding)> + 'static) -> Rc<Module> {
        Rc::new(Module {
            name,
            env: Environment::new(None),
            exports: None,
            pending: RefCell::new(Some(Box::new(build))),
            ready: std::cell::Cell::new(false),
            late: RefCell::new(Vec::new()),
        })
    }

    /// Defers one member of a lazy module until it is read by name.
    ///
    /// `sys.env` is the case: it is a copy of the whole process environment, a program that
    /// reads `sys.argv` has no use for it, and building it with the rest of the module made
    /// every such program pay for a hundred variables it never looked at.
    pub fn defer(&self, name: &'static str, build: impl FnOnce() -> Value + 'static) {
        self.late.borrow_mut().push((name, Box::new(build)));
    }

    /// Builds every deferred member, for the one reader that wants all of a module's names.
    pub fn force_all(&self) {
        self.force();
        let names: Vec<&'static str> = self.late.borrow().iter().map(|(name, _)| *name).collect();
        for name in names {
            self.build_late(name);
        }
    }

    /// Builds the deferred member `name` and binds it, or `None` when there is none.
    #[cold]
    fn build_late(&self, name: &str) -> Option<Value> {
        let position = self.late.borrow().iter().position(|(late, _)| *late == name)?;
        let (late, build) = self.late.borrow_mut().remove(position);
        let value = build();
        // A constant, like every binding a built-in module has.
        let _ = define(&self.env, late, value.clone(), true);
        Some(value)
    }

    /// Runs what fills a lazy module's environment, once. A module that is not lazy, or
    /// that has already been filled, returns after one flag test.
    #[inline]
    pub fn force(&self) {
        if self.ready.get() {
            return;
        }
        self.fill();
    }

    #[cold]
    fn fill(&self) {
        // The flag goes up before the build runs, so a build that read this module back
        // would see it empty rather than recurse; none does.
        self.ready.set(true);
        let build = self.pending.borrow_mut().take();
        if let Some(build) = build {
            self.env.borrow_mut().bindings.extend_shared(build());
        }
    }

    /// A module that exposes only `exports`, in source order.
    ///
    /// A file that marks nothing with `export` shares nothing, so this is called
    /// with an empty list for a module that keeps everything private. The parser
    /// records an export only for the declaration it marks, so every name here is
    /// one the module declares.
    pub fn with_exports(name: String, env: Env, exports: Vec<String>) -> Rc<Module> {
        Rc::new(Module {
            name,
            env,
            exports: Some(exports),
            pending: RefCell::new(None),
            ready: std::cell::Cell::new(true),
            late: RefCell::new(Vec::new()),
        })
    }

    /// Reads a member this module exposes, or `None` for a name it does not share.
    ///
    /// A file that was imported keeps its own private names in the same environment,
    /// because its functions close over that environment; this is the gate that
    /// keeps those names out of reach from other files.
    pub fn member(&self, name: &str) -> Option<Value> {
        self.force();
        if let Some(exports) = &self.exports {
            // A module's export list is short, so a scan is cheaper than a set and
            // keeps the source order for diagnostics.
            if !exports.iter().any(|exported| exported == name) {
                return None;
            }
        }
        if let Some(value) = get_own(&self.env, name) {
            return Some(value);
        }
        // A deferred member is built the first time it is asked for by name, and bound so the
        // next read finds it where every other member is.
        self.build_late(name)
    }
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
///
/// `consumes_iterables` marks a built-in that walks its arguments: the evaluator
/// drains a lazy iterator argument into a list before the call, so `list`, `sum`,
/// `zip`, and their kin accept a generator exactly as they accept a list.
pub struct Builtin {
    /// The name diagnostics quote. A shared string rather than an owned one, because a
    /// built-in is built on every method access and most names fit in the handle.
    pub name: RcStr,
    pub higher_order: bool,
    pub consumes_iterables: bool,
    pub payload: Option<Value>,
    pub call: Rc<BuiltinFn>,
}

impl Builtin {
    pub fn new<F>(name: &str, call: F) -> Rc<Builtin>
    where
        F: Fn(&mut dyn Host, &[Value], &[Keyword]) -> Result<Value, String> + 'static,
    {
        Rc::new(Builtin { name: RcStr::new(name), higher_order: false, consumes_iterables: false, payload: None, call: Rc::new(call) })
    }

    /// Builds a built-in that walks its arguments, so the evaluator drains any
    /// lazy iterator among them first.
    pub fn consuming<F>(name: &str, call: F) -> Rc<Builtin>
    where
        F: Fn(&mut dyn Host, &[Value], &[Keyword]) -> Result<Value, String> + 'static,
    {
        Rc::new(Builtin { name: RcStr::new(name), higher_order: false, consumes_iterables: true, payload: None, call: Rc::new(call) })
    }

    /// Builds a placeholder for a callable the evaluator dispatches itself.
    pub fn higher_order(name: &str, payload: Option<Value>) -> Rc<Builtin> {
        let label = RcStr::new(name);
        Rc::new(Builtin {
            name: label.clone(),
            higher_order: true,
            consumes_iterables: false,
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
    /// Whether this handle has changed the file it names. A handle that only read
    /// never has, and closing it must not write the bytes back: that would be a
    /// syscall spent changing nothing, and it could clobber what another writer
    /// put there in the meantime.
    pub dirty: bool,
}

/// Formats a value for `print`, `str()`, and diagnostics.
pub fn format(value: &Value) -> String {
    format_value(value, &mut Vec::new(), false)
}

fn format_value(value: &Value, seen: &mut Vec<usize>, nested: bool) -> String {
    let mut out = String::new();
    write_value(&mut out, value, seen, nested);
    out
}

/// Appends `value` in the form [`format`] returns to `out`.
///
/// Every value of a printed line, and every element of a nested collection, is
/// written into the one buffer, so rendering allocates nothing beyond the
/// caller's own string.
pub fn write_format(out: &mut String, value: &Value) {
    write_value(out, value, &mut Vec::new(), false);
}

fn write_value(out: &mut String, value: &Value, seen: &mut Vec<usize>, nested: bool) {
    match value {
        Value::Number(number) => tiger_format::write_number(out, *number),
        Value::String(text) => {
            if nested {
                tiger_format::write_quote(out, text);
            } else {
                out.push_str(text);
            }
        }
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
        Value::List(elements) => {
            let pointer = Rc::as_ptr(elements) as usize;
            if seen.contains(&pointer) {
                out.push_str("[...]");
                return;
            }
            seen.push(pointer);
            out.push('[');
            {
                let borrowed = elements.borrow();
                for (index, element) in borrowed.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    write_value(out, element, seen, true);
                }
            }
            out.push(']');
            seen.pop();
        }
        Value::Tuple(elements) => {
            // A tuple cannot contain itself, so only its elements need cycle marks.
            out.push('(');
            for (index, element) in elements.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_value(out, element, seen, true);
            }
            if elements.len() == 1 {
                out.push(',');
            }
            out.push(')');
        }
        Value::Set(set) => {
            let borrowed = set.borrow();
            if borrowed.entries.is_empty() {
                // `{}` stays an empty dictionary, so the empty set keeps its
                // constructor spelling, as in Python.
                out.push_str("set()");
                return;
            }
            out.push('{');
            for (index, element) in borrowed.entries.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_value(out, element, seen, true);
            }
            out.push('}');
        }
        Value::Dict(dict) => {
            let pointer = Rc::as_ptr(dict) as usize;
            if seen.contains(&pointer) {
                out.push_str("{...}");
                return;
            }
            seen.push(pointer);
            out.push('{');
            {
                let borrowed = dict.borrow();
                for (index, (key, entry)) in borrowed.entries.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    write_value(out, key, seen, true);
                    out.push_str(": ");
                    write_value(out, entry, seen, true);
                }
            }
            out.push('}');
            seen.pop();
        }
        Value::Function(function) => {
            let name: &str = if function.declaration.name.is_empty() { "anonymous" } else { &function.declaration.name };
            out.push_str("<function ");
            out.push_str(name);
            out.push('>');
        }
        Value::Module(module) => {
            out.push_str("<module ");
            out.push_str(&module.name);
            out.push('>');
        }
        Value::File(file) => {
            out.push_str("<file ");
            tiger_format::write_quote(out, &file.borrow().name);
            out.push('>');
        }
        Value::Class(class) => {
            out.push_str("<class ");
            out.push_str(&class.name);
            out.push('>');
        }
        Value::Instance(instance) => {
            out.push('<');
            out.push_str(&instance.class.name);
            out.push_str(" instance>");
        }
        Value::BoundMethod(bound) => {
            out.push_str("<bound method ");
            out.push_str(&bound.method.owner.name);
            out.push('.');
            out.push_str(&bound.method.function.declaration.name);
            out.push('>');
        }
        Value::Super(_) => out.push_str("<super>"),
        Value::Builtin(builtin) => {
            out.push_str("<builtin ");
            out.push_str(&builtin.name);
            out.push('>');
        }
        Value::Error(error) => out.push_str(&error.text()),
        // A lazy iterator has not produced its elements yet, so it prints as
        // itself rather than materializing. Use `list(...)` for the values.
        Value::Iterator(_) => out.push_str("<iterator>"),
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
        Value::Tuple(elements) => !elements.is_empty(),
        Value::Set(set) => !set.borrow().entries.is_empty(),
        Value::Dict(dict) => !dict.borrow().entries.is_empty(),
        _ => true,
    }
}

/// Structural equality with cycle detection, mirroring `object.Equal`.
pub fn equal(left: &Value, right: &Value) -> bool {
    // The scalar cases are what a comparison usually hits, and none of them can
    // cycle, so they are decided before a cycle guard is even set up.
    match (left, right) {
        (Value::Number(first), Value::Number(second)) => return first == second,
        (Value::String(first), Value::String(second)) => return first == second,
        (Value::Bool(first), Value::Bool(second)) => return first == second,
        (Value::Null, Value::Null) => return true,
        _ => {}
    }
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
        (Value::Tuple(first), Value::Tuple(second)) => {
            first.len() == second.len() && first.iter().zip(second.iter()).all(|(a, b)| equal_inner(a, b, seen))
        }
        (Value::Set(first), Value::Set(second)) => {
            let first = first.borrow();
            let second = second.borrow();
            // Membership is by hash key, so order does not matter.
            first.entries.len() == second.entries.len() && first.indices.keys().all(|key| second.indices.contains_key(key))
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
        (Value::Iterator(first), Value::Iterator(second)) => Rc::ptr_eq(first, second),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(value: f64) -> Value {
        Value::Number(value)
    }

    #[test]
    fn a_read_then_a_store_for_the_same_key_lands_in_the_same_entry() {
        // The shape the dictionary remembers a position for: read through the index, then store back
        // for the same key. What must not change is which entry holds what, or the order.
        let mut dict = dict_with(&[(number(1.0), "one"), (number(2.0), "two")]);
        assert_eq!(dict.get(&number(1.0)).expect("get").map(|value| text(&value)), Some("one".to_string()));
        dict.set(number(1.0), Value::string("ONE")).expect("set after a read");
        assert_eq!(dict.entries.len(), 2);
        assert_eq!(text(&dict.entries[0].0), "1");
        assert_eq!(text(&dict.entries[0].1), "ONE");

        // A store for a key nothing read still finds its entry.
        dict.set(number(2.0), Value::string("TWO")).expect("set");
        assert_eq!(text(&dict.entries[1].1), "TWO");

        // An insert appends, so it does not move the entry a lookup found.
        dict.set(number(3.0), Value::string("three")).expect("insert");
        dict.get(&number(2.0)).expect("get");
        dict.set(number(2.0), Value::string("twice")).expect("set after an insert");
        assert_eq!(text(&dict.entries[1].1), "twice");
        assert_eq!(text(&dict.entries[2].0), "3");

        // A `delete` shifts everything after it, so a remembered position cannot outlive one.
        assert!(dict.delete(&number(1.0)).expect("delete"));
        dict.get(&number(2.0)).expect("get");
        dict.set(number(2.0), Value::string("again")).expect("set after a delete");
        assert_eq!(dict.entries.len(), 2);
        assert_eq!(text(&dict.entries[0].0), "2");
        assert_eq!(text(&dict.entries[0].1), "again");

        // And a `clear` leaves nothing to remember at all.
        dict.clear();
        dict.set(number(4.0), Value::string("four")).expect("set after a clear");
        assert_eq!(dict.entries.len(), 1);
        assert_eq!(text(&dict.entries[0].1), "four");
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
    fn the_scope_name_hasher_separates_names() {
        use std::hash::Hash;

        let hash_of = |name: &str| {
            let mut hasher = NameHasher::default();
            name.hash(&mut hasher);
            hasher.finish()
        };
        // Equal names hash equal, which is what a lookup relies on.
        assert_eq!(hash_of("count"), hash_of("count"));
        assert_eq!(hash_of(""), hash_of(""));
        // A name is not confused with a different one, with its own extension, or
        // with a rearrangement of its bytes.
        assert_ne!(hash_of("count"), hash_of("counts"));
        assert_ne!(hash_of("count"), hash_of("contu"));
        assert_ne!(hash_of("a"), hash_of("b"));
        assert_ne!(hash_of(""), hash_of("a"));
        // Names longer than one word still separate.
        assert_ne!(hash_of("a_very_long_identifier_name"), hash_of("a_very_long_identifier_namf"));
    }

    #[test]
    fn a_scope_past_the_threshold_still_resolves_every_name() {
        // Past `BINDINGS_INDEX_THRESHOLD` a scope gains a hash index, so the
        // indexed path is the one that has to agree with the linear scan.
        let env = Environment::new(None);
        let names: Vec<String> = (0..24).map(|index| format!("name{}", index)).collect();
        for (index, name) in names.iter().enumerate() {
            define(&env, name, number(index as f64), false).expect("define");
        }
        assert!(env.borrow().bindings.index.is_some(), "the scope must have been indexed");
        for (index, name) in names.iter().enumerate() {
            match get(&env, name) {
                Ok(Value::Number(value)) => assert_eq!(value, index as f64, "{}", name),
                other => panic!("{} resolved to {:?}", name, other.is_ok()),
            }
        }
        // A name that is not there stays undefined rather than resolving to a
        // neighbour.
        assert!(get(&env, "name99").is_err());
        assert!(get(&env, "nam").is_err());
        // A shadowing declaration in an inner scope still wins over the index.
        let inner = Environment::new(Some(env.clone()));
        define(&inner, "name3", number(-1.0), false).expect("define");
        assert_eq!(get(&inner, "name3").map(|value| text(&value)).unwrap(), "-1");
        assert_eq!(get(&env, "name3").map(|value| text(&value)).unwrap(), "3");
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
    fn formats_sets_and_tuples() {
        assert_eq!(text(&Value::tuple(vec![number(1.0), number(2.0)])), "(1, 2)");
        assert_eq!(text(&Value::tuple(vec![number(1.0)])), "(1,)");
        assert_eq!(text(&Value::tuple(Vec::new())), "()");
        assert_eq!(text(&Value::tuple(vec![Value::string("a"), Value::list(vec![number(1.0)])])), "(\"a\", [1])");
        assert_eq!(text(&Value::set(vec![number(1.0), number(2.0)]).expect("set")), "{1, 2}");
        assert_eq!(text(&Value::set(Vec::new()).expect("set")), "set()");
        // A set absorbs repeats and keeps the order it was built in.
        assert_eq!(text(&Value::set(vec![number(2.0), number(1.0), number(2.0)]).expect("set")), "{2, 1}");
    }

    #[test]
    fn sets_and_tuples_compare_structurally() {
        assert!(equal(&Value::tuple(vec![number(1.0), Value::string("a")]), &Value::tuple(vec![number(1.0), Value::string("a")])));
        assert!(!equal(&Value::tuple(vec![number(1.0)]), &Value::tuple(vec![number(2.0)])));
        // A tuple and a list of the same elements stay distinct types.
        assert!(!equal(&Value::tuple(vec![number(1.0)]), &Value::list(vec![number(1.0)])));
        // Sets compare by membership, so insertion order does not matter.
        assert!(equal(
            &Value::set(vec![number(1.0), number(2.0)]).expect("set"),
            &Value::set(vec![number(2.0), number(1.0)]).expect("set")
        ));
        assert!(!equal(&Value::set(vec![number(1.0)]).expect("set"), &Value::set(vec![number(2.0)]).expect("set")));
        assert!(!equal(&Value::set(vec![number(1.0)]).expect("set"), &Value::list(vec![number(1.0)])));
    }

    #[test]
    fn set_elements_and_dictionary_keys_use_the_same_scalars() {
        // A mutable value cannot be an element, and the message names the role.
        assert_eq!(Value::set(vec![Value::list(Vec::new())]).err().unwrap(), "list cannot be a set element");
        assert_eq!(Value::set(vec![Value::Null]).err().unwrap(), "null cannot be a set element");
        assert_eq!(element_key(&number(f64::NAN)).unwrap_err(), "set elements must be finite");
        // Neither new type is a dictionary key.
        assert_eq!(hash(&Value::tuple(vec![number(1.0)])).unwrap_err(), "tuple cannot be a dictionary key");
        assert_eq!(hash(&Value::set(Vec::new()).expect("set")).unwrap_err(), "set cannot be a dictionary key");
    }

    #[test]
    fn set_membership_and_removal_keep_order() {
        let Value::Set(set) = Value::set(vec![number(1.0), number(2.0), number(3.0)]).expect("set") else { panic!("set") };
        assert!(set.borrow().contains(&number(2.0)).expect("contains"));
        assert!(!set.borrow().contains(&number(9.0)).expect("contains"));
        assert!(set.borrow_mut().remove(&number(2.0)).expect("remove"));
        assert!(!set.borrow_mut().remove(&number(2.0)).expect("remove"));
        assert_eq!(text(&Value::Set(set.clone())), "{1, 3}");
        set.borrow_mut().clear();
        assert_eq!(text(&Value::Set(set)), "set()");
    }

    #[test]
    fn the_new_collections_are_truthy_when_populated() {
        assert!(!truthy(&Value::set(Vec::new()).expect("set")));
        assert!(truthy(&Value::set(vec![number(0.0)]).expect("set")));
        assert!(!truthy(&Value::tuple(Vec::new())));
        assert!(truthy(&Value::tuple(vec![number(0.0)])));
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
    fn a_membership_test_leaves_the_entry_it_found_for_the_store() {
        // A test is a read that throws the value away, so it leaves the position a
        // store for the same key then writes through — the same thing a read does.
        let mut dict = dict_with(&[(number(1.0), "one"), (number(2.0), "two")]);
        assert!(dict.contains(&number(1.0)).expect("contains"));
        assert!(!dict.contains(&number(9.0)).expect("contains"));
        // A key that is not a scalar is refused exactly as a read refuses it.
        assert_eq!(dict.contains(&Value::list(Vec::new())).unwrap_err(), "list cannot be a dictionary key");
        // The store that follows finds its entry without probing the index again,
        // which is what the remembered position is for; the entry count is unchanged.
        dict.set(number(1.0), Value::string("uno")).expect("set");
        assert_eq!(dict.entries.len(), 2);
        assert_eq!(dict.get(&number(1.0)).unwrap().map(|value| text(&value)).as_deref(), Some("uno"));
        // A miss remembers nothing, so a store for that key still appends.
        assert!(!dict.contains(&number(9.0)).expect("contains"));
        dict.set(number(9.0), Value::string("nine")).expect("set");
        assert_eq!(dict.entries.len(), 3);
        assert_eq!(text(&Value::Dict(Rc::new(RefCell::new(dict)))), "{1: \"uno\", 2: \"two\", 9: \"nine\"}");
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
