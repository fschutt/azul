//! The author's signature over a pointer, and the witness that it was checked (planning/architecture.md
//! section 5.7 "Pointer v2": `author_device`, `sig`; rule M2 of section 6.10: "pointers against their
//! author's signature"; build unit S14, lane W1-S14d).
//!
//! THE SIGNATURE (brief P-1, DRIFT X-DELTA-W1S14D-1): Ed25519 over the one signing rule of ARCH29 9.13
//! (`azlin_keys::sign::signing_input`), domain `azlin-pointer-v2`, of
//!
//! ```text
//! space_id (16 bytes) || the canonical CBOR of the pointer WITHOUT key 14 (sig)
//! ```
//!
//! so a pointer signed for one space is refused in another, and every field is covered: the unknown
//! ones, `crit`, every chunk reference and the author's own id. The preimage holds the chunk hashes
//! (key material), so it is built in `Zeroizing` buffers and handed to the raw layer of azlin-keys
//! (`sign_preimage`, `verify_preimage`) framed by `signing_input`: the framing is the one definition.
//!
//! WHAT THIS DOES NOT BIND (open, DRIFT X-DELTA-W1S14D-1): the file's place in the repository (its
//! path or node id) and the previous version's hash, so a member who holds `K_seal` can move a signed
//! pointer to another file; that needs the tree entry to be signed as well, which the plan does not
//! say.
//!
//! THE INSTANT (DRIFT X-DELTA-W1S14D-2): `verify` takes `VerifyAt`. The pointer's own `created_at` is
//! the author's clock and a revoked device can backdate it, so a verifier asks the directory at the
//! repository log entry's chain position (`VerifyAt::ChainSeq`), not at `created_at`.

use core::fmt;
use core::ops::Deref;

use azlin_keys::sign::{sign_preimage, signing_input, verify_preimage, POINTER_V2};
use azlin_keys::{DeviceDirectory, DeviceId, DirectoryError, SignKeypair, SpaceId, VerifyAt};
use zeroize::Zeroizing;

use super::error::PointerError;
use super::PointerV2;

/// The witness that a pointer's signature was checked: it borrows the pointer, so it is a statement
/// about THAT pointer (which cannot change while the witness lives), by this author, for this space, at
/// this instant. Only [`PointerV2::verify`] makes one (the fields are private, there is no `Clone`,
/// `Default` or `From`). It dereferences to the pointer.
///
/// ```compile_fail
/// use azlin_content::pointer::Verified;
/// fn needs_clone<T: Clone>() {}
/// needs_clone::<Verified<'static>>();
/// ```
///
/// Caller: INT-S14-1 and INT-S14-2 (the apply step takes a `Verified`), S14e2 (the live set is read
/// from verified pointers).
pub struct Verified<'p> {
    pointer: &'p PointerV2,
    author: DeviceId,
    space: SpaceId,
    at: VerifyAt,
}

impl<'p> Verified<'p> {
    /// The pointer whose signature was checked.
    ///
    /// Caller: INT-S14-1, INT-S14-2.
    pub fn pointer(&self) -> &'p PointerV2 {
        self.pointer
    }

    /// The device whose key verified the signature.
    ///
    /// Caller: INT-S14-1, INT-S14-2 (who made this version).
    pub fn author(&self) -> DeviceId {
        self.author
    }

    /// The space the signature was checked for.
    ///
    /// Caller: INT-S14-1.
    pub fn space(&self) -> SpaceId {
        self.space
    }

    /// The instant the author's key was asked for.
    ///
    /// Caller: INT-S14-1 (a log of what was verified when).
    pub fn at(&self) -> VerifyAt {
        self.at
    }
}

impl Deref for Verified<'_> {
    type Target = PointerV2;

    fn deref(&self) -> &PointerV2 {
        self.pointer
    }
}

impl fmt::Debug for Verified<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Verified")
            .field("author", &self.author)
            .field("space", &self.space)
            .field("at", &self.at)
            .finish()
    }
}

