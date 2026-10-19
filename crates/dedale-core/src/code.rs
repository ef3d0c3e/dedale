//! Maze codes: the string that reproduces a maze.
//!
//! `ded1-31x31-l25-p100-9f3a0c11d2e4b6a8` is generator version 1, a 31×31
//! maze, 25% of dead ends opened into loops, points on 100% of the open
//! cells, and the 64-bit seed in hexadecimal.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::grid::MAX_SIZE;
use crate::maze::MIN_SIZE;
use crate::rng::fresh_seed;

/// The generator version this build implements. Any change to generator
/// output must bump it.
pub const CODE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CodeError {
    #[error(
        "malformed maze code {0:?}; expected ded{CODE_VERSION}-<W>x<H>-l<loops>-p<density>-<16 hex digits>"
    )]
    Malformed(String),
    #[error("unsupported maze code version {0:?}; this Dedale supports ded{CODE_VERSION}")]
    UnsupportedVersion(String),
    #[error(
        "a {width}×{height} maze is not allowed; both sides must be odd, from {MIN_SIZE} to {MAX_SIZE}"
    )]
    BadSize { width: u32, height: u32 },
    #[error("{name} is {value}%; it must be from 0 to 100")]
    BadPercent { name: &'static str, value: u32 },
}

/// Everything about a maze except its seed: shared by every game of a
/// tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MazeParams {
    width: u8,
    height: u8,
    loops: u8,
    density: u8,
}

impl MazeParams {
    /// # Errors
    ///
    /// If a side is even or outside `MIN_SIZE..=MAX_SIZE`, or a percentage
    /// is above 100.
    pub fn new(width: u32, height: u32, loops: u32, density: u32) -> Result<Self, CodeError> {
        let side = |n: u32| {
            u8::try_from(n)
                .ok()
                .filter(|&n| n >= MIN_SIZE && n % 2 == 1)
        };
        let (Some(w), Some(h)) = (side(width), side(height)) else {
            return Err(CodeError::BadSize { width, height });
        };
        let percent = |name, value: u32| {
            u8::try_from(value)
                .ok()
                .filter(|&p| p <= 100)
                .ok_or(CodeError::BadPercent { name, value })
        };
        Ok(Self {
            width: w,
            height: h,
            loops: percent("loops", loops)?,
            density: percent("density", density)?,
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

    /// Share of dead ends opened into loops, in percent.
    #[must_use]
    pub fn loops(&self) -> u8 {
        self.loops
    }

    /// Share of open cells holding a point, in percent.
    #[must_use]
    pub fn density(&self) -> u8 {
        self.density
    }
}

/// 31×31, 25% loops, points everywhere.
impl Default for MazeParams {
    fn default() -> Self {
        Self {
            width: 31,
            height: 31,
            loops: 25,
            density: 100,
        }
    }
}

/// Maze parameters plus a seed: everything needed to regenerate a maze.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MazeCode {
    params: MazeParams,
    seed: u64,
}

impl MazeCode {
    #[must_use]
    pub fn new(params: MazeParams, seed: u64) -> Self {
        Self { params, seed }
    }

    /// A code with a fresh seed.
    #[must_use]
    pub fn random(params: MazeParams) -> Self {
        Self::new(params, fresh_seed())
    }

    #[must_use]
    pub fn params(&self) -> MazeParams {
        self.params
    }

    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }
}

impl fmt::Display for MazeCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = &self.params;
        write!(
            f,
            "ded{CODE_VERSION}-{}x{}-l{}-p{}-{:016x}",
            p.width, p.height, p.loops, p.density, self.seed
        )
    }
}

impl FromStr for MazeCode {
    type Err = CodeError;

    /// Accepts the canonical form only (no leading zeros, lowercase hex), so
    /// a code that parses prints back identically.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let malformed = || CodeError::Malformed(s.to_owned());
        let parts: Vec<&str> = s.split('-').collect();
        let version = parts[0];
        if version != format!("ded{CODE_VERSION}") {
            let other_version = version
                .strip_prefix("ded")
                .is_some_and(|v| decimal(v).is_some());
            return Err(if other_version {
                CodeError::UnsupportedVersion(version.to_owned())
            } else {
                malformed()
            });
        }
        let &[_, size, loops, density, seed] = parts.as_slice() else {
            return Err(malformed());
        };
        let (width, height) = size.split_once('x').ok_or_else(malformed)?;
        let field = |text: &str, prefix: &str| text.strip_prefix(prefix).and_then(decimal);
        let width = decimal(width).ok_or_else(malformed)?;
        let height = decimal(height).ok_or_else(malformed)?;
        let loops = field(loops, "l").ok_or_else(malformed)?;
        let density = field(density, "p").ok_or_else(malformed)?;
        let seed = hex_seed(seed).ok_or_else(malformed)?;
        Ok(Self::new(
            MazeParams::new(width, height, loops, density)?,
            seed,
        ))
    }
}

/// A canonical decimal of at most 3 digits: `0`, or digits without a leading
/// zero.
fn decimal(s: &str) -> Option<u32> {
    let canonical = (1..=3).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_digit())
        && (s == "0" || !s.starts_with('0'));
    canonical.then(|| s.parse().expect("checked digits"))
}

