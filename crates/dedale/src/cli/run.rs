//! `dedale run`: one game.

use std::ffi::OsString;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use clap::{Args, ValueEnum};
use dedale_core::game::{Game, Seating, Snapshot, default_max_turns};
use dedale_core::record::TurnRecord;

use super::{CliResult, ColorChoice, MazeArgs, summary, timestamp};
use crate::bot::BotSpec;
use crate::python::PythonRuntime;
use crate::referee::{Entrant, MatchConfig, run_match};
use crate::render::{Frame, Style, render};
use crate::replay;

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
    /// Start player A from the bottom-right corner instead.
    #[arg(long)]
    swap: bool,
    #[command(flatten)]
    view: ViewArgs,
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
pub(crate) struct ViewArgs {
    /// Show every turn as it is played.
    #[arg(long)]
    pub watch: bool,
    /// Pause after each shown turn, in milliseconds.
    #[arg(long, value_name = "MS", default_value_t = 0, requires = "watch")]
    pub delay: u64,
    /// Whose marks to draw.
    #[arg(long, value_enum, default_value_t = Marks::A)]
    pub marks: Marks,
    /// Draw with ASCII characters only.
    #[arg(long)]
    pub ascii: bool,
    /// Colour the output.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
}

impl ViewArgs {
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
    None,
}

impl Marks {
    pub(crate) fn player(self) -> Option<usize> {
        match self {
            Marks::A => Some(0),
            Marks::B => Some(1),
            Marks::None => None,
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
    let launcher = runtime.launcher(args.python.clone());
    let specs: Vec<BotSpec> = args.bots.iter().map(BotSpec::from_path).collect();
    let names = unique_names(&specs);
    let entrants = specs
        .iter()
        .zip(&names)
        .map(|(spec, name)| Entrant {
            name: name.clone(),
            path: spec.path().display().to_string(),
            command: spec.command(&launcher),
            stderr_log: Some(out.join(format!("{name}.stderr.log"))),
        })
        .collect();

    let config = args.timing.config(seating);
    let style = args.view.style();
    let max_turns = config.max_turns.unwrap_or_else(|| default_max_turns(&maze));
    let mut watcher = args.view.watch.then(|| Watcher {
        names: names.clone(),
        max_turns,
        style,
        marks: args.view.marks.player(),
        clear: std::io::stdout().is_terminal(),
        delay: Duration::from_millis(args.view.delay),
    });
    if let Some(watcher) = &mut watcher {
        let start = Game::new(maze.clone(), seating, max_turns);
        watcher.show(start.snapshot(), None);
    }
    let record = run_match(maze, code, entrants, &config, |snapshot, turn| {
        if let Some(watcher) = &mut watcher {
            watcher.show(snapshot, Some(turn));
        }
    })?;

    let path = out.join("replay.json");
    replay::write(&path, &record).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let mut stdout = std::io::stdout().lock();
    summary::print_game(&mut stdout, &record, style)?;
    writeln!(stdout, "replay  {}", path.display())?;
    Ok(())
}

/// Shows each turn of a game as it is played.
struct Watcher {
    names: Vec<String>,
    max_turns: u32,
    style: Style,
    marks: Option<usize>,
    /// Redraw in place (on a terminal) rather than print frame after frame.
    clear: bool,
    delay: Duration,
}

impl Watcher {
    fn show(&mut self, snapshot: &Snapshot, last: Option<&TurnRecord>) {
        let marks = last
            .zip(self.marks)
            .and_then(|(turn, player)| turn.players.get(player))
            .map_or(&[][..], |p| p.marks.as_slice());
        let frame = Frame {
            snapshot,
            names: &self.names,
            max_turns: self.max_turns,
            last,
            marks,
        };
        let text = render(&frame, self.style);
        let mut stdout = std::io::stdout().lock();
        let _ = if self.clear {
            write!(stdout, "\x1b[H\x1b[2J{text}")
        } else {
            writeln!(stdout, "{text}")
        };
        let _ = stdout.flush();
        if !self.delay.is_zero() {
            sleep(self.delay);
        }
    }
}

/// Bot names from file names, made unique with a suffix.
pub(crate) fn unique_names(specs: &[BotSpec]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for spec in specs {
        let base = spec.name();
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
    use super::unique_names;
    use crate::bot::BotSpec;

    #[test]
    fn names_are_unique() {
        let specs: Vec<_> = ["a/greedy.py", "b/greedy.py", "random.py"]
            .into_iter()
            .map(BotSpec::from_path)
            .collect();
        assert_eq!(unique_names(&specs), ["greedy", "greedy-2", "random"]);
    }
}
