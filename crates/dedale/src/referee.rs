//! Runs one game end to end.
//!
//! Spawn the bots, wait for their `hello`, send `init` and give them the
//! startup budget, then play turns until the engine says the game is over.
//! A bot is never waited on past a deadline, and a bot that misbehaves only
//! ever costs itself: the turn resolves with a non-`ok` outcome.

use std::io;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use dedale_core::code::MazeCode;
use dedale_core::game::{
    Failure, Game, GameResult, Input, Seating, Snapshot, TurnReport, default_max_turns,
};
use dedale_core::grid::Pos;
use dedale_core::maze::Maze;
use dedale_core::protocol::{
    About, BotError, DecodeError, End, ErrorReply, FromBot, Init, InitPlayer, MAX_LINE, Mark,
    MoveReply, PROTOCOL_VERSION, Ready, ToBot, Turn, decode_bot_line,
};
use dedale_core::record::{
    ConfigRecord, EventKind, EventRecord, ExitRecord, GameRecord, PlayerRecord, PlayerTurnRecord,
    REPLAY_FORMAT, StartupOutcome, StartupRecord, TurnRecord,
};
use dedale_core::stats::compute_stats;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::interrupt;
use crate::process::{BotProcess, ExitReport, Output};

/// Events kept per player; a flood beyond this is summarized once.
const MAX_EVENTS_PER_PLAYER: usize = 1000;
/// Time a bot whose stdout closed gets to finish exiting, so its exit
/// status is its own rather than Dedale's kill.
const CRASH_GRACE: Duration = Duration::from_millis(100);
/// Background outputs handled per bot between two turns, so a bot writing
/// without pause cannot hold the referee.
const MAX_DRAIN: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchConfig {
    pub startup: Duration,
    pub move_timeout: Duration,
    /// `None` for the default, `4·W·H`.
    pub max_turns: Option<u32>,
    pub seating: Seating,
    /// Time for a bot to say `hello` (interpreter start), not counted
    /// against startup.
    pub hello_timeout: Duration,
    /// Time a bot gets to exit after `end` before it is killed.
    pub end_grace: Duration,
    /// Whether the entrants' commands run in a sandbox, for the record.
    pub sandbox: bool,
}

impl MatchConfig {
    /// The default settings for `seating`.
    #[must_use]
    pub fn new(seating: Seating) -> Self {
        Self {
            startup: Duration::from_secs(2),
            move_timeout: Duration::from_millis(100),
            max_turns: None,
            seating,
            hello_timeout: Duration::from_secs(10),
            end_grace: Duration::from_secs(1),
            sandbox: false,
        }
    }
}

/// A competitor ready to be started.
#[derive(Debug)]
pub struct Entrant {
    /// What the player is called.
    pub name: String,
    /// The bot's path, as the user gave it (for the record).
    pub path: String,
    /// What the bot says about itself, read before the game
    /// ([`crate::bot::BotSpec::about`]).
    pub about: About,
    /// The CPU its command pins it to, if any, for the record.
    pub cpu: Option<usize>,
    pub command: Command,
    pub stderr_log: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum MatchError {
    #[error("cannot start {player}: {source}")]
    Spawn {
        player: String,
        #[source]
        source: io::Error,
    },
    #[error("{seating:?} needs {expected} bots, got {got}")]
    WrongPlayerCount {
        seating: Seating,
        expected: usize,
        got: usize,
    },
    #[error("interrupted")]
    Interrupted,
}

/// A game about to be played, as [`Observer::game_start`] sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameStart {
    pub code: Option<MazeCode>,
    pub maze: Maze,
    pub config: ConfigRecord,
    pub players: Vec<PlayerStart>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerStart {
    pub name: String,
    pub path: String,
    pub about: About,
    /// The CPU the bot is pinned to, if any; written only when it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<u32>,
    pub start: Pos,
}

/// Watches a game as it is played and sets its pace. Every
/// method does nothing by default; `()` is the observer that sees nothing.
pub trait Observer {
    /// Before the bots are started.
    fn game_start(&mut self, _start: &GameStart) {}

