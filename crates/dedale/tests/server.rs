//! The local web server over real sockets: routes, the token and Host
//! checks, bad requests, server-sent events.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use dedale::web::assets;
use dedale::web::http::{MAX_BODY, MAX_HEAD, Method, Request, Response};
use dedale::web::server::{Handler, MAX_CONNECTIONS, Server, ServerHandle};
use dedale::web::sse::{Broadcast, Event};
use serde_json::json;

#[derive(Default)]
struct TestApp {
    events: Mutex<Broadcast>,
}

impl Handler for TestApp {
    fn session(&self) -> serde_json::Value {
        json!({ "mode": "test" })
    }

    fn api(&self, request: &Request) -> Response {
        match (request.path.as_str(), &request.method) {
            ("/api/echo", Method::Post) => {
                Response::bytes(200, "application/octet-stream", request.body.clone())
            }
            ("/api/events", Method::Get) => {
                Response::events(self.events.lock().unwrap().subscribe())
            }
            _ => Response::not_found(),
        }
    }
}

struct Running {
    app: Arc<TestApp>,
    handle: ServerHandle,
    port: u16,
    token: String,
}

impl Running {
    fn addr(&self) -> SocketAddr {
        self.handle.addr()
    }

    fn host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn get(&self, target: &str) -> Reply {
        send(
            self.addr(),
            format!("GET {target} HTTP/1.1\r\nHost: {}\r\n\r\n", self.host()).as_bytes(),
        )
    }

    fn api(&self, path: &str) -> String {
        format!("{path}?token={}", self.token)
    }

    /// Waits until `n` clients listen to the event stream.
    fn wait_for_subscribers(&self, n: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.app.events.lock().unwrap().len() != n {
            assert!(Instant::now() < deadline, "never {n} subscribers");
            // Publishing is what notices departures.
            self.app.events.lock().unwrap().publish(&Event {
                name: "ping".to_owned(),
                data: String::new(),
            });
            sleep(Duration::from_millis(10));
        }
    }
}

fn start() -> Running {
    let server = Server::bind(None).unwrap();
    let (port, token) = (server.port(), server.token().to_owned());
    assert_eq!(
        server.url(),
        format!("http://127.0.0.1:{port}/?token={token}")
    );
    let app = Arc::new(TestApp::default());
    let handle = server.spawn(app.clone()).unwrap();
    Running {
        app,
        handle,
        port,
        token,
    }
}

#[derive(Debug)]
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn json(&self) -> serde_json::Value {
        assert_eq!(self.header("content-type"), Some("application/json"));
        serde_json::from_slice(&self.body).unwrap()
    }
}

fn connect(addr: SocketAddr) -> TcpStream {
    let stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
}

fn send(addr: SocketAddr, raw: &[u8]) -> Reply {
    let mut stream = connect(addr);
    stream.write_all(raw).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    parse_reply(&bytes)
}

fn parse_reply(bytes: &[u8]) -> Reply {
    let end = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a whole head");
    let head = std::str::from_utf8(&bytes[..end]).unwrap();
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap();
    assert!(status.starts_with("HTTP/1.1 "), "{status}");
    let status = status[9..12].parse().unwrap();
    let headers: Vec<(String, String)> = lines
        .map(|l| {
            let (n, v) = l.split_once(": ").unwrap();
            (n.to_owned(), v.to_owned())
        })
        .collect();
    let body = bytes[end + 4..].to_vec();
    let reply = Reply {
        status,
        headers,
        body,
    };
    let length: usize = reply.header("content-length").unwrap().parse().unwrap();
    assert_eq!(reply.body.len(), length);
    reply
}

