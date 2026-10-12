//! The playground's development server.
//!
//! A port of `cmd/web`: it builds the browser runtime into `web/tiger.wasm` and
//! serves `web/`, `examples/`, and `portfolio-features/`. It also sends the
//! cross-origin isolation headers the runtime needs, because a worker can only
//! wait for `input()` on a `SharedArrayBuffer` when the page is isolated.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The address the playground prefers; it falls back to a free port when busy.
const DEFAULT_ADDRESS: &str = "127.0.0.1:7171";

fn main() {
    let mut address = DEFAULT_ADDRESS.to_string();
    let mut build_only = false;
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-addr" => match arguments.next() {
                Some(value) => address = value,
                None => {
                    eprintln!("tiger-web: -addr needs a value");
                    std::process::exit(2);
                }
            },
            "-build-only" => build_only = true,
            other => {
                eprintln!("tiger-web: unknown argument {:?}", other);
                std::process::exit(2);
            }
        }
    }
    if let Err(error) = build_wasm() {
        eprintln!("tiger-web: {}", error);
        std::process::exit(1);
    }
    if build_only {
        println!("Built web/tiger.wasm");
        return;
    }
    let (listener, address) = match TcpListener::bind(&address) {
        Ok(listener) => (listener, address.clone()),
        Err(error) if address == DEFAULT_ADDRESS => {
            println!("{} is busy ({}); using a free port instead", DEFAULT_ADDRESS, error);
            match TcpListener::bind("127.0.0.1:0") {
                Ok(listener) => {
                    let local = listener.local_addr().map(|value| value.to_string()).unwrap_or_default();
                    (listener, local)
                }
                Err(error) => {
                    eprintln!("tiger-web: {}", error);
                    std::process::exit(1);
                }
            }
        }
        Err(error) => {
            eprintln!("tiger-web: {}", error);
            std::process::exit(1);
        }
    };
    println!("Tiger playground: http://{}", address);
    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                std::thread::spawn(move || {
                    let _ = serve(stream);
                });
            }
            Err(error) => eprintln!("tiger-web: {}", error),
        }
    }
}

/// The repository root, two directories above this crate's manifest.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Builds `web/tiger.wasm` from the browser runtime.
///
/// The Rust equivalent of `cmd/web`'s build step: a release build for
/// `wasm32-unknown-unknown`, stripped by the release profile, then `wasm-opt`
/// when Binaryen is on `PATH`.
fn build_wasm() -> Result<(), String> {
    let root = root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(&cargo)
        .args(["build", "-p", "tiger-wasm", "--release", "--target", "wasm32-unknown-unknown"])
        .current_dir(&root)
        .status()
        .map_err(|error| format!("run cargo: {}", error))?;
    if !status.success() {
        return Err("the browser runtime failed to build".to_string());
    }
    let built = root.join("target").join("wasm32-unknown-unknown").join("release").join("tiger_wasm.wasm");
    let destination = root.join("web").join("tiger.wasm");
    std::fs::copy(&built, &destination).map_err(|error| format!("copy {}: {}", built.display(), error))?;
    match Command::new("wasm-opt")
        .args(["-Oz", "--converge", "--strip-debug", "--strip-dwarf", "--strip-producers"])
        .arg(&destination)
        .arg("-o")
        .arg(root.join("web").join("tiger.wasm.opt"))
        .status()
    {
        Ok(status) if status.success() => {
            std::fs::rename(root.join("web").join("tiger.wasm.opt"), &destination)
                .map_err(|error| format!("replace tiger.wasm: {}", error))?;
            println!("Optimized web/tiger.wasm with wasm-opt -Oz");
        }
        Ok(_) => {
            let _ = std::fs::remove_file(root.join("web").join("tiger.wasm.opt"));
            println!("wasm-opt skipped: it reported a failure");
        }
        Err(_) => {}
    }
    Ok(())
}

/// Serves one request and closes the connection.
fn serve(stream: TcpStream) -> std::io::Result<()> {
    let mut stream = stream;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    if reader.read_line(&mut request)? == 0 {
        return Ok(());
    }
    // Drain the headers so the client's write side finishes.
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" || header == "\n" {
            break;
        }
    }
    let mut parts = request.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    if method != "GET" && method != "HEAD" {
        return respond(&mut stream, 405, "Method Not Allowed", "text/plain", b"", false);
    }
    let path = target.split(['?', '#']).next().unwrap_or("/");
    match resolve(path) {
        None => respond(&mut stream, 403, "Forbidden", "text/plain", b"", false),
        Some(file) => match std::fs::read(&file) {
            Ok(data) => respond(&mut stream, 200, "OK", content_type(&file), &data, method == "HEAD"),
            Err(_) => respond(&mut stream, 404, "Not Found", "text/plain", b"", false),
        },
    }
}

/// Maps a request path onto a file, refusing anything that escapes its tree.
fn resolve(target: &str) -> Option<PathBuf> {
    let root = root();
    let relative = target.trim_start_matches('/');
    let relative = if relative.is_empty() { "index.html" } else { relative };
    if relative.split('/').any(|segment| segment == ".." || segment == "." || segment.contains('\\')) {
        return None;
    }
    let (directory, rest) = match relative.split_once('/') {
        Some(("examples", rest)) => (root.join("examples"), rest),
        Some(("portfolio-features", rest)) => (root.join("portfolio-features"), rest),
        _ => (root.join("web"), relative),
    };
    if rest.is_empty() {
        return None;
    }
    Some(directory.join(rest))
}

/// The content type a file is served with.
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()).unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "txt" | "tg" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Writes a response. The isolation headers let the worker use a
/// `SharedArrayBuffer`, which is what makes `input()` work.
fn respond(stream: &mut TcpStream, status: u16, reason: &str, kind: &str, body: &[u8], head: bool) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-cache\r\n\
         Cross-Origin-Opener-Policy: same-origin\r\n\
         Cross-Origin-Embedder-Policy: require-corp\r\n\
         Cross-Origin-Resource-Policy: same-origin\r\n\
         Connection: close\r\n\r\n",
        status,
        reason,
        kind,
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    if !head {
        stream.write_all(body)?;
    }
    stream.flush()
}
