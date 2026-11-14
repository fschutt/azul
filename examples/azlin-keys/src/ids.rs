//! Typed ids: the one definition of `AccountId`, `MemberId`, `DeviceId`, `SpaceId`, `GroupId`,
//! `KeyId`, `Epoch` and `ChainSeq` for the whole engine (COMMON-W1R R3: azlin-regbase and
//! azlin-registry re-export them; the iso crates use them through a path dependency).
//!
//! Implements planning/architecture.md section 5.3 (the registry's `account_id`, `member_id`,
//! `device_id`, `space_id`, `group_id`), section 5.5 (`rcpt`, the key id an epoch is committed
//! under) and section 5.9 (positions in a chain). Build unit S15, lane W1-S15a.
//!
//! - The five 16-byte ids are opaque random bytes; the text form is 32 lowercase hex digits and
//!   [`FromStr`] accepts nothing else (no uppercase, no whitespace, no `0x`, no other length).
//!   They have no `Default`: an id is made from bytes, from text or by `generate`.
//! - Ids of different types do not unify:
//!
//! ```compile_fail
//! use azlin_keys::{DeviceId, SpaceId};
//! fn takes_space(_: SpaceId) {}
//! takes_space(DeviceId::from_bytes([0; 16]));
//! ```
//!
//! The same call with the right type compiles, and the text form round-trips:
//!
//! ```
//! use azlin_keys::{SpaceId, DeviceId};
//! fn takes_space(_: SpaceId) {}
//! takes_space(SpaceId::from_bytes([0; 16]));
//!
//! let d = DeviceId::from_bytes([0xab; 16]);
//! assert_eq!(d.to_string(), "abababababababababababababababab");
//! assert_eq!(d.to_string().parse::<DeviceId>(), Ok(d));
//! assert!("ABABABABABABABABABABABABABABABAB".parse::<DeviceId>().is_err());
//! ```

use core::fmt;
use core::str::FromStr;

use crate::derive::{self, KEY_ID};
use crate::error::{EntropyError, IdError};
use crate::seam::Entropy;

/// Writes `bytes` as lowercase hex (two digits per byte).
pub(crate) fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}

/// The value of a lowercase hex digit; `None` for anything else (uppercase included).
fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

/// Strict hex: exactly `2 * N` lowercase hex digits, nothing else.
pub(crate) fn parse_hex<const N: usize>(text: &str) -> Result<[u8; N], IdError> {
    let digits = text.as_bytes();
    if digits.len() != N.saturating_mul(2) {
        return Err(IdError::Length);
    }
    let mut out = [0u8; N];
    // The length is exactly `2 * N`, so the pairs cover every digit and nothing is left over.
    let (pairs, _) = digits.as_chunks::<2>();
    for (byte, [high, low]) in out.iter_mut().zip(pairs) {
        match (nibble(*high), nibble(*low)) {
            (Some(high), Some(low)) => *byte = (high << 4) | low,
            _ => return Err(IdError::BadDigit),
        }
    }
    Ok(out)
}

/// One 16-byte id type: bytes, lowercase hex `Display`, strict `FromStr`, `generate` from the
/// entropy seam, no `Default`.
macro_rules! id16 {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 16]);

        impl $name {
            /// The number of bytes of the id.
            pub const LEN: usize = 16;

            /// The id with these bytes.
            pub const fn from_bytes(bytes: [u8; 16]) -> $name {
                $name(bytes)
            }

            /// The bytes of the id.
            pub const fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }

            /// A fresh random id: 16 bytes from the entropy seam. On an entropy failure there is
            /// no id (never an id of zeroes).
            pub fn generate(ent: &mut dyn Entropy) -> Result<$name, EntropyError> {
                let mut bytes = [0u8; 16];
                ent.fill(&mut bytes)?;
                Ok($name(bytes))
            }
        }

        /// 32 lowercase hex digits.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_hex(f, &self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self)
            }
        }

        /// Strict: exactly 32 lowercase hex digits.
        impl FromStr for $name {
            type Err = IdError;

            fn from_str(text: &str) -> Result<$name, IdError> {
                parse_hex::<16>(text).map($name)
            }
        }
    };
}

id16! {
    /// The id of an account (a family, a team, a business): `account_id` of `reg_account` and of
    /// every `reg_*` row (planning/architecture.md section 5.3).
    ///
    /// Caller: every lane (S9 registry, S13 entitlements, S16/S17 spaces and the team log).
    AccountId
}

id16! {
    /// The pseudonymous id of a member (a person or an app): `member_id` of `reg_member`
    /// (planning/architecture.md section 5.3).
    ///
    /// Caller: every lane (S9, S15c1 member keys, S16 grants, S17 the team log).
    MemberId
}

id16! {
    /// The id of a device: `device_id` of `reg_device`; the `signer` of an envelope, the
    /// `author_device` of a pointer or a log record (planning/architecture.md sections 5.3, 5.5).
    ///
    /// Caller: every lane (S9 devices, S15b envelopes, S14d pointers, S17 the team log).
    DeviceId
}

id16! {
    /// The id of a space (a repository or an append log): `space_id` of `reg_space`; the first
    /// segment of an envelope's `ctx` (planning/architecture.md sections 5.4, 5.5).
    ///
    /// Caller: every lane (S15c1 space keys, S14 content, S16 spaces).
    SpaceId
}

