//! Choosing a maze: a code, parameters, or a map file.

use std::fs;
use std::path::PathBuf;

use clap::Args;
use dedale_core::code::{MazeCode, MazeParams};
use dedale_core::generator::generate_normalized;
use dedale_core::maze::Maze;

use super::CliResult;

#[derive(Debug, Clone, Args)]
pub struct MazeArgs {
    /// Replay a maze from its code (printed at the start of every game).
    #[arg(long, value_name = "CODE", conflicts_with_all = ["size", "loops", "points", "seed", "map"])]
    pub maze: Option<MazeCode>,
    /// Maze size, odd sides from 5 to 255 [default: 31x31].
    #[arg(long, value_name = "WxH", value_parser = parse_size)]
    pub size: Option<(u32, u32)>,
    /// Share of dead ends opened into loops, in percent [default: 25].
    #[arg(long, value_name = "PERCENT")]
    pub loops: Option<u32>,
    /// Number of points; more than the maze holds means all of them
    /// [default: all].
    #[arg(long, value_name = "N")]
    pub points: Option<u32>,
    /// Seed, up to 16 hex digits [default: random].
    #[arg(long, value_name = "HEX", value_parser = parse_seed)]
    pub seed: Option<u64>,
    /// Load the maze from a text file (`#` wall, `.` point, `_` empty).
    #[arg(long, value_name = "FILE", conflicts_with_all = ["size", "loops", "points", "seed"])]
    pub map: Option<PathBuf>,
}

impl MazeArgs {
    /// The parameters these arguments describe (defaults filled in);
    /// meaningless with `--maze` or `--map`.
    pub(crate) fn params(&self) -> CliResult<MazeParams> {
        let d = MazeParams::default();
        let (width, height) = self.size.unwrap_or((d.width().into(), d.height().into()));
        Ok(MazeParams::new(
            width,
            height,
            self.loops.unwrap_or(d.loops().into()),
            self.points.unwrap_or(d.points().into()),
        )?)
    }

    /// The maze, and its normalized code (with the number of points
    /// actually placed) unless it came from a file.
    pub(crate) fn resolve(&self) -> CliResult<(Maze, Option<MazeCode>)> {
        if let Some(code) = self.maze {
            let (maze, code) = generate_normalized(&code);
            return Ok((maze, Some(code)));
        }
        if let Some(path) = &self.map {
            let text = fs::read_to_string(path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            let maze = Maze::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            return Ok((maze, None));
        }
        let params = self.params()?;
        let code = self.seed.map_or_else(
            || MazeCode::random(params),
            |seed| MazeCode::new(params, seed),
        );
        let (maze, code) = generate_normalized(&code);
        Ok((maze, Some(code)))
    }
}

/// `31x31`, or `31` for a square.
fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let side = |s: &str| {
        s.parse::<u32>()
            .map_err(|_| format!("bad size {text:?}; expected WxH"))
    };
    match text.split_once('x') {
        Some((w, h)) => Ok((side(w)?, side(h)?)),
        None => side(text).map(|n| (n, n)),
    }
}

fn parse_seed(text: &str) -> Result<u64, String> {
    if text.is_empty() || text.len() > 16 {
        return Err(format!("bad seed {text:?}; expected 1 to 16 hex digits"));
    }
    u64::from_str_radix(text, 16).map_err(|_| format!("bad seed {text:?}; expected hex digits"))
}

#[cfg(test)]
mod tests {
    use super::{parse_seed, parse_size};

    #[test]
    fn sizes_and_seeds() {
        assert_eq!(parse_size("31x21"), Ok((31, 21)));
        assert_eq!(parse_size("11"), Ok((11, 11)));
        assert!(parse_size("31x").is_err());
        assert!(parse_size("big").is_err());
        assert_eq!(parse_seed("ff"), Ok(255));
        assert_eq!(parse_seed("9f3a0c11d2e4b6a8"), Ok(0x9f3a_0c11_d2e4_b6a8));
        assert!(parse_seed("").is_err());
        assert!(parse_seed("0123456789abcdef0").is_err());
        assert!(parse_seed("xyz").is_err());
    }
}
