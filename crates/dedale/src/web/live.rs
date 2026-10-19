//! Live sessions: `dedale run --view` and `dedale tournament --view`.
//!
//! The page starts the session (no bot runs before `POST /api/start`),
//! sets its pace, and follows it as a stream of events built from the
//! replay types, so it can assemble exactly the records Dedale writes.
//! Every connection gets a snapshot first; snapshots and events are made
//! under one lock, so the events after a snapshot are exactly the ones it
//! does not include.

use std::io;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use dedale_core::game::{GameResult, Snapshot};
use dedale_core::record::{EventRecord, GameRecord, PlayerRecord, StartupRecord, TurnRecord};
use dedale_core::stats::PlayerStats;
use serde::{Deserialize, Serialize};

use super::browse::Browse;
use super::http::{Method, Request, Response};
use super::server::Handler;
use super::sse::{Broadcast, Event};
use crate::interrupt;
use crate::referee::{GameStart, MatchError, Observer};
use crate::tournament::{Mode, Standing};

/// Longest delay between turns the page may ask for.
pub const MAX_DELAY_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveMode {
    Run,
    Tournament,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Until the page presses Start.
    Waiting,
    Running,
    Finished,
    Failed,
}

/// How fast games are played: full speed by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pace {
    pub paused: bool,
    /// Held after each turn.
    pub delay_ms: u64,
}

/// What a session will play, shown before Start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub mode: LiveMode,
    pub title: String,
    pub bots: Vec<String>,
    /// Games planned.
    pub games: usize,
    /// Timings and mazes, in a line.
    pub settings: String,
    /// A tournament's mode.
    pub tournament: Option<Mode>,
    /// A run's game, before it starts.
    pub start: Option<GameStart>,
}

/// `GET /api/session`, and the session in a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub mode: LiveMode,
    pub title: String,
    pub bots: Vec<String>,
    /// Games planned.
    pub games: usize,
    pub settings: String,
    pub tournament: Option<Mode>,
    pub start: Option<GameStart>,
    pub phase: Phase,
    /// Why the session failed.
    pub error: Option<String>,
    /// Once finished, `replay.json` or `tournament.json`, relative to the
    /// output directory (`GET /api/file`).
    pub file: Option<String>,
}

/// What a game's record holds besides its start and turns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameEnd {
    pub format: u32,
    pub dedale: String,
    pub players: Vec<PlayerRecord>,
    pub events: Vec<EventRecord>,
    pub result: GameResult,
    pub stats: Vec<PlayerStats>,
    /// The game's `replay.json`, relative to the output directory.
    pub replay: String,
}

/// A game as far as it went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveGame {
    /// 1-based.
    pub game: usize,
    pub start: GameStart,
    /// The latest startup report per player.
    pub startups: Vec<Option<StartupRecord>>,
    /// The turns so far while it is played; empty once it ended, since its
    /// replay holds them.
    pub turns: Vec<TurnRecord>,
    pub end: Option<GameEnd>,
}

/// The `snapshot` event: everything so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveSnapshot {
    pub session: Session,
    pub pace: Pace,
    pub games: Vec<LiveGame>,
    pub standings: Option<Vec<Standing>>,
}

/// A live session, shared by the referee's thread and the server's.
#[derive(Debug)]
pub struct Live {
    files: Browse,
    state: Mutex<State>,
    started: Condvar,
}

#[derive(Debug)]
struct State {
    session: Session,
    pace: Pace,
    games: Vec<LiveGame>,
    standings: Option<Vec<Standing>>,
    broadcast: Broadcast,
}

impl State {
    fn publish(&mut self, name: &str, data: &impl Serialize) {
        self.broadcast.publish(&Event::json(name, data));
    }

    fn snapshot(&self) -> LiveSnapshot {
        LiveSnapshot {
            session: self.session.clone(),
            pace: self.pace,
            games: self.games.clone(),
            standings: self.standings.clone(),
        }
    }
}

