//! Small, fast, deterministic PRNG (wyrand) with SplitMix64 seeding.
//!
//! Every organism carries its own [`Rng`], so results never depend on thread
//! scheduling or iteration order of other organisms.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rng(u64);

/// SplitMix64 finalizer: turns any seed (including 0 or sequential ids) into a well-mixed state.
pub fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(mix64(seed))
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0xA076_1D64_78BD_642F);
        let t = (self.0 as u128).wrapping_mul((self.0 ^ 0xE703_7ED1_A0B4_28DB) as u128);
        ((t >> 64) ^ t) as u64
    }

    /// Uniform in `[0, 1)`.
    #[inline]
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / (1u64 << 24) as f32)
    }

    /// Uniform in `[lo, hi)`.
    #[inline]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Uniform in `[-1, 1)`.
    #[inline]
    pub fn signed(&mut self) -> f32 {
        self.f32() * 2.0 - 1.0
    }

    /// Uniform integer in `[0, n)`; `n` must be > 0.
    #[inline]
    pub fn below(&mut self, n: u32) -> u32 {
        (((self.next_u64() >> 32) * n as u64) >> 32) as u32
    }

    #[inline]
    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// Uniform angle in `[0, 2π)`.
    #[inline]
    pub fn angle(&mut self) -> f32 {
        self.f32() * std::f32::consts::TAU
    }

    /// Independent child stream.
    pub fn fork(&mut self) -> Rng {
        Rng::new(self.next_u64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_in_range() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            let x = a.f32();
            assert_eq!(x, b.f32());
            assert!((0.0..1.0).contains(&x));
            assert!(a.below(7) < 7);
            b.below(7);
        }
    }

    #[test]
    fn roughly_uniform() {
        let mut r = Rng::new(1);
        let n = 100_000;
        let mean: f32 = (0..n).map(|_| r.f32()).sum::<f32>() / n as f32;
        assert!((mean - 0.5).abs() < 0.01);
    }
}
