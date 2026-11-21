//! The group key, per epoch, and its wrap to each member of the group (build unit S15, lane
//! W1-S15c1).
//!
//! Implements planning/architecture.md section 5.5 "Keys": "Group key, per epoch: wrapped to each
//! member of the group. Spaces granted to a group wrap their key to the group key, so one member
//! leaving costs (members of the group) + (spaces of the group) wraps, not members x spaces", and
//! section 5.6 "What a removal costs".
//!
//! # The group key is an X25519 key pair
//!
//! The plan says a space key is wrapped "to the group key" and does not say what kind of key that
//! is: a symmetric key can be wrapped only by someone who holds it, a key pair by anyone who knows
//! its public half. P-3 of the brief makes it an X25519 key pair (DRIFT X-DELTA-W1S15C1-3): the
//! 32 secret bytes are what a member's wrap carries, and a space key is wrapped to the group with
//! the same sealed-box code (algorithm 1) as to a member. A group key is therefore a classical
//! recipient even in a build with the feature `pq`; the owner may want an ML-KEM half derived
//! the way the epoch chain does it (see `space.rs`).
//!
//! A [`GroupKey`] draws 32 bytes from the entropy seam ([`GroupKey::generate`]). Its public half
//! is a [`GroupPublic`]: the group, the epoch and the X25519 public key, which is all a wrapper of
//! a space key to the group needs. The registry has no column for it (DRIFT X-DELTA-W1S15C1-6).
//!
//! The envelope of a member's wrap has the context `group/<group id>/epoch/<n>`, so the epoch is
//! bound by the associated data and the signature: a group key of another epoch is refused as
//! `CtxMismatch` before anything is decrypted.
//!
//! The public half can be copied; the key cannot (the first example is the control: the same call
//! compiles for the public half):
//!
//! ```
//! use azlin_keys::group::GroupPublic;
//! fn needs_clone<T: Clone>() {}
//! needs_clone::<GroupPublic>();
//! ```
//!
//! ```compile_fail
//! use azlin_keys::group::GroupKey;
//! fn needs_clone<T: Clone>() {}
//! needs_clone::<GroupKey>();
//! ```
//!
//! ```compile_fail
//! use azlin_keys::group::GroupKey;
//! fn needs_copy<T: Copy>() {}
//! needs_copy::<GroupKey>();
//! ```

use core::fmt;

use crate::derive::{derive_public, GROUP_KEY_ID};
use crate::envelope::{open, Ctx, Envelope, RecipientSecret, RecipientX25519};
use crate::ids::{DeviceId, Epoch, GroupId, KeyId};
pub use crate::member::KeyError;
use crate::member::{KeyBundle, PublicBundle};
use crate::seam::Entropy;
use crate::secret::Secret;
use crate::sign::{DeviceDirectory, SignKeypair, VerifyAt};

/// The public half of a group key: what someone who wraps a space key to a group needs to know.
/// Public data: `Copy`, compared and hashed as bytes.
///
/// Caller: S15c2 (`WrapTarget::Group` of a rekey plan), S16 (a grant to a group), INT-S15-1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GroupPublic {
    /// The group.
    pub group: GroupId,
    /// The epoch of the group key.
    pub epoch: Epoch,
    /// The X25519 public key of the group key.
    pub kx_pub: [u8; 32],
}

impl GroupPublic {
    /// The id of the group key epoch: `derive_key("azlin 2026-11 group key id v1", group ||
    /// epoch || kx_pub)` with the epoch as four bytes, big-endian. It commits to the group, the
    /// epoch and the public key, so two epochs of a group never share an id.
    ///
    /// Caller: S15c2 (the registry's `reg_group.key_epoch` and the rekey plan name the key by it),
    /// S16.
    pub fn id(&self) -> KeyId {
        let mut material = Vec::with_capacity(16 + 4 + 32);
        material.extend_from_slice(self.group.as_bytes());
        material.extend_from_slice(&self.epoch.get().to_be_bytes());
        material.extend_from_slice(&self.kx_pub);
        KeyId::from_bytes(derive_public(&GROUP_KEY_ID, &material))
    }