/// The directory's refusal as the pointer's: an unknown device, a key not active yet, a revoked key and
/// a directory that cannot answer are four different words, and none becomes a default key.
fn from_directory(e: DirectoryError) -> PointerError {
    match e {
        DirectoryError::Unknown => PointerError::UnknownAuthor,
        DirectoryError::NotYetActive => PointerError::AuthorNotYetActive,
        DirectoryError::Revoked => PointerError::AuthorRevoked,
        DirectoryError::Unavailable => PointerError::DirectoryUnavailable,
    }
}

impl PointerV2 {
    /// The bytes the signature covers, framed: `"azlin-pointer-v2" || 0x00 || space_id || the canonical
    /// CBOR without key 14`. Every buffer on the way is wiped when it drops.
    fn preimage(&self, space: &SpaceId) -> Result<Zeroizing<Vec<u8>>, PointerError> {
        let body = self.encode_unsigned()?;
        let mut message =
            Zeroizing::new(Vec::with_capacity(SpaceId::LEN.saturating_add(body.len())));
        message.extend_from_slice(space.as_bytes());
        message.extend_from_slice(&body);
        Ok(Zeroizing::new(signing_input(&POINTER_V2, &message)))
    }

    /// Signs the pointer for `space` with the author device's key: `sig` becomes Ed25519 over the
    /// preimage of the module documentation. The pointer is validated first
    /// (a pointer that this build would refuse to write is not signed), and a signature the pointer
    /// cannot carry (it would not encode within the limits) is not set. Signing again replaces the
    /// signature; the signature is deterministic.
    ///
    /// The key is the author device's: this call cannot know that `author_device` names it (there is no
    /// directory here), and [`PointerV2::verify`] finds out.
    ///
    /// Caller: INT-S14-1 and INT-S14-2 (the upload pipeline signs the pointer it built before the
    /// repository's log entry), INT-S16 (log records carry signed pointers).
    pub fn sign(&mut self, space: &SpaceId, key: &SignKeypair) -> Result<(), PointerError> {
        self.validate()?;
        let preimage = self.preimage(space)?;
        let signature = sign_preimage(&preimage, key);
        let previous = self.sig.replace(signature);
        if let Err(e) = self.to_cbor() {
            self.sig = previous;
            return Err(e);
        }
        Ok(())
    }

    /// Checks the author's signature, in this order: there is a signature (`Unsigned`); the directory
    /// names a key for `author_device` at `at` (`UnknownAuthor`, `AuthorNotYetActive`, `AuthorRevoked`,
    /// `DirectoryUnavailable`: unknown is a state, never "valid"); the signature verifies over the
    /// preimage of `space` (`BadSignature`: another space, another key, any changed field, the unknown
    /// fields and `crit` included); and only then the content is judged ([`PointerV2::validate`]), so a
    /// flipped field is a bad signature whatever else it breaks (DRIFT X-DELTA-W1S14D-11). The witness
    /// [`Verified`] borrows the pointer.
    ///
    /// `at` is the instant of the author's key: pass the repository log entry's chain position
    /// (`VerifyAt::ChainSeq`), not the pointer's `created_at`.
    ///
    /// Caller: INT-S14-1 and INT-S14-2 (every pointer read from the repository), INT-S16 (log
    /// records), the fuzz target `pointer_decode`.
    pub fn verify<'p>(
        &'p self,
        space: &SpaceId,
        dir: &dyn DeviceDirectory,
        at: VerifyAt,
    ) -> Result<Verified<'p>, PointerError> {
        let signature = self.sig.ok_or(PointerError::Unsigned)?;
        let key = dir
            .verify_key(&self.author_device, at)
            .map_err(from_directory)?;
        let preimage = self.preimage(space)?;
        verify_preimage(&preimage, &key, &signature).map_err(|_| PointerError::BadSignature)?;
        self.validate()?;
        Ok(Verified {
            pointer: self,
            author: self.author_device,
            space: *space,
            at,
        })
    }
}
