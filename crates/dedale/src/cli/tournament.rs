//! `dedale tournament`: a series of games and a leaderboard.

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use clap::Args;
use dedale_core::code::MazeParams;
use dedale_core::game::Seating;
use dedale_core::game::Snapshot;
use dedale_core::generator::generate;
use dedale_core::maze::Maze;
use dedale_core::record::{GameRecord, StartupRecord, TurnRecord};
use dedale_core::rng::fresh_seed;

use super::run::{TimingArgs, unique_names};
use super::serve::LiveArgs;
use super::summary::{describe, format_table, score};
use super::{CliResult, ColorChoice, MazeArgs, timestamp};
use crate::bot::BotSpec;
use crate::python::PythonRuntime;
use crate::referee::{Entrant, GameStart, Observer};
use crate::render::{Style, format_us};
use crate::tournament::{
    BotInfo, GameEntry, GamePlan, MazeSource, Mode, Standing, TOURNAMENT_FORMAT,
    TournamentObserver, TournamentReport, game_dir, play, schedule, standings,
};
use crate::web::live::{Live, LiveMode, LiveObserver};

#[derive(Debug, Args)]
pub(crate) struct TournamentArgs {
    /// The competitors: two for a duo tournament, or any number with --solo.
    #[arg(required = true, value_name = "BOT")]
    bots: Vec<PathBuf>,
    /// Run each bot alone on every maze (fewest turns to clear wins).
    #[arg(long)]
    solo: bool,
    /// Number of mazes (a duo plays each twice, corners swapped).
    #[arg(long, value_name = "N", default_value_t = 10)]
    games: u32,
    /// Mazes: --seed is the master seed every maze code derives from.
    #[command(flatten)]
    maze: MazeArgs,
    #[command(flatten)]
    timing: TimingArgs,
    /// Draw with ASCII characters only.
    #[arg(long)]
    ascii: bool,
    /// Colour the output.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,
    #[command(flatten)]
    live: LiveArgs,
    /// Output directory [default: dedale-out/<time>-tournament].
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
    /// Python interpreter for `.py` bots.
    #[arg(
        long,
        value_name = "PATH",
        env = "DEDALE_PYTHON",
        default_value = "python3"
    )]
    python: OsString,
}

pub(crate) fn run(args: &TournamentArgs) -> CliResult {
    let mode = mode(args)?;
    let master_seed = args.maze.seed.unwrap_or_else(fresh_seed);
    let (params, map, source) = mazes(args)?;
    let plans = schedule(mode, args.bots.len(), args.games, params, master_seed);
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from("dedale-out").join(format!("{}-tournament", timestamp())));
    fs::create_dir_all(&out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;

    let mut stdout = io::stdout().lock();
    let mode_name = match mode {
        Mode::Solo => "solo",
        Mode::Duo => "duo",
    };
    writeln!(
        stdout,
        "tournament  {mode_name}, {} mazes, {} games, master seed {master_seed:016x}",
        args.games,
        plans.len()
    )?;

    let runtime = PythonRuntime::extract()?;
    let launcher = runtime.launcher(args.python.clone());
    let specs: Vec<BotSpec> = args.bots.iter().map(BotSpec::from_path).collect();
    let names = unique_names(&specs);
    let entrant = |bot: usize, dir: &Path| Entrant {
        name: names[bot].clone(),
        path: specs[bot].path().display().to_string(),
        command: specs[bot].command(&launcher),
        stderr_log: Some(dir.join(format!("{}.stderr.log", names[bot]))),
    };
    let maze = |plan: &GamePlan| match (&map, plan.code) {
        (Some(maze), _) => maze.clone(),
        (None, Some(code)) => generate(&code),
        (None, None) => unreachable!("generated mazes have codes"),
    };
    let title = format!(
        "{mode_name} tournament: {}, {} mazes",
        names.join(", "),
        args.games
    );
    let live = args.live.start(
        LiveMode::Tournament,
        title,
        names.clone(),
        plans.len(),
        &out,
    )?;
    let fail = |e: Box<dyn Error>| {
        if let Some(l) = &live {
            l.live.failed(&e.to_string());
        }
        e
    };
    // The seating is set per game.
    let config = args.timing.config(Seating::Solo);
    let mut progress = Progress {
        names: &names,
        total: plans.len(),
        mode,
        live: live
            .as_ref()
            .map(|l| (Arc::clone(&l.live), l.live.observer())),
    };
    let games = play(&plans, &names, maze, entrant, &config, &out, &mut progress)
        .map_err(|e| fail(e.into()))?;

    let report = report(args, mode, master_seed, source, &specs, &names, &games);
    let path = out.join("tournament.json");
    fs::write(&path, serde_json::to_string_pretty(&report)? + "\n")
        .map_err(|e| fail(format!("cannot write {}: {e}", path.display()).into()))?;

    let style = Style {
        ascii: args.ascii,
        color: args.color.enabled(),
    };
    writeln!(stdout)?;
    write!(
        stdout,
        "{}",
        leaderboard_table(mode, &report.leaderboard, style)
    )?;
    writeln!(stdout)?;
    writeln!(stdout, "results  {}", path.display())?;
    drop(stdout);
    match live {
        Some(l) => {
            l.live.finished("tournament.json");
            l.linger()
        }
        None => Ok(()),
    }
}

/// Prints a line per finished game, and tells the live page (`--view`)
/// everything.
struct Progress<'a> {
    names: &'a [String],
    total: usize,
    mode: Mode,
    live: Option<(Arc<Live>, LiveObserver)>,
}

