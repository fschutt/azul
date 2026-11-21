//! The errors of envelope v2: one small `Copy` enum per layer, no strings and no data that could
//! carry a key or a plaintext, so a `Display` can be logged as it is and a caller maps each
//! variant to its own status (planning/architecture.md section 5.5; build unit S15, lane
//! W1-S15b).
//!
//! - [`EnvelopeError`]: the KEY-LESS layer. Bytes that are no envelope, an envelope that breaks
//!   the field rules of its algorithm, and a signature that does not verify. The node calls only
//!   this layer (`check_shape`, `verify_signature`).
//! - [`WrapError`]: why no envelope was made. After an error NO envelope exists.
//! - [`OpenError`]: why an envelope did not open. The checks run in a fixed order and the
//!   variants say how far it got; the AEAD failure is one word, [`OpenError::Auth`], whichever
//!   part of the ciphertext, the header or the key was wrong.

use core::fmt;

use azlin_cbor::{DecodeError, EncodeError, ShapeError};

use super::alg_table::Alg;
use super::ctx::CtxError;
use crate::error::EntropyError;

/// Why the `params` map of an envelope is refused. The maps are closed: exactly the keys of the
/// algorithm, each of its own type and range (DRIFT X-DELTA-W1S15B-1, -11: the plan gives
/// the field no shape).
///
/// Caller: INT-S15-2 (the node maps it to `bad_request`), S15d and S17 (they read `params`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamsError {
    /// `params` is not a map.
    NotAMap,
    /// The map has a wrong shape: a key is missing, has the wrong type or length, or a key is
    /// not an unsigned integer.
    Shape(ShapeError),
    /// The map holds a key the algorithm does not define (the maps are closed: new parameters
    /// need a new version).
    UnknownKey(u64),
    /// A value is out of its range (the integer key of the parameter).
    OutOfRange(u64),
    /// The `params` are of another algorithm than the envelope names.
    WrongKind,
}

impl fmt::Display for ParamsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParamsError::NotAMap => f.write_str("params is not a map"),
            ParamsError::Shape(e) => write!(f, "params: {e}"),
            ParamsError::UnknownKey(k) => write!(f, "params holds the unknown key {k}"),
            ParamsError::OutOfRange(k) => write!(f, "params key {k} is out of range"),
            ParamsError::WrongKind => f.write_str("params are of another algorithm"),
        }
    }
}

impl std::error::Error for ParamsError {}

impl From<ShapeError> for ParamsError {
    fn from(e: ShapeError) -> ParamsError {
        ParamsError::Shape(e)
    }
}

