//! Whole games against scripted fake bots: every outcome must be produced
//! and recorded.

mod common;

use std::time::{Duration, Instant};

use common::fake_bot;
use dedale::referee::{Entrant, MatchConfig, MatchError, run_match};
use dedale_core::game::{EndReason, Outcome, Seating};
use dedale_core::grid::{Move, Pos};
use dedale_core::maze::Maze;
use dedale_core::record::{EventKind, GameRecord, StartupOutcome};

/// A corridor ring around a block of walls, 14 points.
const RING: &str = "\
_....
.###.
.###.
.###.
...._
";

/// One lap of the ring from (0, 0): clears it on turn 15.
const LAP: &str = "right,right,right,right,down,down,down,down,left,left,left,left,up,up,up,up";

fn ring() -> Maze {
    Maze::parse(RING).unwrap()
}

fn entrant(name: &str, args: &[&str]) -> Entrant {
    Entrant {
        name: name.to_owned(),
        path: format!("fake:{name}"),
        command: fake_bot(args),
        stderr_log: None,
    }
}

fn config(seating: Seating) -> MatchConfig {
    MatchConfig {
        startup: Duration::from_millis(500),
        move_timeout: Duration::from_millis(100),
        max_turns: Some(50),
        seating,
        hello_timeout: Duration::from_secs(10),
        end_grace: Duration::from_millis(200),
    }
}

fn solo(args: &[&str]) -> GameRecord {
    solo_with(args, &config(Seating::Solo))
}

fn solo_with(args: &[&str], config: &MatchConfig) -> GameRecord {
    run_match(ring(), None, vec![entrant("bot", args)], config, |_, _| {}).unwrap()
}

fn outcomes(record: &GameRecord, player: usize) -> Vec<Outcome> {
    record
        .turns
        .iter()
        .map(|t| t.players[player].outcome)
        .collect()
}

#[test]
fn a_solo_bot_clears_the_ring() {
    let record = solo(&["--moves", LAP, "--log", "thinking"]);
    assert_eq!(record.result.reason, EndReason::Cleared);
    assert_eq!(record.result.cleared_turn, Some(15));
    assert_eq!(record.result.half_points, vec![28]);
    assert_eq!(record.turns.len(), 15);
    assert!(outcomes(&record, 0).iter().all(|&o| o == Outcome::Ok));
    for turn in &record.turns {
        let p = &turn.players[0];
        assert!(p.latency_us.is_some_and(|us| us < 100_000));
        assert_eq!(p.think_us, Some(1));
        assert_eq!(p.log, vec!["thinking".to_owned()]);
    }
    let player = &record.players[0];
    assert_eq!(player.startup.outcome, StartupOutcome::Ok);
    assert!(player.startup.startup_us.is_some());
    assert_eq!(player.start, Pos::new(0, 0));
    let exit = player.exit.as_ref().unwrap();
    assert_eq!((exit.code, exit.killed), (Some(0), false));
    assert!(record.events.is_empty());
}

#[test]
fn duo_winner_and_swapped_seats() {
    let entrants = || {
        vec![
            entrant("runner", &["--moves", LAP]),
            entrant("sitter", &["--moves", "stay"]),
        ]
    };
    let record = run_match(ring(), None, entrants(), &config(Seating::Duo), |_, _| {}).unwrap();
    assert_eq!(record.result.reason, EndReason::Cleared);
    assert_eq!(record.result.winner, Some(0));

    let record = run_match(
        ring(),
        None,
        entrants(),
        &config(Seating::DuoSwapped),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(record.players[0].start, Pos::new(4, 4));
    assert_eq!(record.players[1].start, Pos::new(0, 0));
    assert_eq!(record.result.winner, Some(0));
}

#[test]
fn illegal_and_invalid_moves() {
    let record = solo(&[
        "--moves",
        "up,north",
        "--garbage-at",
        "3",
        "--oversized-at",
        "4",
    ]);
    let turns = &record.turns;
    assert_eq!(turns[0].players[0].outcome, Outcome::Illegal);
    assert_eq!(turns[0].players[0].requested, Some(Move::Up));
    assert_eq!(turns[0].players[0].applied, Move::Stay);
    for (i, needle) in [(1, "north"), (2, "bad message"), (3, "longer than")] {
        let p = &turns[i].players[0];
        assert_eq!(p.outcome, Outcome::Invalid, "turn {}", i + 1);
        assert!(
            p.invalid.as_ref().unwrap().contains(needle),
            "{:?}",
            p.invalid
        );
        assert!(p.latency_us.is_some());
    }
}

#[test]
fn errors_in_move() {
    let record = solo(&["--error-at", "2"]);
    let p = &record.turns[1].players[0];
    assert_eq!(p.outcome, Outcome::Error);
    assert_eq!(p.error.as_ref().unwrap().message, "scripted move error");
    assert_eq!(record.turns[2].players[0].outcome, Outcome::Ok);
}

#[test]
fn timeouts_and_late_replies() {
    let record = solo(&["--silent-at", "1", "--sleep-at", "2:250", "--stale-at", "5"]);
    let outcomes = outcomes(&record, 0);
    assert_eq!(outcomes[0], Outcome::Timeout);
    assert_eq!(outcomes[1], Outcome::Timeout);
    assert_eq!(outcomes[4], Outcome::Timeout);
    // Turn 2's answer arrives during turn 3 or 4; turn 5's carries id 4.
    let late: Vec<u32> = record
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Late { for_turn } => Some(for_turn),
            EventKind::Ignored { .. } => None,
        })
        .collect();
    assert!(late.contains(&2), "{:?}", record.events);
    assert!(late.contains(&4), "{:?}", record.events);
    assert!(record.turns[0].players[0].latency_us.is_none());
}

