//! The errors of the vocabulary modules (`ids`, `seam`, `sign`): one small `Copy` enum per kind of
//! failure, no strings as control flow and no data that could carry a secret, so a caller maps each
//! to its own status and a `Display` can be logged as it is.
//!
//! UNKNOWN IS A STATE: an entropy source that failed, a clock that cannot be read, a directory
//! that is down and a chain that cannot be read are errors that stop the operation, never a zero,
//! a default key or a `false` (planning/architecture.md section 1.4 rules 1 and 11; build unit S15,
//! lane W1-S15a). [`KeysError`] gathers them for a caller that wants one type.

use core::fmt;

/// Why a text is not an id, a key or an epoch. Parsing is strict: the exact length, lowercase hex
/// digits and nothing else; an epoch is plain decimal without sign or leading zero.
///
/// Caller: every lane that reads an id from text (the `ctx` grammar of S15b, the text forms of
/// S15d, the registry's columns at S9 integration).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdError {
    /// The text is not exactly as long as the form (32 digits for a 16-byte id, 64 for a 32-byte
    /// id or key, 128 for a signature), or it is empty.
    Length,
    /// A character is not allowed: not a lowercase hex digit; for an epoch not a decimal digit, or
    /// a leading zero.
    BadDigit,
    /// A decimal epoch outside 1 to 4,294,967,295.
    OutOfRange,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            IdError::Length => "the text has the wrong length",
            IdError::BadDigit => "the text holds a character that is not allowed",
            IdError::OutOfRange => "the number is out of range",
        })
    }
}

impl std::error::Error for IdError {}

/// Why an entropy source gave no bytes. After an error the buffer holds NO usable bytes: a caller
/// returns the error and never goes on with zeroes.
///
/// Caller: every generator (ids, keys, nonces, ephemeral keys) of this crate and its dependents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntropyError {
    /// The operating system's source failed (`getrandom`).
    Os,
    /// The source has no more bytes to give (a simulated source that ran out, or one that failed
    /// before and stays failed).
    Exhausted,
}

impl fmt::Display for EntropyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EntropyError::Os => "the operating system's random source failed",
            EntropyError::Exhausted => "the random source has no more bytes",
        })
    }
}

impl std::error::Error for EntropyError {}

/// Why a clock gave no time. An unreadable clock is an error, never the time 0 (QUALITY rule 11).
///
/// Caller: every component that stamps, expires or compares time through the `Clock` seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClockError {
    /// The clock cannot be read.
    Unreadable,
    /// The clock reads a time before 1970-01-01 (a value that is no `UnixTime`).
    BeforeEpoch,
}

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ClockError::Unreadable => "the clock cannot be read",
            ClockError::BeforeEpoch => "the clock reads a time before 1970",
        })
    }
}

impl std::error::Error for ClockError {}

/// Why a [`crate::DeviceDirectory`] has no key for a device at a point of time or of a chain.
///
/// Caller: the directories (the registry at S9 integration, the team log at S17, `MapDirectory`
/// in tests) answer with it; `sign::verify_with` and S15b/S15c2 map it to their own errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DirectoryError {
    /// No such device.
    Unknown,
    /// The device exists but its key is not valid yet (pending, or the point asked about is
    /// before it was added).
    NotYetActive,
    /// The device's key is revoked (or was, at the point asked about).
    Revoked,
    /// The directory cannot answer (a store failure). Never `Unknown`: an unreadable store is not
    /// an absent device.
    Unavailable,
}

impl fmt::Display for DirectoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DirectoryError::Unknown => "the device is unknown",
            DirectoryError::NotYetActive => "the device's key is not active yet",
            DirectoryError::Revoked => "the device's key is revoked",
            DirectoryError::Unavailable => "the device directory is unavailable",
        })
    }
}

impl std::error::Error for DirectoryError {}

/// Why a signature is refused. `Bad` covers everything the mathematics refuses (a wrong message,
/// domain, key or signature, a non-canonical signature, a key of small order, bytes that are no
/// curve point) in one word, so the refusal says nothing about which part failed; the other
/// variants are the directory's answers, kept distinct because a caller reacts to them
/// differently (a revoked signer is not an unknown one).
///
/// Caller: S15b (envelope signatures), S14d (pointers), S16/S17 (log records, chain entries) and
/// the registry's `reg_wrap` validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SignError {
    /// The signature does not verify.
    Bad,
    /// The directory knows no such signer.
    UnknownSigner,
    /// The signer's key is not valid yet.
    NotYetActive,
    /// The signer's key is revoked.
    Revoked,
    /// The directory could not answer.
    DirectoryUnavailable,
}

