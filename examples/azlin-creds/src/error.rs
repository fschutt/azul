//! The typed errors of azlin-creds (planning/architecture.md section 1.4 rules 4 and 10: every limit
//! refuses with a typed error, every error has one class; QUALITY rule 26: no error text carries a
//! key, a signature or a sealed byte).
//!
//! Every refusal is a small `Copy` value that names WHAT was refused and nothing of the refused bytes.
//! A verifier answers with exactly one of them, and the order in which the checks run is part of the
//! contract (documented at `verify_credential` and `verify_assertion`): a caller maps each refusal to its
//! own status, and the property test of the crate holds the order against an independent oracle.
//!
//! | enum | who returns it |
//! |---|---|
//! | [`CanonicalError`] | the encoders: a structure that cannot be written as canonical bytes |
//! | [`WireError`] | the decoders: bytes that are not the strict encoding of one structure |
//! | [`CertError`] | `certify`, `verify_cert` |
//! | [`CredError`] | `verify_credential` |
//! | [`AssertError`] | `verify_assertion` |
//! | [`EntitlementError`] | `verify_entitlement`, `sign_entitlement` |
//! | [`IssueError`], [`IssuerError`] | the issuers: a claim set they refuse, a probe that failed |

use azlin_cbor::{DecodeError, ShapeError};

use crate::signer::SignError;

/// Why a structure could not be written as canonical bytes: the bytes that are signed are the ONE
/// deterministic encoding of a value, so a value that has none is refused and never "fixed".
///
/// Caller: `Canonical::canonical_bytes` and every `encode`; the issuers map it to `IssueError::Encode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CanonicalError {
    /// The structure is larger than the codec's limits for it.
    #[error("the structure is larger than the codec's limits")]
    TooLarge,
    /// A map of the structure could not be built (a programming error: a key put twice).
    #[error("a map of the structure could not be built")]
    Map,
    /// A set that is not in its one canonical form: not sorted, with a duplicate, or above its limit.
    /// Two encodings of one set must never both verify, so the encoder refuses the second form too.
    #[error("a set is not in its one canonical form")]
    NotCanonical,
    /// A retention that is not a whole number of days (the wire holds days, never seconds).
    #[error("a retention is not a whole number of days")]
    NotWholeDays,
}

/// Why bytes are not the strict encoding of one structure. The decoders refuse; they never repair.
/// A field this build does not know is IGNORED unless the sender listed it in `crit` (planning/
/// architecture.md section 5.1 principle 6), with one exception: the certificate is a closed schema.
///
/// Caller: `Credential::decode`, `Assertion::decode`, `SignedEntitlement::decode`, `Certificate::decode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    /// The bytes are not one item in the deterministic encoding (the codec's own refusal).
    #[error("not the strict encoding of one item: {0}")]
    Cbor(#[from] DecodeError),
    /// The item has the wrong shape: a missing key, a wrong type, a wrong length, a bad `crit`.
    #[error("the structure has the wrong shape: {0}")]
    Shape(#[from] ShapeError),
    /// A scope kind or a permission bit this build does not know: refused, never read as "allow all".
    #[error("an unknown scope kind or permission")]
    UnknownScope,
    /// A word that is not one of its vocabulary (a role, an account kind, an entitlement state).
    #[error("a word that is not of its vocabulary")]
    UnknownWord,
    /// The key holds a number that does not fit its type, or a time that is not a day start.
    #[error("key {0} holds a number out of range")]
    OutOfRange(u64),
    /// A set that is not in its one canonical form: not strictly ascending, or a duplicate.
    #[error("a set is not in its one canonical form")]
    NotCanonical,
    /// More entries than the limit of the set.
    #[error("more entries than the limit")]
    TooMany,
    /// A key a closed schema does not know (the certificate is closed: key 0, `crit`, is one too).
    #[error("key {0} is not part of this structure")]
    UnknownField(u64),
    /// A value that is well formed CBOR and not a value of the structure (a text outside printable
    /// ASCII, a scope of the wrong arity, an id of the wrong length inside a list).
    #[error("a value that the structure does not allow")]
    Malformed,
}

/// Why a certificate was not made or does not verify (planning/architecture.md section 2.4: "the
/// identity key certifies the new one").
///
/// Caller: `certify` (S9f's rotation), `verify_cert` (S9f's `StageCredential`, S9g).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CertError {
    /// The window is not sane: `not_after <= not_before`, or longer than `MAX_CERT_SPAN`.
    #[error("the certificate's window is not sane")]
    Malformed,
    /// The key that signed the certificate is no identity key of its group in the anchors (an empty
    /// anchor set knows none, and refuses).
    #[error("the signing key is no identity key of the certificate's group")]
    UnknownIdentity,
    /// The signing key is an identity key of the group, but its window does not cover the moment the
    /// certificate was made (or, in a credential's chain, the moment of the check).
    #[error("the identity key's window does not cover the certificate")]
    IdentityWindow,
    /// The identity key's signature over the certificate does not verify.
    #[error("the certificate's signature does not verify")]
    BadSignature,
    /// `certify` only: the identity key's holder refused or could not sign.
    #[error("the identity key's holder could not sign: {0}")]
    Sign(SignError),
    /// `certify` only: the signature the holder returned does not verify under its own public key.
    #[error("the holder's signature does not verify under its own public key")]
    SignerFault,
    /// `certify` only: the certificate could not be written as canonical bytes.
    #[error("the certificate could not be encoded: {0}")]
    Encode(CanonicalError),
}

