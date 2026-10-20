//! `dedale replay`: rewind and inspect a game in the terminal.

use std::fmt::Write as _;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use clap::Args;
use dedale_core::record::{EventKind, GameRecord, StartupOutcome};
use dedale_core::verify::verify;

use super::{CliResult, ColorChoice, summary};
use crate::render::{Frame, Style, format_us, outcome_name, render};
use crate::replay;

#[derive(Debug, Args)]
pub(crate) struct ReplayArgs {
    /// A replay.json written by `dedale run`.
    file: PathBuf,
    /// Print the state after this turn and exit.
    #[arg(long, value_name = "N")]
    turn: Option<u32>,
    /// Check the replay against the rules and exit (status 1 on a mismatch).
    #[arg(long, conflicts_with = "turn")]
    verify: bool,
    /// Whose marks, log and errors to show first.
    #[arg(long, value_enum, default_value_t = Player::A)]
    player: Player,
    /// Draw with ASCII characters only.
    #[arg(long)]
    ascii: bool,
    /// Colour the output.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Player {
    A,
    B,
}

const LABELS: [char; 2] = ['A', 'B'];

const HELP: &str = "\
commands: Enter or n next, p previous, a turn number to jump there,
          g first, G last, a or b to choose the player, q quit
";

pub(crate) fn run(args: &ReplayArgs) -> CliResult {
    let record = replay::read(&args.file)?;
    if args.verify {
        verify(&record).map_err(|e| format!("{}: {e}", args.file.display()))?;
        println!(
            "{}: verified, {} turns",
            args.file.display(),
            record.turns.len()
        );
        return Ok(());
    }
    let style = Style {
        ascii: args.ascii,
        color: args.color.enabled(),
    };
    let player = match args.player {
        Player::B if record.players.len() > 1 => 1,
        _ => 0,
    };
    if let Some(turn) = args.turn {
        let last = last_turn(&record);
        if turn > last {
            return Err(format!("turn {turn} is past the end of the game (turn {last})").into());
        }
        print!("{}", view(&record, turn, player, style));
        return Ok(());
    }
    let clear = io::stdout().is_terminal();
    interact(
        &record,
        player,
        style,
        clear,
        io::stdin().lock(),
        io::stdout().lock(),
    )?;
    Ok(())
}

fn last_turn(record: &GameRecord) -> u32 {
    u32::try_from(record.turns.len()).unwrap_or(u32::MAX)
}

/// The command loop: one view per command until `q` or end of input.
fn interact(
    record: &GameRecord,
    mut player: usize,
    style: Style,
    clear: bool,
    input: impl BufRead,
    mut out: impl Write,
) -> io::Result<()> {
    let last = last_turn(record);
    let mut turn = 0;
    let mut note = String::new();
    let mut lines = input.lines();
    loop {
        if clear {
            write!(out, "\x1b[H\x1b[2J")?;
        }
        write!(out, "{}", view(record, turn, player, style))?;
        write!(out, "{note}> ")?;
        out.flush()?;
        note.clear();
        let Some(line) = lines.next().transpose()? else {
            writeln!(out)?;
            return Ok(());
        };
        match line.trim() {
            "" | "n" => turn = (turn + 1).min(last),
            "p" => turn = turn.saturating_sub(1),
            "g" => turn = 0,
            "G" => turn = last,
            "a" => player = 0,
            "b" if record.players.len() > 1 => player = 1,
            "q" => return Ok(()),
            "?" | "h" | "help" => note.push_str(HELP),
            other => match other.parse::<u32>() {
                Ok(n) if n <= last => turn = n,
                Ok(n) => note = format!("turn {n} is past the end (turn {last})\n"),
                Err(_) => note = format!("unknown command {other:?}; ? for help\n"),
            },
        }
    }
}

/// The frame after `turn`, then what `player` reported for it.
fn view(record: &GameRecord, turn: u32, player: usize, style: Style) -> String {
    let snapshot = record.state_at(turn).expect("turn within the game");
    let names: Vec<String> = record.players.iter().map(|p| p.name.clone()).collect();
    let index = usize::try_from(turn).unwrap_or(usize::MAX);
    let last = index.checked_sub(1).and_then(|i| record.turns.get(i));
    let marks = last
        .and_then(|t| t.players.get(player))
        .map_or(&[][..], |p| p.marks.as_slice());
    let frame = Frame {
        snapshot: &snapshot,
        names: &names,
        max_turns: record.config.max_turns,
        last,
        marks: &[(player, marks)],
    };
    let mut out = render(&frame, style);
    let label = LABELS[player];
    let name = &record.players[player].name;
    match last.and_then(|t| t.players.get(player)) {
        None => {
            let startup = &record.players[player].startup;
            let outcome = match startup.outcome {
                StartupOutcome::Ok => "ok",
                StartupOutcome::Late => "late",
                StartupOutcome::InitError => "init() raised",
                StartupOutcome::Timeout => "never ready",
                StartupOutcome::Crashed => "failed to start",
            };
            let _ = write!(out, "[{label} {name}] startup {outcome}");
            if let Some(us) = startup.startup_us {
                let _ = write!(out, " in {}", format_us(us, style));
            }
            if let Some(us) = startup.init_us {
                let _ = write!(out, " (import + init {})", format_us(us, style));
            }
            out.push('\n');
            details(&mut out, &startup.log, startup.error.as_ref(), None);
        }
        Some(entry) => {
            let _ = write!(out, "[{label} {name}] {}", outcome_name(entry.outcome));
            if let Some(us) = entry.think_us {
                let _ = write!(out, ", think {}", format_us(us, style));
            }
            if !entry.marks.is_empty() {
                let _ = write!(out, ", {} marks", entry.marks.len());
            }
            out.push('\n');
            details(
                &mut out,
                &entry.log,
                entry.error.as_ref(),
                entry.invalid.as_deref(),
            );
        }
    }
    for event in record.events.iter().filter(|e| e.turn == turn) {
        let who = LABELS.get(event.player).copied().unwrap_or('?');
        let what = match &event.kind {
            EventKind::Late { for_turn } => format!("late reply for turn {for_turn}"),
            EventKind::Ignored { reason } => format!("ignored: {reason}"),
        };
        let _ = writeln!(out, "  event {who}: {what}");
    }
    if turn == last_turn(record) {
        let _ = writeln!(out, "result  {}", summary::describe(record));
    }
    out
}

fn details(
    out: &mut String,
    log: &[String],
    error: Option<&dedale_core::protocol::BotError>,
    invalid: Option<&str>,
) {
    for line in log {
        let _ = writeln!(out, "  log: {line}");
    }
    if let Some(reason) = invalid {
        let _ = writeln!(out, "  invalid: {reason}");
    }
    if let Some(error) = error {
        let _ = writeln!(out, "  error: {}", error.message);
        for line in error.traceback.iter().flat_map(|t| t.lines()) {
            let _ = writeln!(out, "    {line}");
        }
    }
}
