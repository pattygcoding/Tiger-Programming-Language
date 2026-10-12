//! Byte-level filesystems and the file object API.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use tiger_object as object;
use tiger_object::{FileData, Keyword, Value};

/// Host file storage: the memory filesystem for tests and the browser, the
/// operating-system filesystem for the native CLI and WASI.
pub trait FileSystem {
    fn exists(&mut self, name: &str) -> Result<bool, String>;
    fn load(&mut self, name: &str) -> Result<Vec<u8>, String>;
    fn persist(&mut self, name: &str, data: &[u8]) -> Result<(), String>;
    fn remove(&mut self, name: &str) -> Result<(), String>;
    fn list(&mut self, name: &str) -> Result<Vec<String>, String>;
}

/// An in-memory filesystem; hermetic, so tests and the playground never touch
/// the host disk.
#[derive(Default)]
pub struct MemoryFileSystem {
    entries: HashMap<String, Vec<u8>>,
}

impl MemoryFileSystem {
    pub fn new() -> MemoryFileSystem {
        MemoryFileSystem::default()
    }
}

impl FileSystem for MemoryFileSystem {
    fn exists(&mut self, name: &str) -> Result<bool, String> {
        let key = memory_key(name)?;
        Ok(self.entries.contains_key(&key))
    }

    fn load(&mut self, name: &str) -> Result<Vec<u8>, String> {
        let key = memory_key(name)?;
        match self.entries.get(&key) {
            Some(data) => Ok(data.clone()),
            None => Err(format!("open {}: file does not exist", name)),
        }
    }

    fn persist(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        let key = memory_key(name)?;
        self.entries.insert(key, data.to_vec());
        Ok(())
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        let key = memory_key(name)?;
        if self.entries.remove(&key).is_some() {
            Ok(())
        } else {
            Err(format!("remove {}: file does not exist", name))
        }
    }

    fn list(&mut self, name: &str) -> Result<Vec<String>, String> {
        let key = memory_list_key(name);
        let mut names = Vec::new();
        let mut exists = key.is_empty();
        for entry in self.entries.keys() {
            if key.is_empty() {
                if !entry.contains('/') {
                    names.push(entry.clone());
                }
                continue;
            }
            let prefix = format!("{}/", key);
            if !entry.starts_with(&prefix) {
                continue;
            }
            exists = true;
            let rest = &entry[prefix.len()..];
            if !rest.contains('/') {
                names.push(rest.to_string());
            }
        }
        if !exists {
            return Err(format!("list {}: file does not exist", name));
        }
        names.sort();
        Ok(names)
    }
}

/// The operating system's filesystem, used by the native CLI and WASI.
pub struct OsFileSystem;

impl FileSystem for OsFileSystem {
    fn exists(&mut self, name: &str) -> Result<bool, String> {
        match std::fs::metadata(name) {
            Ok(metadata) => Ok(!metadata.is_dir()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("stat {}: {}", name, error)),
        }
    }

    fn load(&mut self, name: &str) -> Result<Vec<u8>, String> {
        std::fs::read(name).map_err(|error| os_error("open", name, error))
    }

    fn persist(&mut self, name: &str, data: &[u8]) -> Result<(), String> {
        std::fs::write(name, data).map_err(|error| os_error("open", name, error))
    }

    fn remove(&mut self, name: &str) -> Result<(), String> {
        std::fs::remove_file(name).map_err(|error| os_error("remove", name, error))
    }

    fn list(&mut self, name: &str) -> Result<Vec<String>, String> {
        let entries = std::fs::read_dir(name).map_err(|error| os_error("open", name, error))?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let mut child = entry.file_name().to_string_lossy().to_string();
            if entry.path().is_dir() {
                child.push('/');
            }
            names.push(child);
        }
        names.sort();
        Ok(names)
    }
}

