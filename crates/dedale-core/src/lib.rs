//! Deterministic core of Dedale.
//!
//! Everything here is pure: the same inputs always give the same outputs, so
//! it can be property-tested and fuzzed without spawning bots.

pub mod grid;
pub mod maze;
pub mod rng;
