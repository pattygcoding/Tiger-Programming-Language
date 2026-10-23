//! Module loaders and the built-in `math`, `algo`, `random`, `secrets`, `json`,
//! `time`, `regex`, and `sys` modules.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use tiger_object as object;
use tiger_object::{Module, Value};

/// Resolves and loads imported `.tg` sources.
pub trait ModuleLoader {
    fn resolve(&self, importer: &str, requested: &str) -> Result<String, String>;
    fn load(&self, filename: &str) -> Result<String, String>;
}

/// Loads imports from the host filesystem.
pub struct FileLoader;

impl ModuleLoader for FileLoader {
    fn resolve(&self, importer: &str, requested: &str) -> Result<String, String> {
        if !requested.ends_with(".tg") {
            return Err("import path must end in .tg".to_string());
        }
        let requested = requested.replace('/', std::path::MAIN_SEPARATOR_STR);
        let requested_path = std::path::Path::new(&requested);
        let joined = if requested_path.is_absolute() {
            requested_path.to_path_buf()
        } else {
            let base = std::path::Path::new(importer).parent().unwrap_or(std::path::Path::new(""));
            base.join(requested_path)
        };
        let absolute = match std::fs::canonicalize(&joined) {
            Ok(canonical) => canonical,
            Err(_) => match std::env::current_dir() {
                Ok(directory) => directory.join(&joined),
                Err(_) => joined,
            },
        };
        Ok(strip_unc_prefix(&absolute.to_string_lossy()))
    }

    fn load(&self, filename: &str) -> Result<String, String> {
        std::fs::read_to_string(filename).map_err(|error| format!("open {}: {}", filename, error))
    }
}

/// Removes the Windows `\\?\` verbatim prefix so paths match Go's output.
fn strip_unc_prefix(path: &str) -> String {
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

/// Loads imports from an in-memory bundle produced by the standalone builder.
#[derive(Default)]
pub struct BundleLoader {
    pub sources: HashMap<String, String>,
    pub paths: HashMap<String, HashMap<String, String>>,
}

impl ModuleLoader for BundleLoader {
    fn resolve(&self, importer: &str, requested: &str) -> Result<String, String> {
        if let Some(map) = self.paths.get(importer) {
            if let Some(filename) = map.get(requested) {
                return Ok(filename.clone());
            }
        }
        Err(format!("module {:?} imported from {:?} is not bundled", requested, importer))
    }

    fn load(&self, filename: &str) -> Result<String, String> {
        self.sources.get(filename).cloned().ok_or_else(|| format!("module {:?} is not bundled", filename))
    }
}

/// Builds a module from a list of named built-ins.
fn module(name: &str, definitions: &[(&str, Rc<object::Builtin>)]) -> Rc<Module> {
    let env = object::Environment::new(None);
    for (entry, builtin) in definitions {
        let _ = object::define(&env, entry, Value::Builtin(builtin.clone()), true);
    }
    Module::new(name.to_string(), env)
}

/// The `math` module: rounding, roots, powers, logarithms, trigonometry, min/max.
pub fn math_module() -> Rc<Module> {
    module(
        "math",
        &[
            ("ceil", math_unary("ceil", |x| x.ceil())),
            ("floor", math_unary("floor", |x| x.floor())),
            ("round", math_unary("round", |x| x.round_ties_even())),
            ("abs", math_unary("abs", |x| x.abs())),
            ("sin", math_unary("sin", |x| x.sin())),
            ("cos", math_unary("cos", |x| x.cos())),
            ("tan", math_unary("tan", |x| x.tan())),
            ("sqrt", math_unary_checked("sqrt", |x| if x < 0.0 { Err(math_domain()) } else { Ok(x.sqrt()) })),
            ("log", math_unary_checked("log", |x| if x <= 0.0 { Err(math_domain()) } else { Ok(x.ln()) })),
            (
                "pow",
                math_binary_checked("pow", |x, y| {
                    if x == 0.0 && y < 0.0 {
                        return Err(math_domain());
                    }
                    let result = x.powf(y);
                    if result.is_nan() {
                        return Err(math_domain());
                    }
                    if result.is_infinite() {
                        return Err("math range error".to_string());
                    }
                    Ok(result)
                }),
            ),
            ("min", math_binary("min", |a, b| a.min(b))),
            ("max", math_binary("max", |a, b| a.max(b))),
        ],
    )
}

fn math_domain() -> String {
    "math domain error".to_string()
}

fn math_unary(name: &'static str, apply: fn(f64) -> f64) -> Rc<object::Builtin> {
    math_unary_checked(name, move |x| Ok(apply(x)))
}

fn math_unary_checked<F>(name: &'static str, apply: F) -> Rc<object::Builtin>
where
    F: Fn(f64) -> Result<f64, String> + 'static,
{
    let qualified = format!("math.{}", name);
    let label = qualified.clone();
    object::Builtin::new(&label, move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err(format!("{} does not accept keyword arguments", qualified));
        }
        if arguments.len() != 1 {
            return Err(format!("{} expects 1 argument, got {}", qualified, arguments.len()));
        }
        let number = math_number(&qualified, &arguments[0])?;
        // The `+ 0` normalizes a negative zero, as the Go math module does.
        apply(number).map(|value| Value::Number(value + 0.0)).map_err(|message| format!("{}: {}", qualified, message))
    })
}

