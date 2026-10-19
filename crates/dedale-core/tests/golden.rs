//! Pins the generator's output: a maze code must mean the same maze forever.
//!
//! Run with `DEDALE_BLESS=1` to write the files after an intended change, which
//! must come with a `CODE_VERSION` bump.

use std::fs;
use std::path::Path;

use dedale_core::code::MazeCode;
use dedale_core::generator::generate;

const CODES: [&str; 5] = [
    "ded2-5x5-l0-p65535-0000000000000000",
    "ded2-11x7-l25-p65535-0123456789abcdef",
    "ded2-21x15-l100-p60-00000000deadbeef",
    "ded2-25x25-l25-p101-0000000000000007",
    "ded2-31x31-l25-p65535-9f3a0c11d2e4b6a8",
];

#[test]
fn generator_output_is_pinned() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let bless = std::env::var_os("DEDALE_BLESS").is_some();
    for code in CODES {
        let maze = generate(&code.parse::<MazeCode>().unwrap()).to_string();
        let path = dir.join(format!("{code}.txt"));
        if bless {
            fs::write(&path, &maze).unwrap();
            continue;
        }
        let want = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} (DEDALE_BLESS=1 writes it)", path.display()));
        assert_eq!(
            maze, want,
            "{code}: generator output changed; bump CODE_VERSION if this is intended"
        );
    }
}
