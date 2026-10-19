//! What a game's result looks like in the terminal.

use std::io::{self, Write};

use dedale_core::game::{EndReason, Seating};
use dedale_core::record::{GameRecord, StartupOutcome};
use dedale_core::stats::PlayerStats;

use crate::render::{Style, format_us};

const LABELS: [char; 2] = ['A', 'B'];

pub(crate) fn print_game(
    out: &mut impl Write,
    record: &GameRecord,
    style: Style,
) -> io::Result<()> {
    writeln!(out, "result  {}", describe(record))?;
    writeln!(out)?;
    write!(out, "{}", table(record, style))?;
    writeln!(out)
}

/// One line: how the game ended and who won.
pub(crate) fn describe(record: &GameRecord) -> String {
    let result = &record.result;
    let turns = result.turns;
    let ending = match result.reason {
        EndReason::Cleared => format!("cleared on turn {turns}"),
        EndReason::TurnLimit => format!("turn limit reached after {turns} turns"),
        EndReason::Abandoned => format!("abandoned on turn {turns}: every bot crashed"),
    };
    let scores: Vec<String> = result.half_points.iter().map(|&h| score(h)).collect();
    match (record.config.seating, result.winner) {
        (Seating::Solo, _) => format!("{ending}, {} points", scores[0]),
        (_, Some(winner)) => format!(
            "{ending}: {} {} wins, {} to {}",
            LABELS[winner],
            record.players[winner].name,
            scores[winner],
            scores[1 - winner]
        ),
        (_, None) => format!("{ending}: draw, {} to {}", scores[0], scores[1]),
    }
}

pub(crate) fn score(half_points: u32) -> String {
    if half_points.is_multiple_of(2) {
        (half_points / 2).to_string()
    } else {
        format!("{}.5", half_points / 2)
    }
}

/// One row per player: score, steps, startup, latency, problems.
fn table(record: &GameRecord, style: Style) -> String {
    let header = [
        "",
        "player",
        "score",
        "steps",
        "startup",
        "latency p50 / p95 / max",
        "problems",
    ];
    let dash = || "-".to_owned();
    let mut rows = vec![header.map(str::to_owned)];
    for (i, (player, stats)) in record.players.iter().zip(&record.stats).enumerate() {
        let latency = stats.latency_us.map_or_else(dash, |s| {
            let us = |v| format_us(v, style);
            format!("{} / {} / {}", us(s.p50), us(s.p95), us(s.max))
        });
        let problems = problems(record, i, stats);
        rows.push([
            LABELS.get(i).map_or_else(String::new, char::to_string),
            player.name.clone(),
            score(stats.half_points),
            stats.steps.to_string(),
            stats
                .startup_us
                .map_or_else(dash, |us| format_us(us, style)),
            latency,
            if problems.is_empty() {
                dash()
            } else {
                problems.join(", ")
            },
        ]);
    }
    format_table(&rows, &[2, 3])
}

/// Rows of cells in aligned columns, two spaces apart; the columns in
/// `right` (numbers) are right-aligned.
pub(crate) fn format_table<R: AsRef<[String]>>(rows: &[R], right: &[usize]) -> String {
    let columns = rows.iter().map(|r| r.as_ref().len()).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.as_ref().get(c))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for row in rows {
        let mut line = String::new();
        for (c, cell) in row.as_ref().iter().enumerate() {
            let pad = " ".repeat(widths[c] - cell.chars().count());
            if right.contains(&c) {
                line.push_str(&pad);
                line.push_str(cell);
            } else {
                line.push_str(cell);
                line.push_str(&pad);
            }
            line.push_str("  ");
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn problems(record: &GameRecord, player: usize, stats: &PlayerStats) -> Vec<String> {
    let mut problems = Vec::new();
    match record.players[player].startup.outcome {
        StartupOutcome::Ok => {}
        StartupOutcome::Late => problems.push("slow startup".to_owned()),
        StartupOutcome::InitError => problems.push("init() raised".to_owned()),
        StartupOutcome::Timeout => problems.push("never ready".to_owned()),
        StartupOutcome::Crashed => problems.push("failed to start".to_owned()),
    }
    let o = &stats.outcomes;
    let counts = [
        (o.illegal, "illegal"),
        (o.invalid, "invalid"),
        (o.error, "error"),
        (o.timeout, "timeout"),
        (o.not_ready, "not ready"),
        (o.crashed, "crashed"),
        (stats.late, "late"),
    ];
    problems.extend(
        counts
            .into_iter()
            .filter(|&(n, _)| n > 0)
            .map(|(n, what)| format!("{n} {what}")),
    );
    if record.players[player]
        .exit
        .as_ref()
        .is_some_and(|e| e.killed)
    {
        problems.push("killed".to_owned());
    }
    problems
}
