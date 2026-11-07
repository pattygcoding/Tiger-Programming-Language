//! The `api` module: HTTP requests, and the pure URL and query helpers.
//!
//! A hand-written HTTP/1.1 client over `std::net` on native targets. The engine
//! lives in `api_engine.rs` on native and in `api_stub.rs` in the browser and
//! WASI builds, so a sandboxed target never links a socket. The URL and query
//! helpers are pure and work on every target.
//!
//! The response is an ordinary dictionary, so it needs no new value type and
//! composes with the rest of the language: `status`, `reason`, `ok`, `url`,
//! `headers`, and `body`. A 4xx or 5xx status is a successful call with
//! `ok == false`, matching `requests` and `fetch`; only a transport failure
//! (a refused connection, a timeout, or a malformed URL) raises an error.

use std::cell::RefCell;
use std::rc::Rc;

use tiger_object as object;
use tiger_object::{Keyword, Module, Value};

#[cfg(not(target_arch = "wasm32"))]
use crate::api_engine as engine;
#[cfg(target_arch = "wasm32")]
use crate::api_stub as engine;

/// The default number of seconds a request waits when it names no timeout.
const DEFAULT_TIMEOUT: f64 = 30.0;

/// A validated request the engine sends.
///
/// The sandboxed build's stub never reads the fields, so the whole struct is
/// allowed to be dead there without warning; the native engine reads every one.
#[derive(Clone)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub struct Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout: Option<f64>,
    pub follow_redirects: bool,
}

/// A completed response.
pub struct Response {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub url: String,
}

/// A URL split into its components.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedUrl {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub query: String,
    pub fragment: String,
}

/// The `api` module: `get`, `post`, `put`, `patch`, `delete`, `head`, `request`,
/// and the pure `parse_url` and `encode_query` helpers.
pub fn api_module() -> Rc<Module> {
    let definitions: Vec<(&'static str, Rc<object::Builtin>)> = vec![
        ("request", request_builtin("request", None)),
        ("get", request_builtin("get", Some("GET"))),
        ("head", request_builtin("head", Some("HEAD"))),
        ("post", request_builtin("post", Some("POST"))),
        ("put", request_builtin("put", Some("PUT"))),
        ("patch", request_builtin("patch", Some("PATCH"))),
        ("delete", request_builtin("delete", Some("DELETE"))),
        ("parse_url", parse_url_builtin()),
        ("encode_query", encode_query_builtin()),
    ];
    let env = object::Environment::new(None);
    for (name, function) in definitions {
        let _ = object::define(&env, name, Value::Builtin(function), true);
    }
    Module::new("api".to_string(), env)
}

/// Builds a request method, either fixed (`get`, `post`, ...) or chosen by the
/// caller (`request`).
fn request_builtin(name: &'static str, fixed_method: Option<&'static str>) -> Rc<object::Builtin> {
    let label = format!("api.{}", name);
    let display = label.clone();
    object::Builtin::new(&display, move |_host, arguments, keywords| {
        let request = build_request(&label, fixed_method, arguments, keywords)?;
        engine::request(&request).map(|response| response_value(&response))
    })
}