/// Why bytes are no envelope, an envelope breaks the rules of its algorithm, or its signature
/// does not verify: everything a verifier that holds no key can say.
///
/// Caller: `Envelope::from_cbor`, `check_shape` and `verify_signature` (INT-S15-2: the node
/// refuses a `reg_wrap` write with it), `open` (inside [`OpenError::Malformed`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnvelopeError {
    /// The bytes are not strict deterministic CBOR (truncated, trailing bytes, a non-shortest
    /// integer, duplicate or unsorted keys, a float, a tag ...).
    Cbor(DecodeError),
    /// The CBOR is not the shape of an envelope: a key is missing, has the wrong type or the
    /// wrong length, `crit` is malformed, or `crit` names a key this reader does not know.
    Shape(ShapeError),
    /// `v` is not 2.
    UnsupportedVersion(u64),
    /// `alg` is none of the seven (the number it names).
    UnknownAlg(u64),
    /// The algorithm requires this field (its integer key) and it is absent.
    FieldRequired(u64),
    /// The algorithm forbids this field (its integer key) and it is present.
    FieldForbidden(u64),
    /// `ctx` breaks the grammar.
    BadCtx(CtxError),
    /// `params` break the rules of the algorithm.
    BadParams(ParamsError),
    /// An unknown entry is not an unsigned-integer key outside the known keys and key 0, or two
    /// unknown entries share a key (only a hand-built envelope has this; the decoder cannot).
    BadUnknown,
    /// `crit` lists key 0 or a key this version does not know (only a hand-built envelope has
    /// this; the decoder refuses it as [`EnvelopeError::Shape`]).
    BadCrit,
    /// The bytes are longer than `MAX_ENVELOPE` (`check_shape` only: a wrap stored in the
    /// registry is small).
    TooLarge,
    /// The envelope breaks a limit of the wire format when it is written.
    Encode(EncodeError),
    /// The envelope does not write back to the bytes it was read from. The strict decoder makes
    /// this impossible; it is checked anyway because a signature covers canonical bytes.
    NotCanonical,
    /// The signature does not verify (also: a signature that is not canonical, a signer key of
    /// small order, a signature over other bytes).
    BadSignature,
    /// The directory knows no such signer.
    UnknownSigner,
    /// The signer's key was not valid yet at the point asked about.
    SignerNotYetActive,
    /// The signer's key was revoked at the point asked about.
    SignerRevoked,
    /// The directory could not answer. Never read as "unknown signer".
    DirectoryUnavailable,
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvelopeError::Cbor(e) => write!(f, "not strict deterministic CBOR: {e}"),
            EnvelopeError::Shape(e) => write!(f, "not the shape of an envelope: {e}"),
            EnvelopeError::UnsupportedVersion(v) => {
                write!(f, "envelope version {v} is not supported")
            }
            EnvelopeError::UnknownAlg(n) => write!(f, "envelope algorithm {n} is unknown"),
            EnvelopeError::FieldRequired(k) => {
                write!(f, "field {k} is required by the algorithm")
            }
            EnvelopeError::FieldForbidden(k) => {
                write!(f, "field {k} is forbidden by the algorithm")
            }
            EnvelopeError::BadCtx(e) => write!(f, "bad ctx: {e}"),
            EnvelopeError::BadParams(e) => write!(f, "bad params: {e}"),
            EnvelopeError::BadUnknown => f.write_str("an unknown entry is not allowed"),
            EnvelopeError::BadCrit => f.write_str("crit is not allowed"),
            EnvelopeError::TooLarge => f.write_str("the envelope is larger than the limit"),
            EnvelopeError::Encode(e) => write!(f, "the envelope cannot be written: {e}"),
            EnvelopeError::NotCanonical => {
                f.write_str("the envelope does not write back to the same bytes")
            }
            EnvelopeError::BadSignature => f.write_str("the signature does not verify"),
            EnvelopeError::UnknownSigner => f.write_str("the signer is unknown"),
            EnvelopeError::SignerNotYetActive => f.write_str("the signer's key is not active yet"),
            EnvelopeError::SignerRevoked => f.write_str("the signer's key is revoked"),
            EnvelopeError::DirectoryUnavailable => {
                f.write_str("the signer directory is unavailable")
            }
        }
    }
}

impl std::error::Error for EnvelopeError {}

/// Why no envelope was made. After an error NO envelope exists and no partial value is
/// returned.
///
/// Caller: `wrap_x25519`, `wrap_hybrid`, `seal_with_key`, `seal_under` and their callers
/// (INT-S15-1, S15c1, S15d).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WrapError {
    /// The entropy seam failed. Nothing was made from zeroes.
    Entropy(EntropyError),
    /// The algorithm is not in this build (algorithm 2 without the feature `pq`) or not in this
    /// crate (algorithms 6 and 7: S17). Refused before any entropy is drawn.
    Unsupported(Alg),
    /// This entry point does not make envelopes of this algorithm (`seal_with_key` is for
    /// algorithms 3 to 7; algorithms 1 and 2 have their own constructions).
    WrongAlg(Alg),
    /// The recipient's ML-KEM encapsulation key is not 1,184 bytes or is not the canonical
    /// encoding of a key (FIPS 203 section 7.2's modulus check).
    BadRecipientKey,
    /// The key exchange gave the all-zero shared secret: the recipient key (or the ephemeral
    /// key) is of low order. Such a secret never keys anything.
    NonContributory,
    /// The plaintext is longer than `MAX_PLAINTEXT`.
    PlaintextTooLarge,
    /// The cipher refused to seal (cannot happen for inputs within the limits).
    Cipher,
    /// The KEM refused to encapsulate (cannot happen for a checked key).
    Kem,
    /// The envelope that was built breaks the rules of its algorithm (a hand-made `Header`).
    Envelope(EnvelopeError),
}

impl fmt::Display for WrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WrapError::Entropy(e) => write!(f, "no envelope: {e}"),
            WrapError::Unsupported(a) => {
                write!(f, "algorithm {} is not supported here", a.number())
            }
            WrapError::WrongAlg(a) => {
                write!(
                    f,
                    "this function does not make algorithm {} envelopes",
                    a.number()
                )
            }
            WrapError::BadRecipientKey => f.write_str("the recipient's ML-KEM key is not valid"),
            WrapError::NonContributory => {
                f.write_str("the key exchange is non-contributory (a key of low order)")
            }
            WrapError::PlaintextTooLarge => f.write_str("the plaintext is larger than the limit"),
            WrapError::Cipher => f.write_str("the cipher refused to seal"),
            WrapError::Kem => f.write_str("the KEM refused to encapsulate"),
            WrapError::Envelope(e) => write!(f, "the envelope is not valid: {e}"),
        }
    }
}

