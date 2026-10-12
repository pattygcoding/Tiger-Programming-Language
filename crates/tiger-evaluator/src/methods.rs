//! String, list, and dictionary member methods, plus the `To*` conversions.

use std::cell::RefCell;
use std::rc::Rc;

use tiger_object as object;
use tiger_object::{Keyword, Value};

/// Errors unless the call has no arguments.
pub(crate) fn no_arguments(name: &str, arguments: &[Value], keywords: &[Keyword]) -> Result<(), String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if !arguments.is_empty() {
        return Err(format!("{} expects 0 arguments, got {}", name, arguments.len()));
    }
    Ok(())
}

fn string_argument(name: &str, role: &str, value: &Value) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(text.to_string()),
        other => Err(format!("{} {} must be a string, got {}", name, role, other.type_name())),
    }
}

/// Builds the string member method named `name`, or `None`.
pub(crate) fn string_method(name: &str, receiver: Rc<str>) -> Option<Value> {
    match name {
        "upper" => Some(transform(name, receiver, |text| text.to_uppercase())),
        "lower" => Some(transform(name, receiver, |text| text.to_lowercase())),
        "trim" => Some(transform(name, receiver, |text| text.trim().to_string())),
        "split" => Some(split_method(receiver)),
        "replace" => Some(replace_method(receiver)),
        "startswith" => Some(affix_method(name, "prefix", receiver, |text, affix| text.starts_with(affix))),
        "endswith" => Some(affix_method(name, "suffix", receiver, |text, affix| text.ends_with(affix))),
        "find" => Some(find_method(receiver)),
        "padStart" => Some(pad_method(name, receiver, true)),
        "padEnd" => Some(pad_method(name, receiver, false)),
        _ => None,
    }
}

fn transform(name: &str, receiver: Rc<str>, apply: fn(&str) -> String) -> Value {
    let label = name.to_string();
    Value::Builtin(object::Builtin::new(name, move |_host, arguments, keywords| {
        no_arguments(&label, arguments, keywords)?;
        Ok(Value::string(&apply(&receiver)))
    }))
}

/// Mirrors Python's `str.split`: no argument folds whitespace runs; an explicit
/// separator splits exactly and keeps empty fields.
fn split_method(receiver: Rc<str>) -> Value {
    Value::Builtin(object::Builtin::new("split", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("split does not accept keyword arguments".to_string());
        }
        let parts: Vec<String> = match arguments.len() {
            0 => receiver.split_whitespace().map(str::to_string).collect(),
            1 => {
                let separator = string_argument("split", "separator", &arguments[0])?;
                if separator.is_empty() {
                    return Err("split: empty separator".to_string());
                }
                receiver.split(separator.as_str()).map(str::to_string).collect()
            }
            count => return Err(format!("split expects at most 1 argument, got {}", count)),
        };
        Ok(Value::list(parts.iter().map(|part| Value::string(part)).collect()))
    }))
}

fn replace_method(receiver: Rc<str>) -> Value {
    Value::Builtin(object::Builtin::new("replace", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("replace does not accept keyword arguments".to_string());
        }
        if arguments.len() != 2 {
            return Err(format!("replace expects 2 arguments, got {}", arguments.len()));
        }
        let old = string_argument("replace", "old", &arguments[0])?;
        let new = string_argument("replace", "new", &arguments[1])?;
        Ok(Value::string(&receiver.replace(old.as_str(), new.as_str())))
    }))
}

fn affix_method(name: &str, role: &str, receiver: Rc<str>, predicate: fn(&str, &str) -> bool) -> Value {
    let label = name.to_string();
    let role = role.to_string();
    Value::Builtin(object::Builtin::new(name, move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err(format!("{} does not accept keyword arguments", label));
        }
        if arguments.len() != 1 {
            return Err(format!("{} expects 1 argument, got {}", label, arguments.len()));
        }
        let affix = string_argument(&label, &role, &arguments[0])?;
        Ok(Value::Bool(predicate(&receiver, &affix)))
    }))
}

/// Returns the code-point index of the first occurrence, or -1.
fn find_method(receiver: Rc<str>) -> Value {
    Value::Builtin(object::Builtin::new("find", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("find does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("find expects 1 argument, got {}", arguments.len()));
        }
        let needle = string_argument("find", "argument", &arguments[0])?;
        match receiver.find(needle.as_str()) {
            Some(index) => Ok(Value::Number(receiver[..index].chars().count() as f64)),
            None => Ok(Value::Number(-1.0)),
        }
    }))
}

const MAX_PAD_WIDTH: f64 = 1_000_000.0;