/// Normalizes a path the way Go's `path.Clean` does.
fn path_clean(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let rooted = path.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if let Some(last) = segments.last() {
                    if *last != ".." {
                        segments.pop();
                        continue;
                    }
                }
                if rooted {
                    continue;
                }
                segments.push("..");
            }
            other => segments.push(other),
        }
    }
    let mut result = segments.join("/");
    if rooted {
        result.insert(0, '/');
    }
    if result.is_empty() {
        result = if rooted { "/".to_string() } else { ".".to_string() };
    }
    result
}

fn memory_key(name: &str) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("path must not be empty".to_string());
    }
    let key = path_clean(&name.replace('\\', "/"));
    if key == "." || key == "/" {
        return Err(format!("invalid path {:?}", name));
    }
    Ok(key.trim_start_matches('/').to_string())
}

fn memory_list_key(name: &str) -> String {
    let key = path_clean(&name.replace('\\', "/"));
    if key == "." || key == "/" {
        return String::new();
    }
    key.trim_start_matches('/').to_string()
}

/// A parsed file mode string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileMode {
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub truncate: bool,
    pub create: bool,
    pub binary: bool,
}

/// Accepts `r`, `w`, and `a` with an optional `b`, an optional `+`, and the two
/// combined forms `r+b` and `rb+`.
pub fn parse_file_mode(mode: &str) -> Result<FileMode, String> {
    let mut flag = FileMode { read: false, write: false, append: false, truncate: false, create: false, binary: false };
    let mut base = String::new();
    let mut plus = false;
    for ch in mode.chars() {
        match ch {
            'b' | 'B' => {
                if flag.binary || base.is_empty() {
                    return Err(format!("invalid mode {:?}", mode));
                }
                flag.binary = true;
            }
            '+' => {
                if plus || base.is_empty() {
                    return Err(format!("invalid mode {:?}", mode));
                }
                plus = true;
            }
            'r' | 'R' | 'w' | 'W' | 'a' | 'A' => {
                if !base.is_empty() {
                    return Err(format!("invalid mode {:?}", mode));
                }
                base = ch.to_ascii_lowercase().to_string();
            }
            _ => return Err(format!("invalid mode {:?}", mode)),
        }
    }
    match base.as_str() {
        "r" => flag.read = true,
        "w" => {
            flag.write = true;
            flag.truncate = true;
            flag.create = true;
        }
        "a" => {
            flag.write = true;
            flag.append = true;
            flag.create = true;
        }
        _ => return Err(format!("invalid mode {:?}", mode)),
    }
    if plus {
        flag.read = true;
        flag.write = true;
    }
    Ok(flag)
}

/// Opens a file object using the host filesystem, honoring the mode flags.
pub fn open_file(host: &mut dyn object::Host, name: &str, mode: &str) -> Result<Value, String> {
    let flag = parse_file_mode(mode)?;
    let exists = host.file_exists(name)?;
    if !exists && !flag.create {
        return Err(format!("open {}: file does not exist", name));
    }
    let mut data = if exists { host.load_file(name)? } else { Vec::new() };
    if flag.truncate {
        data.clear();
    }
    let offset = if flag.append { data.len() } else { 0 };
    let file = Rc::new(RefCell::new(FileData {
        name: name.to_string(),
        mode: mode.to_string(),
        binary: flag.binary,
        data,
        offset,
        append: flag.append,
        read: flag.read,
        write: flag.write,
        open: true,
    }));
    Ok(Value::File(file))
}

/// Writes a file object's bytes back to the host filesystem.
pub fn persist(file: &Rc<RefCell<FileData>>, host: &mut dyn object::Host) -> Result<(), String> {
    let (name, data) = {
        let borrowed = file.borrow();
        (borrowed.name.clone(), borrowed.data.clone())
    };
    host.persist_file(&name, &data)
}

