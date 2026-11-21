//! Envelope v2: the wrap format of every key the engine moves, algorithms 1 and 2 (the X25519
//! sealed box and the X25519 + ML-KEM-768 hybrid), the signer, and the node-side shape check
//! (planning/architecture.md section 5.5 "Envelope v2 (every wrap)", section 5.3 `reg_wrap`,
//! section 2.5 signed structures; build unit S15, lane W1-S15b).
//!
//! | field | meaning |
//! |---|---|
//! | `v` | 2 |
//! | `alg` | 1 X25519 sealed box; 2 X25519 + ML-KEM-768 hybrid; 3 Argon2id passphrase; 4 PRF; 5 Shamir share; 6 sealed to an attested key; 7 threshold escrow |
//! | `rcpt` | the id of the recipient's public keys (`KeyId::of_bundle`) |
//! | `epk`, `kem_ct` | the ephemeral X25519 key; the ML-KEM ciphertext |
//! | `nonce`, `ct` | XChaCha20-Poly1305 |
//! | `ctx` | what is wrapped (`space/<id>/epoch/<n>`); also the associated data |
//! | `signer`, `sig` | the wrapping device and its signature |
//!
//! # What is here
//!
//! - [`Envelope`]: the strict deterministic CBOR format (module [`wire`]), with `to_cbor`,
//!   `from_cbor`, the signing input and the associated data. Unknown keys are kept, `crit` is
//!   kept (rule R18), a decoded envelope writes back to the same bytes.
//! - [`Ctx`]: what is wrapped, with typed constructors and a strict grammar (module [`ctx`]).
//! - [`wrap_x25519`], [`wrap_hybrid`], [`open`]: algorithms 1 and 2, signed by the wrapping
//!   device. Algorithm 2 needs the feature `pq` (not a default feature until the owner has
//!   reviewed the ML-KEM crate); without it `wrap_hybrid` and the open of an algorithm 2
//!   envelope are `Unsupported`, and nothing falls back to algorithm 1.
//! - [`seal_under`] and [`open_under`] (the AEAD layer) and [`seal_with_key`] and
//!   [`open_with_key`] (the same pipeline for a key the caller derived): what lane W1-S15d builds
//!   algorithms 3, 4 and 5 on, and S17 algorithms 6 and 7.
//! - [`check_shape`] and [`verify_signature`]: the KEY-LESS functions the registry calls on every
//!   `reg_wrap` write, which it stores untouched and can never open.
//! - [`ALG_TABLE`] and [`rules`]: the algorithm table and the field rules per algorithm, as data.
//!
//! # The order of things
//!
//! A wrap draws, in this order, from the entropy seam: the 32-byte ephemeral X25519 secret, for
//! algorithm 2 the 32-byte ML-KEM message, and last the 24-byte nonce. `open` checks, in this
//! order: the format, the algorithm, that the context is the expected one, that the envelope is
//! for this recipient, the signature of the wrapping device, and only then the key exchange and
//! the AEAD. The associated data is the canonical CBOR of the header fields; the signature
//! covers everything but itself, ciphertext included.
//!
//! ```
//! # #[cfg(feature = "os-rng")]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use azlin_keys::envelope::{check_shape, open, wrap_x25519, Ctx, Envelope, RecipientSecret};
//! use azlin_keys::{
//!     DeviceDirectory, DeviceId, DirectoryError, Epoch, OsEntropy, Secret, SignKeypair, SpaceId,
//!     VerifyAt, VerifyKey,
//! };
//!
//! /// A directory that knows one device.
//! struct OneDevice(VerifyKey);
//! impl DeviceDirectory for OneDevice {
//!     fn verify_key(&self, _: &DeviceId, _: VerifyAt) -> Result<VerifyKey, DirectoryError> {
//!         Ok(self.0)
//!     }
//! }
//!
//! let mut ent = OsEntropy;
//! let me = RecipientSecret::new(&Secret::<32>::generate(&mut ent)?, None);
//! let device = DeviceId::generate(&mut ent)?;
//! let signer = SignKeypair::generate(&mut ent)?;
//! let ctx = Ctx::space_epoch(&SpaceId::generate(&mut ent)?, Epoch::FIRST);
//!
//! let envelope = wrap_x25519(&me.recipient_x25519(), &ctx, b"a space key", &device, &signer, &mut ent)?;
//! let bytes = envelope.to_cbor()?;
//!
//! // A node checks the shape without any key ...
//! assert_eq!(check_shape(&bytes)?.ctx, ctx);
//! // ... and the recipient opens it, for the context it expects.
//! let envelope = Envelope::from_cbor(&bytes)?;
//! let plaintext = open(&envelope, &me, &ctx, &OneDevice(signer.verify_key()), VerifyAt::Now)?;
//! assert_eq!(plaintext.as_slice(), b"a space key");
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "os-rng"))]
//! # fn main() {}
//! ```

pub mod aead;
pub mod alg_table;
pub mod combiner;
pub mod ctx;
pub mod error;
pub mod hybrid;
pub mod opening;
pub mod recipient;
pub mod verify;
pub mod wire;
pub mod x25519;

#[cfg(test)]
mod internal_tests;

pub use aead::{open_under, seal_under, NONCE_LEN, TAG_LEN};
pub use alg_table::{rules, Alg, FieldRules, ParamsKind, Presence, ALG_TABLE};
pub use ctx::{Ctx, CtxError, CtxKind, RecoveryMethod, MAX_CTX_LEN};
pub use error::{EnvelopeError, OpenError, ParamsError, WrapError};
pub use hybrid::{wrap_hybrid, RecipientHybrid, PQ_KEY_LEN};
pub use opening::{open, open_with_key, seal_with_key};
pub use recipient::{Recipient, RecipientSecret};
pub use verify::{check_shape, verify_signature, EnvelopeMeta, MAX_ENVELOPE};
pub use wire::{Envelope, Header, Params, KEM_CT_LEN, MAX_PLAINTEXT};
pub use x25519::{wrap_x25519, RecipientX25519};
