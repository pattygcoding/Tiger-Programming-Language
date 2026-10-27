//! The dashboard's server: static files, a state endpoint, an event stream, and the
//! endpoint that starts a run.
//!
//! Plain `std`, the same way `crates/tiger-web` serves the playground. The event
//! stream is what makes a run watchable: every section and every program is pushed
//! as it is measured, so the bars fill in while the work happens rather than after
//! it is over.
//!
//! A run holds the interpreter, whose values are `Rc`-based and not `Send`, so the
//! measurement happens on its own thread and only strings cross back. That thread is
//! given a 64 MiB stack because the paired workload recurses deeply, which is the
//! arrangement every harness in the repository uses.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::report::{self, Plan, Report};

/// Everything the server keeps between requests.
pub struct Dashboard {
    /// The last completed run, which is what a page that connects late shows.
    report: Mutex<Option<Arc<Report>>>,
    /// One sender per connected event stream.
    subscribers: Mutex<Vec<Sender<String>>>,
    /// Whether a run is in flight. One at a time: two would measure each other.
    running: AtomicBool,
    /// The plan the page's controls start from.
    plan: Mutex<Plan>,
    /// Where this dashboard's own HTML, CSS, and JavaScript live.
    assets: PathBuf,
}

impl Dashboard {
    pub fn new(assets: PathBuf, plan: Plan) -> Arc<Dashboard> {
        Arc::new(Dashboard {
            report: Mutex::new(None),
            subscribers: Mutex::new(Vec::new()),
            running: AtomicBool::new(false),
            plan: Mutex::new(plan),
            assets,
        })
    }

    /// Whether a run is in flight.
    pub fn running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// The plan a fresh page's controls should show.
    pub fn plan(&self) -> Plan {
        *self.plan.lock().expect("the plan lock is never poisoned")
    }

    /// Pushes one event to every connected stream, dropping the ones that have gone.
    ///
    /// A subscriber whose receiver has been dropped makes `send` fail, and that is
    /// how a closed browser tab is noticed: the stream's own thread exits when its
    /// keepalive write fails, which drops the receiver.
    fn broadcast(&self, payload: String) {
        self.subscribers.lock().expect("the subscriber lock is never poisoned").retain(|sender| sender.send(payload.clone()).is_ok());
    }

    /// The JSON a page reads to draw what has already happened.
    fn state_json(&self) -> String {
        let report = self
            .report
            .lock()
            .expect("the report lock is never poisoned")
            .as_ref()
            .map_or_else(|| "null".to_string(), |report| report.to_json());
        format!(
            "{{\"running\":{},\"plan\":{},\"report\":{}}}",
            if self.running() { "true" } else { "false" },
            report::plan_json(self.plan()),
            report
        )
    }

    /// Starts a run, unless one is already in flight.
    pub fn start(self: &Arc<Dashboard>, plan: Plan) -> bool {
        if self.running.swap(true, Ordering::SeqCst) {
            return false;
        }
        *self.plan.lock().expect("the plan lock is never poisoned") = plan;
        let state = Arc::clone(self);
        let spawned = std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(move || {
            state.broadcast(format!("{{\"type\":\"start\",\"plan\":{}}}", report::plan_json(plan)));
            let mut emit = |progress: report::Progress| state.broadcast(progress.to_json());
            let report = report::run(plan, &mut emit);
            let document = report.to_json();
            *state.report.lock().expect("the report lock is never poisoned") = Some(Arc::new(report));
            state.broadcast(format!("{{\"type\":\"report\",\"report\":{document}}}"));
            state.running.store(false, Ordering::SeqCst);
        });
        match spawned {
            Ok(_) => true,
            Err(error) => {
                self.running.store(false, Ordering::SeqCst);
                eprintln!("tiger-perf-dashboard: could not start the run thread: {error}");
                false
            }
        }
    }
}

/// One HTTP request, as much of it as this server needs.
struct Request {
    method: String,
    target: String,
    body: String,
}

/// Reads a request's head and, when it has one, its body.
fn read_request(reader: &mut BufReader<TcpStream>) -> std::io::Result<Request> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(Request { method: String::new(), target: String::new(), body: String::new() });
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" || header == "\n" {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(Request { method, target, body: String::from_utf8_lossy(&body).into_owned() })
}

impl Dashboard {
    /// Answers one connection and closes it.
    ///
    /// The receiver is `&Arc<Self>` rather than `&self` so that the one route which
    /// starts work — `POST /api/play` — can hand the run thread its own handle.
    pub fn handle(self: &Arc<Dashboard>, stream: TcpStream) -> std::io::Result<()> {
        let mut stream = stream;
        let mut reader = BufReader::new(stream.try_clone()?);
        let request = read_request(&mut reader)?;
        let head = request.method == "HEAD";
        let path = request.target.split(['?', '#']).next().unwrap_or("/").to_string();
        match (request.method.as_str(), path.as_str()) {
            ("GET" | "HEAD", "/api/state") => respond(&mut stream, 200, "OK", "application/json", self.state_json().as_bytes(), head),
            ("GET" | "HEAD", "/api/events") => self.events(stream),
            ("POST", "/api/play") => {
                let plan = plan_from_body(&request.body, self.plan());
                if self.start(plan) {
                    let body = format!("{{\"started\":true,\"plan\":{}}}", report::plan_json(plan));
                    respond(&mut stream, 202, "Accepted", "application/json", body.as_bytes(), false)
                } else {
                    let body = "{\"started\":false,\"reason\":\"a run is already in flight\"}";
                    respond(&mut stream, 409, "Conflict", "application/json", body.as_bytes(), false)
                }
            }
            ("GET" | "HEAD", _) => self.static_file(&mut stream, &path, head),
            _ => respond(&mut stream, 405, "Method Not Allowed", "text/plain", b"", false),
        }
    }