id16! {
    /// The id of a group of members: `group_id` of `reg_group`; a group key is per group and
    /// epoch (planning/architecture.md sections 5.5, 5.6).
    ///
    /// Caller: S15c1 (group keys), S15c2 (rekey plans), S16/S17 (grants to groups).
    GroupId
}

/// The id of a recipient's public key bundle: the `rcpt` of an envelope and the "recipient key id"
/// of a `reg_wrap` row (planning/architecture.md section 5.5), and the id a signed chain entry
/// commits an epoch's key under. 32 bytes of BLAKE3, 64 lowercase hex digits as text.
///
/// Caller: S15b (`rcpt`), S15c1 (`PublicBundle::key_id`, `SpaceKey::key_id`), S15c2 (`reg_wrap`
/// rows), S16b (chain commitments).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyId([u8; 32]);

impl KeyId {
    /// The number of bytes of the id.
    pub const LEN: usize = 32;

    /// The key id with these bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> KeyId {
        KeyId(bytes)
    }

    /// The bytes of the key id.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The id of the recipient bundle `(kx, pq)`: `kx` is the X25519 public key and `pq` the
    /// ML-KEM-768 encapsulation key when the recipient has one. It is
    /// `derive_key("azlin key id v1", kx || pq)`: one id per bundle, so the `rcpt` of an envelope
    /// names exactly the keys it was wrapped to. `Some` of an empty slice is the same bundle as
    /// `None` (no `pq` key).
    ///
    /// DRIFT X-DELTA-W1S15A-1: section 5.5 says `rcpt` is "BLAKE3 of the recipient's public key",
    /// and a recipient has up to three public keys; this is the one place that reading lives.
    ///
    /// Caller: S15b (`rcpt` of every wrap), S15c1 (`PublicBundle::key_id`).
    pub fn of_bundle(kx: &[u8; 32], pq: Option<&[u8]>) -> KeyId {
        let material = match pq {
            Some(pq) => [kx.as_slice(), pq].concat(),
            None => kx.to_vec(),
        };
        KeyId(derive::derive_public(&KEY_ID, &material))
    }
}

/// 64 lowercase hex digits.
impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeyId({self})")
    }
}

/// Strict: exactly 64 lowercase hex digits.
impl FromStr for KeyId {
    type Err = IdError;

    fn from_str(text: &str) -> Result<KeyId, IdError> {
        parse_hex::<32>(text).map(KeyId)
    }
}

/// A key epoch: the generation of a group key or a space key, starting at 1
/// (planning/architecture.md section 5.5 "per epoch"). Epoch 0 does not exist; the next epoch is
/// checked, so a counter never wraps.
///
/// Caller: S15b (`ctx` of an envelope), S15c1 (group and space keys, the epoch chain), S15c2
/// (authorized epochs, `reg_wrap` rows), S16b (chain commitments).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Epoch(u32);

impl Epoch {
    /// The first epoch of a key.
    pub const FIRST: Epoch = Epoch(1);

    /// The epoch with this number; `None` for 0.
    pub const fn new(number: u32) -> Option<Epoch> {
        if number == 0 {
            None
        } else {
            Some(Epoch(number))
        }
    }

    /// The number of the epoch (at least 1).
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The epoch after this one; `None` at the largest number.
    pub fn next(self) -> Option<Epoch> {
        self.0.checked_add(1).map(Epoch)
    }

    /// The epoch before this one; `None` for the first epoch.
    pub fn prev(self) -> Option<Epoch> {
        self.0.checked_sub(1).and_then(Epoch::new)
    }
}

/// Decimal.
impl fmt::Display for Epoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Epoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Epoch({})", self.0)
    }
}

/// Strict decimal: digits only, no sign, no leading zero, 1 to 4,294,967,295.
impl FromStr for Epoch {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Epoch, IdError> {
        let digits = text.as_bytes();
        if digits.is_empty() {
            return Err(IdError::Length);
        }
        if !digits.iter().all(u8::is_ascii_digit) {
            return Err(IdError::BadDigit);
        }
        if digits.len() > 1 && digits.first() == Some(&b'0') {
            return Err(IdError::BadDigit);
        }
        match text.parse::<u32>() {
            Ok(number) => Epoch::new(number).ok_or(IdError::OutOfRange),
            Err(_) => Err(IdError::OutOfRange),
        }
    }
}

/// A position in a space chain or in the team log: the sequence number of an entry
/// (planning/architecture.md section 5.9). The next position is checked.
///
/// Caller: S15a's `VerifyAt::ChainSeq` and `SpaceChainView`, S16 (space chains), S17 (the team
/// log), S15c2 (`FreshHead`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainSeq(u64);

impl ChainSeq {
    /// The smallest position.
    pub const MIN: ChainSeq = ChainSeq(0);

    /// The position with this number.
    pub const fn new(number: u64) -> ChainSeq {
        ChainSeq(number)
    }

    /// The number of the position.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The position after this one; `None` at the largest number.
    pub fn next(self) -> Option<ChainSeq> {
        self.0.checked_add(1).map(ChainSeq)
    }
}

/// Decimal.
impl fmt::Display for ChainSeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for ChainSeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChainSeq({})", self.0)
    }
}
