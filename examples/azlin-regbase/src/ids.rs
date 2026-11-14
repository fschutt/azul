//! Opaque ids and counters of the registry (planning/architecture.md section 5.3 "`member_id`
//! (pseudonymous)", section 2.4 "which metadata group", section 5.13 "the entry's Raft log index"
//! and "a set of (group, group epoch, index)"; build unit S9, lane W1-S9a).
//!
//! The ids of accounts, members, devices, spaces and user groups are ONE definition for the whole
//! engine: [`azlin_keys::ids`](azlin_keys) (COMMON-W1R R3a); they are re-exported here, never
//! defined twice. This module adds the ids and counters that only the registry knows:
//!
//! | type | what | size |
//! |---|---|---|
//! | [`FamilyId`] | a token family (`reg_family.family_id`) | 16 bytes |
//! | [`CredentialNonce`] | the nonce of one credential (`reg_revocation` subject) | 16 bytes |
//! | [`TokenHash`] | BLAKE3 of a token secret under `azlin-token-hash-v1`: not a secret, never the token | 32 bytes |
//! | [`VoterId`] | the Raft node id of a voter | `u64` |
//! | [`MetaGroupId`] | which METADATA group (group 0 is the first); not a user group | `u16` |
//! | [`MetaGroupEpoch`] | raised by an account move or a restore (the cursor of section 5.13) | `u64` |
//! | [`LogIndex`] | the Raft log index of a committed entry | `u64` |
//! | [`KeyEpoch`] | the epoch of a metadata group's identity or credential key | `u32` |
//! | [`Generation`] | how many times a token family has rotated | `u32` |
//!
//! The 16- and 32-byte ids are random bytes chosen by the PROPOSER of an entry (an apply draws
//! nothing: [`FamilyId::generate`] takes the entropy seam); their text is lower-case hex and
//! [`FromStr`] / `parse_hex` accept nothing else. None has a `Default`: an id is made from bytes,
//! from text or by `generate`. `Debug` of an id of this module prints at most the first four
//! bytes, so a log line never carries a whole family id or hash.
//!
//! ```compile_fail,E0599
//! // no id has a Default: an all-zero id is a value somebody must choose.
//! let _ = azlin_regbase::FamilyId::default();
//! ```
//!
//! ```compile_fail,E0599
//! let _ = azlin_regbase::TokenHash::default();
//! ```
//!
//! ```compile_fail,E0308
//! // ids of different types do not unify.
//! fn takes_family(_: azlin_regbase::FamilyId) {}
//! takes_family(azlin_regbase::CredentialNonce::from_bytes([0; 16]));
//! ```

use core::fmt;
use core::hash::{Hash, Hasher};
use core::str::FromStr;

use azlin_keys::{Entropy, EntropyError, Secret};
use subtle::{Choice, ConstantTimeEq};

pub use azlin_keys::{AccountId, DeviceId, GroupId, MemberId, SpaceId};

use crate::error::IdParseError;

/// The id of a user group (a group of members): `reg_group.group_id` of section 5.3, the
/// `admin_scope` of a group admin. It is `azlin_keys::GroupId`; S17 (the team log) owns the groups.
///
/// Caller: S9e (member assertions list a member's groups), S9g (`GroupsView`), S17c.
pub type UserGroupId = GroupId;

/// How many leading bytes `Debug` of an id prints.
const DEBUG_PREFIX_LEN: usize = 4;

/// The value of a lower-case hex digit; `None` for anything else (upper case included).
fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

/// Strict hex: exactly `2 * N` lower-case hex digits, nothing else.
fn parse_hex<const N: usize>(text: &str) -> Result<[u8; N], IdParseError> {
    let digits = text.as_bytes();
    if digits.len() != N.saturating_mul(2) {
        return Err(IdParseError::Length);
    }
    let mut out = [0u8; N];
    // The length is exactly `2 * N`, so the pairs cover every digit and nothing is left over.
    let (pairs, _) = digits.as_chunks::<2>();
    for (byte, [high, low]) in out.iter_mut().zip(pairs) {
        match (nibble(*high), nibble(*low)) {
            (Some(high), Some(low)) => *byte = (high << 4) | low,
            _ => return Err(IdParseError::BadDigit),
        }
    }
    Ok(out)
}

