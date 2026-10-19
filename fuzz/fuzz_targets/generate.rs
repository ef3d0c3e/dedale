//! Arbitrary maze codes through the generator.

#![no_main]

use dedale_core::code::{MazeCode, MazeParams};
use dedale_core::generator::generate;
use dedale_core::grid::Cell;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&[w, h, loops, density], seed)) = data.split_first_chunk::<4>() else {
        return;
    };
    // Up to 101×101 keeps runs fast; proptest and a unit test cover 255×255.
    let side = |b: u8| 5 + 2 * u32::from(b % 49);
    let params = MazeParams::new(
        side(w),
        side(h),
        u32::from(loops % 101),
        u32::from(density % 101),
    )
    .expect("in range by construction");
    let mut seed_bytes = [0; 8];
    for (dst, src) in seed_bytes.iter_mut().zip(seed) {
        *dst = *src;
    }
    let code = MazeCode::new(params, u64::from_le_bytes(seed_bytes));

    let maze = generate(&code);
    assert_eq!(maze.width(), params.width());
    assert_eq!(maze.height(), params.height());
    for pos in maze.positions() {
        assert_eq!(maze.get(pos), maze.get(maze.rot(pos)));
    }
    if params.density() == 0 {
        assert_eq!(maze.point_count(), 0);
    }
    if params.density() == 100 {
        let open = maze.cells().iter().filter(|c| c.is_open()).count();
        assert_eq!(maze.point_count(), open - 2);
    }
    for start in maze.starts() {
        assert_eq!(maze.get(start), Cell::Empty);
    }
    assert_eq!(generate(&code), maze);
});