/// Implements `padStart`/`padEnd(width, fill=" ")`; fill must be one code point.
fn pad_method(name: &str, receiver: Rc<str>, start: bool) -> Value {
    let label = name.to_string();
    Value::Builtin(object::Builtin::new(name, move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err(format!("{} does not accept keyword arguments", label));
        }
        if arguments.is_empty() || arguments.len() > 2 {
            return Err(format!("{} expects 1 or 2 arguments, got {}", label, arguments.len()));
        }
        let width = pad_width(&label, &arguments[0])?;
        let mut fill = " ".to_string();
        if arguments.len() == 2 {
            fill = string_argument(&label, "fill", &arguments[1])?;
            if fill.chars().count() != 1 {
                return Err(format!("{} fill must be a single character", label));
            }
        }
        let length = receiver.chars().count();
        if width <= length {
            return Ok(Value::string(&receiver));
        }
        let padding: String = fill.repeat(width - length);
        if start {
            Ok(Value::string(&format!("{}{}", padding, receiver)))
        } else {
            Ok(Value::string(&format!("{}{}", receiver, padding)))
        }
    }))
}

fn pad_width(name: &str, value: &Value) -> Result<usize, String> {
    let Value::Number(number) = value else {
        return Err(format!("{} width must be an integer", name));
    };
    if number.is_nan() || number.is_infinite() || number.trunc() != *number {
        return Err(format!("{} width must be an integer", name));
    }
    if *number < 0.0 {
        return Err(format!("{} width must not be negative", name));
    }
    if *number > MAX_PAD_WIDTH {
        return Err(format!("{} width must be at most {}", name, MAX_PAD_WIDTH as i64));
    }
    Ok(*number as usize)
}

/// Builds the list member method named `name`, or `None`. `sort` is handled by
/// the evaluator because it calls a user key.
pub(crate) fn list_method(name: &str, receiver: Rc<RefCell<Vec<Value>>>) -> Option<Value> {
    match name {
        "join" => Some(join_method(receiver)),
        "pop" => Some(pop_method(receiver)),
        "remove" => Some(remove_method(receiver)),
        "insert" => Some(insert_method(receiver)),
        "index" => Some(list_index_method(receiver)),
        "extend" => Some(extend_method(receiver)),
        "reverse" => Some(list_reverse_method(receiver)),
        "clear" => Some(list_clear_method(receiver)),
        _ => None,
    }
}

/// Joins a list of strings with a separator: `items.join(separator)`.
fn join_method(receiver: Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Builtin(object::Builtin::new("join", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("join does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("join expects 1 argument, got {}", arguments.len()));
        }
        let separator = string_argument("join", "separator", &arguments[0])?;
        let list = receiver.borrow();
        let mut parts = Vec::with_capacity(list.len());
        for (index, element) in list.iter().enumerate() {
            match element {
                Value::String(text) => parts.push(text.to_string()),
                other => return Err(format!("join expects a list of strings, got {} at index {}", other.type_name(), index)),
            }
        }
        Ok(Value::string(&parts.join(&separator)))
    }))
}

fn pop_method(receiver: Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Builtin(object::Builtin::new("pop", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("pop does not accept keyword arguments".to_string());
        }
        if arguments.len() > 1 {
            return Err(format!("pop expects at most 1 argument, got {}", arguments.len()));
        }
        let mut list = receiver.borrow_mut();
        let index = if arguments.len() == 1 { element_index("pop", &arguments[0], list.len())? as i64 } else { list.len() as i64 - 1 };
        if index < 0 {
            return Err("pop from empty list".to_string());
        }
        Ok(list.remove(index as usize))
    }))
}

fn remove_method(receiver: Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Builtin(object::Builtin::new("remove", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("remove does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("remove expects 1 argument, got {}", arguments.len()));
        }
        let mut list = receiver.borrow_mut();
        for index in 0..list.len() {
            if object::equal(&list[index], &arguments[0]) {
                list.remove(index);
                return Ok(Value::Null);
            }
        }
        Err("remove: value not in list".to_string())
    }))
}

/// Inserts `value` before `index`, clamping out-of-range indexes like Python.
fn insert_method(receiver: Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Builtin(object::Builtin::new("insert", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("insert does not accept keyword arguments".to_string());
        }
        if arguments.len() != 2 {
            return Err(format!("insert expects 2 arguments, got {}", arguments.len()));
        }
        let number = integer_value("insert", "index", &arguments[0])?;
        let mut list = receiver.borrow_mut();
        let mut index = number as i64;
        if index < 0 {
            index += list.len() as i64;
            if index < 0 {
                index = 0;
            }
        }
        if index > list.len() as i64 {
            index = list.len() as i64;
        }
        list.insert(index as usize, arguments[1].clone());
        Ok(Value::Null)
    }))
}

/// Returns the first index of the value, or -1, like string `find`.
fn list_index_method(receiver: Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Builtin(object::Builtin::new("index", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("index does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("index expects 1 argument, got {}", arguments.len()));
        }
        let list = receiver.borrow();
        for (index, element) in list.iter().enumerate() {
            if object::equal(element, &arguments[0]) {
                return Ok(Value::Number(index as f64));
            }
        }
        Ok(Value::Number(-1.0))
    }))
}

