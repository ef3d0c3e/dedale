//! Running a bot without ever blocking on it.
//!
//! Every pipe has its own thread: stdin is fed from a queue, stdout is split
//! into capped lines sent over a bounded channel, stderr goes to a log file
//! and a tail buffer. The referee only ever waits on channels, with deadlines.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use dedale_core::protocol::{MAX_LINE, ToBot, encode_line};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use thiserror::Error;

/// Lines buffered between the stdout thread and the referee; beyond this
/// the thread stops reading and the bot blocks on its writes.
const LINE_QUEUE: usize = 256;
/// Bytes of stderr kept in the log file.
pub const STDERR_LOG_CAP: usize = 16 << 20;
/// Bytes of stderr kept in memory for crash reports.
pub const STDERR_TAIL: usize = 4096;

/// Something the bot wrote on stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// A complete line, newline excluded, received at `at`.
    Line { at: Instant, line: Vec<u8> },
    /// A line longer than [`MAX_LINE`], discarded.
    Oversized { at: Instant },
    /// Stdout closed: the bot exited or crashed.
    Closed { at: Instant },
}

#[derive(Debug, Error)]
#[error("the bot's stdin is closed")]
pub struct BotGone;

/// How a bot process ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitReport {
    pub status: Option<ExitStatus>,
    /// True when Dedale had to kill it.
    pub killed: bool,
    /// The last [`STDERR_TAIL`] bytes of stderr, lossily decoded.
    pub stderr_tail: String,
}

/// A running bot. Dropping it kills the bot's whole process group.
#[derive(Debug)]
pub struct BotProcess {
    child: Child,
    stdin: Option<Sender<Vec<u8>>>,
    stdout: Receiver<Output>,
    stderr_tail: Arc<Mutex<Vec<u8>>>,
    stderr_done: Receiver<()>,
    reaped: Option<ExitStatus>,
    group_killed: bool,
}

impl BotProcess {
    /// Starts `command` in its own process group with piped stdio, writing
    /// its stderr to `stderr_log` when given.
    ///
    /// # Errors
    ///
    /// When the log file cannot be created or the program cannot start.
    ///
    /// # Panics
    ///
    /// Never: the three pipes are requested before spawning.
    pub fn spawn(mut command: Command, stderr_log: Option<&Path>) -> io::Result<Self> {
        let log = stderr_log.map(File::create).transpose()?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        let (stdin_tx, stdin_rx) = mpsc::channel();
        let (stdout_tx, stdout_rx) = mpsc::sync_channel(LINE_QUEUE);
        let (done_tx, done_rx) = mpsc::channel();
        let tail = Arc::new(Mutex::new(Vec::new()));
        // The threads end on their own when the pipes close; nothing joins
        // them, so a stuck pipe can never hang the referee.
        thread::spawn(move || feed_stdin(stdin, &stdin_rx));
        thread::spawn(move || read_stdout(stdout, &stdout_tx));
        let tail_writer = Arc::clone(&tail);
        thread::spawn(move || {
            drain_stderr(stderr, log, &tail_writer);
            let _ = done_tx.send(());
        });

        Ok(Self {
            child,
            stdin: Some(stdin_tx),
            stdout: stdout_rx,
            stderr_tail: tail,
            stderr_done: done_rx,
            reaped: None,
            group_killed: false,
        })
    }

    #[must_use]
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Queues a message for the bot's stdin; never blocks.
    ///
    /// # Errors
    ///
    /// When stdin was closed by [`BotProcess::close_stdin`] or the bot stopped
    /// reading it for good.
    pub fn send(&self, msg: &ToBot) -> Result<(), BotGone> {
        let stdin = self.stdin.as_ref().ok_or(BotGone)?;
        stdin
            .send(encode_line(msg).into_bytes())
            .map_err(|_| BotGone)
    }

    /// Closes the bot's stdin once queued messages are written.
    pub fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// The next output, waiting until `deadline` at most; `None` when the
    /// deadline passed first.
    #[must_use]
    pub fn recv_until(&self, deadline: Instant) -> Option<Output> {
        let timeout = deadline.saturating_duration_since(Instant::now());
        match self.stdout.recv_timeout(timeout) {
            Ok(output) => Some(output),
            Err(RecvTimeoutError::Timeout) => None,
            // The thread sends `Closed` before hanging up, so this only
            // repeats what was already reported.
            Err(RecvTimeoutError::Disconnected) => Some(Output::Closed { at: Instant::now() }),
        }
    }

    /// The next output if one is waiting.
    #[must_use]
    pub fn try_recv(&self) -> Option<Output> {
        self.recv_until(Instant::now())
    }

