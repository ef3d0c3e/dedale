//! Cells, positions, moves and the raw grid with its text format.
//!
//! A [`Grid`] is any rectangle of cells. [`crate::maze::Maze`] is a grid that
//! passed validation.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Largest width or height of a grid.
pub const MAX_SIZE: u8 = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cell {
    Wall,
    Empty,
    Point,
}

impl Cell {
    #[must_use]
    pub fn is_open(self) -> bool {
        self != Cell::Wall
    }

    /// The character used by the text format: `#` wall, `_` empty, `.` point.
    #[must_use]
    pub fn to_char(self) -> char {
        match self {
            Cell::Wall => '#',
            Cell::Empty => '_',
            Cell::Point => '.',
        }
    }

    #[must_use]
    pub fn from_char(ch: char) -> Option<Self> {
        match ch {
            '#' => Some(Cell::Wall),
            '_' => Some(Cell::Empty),
            '.' => Some(Cell::Point),
            _ => None,
        }
    }
}

/// A cell position: origin top-left, `x` to the right, `y` down.
///
/// Serialized as `[x, y]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "(u8, u8)", into = "(u8, u8)")]
pub struct Pos {
    pub x: u8,
    pub y: u8,
}

impl Pos {
    #[must_use]
    pub const fn new(x: u8, y: u8) -> Self {
        Self { x, y }
    }
}

impl From<(u8, u8)> for Pos {
    fn from((x, y): (u8, u8)) -> Self {
        Self { x, y }
    }
}

impl From<Pos> for (u8, u8) {
    fn from(pos: Pos) -> Self {
        (pos.x, pos.y)
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

/// What a character does in one turn. Serialized in lowercase (`"up"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Move {
    Up,
    Down,
    Left,
    Right,
    Stay,
}

impl Move {
    pub const ALL: [Move; 5] = [Move::Up, Move::Down, Move::Left, Move::Right, Move::Stay];

    /// `(dx, dy)`, with `y` growing downwards.
    #[must_use]
    pub const fn delta(self) -> (i8, i8) {
        match self {
            Move::Up => (0, -1),
            Move::Down => (0, 1),
            Move::Left => (-1, 0),
            Move::Right => (1, 0),
            Move::Stay => (0, 0),
        }
    }