fn extend_method(receiver: Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Builtin(object::Builtin::new("extend", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("extend does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("extend expects 1 argument, got {}", arguments.len()));
        }
        let Value::List(other) = &arguments[0] else {
            return Err(format!("extend expects a list, got {}", arguments[0].type_name()));
        };
        let extra = other.borrow().clone();
        receiver.borrow_mut().extend(extra);
        Ok(Value::Null)
    }))
}

fn list_reverse_method(receiver: Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Builtin(object::Builtin::new("reverse", move |_host, arguments, keywords| {
        no_arguments("reverse", arguments, keywords)?;
        receiver.borrow_mut().reverse();
        Ok(Value::Null)
    }))
}

fn list_clear_method(receiver: Rc<RefCell<Vec<Value>>>) -> Value {
    Value::Builtin(object::Builtin::new("clear", move |_host, arguments, keywords| {
        no_arguments("clear", arguments, keywords)?;
        receiver.borrow_mut().clear();
        Ok(Value::Null)
    }))
}

fn element_index(name: &str, value: &Value, length: usize) -> Result<usize, String> {
    let number = integer_value(name, "index", value)?;
    let mut index = number as i64;
    if index < 0 {
        index += length as i64;
    }
    if index < 0 || index >= length as i64 {
        return Err(format!("{} index out of range", name));
    }
    Ok(index as usize)
}

fn integer_value(name: &str, role: &str, value: &Value) -> Result<f64, String> {
    let Value::Number(number) = value else {
        return Err(format!("{} {} must be an integer", name, role));
    };
    if number.is_nan() || number.is_infinite() || number.trunc() != *number {
        return Err(format!("{} {} must be an integer", name, role));
    }
    Ok(*number)
}

/// Builds the dictionary member method named `name`, or `None`.
pub(crate) fn dict_method(name: &str, receiver: Rc<RefCell<object::Dict>>) -> Option<Value> {
    match name {
        "keys" => Some(dict_keys_method(receiver)),
        "values" => Some(dict_values_method(receiver)),
        "items" => Some(dict_items_method(receiver)),
        "get" => Some(dict_get_method(receiver)),
        "pop" => Some(dict_pop_method(receiver)),
        "delete" => Some(dict_delete_method(receiver)),
        "clear" => Some(dict_clear_method(receiver)),
        _ => None,
    }
}

fn dict_keys_method(receiver: Rc<RefCell<object::Dict>>) -> Value {
    Value::Builtin(object::Builtin::new("keys", move |_host, arguments, keywords| {
        no_arguments("keys", arguments, keywords)?;
        let dict = receiver.borrow();
        Ok(Value::list(dict.entries.iter().map(|(key, _)| key.clone()).collect()))
    }))
}

fn dict_values_method(receiver: Rc<RefCell<object::Dict>>) -> Value {
    Value::Builtin(object::Builtin::new("values", move |_host, arguments, keywords| {
        no_arguments("values", arguments, keywords)?;
        let dict = receiver.borrow();
        Ok(Value::list(dict.entries.iter().map(|(_, value)| value.clone()).collect()))
    }))
}

/// Returns `[key, value]` lists because there is no tuple type.
fn dict_items_method(receiver: Rc<RefCell<object::Dict>>) -> Value {
    Value::Builtin(object::Builtin::new("items", move |_host, arguments, keywords| {
        no_arguments("items", arguments, keywords)?;
        let dict = receiver.borrow();
        Ok(Value::list(dict.entries.iter().map(|(key, value)| Value::list(vec![key.clone(), value.clone()])).collect()))
    }))
}

fn dict_get_method(receiver: Rc<RefCell<object::Dict>>) -> Value {
    Value::Builtin(object::Builtin::new("get", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("get does not accept keyword arguments".to_string());
        }
        if arguments.is_empty() || arguments.len() > 2 {
            return Err(format!("get expects 1 or 2 arguments, got {}", arguments.len()));
        }
        match receiver.borrow().get(&arguments[0])? {
            Some(value) => Ok(value),
            None => Ok(if arguments.len() == 2 { arguments[1].clone() } else { Value::Null }),
        }
    }))
}

fn dict_pop_method(receiver: Rc<RefCell<object::Dict>>) -> Value {
    Value::Builtin(object::Builtin::new("pop", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("pop does not accept keyword arguments".to_string());
        }
        if arguments.is_empty() || arguments.len() > 2 {
            return Err(format!("pop expects 1 or 2 arguments, got {}", arguments.len()));
        }
        let value = receiver.borrow().get(&arguments[0])?;
        let Some(value) = value else {
            if arguments.len() == 2 {
                return Ok(arguments[1].clone());
            }
            return Err(format!("pop: key not found: {}", object::format(&arguments[0])));
        };
        receiver.borrow_mut().delete(&arguments[0])?;
        Ok(value)
    }))
}

