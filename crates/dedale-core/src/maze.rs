//! Validated mazes.

use std::collections::VecDeque;
use std::fmt;
use std::ops::Deref;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::grid::{Cell, Grid, GridError, MAX_SIZE, Pos};

/// Smallest width or height of a maze.
pub const MIN_SIZE: u8 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MazeError {
    #[error("the maze is {width}×{height}; both sides must be odd, from {MIN_SIZE} to {MAX_SIZE}")]
    BadSize { width: u8, height: u8 },
    #[error("the maze is not symmetric: {pos} and {twin} differ")]
    Asymmetric { pos: Pos, twin: Pos },
    #[error("start {pos} is a wall")]
    StartIsWall { pos: Pos },
    #[error("start {pos} holds a point")]
    StartHasPoint { pos: Pos },
    #[error("{pos} cannot be reached from the starts")]
    Unreachable { pos: Pos },
}

/// Why a map file was rejected.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MapError {
    #[error(transparent)]
    Grid(#[from] GridError),
    #[error(transparent)]
    Maze(#[from] MazeError),
}

/// A grid that is a playable maze: odd sides within bounds, symmetric under
/// the 180° rotation, open starts without points, every open cell reachable.
///
/// Serialized as `{"width", "height", "cells"}` with flat cells, and
/// validated when deserialized.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "MazeRepr", into = "MazeRepr")]
pub struct Maze {
    grid: Grid,
}

#[derive(Serialize, Deserialize)]
struct MazeRepr {
    width: u8,
    height: u8,
    cells: String,
}

impl From<Maze> for MazeRepr {
    fn from(maze: Maze) -> Self {
        Self {
            width: maze.width(),
            height: maze.height(),
            cells: maze.to_flat(),
        }
    }
}

impl TryFrom<MazeRepr> for Maze {
    type Error = MapError;

    fn try_from(repr: MazeRepr) -> Result<Self, MapError> {
        let grid = Grid::from_flat(repr.width, repr.height, &repr.cells)?;
        Ok(Maze::new(grid)?)
    }
}

impl Maze {
    /// # Errors
    ///
    /// The first rule `grid` breaks, checked in the order of [`MazeError`].
    pub fn new(grid: Grid) -> Result<Self, MazeError> {
        let (width, height) = (grid.width(), grid.height());
        if width < MIN_SIZE
            || height < MIN_SIZE
            || width.is_multiple_of(2)
            || height.is_multiple_of(2)
        {
            return Err(MazeError::BadSize { width, height });
        }
        if let Some(pos) = grid
            .positions()
            .find(|&p| grid.get(p) != grid.get(grid.rot(p)))
        {
            let twin = grid.rot(pos);
            return Err(MazeError::Asymmetric { pos, twin });
        }
        let starts = starts(&grid);
        for pos in starts {
            match grid.get(pos) {
                Cell::Wall => return Err(MazeError::StartIsWall { pos }),
                Cell::Point => return Err(MazeError::StartHasPoint { pos }),
                Cell::Empty => {}
            }
        }
        if let Some(pos) = first_unreachable(&grid, starts[0]) {
            return Err(MazeError::Unreachable { pos });
        }
        Ok(Self { grid })
    }

    /// Parses the multi-line text format and validates the result.
    ///
    /// # Errors
    ///
    /// On a malformed grid or a grid that is not a valid maze.
    pub fn parse(text: &str) -> Result<Self, MapError> {
        Ok(Self::new(Grid::parse(text)?)?)
    }

    #[must_use]
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    #[must_use]
    pub fn into_grid(self) -> Grid {
        self.grid
    }

    /// Player A's start `(0, 0)` and player B's `(W-1, H-1)`.
    #[must_use]
    pub fn starts(&self) -> [Pos; 2] {
        starts(&self.grid)
    }
}

impl Deref for Maze {
    type Target = Grid;

    fn deref(&self) -> &Grid {
        &self.grid
    }
}

impl fmt::Display for Maze {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.grid.fmt(f)
    }
}

fn starts(grid: &Grid) -> [Pos; 2] {
    let a = Pos::new(0, 0);
    [a, grid.rot(a)]
}

/// The first open cell, row-major, that a breadth-first search from `start`
/// does not reach.
fn first_unreachable(grid: &Grid, start: Pos) -> Option<Pos> {
    let mut seen = vec![false; grid.cells().len()];
    let mut queue = VecDeque::from([start]);
    seen[grid.index(start)] = true;
    while let Some(pos) = queue.pop_front() {
        for (_, next) in grid.open_neighbors(pos) {
            let index = grid.index(next);
            if !seen[index] {
                seen[index] = true;
                queue.push_back(next);
            }
        }
    }
    grid.positions()
        .find(|&p| grid.get(p).is_open() && !seen[grid.index(p)])
}

#[cfg(test)]
mod tests {
    use super::{MapError, Maze, MazeError};
    use crate::grid::{GridError, Pos};

    const OPEN: &str = "\
_....
.#.#.
.....
.#.#.
...._
";

    #[test]
    fn accepts_a_valid_maze() {
        let maze = Maze::parse(OPEN).unwrap();
        assert_eq!(maze.starts(), [Pos::new(0, 0), Pos::new(4, 4)]);
        assert_eq!(maze.point_count(), 19);
        assert_eq!(maze.to_string(), OPEN);
    }

    #[test]
    fn rejects_bad_sizes() {
        assert_eq!(
            Maze::parse("_._\n.#.\n._.\n"),
            Err(MapError::Maze(MazeError::BadSize {
                width: 3,
                height: 3
            }))
        );
        let even = "_.....\n.#..#.\n......\n.#..#.\n....._\n";
        assert_eq!(
            Maze::parse(even),
            Err(MapError::Maze(MazeError::BadSize {
                width: 6,
                height: 5
            }))
        );
    }

    #[test]
    fn rejects_asymmetry() {
        let text = OPEN.replacen("_....", "_.#..", 1);
        assert_eq!(
            Maze::parse(&text),
            Err(MapError::Maze(MazeError::Asymmetric {
                pos: Pos::new(2, 0),
                twin: Pos::new(2, 4)
            }))
        );
    }

    #[test]
    fn rejects_bad_starts() {
        let wall = OPEN.replace('_', "#");
        assert_eq!(
            Maze::parse(&wall),
            Err(MapError::Maze(MazeError::StartIsWall {
                pos: Pos::new(0, 0)
            }))
        );
        let point = OPEN.replace('_', ".");
        assert_eq!(
            Maze::parse(&point),
            Err(MapError::Maze(MazeError::StartHasPoint {
                pos: Pos::new(0, 0)
            }))
        );
    }

    #[test]
    fn rejects_unreachable_cells() {
        let enclosed = "\
_....
.###.
.#.#.
.###.
...._
";
        assert_eq!(
            Maze::parse(enclosed),
            Err(MapError::Maze(MazeError::Unreachable {
                pos: Pos::new(2, 2)
            }))
        );
    }

    #[test]
    fn serde_validates() {
        let maze = Maze::parse(OPEN).unwrap();
        let json = serde_json::to_string(&maze).unwrap();
        assert_eq!(
            json,
            r#"{"width":5,"height":5,"cells":"_.....#.#.......#.#....._"}"#
        );
        assert_eq!(serde_json::from_str::<Maze>(&json).unwrap(), maze);
        let asymmetric = json.replacen("_....", "_.#..", 1);
        assert!(serde_json::from_str::<Maze>(&asymmetric).is_err());
    }

    #[test]
    fn reports_grid_errors() {
        assert_eq!(Maze::parse(""), Err(MapError::Grid(GridError::Empty)));
    }
}