    /// A bot's startup changed: it became ready (in time or late), its
    /// `init()` failed, it crashed before being ready, or the startup budget
    /// ran out before it was ready (a timeout, which becomes `late` if it
    /// gets ready afterwards).
    fn startup(&mut self, _player: usize, _startup: &StartupRecord) {}

    /// After every turn, with the state it led to.
    fn turn(&mut self, _snapshot: &Snapshot, _turn: &TurnRecord) {}

    /// Once the game is over, with the record [`run_match`] returns.
    fn game_end(&mut self, _record: &GameRecord) {}

    /// Asked before each turn is sent: `None` plays it, `Some(wait)` holds
    /// the game and asks again after `wait` (at most [`interrupt::POLL`], so
    /// a stop request is still honoured). Holding happens only between
    /// turns, before a bot's clock starts.
    fn hold(&mut self, _turn: u32) -> Option<Duration> {
        None
    }
}

impl Observer for () {}

/// Plays one game and returns its full record, telling `observer` what
/// happens as it happens.
///
/// # Errors
///
/// When the number of entrants does not match the seating, a bot cannot be
/// started, or [`interrupt::request`] was called (every bot is killed).
/// Misbehaving bots are never an error.
///
/// # Panics
///
/// Never: the turn loop only ends once the engine reports a result.
pub fn run_match(
    maze: Maze,
    code: Option<MazeCode>,
    entrants: Vec<Entrant>,
    config: &MatchConfig,
    observer: &mut dyn Observer,
) -> Result<GameRecord, MatchError> {
    let expected = config.seating.players();
    if entrants.len() != expected {
        return Err(MatchError::WrongPlayerCount {
            seating: config.seating,
            expected,
            got: entrants.len(),
        });
    }
    let start = GameStart::new(maze, code, &entrants, config);
    observer.game_start(&start);
    let GameStart {
        maze,
        config: config_record,
        players: seated,
        ..
    } = start;
    let max_turns = config_record.max_turns;
    let starts: Vec<Pos> = seated.iter().map(|p| p.start).collect();

    let mut referee = Referee {
        seats: Vec::new(),
        events: Vec::new(),
        observer,
    };
    for entrant in entrants {
        referee.seats.push(Seat::spawn(entrant)?);
    }

    referee.handshake(config.hello_timeout)?;
    let init = |you| {
        ToBot::Init(Init {
            protocol: PROTOCOL_VERSION,
            code: code.map(|c| c.to_string()),
            width: maze.width(),
            height: maze.height(),
            cells: maze.to_flat(),
            you,
            players: starts.iter().map(|&pos| InitPlayer { pos }).collect(),
            startup_ms: millis(config.startup),
            move_ms: millis(config.move_timeout),
            max_turns,
        })
    };
    let init_sent = Instant::now();
    for (you, seat) in referee.seats.iter_mut().enumerate() {
        seat.send(&init(you));
        seat.init_sent = init_sent;
        seat.startup_deadline = init_sent + config.startup;
    }
    referee.await_startup(init_sent + config.startup)?;

    let mut game = Game::new(maze.clone(), config.seating, max_turns);
    let mut turns: Vec<TurnRecord> = Vec::new();
    while !game.is_over() {
        let collected = turns.last().map(|t| t.collected.clone());
        let turn = referee.play_turn(&mut game, collected.unwrap_or_default(), config)?;
        turns.push(turn);
    }

    let result = game.result().expect("the game is over");
    referee.finish(&result, config.end_grace);
    let players = referee
        .seats
        .into_iter()
        .zip(starts)
        .map(|(seat, start)| seat.into_record(start))
        .collect();
    let mut record = GameRecord {
        format: REPLAY_FORMAT,
        dedale: env!("CARGO_PKG_VERSION").to_owned(),
        code,
        maze,
        config: config_record,
        players,
        turns,
        events: referee.events,
        result,
        stats: Vec::new(),
    };
    record.stats = compute_stats(&record);
    referee.observer.game_end(&record);
    Ok(record)
}

impl GameStart {
    /// The game `run_match` would play with these arguments, as the
    /// observer is told before the bots start.
    #[must_use]
    pub fn new(
        maze: Maze,
        code: Option<MazeCode>,
        entrants: &[Entrant],
        config: &MatchConfig,
    ) -> Self {
        let max_turns = config.max_turns.unwrap_or_else(|| default_max_turns(&maze));
        let players = entrants
            .iter()
            .zip(config.seating.starts(&maze))
            .map(|(entrant, start)| PlayerStart {
                name: entrant.name.clone(),
                path: entrant.path.clone(),
                about: entrant.about.clone(),
                cpu: entrant.cpu.and_then(|cpu| u32::try_from(cpu).ok()),
                start,
            })
            .collect();
        Self {
            code,
            maze,
            config: ConfigRecord {
                startup_ms: millis(config.startup),
                move_ms: millis(config.move_timeout),
                max_turns,
                seating: config.seating,
                sandbox: config.sandbox,
            },
            players,
        }
    }
}

struct Referee<'o> {
    seats: Vec<Seat>,
    events: Vec<EventRecord>,
    observer: &'o mut dyn Observer,
}