/// Turns a call's positional and keyword arguments into a validated request.
///
/// Positional arguments are the method (for `request`), the URL, and — for a
/// method that carries one — the body. Everything else is a keyword argument.
fn build_request(label: &str, fixed_method: Option<&'static str>, arguments: &[Value], keywords: &[Keyword]) -> Result<Request, String> {
    let mut index = 0usize;
    let method = match fixed_method {
        Some(method) => method.to_string(),
        None => {
            let Some(value) = arguments.get(index) else {
                return Err(format!("{} expects a method and a URL", label));
            };
            index += 1;
            text_argument(label, "method", value)?.to_ascii_uppercase()
        }
    };
    let Some(url_value) = arguments.get(index) else {
        return Err(format!("{} expects a URL", label));
    };
    index += 1;
    let mut url = text_argument(label, "url", url_value)?;
    let body_allowed = method_allows_body(&method);
    let mut body: Option<Value> = None;
    if let Some(value) = arguments.get(index) {
        if !body_allowed {
            return Err(format!("{} expects at most {} positional argument(s), got {}", label, index, arguments.len()));
        }
        body = Some(value.clone());
        index += 1;
    }
    if index < arguments.len() {
        return Err(format!("{} expects at most {} positional argument(s), got {}", label, index, arguments.len()));
    }

    let mut headers_value: Option<Value> = None;
    let mut params_value: Option<Value> = None;
    let mut timeout_value: Option<Value> = None;
    let mut json_value: Option<Value> = None;
    let mut follow_redirects = true;
    for keyword in keywords {
        match keyword.name.as_str() {
            "headers" => headers_value = Some(keyword.value.clone()),
            "params" => params_value = Some(keyword.value.clone()),
            "timeout" => timeout_value = Some(keyword.value.clone()),
            "json" => {
                require_body(label, body_allowed)?;
                json_value = Some(keyword.value.clone());
            }
            "body" => {
                require_body(label, body_allowed)?;
                if body.is_some() {
                    return Err(format!("{} accepts a body once", label));
                }
                body = Some(keyword.value.clone());
            }
            "follow_redirects" => follow_redirects = bool_argument(label, "follow_redirects", &keyword.value)?,
            other => return Err(format!("{} does not accept keyword argument {:?}", label, other)),
        }
    }
    if body.is_some() && json_value.is_some() {
        return Err(format!("{} accepts either body or json, not both", label));
    }

    let mut headers = Vec::new();
    if let Some(value) = headers_value {
        headers = headers_argument(label, &value)?;
    }
    let mut bytes: Option<Vec<u8>> = None;
    if let Some(value) = json_value {
        let text = crate::modules::stringify(&value)?;
        if !has_header(&headers, "content-type") {
            headers.push(("Content-Type".to_string(), "application/json".to_string()));
        }
        bytes = Some(text.into_bytes());
    }
    if let Some(value) = body {
        let text = match &value {
            Value::String(text) => text.to_string(),
            Value::Null => String::new(),
            other => return Err(format!("{} expects a string body, got {}", label, other.type_name())),
        };
        if !has_header(&headers, "content-type") {
            headers.push(("Content-Type".to_string(), "text/plain; charset=utf-8".to_string()));
        }
        bytes = Some(text.into_bytes());
    }

    if let Some(value) = params_value {
        let query = params_argument(label, &value)?;
        if !query.is_empty() {
            let (before, fragment) = match url.split_once('#') {
                Some((before, after)) => (before.to_string(), Some(after.to_string())),
                None => (url, None),
            };
            let mut combined = before;
            combined.push(if combined.contains('?') { '&' } else { '?' });
            combined.push_str(&query);
            if let Some(fragment) = fragment {
                combined.push('#');
                combined.push_str(&fragment);
            }
            url = combined;
        }
    }

    let timeout = match timeout_value {
        Some(value) => timeout_argument(label, &value)?,
        None => Some(DEFAULT_TIMEOUT),
    };
    Ok(Request { method, url, headers, body: bytes, timeout, follow_redirects })
}

/// Reports whether a method carries a body. A GET or a HEAD does not.
fn method_allows_body(method: &str) -> bool {
    !matches!(method, "GET" | "HEAD")
}

fn require_body(label: &str, allowed: bool) -> Result<(), String> {
    if allowed {
        Ok(())
    } else {
        Err(format!("{} does not accept a body", label))
    }
}

/// Reads a string argument, naming the role in the diagnostic.
fn text_argument(label: &str, role: &str, value: &Value) -> Result<String, String> {
    match value {
        Value::String(text) => Ok(text.to_string()),
        other => Err(format!("{} expects a string {} argument, got {}", label, role, other.type_name())),
    }
}

fn bool_argument(label: &str, role: &str, value: &Value) -> Result<bool, String> {
    match value {
        Value::Bool(flag) => Ok(*flag),
        other => Err(format!("{} expects a boolean {} argument, got {}", label, role, other.type_name())),
    }
}

/// Reads a timeout in seconds. `null` and `0` mean "wait indefinitely".
fn timeout_argument(label: &str, value: &Value) -> Result<Option<f64>, String> {
    match value {
        Value::Null => Ok(None),
        Value::Number(number) if number.is_finite() => {
            if *number < 0.0 {
                return Err(format!("{} expects a non-negative timeout, got {}", label, tiger_format::number(*number)));
            }
            if *number == 0.0 {
                Ok(None)
            } else {
                Ok(Some(*number))
            }
        }
        other => Err(format!("{} expects a number of seconds for timeout, got {}", label, other.type_name())),
    }
}

