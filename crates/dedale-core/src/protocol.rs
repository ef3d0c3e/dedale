//! Messages between Dedale and bots.
//!
//! One JSON object per line. Bot output is untrusted: [`decode_bot_line`]
//! never panics, ignores unknown fields, and caps logs and marks whatever the
//! harness did.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::game::{EndReason, PlayerState};
use crate::grid::{Move, Pos, UnknownMove};

pub const PROTOCOL_VERSION: u32 = 1;

/// Longest line a bot may send, newline excluded.
pub const MAX_LINE: usize = 64 * 1024;
/// Total bytes of log entries kept per call.
pub const MAX_LOG_BYTES: usize = 4096;
/// Marks kept per call.
pub const MAX_MARKS: usize = 64;
/// Cells kept across all marks of a call.
pub const MAX_MARK_CELLS: usize = 4096;
/// Longest mark label or colour, and longest error message or traceback, in
/// bytes.
pub const MAX_LABEL: usize = 64;
pub const MAX_ERROR_TEXT: usize = 4096;
/// Log entry appended when something was cut.
pub const TRUNCATED: &str = "[truncated]";

/// Dedale → bot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToBot {
    Init(Init),
    Turn(Turn),
    End(End),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Init {
    pub protocol: u32,
    /// `None` for a maze loaded from a file.
    pub code: Option<String>,
    pub width: u8,
    pub height: u8,
    /// Row-major, `width × height` characters: `#` wall, `.` point, `_` empty.
    pub cells: String,
    /// The receiving bot's index in `players`.
    pub you: usize,
    pub players: Vec<InitPlayer>,
    pub startup_ms: u64,
    pub move_ms: u64,
    pub max_turns: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitPlayer {
    pub pos: Pos,
}

/// The state after turn `turn - 1`; the bot answers with its move for `turn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    pub turn: u32,
    pub players: Vec<PlayerState>,
    /// Cells whose point was taken during turn `turn - 1`.
    pub collected: Vec<Pos>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct End {
    pub reason: EndReason,
    pub turn: u32,
    pub half_points: Vec<u32>,
}

/// Bot → Dedale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromBot {
    Hello(Hello),
    Ready(Ready),
    Error(ErrorReply),
    Move(MoveReply),
}

/// Sent once the harness runs, before the bot module is imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
}

/// Startup finished: the bot module is imported and `init()` returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ready {
    /// Import + `init()`, measured by the harness.
    #[serde(default)]
    pub init_us: Option<u64>,
    #[serde(default)]
    pub log: Vec<String>,
    #[serde(default)]
    pub marks: Vec<Mark>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Init,
}

/// Startup failed; the harness keeps answering turns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorReply {
    pub phase: Phase,
    #[serde(flatten)]
    pub error: BotError,
    #[serde(default)]
    pub log: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotError {
    pub message: String,
    #[serde(default)]
    pub traceback: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveReply {
    pub turn: u32,
    /// Kept as text so an unknown move still delivers the turn id, logs and
    /// marks; see [`MoveReply::requested`].
    #[serde(rename = "move", default)]
    pub mv: Option<String>,
    /// `move()`, measured by the harness.
    #[serde(default)]
    pub think_us: Option<u64>,
    #[serde(default)]
    pub log: Vec<String>,
    #[serde(default)]
    pub marks: Vec<Mark>,
    /// Set when `move()` raised.
    #[serde(default)]
    pub error: Option<BotError>,
}

impl MoveReply {
    /// The requested move; `Ok(None)` when the reply carries none.
    ///
    /// # Errors
    ///
    /// When the move is not a lowercase move name.
    pub fn requested(&self) -> Result<Option<Move>, UnknownMove> {
        self.mv.as_deref().map(str::parse).transpose()
    }
}

/// Cells a bot highlights for the replay and the debugger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub cells: Vec<Pos>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    #[error("line longer than {MAX_LINE} bytes")]
    TooLong,
    #[error("line is not UTF-8")]
    NotUtf8,
    #[error("bad message: {0}")]
    Json(String),
    #[error("bot speaks protocol {found}; Dedale speaks {PROTOCOL_VERSION}")]
    ProtocolVersion { found: u32 },
}