fn math_binary(name: &'static str, apply: fn(f64, f64) -> f64) -> Rc<object::Builtin> {
    math_binary_checked(name, move |a, b| Ok(apply(a, b)))
}

fn math_binary_checked<F>(name: &'static str, apply: F) -> Rc<object::Builtin>
where
    F: Fn(f64, f64) -> Result<f64, String> + 'static,
{
    let qualified = format!("math.{}", name);
    let label = qualified.clone();
    object::Builtin::new(&label, move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err(format!("{} does not accept keyword arguments", qualified));
        }
        if arguments.len() != 2 {
            return Err(format!("{} expects 2 arguments, got {}", qualified, arguments.len()));
        }
        let first = math_number(&qualified, &arguments[0])?;
        let second = math_number(&qualified, &arguments[1])?;
        // The `+ 0` normalizes a negative zero, as the Go math module does.
        apply(first, second).map(|value| Value::Number(value + 0.0)).map_err(|message| format!("{}: {}", qualified, message))
    })
}

fn math_number(qualified: &str, value: &Value) -> Result<f64, String> {
    match value {
        Value::Number(number) => Ok(*number),
        other => Err(format!("{} expects a number, got {}", qualified, other.type_name())),
    }
}

/// The default seed for the `random` module's xorshift64* generator.
const DEFAULT_RANDOM_SEED: u64 = 0x2545F4914F6CDD1D;

fn random_next(state: &mut u64) -> u64 {
    let mut value = *state;
    value ^= value >> 12;
    value ^= value << 25;
    value ^= value >> 27;
    *state = value;
    value.wrapping_mul(2685821657736338717)
}

/// A value in [0, 1) built from the top 53 bits, which `f64` holds exactly.
fn random_fraction(state: &mut u64) -> f64 {
    (random_next(state) >> 11) as f64 / (1u64 << 53) as f64
}