struct Seat {
    name: String,
    path: String,
    about: About,
    cpu: Option<usize>,
    /// `None` once the bot is gone (crashed or failed to start) and reaped.
    process: Option<BotProcess>,
    exit: Option<ExitReport>,
    ready: bool,
    init_sent: Instant,
    startup_deadline: Instant,
    startup: StartupRecord,
    events: usize,
}

/// A bot's answer to one turn, before the engine resolves it.
struct Reply {
    input: Input,
    latency_us: Option<u64>,
    think_us: Option<u64>,
    log: Vec<String>,
    marks: Vec<Mark>,
    error: Option<BotError>,
    invalid: Option<String>,
}

impl Reply {
    fn failed(failure: Failure) -> Self {
        Self {
            input: Input::Failed(failure),
            latency_us: None,
            think_us: None,
            log: Vec::new(),
            marks: Vec::new(),
            error: None,
            invalid: None,
        }
    }

    fn invalid(reason: String, latency_us: u64) -> Self {
        Self {
            latency_us: Some(latency_us),
            invalid: Some(reason),
            ..Self::failed(Failure::Invalid)
        }
    }

    fn from_move(reply: MoveReply, latency_us: u64) -> Self {
        let (input, invalid) = if reply.error.is_some() {
            (Input::Failed(Failure::Error), None)
        } else {
            match reply.requested() {
                Ok(Some(mv)) => (Input::Move(mv), None),
                Ok(None) => (Input::Failed(Failure::Invalid), Some("no move".to_owned())),
                Err(e) => (Input::Failed(Failure::Invalid), Some(e.to_string())),
            }
        };
        Self {
            input,
            latency_us: Some(latency_us),
            think_us: reply.think_us,
            log: reply.log,
            marks: reply.marks,
            error: reply.error,
            invalid,
        }
    }
}

impl Seat {
    fn spawn(entrant: Entrant) -> Result<Self, MatchError> {
        let process = BotProcess::spawn(entrant.command, entrant.stderr_log.as_deref()).map_err(
            |source| MatchError::Spawn {
                player: entrant.name.clone(),
                source,
            },
        )?;
        let now = Instant::now();
        Ok(Self {
            name: entrant.name,
            path: entrant.path,
            about: entrant.about,
            cpu: entrant.cpu,
            process: Some(process),
            exit: None,
            ready: false,
            init_sent: now,
            startup_deadline: now,
            startup: StartupRecord::pending(),
            events: 0,
        })
    }