impl fmt::Display for SignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SignError::Bad => "the signature does not verify",
            SignError::UnknownSigner => "the signer is unknown",
            SignError::NotYetActive => "the signer's key is not active yet",
            SignError::Revoked => "the signer's key is revoked",
            SignError::DirectoryUnavailable => "the signer directory is unavailable",
        })
    }
}

impl std::error::Error for SignError {}

impl From<DirectoryError> for SignError {
    fn from(e: DirectoryError) -> SignError {
        match e {
            DirectoryError::Unknown => SignError::UnknownSigner,
            DirectoryError::NotYetActive => SignError::NotYetActive,
            DirectoryError::Revoked => SignError::Revoked,
            DirectoryError::Unavailable => SignError::DirectoryUnavailable,
        }
    }
}

/// Why a [`crate::SpaceChainView`] cannot answer. `Ok(None)` ("the chain has no commitment") and
/// an error are different answers: an unreadable chain is Unknown, never "not authorized".
///
/// Caller: S16b's verified space chains implement the view; S15c2's `authorize_epoch` and
/// `RekeyPlan::is_stale` map it to their refusals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChainError {
    /// The chain cannot be read (a store or transport failure).
    Unreadable,
    /// No chain is known for the space at all.
    UnknownSpace,
    /// The chain fails its own verification (a bad signature or a broken link): nothing it says
    /// can be trusted.
    Corrupt,
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ChainError::Unreadable => "the space chain cannot be read",
            ChainError::UnknownSpace => "no chain is known for the space",
            ChainError::Corrupt => "the space chain fails its own verification",
        })
    }
}

impl std::error::Error for ChainError {}

/// One error type over the vocabulary modules, for a caller that does not want to name each.
///
/// Caller: INT-S15-1 (the client's key flows) and any lane that returns vocabulary errors
/// unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeysError {
    /// A text is not an id, key or epoch.
    Id(IdError),
    /// An entropy source failed.
    Entropy(EntropyError),
    /// A clock cannot be read.
    Clock(ClockError),
    /// A signature is refused.
    Sign(SignError),
    /// A device directory has no key.
    Directory(DirectoryError),
    /// A space chain cannot answer.
    Chain(ChainError),
}

impl fmt::Display for KeysError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeysError::Id(e) => fmt::Display::fmt(e, f),
            KeysError::Entropy(e) => fmt::Display::fmt(e, f),
            KeysError::Clock(e) => fmt::Display::fmt(e, f),
            KeysError::Sign(e) => fmt::Display::fmt(e, f),
            KeysError::Directory(e) => fmt::Display::fmt(e, f),
            KeysError::Chain(e) => fmt::Display::fmt(e, f),
        }
    }
}

impl std::error::Error for KeysError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            KeysError::Id(e) => Some(e),
            KeysError::Entropy(e) => Some(e),
            KeysError::Clock(e) => Some(e),
            KeysError::Sign(e) => Some(e),
            KeysError::Directory(e) => Some(e),
            KeysError::Chain(e) => Some(e),
        }
    }
}

impl From<IdError> for KeysError {
    fn from(e: IdError) -> KeysError {
        KeysError::Id(e)
    }
}

impl From<EntropyError> for KeysError {
    fn from(e: EntropyError) -> KeysError {
        KeysError::Entropy(e)
    }
}

impl From<ClockError> for KeysError {
    fn from(e: ClockError) -> KeysError {
        KeysError::Clock(e)
    }
}

impl From<SignError> for KeysError {
    fn from(e: SignError) -> KeysError {
        KeysError::Sign(e)
    }
}

impl From<DirectoryError> for KeysError {
    fn from(e: DirectoryError) -> KeysError {
        KeysError::Directory(e)
    }
}

impl From<ChainError> for KeysError {
    fn from(e: ChainError) -> KeysError {
        KeysError::Chain(e)
    }
}