/// The `random` module: a reproducible xorshift64* PRNG with no clock dependency.
pub fn random_module() -> Rc<Module> {
    let state = Rc::new(RefCell::new(DEFAULT_RANDOM_SEED));
    let seed_state = state.clone();
    let seed = object::Builtin::new("random.seed", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("random.seed does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("random.seed expects 1 argument, got {}", arguments.len()));
        }
        let Value::Number(number) = &arguments[0] else {
            return Err(format!("random.seed expects a number, got {}", arguments[0].type_name()));
        };
        if !number.is_finite() {
            return Err(format!("random.seed expects a number, got {}", arguments[0].type_name()));
        }
        let mut value = *number as i64 as u64;
        if value == 0 {
            value = DEFAULT_RANDOM_SEED;
        }
        *seed_state.borrow_mut() = value;
        Ok(Value::Null)
    });
    let random_state = state.clone();
    let random = object::Builtin::new("random.random", move |_host, arguments, keywords| {
        if !arguments.is_empty() || !keywords.is_empty() {
            return Err(format!("random.random expects 0 arguments, got {}", arguments.len()));
        }
        Ok(Value::Number(random_fraction(&mut random_state.borrow_mut())))
    });
    let int_state = state.clone();
    let int = object::Builtin::new("random.int", move |_host, arguments, keywords| {
        let (low, high) = random_range("random.int", arguments, keywords)?;
        let width = (high - low) as u64 + 1;
        let offset = random_next(&mut int_state.borrow_mut()) % width;
        Ok(Value::Number((low + offset as i64) as f64))
    });
    let uniform_state = state.clone();
    let uniform = object::Builtin::new("random.uniform", move |_host, arguments, keywords| {
        let (low, high) = random_float_range("random.uniform", arguments, keywords)?;
        let fraction = random_fraction(&mut uniform_state.borrow_mut());
        Ok(Value::Number(low + fraction * (high - low)))
    });
    let choice_state = state.clone();
    let choice = object::Builtin::new("random.choice", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("random.choice does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("random.choice expects 1 argument, got {}", arguments.len()));
        }
        let Value::List(list) = &arguments[0] else {
            return Err(format!("random.choice expects a list, got {}", arguments[0].type_name()));
        };
        let list = list.borrow();
        if list.is_empty() {
            return Err("random.choice cannot choose from an empty list".to_string());
        }
        let index = (random_next(&mut choice_state.borrow_mut()) % list.len() as u64) as usize;
        Ok(list[index].clone())
    });
    let shuffle_state = state.clone();
    let shuffle = object::Builtin::new("random.shuffle", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("random.shuffle does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("random.shuffle expects 1 argument, got {}", arguments.len()));
        }
        let Value::List(list) = &arguments[0] else {
            return Err(format!("random.shuffle expects a list, got {}", arguments[0].type_name()));
        };
        let mut list = list.borrow_mut();
        let mut state = shuffle_state.borrow_mut();
        for index in (1..list.len()).rev() {
            let swap = (random_next(&mut state) % (index as u64 + 1)) as usize;
            list.swap(index, swap);
        }
        Ok(Value::Null)
    });
    module("random", &[("seed", seed), ("random", random), ("int", int), ("uniform", uniform), ("choice", choice), ("shuffle", shuffle)])
}

fn random_range(name: &str, arguments: &[Value], keywords: &[tiger_object::Keyword]) -> Result<(i64, i64), String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.len() != 2 {
        return Err(format!("{} expects 2 arguments, got {}", name, arguments.len()));
    }
    let low = random_integer(name, "low", &arguments[0])?;
    let high = random_integer(name, "high", &arguments[1])?;
    if high < low {
        return Err(format!("{} expects low <= high, got {} and {}", name, low, high));
    }
    Ok((low, high))
}

fn random_float_range(name: &str, arguments: &[Value], keywords: &[tiger_object::Keyword]) -> Result<(f64, f64), String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.len() != 2 {
        return Err(format!("{} expects 2 arguments, got {}", name, arguments.len()));
    }
    let low = random_number(name, "low", &arguments[0])?;
    let high = random_number(name, "high", &arguments[1])?;
    if high < low {
        return Err(format!("{} expects low <= high, got {} and {}", name, tiger_format::number_g(low), tiger_format::number_g(high)));
    }
    Ok((low, high))
}

fn random_integer(name: &str, role: &str, value: &Value) -> Result<i64, String> {
    let number = random_number(name, role, value)?;
    if number.trunc() != number || number.abs() > 9007199254740991.0 {
        return Err(format!("{} {} must be a safe integer", name, role));
    }
    Ok(number as i64)
}