impl std::error::Error for WrapError {}

impl From<EntropyError> for WrapError {
    fn from(e: EntropyError) -> WrapError {
        WrapError::Entropy(e)
    }
}

impl From<EnvelopeError> for WrapError {
    fn from(e: EnvelopeError) -> WrapError {
        WrapError::Envelope(e)
    }
}

/// Why an envelope did not open. `open` checks, in this order: the format and the algorithm
/// ([`OpenError::Malformed`], [`OpenError::Unsupported`]), the context ([`OpenError::CtxMismatch`]),
/// the recipient ([`OpenError::NotForMe`]), the signature (the four signer errors and
/// [`OpenError::BadSignature`]), and last the AEAD ([`OpenError::Auth`]). Nothing is decrypted
/// before the signature has verified.
///
/// Caller: `open`, `open_with_key`, `open_under` and their callers (INT-S15-1, S15c1, S15d).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpenError {
    /// The envelope breaks the field rules of its algorithm (only a hand-built envelope: the
    /// decoder already refused such bytes).
    Malformed(EnvelopeError),
    /// The algorithm's construction is not in this build (algorithm 2 without the feature `pq`)
    /// or not in this function (algorithms 3 to 5 open with their own functions in S15d, 6 and
    /// 7 in S17).
    Unsupported(Alg),
    /// This entry point does not open envelopes of this algorithm.
    WrongAlg(Alg),
    /// The envelope is for another context than the caller expects (checked before the
    /// signature and before any decryption).
    CtxMismatch,
    /// The envelope's `rcpt` is not the key id of this recipient.
    NotForMe,
    /// The signature does not verify.
    BadSignature,
    /// The directory knows no such signer.
    UnknownSigner,
    /// The signer's key was not valid yet at the point asked about.
    SignerNotYetActive,
    /// The signer's key was revoked at the point asked about.
    SignerRevoked,
    /// The directory could not answer.
    DirectoryUnavailable,
    /// The key exchange with the envelope's ephemeral key gave the all-zero secret (a key of
    /// low order). This depends on the public key alone.
    NonContributory,
    /// The ciphertext does not authenticate under the derived key and the header. One word for
    /// every cause (a wrong key, a changed header field, a changed ciphertext or nonce, a
    /// corrupted KEM ciphertext: ML-KEM rejects implicitly and the AEAD then fails).
    Auth,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::Malformed(e) => write!(f, "the envelope is malformed: {e}"),
            OpenError::Unsupported(a) => {
                write!(f, "algorithm {} is not supported here", a.number())
            }
            OpenError::WrongAlg(a) => {
                write!(
                    f,
                    "this function does not open algorithm {} envelopes",
                    a.number()
                )
            }
            OpenError::CtxMismatch => f.write_str("the envelope is for another context"),
            OpenError::NotForMe => f.write_str("the envelope is not for this recipient"),
            OpenError::BadSignature => f.write_str("the signature does not verify"),
            OpenError::UnknownSigner => f.write_str("the signer is unknown"),
            OpenError::SignerNotYetActive => f.write_str("the signer's key is not active yet"),
            OpenError::SignerRevoked => f.write_str("the signer's key is revoked"),
            OpenError::DirectoryUnavailable => f.write_str("the signer directory is unavailable"),
            OpenError::NonContributory => {
                f.write_str("the key exchange is non-contributory (a key of low order)")
            }
            OpenError::Auth => f.write_str("the ciphertext does not authenticate"),
        }
    }
}

impl std::error::Error for OpenError {}

/// The `OpenError` of an `EnvelopeError` met on the way to the AEAD: the five signature
/// refusals keep their own names, everything else is a malformed envelope.
pub(crate) fn open_error_of(e: EnvelopeError) -> OpenError {
    match e {
        EnvelopeError::BadSignature => OpenError::BadSignature,
        EnvelopeError::UnknownSigner => OpenError::UnknownSigner,
        EnvelopeError::SignerNotYetActive => OpenError::SignerNotYetActive,
        EnvelopeError::SignerRevoked => OpenError::SignerRevoked,
        EnvelopeError::DirectoryUnavailable => OpenError::DirectoryUnavailable,
        other => OpenError::Malformed(other),
    }
}
