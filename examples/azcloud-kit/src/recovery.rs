//! A drive's recovery key at the token server (§18.7): an Ed25519 key derived from the drive's
//! recovery code - the one the user wrote down - so it exists wherever the code is typed and
//! nowhere else. Its public half is registered with the drive token
//! ([`crate::TokenServer::set_recovery_key`]); it signs a lockdown request that needs no drive
//! token ([`crate::TokenServer::recovery_lockdown`]): from a computer whose devices were taken
//! over, the drive is frozen for 48 hours, the owner's other devices may cancel it, and then it
//! is this computer's alone.
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
/// The HKDF info of a code's FINDABLE key ([`RecoveryKey::derive_findable`]): no drive id has
/// this form, so the key is the code's alone.
pub const FINDABLE_INFO: &str = "azlin-recovery-findable-v1";

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

    /// The FINDABLE recovery key of a recovery code: [`RecoveryKey::derive`] with
    /// [`FINDABLE_INFO`] instead of a drive id. Registered as one more recovery key of the
    /// drive, it names the drive to a computer that never had it - the kit leaves the drive id
    /// out - through [`crate::TokenServer::recovery_lookup`]; it signs a lockdown too.
    #[must_use]
    pub fn derive_findable(code: &[u8]) -> RecoveryKey {
        RecoveryKey::derive(code, FINDABLE_INFO)
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
