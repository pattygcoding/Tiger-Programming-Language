//! The HTTP/1.1 client behind the `api` module, used on native targets.
//!
//! A dependency-free client over `std::net::TcpStream`. It speaks plain
//! `http://` only; `https://` needs a TLS stack this build does not carry, so it
//! reports a clear error instead of a confusing failure. The whole module stays
//! out of the browser and WASI builds, whose `api_stub.rs` reports the same
//! error without linking a socket.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::apimod::{parse_url, ParsedUrl, Request, Response};

/// The default number of seconds to wait when a request names no timeout.
const DEFAULT_TIMEOUT: f64 = 30.0;

/// The largest response this client will accept, in bytes.
const MAX_BODY: usize = 64 * 1024 * 1024;

/// The largest response header block this client will accept, in bytes.
const MAX_HEADERS: usize = 1024 * 1024;

/// How many redirects this client follows before reporting the last response.
const MAX_REDIRECTS: usize = 10;

/// Sends one request, following redirects, and returns the final response.
pub fn request(request: &Request) -> Result<Response, String> {
    let mut method = request.method.clone();
    let mut body = request.body.clone();
    let mut url = request.url.clone();
    let mut redirects = 0usize;
    loop {
        let parsed = parse_url(&url)?;
        if parsed.scheme != "http" {
            return Err(format!(
                "api: {:?} is not supported by the native client; use http:// (https:// needs a TLS stack this build does not carry)",
                parsed.scheme
            ));
        }
        let mut response = send(&parsed, &method, &request.headers, &body, request.timeout)?;
        response.url = url.clone();
        if request.follow_redirects && redirects < MAX_REDIRECTS {
            if let Some(location) = redirect_location(&response) {
                let target = resolve_location(&url, &location)?;
                // A 303 always switches to GET; 301 and 302 do the same for a POST,
                // which is what browsers and `requests` do. 307 and 308 keep the method.
                if matches!(response.status, 301..=303) && method == "POST" {
                    method = "GET".to_string();
                    body = None;
                }
                redirects += 1;
                url = target;
                continue;
            }
        }
        return Ok(response);
    }
}

/// The `Location` of a redirect response, or `None` for a status that does not
/// redirect or a response without one.
fn redirect_location(response: &Response) -> Option<String> {
    if !matches!(response.status, 301 | 302 | 303 | 307 | 308) {
        return None;
    }
    header(response, "location")
}

/// Resolves a redirect target against the URL that produced it.
fn resolve_location(base: &str, location: &str) -> Result<String, String> {
    let location = location.trim();
    if location.starts_with("http://") || location.starts_with("https://") {
        return Ok(location.to_string());
    }
    let parsed = parse_url(base)?;
    if let Some(rest) = location.strip_prefix("//") {
        return Ok(format!("{}://{}", parsed.scheme, rest));
    }
    if location.starts_with('/') {
        return Ok(format!("{}://{}:{}{}", parsed.scheme, parsed.host, parsed.port, location));
    }
    let directory = match parsed.path.rfind('/') {
        Some(index) => &parsed.path[..=index],
        None => "/",
    };
    Ok(format!("{}://{}:{}{}{}", parsed.scheme, parsed.host, parsed.port, directory, location))
}

/// The first value of a response header, case-insensitively.
fn header(response: &Response, name: &str) -> Option<String> {
    response.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.clone())
}

/// Sends one request over a fresh connection and reads the response.
fn send(
    parsed: &ParsedUrl,
    method: &str,
    headers: &[(String, String)],
    body: &Option<Vec<u8>>,
    timeout: Option<f64>,
) -> Result<Response, String> {
    let address = format!("{}:{}", parsed.host, parsed.port);
    let mut stream = connect(&address, timeout)?;
    let mut lines = format!("{} {} HTTP/1.1\r\n", method, request_target(parsed));
    let mut outgoing: Vec<(String, String)> = headers.to_vec();
    if !has_header(&outgoing, "host") {
        outgoing.push(("Host".to_string(), host_header(parsed)));
    }
    if !has_header(&outgoing, "user-agent") {
        outgoing.push(("User-Agent".to_string(), "tiger".to_string()));
    }
    if !has_header(&outgoing, "accept") {
        outgoing.push(("Accept".to_string(), "*/*".to_string()));
    }
    if !has_header(&outgoing, "connection") {
        outgoing.push(("Connection".to_string(), "close".to_string()));
    }
    if let Some(body) = body {
        if !has_header(&outgoing, "content-length") {
            outgoing.push(("Content-Length".to_string(), body.len().to_string()));
        }
    }
    for (name, value) in &outgoing {
        lines.push_str(name);
        lines.push_str(": ");
        lines.push_str(value);
        lines.push_str("\r\n");
    }
    lines.push_str("\r\n");
    stream.write_all(lines.as_bytes()).map_err(|error| format!("api: write failed: {}", error))?;
    if let Some(body) = body {
        stream.write_all(body).map_err(|error| format!("api: write failed: {}", error))?;
    }
    stream.flush().map_err(|error| format!("api: write failed: {}", error))?;
    let mut reader = BufReader::new(stream);
    read_response(&mut reader, method)
}

