//! The rules of a game.
//!
//! Pure and synchronous: the runner turns bot replies into [`Input`]s, the
//! engine resolves the turn. Replay verification and fuzzing drive the same
//! code.

use serde::{Deserialize, Serialize};

use crate::grid::{Cell, Grid, Move, Pos};
use crate::maze::Maze;

/// How a player's turn went. Everything except `Ok` resolves as a stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    /// The move led off the grid or into a wall.
    Illegal,
    /// The reply was unreadable, unknown or not a move.
    Invalid,
    /// The bot's `move()` raised.
    Error,
    Timeout,
    /// The bot was still in its startup phase.
    NotReady,
    /// The bot's process is gone.
    Crashed,
}

/// Why a player has no move this turn, as decided by the runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Failure {
    Invalid,
    Error,
    Timeout,
    NotReady,
    Crashed,
}

impl From<Failure> for Outcome {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Invalid => Outcome::Invalid,
            Failure::Error => Outcome::Error,
            Failure::Timeout => Outcome::Timeout,
            Failure::NotReady => Outcome::NotReady,
            Failure::Crashed => Outcome::Crashed,
        }
    }
}

/// One player's input to a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Input {
    Move(Move),
    Failed(Failure),
}

/// Who plays from which corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Seating {
    /// One player, from `(0, 0)`.
    Solo,
    /// Player 0 from `(0, 0)`, player 1 from `(W-1, H-1)`.
    Duo,
    /// Corners swapped: player 0 from `(W-1, H-1)`.
    DuoSwapped,
}

impl Seating {
    #[must_use]
    pub fn players(self) -> usize {
        match self {
            Seating::Solo => 1,
            Seating::Duo | Seating::DuoSwapped => 2,
        }
    }

