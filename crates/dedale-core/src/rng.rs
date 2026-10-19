//! The only source of randomness in Dedale.
//!
//! A maze code must describe the same maze forever, so the generators here are
//! implemented in-tree and pinned by known-answer tests instead of coming from a
//! crate whose output may change between versions.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// `SplitMix64`, used to expand a 64-bit seed into [`Rng`] state.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// xoshiro256** seeded through [`SplitMix64`].
#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        let mut mix = SplitMix64::new(seed);
        // SplitMix64 outputs are a bijection of distinct counters, so at most one
        // word is zero: the state is never the all-zero fixed point.
        Self {
            s: std::array::from_fn(|_| mix.next_u64()),
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// A uniform integer in `0..n`, without bias (Lemire's multiply-and-reject).
    ///
    /// # Panics
    ///
    /// If `n` is 0.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "Rng::below(0)");
        let mut m = u128::from(self.next_u64()) * u128::from(n);
        if low_word(m) < n {
            let threshold = n.wrapping_neg() % n;
            while low_word(m) < threshold {
                m = u128::from(self.next_u64()) * u128::from(n);
            }
        }
        high_word(m)
    }

    /// A uniform index in `0..len`.
    ///
    /// # Panics
    ///
    /// If `len` is 0.
    pub fn index(&mut self, len: usize) -> usize {
        let len = u64::try_from(len).expect("usize fits in u64");
        usize::try_from(self.below(len)).expect("below(len) < len fits in usize")
    }

    /// True with probability `percent / 100` (always for 100 and above).
    ///
    /// Draws the same randomness whatever `percent` is, so changing a percentage
    /// never shifts the rest of the stream.
    pub fn chance(&mut self, percent: u8) -> bool {
        self.below(100) < u64::from(percent)
    }

    /// Fisher–Yates shuffle.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.index(i + 1));
        }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "keeps the low 64 bits on purpose"
)]
fn low_word(m: u128) -> u64 {
    m as u64
}

fn high_word(m: u128) -> u64 {
    u64::try_from(m >> 64).expect("the high half of a u128 fits in u64")
}

/// A seed for a new game, different on every call.
///
/// Mixes the clock, the process id and a call counter through std's randomly
/// keyed hasher (seeded from the OS), so it needs no extra dependency.
#[must_use]
pub fn fresh_seed() -> u64 {
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let calls = CALLS.fetch_add(1, Ordering::Relaxed);
    let hashed = RandomState::new().hash_one((nanos, std::process::id(), calls));
    SplitMix64::new(hashed).next_u64()
}

#[cfg(test)]
mod tests {
    use super::{Rng, SplitMix64, fresh_seed};
    use proptest::prelude::*;

    // Known answers computed with the reference C code from prng.di.unimi.it.

    #[test]
    fn splitmix64_known_answers() {
        let mut mix = SplitMix64::new(0);
        let expected = [
            0xe220_a839_7b1d_cdaf,
            0x6e78_9e6a_a1b9_65f4,
            0x06c4_5d18_8009_454f,
            0xf88b_b8a8_724c_81ec,
            0x1b39_896a_51a8_749b,
        ];
        for want in expected {
            assert_eq!(mix.next_u64(), want);
        }

        let mut mix = SplitMix64::new(0x0123_4567_89ab_cdef);
        let expected = [
            0x157a_3807_a48f_aa9d,
            0xd573_529b_34a1_d093,
            0x2f90_b72e_996d_ccbe,
            0xa2d4_1933_4c46_67ec,
            0x0140_4ce9_1493_8008,
        ];
        for want in expected {
            assert_eq!(mix.next_u64(), want);
        }
    }

