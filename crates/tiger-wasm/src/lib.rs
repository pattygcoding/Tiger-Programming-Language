//! The browser runtime: a `wasm32-unknown-unknown` module the playground drives.
//!
//! A port of `cmd/wasm/main_js.go`. It keeps the same surface — the JavaScript
//! glue exposes `tigerRun(source, filename)` and `tigerProvideInput(text)`, and
//! the runtime calls back through `tigerOutput`, `tigerRequestInput`,
//! `tigerResolveModule`, and `tigerLoadModule` — but the host services are
//! plain `extern "C"` imports rather than `syscall/js`, so the module needs no
//! bindgen toolchain and no JavaScript glue code from Rust.
//!
//! The one structural difference from the Go runtime is how `input()` blocks.
//! Go parks the calling goroutine and lets its event loop keep running; Rust on
//! `wasm32-unknown-unknown` has no goroutines, so the imported `tiger_input`
//! blocks inside JavaScript instead (the playground's worker waits on a
//! `SharedArrayBuffer`), and the program simply resumes when the line arrives.
//! The observable behaviour is the same.
//!
//! ## The ABI
//!
//! Exports (called by the JavaScript glue):
//!
//! * `tiger_alloc(size)` / `tiger_free(pointer, size)` — hand strings to Rust.
//! * `tiger_run(source_ptr, source_len, filename_ptr, filename_len)` — run a
//!   program, then read the result with `tiger_result_ptr`/`tiger_result_len`.
//!   The result is `[u32 output_len][output][u32 error_len][error]`, both
//!   lengths little-endian.
//!
//! Imports (provided by the JavaScript glue):
//!
//! * `tiger_output(ptr, len)` — stream a program's output.
//! * `tiger_input()` / `tiger_input_ptr()` — the next line's UTF-8 bytes, or `-1`
//!   at end of input. The host owns the buffer and may reuse it.
//! * `tiger_resolve(importer, requested)` / `tiger_load(filename)` — module
//!   loading; each returns `0` on success and `1` on failure, leaving the text
//!   in `tiger_response_ptr`/`tiger_response_len`.
//!
//! The crate is empty on every other target, so a plain `cargo build` does not
//! try to link the browser's host imports.

#![cfg(all(target_arch = "wasm32", target_os = "unknown"))]

use std::cell::RefCell;
use std::io::{BufRead, Error, ErrorKind, Read, Write};
use std::rc::Rc;

use tiger_evaluator::{FileSystem, MemoryFileSystem, ModuleLoader};

/// The output cap the Go runtime applies, in bytes.
const OUTPUT_LIMIT: usize = 1 << 20;

#[link(wasm_import_module = "tiger")]
extern "C" {
    fn tiger_output(text: *const u8, length: usize);
    fn tiger_input() -> i32;
    fn tiger_input_ptr() -> *const u8;
    /// Whether the page can deliver input and load modules.
    fn tiger_has_input() -> i32;
    fn tiger_has_loader() -> i32;
    fn tiger_resolve(importer: *const u8, importer_length: usize, requested: *const u8, requested_length: usize) -> i32;
    fn tiger_load(filename: *const u8, filename_length: usize) -> i32;
    fn tiger_response_ptr() -> *const u8;
    fn tiger_response_len() -> usize;
}

/// Copies `length` bytes out of WebAssembly memory.
unsafe fn read_text(pointer: *const u8, length: usize) -> String {
    if pointer.is_null() || length == 0 {
        return String::new();
    }
    let bytes = std::slice::from_raw_parts(pointer, length);
    String::from_utf8_lossy(bytes).to_string()
}

/// Copies `length` bytes out of WebAssembly memory.
unsafe fn read_bytes(pointer: *const u8, length: usize) -> Vec<u8> {
    if pointer.is_null() || length == 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts(pointer, length).to_vec()
}

/// Gives the glue a block of WebAssembly memory to write into.
#[no_mangle]
pub extern "C" fn tiger_alloc(size: usize) -> *mut u8 {
    if size == 0 {
        return std::ptr::null_mut();
    }
    unsafe { std::alloc::alloc(std::alloc::Layout::from_size_align_unchecked(size, 1)) }
}

/// Releases a block from [`tiger_alloc`]; `size` must match the request.
#[no_mangle]
pub extern "C" fn tiger_free(pointer: *mut u8, size: usize) {
    if pointer.is_null() || size == 0 {
        return;
    }
    unsafe { std::alloc::dealloc(pointer, std::alloc::Layout::from_size_align_unchecked(size, 1)) }
}