    /// Serves one of this dashboard's own files.
    fn static_file(&self, stream: &mut TcpStream, path: &str, head: bool) -> std::io::Result<()> {
        let relative = path.trim_start_matches('/');
        let relative = if relative.is_empty() { "index.html" } else { relative };
        // Nothing may escape the dashboard's directory, and the page is three files
        // at one level, so a name with no separator is all that is ever needed.
        if relative.contains("..") || relative.contains('\\') || relative.contains('/') {
            return respond(stream, 403, "Forbidden", "text/plain", b"", head);
        }
        match std::fs::read(self.assets.join(relative)) {
            Ok(data) => respond(stream, 200, "OK", content_type(relative), &data, head),
            Err(_) => respond(stream, 404, "Not Found", "text/plain", b"", head),
        }
    }
}

impl Dashboard {
    /// Streams every event of a run, and of the runs after it, until the client goes.
    ///
    /// A ten-second keepalive comment is what notices a closed tab: the write fails,
    /// the thread ends, and its receiver being dropped is what prunes the subscriber
    /// from the next broadcast.
    fn events(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let header = "HTTP/1.1 200 OK\r\n\
                      Content-Type: text/event-stream\r\n\
                      Cache-Control: no-cache\r\n\
                      Connection: keep-alive\r\n\
                      X-Accel-Buffering: no\r\n\r\n";
        stream.write_all(header.as_bytes())?;
        stream.flush()?;
        let (sender, receiver): (Sender<String>, Receiver<String>) = mpsc::channel();
        self.subscribers.lock().expect("the subscriber lock is never poisoned").push(sender);
        loop {
            let frame = match receiver.recv_timeout(Duration::from_secs(10)) {
                Ok(payload) => format!("data: {payload}\n\n"),
                Err(RecvTimeoutError::Timeout) => ": keepalive\n\n".to_string(),
                Err(RecvTimeoutError::Disconnected) => break,
            };
            if stream.write_all(frame.as_bytes()).and_then(|()| stream.flush()).is_err() {
                break;
            }
        }
        Ok(())
    }
}

/// Writes a response with a known length.
fn respond(stream: &mut TcpStream, status: u16, reason: &str, kind: &str, body: &[u8], head: bool) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {kind}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    if !head {
        stream.write_all(body)?;
    }
    stream.flush()
}

/// The content type a dashboard file is served with.
fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The plan a `POST /api/play` body asks for, with the previous plan's values for
/// anything it leaves out.
fn plan_from_body(body: &str, fallback: Plan) -> Plan {
    let iterations = number_field(body, "iterations")
        .filter(|value| *value >= 1.0 && *value <= f64::from(u32::MAX))
        .unwrap_or(f64::from(fallback.iterations));
    Plan {
        iterations: iterations as u32,
        corpus: boolean_field(body, "corpus").unwrap_or(fallback.corpus),
        engines: boolean_field(body, "engines").unwrap_or(fallback.engines),
    }
}

/// The text that follows `"key":` in a small, flat JSON object.
fn field(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    let rest = rest[rest.find(':')? + 1..].trim_start();
    let end = rest.find([',', '}', ' ', '\n']).unwrap_or(rest.len());
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn number_field(body: &str, key: &str) -> Option<f64> {
    field(body, key)?.parse().ok()
}

fn boolean_field(body: &str, key: &str) -> Option<bool> {
    match field(body, key)?.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_fields_a_play_request_carries() {
        let plan = plan_from_body("{\"iterations\":300,\"corpus\":true,\"engines\":false}", Plan::default());
        assert_eq!(plan.iterations, 300);
        assert!(plan.corpus);
        assert!(!plan.engines);
    }

    #[test]
    fn a_key_is_not_confused_with_a_longer_one() {
        // `engine` must not be found inside `"engines"`.
        assert_eq!(field("{\"engines\":true}", "engine"), None);
        assert_eq!(boolean_field("{\"engines\":true}", "engines"), Some(true));
    }

    #[test]
    fn a_missing_field_falls_back_to_the_previous_plan() {
        let previous = Plan { iterations: 25, corpus: false, engines: true };
        let plan = plan_from_body("{}", previous);
        assert_eq!(plan.iterations, 25);
        assert!(!plan.corpus);
        assert!(plan.engines);
    }

    #[test]
    fn an_impossible_iteration_count_is_ignored() {
        let previous = Plan::default();
        // Zero runs is not a measurement, and a negative or unreadable count is not
        // a count at all, so the previous plan stands.
        assert_eq!(plan_from_body("{\"iterations\":0}", previous).iterations, previous.iterations);
        assert_eq!(plan_from_body("{\"iterations\":-5}", previous).iterations, previous.iterations);
        assert_eq!(plan_from_body("{\"iterations\":\"many\"}", previous).iterations, previous.iterations);
        assert_eq!(plan_from_body("{\"iterations\":1e400}", previous).iterations, previous.iterations);
    }

    #[test]
    fn a_dashboard_file_gets_its_content_type() {
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(content_type("dashboard.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("dashboard.js"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("tiger.wasm"), "application/octet-stream");
    }

    #[test]
    fn a_fresh_dashboard_reports_no_run() {
        let dashboard = Dashboard::new(PathBuf::from("."), Plan::default());
        assert!(!dashboard.running());
        assert!(dashboard.state_json().contains("\"report\":null"));
    }
}
