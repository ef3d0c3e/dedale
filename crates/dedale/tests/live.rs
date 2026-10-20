//! Live sessions (`--view`): started and paced from the page, streamed as
//! events that assemble into the records Dedale writes.

mod common;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use common::http::{EventStream, get, get_json, parse_view_line, post};
use common::{fake_bot, fake_bot_path};
use dedale::referee::{Entrant, GameStart, MatchConfig, run_match};
use dedale::replay;
use dedale::tournament::TournamentReport;
use dedale::web::live::{GameEnd, Live, LiveMode, LiveSnapshot, Phase, Plan};
use dedale::web::server::Server;
use dedale_core::game::Seating;
use dedale_core::maze::Maze;
use dedale_core::protocol::About;
use dedale_core::record::{GameRecord, StartupRecord, TurnRecord};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde_json::Value;

const RING: &str = "_....\n.###.\n.###.\n.###.\n...._\n";
const LAP: &str = "right,right,right,right,down,down,down,down,left,left,left,left,up,up,up,up";

/// A `dedale` command with `--view`, its stdout read up to the address.
/// Killed if the test fails.
struct Viewing {
    child: Child,
    stdout: BufReader<ChildStdout>,
    port: u16,
    token: String,
}

impl Viewing {
    fn spawn(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_dedale"))
            .args(args)
            .args(["--view", "--no-open"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut viewing = Self {
            child,
            stdout,
            port: 0,
            token: String::new(),
        };
        let line = viewing.wait_for_output("view    ");
        assert!(line.trim_end().ends_with("#/live"), "{line}");
        (viewing.port, viewing.token) = parse_view_line(&line);
        viewing
    }

    fn api(&self, path: &str) -> String {
        let sep = if path.contains('?') { '&' } else { '?' };
        format!("{path}{sep}token={}", self.token)
    }

    fn events(&self) -> EventStream {
        EventStream::open(self.port, &self.token)
    }

    /// The bots it runs, from /proc.
    fn children(&self) -> Vec<u32> {
        let pid = self.child.id();
        fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
            .unwrap_or_default()
            .split_whitespace()
            .map(|p| p.parse().unwrap())
            .collect()
    }

    /// Waits for a line holding `needle` on stdout.
    fn wait_for_output(&mut self, needle: &str) -> String {
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "never printed {needle:?}"
            );
            if line.contains(needle) {
                return line;
            }
        }
    }

    /// Ctrl-C; the exit code.
    fn interrupt(mut self) -> Option<i32> {
        let pid = Pid::from_raw(i32::try_from(self.child.id()).unwrap());
        kill(pid, Signal::SIGINT).unwrap();
        self.child.wait().unwrap().code()
    }
}

impl Drop for Viewing {
    fn drop(&mut self) {
        // Does nothing once waited for.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn parse<T: serde::de::DeserializeOwned>(value: &Value) -> T {
    serde_json::from_value(value.clone()).unwrap()
}

/// Assembles one game from its events: `game_start`, `startup`, `turn`
/// and `game_end`.
#[derive(Default)]
struct Assembly {
    start: Option<GameStart>,
    startups: Vec<(usize, StartupRecord)>,
    turns: Vec<TurnRecord>,
    end: Option<GameEnd>,
}

impl Assembly {
    fn feed(&mut self, name: &str, data: &Value) {
        match name {
            "game_start" => self.start = Some(parse(data)),
            "startup" => {
                let player = usize::try_from(data["player"].as_u64().unwrap()).unwrap();
                self.startups.push((player, parse(&data["startup"])));
            }
            "turn" => self.turns.push(parse(&data["turn"])),
            "game_end" => self.end = Some(parse(data)),
            other => panic!("not a game event: {other}"),
        }
    }

