//! The global built-in functions and module installation.

use std::rc::Rc;

use tiger_object as object;
use tiger_object::{Env, Value};

use crate::modules;

fn define(env: &Env, name: &str, value: Value) {
    let _ = object::define(env, name, value, true);
}

/// Installs every built-in and module into the top-level scope.
///
/// `arguments` is the running program's `sys.argv`: its own path first, then each
/// argument, which is what the host passes in.
pub(crate) fn install(env: &Env, arguments: &[String]) {
    define(env, "range", Value::Builtin(range_builtin()));
    define(env, "print", Value::Builtin(print_builtin()));
    define(env, "len", Value::Builtin(len_builtin()));
    define(env, "type", Value::Builtin(type_builtin()));
    define(env, "isinstance", Value::Builtin(isinstance_builtin()));
    define(env, "graphemes", Value::Builtin(graphemes_builtin()));
    define(env, "input", Value::Builtin(input_builtin()));
    for name in ["int", "float", "str", "bool"] {
        define(env, name, Value::Builtin(conversion_builtin(name)));
    }
    // The collection constructors. `set()` is the only way to spell an empty set,
    // because `{}` stays an empty dictionary.
    for name in ["list", "tuple", "set"] {
        define(env, name, Value::Builtin(collection_builtin(name)));
    }
    for (name, builtin) in aggregate_builtins() {
        define(env, name, Value::Builtin(builtin));
    }
    for name in ["sorted", "map", "filter"] {
        define(env, name, Value::Builtin(object::Builtin::higher_order(name, None)));
    }
    for (name, builtin) in crate::files::file_builtins() {
        define(env, name, Value::Builtin(builtin));
    }
    define(env, "math", Value::Module(modules::math_module()));
    define(env, "algo", Value::Module(modules::algo_module()));
    define(env, "random", Value::Module(modules::random_module()));
    define(env, "secrets", Value::Module(modules::secrets_module()));
    define(env, "json", Value::Module(modules::json_module()));
    define(env, "time", Value::Module(modules::time_module()));
    define(env, "regex", Value::Module(modules::regex_module()));
    define(env, "sys", Value::Module(modules::sys_module(arguments)));
}

/// Implements `range(start?, stop, step?)`.
fn range_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("range", |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("range does not accept keyword arguments".to_string());
        }
        if arguments.is_empty() || arguments.len() > 3 {
            return Err(format!("range expects 1 to 3 arguments, got {}", arguments.len()));
        }
        let mut values = Vec::new();
        for argument in arguments {
            let Value::Number(number) = argument else {
                return Err("range arguments must be safe integers (absolute value <= 9007199254740991)".to_string());
            };
            if number.is_nan() || number.is_infinite() || number.trunc() != *number || number.abs() > 9007199254740991.0 {
                return Err("range arguments must be safe integers (absolute value <= 9007199254740991)".to_string());
            }
            values.push(*number as i64);
        }
        let (start, stop, step) = match values.len() {
            1 => (0, values[0], 1),
            2 => (values[0], values[1], 1),
            _ => (values[0], values[1], values[2]),
        };
        if step == 0 {
            return Err("range step cannot be zero".to_string());
        }
        let mut count: i64 = 0;
        if step > 0 && start < stop {
            count = (stop - start - 1) / step + 1;
        }
        if step < 0 && start > stop {
            count = (start - stop - 1) / (-step) + 1;
        }
        if count > 1_000_000 {
            return Err("range exceeds 1000000 elements".to_string());
        }
        // A lazy iterator: `for`, `in`, and the reshaping built-ins step it one
        // element at a time, so nothing is materialized unless it is needed.
        Ok(Value::iterator(object::IteratorKind::Range { current: start, stop, step }))
    })
}

