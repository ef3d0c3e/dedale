//! A strict HTTP/1.1 request parser and the response writer of the local
//! server. One request per connection (`Connection: close`).
//!
//! Only the subset Dedale needs is accepted; anything else is refused with
//! a status: origin-form targets, HTTP/1.1, no `Transfer-Encoding`, no
//! folded headers, ASCII heads, at most [`MAX_HEAD`] bytes of request line
//! and headers and [`MAX_BODY`] bytes of body.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::io::{self, ErrorKind, Read, Write};
use std::sync::mpsc::Receiver;

use serde::Serialize;
use thiserror::Error;

use super::sse::Event;

/// Request line and headers, blank line included.
pub const MAX_HEAD: usize = 8 * 1024;
pub const MAX_BODY: usize = 4 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    /// Any other method token, as sent.
    Other(String),
}

impl Method {
    fn parse(token: &str) -> Self {
        match token {
            "GET" => Self::Get,
            "POST" => Self::Post,
            other => Self::Other(other.to_owned()),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Other(token) => token,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    /// Percent-decoded.
    pub path: String,
    /// Percent-decoded pairs in order; `+` is a space.
    pub query: Vec<(String, String)>,
    /// Lowercase names and trimmed values, in order.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The first value of a header (`name` in lowercase).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The first value of a query parameter.
    #[must_use]
    pub fn query(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The request in wire form, which [`read_request`] parses back to an
    /// equal request.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut head = format!("{} {}", self.method.as_str(), encode_path(&self.path));
        for (i, (name, value)) in self.query.iter().enumerate() {
            head.push(if i == 0 { '?' } else { '&' });
            head.push_str(&encode_query(name));
            head.push('=');
            head.push_str(&encode_query(value));
        }
        head.push_str(" HTTP/1.1\r\n");
        for (name, value) in &self.headers {
            let _ = write!(head, "{name}: {value}\r\n");
        }
        head.push_str("\r\n");
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("connection closed before a whole request")]
    Incomplete,
    #[error("timed out waiting for the request")]
    Timeout,
    #[error("request head larger than {MAX_HEAD} bytes")]
    HeadTooLarge,
    #[error("request body larger than {MAX_BODY} bytes")]
    BodyTooLarge,
    #[error("unsupported HTTP version")]
    Version,
    #[error("transfer encodings are not supported")]
    TransferEncoding,
    #[error("bad request: {0}")]
    Bad(&'static str),
    #[error(transparent)]
    Io(io::Error),
}

impl HttpError {
    /// The status to answer with; `None` when there is no one to answer.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Incomplete | Self::Io(_) => None,
            Self::Timeout => Some(408),
            Self::HeadTooLarge => Some(431),
            Self::BodyTooLarge => Some(413),
            Self::Version => Some(505),
            Self::TransferEncoding => Some(501),
            Self::Bad(_) => Some(400),
        }
    }
}

/// Reads one request, never more than [`MAX_HEAD`] + [`MAX_BODY`] bytes
/// (plus one read's worth).
///
/// # Errors
///
/// When the request is malformed, too large, unsupported, incomplete, or
/// reading fails or times out.
pub fn read_request(reader: &mut impl Read) -> Result<Request, HttpError> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0; 1024];
    let mut searched = 0;
    let head_end = loop {
        // The terminator may straddle two reads.
        if let Some(at) = find(&buf[searched..], b"\r\n\r\n") {
            break searched + at + 4;
        }
        if buf.len() >= MAX_HEAD {
            return Err(HttpError::HeadTooLarge);
        }
        searched = buf.len().saturating_sub(3);
        let n = fill(reader, &mut chunk)?;
        buf.extend_from_slice(&chunk[..n]);
    };
    if head_end > MAX_HEAD {
        return Err(HttpError::HeadTooLarge);
    }
    let mut request = parse_head(&buf[..head_end - 4])?;

    let length = content_length(&request.headers)?;
    if length > MAX_BODY {
        return Err(HttpError::BodyTooLarge);
    }
    let mut body = buf.split_off(head_end);
    while body.len() < length {
        let n = fill(reader, &mut chunk)?;
        body.extend_from_slice(&chunk[..n]);
    }
    // Anything after the body is ignored: one request per connection.
    body.truncate(length);
    request.body = body;
    Ok(request)
}

