//! The member key and the device key: one bundle of an Ed25519 signing seed, an X25519 secret and
//! an ML-KEM-768 seed, and the wrap of a member key to each of the person's devices (build unit
//! S15, lane W1-S15c1).
//!
//! Implements planning/architecture.md section 5.5 "Keys": "Device keys sign log entries and
//! receive wraps", "Member key: one X25519 + Ed25519 pair per person per account, wrapped to each
//! of the person's devices. Adding a device wraps one key, not every space. Different accounts
//! use different member keys, so two accounts cannot be linked by a device key"; and section 5.3
//! (`reg_member` and `reg_device`: the columns `sign_pub`, `kx_pub`, `pq_pub`).
//!
//! # One shape, two roles
//!
//! A member key and a device key are the same thing, a [`KeyBundle`] (the aliases [`MemberKeys`]
//! and [`DeviceKeys`] name the role). They are two INDEPENDENT draws from the entropy seam, never
//! derived from each other: a device key that derived a member key would link two accounts of one
//! person. [`KeyBundle::generate`] draws, in this order, 32 bytes for the Ed25519 seed, 32 bytes
//! for the X25519 secret and 64 bytes for the ML-KEM-768 seed `(d, z)`: 128 bytes in all, three
//! draws (P-2 of the brief). [`KeyBundle::public`] is the [`PublicBundle`]: exactly the columns
//! `sign_pub`, `kx_pub` and `pq_pub` of the registry.
//!
//! ML-KEM-768 is the crate `ml-kem` behind the feature `pq` (not a default feature until the owner
//! has reviewed it). Without the feature the seed is still drawn and kept, but the public key
//! cannot be computed: `pq_pub` is `None` and the bundle is a recipient of algorithm 1 only
//! (DRIFT X-DELTA-W1S15C1-16).
//!
//! # What a wrap carries
//!
//! The plaintext of a member key wrap is the 128 bytes `ed25519 seed || x25519 secret || ML-KEM
//! seed` ([`KeyBundle::to_plaintext`], DRIFT X-DELTA-W1S15C1-10), sealed by [`wrap_member_key_to_device`]
//! under the context `member/<member id>/key` to the device's public bundle: hybrid (algorithm 2)
//! when the device has an ML-KEM key, X25519 (algorithm 1) otherwise (P-6 of envelope v2). The
//! device opens it with [`open_member_key`], which also checks that the opened bytes are the keys
//! the registry names for the member.
//!
//! # Errors
//!
//! [`KeyError`] is the one error of the three modules `member`, `group` and `space` (DRIFT
//! X-DELTA-W1S15C1-15): a small `Copy` enum, no strings, no data that could carry a key.
//!
//! The public half is data and can be copied; the keys cannot (the first example is the control:
//! the same call compiles for the public bundle):
//!
//! ```
//! use azlin_keys::member::PublicBundle;
//! fn needs_clone<T: Clone>() {}
//! needs_clone::<PublicBundle>();
//! ```
//!
//! ```compile_fail
//! use azlin_keys::member::KeyBundle;
//! fn needs_clone<T: Clone>() {}
//! needs_clone::<KeyBundle>();
//! ```
//!
//! ```compile_fail
//! use azlin_keys::member::KeyBundle;
//! fn needs_copy<T: Copy>() {}
//! needs_copy::<KeyBundle>();
//! ```

use core::fmt;

use crate::envelope::{open, Ctx, Envelope, OpenError, Recipient, RecipientSecret, WrapError};
use crate::error::EntropyError;
use crate::ids::{DeviceId, Epoch, KeyId, MemberId};
use crate::seam::Entropy;
use crate::secret::Secret;
use crate::sign::{DeviceDirectory, SignKeypair, VerifyAt, VerifyKey};

// ---------------------------------------------------------------------------------------------
// The error of the key hierarchy
// ---------------------------------------------------------------------------------------------

