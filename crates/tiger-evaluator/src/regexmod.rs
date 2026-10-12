//! The `regex` module: `match`, `find`, `findAll`, `groups`, `replace`, `split`.
//!
//! A port of `pkg/evaluator/regexmodule.go`. The engine itself lives in
//! `regex_engine.rs` on native and WASI targets and in `regex_stub.rs` in the
//! browser build, which keeps the RE2 machinery out of `tiger.wasm`.

use std::rc::Rc;

use tiger_object as object;
use tiger_object::{Module, Value};

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
use crate::regex_engine as engine;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use crate::regex_stub as engine;

/// Checks the argument count and types, returning the arguments as strings.
fn regex_arguments(name: &str, arguments: &[Value], keywords: &[object::Keyword], count: usize) -> Result<Vec<String>, String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.len() != count {
        return Err(format!("{} expects {} arguments, got {}", name, count, arguments.len()));
    }
    let mut values = Vec::with_capacity(count);
    for (index, argument) in arguments.iter().enumerate() {
        match argument {
            Value::String(text) => values.push(text.to_string()),
            other => return Err(format!("{} expects string arguments, got {} at position {}", name, other.type_name(), index + 1)),
        }
    }
    Ok(values)
}

/// Wraps a function taking the validated string arguments.
fn builtin<F>(name: &'static str, count: usize, function: F) -> Rc<object::Builtin>
where
    F: Fn(Vec<String>) -> Result<Value, String> + 'static,
{
    let qualified = format!("regex.{}", name);
    let label = qualified.clone();
    object::Builtin::new(&label, move |_host, arguments, keywords| function(regex_arguments(&qualified, arguments, keywords, count)?))
}

fn string_list(values: Vec<String>) -> Value {
    Value::list(values.into_iter().map(|value| Value::string(&value)).collect())
}

/// The `regex` module.
pub fn regex_module() -> Rc<Module> {
    let definitions: Vec<(&'static str, Rc<object::Builtin>)> = vec![
        ("match", builtin("match", 2, |values| engine::match_string(&values[0], &values[1]).map(Value::Bool))),
        (
            "find",
            builtin("find", 2, |values| match engine::find_string(&values[0], &values[1])? {
                Some(found) => Ok(Value::string(&found)),
                None => Ok(Value::Null),
            }),
        ),
        ("findAll", builtin("findAll", 2, |values| engine::find_all_strings(&values[0], &values[1]).map(string_list))),
        (
            "groups",
            builtin("groups", 2, |values| match engine::groups_string(&values[0], &values[1])? {
                Some(groups) => Ok(string_list(groups)),
                None => Ok(Value::Null),
            }),
        ),
        (
            "replace",
            builtin("replace", 3, |values| engine::replace_string(&values[0], &values[1], &values[2]).map(|text| Value::string(&text))),
        ),
        ("split", builtin("split", 2, |values| engine::split_string(&values[0], &values[1]).map(string_list))),
    ];
    let env = object::Environment::new(None);
    for (name, function) in definitions {
        let _ = object::define(&env, name, Value::Builtin(function), true);
    }
    Rc::new(Module { name: "regex".to_string(), env })
}
