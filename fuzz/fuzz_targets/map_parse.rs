//! Arbitrary text through the map parser and maze validation.

#![no_main]

use dedale_core::grid::Grid;
use dedale_core::maze::Maze;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(grid) = Grid::parse(text) else {
        return;
    };
    let formatted = grid.to_string();
    assert!(formatted == text || formatted.strip_suffix('\n') == Some(text));
    let flat = Grid::from_flat(grid.width(), grid.height(), &grid.to_flat());
    assert_eq!(flat.as_ref(), Ok(&grid));

    if let Ok(maze) = Maze::new(grid) {
        for pos in maze.positions() {
            assert_eq!(maze.get(pos), maze.get(maze.rot(pos)));
        }
        for start in maze.starts() {
            assert!(maze.get(start).is_open());
        }
        assert_eq!(Maze::parse(text).as_ref(), Ok(&maze));
    }
});