/// Reads up to `size` bytes (or all remaining when negative) from a file object.
pub fn read_text(file: &RefCell<FileData>, size: i64) -> Result<String, String> {
    let mut borrowed = file.borrow_mut();
    if !borrowed.open {
        return Err("I/O operation on closed file".to_string());
    }
    if !borrowed.read {
        return Err(format!("file {} is not open for reading", tiger_format::quote(&borrowed.name)));
    }
    let end = if size < 0 { borrowed.data.len() } else { (borrowed.offset + size as usize).min(borrowed.data.len()) };
    let start = borrowed.offset;
    let text = String::from_utf8_lossy(&borrowed.data[start..end]).to_string();
    borrowed.offset = end;
    Ok(text)
}

/// Reads one line including its trailing newline, matching the Go runtime.
pub fn read_line(file: &RefCell<FileData>) -> Result<String, String> {
    let mut borrowed = file.borrow_mut();
    if !borrowed.open {
        return Err("I/O operation on closed file".to_string());
    }
    if !borrowed.read {
        return Err(format!("file {} is not open for reading", tiger_format::quote(&borrowed.name)));
    }
    if borrowed.offset >= borrowed.data.len() {
        return Ok(String::new());
    }
    let start = borrowed.offset;
    let mut end = borrowed.data.len();
    if let Some(offset) = borrowed.data[start..].iter().position(|byte| *byte == b'\n') {
        end = start + offset + 1;
    }
    let text = String::from_utf8_lossy(&borrowed.data[start..end]).to_string();
    borrowed.offset = end;
    Ok(text)
}

/// Writes text at the current offset, extending the buffer as needed.
pub fn write_text(file: &RefCell<FileData>, text: &str) -> Result<usize, String> {
    let mut borrowed = file.borrow_mut();
    if !borrowed.open {
        return Err("I/O operation on closed file".to_string());
    }
    if !borrowed.write {
        return Err(format!("file {} is not open for writing", tiger_format::quote(&borrowed.name)));
    }
    if borrowed.append || borrowed.offset > borrowed.data.len() {
        borrowed.offset = borrowed.data.len();
    }
    let bytes = text.as_bytes();
    let start = borrowed.offset;
    let end = start + bytes.len();
    if end > borrowed.data.len() {
        borrowed.data.resize(end, 0);
    }
    borrowed.data[start..end].copy_from_slice(bytes);
    borrowed.offset = end;
    Ok(bytes.len())
}

/// Seeks within a file object; `whence` is 0 (start), 1 (current), or 2 (end).
pub fn seek_file(file: &RefCell<FileData>, offset: i64, whence: i64) -> Result<i64, String> {
    let mut borrowed = file.borrow_mut();
    if !borrowed.open {
        return Err("I/O operation on closed file".to_string());
    }
    let base = match whence {
        0 => 0i64,
        1 => borrowed.offset as i64,
        2 => borrowed.data.len() as i64,
        other => return Err(format!("invalid seek origin {}", other)),
    };
    let position = base + offset;
    if position < 0 {
        return Err(format!("negative seek position {}", position));
    }
    borrowed.offset = position as usize;
    Ok(position)
}