    #[test]
    fn xoshiro_known_answers_from_state() {
        let mut rng = Rng { s: [1, 2, 3, 4] };
        let expected = [
            11_520,
            0,
            1_509_978_240,
            1_215_971_899_390_074_240,
            1_216_172_134_540_287_360,
            607_988_272_756_665_600,
            16_172_922_978_634_559_625,
            8_476_171_486_693_032_832,
            10_595_114_339_597_558_777,
            2_904_607_092_377_533_576,
        ];
        for want in expected {
            assert_eq!(rng.next_u64(), want);
        }
    }

    #[test]
    fn xoshiro_known_answers_from_seed() {
        let cases: [(u64, [u64; 6]); 2] = [
            (
                0,
                [
                    0x99ec_5f36_cb75_f2b4,
                    0xbf6e_1f78_4956_452a,
                    0x1a5f_849d_4933_e6e0,
                    0x6aa5_94f1_262d_2d2c,
                    0xbba5_ad4a_1f84_2e59,
                    0xffef_8375_d9eb_caca,
                ],
            ),
            (
                0x9f3a_0c11_d2e4_b6a8,
                [
                    0xb30a_caa1_8341_baf0,
                    0xeb76_52ac_462c_1b87,
                    0x2da6_1b9a_65b7_f439,
                    0x1e2b_c2f9_5075_56fe,
                    0x8a78_b7aa_c1cc_ea45,
                    0x8dd2_effa_4ddb_2f13,
                ],
            ),
        ];
        for (seed, expected) in cases {
            let mut rng = Rng::from_seed(seed);
            for want in expected {
                assert_eq!(rng.next_u64(), want, "seed {seed:#x}");
            }
        }
    }

    #[test]
    fn below_one_is_zero() {
        let mut rng = Rng::from_seed(7);
        for _ in 0..100 {
            assert_eq!(rng.below(1), 0);
        }
    }

    #[test]
    #[should_panic(expected = "Rng::below(0)")]
    fn below_zero_panics() {
        Rng::from_seed(0).below(0);
    }

    #[test]
    fn below_is_roughly_uniform() {
        let mut rng = Rng::from_seed(42);
        let mut counts = [0u32; 6];
        for _ in 0..60_000 {
            counts[rng.index(6)] += 1;
        }
        for count in counts {
            assert!((9_500..=10_500).contains(&count), "{counts:?}");
        }
    }

    #[test]
    fn chance_extremes() {
        let mut rng = Rng::from_seed(3);
        for _ in 0..1_000 {
            assert!(!rng.chance(0));
            assert!(rng.chance(100));
        }
    }

    #[test]
    fn fresh_seeds_differ() {
        let seeds: std::collections::HashSet<u64> = (0..100).map(|_| fresh_seed()).collect();
        assert_eq!(seeds.len(), 100);
    }

    proptest! {
        #[test]
        fn below_stays_in_range(seed: u64, n in 1u64..) {
            let mut rng = Rng::from_seed(seed);
            for _ in 0..16 {
                prop_assert!(rng.below(n) < n);
            }
        }

        // Close to 2^63 + 1 the rejection threshold is about half the range, so
        // the retry loop runs often.
        #[test]
        fn below_with_heavy_rejection(seed: u64, extra in 0u64..1_000) {
            let n = (1u64 << 63) + 1 + extra;
            let mut rng = Rng::from_seed(seed);
            for _ in 0..16 {
                prop_assert!(rng.below(n) < n);
            }
        }

        #[test]
        fn chance_draws_the_same_stream(seed: u64, a in 0u8..=100, b in 0u8..=100) {
            let mut x = Rng::from_seed(seed);
            let mut y = Rng::from_seed(seed);
            x.chance(a);
            y.chance(b);
            prop_assert_eq!(x.next_u64(), y.next_u64());
        }

        #[test]
        fn shuffle_is_a_permutation(seed: u64, mut items in prop::collection::vec(any::<u16>(), 0..64)) {
            let mut sorted = items.clone();
            sorted.sort_unstable();
            Rng::from_seed(seed).shuffle(&mut items);
            items.sort_unstable();
            prop_assert_eq!(items, sorted);
        }
    }
}
