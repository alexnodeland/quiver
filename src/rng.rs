//! Seedable Random Number Generation for `no_std`
//!
//! This module provides a seedable RNG implementation that works in `no_std`
//! environments. It uses the Xorshift128+ algorithm which is fast and produces
//! good quality random numbers suitable for audio synthesis applications.
//!
//! When the `std` feature is enabled, the default seed is derived from the
//! system time. In `no_std` mode, a fixed default seed is used unless
//! explicitly set.

#[cfg(feature = "std")]
use core::cell::Cell;

#[cfg(feature = "std")]
std::thread_local! {
    /// Thread-local random number generator state.
    ///
    /// In `std` mode, it uses thread-local storage.
    static RNG_STATE: Cell<Rng> = Cell::new(Rng::from_system_time());
}

#[cfg(not(feature = "std"))]
static mut RNG_STATE: Rng = Rng::new(0x853c49e6748fea9b, 0xda3e39cb94b95bdb);

/// A seedable random number generator using Xorshift128+.
///
/// This RNG is fast, has a period of 2^128 - 1, and passes most statistical
/// tests. It is suitable for audio applications like noise generation.
#[derive(Debug, Clone, Copy)]
pub struct Rng {
    s0: u64,
    s1: u64,
}

impl Rng {
    /// Create a new RNG with the given seed values.
    ///
    /// The seeds should not both be zero.
    #[inline]
    pub const fn new(s0: u64, s1: u64) -> Self {
        // Ensure at least one seed is non-zero
        let s0 = if s0 == 0 && s1 == 0 { 1 } else { s0 };
        Self { s0, s1 }
    }

    /// Create a new RNG from a single 64-bit seed.
    ///
    /// The seed is split into two state values using a mixing function.
    #[inline]
    pub fn from_seed(seed: u64) -> Self {
        // Use splitmix64 to derive state from seed
        let s0 = splitmix64(seed);
        let s1 = splitmix64(seed.wrapping_add(0x9e3779b97f4a7c15));
        Self::new(s0, s1)
    }

    /// Create a new RNG seeded from system time (std only).
    #[cfg(feature = "std")]
    pub fn from_system_time() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        let seed = duration.as_nanos() as u64;
        Self::from_seed(seed)
    }

    /// Generate the next u64 value.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let s0 = self.s0;
        let mut s1 = self.s1;
        let result = s0.wrapping_add(s1);

        s1 ^= s0;
        self.s0 = s0.rotate_left(24) ^ s1 ^ (s1 << 16);
        self.s1 = s1.rotate_left(37);

        result
    }

    /// Generate a random f64 in the range [0.0, 1.0).
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        // Use the upper 53 bits for the mantissa
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Generate a random f64 in the range [-1.0, 1.0).
    #[inline]
    pub fn next_f64_bipolar(&mut self) -> f64 {
        self.next_f64() * 2.0 - 1.0
    }

    /// Generate a random bool with 50% probability.
    #[inline]
    pub fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// Generate a random bool with the given probability (0.0 to 1.0).
    #[inline]
    pub fn next_bool_with_probability(&mut self, probability: f64) -> bool {
        self.next_f64() < probability
    }

    /// Jump the RNG state forward by 2^64 steps.
    ///
    /// Useful for creating independent streams.
    pub fn jump(&mut self) {
        const JUMP: [u64; 2] = [0xdf900294d8f554a5, 0x170865df4b3201fc];

        let mut s0 = 0u64;
        let mut s1 = 0u64;

        for jump_val in JUMP.iter() {
            for b in 0..64 {
                if (jump_val >> b) & 1 != 0 {
                    s0 ^= self.s0;
                    s1 ^= self.s1;
                }
                self.next_u64();
            }
        }

        self.s0 = s0;
        self.s1 = s1;
    }
}

impl Default for Rng {
    fn default() -> Self {
        #[cfg(feature = "std")]
        {
            Self::from_system_time()
        }
        #[cfg(not(feature = "std"))]
        {
            Self::new(0x853c49e6748fea9b, 0xda3e39cb94b95bdb)
        }
    }
}

/// Splitmix64 mixing function for deriving state from seeds.
#[inline]
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// Trait for seedable RNGs.
pub trait SeedableRng: Sized {
    /// Create from a 64-bit seed.
    fn from_seed(seed: u64) -> Self;

    /// Generate the next random f64 in [0.0, 1.0).
    fn next_f64(&mut self) -> f64;

    /// Generate a random f64 in [-1.0, 1.0).
    fn next_f64_bipolar(&mut self) -> f64 {
        self.next_f64() * 2.0 - 1.0
    }
}

