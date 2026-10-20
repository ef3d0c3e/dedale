//! Messages between Dedale and bots.
//!
//! One JSON object per line. Bot output is untrusted: [`decode_bot_line`]
//! never panics, ignores unknown fields, and caps logs and marks whatever the
//! harness did.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
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
/// Longest mark label, and longest error message or traceback, in bytes.
pub const MAX_LABEL: usize = 64;
pub const MAX_ERROR_TEXT: usize = 4096;
/// Longest name or author a bot can give, and longest description, in bytes.
pub const MAX_NAME: usize = 64;
pub const MAX_DESCRIPTION: usize = 1024;
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    #[serde(flatten)]
    pub about: About,
}

/// What a bot says about itself, for people: never an identifier or a path.
/// Any value but a string reads as `None` for the texts, and anything but a
/// whole number from 1 for the version; [`decode_bot_line`] cleans the rest
/// with [`About::clean`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct About {
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub author: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub description: Option<String>,
    /// Raised by the bot's author whenever it changes, from 1.
    #[serde(default, deserialize_with = "positive")]
    pub version: Option<u32>,
}

impl About {
    /// Makes each text one line of plain text: control and bidirectional
    /// formatting characters removed, whitespace runs made single spaces,
    /// trimmed, and capped at [`MAX_NAME`] or [`MAX_DESCRIPTION`] bytes. An
    /// empty text becomes `None`. Cleaning twice changes nothing.
    pub fn clean(&mut self) {
        for (text, max) in [
            (&mut self.name, MAX_NAME),
            (&mut self.author, MAX_NAME),
            (&mut self.description, MAX_DESCRIPTION),
        ] {
            *text = text.as_deref().and_then(|t| clean_text(t, max));
        }
        self.version = self.version.filter(|&v| v > 0);
    }

    /// `Scorer v3`, or the name alone without a version.
    #[must_use]
    pub fn versioned(&self, name: &str) -> String {
        match self.version {
            Some(version) => format!("{name} v{version}"),
            None => name.to_owned(),
        }
    }
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
    /// `None` draws in the player's colour. Anything but a palette name
    /// reads as `None`.
    #[serde(default, deserialize_with = "lenient")]
    pub color: Option<MarkColor>,
    /// Omitted when writing cells (the default); an unknown kind reads as cells.
    #[serde(
        default,
        deserialize_with = "lenient_kind",
        skip_serializing_if = "MarkKind::is_cells"
    )]
    pub kind: MarkKind,
}

/// The colours a mark can take; viewers choose the shades.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkColor {
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Blue,
    Purple,
    Pink,
}

impl MarkColor {
    pub const ALL: [MarkColor; 8] = [
        MarkColor::Red,
        MarkColor::Orange,
        MarkColor::Yellow,
        MarkColor::Green,
        MarkColor::Teal,
        MarkColor::Blue,
        MarkColor::Purple,
        MarkColor::Pink,
    ];
}

/// How a mark is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    /// The cells are highlighted.
    #[default]
    Cells,
    /// A line through the cells, in order: a route.
    Path,
}

impl MarkKind {
    #[must_use]
    pub fn is_cells(&self) -> bool {
        *self == MarkKind::Cells
    }
}

/// A value Dedale knows, or `None` for anything else (other names, other
/// types): bots' marks are decoration, never a reason to drop a message.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<T>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).ok())
}

/// A whole number from 1, or `None` for anything else (0, negative, too
/// large, a float, a string, a boolean).
fn positive<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    Ok(lenient::<D, u32>(d)?.filter(|&v| v > 0))
}