/// The refusals every chain check shares (credentials and assertions). Crate-internal: the public
/// enums carry the same names so that a caller matches on one flat set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Chain {
    ClockUntrusted,
    Malformed,
    LifetimeTooLong,
    NotYetValid,
    Expired,
}

/// Why a credential is refused (planning/architecture.md section 5.11 "Credential v2"; section 2.4).
///
/// The checks run in this order and the first that fails is the answer: `ClockUntrusted`, `Malformed`,
/// `LifetimeTooLong`, `CertMalformed`, `CertMismatch`, `NotYetValid`, `Expired`, `CertWindow`,
/// `UnknownIdentity`, `IdentityWindow`, `BadCert`, `BadSignature`. (`UnknownScope` is raised by the decoder, at
/// the step where `Malformed` is.)
///
/// Caller: `verify_credential` (S9g maps each to a status of its own).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CredError {
    /// The node's clock is not trusted (it differs from the median by more than 10 s): a node in that
    /// state decides nothing by time and says so. Never "valid", never "expired".
    #[error("the node's clock is not trusted: no decision by time")]
    ClockUntrusted,
    /// The signed bytes do not parse, or the claims break a rule of their shape: `expires_at <=
    /// issued_at`, a device credential without its family, a credential of no device without scopes,
    /// scopes that are present and empty.
    #[error("the credential is malformed")]
    Malformed,
    /// `expires_at - issued_at` is above the verifier's `max_credential_lifetime`.
    #[error("the credential lives longer than the rules allow")]
    LifetimeTooLong,
    /// The certificate's window is not sane (`not_after <= not_before`, or above `MAX_CERT_SPAN`).
    #[error("the certificate's window is not sane")]
    CertMalformed,
    /// The credential's `key_epoch` is not the epoch of the certificate it carries.
    #[error("the credential's key epoch is not its certificate's")]
    CertMismatch,
    /// The credential is issued more than `skew` after the node's now.
    #[error("the credential is not valid yet")]
    NotYetValid,
    /// The node's now is at or after `expires_at` (the expiry second belongs to the expired side).
    #[error("the credential has expired")]
    Expired,
    /// The certificate's window does not cover the credential's `issued_at`, or the node's now.
    #[error("the certificate's window does not cover the credential")]
    CertWindow,
    /// The key that signed the certificate is no identity key of its group in the anchors.
    #[error("the certificate's signing key is no identity key of its group")]
    UnknownIdentity,
    /// The identity key's window does not cover the certificate's `not_before` or the node's now.
    #[error("the identity key's window does not cover the certificate")]
    IdentityWindow,
    /// The identity key's signature over the certificate does not verify.
    #[error("the certificate's signature does not verify")]
    BadCert,
    /// The credential key's signature over the claims does not verify.
    #[error("the credential's signature does not verify")]
    BadSignature,
    /// A scope kind this build does not know: refused, never read as "allow all".
    #[error("the credential holds an unknown scope")]
    UnknownScope,
}

impl From<Chain> for CredError {
    fn from(c: Chain) -> CredError {
        match c {
            Chain::ClockUntrusted => CredError::ClockUntrusted,
            Chain::Malformed => CredError::Malformed,
            Chain::LifetimeTooLong => CredError::LifetimeTooLong,
            Chain::NotYetValid => CredError::NotYetValid,
            Chain::Expired => CredError::Expired,
        }
    }
}