impl Observer for Progress<'_> {
    fn game_start(&mut self, start: &GameStart) {
        if let Some((_, observer)) = &mut self.live {
            observer.game_start(start);
        }
    }

    fn startup(&mut self, player: usize, startup: &StartupRecord) {
        if let Some((_, observer)) = &mut self.live {
            observer.startup(player, startup);
        }
    }

    fn turn(&mut self, snapshot: &Snapshot, turn: &TurnRecord) {
        if let Some((_, observer)) = &mut self.live {
            observer.turn(snapshot, turn);
        }
    }

    fn hold(&mut self, turn: u32) -> Option<Duration> {
        self.live
            .as_mut()
            .and_then(|(_, observer)| observer.hold(turn))
    }
}

impl TournamentObserver for Progress<'_> {
    fn game_done(&mut self, games: &[(GamePlan, GameRecord)]) {
        let Some((plan, record)) = games.last() else {
            return;
        };
        let dir = game_dir(plan, self.names);
        let name = dir
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        // Same thread as the caller's lock: stdout's lock is reentrant.
        let _ = writeln!(
            io::stdout(),
            "game {:>3}/{}  {name}  {}",
            plan.index,
            self.total,
            describe(record)
        );
        if let Some((live, _)) = &self.live {
            live.game_done(record, &dir.join("replay.json").display().to_string());
            live.standings(&standings(self.mode, self.names, games));
        }
    }
}

/// `tournament.json`.
fn report(
    args: &TournamentArgs,
    mode: Mode,
    master_seed: u64,
    maze: MazeSource,
    specs: &[BotSpec],
    names: &[String],
    games: &[(GamePlan, GameRecord)],
) -> TournamentReport {
    TournamentReport {
        format: TOURNAMENT_FORMAT,
        dedale: env!("CARGO_PKG_VERSION").to_owned(),
        master_seed: format!("{master_seed:016x}"),
        mode,
        maze,
        startup_ms: args.timing.startup_ms,
        move_ms: args.timing.move_ms,
        max_turns: args.timing.max_turns,
        bots: specs
            .iter()
            .zip(names)
            .map(|(spec, name)| BotInfo {
                name: name.clone(),
                path: spec.path().display().to_string(),
            })
            .collect(),
        games: games
            .iter()
            .map(|(plan, record)| entry(plan, record, names))
            .collect(),
        leaderboard: standings(mode, names, games),
    }
}

/// Duo for two bots, solo for one or with `--solo`.
fn mode(args: &TournamentArgs) -> CliResult<Mode> {
    if args.maze.maze.is_some() {
        return Err(
            "a tournament draws its mazes from --seed; use dedale run to replay one maze".into(),
        );
    }
    match (args.solo, args.bots.len()) {
        (true, _) | (false, 1) => Ok(Mode::Solo),
        (false, 2) => Ok(Mode::Duo),
        (false, n) => Err(format!("a duo tournament takes 2 bots, not {n}; add --solo").into()),
    }
}

/// Generated mazes (their parameters) or one map for every game.
fn mazes(args: &TournamentArgs) -> CliResult<(Option<MazeParams>, Option<Maze>, MazeSource)> {
    if let Some(path) = &args.maze.map {
        let (maze, _) = args.maze.resolve()?;
        let path = path.display().to_string();
        return Ok((None, Some(maze), MazeSource::Map { path }));
    }
    let p = args.maze.params()?;
    let source = MazeSource::Generated {
        width: p.width(),
        height: p.height(),
        loops: p.loops(),
        points: p.points(),
    };
    Ok((Some(p), None, source))
}

fn entry(plan: &GamePlan, record: &GameRecord, names: &[String]) -> GameEntry {
    GameEntry {
        plan: plan.clone(),
        result: record.result.clone(),
        replay: game_dir(plan, names)
            .join("replay.json")
            .display()
            .to_string(),
    }
}

fn leaderboard_table(mode: Mode, standings: &[Standing], style: Style) -> String {
    let latency = |s: &Standing| {
        s.latency_us.map_or_else(
            || "-".to_owned(),
            |l| format!("{} / {}", format_us(l.p50, style), format_us(l.p95, style)),
        )
    };
    let total = |half: u64| score(u32::try_from(half).unwrap_or(u32::MAX));
    let (header, right): (Vec<&str>, &[usize]) = match mode {
        Mode::Duo => (
            vec![
                "rank",
                "bot",
                "points",
                "won",
                "drawn",
                "lost",
                "score",
                "failures",
                "latency p50 / p95",
            ],
            &[2, 3, 4, 5, 6, 7],
        ),
        Mode::Solo => (
            vec![
                "rank",
                "bot",
                "cleared",
                "turns",
                "score",
                "failures",
                "latency p50 / p95",
            ],
            &[2, 3, 4, 5],
        ),
    };
    let mut rows = vec![header.into_iter().map(str::to_owned).collect::<Vec<_>>()];
    for s in standings {
        let row = match mode {
            Mode::Duo => vec![
                s.rank.to_string(),
                s.name.clone(),
                s.match_points.to_string(),
                s.wins.to_string(),
                s.draws.to_string(),
                s.losses.to_string(),
                total(s.half_points),
                s.failures.to_string(),
                latency(s),
            ],
            Mode::Solo => vec![
                s.rank.to_string(),
                s.name.clone(),
                format!("{}/{}", s.cleared, s.games),
                s.turns_to_clear.to_string(),
                total(s.half_points),
                s.failures.to_string(),
                latency(s),
            ],
        };
        rows.push(row);
    }
    format_table(&rows, right)
}
