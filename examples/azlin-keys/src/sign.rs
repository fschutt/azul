//! Domain-separated Ed25519 signatures and the seam through which a verifier finds a device's
//! public key (planning/architecture.md section 2.5 "Signed structures, each with its own domain
//! string", section 5.5 "Envelopes carry the signer", section 5.9; build unit S15, lane
//! W1-S15a).
//!
//! THE ONE SIGNING RULE (ARCH29 9.13 decision 2, COMMON-W1R R4): the preimage of every signature
//! is `domain || 0x00 || bytes`, built once by [`signing_input`]; [`sign`], [`verify`] and
//! [`verify_with`] are built on it, and azlin-regbase re-exports it. A signature made under one
//! domain is no signature under another, because the domain is NUL-free ASCII and the first
//! `0x00` ends it: two different domains never give equal inputs, whatever the messages.
//!
//! Verification is `verify_strict`: a non-canonical signature (`S + L`), a signature whose `R` or
//! key has small order, and bytes that are no curve point are all [`SignError::Bad`], one word
//! that says nothing about which part failed. There is no default key: a directory that cannot
//! name a key stops the verification with its own error ([`verify_with`]).
//!
//! ```
//! use azlin_keys::sign::{self, signing_input, ENVELOPE_V2, POINTER_V2};
//! use azlin_keys::{Secret, SignError, SignKeypair};
//!
//! let key = SignKeypair::from_seed(&Secret::from_bytes([7u8; 32]));
//! let sig = sign::sign(&POINTER_V2, b"a pointer", &key);
//! assert_eq!(sign::verify(&POINTER_V2, b"a pointer", &key.verify_key(), &sig), Ok(()));
//! // The same bytes under another domain do not verify.
//! assert_eq!(
//!     sign::verify(&ENVELOPE_V2, b"a pointer", &key.verify_key(), &sig),
//!     Err(SignError::Bad)
//! );
//! assert_eq!(signing_input(&POINTER_V2, b"x"), b"azlin-pointer-v2\0x".to_vec());
//! ```

use core::fmt;

use ed25519_dalek::{Signature as DalekSignature, Signer, SigningKey, VerifyingKey};

use crate::error::EntropyError;
pub use crate::error::{DirectoryError, SignError};
use crate::ids::{write_hex, ChainSeq, DeviceId};
use crate::seam::{Entropy, UnixTime};
use crate::secret::Secret;

#[cfg(any(test, feature = "test-support"))]
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------------------------
// Domains and the preimage
// ---------------------------------------------------------------------------------------------

/// The domain string of one signed structure: lowercase NUL-free ASCII such as
/// `azlin-pointer-v2`, a `const` of the crate that owns the structure. Every signature is made
/// over `domain || 0x00 || bytes`, so a signature of one structure is no signature of another
/// (planning/architecture.md section 2.5).
///
/// `Domain::new` does not validate (it is a `const fn` and cannot fail without a panic, DRIFT
/// X-DELTA-W1S15A-8): [`Domain::is_wellformed`] does, and the test `domains_are_unique_and_ascii_without_nul` runs it over [`ALL_DOMAINS`];
/// a crate that defines domains tests its own list the same way. The registry of every domain
/// of the engine is azlin-contract's `signed.rs` (S5).
///
/// Caller: S15b (envelopes), S14d (pointers), S16 and S17 (log records, chain entries) and every
/// unit that introduces a signed structure (W1/CONFLICTS.md C-12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Domain(&'static str);

impl Domain {
    /// The longest domain, in bytes.
    pub const MAX_LEN: usize = 64;

    /// The domain with this text. Define it as a `const`; see the type documentation.
    pub const fn new(text: &'static str) -> Domain {
        Domain(text)
    }

    /// The domain text.
    pub const fn as_str(&self) -> &'static str {
        self.0
    }

    /// Whether the domain can be used: 1 to [`Domain::MAX_LEN`] bytes, every byte printable ASCII
    /// (`0x21` to `0x7e`: no NUL, no space, no control character, nothing above ASCII). A domain
    /// that is not well formed breaks the rule that two domains never give equal inputs.
    pub fn is_wellformed(&self) -> bool {
        let bytes = self.0.as_bytes();
        !bytes.is_empty()
            && bytes.len() <= Domain::MAX_LEN
            && bytes.iter().all(|byte| (0x21..=0x7e).contains(byte))
    }
}

