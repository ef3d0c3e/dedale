//! A process-wide stop request, set from a signal handler (Ctrl-C).
//!
//! The referee checks it at least every [`POLL`]; a game then ends with
//! [`crate::referee::MatchError::Interrupted`] and every bot is killed as
//! its `BotProcess` drops.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Longest wait between two checks.
pub const POLL: Duration = Duration::from_millis(100);

pub fn request() {
    REQUESTED.store(true, Ordering::SeqCst);
}

#[must_use]
pub fn requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}