/// The file built-in functions: `open`, `read_file`, `write_file`,
/// `append_file`, `file_exists`, `remove_file`, and `list_dir`.
pub(crate) fn file_builtins() -> Vec<(&'static str, Rc<object::Builtin>)> {
    vec![
        (
            "open",
            object::Builtin::new("open", |host, arguments, keywords| {
                if !keywords.is_empty() {
                    return Err("open does not accept keyword arguments".to_string());
                }
                if arguments.is_empty() || arguments.len() > 2 {
                    return Err(format!("open expects 1 or 2 arguments, got {}", arguments.len()));
                }
                let Value::String(name) = &arguments[0] else {
                    return Err(format!("open expects a path string, got {}", arguments[0].type_name()));
                };
                let mode = if arguments.len() == 2 {
                    match &arguments[1] {
                        Value::String(text) => text.to_string(),
                        other => return Err(format!("open mode must be a string, got {}", other.type_name())),
                    }
                } else {
                    "r".to_string()
                };
                open_file(host, name, &mode)
            }),
        ),
        (
            "read_file",
            object::Builtin::new("read_file", |host, arguments, keywords| {
                let name = file_path_argument("read_file", arguments, keywords)?;
                let data = host.load_file(&name)?;
                Ok(Value::string(&String::from_utf8_lossy(&data)))
            }),
        ),
        (
            "write_file",
            object::Builtin::new("write_file", |host, arguments, keywords| {
                let (name, text) = file_text_arguments("write_file", arguments, keywords)?;
                host.persist_file(&name, text.as_bytes())?;
                Ok(Value::Number(text.len() as f64))
            }),
        ),
        (
            "append_file",
            object::Builtin::new("append_file", |host, arguments, keywords| {
                let (name, text) = file_text_arguments("append_file", arguments, keywords)?;
                let mut data = if host.file_exists(&name)? { host.load_file(&name)? } else { Vec::new() };
                data.extend_from_slice(text.as_bytes());
                host.persist_file(&name, &data)?;
                Ok(Value::Number(text.len() as f64))
            }),
        ),
        (
            "file_exists",
            object::Builtin::new("file_exists", |host, arguments, keywords| {
                let name = file_path_argument("file_exists", arguments, keywords)?;
                Ok(Value::Bool(host.file_exists(&name)?))
            }),
        ),
        (
            "remove_file",
            object::Builtin::new("remove_file", |host, arguments, keywords| {
                let name = file_path_argument("remove_file", arguments, keywords)?;
                host.remove_file(&name)?;
                Ok(Value::Null)
            }),
        ),
        (
            "list_dir",
            object::Builtin::new("list_dir", |host, arguments, keywords| {
                let name = file_path_argument("list_dir", arguments, keywords)?;
                let names = host.list_dir(&name)?;
                Ok(Value::list(names.iter().map(|entry| Value::string(entry)).collect()))
            }),
        ),
    ]
}

fn file_path_argument(name: &str, arguments: &[Value], keywords: &[Keyword]) -> Result<String, String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.len() != 1 {
        return Err(format!("{} expects 1 argument, got {}", name, arguments.len()));
    }
    match &arguments[0] {
        Value::String(path) => Ok(path.to_string()),
        other => Err(format!("{} expects a path string, got {}", name, other.type_name())),
    }
}

fn file_text_arguments(name: &str, arguments: &[Value], keywords: &[Keyword]) -> Result<(String, String), String> {
    if !keywords.is_empty() {
        return Err(format!("{} does not accept keyword arguments", name));
    }
    if arguments.len() != 2 {
        return Err(format!("{} expects 2 arguments, got {}", name, arguments.len()));
    }
    let path = match &arguments[0] {
        Value::String(path) => path.to_string(),
        other => return Err(format!("{} expects a path string, got {}", name, other.type_name())),
    };
    let text = match &arguments[1] {
        Value::String(text) => text.to_string(),
        other => return Err(format!("{} expects a string value, got {}", name, other.type_name())),
    };
    Ok((path, text))
}

/// Formats an operating-system I/O error the way Go's `os` package does.
fn os_error(op: &str, name: &str, error: std::io::Error) -> String {
    let reason = if error.kind() == std::io::ErrorKind::NotFound { "no such file or directory".to_string() } else { error.to_string() };
    format!("{} {}: {}", op, name, reason)
}

const FILE_METHODS: &[&str] = &["read", "readline", "readlines", "write", "writelines", "flush", "close", "seek", "tell"];

impl crate::Evaluator {
    /// Reads a property or bound method from a file object.
    pub(crate) fn file_property(&mut self, base: &tiger_ast::Base, file: &Rc<RefCell<FileData>>, name: &str) -> crate::Eval {
        match name {
            "name" => return Ok(Value::string(&file.borrow().name)),
            "mode" => return Ok(Value::string(&file.borrow().mode)),
            "closed" => return Ok(Value::Bool(!file.borrow().open)),
            _ => {}
        }
        if !FILE_METHODS.contains(&name) {
            return Err(crate::runtime(base, format!("file has no property {:?}", name)));
        }
        Ok(Value::Builtin(file_method(name, file.clone())))
    }
}