    /// The recipient a space key is wrapped to: the group's X25519 key.
    ///
    /// Caller: `WrapTarget::recipient` (space.rs), S15c2.
    pub fn recipient(&self) -> RecipientX25519 {
        RecipientX25519 { kx: self.kx_pub }
    }

    /// The `rcpt` of an envelope wrapped to this group key, and the "recipient key id" of the
    /// `reg_wrap` row of a space key wrapped to a group. It is the id of the X25519 key alone
    /// (`KeyId::of_bundle` of `kx_pub`), not [`GroupPublic::id`].
    ///
    /// Caller: S15c2 (`reg_wrap` rows of a space key for a group), S9.
    pub fn recipient_id(&self) -> KeyId {
        self.recipient().key_id()
    }
}

/// The key of one epoch of a group: an X25519 secret, wrapped to every member of the group. A
/// space key granted to the group is wrapped to its public half, so a member opens the group key
/// first and the space key with it (section 5.6). The secret is a [`Secret`] (wiped on drop,
/// never printed); the type is not `Clone`, not `Copy`, and its `Debug` shows no byte. A second
/// owner is made only by [`GroupKey::duplicate`].
///
/// Caller: INT-S15-1 (the client's group flows), S15c2 (a removal makes a new group key epoch and
/// wraps it to the remaining members), S17.
pub struct GroupKey {
    /// The epoch of the key, from 1: the first group key is epoch 1 and a rotation is one more.
    /// Read it; never change it.
    pub epoch: Epoch,
    group: GroupId,
    key: Secret<32>,
    kx_pub: [u8; 32],
}

impl GroupKey {
    /// The key of these 32 secret bytes (the X25519 secret) for the group and the epoch.
    fn from_secret(group: &GroupId, epoch: Epoch, key: Secret<32>) -> GroupKey {
        let kx_pub = *RecipientSecret::new(&key, None).kx_public();
        GroupKey {
            epoch,
            group: *group,
            key,
            kx_pub,
        }
    }

    /// A fresh group key for `epoch`: 32 bytes from the entropy seam (one draw). A failed draw is
    /// [`KeyError::Entropy`] and no key exists. Whether `epoch` follows the group's previous one
    /// is the rekey plan's rule (S15c2), not this function's.
    ///
    /// Caller: S15c2 (`execute` of a rekey), INT-S15-1 (a new group).
    pub fn generate(
        group: &GroupId,
        epoch: Epoch,
        ent: &mut dyn Entropy,
    ) -> Result<GroupKey, KeyError> {
        // DRIFT X-DELTA-W1S15C1-3: the group key is an X25519 secret (P-3).
        let key = Secret::<32>::generate(ent)?;
        Ok(GroupKey::from_secret(group, epoch, key))
    }

    /// The group key of these 32 bytes, the bytes [`GroupKey::to_plaintext`] makes; any other
    /// length is [`KeyError::BadPlaintext`].
    ///
    /// Caller: [`open_group_key`], INT-S15-1 (a stored key is loaded).
    pub fn from_plaintext(
        group: &GroupId,
        epoch: Epoch,
        bytes: &[u8],
    ) -> Result<GroupKey, KeyError> {
        let key = Secret::<32>::from_slice(bytes).ok_or(KeyError::BadPlaintext)?;
        Ok(GroupKey::from_secret(group, epoch, key))
    }

    /// The 32 bytes a wrap of this key carries: the X25519 secret.
    ///
    /// Caller: [`wrap_group_key_to_member`], INT-S15-1 (the key store).
    pub fn to_plaintext(&self) -> Secret<32> {
        self.key.duplicate()
    }

    /// The group this key belongs to.
    ///
    /// Caller: S15c2 (a plan checks that a key is the group's), INT-S15-1.
    pub fn group(&self) -> &GroupId {
        &self.group
    }