/// Reads the `headers` keyword: a dictionary of string names to string values.
fn headers_argument(label: &str, value: &Value) -> Result<Vec<(String, String)>, String> {
    let Value::Dict(entries) = value else {
        return Err(format!("{} expects headers to be a dictionary, got {}", label, value.type_name()));
    };
    let mut headers = Vec::new();
    for (key, value) in entries.borrow().entries.iter() {
        let Value::String(name) = key else {
            return Err(format!("{} expects header names to be strings, got {}", label, key.type_name()));
        };
        let Value::String(text) = value else {
            return Err(format!("{} expects header values to be strings, got {}", label, value.type_name()));
        };
        headers.push((name.to_string(), text.to_string()));
    }
    Ok(headers)
}

/// Reads the `params` keyword: a dictionary encoded as a query string, or a
/// string already in query form.
fn params_argument(label: &str, value: &Value) -> Result<String, String> {
    match value {
        Value::Null => Ok(String::new()),
        Value::String(text) => Ok(text.to_string()),
        Value::Dict(_) => encode_query(label, value),
        other => Err(format!("{} expects params to be a dictionary or a string, got {}", label, other.type_name())),
    }
}

/// Whether the header list already carries `name`, case-insensitively.
fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(key, _)| key.eq_ignore_ascii_case(name))
}

/// Splits a URL into its components, or reports why it is invalid.
///
/// Only `http` and `https` are recognized. A URL without a scheme, without a
/// host, or with a port that is not a number is rejected before any socket is
/// opened, so a bad URL never reaches the network.
pub fn parse_url(url: &str) -> Result<ParsedUrl, String> {
    let text = url.trim();
    let (scheme, rest) = text.split_once("://").ok_or_else(|| format!("invalid URL {:?}: expected scheme://host", text))?;
    if scheme.is_empty() {
        return Err(format!("invalid URL {:?}: empty scheme", text));
    }
    let scheme = scheme.to_ascii_lowercase();
    let split = rest.find(['/', '?', '#']);
    let (authority, remainder) = match split {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    let authority = match authority.rsplit_once('@') {
        Some((_, host)) => host,
        None => authority,
    };
    let (host, port) = split_host_port(authority, &scheme)?;
    if host.is_empty() {
        return Err(format!("invalid URL {:?}: missing host", text));
    }
    let (before_fragment, fragment) = match remainder.split_once('#') {
        Some((before, after)) => (before, after),
        None => (remainder, ""),
    };
    let (path, query) = match before_fragment.split_once('?') {
        Some((before, after)) => (before, after),
        None => (before_fragment, ""),
    };
    let path = if path.is_empty() { "/".to_string() } else { path.to_string() };
    Ok(ParsedUrl { scheme, host, port, path, query: query.to_string(), fragment: fragment.to_string() })
}

/// Splits `host:port`, applying the scheme's default port when none is given.
fn split_host_port(authority: &str, scheme: &str) -> Result<(String, u16), String> {
    let default = match scheme {
        "http" => 80,
        "https" => 443,
        other => return Err(format!("unsupported URL scheme {:?}", other)),
    };
    if authority.is_empty() {
        return Err("missing host".to_string());
    }
    if authority.starts_with('[') {
        // An IPv6 literal keeps its brackets, so `[::1]:8080` parses as `[::1]`.
        let close = authority.find(']').ok_or_else(|| "invalid IPv6 host".to_string())?;
        let host = &authority[..=close];
        let rest = &authority[close + 1..];
        let port = match rest.strip_prefix(':') {
            Some(port) => port.parse::<u16>().map_err(|_| format!("invalid port {:?}", port))?,
            None if rest.is_empty() => default,
            None => return Err(format!("invalid host {:?}", authority)),
        };
        return Ok((host.to_string(), port));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => {
            let port = port.parse::<u16>().map_err(|_| format!("invalid port {:?}", port))?;
            Ok((host.to_string(), port))
        }
        None => Ok((authority.to_string(), default)),
    }
}

/// Encodes a dictionary as an `a=1&b=2` query string.
fn encode_query(label: &str, value: &Value) -> Result<String, String> {
    let Value::Dict(entries) = value else {
        return Err(format!("{} expects a dictionary, got {}", label, value.type_name()));
    };
    let mut pairs = Vec::new();
    for (key, value) in entries.borrow().entries.iter() {
        let key_text = match key {
            Value::String(text) => text.to_string(),
            other => return Err(format!("{} expects string keys, got {}", label, other.type_name())),
        };
        append_query_pairs(&mut pairs, &key_text, value);
    }
    Ok(pairs.join("&"))
}

/// Appends one key's pairs. A list or tuple repeats the key once per element.
fn append_query_pairs(pairs: &mut Vec<String>, key: &str, value: &Value) {
    match value {
        Value::List(elements) => {
            for element in elements.borrow().iter() {
                append_query_pairs(pairs, key, element);
            }
        }
        Value::Tuple(elements) => {
            for element in elements.iter() {
                append_query_pairs(pairs, key, element);
            }
        }
        Value::Null => pairs.push(format!("{}=", percent_encode(key))),
        Value::String(text) => pairs.push(format!("{}={}", percent_encode(key), percent_encode(text))),
        Value::Number(number) => pairs.push(format!("{}={}", percent_encode(key), percent_encode(&tiger_format::number(*number)))),
        Value::Bool(flag) => pairs.push(format!("{}={}", percent_encode(key), if *flag { "true" } else { "false" })),
        other => pairs.push(format!("{}={}", percent_encode(key), percent_encode(&object::format(other)))),
    }
}

/// Percent-encodes everything outside the RFC 3986 unreserved set.
fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

/// `api.parse_url(url)`: the components of a URL as a dictionary.
fn parse_url_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("api.parse_url", |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("api.parse_url does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("api.parse_url expects 1 argument, got {}", arguments.len()));
        }
        let url = text_argument("api.parse_url", "url", &arguments[0])?;
        Ok(parsed_value(&parse_url(&url)?))
    })
}