/// Implements `print(*values, end="\n")`.
fn print_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("print", |host, arguments, keywords| {
        let mut ending = "\n";
        for entry in keywords {
            if entry.name != "end" {
                return Err(format!("print does not accept keyword argument {:?}", entry.name));
            }
            match &entry.value {
                Value::String(text) => ending = text,
                other => return Err(format!("print end must be a string, got {}", other.type_name())),
            }
        }
        // One buffer for the whole line. `print` is the busiest formatting path in
        // most programs, and a string per argument plus a `Vec`, a join and a
        // `format!` would allocate several times for every call.
        let mut line = String::new();
        for (index, argument) in arguments.iter().enumerate() {
            if index > 0 {
                line.push(' ');
            }
            object::write_format(&mut line, argument);
        }
        line.push_str(ending);
        host.output(&line)?;
        Ok(Value::Null)
    })
}

/// The length `len(value)`, `value.size()`, and `value.length()` all share: a
/// string's Unicode code points, a list's elements, or a dictionary's entries.
///
/// `None` for every other value, because the three spellings are one operation
/// with one domain. A caller reports the rejection with [`length_error`], so no
/// spelling can drift from the others.
pub(crate) fn length_of(value: &Value) -> Option<f64> {
    match value {
        Value::String(text) => Some(code_points(text) as f64),
        Value::List(elements) => Some(elements.borrow().len() as f64),
        Value::Tuple(elements) => Some(elements.len() as f64),
        Value::Set(set) => Some(set.borrow().entries.len() as f64),
        Value::Dict(dict) => Some(dict.borrow().entries.len() as f64),
        _ => None,
    }
}

/// Counts Unicode code points, reading the byte length for ASCII text.
pub(crate) fn code_points(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.chars().count()
    }
}

/// The message a value outside [`length_of`]'s domain produces, labelled with the
/// spelling the program used: `len`, `size`, or `length`.
pub(crate) fn length_error(name: &str, value: &Value) -> String {
    format!("{} does not accept {}", name, value.type_name())
}

fn len_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("len", |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("len does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("len expects 1 argument, got {}", arguments.len()));
        }
        match length_of(&arguments[0]) {
            Some(length) => Ok(Value::Number(length)),
            None => Err(length_error("len", &arguments[0])),
        }
    })
}

fn type_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("type", |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("type does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("type expects 1 argument, got {}", arguments.len()));
        }
        Ok(Value::string(arguments[0].type_name()))
    })
}

const TYPE_NAMES: &[&str] = &[
    "number",
    "string",
    "bool",
    "null",
    "list",
    "tuple",
    "set",
    "dict",
    "function",
    "builtin",
    "class",
    "instance",
    "bound method",
    "super",
    "module",
    "file",
    "error",
    "iterator",
];

fn isinstance_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("isinstance", |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("isinstance does not accept keyword arguments".to_string());
        }
        if arguments.len() != 2 {
            return Err(format!("isinstance expects 2 arguments, got {}", arguments.len()));
        }
        let names = type_name_list(&arguments[1])?;
        Ok(Value::Bool(names.iter().any(|name| *name == arguments[0].type_name())))
    })
}

fn type_name_list(value: &Value) -> Result<Vec<&'static str>, String> {
    match value {
        Value::String(name) => Ok(vec![checked_type_name(name)?]),
        Value::List(elements) => {
            let mut names = Vec::new();
            for element in elements.borrow().iter() {
                match element {
                    Value::String(name) => names.push(checked_type_name(name)?),
                    other => return Err(format!("isinstance: type name must be a string, got {}", other.type_name())),
                }
            }
            Ok(names)
        }
        other => Err(format!("isinstance: expected a string or list of strings, got {}", other.type_name())),
    }
}

fn checked_type_name(name: &str) -> Result<&'static str, String> {
    match TYPE_NAMES.iter().find(|candidate| **candidate == name) {
        Some(found) => Ok(found),
        None => Err(format!("isinstance: unknown type name {:?}", name)),
    }
}

