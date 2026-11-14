//! Shared helpers of the tests: a seeded generator (xorshift64*, no dependency) and two entropy
//! sources (a counting one and a failing one).

use azlin_keys::{Entropy, EntropyError};

/// A xorshift64* generator: the same seed gives the same sequence on every machine.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

/// An entropy source that fills with 1, 2, 3, ... (wrapping): never zeroes.
pub struct Counting(pub u8);

impl Entropy for Counting {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        for byte in out.iter_mut() {
            self.0 = self.0.wrapping_add(1);
            *byte = self.0;
        }
        Ok(())
    }
}

/// An entropy source that always fails.
pub struct Failing;

impl Entropy for Failing {
    fn fill(&mut self, _out: &mut [u8]) -> Result<(), EntropyError> {
        Err(EntropyError::Exhausted)
    }
}