/// The domain of an envelope's `sig` (planning/architecture.md section 5.5: `signer`, `sig`).
/// Proposed by the units because section 2.5's list gives the envelope no domain (S5's
/// `signed.rs` holds it as `Proposed`; DRIFT X-DELTA-W1S15A-2).
///
/// Caller: S15b (`wrap_x25519`, `wrap_hybrid`, `verify_signature`).
pub const ENVELOPE_V2: Domain = Domain::new("azlin-envelope-v2");

/// The domain of a pointer's `sig` (planning/architecture.md section 5.7: `author_device`,
/// `sig`); proposed like [`ENVELOPE_V2`].
///
/// Caller: S14d (pointer v2 signing and validation).
pub const POINTER_V2: Domain = Domain::new("azlin-pointer-v2");

/// The domain of an append-log record's signature (planning/architecture.md section 5.13:
/// "Every record carries `author_device` and a signature"); proposed like [`ENVELOPE_V2`].
///
/// Caller: S16 (log records).
pub const LOG_RECORD_V1: Domain = Domain::new("azlin-log-record-v1");

/// The three domains this crate defines. It defines no other: a unit that introduces a signed
/// structure defines its domain with [`Domain::new`] from the list in W1/CONFLICTS.md C-12.
///
/// Caller: the test `domains_are_unique_and_ascii_without_nul`, INT-WIRE (the comparison with
/// azlin-contract's `SIGNED_DOMAINS`).
pub const ALL_DOMAINS: &[Domain] = &[ENVELOPE_V2, POINTER_V2, LOG_RECORD_V1];

/// The bytes that are signed: `domain || 0x00 || msg`. The one definition of the rule; every
/// signature and every verification of the engine goes through it.
///
/// Caller: `sign`, `verify`; azlin-regbase re-exports it (R4); azlin-seams' verifier takes its
/// output as the preimage; S9e's signers frame with it.
pub fn signing_input(domain: &Domain, msg: &[u8]) -> Vec<u8> {
    let separator: &[u8] = &[0u8];
    [domain.0.as_bytes(), separator, msg].concat()
}

// ---------------------------------------------------------------------------------------------
// Keys and signatures
// ---------------------------------------------------------------------------------------------

/// An Ed25519 signing key: the seed, held so that it is wiped on drop and never printed. No
/// `Clone`: a second holder is made from the seed with [`SignKeypair::from_seed`].
///
/// Caller: S15c1 (`KeyBundle::signer`), S15b (the wrapping device signs), S14d, S16, S17.
pub struct SignKeypair {
    key: SigningKey,
}

impl SignKeypair {
    /// The key pair of this 32-byte seed (RFC 8032 section 5.1.5).
    pub fn from_seed(seed: &Secret<32>) -> SignKeypair {
        SignKeypair {
            key: SigningKey::from_bytes(seed.expose()),
        }
    }

    /// A fresh key pair: the seed comes from the entropy seam; on a failure there is no key.
    pub fn generate(ent: &mut dyn Entropy) -> Result<SignKeypair, EntropyError> {
        let seed = Secret::<32>::generate(ent)?;
        Ok(SignKeypair::from_seed(&seed))
    }

    /// The public key.
    pub fn verify_key(&self) -> VerifyKey {
        VerifyKey(self.key.verifying_key().to_bytes())
    }
}

/// Prints the public key and `seed ***`.
impl fmt::Debug for SignKeypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SignKeypair({:?}, seed ***)", self.verify_key())
    }
}

/// An Ed25519 public key: 32 bytes of public data (the `sign_pub` column), compared, ordered and
/// hashed as bytes so it can be a map key. azlin-regbase names it `PublicKey32`. The bytes are
/// not checked to be a curve point until they are used to verify: a bad key fails there.
///
/// Caller: S15b (`signer` lookups), S9 (`reg_member`, `reg_device`), S16/S17, azlin-regbase
/// (`PublicKey32`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerifyKey([u8; 32]);

impl VerifyKey {
    /// The number of bytes of the key.
    pub const LEN: usize = 32;

    /// The key with these bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> VerifyKey {
        VerifyKey(bytes)
    }

    /// The bytes of the key.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// 64 lowercase hex digits.
impl fmt::Display for VerifyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for VerifyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VerifyKey({self})")
    }
}

/// An Ed25519 signature: 64 bytes of public data (`R || S`), compared, ordered and hashed as
/// bytes. azlin-regbase names it `Signature64`.
///
/// Caller: S15b (`sig` of an envelope), S14d (`sig` of a pointer), S16/S17, azlin-regbase
/// (`Signature64`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Signature([u8; 64]);

impl Signature {
    /// The number of bytes of the signature.
    pub const LEN: usize = 64;