/// Why a link of the epoch chain is not usable. Always paired with the epoch of the link in
/// [`KeyError::ChainLink`].
///
/// Caller: `chain_open` and `chain_open_link` (space.rs), S15c2 (`authorize_epoch` maps it to its
/// own refusals), INT-S15-1 (the client's history reads).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkFault {
    /// The walk needs the link of this epoch and the wraps hold none.
    Missing,
    /// Two different envelopes claim the chain context of the epoch (the registry holds one row
    /// per epoch: two is a fault of a server or a replay, never a choice for the reader).
    Duplicate,
    /// The link did not open: the context, the recipient, the signature of the wrapping device
    /// (through the directory) or the cipher refused it.
    Open(OpenError),
    /// The link opened to bytes that are no space key of this epoch.
    Plaintext,
    /// The key the link opened to cannot be the one a rotation into the newer epoch rotated from:
    /// its dedup seed is neither the newer epoch's carried seed nor replaced by a fresh one.
    Inconsistent,
}

impl fmt::Display for LinkFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkFault::Missing => f.write_str("the link is missing"),
            LinkFault::Duplicate => f.write_str("two different links claim the epoch"),
            LinkFault::Open(e) => write!(f, "the link does not open: {e}"),
            LinkFault::Plaintext => f.write_str("the link holds no space key of this epoch"),
            LinkFault::Inconsistent => {
                f.write_str("the link's key does not fit the epoch it was sealed under")
            }
        }
    }
}

/// Why a key of the hierarchy (a member or device key bundle, a group key, a space key, a wrap or
/// an epoch chain) was not made or did not open. One `Copy` enum for the modules `member`, `group`
/// and `space`; nothing in it can carry a key.
///
/// After an error NO key and NO envelope exists: a generator that fails draws are never completed
/// with zeroes (UNKNOWN IS A STATE).
///
/// Caller: every function of the three modules; INT-S15-1 and S15c2 map the variants to their
/// own refusals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyError {
    /// The entropy seam failed: no key was made.
    Entropy(EntropyError),
    /// No envelope was made (the envelope layer's reason).
    Wrap(WrapError),
    /// The envelope did not open (the envelope layer's reason: the context, the recipient, the
    /// signer's signature through the directory, or the cipher).
    Open(OpenError),
    /// The opened bytes are no key of this kind: a wrong length, or a dedup epoch that is 0 or
    /// above the key's epoch.
    BadPlaintext,
    /// The opened member key is not the key bundle the registry names for the member.
    BundleMismatch,
    /// The key (or one of a pair of keys) belongs to another space than the one named.
    WrongSpace,
    /// The key belongs to another group than the one named.
    WrongGroup,
    /// The epochs are not consecutive: a new epoch follows its predecessor by exactly one.
    NotNextEpoch,
    /// The key of the previous epoch is needed and was not given: a `Carry` has nothing to carry,
    /// and only the first epoch of a space has no predecessor.
    MissingPrevious,
    /// `upto` of a chain walk is above the newest epoch.
    UptoAboveNewest,
    /// A chain walk longer than `CHAIN_MAX` epochs.
    ChainTooLong,
    /// A link of the epoch chain is not usable: the walk stops at this epoch.
    ChainLink {
        /// The epoch the link seals (the older one).
        epoch: Epoch,
        /// What is wrong with it.
        fault: LinkFault,
    },
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyError::Entropy(e) => write!(f, "no key: {e}"),
            KeyError::Wrap(e) => write!(f, "no envelope: {e}"),
            KeyError::Open(e) => write!(f, "the envelope does not open: {e}"),
            KeyError::BadPlaintext => f.write_str("the opened bytes are no key of this kind"),
            KeyError::BundleMismatch => {
                f.write_str("the opened member key is not the one the registry names")
            }
            KeyError::WrongSpace => f.write_str("the key belongs to another space"),
            KeyError::WrongGroup => f.write_str("the key belongs to another group"),
            KeyError::NotNextEpoch => f.write_str("the epochs are not consecutive"),
            KeyError::MissingPrevious => {
                f.write_str("the key of the previous epoch is needed and missing")
            }
            KeyError::UptoAboveNewest => f.write_str("the oldest epoch asked for is a newer one"),
            KeyError::ChainTooLong => f.write_str("the chain walk is longer than the limit"),
            KeyError::ChainLink { epoch, fault } => {
                write!(f, "the chain link of epoch {epoch}: {fault}")
            }
        }
    }
}

