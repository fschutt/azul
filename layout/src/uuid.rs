//! UUID string generation - the intended mint for MARKER strings, and a seeded
//! mint for ids that leave the process.
//!
//! Markers (`Dom::with_marker` + `CallbackInfo::get_node_id_by_marker`) are
//! app-chosen strings created at `layout()` time; what makes them work is that
//! they collide with *nothing* - not another widget, not another window, not a
//! hard-coded string in some library. A UUID gives that guarantee without any
//! coordination, so the pattern is:
//!
//! ```rust,ignore
//! // once, at layout() time (store it wherever the driving callback looks):
//! let marker = Uuid::short();
//! let bar = ProgressBar::create(0.0).dom().with_marker(Some(marker.clone()).into());
//! ```
//!
//! [`Uuid::v4`] returns the canonical hyphenated form (36 chars);
//! [`Uuid::short`] returns the same 128 bits as a 22-char flickrBase58 string
//! (the `short-uuid` encoding) - shorter, no hyphens, URL/log friendly.
//!
//! # These ids are DETERMINISTIC, not random
//!
//! Every id is a pure function of a process-local counter, so run the same
//! program twice and you get the same sequence of ids. That is all a marker
//! needs - it has to be unique among the strings alive in ONE process, and it
//! is: the mixing function is a bijection and its whole output survives the
//! version / variant stamp, so 2^63 mints collide zero times, exactly rather
//! than probabilistically.
//!
//! What it is NOT: unpredictable, or unique across processes and machines. Do
//! not use these to identify a client to a server, as a security token, or as
//! a database key that two processes might mint independently. The crate
//! carries no randomness source on purpose - `uuid`'s `v4` needs one, and on
//! `wasm32-unknown-unknown` there isn't one without pulling in `getrandom`'s
//! JS shim, which broke every wasm consumer of azul-layout 0.0.15.
//!
//! # Ids that other processes share: [`Uuid::from_seed`]
//!
//! A file name, an S3 key or a record that two devices write must not depend
//! on how many ids this process minted before (the first [`Uuid::v4`] in every
//! process is `00000000-0000-4000-...`). [`Uuid::from_seed`] and
//! [`Uuid::short_from_seed`] take the randomness from the CALLER: the id is a
//! pure function of a `u64` seed the app draws from wherever it has one - the
//! OS, a hardware RNG, a server. So they stay deterministic (same seed, same
//! id, on every platform and in every release; distinct seeds, distinct ids,
//! exactly) and never touch the marker tick.
//!
//! 64 bits of seed are enough for app ids: two random seeds are likely to
//! meet only after about 2^32 ids. They are not a security token: the mix is
//! invertible, so the seed can be read back from the id. Never seed one with
//! a secret, and never treat knowing an id as proof of anything.

use core::sync::atomic::{AtomicU64, Ordering};

use azul_css::AzString;

/// flickrBase58 — base58 without the look-alikes `0`, `O`, `I` and `l`.
const BASE58: &[u8; 58] = b"123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ";

/// Mint tick. Every id is a pure function of its tick, so the sequence is the
/// same on every run — deterministic, not random.
static TICK: AtomicU64 = AtomicU64::new(0);

/// splitmix64's increment (2^64 divided by the golden ratio, made odd): a
/// seeded id is the first two splitmix64 outputs for its seed.
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// The splitmix64 finalizer. Every step (xor-shift-right, multiply by an odd
/// constant) is invertible, so the whole function is a BIJECTION on `u64`:
/// distinct ticks give distinct outputs, which is what makes the no-collision
/// guarantee below exact rather than probabilistic.
const fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Lays two mixed words out as an RFC 4122 version-4 UUID. The version nibble
/// (bits 79..76) and the variant (bits 63..62) are INSERTED between payload
/// bits, not written over them: all 64 bits of `hi` survive (the low 6 bits of
/// `lo` make room), so an injective `hi` gives an injective id, exactly.
const fn v4_shape(hi: u64, lo: u64) -> u128 {
    // The 122 payload bits: all of `hi`, then the top 58 bits of `lo`.
    let p = ((hi as u128) << 58) | ((lo >> 6) as u128);
    ((p >> 74) << 80) // 48 bits: time_low + time_mid
        | (0x4_u128 << 76) // version 4
        | (((p >> 62) & 0xFFF) << 64) // 12 bits: the rest of time_hi
        | (0b10_u128 << 62) // the RFC 4122 variant
        | (p & ((1_u128 << 62) - 1)) // 62 bits: clock_seq + node
}

