//! A stop request while the game is held. In its own test binary: the
//! request is process-wide and would stop every other test's game.

mod common;

use std::time::{Duration, Instant};

use common::fake_bot;
use dedale::interrupt;
use dedale::referee::{Entrant, MatchConfig, MatchError, Observer, run_match};
use dedale_core::game::Seating;
use dedale_core::maze::Maze;

/// Pauses forever before turn 2, and asks to stop once there.
#[derive(Default)]
struct Pause {
    asked: Vec<u32>,
    requested: Option<Instant>,
}

impl Observer for Pause {
    fn hold(&mut self, turn: u32) -> Option<Duration> {
        self.asked.push(turn);
        if turn < 2 {
            return None;
        }
        interrupt::request();
        self.requested.get_or_insert_with(Instant::now);
        Some(Duration::from_secs(60))
    }
}

#[test]
fn a_stop_request_ends_a_held_game() {
    let entrant = Entrant {
        name: "bot".to_owned(),
        path: "fake:bot".to_owned(),
        command: fake_bot(&[]),
        stderr_log: None,
    };
    let mut config = MatchConfig::new(Seating::Solo);
    config.startup = Duration::from_millis(500);
    let maze = Maze::parse("_....\n.###.\n.###.\n.###.\n...._\n").unwrap();
    let mut pause = Pause::default();
    let err = run_match(maze, None, vec![entrant], &config, &mut pause).unwrap_err();
    assert!(matches!(err, MatchError::Interrupted), "{err}");
    assert_eq!(pause.asked, [1, 2]);
    // Within one poll (100 ms) of the request, not after the 60 s hold.
    let stopped = pause.requested.unwrap().elapsed();
    assert!(stopped < Duration::from_secs(1), "{stopped:?}");
}
