//! The symmetric maze generator.
//!
//! Rooms are the cells with both coordinates even, pillars the cells with both
//! odd (always walls), and the remaining cells are edges joining two rooms.
//! With odd sides, the 180° rotation maps rooms to rooms and edges to edges.
//!
//! The output depends only on the maze code: every random draw comes from one
//! stream, in a fixed order. Changing that order or any step changes the
//! output and requires bumping [`crate::code::CODE_VERSION`].

use crate::code::MazeCode;
use crate::grid::{Cell, Grid, Move, Pos};
use crate::maze::Maze;
use crate::rng::Rng;

/// The maze a code describes.
///
/// # Panics
///
/// Only on a generator bug: the result is validated as a [`Maze`].
#[must_use]
pub fn generate(code: &MazeCode) -> Maze {
    let params = code.params();
    let mut rng = Rng::from_seed(code.seed());
    let mut grid = Grid::filled(params.width(), params.height(), Cell::Wall);
    let rooms: Vec<Pos> = grid.positions().filter(|&p| is_room(p)).collect();
    for room in rooms {
        grid.set(room, Cell::Empty);
    }
    connect(&mut grid, &mut rng);
    open_loops(&mut grid, &mut rng, params.loops());
    place_points(&mut grid, &mut rng, params.density());
    Maze::new(grid).expect("the generator builds valid mazes")
}

fn is_room(pos: Pos) -> bool {
    pos.x.is_multiple_of(2) && pos.y.is_multiple_of(2)
}

fn is_edge(pos: Pos) -> bool {
    pos.x % 2 != pos.y % 2
}

/// The two rooms an edge separates. Edges never touch the border on their
/// odd axis, so both rooms are inside the grid.
fn edge_rooms(edge: Pos) -> (Pos, Pos) {
    if edge.x % 2 == 1 {
        (Pos::new(edge.x - 1, edge.y), Pos::new(edge.x + 1, edge.y))
    } else {
        (Pos::new(edge.x, edge.y - 1), Pos::new(edge.x, edge.y + 1))
    }
}

/// True for the cell of its orbit `{pos, rot(pos)}` that comes first
/// row-major.
fn is_orbit_representative(grid: &Grid, pos: Pos) -> bool {
    grid.index(pos) <= grid.index(grid.rot(pos))
}

/// Step 1: symmetric randomized Kruskal. Each edge orbit is opened when either
/// of its edges joins two components, so the result is symmetric, connected,
/// and may already hold a few loops.
fn connect(grid: &mut Grid, rng: &mut Rng) {
    let mut orbits: Vec<Pos> = grid
        .positions()
        .filter(|&p| is_edge(p) && is_orbit_representative(grid, p))
        .collect();
    rng.shuffle(&mut orbits);
    let mut sets = DisjointSets::new(grid.cells().len());
    for edge in orbits {
        let twin = grid.rot(edge);
        if joins(&mut sets, grid, edge) || joins(&mut sets, grid, twin) {
            for e in [edge, twin] {
                let (a, b) = edge_rooms(e);
                sets.union(grid.index(a), grid.index(b));
                grid.set(e, Cell::Empty);
            }
        }
    }
}

fn joins(sets: &mut DisjointSets, grid: &Grid, edge: Pos) -> bool {
    let (a, b) = edge_rooms(edge);
    sets.find(grid.index(a)) != sets.find(grid.index(b))
}

/// Step 2: dead-end rooms, one per orbit in row-major order, are opened with
/// probability `loops`% through a random closed edge, mirrored. Opening edges
/// never creates a dead end, so `loops = 100` leaves none.
fn open_loops(grid: &mut Grid, rng: &mut Rng, loops: u8) {
    let rooms: Vec<Pos> = grid
        .positions()
        .filter(|&p| is_room(p) && is_orbit_representative(grid, p))
        .collect();
    for room in rooms {
        if grid.open_neighbors(room).count() != 1 || !rng.chance(loops) {
            continue;
        }
        // A room's in-bounds neighbours are edges, each leading to another
        // room. Mazes are at least 5×5, so every room has two of them.
        let closed: Vec<Pos> = [Move::Up, Move::Down, Move::Left, Move::Right]
            .into_iter()
            .filter_map(|mv| grid.neighbor(room, mv))
            .filter(|&edge| grid.get(edge) == Cell::Wall)
            .collect();
        let edge = closed[rng.index(closed.len())];
        let twin = grid.rot(edge);
        grid.set(edge, Cell::Empty);
        grid.set(twin, Cell::Empty);
    }
}