    /// The same move seen through the maze's 180° rotation.
    #[must_use]
    pub const fn rotated(self) -> Self {
        match self {
            Move::Up => Move::Down,
            Move::Down => Move::Up,
            Move::Left => Move::Right,
            Move::Right => Move::Left,
            Move::Stay => Move::Stay,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Move::Up => "up",
            Move::Down => "down",
            Move::Left => "left",
            Move::Right => "right",
            Move::Stay => "stay",
        }
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown move {0:?}; expected up, down, left, right or stay")]
pub struct UnknownMove(pub String);

impl FromStr for Move {
    type Err = UnknownMove;

    /// Strict: lowercase wire names only.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Move::ALL
            .into_iter()
            .find(|m| m.as_str() == s)
            .ok_or_else(|| UnknownMove(s.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum GridError {
    #[error("the grid is empty")]
    Empty,
    #[error("the grid is {width}×{height}; sides are at most {MAX_SIZE}")]
    TooLarge { width: usize, height: usize },
    #[error("row {y} has {len} cells, expected {expected}")]
    RaggedRow {
        y: usize,
        len: usize,
        expected: usize,
    },
    #[error("unknown cell {ch:?} at ({x}, {y}); expected '#', '.' or '_'")]
    BadCell { x: usize, y: usize, ch: char },
    #[error("expected {expected} cells, got {len}")]
    WrongLength { expected: usize, len: usize },
}

/// A rectangle of cells, stored row-major.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Grid {
    width: u8,
    height: u8,
    cells: Vec<Cell>,
}

impl Grid {
    /// # Panics
    ///
    /// If a side is 0.
    #[must_use]
    pub fn filled(width: u8, height: u8, cell: Cell) -> Self {
        assert!(width > 0 && height > 0, "empty grid");
        Self {
            width,
            height,
            cells: vec![cell; usize::from(width) * usize::from(height)],
        }
    }

    /// # Errors
    ///
    /// If a side is 0 or `cells` does not hold `width × height` cells.
    pub fn from_cells(width: u8, height: u8, cells: Vec<Cell>) -> Result<Self, GridError> {
        if width == 0 || height == 0 {
            return Err(GridError::Empty);
        }
        let expected = usize::from(width) * usize::from(height);
        if cells.len() != expected {
            return Err(GridError::WrongLength {
                expected,
                len: cells.len(),
            });
        }
        Ok(Self {
            width,
            height,
            cells,
        })
    }

    #[must_use]
    pub fn width(&self) -> u8 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u8 {
        self.height
    }

    /// Row-major cells.
    #[must_use]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    #[must_use]
    pub fn contains(&self, pos: Pos) -> bool {
        pos.x < self.width && pos.y < self.height
    }

    /// Row-major index of `pos`, which must be inside the grid.
    #[must_use]
    pub fn index(&self, pos: Pos) -> usize {
        debug_assert!(self.contains(pos), "{pos} outside the grid");
        usize::from(pos.y) * usize::from(self.width) + usize::from(pos.x)
    }

    /// The position of a row-major index.
    ///
    /// # Panics
    ///
    /// If `index` is outside the grid.
    #[must_use]
    pub fn pos(&self, index: usize) -> Pos {
        assert!(index < self.cells.len(), "index {index} outside the grid");
        let width = usize::from(self.width);
        Pos::new(narrow(index % width), narrow(index / width))
    }

    /// # Panics
    ///
    /// If `pos` is outside the grid.
    #[must_use]
    pub fn get(&self, pos: Pos) -> Cell {
        assert!(self.contains(pos), "{pos} outside the grid");
        self.cells[self.index(pos)]
    }

    /// # Panics
    ///
    /// If `pos` is outside the grid.
    pub fn set(&mut self, pos: Pos, cell: Cell) {
        assert!(self.contains(pos), "{pos} outside the grid");
        let index = self.index(pos);
        self.cells[index] = cell;
    }

    /// Every position, row-major.
    pub fn positions(&self) -> impl Iterator<Item = Pos> + use<> {
        let (width, height) = (self.width, self.height);
        (0..height).flat_map(move |y| (0..width).map(move |x| Pos::new(x, y)))
    }

    /// The cell next to `pos` in direction `mv`, if inside the grid (walls
    /// included).
    #[must_use]
    pub fn neighbor(&self, pos: Pos, mv: Move) -> Option<Pos> {
        let (dx, dy) = mv.delta();
        let x = pos.x.checked_add_signed(dx)?;
        let y = pos.y.checked_add_signed(dy)?;
        let next = Pos::new(x, y);
        self.contains(next).then_some(next)
    }

    /// Where `mv` takes a character standing on `pos`: `None` when it would
    /// leave the grid or enter a wall.
    #[must_use]
    pub fn step(&self, pos: Pos, mv: Move) -> Option<Pos> {
        self.neighbor(pos, mv)
            .filter(|&next| self.get(next).is_open())
    }

    /// The legal non-stay moves from `pos` and where they lead.
    pub fn open_neighbors(&self, pos: Pos) -> impl Iterator<Item = (Move, Pos)> + '_ {
        [Move::Up, Move::Down, Move::Left, Move::Right]
            .into_iter()
            .filter_map(move |mv| self.step(pos, mv).map(|next| (mv, next)))
    }

    /// The image of `pos` under the 180° rotation.
    #[must_use]
    pub fn rot(&self, pos: Pos) -> Pos {
        debug_assert!(self.contains(pos), "{pos} outside the grid");
        Pos::new(self.width - 1 - pos.x, self.height - 1 - pos.y)
    }

    /// Parses the multi-line text format: one line per row, the final newline
    /// optional.
    ///
    /// # Errors
    ///
    /// The first problem in reading order.
    pub fn parse(text: &str) -> Result<Self, GridError> {
        let body = text.strip_suffix('\n').unwrap_or(text);
        let rows: Vec<&str> = body.split('\n').collect();
        let width = rows[0].chars().count();
        if width == 0 {
            return Err(GridError::Empty);
        }
        let (Ok(w), Ok(h)) = (u8::try_from(width), u8::try_from(rows.len())) else {
            return Err(GridError::TooLarge {
                width,
                height: rows.len(),
            });
        };
        let mut cells = Vec::with_capacity(width * rows.len());
        for (y, row) in rows.iter().enumerate() {
            let mut len = 0;
            for (x, ch) in row.chars().enumerate() {
                cells.push(Cell::from_char(ch).ok_or(GridError::BadCell { x, y, ch })?);
                len += 1;
            }
            if len != width {
                return Err(GridError::RaggedRow {
                    y,
                    len,
                    expected: width,
                });
            }
        }
        Self::from_cells(w, h, cells)
    }

    /// Parses the flat format used by the protocol: `width × height`
    /// characters, row-major, no newlines.
    ///
    /// # Errors
    ///
    /// On an empty side, a wrong length or an unknown character.
    pub fn from_flat(width: u8, height: u8, flat: &str) -> Result<Self, GridError> {
        let w = usize::from(width);
        let cells = flat
            .chars()
            .enumerate()
            .map(|(i, ch)| {
                Cell::from_char(ch).ok_or(GridError::BadCell {
                    x: i % w.max(1),
                    y: i / w.max(1),
                    ch,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_cells(width, height, cells)
    }

    /// The flat format: row-major characters, no newlines.
    #[must_use]
    pub fn to_flat(&self) -> String {
        self.cells.iter().map(|c| c.to_char()).collect()
    }

    /// Number of cells holding a point.
    #[must_use]
    pub fn point_count(&self) -> usize {
        self.cells.iter().filter(|&&c| c == Cell::Point).count()
    }
}

/// Multi-line text format, every row followed by a newline.
impl fmt::Display for Grid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for row in self.cells.chunks(usize::from(self.width)) {
            let line: String = row.iter().map(|c| c.to_char()).collect();
            writeln!(f, "{line}")?;
        }
        Ok(())
    }
}

impl FromStr for Grid {
    type Err = GridError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Coordinates are below [`MAX_SIZE`] by construction.
fn narrow(n: usize) -> u8 {
    u8::try_from(n).expect("grid coordinates fit in u8")
}

#[cfg(test)]
mod tests {
    use super::{Cell, Grid, GridError, Move, Pos};
    use proptest::prelude::*;

    fn cell() -> impl Strategy<Value = Cell> {
        prop_oneof![Just(Cell::Wall), Just(Cell::Empty), Just(Cell::Point)]
    }

    fn grid() -> impl Strategy<Value = Grid> {
        (1u8..=12, 1u8..=12).prop_flat_map(|(w, h)| {
            prop::collection::vec(cell(), usize::from(w) * usize::from(h))
                .prop_map(move |cells| Grid::from_cells(w, h, cells).unwrap())
        })
    }

    fn grid_and_pos() -> impl Strategy<Value = (Grid, Pos)> {
        grid().prop_flat_map(|g| {
            let (w, h) = (g.width(), g.height());
            (Just(g), (0..w, 0..h).prop_map(|(x, y)| Pos::new(x, y)))
        })
    }

    #[test]
    fn parses_text() {
        let grid = Grid::parse("#._\n_.#\n").unwrap();
        assert_eq!((grid.width(), grid.height()), (3, 2));
        assert_eq!(grid.get(Pos::new(0, 0)), Cell::Wall);
        assert_eq!(grid.get(Pos::new(1, 0)), Cell::Point);
        assert_eq!(grid.get(Pos::new(0, 1)), Cell::Empty);
        assert_eq!(grid.point_count(), 2);
        assert_eq!(Grid::parse("#._\n_.#"), Ok(grid.clone()));
        assert_eq!(grid.to_string(), "#._\n_.#\n");
        assert_eq!(grid.to_flat(), "#.__.#");
    }

    #[test]
    fn parse_errors() {
        assert_eq!(Grid::parse(""), Err(GridError::Empty));
        assert_eq!(Grid::parse("\n"), Err(GridError::Empty));
        assert_eq!(
            Grid::parse("#.\n#\n"),
            Err(GridError::RaggedRow {
                y: 1,
                len: 1,
                expected: 2
            })
        );
        assert_eq!(
            Grid::parse("#.\n# \n"),
            Err(GridError::BadCell {
                x: 1,
                y: 1,
                ch: ' '
            })
        );
        assert_eq!(
            Grid::parse("#.\r\n#.\n"),
            Err(GridError::BadCell {
                x: 2,
                y: 0,
                ch: '\r'
            })
        );
        assert_eq!(
            Grid::parse(&"#".repeat(256)),
            Err(GridError::TooLarge {
                width: 256,
                height: 1
            })
        );
        assert_eq!(
            Grid::parse(&"#\n".repeat(256)),
            Err(GridError::TooLarge {
                width: 1,
                height: 256
            })
        );
        assert!(Grid::parse(&format!("{}\n", "#".repeat(255)).repeat(255)).is_ok());
    }

    #[test]
    fn flat_errors() {
        assert_eq!(
            Grid::from_flat(2, 2, "#.#"),
            Err(GridError::WrongLength {
                expected: 4,
                len: 3
            })
        );
        assert_eq!(
            Grid::from_flat(2, 2, "#.#x"),
            Err(GridError::BadCell {
                x: 1,
                y: 1,
                ch: 'x'
            })
        );
        assert_eq!(Grid::from_flat(0, 2, ""), Err(GridError::Empty));
        assert_eq!(Grid::from_flat(0, 2, "#"), Err(GridError::Empty));
    }

    #[test]
    fn moves() {
        assert_eq!("up".parse(), Ok(Move::Up));
        assert_eq!("stay".parse(), Ok(Move::Stay));
        assert!("Up".parse::<Move>().is_err());
        assert!("north".parse::<Move>().is_err());
        for mv in Move::ALL {
            assert_eq!(mv.to_string().parse(), Ok(mv));
            assert_eq!(mv.rotated().rotated(), mv);
            let (dx, dy) = mv.delta();
            assert_eq!(mv.rotated().delta(), (-dx, -dy));
        }
    }

    #[test]
    fn serde_forms() {
        assert_eq!(serde_json::to_string(&Pos::new(3, 7)).unwrap(), "[3,7]");
        assert_eq!(
            serde_json::from_str::<Pos>("[3,7]").unwrap(),
            Pos::new(3, 7)
        );
        assert_eq!(serde_json::to_string(&Move::Left).unwrap(), "\"left\"");
        assert_eq!(
            serde_json::from_str::<Move>("\"down\"").unwrap(),
            Move::Down
        );
        assert!(serde_json::from_str::<Move>("\"Down\"").is_err());
    }

    #[test]
    fn steps_and_neighbors() {
        let grid = Grid::parse("_#\n._\n").unwrap();
        let origin = Pos::new(0, 0);
        assert_eq!(grid.step(origin, Move::Up), None);
        assert_eq!(grid.step(origin, Move::Left), None);
        assert_eq!(grid.step(origin, Move::Right), None);
        assert_eq!(grid.step(origin, Move::Down), Some(Pos::new(0, 1)));
        assert_eq!(grid.step(origin, Move::Stay), Some(origin));
        assert_eq!(
            grid.open_neighbors(origin).collect::<Vec<_>>(),
            vec![(Move::Down, Pos::new(0, 1))]
        );
        assert_eq!(grid.rot(origin), Pos::new(1, 1));
    }

    proptest! {
        #[test]
        fn text_roundtrip(grid in grid()) {
            prop_assert_eq!(Grid::parse(&grid.to_string()), Ok(grid.clone()));
        }

        #[test]
        fn flat_roundtrip(grid in grid()) {
            prop_assert_eq!(
                Grid::from_flat(grid.width(), grid.height(), &grid.to_flat()),
                Ok(grid.clone())
            );
        }

        #[test]
        fn index_roundtrip((grid, pos) in grid_and_pos()) {
            prop_assert_eq!(grid.pos(grid.index(pos)), pos);
        }

        #[test]
        fn rotation_is_an_involution((grid, pos) in grid_and_pos()) {
            let twin = grid.rot(pos);
            prop_assert!(grid.contains(twin));
            prop_assert_eq!(grid.rot(twin), pos);
        }

        #[test]
        fn steps_stay_on_open_cells((grid, pos) in grid_and_pos()) {
            for mv in Move::ALL {
                if let Some(next) = grid.step(pos, mv) {
                    prop_assert!(grid.contains(next));
                    prop_assert!(grid.get(next).is_open());
                    let (dx, dy) = mv.delta();
                    prop_assert_eq!(i16::from(next.x) - i16::from(pos.x), i16::from(dx));
                    prop_assert_eq!(i16::from(next.y) - i16::from(pos.y), i16::from(dy));
                }
            }
        }

        // Rotating the position and the move commutes with stepping.
        #[test]
        fn neighbors_commute_with_rotation((grid, pos) in grid_and_pos()) {
            for mv in Move::ALL {
                let twin = grid.neighbor(grid.rot(pos), mv.rotated());
                prop_assert_eq!(grid.neighbor(pos, mv).map(|p| grid.rot(p)), twin);
            }
        }
    }
}
