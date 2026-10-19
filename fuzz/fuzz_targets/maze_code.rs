//! Arbitrary strings through the maze code parser.

#![no_main]

use dedale_core::code::MazeCode;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(code) = text.parse::<MazeCode>() {
        assert_eq!(code.to_string(), text);
    }
});
