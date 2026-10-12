//! The RE2 engine behind the `regex` module, used on native and WASI targets.
//!
//! A port of `pkg/evaluator/regex_os.go`: every entry point compiles the pattern
//! first and reports `regex: invalid pattern "..."` when compilation fails.

/// Compiles a pattern, naming it in the error when it is invalid.
fn compile(pattern: &str) -> Result<regex::Regex, String> {
    regex::Regex::new(pattern).map_err(|_| format!("regex: invalid pattern {}", tiger_format::quote(pattern)))
}

/// Reports whether the pattern matches anywhere in the text.
pub fn match_string(pattern: &str, text: &str) -> Result<bool, String> {
    Ok(compile(pattern)?.is_match(text))
}

/// Returns the first match, or `None` when there is none.
pub fn find_string(pattern: &str, text: &str) -> Result<Option<String>, String> {
    Ok(compile(pattern)?.find(text).map(|found| found.as_str().to_string()))
}

/// Returns every non-overlapping match.
pub fn find_all_strings(pattern: &str, text: &str) -> Result<Vec<String>, String> {
    Ok(compile(pattern)?.find_iter(text).map(|found| found.as_str().to_string()).collect())
}

/// Returns the capture groups of the first match, or `None` when there is none.
pub fn groups_string(pattern: &str, text: &str) -> Result<Option<Vec<String>>, String> {
    let expression = compile(pattern)?;
    Ok(expression.captures(text).map(|captures| {
        (1..captures.len()).map(|index| captures.get(index).map(|group| group.as_str().to_string()).unwrap_or_default()).collect()
    }))
}

/// Replaces every match with the replacement, expanding `$1` and `${name}`.
pub fn replace_string(pattern: &str, text: &str, replacement: &str) -> Result<String, String> {
    Ok(compile(pattern)?.replace_all(text, replacement).into_owned())
}

/// Returns the pieces of the text between matches.
pub fn split_string(pattern: &str, text: &str) -> Result<Vec<String>, String> {
    Ok(compile(pattern)?.split(text).map(|part| part.to_string()).collect())
}
