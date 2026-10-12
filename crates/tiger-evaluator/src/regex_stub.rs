//! The browser build's `regex` stub.
//!
//! A port of `pkg/evaluator/regex_js.go`: the RE2 engine (and its `aho-corasick`,
//! `memchr`, and `regex-syntax` dependencies) must stay out of `tiger.wasm`, so
//! every entry point keeps the module visible with a clear error instead.

/// The single message every browser `regex` call reports.
pub const UNAVAILABLE: &str = "the regex module is not available in the browser build; use the native CLI or the WASI target";

pub fn match_string(_pattern: &str, _text: &str) -> Result<bool, String> {
    Err(UNAVAILABLE.to_string())
}

pub fn find_string(_pattern: &str, _text: &str) -> Result<Option<String>, String> {
    Err(UNAVAILABLE.to_string())
}

pub fn find_all_strings(_pattern: &str, _text: &str) -> Result<Vec<String>, String> {
    Err(UNAVAILABLE.to_string())
}

pub fn groups_string(_pattern: &str, _text: &str) -> Result<Option<Vec<String>>, String> {
    Err(UNAVAILABLE.to_string())
}

pub fn replace_string(_pattern: &str, _text: &str, _replacement: &str) -> Result<String, String> {
    Err(UNAVAILABLE.to_string())
}

pub fn split_string(_pattern: &str, _text: &str) -> Result<Vec<String>, String> {
    Err(UNAVAILABLE.to_string())
}