    /// The signature with these bytes.
    pub const fn from_bytes(bytes: [u8; 64]) -> Signature {
        Signature(bytes)
    }

    /// The bytes of the signature.
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }
}

/// 128 lowercase hex digits.
impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({self})")
    }
}

// ---------------------------------------------------------------------------------------------
// Sign and verify
// ---------------------------------------------------------------------------------------------

/// Signs `msg` under `domain`: Ed25519 over [`signing_input`]. Deterministic: the same key,
/// domain and message always give the same signature.
///
/// Caller: S15b (`wrap_x25519`, `wrap_hybrid`), S14d (pointer signing), S16 (log records, chain
/// entries).
pub fn sign(domain: &Domain, msg: &[u8], key: &SignKeypair) -> Signature {
    sign_preimage(&signing_input(domain, msg), key)
}

/// Verifies a signature made by [`sign`] under the same domain: `verify_strict` over
/// [`signing_input`]. Every refusal is [`SignError::Bad`]; nothing here panics on any input.
///
/// Caller: S15b (`verify_signature`), S14d (pointer validation), S16/S17 (entries).
pub fn verify(
    domain: &Domain,
    msg: &[u8],
    key: &VerifyKey,
    sig: &Signature,
) -> Result<(), SignError> {
    verify_preimage(&signing_input(domain, msg), key, sig)
}

/// Signs an already-framed preimage: the raw layer under [`sign`]. The bytes MUST be the output
/// of [`signing_input`]; signing bytes of any other shape defeats domain separation.
///
/// Caller: `sign`, the RFC 8032 known-answer tests; azlin-seams' and S9e's signers that frame
/// with [`signing_input`] themselves.
pub fn sign_preimage(preimage: &[u8], key: &SignKeypair) -> Signature {
    let signature: DalekSignature = key.key.sign(preimage);
    Signature(signature.to_bytes())
}

/// Verifies a signature over an already-framed preimage: the raw layer under [`verify`], with
/// `verify_strict` (canonical `S`, no small-order `R` or key). The bytes MUST be the output of
/// [`signing_input`].
///
/// Caller: `verify`; azlin-seams' `Ed25519Verifier` (R4: its preimage is already framed).
pub fn verify_preimage(preimage: &[u8], key: &VerifyKey, sig: &Signature) -> Result<(), SignError> {
    let verifying = VerifyingKey::from_bytes(key.as_bytes()).map_err(|_| SignError::Bad)?;
    let signature = DalekSignature::from_bytes(sig.as_bytes());
    verifying
        .verify_strict(preimage, &signature)
        .map_err(|_| SignError::Bad)
}

// ---------------------------------------------------------------------------------------------
// The device directory seam
// ---------------------------------------------------------------------------------------------

/// The point of time or of a chain at which a device's key is asked for: now, a point of time, or
/// a position of the team log or a space chain ("valid at the log position of the write").
///
/// Caller: every `DeviceDirectory` caller (S15b `open`, S15c2 `validate_row`, S17 the team log).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VerifyAt {
    /// At the directory's current state.
    Now,
    /// At this point of time.
    Time(UnixTime),
    /// At this position of the team log or the space chain.
    ChainSeq(ChainSeq),
}

/// The seam through which a verifier finds the public key of a device ("which key was valid for
/// `device` at `at`"). `Ok` carries the key; every other answer is a [`DirectoryError`]: an
/// unknown device, a key not active yet, a revoked key, and a directory that cannot answer are four
/// different refusals, and none of them is ever turned into a default key.
///
/// Seam: S9's registry (`RegistryDirectory` over `reg_device`) and S17's team log answer it;
/// `verify_with`, S15b's `open` and S15c2's `validate_row` call it.
pub trait DeviceDirectory {
    /// The signing key of `device` at `at`.
    fn verify_key(&self, device: &DeviceId, at: VerifyAt) -> Result<VerifyKey, DirectoryError>;
}

/// Verifies `sig` over `msg` under `domain` with the key the directory names for `device` at `at`.
/// Each [`DirectoryError`] becomes its own [`SignError`] (`UnknownSigner`, `NotYetActive`,
/// `Revoked`, `DirectoryUnavailable`) and the verification is not tried; a key that does not
/// verify is [`SignError::Bad`]. There is no default key.
///
/// Caller: S15b (`verify_signature`, `open`), S15c2 (`authorize_epoch`, `validate_row`), S14d
/// (the author's signature of a pointer).
pub fn verify_with(
    dir: &dyn DeviceDirectory,
    device: &DeviceId,
    at: VerifyAt,
    domain: &Domain,
    msg: &[u8],
    sig: &Signature,
) -> Result<(), SignError> {
    let key = dir.verify_key(device, at).map_err(SignError::from)?;
    verify(domain, msg, &key, sig)
}

