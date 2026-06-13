//! Seedable, replayable RNG. All shuffles / coin flips draw from this so that
//! (same seed + same agent decisions) ⇒ an identical, replayable game.

use serde::{Deserialize, Serialize};

/// A small deterministic xorshift64* PRNG — platform-independent, cheap to clone
/// alongside game state for search/branching. Serializable (its single `u64` of state)
/// so a full `GameState` snapshot replays identically (ENGINE_PLAN §7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Fisher–Yates shuffle of `slice`, drawing from this RNG (deterministic for a seed).
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        let n = slice.len();
        for i in (1..n).rev() {
            let j = self.below((i + 1) as u64) as usize;
            slice.swap(i, j);
        }
    }
}

impl Rng {
    /// Create an RNG from a seed (0 is remapped off the all-zero fixed point).
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed },
        }
    }

    /// Next pseudo-random `u64` (xorshift64*).
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-ish integer in `[0, n)`; `n` must be > 0.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "Rng::below requires n > 0");
        self.next_u64() % n
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn deterministic_for_same_seed() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}