/// Implements `input([prompt])`: writes the prompt without a newline and
/// returns one line of input without its line ending.
fn input_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("input", |host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("input does not accept keyword arguments".to_string());
        }
        if arguments.len() > 1 {
            return Err(format!("input expects at most 1 argument, got {}", arguments.len()));
        }
        let prompt = arguments.first().map(object::format);
        Ok(Value::string(&host.input_line(prompt.as_deref())?))
    })
}

/// Implements `int`, `float`, `str`, and `bool`.
fn conversion_builtin(name: &'static str) -> Rc<object::Builtin> {
    object::Builtin::new(name, move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err(format!("{} does not accept keyword arguments", name));
        }
        match arguments.len() {
            0 => Ok(match name {
                "str" => Value::string(""),
                "bool" => Value::Bool(false),
                _ => Value::Number(0.0),
            }),
            1 => crate::methods::convert(name, &arguments[0]),
            count => Err(format!("{} expects at most 1 argument, got {}", name, count)),
        }
    })
}

/// Implements `list(iterable?)`, `tuple(iterable?)`, and `set(iterable?)`.
///
/// Each constructor copies the values any `for` loop could walk: a list, a tuple,
/// a set, a dictionary's keys, or a string's characters. `set()` with no argument
/// is how an empty set is written, because `{}` stays an empty dictionary, and an
/// unhashable element is rejected the way a dictionary key would be.
fn collection_builtin(name: &'static str) -> Rc<object::Builtin> {
    object::Builtin::consuming(name, move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err(format!("{} does not accept keyword arguments", name));
        }
        if arguments.len() > 1 {
            return Err(format!("{} expects at most 1 argument, got {}", name, arguments.len()));
        }
        let elements = match arguments.first() {
            Some(value) => crate::iterable_elements(value).map_err(|kind| format!("{} expects an iterable, got {}", name, kind))?,
            None => Vec::new(),
        };
        match name {
            "list" => Ok(Value::list(elements)),
            "tuple" => Ok(Value::tuple(elements)),
            _ => Value::set(elements),
        }
    })
}

/// Implements `graphemes(value)`: an approximate grapheme-cluster split.
fn graphemes_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("graphemes", |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("graphemes does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("graphemes expects 1 argument, got {}", arguments.len()));
        }
        match &arguments[0] {
            Value::String(text) => Ok(Value::list(split_graphemes(text).iter().map(|cluster| Value::string(cluster)).collect())),
            other => Err(format!("graphemes expects a string, got {}", other.type_name())),
        }
    })
}

const ZERO_WIDTH_JOINER: char = '\u{200D}';

/// Groups a base character with its combining marks, variation selectors, emoji
/// modifiers, and ZWJ continuations, and pairs regional indicators into flags.
fn split_graphemes(text: &str) -> Vec<String> {
    let mut clusters: Vec<String> = Vec::new();
    let mut current: Vec<char> = Vec::new();
    let mut join = false;
    for ch in text.chars() {
        let separate = if current.is_empty() {
            false
        } else if join {
            join = false;
            false
        } else if extends_grapheme(ch) {
            false
        } else if is_regional_indicator(ch) && current.len() == 1 && is_regional_indicator(current[0]) {
            false
        } else {
            !(ch == '\n' && *current.last().unwrap() == '\r')
        };
        if separate {
            clusters.push(current.iter().collect());
            current.clear();
        }
        current.push(ch);
        if ch == ZERO_WIDTH_JOINER {
            join = true;
        }
    }
    if !current.is_empty() {
        clusters.push(current.iter().collect());
    }
    clusters
}