#[test]
fn serves_the_app_without_a_token() {
    let server = start();
    let page = server.get("/");
    assert_eq!(page.status, 200);
    assert_eq!(
        page.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(page.body, assets::asset("index.html").unwrap().bytes);
    assert_eq!(page.header("connection"), Some("close"));
    assert_eq!(page.header("cache-control"), Some("no-store"));
    assert_eq!(page.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(page.header("referrer-policy"), Some("no-referrer"));
    assert!(
        page.header("content-security-policy")
            .unwrap()
            .contains("default-src 'self'")
    );

    let script = server.get("/assets/main.js");
    assert_eq!(script.status, 200);
    assert_eq!(
        script.header("content-type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(script.body, assets::asset("assets/main.js").unwrap().bytes);
    let token = server.get(&format!("/?token={}", server.token));
    assert_eq!(token.body, page.body);

    for target in [
        "/index.html",
        "/assets/missing.js",
        "/assets/../index.html",
        "/assets/%2e%2e/index.html",
        "/nope",
        "/assets",
    ] {
        let reply = server.get(target);
        assert_eq!(reply.status, 404, "{target}");
        assert_eq!(reply.json()["error"], "not found");
    }
    let post = send(
        server.addr(),
        format!("POST / HTTP/1.1\r\nHost: {}\r\n\r\n", server.host()).as_bytes(),
    );
    assert_eq!((post.status, post.header("allow")), (405, Some("GET")));
}

#[test]
fn the_api_needs_the_token() {
    let server = start();
    for target in [
        "/api/session".to_owned(),
        "/api/session?token=".to_owned(),
        "/api/session?token=00000000000000000000000000000000".to_owned(),
        format!("/api/session?token={}x", server.token),
        format!("/api/session?token={}", server.token.to_uppercase()),
        "/api/nope".to_owned(),
    ] {
        let reply = server.get(&target);
        assert_eq!(reply.status, 403, "{target}");
        assert_eq!(reply.json()["error"], "missing or wrong token");
    }

    let session = server.get(&server.api("/api/session"));
    assert_eq!(session.status, 200);
    assert_eq!(session.json(), json!({ "mode": "test" }));
    assert_eq!(server.get(&server.api("/api/nope")).status, 404);
    let post = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\n\r\n",
        server.api("/api/session"),
        server.host()
    );
    assert_eq!(send(server.addr(), post.as_bytes()).status, 405);

    let echo = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Length: 5\r\n\r\nhello",
        server.api("/api/echo"),
        server.host()
    );
    let reply = send(server.addr(), echo.as_bytes());
    assert_eq!((reply.status, reply.body.as_slice()), (200, &b"hello"[..]));
}

#[test]
fn the_host_must_be_this_server() {
    let server = start();
    let request = |host: &str| format!("GET / HTTP/1.1\r\n{host}\r\n");
    let other = server.port.wrapping_add(1);
    for (host, status) in [
        (format!("Host: localhost:{}\r\n", server.port), 200),
        (format!("Host: LocalHost:{}\r\n", server.port), 200),
        ("Host: evil.example\r\n".to_owned(), 403),
        (format!("Host: evil.example:{}\r\n", server.port), 403),
        (format!("Host: 127.0.0.1:{other}\r\n"), 403),
        (format!("Host: 127.0.0.1:{}x\r\n", server.port), 403),
        (format!("Host: 127.0.0.2:{}\r\n", server.port), 403),
        (String::new(), 400),
        (format!("Host: {0}\r\nHost: {0}\r\n", server.host()), 400),
    ] {
        let reply = send(server.addr(), request(&host).as_bytes());
        assert_eq!(reply.status, status, "{host:?}");
    }
}

#[test]
fn bad_requests_are_refused() {
    let server = start();
    let host = server.host();
    let long = "x".repeat(MAX_HEAD);
    for (raw, status) in [
        ("GARBAGE\r\n\r\n".to_owned(), 400),
        (format!("GET /%zz HTTP/1.1\r\nHost: {host}\r\n\r\n"), 400),
        (format!("GET / HTTP/1.0\r\nHost: {host}\r\n\r\n"), 505),
        (
            format!("GET / HTTP/1.1\r\nHost: {host}\r\nX: {long}\r\n\r\n"),
            431,
        ),
        (
            format!(
                "POST /api/echo HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\n\r\n{}",
                MAX_BODY + 1,
                "y".repeat(MAX_BODY + 1)
            ),
            413,
        ),
        (
            format!(
                "POST / HTTP/1.1\r\nHost: {host}\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
            ),
            501,
        ),
    ] {
        let reply = send(server.addr(), raw.as_bytes());
        assert_eq!(reply.status, status, "{:?}", &raw[..raw.len().min(60)]);
        assert!(reply.json()["error"].is_string());
    }
}

#[test]
fn a_stalled_client_does_not_hold_others() {
    let server = start();
    let _stalled = connect(server.addr());
    let mut half = connect(server.addr());
    half.write_all(b"GET / HTTP/1.1\r\nHo").unwrap();
    let begun = Instant::now();
    assert_eq!(server.get("/").status, 200);
    assert!(begun.elapsed() < Duration::from_secs(1));
}

#[test]
fn connections_are_capped() {
    let server = start();
    let stalled: Vec<TcpStream> = (0..MAX_CONNECTIONS)
        .map(|_| connect(server.addr()))
        .collect();
    // Without sending anything, so the close cannot reset the connection.
    let mut turned_away = connect(server.addr());
    let mut bytes = Vec::new();
    turned_away.read_to_end(&mut bytes).unwrap();
    let busy = parse_reply(&bytes);
    assert_eq!(busy.status, 503);
    assert_eq!(busy.json()["error"], "too many connections");
    drop(stalled);
    // Each stalled connection ends once its client is gone.
    let deadline = Instant::now() + Duration::from_secs(5);
    while server.get("/").status != 200 {
        assert!(Instant::now() < deadline, "the cap never cleared");
        sleep(Duration::from_millis(20));
    }
}

/// Reads one event (skipping `retry` and comments) from a stream.
fn next_event(reader: &mut impl BufRead) -> Event {
    let (mut name, mut data) = (None, Vec::new());
    loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0, "stream ended");
        let line = line.trim_end_matches('\n');
        if line.is_empty() {
            if let Some(name) = name.take() {
                return Event {
                    name,
                    data: data.join("\n"),
                };
            }
        } else if let Some(n) = line.strip_prefix("event: ") {
            name = Some(n.to_owned());
        } else if let Some(d) = line.strip_prefix("data: ") {
            data.push(d.to_owned());
        }
    }
}