impl SeedableRng for Rng {
    fn from_seed(seed: u64) -> Self {
        Rng::from_seed(seed)
    }

    fn next_f64(&mut self) -> f64 {
        self.next_f64()
    }

    fn next_f64_bipolar(&mut self) -> f64 {
        self.next_f64_bipolar()
    }
}

/// Get a random f64 in the range [0.0, 1.0) from the thread-local RNG.
///
/// This is a convenience function that mimics the behavior of `rand::random()`.
#[inline]
pub fn random() -> f64 {
    #[cfg(feature = "std")]
    {
        RNG_STATE.with(|cell| {
            let mut rng = cell.get();
            let value = rng.next_f64();
            cell.set(rng);
            value
        })
    }
    #[cfg(not(feature = "std"))]
    {
        // Safety: Single-threaded no_std contexts only
        unsafe { RNG_STATE.next_f64() }
    }
}

/// Get a random f64 in the range [-1.0, 1.0) from the thread-local RNG.
#[inline]
pub fn random_bipolar() -> f64 {
    random() * 2.0 - 1.0
}

/// Seed the thread-local RNG.
#[inline]
pub fn seed(seed: u64) {
    #[cfg(feature = "std")]
    {
        RNG_STATE.with(|cell| {
            cell.set(Rng::from_seed(seed));
        });
    }
    #[cfg(not(feature = "std"))]
    {
        // Safety: Single-threaded no_std contexts only
        unsafe {
            RNG_STATE = Rng::from_seed(seed);
        }
    }
}

/// Get a random bool with the given probability.
#[inline]
pub fn random_bool(probability: f64) -> bool {
    random() < probability
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rng_deterministic() {
        let mut rng1 = Rng::from_seed(12345);
        let mut rng2 = Rng::from_seed(12345);

        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn test_rng_different_seeds() {
        let mut rng1 = Rng::from_seed(12345);
        let mut rng2 = Rng::from_seed(54321);

        // Different seeds should produce different sequences
        assert_ne!(rng1.next_u64(), rng2.next_u64());
    }

    #[test]
    fn test_rng_f64_range() {
        let mut rng = Rng::from_seed(42);

        for _ in 0..1000 {
            let v = rng.next_f64();
            assert!(v >= 0.0 && v < 1.0, "Value {} out of range", v);
        }
    }

    #[test]
    fn test_rng_bipolar_range() {
        let mut rng = Rng::from_seed(42);

        for _ in 0..1000 {
            let v = rng.next_f64_bipolar();
            assert!(v >= -1.0 && v < 1.0, "Value {} out of range", v);
        }
    }

    #[test]
    fn test_rng_distribution() {
        let mut rng = Rng::from_seed(42);
        let mut sum = 0.0;
        let count = 10000;

        for _ in 0..count {
            sum += rng.next_f64();
        }

        let mean = sum / count as f64;
        // Mean should be close to 0.5
        assert!((mean - 0.5).abs() < 0.02, "Mean {} too far from 0.5", mean);
    }

    #[test]
    fn test_global_random() {
        seed(12345);
        let v1 = random();
        let v2 = random();

        // Should produce different values
        assert_ne!(v1, v2);

        // Should be in valid range
        assert!(v1 >= 0.0 && v1 < 1.0);
        assert!(v2 >= 0.0 && v2 < 1.0);
    }

    #[test]
    fn test_random_bipolar() {
        seed(42);
        for _ in 0..100 {
            let v = random_bipolar();
            assert!(v >= -1.0 && v < 1.0);
        }
    }

    #[test]
    fn test_random_bool() {
        seed(42);
        let mut true_count = 0;
        let count = 10000;

        for _ in 0..count {
            if random_bool(0.3) {
                true_count += 1;
            }
        }

        let ratio = true_count as f64 / count as f64;
        // Should be close to 30%
        assert!(
            (ratio - 0.3).abs() < 0.03,
            "Ratio {} too far from 0.3",
            ratio
        );
    }

    #[test]
    fn test_rng_jump() {
        let mut rng1 = Rng::from_seed(42);
        let mut rng2 = Rng::from_seed(42);

        rng1.jump();

        // After jump, sequences should be different
        assert_ne!(rng1.next_u64(), rng2.next_u64());
    }

    #[test]
    fn test_zero_seed_handling() {
        // Zero seeds should still produce valid output
        let mut rng = Rng::new(0, 0);
        let v = rng.next_f64();
        assert!(v >= 0.0 && v < 1.0);
    }
}
