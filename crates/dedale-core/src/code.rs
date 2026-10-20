//! Maze codes: the string that reproduces a maze.
//!
//! `ded2-31x31-l25-p534-9f3a0c11d2e4b6a8` is generator version 2, a 31×31
//! maze, 25% of dead ends opened into loops, 534 points, and the 64-bit seed
//! in hexadecimal.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::grid::MAX_SIZE;
use crate::maze::MIN_SIZE;
use crate::rng::fresh_seed;

/// The generator version this build implements. Any change to generator
/// output must bump it.
pub const CODE_VERSION: u32 = 2;

/// A point count meaning "every open cell"; larger requests are capped to it.
pub const ALL_POINTS: u16 = u16::MAX;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CodeError {
    #[error(
        "malformed maze code {0:?}; expected ded{CODE_VERSION}-<W>x<H>-l<loops>-p<points>-<16 hex digits>"
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
    #[error("a maze code asks for {value} points; codes go up to {ALL_POINTS} (every open cell)")]
    BadPoints { value: u32 },
}

/// Everything about a maze except its seed: shared by every game of a
/// tournament.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MazeParams {
    width: u8,
    height: u8,
    loops: u8,
    points: u16,
}

impl MazeParams {
    /// `points` above [`ALL_POINTS`] is capped to it, like any count the maze
    /// cannot hold (see the generator).
    ///
    /// # Errors
    ///
    /// If a side is even or outside `MIN_SIZE..=MAX_SIZE`, or `loops` is
    /// above 100.
    pub fn new(width: u32, height: u32, loops: u32, points: u32) -> Result<Self, CodeError> {
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
            points: u16::try_from(points).unwrap_or(ALL_POINTS),
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

    /// Points requested; the generator caps it to what the maze holds.
    #[must_use]
    pub fn points(&self) -> u16 {
        self.points
    }
}

/// 31×31, 80% loops, 50 points: open mazes with points worth fighting
/// over, where denying the opponent pays.
impl Default for MazeParams {
    fn default() -> Self {
        Self {
            width: 31,
            height: 31,
            loops: 80,
            points: 50,
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

    /// The same code asking for `points` points. With the number of points
    /// the requested code actually placed, this is the normalized code: it
    /// generates the same maze and says how many points it has.
    #[must_use]
    pub fn with_points(self, points: u16) -> Self {
        let params = MazeParams {
            points,
            ..self.params
        };
        Self { params, ..self }
    }
}

impl fmt::Display for MazeCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = &self.params;
        write!(
            f,
            "ded{CODE_VERSION}-{}x{}-l{}-p{}-{:016x}",
            p.width, p.height, p.loops, p.points, self.seed
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
                .is_some_and(|v| decimal(v, 3).is_some());
            return Err(if other_version {
                CodeError::UnsupportedVersion(version.to_owned())
            } else {
                malformed()
            });
        }
        let &[_, size, loops, points, seed] = parts.as_slice() else {
            return Err(malformed());
        };
        let (width, height) = size.split_once('x').ok_or_else(malformed)?;
        let field = |text: &str, prefix: &str, digits| {
            text.strip_prefix(prefix).and_then(|n| decimal(n, digits))
        };
        let width = decimal(width, 3).ok_or_else(malformed)?;
        let height = decimal(height, 3).ok_or_else(malformed)?;
        let loops = field(loops, "l", 3).ok_or_else(malformed)?;
        let points = field(points, "p", 5).ok_or_else(malformed)?;
        let seed = hex_seed(seed).ok_or_else(malformed)?;
        // Capping would break the round trip: in a code, it is an error.
        if points > u32::from(ALL_POINTS) {
            return Err(CodeError::BadPoints { value: points });
        }
        Ok(Self::new(
            MazeParams::new(width, height, loops, points)?,
            seed,
        ))
    }
}

/// Serialized as its string form.
impl Serialize for MazeCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for MazeCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// A canonical decimal of at most `digits` digits: `0`, or digits without a
/// leading zero.
fn decimal(s: &str, digits: usize) -> Option<u32> {
    let canonical = (1..=digits).contains(&s.len())
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
    use super::{ALL_POINTS, CodeError, MazeCode, MazeParams};
    use proptest::prelude::*;

    fn params() -> impl Strategy<Value = MazeParams> {
        (2u32..=127, 2u32..=127, 0u32..=100, 0u32..=65_535)
            .prop_map(|(w, h, l, p)| MazeParams::new(2 * w + 1, 2 * h + 1, l, p).unwrap())
    }

    #[test]
    fn formats_and_parses() {
        let code: MazeCode = "ded2-31x31-l25-p534-9f3a0c11d2e4b6a8".parse().unwrap();
        assert_eq!(code.params(), MazeParams::new(31, 31, 25, 534).unwrap());
        assert_eq!(code.seed(), 0x9f3a_0c11_d2e4_b6a8);
        assert_eq!(code.to_string(), "ded2-31x31-l25-p534-9f3a0c11d2e4b6a8");

        let small = MazeCode::new(MazeParams::new(5, 255, 0, 0).unwrap(), 1);
        assert_eq!(small.to_string(), "ded2-5x255-l0-p0-0000000000000001");
        let default = MazeCode::new(MazeParams::default(), 2);
        assert_eq!(default.to_string(), "ded2-31x31-l80-p50-0000000000000002");
        assert_eq!(
            default.with_points(534).to_string(),
            "ded2-31x31-l80-p534-0000000000000002"
        );
    }

    #[test]
    fn counts_are_capped() {
        let params = MazeParams::new(31, 31, 25, 1_000_000).unwrap();
        assert_eq!(params.points(), ALL_POINTS);
    }

    #[test]
    fn rejects_non_canonical_forms() {
        for s in [
            "",
            "ded2",
            "ded2-31x31-l25-p100",
            "ded2-31x31-l25-p100-9f3a0c11d2e4b6a8-x",
            "ded2-31-l25-p100-9f3a0c11d2e4b6a8",
            "ded2-031x31-l25-p100-9f3a0c11d2e4b6a8",
            "ded2-31x31-l025-p100-9f3a0c11d2e4b6a8",
            "ded2-31x31-l25-p0100-9f3a0c11d2e4b6a8",
            "ded2-31x31-l25-p123456-9f3a0c11d2e4b6a8",
            "ded2-31x31-l+5-p100-9f3a0c11d2e4b6a8",
            "ded2-31x31-25-p100-9f3a0c11d2e4b6a8",
            "ded2-31x31-l25-d100-9f3a0c11d2e4b6a8",
            "ded2-31x31-l25-p100-9F3A0C11D2E4B6A8",
            "ded2-31x31-l25-p100-9f3a0c11d2e4b6a",
            "ded2-31x31-l25-p100-09f3a0c11d2e4b6a8",
            "ded2-1031x31-l25-p100-9f3a0c11d2e4b6a8",
            "DED2-31x31-l25-p100-9f3a0c11d2e4b6a8",
            "ded02-31x31-l25-p100-9f3a0c11d2e4b6a8",
            " ded2-31x31-l25-p100-9f3a0c11d2e4b6a8",
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
        // ded1's p was a density: its codes mean other mazes.
        assert_eq!(
            "ded1-31x31-l25-p100-9f3a0c11d2e4b6a8".parse::<MazeCode>(),
            Err(CodeError::UnsupportedVersion("ded1".to_owned()))
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
            parse("ded2-30x31-l25-p100-0000000000000000"),
            bad_size(30, 31)
        );
        assert_eq!(
            parse("ded2-31x3-l25-p100-0000000000000000"),
            bad_size(31, 3)
        );
        assert_eq!(
            parse("ded2-257x31-l25-p100-0000000000000000"),
            bad_size(257, 31)
        );
        assert_eq!(
            parse("ded2-0x31-l25-p100-0000000000000000"),
            bad_size(0, 31)
        );
        assert_eq!(
            parse("ded2-31x31-l101-p100-0000000000000000"),
            Err(CodeError::BadPercent {
                name: "loops",
                value: 101
            })
        );
        assert_eq!(
            parse("ded2-31x31-l25-p65536-0000000000000000"),
            Err(CodeError::BadPoints { value: 65_536 })
        );
    }

    #[test]
    fn serde_uses_the_string_form() {
        let code: MazeCode = "ded2-31x31-l25-p534-9f3a0c11d2e4b6a8".parse().unwrap();
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, r#""ded2-31x31-l25-p534-9f3a0c11d2e4b6a8""#);
        assert_eq!(serde_json::from_str::<MazeCode>(&json).unwrap(), code);
        assert!(
            serde_json::from_str::<MazeCode>(r#""ded2-30x31-l25-p100-0000000000000000""#).is_err()
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
        fn string_roundtrip(s in "ded2-[0-9]{1,3}x[0-9]{1,3}-l[0-9]{1,3}-p[0-9]{1,5}-[0-9a-f]{16}") {
            if let Ok(code) = s.parse::<MazeCode>() {
                prop_assert_eq!(code.to_string(), s);
            }
        }
    }
}