    /// The exit status, if the bot has exited.
    pub fn try_exit_status(&mut self) -> Option<ExitStatus> {
        if self.reaped.is_none() {
            self.reaped = self.child.try_wait().ok().flatten();
        }
        self.reaped
    }

    #[must_use]
    pub fn stderr_tail(&self) -> String {
        let tail = self
            .stderr_tail
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&tail).into_owned()
    }

    /// Closes stdin, gives the bot `grace` to exit, then kills its process
    /// group (which also removes anything it left running).
    #[must_use]
    pub fn finish(mut self, grace: Duration) -> ExitReport {
        self.close_stdin();
        let deadline = Instant::now() + grace;
        while self.try_exit_status().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let killed = self.reaped.is_none();
        self.kill();
        // Let the stderr thread catch up with what was written before the
        // end, without trusting it to ever finish.
        let _ = self.stderr_done.recv_timeout(Duration::from_millis(200));
        ExitReport {
            status: self.reaped,
            killed,
            stderr_tail: self.stderr_tail(),
        }
    }

    fn kill(&mut self) {
        // The bot leads its own process group (`process_group(0)`), so its
        // id is the group id. The kernel keeps a pid out of reuse while a
        // group of that id exists; killing once, right away, leaves no
        // window for an unrelated group to take the id.
        if !self.group_killed {
            self.group_killed = true;
            if let Ok(pgid) = i32::try_from(self.child.id()) {
                let _ = killpg(Pid::from_raw(pgid), Signal::SIGKILL);
            }
        }
        if self.reaped.is_none() {
            let _ = self.child.kill();
            self.reaped = self.child.wait().ok();
        }
    }
}

impl Drop for BotProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

fn feed_stdin(mut stdin: ChildStdin, queue: &Receiver<Vec<u8>>) {
    for bytes in queue {
        if stdin
            .write_all(&bytes)
            .and_then(|()| stdin.flush())
            .is_err()
        {
            return;
        }
    }
}

/// Splits stdout into lines of at most [`MAX_LINE`] bytes. Longer lines are
/// dropped as they arrive, so memory stays bounded whatever the bot writes.
fn read_stdout(stdout: ChildStdout, out: &SyncSender<Output>) {
    let mut reader = BufReader::with_capacity(MAX_LINE, stdout);
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let buf = match reader.fill_buf() {
            Ok([]) | Err(_) => break,
            Ok(buf) => buf,
        };
        let newline = buf.iter().position(|&b| b == b'\n');
        let chunk = &buf[..newline.unwrap_or(buf.len())];
        if !oversized {
            if line.len() + chunk.len() > MAX_LINE {
                oversized = true;
                line = Vec::new();
            } else {
                line.extend_from_slice(chunk);
            }
        }
        let consumed = chunk.len() + usize::from(newline.is_some());
        reader.consume(consumed);
        if newline.is_some() {
            let at = Instant::now();
            let output = if oversized {
                Output::Oversized { at }
            } else {
                Output::Line {
                    at,
                    line: std::mem::take(&mut line),
                }
            };
            oversized = false;
            if out.send(output).is_err() {
                return;
            }
        }
    }
    let at = Instant::now();
    if oversized {
        let _ = out.send(Output::Oversized { at });
    } else if !line.is_empty() {
        let _ = out.send(Output::Line { at, line });
    }
    let _ = out.send(Output::Closed { at });
}

/// Copies stderr to the log (up to [`STDERR_LOG_CAP`]) and keeps the last
/// [`STDERR_TAIL`] bytes. Keeps reading past the cap so the bot never blocks.
fn drain_stderr(mut stderr: ChildStderr, mut log: Option<File>, tail: &Mutex<Vec<u8>>) {
    let mut buf = [0; 8192];
    let mut logged = 0;
    loop {
        let n = match stderr.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if let Some(file) = &mut log {
            let take = n.min(STDERR_LOG_CAP - logged);
            let mut write = || -> io::Result<()> {
                file.write_all(&buf[..take])?;
                logged += take;
                if logged == STDERR_LOG_CAP {
                    file.write_all(b"\n[dedale: stderr log truncated]\n")?;
                }
                Ok(())
            };
            if take == 0 || write().is_err() {
                log = None;
            }
        }
        let mut tail = tail.lock().unwrap_or_else(PoisonError::into_inner);
        tail.extend_from_slice(&buf[..n]);
        let excess = tail.len().saturating_sub(STDERR_TAIL);
        tail.drain(..excess);
    }
}
