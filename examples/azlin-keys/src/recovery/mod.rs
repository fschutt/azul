//! Key recovery: how a person gets their member key back (planning/architecture.md section 5.5
//! "Recovery", "Envelope v2 (every wrap)", decision D51 "at least two independent methods per
//! account", section 7.4 "The paper key", section 7.5 "Notice and hold"; build unit S15, lane
//! W1-S15d). The pure logic only: no platform call (WebAuthn, the keychain, QR codes, the save
//! dialog), no clock (the 48 hour hold and its notices are the server's, section 7.5), no screen.
//!
//! | what | where | wraps |
//! |---|---|---|
//! | the recovery code: 128 bits as 26 Crockford symbols in five groups, its check value | [`code`] | |
//! | the written-down quiz: 4 of the 5 groups | [`quiz`] | |
//! | algorithm 3: the member key under the code, Argon2id | [`passphrase`] | `recovery/<member>/code` |
//! | algorithm 4: the member key under a passkey's PRF output | [`prf`] | `recovery/<member>/prf` |
//! | Shamir shares of the CODE over GF(256) | [`shamir`] | |
//! | algorithm 5: a share sealed to a trusted contact | [`sealed`] | `recovery/<member>/share` |
//! | the text forms, the safety number | [`contacts`] | |
//!
//! THE CHAIN: the code wraps the member key (algorithm 3); its 2-of-3 Shamir shares go to
//! contacts (algorithm 5), and two shares give the CODE back, which opens the algorithm 3 wrap. No
//! share ever wraps the member key directly. The passkey wrap (algorithm 4) is the second
//! independent method for personal and family accounts (on by default, section 5.5). Every
//! recovery meets the same hold on the server (section 7.5).
//!
//! THE ORDER OF THE CHECKS of every unwrap (`open::open_recovery`): the format, the algorithm, the
//! context, the cheap checks of the wrap (the Argon2id cost against the floor and the limits, the
//! recipient of a share), the signature of the wrapping device, and only then the key derivation
//! and the cipher: a hostile server never makes a device pay for Argon2id.
//!
//! ```
//! # #[cfg(feature = "test-support")]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use azlin_keys::envelope::{Ctx, RecoveryMethod};
//! use azlin_keys::recovery::{
//!     combine, split, unwrap_with_code, wrap_with_code, KdfFloor, KdfProfile, RecoveryCode,
//! };
//! use azlin_keys::{
//!     DeviceId, MapDirectory, MemberId, Secret, SeededEntropy, SignKeypair, VerifyAt,
//! };
//!
//! let mut ent = SeededEntropy::from_u64(7);
//! let member = MemberId::generate(&mut ent)?;
//! let device = DeviceId::generate(&mut ent)?;
//! let signer = SignKeypair::generate(&mut ent)?;
//! let mut dir = MapDirectory::new();
//! dir.insert_active(device, signer.verify_key());
//!
//! // Setup: a code, the member key under it, and 3 shares of the code (any 2 give it back).
//! let code = RecoveryCode::generate(&mut ent)?;
//! let member_key = Secret::<32>::generate(&mut ent)?;
//! let ctx = Ctx::recovery(&member, RecoveryMethod::Code);
//! let wrap = wrap_with_code(&member_key, &code, &ctx, KdfProfile::Test, &device, &signer, &mut ent)?;
//! let shares = split(code.as_secret(), 2, 3, &mut ent)?;
//!
//! // Recovery: two shares give the code, the code opens the wrap.
//! let back = RecoveryCode::from_secret(combine(&shares[1..])?);
//! let opened = unwrap_with_code(&wrap, &back, &ctx, &dir, VerifyAt::Now, KdfFloor::TEST)?;
//! assert_eq!(opened.as_slice(), member_key.expose());
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "test-support"))]
//! # fn main() {}
//! ```

pub mod code;
pub mod contacts;
pub mod error;
mod open;
pub mod passphrase;
pub mod prf;
pub mod quiz;
pub mod sealed;
pub mod shamir;

#[cfg(test)]
mod internal_tests;

pub use code::{RecoveryCode, CHECK_LEN, CODE_LEN, CODE_SYMBOLS, GROUP_LENGTHS};
pub use contacts::{
    new_contact_secret, printed_share_from_text, printed_share_text, reply_from_text, reply_text,
    safety_number, safety_number_text, share_from_text, share_text, ContactKey, RequestKey,
    CONTACT_PREFIX, MAX_TEXT_LEN, REPLY_PREFIX, REQUEST_PREFIX, SAFETY_DIGITS, SHARE_PREFIX,
};
pub use error::RecoveryError;
pub use open::{LOCAL_CONTEXTS, SAFETY_NUMBER_CONTEXT, SLOT_ID_CONTEXT};
pub use passphrase::{
    unwrap_with_code, wrap_with_code, KdfFloor, KdfParams, KdfProfile, Kib, MAX_KDF_LANES,
    MAX_KDF_MEMORY, MAX_KDF_PASSES, MAX_KDF_WORK,
};
pub use prf::{new_prf_salt, prf_inputs, unwrap_with_prf, wrap_with_prf, PrfInputs, PRF_BYTES};
pub use quiz::{Quiz, QuizFailure, QUIZ_ASKED, QUIZ_GROUPS};
pub use sealed::{open_share, seal_share_to_contact, OpenedShare, MAX_OWNER_WORDS};
pub use shamir::{
    combine, gf_inv, gf_mul, split, Share, DEFAULT_SHARES, DEFAULT_THRESHOLD, MAX_SHARES,
};