/// One message as a line, newline included.
///
/// # Panics
///
/// Never for the protocol types: they always serialize.
#[must_use]
pub fn encode_line<T: Serialize>(msg: &T) -> String {
    let mut line = serde_json::to_string(msg).expect("protocol messages serialize");
    line.push('\n');
    line
}

/// Decodes and sanitizes a line from a bot (trailing newline optional).
///
/// # Errors
///
/// When the line is too long, not UTF-8, not a known message, or a `hello`
/// for another protocol version.
pub fn decode_bot_line(line: &[u8]) -> Result<FromBot, DecodeError> {
    let mut msg: FromBot = decode(line)?;
    match &mut msg {
        FromBot::Hello(hello) if hello.protocol != PROTOCOL_VERSION => {
            return Err(DecodeError::ProtocolVersion {
                found: hello.protocol,
            });
        }
        FromBot::Hello(_) => {}
        FromBot::Ready(ready) => {
            cap_marks(&mut ready.marks, &mut ready.log);
            cap_log(&mut ready.log);
        }
        FromBot::Error(reply) => {
            cap_error(&mut reply.error);
            cap_log(&mut reply.log);
        }
        FromBot::Move(reply) => {
            cap_marks(&mut reply.marks, &mut reply.log);
            cap_log(&mut reply.log);
            if let Some(error) = &mut reply.error {
                cap_error(error);
            }
        }
    }
    Ok(msg)
}

/// Decodes a line from Dedale, for bots written in Rust (trailing newline
/// optional).
///
/// # Errors
///
/// When the line is too long, not UTF-8 or not a known message.
pub fn decode_dedale_line(line: &[u8]) -> Result<ToBot, DecodeError> {
    decode(line)
}

fn decode<T: DeserializeOwned>(line: &[u8]) -> Result<T, DecodeError> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    if line.len() > MAX_LINE {
        return Err(DecodeError::TooLong);
    }
    let text = std::str::from_utf8(line).map_err(|_| DecodeError::NotUtf8)?;
    serde_json::from_str(text).map_err(|e| DecodeError::Json(e.to_string()))
}

/// Keeps at most [`MAX_LOG_BYTES`] of entries, markers included: the entry
/// that overflows is cut and followed by [`TRUNCATED`]. A log within the cap
/// is left alone, so capping twice changes nothing.
fn cap_log(log: &mut Vec<String>) {
    if log.iter().map(String::len).sum::<usize>() <= MAX_LOG_BYTES {
        return;
    }
    let mut budget = MAX_LOG_BYTES - TRUNCATED.len();
    let overflow = log
        .iter()
        .position(|entry| {
            let over = entry.len() > budget;
            if !over {
                budget -= entry.len();
            }
            over
        })
        .expect("a log over the cap overflows at some entry");
    truncate_at_char(&mut log[overflow], budget);
    log.truncate(overflow + 1);
    log.push(TRUNCATED.to_owned());
}

fn cap_marks(marks: &mut Vec<Mark>, log: &mut Vec<String>) {
    let mut cut = marks.len() > MAX_MARKS;
    marks.truncate(MAX_MARKS);
    let mut budget = MAX_MARK_CELLS;
    for mark in marks.iter_mut() {
        if mark.cells.len() > budget {
            mark.cells.truncate(budget);
            cut = true;
        }
        budget -= mark.cells.len();
        for text in [&mut mark.label, &mut mark.color].into_iter().flatten() {
            cut |= truncate_at_char(text, MAX_LABEL);
        }
    }
    if cut && log.last().is_none_or(|last| last != TRUNCATED) {
        log.push(TRUNCATED.to_owned());
    }
}

fn cap_error(error: &mut BotError) {
    truncate_at_char(&mut error.message, MAX_ERROR_TEXT);
    if let Some(traceback) = &mut error.traceback {
        truncate_at_char(traceback, MAX_ERROR_TEXT);
    }
}

