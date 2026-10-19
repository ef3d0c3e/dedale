//! The browser app's test fixtures (`web/test/fixtures/`), made from the
//! golden replay: the TypeScript tests must read the same records and
//! rebuild the same states as the Rust code. `DEDALE_BLESS=1` rewrites
//! them; otherwise they are checked.

mod common;

use std::fs;

use dedale::referee::{GameStart, Observer, PlayerStart};
use dedale::tournament::{
    BotInfo, GameEntry, GamePlan, MazeSource, Mode, TOURNAMENT_FORMAT, TournamentReport, standings,
};
use dedale::web::http::{Body, Method, Request};
use dedale::web::live::{Live, LiveMode, Plan};
use dedale::web::server::Handler;
use dedale_core::game::{PlayerState, Seating};
use dedale_core::record::GameRecord;
use serde::Serialize;
use serde_json::{Value, json};

const GOLDEN: &str = include_str!("golden/replay.json");

fn check(name: &str, contents: &str) {
    let path = common::repo().join("web/test/fixtures").join(name);
    if std::env::var_os("DEDALE_BLESS").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        return;
    }
    let current = fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == contents,
        "web/test/fixtures/{name} is stale: run DEDALE_BLESS=1 cargo test --all-features --test web_fixtures"
    );
}

fn pretty(value: &impl Serialize) -> String {
    serde_json::to_string_pretty(value).unwrap() + "\n"
}

fn golden() -> GameRecord {
    GameRecord::from_json(GOLDEN).unwrap()
}

/// The state after every turn, as `GameRecord::state_at` rebuilds it.
#[derive(Serialize)]
struct State {
    turn: u32,
    cells: String,
    players: Vec<PlayerState>,
}

fn request(method: Method, path: &str) -> Request {
    Request {
        method,
        path: path.to_owned(),
        query: Vec::new(),
        headers: Vec::new(),
        body: Vec::new(),
    }
}

/// What a live session streams for the golden game, from a page that
/// connected before Start.
fn events(record: &GameRecord) -> Vec<Value> {
    let out = tempfile::tempdir().unwrap();
    let names = record.players.iter().map(|p| p.name.clone()).collect();
    let start = GameStart {
        code: record.code,
        maze: record.maze.clone(),
        config: record.config,
        players: record
            .players
            .iter()
            .map(|p| PlayerStart {
                name: p.name.clone(),
                path: p.path.clone(),
                start: p.start,
            })
            .collect(),
    };
    let plan = Plan {
        mode: LiveMode::Run,
        title: "golden".to_owned(),
        bots: names,
        games: 1,
        settings: "duo, 500 ms startup, 100 ms per move, 12 turns at most".to_owned(),
        tournament: None,
        start: Some(start.clone()),
    };
    let live = Live::new(plan, out.path()).unwrap();
    let Body::Events(stream) = live.api(&request(Method::Get, "/api/events")).body else {
        panic!("not a stream");
    };
    assert_eq!(live.api(&request(Method::Post, "/api/start")).status, 200);
    let mut observer = live.observer();
    observer.game_start(&start);
    for (player, p) in record.players.iter().enumerate() {
        observer.startup(player, &p.startup);
    }
    for (played, turn) in (1..).zip(&record.turns) {
        observer.turn(&record.state_at(played).unwrap(), turn);
    }
    live.game_done(record, "replay.json");
    live.finished("replay.json");
    stream
        .try_iter()
        .map(|e| json!({ "name": e.name, "data": serde_json::from_str::<Value>(&e.data).unwrap() }))
        .collect()
}

fn tournament(record: &GameRecord) -> TournamentReport {
    let names: Vec<String> = record.players.iter().map(|p| p.name.clone()).collect();
    let plans = [
        GamePlan {
            index: 1,
            code: record.code,
            seating: Seating::Duo,
            bots: vec![0, 1],
        },
        GamePlan {
            index: 2,
            code: record.code,
            seating: Seating::DuoSwapped,
            bots: vec![0, 1],
        },
    ];
    let games: Vec<(GamePlan, GameRecord)> = plans
        .iter()
        .map(|plan| (plan.clone(), record.clone()))
        .collect();
    TournamentReport {
        format: TOURNAMENT_FORMAT,
        dedale: record.dedale.clone(),
        master_seed: "0000000000000001".to_owned(),
        mode: Mode::Duo,
        maze: MazeSource::Generated {
            width: 5,
            height: 5,
            loops: 0,
            points: 17,
        },
        startup_ms: 500,
        move_ms: 100,
        max_turns: Some(12),
        bots: record
            .players
            .iter()
            .map(|p| BotInfo {
                name: p.name.clone(),
                path: p.path.clone(),
            })
            .collect(),
        games: games
            .iter()
            .map(|(plan, record)| GameEntry {
                plan: plan.clone(),
                result: record.result.clone(),
                replay: format!("games/{:03}/replay.json", plan.index),
            })
            .collect(),
        leaderboard: standings(Mode::Duo, &names, &games),
    }
}

#[test]
fn web_fixtures_are_current() {
    let record = golden();
    check("replay.json", GOLDEN);
    let states: Vec<State> = (0..=record.result.turns)
        .map(|turn| {
            let state = record.state_at(turn).unwrap();
            State {
                turn,
                cells: state.board.to_flat(),
                players: state.players,
            }
        })
        .collect();
    check("states.json", &pretty(&states));
    check("tournament.json", &pretty(&tournament(&record)));
    check("events.json", &pretty(&events(&record)));
}