fn lenient_kind<'de, D: Deserializer<'de>>(d: D) -> Result<MarkKind, D::Error> {
    Ok(lenient(d)?.unwrap_or_default())
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
        FromBot::Hello(hello) => hello.about.clean(),
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
        if let Some(label) = &mut mark.label {
            cut |= truncate_at_char(label, MAX_LABEL);
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

fn clean_text(text: &str, max: usize) -> Option<String> {
    let words = text
        .split(|c: char| c.is_whitespace() || c.is_control())
        .map(|word| {
            word.chars()
                .filter(|&c| !is_bidi_control(c))
                .collect::<String>()
        })
        .filter(|word| !word.is_empty());
    let mut clean = String::new();
    for word in words {
        if clean.len() > max {
            break;
        }
        if !clean.is_empty() {
            clean.push(' ');
        }
        clean.push_str(&word);
    }
    truncate_at_char(&mut clean, max);
    clean.truncate(clean.trim_end().len());
    (!clean.is_empty()).then_some(clean)
}

/// Characters that reorder the text around them, so a name could read as
/// another.
fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
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
        About, BotError, DecodeError, End, ErrorReply, FromBot, Hello, Init, InitPlayer,
        MAX_DESCRIPTION, MAX_LABEL, MAX_LINE, MAX_LOG_BYTES, MAX_MARK_CELLS, MAX_MARKS, MAX_NAME,
        Mark, MarkColor, MarkKind, MoveReply, Phase, Ready, TRUNCATED, ToBot, Turn,
        decode_bot_line, decode_dedale_line, encode_line, is_bidi_control,
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
            code: Some("ded2-5x5-l0-p14-0000000000000000".to_owned()),
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
        let json = r#"{"type":"init","protocol":1,"code":"ded2-5x5-l0-p14-0000000000000000","width":5,"height":5,"cells":"_.....###.......###....._","you":0,"players":[{"pos":[0,0]},{"pos":[4,4]}],"startup_ms":2000,"move_ms":100,"max_turns":100}"#;
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
                FromBot::Hello(Hello {
                    protocol: 1,
                    about: About::default(),
                }),
                r#"{"type":"hello","protocol":1,"name":null,"author":null,"description":null,"version":null}"#,
            ),
            (
                FromBot::Hello(Hello {
                    protocol: 1,
                    about: About {
                        name: Some("Scorer".to_owned()),
                        author: Some("Ada".to_owned()),
                        description: Some("Scores points.".to_owned()),
                        version: Some(3),
                    },
                }),
                r#"{"type":"hello","protocol":1,"name":"Scorer","author":"Ada","description":"Scores points.","version":3}"#,
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
                        kind: MarkKind::Cells,
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
    fn hellos_are_cleaned() {
        let hello = |json: &str| match decode_bot_line(json.as_bytes()) {
            Ok(FromBot::Hello(hello)) => hello.about,
            other => panic!("{other:?}"),
        };
        assert_eq!(hello(r#"{"type":"hello","protocol":1}"#), About::default());
        let about = hello(
            r#"{"type":"hello","protocol":1,"name":"  Scorer\n\t v2 ","author":"\u202eAda\u001b[31m","description":42}"#,
        );
        assert_eq!(about.name.as_deref(), Some("Scorer v2"));
        assert_eq!(about.author.as_deref(), Some("Ada [31m"));
        assert_eq!(about.description, None);
        let about = hello(r#"{"type":"hello","protocol":1,"name":" \u200e ","author":["Ada"]}"#);
        assert_eq!(about, About::default());
        let version = |json: &str| {
            hello(&format!(
                r#"{{"type":"hello","protocol":1,"version":{json}}}"#
            ))
            .version
        };
        assert_eq!(version("1"), Some(1));
        assert_eq!(version("4294967295"), Some(u32::MAX));
        for bad in [
            "0",
            "-2",
            "4294967296",
            "2.5",
            "3.0",
            r#""3""#,
            "true",
            "null",
            "[3]",
        ] {
            assert_eq!(version(bad), None, "{bad}");
        }

        let long = format!(
            r#"{{"type":"hello","protocol":1,"name":"{}","description":"{}"}}"#,
            "é".repeat(MAX_NAME),
            "word ".repeat(MAX_DESCRIPTION)
        );
        let about = hello(&long);
        assert_eq!(about.name, Some("é".repeat(MAX_NAME / 2)));
        let description = about.description.unwrap();
        assert!(description.len() <= MAX_DESCRIPTION && description.ends_with("word"));
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
    fn mark_colours_and_kinds() {
        let reply = |marks: &str| {
            let line = format!(r#"{{"type":"move","turn":1,"move":"up","marks":[{marks}]}}"#);
            let Ok(FromBot::Move(reply)) = decode_bot_line(line.as_bytes()) else {
                panic!("{line}");
            };
            reply.marks
        };
        let marks = reply(r#"{"cells":[[0,0],[1,0]],"color":"teal","kind":"path"}"#);
        assert_eq!(
            (marks[0].color, marks[0].kind),
            (Some(MarkColor::Teal), MarkKind::Path)
        );
        // Bots' mistakes in decoration never cost the move.
        for (mark, color, kind) in [
            (r#"{"cells":[],"color":"magenta"}"#, None, MarkKind::Cells),
            (r##"{"cells":[],"color":"#ff0000"}"##, None, MarkKind::Cells),
            (
                r#"{"cells":[],"color":5,"kind":"circle"}"#,
                None,
                MarkKind::Cells,
            ),
            (
                r#"{"cells":[],"color":"PINK","kind":7}"#,
                None,
                MarkKind::Cells,
            ),
            (
                r#"{"cells":[],"color":"pink"}"#,
                Some(MarkColor::Pink),
                MarkKind::Cells,
            ),
        ] {
            let marks = reply(mark);
            assert_eq!((marks[0].color, marks[0].kind), (color, kind), "{mark}");
        }
        // Cells are the default kind and are written without it.
        let cells = Mark {
            cells: vec![Pos::new(2, 3)],
            label: None,
            color: Some(MarkColor::Red),
            kind: MarkKind::Cells,
        };
        let path = Mark {
            kind: MarkKind::Path,
            ..cells.clone()
        };
        assert_eq!(
            serde_json::to_string(&cells).unwrap(),
            r#"{"cells":[[2,3]],"label":null,"color":"red"}"#
        );
        assert_eq!(
            serde_json::to_string(&path).unwrap(),
            r#"{"cells":[[2,3]],"label":null,"color":"red","kind":"path"}"#
        );
    }

    #[test]
    fn caps_marks() {
        let mark = |n: usize| Mark {
            cells: vec![Pos::new(0, 0); n],
            label: Some("l".repeat(100)),
            color: None,
            kind: MarkKind::Cells,
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
            prop::option::of(prop::sample::select(MarkColor::ALL.to_vec())),
            prop::sample::select(vec![MarkKind::Cells, MarkKind::Path]),
        )
            .prop_map(|(cells, label, color, kind)| Mark {
                cells,
                label,
                color,
                kind,
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
            (
                prop::option::of(text()),
                prop::option::of(text()),
                prop::option::of(text()),
                prop::option::of(any::<u32>())
            )
                .prop_map(|(name, author, description, version)| {
                    let mut about = About {
                        name,
                        author,
                        description,
                        version,
                    };
                    about.clean();
                    FromBot::Hello(Hello { protocol: 1, about })
                }),
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
        fn cleaned_texts_are_plain_lines(
            name in any::<String>(),
            author in "(\\s|\\PC|\\p{Cc}|\u{202e}){0,100}",
            description in "\\PC{0,1500}",
            version in any::<u32>(),
        ) {
            let mut about = About {
                name: Some(name),
                author: Some(author),
                description: Some(description),
                version: Some(version),
            };
            about.clean();
            prop_assert_eq!(about.version, (version > 0).then_some(version));
            for (text, max) in [
                (&about.name, MAX_NAME),
                (&about.author, MAX_NAME),
                (&about.description, MAX_DESCRIPTION),
            ] {
                let Some(text) = text else { continue };
                prop_assert!(!text.is_empty() && text.len() <= max);
                prop_assert!(!text.starts_with(' ') && !text.ends_with(' ') && !text.contains("  "));
                let odd = |c: char| (c.is_whitespace() && c != ' ') || c.is_control() || is_bidi_control(c);
                prop_assert!(!text.chars().any(odd));
            }
            let mut again = about.clone();
            again.clean();
            prop_assert_eq!(again, about);
        }

        #[test]
        fn caps_hold_and_are_idempotent(
            log in prop::collection::vec("\\PC{0,600}", 0..30),
            marks in prop::collection::vec(
                (0usize..200).prop_map(|n| Mark {
                    cells: vec![Pos::new(1, 2); n],
                    label: None,
                    color: None,
                    kind: MarkKind::Cells,
                }),
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