    fn record(self) -> GameRecord {
        let (start, end) = (self.start.unwrap(), self.end.unwrap());
        // The latest startup report of each player is the recorded one.
        for (i, player) in end.players.iter().enumerate() {
            let last = self.startups.iter().rev().find(|(p, _)| *p == i);
            assert_eq!(last.map(|(_, s)| s), Some(&player.startup));
        }
        GameRecord {
            format: end.format,
            dedale: end.dedale,
            code: start.code,
            maze: start.maze,
            config: start.config,
            players: end.players,
            turns: self.turns,
            events: end.events,
            result: end.result,
            stats: end.stats,
        }
    }
}

fn snapshot(data: &Value) -> LiveSnapshot {
    serde_json::from_value(data.clone()).unwrap()
}

#[test]
fn a_live_run_starts_from_the_page_and_streams_its_replay() {
    let dir = tempfile::tempdir().unwrap();
    let map = dir.path().join("ring.txt");
    fs::write(&map, RING).unwrap();
    let out = dir.path().join("out");
    let mut viewing = Viewing::spawn(&[
        "run",
        fake_bot_path(),
        "--map",
        map.to_str().unwrap(),
        "--max-turns",
        "20",
        "--out",
        out.to_str().unwrap(),
    ]);
    let port = viewing.port;

    let events = viewing.events();
    let (name, data) = events.next();
    assert_eq!(name, "snapshot");
    let first = snapshot(&data);
    assert_eq!(first.session.phase, Phase::Waiting);
    assert_eq!(first.session.mode, LiveMode::Run);
    assert_eq!(first.session.bots, ["dedale-fake-bot"]);
    assert!(first.games.is_empty());
    // The game to come, before Start.
    let preview = first.session.start.as_ref().unwrap();
    assert_eq!(preview.maze, Maze::parse(RING).unwrap());
    assert_eq!(preview.config.max_turns, 20);
    assert!(
        first.session.settings.contains("20 turns at most"),
        "{}",
        first.session.settings
    );
    assert_eq!(events.next().0, "waiting");
    let session = get_json(port, &viewing.api("/api/session"));
    assert_eq!(session["phase"], "waiting");
    // No bot before Start.
    thread::sleep(Duration::from_millis(300));
    assert!(viewing.children().is_empty());

    assert_eq!(post(port, &viewing.api("/api/start"), "").0, 200);
    assert_eq!(post(port, &viewing.api("/api/start"), "").0, 409);
    let mut game = Assembly::default();
    let finished = loop {
        let (name, data) = events.next();
        match name.as_str() {
            "finished" => break data,
            other => {
                assert_eq!(data["game"], 1, "{other}");
                game.feed(other, &data);
            }
        }
    };
    assert_eq!(finished["file"], "replay.json");
    let streamed = game.record();
    let written = replay::read(&out.join("replay.json")).unwrap();
    assert_eq!(streamed, written);
    assert_eq!(streamed.turns.len(), 20);

    // The finished game stays served.
    viewing.wait_for_output("still serving");
    let (status, body) = get(port, &viewing.api("/api/file?path=replay.json"));
    assert_eq!(
        (status, body),
        (200, fs::read(out.join("replay.json")).unwrap())
    );
    let late = viewing.events();
    let (name, data) = late.next();
    assert_eq!(name, "snapshot");
    let last = snapshot(&data);
    assert_eq!(last.session.phase, Phase::Finished);
    assert_eq!(last.session.file.as_deref(), Some("replay.json"));
    assert!(last.games[0].turns.is_empty(), "the replay holds them");
    assert_eq!(last.games[0].end.as_ref().unwrap().result, written.result);
    assert_eq!(viewing.interrupt(), Some(0));
}

#[test]
fn ctrl_c_while_waiting_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let viewing = Viewing::spawn(&[
        "run",
        fake_bot_path(),
        "--size",
        "5x5",
        "--out",
        dir.path().to_str().unwrap(),
    ]);
    assert!(viewing.children().is_empty());
    assert_eq!(viewing.interrupt(), Some(130));
    assert!(!dir.path().join("replay.json").exists());
}