/// One read into `chunk`; end of input is an error.
fn fill(reader: &mut impl Read, chunk: &mut [u8]) -> Result<usize, HttpError> {
    loop {
        return match reader.read(chunk) {
            Ok(0) => Err(HttpError::Incomplete),
            Ok(n) => Ok(n),
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                Err(HttpError::Timeout)
            }
            Err(e) => Err(HttpError::Io(e)),
        };
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Request line and headers, without the blank line.
fn parse_head(head: &[u8]) -> Result<Request, HttpError> {
    let text = std::str::from_utf8(head)
        .ok()
        .filter(|t| t.is_ascii())
        .ok_or(HttpError::Bad("non-ASCII request head"))?;
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(HttpError::Bad("malformed request line"));
    };
    if method.is_empty() || !method.bytes().all(is_tchar) {
        return Err(HttpError::Bad("bad method"));
    }
    if version != "HTTP/1.1" {
        return Err(if version.starts_with("HTTP/") {
            HttpError::Version
        } else {
            HttpError::Bad("malformed request line")
        });
    }
    let (path, query) = parse_target(target)?;

    let mut headers = Vec::new();
    for line in lines {
        if line.starts_with([' ', '\t']) {
            return Err(HttpError::Bad("folded header"));
        }
        let (name, value) = line
            .split_once(':')
            .ok_or(HttpError::Bad("malformed header"))?;
        if name.is_empty() || !name.bytes().all(is_tchar) {
            return Err(HttpError::Bad("bad header name"));
        }
        let value = value.trim_matches([' ', '\t']);
        if !value
            .bytes()
            .all(|b| b == b'\t' || (b' '..=b'~').contains(&b))
        {
            return Err(HttpError::Bad("bad header value"));
        }
        headers.push((name.to_ascii_lowercase(), value.to_owned()));
    }
    if headers.iter().any(|(n, _)| n == "transfer-encoding") {
        return Err(HttpError::TransferEncoding);
    }
    Ok(Request {
        method: Method::parse(method),
        path,
        query,
        headers,
        body: Vec::new(),
    })
}

/// RFC 9110 token characters.
fn is_tchar(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)
}

type Query = Vec<(String, String)>;

fn parse_target(target: &str) -> Result<(String, Query), HttpError> {
    if !target.starts_with('/') {
        return Err(HttpError::Bad("target must be a path"));
    }
    if !target.bytes().all(|b| b.is_ascii_graphic()) || target.contains('#') {
        return Err(HttpError::Bad("bad target"));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let path = decode(path, false)?;
    let query = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            Ok((decode(name, true)?, decode(value, true)?))
        })
        .collect::<Result<_, HttpError>>()?;
    Ok((path, query))
}

/// Percent-decodes `text` (and `+` as a space in a query); the result must
/// be UTF-8 without control characters.
fn decode(text: &str, plus_is_space: bool) -> Result<String, HttpError> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'%' => {
                let hex = bytes
                    .get(i + 1..i + 3)
                    .and_then(|h| std::str::from_utf8(h).ok())
                    .filter(|h| h.bytes().all(|c| c.is_ascii_hexdigit()))
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                    .ok_or(HttpError::Bad("bad percent-encoding"))?;
                out.push(hex);
                i += 3;
            }
            b'+' if plus_is_space => {
                out.push(b' ');
                i += 1;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out)
        .ok()
        .filter(|s| !s.chars().any(char::is_control))
        .ok_or(HttpError::Bad("bad characters in target"))
}

/// Characters written as they are: printable ASCII that decodes to itself.
fn encode_with(text: &str, keep: impl Fn(u8) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if keep(b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn encode_path(path: &str) -> String {
    encode_with(path, |b| b.is_ascii_graphic() && !b"%?#".contains(&b))
}

fn encode_query(text: &str) -> String {
    encode_with(text, |b| b.is_ascii_graphic() && !b"%?#&=+".contains(&b)).replace("%20", "+")
}

fn content_length(headers: &[(String, String)]) -> Result<usize, HttpError> {
    let mut values = headers.iter().filter(|(n, _)| n == "content-length");
    let Some((_, value)) = values.next() else {
        return Ok(0);
    };
    if values.next().is_some() {
        return Err(HttpError::Bad("several Content-Length headers"));
    }
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(HttpError::Bad("bad Content-Length"));
    }
    // Too many digits for usize is too large in any case.
    Ok(value.parse().unwrap_or(usize::MAX))
}

#[derive(Debug)]
pub enum Body {
    Bytes(Cow<'static, [u8]>),
    /// A `text/event-stream`, open until the sender is dropped.
    Events(Receiver<Event>),
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Body,
    /// The `Allow` header of a 405.
    pub allow: Option<&'static str>,
}

impl Response {
    #[must_use]
    pub fn bytes(
        status: u16,
        content_type: &'static str,
        body: impl Into<Cow<'static, [u8]>>,
    ) -> Self {
        Self {
            status,
            content_type,
            body: Body::Bytes(body.into()),
            allow: None,
        }
    }

    /// `value` as JSON.
    #[must_use]
    pub fn json(status: u16, value: &impl Serialize) -> Self {
        match serde_json::to_vec(value) {
            Ok(body) => Self::bytes(status, "application/json", body),
            Err(e) => Self::error(500, &format!("cannot encode the response: {e}")),
        }
    }

    /// `{"error": message}`.
    #[must_use]
    pub fn error(status: u16, message: &str) -> Self {
        let body = serde_json::json!({ "error": message }).to_string();
        Self::bytes(status, "application/json", body.into_bytes())
    }

    #[must_use]
    pub fn not_found() -> Self {
        Self::error(404, "not found")
    }

    #[must_use]
    pub fn method_not_allowed(allow: &'static str) -> Self {
        Self {
            allow: Some(allow),
            ..Self::error(405, "method not allowed")
        }
    }

    /// A stream of server-sent events.
    #[must_use]
    pub fn events(events: Receiver<Event>) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream",
            body: Body::Events(events),
            allow: None,
        }
    }
}

