//! The errors of the recovery module: one small `Copy` enum, no strings and no data that could
//! carry a key, a code or a share, so a `Display` can be logged as it is and a caller maps each
//! variant to its own message (planning/architecture.md section 5.5 "Recovery"; build unit S15,
//! lane W1-S15d).
//!
//! The envelope layer's own words are kept where they say something the caller acts on: a refused
//! context, a signature that does not verify, a signer that is unknown or revoked, a directory that
//! cannot answer ([`RecoveryError::Open`]); the cipher's refusal is one word, [`RecoveryError::Auth`]
//! (a wrong code, a wrong passkey output, a changed header or ciphertext all look the same).

use core::fmt;

use crate::envelope::{Alg, OpenError, WrapError};
use crate::error::EntropyError;

/// Why a recovery operation did not happen or did not give a key. After an error NO partial value
/// exists: no wrap, no share, no code.
///
/// Caller: every function of the module (INT-S15-1: the setup and recovery screens map each variant
/// to its own sentence).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecoveryError {
    /// The entropy seam failed. Nothing was made from zeroes.
    Entropy(EntropyError),
    /// Sealing failed for a reason other than the entropy (the envelope layer's word).
    Wrap(WrapError),
    /// The envelope layer refused before the cipher: a malformed envelope, a context that is not
    /// the one asked for, a recipient that is not this one, a signature or a signer that is not
    /// right (the envelope layer's word; the cipher's refusal is [`RecoveryError::Auth`]).
    Open(OpenError),
    /// The cipher refused: a wrong recovery code, a wrong passkey output, or a header, parameter or
    /// ciphertext that was changed. One word for every cause.
    Auth,
    /// The envelope is of another algorithm than the function opens.
    WrongAlg(Alg),
    /// The algorithm is a shape only in this crate (6 and 7: lane S17).
    Unsupported(Alg),
    /// The context is not `recovery/<member>/<method>` of the method the function handles.
    WrongCtx,
    /// The recovery code's text does not have 26 symbols.
    CodeLength,
    /// The recovery code's text holds a character that is no Crockford symbol (`U` included).
    CodeCharacter,
    /// The two padding bits of the recovery code's last symbol are not zero.
    CodePadding,
    /// The envelope's Argon2id cost is below the floor this device accepts (a server that lowers
    /// it, or an old weak wrap replayed). Refused before anything is hashed.
    BelowFloor,
    /// The envelope's Argon2id cost is above what a device derives with (memory, passes, lanes or
    /// the work of them together). Refused before any memory is asked for.
    AboveLimit,
    /// The key derivation could not run (the memory could not be had, or the cost is no
    /// parameters set of the Argon2id implementation).
    Kdf,
    /// A split that cannot be made: the threshold is below 2 or above the number of shares.
    BadSplit,
    /// Fewer shares than the threshold (or none).
    NotEnough,
    /// A share at index 0 (the secret itself).
    ShareIndex,
    /// The same share index twice.
    DuplicateShare,
    /// Shares of different splits (another code, another threshold or another count).
    MixedSplits,
    /// A share that cannot come from a split: a threshold below 2 or above the count, or an index
    /// above the count.
    BadShare,
    /// The shares recombine to something that is not the code they split: one is mistyped,
    /// changed, or of another split of the same code.
    CheckMismatch,
    /// The owner's words are too long or hold a control character.
    BadWords,
    /// The text does not start with the prefix of its form.
    TextPrefix,
    /// The text has the wrong number of digits or fields for its form.
    TextLength,
    /// The text holds a character that is not allowed: a hex digit must be a lower case digit.
    TextDigit,
    /// The text is longer than the limit of its form.
    TextTooLong,
    /// The hex of a sealed share or a reply is no envelope.
    NotAnEnvelope,
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecoveryError::Entropy(e) => write!(f, "nothing was made: {e}"),
            RecoveryError::Wrap(e) => write!(f, "the wrap was not made: {e}"),
            RecoveryError::Open(e) => write!(f, "the wrap was refused: {e}"),
            RecoveryError::Auth => {
                f.write_str("the wrap does not open: wrong code or key, or it was changed")
            }
            RecoveryError::WrongAlg(a) => {
                write!(
                    f,
                    "this function does not open algorithm {} envelopes",
                    a.number()
                )
            }
            RecoveryError::Unsupported(a) => {
                write!(f, "algorithm {} is not supported here", a.number())
            }
            RecoveryError::WrongCtx => {
                f.write_str("the context is not the recovery context of this method")
            }
            RecoveryError::CodeLength => f.write_str("a recovery code has 26 symbols"),
            RecoveryError::CodeCharacter => {
                f.write_str("a symbol of the recovery code is not allowed")
            }
            RecoveryError::CodePadding => {
                f.write_str("the last symbol of the recovery code is not valid")
            }
            RecoveryError::BelowFloor => f.write_str("the key derivation cost is below the floor"),
            RecoveryError::AboveLimit => f.write_str("the key derivation cost is above the limit"),
            RecoveryError::Kdf => f.write_str("the key derivation could not run"),
            RecoveryError::BadSplit => f.write_str(
                "a split needs a threshold of at least 2 and at most the number of shares",
            ),
            RecoveryError::NotEnough => f.write_str("not enough shares"),
            RecoveryError::ShareIndex => f.write_str("a share at index 0"),
            RecoveryError::DuplicateShare => f.write_str("the same share twice"),
            RecoveryError::MixedSplits => f.write_str("shares of different splits"),
            RecoveryError::BadShare => f.write_str("not a share of any split"),
            RecoveryError::CheckMismatch => {
                f.write_str("the shares do not give back the code they split")
            }
            RecoveryError::BadWords => f.write_str("the owner's words are not allowed"),
            RecoveryError::TextPrefix => f.write_str("the text starts with the wrong prefix"),
            RecoveryError::TextLength => f.write_str("the text has the wrong length"),
            RecoveryError::TextDigit => {
                f.write_str("the text holds a character that is not allowed")
            }
            RecoveryError::TextTooLong => f.write_str("the text is longer than the limit"),
            RecoveryError::NotAnEnvelope => f.write_str("the text is no envelope"),
        }
    }
}

impl std::error::Error for RecoveryError {}

impl From<EntropyError> for RecoveryError {
    fn from(e: EntropyError) -> RecoveryError {
        RecoveryError::Entropy(e)
    }
}

/// An entropy failure inside a wrap is the same word as one outside it.
impl From<WrapError> for RecoveryError {
    fn from(e: WrapError) -> RecoveryError {
        match e {
            WrapError::Entropy(e) => RecoveryError::Entropy(e),
            other => RecoveryError::Wrap(other),
        }
    }
}

/// The cipher's refusal is `Auth`; the other words of the envelope layer are kept.
impl From<OpenError> for RecoveryError {
    fn from(e: OpenError) -> RecoveryError {
        match e {
            OpenError::Auth => RecoveryError::Auth,
            OpenError::WrongAlg(a) => RecoveryError::WrongAlg(a),
            OpenError::Unsupported(a) => RecoveryError::Unsupported(a),
            other => RecoveryError::Open(other),
        }
    }
}