/// Reports whether `ch` continues the preceding cluster without starting one:
/// a combining mark, variation selector, or emoji skin-tone modifier.
fn extends_grapheme(ch: char) -> bool {
    if ch == ZERO_WIDTH_JOINER {
        return true;
    }
    if ('\u{FE00}'..='\u{FE0F}').contains(&ch) || ('\u{E0100}'..='\u{E01EF}').contains(&ch) {
        return true;
    }
    if ('\u{1F3FB}'..='\u{1F3FF}').contains(&ch) {
        return true;
    }
    matches!(ch,
        '\u{0300}'..='\u{036F}' | '\u{0483}'..='\u{0489}' | '\u{0591}'..='\u{05BD}'
        | '\u{0610}'..='\u{061A}' | '\u{064B}'..='\u{065F}' | '\u{0670}'
        | '\u{06D6}'..='\u{06DC}' | '\u{06DF}'..='\u{06E4}' | '\u{06E7}'..='\u{06E8}'
        | '\u{06EA}'..='\u{06ED}' | '\u{0711}' | '\u{0730}'..='\u{074A}'
        | '\u{07A6}'..='\u{07B0}' | '\u{07EB}'..='\u{07F3}' | '\u{0900}'..='\u{0903}'
        | '\u{093A}'..='\u{093C}' | '\u{0941}'..='\u{0948}' | '\u{094D}'
        | '\u{0951}'..='\u{0957}' | '\u{0962}'..='\u{0963}' | '\u{0E31}'
        | '\u{0E34}'..='\u{0E3A}' | '\u{0E47}'..='\u{0E4E}'
        | '\u{0F71}'..='\u{0F84}' | '\u{135D}'..='\u{135F}' | '\u{17B4}'..='\u{17D3}'
        | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}' | '\u{20D0}'..='\u{20F0}'
        | '\u{2CEF}'..='\u{2CF1}' | '\u{2DE0}'..='\u{2DFF}' | '\u{302A}'..='\u{302F}'
        | '\u{3099}'..='\u{309A}' | '\u{A66F}' | '\u{A674}'..='\u{A67D}'
        | '\u{A69E}'..='\u{A69F}' | '\u{A6F0}'..='\u{A6F1}' | '\u{A8E0}'..='\u{A8F1}'
        | '\u{A926}'..='\u{A92D}' | '\u{A947}'..='\u{A951}' | '\u{A980}'..='\u{A982}'
        | '\u{A9B3}' | '\u{AAB0}' | '\u{AAB2}'..='\u{AAB4}' | '\u{FB1E}'
        | '\u{FE20}'..='\u{FE2F}' | '\u{101FD}' | '\u{102E0}'
        | '\u{1D165}'..='\u{1D169}' | '\u{1D16D}'..='\u{1D172}'
        | '\u{1D17B}'..='\u{1D182}' | '\u{1D185}'..='\u{1D18B}'
        | '\u{1D1AA}'..='\u{1D1AD}' | '\u{1E8D0}'..='\u{1E8D6}'
        | '\u{1E944}'..='\u{1E94A}'
    )
}

fn is_regional_indicator(ch: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&ch)
}

