//! The `api` module: HTTP requests against a loopback server, and the pure URL
//! and query helpers.
//!
//! Every request test starts a real `TcpListener` on a free loopback port, runs
//! the program against it, and checks what the server received and what the
//! program printed. Nothing reaches the network beyond `127.0.0.1`, so the suite
//! stays hermetic.

mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::JoinHandle;

use common::{expect_errors, expect_outputs, output};

/// A loopback server that answers a fixed number of connections with a handler.
struct Server {
    port: u16,
    handle: JoinHandle<()>,
}

impl Server {
    /// Starts a server that answers `requests` connections with `handler`.
    fn start(handler: impl Fn(&str) -> String + Send + 'static, requests: usize) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("loopback address").port();
        let handle = std::thread::spawn(move || {
            for _ in 0..requests {
                let Ok((mut stream, _)) = listener.accept() else { break };
                let request = read_request(&mut stream);
                let response = handler(&request);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        Server { port, handle }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    fn finish(self) {
        let _ = self.handle.join();
    }
}

/// Reads one HTTP request: the head, then the body named by Content-Length.
fn read_request(stream: &mut TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                if let Some(end) = find(&buffer, b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buffer[..end]).to_string();
                    if buffer.len() >= end + 4 + content_length(&head) {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buffer).to_string()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn content_length(head: &str) -> usize {
    head.lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("content-length") {
                value.trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(0)
}

/// Builds a response with a plain-text body.
fn respond(status: u16, reason: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        reason,
        body.len(),
        body
    )
}

#[test]
fn api_helpers_are_pure() {
    expect_outputs(&[
        (
            r#"const parsed = api.parse_url("http://example.com:8080/a/b?x=1#top");
print(parsed["scheme"], parsed["host"], parsed["port"]);
print(parsed["path"], parsed["query"], parsed["fragment"]);"#,
            "http example.com 8080\n/a/b x=1 top\n",
        ),
        (r#"print(api.parse_url("https://example.com")["port"], api.parse_url("https://example.com")["scheme"]);"#, "443 https\n"),
        (
            r#"print(api.encode_query({"q": "two words", "n": 3, "ok": true, "tag": ["a", "b"]}));"#,
            "q=two%20words&n=3&ok=true&tag=a&tag=b\n",
        ),
        (r#"print(api.encode_query({}));"#, "\n"),
    ]);
}

#[test]
fn api_helper_errors() {
    expect_errors(&[
        (r#"api.parse_url("example.com");"#, "invalid URL"),
        (r#"api.parse_url("ftp://example.com");"#, "unsupported URL scheme"),
        (r#"api.parse_url("http://:80/");"#, "missing host"),
        (r#"api.parse_url("http://example.com:notaport/");"#, "invalid port"),
        (r#"api.parse_url();"#, "api.parse_url expects 1 argument"),
        (r#"api.parse_url("http://a", "b");"#, "api.parse_url expects 1 argument"),
        (r#"api.parse_url(url="http://a");"#, "api.parse_url does not accept keyword arguments"),
        (r#"api.encode_query(["a"]);"#, "api.encode_query expects a dictionary"),
        (r#"api.encode_query({1: "a"});"#, "api.encode_query expects string keys"),
    ]);
}

#[test]
fn api_get_reads_a_response() {
    let server = Server::start(
        |request| {
            assert!(request.starts_with("GET /hello?x=1 HTTP/1.1\r\n"), "request: {:?}", request);
            respond(200, "OK", "hello")
        },
        1,
    );
    let source = format!(
        r#"const response = api.get("{}");
print(response["status"], response["reason"], response["ok"]);
print(response["headers"]["content-type"]);
print(response["body"]);"#,
        server.url("/hello?x=1")
    );
    let text = output(&source);
    server.finish();
    assert_eq!(text, "200 OK true\ntext/plain\nhello\n");
}

#[test]
fn api_post_sends_a_json_body() {
    let server = Server::start(
        |request| {
            assert!(request.starts_with("POST /items HTTP/1.1\r\n"), "request: {:?}", request);
            assert!(request.to_ascii_lowercase().contains("content-type: application/json"), "request: {:?}", request);
            assert!(request.ends_with(r#"{"name":"bolt"}"#), "request: {:?}", request);
            respond(201, "Created", "stored")
        },
        1,
    );
    let source = format!(
        r#"const response = api.post("{}", json={{"name": "bolt"}});
print(response["status"], response["reason"], response["ok"]);
print(response["body"]);"#,
        server.url("/items")
    );
    let text = output(&source);
    server.finish();
    assert_eq!(text, "201 Created true\nstored\n");
}

#[test]
fn api_post_sends_a_raw_body() {
    let server = Server::start(
        |request| {
            assert!(request.starts_with("POST /raw HTTP/1.1\r\n"), "request: {:?}", request);
            assert!(request.ends_with("plain text"), "request: {:?}", request);
            respond(200, "OK", "ok")
        },
        1,
    );
    let source = format!(
        r#"const response = api.post("{}", body="plain text", headers={{"content-type": "text/plain"}});
print(response["status"]);"#,
        server.url("/raw")
    );
    let text = output(&source);
    server.finish();
    assert_eq!(text, "200\n");
}

#[test]
fn api_other_methods_reach_the_server() {
    let server = Server::start(
        |request| {
            let line = request.lines().next().unwrap_or("").to_string();
            let method = line.split_whitespace().next().unwrap_or("").to_string();
            respond(200, "OK", &method)
        },
        4,
    );
    let source = format!(
        r#"print(api.put("{0}", body="x")["body"]);
print(api.patch("{0}", body="x")["body"]);
print(api.delete("{0}")["body"]);
print(api.head("{0}")["status"]);"#,
        server.url("/thing")
    );
    let text = output(&source);
    server.finish();
    assert_eq!(text, "PUT\nPATCH\nDELETE\n200\n");
}

#[test]
fn api_follows_redirects() {
    let server = Server::start(
        |request| {
            if request.starts_with("GET /start HTTP/1.1\r\n") {
                "HTTP/1.1 302 Found\r\nLocation: /end\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
            } else {
                assert!(request.starts_with("GET /end HTTP/1.1\r\n"), "request: {:?}", request);
                respond(200, "OK", "arrived")
            }
        },
        2,
    );
    let source = format!(
        r#"const response = api.get("{}");
print(response["status"], response["ok"], response["body"]);"#,
        server.url("/start")
    );
    let text = output(&source);
    server.finish();
    assert_eq!(text, "200 true arrived\n");
}

#[test]
fn api_stops_following_redirects_when_asked() {
    let server = Server::start(|_| "HTTP/1.1 302 Found\r\nLocation: /end\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(), 1);
    let source = format!(
        r#"const response = api.get("{}", follow_redirects=false);
print(response["status"], response["ok"], response["headers"]["location"]);"#,
        server.url("/start")
    );
    let text = output(&source);
    server.finish();
    assert_eq!(text, "302 true /end\n");
}

#[test]
fn api_reports_a_non_2xx_response_as_not_ok() {
    let server = Server::start(|_| respond(404, "Not Found", "missing"), 1);
    let source = format!(
        r#"const response = api.get("{}");
print(response["status"], response["reason"], response["ok"]);
print(response["body"]);"#,
        server.url("/gone")
    );
    let text = output(&source);
    server.finish();
    assert_eq!(text, "404 Not Found false\nmissing\n");
}

/// A port that was just bound and released, so a connection to it is refused.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("loopback address").port();
    drop(listener);
    port
}

#[test]
fn api_errors_are_catchable_and_positioned() {
    let refused = format!("api.get(\"http://127.0.0.1:{}/\");", closed_port());
    expect_errors(&[
        (refused.as_str(), "cannot connect"),
        (r#"api.get("not a url");"#, "invalid URL"),
        (r#"api.get("https://example.com/");"#, "https:// needs a TLS stack"),
        (r#"api.get();"#, "api.get expects a URL"),
        (r#"api.get("http://a", "b");"#, "api.get expects at most 1 positional argument(s), got 2"),
        (r#"api.get("http://a", body="x");"#, "api.get does not accept a body"),
        (r#"api.get("http://a", timeout=-1);"#, "api.get expects a non-negative timeout, got -1"),
        (r#"api.get("http://a", headers=["x"]);"#, "api.get expects headers to be a dictionary"),
        (r#"api.get("http://a", headers={1: "x"});"#, "api.get expects header names to be strings"),
        (r#"api.get("http://a", params=1);"#, "api.get expects params to be a dictionary or a string"),
        (r#"api.get("http://a", follow_redirects=1);"#, "api.get expects a boolean follow_redirects argument"),
        (r#"api.get("http://a", bogus=1);"#, "api.get does not accept keyword argument"),
        (r#"api.request();"#, "api.request expects a method and a URL"),
        (r#"api.request(1, "http://a");"#, "api.request expects a string method argument"),
        (r#"api.post("http://a", body="x", json={});"#, "api.post accepts either body or json, not both"),
    ]);
}

#[test]
fn api_errors_are_catchable_in_a_try_block() {
    let source = r#"var message = "";
try {
    api.get("not a url");
} catch (error) {
    message = error["message"];
}
print(message);"#;
    let text = output(source);
    assert!(text.contains("invalid URL"), "unexpected message: {:?}", text);
}