impl std::error::Error for KeyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            KeyError::Entropy(e) => Some(e),
            KeyError::Wrap(e) => Some(e),
            KeyError::Open(e) => Some(e),
            _ => None,
        }
    }
}

impl From<EntropyError> for KeyError {
    fn from(e: EntropyError) -> KeyError {
        KeyError::Entropy(e)
    }
}

impl From<WrapError> for KeyError {
    fn from(e: WrapError) -> KeyError {
        KeyError::Wrap(e)
    }
}

impl From<OpenError> for KeyError {
    fn from(e: OpenError) -> KeyError {
        KeyError::Open(e)
    }
}

// ---------------------------------------------------------------------------------------------
// The public bundle: the columns of reg_member and reg_device
// ---------------------------------------------------------------------------------------------

/// The public half of a [`KeyBundle`]: exactly the columns `sign_pub`, `kx_pub` and `pq_pub` of
/// `reg_member` and `reg_device` (planning/architecture.md section 5.3). Public data: it is
/// `Clone`, compared and hashed as bytes.
///
/// `pq_pub` is the ML-KEM-768 encapsulation key (1,184 bytes) when the bundle has one, `None`
/// otherwise; `Some` of an empty key is the same recipient as `None` (the envelope layer's rule).
///
/// Caller: INT-S15-1 (the registry's rows are read into it), S15c2 (the recipients of a rekey),
/// S9 (the columns).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PublicBundle {
    /// The Ed25519 public key (`sign_pub`): the key that verifies this bundle's signatures.
    pub sign_pub: [u8; 32],
    /// The X25519 public key (`kx_pub`).
    pub kx_pub: [u8; 32],
    /// The ML-KEM-768 encapsulation key (`pq_pub`), 1,184 bytes; `None` for a bundle without one.
    pub pq_pub: Option<Vec<u8>>,
}

impl PublicBundle {
    /// The id of this bundle as a recipient: the `rcpt` of every envelope wrapped to it and the
    /// "recipient key id" of its `reg_wrap` rows. It is S15a's `KeyId::of_bundle` of `kx_pub` and
    /// `pq_pub`, so it names exactly the keys a wrap was made to.
    ///
    /// Caller: S15c2 (`reg_wrap` rows), INT-S15-1.
    pub fn key_id(&self) -> KeyId {
        KeyId::of_bundle(&self.kx_pub, self.pq_pub.as_deref())
    }

    /// The recipient to wrap to: hybrid (algorithm 2) when there is an ML-KEM key, X25519
    /// (algorithm 1) otherwise.
    ///
    /// Caller: every wrap of this module, `group.rs` and `space.rs`; INT-S15-1.
    pub fn recipient(&self) -> Recipient {
        Recipient::from_public(self.kx_pub, self.pq_pub.clone())
    }

    /// The Ed25519 public key as the crate's typed key.
    ///
    /// Caller: INT-S15-1 and S9 (the directory of signers is built from `reg_device`'s keys).
    pub fn verify_key(&self) -> VerifyKey {
        VerifyKey::from_bytes(self.sign_pub)
    }
}

// ---------------------------------------------------------------------------------------------
// The key bundle
// ---------------------------------------------------------------------------------------------

/// The keys of one member or one device: an Ed25519 seed, an X25519 secret and an ML-KEM-768 seed.
/// Every secret is a [`Secret`] (wiped on drop, never printed); the type is not `Clone`, not
/// `Copy` and has no `Debug` that shows a byte. A second owner is made only by
/// [`KeyBundle::duplicate`], which is greppable.
///
/// Caller: INT-S15-1 (azul-storage's key flows replace `crypto/keys.rs`, `device.rs`,
/// `keystate.rs`, `rotation.rs`), S15c2, S15d (the member key is what the recovery paths wrap),
/// S17 (a remaining manager's device or the key agent rotates).
pub struct KeyBundle {
    sign_seed: Secret<32>,
    kx: Secret<32>,
    pq_seed: Secret<64>,
    public: PublicBundle,
}

