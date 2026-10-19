//! Arbitrary maze codes through the generator.

#![no_main]

use dedale_core::code::{ALL_POINTS, MazeCode, MazeParams};
use dedale_core::generator::{generate, generate_normalized};
use dedale_core::grid::Cell;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&[w, h, loops, p0, p1], seed)) = data.split_first_chunk::<5>() else {
        return;
    };
    // Up to 101×101 keeps runs fast; proptest and a unit test cover 255×255.
    let side = |b: u8| 5 + 2 * u32::from(b % 49);
    let points = u32::from(u16::from_le_bytes([p0, p1]));
    let params = MazeParams::new(side(w), side(h), u32::from(loops % 101), points)
        .expect("in range by construction");
    let mut seed_bytes = [0; 8];
    for (dst, src) in seed_bytes.iter_mut().zip(seed) {
        *dst = *src;
    }
    let code = MazeCode::new(params, u64::from_le_bytes(seed_bytes));

    let (maze, normalized) = generate_normalized(&code);
    assert_eq!(maze.width(), params.width());
    assert_eq!(maze.height(), params.height());
    for pos in maze.positions() {
        assert_eq!(maze.get(pos), maze.get(maze.rot(pos)));
    }
    for start in maze.starts() {
        assert_eq!(maze.get(start), Cell::Empty);
    }
    // Capped, never more than asked; everything when asked for everything.
    assert!(maze.point_count() <= usize::from(params.points()));
    if params.points() == ALL_POINTS {
        let open = maze.cells().iter().filter(|c| c.is_open()).count();
        assert_eq!(maze.point_count(), open - 2);
    }
    assert_eq!(
        usize::from(normalized.params().points()),
        maze.point_count()
    );
    assert_eq!(generate(&normalized), maze);
    assert_eq!(generate(&code), maze);
});
