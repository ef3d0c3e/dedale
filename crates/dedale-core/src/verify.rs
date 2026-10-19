//! Checks a replay against the rules.
//!
//! Regenerates the maze from its code, replays the applied inputs through the
//! engine, and compares every stored position, outcome, collection and score,
//! the result and the stats.

use std::fmt;

use crate::game::{Failure, Game, Input, Outcome};
use crate::generator::generate;
use crate::record::{GameRecord, PlayerTurnRecord, REPLAY_FORMAT};
use crate::stats::compute_stats;

/// Where a replay disagrees with the rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyError {
    /// The turn at fault, when there is one.
    pub turn: Option<u32>,
    pub message: String,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.turn {
            Some(turn) => write!(f, "turn {turn}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for VerifyError {}

fn fail<T>(turn: Option<u32>, message: impl Into<String>) -> Result<T, VerifyError> {
    Err(VerifyError {
        turn,
        message: message.into(),
    })
}

/// # Errors
///
/// The first disagreement between the replay and the rules.
pub fn verify(record: &GameRecord) -> Result<(), VerifyError> {
    if record.format != REPLAY_FORMAT {
        return fail(None, format!("unknown replay format {}", record.format));
    }
    if let Some(code) = record.code
        && generate(&code) != record.maze
    {
        return fail(
            None,
            format!("the maze is not the one code {code} describes"),
        );
    }
    let seating = record.config.seating;
    if record.players.len() != seating.players() {
        let n = record.players.len();
        return fail(None, format!("{n} players for a {seating:?} game"));
    }
    for (i, (player, start)) in record
        .players
        .iter()
        .zip(seating.starts(&record.maze))
        .enumerate()
    {
        if player.start != start {
            return fail(
                None,
                format!("player {i} starts at {}, not {start}", player.start),
            );
        }
    }

    let mut game = Game::new(record.maze.clone(), seating, record.config.max_turns);
    for (expected, turn) in (1..).zip(&record.turns) {
        let at = Some(turn.turn);
        if turn.turn != expected {
            return fail(at, format!("expected turn {expected}"));
        }
        if game.is_over() {
            return fail(at, "the game was already over");
        }
        if turn.players.len() != record.players.len() {
            return fail(at, format!("{} player entries", turn.players.len()));
        }
        let inputs = turn
            .players
            .iter()
            .enumerate()
            .map(|(i, p)| {
                input(p).ok_or_else(|| VerifyError {
                    turn: at,
                    message: format!("player {i} has outcome {:?} without a move", p.outcome),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let report = game.step(&inputs);
        for (i, (got, want)) in report.players.iter().zip(&turn.players).enumerate() {
            let fields = [
                ("requested move", got.requested != want.requested),
                ("applied move", got.applied != want.applied),
                ("outcome", got.outcome != want.outcome),
                ("position", got.pos != want.pos),
                ("points gained", got.gained != want.gained),
                ("score", got.half_points != want.half_points),
            ];
            if let Some((field, _)) = fields.iter().find(|(_, differs)| *differs) {
                return fail(
                    at,
                    format!("player {i}: the {field} does not follow the rules"),
                );
            }
        }
        if report.collected != turn.collected {
            return fail(at, "the collected cells do not follow the rules");
        }
    }
    let Some(result) = game.result() else {
        return fail(None, "the replay stops before the game ends");
    };
    if result != record.result {
        return fail(None, "the result does not follow the rules");
    }
    if compute_stats(record) != record.stats {
        return fail(None, "the stats do not match the turns");
    }
    Ok(())
}

/// What the engine was given, as far as the record tells.
fn input(turn: &PlayerTurnRecord) -> Option<Input> {
    let failure = match turn.outcome {
        Outcome::Ok | Outcome::Illegal => return turn.requested.map(Input::Move),
        Outcome::Invalid => Failure::Invalid,
        Outcome::Error => Failure::Error,
        Outcome::Timeout => Failure::Timeout,
        Outcome::NotReady => Failure::NotReady,
        Outcome::Crashed => Failure::Crashed,
    };
    Some(Input::Failed(failure))
}

#[cfg(test)]
mod tests {
    use super::verify;
    use crate::code::MazeCode;
    use crate::game::{EndReason, Outcome};
    use crate::grid::{Cell, Pos};
    use crate::record::GameRecord;
    use crate::test_support::record;
    use proptest::prelude::*;

    /// Ways to break a record; each must be caught.
    fn tamper(record: &mut GameRecord, kind: usize, pick: usize) -> bool {
        let turns = record.turns.len();
        let players = record.players.len();
        let (t, p) = (pick % turns.max(1), pick % players);
        match kind {
            0 if turns > 0 => record.turns[t].players[p].pos.x ^= 1,
            1 if turns > 0 => record.turns[t].players[p].gained += 1,
            2 if turns > 0 => record.turns[t].players[p].half_points += 1,
            3 if turns > 0 => {
                let entry = &mut record.turns[t].players[p];
                entry.outcome = if entry.outcome == Outcome::Timeout {
                    Outcome::Invalid
                } else {
                    Outcome::Timeout
                };
            }
            4 if turns > 0 => record.turns[t].collected.push(Pos::new(0, 0)),
            5 if turns > 0 => record.turns[t].turn += 1,
            6 if turns > 0 => _ = record.turns.pop(),
            7 => record.result.turns += 1,
            8 => {
                record.result.reason = match record.result.reason {
                    EndReason::TurnLimit => EndReason::Cleared,
                    _ => EndReason::TurnLimit,
                }
            }
            9 => {
                let code = record.code.unwrap();
                record.code = Some(MazeCode::new(code.params(), code.seed() ^ 1));
            }
            10 => record.stats[p].half_points += 1,
            11 => record.stats[p].startup_us = None,
            12 => record.players[p].start.x ^= 1,
            _ => return false,
        }
        true
    }

    #[test]
    fn errors_name_the_turn() {
        let error = super::VerifyError {
            turn: Some(4),
            message: "player 1: the position does not follow the rules".to_owned(),
        };
        assert_eq!(
            error.to_string(),
            "turn 4: player 1: the position does not follow the rules"
        );
    }

    proptest! {
        #[test]
        fn honest_records_verify(record in record()) {
            prop_assert_eq!(verify(&record), Ok(()));
        }

        #[test]
        fn tampered_records_fail(mut record in record(), kind in 0usize..13, pick: usize) {
            prop_assume!(tamper(&mut record, kind, pick));
            prop_assert!(verify(&record).is_err());
        }

        #[test]
        fn json_roundtrip(record in record()) {
            let back = GameRecord::from_json(&record.to_json()).unwrap();
            prop_assert_eq!(verify(&back), Ok(()));
            prop_assert_eq!(back, record);
        }

        #[test]
        fn states_follow_the_turns(record in record()) {
            let start = record.state_at(0).unwrap();
            prop_assert_eq!(&start.board, record.maze.grid());
            for (i, turn) in record.turns.iter().enumerate() {
                let state = record.state_at(turn.turn).unwrap();
                prop_assert_eq!(usize::try_from(state.turn).unwrap(), i + 1);
                for (player, entry) in state.players.iter().zip(&turn.players) {
                    prop_assert_eq!(player.pos, entry.pos);
                    prop_assert_eq!(player.half_points, entry.half_points);
                }
                for pos in &turn.collected {
                    prop_assert_eq!(state.board.get(*pos), Cell::Empty);
                }
            }
            let past = u32::try_from(record.turns.len()).unwrap() + 1;
            prop_assert!(record.state_at(past).is_none());
        }
    }
}