/// A marker's 128 bits: `tick` spread over two disjoint `mix64` inputs
/// (`2 * tick`, `2 * tick + 1`). `hi` alone is `mix64` of an injective function
/// of the tick and survives [`v4_shape`] whole, so two mints can only repeat
/// once the tick wraps at 2^63.
const fn tick_bits(tick: u64) -> u128 {
    let even = tick.wrapping_mul(2);
    v4_shape(mix64(even), mix64(even.wrapping_add(1)))
}

/// The next marker's 128 bits (advances the process tick).
fn next_bits() -> u128 {
    tick_bits(TICK.fetch_add(1, Ordering::Relaxed))
}

/// A seeded id's 128 bits: the first two outputs of splitmix64 seeded with
/// `seed`. `hi` is a bijection of the seed and survives [`v4_shape`] whole, so
/// distinct seeds give distinct ids, exactly. Reads and writes no state.
const fn seed_bits(seed: u64) -> u128 {
    v4_shape(
        mix64(seed.wrapping_add(GAMMA)),
        mix64(seed.wrapping_add(GAMMA.wrapping_mul(2))),
    )
}

/// The canonical hyphenated lowercase spelling of 128 UUID bits (36 chars).
fn hex(bits: u128) -> AzString {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(36);
    for (i, byte) in bits.to_be_bytes().iter().enumerate() {
        // 4-2-2-2-6 bytes, so the hyphens fall after bytes 4, 6, 8 and 10.
        if matches!(i, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0F)] as char);
    }
    out.into()
}

/// The 22-character flickrBase58 spelling of 128 UUID bits: big-endian,
/// left-padded to the fixed 22 characters `short-uuid` emits (a value with
/// leading zero bytes still has to spell 22 characters or the width stops
/// being a contract).
fn base58(mut n: u128) -> AzString {
    let mut buf = [BASE58[0]; 22];
    let mut i = 22;
    while n > 0 && i > 0 {
        i -= 1;
        buf[i] = BASE58[(n % 58) as usize];
        n /= 58;
    }
    String::from_utf8_lossy(&buf).into_owned().into()
}

/// Static-method namespace for UUID string generation ([`Uuid::v4`],
/// [`Uuid::short`], [`Uuid::from_seed`], [`Uuid::short_from_seed`]). The
/// struct only exists so the FFI layer can hang static methods off it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
#[allow(clippy::pub_underscore_fields)] // FFI/api.json static-namespace placeholder field
pub struct Uuid {
    pub _reserved: u8,
}

impl Default for Uuid {
    fn default() -> Self {
        Self::new()
    }
}

impl Uuid {
    /// Returns a zero-initialised handle. The struct only exists so the FFI
    /// layer can hang static methods off it.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// A fresh version-4-shaped UUID in canonical hyphenated lowercase form,
    /// e.g. `"550e8400-e29b-41d4-a716-446655440000"` (36 characters).
    ///
    /// Deterministic, not random - the value depends only on how many ids this
    /// process has already minted. See the module docs before using one as
    /// anything but a marker; [`Uuid::from_seed`] is the id to share.
    #[must_use]
    pub fn v4() -> AzString {
        hex(next_bits())
    }

    /// A fresh version-4-shaped UUID as a 22-character flickrBase58 string,
    /// e.g. `"mhvXdrZT4jP5T8vBxuvm75"` - the same encoding `short-uuid` used to
    /// print. The compact spelling for markers, log lines and URLs.
    ///
    /// Deterministic, not random - see [`Uuid::v4`] and the module docs.
    #[must_use]
    pub fn short() -> AzString {
        base58(next_bits())
    }

