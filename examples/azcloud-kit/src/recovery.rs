//! A drive's recovery key at the token server (§18.7): an Ed25519 key derived from the drive's
//! recovery code - the one the user wrote down - so it exists wherever the code is typed and
//! nowhere else. Its public half is registered with the drive token
//! ([`crate::TokenServer::set_recovery_key`]); it signs a lockdown request that needs no drive
//! token ([`crate::TokenServer::recovery_lockdown`]): from a computer whose devices were taken
//! over, the drive is frozen for 48 hours, the recovery code may cancel it
//! ([`crate::TokenServer::lockdown_cancel_signed`]; F12: "the recovery code always wins", a
//! device's token alone cancels nothing), and then it is this computer's alone.
//!
//! The derivation: HKDF-SHA256 over the recovery code's bytes, salt
//! `azlin-recovery-lockdown-v1`, info the drive id - 32 bytes, the Ed25519 seed.

use std::fmt;

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

/// The HKDF salt of the derivation.
const SALT: &[u8] = b"azlin-recovery-lockdown-v1";

/// A drive's recovery key. `Debug` shows none of it.
pub struct RecoveryKey {
    signing: SigningKey,
}

impl fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecoveryKey")
            .field("key", &"<hidden>")
            .finish()
    }
}

impl RecoveryKey {
    /// The recovery key of `drive_id` from the bytes of its recovery code.
    #[must_use]
    pub fn derive(code: &[u8], drive_id: &str) -> RecoveryKey {
        let mut seed = Zeroizing::new([0_u8; 32]);
        Hkdf::<Sha256>::new(Some(SALT), code)
            .expand(drive_id.as_bytes(), &mut seed[..])
            .expect("32 bytes are a valid HKDF-SHA256 length");
        RecoveryKey {
            signing: SigningKey::from_bytes(&seed),
        }
    }

    /// The public key as the token server takes it: 32 bytes, standard base64.
    #[must_use]
    pub fn public_base64(&self) -> String {
        STANDARD.encode(self.signing.verifying_key().to_bytes())
    }

    /// The Ed25519 signature of `message`, standard base64 (what
    /// [`crate::TokenServer::recovery_lockdown`]'s signer answers).
    #[must_use]
    pub fn sign_base64(&self, message: &[u8]) -> String {
        STANDARD.encode(self.signing.sign(message).to_bytes())
    }
}
