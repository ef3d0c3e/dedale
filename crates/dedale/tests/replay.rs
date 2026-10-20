//! Replays of real games: written, read back, verified, and pinned.

mod common;

use std::fs;
use std::path::Path;
use std::time::Duration;

use common::fake_bot;
use dedale::referee::{Entrant, MatchConfig, run_match};
use dedale::replay::{self, ReadError};
use dedale_core::code::MazeCode;
use dedale_core::game::Seating;
use dedale_core::generator::generate_normalized;
use dedale_core::protocol::About;
use dedale_core::record::GameRecord;
use dedale_core::stats::compute_stats;
use dedale_core::verify::verify;

const LAP: &str = "right,right,right,right,down,down,down,down,left,left,left,left,up,up,up,up";

/// A duo game on a generated 5×5 maze between two fake bots, one of them
/// misbehaving now and then.
fn game() -> GameRecord {
    let code: MazeCode = "ded2-5x5-l0-p65535-0000000000000000".parse().unwrap();
    let (maze, code) = generate_normalized(&code);
    let entrants = vec![
        Entrant {
            name: "lap".to_owned(),
            path: "fake:lap".to_owned(),
            about: About {
                name: Some("lap".to_owned()),
                author: Some("Ada".to_owned()),
                description: Some("Laps the maze, clockwise.".to_owned()),
            },
            command: fake_bot(&["--moves", LAP, "--log", "on my way"]),
            stderr_log: None,
        },
        Entrant {
            name: "flaky".to_owned(),
            path: "fake:flaky".to_owned(),
            about: About::default(),
            command: fake_bot(&["--moves", "up,north", "--error-at", "3"]),
            stderr_log: None,
        },
    ];
    let mut config = MatchConfig::new(Seating::Duo);
    config.startup = Duration::from_millis(500);
    config.max_turns = Some(12);
    config.end_grace = Duration::from_millis(500);
    run_match(maze, Some(code), entrants, &config, &mut ()).unwrap()
}

/// Replaces what varies between runs (timings, version) with fixed values.
fn scrub(mut record: GameRecord) -> GameRecord {
    "x".clone_into(&mut record.dedale);
    for player in &mut record.players {
        player.startup.startup_us = player.startup.startup_us.map(|_| 0);
        if let Some(exit) = &mut player.exit {
            exit.stderr_tail.clear();
        }
    }
    for turn in &mut record.turns {
        for entry in &mut turn.players {
            entry.latency_us = entry.latency_us.map(|_| 0);
        }
    }
    record.stats = compute_stats(&record);
    record
}

#[test]
fn real_games_verify_and_roundtrip() {
    let record = game();
    assert_eq!(verify(&record), Ok(()));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("replay.json");
    replay::write(&path, &record).unwrap();
    assert_eq!(replay::read(&path).unwrap(), record);
}

#[test]
fn the_replay_format_is_pinned() {
    let record = scrub(game());
    let pretty = serde_json::to_string_pretty(&record).unwrap() + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/replay.json");
    if std::env::var_os("DEDALE_BLESS").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &pretty).unwrap();
    }
    let want = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (DEDALE_BLESS=1 writes it)", path.display()));
    assert_eq!(
        pretty, want,
        "the replay format changed; bump REPLAY_FORMAT if intended"
    );
}

#[test]
fn tampered_files_fail_verification() {
    let record = game();
    let mut moved = record.clone();
    moved.turns[2].players[0].pos.y += 1;
    assert!(
        verify(&moved)
            .unwrap_err()
            .to_string()
            .starts_with("turn 3: player 0")
    );
    let mut rescored = record;
    rescored.result.half_points[1] += 2;
    assert!(verify(&rescored).is_err());
}

#[test]
fn reading_reports_bad_files() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.json");
    assert!(matches!(replay::read(&missing), Err(ReadError::Io { .. })));
    let garbage = dir.path().join("garbage.json");
    fs::write(&garbage, "{").unwrap();
    assert!(matches!(
        replay::read(&garbage),
        Err(ReadError::Replay { .. })
    ));
}