#[test]
fn a_slow_startup_plays_not_ready_turns() {
    let record = solo(&["--init-sleep-ms", "800", "--moves", "right"]);
    let player = &record.players[0];
    assert_eq!(player.startup.outcome, StartupOutcome::Late);
    assert!(player.startup.startup_us.unwrap() >= 800_000);
    // Turns start at 500 ms, 100 ms each: at least two pass before the
    // bot is ready at 800 ms, and none after.
    let outcomes = outcomes(&record, 0);
    let ready_at = outcomes
        .iter()
        .position(|&o| o != Outcome::NotReady)
        .unwrap();
    assert!(ready_at >= 2, "{outcomes:?}");
    assert!(
        !outcomes[ready_at..].contains(&Outcome::NotReady),
        "{outcomes:?}"
    );
    assert!(outcomes.contains(&Outcome::Ok));
    // The fake bot answers every queued turn: those answers are late.
    assert!(
        record
            .events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Late { .. }))
    );
}

#[test]
fn an_init_error_still_plays() {
    let record = solo(&["--init-error", "--moves", LAP]);
    let startup = &record.players[0].startup;
    assert_eq!(startup.outcome, StartupOutcome::InitError);
    assert_eq!(
        startup.error.as_ref().unwrap().message,
        "scripted init error"
    );
    assert_eq!(record.result.reason, EndReason::Cleared);
}

#[test]
fn a_crash_abandons_a_solo_game() {
    let record = solo(&["--crash-at", "3"]);
    assert_eq!(
        outcomes(&record, 0),
        vec![Outcome::Ok, Outcome::Ok, Outcome::Crashed]
    );
    assert_eq!(record.result.reason, EndReason::Abandoned);
    let exit = record.players[0].exit.as_ref().unwrap();
    assert_eq!(exit.code, Some(3));
}

#[test]
fn a_crash_leaves_the_opponent_playing() {
    let entrants = vec![
        entrant("crasher", &["--crash-at", "2"]),
        entrant(
            "runner",
            &[
                "--moves",
                "up,up,up,up,left,left,left,left,down,down,down,down,right,right,right,right",
            ],
        ),
    ];
    let record = run_match(ring(), None, entrants, &config(Seating::Duo), |_, _| {}).unwrap();
    assert_eq!(record.result.reason, EndReason::Cleared);
    assert_eq!(record.result.winner, Some(1));
    assert!(
        outcomes(&record, 0)[1..]
            .iter()
            .all(|&o| o == Outcome::Crashed)
    );
}

#[test]
fn a_failed_handshake_is_a_crash() {
    let mut config = config(Seating::Solo);
    config.hello_timeout = Duration::from_millis(200);
    for args in [&["--no-hello"][..], &["--protocol", "2"][..]] {
        let record = solo_with(args, &config);
        let startup = &record.players[0].startup;
        assert_eq!(startup.outcome, StartupOutcome::Crashed);
        assert!(startup.error.is_some());
        assert_eq!(outcomes(&record, 0), vec![Outcome::Crashed]);
        assert_eq!(record.result.reason, EndReason::Abandoned);
    }
}

#[test]
fn the_turn_limit_ends_the_game() {
    let mut config = config(Seating::Solo);
    config.max_turns = Some(3);
    let record = solo_with(&[], &config);
    assert_eq!(record.result.reason, EndReason::TurnLimit);
    assert_eq!(record.turns.len(), 3);
    assert_eq!(record.config.max_turns, 3);
}

#[test]
fn a_slow_bot_never_delays_the_other_beyond_the_timeout() {
    let entrants = vec![
        entrant("fast", &[]),
        entrant("slow", &["--sleep-at", "1:1000"]),
    ];
    let mut config = config(Seating::Duo);
    config.max_turns = Some(3);
    let mut seen = Vec::new();
    let record = run_match(ring(), None, entrants, &config, |_, _| {
        seen.push(Instant::now());
    })
    .unwrap();
    assert_eq!(outcomes(&record, 1)[0], Outcome::Timeout);
    for pair in seen.windows(2) {
        let gap = pair[1] - pair[0];
        assert!(gap < Duration::from_millis(300), "{gap:?}");
    }
}

#[test]
fn records_the_maze_and_config() {
    let record = solo(&["--moves", LAP]);
    assert_eq!(record.maze, ring());
    assert_eq!(record.code, None);
    assert_eq!(record.config.startup_ms, 500);
    assert_eq!(record.config.move_ms, 100);
    assert_eq!(record.config.seating, Seating::Solo);
    assert_eq!(record.players[0].name, "bot");
}

#[test]
fn wrong_player_counts_and_missing_programs_are_errors() {
    let err = run_match(ring(), None, vec![], &config(Seating::Solo), |_, _| {}).unwrap_err();
    assert!(matches!(err, MatchError::WrongPlayerCount { .. }));
    let missing = Entrant {
        name: "ghost".to_owned(),
        path: "/nonexistent".to_owned(),
        command: std::process::Command::new("/nonexistent/bot"),
        stderr_log: None,
    };
    let err = run_match(
        ring(),
        None,
        vec![missing],
        &config(Seating::Solo),
        |_, _| {},
    )
    .unwrap_err();
    assert!(matches!(err, MatchError::Spawn { .. }));
}
