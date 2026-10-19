//! Python bots through the embedded harness and the referee: the examples
//! play real games, the misbehaving bots produce the expected outcomes.

mod common;

use std::fs;
use std::time::Duration;

use common::{example_bot, python_entrant, test_bot};
use dedale::python::PythonRuntime;
use dedale::referee::{MatchConfig, run_match};
use dedale_core::code::MazeCode;
use dedale_core::game::{EndReason, Outcome, Seating};
use dedale_core::generator::generate;
use dedale_core::maze::Maze;
use dedale_core::record::{EventKind, GameRecord, StartupOutcome};

const RING: &str = "_....\n.###.\n.###.\n.###.\n...._\n";

const LAP_BOT: &str = r#"
import dedale
from dedale import Move

LAP = [Move.RIGHT] * 4 + [Move.DOWN] * 4 + [Move.LEFT] * 4 + [Move.UP] * 4

def init():
    print("points:", len(dedale.points))

def move():
    dedale.mark([dedale.me.pos], label="me")
    return LAP[(dedale.turn - 1) % len(LAP)]
"#;

/// Quick games on the ring: 8 turns, short timeouts.
fn ring_config() -> MatchConfig {
    let mut config = MatchConfig::new(Seating::Solo);
    config.startup = Duration::from_millis(500);
    config.max_turns = Some(8);
    config.end_grace = Duration::from_millis(500);
    config
}

fn play_test_bot(name: &str) -> GameRecord {
    let runtime = PythonRuntime::extract().unwrap();
    let entrant = python_entrant(&runtime, &test_bot(name));
    let maze = Maze::parse(RING).unwrap();
    run_match(maze, None, vec![entrant], &ring_config(), |_, _| {}).unwrap()
}

fn outcomes(record: &GameRecord) -> Vec<Outcome> {
    record.turns.iter().map(|t| t.players[0].outcome).collect()
}

#[test]
fn a_python_bot_clears_the_ring() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lap.py");
    fs::write(&path, LAP_BOT).unwrap();
    let runtime = PythonRuntime::extract().unwrap();
    let mut entrant = python_entrant(&runtime, &path);
    entrant.stderr_log = Some(dir.path().join("lap.stderr.log"));
    let mut config = MatchConfig::new(Seating::Solo);
    config.max_turns = Some(50);

    let maze = Maze::parse(RING).unwrap();
    let record = run_match(maze, None, vec![entrant], &config, |_, _| {}).unwrap();

    let startup = &record.players[0].startup;
    assert_eq!(startup.outcome, StartupOutcome::Ok, "{startup:?}");
    assert!(startup.init_us.is_some());
    assert_eq!(startup.log, vec!["points: 14".to_owned()]);
    assert_eq!(record.result.reason, EndReason::Cleared);
    assert_eq!(record.result.cleared_turn, Some(15));
    for turn in &record.turns {
        let p = &turn.players[0];
        assert_eq!(p.outcome, Outcome::Ok);
        assert!(p.think_us.is_some());
        assert_eq!(p.marks[0].label.as_deref(), Some("me"));
    }
    let exit = record.players[0].exit.as_ref().unwrap();
    assert_eq!((exit.code, exit.killed), (Some(0), false), "{exit:?}");
}

#[test]
fn greedy_clears_a_full_size_maze() {
    let runtime = PythonRuntime::extract().unwrap();
    let code: MazeCode = "ded1-31x31-l25-p100-9f3a0c11d2e4b6a8".parse().unwrap();
    let entrant = python_entrant(&runtime, &example_bot("greedy_bot"));
    let config = MatchConfig::new(Seating::Solo);
    let record = run_match(
        generate(&code),
        Some(code),
        vec![entrant],
        &config,
        |_, _| {},
    )
    .unwrap();
    assert_eq!(
        record.result.reason,
        EndReason::Cleared,
        "{:?}",
        record.result
    );
    assert_eq!(record.players[0].startup.outcome, StartupOutcome::Ok);
    assert!(outcomes(&record).iter().all(|&o| o == Outcome::Ok));
    assert!(
        record
            .turns
            .iter()
            .all(|t| t.players[0].marks[0].label.as_deref() == Some("path"))
    );
}

#[test]
fn greedy_beats_random() {
    let runtime = PythonRuntime::extract().unwrap();
    let code: MazeCode = "ded1-21x21-l25-p100-0123456789abcdef".parse().unwrap();
    let entrants = vec![
        python_entrant(&runtime, &example_bot("greedy_bot")),
        python_entrant(&runtime, &example_bot("random_bot")),
    ];
    let config = MatchConfig::new(Seating::Duo);
    let record = run_match(generate(&code), Some(code), entrants, &config, |_, _| {}).unwrap();
    assert_eq!(record.result.reason, EndReason::Cleared);
    assert_eq!(record.result.winner, Some(0), "{:?}", record.result);
}

