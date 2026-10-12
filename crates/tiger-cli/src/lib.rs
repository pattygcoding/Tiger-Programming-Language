//! The `tiger` command-line interface.
//!
//! A port of `cmd/tiger`: `run`, `build`, and `help`, with the same diagnostics
//! and exit codes. `build` produces a standalone executable by copying this
//! binary and appending the collected module bundle, so the result runs without
//! a toolchain, without a `PATH`, and without the `.tg` sources on disk.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tiger_evaluator::{BundleLoader, FileLoader, ModuleLoader};

/// The usage text, exactly as the Go CLI prints it.
pub const USAGE: &str = "Usage:\n  tiger run <file.tg>\n  tiger build <file.tg> [-o <executable>]\n  tiger help\n";

/// The trailer that marks a built executable and locates its bundle.
pub const MAGIC: &[u8; 16] = b"tiger-bundle-v1\n";

/// Runs one CLI invocation and returns its exit code.
///
/// `stdin` is `None` when the program has no input stream, which makes
/// `input(...)` fail the way Go's `io.Reader`-less evaluator does.
pub fn run(args: &[String], stdin: Option<Box<dyn BufRead>>, stdout: Box<dyn Write>, stderr: Box<dyn Write>) -> i32 {
    let mut stdout = stdout;
    let mut stderr = stderr;
    if args.len() == 1 && (args[0] == "help" || args[0] == "-h" || args[0] == "--help") {
        let _ = stdout.write_all(USAGE.as_bytes());
        return 0;
    }
    if args.len() < 2 || (args[0] != "run" && args[0] != "build") {
        let _ = stderr.write_all(USAGE.as_bytes());
        return 2;
    }
    let building = args[0] == "build";
    let mut filename = String::new();
    let mut output = String::new();
    let mut index = 1;
    while index < args.len() {
        let argument = &args[index];
        if building && argument == "-o" {
            if !output.is_empty() || index + 1 == args.len() {
                let _ = writeln!(stderr, "build requires one output path after -o");
                return 2;
            }
            index += 1;
            output = args[index].clone();
        } else if argument.starts_with('-') || !filename.is_empty() {
            let _ = stderr.write_all(USAGE.as_bytes());
            return 2;
        } else {
            filename = argument.clone();
        }
        index += 1;
    }
    if extension(&filename) != ".tg" {
        let _ = writeln!(stderr, "input must be a .tg file");
        return 2;
    }
    if !building {
        return match run_file(&filename, stdin, stdout) {
            Ok(()) => 0,
            Err(error) => {
                let _ = writeln!(stderr, "{}", error);
                1
            }
        };
    }
    let source = match std::fs::read_to_string(&filename) {
        Ok(source) => source,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    if output.is_empty() {
        output = default_output(&filename);
    }
    if same_file(&filename, &output) {
        let _ = writeln!(stderr, "output must be a valid path different from the input file");
        return 2;
    }
    match build_standalone(&source, &filename, &output) {
        Ok(()) => {
            let _ = writeln!(stdout, "Built {}", output);
            0
        }
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            1
        }
    }
}

/// Go's `filepath.Ext`: the suffix from the final dot of the final element.
pub fn extension(path: &str) -> String {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match base.rfind('.') {
        Some(index) => base[index..].to_string(),
        None => String::new(),
    }
}

/// Go's default output name: the input's base name without `.tg`, plus `.exe`
/// on Windows.
fn default_output(filename: &str) -> String {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let stem = base.strip_suffix(".tg").unwrap_or(base);
    if cfg!(windows) {
        format!("{}.exe", stem)
    } else {
        stem.to_string()
    }
}

/// Reads a program through the host filesystem and runs it.
///
/// The Rust counterpart of Go's `evaluator.RunFileWithInput`, shared by the
/// `tiger run` command and the WASI entry point.
pub fn run_file(filename: &str, stdin: Option<Box<dyn BufRead>>, output: Box<dyn Write>) -> Result<(), String> {
    let loader = FileLoader;
    let canonical = loader.resolve("", filename)?;
    let source = loader.load(&canonical)?;
    tiger_evaluator::run_with_input(&source, &canonical, Some(Rc::new(FileLoader)), None, stdin, output)
}

