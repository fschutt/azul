//! The seed for ids that leave the process - the ONE seed source of the repo.
//!
//! A document, note, deck or project id names a folder in the drive (an S3
//! bucket later), so it must not repeat across launches and devices.
//! `azul::uuid::Uuid::v4` is a process-local counter - the same sequence in
//! every run - and azul carries no randomness source on purpose, so the id
//! is `Uuid::from_seed(random_seed())`: azul's mint, seeded from here.
//! Plain-Rust code that cannot call azul (azul-appkit's plain modules, the
//! tests) takes [`new_uuid`]: the same kind of id, 128 random bits from two
//! seeds.

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

/// A new random id for a record file (`contacts/<id>.vcf`): a version 4 UUID
/// in its usual 8-4-4-4-12 lowercase form, from two [`random_seed`]s. For
/// code without azul; with azul, `Uuid::from_seed(random_seed())` is the
/// same kind of id.
#[must_use]
pub fn new_uuid() -> String {
    uuid_from_words(random_seed(), random_seed())
}

/// The UUID text of 128 bits, with the version (4) and variant (10xx) bits set.
#[must_use]
pub fn uuid_from_words(hi: u64, lo: u64) -> String {
    let hi = (hi & 0xffff_ffff_ffff_0fff) | 0x0000_0000_0000_4000;
    let lo = (lo & 0x3fff_ffff_ffff_ffff) | 0x8000_0000_0000_0000;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        hi >> 32,
        (hi >> 16) & 0xffff,
        hi & 0xffff,
        lo >> 48,
        lo & 0xffff_ffff_ffff
    )
}

/// Whether `text` is a UUID in the 8-4-4-4-12 hex form (any case).
#[must_use]
pub fn is_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::{is_uuid, new_uuid, random_seed, uuid_from_words};

    #[test]
    fn two_seeds_differ() {
        let seeds: std::collections::HashSet<u64> = (0..1000).map(|_| random_seed()).collect();
        assert_eq!(seeds.len(), 1000, "a thousand seeds, a thousand values");
    }

    #[test]
    fn a_new_uuid_is_a_lowercase_version_4_uuid_and_never_repeats() {
        let ids: std::collections::HashSet<String> = (0..1000).map(|_| new_uuid()).collect();
        assert_eq!(ids.len(), 1000, "a thousand ids, a thousand values");
        for id in &ids {
            assert!(is_uuid(id), "{id}");
            assert_eq!(id.as_bytes()[14], b'4', "the version digit: {id}");
            assert!(
                matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'),
                "the variant: {id}"
            );
            assert_eq!(*id, id.to_lowercase());
            assert!(
                !id.starts_with("00000000-0000"),
                "not the process-local marker sequence: {id}"
            );
        }
    }

    #[test]
    fn the_uuid_text_of_fixed_bits_is_stable() {
        assert_eq!(
            uuid_from_words(0, 0),
            "00000000-0000-4000-8000-000000000000"
        );
        assert_eq!(
            uuid_from_words(u64::MAX, u64::MAX),
            "ffffffff-ffff-4fff-bfff-ffffffffffff"
        );
    }

    #[test]
    fn is_uuid_accepts_the_8_4_4_4_12_hex_form_only() {
        assert!(is_uuid("00000000-0000-4000-8000-000000000000"));
        assert!(is_uuid("ABCDEF01-2345-4678-9ABC-DEF012345678"), "any case");
        assert!(!is_uuid("not-a-uuid"));
        assert!(
            !is_uuid("00000000-0000-4000-8000-00000000000"),
            "one digit short"
        );
        assert!(!is_uuid("00000000-0000-4000-8000-00000000000g"), "not hex");
        assert!(!is_uuid(""));
    }
}