// ---------------------------------------------------------------------------------------------
// Test double
// ---------------------------------------------------------------------------------------------

/// What `VerifyAt::Now` answers for a device of a [`MapDirectory`].
///
/// Under the feature `test-support`.
///
/// Seam: tests of every lane that verifies a signer.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceState {
    /// Added but not active yet: `NotYetActive`.
    Pending,
    /// Active: the key.
    Active,
    /// Revoked: `Revoked`.
    Revoked,
}

/// One device of a [`MapDirectory`]: its key, its state now, and the windows in which its key was
/// valid in time and in chain positions (`added` inclusive, `removed` exclusive).
///
/// Under the feature `test-support`.
///
/// Seam: tests of every lane that verifies a signer.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectoryEntry {
    /// The device's signing key.
    pub key: VerifyKey,
    /// What `VerifyAt::Now` answers.
    pub now: DeviceState,
    /// The key is valid from this time on (`VerifyAt::Time`).
    pub added_at: UnixTime,
    /// The key is revoked from this time on (`VerifyAt::Time`); `None`: never.
    pub removed_at: Option<UnixTime>,
    /// The key is valid from this chain position on (`VerifyAt::ChainSeq`).
    pub added_seq: ChainSeq,
    /// The key is revoked from this chain position on (`VerifyAt::ChainSeq`); `None`: never.
    pub removed_seq: Option<ChainSeq>,
}

#[cfg(any(test, feature = "test-support"))]
impl DirectoryEntry {
    /// An entry that is valid at every time and every position, now included.
    pub fn active(key: VerifyKey) -> DirectoryEntry {
        DirectoryEntry {
            key,
            now: DeviceState::Active,
            added_at: UnixTime::EPOCH,
            removed_at: None,
            added_seq: ChainSeq::MIN,
            removed_seq: None,
        }
    }
}

/// A [`DeviceDirectory`] a test fills by hand. It answers `Unknown` for a device it does not
/// hold, `NotYetActive` before an entry's `added` point, `Revoked` from its `removed` point, and
/// `Unavailable` for everything while it is switched off.
///
/// Under the feature `test-support`.
///
/// Seam: tests of S15b, S15c2, S14d, S16 and S17.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Debug, Default)]
pub struct MapDirectory {
    entries: BTreeMap<DeviceId, DirectoryEntry>,
    unavailable: bool,
}

#[cfg(any(test, feature = "test-support"))]
impl MapDirectory {
    /// An empty directory.
    pub fn new() -> MapDirectory {
        MapDirectory::default()
    }

    /// Adds a device (replacing an entry of the same id).
    pub fn insert(&mut self, device: DeviceId, entry: DirectoryEntry) {
        self.entries.insert(device, entry);
    }

    /// Adds a device whose key is valid at every time and position.
    pub fn insert_active(&mut self, device: DeviceId, key: VerifyKey) {
        self.insert(device, DirectoryEntry::active(key));
    }

    /// While `unavailable` is true every question is answered `Unavailable`.
    pub fn set_unavailable(&mut self, unavailable: bool) {
        self.unavailable = unavailable;
    }
}

#[cfg(any(test, feature = "test-support"))]
impl DeviceDirectory for MapDirectory {
    fn verify_key(&self, device: &DeviceId, at: VerifyAt) -> Result<VerifyKey, DirectoryError> {
        if self.unavailable {
            return Err(DirectoryError::Unavailable);
        }
        let entry = self.entries.get(device).ok_or(DirectoryError::Unknown)?;
        match at {
            VerifyAt::Now => match entry.now {
                DeviceState::Active => Ok(entry.key),
                DeviceState::Pending => Err(DirectoryError::NotYetActive),
                DeviceState::Revoked => Err(DirectoryError::Revoked),
            },
            VerifyAt::Time(time) => {
                if time < entry.added_at {
                    Err(DirectoryError::NotYetActive)
                } else if entry.removed_at.is_some_and(|removed| time >= removed) {
                    Err(DirectoryError::Revoked)
                } else {
                    Ok(entry.key)
                }
            }
            VerifyAt::ChainSeq(seq) => {
                if seq < entry.added_seq {
                    Err(DirectoryError::NotYetActive)
                } else if entry.removed_seq.is_some_and(|removed| seq >= removed) {
                    Err(DirectoryError::Revoked)
                } else {
                    Ok(entry.key)
                }
            }
        }
    }
}