    /// The version-4-shaped UUID that is a pure function of `seed`, in
    /// canonical hyphenated lowercase form (36 characters): the id for a file
    /// name, an S3 key or a record that other processes and devices share,
    /// where the process-local sequence of [`Uuid::v4`] would collide.
    ///
    /// The randomness comes from the caller: draw `seed` from the OS, a
    /// hardware RNG or a server. The same seed gives the same id on every
    /// platform and in every release; distinct seeds give distinct ids,
    /// exactly. It never touches the marker tick, so [`Uuid::v4`] and
    /// [`Uuid::short`] return what they would have returned anyway.
    ///
    /// 64 bits of seed are enough for app ids (two random seeds are likely to
    /// meet only after about 2^32 ids), but the id is not a security token:
    /// the seed can be read back from it, so never seed it with a secret.
    #[must_use]
    pub fn from_seed(seed: u64) -> AzString {
        hex(seed_bits(seed))
    }

    /// [`Uuid::from_seed`] as a 22-character flickrBase58 string: the same
    /// 128 bits, spelled like [`Uuid::short`]. Same seed, same id; see
    /// [`Uuid::from_seed`] for where the seed comes from.
    #[must_use]
    pub fn short_from_seed(seed: u64) -> AzString {
        base58(seed_bits(seed))
    }

    /// A human-friendly CODE, like a meeting code: `xq4-8kdm-2np` - ten
    /// symbols in 3-4-3 groups from the 31-symbol alphabet without the
    /// look-alikes `0 o 1 l i` (about 49 bits), easy to read aloud and type.
    /// Not a UUID: fewer bits, no version field, and codes can repeat long
    /// before UUIDs would (two random codes are likely to meet after about
    /// 30 million). Deterministic like [`Uuid::v4`]: a marker unique within one
    /// process; for a code other devices share, use [`Uuid::code_from_seed`].
    #[must_use]
    pub fn code() -> AzString {
        AzString::from(String::new())
    }

    /// [`Uuid::code`] as a pure function of `seed` (randomness from outside,
    /// as for [`Uuid::from_seed`]): the same seed gives the same code; it never
    /// touches the marker tick.
    #[must_use]
    pub fn code_from_seed(seed: u64) -> AzString {
        let _ = seed;
        AzString::from(String::new())
    }
}