fn random_number(name: &str, role: &str, value: &Value) -> Result<f64, String> {
    match value {
        Value::Number(number) if !number.is_nan() && !number.is_infinite() => Ok(*number),
        other => Err(format!("{} {} must be a number, got {}", name, role, other.type_name())),
    }
}

/// The error the browser build reports for the whole `secrets` module.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
const SECRETS_UNAVAILABLE: &str = "the secrets module is not available in the browser build; use the native CLI or the WASI target";

/// Seeds the `secrets` generator from host entropy, so two runs never match.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn secrets_seed() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(host_nanoseconds());
    hasher.write_u32(std::process::id());
    let seed = hasher.finish();
    if seed == 0 {
        DEFAULT_RANDOM_SEED
    } else {
        seed
    }
}

/// The browser build has no host entropy, so the module keeps its surface and
/// reports the documented error, exactly as `time` and `regex` do.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn secrets_seed() -> u64 {
    0
}

/// A nanosecond reading from the host clock.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn host_nanoseconds() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|elapsed| elapsed.as_nanos() as u64).unwrap_or(0)
}

/// Advances the generator, mixing in a fresh clock reading so successive draws
/// differ even when two runs begin at the same instant.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn secrets_next(state: &mut u64) -> Result<u64, String> {
    *state ^= host_nanoseconds().rotate_left(21) ^ (std::process::id() as u64);
    if *state == 0 {
        *state = DEFAULT_RANDOM_SEED;
    }
    Ok(random_next(state))
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn secrets_next(_state: &mut u64) -> Result<u64, String> {
    Err(SECRETS_UNAVAILABLE.to_string())
}

/// A value in [0, 1) built from the top 53 bits, which `f64` holds exactly.
fn secrets_fraction(value: u64) -> f64 {
    (value >> 11) as f64 / (1u64 << 53) as f64
}

/// The `secrets` module: non-reproducible randomness seeded from the host, so a
/// program never has to call `seed`.
pub fn secrets_module() -> Rc<Module> {
    let state = Rc::new(RefCell::new(secrets_seed()));
    let random_state = state.clone();
    let random = object::Builtin::new("secrets.random", move |_host, arguments, keywords| {
        no_args("secrets.random", arguments, keywords)?;
        Ok(Value::Number(secrets_fraction(secrets_next(&mut random_state.borrow_mut())?)))
    });
    let int_state = state.clone();
    let int = object::Builtin::new("secrets.int", move |_host, arguments, keywords| {
        let (low, high) = random_range("secrets.int", arguments, keywords)?;
        let width = (high - low) as u64 + 1;
        let offset = secrets_next(&mut int_state.borrow_mut())? % width;
        Ok(Value::Number((low + offset as i64) as f64))
    });
    let uniform_state = state.clone();
    let uniform = object::Builtin::new("secrets.uniform", move |_host, arguments, keywords| {
        let (low, high) = random_float_range("secrets.uniform", arguments, keywords)?;
        let fraction = secrets_fraction(secrets_next(&mut uniform_state.borrow_mut())?);
        Ok(Value::Number(low + fraction * (high - low)))
    });
    let choice_state = state.clone();
    let choice = object::Builtin::new("secrets.choice", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("secrets.choice does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("secrets.choice expects 1 argument, got {}", arguments.len()));
        }
        let Value::List(list) = &arguments[0] else {
            return Err(format!("secrets.choice expects a list, got {}", arguments[0].type_name()));
        };
        let list = list.borrow();
        if list.is_empty() {
            return Err("secrets.choice cannot choose from an empty list".to_string());
        }
        let index = (secrets_next(&mut choice_state.borrow_mut())? % list.len() as u64) as usize;
        Ok(list[index].clone())
    });
    let shuffle_state = state.clone();
    let shuffle = object::Builtin::new("secrets.shuffle", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("secrets.shuffle does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("secrets.shuffle expects 1 argument, got {}", arguments.len()));
        }
        let Value::List(list) = &arguments[0] else {
            return Err(format!("secrets.shuffle expects a list, got {}", arguments[0].type_name()));
        };
        let mut list = list.borrow_mut();
        let mut state = shuffle_state.borrow_mut();
        for index in (1..list.len()).rev() {
            let swap = (secrets_next(&mut state)? % (index as u64 + 1)) as usize;
            list.swap(index, swap);
        }
        Ok(Value::Null)
    });
    module("secrets", &[("random", random), ("int", int), ("uniform", uniform), ("choice", choice), ("shuffle", shuffle)])
}