/// `api.encode_query(dict)`: a dictionary as an `a=1&b=2` query string.
fn encode_query_builtin() -> Rc<object::Builtin> {
    object::Builtin::new("api.encode_query", |_host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err("api.encode_query does not accept keyword arguments".to_string());
        }
        if arguments.len() != 1 {
            return Err(format!("api.encode_query expects 1 argument, got {}", arguments.len()));
        }
        Ok(Value::string(&encode_query("api.encode_query", &arguments[0])?))
    })
}

/// The dictionary `api.parse_url` returns.
fn parsed_value(parsed: &ParsedUrl) -> Value {
    let mut dict = object::Dict::default();
    let _ = dict.set(Value::string("scheme"), Value::string(&parsed.scheme));
    let _ = dict.set(Value::string("host"), Value::string(&parsed.host));
    let _ = dict.set(Value::string("port"), Value::Number(parsed.port as f64));
    let _ = dict.set(Value::string("path"), Value::string(&parsed.path));
    let _ = dict.set(Value::string("query"), Value::string(&parsed.query));
    let _ = dict.set(Value::string("fragment"), Value::string(&parsed.fragment));
    Value::Dict(Rc::new(RefCell::new(dict)))
}

/// The dictionary a request method returns.
fn response_value(response: &Response) -> Value {
    let mut dict = object::Dict::default();
    let _ = dict.set(Value::string("status"), Value::Number(response.status as f64));
    let _ = dict.set(Value::string("reason"), Value::string(&response.reason));
    let _ = dict.set(Value::string("ok"), Value::Bool(response.status < 400));
    let _ = dict.set(Value::string("url"), Value::string(&response.url));
    let _ = dict.set(Value::string("headers"), response_headers(&response.headers));
    let _ = dict.set(Value::string("body"), Value::string(&String::from_utf8_lossy(&response.body)));
    Value::Dict(Rc::new(RefCell::new(dict)))
}

/// The response headers as a dictionary, lowercased, with duplicates joined by
/// `, ` so every name appears once.
fn response_headers(headers: &[(String, String)]) -> Value {
    let mut dict = object::Dict::default();
    for (name, value) in headers {
        let key = Value::string(&name.to_ascii_lowercase());
        if let Ok(Some(Value::String(existing))) = dict.get(&key) {
            let combined = format!("{}, {}", existing, value);
            let _ = dict.set(key, Value::string(&combined));
            continue;
        }
        let _ = dict.set(key, Value::string(value));
    }
    Value::Dict(Rc::new(RefCell::new(dict)))
}
