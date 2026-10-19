//! Helpers for tests: records of simulated games, without any process.

use proptest::prelude::*;

use crate::code::{MazeCode, MazeParams};
use crate::game::{Failure, Game, Input, Seating};
use crate::generator::generate;
use crate::grid::Move;
use crate::record::{
    ConfigRecord, GameRecord, PlayerRecord, PlayerTurnRecord, REPLAY_FORMAT, StartupOutcome,
    StartupRecord, TurnRecord,
};
use crate::stats::compute_stats;

/// Plays `inputs` (then stays until the game ends) on the maze of `code` and
/// records it the way the runner would, with made-up timings.
pub(crate) fn simulate(
    code: MazeCode,
    seating: Seating,
    max_turns: u32,
    inputs: &[Vec<Input>],
) -> GameRecord {
    let maze = generate(&code);
    let mut game = Game::new(maze.clone(), seating, max_turns);
    let stay = vec![Input::Move(Move::Stay); seating.players()];
    let mut turns = Vec::new();
    for inputs in inputs.iter().chain(std::iter::repeat(&stay)) {
        if game.is_over() {
            break;
        }
        let report = game.step(inputs);
        let players = report
            .players
            .iter()
            .map(|p| PlayerTurnRecord {
                requested: p.requested,
                applied: p.applied,
                outcome: p.outcome,
                latency_us: Some(100 + u64::from(report.turn)),
                think_us: Some(50),
                pos: p.pos,
                gained: p.gained,
                half_points: p.half_points,
                log: Vec::new(),
                marks: Vec::new(),
                error: None,
                invalid: None,
            })
            .collect();
        turns.push(TurnRecord {
            turn: report.turn,
            players,
            collected: report.collected,
        });
    }
    let players = seating
        .starts(&maze)
        .into_iter()
        .enumerate()
        .map(|(i, start)| PlayerRecord {
            name: format!("bot{i}"),
            path: format!("bot{i}.py"),
            start,
            startup: StartupRecord {
                outcome: StartupOutcome::Ok,
                startup_us: Some(1000),
                init_us: Some(900),
                log: Vec::new(),
                marks: Vec::new(),
                error: None,
            },
            exit: None,
        })
        .collect();
    let mut record = GameRecord {
        format: REPLAY_FORMAT,
        dedale: "test".to_owned(),
        code: Some(code),
        maze,
        config: ConfigRecord {
            startup_ms: 2000,
            move_ms: 100,
            max_turns,
            seating,
        },
        players,
        turns,
        events: Vec::new(),
        result: game.result().expect("played to the end"),
        stats: Vec::new(),
    };
    record.stats = compute_stats(&record);
    record
}

fn input() -> impl Strategy<Value = Input> {
    prop_oneof![
        10 => prop::sample::select(Move::ALL.to_vec()).prop_map(Input::Move),
        1 => prop::sample::select(vec![
            Failure::Invalid,
            Failure::Error,
            Failure::Timeout,
            Failure::NotReady,
        ])
        .prop_map(Input::Failed),
    ]
}

/// Records of random games on small mazes.
pub(crate) fn record() -> impl Strategy<Value = GameRecord> {
    let seating = prop_oneof![
        Just(Seating::Solo),
        Just(Seating::Duo),
        Just(Seating::DuoSwapped)
    ];
    (
        2u32..=5,
        2u32..=5,
        0u32..=100,
        any::<u64>(),
        seating,
        1u32..=80,
    )
        .prop_flat_map(|(w, h, points, seed, seating, max_turns)| {
            let params = MazeParams::new(2 * w + 1, 2 * h + 1, 25, points).unwrap();
            let code = MazeCode::new(params, seed);
            let turn = prop::collection::vec(input(), seating.players());
            prop::collection::vec(turn, 0..80)
                .prop_map(move |inputs| simulate(code, seating, max_turns, &inputs))
        })
}