    fn alive(&self) -> bool {
        self.process.is_some()
    }

    fn send(&mut self, msg: &ToBot) {
        // A failed send means stdin is gone; the closed stdout that comes
        // with it is how the crash gets reported.
        if let Some(process) = &self.process {
            let _ = process.send(msg);
        }
    }

    /// The next output before `deadline`, waking up every
    /// [`interrupt::POLL`] to honour a stop request.
    fn recv_until(&self, deadline: Instant) -> Result<Option<Output>, MatchError> {
        let Some(process) = &self.process else {
            return Ok(None);
        };
        loop {
            if interrupt::requested() {
                return Err(MatchError::Interrupted);
            }
            let slice = deadline.min(Instant::now() + interrupt::POLL);
            if let Some(output) = process.recv_until(slice) {
                return Ok(Some(output));
            }
            if slice >= deadline {
                return Ok(None);
            }
        }
    }

    /// Reaps a bot that is gone or must go; it plays no further part.
    fn retire(&mut self, grace: Duration) {
        if let Some(process) = self.process.take() {
            self.exit = Some(process.finish(grace));
        }
    }

    /// The bot's stdout closed: it exited or crashed.
    fn crashed(&mut self) {
        self.retire(CRASH_GRACE);
        if !self.ready {
            self.startup.outcome = StartupOutcome::Crashed;
        }
    }

    fn fail_startup(&mut self, message: String) {
        self.startup.error = Some(BotError {
            message,
            traceback: None,
        });
        self.crashed();
    }

    fn on_ready(&mut self, ready: Ready, at: Instant) {
        self.ready = true;
        self.startup.outcome = if at <= self.startup_deadline {
            StartupOutcome::Ok
        } else {
            StartupOutcome::Late
        };
        self.startup.startup_us = Some(micros(at - self.init_sent));
        self.startup.init_us = ready.init_us;
        self.startup.log = ready.log;
        self.startup.marks = ready.marks;
    }

    fn on_init_error(&mut self, reply: ErrorReply, at: Instant) {
        // The harness keeps answering turns after a failed init().
        self.ready = true;
        self.startup.outcome = StartupOutcome::InitError;
        self.startup.startup_us = Some(micros(at - self.init_sent));
        self.startup.log = reply.log;
        self.startup.error = Some(reply.error);
    }

    fn into_record(self, start: Pos) -> PlayerRecord {
        PlayerRecord {
            name: self.name,
            path: self.path,
            about: self.about,
            cpu: self.cpu.and_then(|cpu| u32::try_from(cpu).ok()),
            start,
            startup: self.startup,
            exit: self.exit.map(|exit| ExitRecord {
                code: exit.status.and_then(|s| s.code()),
                signal: exit.status.and_then(|s| s.signal()),
                killed: exit.killed,
                stderr_tail: exit.stderr_tail,
            }),
        }
    }
}