/// Reports whether two paths name the same file, as Go's `os.SameFile` does.
fn same_file(first: &str, second: &str) -> bool {
    let first_path = absolute(first);
    let second_path = absolute(second);
    if first_path.is_empty() || second_path.is_empty() {
        return true;
    }
    if first_path == second_path {
        return true;
    }
    if cfg!(windows) && first_path.eq_ignore_ascii_case(&second_path) {
        return true;
    }
    match (std::fs::canonicalize(&first_path), std::fs::canonicalize(&second_path)) {
        (Ok(one), Ok(other)) => one == other,
        _ => false,
    }
}

/// An absolute spelling of a path, or an empty string when none is available.
fn absolute(path: &str) -> String {
    match Path::new(path).canonicalize() {
        Ok(canonical) => canonical.to_string_lossy().to_string(),
        Err(_) => match std::env::current_dir() {
            Ok(directory) => directory.join(path).to_string_lossy().to_string(),
            Err(_) => String::new(),
        },
    }
}

/// Builds a standalone executable from a program and every module it imports.
pub fn build_standalone(source: &str, filename: &str, output: &str) -> Result<(), String> {
    let (bundle, entry) = collect_modules(source, filename)?;
    let payload = encode_bundle(&bundle, &entry);
    let executable = std::env::current_exe().map_err(|error| format!("cannot locate the tiger executable: {}", error))?;
    let mut data = std::fs::read(&executable).map_err(|error| error.to_string())?;
    data.extend_from_slice(&payload);
    data.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    data.extend_from_slice(MAGIC);
    let temporary = PathBuf::from(format!("{}.building", output));
    std::fs::write(&temporary, &data).map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(output);
    match std::fs::rename(&temporary, output) {
        Ok(()) => Ok(()),
        Err(_) => {
            let result = std::fs::copy(&temporary, output).map(|_| ()).map_err(|error| error.to_string());
            let _ = std::fs::remove_file(&temporary);
            result
        }
    }
}

/// Collects the entry source and every module it imports, resolving each import
/// the way the interpreter would.
fn collect_modules(source: &str, filename: &str) -> Result<(BundleLoader, String), String> {
    let loader = FileLoader;
    let entry = loader.resolve("", filename)?;
    let mut bundle = BundleLoader::default();
    collect(&mut bundle, &loader, &entry, source, 0)?;
    Ok((bundle, entry))
}

fn collect(bundle: &mut BundleLoader, loader: &FileLoader, filename: &str, source: &str, depth: usize) -> Result<(), String> {
    if bundle.sources.contains_key(filename) {
        return Ok(());
    }
    if depth > 128 {
        return Err(format!("{}: maximum import depth exceeded", filename));
    }
    let program = tiger_parser::parse_source(source, filename)?;
    bundle.sources.insert(filename.to_string(), source.to_string());
    bundle.paths.insert(filename.to_string(), HashMap::new());
    for declaration in tiger_ast::import_decls(&program) {
        let dependency = loader.resolve(filename, &declaration.path).map_err(|error| declaration.base.errorf(&error))?;
        if let Some(paths) = bundle.paths.get_mut(filename) {
            paths.insert(declaration.path.clone(), dependency.clone());
        }
        if bundle.sources.contains_key(&dependency) {
            continue;
        }
        let content =
            loader.load(&dependency).map_err(|error| declaration.base.errorf(&format!("cannot import {:?}: {}", dependency, error)))?;
        collect(bundle, loader, &dependency, &content, depth + 1)?;
    }
    Ok(())
}

fn put_count(out: &mut Vec<u8>, count: usize) {
    out.extend_from_slice(&(count as u32).to_le_bytes());
}

fn put_text(out: &mut Vec<u8>, text: &str) {
    put_count(out, text.len());
    out.extend_from_slice(text.as_bytes());
}