/// The `json` module: `parse` and `stringify`.
pub fn json_module() -> Rc<Module> {
    let parse = object::Builtin::new("json.parse", |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("json.parse does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("json.parse expects 1 argument, got {}", arguments.len()));
        }
        let Value::String(text) = &arguments[0] else {
            return Err(format!("json.parse expects a string, got {}", arguments[0].type_name()));
        };
        parse_json(text)
    });
    let stringify = object::Builtin::new("json.stringify", |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("json.stringify does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("json.stringify expects 1 argument, got {}", arguments.len()));
        }
        let mut out = String::new();
        write_json(&mut out, &arguments[0], &mut Vec::new())?;
        Ok(Value::string(&out))
    });
    module("json", &[("parse", parse), ("stringify", stringify)])
}

fn parse_json(text: &str) -> Result<Value, String> {
    let mut parser = JsonParser { text: text.chars().collect(), pos: 0 };
    let value = parser.value()?;
    parser.skip_space();
    if parser.pos != parser.text.len() {
        return Err(parser.error("unexpected trailing content"));
    }
    Ok(value)
}

struct JsonParser {
    text: Vec<char>,
    pos: usize,
}

impl JsonParser {
    fn error(&self, message: &str) -> String {
        format!("json.parse at position {}: {}", self.pos, message)
    }

