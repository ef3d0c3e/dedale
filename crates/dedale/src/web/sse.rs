//! Server-sent events: the live stream of a session.

use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::Duration;

use serde::Serialize;

use super::http::{Response, write_head};

/// Events a subscriber may fall behind by before it is dropped; its page
/// reconnects and catches up from a fresh snapshot.
pub const BACKLOG: usize = 4096;
/// Silence after which a comment is sent, which also finds closed clients.
pub const KEEP_ALIVE: Duration = Duration::from_secs(15);
/// How soon a browser reconnects after the stream ends, in milliseconds.
const RETRY_MS: u32 = 1000;

/// One event: a name and a line of data (JSON).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: String,
    pub data: String,
}

impl Event {
    /// `data` encoded as JSON.
    ///
    /// # Panics
    ///
    /// When `data` cannot be encoded (a map with non-string keys).
    #[must_use]
    pub fn json(name: &str, data: &impl Serialize) -> Self {
        Self {
            name: name.to_owned(),
            data: serde_json::to_string(data).expect("events encode as JSON"),
        }
    }

    /// The event in wire form.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut text = format!("event: {}\n", self.name);
        for line in self.data.split('\n') {
            text.push_str("data: ");
            text.push_str(line);
            text.push('\n');
        }
        text.push('\n');
        text
    }
}

/// Everyone listening to a stream. Publishing never blocks: a subscriber
/// that is gone, or more than [`BACKLOG`] events behind, is dropped.
#[derive(Debug, Default)]
pub struct Broadcast {
    subscribers: Vec<SyncSender<Event>>,
}

impl Broadcast {
    /// A new subscriber, which receives every event published from now on.
    pub fn subscribe(&mut self) -> Receiver<Event> {
        let (sender, receiver) = mpsc::sync_channel(BACKLOG);
        self.subscribers.push(sender);
        receiver
    }

    pub fn publish(&mut self, event: &Event) {
        self.subscribers
            .retain(|s| match s.try_send(event.clone()) {
                Ok(()) => true,
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
            });
    }

    /// The subscribers still listening, as of the last publish.
    #[must_use]
    pub fn len(&self) -> usize {
        self.subscribers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.subscribers.is_empty()
    }
}

/// Writes `response` (an event stream) until its sender is dropped or the
/// client goes away.
pub(super) fn stream(
    out: &mut impl Write,
    response: &Response,
    events: &Receiver<Event>,
) -> io::Result<()> {
    write_head(out, response, None)?;
    write!(out, "retry: {RETRY_MS}\n\n")?;
    out.flush()?;
    loop {
        match events.recv_timeout(KEEP_ALIVE) {
            Ok(event) => out.write_all(event.encode().as_bytes())?,
            Err(RecvTimeoutError::Timeout) => out.write_all(b": keep-alive\n\n")?,
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        out.flush()?;
    }
}

#[cfg(test)]
mod tests {
    use super::{BACKLOG, Broadcast, Event};

    #[test]
    fn events_encode_line_by_line() {
        let event = Event::json("turn", &serde_json::json!({ "turn": 3 }));
        assert_eq!(event.encode(), "event: turn\ndata: {\"turn\":3}\n\n");
        let event = Event {
            name: "x".to_owned(),
            data: "a\nb".to_owned(),
        };
        assert_eq!(event.encode(), "event: x\ndata: a\ndata: b\n\n");
    }

    #[test]
    fn slow_and_gone_subscribers_are_dropped() {
        let mut broadcast = Broadcast::default();
        let reader = broadcast.subscribe();
        let _stalled = broadcast.subscribe();
        drop(broadcast.subscribe());
        let event = Event {
            name: "e".to_owned(),
            data: String::new(),
        };
        broadcast.publish(&event);
        assert_eq!(broadcast.len(), 2);
        for _ in 0..BACKLOG {
            reader.recv().unwrap();
            broadcast.publish(&event);
        }
        // The stalled one fell behind; the reader kept up.
        assert_eq!(broadcast.len(), 1);
        assert_eq!(reader.try_iter().count(), 1);
    }
}