/// Opens a connection to `address`, applying the timeout to connecting and to
/// each read and write.
fn connect(address: &str, timeout: Option<f64>) -> Result<TcpStream, String> {
    let seconds = timeout.unwrap_or(DEFAULT_TIMEOUT).max(0.001);
    let duration = Duration::from_secs_f64(seconds);
    let mut addresses = address.to_socket_addrs().map_err(|error| format!("api: cannot resolve {:?}: {}", address, error))?;
    let mut last_error = None;
    for candidate in addresses.by_ref() {
        match TcpStream::connect_timeout(&candidate, duration) {
            Ok(stream) => {
                stream.set_read_timeout(Some(duration)).map_err(|error| format!("api: cannot set a read timeout: {}", error))?;
                stream.set_write_timeout(Some(duration)).map_err(|error| format!("api: cannot set a write timeout: {}", error))?;
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }
    match last_error {
        Some(error) => Err(format!("api: cannot connect to {}: {}", address, error)),
        None => Err(format!("api: no address to connect to for {}", address)),
    }
}

/// The request target: the path plus the query string, without the fragment.
fn request_target(parsed: &ParsedUrl) -> String {
    let mut target = parsed.path.clone();
    if !parsed.query.is_empty() {
        target.push('?');
        target.push_str(&parsed.query);
    }
    target
}

/// The `Host` header: the host alone at the scheme's default port, otherwise
/// `host:port`.
fn host_header(parsed: &ParsedUrl) -> String {
    let default = match parsed.scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => 0,
    };
    if parsed.port == default {
        parsed.host.clone()
    } else {
        format!("{}:{}", parsed.host, parsed.port)
    }
}

/// Whether the outgoing header list already carries `name`, case-insensitively.
fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(key, _)| key.eq_ignore_ascii_case(name))
}

/// Reads the status line, headers, and body of a response.
fn read_response(reader: &mut BufReader<TcpStream>, method: &str) -> Result<Response, String> {
    let mut status_line = Vec::new();
    reader.read_until(b'\n', &mut status_line).map_err(|error| format!("api: read failed: {}", error))?;
    if status_line.is_empty() {
        return Err("api: the server closed the connection without a response".to_string());
    }
    let (status, reason) = parse_status_line(&String::from_utf8_lossy(&status_line))?;
    let mut headers = Vec::new();
    let mut header_bytes = 0usize;
    loop {
        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line).map_err(|error| format!("api: read failed: {}", error))?;
        if line.is_empty() {
            return Err("api: the server closed the connection before the response headers".to_string());
        }
        header_bytes += line.len();
        if header_bytes > MAX_HEADERS {
            return Err("api: the response headers are too large".to_string());
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            break;
        }
        if let Some((name, value)) = text.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    let mut body = Vec::new();
    let no_body = method == "HEAD" || matches!(status, 204 | 304) || (100..200).contains(&status);
    if !no_body {
        if let Some(encoding) = find_header(&headers, "transfer-encoding") {
            if encoding.to_ascii_lowercase().contains("chunked") {
                read_chunked(reader, &mut body)?;
            } else {
                read_to_end(reader, &mut body)?;
            }
        } else if let Some(length) = find_header(&headers, "content-length") {
            let length: usize = length.parse().map_err(|_| format!("api: invalid Content-Length {:?}", length))?;
            if length > MAX_BODY {
                return Err("api: the response body is too large".to_string());
            }
            body.resize(length, 0);
            reader.read_exact(&mut body).map_err(|error| format!("api: read failed: {}", error))?;
        } else {
            read_to_end(reader, &mut body)?;
        }
    }
    Ok(Response { status, reason, headers, body, url: String::new() })
}

/// Reads the rest of the stream, refusing a body larger than `MAX_BODY`.
fn read_to_end(reader: &mut BufReader<TcpStream>, body: &mut Vec<u8>) -> Result<(), String> {
    let mut limited = (&mut *reader).take(MAX_BODY as u64 + 1);
    limited.read_to_end(body).map_err(|error| format!("api: read failed: {}", error))?;
    if body.len() > MAX_BODY {
        return Err("api: the response body is too large".to_string());
    }
    Ok(())
}

/// Reads a `Transfer-Encoding: chunked` body, stopping at the zero-length chunk.
fn read_chunked(reader: &mut BufReader<TcpStream>, body: &mut Vec<u8>) -> Result<(), String> {
    loop {
        let mut size_line = Vec::new();
        reader.read_until(b'\n', &mut size_line).map_err(|error| format!("api: read failed: {}", error))?;
        if size_line.is_empty() {
            return Err("api: the server closed the connection in a chunked body".to_string());
        }
        let text = String::from_utf8_lossy(&size_line);
        let text = text.trim_end_matches(['\r', '\n']);
        let text = text.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(text, 16).map_err(|_| format!("api: invalid chunk size {:?}", text))?;
        if size == 0 {
            // Skip the trailers up to the blank line.
            loop {
                let mut trailer = Vec::new();
                reader.read_until(b'\n', &mut trailer).map_err(|error| format!("api: read failed: {}", error))?;
                if trailer.is_empty() || String::from_utf8_lossy(&trailer).trim_end_matches(['\r', '\n']).is_empty() {
                    break;
                }
            }
            return Ok(());
        }
        if body.len() + size > MAX_BODY {
            return Err("api: the response body is too large".to_string());
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..]).map_err(|error| format!("api: read failed: {}", error))?;
        // Each chunk's data is followed by CRLF, which is consumed here.
        let mut crlf = Vec::new();
        reader.read_until(b'\n', &mut crlf).map_err(|error| format!("api: read failed: {}", error))?;
    }
}

/// Splits `HTTP/1.1 200 OK` into the status code and the reason phrase.
fn parse_status_line(text: &str) -> Result<(u16, String), String> {
    let text = text.trim_end_matches(['\r', '\n']);
    let mut parts = text.splitn(3, ' ');
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        return Err(format!("api: invalid status line {:?}", text));
    }
    let status = parts.next().ok_or_else(|| format!("api: invalid status line {:?}", text))?;
    let status: u16 = status.parse().map_err(|_| format!("api: invalid status line {:?}", text))?;
    let reason = parts.next().unwrap_or("").to_string();
    Ok((status, reason))
}

/// The first value of a header in a plain list, case-insensitively.
fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a String> {
    headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value)
}
