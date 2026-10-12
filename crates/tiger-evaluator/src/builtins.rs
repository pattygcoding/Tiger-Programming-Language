//! The global built-in functions and module installation.

use std::rc::Rc;

use tiger_object as object;
use tiger_object::{Env, Value};

use crate::modules;

fn define(env: &Env, name: &str, value: Value) {
    let _ = object::define(env, name, value, true);
}

/// Installs every built-in and module into the top-level scope.
pub(crate) fn install(env: &Env) {
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
        Ok(Value::list((0..count).map(|index| Value::Number((start + index * step) as f64)).collect()))
    })
}

/// Implements `print(*values, end="\n")`.
fn print_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("print", |host, arguments, keywords| {
        let mut ending = "\n".to_string();
        for entry in keywords {
            if entry.name != "end" {
                return Err(format!("print does not accept keyword argument {:?}", entry.name));
            }
            match &entry.value {
                Value::String(text) => ending = text.to_string(),
                other => return Err(format!("print end must be a string, got {}", other.type_name())),
            }
        }
        let parts: Vec<String> = arguments.iter().map(object::format).collect();
        host.output(&format!("{}{}", parts.join(" "), ending))?;
        Ok(Value::Null)
    })
}

fn len_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("len", |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("len does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("len expects 1 argument, got {}", arguments.len()));
        }
        match &arguments[0] {
            Value::String(text) => Ok(Value::Number((if text.is_ascii() { text.len() } else { text.chars().count() }) as f64)),
            Value::List(elements) => Ok(Value::Number(elements.borrow().len() as f64)),
            Value::Dict(dict) => Ok(Value::Number(dict.borrow().entries.len() as f64)),
            other => Err(format!("len does not accept {}", other.type_name())),
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
            object::Builtin::new("sum", |_host, arguments, keywords| {
                let list = list_argument("sum", arguments, keywords)?;
                let mut total = 0.0;
                for (index, element) in list.borrow().iter().enumerate() {
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
        ("min", object::Builtin::new("min", |_host, arguments, keywords| extreme("min", arguments, keywords, -1))),
        ("max", object::Builtin::new("max", |_host, arguments, keywords| extreme("max", arguments, keywords, 1))),
        (
            "any",
            object::Builtin::new("any", |_host, arguments, keywords| {
                let list = list_argument("any", arguments, keywords)?;
                let found = list.borrow().iter().any(object::truthy);
                Ok(Value::Bool(found))
            }),
        ),
        (
            "all",
            object::Builtin::new("all", |_host, arguments, keywords| {
                let list = list_argument("all", arguments, keywords)?;
                let every = list.borrow().iter().all(object::truthy);
                Ok(Value::Bool(every))
            }),
        ),
        (
            "enumerate",
            object::Builtin::new("enumerate", |_host, arguments, keywords| {
                let list = list_argument("enumerate", arguments, keywords)?;
                let elements = list.borrow();
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
            object::Builtin::new("zip", |_host, arguments, keywords| {
                if !keywords.is_empty() {
                    return Err("zip does not accept keyword arguments".to_string());
                }
                if arguments.is_empty() {
                    return Err("zip expects at least 1 argument".to_string());
                }
                let mut lists = Vec::new();
                let mut shortest = usize::MAX;
                for (index, argument) in arguments.iter().enumerate() {
                    let Value::List(list) = argument else {
                        return Err(format!("zip expects lists, got {} at position {}", argument.type_name(), index + 1));
                    };
                    shortest = shortest.min(list.borrow().len());
                    lists.push(list.clone());
                }
                let mut rows = Vec::new();
                for position in 0..shortest {
                    let row: Vec<Value> = lists.iter().map(|list| list.borrow()[position].clone()).collect();
                    rows.push(Value::list(row));
                }
                Ok(Value::list(rows))
            }),
        ),
    ]
}

fn list_argument(
    name: &str,
    arguments: &[Value],
    keywords: &[tiger_object::Keyword],
) -> Result<Rc<std::cell::RefCell<Vec<Value>>>, String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.len() != 1 {
        return Err(format!("{} expects 1 argument, got {}", name, arguments.len()));
    }
    match &arguments[0] {
        Value::List(list) => Ok(list.clone()),
        other => Err(format!("{} expects a list, got {}", name, other.type_name())),
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
        if let Value::List(list) = &arguments[0] {
            return Ok(list.borrow().clone());
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