impl From<CertError> for CredError {
    fn from(e: CertError) -> CredError {
        match e {
            CertError::Malformed => CredError::CertMalformed,
            CertError::UnknownIdentity => CredError::UnknownIdentity,
            CertError::IdentityWindow => CredError::IdentityWindow,
            // The three that only `certify` produces cannot come out of a verification; a certificate
            // that cannot be checked is a bad one.
            CertError::BadSignature
            | CertError::Sign(_)
            | CertError::SignerFault
            | CertError::Encode(_) => CredError::BadCert,
        }
    }
}

impl From<WireError> for CredError {
    fn from(e: WireError) -> CredError {
        match e {
            WireError::UnknownScope => CredError::UnknownScope,
            WireError::Cbor(_)
            | WireError::Shape(_)
            | WireError::UnknownWord
            | WireError::OutOfRange(_)
            | WireError::NotCanonical
            | WireError::TooMany
            | WireError::UnknownField(_)
            | WireError::Malformed => CredError::Malformed,
        }
    }
}

/// Why a member assertion is refused (planning/architecture.md section 5.11 "Member assertion").
///
/// The checks run in this order: `ClockUntrusted`, `Malformed`, `LifetimeTooLong`, `WrongAccount`,
/// `CertMalformed`, `NotYetValid`, `Expired`, `CertWindow`, `UnknownIdentity`, `IdentityWindow`,
/// `BadCert`, `BadSignature`.
///
/// Caller: `verify_assertion` (the services A2 to A5 map each to their own answer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AssertError {
    /// The node's clock is not trusted: no decision by time.
    #[error("the node's clock is not trusted: no decision by time")]
    ClockUntrusted,
    /// The signed bytes do not parse, `expires <= issued`, or the groups are not sorted, not distinct or
    /// more than `MAX_GROUPS` (two encodings of one set must never both verify).
    #[error("the assertion is malformed")]
    Malformed,
    /// `expires - issued` is above the verifier's `max_assertion_lifetime` (ten minutes).
    #[error("the assertion lives longer than the rules allow")]
    LifetimeTooLong,
    /// The account of the assertion is not the account this service was installed for.
    #[error("the assertion is for another account")]
    WrongAccount,
    /// The certificate's window is not sane.
    #[error("the certificate's window is not sane")]
    CertMalformed,
    /// The assertion is issued more than `skew` after the node's now.
    #[error("the assertion is not valid yet")]
    NotYetValid,
    /// The node's now is at or after `expires`.
    #[error("the assertion has expired")]
    Expired,
    /// The certificate's window does not cover the assertion's `issued`, or the node's now.
    #[error("the certificate's window does not cover the assertion")]
    CertWindow,
    /// The key that signed the certificate is no identity key of its group in the anchors.
    #[error("the certificate's signing key is no identity key of its group")]
    UnknownIdentity,
    /// The identity key's window does not cover the certificate's `not_before` or the node's now.
    #[error("the identity key's window does not cover the certificate")]
    IdentityWindow,
    /// The identity key's signature over the certificate does not verify.
    #[error("the certificate's signature does not verify")]
    BadCert,
    /// The credential key's signature over the claims does not verify.
    #[error("the assertion's signature does not verify")]
    BadSignature,
}

impl From<Chain> for AssertError {
    fn from(c: Chain) -> AssertError {
        match c {
            Chain::ClockUntrusted => AssertError::ClockUntrusted,
            Chain::Malformed => AssertError::Malformed,
            Chain::LifetimeTooLong => AssertError::LifetimeTooLong,
            Chain::NotYetValid => AssertError::NotYetValid,
            Chain::Expired => AssertError::Expired,
        }
    }
}

impl From<CertError> for AssertError {
    fn from(e: CertError) -> AssertError {
        match e {
            CertError::Malformed => AssertError::CertMalformed,
            CertError::UnknownIdentity => AssertError::UnknownIdentity,
            CertError::IdentityWindow => AssertError::IdentityWindow,
            CertError::BadSignature
            | CertError::Sign(_)
            | CertError::SignerFault
            | CertError::Encode(_) => AssertError::BadCert,
        }
    }
}

impl From<WireError> for AssertError {
    fn from(_: WireError) -> AssertError {
        AssertError::Malformed
    }
}