#[derive(Serialize)]
struct GameStartEvent<'a> {
    game: usize,
    #[serde(flatten)]
    start: &'a GameStart,
}

#[derive(Serialize)]
struct StartupEvent<'a> {
    game: usize,
    player: usize,
    startup: &'a StartupRecord,
}

#[derive(Serialize)]
struct TurnEvent<'a> {
    game: usize,
    turn: &'a TurnRecord,
}

#[derive(Serialize)]
struct GameEndEvent<'a> {
    game: usize,
    #[serde(flatten)]
    end: &'a GameEnd,
}

/// `POST /api/control`: what to change.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Control {
    paused: Option<bool>,
    delay_ms: Option<u64>,
}

impl Live {
    /// A session waiting for Start, whose results are written under `out`.
    ///
    /// # Errors
    ///
    /// When `out` is not a directory.
    pub fn new(plan: Plan, out: &Path) -> io::Result<Arc<Self>> {
        Ok(Arc::new(Self {
            files: Browse::new(out)?,
            state: Mutex::new(State {
                session: Session {
                    mode: plan.mode,
                    title: plan.title,
                    bots: plan.bots,
                    games: plan.games,
                    settings: plan.settings,
                    tournament: plan.tournament,
                    start: plan.start,
                    phase: Phase::Waiting,
                    error: None,
                    file: None,
                },
                pace: Pace::default(),
                games: Vec::new(),
                standings: None,
                broadcast: Broadcast::default(),
            }),
            started: Condvar::new(),
        }))
    }

    /// A panic on another thread leaves the state usable.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Blocks until the page presses Start.
    ///
    /// # Errors
    ///
    /// [`MatchError::Interrupted`] on Ctrl-C.
    pub fn wait_for_start(&self) -> Result<(), MatchError> {
        let mut state = self.lock();
        while state.session.phase == Phase::Waiting {
            if interrupt::requested() {
                return Err(MatchError::Interrupted);
            }
            state = self
                .started
                .wait_timeout(state, interrupt::POLL)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        Ok(())
    }

    /// Watches games for the page and paces them.
    #[must_use]
    pub fn observer(self: &Arc<Self>) -> LiveObserver {
        LiveObserver {
            live: Arc::clone(self),
            last_turn: Instant::now(),
        }
    }

    #[must_use]
    pub fn phase(&self) -> Phase {
        self.lock().session.phase
    }

    #[must_use]
    pub fn pace(&self) -> Pace {
        self.lock().pace
    }

    /// The game being played ended and its replay is at `replay` (relative
    /// to the output directory).
    pub fn game_done(&self, record: &GameRecord, replay: &str) {
        let mut state = self.lock();
        let Some(game) = state.games.last_mut() else {
            return;
        };
        let end = GameEnd {
            format: record.format,
            dedale: record.dedale.clone(),
            players: record.players.clone(),
            events: record.events.clone(),
            result: record.result.clone(),
            stats: record.stats.clone(),
            replay: replay.to_owned(),
        };
        game.turns = Vec::new();
        let event = GameEndEvent {
            game: game.game,
            end: &end,
        };
        let event = Event::json("game_end", &event);
        game.end = Some(end);
        state.broadcast.publish(&event);
    }

    /// The tournament's leaderboard after a game.
    pub fn standings(&self, standings: &[Standing]) {
        let mut state = self.lock();
        state.standings = Some(standings.to_vec());
        state.publish("standings", &serde_json::json!({ "standings": standings }));
    }

    /// Everything was played; `file` is the result (relative to the output
    /// directory).
    pub fn finished(&self, file: &str) {
        let mut state = self.lock();
        state.session.phase = Phase::Finished;
        state.session.file = Some(file.to_owned());
        state.publish("finished", &serde_json::json!({ "file": file }));
    }

    pub fn failed(&self, message: &str) {
        let mut state = self.lock();
        state.session.phase = Phase::Failed;
        state.session.error = Some(message.to_owned());
        state.publish("error", &serde_json::json!({ "message": message }));
    }

    fn events(&self) -> Response {
        let mut state = self.lock();
        let mut first = vec![Event::json("snapshot", &state.snapshot())];
        if state.session.phase == Phase::Waiting {
            first.push(Event::json("waiting", &serde_json::json!({})));
        }
        Response::events(state.broadcast.subscribe_with(first))
    }

    fn start(&self) -> Response {
        let mut state = self.lock();
        if state.session.phase != Phase::Waiting {
            return Response::error(409, "already started");
        }
        state.session.phase = Phase::Running;
        self.started.notify_all();
        Response::json(200, &serde_json::json!({}))
    }

    fn control(&self, body: &[u8]) -> Response {
        let control: Control = match serde_json::from_slice(body) {
            Ok(control) => control,
            Err(e) => return Response::error(400, &format!("bad control: {e}")),
        };
        if control.delay_ms.is_some_and(|ms| ms > MAX_DELAY_MS) {
            return Response::error(400, &format!("delay_ms is at most {MAX_DELAY_MS}"));
        }
        let mut state = self.lock();
        if let Some(paused) = control.paused {
            state.pace.paused = paused;
        }
        if let Some(delay_ms) = control.delay_ms {
            state.pace.delay_ms = delay_ms;
        }
        let pace = state.pace;
        state.publish("pace", &pace);
        Response::json(200, &pace)
    }
}

impl Handler for Live {
    fn session(&self) -> serde_json::Value {
        serde_json::to_value(&self.lock().session).unwrap_or_default()
    }