/// The reduction and reshaping built-ins that do not call back into user code.
fn aggregate_builtins() -> Vec<(&'static str, Rc<object::Builtin>)> {
    vec![
        (
            "sum",
            object::Builtin::consuming("sum", |_host, arguments, keywords| {
                let elements = sequence_argument("sum", arguments, keywords)?;
                let mut total = 0.0;
                for (index, element) in elements.iter().enumerate() {
                    let Value::Number(number) = element else {
                        return Err(format!("sum expects numbers, got {} at index {}", element.type_name(), index));
                    };
                    total += number;
                }
                if total.is_infinite() || total.is_nan() {
                    return Err("non-finite numeric result".to_string());
                }
                Ok(Value::Number(total))
            }),
        ),
        ("min", object::Builtin::consuming("min", |_host, arguments, keywords| extreme("min", arguments, keywords, -1))),
        ("max", object::Builtin::consuming("max", |_host, arguments, keywords| extreme("max", arguments, keywords, 1))),
        (
            "any",
            object::Builtin::consuming("any", |_host, arguments, keywords| {
                let elements = sequence_argument("any", arguments, keywords)?;
                let found = elements.iter().any(object::truthy);
                Ok(Value::Bool(found))
            }),
        ),
        (
            "all",
            object::Builtin::consuming("all", |_host, arguments, keywords| {
                let elements = sequence_argument("all", arguments, keywords)?;
                let every = elements.iter().all(object::truthy);
                Ok(Value::Bool(every))
            }),
        ),
        (
            "enumerate",
            object::Builtin::consuming("enumerate", |_host, arguments, keywords| {
                let elements = sequence_argument("enumerate", arguments, keywords)?;
                Ok(Value::list(
                    elements
                        .iter()
                        .enumerate()
                        .map(|(index, element)| Value::list(vec![Value::Number(index as f64), element.clone()]))
                        .collect(),
                ))
            }),
        ),
        (
            "zip",
            object::Builtin::consuming("zip", |_host, arguments, keywords| {
                if !keywords.is_empty() {
                    return Err("zip does not accept keyword arguments".to_string());
                }
                if arguments.is_empty() {
                    return Err("zip expects at least 1 argument".to_string());
                }
                let mut lists: Vec<Vec<Value>> = Vec::new();
                let mut shortest = usize::MAX;
                for (index, argument) in arguments.iter().enumerate() {
                    let Some(elements) = argument.sequence() else {
                        return Err(format!("zip expects lists or tuples, got {} at position {}", argument.type_name(), index + 1));
                    };
                    shortest = shortest.min(elements.len());
                    lists.push(elements);
                }
                let mut rows = Vec::new();
                for position in 0..shortest {
                    let row: Vec<Value> = lists.iter().map(|list| list[position].clone()).collect();
                    rows.push(Value::list(row));
                }
                Ok(Value::list(rows))
            }),
        ),
    ]
}

fn sequence_argument(name: &str, arguments: &[Value], keywords: &[tiger_object::Keyword]) -> Result<Vec<Value>, String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.len() != 1 {
        return Err(format!("{} expects 1 argument, got {}", name, arguments.len()));
    }
    match arguments[0].sequence() {
        Some(elements) => Ok(elements),
        None => Err(format!("{} expects a list or tuple, got {}", name, arguments[0].type_name())),
    }
}

/// Picks the smallest (direction -1) or largest (direction 1) value from a
/// single list or from several positional arguments.
fn extreme(name: &str, arguments: &[Value], keywords: &[tiger_object::Keyword], direction: i32) -> Result<Value, String> {
    let values = sequence_arguments(name, arguments, keywords)?;
    if values.is_empty() {
        return Err(format!("{} of an empty sequence", name));
    }
    let mut best = values[0].clone();
    for value in &values[1..] {
        let order = order(value, &best).map_err(|message| format!("{}: {}", name, message))?;
        if order * direction > 0 {
            best = value.clone();
        }
    }
    Ok(best)
}

fn sequence_arguments(name: &str, arguments: &[Value], keywords: &[tiger_object::Keyword]) -> Result<Vec<Value>, String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.is_empty() {
        return Err(format!("{} expects at least 1 argument", name));
    }
    if arguments.len() == 1 {
        if let Some(elements) = arguments[0].sequence() {
            return Ok(elements);
        }
    }
    Ok(arguments.to_vec())
}

/// Returns -1, 0, or 1 for two numbers or two strings.
fn order(left: &Value, right: &Value) -> Result<i32, String> {
    match (left, right) {
        (Value::Number(first), Value::Number(second)) => Ok(if first < second {
            -1
        } else if first > second {
            1
        } else {
            0
        }),
        (Value::String(first), Value::String(second)) => Ok(if first < second {
            -1
        } else if first > second {
            1
        } else {
            0
        }),
        _ => Err(format!("cannot compare {} and {}", left.type_name(), right.type_name())),
    }
}