    fn skip_space(&mut self) {
        while self.pos < self.text.len() && matches!(self.text[self.pos], ' ' | '\t' | '\n' | '\r') {
            self.pos += 1;
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.skip_space();
        let Some(ch) = self.text.get(self.pos).copied() else {
            return Err(self.error("unexpected end of input"));
        };
        match ch {
            '{' => self.object(),
            '[' => self.array(),
            '"' => Ok(Value::string(&self.string()?)),
            't' => self.literal("true", Value::Bool(true)),
            'f' => self.literal("false", Value::Bool(false)),
            'n' => self.literal("null", Value::Null),
            '-' | '0'..='9' => self.number(),
            other => Err(self.error(&format!("unexpected character {:?}", other.to_string()))),
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, String> {
        for expected in word.chars() {
            if self.text.get(self.pos) != Some(&expected) {
                return Err(self.error(&format!("invalid literal, expected {:?}", word)));
            }
            self.pos += 1;
        }
        Ok(value)
    }

    fn object(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let dict = Rc::new(RefCell::new(object::Dict::default()));
        self.skip_space();
        if self.text.get(self.pos) == Some(&'}') {
            self.pos += 1;
            return Ok(Value::Dict(dict));
        }
        loop {
            self.skip_space();
            let key = self.string()?;
            self.skip_space();
            if self.text.get(self.pos) != Some(&':') {
                return Err(self.error("expected ':'"));
            }
            self.pos += 1;
            let value = self.value()?;
            dict.borrow_mut().set(Value::string(&key), value).map_err(|message| self.error(&message))?;
            self.skip_space();
            match self.text.get(self.pos) {
                Some(',') => self.pos += 1,
                Some('}') => {
                    self.pos += 1;
                    return Ok(Value::Dict(dict));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.pos += 1;
        let mut elements = Vec::new();
        self.skip_space();
        if self.text.get(self.pos) == Some(&']') {
            self.pos += 1;
            return Ok(Value::list(elements));
        }
        loop {
            elements.push(self.value()?);
            self.skip_space();
            match self.text.get(self.pos) {
                Some(',') => self.pos += 1,
                Some(']') => {
                    self.pos += 1;
                    return Ok(Value::list(elements));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        if self.text.get(self.pos) != Some(&'"') {
            return Err(self.error("expected a string"));
        }
        self.pos += 1;
        let mut out = String::new();
        loop {
            let Some(ch) = self.text.get(self.pos).copied() else {
                return Err(self.error("unterminated string"));
            };
            self.pos += 1;
            match ch {
                '"' => return Ok(out),
                '\\' => {
                    let Some(escape) = self.text.get(self.pos).copied() else {
                        return Err(self.error("unterminated escape"));
                    };
                    self.pos += 1;
                    match escape {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        '/' => out.push('/'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{C}'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'u' => {
                            let mut code = 0u32;
                            for _ in 0..4 {
                                let digit = self.text.get(self.pos).and_then(|c| c.to_digit(16));
                                let Some(digit) = digit else {
                                    return Err(self.error("invalid unicode escape"));
                                };
                                code = code * 16 + digit;
                                self.pos += 1;
                            }
                            out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(self.error("invalid escape")),
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.text.get(self.pos) == Some(&'-') {
            self.pos += 1;
        }
        while matches!(self.text.get(self.pos), Some('0'..='9')) {
            self.pos += 1;
        }
        if self.text.get(self.pos) == Some(&'.') {
            self.pos += 1;
            while matches!(self.text.get(self.pos), Some('0'..='9')) {
                self.pos += 1;
            }
        }
        if matches!(self.text.get(self.pos), Some('e') | Some('E')) {
            self.pos += 1;
            if matches!(self.text.get(self.pos), Some('+') | Some('-')) {
                self.pos += 1;
            }
            while matches!(self.text.get(self.pos), Some('0'..='9')) {
                self.pos += 1;
            }
        }
        let text: String = self.text[start..self.pos].iter().collect();
        match text.parse::<f64>() {
            Ok(number) if !number.is_infinite() && !number.is_nan() => Ok(Value::Number(number)),
            _ => Err(self.error(&format!("invalid or out-of-range number {:?}", text))),
        }
    }
}

/// Serializes a value to JSON, rejecting cyclic structures and non-string keys.
fn write_json(out: &mut String, value: &Value, seen: &mut Vec<usize>) -> Result<(), String> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => {
            if !number.is_finite() {
                return Err("json.stringify cannot serialize a non-finite number".to_string());
            }
            out.push_str(&tiger_format::number(*number));
        }
        Value::String(text) => write_json_string(out, text),
        Value::List(elements) => {
            let pointer = Rc::as_ptr(elements) as usize;
            if seen.contains(&pointer) {
                return Err("json.stringify cannot serialize a cyclic structure".to_string());
            }
            seen.push(pointer);
            out.push('[');
            let items = elements.borrow().clone();
            for (index, element) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json(out, element, seen)?;
            }
            out.push(']');
            seen.pop();
        }
        Value::Tuple(elements) => {
            // A tuple has no JSON shape of its own, so it serializes as an array,
            // as in Python; a set has no order to preserve and is rejected below.
            out.push('[');
            for (index, element) in elements.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json(out, element, seen)?;
            }
            out.push(']');
        }
        Value::Dict(dict) => {
            let pointer = Rc::as_ptr(dict) as usize;
            if seen.contains(&pointer) {
                return Err("json.stringify cannot serialize a cyclic structure".to_string());
            }
            seen.push(pointer);
            out.push('{');
            let entries = dict.borrow().entries.clone();
            for (index, (key, value)) in entries.iter().enumerate() {
                let Value::String(key) = key else {
                    return Err(format!("json.stringify requires string keys, got {}", key.type_name()));
                };
                if index > 0 {
                    out.push(',');
                }
                write_json_string(out, key);
                out.push(':');
                write_json(out, value, seen)?;
            }
            out.push('}');
            seen.pop();
        }
        other => return Err(format!("json.stringify cannot serialize {}", other.type_name())),
    }
    Ok(())
}

fn write_json_string(out: &mut String, text: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other if (other as u32) < 0x20 => {
                let code = other as u32;
                out.push_str("\\u00");
                out.push(HEX[((code >> 4) & 0xF) as usize] as char);
                out.push(HEX[(code & 0xF) as usize] as char);
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// The `time` module: the clock, sleeping, and UTC formatting.
pub fn time_module() -> Rc<Module> {
    let now = object::Builtin::new("time.now", |_host, arguments, keywords| {
        no_args("time.now", arguments, keywords)?;
        Ok(Value::Number(time_now()?))
    });
    let now_ms = object::Builtin::new("time.now_ms", |_host, arguments, keywords| {
        no_args("time.now_ms", arguments, keywords)?;
        Ok(Value::Number(time_now_ms()?))
    });
    let sleep = object::Builtin::new("time.sleep", |_host, arguments, keywords| {
        let seconds = timestamp_argument("time.sleep", arguments, keywords)?;
        if seconds < 0.0 {
            return Err("time.sleep expects a non-negative number of seconds".to_string());
        }
        time_sleep(seconds)?;
        Ok(Value::Null)
    });
    let iso = object::Builtin::new("time.iso", |_host, arguments, keywords| {
        let seconds = timestamp_argument("time.iso", arguments, keywords)?;
        Ok(Value::string(&time_iso(seconds)?))
    });
    let parts = object::Builtin::new("time.parts", |_host, arguments, keywords| {
        let seconds = timestamp_argument("time.parts", arguments, keywords)?;
        time_parts_value(seconds)
    });
    module("time", &[("now", now), ("now_ms", now_ms), ("sleep", sleep), ("iso", iso), ("parts", parts)])
}

/// The error the browser build reports for the whole `time` module.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
const TIME_UNAVAILABLE: &str = "the time module is not available in the browser build; use the native CLI or the WASI target";

/// The host clock: seconds since the Unix epoch.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn time_now() -> Result<f64, String> {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(elapsed) => Ok(elapsed.as_secs_f64()),
        Err(_) => Ok(0.0),
    }
}

/// The host clock: milliseconds since the Unix epoch.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn time_now_ms() -> Result<f64, String> {
    Ok((time_now()? * 1000.0).floor())
}

/// Suspends the host for a number of seconds.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn time_sleep(seconds: f64) -> Result<(), String> {
    std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
    Ok(())
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn time_iso(seconds: f64) -> Result<String, String> {
    Ok(iso_utc(seconds))
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn time_parts_value(seconds: f64) -> Result<Value, String> {
    Ok(time_parts(seconds))
}

/// The browser build has no clock, so the module's surface stays visible with a
/// clear error instead of the panic `SystemTime::now` raises there. Go's
/// `time_js.go` omits `iso` and `parts` for the same reason.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn time_now() -> Result<f64, String> {
    Err(TIME_UNAVAILABLE.to_string())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn time_now_ms() -> Result<f64, String> {
    Err(TIME_UNAVAILABLE.to_string())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn time_sleep(_seconds: f64) -> Result<(), String> {
    Err(TIME_UNAVAILABLE.to_string())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn time_iso(_seconds: f64) -> Result<String, String> {
    Err(TIME_UNAVAILABLE.to_string())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn time_parts_value(_seconds: f64) -> Result<Value, String> {
    Err(TIME_UNAVAILABLE.to_string())
}

fn no_args(name: &str, arguments: &[Value], keywords: &[tiger_object::Keyword]) -> Result<(), String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if !arguments.is_empty() {
        return Err(format!("{} expects 0 arguments, got {}", name, arguments.len()));
    }
    Ok(())
}

fn timestamp_argument(name: &str, arguments: &[Value], keywords: &[tiger_object::Keyword]) -> Result<f64, String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.len() != 1 {
        return Err(format!("{} expects 1 argument, got {}", name, arguments.len()));
    }
    match &arguments[0] {
        Value::Number(number) if !number.is_nan() && !number.is_infinite() => Ok(*number),
        other => Err(format!("{} expects a number of seconds, got {}", name, other.type_name())),
    }
}

/// Splits a fractional Unix timestamp into whole seconds.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn split_seconds(seconds: f64) -> i64 {
    seconds.floor() as i64
}

/// Converts days since 1970-01-01 to a (year, month, day) civil date.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { y + 1 } else { y }, month, day)
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn iso_utc(seconds: f64) -> String {
    let whole = split_seconds(seconds);
    let days = whole.div_euclid(86400);
    let remainder = whole.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    let hour = remainder / 3600;
    let minute = (remainder % 3600) / 60;
    let second = remainder % 60;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hour, minute, second)
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn time_parts(seconds: f64) -> Value {
    let whole = split_seconds(seconds);
    let days = whole.div_euclid(86400);
    let remainder = whole.rem_euclid(86400);
    let (year, month, day) = civil_from_days(days);
    let hour = remainder / 3600;
    let minute = (remainder % 3600) / 60;
    let second = remainder % 60;
    let weekday = (days + 4).rem_euclid(7);
    let start_of_year = days_from_civil(year, 1, 1);
    let yearday = days - start_of_year + 1;
    let dict = Rc::new(RefCell::new(object::Dict::default()));
    let fields: [(&str, f64); 8] = [
        ("year", year as f64),
        ("month", month as f64),
        ("day", day as f64),
        ("hour", hour as f64),
        ("minute", minute as f64),
        ("second", second as f64),
        ("weekday", weekday as f64),
        ("yearday", yearday as f64),
    ];
    for (name, value) in fields {
        let _ = dict.borrow_mut().set(Value::string(name), Value::Number(value));
    }
    Value::Dict(dict)
}

/// Converts a civil date to days since 1970-01-01.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// The process environment a program reads through `sys.env`.
///
/// The browser build has no environment to read, so it reports none. The gate
/// keeps `std::env::vars` out of `tiger.wasm`, the way the `time` and `regex`
/// stubs keep the host clock and the RE2 engine out.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn environment_variables() -> Vec<(String, String)> {
    std::env::vars().collect()
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn environment_variables() -> Vec<(String, String)> {
    Vec::new()
}

/// The `sys` module: `argv`, `env`, and `exit`.
///
/// `argv` holds the program's arguments with its own path first, exactly as
/// Python's `sys.argv` does; a host that runs a program without a name leaves it
/// empty. `env` is a dictionary of the process environment's variables, and
/// changing that dictionary does not change the process. `exit(status)` stops the
/// program with the status Python's `sys.exit` would use; see [`crate::Exit`].
pub fn sys_module(arguments: &[String]) -> Rc<Module> {
    let env = object::Environment::new(None);
    let argv: Vec<Value> = arguments.iter().map(|argument| Value::string(argument)).collect();
    let _ = object::define(&env, "argv", Value::list(argv), true);
    let variables = Rc::new(RefCell::new(object::Dict::default()));
    for (name, value) in environment_variables() {
        let _ = variables.borrow_mut().set(Value::string(&name), Value::string(&value));
    }
    let _ = object::define(&env, "env", Value::Dict(variables), true);
    let _ = object::define(&env, "exit", Value::Builtin(object::Builtin::higher_order("sys.exit", None)), true);
    Module::new("sys".to_string(), env)
}

/// The `regex` module: regular expressions via the standard RE2 engine.
pub fn regex_module() -> Rc<Module> {
    crate::regexmod::regex_module()
}

/// The `algo` module: a suite of classic algorithms.
pub fn algo_module() -> Rc<Module> {
    crate::algo::algo_module()
}
