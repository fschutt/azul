//! The seeded generator and the shrinker of every simulation test of the engine
//! (planning/architecture.md section 1.4 rules 1 and 2: "tests pass a simulated one driven by a
//! seeded generator, so a failing seed replays exactly"; build unit S17, lane W1-S17a).
//!
//! - [`Rng`] is SplitMix64: a 64-bit state, one output per step, the same stream for the same seed
//!   on every platform. It is for SIMULATIONS (the choice of an operation, an interleaving, a
//!   byte string to mutate): it is NOT a source of key material and does not implement
//!   `Entropy`. A failing case prints its seed, and `Rng::new(seed)` replays it. It is the one copy
//!   for the new crates (azctl's `soak.rs` and nine more private copies of splitmix64 are the old
//!   path and stay until their units move).
//! - [`minimize`] is delta debugging (Zeller's ddmin): given a sequence that fails a test it returns
//!   a subsequence that still fails and from which no single element can be removed. It is the
//!   one shrinker of the S47 lanes and of every property test that wants a small counterexample.

use azlin_keys::{Entropy, EntropyError};

/// A seeded SplitMix64 generator.
///
/// Implements planning/architecture.md section 1.4 rule 1 (randomness is a parameter) and rule 2
/// (the simulator replays from a seed).
///
/// Caller: the property tests of azlin-chain and every S47 lane (`azlin-escalate-e*`), S16a and
/// S17c (seeded schedules and entry streams).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// The generator of this seed: the same seed gives the same stream.
    pub const fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    /// A generator seeded with 8 bytes from the entropy seam (the little-endian `u64`); on an
    /// entropy failure there is no generator.
    pub fn from_entropy(ent: &mut dyn Entropy) -> Result<Rng, EntropyError> {
        let mut seed = [0u8; 8];
        ent.fill(&mut seed)?;
        Ok(Rng(u64::from_le_bytes(seed)))
    }

    /// The next 64 random bits (the SplitMix64 step).
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`, uniform (no modulo bias: draws below `2^64 mod n` are rejected);
    /// `None` when `n` is 0 (the range is empty).
    pub fn below(&mut self, n: u64) -> Option<u64> {
        if n == 0 {
            return None;
        }
        if mutant!("seams_rng_below_modulo") {
            return Some(self.next_u64() % n);
        }
        // 2^64 mod n: the draws below it would make small remainders more likely.
        let threshold = n.wrapping_neg() % n;
        loop {
            let draw = self.next_u64();
            if draw >= threshold {
                return Some(draw % n);
            }
        }
    }

    /// A number in `lo..hi` (`hi` excluded), uniform; `None` when `lo >= hi` (the range is empty).
    pub fn range(&mut self, lo: u64, hi: u64) -> Option<u64> {
        let width = hi.checked_sub(lo)?;
        let offset = self.below(width)?;
        lo.checked_add(offset)
    }

    /// True with the probability `num / den`: always when `num >= den`, never when `num` is 0 or
    /// `den` is 0 (an event of probability 0).
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        match self.below(u64::from(den)) {
            Some(draw) => draw < u64::from(num),
            None => false,
        }
    }

    /// One element of `items`, uniform; `None` when `items` is empty.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        // A `usize` fits a `u64`, and an index below `items.len()` fits a `usize` (lib.rs).
        let index = self.below(items.len() as u64)?;
        items.get(index as usize)
    }

    /// Shuffles `items` in place (Fisher-Yates): every permutation is equally likely. A slice of
    /// 0 or 1 elements is left as it is, and consumes no draw.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            if let Some(draw) = self.below((i as u64).saturating_add(1)) {
                items.swap(i, draw as usize);
            }
        }
    }

    /// Fills `out` with random bytes.
    pub fn fill_bytes(&mut self, out: &mut [u8]) {
        for chunk in out.chunks_mut(8) {
            let word = self.next_u64().to_le_bytes();
            for (dst, src) in chunk.iter_mut().zip(word) {
                *dst = src;
            }
        }
    }

    /// `len` random bytes.
    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        self.fill_bytes(&mut out);
        out
    }
}

/// The most calls of the predicate [`minimize`] makes: a bound on the work. A sequence of `n`
/// elements needs about `n log n` calls for a small counterexample and at most about `n^2` in the
/// worst case; at the bound the shrinker stops and returns the smallest failing sequence it has
/// found.
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up front).
///
/// Caller: `minimize`, the tests of the S47 lanes that bound a shrink.
pub const MINIMIZE_MAX_CALLS: usize = 1_000_000;

/// The predicate with its call budget.
struct Budget<'a, T> {
    still_fails: &'a dyn Fn(&[T]) -> bool,
    left: usize,
}

impl<T> Budget<'_, T> {
    /// Whether `candidate` still fails; `None` when the budget is spent.
    fn fails(&mut self, candidate: &[T]) -> Option<bool> {
        self.left = self.left.checked_sub(1)?;
        Some((self.still_fails)(candidate))
    }
}

/// Delta debugging (ddmin): `still_fails` says whether a sequence still shows the failure. When
/// `items` does not fail at all the input is returned as it is (nothing can go). Otherwise the
/// result is a subsequence of `items` (the order is kept) that still fails and is 1-minimal:
/// removing any single element makes it pass. A failure that needs no element at all gives the
/// empty sequence. The predicate must be deterministic; it is called at most
/// [`MINIMIZE_MAX_CALLS`] times.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays; a shrunk case
/// is what a person reads).
///
/// Caller: the property tests of azlin-chain (a failing interleaving) and the S47 lanes (a failing
/// story, a failing entry stream).
pub fn minimize<T: Clone>(items: Vec<T>, still_fails: impl Fn(&[T]) -> bool) -> Vec<T> {
    let mut budget = Budget {
        still_fails: &still_fails,
        left: MINIMIZE_MAX_CALLS,
    };
    let mut current = items;
    if budget.fails(&current) != Some(true) {
        return current;
    }
    let mut parts = 2usize;
    'rounds: while current.len() >= 2 {
        let size = current.len().div_ceil(parts);
        // 1. A single part that fails on its own.
        let mut next: Option<(Vec<T>, usize)> = None;
        for part in current.chunks(size) {
            match budget.fails(part) {
                None => break 'rounds,
                Some(true) => {
                    next = Some((part.to_vec(), 2));
                    break;
                }
                Some(false) => {}
            }
        }
        // 2. The sequence without one part (with two parts that is the other part: tried).
        if next.is_none() && parts > 2 {
            let count = current.len().div_ceil(size);
            for skip in 0..count {
                let rest: Vec<T> = current
                    .chunks(size)
                    .enumerate()
                    .filter(|(index, _)| *index != skip)
                    .flat_map(|(_, part)| part.iter().cloned())
                    .collect();
                match budget.fails(&rest) {
                    None => break 'rounds,
                    Some(true) => {
                        next = Some((rest, parts.saturating_sub(1).max(2)));
                        break;
                    }
                    Some(false) => {}
                }
            }
        }
        match next {
            Some((smaller, granularity)) => {
                current = smaller;
                parts = granularity;
            }
            None => {
                if parts >= current.len() {
                    break;
                }
                parts = parts.saturating_mul(2).min(current.len());
            }
        }
    }
    // The last element can go too when the failure needs no element at all.
    if current.len() == 1 && budget.fails(&[]) == Some(true) {
        current.clear();
    }
    current
}