/// Cuts `text` to at most `max` bytes on a character boundary; true if it cut.
fn truncate_at_char(text: &mut String, max: usize) -> bool {
    if text.len() <= max {
        return false;
    }
    let end = (0..=max)
        .rev()
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(0);
    text.truncate(end);
    true
}

#[cfg(test)]
mod tests {
    use super::{
        BotError, DecodeError, End, ErrorReply, FromBot, Hello, Init, InitPlayer, MAX_LABEL,
        MAX_LINE, MAX_LOG_BYTES, MAX_MARK_CELLS, MAX_MARKS, Mark, MoveReply, Phase, Ready,
        TRUNCATED, ToBot, Turn, decode_bot_line, decode_dedale_line, encode_line,
    };
    use crate::game::{EndReason, PlayerState};
    use crate::grid::{Move, Pos};
    use proptest::prelude::*;

    fn line(json: &str) -> String {
        format!("{json}\n")
    }

    #[test]
    fn golden_dedale_messages() {
        let init = ToBot::Init(Init {
            protocol: 1,
            code: Some("ded1-5x5-l0-p100-0000000000000000".to_owned()),
            width: 5,
            height: 5,
            cells: "_.....###.......###....._".to_owned(),
            you: 0,
            players: vec![
                InitPlayer {
                    pos: Pos::new(0, 0),
                },
                InitPlayer {
                    pos: Pos::new(4, 4),
                },
            ],
            startup_ms: 2000,
            move_ms: 100,
            max_turns: 100,
        });
        let json = r#"{"type":"init","protocol":1,"code":"ded1-5x5-l0-p100-0000000000000000","width":5,"height":5,"cells":"_.....###.......###....._","you":0,"players":[{"pos":[0,0]},{"pos":[4,4]}],"startup_ms":2000,"move_ms":100,"max_turns":100}"#;
        assert_eq!(encode_line(&init), line(json));
        assert_eq!(decode_dedale_line(json.as_bytes()), Ok(init));

        let turn = ToBot::Turn(Turn {
            turn: 1,
            players: vec![
                PlayerState {
                    pos: Pos::new(0, 0),
                    half_points: 0,
                    last_move: None,
                },
                PlayerState {
                    pos: Pos::new(4, 4),
                    half_points: 2,
                    last_move: Some(Move::Up),
                },
            ],
            collected: vec![Pos::new(1, 0)],
        });
        let json = r#"{"type":"turn","turn":1,"players":[{"pos":[0,0],"half_points":0,"last_move":null},{"pos":[4,4],"half_points":2,"last_move":"up"}],"collected":[[1,0]]}"#;
        assert_eq!(encode_line(&turn), line(json));
        assert_eq!(decode_dedale_line(json.as_bytes()), Ok(turn));

        let end = ToBot::End(End {
            reason: EndReason::Cleared,
            turn: 812,
            half_points: vec![700, 692],
        });
        let json = r#"{"type":"end","reason":"cleared","turn":812,"half_points":[700,692]}"#;
        assert_eq!(encode_line(&end), line(json));
        assert_eq!(decode_dedale_line(json.as_bytes()), Ok(end));
    }

    #[test]
    fn golden_bot_messages() {
        let cases = [
            (
                FromBot::Hello(Hello { protocol: 1 }),
                r#"{"type":"hello","protocol":1}"#,
            ),
            (
                FromBot::Ready(Ready {
                    init_us: Some(39870),
                    log: vec![],
                    marks: vec![],
                }),
                r#"{"type":"ready","init_us":39870,"log":[],"marks":[]}"#,
            ),
            (
                FromBot::Error(ErrorReply {
                    phase: Phase::Init,
                    error: BotError {
                        message: "boom".to_owned(),
                        traceback: Some("Traceback ...".to_owned()),
                    },
                    log: vec![],
                }),
                r#"{"type":"error","phase":"init","message":"boom","traceback":"Traceback ...","log":[]}"#,
            ),
            (
                FromBot::Move(MoveReply {
                    turn: 1,
                    mv: Some("up".to_owned()),
                    think_us: Some(812),
                    log: vec!["target (5,3)".to_owned()],
                    marks: vec![Mark {
                        cells: vec![Pos::new(1, 0), Pos::new(2, 0)],
                        label: Some("path".to_owned()),
                        color: None,
                    }],
                    error: None,
                }),
                r#"{"type":"move","turn":1,"move":"up","think_us":812,"log":["target (5,3)"],"marks":[{"cells":[[1,0],[2,0]],"label":"path","color":null}],"error":null}"#,
            ),
        ];
        for (msg, json) in cases {
            assert_eq!(encode_line(&msg), line(json));
            assert_eq!(decode_bot_line(json.as_bytes()), Ok(msg.clone()));
            assert_eq!(decode_bot_line(line(json).as_bytes()), Ok(msg));
        }
    }

