//! `dedale run`: one game.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::{Args, ValueEnum};
use dedale_core::game::{Game, Seating, Snapshot};
use dedale_core::protocol::{About, Mark};
use dedale_core::record::TurnRecord;

use super::serve::LiveArgs;
use super::{CliResult, ColorChoice, MazeArgs, summary, timestamp};
use crate::bot::{BotSpec, Launcher, abouts};
use crate::python::PythonRuntime;
use crate::referee::{Entrant, GameStart, MatchConfig, Observer, run_match};
use crate::render::{Frame, Style, render};
use crate::replay;
use crate::sandbox::{self, Limits, Sandbox};
use crate::web::live::LiveMode;
use crate::web::live::Plan;

#[derive(Debug, Args)]
pub(crate) struct RunArgs {
    /// One bot (solo) or two (duo): `.py` files, or executables speaking the
    /// protocol.
    #[arg(required = true, num_args = 1..=2, value_name = "BOT")]
    bots: Vec<PathBuf>,
    #[command(flatten)]
    maze: MazeArgs,
    #[command(flatten)]
    timing: TimingArgs,
    #[command(flatten)]
    sandbox: SandboxArgs,
    /// Start player A from the bottom-right corner instead.
    #[arg(long)]
    swap: bool,
    #[command(flatten)]
    display: WatchArgs,
    #[command(flatten)]
    live: LiveArgs,
    /// Output directory [default: dedale-out/<time>-<maze>].
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

#[derive(Debug, Clone, Args)]
pub(crate) struct TimingArgs {
    /// Startup budget per bot (importing it and running its init), in milliseconds.
    #[arg(long, value_name = "MS", default_value_t = 2000)]
    pub startup_ms: u64,
    /// Time per move, in milliseconds.
    #[arg(long, value_name = "MS", default_value_t = 100)]
    pub move_ms: u64,
    /// Turn limit [default: 4·W·H].
    #[arg(long, value_name = "N")]
    pub max_turns: Option<u32>,
}

impl TimingArgs {
    pub(crate) fn config(&self, seating: Seating) -> MatchConfig {
        MatchConfig {
            startup: Duration::from_millis(self.startup_ms),
            move_timeout: Duration::from_millis(self.move_ms),
            max_turns: self.max_turns,
            ..MatchConfig::new(seating)
        }
    }
}

#[derive(Debug, Clone, Args)]
pub(crate) struct SandboxArgs {
    /// Run every bot in a sandbox (Linux, with bubblewrap): no network, a
    /// read-only view of the system and of its own directory, resource
    /// limits, and a CPU of its own.
    #[arg(long)]
    pub sandbox: bool,
    /// Memory per sandboxed bot process, in MiB.
    #[arg(long, value_name = "MIB", default_value_t = Limits::default().memory_mib, requires = "sandbox")]
    pub memory_mib: u64,
    /// Processes and threads per sandboxed bot.
    #[arg(long, value_name = "N", default_value_t = Limits::default().processes, requires = "sandbox")]
    pub processes: u64,
}

impl SandboxArgs {
    /// The sandbox asked for, checked to work here; `None` without
    /// `--sandbox`.
    pub(crate) fn sandbox(&self, python: &OsStr) -> CliResult<Option<Sandbox>> {
        if !self.sandbox {
            return Ok(None);
        }
        let limits = Limits {
            memory_mib: self.memory_mib,
            processes: self.processes,
            ..Limits::default()
        };
        Ok(Some(Sandbox::new(limits, python)?))
    }