thread_local! {
    /// The last run's encoded result, read through the pointer exports.
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn tiger_result_ptr() -> *const u8 {
    RESULT.with(|cell| cell.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn tiger_result_len() -> usize {
    RESULT.with(|cell| cell.borrow().len())
}

thread_local! {
    /// The current run's output, streamed to the page and capped at 1 MiB.
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Streams output to the page and remembers it for the run's result.
struct BrowserOutput;

impl Write for BrowserOutput {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        OUTPUT.with(|cell| {
            let mut written = cell.borrow_mut();
            if written.len() + data.len() > OUTPUT_LIMIT {
                return Err(Error::new(ErrorKind::Other, "output limit exceeded (1 MiB)"));
            }
            unsafe { tiger_output(data.as_ptr(), data.len()) };
            written.extend_from_slice(data);
            Ok(data.len())
        })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Reads lines from the page, blocking inside the host until each one arrives.
///
/// A negative length means the page ended input, which surfaces as the same
/// "EOF when reading a line" error the native runtime reports.
#[derive(Default)]
struct BrowserInput {
    pending: Vec<u8>,
    position: usize,
}

impl BufRead for BrowserInput {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.position >= self.pending.len() {
            self.pending.clear();
            self.position = 0;
            let length = unsafe { tiger_input() };
            if length < 0 {
                return Ok(&[]);
            }
            self.pending = unsafe { read_bytes(tiger_input_ptr(), length as usize) };
            self.pending.push(b'\n');
        }
        Ok(&self.pending[self.position..])
    }

    fn consume(&mut self, amount: usize) {
        self.position = (self.position + amount).min(self.pending.len());
    }
}

impl Read for BrowserInput {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = {
            let available = self.fill_buf()?;
            let count = available.len().min(buffer.len());
            buffer[..count].copy_from_slice(&available[..count]);
            count
        };
        self.consume(count);
        Ok(count)
    }
}

/// Resolves and loads modules through the page, which fetches them itself.
struct BrowserLoader;

impl ModuleLoader for BrowserLoader {
    fn resolve(&self, importer: &str, requested: &str) -> Result<String, String> {
        let status = unsafe { tiger_resolve(importer.as_ptr(), importer.len(), requested.as_ptr(), requested.len()) };
        response(status)
    }

    fn load(&self, filename: &str) -> Result<String, String> {
        let status = unsafe { tiger_load(filename.as_ptr(), filename.len()) };
        response(status)
    }
}

/// Reads the text the last `tiger_resolve` or `tiger_load` left behind.
fn response(status: i32) -> Result<String, String> {
    let length = unsafe { tiger_response_len() };
    let text = unsafe { read_text(tiger_response_ptr(), length) };
    if status == 0 {
        Ok(text)
    } else {
        Err(text)
    }
}

/// Runs one program, returning its error and leaving its output in [`OUTPUT`].
fn execute(source: &str, filename: &str) -> Result<(), String> {
    OUTPUT.with(|cell| cell.borrow_mut().clear());
    let input: Option<Box<dyn BufRead>> = if unsafe { tiger_has_input() } != 0 { Some(Box::new(BrowserInput::default())) } else { None };
    let loader: Option<Rc<dyn ModuleLoader>> = if unsafe { tiger_has_loader() } != 0 { Some(Rc::new(BrowserLoader)) } else { None };
    let mut filename = filename.to_string();
    if let Some(loader) = &loader {
        filename = loader.resolve("", &filename)?;
    }
    let filesystem: Option<Box<dyn FileSystem>> = Some(Box::new(MemoryFileSystem::default()));
    tiger_evaluator::run_with_input(source, &filename, loader, filesystem, input, Box::new(BrowserOutput))
}

/// Runs a program for the page and encodes `{output, error}` for the glue.
#[no_mangle]
pub extern "C" fn tiger_run(source_ptr: *const u8, source_len: usize, filename_ptr: *const u8, filename_len: usize) -> i32 {
    let source = unsafe { read_text(source_ptr, source_len) };
    let filename = unsafe { read_text(filename_ptr, filename_len) };
    let error = execute(&source, &filename).err().unwrap_or_default();
    let output = OUTPUT.with(|cell| cell.borrow().clone());
    RESULT.with(|cell| {
        let mut result = cell.borrow_mut();
        result.clear();
        result.extend_from_slice(&(output.len() as u32).to_le_bytes());
        result.extend_from_slice(&output);
        result.extend_from_slice(&(error.len() as u32).to_le_bytes());
        result.extend_from_slice(error.as_bytes());
    });
    0
}