fn file_method(name: &str, file: Rc<RefCell<FileData>>) -> Rc<object::Builtin> {
    let label = name.to_string();
    object::Builtin::new(name, move |host, arguments, keywords| {
        if !keywords.is_empty() {
            return Err(format!("file {} does not accept keyword arguments", label));
        }
        match label.as_str() {
            "read" => {
                if arguments.len() > 1 {
                    return Err(format!("read expects 0 or 1 arguments, got {}", arguments.len()));
                }
                let size = if arguments.len() == 1 { file_integer(&arguments[0], "read size")? } else { -1 };
                Ok(Value::string(&read_text(&file, size)?))
            }
            "readline" => {
                if !arguments.is_empty() {
                    return Err(format!("readline expects 0 arguments, got {}", arguments.len()));
                }
                Ok(Value::string(&read_line(&file)?))
            }
            "readlines" => {
                if !arguments.is_empty() {
                    return Err(format!("readlines expects 0 arguments, got {}", arguments.len()));
                }
                let mut lines = Vec::new();
                loop {
                    let line = read_line(&file)?;
                    if line.is_empty() {
                        return Ok(Value::list(lines));
                    }
                    lines.push(Value::string(&line));
                }
            }
            "write" => {
                if arguments.len() != 1 {
                    return Err(format!("write expects 1 argument, got {}", arguments.len()));
                }
                match &arguments[0] {
                    Value::String(text) => Ok(Value::Number(write_text(&file, text)? as f64)),
                    other => Err(format!("write expects a string, got {}", other.type_name())),
                }
            }
            "writelines" => {
                if arguments.len() != 1 {
                    return Err(format!("writelines expects 1 argument, got {}", arguments.len()));
                }
                let Value::List(elements) = &arguments[0] else {
                    return Err(format!("writelines expects a list, got {}", arguments[0].type_name()));
                };
                let elements = elements.borrow().clone();
                for element in elements {
                    match element {
                        Value::String(text) => {
                            write_text(&file, &text)?;
                        }
                        other => return Err(format!("writelines expects a list of strings, got {}", other.type_name())),
                    }
                }
                Ok(Value::Null)
            }
            "flush" => {
                if !arguments.is_empty() {
                    return Err(format!("flush expects 0 arguments, got {}", arguments.len()));
                }
                persist(&file, host)?;
                Ok(Value::Null)
            }
            "close" => {
                if !arguments.is_empty() {
                    return Err(format!("close expects 0 arguments, got {}", arguments.len()));
                }
                let was_open = file.borrow().open;
                file.borrow_mut().open = false;
                if was_open {
                    persist(&file, host)?;
                }
                Ok(Value::Null)
            }
            "seek" => {
                if arguments.is_empty() || arguments.len() > 2 {
                    return Err(format!("seek expects 1 or 2 arguments, got {}", arguments.len()));
                }
                let offset = file_integer(&arguments[0], "seek offset")?;
                let mut whence = 0i64;
                if arguments.len() == 2 {
                    whence = file_integer(&arguments[1], "seek origin")?;
                    if !(0..=2).contains(&whence) {
                        return Err(format!("seek origin must be 0, 1, or 2, got {}", whence));
                    }
                }
                Ok(Value::Number(seek_file(&file, offset, whence)? as f64))
            }
            "tell" => {
                if !arguments.is_empty() {
                    return Err(format!("tell expects 0 arguments, got {}", arguments.len()));
                }
                Ok(Value::Number(file.borrow().offset as f64))
            }
            _ => Err(format!("file has no property {:?}", label)),
        }
    })
}

fn file_integer(value: &Value, label: &str) -> Result<i64, String> {
    let Value::Number(number) = value else {
        return Err(format!("{} must be a safe integer", label));
    };
    if number.is_nan() || number.is_infinite() || number.trunc() != *number || number.abs() > 9007199254740991.0 {
        return Err(format!("{} must be a safe integer", label));
    }
    Ok(*number as i64)
}
