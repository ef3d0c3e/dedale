//! Tournaments: a series of games and a leaderboard.
//!
//! Every maze comes from one master seed, so a tournament can be replayed
//! exactly. Duo tournaments play each maze twice with the corners swapped;
//! solo tournaments run each bot alone on every maze. Games run one after
//! the other, so bots never compete for the CPU.

use std::cmp::Reverse;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use dedale_core::code::{MazeCode, MazeParams};
use dedale_core::game::{EndReason, GameResult, Seating};
use dedale_core::maze::Maze;
use dedale_core::record::GameRecord;
use dedale_core::rng::Rng;
use dedale_core::stats::Summary;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::referee::{Entrant, MatchConfig, MatchError, run_match};
use crate::replay;

/// Version of `tournament.json`.
pub const TOURNAMENT_FORMAT: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Each bot alone on every maze.
    Solo,
    /// Two bots, each maze twice with the corners swapped.
    Duo,
}

/// One game to play.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GamePlan {
    /// 1-based.
    pub index: usize,
    /// `None` when every game uses the same map file.
    pub code: Option<MazeCode>,
    pub seating: Seating,
    /// The bots playing, by tournament index, in player order.
    pub bots: Vec<usize>,
}

/// The games of a tournament, in order. Maze `i` gets the `i`-th seed of
/// the master stream; with a map file (`params == None`) every game uses it.
#[must_use]
pub fn schedule(
    mode: Mode,
    bots: usize,
    mazes: u32,
    params: Option<MazeParams>,
    master_seed: u64,
) -> Vec<GamePlan> {
    let mut seeds = Rng::from_seed(master_seed);
    let mut plans = Vec::new();
    for _ in 0..mazes {
        let seed = seeds.next_u64();
        let code = params.map(|p| MazeCode::new(p, seed));
        let mut push = |seating, players: Vec<usize>| {
            plans.push(GamePlan {
                index: plans.len() + 1,
                code,
                seating,
                bots: players,
            });
        };
        match mode {
            Mode::Duo => {
                push(Seating::Duo, vec![0, 1]);
                push(Seating::DuoSwapped, vec![0, 1]);
            }
            Mode::Solo => {
                for bot in 0..bots {
                    push(Seating::Solo, vec![bot]);
                }
            }
        }
    }
    plans
}

/// A bot's line on the leaderboard. Fields of the other mode stay 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standing {
    /// 1-based; tied bots share a rank.
    pub rank: usize,
    pub bot: usize,
    pub name: String,
    pub games: u32,
    /// Duo: win 2, draw 1, loss 0.
    pub match_points: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    /// Solo: mazes cleared, and the turns they took in total.
    pub cleared: u32,
    pub turns_to_clear: u64,
    pub half_points: u64,
    /// Turns with any outcome but `ok`.
    pub failures: u64,
    /// Over every reply in every game.
    pub latency_us: Option<Summary>,
}

/// Ranks the bots: duo by match points, then score, then fewer failures;
/// solo by mazes cleared, then fewer turns to clear them, then score.
#[must_use]
pub fn standings(mode: Mode, names: &[String], games: &[(GamePlan, GameRecord)]) -> Vec<Standing> {
    let mut table: Vec<Standing> = names
        .iter()
        .enumerate()
        .map(|(bot, name)| Standing {
            rank: 0,
            bot,
            name: name.clone(),
            games: 0,
            match_points: 0,
            wins: 0,
            draws: 0,
            losses: 0,
            cleared: 0,
            turns_to_clear: 0,
            half_points: 0,
            failures: 0,
            latency_us: None,
        })
        .collect();
    let mut latencies: Vec<Vec<u64>> = vec![Vec::new(); names.len()];
    for (plan, record) in games {
        let result = &record.result;
        for (player, &bot) in plan.bots.iter().enumerate() {
            let line = &mut table[bot];
            line.games += 1;
            line.half_points += u64::from(result.half_points[player]);
            match (mode, result.winner) {
                (Mode::Duo, Some(w)) if w == player => {
                    line.wins += 1;
                    line.match_points += 2;
                }
                (Mode::Duo, Some(_)) => line.losses += 1,
                (Mode::Duo, None) => {
                    line.draws += 1;
                    line.match_points += 1;
                }
                (Mode::Solo, _) => {
                    if result.reason == EndReason::Cleared {
                        line.cleared += 1;
                        line.turns_to_clear += u64::from(result.turns);
                    }
                }
            }
            if let Some(stats) = record.stats.get(player) {
                line.failures += u64::from(stats.outcomes.failures());
            }
            latencies[bot].extend(
                record
                    .turns
                    .iter()
                    .filter_map(|t| t.players[player].latency_us),
            );
        }
    }
    for (line, samples) in table.iter_mut().zip(latencies) {
        line.latency_us = Summary::of(samples);
    }
    let key = |s: &Standing| match mode {
        Mode::Duo => (
            s.match_points,
            Reverse(0),
            s.half_points,
            Reverse(s.failures),
        ),
        Mode::Solo => (
            s.cleared,
            Reverse(s.turns_to_clear),
            s.half_points,
            Reverse(0),
        ),
    };
    table.sort_by_key(|s| (Reverse(key(s)), s.bot));
    for i in 0..table.len() {
        table[i].rank = if i > 0 && key(&table[i]) == key(&table[i - 1]) {
            table[i - 1].rank
        } else {
            i + 1
        };
    }
    table
}

