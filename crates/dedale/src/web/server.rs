//! The local server: 127.0.0.1 only, a thread per
//! connection, one request per connection.
//!
//! Every request must name the server in `Host` (`127.0.0.1:<port>` or
//! `localhost:<port>`), so a page on another site cannot reach it through
//! DNS rebinding. `/api/` routes also need the session token, which only
//! the printed URL carries. The app files are served without it: they are
//! the same for everyone, and the page reads the token from its own URL.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::assets;
use super::http::{Body, Method, Request, Response, read_request, write_head};
use super::sse;

/// Connections served at once; more are turned away with a 503.
pub const MAX_CONNECTIONS: usize = 64;
/// Time a client gets to send its request, and to take each write.
const IO_TIMEOUT: Duration = Duration::from_secs(10);
/// Request bytes read and dropped after answering, so closing the socket
/// does not reset the connection before the client reads the answer.
const DRAIN_LIMIT: u64 = 64 * 1024;
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// What a mode (browse, live run, live tournament) serves under `/api/`.
/// The server has checked `Host` and the token before calling it.
pub trait Handler: Send + Sync {
    /// `GET /api/session`: what this server is.
    fn session(&self) -> serde_json::Value;

    /// Any other `/api/` route.
    fn api(&self, _request: &Request) -> Response {
        Response::not_found()
    }
}

/// A bound listener, not yet serving.
#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    port: u16,
    token: String,
}

impl Server {
    /// Listens on `127.0.0.1:port`, or any free port.
    ///
    /// # Errors
    ///
    /// When the port cannot be bound or no random token can be drawn.
    pub fn bind(port: Option<u16>) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port.unwrap_or(0)))?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            port,
            token: token()?,
        })
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The app's address, token included.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/?token={}", self.port, self.token)
    }

    /// Serves on a background thread until the handle is stopped or dropped.
    ///
    /// # Errors
    ///
    /// When the thread cannot be started.
    pub fn spawn(self, handler: Arc<dyn Handler>) -> io::Result<ServerHandle> {
        let addr = self.listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            handler,
            port: self.port,
            token: self.token,
            active: AtomicUsize::new(0),
        });
        let listener = self.listener;
        let thread = {
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("dedale-http".to_owned())
                .spawn(move || accept(&listener, &shared, &stop))?
        };
        Ok(ServerHandle {
            addr,
            stop,
            thread: Some(thread),
        })
    }
}

/// A running server; stops accepting connections when dropped.
#[derive(Debug)]
pub struct ServerHandle {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ServerHandle {
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops accepting connections (those open finish on their own).
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wakes the accept loop up; without that connection it would wait
        // for the next client, so it is not joined then.
        let woken = TcpStream::connect(self.addr).is_ok();
        if let Some(thread) = self.thread.take()
            && woken
        {
            let _ = thread.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Shared {
    handler: Arc<dyn Handler>,
    port: u16,
    token: String,
    active: AtomicUsize,
}

fn accept(listener: &TcpListener, shared: &Arc<Shared>, stop: &AtomicBool) {
    for stream in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let Ok(mut stream) = stream else {
            // Out of descriptors, most likely: let connections finish.
            thread::sleep(Duration::from_millis(10));
            continue;
        };
        if shared.active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            shared.active.fetch_sub(1, Ordering::SeqCst);
            turn_away(&mut stream);
            continue;
        }
        let connection = Arc::clone(shared);
        let spawned = thread::Builder::new()
            .name("dedale-http-conn".to_owned())
            .spawn(move || {
                serve(stream, &connection);
                connection.active.fetch_sub(1, Ordering::SeqCst);
            });
        if spawned.is_err() {
            shared.active.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

fn serve(mut stream: TcpStream, shared: &Shared) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let response = match read_request(&mut stream) {
        Ok(request) => shared.route(&request),
        Err(e) => match e.status() {
            Some(status) => Response::error(status, &e.to_string()),
            None => return,
        },
    };
    let _ = respond(&mut stream, &response);
    // Drop what the client may still be sending (an oversized body), so the
    // close does not reset the connection before it reads the answer.
    let _ = stream.shutdown(Shutdown::Write);
    let _ = stream.set_read_timeout(Some(DRAIN_TIMEOUT));
    let _ = io::copy(&mut (&stream).take(DRAIN_LIMIT), &mut io::sink());
}

/// Answers 503 without blocking the accept loop: whatever the client sent
/// so far is dropped (so the close is less likely to reset the
/// connection), and the short answer fits in the empty send buffer.
fn turn_away(stream: &mut TcpStream) {
    let drain = |stream: &mut TcpStream| {
        let _ = io::copy(&mut stream.take(DRAIN_LIMIT), &mut io::sink());
    };
    if stream.set_nonblocking(true).is_err() {
        return;
    }
    drain(stream);
    let _ = respond(stream, &Response::error(503, "too many connections"));
    let _ = stream.shutdown(Shutdown::Write);
    drain(stream);
}

fn respond(stream: &mut TcpStream, response: &Response) -> io::Result<()> {
    match &response.body {
        Body::Bytes(bytes) => {
            write_head(stream, response, Some(bytes.len()))?;
            stream.write_all(bytes)?;
            stream.flush()
        }
        Body::Events(events) => sse::stream(stream, response, events),
    }
}

impl Shared {
    fn route(&self, request: &Request) -> Response {
        let hosts: Vec<&str> = request
            .headers
            .iter()
            .filter(|(n, _)| n == "host")
            .map(|(_, v)| v.as_str())
            .collect();
        match hosts[..] {
            [host] if self.is_own_host(host) => {}
            [_] => return Response::error(403, "wrong Host"),
            _ => return Response::error(400, "exactly one Host header is needed"),
        }

        if request.path.starts_with("/api/") {
            if !request.query("token").is_some_and(|t| same(t, &self.token)) {
                return Response::error(403, "missing or wrong token");
            }
            return match (request.path.as_str(), &request.method) {
                ("/api/session", Method::Get) => Response::json(200, &self.handler.session()),
                ("/api/session", _) => Response::method_not_allowed("GET"),
                _ => self.handler.api(request),
            };
        }

        let file = match request.path.as_str() {
            "/" => "index.html".to_owned(),
            path => match path.strip_prefix("/assets/") {
                Some(name) => format!("assets/{name}"),
                None => return Response::not_found(),
            },
        };
        let Some(asset) = assets::asset(&file) else {
            return Response::not_found();
        };
        if request.method != Method::Get {
            return Response::method_not_allowed("GET");
        }
        Response::bytes(200, asset.content_type, asset.bytes)
    }

    fn is_own_host(&self, host: &str) -> bool {
        let Some((name, port)) = host.rsplit_once(':') else {
            return false;
        };
        (name == "127.0.0.1" || name.eq_ignore_ascii_case("localhost"))
            && port == self.port.to_string()
    }
}

/// Compares in constant time, so the token cannot be guessed byte by byte.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// 128 random bits from the OS, in hex.
fn token() -> io::Result<String> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    }))
}

/// Opens `url` in the default browser (`xdg-open`, or `open` on macOS),
/// without waiting for it.
///
/// # Errors
///
/// When the opener cannot be started.
pub fn open_browser(url: &str) -> io::Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut child = Command::new(opener)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    // Reaped in the background: the opener may outlive this call.
    thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{same, token};

    #[test]
    fn tokens_are_random_hex() {
        let (a, b) = (token().unwrap(), token().unwrap());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn comparison() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "ab"));
        assert!(!same("", "a"));
    }
}
