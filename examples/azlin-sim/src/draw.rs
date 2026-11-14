//! Seeded draws (planning/architecture.md section 1.4 rule 1: randomness is a parameter; section
//! 12: a failing seed replays exactly).
//!
//! Every random decision a step causes draws from `Rng::new(mix(seed, salt))` in a fixed order,
//! never from a stream shared between steps (rule H2 of the harness): removing a step while
//! shrinking never changes the draws of another step.

use azlin_seams::hash32;
use azlin_seams::sim::Rng;

/// The domain of the harness's mixing hash (a test domain, never used by production code).
const MIX_DOMAIN: &str = "azlin-sim-mix-v1";

/// Mixes a seed and a salt into one 64-bit value: the first eight bytes of a domain-separated
/// BLAKE3 hash of both.
///
/// Implements planning/architecture.md section 1.4 rule 1 (randomness is a parameter).
///
/// Caller: `step_rng`, `Config::standard_with`, `keys::Keyring`.
pub fn mix(seed: u64, salt: u64) -> u64 {
    let digest = hash32(MIX_DOMAIN, &[&seed.to_le_bytes(), &salt.to_le_bytes()]);
    let bytes = digest.as_bytes();
    let mut out = [0u8; 8];
    for (dst, src) in out.iter_mut().zip(bytes.iter()) {
        *dst = *src;
    }
    u64::from_le_bytes(out)
}

/// The generator of one step: `Rng::new(mix(seed, salt))`.
///
/// Implements planning/architecture.md section 1.4 rule 1 and rule 2 (a failing seed replays).
///
/// Caller: `world::World::step`, `scenario::Scenario::run`.
pub fn step_rng(seed: u64, salt: u32) -> Rng {
    Rng::new(mix(seed, u64::from(salt)))
}

/// A number in `0..n`; `0` when `n` is 0 (an empty range draws nothing and is the first element).
///
/// Implements planning/architecture.md section 1.4 rule 1 (randomness is a parameter).
///
/// Caller: every drawing module of the harness.
pub fn below(rng: &mut Rng, n: u64) -> u64 {
    match rng.below(n) {
        Some(value) => value,
        // n == 0: the range is empty, the draw is the index of the only sensible choice.
        None => 0,
    }
}

/// A number in `lo..=hi` (both included); `lo` when the range is empty or reversed.
///
/// Implements planning/architecture.md section 1.4 rule 1 (randomness is a parameter).
///
/// Caller: `net::SimNet::send`, `scenario`.
pub fn between(rng: &mut Rng, lo: u64, hi: u64) -> u64 {
    if hi <= lo {
        return lo;
    }
    lo.saturating_add(below(rng, hi.saturating_sub(lo).saturating_add(1)))
}

/// One element of `items` chosen by weight (a weight of 0 is never chosen); `None` when every
/// weight is 0 or the list is empty.
///
/// Implements planning/architecture.md section 1.4 rule 1 (randomness is a parameter).
///
/// Caller: `scenario::Scenario::run` (the step weights of a profile).
pub fn weighted<'a, T>(rng: &mut Rng, items: &'a [(u32, T)]) -> Option<&'a T> {
    let total: u64 = items.iter().map(|(weight, _)| u64::from(*weight)).sum();
    if total == 0 {
        return None;
    }
    let mut pick = below(rng, total);
    for (weight, item) in items {
        let weight = u64::from(*weight);
        if pick < weight {
            return Some(item);
        }
        pick = pick.saturating_sub(weight);
    }
    None
}
