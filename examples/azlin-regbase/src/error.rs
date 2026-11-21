//! The typed errors of azlin-regbase (planning/architecture.md section 1.4 rule 4 "Everything has
//! a limit, stated up front"; section 5.3 "pseudonymous" ids; section 2.4 "device key").
//!
//! Every refusal is a typed value that names what was refused and nothing of the refused bytes: no
//! error carries a key, a signature or a sealed blob (QUALITY rule 26). A refusal of a bound does
//! not truncate: the value is not made.

/// Why a text is not an id: an id has exactly one spelling, lower-case hex digits of the id's
/// length and nothing else (no upper case, no whitespace, no `0x`, no other length).
///
/// Implements planning/architecture.md section 5.3 (ids are opaque and pseudonymous).
///
/// Caller: `parse_hex` and `FromStr` of every id of this crate; S9b (the text of a token).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdParseError {
    /// The text does not have exactly twice the id's number of bytes in characters.
    #[error("an id has exactly twice its byte length in hex digits")]
    Length,
    /// A character that is not a lower-case hex digit.
    #[error("an id is lower-case hex digits only")]
    BadDigit,
}

/// Why a value could not be made within its bound: the bounded newtypes of
/// [`crate::bytes`] refuse, they never truncate.
///
/// Implements planning/architecture.md section 1.4 rule 4 (limits are stated up front).
///
/// Caller: every constructor of [`crate::bytes`] and [`crate::anchors::AnchorSet::check`]; the
/// rows of azlin-registry (S9a), S9c (sealed blobs), S9d (features, regions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BoundError {
    /// A byte string longer than its limit.
    #[error("{what}: {len} bytes, the most is {max}")]
    TooLong {
        /// Which type refused.
        what: &'static str,
        /// The length offered.
        len: usize,
        /// The limit.
        max: usize,
    },
    /// A set with more items than its limit.
    #[error("{what}: {count} items, the most is {max}")]
    TooMany {
        /// Which type refused.
        what: &'static str,
        /// The count offered.
        count: usize,
        /// The limit.
        max: usize,
    },
    /// A byte string that must not be empty.
    #[error("{what}: must not be empty")]
    Empty {
        /// Which type refused.
        what: &'static str,
    },
    /// A text with a byte outside printable ASCII (or, for a word, outside lower-case letters,
    /// digits and underscore).
    #[error("{what}: not an allowed character")]
    BadChar {
        /// Which type refused.
        what: &'static str,
    },
}

/// Why a key proof does not hold (planning/architecture.md section 2.4 "refresh: device key or
/// token"). The verification refusal is one word, as azlin-keys' `SignError::Bad` is: it says
/// nothing about which part (purpose, account, subject, challenge, signature, key) failed.
///
/// Caller: `verify_key_proof` and `KeyProof::from_parts`; S9b (device and app refresh), S9c
/// (`device_enroll_finish`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProofError {
    /// The signature does not verify under the key for exactly this purpose, account, subject and
    /// challenge.
    #[error("the key proof does not verify")]
    Bad,
    /// The signature bytes are not 64 bytes long.
    #[error("a signature is 64 bytes")]
    SignatureLength,
}

/// Why a text is not a client version `major.minor.patch`.
///
/// Caller: `ClientVersion::parse`; S9b (`client_too_old` against `reg_account.min_client`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VersionError {
    /// Not three dot-separated numbers.
    #[error("a version is major.minor.patch")]
    Shape,
    /// A part that is not a plain decimal number below 65,536 (no sign, no leading zero).
    #[error("a version part is a plain decimal number below 65536")]
    Number,
}
