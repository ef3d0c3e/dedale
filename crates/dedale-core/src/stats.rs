//! Per-bot statistics of a game.
//!
//! Integers only (microseconds, counts), so recomputing them from a replay
//! gives exactly what was stored.

use serde::{Deserialize, Serialize};

use crate::game::{EndReason, Outcome, Seating};
use crate::grid::Move;
use crate::record::{EventKind, GameRecord};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerStats {
    pub startup_us: Option<u64>,
    pub init_us: Option<u64>,
    /// Over the replies Dedale received.
    pub latency_us: Option<Summary>,
    /// Over the replies that reported it.
    pub think_us: Option<Summary>,
    pub outcomes: OutcomeCounts,
    /// Replies that arrived after their turn was resolved.
    pub late: u32,
    pub turns: u32,
    /// Legal moves other than `stay`.
    pub steps: u32,
    /// Turns where the bot asked to stay.
    pub stays: u32,
    pub half_points: u32,
    /// Solo games that were cleared: the turn of the last collection.
    pub turns_to_clear: Option<u32>,
}

/// Distribution of a set of samples. Percentiles are nearest-rank: exact
/// samples, never interpolated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub count: u32,
    pub min: u64,
    /// Rounded down.
    pub mean: u64,
    pub p50: u64,
    pub p95: u64,
    pub p99: u64,
    pub max: u64,
}

impl Summary {
    /// `None` without samples.
    ///
    /// # Panics
    ///
    /// Never: the mean of `u64` samples fits in a `u64`.
    #[must_use]
    pub fn of(mut samples: Vec<u64>) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        samples.sort_unstable();
        let n = samples.len();
        let sum: u128 = samples.iter().map(|&s| u128::from(s)).sum();
        let count = u128::try_from(n).expect("usize fits in u128");
        let mean = u64::try_from(sum / count).expect("the mean of u64 samples fits in u64");
        // Rank ceil(p·n/100), 1-based.
        let rank = |p: usize| samples[(p * n).div_ceil(100).max(1) - 1];
        Some(Self {
            count: u32::try_from(n).unwrap_or(u32::MAX),
            min: samples[0],
            mean,
            p50: rank(50),
            p95: rank(95),
            p99: rank(99),
            max: samples[n - 1],
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeCounts {
    pub ok: u32,
    pub illegal: u32,
    pub invalid: u32,
    pub error: u32,
    pub timeout: u32,
    pub not_ready: u32,
    pub crashed: u32,
}

impl OutcomeCounts {
    fn count(&mut self, outcome: Outcome) {
        let slot = match outcome {
            Outcome::Ok => &mut self.ok,
            Outcome::Illegal => &mut self.illegal,
            Outcome::Invalid => &mut self.invalid,
            Outcome::Error => &mut self.error,
            Outcome::Timeout => &mut self.timeout,
            Outcome::NotReady => &mut self.not_ready,
            Outcome::Crashed => &mut self.crashed,
        };
        *slot += 1;
    }

    /// Every outcome except `ok`.
    #[must_use]
    pub fn failures(&self) -> u32 {
        self.illegal + self.invalid + self.error + self.timeout + self.not_ready + self.crashed
    }
}

/// The stats of every player, from the record's turns, events and startup.
#[must_use]
pub fn compute_stats(record: &GameRecord) -> Vec<PlayerStats> {
    let cleared_solo =
        record.config.seating == Seating::Solo && record.result.reason == EndReason::Cleared;
    record
        .players
        .iter()
        .enumerate()
        .map(|(i, player)| {
            let mut outcomes = OutcomeCounts::default();
            let (mut steps, mut stays, mut half_points) = (0, 0, 0);
            let mut latency = Vec::new();
            let mut think = Vec::new();
            for turn in record.turns.iter().filter_map(|t| t.players.get(i)) {
                outcomes.count(turn.outcome);
                if turn.outcome == Outcome::Ok && turn.applied != Move::Stay {
                    steps += 1;
                }
                if turn.requested == Some(Move::Stay) {
                    stays += 1;
                }
                half_points = turn.half_points;
                latency.extend(turn.latency_us);
                think.extend(turn.think_us);
            }
            let late = record
                .events
                .iter()
                .filter(|e| e.player == i && matches!(e.kind, EventKind::Late { .. }))
                .count();
            PlayerStats {
                startup_us: player.startup.startup_us,
                init_us: player.startup.init_us,
                latency_us: Summary::of(latency),
                think_us: Summary::of(think),
                outcomes,
                late: u32::try_from(late).unwrap_or(u32::MAX),
                turns: u32::try_from(record.turns.len()).unwrap_or(u32::MAX),
                steps,
                stays,
                half_points,
                turns_to_clear: if cleared_solo {
                    record.result.cleared_turn
                } else {
                    None
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::Summary;
    use proptest::prelude::*;

    #[test]
    fn nearest_rank_percentiles() {
        let summary = Summary::of((1..=100).collect()).unwrap();
        assert_eq!(
            summary,
            Summary {
                count: 100,
                min: 1,
                mean: 50,
                p50: 50,
                p95: 95,
                p99: 99,
                max: 100,
            }
        );
        let one = Summary::of(vec![7]).unwrap();
        assert_eq!(
            (one.min, one.p50, one.p99, one.max, one.mean),
            (7, 7, 7, 7, 7)
        );
        let two = Summary::of(vec![10, 2]).unwrap();
        assert_eq!((two.p50, two.p95, two.mean), (2, 10, 6));
        assert_eq!(Summary::of(Vec::new()), None);
    }

    #[test]
    fn huge_samples_do_not_overflow() {
        let summary = Summary::of(vec![u64::MAX; 3]).unwrap();
        assert_eq!(summary.mean, u64::MAX);
    }

    proptest! {
        #[test]
        fn summaries_are_ordered_samples(samples in prop::collection::vec(any::<u64>(), 1..200)) {
            let s = Summary::of(samples.clone()).unwrap();
            prop_assert!(s.min <= s.p50 && s.p50 <= s.p95 && s.p95 <= s.p99 && s.p99 <= s.max);
            prop_assert!(s.min <= s.mean && s.mean <= s.max);
            for value in [s.p50, s.p95, s.p99] {
                prop_assert!(samples.contains(&value));
            }
        }
    }
}
