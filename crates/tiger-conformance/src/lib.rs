//! The differential conformance harness.
//!
//! It drives the repository's own fixtures — the 42 benchmark programs, the
//! `tg`/`text`/`error`/`input` fences in `docs/*.md` and `README.md`, and the
//! `examples/` and `portfolio-features/` programs — through the Rust
//! interpreter, replicating the Go tests (`benchmarks/benchmarks_test.go`,
//! `docs/docs_test.go`, and `cmd/tiger/main_test.go`) exactly.
//!
//! Deep recursion needs a large stack, so every program runs on its own thread
//! with a 64 MiB stack, the same arrangement the CLI uses.

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::rc::Rc;

pub use tiger_evaluator::{Engine, FileLoader, FileSystem, MemoryFileSystem, ModuleLoader, RunOptions};

/// The repository root, two directories above this crate's manifest.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The CRLF-to-LF normalization every fixture comparison uses.
pub fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Collects written bytes so a caller can compare them.
pub struct Shared(pub Rc<RefCell<Vec<u8>>>);

impl Shared {
    pub fn new() -> Shared {
        Shared(Rc::new(RefCell::new(Vec::new())))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow()).to_string()
    }
}

impl Write for Shared {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Which filesystem a program sees: the hermetic one, or the host's.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Files {
    Memory,
    Host,
}

/// What a program produced.
pub struct Outcome {
    pub output: String,
    pub error: Option<String>,
}

/// Runs a program on a big-stack thread and reports its output and error.
///
/// `loader` selects whether `import` is available, mirroring the Go tests that
/// pass either a `FileLoader` or `nil`.
pub fn run(source: &str, filename: &str, files: Files, loader: bool, input: Option<&str>) -> Outcome {
    run_with(source, filename, files, loader, input, RunOptions::default())
}

/// Runs a benchmark-style program whose imports resolve inside `directory`.
///
/// The benchmark corpus keeps its programs' names bare, so a diagnostic quotes
/// them the way the checked-in `.txt` files do, and its helper modules live in
/// `benchmarks/support/`. This loader resolves `import "support/x.tg"` from the
/// program's own file name the way `tiger run benchmarks/NN.tg` does from its path.
pub fn run_in_directory(
    source: &str,
    filename: &str,
    directory: PathBuf,
    files: Files,
    input: Option<&str>,
    options: RunOptions,
) -> Outcome {
    run_loaded(source, filename, files, input, options, Some(directory))
}

/// Runs a program on a chosen engine with no step limit.
///
/// The differential tests run every fixture this way under [`Engine::Tree`] and
/// compare the result with the same fixture under [`Engine::Auto`], so the two
/// engines are compared on semantics rather than on how they count steps.
pub fn run_engine(source: &str, filename: &str, files: Files, loader: bool, input: Option<&str>, engine: Engine) -> Outcome {
    run_with(source, filename, files, loader, input, RunOptions { engine, max_steps: 0, arguments: Vec::new() })
}

/// Runs a program with explicit engine options on a big-stack thread.
pub fn run_with(source: &str, filename: &str, files: Files, loader: bool, input: Option<&str>, options: RunOptions) -> Outcome {
    let directory = if loader { Some(PathBuf::new()) } else { None };
    run_loaded(source, filename, files, input, options, directory)
}