/// Removes a key and reports whether it was present.
fn dict_delete_method(receiver: Rc<RefCell<object::Dict>>) -> Value {
    Value::Builtin(object::Builtin::new("delete", move |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("delete does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("delete expects 1 argument, got {}", arguments.len()));
        }
        Ok(Value::Bool(receiver.borrow_mut().delete(&arguments[0])?))
    }))
}

fn dict_clear_method(receiver: Rc<RefCell<object::Dict>>) -> Value {
    Value::Builtin(object::Builtin::new("clear", move |_host, arguments, keywords| {
        no_arguments("clear", arguments, keywords)?;
        receiver.borrow_mut().clear();
        Ok(Value::Null)
    }))
}

/// Builds a bound `ToInt`/`ToFloat`/`ToString`/`ToBool` method, or `None`.
pub(crate) fn conversion_method(name: &str, receiver: &Value) -> Option<Value> {
    if !matches!(name, "ToInt" | "ToFloat" | "ToString" | "ToBool") {
        return None;
    }
    let receiver = receiver.clone();
    let label = name.to_string();
    Some(Value::Builtin(object::Builtin::new(name, move |_host, arguments, keywords| {
        if !arguments.is_empty() || !keywords.is_empty() {
            return Err(format!("{} expects 0 arguments", label));
        }
        convert(&label, &receiver)
    })))
}

/// Converts a value using the same rules as the `int`/`float`/`str`/`bool`
/// built-ins, naming the caller in diagnostics.
pub(crate) fn convert(name: &str, value: &Value) -> Result<Value, String> {
    match name {
        "int" | "ToInt" => to_int(name, value),
        "float" | "ToFloat" => to_float(name, value),
        "str" | "ToString" => Ok(Value::string(&object::format(value))),
        "bool" | "ToBool" => Ok(Value::Bool(object::truthy(value))),
        _ => Err(format!("{} cannot convert {}", name, value.type_name())),
    }
}

fn to_int(name: &str, value: &Value) -> Result<Value, String> {
    match value {
        // The `+ 0` normalizes a negative zero, as the Go conversion does.
        Value::Number(number) => Ok(Value::Number(number.trunc() + 0.0)),
        Value::Bool(flag) => Ok(Value::Number(if *flag { 1.0 } else { 0.0 })),
        Value::String(text) => {
            let trimmed = text.trim();
            if !is_integer_literal(trimmed) {
                return Err(format!("{}: invalid integer literal {}", name, tiger_format::quote(text)));
            }
            let number = parse_finite(name, trimmed)?;
            Ok(Value::Number(number + 0.0))
        }
        other => Err(format!("{} cannot convert {}", name, other.type_name())),
    }
}

fn to_float(name: &str, value: &Value) -> Result<Value, String> {
    match value {
        Value::Number(number) => Ok(Value::Number(*number)),
        Value::Bool(flag) => Ok(Value::Number(if *flag { 1.0 } else { 0.0 })),
        Value::String(text) => {
            let trimmed = text.trim();
            if !is_float_literal(trimmed) {
                return Err(format!("{}: invalid number literal {}", name, tiger_format::quote(text)));
            }
            Ok(Value::Number(parse_finite(name, trimmed)?))
        }
        other => Err(format!("{} cannot convert {}", name, other.type_name())),
    }
}

fn parse_finite(name: &str, text: &str) -> Result<f64, String> {
    match text.parse::<f64>() {
        Ok(number) if !number.is_infinite() => Ok(number),
        _ => Err(format!("{}: {} is out of range", name, text)),
    }
}

/// Matches an optional sign followed by decimal digits.
fn is_integer_literal(text: &str) -> bool {
    let text = trim_sign(text);
    !text.is_empty() && all_digits(text)
}

/// Matches `[+-](digits[.digits] | .digits)[(e|E)[+-]digits]`.
fn is_float_literal(text: &str) -> bool {
    let mut text = trim_sign(text);
    if let Some(index) = text.find(['e', 'E']) {
        let exponent = trim_sign(&text[index + 1..]);
        if exponent.is_empty() || !all_digits(exponent) {
            return false;
        }
        text = &text[..index];
    }
    match text.split_once('.') {
        Some((whole, fraction)) => !(whole.is_empty() && fraction.is_empty()) && all_digits(whole) && all_digits(fraction),
        None => !text.is_empty() && all_digits(text),
    }
}

fn trim_sign(text: &str) -> &str {
    text.strip_prefix(['+', '-']).unwrap_or(text)
}

fn all_digits(text: &str) -> bool {
    text.bytes().all(|byte| byte.is_ascii_digit())
}