    #[must_use]
    pub fn starts(self, maze: &Maze) -> Vec<Pos> {
        let [a, b] = maze.starts();
        match self {
            Seating::Solo => vec![a],
            Seating::Duo => vec![a, b],
            Seating::DuoSwapped => vec![b, a],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// No points remain.
    Cleared,
    /// Every bot crashed.
    Abandoned,
    /// `max_turns` was reached.
    TurnLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerState {
    pub pos: Pos,
    /// Score in half-points: a point is worth 2, a contested point 1 each.
    pub half_points: u32,
    /// The move applied last turn (`stay` when rejected), `None` before the
    /// first turn.
    pub last_move: Option<Move>,
}

/// The state after a turn: what bots see and the renderer draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Turns played so far.
    pub turn: u32,
    /// The maze with collected points removed.
    pub board: Grid,
    pub players: Vec<PlayerState>,
}

/// One player's part of a [`TurnReport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerTurn {
    pub requested: Option<Move>,
    pub applied: Move,
    pub outcome: Outcome,
    pub pos: Pos,
    pub gained: u32,
    pub half_points: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnReport {
    pub turn: u32,
    pub players: Vec<PlayerTurn>,
    /// Cells whose point was taken this turn, row-major.
    pub collected: Vec<Pos>,
    pub end: Option<EndReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameResult {
    pub reason: EndReason,
    /// Turns played.
    pub turns: u32,
    pub half_points: Vec<u32>,
    /// Duo only: the player with more points; `None` on a draw and in solo.
    pub winner: Option<usize>,
    /// The turn the last point was collected, when the maze was cleared.
    pub cleared_turn: Option<u32>,
}

/// The default turn limit: `4·W·H`.
#[must_use]
pub fn default_max_turns(maze: &Maze) -> u32 {
    4 * u32::from(maze.width()) * u32::from(maze.height())
}

#[derive(Debug, Clone)]
pub struct Game {
    maze: Maze,
    max_turns: u32,
    state: Snapshot,
    remaining: usize,
    end: Option<EndReason>,
}

impl Game {
    /// A game at turn 0. It is already over when the maze has no points
    /// (`Cleared`) or `max_turns` is 0 (`TurnLimit`).
    #[must_use]
    pub fn new(maze: Maze, seating: Seating, max_turns: u32) -> Self {
        let players = seating
            .starts(&maze)
            .into_iter()
            .map(|pos| PlayerState {
                pos,
                half_points: 0,
                last_move: None,
            })
            .collect();
        let remaining = maze.point_count();
        let end = if remaining == 0 {
            Some(EndReason::Cleared)
        } else if max_turns == 0 {
            Some(EndReason::TurnLimit)
        } else {
            None
        };
        let state = Snapshot {
            turn: 0,
            board: maze.grid().clone(),
            players,
        };
        Self {
            maze,
            max_turns,
            state,
            remaining,
            end,
        }
    }

    /// The maze as it was at turn 0.
    #[must_use]
    pub fn maze(&self) -> &Maze {
        &self.maze
    }

    #[must_use]
    pub fn max_turns(&self) -> u32 {
        self.max_turns
    }

    #[must_use]
    pub fn snapshot(&self) -> &Snapshot {
        &self.state
    }

    #[must_use]
    pub fn remaining_points(&self) -> usize {
        self.remaining
    }

    #[must_use]
    pub fn is_over(&self) -> bool {
        self.end.is_some()
    }

    /// Resolves one turn: illegal moves become stays, every character moves
    /// at once, and points are collected (split when contested).
    ///
    /// # Panics
    ///
    /// If the game is over or `inputs` does not hold one input per player.
    pub fn step(&mut self, inputs: &[Input]) -> TurnReport {
        assert!(!self.is_over(), "the game is over");
        assert_eq!(
            inputs.len(),
            self.state.players.len(),
            "one input per player"
        );
        self.state.turn += 1;

        let mut players: Vec<PlayerTurn> = self
            .state
            .players
            .iter()
            .zip(inputs)
            .map(|(player, &input)| self.resolve(player.pos, input))
            .collect();

        let mut collected = Vec::new();
        for i in 0..players.len() {
            let pos = players[i].pos;
            if self.state.board.get(pos) != Cell::Point {
                continue;
            }
            let here: Vec<usize> = (0..players.len())
                .filter(|&j| players[j].pos == pos)
                .collect();
            let share = if here.len() == 1 { 2 } else { 1 };
            for j in here {
                players[j].gained += share;
            }
            self.state.board.set(pos, Cell::Empty);
            self.remaining -= 1;
            collected.push(pos);
        }
        collected.sort_by_key(|&pos| self.state.board.index(pos));

        for (state, turn) in self.state.players.iter_mut().zip(&mut players) {
            state.pos = turn.pos;
            state.half_points += turn.gained;
            state.last_move = Some(turn.applied);
            turn.half_points = state.half_points;
        }

        self.end = if self.remaining == 0 {
            Some(EndReason::Cleared)
        } else if inputs
            .iter()
            .all(|&input| input == Input::Failed(Failure::Crashed))
        {
            Some(EndReason::Abandoned)
        } else if self.state.turn >= self.max_turns {
            Some(EndReason::TurnLimit)
        } else {
            None
        };

        TurnReport {
            turn: self.state.turn,
            players,
            collected,
            end: self.end,
        }
    }

    fn resolve(&self, from: Pos, input: Input) -> PlayerTurn {
        let (requested, outcome, applied) = match input {
            Input::Failed(failure) => (None, failure.into(), Move::Stay),
            Input::Move(mv) => match self.state.board.step(from, mv) {
                Some(_) => (Some(mv), Outcome::Ok, mv),
                None => (Some(mv), Outcome::Illegal, Move::Stay),
            },
        };
        let pos = self
            .state
            .board
            .step(from, applied)
            .expect("applied moves are legal");
        PlayerTurn {
            requested,
            applied,
            outcome,
            pos,
            gained: 0,
            half_points: 0,
        }
    }

    /// The final result, once the game is over.
    #[must_use]
    pub fn result(&self) -> Option<GameResult> {
        let reason = self.end?;
        let half_points: Vec<u32> = self.state.players.iter().map(|p| p.half_points).collect();
        let winner = match half_points.as_slice() {
            [a, b] if a > b => Some(0),
            [a, b] if b > a => Some(1),
            _ => None,
        };
        Some(GameResult {
            reason,
            turns: self.state.turn,
            half_points,
            winner,
            cleared_turn: (reason == EndReason::Cleared).then_some(self.state.turn),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{EndReason, Failure, Game, Input, Outcome, Seating, default_max_turns};
    use crate::code::{MazeCode, MazeParams};
    use crate::generator::generate;
    use crate::grid::{Cell, Move, Pos};
    use crate::maze::Maze;
    use proptest::prelude::*;

    /// A corridor ring around a block of walls, points everywhere.
    const RING: &str = "\
_....
.###.
.###.
.###.
...._
";

    fn mv(m: Move) -> Input {
        Input::Move(m)
    }

    fn ring() -> Maze {
        Maze::parse(RING).unwrap()
    }

    #[test]
    fn illegal_moves_stay() {
        let mut game = Game::new(ring(), Seating::Solo, 100);
        for (m, outcome) in [(Move::Up, Outcome::Illegal), (Move::Left, Outcome::Illegal)] {
            let report = game.step(&[mv(m)]);
            let p = report.players[0];
            assert_eq!(
                (p.requested, p.applied, p.outcome),
                (Some(m), Move::Stay, outcome)
            );
            assert_eq!(p.pos, Pos::new(0, 0));
        }
        game.step(&[mv(Move::Right)]);
        let report = game.step(&[mv(Move::Down)]);
        assert_eq!(report.players[0].outcome, Outcome::Illegal);
        assert_eq!(report.players[0].pos, Pos::new(1, 0));
    }

    #[test]
    fn failures_stay() {
        let mut game = Game::new(ring(), Seating::Solo, 100);
        for failure in [
            Failure::Invalid,
            Failure::Error,
            Failure::Timeout,
            Failure::NotReady,
        ] {
            let p = game.step(&[Input::Failed(failure)]).players[0];
            assert_eq!((p.requested, p.applied), (None, Move::Stay));
            assert_eq!(p.outcome, Outcome::from(failure));
            assert_eq!(p.pos, Pos::new(0, 0));
        }
        assert!(!game.is_over());
    }

    #[test]
    fn collects_points() {
        let mut game = Game::new(ring(), Seating::Solo, 100);
        assert_eq!(game.remaining_points(), 14);
        let report = game.step(&[mv(Move::Right)]);
        assert_eq!(report.collected, vec![Pos::new(1, 0)]);
        assert_eq!(
            (report.players[0].gained, report.players[0].half_points),
            (2, 2)
        );
        assert_eq!(game.snapshot().board.get(Pos::new(1, 0)), Cell::Empty);
        let report = game.step(&[mv(Move::Left)]);
        assert!(report.collected.is_empty());
        assert_eq!(report.players[0].gained, 0);
        assert_eq!(game.remaining_points(), 13);
    }

    #[test]
    fn characters_swap_and_share_cells() {
        let mut game = Game::new(ring(), Seating::Duo, 100);
        for p1 in [Move::Up, Move::Up, Move::Up, Move::Stay] {
            game.step(&[mv(Move::Right), mv(p1)]);
        }
        let players = &game.snapshot().players;
        assert_eq!(
            (players[0].pos, players[1].pos),
            (Pos::new(4, 0), Pos::new(4, 1))
        );

        let report = game.step(&[mv(Move::Down), mv(Move::Up)]);
        assert!(report.players.iter().all(|p| p.outcome == Outcome::Ok));
        assert_eq!(
            (report.players[0].pos, report.players[1].pos),
            (Pos::new(4, 1), Pos::new(4, 0))
        );

        let report = game.step(&[mv(Move::Up), mv(Move::Stay)]);
        assert_eq!(
            (report.players[0].pos, report.players[1].pos),
            (Pos::new(4, 0), Pos::new(4, 0))
        );
    }

    #[test]
    fn contested_points_are_split() {
        // Both players reach (4, 0) on turn 4; it is the only point left on
        // that path, so both arrive together and split it.
        let mut game = Game::new(ring(), Seating::Duo, 100);
        for _ in 0..3 {
            game.step(&[mv(Move::Right), mv(Move::Up)]);
        }
        let report = game.step(&[mv(Move::Right), mv(Move::Up)]);
        assert_eq!(report.collected, vec![Pos::new(4, 0)]);
        assert_eq!(report.players[0].gained, 1);
        assert_eq!(report.players[1].gained, 1);
        assert_eq!(report.players[0].half_points, report.players[1].half_points);
    }

    #[test]
    fn swapped_seating() {
        let game = Game::new(ring(), Seating::DuoSwapped, 100);
        let players = &game.snapshot().players;
        assert_eq!(
            (players[0].pos, players[1].pos),
            (Pos::new(4, 4), Pos::new(0, 0))
        );
    }

    #[test]
    fn clearing_ends_the_game() {
        let mut game = Game::new(ring(), Seating::Solo, 100);
        let lap = [
            [Move::Right; 4],
            [Move::Down; 4],
            [Move::Left; 4],
            [Move::Up; 4],
        ];
        let mut last = None;
        for m in lap.concat() {
            if game.is_over() {
                break;
            }
            last = Some(game.step(&[mv(m)]));
        }
        let last = last.unwrap();
        assert_eq!(last.end, Some(EndReason::Cleared));
        assert_eq!(last.turn, 15);
        let result = game.result().unwrap();
        assert_eq!(result.reason, EndReason::Cleared);
        assert_eq!(result.cleared_turn, Some(15));
        assert_eq!(result.half_points, vec![28]);
        assert_eq!(result.winner, None);
    }

    #[test]
    fn turn_limit_ends_the_game() {
        let mut game = Game::new(ring(), Seating::Duo, 2);
        assert_eq!(game.step(&[mv(Move::Right), mv(Move::Stay)]).end, None);
        assert_eq!(
            game.step(&[mv(Move::Right), mv(Move::Stay)]).end,
            Some(EndReason::TurnLimit)
        );
        let result = game.result().unwrap();
        assert_eq!(result.reason, EndReason::TurnLimit);
        assert_eq!(result.turns, 2);
        assert_eq!(result.winner, Some(0));
        assert_eq!(result.cleared_turn, None);
    }

    #[test]
    fn abandoned_when_every_bot_crashed() {
        let crashed = Input::Failed(Failure::Crashed);
        let mut game = Game::new(ring(), Seating::Duo, 100);
        assert_eq!(game.step(&[crashed, mv(Move::Stay)]).end, None);
        assert_eq!(
            game.step(&[crashed, crashed]).end,
            Some(EndReason::Abandoned)
        );
        assert_eq!(game.result().unwrap().winner, None);
    }

    #[test]
    fn games_without_points_or_turns_are_over_at_once() {
        let empty = Maze::parse(&RING.replace('.', "_")).unwrap();
        let game = Game::new(empty, Seating::Solo, 100);
        assert!(game.is_over());
        assert_eq!(game.result().unwrap().reason, EndReason::Cleared);
        assert_eq!(game.result().unwrap().cleared_turn, Some(0));

        let game = Game::new(ring(), Seating::Solo, 0);
        assert_eq!(game.result().unwrap().reason, EndReason::TurnLimit);
    }

    #[test]
    #[should_panic(expected = "the game is over")]
    fn stepping_a_finished_game_panics() {
        let mut game = Game::new(ring(), Seating::Solo, 1);
        game.step(&[mv(Move::Stay)]);
        game.step(&[mv(Move::Stay)]);
    }

    #[test]
    fn default_turn_limit() {
        assert_eq!(default_max_turns(&ring()), 100);
    }

    fn maze() -> impl Strategy<Value = Maze> {
        (2u32..=8, 2u32..=8, 0u32..=100, 0u32..=100, any::<u64>()).prop_map(|(w, h, l, p, s)| {
            let params = MazeParams::new(2 * w + 1, 2 * h + 1, l, p).unwrap();
            generate(&MazeCode::new(params, s))
        })
    }

    fn input() -> impl Strategy<Value = Input> {
        prop_oneof![
            12 => prop::sample::select(Move::ALL.to_vec()).prop_map(Input::Move),
            1 => prop::sample::select(vec![
                Failure::Invalid,
                Failure::Error,
                Failure::Timeout,
                Failure::NotReady,
                Failure::Crashed,
            ])
            .prop_map(Input::Failed),
        ]
    }

    proptest! {
        #[test]
        fn engine_invariants(
            maze in maze(),
            turns in prop::collection::vec((input(), input()), 0..400),
        ) {
            let initial = maze.point_count();
            let mut game = Game::new(maze, Seating::Duo, 300);
            let mut remaining = game.remaining_points();
            for (a, b) in turns {
                if game.is_over() {
                    break;
                }
                let before = game.snapshot().clone();
                let report = game.step(&[a, b]);
                let after = game.snapshot();
                for (i, (p, input)) in report.players.iter().zip([a, b]).enumerate() {
                    prop_assert!(after.board.get(p.pos).is_open());
                    prop_assert_eq!(after.players[i].pos, p.pos);
                    let from = before.players[i].pos;
                    if let Input::Move(m) = input {
                        let legal = before.board.step(from, m).is_some();
                        prop_assert_eq!(p.outcome == Outcome::Ok, legal);
                    } else {
                        prop_assert_eq!(p.pos, from);
                    }
                }
                for &pos in &report.collected {
                    prop_assert_eq!(before.board.get(pos), Cell::Point);
                    prop_assert_eq!(after.board.get(pos), Cell::Empty);
                }
                prop_assert!(game.remaining_points() <= remaining);
                remaining = game.remaining_points();
                let total: u32 = after.players.iter().map(|p| p.half_points).sum();
                prop_assert_eq!(usize::try_from(total).unwrap(), 2 * (initial - remaining));
                prop_assert_eq!(remaining, after.board.point_count());
                if remaining == 0 {
                    prop_assert_eq!(report.end, Some(EndReason::Cleared));
                }
            }
        }

        // Player 1 mirrors player 0 through the rotation: by symmetry it
        // always stands on the rotated cell and scores the same.
        #[test]
        fn mirror_play_scores_equal(
            maze in maze(),
            moves in prop::collection::vec(prop::sample::select(Move::ALL.to_vec()), 0..400),
        ) {
            let mut game = Game::new(maze, Seating::Duo, 1_000);
            for m in moves {
                if game.is_over() {
                    break;
                }
                game.step(&[Input::Move(m), Input::Move(m.rotated())]);
                let s = game.snapshot();
                prop_assert_eq!(s.players[1].pos, s.board.rot(s.players[0].pos));
                prop_assert_eq!(s.players[0].half_points, s.players[1].half_points);
            }
        }
    }
}
