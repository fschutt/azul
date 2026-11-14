//! Cosignatures: a witness's signed statement that a checkpoint is consistent with everything it has seen
//! (C2SP `tlog-cosignature`, "cosignature/v1"; planning/architecture.md section 2.20: "a Merkle tree whose
//! checkpoints independent witnesses cosign").
//!
//! The layout of a cosignature line (T8): the key hash (type byte `0x04`), then 8 bytes of Unix time in
//! big-endian order, then the 64-byte Ed25519 signature over
//!
//! ```text
//! cosignature/v1\n
//! time <decimal time>\n
//! <the checkpoint text, with its final newline>
//! ```
//!
//! The time is inside the signed message (T10): a relay that changes it breaks the signature, and a witness
//! that backdates it is outvoted by the quorum rule of [`Policy`](crate::Policy) (T12).
//!
//! Seam: the first callers are `witness::Witness::add_checkpoint` (every cosignature a witness makes) and
//! `policy::Policy::check` (every cosignature a verifier reads); the Keychain- or HSM-backed cosigner of a
//! production witness is a later unit.

use crate::checkpoint::Checkpoint;
use crate::names::{KeyHash, KeyName, SigKind};
use crate::note::{MemoryKey, NoteSig, SignError};
use crate::time::UnixTime;

/// The message a cosignature signs: `cosignature/v1\ntime <t>\n` and the checkpoint text.
pub(crate) fn cosignature_message(text: &str, time: UnixTime) -> Vec<u8> {
    let mut message = Vec::with_capacity(48 + text.len());
    message.extend_from_slice(b"cosignature/v1\ntime ");
    message.extend_from_slice(time.get().to_string().as_bytes());
    message.push(b'\n');
    message.extend_from_slice(text.as_bytes());
    message
}

/// A key that cosigns checkpoint texts: a witness's key (or the log operator's own, which counts as no
/// witness). The signature is type `0x04`.
///
/// Seam: first non-test implementors are the witnesses of S45-INT-1 and the Keychain-backed operator key.
pub trait Cosigner {
    /// The cosigner's name.
    fn name(&self) -> &KeyName;
    /// The key hash of this key for type `0x04`.
    fn key_hash(&self) -> KeyHash;
    /// The cosignature line for the checkpoint text `checkpoint_text` at `time` (the text of a checkpoint,
    /// with its final newline).
    fn cosign(&self, checkpoint_text: &str, time: UnixTime) -> Result<NoteSig, SignError>;
}

impl Cosigner for MemoryKey {
    fn name(&self) -> &KeyName {
        MemoryKey::name(self)
    }

    fn key_hash(&self) -> KeyHash {
        self.hash_for(SigKind::Cosignature)
    }

    fn cosign(&self, checkpoint_text: &str, time: UnixTime) -> Result<NoteSig, SignError> {
        // Only a checkpoint text is signed under the cosignature domain.
        if Checkpoint::parse_text(checkpoint_text).is_err() {
            return Err(SignError::NotACheckpoint);
        }
        let signature = self.sign_bytes(&cosignature_message(checkpoint_text, time));
        let mut payload = Vec::with_capacity(8 + signature.len());
        payload.extend_from_slice(&time.get().to_be_bytes());
        payload.extend_from_slice(&signature);
        Ok(NoteSig::raw(
            MemoryKey::name(self).clone(),
            self.hash_for(SigKind::Cosignature),
            payload,
        ))
    }
}