    #[test]
    fn minimal_bot_messages_use_defaults() {
        let reply = decode_bot_line(br#"{"type":"move","turn":3,"move":"left","extra":true}"#);
        let Ok(FromBot::Move(reply)) = reply else {
            panic!("{reply:?}");
        };
        assert_eq!(reply.turn, 3);
        assert_eq!(reply.requested(), Ok(Some(Move::Left)));
        assert!(reply.log.is_empty() && reply.marks.is_empty());
        assert_eq!((reply.think_us, reply.error), (None, None));

        let Ok(FromBot::Ready(ready)) = decode_bot_line(br#"{"type":"ready"}"#) else {
            panic!();
        };
        assert_eq!(ready.init_us, None);
    }

    #[test]
    fn unknown_moves_keep_the_message() {
        let Ok(FromBot::Move(reply)) =
            decode_bot_line(br#"{"type":"move","turn":3,"move":"north"}"#)
        else {
            panic!();
        };
        assert!(reply.requested().is_err());
        let Ok(FromBot::Move(reply)) = decode_bot_line(br#"{"type":"move","turn":3,"move":null}"#)
        else {
            panic!();
        };
        assert_eq!(reply.requested(), Ok(None));
    }

    #[test]
    fn rejects_bad_lines() {
        assert_eq!(decode_bot_line(b"\xff"), Err(DecodeError::NotUtf8));
        assert!(matches!(
            decode_bot_line(b"hello"),
            Err(DecodeError::Json(_))
        ));
        assert!(matches!(decode_bot_line(b"{}"), Err(DecodeError::Json(_))));
        assert!(matches!(
            decode_bot_line(br#"{"type":"turn","turn":1,"players":[],"collected":[]}"#),
            Err(DecodeError::Json(_))
        ));
        assert!(matches!(
            decode_bot_line(br#"{"type":"move","turn":"1"}"#),
            Err(DecodeError::Json(_))
        ));
        assert_eq!(
            decode_bot_line(br#"{"type":"hello","protocol":2}"#),
            Err(DecodeError::ProtocolVersion { found: 2 })
        );
        let long = format!(r#"{{"type":"ready","log":["{}"]}}"#, "x".repeat(MAX_LINE));
        assert_eq!(decode_bot_line(long.as_bytes()), Err(DecodeError::TooLong));
    }

    #[test]
    fn caps_logs() {
        let log = vec!["a".repeat(4000), "é".repeat(100), "lost".to_owned()];
        let mut reply = MoveReply {
            turn: 1,
            mv: None,
            think_us: None,
            log,
            marks: vec![],
            error: None,
        };
        let json = encode_line(&FromBot::Move(reply.clone()));
        let Ok(FromBot::Move(capped)) = decode_bot_line(json.as_bytes()) else {
            panic!();
        };
        assert_eq!(capped.log.len(), 3);
        assert_eq!(capped.log[1], "é".repeat(42));
        assert_eq!(capped.log[2], TRUNCATED);
        let kept: usize = capped.log.iter().map(String::len).sum();
        assert!(kept <= MAX_LOG_BYTES);

        reply.log = vec!["short".to_owned()];
        let json = encode_line(&FromBot::Move(reply.clone()));
        assert_eq!(decode_bot_line(json.as_bytes()), Ok(FromBot::Move(reply)));
    }

    #[test]
    fn caps_marks() {
        let mark = |n: usize| Mark {
            cells: vec![Pos::new(0, 0); n],
            label: Some("l".repeat(100)),
            color: None,
        };
        let ready = Ready {
            init_us: None,
            log: vec![],
            // 70 × 70 cells exceeds both mark caps and stays under MAX_LINE.
            marks: (0..70).map(|_| mark(70)).collect(),
        };
        let Ok(FromBot::Ready(capped)) =
            decode_bot_line(encode_line(&FromBot::Ready(ready)).as_bytes())
        else {
            panic!();
        };
        assert_eq!(capped.marks.len(), MAX_MARKS);
        let cells: usize = capped.marks.iter().map(|m| m.cells.len()).sum();
        assert_eq!(cells, MAX_MARK_CELLS);
        assert!(
            capped
                .marks
                .iter()
                .all(|m| m.label.as_ref().unwrap().len() == MAX_LABEL)
        );
        assert_eq!(capped.log, vec![TRUNCATED.to_owned()]);
    }

    fn text() -> impl Strategy<Value = String> {
        "\\PC{0,24}"
    }

    fn pos() -> impl Strategy<Value = Pos> {
        any::<(u8, u8)>().prop_map(Pos::from)
    }

    fn mark() -> impl Strategy<Value = Mark> {
        (
            prop::collection::vec(pos(), 0..8),
            prop::option::of("\\PC{0,16}"),
            prop::option::of("\\PC{0,16}"),
        )
            .prop_map(|(cells, label, color)| Mark {
                cells,
                label,
                color,
            })
    }

    fn bot_error() -> impl Strategy<Value = BotError> {
        (text(), prop::option::of(text()))
            .prop_map(|(message, traceback)| BotError { message, traceback })
    }

    fn from_bot() -> impl Strategy<Value = FromBot> {
        let log = || prop::collection::vec(text(), 0..8);
        let marks = || prop::collection::vec(mark(), 0..8);
        prop_oneof![
            Just(FromBot::Hello(Hello { protocol: 1 })),
            (prop::option::of(any::<u64>()), log(), marks()).prop_map(|(init_us, log, marks)| {
                FromBot::Ready(Ready {
                    init_us,
                    log,
                    marks,
                })
            }),
            (bot_error(), log()).prop_map(|(error, log)| FromBot::Error(ErrorReply {
                phase: Phase::Init,
                error,
                log,
            })),
            (
                any::<u32>(),
                prop::option::of(text()),
                prop::option::of(any::<u64>()),
                log(),
                marks(),
                prop::option::of(bot_error()),
            )
                .prop_map(|(turn, mv, think_us, log, marks, error)| {
                    FromBot::Move(MoveReply {
                        turn,
                        mv,
                        think_us,
                        log,
                        marks,
                        error,
                    })
                }),
        ]
    }

    proptest! {
        #[test]
        fn bot_messages_roundtrip(msg in from_bot()) {
            prop_assert_eq!(decode_bot_line(encode_line(&msg).as_bytes()), Ok(msg));
        }

        #[test]
        fn caps_hold_and_are_idempotent(
            log in prop::collection::vec("\\PC{0,600}", 0..30),
            marks in prop::collection::vec(
                (0usize..200).prop_map(|n| Mark { cells: vec![Pos::new(1, 2); n], label: None, color: None }),
                0..80,
            ),
        ) {
            let msg = FromBot::Ready(Ready { init_us: None, log, marks });
            if let Ok(capped) = decode_bot_line(encode_line(&msg).as_bytes()) {
                let FromBot::Ready(ready) = &capped else { unreachable!() };
                prop_assert!(ready.log.iter().map(String::len).sum::<usize>() <= MAX_LOG_BYTES);
                prop_assert!(ready.marks.len() <= MAX_MARKS);
                prop_assert!(ready.marks.iter().map(|m| m.cells.len()).sum::<usize>() <= MAX_MARK_CELLS);
                let again = decode_bot_line(encode_line(&capped).as_bytes());
                prop_assert_eq!(again, Ok(capped));
            }
        }
    }
}