#[test]
fn a_live_tournament_streams_every_game_and_the_standings() {
    let dir = tempfile::tempdir().unwrap();
    let map = dir.path().join("ring.txt");
    fs::write(&map, RING).unwrap();
    let out = dir.path().join("out");
    let viewing = Viewing::spawn(&[
        "tournament",
        fake_bot_path(),
        fake_bot_path(),
        "--map",
        map.to_str().unwrap(),
        "--games",
        "1",
        "--max-turns",
        "10",
        "--out",
        out.to_str().unwrap(),
    ]);
    let events = viewing.events();
    let first = snapshot(&events.next().1);
    assert_eq!(first.session.mode, LiveMode::Tournament);
    assert_eq!(first.session.games, 2);
    assert_eq!(
        first.session.tournament,
        Some(dedale::tournament::Mode::Duo)
    );
    assert!(
        first.session.settings.contains("master seed"),
        "{}",
        first.session.settings
    );
    assert_eq!(events.next().0, "waiting");
    assert_eq!(post(viewing.port, &viewing.api("/api/start"), "").0, 200);

    let (mut names, mut standings, mut replays) = (Vec::new(), Vec::new(), Vec::new());
    loop {
        let (name, data) = events.next();
        match name.as_str() {
            "finished" => {
                assert_eq!(data["file"], "tournament.json");
                break;
            }
            "standings" => standings.push(data["standings"].clone()),
            "game_end" => replays.push(data["replay"].as_str().unwrap().to_owned()),
            "game_start" | "startup" | "turn" => {}
            other => panic!("unexpected {other}"),
        }
        if name != "turn" {
            names.push((name, data["game"].clone()));
        }
    }
    let starts: Vec<&Value> = names
        .iter()
        .filter(|(n, _)| n == "game_start")
        .map(|(_, g)| g)
        .collect();
    assert_eq!(starts, [&Value::from(1), &Value::from(2)]);
    // A standings update follows each game's end.
    let order: Vec<&str> = names
        .iter()
        .map(|(n, _)| n.as_str())
        .filter(|n| matches!(*n, "game_end" | "standings"))
        .collect();
    assert_eq!(order, ["game_end", "standings", "game_end", "standings"]);
    let report: TournamentReport =
        serde_json::from_str(&fs::read_to_string(out.join("tournament.json")).unwrap()).unwrap();
    assert_eq!(
        standings.last().unwrap(),
        &serde_json::to_value(&report.leaderboard).unwrap()
    );
    for replay in &replays {
        let target = viewing.api(&format!("/api/file?path={}", replay.replace('/', "%2F")));
        assert_eq!(get(viewing.port, &target).0, 200, "{replay}");
    }
    assert_eq!(viewing.interrupt(), Some(0));
}

/// A session served in this process, whose game runs on a thread once
/// started.
struct InProcess {
    live: Arc<Live>,
    port: u16,
    token: String,
    game: thread::JoinHandle<GameRecord>,
    _server: dedale::web::server::ServerHandle,
}

fn in_process(out: &Path, moves: &str, max_turns: u32) -> InProcess {
    let plan = Plan {
        mode: LiveMode::Run,
        title: "test".to_owned(),
        bots: vec!["bot".to_owned()],
        games: 1,
        settings: String::new(),
        tournament: None,
        start: None,
    };
    let live = Live::new(plan, out).unwrap();
    let server = Server::bind(None).unwrap();
    let (port, token) = (server.port(), server.token().to_owned());
    let server = server.spawn(live.clone()).unwrap();
    let session = Arc::clone(&live);
    let moves = moves.to_owned();
    let game = thread::spawn(move || {
        session.wait_for_start().unwrap();
        let entrant = Entrant {
            name: "bot".to_owned(),
            path: "fake".to_owned(),
            about: About::default(),
            cpu: None,
            command: fake_bot(&["--moves", &moves]),
            stderr_log: None,
        };
        let mut config = MatchConfig::new(Seating::Solo);
        config.startup = Duration::from_millis(500);
        config.max_turns = Some(max_turns);
        let maze = Maze::parse(RING).unwrap();
        let mut observer = session.observer();
        let record = run_match(maze, None, vec![entrant], &config, &mut observer).unwrap();
        session.game_done(&record, "replay.json");
        session.finished("replay.json");
        record
    });
    InProcess {
        live,
        port,
        token,
        game,
        _server: server,
    }
}

impl InProcess {
    fn api(&self, path: &str) -> String {
        format!("{path}?token={}", self.token)
    }

    fn control(&self, body: &str) -> (u16, Value) {
        let (status, body) = post(self.port, &self.api("/api/control"), body);
        (status, serde_json::from_slice(&body).unwrap())
    }
}

