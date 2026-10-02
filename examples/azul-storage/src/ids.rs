//! The seed for ids that leave the process.
//!
//! A document, note, deck or project id names a folder in the drive (an S3
//! bucket later), so it must not repeat across launches and devices.
//! `azul::uuid::Uuid::v4` is a process-local counter - the same sequence in
//! every run - and azul carries no randomness source on purpose, so the id
//! is `Uuid::from_seed(random_seed())`: azul's mint, seeded from here.

use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// 64 random bits for `azul::uuid::Uuid::from_seed`: std's per-process
/// random hasher keys (seeded from the OS), over a per-process counter, the
/// time and the process id - distinct between two calls, two processes and
/// two machines.
#[must_use]
pub fn random_seed() -> u64 {
    static MINTED: AtomicU64 = AtomicU64::new(0);
    let mut h = RandomState::new().build_hasher();
    h.write_u64(MINTED.fetch_add(1, Ordering::Relaxed));
    h.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.write_u32(std::process::id());
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::random_seed;

    #[test]
    fn two_seeds_differ() {
        let seeds: std::collections::HashSet<u64> = (0..1000).map(|_| random_seed()).collect();
        assert_eq!(seeds.len(), 1000, "a thousand seeds, a thousand values");
    }
}