/// Writes `bytes` as lower-case hex (two digits per byte).
fn write_hex<'a>(
    f: &mut fmt::Formatter<'_>,
    bytes: impl IntoIterator<Item = &'a u8>,
) -> fmt::Result {
    for byte in bytes {
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}

/// One id of fixed length: bytes, strict hex text, `generate` from the entropy seam, a `Debug`
/// that prints a prefix only, no `Default`.
macro_rules! byte_id {
    ($(#[$meta:meta])* $name:ident, $len:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; $len]);

        impl $name {
            /// The number of bytes of the id.
            pub const LEN: usize = $len;

            /// The id with these bytes.
            pub const fn from_bytes(bytes: [u8; $len]) -> $name {
                $name(bytes)
            }

            /// The bytes of the id.
            pub const fn as_bytes(&self) -> &[u8; $len] {
                &self.0
            }

            /// The text of the id: lower-case hex, two digits per byte.
            pub fn to_hex(&self) -> String {
                self.to_string()
            }

            /// The id of this text. Strict: exactly the id's length in lower-case hex digits, no
            /// upper case, no whitespace, no prefix.
            pub fn parse_hex(text: &str) -> Result<$name, IdParseError> {
                parse_hex::<$len>(text).map($name)
            }

            /// A fresh random id from the entropy seam; on an entropy failure there is no id
            /// (never an id of zeroes). The PROPOSER of an entry calls it, an apply never does.
            pub fn generate(ent: &mut dyn Entropy) -> Result<$name, EntropyError> {
                let mut bytes = [0u8; $len];
                ent.fill(&mut bytes)?;
                Ok($name(bytes))
            }
        }

        /// Lower-case hex.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_hex(f, &self.0)
            }
        }

        /// The first four bytes and `..`: never the whole id.
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "("))?;
                write_hex(f, self.0.iter().take(DEBUG_PREFIX_LEN))?;
                f.write_str("..)")
            }
        }

        /// Strict, as [`parse_hex`](Self::parse_hex).
        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(text: &str) -> Result<$name, IdParseError> {
                $name::parse_hex(text)
            }
        }
    };
}