/// Exactly 16 lowercase hexadecimal digits.
fn hex_seed(s: &str) -> Option<u64> {
    let canonical = s.len() == 16 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    canonical.then(|| u64::from_str_radix(s, 16).expect("checked hex digits"))
}

#[cfg(test)]
mod tests {
    use super::{CodeError, MazeCode, MazeParams};
    use proptest::prelude::*;

    fn params() -> impl Strategy<Value = MazeParams> {
        (2u32..=127, 2u32..=127, 0u32..=100, 0u32..=100)
            .prop_map(|(w, h, l, p)| MazeParams::new(2 * w + 1, 2 * h + 1, l, p).unwrap())
    }

    #[test]
    fn formats_and_parses() {
        let code: MazeCode = "ded1-31x31-l25-p100-9f3a0c11d2e4b6a8".parse().unwrap();
        assert_eq!(code.params(), MazeParams::default());
        assert_eq!(code.seed(), 0x9f3a_0c11_d2e4_b6a8);
        assert_eq!(code.to_string(), "ded1-31x31-l25-p100-9f3a0c11d2e4b6a8");

        let small = MazeCode::new(MazeParams::new(5, 255, 0, 0).unwrap(), 1);
        assert_eq!(small.to_string(), "ded1-5x255-l0-p0-0000000000000001");
    }

    #[test]
    fn rejects_non_canonical_forms() {
        for s in [
            "",
            "ded1",
            "ded1-31x31-l25-p100",
            "ded1-31x31-l25-p100-9f3a0c11d2e4b6a8-x",
            "ded1-31-l25-p100-9f3a0c11d2e4b6a8",
            "ded1-031x31-l25-p100-9f3a0c11d2e4b6a8",
            "ded1-31x31-l025-p100-9f3a0c11d2e4b6a8",
            "ded1-31x31-l+5-p100-9f3a0c11d2e4b6a8",
            "ded1-31x31-25-p100-9f3a0c11d2e4b6a8",
            "ded1-31x31-l25-d100-9f3a0c11d2e4b6a8",
            "ded1-31x31-l25-p100-9F3A0C11D2E4B6A8",
            "ded1-31x31-l25-p100-9f3a0c11d2e4b6a",
            "ded1-31x31-l25-p100-09f3a0c11d2e4b6a8",
            "ded1-1031x31-l25-p100-9f3a0c11d2e4b6a8",
            "DED1-31x31-l25-p100-9f3a0c11d2e4b6a8",
            "ded01-31x31-l25-p100-9f3a0c11d2e4b6a8",
            " ded1-31x31-l25-p100-9f3a0c11d2e4b6a8",
        ] {
            assert_eq!(
                s.parse::<MazeCode>(),
                Err(CodeError::Malformed(s.to_owned())),
                "{s:?}"
            );
        }
    }

    #[test]
    fn rejects_other_versions() {
        assert_eq!(
            "ded2-31x31-l25-p100-9f3a0c11d2e4b6a8".parse::<MazeCode>(),
            Err(CodeError::UnsupportedVersion("ded2".to_owned()))
        );
        assert_eq!(
            "ded0".parse::<MazeCode>(),
            Err(CodeError::UnsupportedVersion("ded0".to_owned()))
        );
    }

    #[test]
    fn rejects_bad_values() {
        let bad_size = |w, h| {
            Err(CodeError::BadSize {
                width: w,
                height: h,
            })
        };
        let parse = |s: &str| s.parse::<MazeCode>();
        assert_eq!(
            parse("ded1-30x31-l25-p100-0000000000000000"),
            bad_size(30, 31)
        );
        assert_eq!(
            parse("ded1-31x3-l25-p100-0000000000000000"),
            bad_size(31, 3)
        );
        assert_eq!(
            parse("ded1-257x31-l25-p100-0000000000000000"),
            bad_size(257, 31)
        );
        assert_eq!(
            parse("ded1-0x31-l25-p100-0000000000000000"),
            bad_size(0, 31)
        );
        assert_eq!(
            parse("ded1-31x31-l101-p100-0000000000000000"),
            Err(CodeError::BadPercent {
                name: "loops",
                value: 101
            })
        );
        assert_eq!(
            parse("ded1-31x31-l25-p999-0000000000000000"),
            Err(CodeError::BadPercent {
                name: "density",
                value: 999
            })
        );
    }

    #[test]
    fn random_codes_differ() {
        let params = MazeParams::default();
        assert_ne!(MazeCode::random(params), MazeCode::random(params));
    }

    proptest! {
        #[test]
        fn code_roundtrip(params in params(), seed: u64) {
            let code = MazeCode::new(params, seed);
            prop_assert_eq!(code.to_string().parse(), Ok(code));
        }

        #[test]
        fn string_roundtrip(s in "ded1-[0-9]{1,3}x[0-9]{1,3}-l[0-9]{1,3}-p[0-9]{1,3}-[0-9a-f]{16}") {
            if let Ok(code) = s.parse::<MazeCode>() {
                prop_assert_eq!(code.to_string(), s);
            }
        }
    }
}
