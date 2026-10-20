//! The `dedale` command line.

mod about;
mod maze_args;
mod run;
mod serve;
mod summary;
mod tournament;
mod view;
mod viewer;

use std::error::Error;
use std::io::{self, IsTerminal, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, ValueEnum};

use crate::interrupt;
use crate::referee::MatchError;

pub use maze_args::MazeArgs;

#[derive(Debug, Parser)]
#[command(name = "dedale", version, about = "Referee for maze bots")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Play one game: one bot (solo) or two (duo).
    Run(run::RunArgs),
    /// Print a maze in the text format (its code goes to stderr).
    Maze(MazeArgs),
    /// Step through a recorded game, or verify it.
    Replay(viewer::ReplayArgs),
    /// Play a series of games and rank the bots.
    Tournament(tournament::TournamentArgs),
    /// Browse runs and tournaments in the browser.
    View(view::BrowseArgs),
    /// Print what bots say about themselves (name, author, description,
    /// version), one JSON object per bot, without playing.
    About(about::AboutArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    /// `auto` colours a terminal unless `NO_COLOR` is set.
    fn enabled(self) -> bool {
        match self {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
            }
        }
    }
}

/// Runs the command line and returns the process exit code: 0 on success,
/// 1 on an error, 130 when interrupted.
#[must_use]
pub fn main() -> ExitCode {
    let cli = Cli::parse();
    // Bots run in their own process groups, so Ctrl-C reaches only Dedale:
    // stop the game and let the bots be killed on the way out.
    let _ = ctrlc::set_handler(interrupt::request);
    let result = match cli.command {
        Command::Run(args) => run::run(&args),
        Command::Maze(args) => maze(&args),
        Command::Replay(args) => viewer::run(&args),
        Command::Tournament(args) => tournament::run(&args),
        Command::View(args) => view::run(&args),
        Command::About(args) => about::run(&args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        // Output piped into something that stopped reading (`| head`).
        Err(e) if is_broken_pipe(e.as_ref()) => ExitCode::SUCCESS,
        Err(e) if is_interrupted(e.as_ref()) => {
            eprintln!("dedale: interrupted");
            ExitCode::from(130)
        }
        Err(e) => {
            eprintln!("dedale: {}", chain(e.as_ref()));
            ExitCode::FAILURE
        }
    }
}

type CliResult<T = ()> = Result<T, Box<dyn Error>>;

fn maze(args: &MazeArgs) -> CliResult {
    let (maze, code) = args.resolve()?;
    if let Some(code) = code {
        eprintln!("{code}");
    }
    write!(io::stdout().lock(), "{maze}")?;
    Ok(())
}

fn is_interrupted(error: &(dyn Error + 'static)) -> bool {
    matches!(error.downcast_ref(), Some(MatchError::Interrupted))
}

fn is_broken_pipe(error: &(dyn Error + 'static)) -> bool {
    error
        .downcast_ref::<io::Error>()
        .is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe)
}

/// An error and its sources, `: `-separated.
fn chain(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !text.ends_with(&cause_text) {
            text = format!("{text}: {cause_text}");
        }
        source = cause.source();
    }
    text
}

/// `YYYYMMDD-HHMMSS` in UTC, for output directory names.
fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::civil_from_days;

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_745), (2026, 10, 19));
    }
}
