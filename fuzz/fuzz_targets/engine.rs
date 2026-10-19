//! Arbitrary mazes and input sequences through the game engine.

#![no_main]

use dedale_core::code::{MazeCode, MazeParams};
use dedale_core::game::{EndReason, Failure, Game, Input, Outcome, Seating};
use dedale_core::generator::generate;
use dedale_core::grid::{Cell, Move};
use libfuzzer_sys::fuzz_target;

fn input(byte: u8) -> Input {
    match byte % 16 {
        0 => Input::Failed(Failure::Invalid),
        1 => Input::Failed(Failure::Error),
        2 => Input::Failed(Failure::Timeout),
        3 => Input::Failed(Failure::NotReady),
        4 => Input::Failed(Failure::Crashed),
        n => Input::Move(Move::ALL[usize::from(n) % 5]),
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&[w, h, loops, density, seating, max_turns], rest)) = data.split_first_chunk::<6>()
    else {
        return;
    };
    let Some((seed, inputs)) = rest.split_first_chunk::<8>() else {
        return;
    };
    let side = |b: u8| 5 + 2 * u32::from(b % 8);
    let params = MazeParams::new(
        side(w),
        side(h),
        u32::from(loops % 101),
        u32::from(density % 101),
    )
    .expect("in range by construction");
    let maze = generate(&MazeCode::new(params, u64::from_le_bytes(*seed)));
    let initial = maze.point_count();
    let seating = [Seating::Solo, Seating::Duo, Seating::DuoSwapped][usize::from(seating % 3)];
    let players = seating.players();
    let mut game = Game::new(maze, seating, u32::from(max_turns));

    for chunk in inputs.chunks_exact(players) {
        if game.is_over() {
            break;
        }
        let inputs: Vec<Input> = chunk.iter().map(|&b| input(b)).collect();
        let before = game.snapshot().clone();
        let report = game.step(&inputs);
        let after = game.snapshot();
        for (i, (turn, input)) in report.players.iter().zip(&inputs).enumerate() {
            assert!(after.board.get(turn.pos).is_open());
            let from = before.players[i].pos;
            match *input {
                Input::Move(m) => {
                    let legal = before.board.step(from, m).is_some();
                    assert_eq!(turn.outcome == Outcome::Ok, legal);
                }
                Input::Failed(_) => assert_eq!(turn.pos, from),
            }
        }
        for &pos in &report.collected {
            assert_eq!(before.board.get(pos), Cell::Point);
        }
        let total: u32 = after.players.iter().map(|p| p.half_points).sum();
        let remaining = game.remaining_points();
        assert_eq!(usize::try_from(total).unwrap(), 2 * (initial - remaining));
        assert_eq!(remaining, after.board.point_count());
        assert_eq!(remaining == 0, report.end == Some(EndReason::Cleared));
    }
    if let Some(result) = game.result() {
        assert_eq!(result.turns, game.snapshot().turn);
    }
});