    /// The public half: the group, the epoch and the X25519 public key.
    ///
    /// Caller: S15c2 (`WrapTarget::Group`), INT-S15-1.
    pub fn public(&self) -> GroupPublic {
        GroupPublic {
            group: self.group,
            epoch: self.epoch,
            kx_pub: self.kx_pub,
        }
    }

    /// The id of this group key epoch ([`GroupPublic::id`]).
    ///
    /// Caller: S15c2 (the registry's `reg_group.key_epoch`), INT-S15-1.
    pub fn id(&self) -> KeyId {
        self.public().id()
    }

    /// The recipient a space key is wrapped to (P-3: the group key is an X25519 key pair, so a
    /// space key is wrapped to a group with the same sealed-box code as to a member).
    ///
    /// Caller: S15c2 (spaces granted to a group wrap their key to it), INT-S15-1.
    pub fn as_recipient(&self) -> RecipientX25519 {
        RecipientX25519 { kx: self.kx_pub }
    }

    /// What opens an envelope wrapped to this group key: the X25519 secret.
    ///
    /// Caller: `open_space_key_with_group` (space.rs), INT-S15-1.
    pub fn recipient_secret(&self) -> RecipientSecret {
        RecipientSecret::new(&self.key, None)
    }

    /// A second owner of the same key: the one way to copy a group key.
    ///
    /// Seam: INT-S15-1's key store and S15c2's plans, which hold a key beside the caller's.
    pub fn duplicate(&self) -> GroupKey {
        GroupKey {
            epoch: self.epoch,
            group: self.group,
            key: self.key.duplicate(),
            kx_pub: self.kx_pub,
        }
    }
}

/// Prints the group, the epoch and `***`: no byte of the key, not even its public half.
impl fmt::Debug for GroupKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GroupKey({}, epoch {}, ***)", self.group, self.epoch)
    }
}

/// Wraps the group key `g` to the member `m` as an envelope for the context
/// `group/<group id>/epoch/<n>`, signed by the device `signer` with `sk`. Hybrid when the member
/// has an ML-KEM key, X25519 otherwise. `group` must be the key's own group
/// ([`KeyError::WrongGroup`] otherwise). A removal wraps the new group key to each remaining
/// member: one call each (section 5.6).
///
/// The envelope layer draws, in this order: the ephemeral X25519 secret, for a hybrid wrap the
/// ML-KEM message, and last the nonce; a failed draw returns the error and NO envelope.
///
/// Caller: S15c2 (`execute` of a rekey), INT-S15-1 (a member joins a group).
pub fn wrap_group_key_to_member(
    g: &GroupKey,
    group: &GroupId,
    m: &PublicBundle,
    signer: &DeviceId,
    sk: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, KeyError> {
    if g.group != *group {
        return Err(KeyError::WrongGroup);
    }
    let plaintext = g.to_plaintext();
    let ctx = Ctx::group_epoch(group, g.epoch);
    m.recipient()
        .wrap(&ctx, plaintext.expose(), signer, sk, ent)
        .map_err(KeyError::from)
}

/// Opens the wrap of the group key of `epoch` with a member's keys: the envelope must be for the
/// context `group/<group id>/epoch/<epoch>` and for this member, and its signer must verify
/// through the directory at `at`. A group key of another epoch, or of another group, is refused
/// as `CtxMismatch` before any decryption; a member the key was not wrapped to as `NotForMe`.
///
/// Caller: `open_space_key_via_group` (space.rs), INT-S15-1 (a member reads a group's spaces).
pub fn open_group_key(
    env: &Envelope,
    group: &GroupId,
    epoch: Epoch,
    member: &KeyBundle,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<GroupKey, KeyError> {
    let plaintext = open(
        env,
        &member.recipient_secret(),
        &Ctx::group_epoch(group, epoch),
        dir,
        at,
    )?;
    GroupKey::from_plaintext(group, epoch, &plaintext)
}