    fn api(&self, request: &Request) -> Response {
        match (request.path.as_str(), &request.method) {
            ("/api/events", Method::Get) => self.events(),
            ("/api/start", Method::Post) => self.start(),
            ("/api/control", Method::Post) => self.control(&request.body),
            ("/api/events", _) => Response::method_not_allowed("GET"),
            ("/api/start" | "/api/control", _) => Response::method_not_allowed("POST"),
            ("/api/file" | "/api/runs", _) => self.files.api(request),
            _ => Response::not_found(),
        }
    }
}

/// Reports every game to the page and holds it at the page's pace.
#[derive(Debug)]
pub struct LiveObserver {
    live: Arc<Live>,
    last_turn: Instant,
}

impl Observer for LiveObserver {
    fn game_start(&mut self, start: &GameStart) {
        let mut state = self.live.lock();
        let game = state.games.len() + 1;
        state.games.push(LiveGame {
            game,
            start: start.clone(),
            startups: vec![None; start.players.len()],
            turns: Vec::new(),
            end: None,
        });
        state.publish("game_start", &GameStartEvent { game, start });
        self.last_turn = Instant::now();
    }

    fn startup(&mut self, player: usize, startup: &StartupRecord) {
        let mut state = self.live.lock();
        let Some(current) = state.games.last_mut() else {
            return;
        };
        if let Some(slot) = current.startups.get_mut(player) {
            *slot = Some(startup.clone());
        }
        let game = current.game;
        state.publish(
            "startup",
            &StartupEvent {
                game,
                player,
                startup,
            },
        );
    }

    fn turn(&mut self, _snapshot: &Snapshot, turn: &TurnRecord) {
        let mut state = self.live.lock();
        let Some(current) = state.games.last_mut() else {
            return;
        };
        current.turns.push(turn.clone());
        let game = current.game;
        state.publish("turn", &TurnEvent { game, turn });
        self.last_turn = Instant::now();
    }

    fn hold(&mut self, _turn: u32) -> Option<Duration> {
        let pace = self.live.pace();
        if pace.paused {
            return Some(interrupt::POLL);
        }
        (self.last_turn + Duration::from_millis(pace.delay_ms))
            .checked_duration_since(Instant::now())
            .filter(|wait| !wait.is_zero())
    }
}