/// Serializes a bundle: the entry name, then the sources and the import maps,
/// each sorted so a build is reproducible.
fn encode_bundle(bundle: &BundleLoader, entry: &str) -> Vec<u8> {
    let mut out = Vec::new();
    put_text(&mut out, entry);
    let mut sources: Vec<(&String, &String)> = bundle.sources.iter().collect();
    sources.sort();
    put_count(&mut out, sources.len());
    for (name, source) in sources {
        put_text(&mut out, name);
        put_text(&mut out, source);
    }
    let mut importers: Vec<(&String, &HashMap<String, String>)> = bundle.paths.iter().collect();
    importers.sort_by(|left, right| left.0.cmp(right.0));
    put_count(&mut out, importers.len());
    for (importer, paths) in importers {
        put_text(&mut out, importer);
        let mut entries: Vec<(&String, &String)> = paths.iter().collect();
        entries.sort();
        put_count(&mut out, entries.len());
        for (requested, resolved) in entries {
            put_text(&mut out, requested);
            put_text(&mut out, resolved);
        }
    }
    out
}

/// Reads back a bundle written by [`encode_bundle`].
fn decode_bundle(payload: &[u8]) -> Result<(BundleLoader, String), String> {
    let mut at = 0usize;
    let entry = take_text(payload, &mut at)?;
    let mut bundle = BundleLoader::default();
    for _ in 0..take_count(payload, &mut at)? {
        let name = take_text(payload, &mut at)?;
        let source = take_text(payload, &mut at)?;
        bundle.sources.insert(name, source);
    }
    for _ in 0..take_count(payload, &mut at)? {
        let importer = take_text(payload, &mut at)?;
        let mut paths = HashMap::new();
        for _ in 0..take_count(payload, &mut at)? {
            let requested = take_text(payload, &mut at)?;
            let resolved = take_text(payload, &mut at)?;
            paths.insert(requested, resolved);
        }
        bundle.paths.insert(importer, paths);
    }
    Ok((bundle, entry))
}

fn take_count(payload: &[u8], at: &mut usize) -> Result<usize, String> {
    if *at + 4 > payload.len() {
        return Err("the embedded bundle is truncated".to_string());
    }
    let mut count = [0u8; 4];
    count.copy_from_slice(&payload[*at..*at + 4]);
    *at += 4;
    Ok(u32::from_le_bytes(count) as usize)
}

fn take_text(payload: &[u8], at: &mut usize) -> Result<String, String> {
    let length = take_count(payload, at)?;
    if *at + length > payload.len() {
        return Err("the embedded bundle is truncated".to_string());
    }
    let text = String::from_utf8_lossy(&payload[*at..*at + length]).to_string();
    *at += length;
    Ok(text)
}

/// Locates the bundle in a built executable, or `None` for a plain `tiger`.
pub fn find_payload(bytes: &[u8]) -> Option<&[u8]> {
    let trailer = MAGIC.len() + 8;
    if bytes.len() < trailer {
        return None;
    }
    let magic_at = bytes.len() - MAGIC.len();
    if &bytes[magic_at..] != MAGIC {
        return None;
    }
    let length_at = magic_at - 8;
    let mut length = [0u8; 8];
    length.copy_from_slice(&bytes[length_at..magic_at]);
    let length = u64::from_le_bytes(length) as usize;
    if length > length_at {
        return None;
    }
    Some(&bytes[length_at - length..length_at])
}

