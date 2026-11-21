//! The entry function of the fuzz target `fuzz/fuzz_targets/envelope_decode.rs`, which
//! `tests/envelope_hygiene.rs` also runs on 20,000 seeded inputs and their bit flips: a panic is a
//! failed test here and an abort in the shipped profile (planning/architecture.md section 1.4
//! rule 3: every parser has a fuzz target). The file is self-contained (it names only `azlin_keys`
//! and std) because the fuzz target includes it by `#[path]`.
//!
//! The oracles: nothing panics; every input the reader accepts writes back to exactly the same
//! bytes (the signature rule: a signature covers canonical bytes), and `check_shape` agrees with
//! the reader on whether the bytes are an envelope.

use std::sync::OnceLock;

use azlin_keys::envelope::{check_shape, open, verify_signature, Envelope, RecipientSecret};
use azlin_keys::{
    DeviceDirectory, DeviceId, DirectoryError, Secret, SignKeypair, VerifyAt, VerifyKey,
};

/// A directory that vouches for one key for every device: random bytes never carry a valid
/// signature under it, but an envelope that does reaches the key exchange and the AEAD.
struct AnyDevice(VerifyKey);

impl DeviceDirectory for AnyDevice {
    fn verify_key(&self, _device: &DeviceId, _at: VerifyAt) -> Result<VerifyKey, DirectoryError> {
        Ok(self.0)
    }
}

static RECIPIENT: OnceLock<RecipientSecret> = OnceLock::new();
static DIRECTORY: OnceLock<AnyDevice> = OnceLock::new();

fn recipient() -> &'static RecipientSecret {
    RECIPIENT.get_or_init(|| {
        RecipientSecret::new(
            &Secret::from_bytes([7u8; 32]),
            Some(&Secret::from_bytes([9u8; 64])),
        )
    })
}

fn directory() -> &'static AnyDevice {
    DIRECTORY.get_or_init(|| {
        AnyDevice(SignKeypair::from_seed(&Secret::from_bytes([5u8; 32])).verify_key())
    })
}

/// `Envelope::from_cbor`, `check_shape`, `verify_signature` and `open` on arbitrary bytes.
pub fn envelope_decode_entry(data: &[u8]) {
    let shape = check_shape(data);
    match Envelope::from_cbor(data) {
        Ok(envelope) => {
            let written = envelope.to_cbor().expect("a decoded envelope writes back");
            assert_eq!(
                written.as_slice(),
                data,
                "a decoded envelope does not write back to its bytes"
            );
            if data.len() <= azlin_keys::envelope::MAX_ENVELOPE {
                assert!(shape.is_ok(), "check_shape refuses what the reader accepts");
            }
            let dir = directory();
            let _ = verify_signature(&envelope, dir, VerifyAt::Now);
            let _ = open(&envelope, recipient(), &envelope.ctx, dir, VerifyAt::Now);
        }
        Err(_) => {
            assert!(
                shape.is_err(),
                "check_shape accepts what the reader refuses"
            );
        }
    }
}