    /// A launcher for `bots` bots, sandboxed when asked (checked to work
    /// here, its settings printed after `label`), and a CPU for each bot
    /// when sandboxed; Dedale itself keeps off those CPUs.
    pub(crate) fn launcher(
        &self,
        runtime: &PythonRuntime,
        python: &OsStr,
        bots: usize,
        label: &str,
    ) -> CliResult<(Launcher, Vec<usize>)> {
        let launcher = runtime.launcher(python);
        let Some(sandbox) = self.sandbox(python)? else {
            return Ok((launcher, Vec::new()));
        };
        let cpus = sandbox::cores(bots);
        sandbox::keep_off(&cpus);
        // Reentrant: the caller may hold stdout's lock.
        writeln!(
            std::io::stdout().lock(),
            "{label}{}",
            describe(&sandbox, &cpus)
        )?;
        Ok((launcher.sandboxed(Some(sandbox)), cpus))
    }
}

/// The sandbox in a line, for the terminal.
fn describe(sandbox: &Sandbox, cpus: &[usize]) -> String {
    let l = &sandbox.limits;
    let cpus: Vec<String> = cpus.iter().map(ToString::to_string).collect();
    format!(
        "on, CPU {} by bot; {} MiB, {} processes and {} MiB of /tmp per bot",
        cpus.join(", "),
        l.memory_mib,
        l.processes,
        l.tmp_mib
    )
}

#[derive(Debug, Clone, Args)]
pub(crate) struct WatchArgs {
    /// Show every turn as it is played.
    #[arg(long, conflicts_with = "view")]
    pub watch: bool,
    /// Pause after each shown turn, in milliseconds.
    #[arg(long, value_name = "MS", default_value_t = 0, requires = "watch")]
    pub delay: u64,
    /// Whose marks to draw: a player's, both players', or none.
    #[arg(long, value_enum, default_value_t = Marks::A)]
    pub marks: Marks,
    /// Draw with ASCII characters only.
    #[arg(long)]
    pub ascii: bool,
    /// Colour the output.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
}

impl WatchArgs {
    pub(crate) fn style(&self) -> Style {
        Style {
            ascii: self.ascii,
            color: self.color.enabled(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Marks {
    A,
    B,
    Both,
    None,
}

impl Marks {
    /// The players whose marks are drawn, in drawing order.
    pub(crate) fn players(self) -> &'static [usize] {
        match self {
            Marks::A => &[0],
            Marks::B => &[1],
            Marks::Both => &[0, 1],
            Marks::None => &[],
        }
    }
}

pub(crate) fn run(args: &RunArgs) -> CliResult {
    let (maze, code) = args.maze.resolve()?;
    let maze_name = match (&code, &args.maze.map) {
        (Some(code), _) => code.to_string(),
        (None, Some(path)) => path.display().to_string(),
        (None, None) => unreachable!("a maze without a code comes from a file"),
    };
    writeln!(std::io::stdout().lock(), "maze    {maze_name}")?;
    let seating = match (args.bots.len(), args.swap) {
        (1, _) => Seating::Solo,
        (_, false) => Seating::Duo,
        (_, true) => Seating::DuoSwapped,
    };
    let out = args.out.clone().unwrap_or_else(|| {
        let tag = code.map_or_else(|| "map".to_owned(), |c| c.to_string());
        PathBuf::from("dedale-out").join(format!("{}-{tag}", timestamp()))
    });
    fs::create_dir_all(&out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;

    let runtime = PythonRuntime::extract()?;
    let specs: Vec<BotSpec> = args.bots.iter().map(BotSpec::from_path).collect();
    let (launcher, cpus) =
        args.sandbox
            .launcher(&runtime, &args.python, specs.len(), "sandbox ")?;
    let mut config = args.timing.config(seating);
    config.sandbox = launcher.sandbox.is_some();
    // A bot that says nothing plays all the same: its game reports why.
    let abouts: Vec<About> = abouts(&specs, &launcher, config.hello_timeout)
        .into_iter()
        .map(Option::unwrap_or_default)
        .collect();
    let names = player_names(&specs, &abouts);
    let files = file_names(&specs);
    let entrants: Vec<Entrant> = specs
        .iter()
        .zip(abouts)
        .enumerate()
        .map(|(i, (spec, about))| Entrant {
            name: names[i].clone(),
            path: spec.path().display().to_string(),
            about,
            cpu: cpus.get(i).copied(),
            command: spec.command_on(&launcher, cpus.get(i).copied()),
            stderr_log: Some(out.join(format!("{}.stderr.log", files[i]))),
        })
        .collect();

    let style = args.display.style();
    let live = args.live.start(
        || Plan {
            mode: LiveMode::Run,
            title: format!("{} on {maze_name}", names.join(" vs ")),
            bots: names.clone(),
            games: 1,
            settings: settings(&args.timing, seating),
            tournament: None,
            start: Some(GameStart::new(maze.clone(), code, &entrants, &config)),
        },
        &out,
    )?;
    let mut watcher = Watcher {
        names,
        max_turns: 0,
        style,
        marks: args.display.marks.players(),
        clear: std::io::stdout().is_terminal(),
        delay: Duration::from_millis(args.display.delay),
        shown: Instant::now(),
    };
    let mut live_observer = live.as_ref().map(|l| l.live.observer());
    let observer: &mut dyn Observer = match &mut live_observer {
        Some(observer) => observer,
        None if args.display.watch => &mut watcher,
        None => &mut (),
    };
    let fail = |e: Box<dyn std::error::Error>| {
        if let Some(l) = &live {
            l.live.failed(&e.to_string());
        }
        e
    };
    let record = run_match(maze, code, entrants, &config, observer).map_err(|e| fail(e.into()))?;

    let path = out.join("replay.json");
    replay::write(&path, &record)
        .map_err(|e| fail(format!("cannot write {}: {e}", path.display()).into()))?;
    if let Some(l) = &live {
        l.live.game_done(&record, "replay.json");
    }
    let mut stdout = std::io::stdout().lock();
    summary::print_game(&mut stdout, &record, style)?;
    writeln!(stdout, "replay  {}", path.display())?;
    drop(stdout);
    match live {
        Some(l) => {
            l.live.finished("replay.json");
            l.linger()
        }
        None => Ok(()),
    }
}

/// Timings in a line, for the live page.
fn settings(timing: &TimingArgs, seating: Seating) -> String {
    let seats = match seating {
        Seating::Solo => "solo",
        Seating::Duo => "duo",
        Seating::DuoSwapped => "duo, corners swapped",
    };
    let turns = timing
        .max_turns
        .map_or_else(String::new, |n| format!(", {n} turns at most"));
    format!(
        "{seats}, {} ms startup, {} ms per move{turns}",
        timing.startup_ms, timing.move_ms
    )
}

/// Shows each turn of a game as it is played.
struct Watcher {
    names: Vec<String>,
    /// Known once the game starts.
    max_turns: u32,
    style: Style,
    marks: &'static [usize],
    /// Redraw in place (on a terminal) rather than print frame after frame.
    clear: bool,
    /// Held between a frame and the next turn.
    delay: Duration,
    shown: Instant,
}

impl Observer for Watcher {
    fn game_start(&mut self, start: &GameStart) {
        self.max_turns = start.config.max_turns;
        let game = Game::new(start.maze.clone(), start.config.seating, self.max_turns);
        self.show(game.snapshot(), None);
    }

    fn turn(&mut self, snapshot: &Snapshot, turn: &TurnRecord) {
        self.show(snapshot, Some(turn));
    }

    fn hold(&mut self, _turn: u32) -> Option<Duration> {
        (self.shown + self.delay)
            .checked_duration_since(Instant::now())
            .filter(|wait| !wait.is_zero())
    }
}

impl Watcher {
    fn show(&mut self, snapshot: &Snapshot, last: Option<&TurnRecord>) {
        let marks: Vec<(usize, &[Mark])> = last.map_or_else(Vec::new, |turn| {
            self.marks
                .iter()
                .filter_map(|&i| turn.players.get(i).map(|p| (i, p.marks.as_slice())))
                .collect()
        });
        let frame = Frame {
            snapshot,
            names: &self.names,
            max_turns: self.max_turns,
            last,
            marks: &marks,
        };
        let text = render(&frame, self.style);
        let mut stdout = std::io::stdout().lock();
        let _ = if self.clear {
            write!(stdout, "\x1b[H\x1b[2J{text}")
        } else {
            writeln!(stdout, "{text}")
        };
        let _ = stdout.flush();
        self.shown = Instant::now();
    }
}

/// What the players are called: what each bot calls itself, else its file
/// name; made unique.
pub(crate) fn player_names(specs: &[BotSpec], abouts: &[About]) -> Vec<String> {
    let declared = specs
        .iter()
        .zip(abouts)
        .map(|(spec, about)| about.name.clone().unwrap_or_else(|| spec.name()));
    unique_names(declared)
}

/// The bots' file names, made unique: for their files, which never take
/// what a bot calls itself.
pub(crate) fn file_names(specs: &[BotSpec]) -> Vec<String> {
    unique_names(specs.iter().map(BotSpec::name))
}

/// Names made unique with a suffix: `greedy`, `greedy-2`.
fn unique_names(bases: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for base in bases {
        let mut name = base.clone();
        let mut n = 2;
        while names.contains(&name) {
            name = format!("{base}-{n}");
            n += 1;
        }
        names.push(name);
    }
    names
}

#[cfg(test)]
mod tests {
    use super::{file_names, player_names};
    use crate::bot::BotSpec;
    use dedale_core::protocol::About;

    #[test]
    fn names_are_unique() {
        let specs: Vec<_> = ["a/greedy.py", "b/greedy.py", "random.py", "c/bot.py"]
            .into_iter()
            .map(BotSpec::from_path)
            .collect();
        assert_eq!(file_names(&specs), ["greedy", "greedy-2", "random", "bot"]);
        let named = |name: &str| About {
            name: Some(name.to_owned()),
            ..About::default()
        };
        let abouts = [
            About::default(),
            named("random"),
            named("Ada's"),
            named("Ada's"),
        ];
        assert_eq!(
            player_names(&specs, &abouts),
            ["greedy", "random", "Ada's", "Ada's-2"]
        );
    }
}