impl Referee<'_> {
    fn event(&mut self, turn: u32, player: usize, kind: EventKind) {
        let seat = &mut self.seats[player];
        seat.events += 1;
        if seat.events < MAX_EVENTS_PER_PLAYER {
            self.events.push(EventRecord { turn, player, kind });
        } else if seat.events == MAX_EVENTS_PER_PLAYER {
            let reason = format!("{MAX_EVENTS_PER_PLAYER} events; further ones are not recorded");
            self.events.push(EventRecord {
                turn,
                player,
                kind: EventKind::Ignored { reason },
            });
        }
    }

    fn ignored(&mut self, turn: u32, player: usize, reason: impl Into<String>) {
        let reason = reason.into();
        self.event(turn, player, EventKind::Ignored { reason });
    }

    fn report_startup(&mut self, player: usize) {
        self.observer.startup(player, &self.seats[player].startup);
    }

    /// The bot's stdout closed: it exited or crashed.
    fn crashed(&mut self, player: usize) {
        let starting = !self.seats[player].ready;
        self.seats[player].crashed();
        if starting {
            self.report_startup(player);
        }
    }

    /// Holds the game while the observer asks to, honouring a stop request.
    fn hold(&mut self, turn: u32) -> Result<(), MatchError> {
        while let Some(wait) = self.observer.hold(turn) {
            if interrupt::requested() {
                return Err(MatchError::Interrupted);
            }
            thread::sleep(wait.min(interrupt::POLL));
        }
        Ok(())
    }

    /// Waits for every bot's `hello`, all against one deadline. A bot that
    /// fails the handshake is retired at once, so it cannot run alongside
    /// the others.
    fn handshake(&mut self, timeout: Duration) -> Result<(), MatchError> {
        let deadline = Instant::now() + timeout;
        for player in 0..self.seats.len() {
            if !self.seats[player].alive() {
                continue;
            }
            let failure = match self.seats[player].recv_until(deadline)? {
                None => format!("no hello within {} ms", timeout.as_millis()),
                Some(Output::Closed { .. }) => "exited before saying hello".to_owned(),
                Some(Output::Oversized { .. }) => "oversized line instead of hello".to_owned(),
                Some(Output::Line { line, .. }) => match decode_bot_line(&line) {
                    Ok(FromBot::Hello(_)) => continue,
                    Ok(other) => format!("expected hello, got {}", message_type(&other)),
                    Err(e @ DecodeError::ProtocolVersion { .. }) => e.to_string(),
                    Err(e) => format!("expected hello: {e}"),
                },
            };
            self.seats[player].fail_startup(failure);
            self.report_startup(player);
        }
        Ok(())
    }

    /// Gives every bot until `deadline` to report `ready` (or an `init()`
    /// error); returns early once all have.
    fn await_startup(&mut self, deadline: Instant) -> Result<(), MatchError> {
        for player in 0..self.seats.len() {
            while self.seats[player].alive() && !self.seats[player].ready {
                let Some(output) = self.seats[player].recv_until(deadline)? else {
                    break;
                };
                self.background(player, 0, output);
            }
        }
        for player in 0..self.seats.len() {
            if self.seats[player].alive() && !self.seats[player].ready {
                self.report_startup(player);
            }
        }
        Ok(())
    }

    /// Handles whatever bots sent between turns: late replies, a late
    /// `ready`, a crash.
    fn drain(&mut self, turn: u32) {
        for player in 0..self.seats.len() {
            for _ in 0..MAX_DRAIN {
                let Some(output) = self.seats[player]
                    .process
                    .as_ref()
                    .and_then(BotProcess::try_recv)
                else {
                    break;
                };
                self.background(player, turn, output);
            }
        }
    }

    /// An output that is not the reply to the turn being played (`turn` has
    /// not been sent yet, or the bot is still starting).
    fn background(&mut self, player: usize, turn: u32, output: Output) {
        match output {
            Output::Closed { .. } => self.crashed(player),
            Output::Oversized { .. } => {
                self.ignored(turn, player, format!("line longer than {MAX_LINE} bytes"));
            }
            Output::Line { at, line } => match decode_bot_line(&line) {
                Err(e) => self.ignored(turn, player, e.to_string()),
                Ok(msg) => self.message(player, turn, msg, at),
            },
        }
    }

    fn message(&mut self, player: usize, turn: u32, msg: FromBot, at: Instant) {
        let seat = &mut self.seats[player];
        match msg {
            FromBot::Ready(ready) if !seat.ready => {
                seat.on_ready(ready, at);
                self.report_startup(player);
            }
            FromBot::Error(reply) if !seat.ready => {
                seat.on_init_error(reply, at);
                self.report_startup(player);
            }
            FromBot::Move(reply) if reply.turn < turn => {
                let for_turn = reply.turn;
                self.event(turn, player, EventKind::Late { for_turn });
            }
            other => {
                let reason = format!("unexpected {}", message_type(&other));
                self.ignored(turn, player, reason);
            }
        }
    }

    /// Plays the next turn: holds it while the observer asks to, sends it
    /// (with the cells `collected` on the previous one), and resolves it once
    /// every bot answered or the move timeout passed.
    fn play_turn(
        &mut self,
        game: &mut Game,
        collected: Vec<Pos>,
        config: &MatchConfig,
    ) -> Result<TurnRecord, MatchError> {
        let turn = game.snapshot().turn + 1;
        self.hold(turn)?;
        self.drain(turn);
        let msg = ToBot::Turn(Turn {
            turn,
            players: game.snapshot().players.clone(),
            collected,
        });
        let sent = Instant::now();
        for seat in &mut self.seats {
            seat.send(&msg);
        }
        let deadline = sent + config.move_timeout;
        let replies = (0..self.seats.len())
            .map(|player| self.await_reply(player, turn, sent, deadline))
            .collect::<Result<Vec<Reply>, MatchError>>()?;
        let inputs: Vec<Input> = replies.iter().map(|r| r.input).collect();
        let report = game.step(&inputs);
        let record = turn_record(report, replies);
        self.observer.turn(game.snapshot(), &record);
        Ok(record)
    }

    /// Waits for `player`'s reply to `turn` until `deadline`.
    fn await_reply(
        &mut self,
        player: usize,
        turn: u32,
        sent_at: Instant,
        deadline: Instant,
    ) -> Result<Reply, MatchError> {
        loop {
            let seat = &self.seats[player];
            if !seat.alive() {
                return Ok(Reply::failed(Failure::Crashed));
            }
            let ready = seat.ready;
            let Some(output) = seat.recv_until(deadline)? else {
                return Ok(Reply::failed(if ready {
                    Failure::Timeout
                } else {
                    Failure::NotReady
                }));
            };
            match output {
                Output::Line { at, line } if ready => match decode_bot_line(&line) {
                    Ok(FromBot::Move(reply)) if reply.turn == turn => {
                        return Ok(Reply::from_move(reply, micros(at - sent_at)));
                    }
                    Ok(msg) => self.message(player, turn, msg, at),
                    Err(e) => return Ok(Reply::invalid(e.to_string(), micros(at - sent_at))),
                },
                Output::Oversized { at } if ready => {
                    let reason = format!("line longer than {MAX_LINE} bytes");
                    return Ok(Reply::invalid(reason, micros(at - sent_at)));
                }
                other => self.background(player, turn, other),
            }
        }
    }

    /// Sends `end` and gives every bot one shared grace period to exit.
    fn finish(&mut self, result: &GameResult, grace: Duration) {
        let end = ToBot::End(End {
            reason: result.reason,
            turn: result.turns,
            half_points: result.half_points.clone(),
        });
        for seat in &mut self.seats {
            seat.send(&end);
        }
        let deadline = Instant::now() + grace;
        for seat in &mut self.seats {
            seat.retire(deadline.saturating_duration_since(Instant::now()));
        }
    }
}

fn turn_record(report: TurnReport, replies: Vec<Reply>) -> TurnRecord {
    let players = report
        .players
        .into_iter()
        .zip(replies)
        .map(|(turn, reply)| PlayerTurnRecord {
            requested: turn.requested,
            applied: turn.applied,
            outcome: turn.outcome,
            latency_us: reply.latency_us,
            think_us: reply.think_us,
            pos: turn.pos,
            gained: turn.gained,
            half_points: turn.half_points,
            log: reply.log,
            marks: reply.marks,
            error: reply.error,
            invalid: reply.invalid,
        })
        .collect();
    TurnRecord {
        turn: report.turn,
        players,
        collected: report.collected,
    }
}

fn message_type(msg: &FromBot) -> &'static str {
    match msg {
        FromBot::Hello(_) => "hello",
        FromBot::Ready(_) => "ready",
        FromBot::Error(_) => "error",
        FromBot::Move(_) => "move",
    }
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