#[test]
fn a_slow_init_starts_late() {
    let record = play_test_bot("slow_init");
    assert_eq!(record.players[0].startup.outcome, StartupOutcome::Late);
    let outcomes = outcomes(&record);
    assert_eq!(outcomes[0], Outcome::NotReady);
    assert!(outcomes.contains(&Outcome::Ok));
}

#[test]
fn a_slow_move_times_out_and_answers_late() {
    let record = play_test_bot("slow_move");
    let outcomes = outcomes(&record);
    assert_eq!(&outcomes[..3], [Outcome::Ok, Outcome::Ok, Outcome::Timeout]);
    assert_eq!(outcomes.last(), Some(&Outcome::Ok));
    let late_for_3 = record
        .events
        .iter()
        .any(|e| e.kind == EventKind::Late { for_turn: 3 });
    assert!(late_for_3, "{:?}", record.events);
}

#[test]
fn an_init_error_still_plays() {
    let record = play_test_bot("raise_init");
    let startup = &record.players[0].startup;
    assert_eq!(startup.outcome, StartupOutcome::InitError);
    let message = &startup.error.as_ref().unwrap().message;
    assert_eq!(message, "RuntimeError: init failed on purpose");
    assert!(outcomes(&record).iter().all(|&o| o == Outcome::Ok));
}

#[test]
fn an_exception_in_move_is_an_error() {
    let record = play_test_bot("raise_move");
    let outcomes = outcomes(&record);
    assert_eq!(outcomes[1], Outcome::Error);
    let error = record.turns[1].players[0].error.as_ref().unwrap();
    assert_eq!(error.message, "ValueError: move failed on purpose");
    assert!(error.traceback.as_ref().unwrap().contains("raise_move.py"));
    assert_eq!(outcomes[2], Outcome::Ok);
}

#[test]
fn bad_return_values_are_invalid() {
    let record = play_test_bot("bad_returns");
    let outcomes = outcomes(&record);
    assert_eq!(
        &outcomes[..5],
        [
            Outcome::Ok,
            Outcome::Invalid,
            Outcome::Invalid,
            Outcome::Invalid,
            Outcome::Ok
        ]
    );
    let reason = |turn: usize| record.turns[turn].players[0].invalid.clone().unwrap();
    assert_eq!(reason(1), "no move");
    assert!(
        record.turns[1].players[0]
            .log
            .last()
            .unwrap()
            .contains("None")
    );
    assert!(
        record.turns[2].players[0]
            .log
            .last()
            .unwrap()
            .contains("42")
    );
    assert!(reason(3).contains("north"));
}

#[test]
fn print_spam_is_capped() {
    let record = play_test_bot("print_spam");
    assert!(outcomes(&record).iter().all(|&o| o == Outcome::Ok));
    for turn in &record.turns {
        let log = &turn.players[0].log;
        assert_eq!(log.last().map(String::as_str), Some("[truncated]"));
        assert!(log.iter().map(String::len).sum::<usize>() <= 4096);
    }
}

#[test]
fn an_infinite_loop_times_out_and_is_killed() {
    let record = play_test_bot("infinite_loop");
    let outcomes = outcomes(&record);
    assert_eq!(outcomes[0], Outcome::Ok);
    assert!(
        outcomes[1..].iter().all(|&o| o == Outcome::Timeout),
        "{outcomes:?}"
    );
    assert!(record.players[0].exit.as_ref().unwrap().killed);
}

#[test]
fn sys_exit_is_a_crash() {
    let record = play_test_bot("sys_exit");
    assert_eq!(
        outcomes(&record),
        [Outcome::Ok, Outcome::Ok, Outcome::Crashed]
    );
    assert_eq!(record.result.reason, EndReason::Abandoned);
    assert_eq!(record.players[0].exit.as_ref().unwrap().code, Some(7));
}

/// The README shows the greedy example verbatim, so it cannot go stale
/// without this test failing (and the example itself is tested above).
#[test]
fn the_readme_shows_the_tested_example() {
    let readme = std::fs::read_to_string(common::repo().join("README.md")).unwrap();
    let greedy = std::fs::read_to_string(example_bot("greedy_bot")).unwrap();
    assert!(readme.contains(&format!("```python\n{greedy}```")));
}