/// A member's keys: one bundle per person per account, wrapped to each of the person's devices.
///
/// Caller: INT-S15-1, S15c2, S17.
pub type MemberKeys = KeyBundle;

/// A device's keys: one bundle per device, drawn independently of the member key.
///
/// Caller: INT-S15-1, S17.
pub type DeviceKeys = KeyBundle;

impl KeyBundle {
    /// The seeds and the public half together; the public half is computed once.
    fn from_seeds(sign_seed: Secret<32>, kx: Secret<32>, pq_seed: Secret<64>) -> KeyBundle {
        let signer = SignKeypair::from_seed(&sign_seed);
        let recipient = RecipientSecret::new(&kx, Some(&pq_seed));
        let public = PublicBundle {
            sign_pub: *signer.verify_key().as_bytes(),
            kx_pub: *recipient.kx_public(),
            pq_pub: recipient.pq_public().map(<[u8]>::to_vec),
        };
        KeyBundle {
            sign_seed,
            kx,
            pq_seed,
            public,
        }
    }

    /// A fresh bundle: the entropy seam is drawn from three times, in this order: 32 bytes for the
    /// Ed25519 seed, 32 bytes for the X25519 secret, 64 bytes for the ML-KEM-768 seed `(d, z)`
    /// (128 bytes in all). A failed draw is [`KeyError::Entropy`] and no bundle exists.
    ///
    /// Caller: INT-S15-1 (account setup makes the first member key, device enrollment makes a
    /// device key), S17.
    pub fn generate(ent: &mut dyn Entropy) -> Result<KeyBundle, KeyError> {
        let sign_seed = Secret::<32>::generate(ent)?;
        let kx = Secret::<32>::generate(ent)?;
        let pq_seed = Secret::<64>::generate(ent)?;
        Ok(KeyBundle::from_seeds(sign_seed, kx, pq_seed))
    }

    /// The bundle of these 128 bytes (`ed25519 seed || x25519 secret || ML-KEM seed`), the bytes
    /// [`KeyBundle::to_plaintext`] makes; any other length is [`KeyError::BadPlaintext`].
    ///
    /// Caller: [`open_member_key`], S15d (the recovery paths open to these bytes), INT-S15-1.
    pub fn from_plaintext(bytes: &[u8]) -> Result<KeyBundle, KeyError> {
        let (sign, rest) = bytes
            .split_first_chunk::<32>()
            .ok_or(KeyError::BadPlaintext)?;
        let (kx, rest) = rest
            .split_first_chunk::<32>()
            .ok_or(KeyError::BadPlaintext)?;
        let (pq, rest) = rest
            .split_first_chunk::<64>()
            .ok_or(KeyError::BadPlaintext)?;
        if !rest.is_empty() {
            return Err(KeyError::BadPlaintext);
        }
        Ok(KeyBundle::from_seeds(
            Secret::from_bytes(*sign),
            Secret::from_bytes(*kx),
            Secret::from_bytes(*pq),
        ))
    }

    /// The 128 bytes a wrap of this key carries: `ed25519 seed || x25519 secret || ML-KEM seed`.
    ///
    /// Caller: [`wrap_member_key_to_device`], S15d (`wrap_with_code`, `wrap_with_prf` and the
    /// shares wrap these bytes), INT-S15-1.
    pub fn to_plaintext(&self) -> Secret<128> {
        let mut out = Secret::<128>::from_bytes([0u8; 128]);
        let bytes = self
            .sign_seed
            .expose()
            .iter()
            .chain(self.kx.expose().iter())
            .chain(self.pq_seed.expose().iter());
        for (dst, src) in out.expose_mut().iter_mut().zip(bytes) {
            *dst = *src;
        }
        out
    }