/// Step 3: each orbit of open cells except the starts holds points with
/// probability `density`%.
fn place_points(grid: &mut Grid, rng: &mut Rng, density: u8) {
    let start = Pos::new(0, 0);
    let cells: Vec<Pos> = grid
        .positions()
        .filter(|&p| p != start && grid.get(p).is_open() && is_orbit_representative(grid, p))
        .collect();
    for pos in cells {
        if rng.chance(density) {
            let twin = grid.rot(pos);
            grid.set(pos, Cell::Point);
            grid.set(twin, Cell::Point);
        }
    }
}

/// Union–find over grid indices, with path halving.
struct DisjointSets {
    parent: Vec<usize>,
}

impl DisjointSets {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
        }
    }

    fn find(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        self.parent[ra] = rb;
    }
}

#[cfg(test)]
mod tests {
    use super::{generate, is_edge, is_room};
    use crate::code::{MazeCode, MazeParams};
    use crate::grid::Cell;
    use crate::maze::Maze;
    use proptest::prelude::*;

    fn side() -> impl Strategy<Value = u32> {
        prop_oneof![
            9 => (2u32..=20).prop_map(|n| 2 * n + 1),
            1 => (21u32..=127).prop_map(|n| 2 * n + 1),
        ]
    }

    fn code() -> impl Strategy<Value = MazeCode> {
        (side(), side(), 0u32..=100, 0u32..=100, any::<u64>()).prop_map(|(w, h, l, p, seed)| {
            MazeCode::new(MazeParams::new(w, h, l, p).unwrap(), seed)
        })
    }

    fn with(code: MazeCode, loops: u32, density: u32) -> MazeCode {
        let p = code.params();
        let params = MazeParams::new(p.width().into(), p.height().into(), loops, density);
        MazeCode::new(params.unwrap(), code.seed())
    }

    fn dead_ends(maze: &Maze) -> usize {
        maze.positions()
            .filter(|&p| maze.get(p).is_open() && maze.open_neighbors(p).count() == 1)
            .count()
    }

    #[test]
    fn largest_maze() {
        let code = MazeCode::new(MazeParams::new(255, 255, 25, 100).unwrap(), 1);
        let maze = generate(&code);
        assert_eq!((maze.width(), maze.height()), (255, 255));
    }

    #[test]
    fn density_is_roughly_honoured() {
        let code: MazeCode = "ded1-101x101-l25-p40-0000000000000007".parse().unwrap();
        let maze = generate(&code);
        let open = maze.cells().iter().filter(|c| c.is_open()).count();
        let points = maze.point_count();
        assert!(
            (35 * open..45 * open).contains(&(100 * points)),
            "{points}/{open}"
        );
    }

    proptest! {
        #[test]
        fn mazes_have_the_requested_shape(code in code()) {
            let maze = generate(&code);
            let params = code.params();
            prop_assert_eq!((maze.width(), maze.height()), (params.width(), params.height()));
            for pos in maze.positions() {
                let cell = maze.get(pos);
                if is_room(pos) {
                    prop_assert!(cell.is_open(), "room {} is a wall", pos);
                } else if !is_edge(pos) {
                    prop_assert_eq!(cell, Cell::Wall, "pillar {} is open", pos);
                }
            }
        }

        #[test]
        fn generation_is_deterministic(code in code()) {
            prop_assert_eq!(generate(&code), generate(&code));
        }

        #[test]
        fn full_density_fills_every_open_cell(code in code()) {
            let maze = generate(&with(code, code.params().loops().into(), 100));
            let starts = maze.starts();
            for pos in maze.positions() {
                let want = match maze.get(pos) {
                    Cell::Wall => Cell::Wall,
                    _ if starts.contains(&pos) => Cell::Empty,
                    _ => Cell::Point,
                };
                prop_assert_eq!(maze.get(pos), want, "at {}", pos);
            }
        }

        #[test]
        fn zero_density_places_no_points(code in code()) {
            let maze = generate(&with(code, code.params().loops().into(), 0));
            prop_assert_eq!(maze.point_count(), 0);
        }

        #[test]
        fn all_loops_leave_no_dead_ends(code in code()) {
            let maze = generate(&with(code, 100, code.params().density().into()));
            prop_assert_eq!(dead_ends(&maze), 0);
        }

        // Density only places points: the walls do not depend on it.
        #[test]
        fn density_does_not_move_walls(code in code(), density in 0u32..=100) {
            let a = generate(&code);
            let b = generate(&with(code, code.params().loops().into(), density));
            for pos in a.positions() {
                prop_assert_eq!(a.get(pos).is_open(), b.get(pos).is_open());
            }
        }
    }
}