#[test]
fn events_stream_in_order_and_promptly() {
    let server = start();
    let mut stream = connect(server.addr());
    write!(
        stream,
        "GET {} HTTP/1.1\r\nHost: {}\r\n\r\n",
        server.api("/api/events"),
        server.host()
    )
    .unwrap();
    let mut reader = BufReader::new(stream);
    let mut head = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" {
            break;
        }
        head.push(line);
    }
    assert!(head[0].starts_with("HTTP/1.1 200"), "{head:?}");
    assert!(head.contains(&"Content-Type: text/event-stream\r\n".to_owned()));
    assert!(!head.iter().any(|h| h.starts_with("Content-Length")));

    server.wait_for_subscribers(1);
    let begun = Instant::now();
    for turn in 1..=100 {
        let event = Event::json("turn", &json!({ "turn": turn }));
        server.app.events.lock().unwrap().publish(&event);
    }
    let mut turns = Vec::new();
    while turns.len() < 100 {
        let event = next_event(&mut reader);
        if event.name == "turn" {
            let data: serde_json::Value = serde_json::from_str(&event.data).unwrap();
            turns.push(data["turn"].as_u64().unwrap());
        }
    }
    assert_eq!(turns, (1..=100).collect::<Vec<u64>>());
    assert!(
        begun.elapsed() < Duration::from_secs(1),
        "{:?}",
        begun.elapsed()
    );
}

#[test]
fn a_closed_event_client_is_dropped() {
    let server = start();
    let mut client = connect(server.addr());
    write!(
        client,
        "GET {} HTTP/1.1\r\nHost: {}\r\n\r\n",
        server.api("/api/events"),
        server.host()
    )
    .unwrap();
    server.wait_for_subscribers(1);
    drop(client);
    // Publishing never blocks, and finds the client gone.
    server.wait_for_subscribers(0);
    assert_eq!(server.get("/").status, 200);
}

#[test]
fn a_stopped_server_refuses_connections() {
    let server = start();
    let addr = server.addr();
    assert_eq!(server.get("/").status, 200);
    server.handle.stop();
    assert!(TcpStream::connect(addr).is_err());
}
