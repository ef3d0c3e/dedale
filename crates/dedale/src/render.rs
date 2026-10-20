//! Text frames of a game state, shared by `--watch` and `dedale replay`.
//!
//! Every cell is two columns wide so the maze looks square in a terminal.

use std::collections::HashMap;
use std::fmt::Write;

use dedale_core::game::{Outcome, Snapshot};
use dedale_core::grid::{Cell, Move, Pos};
use dedale_core::protocol::{Mark, MarkColor, MarkKind};
use dedale_core::record::TurnRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    /// Plain ASCII instead of block and dot characters.
    pub ascii: bool,
    /// ANSI colours; the caller decides (only on a terminal).
    pub color: bool,
}

/// Everything one frame shows.
#[derive(Debug, Clone, Copy)]
pub struct Frame<'a> {
    pub snapshot: &'a Snapshot,
    /// Player names, by index.
    pub names: &'a [String],
    pub max_turns: u32,
    /// The turn that led to this state, for moves, outcomes and timings.
    pub last: Option<&'a TurnRecord>,
    /// Cells to highlight (usually one player's marks for that turn).
    pub marks: &'a [Mark],
}

const LABELS: [char; 2] = ['A', 'B'];

#[must_use]
pub fn render(frame: &Frame<'_>, style: Style) -> String {
    let mut out = header(frame, style);
    let snapshot = frame.snapshot;
    let board = &snapshot.board;
    // The last mark on a cell wins.
    let marked: HashMap<Pos, &Mark> = frame
        .marks
        .iter()
        .flat_map(|m| m.cells.iter().map(move |&pos| (pos, m)))
        .collect();
    for y in 0..board.height() {
        let mut line = String::new();
        for x in 0..board.width() {
            let pos = Pos::new(x, y);
            let here: Vec<usize> = (0..snapshot.players.len())
                .filter(|&i| snapshot.players[i].pos == pos)
                .collect();
            let (glyph, color) = match (here.as_slice(), board.get(pos)) {
                ([i], _) => (player_glyph(*i), if *i == 0 { GREEN } else { RED }),
                ([_, ..], _) => ("X ", MAGENTA),
                ([], Cell::Wall) => (if style.ascii { "##" } else { "██" }, ""),
                ([], _) if marked.contains_key(&pos) => mark_glyph(marked[&pos], style),
                ([], Cell::Point) => (if style.ascii { ". " } else { "· " }, YELLOW),
                ([], Cell::Empty) => ("  ", ""),
            };
            paint(&mut line, glyph, color, style);
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// A marked cell: `∘` for cells, `•` along a path, in the mark's colour.
fn mark_glyph(mark: &Mark, style: Style) -> (&'static str, &'static str) {
    let glyph = match (mark.kind, style.ascii) {
        (MarkKind::Cells, false) => "∘ ",
        (MarkKind::Cells, true) => "+ ",
        (MarkKind::Path, false) => "• ",
        (MarkKind::Path, true) => "* ",
    };
    let color = match mark.color {
        None => CYAN,
        Some(MarkColor::Red) => "\x1b[31m",
        Some(MarkColor::Orange) => "\x1b[38;5;208m",
        Some(MarkColor::Yellow) => "\x1b[33m",
        Some(MarkColor::Green) => "\x1b[32m",
        Some(MarkColor::Teal) => "\x1b[38;5;30m",
        Some(MarkColor::Blue) => "\x1b[34m",
        Some(MarkColor::Purple) => "\x1b[35m",
        Some(MarkColor::Pink) => "\x1b[38;5;213m",
    };
    (glyph, color)
}

fn player_glyph(i: usize) -> &'static str {
    if i == 0 { "A " } else { "B " }
}

const GREEN: &str = "\x1b[1;32m";
const RED: &str = "\x1b[1;31m";
const MAGENTA: &str = "\x1b[1;35m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";

fn paint(out: &mut String, text: &str, color: &str, style: Style) {
    if style.color && !color.is_empty() {
        out.push_str(color);
        out.push_str(text);
        out.push_str(RESET);
    } else {
        out.push_str(text);
    }
}

/// `turn 12/400 · 87 points left`, then one line per player: label, name,
/// score, and the last turn's move, outcome and latency.
fn header(frame: &Frame<'_>, style: Style) -> String {
    let snapshot = frame.snapshot;
    let dot = if style.ascii { "-" } else { "·" };
    let mut out = format!(
        "turn {}/{} {dot} {} points left\n",
        snapshot.turn,
        frame.max_turns,
        snapshot.board.point_count()
    );
    let width = frame
        .names
        .iter()
        .map(|n| n.chars().count())
        .max()
        .unwrap_or(0);
    for (i, player) in snapshot.players.iter().enumerate() {
        let label = LABELS.get(i).copied().unwrap_or('?');
        let name = frame.names.get(i).map_or("", String::as_str);
        let score = format_score(player.half_points);
        let mut line = format!("{label} {name:<width$}  {score:>6}");
        if let Some(turn) = frame.last.and_then(|t| t.players.get(i)) {
            let mv = turn.requested.map_or("-", Move::as_str);
            let outcome = outcome_name(turn.outcome);
            let latency = turn
                .latency_us
                .map_or_else(|| "-".to_owned(), |us| format_us(us, style));
            let _ = write!(line, "  {mv:<5}  {outcome:<9}  {latency}");
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn format_score(half_points: u32) -> String {
    if half_points.is_multiple_of(2) {
        format!("{}", half_points / 2)
    } else {
        format!("{}.5", half_points / 2)
    }
}

/// `850 µs`, `1.2 ms`, `3.45 s`.
#[must_use]
pub fn format_us(us: u64, style: Style) -> String {
    let micro = if style.ascii { "us" } else { "µs" };
    match us {
        0..1_000 => format!("{us} {micro}"),
        1_000..1_000_000 => format!("{}.{} ms", us / 1_000, us % 1_000 / 100),
        _ => format!("{}.{:02} s", us / 1_000_000, us % 1_000_000 / 10_000),
    }
}

#[must_use]
pub fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Ok => "ok",
        Outcome::Illegal => "illegal",
        Outcome::Invalid => "invalid",
        Outcome::Error => "error",
        Outcome::Timeout => "timeout",
        Outcome::NotReady => "not ready",
        Outcome::Crashed => "crashed",
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, Style, format_us, render};
    use dedale_core::game::{Game, Input, Outcome, Seating};
    use dedale_core::grid::{Move, Pos};
    use dedale_core::maze::Maze;
    use dedale_core::protocol::{Mark, MarkColor, MarkKind};
    use dedale_core::record::{PlayerTurnRecord, TurnRecord};

    const RING: &str = "_....\n.###.\n.###.\n.###.\n...._\n";

    fn names(n: usize) -> Vec<String> {
        ["greedy", "random"][..n]
            .iter()
            .map(|s| (*s).to_owned())
            .collect()
    }

    fn turn_record(players: &[(Move, Outcome, Option<u64>)]) -> TurnRecord {
        TurnRecord {
            turn: 1,
            players: players
                .iter()
                .map(|&(mv, outcome, latency_us)| PlayerTurnRecord {
                    requested: Some(mv),
                    applied: mv,
                    outcome,
                    latency_us,
                    think_us: None,
                    pos: Pos::new(0, 0),
                    gained: 0,
                    half_points: 0,
                    log: Vec::new(),
                    marks: Vec::new(),
                    error: None,
                    invalid: None,
                })
                .collect(),
            collected: Vec::new(),
        }
    }

    #[test]
    fn solo_start() {
        let game = Game::new(Maze::parse(RING).unwrap(), Seating::Solo, 100);
        let names = names(1);
        let frame = Frame {
            snapshot: game.snapshot(),
            names: &names,
            max_turns: 100,
            last: None,
            marks: &[],
        };
        let want = "\
turn 0/100 · 14 points left
A greedy       0
A · · · ·
· ██████·
· ██████·
· ██████·
· · · ·
";
        assert_eq!(render(&frame, Style::default()), want);
    }

    #[test]
    fn duo_shared_cell_marks_and_header() {
        let mut game = Game::new(Maze::parse(RING).unwrap(), Seating::Duo, 100);
        for (a, b) in [(Move::Right, Move::Up); 4] {
            game.step(&[Input::Move(a), Input::Move(b)]);
        }
        let names = names(2);
        let last = turn_record(&[
            (Move::Right, Outcome::Ok, Some(1_234)),
            (Move::Up, Outcome::Illegal, Some(850)),
        ]);
        let marks = [Mark {
            cells: vec![Pos::new(0, 1), Pos::new(0, 2)],
            label: None,
            color: None,
            kind: MarkKind::Cells,
        }];
        let frame = Frame {
            snapshot: game.snapshot(),
            names: &names,
            max_turns: 100,
            last: Some(&last),
            marks: &marks,
        };
        let want = "\
turn 4/100 · 7 points left
A greedy     3.5  right  ok         1.2 ms
B random     3.5  up     illegal    850 µs
        X
∘ ██████
∘ ██████
· ██████
· · · ·
";
        assert_eq!(render(&frame, Style::default()), want);
    }

    #[test]
    fn ascii_and_color() {
        let game = Game::new(Maze::parse(RING).unwrap(), Seating::Duo, 100);
        let names = names(2);
        let frame = Frame {
            snapshot: game.snapshot(),
            names: &names,
            max_turns: 100,
            last: None,
            marks: &[],
        };
        let ascii = render(
            &frame,
            Style {
                ascii: true,
                color: false,
            },
        );
        assert!(ascii.is_ascii());
        assert!(ascii.contains("A . . . .\n. ######.\n"));
        assert!(ascii.ends_with(". . . . B\n"));
        let color = render(
            &frame,
            Style {
                ascii: false,
                color: true,
            },
        );
        assert!(color.contains("\x1b[1;32mA \x1b[0m"));
        assert!(color.contains("\x1b[1;31mB \x1b[0m"));
    }

    #[test]
    fn marks_take_their_colour_and_kind() {
        let game = Game::new(Maze::parse(RING).unwrap(), Seating::Solo, 100);
        let names = names(1);
        let marks = [
            Mark {
                cells: vec![Pos::new(1, 0), Pos::new(2, 0), Pos::new(3, 0)],
                label: Some("route".to_owned()),
                color: Some(MarkColor::Teal),
                kind: MarkKind::Path,
            },
            Mark {
                cells: vec![Pos::new(4, 2), Pos::new(3, 0)],
                label: None,
                color: Some(MarkColor::Pink),
                kind: MarkKind::Cells,
            },
        ];
        let frame = Frame {
            snapshot: game.snapshot(),
            names: &names,
            max_turns: 100,
            last: None,
            marks: &marks,
        };
        let plain = render(&frame, Style::default());
        // The later mark wins (3, 0).
        assert!(plain.contains("A • • ∘ ·\n"), "{plain}");
        assert!(plain.contains("· ██████∘\n"), "{plain}");
        let ascii = render(
            &frame,
            Style {
                ascii: true,
                color: false,
            },
        );
        assert!(ascii.contains("A * * + .\n"), "{ascii}");
        let color = render(
            &frame,
            Style {
                ascii: false,
                color: true,
            },
        );
        assert!(color.contains("\x1b[38;5;30m• \x1b[0m"), "{color}");
        assert!(color.contains("\x1b[38;5;213m∘ \x1b[0m"), "{color}");
    }

    #[test]
    fn durations() {
        let style = Style::default();
        assert_eq!(format_us(0, style), "0 µs");
        assert_eq!(format_us(999, style), "999 µs");
        assert_eq!(format_us(1_000, style), "1.0 ms");
        assert_eq!(format_us(99_999, style), "99.9 ms");
        assert_eq!(format_us(3_456_789, style), "3.45 s");
        let ascii = Style {
            ascii: true,
            color: false,
        };
        assert_eq!(format_us(5, ascii), "5 us");
    }
}
