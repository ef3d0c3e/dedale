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
use dedale_core::protocol::{MarkColor, MarkKind};
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
    dedale.path([dedale.me.pos, (0, 0)], label="home", color="teal")
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
    run_match(maze, None, vec![entrant], &ring_config(), &mut ()).unwrap()
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
    let record = run_match(maze, None, vec![entrant], &config, &mut ()).unwrap();

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
        assert_eq!(
            (p.marks[0].kind, p.marks[1].kind, p.marks[1].color),
            (MarkKind::Cells, MarkKind::Path, Some(MarkColor::Teal))
        );
    }
    let exit = record.players[0].exit.as_ref().unwrap();
    assert_eq!((exit.code, exit.killed), (Some(0), false), "{exit:?}");
}

#[test]
fn greedy_clears_a_full_size_maze() {
    let runtime = PythonRuntime::extract().unwrap();
    let code: MazeCode = "ded2-31x31-l25-p65535-9f3a0c11d2e4b6a8".parse().unwrap();
    let entrant = python_entrant(&runtime, &example_bot("greedy_bot"));
    let config = MatchConfig::new(Seating::Solo);
    let record = run_match(generate(&code), Some(code), vec![entrant], &config, &mut ()).unwrap();
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
            .all(|t| t.players[0].marks[0].label.as_deref() == Some("path")
                && t.players[0].marks[0].kind == MarkKind::Path)
    );
}

#[test]
fn greedy_beats_random() {
    let runtime = PythonRuntime::extract().unwrap();
    let code: MazeCode = "ded2-21x21-l25-p65535-0123456789abcdef".parse().unwrap();
    let entrants = vec![
        python_entrant(&runtime, &example_bot("greedy_bot")),
        python_entrant(&runtime, &example_bot("random_bot")),
    ];
    let config = MatchConfig::new(Seating::Duo);
    let record = run_match(generate(&code), Some(code), entrants, &config, &mut ()).unwrap();
    assert_eq!(record.result.reason, EndReason::Cleared);
    assert_eq!(record.result.winner, Some(0), "{:?}", record.result);
}

/// A game of example bots with room to think, so a loaded machine cannot
/// turn a move into a timeout and change the outcome. Every bot must start
/// in time and answer every turn.
fn example_game(bots: &[&str], code: &str, seating: Seating) -> GameRecord {
    let runtime = PythonRuntime::extract().unwrap();
    let code: MazeCode = code.parse().unwrap();
    let entrants = bots
        .iter()
        .map(|bot| python_entrant(&runtime, &example_bot(bot)))
        .collect();
    let mut config = MatchConfig::new(seating);
    config.startup = Duration::from_secs(10);
    config.move_timeout = Duration::from_secs(1);
    let record = run_match(generate(&code), Some(code), entrants, &config, &mut ()).unwrap();
    for (player, bot) in bots.iter().enumerate() {
        let startup = &record.players[player].startup;
        assert_eq!(startup.outcome, StartupOutcome::Ok, "{bot}: {startup:?}");
        let problems: Vec<_> = record
            .turns
            .iter()
            .filter(|t| t.players[player].outcome != Outcome::Ok)
            .map(|t| (t.turn, t.players[player].outcome, &t.players[player].error))
            .collect();
        assert!(problems.is_empty(), "{bot}: {problems:?}");
    }
    record
}

fn cleared_on(record: &GameRecord) -> u32 {
    assert_eq!(
        record.result.reason,
        EndReason::Cleared,
        "{:?}",
        record.result
    );
    record.result.turns
}

#[test]
fn the_tour_bot_clears_mazes_faster_than_greedy() {
    for code in [
        "ded2-31x31-l25-p65535-9f3a0c11d2e4b6a8",
        "ded2-31x31-l25-p100-0000000000000002",
    ] {
        let greedy = cleared_on(&example_game(&["greedy_bot"], code, Seating::Solo));
        let record = example_game(&["tour_bot"], code, Seating::Solo);
        let tour = cleared_on(&record);
        assert!(
            tour * 100 < greedy * 95,
            "{code}: tour {tour}, greedy {greedy}"
        );
        let log = &record.players[0].startup.log;
        assert!(log[0].starts_with("route: "), "{log:?}");
    }
}

#[test]
fn the_rollout_bot_beats_greedy_from_either_corner() {
    let code = "ded2-21x21-l25-p65535-0123456789abcdef";
    for seating in [Seating::Duo, Seating::DuoSwapped] {
        let record = example_game(&["rollout_bot", "greedy_bot"], code, seating);
        assert_eq!(record.result.reason, EndReason::Cleared);
        assert_eq!(
            record.result.winner,
            Some(0),
            "{seating:?}: {:?}",
            record.result
        );
    }
}

#[test]
fn two_rollout_bots_do_not_stall() {
    // Each expects the other to play greedily: without its guard, both
    // stepped away from the same points until the turn limit.
    let code = "ded2-21x21-l25-p40-0000000000000005";
    let record = example_game(&["rollout_bot", "rollout_bot"], code, Seating::Duo);
    assert_eq!(
        record.result.reason,
        EndReason::Cleared,
        "{:?}",
        record.result
    );
}

#[test]
fn the_combo_bot_plans_alone_and_looks_ahead_in_a_duo() {
    let code = "ded2-21x21-l25-p65535-0123456789abcdef";
    let solo = example_game(&["combo_bot"], code, Seating::Solo);
    assert_eq!(solo.players[0].startup.log[0], "playing tour_bot");
    let tour = example_game(&["tour_bot"], code, Seating::Solo);
    assert_eq!(cleared_on(&solo), cleared_on(&tour));

    let duo = example_game(&["combo_bot", "greedy_bot"], code, Seating::Duo);
    assert_eq!(duo.players[0].startup.log, ["playing rollout_bot"]);
    assert_eq!(duo.result.winner, Some(0), "{:?}", duo.result);
}

#[test]
fn the_score_bot_clears_mazes_faster_than_greedy() {
    for code in [
        "ded2-31x31-l25-p65535-9f3a0c11d2e4b6a8",
        "ded2-31x31-l25-p100-0000000000000002",
    ] {
        let greedy = cleared_on(&example_game(&["greedy_bot"], code, Seating::Solo));
        let score = cleared_on(&example_game(&["score_bot"], code, Seating::Solo));
        assert!(
            score * 100 < greedy * 95,
            "{code}: score {score}, greedy {greedy}"
        );
    }
}

#[test]
fn the_score_bot_beats_greedy_from_either_corner() {
    let code = "ded2-21x21-l25-p65535-0123456789abcdef";
    for seating in [Seating::Duo, Seating::DuoSwapped] {
        let record = example_game(&["score_bot", "greedy_bot"], code, seating);
        assert_eq!(record.result.reason, EndReason::Cleared);
        assert_eq!(
            record.result.winner,
            Some(0),
            "{seating:?}: {:?}",
            record.result
        );
        // It draws its way to the best point, and the next ones.
        let marks = &record.turns[0].players[0].marks;
        assert_eq!(marks[0].kind, MarkKind::Path);
        assert_eq!(marks[1].color, Some(MarkColor::Yellow));
    }
}

#[test]
fn two_score_bots_clear_the_maze() {
    let code = "ded2-21x21-l25-p40-0000000000000005";
    let record = example_game(&["score_bot", "score_bot"], code, Seating::Duo);
    assert_eq!(
        record.result.reason,
        EndReason::Cleared,
        "{:?}",
        record.result
    );
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