/// Runs a program, optionally with a loader that resolves its imports.
///
/// `directory` is `None` for no loader at all, which makes every import fail;
/// [`PathBuf::new`] means the host filesystem, where a relative path resolves
/// against the importing file as the CLI does it.
fn run_loaded(source: &str, filename: &str, files: Files, input: Option<&str>, options: RunOptions, directory: Option<PathBuf>) -> Outcome {
    let source = source.to_string();
    let filename = filename.to_string();
    let input = input.map(str::to_string);
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let buffer = Shared::new();
            let filesystem: Option<Box<dyn FileSystem>> = match files {
                Files::Memory => Some(Box::new(MemoryFileSystem::default())),
                Files::Host => None,
            };
            let reader: Option<Box<dyn BufRead>> = input.map(|text| Box::new(std::io::Cursor::new(text.into_bytes())) as Box<dyn BufRead>);
            let modules: Option<Rc<dyn ModuleLoader>> = match directory {
                None => None,
                Some(directory) if directory.as_os_str().is_empty() => Some(Rc::new(FileLoader)),
                Some(directory) => Some(Rc::new(DirectoryLoader { directory })),
            };
            let error = tiger_evaluator::run_with_options(
                &source,
                &filename,
                modules,
                filesystem,
                reader,
                Box::new(Shared(buffer.0.clone())),
                options,
            )
            .err();
            Outcome { output: buffer.text(), error }
        })
        .expect("the interpreter thread must start");
    match handle.join() {
        Ok(outcome) => outcome,
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            Outcome { output: String::new(), error: Some(format!("interpreter panicked: {}", message)) }
        }
    }
}

/// A loader that resolves a program's imports inside one directory.
///
/// A resolved module's own imports stay relative to that module, so a helper can
/// import its neighbours.
pub struct DirectoryLoader {
    pub directory: PathBuf,
}

impl ModuleLoader for DirectoryLoader {
    fn resolve(&self, importer: &str, requested: &str) -> Result<String, String> {
        if !requested.ends_with(".tg") {
            return Err("import path must end in .tg".to_string());
        }
        let parent = std::path::Path::new(importer).parent().filter(|parent| !parent.as_os_str().is_empty());
        let base = parent.map(|parent| parent.to_path_buf()).unwrap_or_else(|| self.directory.clone());
        let joined = base.join(requested.replace('/', std::path::MAIN_SEPARATOR_STR));
        let canonical = std::fs::canonicalize(&joined).map_err(|error| format!("open {}: {}", joined.display(), error))?;
        Ok(canonical.to_string_lossy().trim_start_matches(r"\\?\").to_string())
    }

    fn load(&self, filename: &str) -> Result<String, String> {
        std::fs::read_to_string(filename).map_err(|error| format!("open {}: {}", filename, error))
    }
}

/// One fenced block in a Markdown document.
pub struct Fence {
    pub language: String,
    pub content: String,
    pub line: usize,
}

/// Splits a Markdown document into fences, mirroring Go's `bufio.Scanner`:
/// a line beginning with ```` ``` ```` opens a fence whose language is the
/// trimmed remainder, a line that is exactly ```` ``` ```` closes it, and the
/// lines between are joined with `\n`, with any carriage return stripped.
pub fn fences(content: &str) -> Vec<Fence> {
    let mut lines: Vec<&str> = content.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut blocks: Vec<Fence> = Vec::new();
    let mut language: Option<String> = None;
    let mut buffer = String::new();
    let mut start = 0usize;
    for (index, raw) in lines.iter().enumerate() {
        let text = raw.strip_suffix('\r').unwrap_or(raw);
        if language.is_none() {
            if let Some(rest) = text.strip_prefix("```") {
                language = Some(rest.trim().to_string());
                buffer.clear();
                start = index + 1;
            }
        } else if text == "```" {
            blocks.push(Fence { language: language.take().unwrap(), content: std::mem::take(&mut buffer), line: start });
        } else {
            buffer.push_str(text);
            buffer.push('\n');
        }
    }
    blocks
}

/// Every Markdown document the documentation tests cover: `README.md` first,
/// then `docs/*.md` in sorted order, as Go's `filepath.Glob` returns them.
pub fn markdown_paths() -> Vec<PathBuf> {
    let root = root();
    let mut paths = vec![root.join("README.md")];
    let mut documents: Vec<PathBuf> = std::fs::read_dir(root.join("docs"))
        .expect("docs directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().map(|extension| extension == "md").unwrap_or(false))
        .collect();
    documents.sort();
    paths.extend(documents);
    paths
}