/// Runs the program embedded in a built executable.
pub fn run_embedded(payload: &[u8], stdin: Option<Box<dyn BufRead>>, stdout: Box<dyn Write>, stderr: Box<dyn Write>) -> i32 {
    let mut stderr = stderr;
    let (bundle, entry) = match decode_bundle(payload) {
        Ok(pair) => pair,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            return 1;
        }
    };
    let source = bundle.sources.get(&entry).cloned().unwrap_or_default();
    match tiger_evaluator::run_with_input(&source, &entry, Some(Rc::new(bundle)), None, stdin, stdout) {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(stderr, "{}", error);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Collects written bytes so a test can inspect them.
    struct Shared(Rc<RefCell<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(data);
            Ok(data.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn invoke(args: &[&str], input: &str) -> (i32, String, String) {
        let out = Rc::new(RefCell::new(Vec::new()));
        let err = Rc::new(RefCell::new(Vec::new()));
        let arguments: Vec<String> = args.iter().map(|value| value.to_string()).collect();
        let code = run(
            &arguments,
            Some(Box::new(std::io::Cursor::new(input.as_bytes().to_vec()))),
            Box::new(Shared(out.clone())),
            Box::new(Shared(err.clone())),
        );
        let out_text = String::from_utf8_lossy(&out.borrow()).to_string();
        let err_text = String::from_utf8_lossy(&err.borrow()).to_string();
        (code, out_text, err_text)
    }

    fn temporary(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("tiger-cli-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("temporary directory");
        directory
    }

    #[test]
    fn runs_and_reports_usage() {
        let directory = temporary("basic");
        let valid = directory.join("valid.tg");
        let invalid = directory.join("invalid.tg");
        let greet = directory.join("greet.tg");
        std::fs::write(&valid, "print(\"hello\");").expect("write valid");
        std::fs::write(&invalid, "print(\"hello\")").expect("write invalid");
        std::fs::write(&greet, "print(\"hi \" + input(\"name: \"));").expect("write greet");
        let valid = valid.to_string_lossy().to_string();
        let invalid = invalid.to_string_lossy().to_string();
        let greet = greet.to_string_lossy().to_string();

        let (code, out, err) = invoke(&["help"], "");
        assert_eq!((code, out.contains("Usage:"), err.is_empty()), (0, true, true));

        let (code, out, err) = invoke(&[], "");
        assert_eq!((code, out.is_empty(), err.contains("Usage:")), (2, true, true));

        let (code, _, err) = invoke(&["unknown", &valid], "");
        assert_eq!((code, err.contains("Usage:")), (2, true));

        let (code, out, err) = invoke(&["run", &valid], "");
        assert_eq!((code, out.as_str(), err.is_empty()), (0, "hello\n", true));

        let (code, _, err) = invoke(&["run", &invalid], "");
        assert_eq!((code, err.contains(":1:")), (1, true));

        let (code, _, err) = invoke(&["run", "missing.tg"], "");
        assert_eq!((code, err.contains("missing.tg")), (1, true));

        let (code, _, err) = invoke(&["run", "wrong.py"], "");
        assert_eq!((code, err.contains(".tg")), (2, true));

        let (code, _, err) = invoke(&["build", &valid, "-o"], "");
        assert_eq!((code, err.contains("output path")), (2, true));

        let (code, _, err) = invoke(&["build", &valid, "-o", &valid], "");
        assert_eq!((code, err.contains("different from the input")), (2, true));

        let (code, _, err) = invoke(&["run", &valid, "extra"], "");
        assert_eq!((code, err.contains("Usage:")), (2, true));

        let (code, out, err) = invoke(&["run", &greet], "Ada\n");
        assert_eq!((code, out.as_str(), err.is_empty()), (0, "name: hi Ada\n", true));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn runs_programs_with_imports() {
        let directory = temporary("imports");
        std::fs::create_dir_all(directory.join("lib")).expect("library directory");
        std::fs::write(directory.join("main.tg"), "import \"lib/math.tg\" as math; print(math.square(9));").expect("write entry");
        std::fs::write(directory.join("lib").join("math.tg"), "function square(value) { return value * value; }").expect("write library");
        let entry = directory.join("main.tg").to_string_lossy().to_string();
        let (code, out, err) = invoke(&["run", &entry], "");
        assert_eq!((code, out.as_str(), err.as_str()), (0, "81\n", ""));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn reads_back_an_embedded_bundle() {
        let mut bundle = BundleLoader::default();
        bundle.sources.insert("a.tg".to_string(), "print(1);".to_string());
        bundle.sources.insert("b.tg".to_string(), "print(2);".to_string());
        bundle.paths.insert("a.tg".to_string(), HashMap::from([("b.tg".to_string(), "b.tg".to_string())]));
        let payload = encode_bundle(&bundle, "a.tg");
        let (decoded, entry) = decode_bundle(&payload).expect("decode");
        assert_eq!(entry, "a.tg");
        assert_eq!(decoded.sources, bundle.sources);
        assert_eq!(decoded.paths, bundle.paths);
    }

    #[test]
    fn finds_the_trailer_of_a_built_executable() {
        let mut bytes = b"an executable".to_vec();
        let payload = b"bundle bytes";
        bytes.extend_from_slice(payload);
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(MAGIC);
        assert_eq!(find_payload(&bytes), Some(&payload[..]));
        assert_eq!(find_payload(b"an executable"), None);
        assert_eq!(find_payload(b"tiger-bundle-v1\n"), None);
    }
}