/// The status line and headers; `length` is `None` for a stream.
///
/// # Errors
///
/// When writing fails.
pub fn write_head(
    out: &mut impl Write,
    response: &Response,
    length: Option<usize>,
) -> io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\n",
        response.status,
        reason(response.status),
        response.content_type
    );
    if let Some(length) = length {
        let _ = write!(head, "Content-Length: {length}\r\n");
    }
    if let Some(allow) = response.allow {
        let _ = write!(head, "Allow: {allow}\r\n");
    }
    head.push_str(concat!(
        "Connection: close\r\n",
        "Cache-Control: no-store\r\n",
        "X-Content-Type-Options: nosniff\r\n",
        "Referrer-Policy: no-referrer\r\n",
        "Content-Security-Policy: default-src 'self'; img-src 'self' data:; ",
        "base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n",
        "\r\n",
    ));
    out.write_all(head.as_bytes())
}

/// The reason phrase of the statuses Dedale sends.
#[must_use]
pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Content Too Large",
        431 => "Request Header Fields Too Large",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        505 => "HTTP Version Not Supported",
        _ => "Error",
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{HttpError, MAX_BODY, MAX_HEAD, Method, Request, read_request};

    fn parse(text: &str) -> Result<Request, HttpError> {
        read_request(&mut text.as_bytes())
    }

    fn status(text: &str) -> Option<u16> {
        parse(text).unwrap_err().status()
    }

    #[test]
    fn parses_a_get() {
        let request =
            parse("GET /api/file?path=run%201%2Freplay.json&token=ab+c HTTP/1.1\r\nHost: 127.0.0.1:80\r\nX-Thing:  spaced \t\r\n\r\n")
                .unwrap();
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.path, "/api/file");
        assert_eq!(request.query("path"), Some("run 1/replay.json"));
        assert_eq!(request.query("token"), Some("ab c"));
        assert_eq!(request.header("host"), Some("127.0.0.1:80"));
        assert_eq!(request.header("x-thing"), Some("spaced"));
        assert!(request.body.is_empty());
    }

    #[test]
    fn parses_a_post_body_and_ignores_what_follows() {
        let request =
            parse("POST /api/control HTTP/1.1\r\nContent-Length: 4\r\n\r\n{\"a\"}next").unwrap();
        assert_eq!(request.method, Method::Post);
        assert_eq!(request.body, b"{\"a\"");
        assert_eq!(
            parse("PUT / HTTP/1.1\r\n\r\n").unwrap().method,
            Method::Other("PUT".to_owned())
        );
    }

    #[test]
    fn reads_across_chunks() {
        // A reader that hands out one byte at a time.
        struct Trickle<'a>(&'a [u8]);
        impl std::io::Read for Trickle<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let Some((&first, rest)) = self.0.split_first() else {
                    return Ok(0);
                };
                buf[0] = first;
                self.0 = rest;
                Ok(1)
            }
        }
        let text = b"POST /x HTTP/1.1\r\nContent-Length: 3\r\n\r\nabc";
        let request = read_request(&mut Trickle(text)).unwrap();
        assert_eq!(
            (request.path.as_str(), request.body.as_slice()),
            ("/x", &b"abc"[..])
        );
    }

    #[test]
    fn malformed_requests_are_bad() {
        for text in [
            "GET /\r\n\r\n",
            "GET  / HTTP/1.1\r\n\r\n",
            "GET / HTTP/1.1 extra\r\n\r\n",
            "G(T / HTTP/1.1\r\n\r\n",
            "GET http://example.com/ HTTP/1.1\r\n\r\n",
            "GET * HTTP/1.1\r\n\r\n",
            "GET /a#b HTTP/1.1\r\n\r\n",
            "GET /%zz HTTP/1.1\r\n\r\n",
            "GET /%2 HTTP/1.1\r\n\r\n",
            "GET /%00 HTTP/1.1\r\n\r\n",
            "GET /%ff HTTP/1.1\r\n\r\n",
            "GET /?a=%0a HTTP/1.1\r\n\r\n",
            "GET / XTTP/1.1\r\n\r\n",
            "GET / HTTP/1.1\r\nNoColon\r\n\r\n",
            "GET / HTTP/1.1\r\nBad Name: x\r\n\r\n",
            "GET / HTTP/1.1\r\n: x\r\n\r\n",
            "GET / HTTP/1.1\r\nA: b\r\n folded\r\n\r\n",
            "GET / HTTP/1.1\r\nA: b\rc\r\n\r\n",
            "GET / HTTP/1.1\r\nA: b\nc\r\n\r\n",
            "GET / HTTP/1.1\r\nA: caf\u{e9}\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: -1\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 1, 1\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\nx",
            "\r\nGET / HTTP/1.1\r\n\r\n",
        ] {
            assert_eq!(status(text), Some(400), "{text:?}");
        }
    }

    #[test]
    fn unsupported_and_oversized_requests() {
        assert_eq!(status("GET / HTTP/1.0\r\n\r\n"), Some(505));
        assert_eq!(status("GET / HTTP/2\r\n\r\n"), Some(505));
        let chunked = "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n";
        assert_eq!(status(chunked), Some(501));
        let body = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert_eq!(status(&body), Some(413));
        let huge = "POST / HTTP/1.1\r\nContent-Length: 99999999999999999999999\r\n\r\n";
        assert_eq!(status(huge), Some(413));
        let head = format!("GET / HTTP/1.1\r\nA: {}\r\n\r\n", "x".repeat(MAX_HEAD));
        assert_eq!(status(&head), Some(431));
        // Exactly at the limit is fine.
        let fill = MAX_HEAD - "GET / HTTP/1.1\r\nA: \r\n\r\n".len();
        let head = format!("GET / HTTP/1.1\r\nA: {}\r\n\r\n", "x".repeat(fill));
        assert!(parse(&head).is_ok());
        let head = format!("GET / HTTP/1.1\r\nA: {}\r\n\r\n", "x".repeat(fill + 1));
        assert_eq!(status(&head), Some(431));
    }

    #[test]
    fn incomplete_requests_have_no_answer() {
        for text in [
            "",
            "GET / HTTP/1.1\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nab",
        ] {
            assert!(
                matches!(parse(text), Err(HttpError::Incomplete)),
                "{text:?}"
            );
        }
        assert_eq!(HttpError::Incomplete.status(), None);
    }

    fn text() -> impl Strategy<Value = String> {
        prop_oneof![
            "[a-z0-9/._~-]{0,12}",
            any::<String>().prop_filter("no controls", |s| !s.chars().any(char::is_control)),
        ]
    }

    fn request() -> impl Strategy<Value = Request> {
        let method = prop_oneof![
            Just(Method::Get),
            Just(Method::Post),
            "[A-Z]{1,8}".prop_map(|m| Method::parse(&m)),
        ];
        let header =
            ("[a-z][a-z0-9-]{0,10}", "[ -~]{0,20}").prop_map(|(name, value): (String, String)| {
                (name, value.trim_matches([' ', '\t']).to_owned())
            });
        (
            method,
            text().prop_map(|p| format!("/{p}")),
            prop::collection::vec((text(), text()), 0..4),
            prop::collection::vec(header, 0..4),
            prop::collection::vec(any::<u8>(), 0..64),
        )
            .prop_map(|(method, path, query, mut headers, body)| {
                headers.retain(|(n, _)| n != "content-length" && n != "transfer-encoding");
                if !body.is_empty() {
                    headers.push(("content-length".to_owned(), body.len().to_string()));
                }
                Request {
                    method,
                    path,
                    query: query.into_iter().filter(|(n, _)| !n.is_empty()).collect(),
                    headers,
                    body,
                }
            })
    }

    proptest! {
        #[test]
        fn encoding_roundtrips(request in request()) {
            prop_assert_eq!(read_request(&mut request.encode().as_slice()).unwrap(), request);
        }

        #[test]
        fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
            if let Ok(request) = read_request(&mut bytes.as_slice()) {
                prop_assert!(request.body.len() <= MAX_BODY);
                let again = read_request(&mut request.encode().as_slice());
                // Re-encoding never grows a request past its limits here.
                prop_assert_eq!(again.unwrap(), request);
            }
        }

        #[test]
        fn mangled_requests_never_panic(request in request(), at in any::<prop::sample::Index>(), byte in any::<u8>()) {
            let mut bytes = request.encode();
            let i = at.index(bytes.len());
            bytes[i] = byte;
            let _ = read_request(&mut bytes.as_slice());
        }
    }
}
