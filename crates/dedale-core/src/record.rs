//! The record of one game, which is also the replay format.
//!
//! The runner fills it while the game runs; everything a viewer, a debugger
//! or a competitor's analysis needs is in here.

use serde::{Deserialize, Serialize};

use crate::code::MazeCode;
use crate::game::{GameResult, Outcome, Seating};
use crate::grid::{Move, Pos};
use crate::maze::Maze;
use crate::protocol::{BotError, Mark};

/// Version of the replay format.
pub const REPLAY_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameRecord {
    pub format: u32,
    /// Version of Dedale that ran the game.
    pub dedale: String,
    /// `None` when the maze came from a file.
    pub code: Option<MazeCode>,
    /// The maze at turn 0, points included.
    pub maze: Maze,
    pub config: ConfigRecord,
    pub players: Vec<PlayerRecord>,
    pub turns: Vec<TurnRecord>,
    pub events: Vec<EventRecord>,
    pub result: GameResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigRecord {
    pub startup_ms: u64,
    pub move_ms: u64,
    pub max_turns: u32,
    pub seating: Seating,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerRecord {
    pub name: String,
    pub path: String,
    pub start: Pos,
    pub startup: StartupRecord,
    pub exit: Option<ExitRecord>,
}

/// How a bot's startup went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupOutcome {
    /// Ready within the startup budget.
    Ok,
    /// Ready after the budget: the turns before were `not_ready`.
    Late,
    /// `init()` raised; the bot kept playing.
    InitError,
    /// Never became ready before the game ended.
    Timeout,
    /// Exited, broke the handshake, or spoke another protocol version.
    Crashed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupRecord {
    pub outcome: StartupOutcome,
    /// From sending `init` to receiving `ready`, measured by Dedale.
    pub startup_us: Option<u64>,
    /// Import + `init()`, measured by the harness.
    pub init_us: Option<u64>,
    pub log: Vec<String>,
    pub marks: Vec<Mark>,
    pub error: Option<BotError>,
}

impl StartupRecord {
    /// A startup that has not finished yet; it becomes a timeout if it never
    /// does.
    #[must_use]
    pub fn pending() -> Self {
        Self {
            outcome: StartupOutcome::Timeout,
            startup_us: None,
            init_us: None,
            log: Vec::new(),
            marks: Vec::new(),
            error: None,
        }
    }
}

/// How the bot's process ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExitRecord {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    /// True when Dedale had to kill it.
    pub killed: bool,
    pub stderr_tail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRecord {
    pub turn: u32,
    pub players: Vec<PlayerTurnRecord>,
    /// Cells whose point was taken this turn, row-major.
    pub collected: Vec<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerTurnRecord {
    pub requested: Option<Move>,
    pub applied: Move,
    pub outcome: Outcome,
    /// From sending `turn` to receiving the reply, measured by Dedale.
    pub latency_us: Option<u64>,
    /// `move()`, measured by the harness.
    pub think_us: Option<u64>,
    /// Position after the turn.
    pub pos: Pos,
    pub gained: u32,
    pub half_points: u32,
    pub log: Vec<String>,
    pub marks: Vec<Mark>,
    /// The exception, for an `error` outcome.
    pub error: Option<BotError>,
    /// Why the reply was rejected, for an `invalid` outcome.
    pub invalid: Option<String>,
}

/// Something a bot did outside the expected exchange.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    /// The turn being played (0 during startup).
    pub turn: u32,
    pub player: usize,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    /// A reply for a turn that was already resolved.
    Late { for_turn: u32 },
    /// A message or line that fit nowhere, with the reason.
    Ignored { reason: String },
}

#[cfg(test)]
mod tests {
    use super::{EventKind, EventRecord};

    #[test]
    fn events_are_flat() {
        let late = EventRecord {
            turn: 7,
            player: 1,
            kind: EventKind::Late { for_turn: 6 },
        };
        let json = serde_json::to_string(&late).unwrap();
        assert_eq!(json, r#"{"turn":7,"player":1,"kind":"late","for_turn":6}"#);
        assert_eq!(serde_json::from_str::<EventRecord>(&json).unwrap(), late);
    }
}