/// Turns streamed until `stop` says so, with their arrival times.
fn collect_turns(
    events: &EventStream,
    turns: &mut Vec<(TurnRecord, Instant)>,
    mut stop: impl FnMut(&str, &[(TurnRecord, Instant)]) -> bool,
) {
    loop {
        let (name, data) = events.next();
        if name == "turn" {
            let turn = serde_json::from_value(data["turn"].clone()).unwrap();
            turns.push((turn, Instant::now()));
        }
        if stop(&name, turns) {
            return;
        }
    }
}

#[test]
fn the_page_paces_the_game_and_late_joiners_catch_up() {
    let dir = tempfile::tempdir().unwrap();
    let session = in_process(dir.path(), LAP, 40);
    let events = EventStream::open(session.port, &session.token);
    assert_eq!(events.next().0, "snapshot");
    assert_eq!(events.next().0, "waiting");

    // Paused before Start: the bot starts, no turn is played.
    let (status, pace) = session.control(r#"{"paused": true}"#);
    assert_eq!(
        (status, &pace),
        (200, &serde_json::json!({ "paused": true, "delay_ms": 0 }))
    );
    assert_eq!(events.next(), ("pace".to_owned(), pace));
    assert_eq!(post(session.port, &session.api("/api/start"), "").0, 200);
    assert_eq!(events.next().0, "game_start");
    assert_eq!(events.next().0, "startup");
    assert_eq!(events.next_within(Duration::from_millis(400)), None);
    assert_eq!(session.live.phase(), Phase::Running);

    // Resumed with a delay between turns.
    let (_, pace) = session.control(r#"{"paused": false, "delay_ms": 60}"#);
    assert_eq!(pace, serde_json::json!({ "paused": false, "delay_ms": 60 }));
    assert_eq!(events.next().0, "pace");
    let mut turns = Vec::new();
    collect_turns(&events, &mut turns, |_, t| t.len() == 4);
    for pair in turns.windows(2) {
        let gap = pair[1].1 - pair[0].1;
        assert!(gap >= Duration::from_millis(50), "{gap:?}");
    }

    // Paused mid-game: whatever was in flight lands, then nothing.
    session.control(r#"{"paused": true}"#);
    collect_turns(&events, &mut turns, |name, _| name == "pace");
    while let Some((name, data)) = events.next_within(Duration::from_millis(300)) {
        assert_eq!(name, "turn");
        turns.push((
            serde_json::from_value(data["turn"].clone()).unwrap(),
            Instant::now(),
        ));
    }
    // A late joiner's snapshot holds every turn so far.
    let late = EventStream::open(session.port, &session.token);
    let (name, data) = late.next();
    assert_eq!(name, "snapshot");
    let caught_up = snapshot(&data);
    let seen: Vec<TurnRecord> = turns.iter().map(|(t, _)| t.clone()).collect();
    assert_eq!(caught_up.games[0].turns, seen);
    assert!(caught_up.pace.paused);

    // Full speed to the end: both clients see the same game.
    session.control(r#"{"paused": false, "delay_ms": 0}"#);
    collect_turns(&events, &mut turns, |name, _| name == "game_end");
    let mut late_turns = Vec::new();
    collect_turns(&late, &mut late_turns, |name, _| name == "game_end");
    let record = session.game.join().unwrap();
    let all: Vec<TurnRecord> = turns.into_iter().map(|(t, _)| t).collect();
    assert_eq!(all, record.turns);
    let rest: Vec<TurnRecord> = late_turns.into_iter().map(|(t, _)| t).collect();
    assert_eq!([seen, rest].concat(), record.turns);
}

#[test]
fn bad_controls_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let session = in_process(dir.path(), "stay", 1);
    for body in [
        "not json",
        r#"{"delay_ms": 60001}"#,
        r#"{"bogus": 1}"#,
        r#"{"paused": "yes"}"#,
    ] {
        let (status, reply) = session.control(body);
        assert_eq!(status, 400, "{body}");
        assert!(reply["error"].is_string());
    }
    assert_eq!(session.live.pace(), dedale::web::live::Pace::default());
    assert_eq!(get(session.port, &session.api("/api/control")).0, 405);
    assert_eq!(get(session.port, &session.api("/api/start")).0, 405);
    assert_eq!(post(session.port, &session.api("/api/events"), "").0, 405);
    assert_eq!(get(session.port, &session.api("/api/nope")).0, 404);
    assert_eq!(post(session.port, &session.api("/api/start"), "").0, 200);
    session.game.join().unwrap();
}