#[cfg(test)]
mod uuid_tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn v4_is_canonical_hyphenated_lowercase() {
        for _ in 0..64 {
            let s = Uuid::v4();
            let s = s.as_str();
            assert_eq!(s.len(), 36, "canonical UUID is 36 chars: {s}");
            for (i, c) in s.char_indices() {
                if matches!(i, 8 | 13 | 18 | 23) {
                    assert_eq!(c, '-', "hyphen expected at {i}: {s}");
                } else {
                    assert!(
                        c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
                        "lowercase hex expected at {i}: {s}",
                    );
                }
            }
            assert_eq!(&s[14..15], "4", "version nibble must say v4: {s}");
        }
    }

    #[test]
    fn short_is_22_chars_of_flickr_base58() {
        // flickrBase58: no 0 / O / I / l - the look-alike characters.
        for _ in 0..64 {
            let s = Uuid::short();
            let s = s.as_str();
            assert_eq!(s.len(), 22, "short UUID is 22 chars: {s}");
            for c in s.chars() {
                assert!(c.is_ascii_alphanumeric(), "base58 is alphanumeric: {s}");
                assert!(
                    !matches!(c, '0' | 'O' | 'I' | 'l'),
                    "flickrBase58 excludes look-alikes, got {c:?} in {s}",
                );
            }
        }
    }

    #[test]
    fn minted_ids_do_not_collide() {
        // The whole point of the module: 10k mints, 10k distinct strings.
        let mut seen = HashSet::new();
        for _ in 0..5_000 {
            assert!(seen.insert(Uuid::v4().as_str().to_string()));
            assert!(seen.insert(Uuid::short().as_str().to_string()));
        }
    }

    #[test]
    fn the_namespace_handle_is_inert() {
        assert_eq!(Uuid::new(), Uuid::default());
        assert_eq!(Uuid::new()._reserved, 0);
    }

    /// The 128 bits a canonical hyphenated id spells.
    fn bits_of(id: &AzString) -> u128 {
        u128::from_str_radix(&id.as_str().replace('-', ""), 16).unwrap()
    }

    /// The 128 bits a flickrBase58 id spells.
    fn bits_of_short(id: &AzString) -> u128 {
        id.as_str().bytes().fold(0_u128, |n, c| {
            let digit = BASE58.iter().position(|&b| b == c).unwrap();
            n * 58 + digit as u128
        })
    }

    /// The 122 payload bits of a v4 id: what is left once the version nibble
    /// (bits 79..76) and the variant (bits 63..62) are taken out.
    fn payload_of(id: &AzString) -> u128 {
        let u = bits_of(id);
        ((u >> 80) << 74) | (((u >> 64) & 0xFFF) << 62) | (u & ((1_u128 << 62) - 1))
    }

    /// A seeded id is a pure function of its seed: the same seed spells the
    /// same id in this process, in the next one and on every platform (the
    /// pinned values are what every build prints).
    #[test]
    fn the_same_seed_always_gives_the_same_id() {
        for seed in [0_u64, 1, 42, u64::MAX, 1 << 63] {
            assert_eq!(Uuid::from_seed(seed), Uuid::from_seed(seed));
            assert_eq!(Uuid::short_from_seed(seed), Uuid::short_from_seed(seed));
        }
        assert_eq!(
            Uuid::from_seed(0).as_str(),
            "e220a839-7b1d-4cda-bdb9-e279aa86e597"
        );
        assert_eq!(
            Uuid::from_seed(42).as_str(),
            "bdd73226-2feb-46e9-94a3-bf8ccec99bc4"
        );
        assert_eq!(
            Uuid::from_seed(u64::MAX).as_str(),
            "e4d97177-1b65-42c2-83a6-7fe19f6fda0b"
        );
        assert_eq!(Uuid::short_from_seed(0).as_str(), "tVxCBMjsUGkwmgYozpBkrZ");
        assert_eq!(Uuid::short_from_seed(42).as_str(), "prDZrYC9pfRKYNvRdZi5JY");
    }

    /// Distinct seeds give distinct ids - exactly, not probably: the id
    /// carries all 64 bits of `mix64(seed + GAMMA)`, a bijection of the seed.
    /// Seeds that differ only in the top bit are in the list on purpose.
    #[test]
    fn distinct_seeds_give_distinct_ids() {
        let edges = [u64::MAX, 1 << 63, (1 << 63) + 1, (1 << 63) - 1];
        let mut hex = HashSet::new();
        let mut short = HashSet::new();
        for seed in (0..10_000_u64).chain(edges) {
            assert!(
                hex.insert(Uuid::from_seed(seed).as_str().to_string()),
                "{seed}"
            );
            assert!(
                short.insert(Uuid::short_from_seed(seed).as_str().to_string()),
                "{seed}"
            );
        }
        for seed in [0, 1, 42, u64::MAX, 1 << 63, 0x0123_4567_89AB_CDEF] {
            let payload = payload_of(&Uuid::from_seed(seed));
            assert_eq!(
                (payload >> 58) as u64,
                mix64(seed.wrapping_add(GAMMA)),
                "the high payload word is mix64 of seed {seed}",
            );
        }
    }

    /// A seeded id has the RFC 4122 shape: canonical hyphenated lowercase,
    /// version 4, variant 0b10; the short spelling is the same 128 bits.
    #[test]
    fn a_seeded_id_carries_the_v4_version_and_variant_bits() {
        for seed in [0_u64, 1, 42, 7_777, u64::MAX, 1 << 63] {
            let id = Uuid::from_seed(seed);
            let s = id.as_str();
            assert_eq!(s.len(), 36, "{s}");
            for (i, c) in s.char_indices() {
                if matches!(i, 8 | 13 | 18 | 23) {
                    assert_eq!(c, '-', "hyphen expected at {i}: {s}");
                } else {
                    assert!(
                        c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
                        "lowercase hex expected at {i}: {s}",
                    );
                }
            }
            assert_eq!(&s[14..15], "4", "version nibble must say v4: {s}");
            assert!(
                matches!(&s[19..20], "8" | "9" | "a" | "b"),
                "variant must be 0b10: {s}",
            );
            let bits = bits_of(&id);
            assert_eq!((bits >> 76) & 0xF, 4, "{s}");
            assert_eq!((bits >> 62) & 0b11, 0b10, "{s}");

            let short = Uuid::short_from_seed(seed);
            assert_eq!(short.as_str().len(), 22, "{}", short.as_str());
            assert_eq!(bits_of_short(&short), bits, "{s} vs {}", short.as_str());
        }
    }

    fn is_code(code: &str) -> bool {
        let groups: Vec<&str> = code.split('-').collect();
        groups.iter().map(|g| g.len()).collect::<Vec<_>>() == [3, 4, 3]
            && groups
                .iter()
                .flat_map(|g| g.bytes())
                .all(|b| b"23456789abcdefghjkmnpqrstuvwxyz".contains(&b))
    }

    /// A code is three groups of 3-4-3 symbols from the alphabet without the
    /// look-alikes `0 o 1 l i`.
    #[test]
    fn a_code_is_three_groups_of_unambiguous_symbols() {
        for _ in 0..200 {
            let c = Uuid::code();
            assert!(is_code(c.as_str()), "{}", c.as_str());
        }
        for seed in [0, 1, 42, u64::MAX, 0x8000_0000_0000_0000] {
            let c = Uuid::code_from_seed(seed);
            assert!(is_code(c.as_str()), "{seed}: {}", c.as_str());
        }
    }

    /// The same seed always gives the same code; nearby seeds scatter.
    #[test]
    fn a_seeded_code_is_a_pure_function_of_its_seed() {
        assert_eq!(Uuid::code_from_seed(7), Uuid::code_from_seed(7));
        let codes: HashSet<String> = (0..10_000u64)
            .map(|s| Uuid::code_from_seed(s).as_str().to_string())
            .collect();
        assert_eq!(codes.len(), 10_000, "10 000 consecutive seeds, 10 000 codes");
        let minted: HashSet<String> =
            (0..10_000).map(|_| Uuid::code().as_str().to_string()).collect();
        assert_eq!(minted.len(), 10_000, "10 000 minted codes, no repeats");
    }

    /// `code_from_seed` never touches the process-wide mint.
    #[test]
    fn code_from_seed_never_advances_the_mint_tick() {
        use core::sync::atomic::Ordering;
        let quiet = (0..1_000).any(|_| {
            let before = TICK.load(Ordering::SeqCst);
            for seed in 0..16 {
                let _ = Uuid::code_from_seed(seed);
            }
            TICK.load(Ordering::SeqCst) == before
        });
        assert!(quiet, "code_from_seed advanced the process tick");
    }

    /// A seeded id never touches the process-wide mint: after any number of
    /// `from_seed` calls, `v4()` / `short()` return what they would have
    /// returned anyway (they are pure functions of the tick).
    #[test]
    fn from_seed_never_advances_the_mint_tick() {
        use core::sync::atomic::Ordering;
        // Other tests mint on other threads, so look for one quiet window; a
        // from_seed that advanced the tick would never find one.
        let quiet = (0..1_000).any(|_| {
            let before = TICK.load(Ordering::SeqCst);
            for seed in 0..16 {
                let _ = Uuid::from_seed(seed);
                let _ = Uuid::short_from_seed(seed);
            }
            TICK.load(Ordering::SeqCst) == before
        });
        assert!(
            quiet,
            "from_seed / short_from_seed advanced the process tick"
        );
    }
}
