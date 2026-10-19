//! A minimal HTTP client for the local server, and an event stream reader.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

/// The server's port and token, from a printed `view    <url>` line.
pub fn parse_view_line(line: &str) -> (u16, String) {
    let url = line
        .strip_prefix("view    http://127.0.0.1:")
        .unwrap_or_else(|| panic!("not a view line: {line:?}"))
        .trim();
    let (port, rest) = url.split_once('/').unwrap();
    let token = rest.strip_prefix("?token=").unwrap();
    let token = token.split('#').next().unwrap();
    (port.parse().unwrap(), token.to_owned())
}

/// One request; the status and body.
pub fn request(port: u16, method: &str, target: &str, body: &[u8]) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let end = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a whole head");
    let status = std::str::from_utf8(&bytes[9..12]).unwrap().parse().unwrap();
    (status, bytes[end + 4..].to_vec())
}

pub fn get(port: u16, target: &str) -> (u16, Vec<u8>) {
    request(port, "GET", target, b"")
}

pub fn post(port: u16, target: &str, body: &str) -> (u16, Vec<u8>) {
    request(port, "POST", target, body.as_bytes())
}

/// `GET` returning JSON with a 200.
pub fn get_json(port: u16, target: &str) -> serde_json::Value {
    let (status, body) = get(port, target);
    assert_eq!(status, 200, "{target}: {}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}

/// Server-sent events, read on a background thread.
pub struct EventStream {
    events: Receiver<(String, serde_json::Value)>,
}

impl EventStream {
    pub fn open(port: u16, token: &str) -> Self {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            stream,
            "GET /api/events?token={token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
        )
        .unwrap();
        let mut reader = BufReader::new(stream);
        let mut status = String::new();
        reader.read_line(&mut status).unwrap();
        assert!(status.starts_with("HTTP/1.1 200"), "{status}");
        let (sender, events) = mpsc::channel();
        thread::spawn(move || {
            let (mut name, mut data) = (None, String::new());
            for line in reader.lines() {
                let Ok(line) = line else {
                    return;
                };
                if let Some(n) = line.strip_prefix("event: ") {
                    name = Some(n.to_owned());
                } else if let Some(d) = line.strip_prefix("data: ") {
                    data.push_str(d);
                } else if line.is_empty()
                    && let Some(name) = name.take()
                {
                    let value = serde_json::from_str(&std::mem::take(&mut data)).unwrap();
                    if sender.send((name, value)).is_err() {
                        return;
                    }
                }
            }
        });
        Self { events }
    }

    /// The next event, or `None` if none comes within `timeout`.
    pub fn next_within(&self, timeout: Duration) -> Option<(String, serde_json::Value)> {
        self.events.recv_timeout(timeout).ok()
    }

    /// The next event, which must come within 10 s.
    pub fn next(&self) -> (String, serde_json::Value) {
        self.next_within(Duration::from_secs(10))
            .expect("an event within 10 s")
    }
}