/// Where the mazes come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MazeSource {
    Generated {
        width: u8,
        height: u8,
        loops: u8,
        density: u8,
    },
    Map {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotInfo {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameEntry {
    #[serde(flatten)]
    pub plan: GamePlan,
    pub result: GameResult,
    /// Relative to `tournament.json`.
    pub replay: String,
}

/// `tournament.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentReport {
    pub format: u32,
    pub dedale: String,
    /// 16 hex digits.
    pub master_seed: String,
    pub mode: Mode,
    pub maze: MazeSource,
    pub startup_ms: u64,
    pub move_ms: u64,
    pub max_turns: Option<u32>,
    pub bots: Vec<BotInfo>,
    pub games: Vec<GameEntry>,
    pub leaderboard: Vec<Standing>,
}

#[derive(Debug, Error)]
pub enum TournamentError {
    #[error("game {game}: {source}")]
    Match {
        game: usize,
        #[source]
        source: MatchError,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: io::Error,
    },
}

/// The directory of one game, relative to the tournament's.
#[must_use]
pub fn game_dir(plan: &GamePlan, names: &[String]) -> PathBuf {
    let maze = plan
        .code
        .map_or_else(|| "map".to_owned(), |c| c.to_string());
    let who = match plan.seating {
        Seating::Solo => names[plan.bots[0]].clone(),
        Seating::Duo => "duo".to_owned(),
        Seating::DuoSwapped => "duo-swapped".to_owned(),
    };
    PathBuf::from("games").join(format!("{:03}-{maze}-{who}", plan.index))
}

/// Plays `plans` in order under `out`, writing each game's replay as soon
/// as it ends. `entrant(bot, dir)` starts a bot with its logs in `dir`;
/// `done` is called after each game.
///
/// # Errors
///
/// When a game cannot be played (including an interruption) or a file
/// cannot be written.
pub fn play(
    plans: &[GamePlan],
    names: &[String],
    maze: impl Fn(&GamePlan) -> Maze,
    mut entrant: impl FnMut(usize, &Path) -> Entrant,
    config: &MatchConfig,
    out: &Path,
    mut done: impl FnMut(&GamePlan, &GameRecord),
) -> Result<Vec<(GamePlan, GameRecord)>, TournamentError> {
    let mut games = Vec::new();
    for plan in plans {
        let dir = out.join(game_dir(plan, names));
        let write_error = |source, path: &Path| TournamentError::Write {
            path: path.display().to_string(),
            source,
        };
        fs::create_dir_all(&dir).map_err(|e| write_error(e, &dir))?;
        let entrants = plan.bots.iter().map(|&bot| entrant(bot, &dir)).collect();
        let config = MatchConfig {
            seating: plan.seating,
            ..config.clone()
        };
        let record =
            run_match(maze(plan), plan.code, entrants, &config, |_, _| {}).map_err(|source| {
                TournamentError::Match {
                    game: plan.index,
                    source,
                }
            })?;
        let path = dir.join("replay.json");
        replay::write(&path, &record).map_err(|e| write_error(e, &path))?;
        done(plan, &record);
        games.push((plan.clone(), record));
    }
    Ok(games)
}

#[cfg(test)]
mod tests {
    use super::{GamePlan, Mode, schedule, standings};
    use dedale_core::code::MazeParams;
    use dedale_core::game::{EndReason, GameResult, Seating};
    use dedale_core::generator::generate;
    use dedale_core::record::{ConfigRecord, GameRecord};

    fn names(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("bot{i}")).collect()
    }

    #[test]
    fn duo_schedules_swap_corners_on_each_maze() {
        let params = MazeParams::default();
        let plans = schedule(Mode::Duo, 2, 3, Some(params), 42);
        assert_eq!(plans.len(), 6);
        for pair in plans.chunks(2) {
            assert_eq!(pair[0].code, pair[1].code);
            assert_eq!(pair[0].seating, Seating::Duo);
            assert_eq!(pair[1].seating, Seating::DuoSwapped);
            assert_eq!(pair[0].bots, vec![0, 1]);
        }
        assert_ne!(plans[0].code, plans[2].code);
        assert_eq!(
            plans.iter().map(|p| p.index).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6]
        );
        assert_eq!(schedule(Mode::Duo, 2, 3, Some(params), 42), plans);
        assert_ne!(schedule(Mode::Duo, 2, 3, Some(params), 43), plans);
    }

    #[test]
    fn solo_schedules_run_every_bot_on_every_maze() {
        let plans = schedule(Mode::Solo, 3, 2, Some(MazeParams::default()), 7);
        assert_eq!(plans.len(), 6);
        let bots: Vec<usize> = plans.iter().map(|p| p.bots[0]).collect();
        assert_eq!(bots, [0, 1, 2, 0, 1, 2]);
        assert!(plans[..3].iter().all(|p| p.code == plans[0].code));
        let maps = schedule(Mode::Solo, 1, 2, None, 7);
        assert!(maps.iter().all(|p| p.code.is_none()));
    }

    /// A record with only what standings read: the result.
    fn game(
        seating: Seating,
        bots: Vec<usize>,
        reason: EndReason,
        turns: u32,
        half_points: Vec<u32>,
    ) -> (GamePlan, GameRecord) {
        let winner = match half_points.as_slice() {
            [a, b] if a > b => Some(0),
            [a, b] if b > a => Some(1),
            _ => None,
        };
        let code = "ded1-5x5-l0-p100-0000000000000000".parse().unwrap();
        let record = GameRecord {
            format: 1,
            dedale: String::new(),
            code: Some(code),
            maze: generate(&code),
            config: ConfigRecord {
                startup_ms: 0,
                move_ms: 0,
                max_turns: 100,
                seating,
            },
            players: Vec::new(),
            turns: Vec::new(),
            events: Vec::new(),
            result: GameResult {
                reason,
                turns,
                half_points,
                winner,
                cleared_turn: (reason == EndReason::Cleared).then_some(turns),
            },
            stats: Vec::new(),
        };
        let plan = GamePlan {
            index: 1,
            code: Some(code),
            seating,
            bots,
        };
        (plan, record)
    }

    #[test]
    fn duo_ranking() {
        let games = [
            game(
                Seating::Duo,
                vec![0, 1],
                EndReason::Cleared,
                50,
                vec![10, 4],
            ),
            game(
                Seating::DuoSwapped,
                vec![0, 1],
                EndReason::Cleared,
                50,
                vec![6, 6],
            ),
        ];
        let table = standings(Mode::Duo, &names(2), &games);
        assert_eq!(table[0].name, "bot0");
        assert_eq!(
            (
                table[0].match_points,
                table[0].wins,
                table[0].draws,
                table[0].losses
            ),
            (3, 1, 1, 0)
        );
        assert_eq!((table[1].match_points, table[1].losses), (1, 1));
        assert_eq!((table[0].rank, table[1].rank), (1, 2));
        assert_eq!(table[0].half_points, 16);
    }

    #[test]
    fn duo_ties_break_on_score_then_share_a_rank() {
        let games = [
            game(
                Seating::Duo,
                vec![0, 1],
                EndReason::Cleared,
                50,
                vec![4, 10],
            ),
            game(
                Seating::DuoSwapped,
                vec![0, 1],
                EndReason::Cleared,
                50,
                vec![9, 1],
            ),
        ];
        let table = standings(Mode::Duo, &names(2), &games);
        // One win each: bot1 scored 11 half-points against 13.
        assert_eq!(table[0].name, "bot0");
        assert_eq!((table[0].rank, table[1].rank), (1, 2));

        let draws = [game(
            Seating::Duo,
            vec![0, 1],
            EndReason::TurnLimit,
            9,
            vec![3, 3],
        )];
        let table = standings(Mode::Duo, &names(2), &draws);
        assert_eq!((table[0].rank, table[1].rank), (1, 1));
        assert_eq!((table[0].bot, table[1].bot), (0, 1));
    }

    #[test]
    fn solo_ranking() {
        let games = [
            game(Seating::Solo, vec![0], EndReason::Cleared, 120, vec![28]),
            game(Seating::Solo, vec![1], EndReason::Cleared, 90, vec![28]),
            game(Seating::Solo, vec![2], EndReason::TurnLimit, 400, vec![26]),
            game(Seating::Solo, vec![0], EndReason::Cleared, 100, vec![28]),
            game(Seating::Solo, vec![1], EndReason::Cleared, 100, vec![28]),
            game(Seating::Solo, vec![2], EndReason::Cleared, 10, vec![28]),
        ];
        let table = standings(Mode::Solo, &names(3), &games);
        let order: Vec<&str> = table.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(order, ["bot1", "bot0", "bot2"]);
        assert_eq!((table[0].cleared, table[0].turns_to_clear), (2, 190));
        assert_eq!((table[2].cleared, table[2].turns_to_clear), (1, 10));
        assert_eq!(table.iter().map(|s| s.rank).collect::<Vec<_>>(), [1, 2, 3]);
    }
}