/// Why an entitlement was not signed or does not verify (planning/architecture.md section 5.11
/// "Entitlement v2"; section 2.4: "entitlements signed by the old key stay valid until they expire").
///
/// `verify_entitlement` checks in this order: `UnknownKey`, `KeyWindow`, `NotYetIssued`, `BadSignature`.
///
/// Caller: `verify_entitlement` (S9i's `CredsEntitlementVerifier`), `sign_entitlement` (S13a at N24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EntitlementError {
    /// The key that signed the entitlement is no payments key in the anchors (an empty anchor set
    /// knows none, and refuses).
    #[error("the signing key is no payments key")]
    UnknownKey,
    /// The key is a payments key, but none of its windows covers the entitlement's `issued_at`: a key
    /// that was not yet active, or was retired or revoked before the entitlement's day.
    #[error("the payments key's window does not cover the day the entitlement was issued")]
    KeyWindow,
    /// The entitlement is issued on a day that has not begun at the leader's stamp (beyond the skew):
    /// a staged key cannot sign into its future window.
    #[error("the entitlement is issued on a day that has not begun")]
    NotYetIssued,
    /// The payments key's signature over the claims does not verify.
    #[error("the entitlement's signature does not verify")]
    BadSignature,
    /// The signed bytes do not parse (or, from `sign_entitlement`, the claims cannot be encoded).
    #[error("the entitlement is malformed")]
    Malformed,
    /// `sign_entitlement` only: the payments key's holder refused or could not sign.
    #[error("the payments key's holder could not sign: {0}")]
    Sign(SignError),
    /// `sign_entitlement` only: the signature the holder returned does not verify under its own key.
    #[error("the holder's signature does not verify under its own public key")]
    SignerFault,
}

impl From<WireError> for EntitlementError {
    fn from(_: WireError) -> EntitlementError {
        EntitlementError::Malformed
    }
}

impl From<CanonicalError> for EntitlementError {
    fn from(_: CanonicalError) -> EntitlementError {
        EntitlementError::Malformed
    }
}

/// Why the issuer would not sign a claim set: it refuses what every verifier would refuse, so no signed
/// garbage leaves it (planning/architecture.md section 1.4 rule 11).
///
/// Caller: `CredentialIssuer::issue` (S9i's `finish_refresh`), `AssertionIssuer::issue` (S9i, S9g's
/// `assertion_claims` output).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IssueError {
    /// The claims break a rule of their shape (the same rules `verify_credential` applies).
    #[error("the claims are refused: {0}")]
    Claims(CredError),
    /// The assertion's claims break a rule of their shape.
    #[error("the assertion's claims are refused: {0}")]
    AssertionClaims(AssertError),
    /// The credential would not live inside its certificate's window: `issued_at` before `not_before`,
    /// or `expires_at` after `not_after`.
    #[error("the credential would outlive its certificate")]
    OutsideCertificate,
    /// The claims could not be written as canonical bytes.
    #[error("the claims could not be encoded: {0}")]
    Encode(CanonicalError),
    /// The key's holder refused or could not sign.
    #[error("the key's holder could not sign: {0}")]
    Sign(SignError),
    /// The signature the holder returned does not verify under its own public key.
    #[error("the holder's signature does not verify under its own public key")]
    SignerFault,
}

/// What the probe of an issuer found (planning/architecture.md section 1.4 rule 9: "New credentials are
/// tested before they are used").
///
/// Caller: `IssuerError::ProbeFailed` (S9f's rotation keeps the old key and shows the failure in its
/// status line).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProbeFailure {
    /// The probe credential or assertion does not verify under the anchors at the probe's time: a
    /// certificate that does not verify, a window that does not cover the moment, anchors that do not
    /// know the identity key, or a signer whose public key is not the certificate's `cred_pub`.
    #[error("the probe credential does not verify: {0}")]
    Credential(CredError),
    /// The probe assertion does not verify.
    #[error("the probe assertion does not verify: {0}")]
    Assertion(AssertError),
    /// The key's holder refused or could not sign the probe.
    #[error("the key's holder could not sign the probe: {0}")]
    Sign(SignError),
    /// The signature the holder returned does not verify under its own public key.
    #[error("the holder's probe signature does not verify under its own public key")]
    SignerFault,
    /// The probe could not be written as canonical bytes.
    #[error("the probe could not be encoded: {0}")]
    Encode(CanonicalError),
}

/// Why no issuer was made: the probe failed, so NO issuer value exists and the old key stays.
///
/// Caller: `CredentialIssuer::new`, `AssertionIssuer::new`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IssuerError {
    /// The probe failed (see [`ProbeFailure`]).
    #[error("the probe failed: {0}")]
    ProbeFailed(ProbeFailure),
}