    /// The public half: the columns `sign_pub`, `kx_pub`, `pq_pub`.
    ///
    /// Caller: INT-S15-1 (registration), S15c2 (recipients), the tests of every wrap.
    pub fn public(&self) -> PublicBundle {
        self.public.clone()
    }

    /// What opens an envelope wrapped to this bundle: the X25519 secret and the ML-KEM seed.
    ///
    /// Caller: `open_member_key`, `open_group_key` and `open_space_key` (this crate), S15d
    /// (`open_share`), INT-S15-1.
    pub fn recipient_secret(&self) -> RecipientSecret {
        RecipientSecret::new(&self.kx, Some(&self.pq_seed))
    }

    /// The signing key (the Ed25519 seed), for the device that signs its wraps and log entries.
    ///
    /// Caller: INT-S15-1 (a device signs what it wraps), S15c2 (`execute`), S17.
    pub fn signer(&self) -> SignKeypair {
        SignKeypair::from_seed(&self.sign_seed)
    }

    /// A second owner of the same keys: the one way to copy a bundle, so every copy in a program
    /// is a call of this function.
    ///
    /// Seam: INT-S15-1's key store and S15c2's plans, which hold a bundle beside the caller's.
    pub fn duplicate(&self) -> KeyBundle {
        KeyBundle {
            sign_seed: self.sign_seed.duplicate(),
            kx: self.kx.duplicate(),
            pq_seed: self.pq_seed.duplicate(),
            public: self.public.clone(),
        }
    }
}

/// Prints the id of the bundle and `***`: no seed, no secret, not even a public key.
impl fmt::Debug for KeyBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeyBundle({}, ***)", self.public.key_id())
    }
}

// ---------------------------------------------------------------------------------------------
// The wrap of a member key to a device
// ---------------------------------------------------------------------------------------------

/// Wraps the member key `m` to the device `d` as an envelope for the context
/// `member/<member id>/key`, signed by the device `signer` with `sk` (the wrapping device: a
/// device of the same member that already holds the member key). Hybrid when the device has an
/// ML-KEM key, X25519 otherwise. Adding a device is exactly ONE such wrap, however many spaces
/// the member has (section 5.5: "Adding a device wraps one key, not every space").
///
/// The envelope layer draws, in this order: the ephemeral X25519 secret, for a hybrid wrap the
/// ML-KEM message, and last the nonce. An entropy failure, a recipient key of low order or an
/// invalid ML-KEM key returns the error and NO envelope.
///
/// Caller: INT-S15-1 (device enrollment and account setup), S17 (a remaining manager's device).
pub fn wrap_member_key_to_device(
    m: &KeyBundle,
    d: &PublicBundle,
    member: &MemberId,
    signer: &DeviceId,
    sk: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, KeyError> {
    let plaintext = m.to_plaintext();
    let ctx = Ctx::member_key(member);
    d.recipient()
        .wrap(&ctx, plaintext.expose(), signer, sk, ent)
        .map_err(KeyError::from)
}

/// Opens the wrap of a member key with the keys of one device: the envelope must be for the
/// context `member/<member id>/key` and for this device, and its signer must verify through the
/// directory at `at` (the envelope layer checks them in that order and decrypts last). The opened
/// bytes must be the member's registered keys: a wrap of any other key is
/// [`KeyError::BundleMismatch`], so a device never adopts a member key the registry does not
/// name.
///
/// Caller: INT-S15-1 (a new device adopts the member key; recovery re-opens it), S17.
pub fn open_member_key(
    env: &Envelope,
    device: &KeyBundle,
    member: &MemberId,
    expect: &PublicBundle,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<KeyBundle, KeyError> {
    let plaintext = open(
        env,
        &device.recipient_secret(),
        &Ctx::member_key(member),
        dir,
        at,
    )?;
    let bundle = KeyBundle::from_plaintext(&plaintext)?;
    if bundle.public != *expect {
        return Err(KeyError::BundleMismatch);
    }
    Ok(bundle)
}