/// One counter: a plain number with a CHECKED successor (a counter never wraps), a decimal
/// `Display`, no `Default`.
macro_rules! counter {
    ($(#[$meta:meta])* $name:ident, $int:ty) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($int);

        impl $name {
            /// The number zero.
            pub const ZERO: $name = $name(0);

            /// The counter with this number.
            pub const fn new(number: $int) -> $name {
                $name(number)
            }

            /// The number.
            pub const fn get(self) -> $int {
                self.0
            }

            /// The counter after this one; `None` at the largest number (a counter never wraps).
            pub fn next(self) -> Option<$name> {
                self.0.checked_add(1).map($name)
            }
        }

        /// Decimal.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }
    };
}

byte_id! {
    /// The id of a token family: `reg_family.family_id` (planning/architecture.md section 5.3
    /// "Devices use rotating families"). Chosen by the proposer of the entry that makes the
    /// family.
    ///
    /// Caller: S9b (refresh, the new family of a key-proof refresh), S9c (enroll finish), S9g
    /// (a credential names its family).
    FamilyId, 16
}

byte_id! {
    /// The nonce of one credential (planning/architecture.md section 5.11 `nonce`; section 5.3
    /// `reg_revocation` "credential nonce"). Chosen by the leader before it proposes the refresh.
    ///
    /// Caller: S9b (the refresh command), S9c (`RevokeCredential`), S9g (a revoked nonce refuses a
    /// credential).
    CredentialNonce, 16
}

counter! {
    /// The Raft node id of a voter of the metadata group (today a `u64`).
    ///
    /// Caller: azlin-registry's `VoterSet` and the sealed key blobs (S9f).
    VoterId, u64
}

counter! {
    /// Which METADATA group (planning/architecture.md section 2.4: "Accounts map to a metadata
    /// group"); group 0 is the first. It is not a user group ([`UserGroupId`]).
    ///
    /// Caller: `ApplyCtx`, `reg_account.meta_group`, the anchors' identity keys, the key epochs.
    MetaGroupId, u16
}

impl MetaGroupId {
    /// The first metadata group: the one a cluster starts with, and the home of the
    /// cluster-wide tables.
    pub const FIRST: MetaGroupId = MetaGroupId(0);
}

counter! {
    /// The epoch of a metadata group's index space: raised by an account move or a restore, so a
    /// cursor of the event feed from an older epoch gets `resync` (planning/architecture.md
    /// section 5.13 "Resume from a cursor").
    ///
    /// Caller: the event feed's cursor (S33), S34b (an account move raises it).
    MetaGroupEpoch, u64
}

counter! {
    /// The Raft log index of a committed entry: events are derived with the entry's index and a
    /// duplicate is recognized by it (planning/architecture.md section 5.13 "Where events come
    /// from").
    ///
    /// Caller: `ApplyCtx::index`, `DerivedEvent::index`.
    LogIndex, u64
}

counter! {
    /// The epoch of a metadata group's identity or credential key (planning/architecture.md
    /// section 2.4: "The credential key rotates every 30 days"); a credential names the epoch it
    /// was signed under (section 5.11 `key_epoch`).
    ///
    /// Caller: S9e (certificates and claims), S9f (rotation), S9g (a revoked epoch refuses).
    KeyEpoch, u32
}

counter! {
    /// How many times a token family has rotated: 0 for a family whose first token was never
    /// rotated away (planning/architecture.md section 5.3 `reg_family`; the old code's
    /// `generation`).
    ///
    /// Caller: azlin-registry's `family` rules, S9b (the grant names its generation).
    Generation, u32
}

/// The hash of a token secret: BLAKE3 in its `derive_key` mode under the context
/// [`TokenHash::DOMAIN`] (`azlin-token-hash-v1`) over the secret's bytes. It is what the registry
/// stores of a token (`reg_family.current` and the used hashes); it is NOT a secret and never the
/// token, and it cannot be turned back into one.
///
/// Equality is CONSTANT TIME (the comparison of a presented token's hash with a stored one must
/// not tell where the bytes differ): `==` on two hashes is safe by construction, and
/// [`TokenHash::ct_choice`] gives the `subtle::Choice` for code that must not branch on it.
/// `Ord` and `Hash` are bytewise (map and digest order); `Debug` prints four bytes at most.
///
/// Implements planning/architecture.md section 5.3 (`reg_family`: "current and previous token
/// hash"); the hash domain is registered for uniqueness in W1/CONFLICTS.md C-12.
///
/// Caller: S9b (`token_hash(secret)` of a new token, `classify` of a presented one), S9c.
#[derive(Clone, Copy)]
pub struct TokenHash([u8; 32]);

impl TokenHash {
    /// The number of bytes of a token hash.
    pub const LEN: usize = 32;

    /// The BLAKE3 context a token secret is hashed under.
    pub const DOMAIN: &'static str = "azlin-token-hash-v1";

    /// The hash with these bytes (as read back from a row).
    pub const fn from_bytes(bytes: [u8; 32]) -> TokenHash {
        TokenHash(bytes)
    }

    /// The bytes of the hash.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The hash of a token's secret: the one derivation of a token hash (a second copy anywhere
    /// is a defect: a hash made another way never matches).
    pub fn of_secret<const N: usize>(secret: &Secret<N>) -> TokenHash {
        TokenHash(blake3::derive_key(TokenHash::DOMAIN, secret.expose()))
    }

    /// Whether the two hashes are equal, in constant time.
    pub fn ct_equal(&self, other: &TokenHash) -> bool {
        bool::from(self.ct_choice(other))
    }

    /// The constant-time equality as a `Choice`, for a classification that must not short-circuit
    /// on which hash matched.
    pub fn ct_choice(&self, other: &TokenHash) -> Choice {
        self.0.as_slice().ct_eq(other.0.as_slice())
    }

    /// The text of the hash: 64 lower-case hex digits.
    pub fn to_hex(&self) -> String {
        self.to_string()
    }

    /// The hash of this text. Strict: exactly 64 lower-case hex digits.
    pub fn parse_hex(text: &str) -> Result<TokenHash, IdParseError> {
        parse_hex::<32>(text).map(TokenHash)
    }
}

/// Constant time.
impl PartialEq for TokenHash {
    fn eq(&self, other: &TokenHash) -> bool {
        self.ct_equal(other)
    }
}

impl Eq for TokenHash {}

impl PartialOrd for TokenHash {
    fn partial_cmp(&self, other: &TokenHash) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TokenHash {
    fn cmp(&self, other: &TokenHash) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl Hash for TokenHash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// Lower-case hex.
impl fmt::Display for TokenHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

/// The first four bytes and `..`: never the whole hash.
impl fmt::Debug for TokenHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TokenHash(")?;
        write_hex(f, self.0.iter().take(DEBUG_PREFIX_LEN))?;
        f.write_str("..)")
    }
}

/// Strict, as [`TokenHash::parse_hex`].
impl FromStr for TokenHash {
    type Err = IdParseError;

    fn from_str(text: &str) -> Result<TokenHash, IdParseError> {
        TokenHash::parse_hex(text)
    }
}
