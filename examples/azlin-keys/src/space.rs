//! The space key, per epoch: `K_seal` and `K_chunk`, the dedup epoch, the epoch chain ("epoch n
//! under epoch n+1"), the two history modes, and the wraps of a space key to a member or a group
//! (build unit S15, lane W1-S15c1).
//!
//! Implements planning/architecture.md section 5.5 "Keys" ("Space key, per epoch. BLAKE3
//! `derive_key` with separate contexts gives `K_seal` ... and `K_chunk`"; "Rotation: a new epoch
//! key, wrapped only to the remaining members and groups, plus the chain wrap (epoch n under
//! epoch n+1)"; "Dedup key epochs: `K_chunk` rotates only on an explicit re-key space, not at every
//! removal"), section 5.6 "What a removal costs" and section 5.4 (`reg_space.history`: `full`,
//! joiners get the epoch chain, or `forward`, rotate on add, no chain for joiners).
//!
//! # The material of one epoch (P-1, DRIFT X-DELTA-W1S15C1-1)
//!
//! Section 5.5 asks for two things that one key cannot give: a space key that rotates at every
//! removal and a `K_chunk` that does NOT rotate at every removal. So the material of an epoch is
//! `(seal_seed, dedup_seed, dedup_epoch)`. `K_seal = derive_key(K_SEAL, seal_seed)` and `K_chunk =
//! derive_key(K_CHUNK, dedup_seed)`. A removal rotation draws a FRESH `seal_seed` and CARRIES
//! `(dedup_seed, dedup_epoch)` ([`DedupChoice::Carry`]); an explicit re-key of the space draws
//! both fresh and sets `dedup_epoch` to the new epoch ([`DedupChoice::Fresh`]). The new
//! `seal_seed` is entropy and nothing else: no function of the previous epoch's material gives
//! it, so a removed member who knows epoch n cannot compute epoch n+1. A `forward` joiner, who
//! receives only the newest epoch, still receives the current dedup seed and so the current
//! `K_chunk`.
//!
//! The plaintext a wrap carries is the 68 bytes `seal_seed || dedup_seed || dedup_epoch` with the
//! dedup epoch as four bytes, big-endian (DRIFT X-DELTA-W1S15C1-10). The epoch and the space are
//! in the context of the envelope, not in the plaintext.
//!
//! # The key id
//!
//! [`SpaceKey::key_id`] is what a signed `space_create` or `space_rekey` chain entry commits to
//! ("an epoch counts only if a signed chain entry from an authorized device commits to its key
//! id"): `derive_key("azlin 2026-11 space key id v1", space || epoch || plaintext)`. It commits to
//! the whole material, so an epoch planted with other seeds has another id.
//!
//! # The epoch chain (DRIFT X-DELTA-W1S15C1-5)
//!
//! A chain link is epoch n sealed to epoch n+1, so the holder of the newest key reads every older
//! epoch. Envelope v2 has no symmetric algorithm, so a link is a wrap to a public key that the
//! newer epoch derives from its own `seal_seed`: BLAKE3 `derive_key` under the context `azlin
//! 2026-11 epoch chain wrap v1` over the seed, read as an extendable output; the first 32 bytes are
//! an X25519 secret and the next 64 an ML-KEM-768 seed. The link is hybrid (algorithm 2) in a
//! build with the feature `pq` and X25519 (algorithm 1) in a build without it, like every other
//! wrap. The three parts do not depend on each other, so breaking X25519 gives the first part
//! only; and none depends on `K_seal` or `K_chunk`, so a holder of those keys (the content
//! crates) cannot open the chain. A link is signed by the wrapping device and its context is
//! `chain/<space>/epoch/<n>`.
//!
//! [`chain_open`] walks down from the newest key, at most [`CHAIN_MAX`] epochs, checks every
//! link's signature through the directory, and stops at the first link that is missing, doubled,
//! tampered with, of another space, or sealed to another epoch.
//!
//! # History
//!
//! [`History::Full`]: a joiner is given the newest key and reads every older epoch through the
//! chain. [`History::Forward`]: the key is rotated when a member is added and no chain wrap is
//! written (DRIFT X-DELTA-W1S15C1-2), so the joiner holds the new epoch only. [`join_grant`] says
//! which epochs a joiner will be able to read.
//!
//! A key can be moved and read but not copied by accident (the first example is the control: the
//! same call compiles for a plain type):
//!
//! ```
//! use azlin_keys::space::JoinGrant;
//! fn needs_clone<T: Clone>() {}
//! needs_clone::<JoinGrant>();
//! ```
//!
//! ```compile_fail
//! use azlin_keys::space::SpaceKey;
//! fn needs_clone<T: Clone>() {}
//! needs_clone::<SpaceKey>();
//! ```
//!
//! ```compile_fail
//! use azlin_keys::space::SpaceKey;
//! fn needs_copy<T: Copy>() {}
//! needs_copy::<SpaceKey>();
//! ```

use core::fmt;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use zeroize::{Zeroize, Zeroizing};

use crate::derive::{derive_public, CHAIN_WRAP, K_CHUNK, K_SEAL, SPACE_KEY_ID};
use crate::envelope::{open, Ctx, CtxKind, Envelope, Recipient, RecipientSecret};
use crate::group::{open_group_key, GroupKey, GroupPublic};
use crate::ids::{DeviceId, Epoch, GroupId, KeyId, SpaceId};
use crate::member::{KeyBundle, PublicBundle};
pub use crate::member::{KeyError, LinkFault};
use crate::seam::Entropy;
use crate::secret::Secret;
use crate::sign::{DeviceDirectory, SignKeypair, VerifyAt};

/// The most epochs one walk of the chain reaches below the newest key. A longer history is read
/// in segments: the oldest key of one walk is the newest key of the next. DRIFT
/// X-DELTA-W1S15C1-4 (P-4: the plan gives no bound).
///
/// Caller: `chain_open`, `join_grant`, S15c2 (`MAX_BUNDLE_ROWS` planning), INT-S15-1.
pub const CHAIN_MAX: u32 = 4096;

// ---------------------------------------------------------------------------------------------
// History modes
// ---------------------------------------------------------------------------------------------

/// `reg_space.history` (planning/architecture.md section 5.4): what a member who joins a space can
/// read of the epochs before the one they join.
///
/// Caller: S15c2 (a rekey writes chain wraps for `Full` spaces only), S16 (the `space_history`
/// chain entry), S9 (the registry column).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum History {
    /// `full`: joiners get the epoch chain and read every older epoch.
    Full,
    /// `forward`: the key is rotated when a member is added and joiners get no chain.
    Forward,
}

impl History {
    /// The word of the registry column: `full` or `forward`.
    ///
    /// Caller: S9 and S16 (the `history` column and the `space_history` chain entry).
    pub const fn word(self) -> &'static str {
        match self {
            History::Full => "full",
            History::Forward => "forward",
        }
    }

    /// The mode of this word; `None` for every other text (strict: no case folding).
    ///
    /// Caller: S9 and S16 (reading the `history` column).
    pub fn from_word(word: &str) -> Option<History> {
        match word {
            "full" => Some(History::Full),
            "forward" => Some(History::Forward),
            _ => None,
        }
    }
}

/// The epochs a joiner will be able to read: newest first.
///
/// Caller: INT-S15-1 (what a new member is shown), S15c2 (what a join grants).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinGrant {
    /// The epochs, newest first: the newest key itself and, for a `full` space whose chain is
    /// available, the older epochs the chain opens (at most [`CHAIN_MAX`] below the newest).
    pub epochs: Vec<Epoch>,
}

/// What a joiner is given. `newest` is the epoch the joiner is wrapped (for a `forward` space the
/// epoch the rotation on add made). `Full` with the chain available: the newest key and every
/// older epoch the chain reaches, at most [`CHAIN_MAX`] below it. `Full` without the chain (a link
/// is missing or was pruned) and `Forward`: only the newest epoch.
///
/// Caller: INT-S15-1 (the join flow), S15c2 (a join plan), S16 (`space_history`).
pub fn join_grant(history: History, newest: Epoch, chain_available: bool) -> JoinGrant {
    let mut epochs = vec![newest];
    if matches!(history, History::Full) && chain_available {
        let mut epoch = newest;
        for _ in 0..CHAIN_MAX {
            match epoch.prev() {
                Some(older) => {
                    epochs.push(older);
                    epoch = older;
                }
                None => break,
            }
        }
    }
    JoinGrant { epochs }
}

// ---------------------------------------------------------------------------------------------
// The space key of one epoch
// ---------------------------------------------------------------------------------------------

/// Whether a rotation keeps the dedup seed. A removal rotation CARRIES it (deduplication survives
/// the removal: otherwise the next edit of every file would upload in full); an explicit re-key of
/// the space makes it FRESH.
///
/// Caller: S15c2 (`plan_member_removed_from_group` and `plan_space_rekey` take it), INT-S15-1
/// ("re-key space" is the user's explicit action).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DedupChoice {
    /// Keep the previous epoch's dedup seed and dedup epoch: `K_chunk` stays the same.
    Carry,
    /// Draw a new dedup seed; the dedup epoch is the new epoch: `K_chunk` changes.
    Fresh,
}

/// The dedup seed of an epoch and the epoch it was drawn at.
struct DedupSeed {
    epoch: Epoch,
    seed: Secret<32>,
}

/// The material of ONE space epoch: a seal seed, a dedup seed and the epoch the dedup seed was
/// drawn at (P-1). `K_seal` and `K_chunk` are derived from the two seeds. The seeds are
/// [`Secret`]s (wiped on drop, never printed); the type is not `Clone`, not `Copy`, and its
/// `Debug` shows no byte. A second owner is made only by [`SpaceKey::duplicate`].
///
/// The content crates never see a space key: they are handed `K_seal` and `K_chunk`.
///
/// Caller: INT-S15-1 (azul-storage's key flows), S15c2 (rekey plans and the keyring), S14 lanes
/// (`k_seal` for pack trailers and pointers, `k_chunk` for chunk ids and keys), S17 (a remaining
/// manager's device or the key agent rotates).
pub struct SpaceKey {
    /// The epoch of the key, from 1: the first key of a space is epoch 1 and every rotation is
    /// one more. Read it; never change it.
    pub epoch: Epoch,
    space: SpaceId,
    seal_seed: Secret<32>,
    dedup: DedupSeed,
}

/// The rule for a new epoch: only the first epoch has no predecessor, and every other epoch
/// follows the key of the same space by exactly one.
fn check_succession(
    space: &SpaceId,
    epoch: Epoch,
    prev: Option<&SpaceKey>,
) -> Result<(), KeyError> {
    match prev {
        None if epoch == Epoch::FIRST => Ok(()),
        None => Err(KeyError::MissingPrevious),
        Some(p) if p.space != *space => Err(KeyError::WrongSpace),
        Some(p) if p.epoch.next() == Some(epoch) => Ok(()),
        Some(_) => Err(KeyError::NotNextEpoch),
    }
}

impl SpaceKey {
    /// The key of epoch `epoch` of `space`, with a fresh `seal_seed` and the dedup seed chosen by
    /// `dedup`. The entropy seam is drawn from in this order: 32 bytes for the seal seed and,
    /// unless the dedup seed is carried, 32 bytes for the dedup seed (so 32 bytes for a `Carry`,
    /// 64 for a `Fresh`). A failed draw is [`KeyError::Entropy`] and no key exists.
    ///
    /// `prev` is the key of the epoch before. The first epoch of a space has none and takes
    /// `Fresh`; every later epoch must follow `prev` by exactly one in the same space
    /// ([`KeyError::NotNextEpoch`], [`KeyError::WrongSpace`]) and a `Carry` needs it
    /// ([`KeyError::MissingPrevious`]). The seal seed is drawn whatever `prev` holds: it is a
    /// function of the entropy alone (check C5), which is why a removed member cannot derive the
    /// next epoch. All of this is refused BEFORE anything is drawn. DRIFT X-DELTA-W1S15C1-8.
    ///
    /// Caller: S15c2 (`execute` of a rekey: a removal `Carry`s, a re-key `Fresh`es), INT-S15-1
    /// (`space_create` makes epoch 1).
    pub fn generate(
        space: &SpaceId,
        epoch: Epoch,
        dedup: DedupChoice,
        prev: Option<&SpaceKey>,
        ent: &mut dyn Entropy,
    ) -> Result<SpaceKey, KeyError> {
        check_succession(space, epoch, prev)?;
        // DRIFT X-DELTA-W1S15C1-1: the material of an epoch is two seeds (P-1).
        let carried = match (dedup, prev) {
            (DedupChoice::Carry, Some(p)) => Some(DedupSeed {
                epoch: p.dedup.epoch,
                seed: p.dedup.seed.duplicate(),
            }),
            (DedupChoice::Carry, None) => return Err(KeyError::MissingPrevious),
            (DedupChoice::Fresh, _) => None,
        };
        let seal_seed = Secret::<32>::generate(ent)?;
        let dedup = match carried {
            Some(seed) => seed,
            None => DedupSeed {
                epoch,
                seed: Secret::<32>::generate(ent)?,
            },
        };
        Ok(SpaceKey {
            epoch,
            space: *space,
            seal_seed,
            dedup,
        })
    }

    /// The key of these 68 bytes for `space` and `epoch`, the bytes [`SpaceKey::to_plaintext`]
    /// makes. Any other length, a dedup epoch of 0 and a dedup epoch above `epoch` are
    /// [`KeyError::BadPlaintext`].
    ///
    /// Caller: `open_space_key`, `open_space_key_with_group` and `chain_open_link` (this module),
    /// INT-S15-1 (a stored key is loaded).
    pub fn from_plaintext(space: &SpaceId, epoch: Epoch, p: &[u8]) -> Result<SpaceKey, KeyError> {
        let (seal, rest) = p.split_first_chunk::<32>().ok_or(KeyError::BadPlaintext)?;
        let (dedup, rest) = rest
            .split_first_chunk::<32>()
            .ok_or(KeyError::BadPlaintext)?;
        let (dedup_epoch, rest) = rest
            .split_first_chunk::<4>()
            .ok_or(KeyError::BadPlaintext)?;
        if !rest.is_empty() {
            return Err(KeyError::BadPlaintext);
        }
        let dedup_epoch =
            Epoch::new(u32::from_be_bytes(*dedup_epoch)).ok_or(KeyError::BadPlaintext)?;
        if dedup_epoch > epoch {
            return Err(KeyError::BadPlaintext);
        }
        Ok(SpaceKey {
            epoch,
            space: *space,
            seal_seed: Secret::from_bytes(*seal),
            dedup: DedupSeed {
                epoch: dedup_epoch,
                seed: Secret::from_bytes(*dedup),
            },
        })
    }

    /// The 68 bytes a wrap of this key carries: `seal_seed || dedup_seed || dedup_epoch` (the
    /// dedup epoch as four bytes, big-endian). A [`Secret`], so it is wiped when it is dropped.
    ///
    /// Caller: `wrap_space_key` and `chain_wrap` (this module), INT-S15-1 (the key store).
    pub fn to_plaintext(&self) -> Secret<68> {
        let mut out = Secret::<68>::from_bytes([0u8; 68]);
        let dedup_epoch = self.dedup.epoch.get().to_be_bytes();
        let bytes = self
            .seal_seed
            .expose()
            .iter()
            .chain(self.dedup.seed.expose().iter())
            .chain(dedup_epoch.iter());
        for (dst, src) in out.expose_mut().iter_mut().zip(bytes) {
            *dst = *src;
        }
        out
    }

    /// `K_seal`: the key of the repository, the pack trailers and the pointers of this epoch:
    /// `derive_key(K_SEAL, seal_seed)`. It changes with every epoch.
    ///
    /// Caller: S14c (pack trailers), S14d (pointers), S16 (request keys derive from it).
    pub fn k_seal(&self) -> Secret<32> {
        crate::derive::derive(&K_SEAL, &self.seal_seed)
    }

    /// `K_chunk`: the key of chunk ids and chunk keys: `derive_key(K_CHUNK, dedup_seed)`. It is
    /// the same in every epoch that carries the dedup seed and changes only when the space is
    /// explicitly re-keyed.
    ///
    /// Caller: S14b (`KChunk::from_secret`: the content crates take this key and never see the
    /// space key).
    pub fn k_chunk(&self) -> Secret<32> {
        crate::derive::derive(&K_CHUNK, &self.dedup.seed)
    }

    /// The id a signed `space_create` or `space_rekey` chain entry commits to: `derive_key("azlin
    /// 2026-11 space key id v1", space || epoch || plaintext)` with the epoch as four bytes,
    /// big-endian. A one-way commitment to the whole material of the epoch, safe to publish.
    ///
    /// Caller: S15c2 (`authorize_epoch` compares it with the chain's commitment; the rekey plan
    /// drafts the entry), S16 (the chain entries).
    pub fn key_id(&self) -> KeyId {
        let plaintext = self.to_plaintext();
        let mut material = Zeroizing::new(Vec::with_capacity(16 + 4 + 68));
        material.extend_from_slice(self.space.as_bytes());
        material.extend_from_slice(&self.epoch.get().to_be_bytes());
        material.extend_from_slice(plaintext.expose());
        KeyId::from_bytes(derive_public(&SPACE_KEY_ID, &material))
    }

    /// The space this key belongs to.
    ///
    /// Caller: S15c2 (a keyring and a plan check that a key is the space's), INT-S15-1.
    pub fn space(&self) -> &SpaceId {
        &self.space
    }

    /// The epoch the dedup seed was drawn at: the epoch of the last explicit re-key (or the first
    /// epoch). Equal in every epoch that carries the same dedup seed.
    ///
    /// Caller: S14b (a chunk records which dedup epoch's `K_chunk` made it), S15c2, INT-S15-1.
    pub fn dedup_epoch(&self) -> Epoch {
        self.dedup.epoch
    }

    /// What opens a chain link sealed to THIS epoch: the secret side of [`SpaceKey::chain_recipient`],
    /// derived from the seal seed alone (DRIFT X-DELTA-W1S15C1-5): the BLAKE3 `derive_key` stream
    /// under the context `CHAIN_WRAP`, its first 32 bytes the X25519 secret and the next 64 the
    /// ML-KEM seed. The hasher and the stream are wiped.
    fn chain_secret(&self) -> RecipientSecret {
        let mut hasher = blake3::Hasher::new_derive_key(CHAIN_WRAP.as_str());
        hasher.update(self.seal_seed.expose());
        let mut stream = hasher.finalize_xof();
        let mut kx = Secret::<32>::from_bytes([0u8; 32]);
        let mut pq_seed = Secret::<64>::from_bytes([0u8; 64]);
        stream.fill(kx.expose_mut());
        stream.fill(pq_seed.expose_mut());
        hasher.zeroize();
        stream.zeroize();
        RecipientSecret::new(&kx, Some(&pq_seed))
    }

    /// The public side a chain link to this epoch is wrapped to: hybrid in a build with the
    /// feature `pq`, X25519 otherwise. Derived on demand from the seal seed; nothing of it is
    /// stored in the registry (the `rcpt` of the link names it).
    ///
    /// Seam: `chain_wrap` (this module) uses it; the tests and S15c2's registry tools read it to
    /// see what a chain row's recipient is.
    pub fn chain_recipient(&self) -> Recipient {
        self.chain_secret().recipient()
    }

    /// A second owner of the same key: the one way to copy a space key.
    ///
    /// Seam: INT-S15-1's key store and S15c2's keyring and plans, which hold a key beside the
    /// caller's.
    pub fn duplicate(&self) -> SpaceKey {
        SpaceKey {
            epoch: self.epoch,
            space: self.space,
            seal_seed: self.seal_seed.duplicate(),
            dedup: DedupSeed {
                epoch: self.dedup.epoch,
                seed: self.dedup.seed.duplicate(),
            },
        }
    }
}

/// Prints the space, the epoch, the dedup epoch and `***`: no byte of the key.
impl fmt::Debug for SpaceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SpaceKey({}, epoch {}, dedup epoch {}, ***)",
            self.space, self.epoch, self.dedup.epoch
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Wraps of a space key to a member or a group
// ---------------------------------------------------------------------------------------------

/// Whom a space key is wrapped to: a member (their public bundle: hybrid when it has an ML-KEM
/// key) or a group (the public half of its key epoch: X25519). Spaces granted to a group wrap
/// their key to the group, so a removal costs members + spaces wraps (section 5.6).
///
/// Caller: S15c2 (the recipients of a rekey: `plan_space_rekey` takes a list of them), INT-S15-1.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WrapTarget {
    /// A member: wrapped to their member key.
    Member(PublicBundle),
    /// A group: wrapped to the group key of one epoch.
    Group(GroupPublic),
}

impl WrapTarget {
    /// The recipient the wrap is made to.
    ///
    /// Caller: `wrap_space_key` (this module), INT-S15-1.
    pub fn recipient(&self) -> Recipient {
        match self {
            WrapTarget::Member(bundle) => bundle.recipient(),
            WrapTarget::Group(group) => Recipient::X25519(group.recipient()),
        }
    }

    /// The `rcpt` of an envelope wrapped to this target: the "recipient key id" of the
    /// `reg_wrap` row.
    ///
    /// Caller: S15c2 (`reg_wrap` rows of a rekey), S9.
    pub fn key_id(&self) -> KeyId {
        self.recipient().key_id()
    }
}

/// Wraps the space key `k` to `to` as an envelope for the context `space/<space id>/epoch/<n>`,
/// signed by the device `signer` with `sk`. `space` must be the key's own space
/// ([`KeyError::WrongSpace`] otherwise). A member target gets algorithm 2 when it has an ML-KEM
/// key and algorithm 1 otherwise; a group target gets algorithm 1.
///
/// The envelope layer draws, in this order: the ephemeral X25519 secret, for a hybrid wrap the
/// ML-KEM message, and last the nonce; a failed draw returns the error and NO envelope.
///
/// Caller: S15c2 (`execute` of a rekey), INT-S15-1 (a grant, a join, `space_create`).
pub fn wrap_space_key(
    k: &SpaceKey,
    space: &SpaceId,
    to: &WrapTarget,
    signer: &DeviceId,
    sk: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, KeyError> {
    if k.space != *space {
        return Err(KeyError::WrongSpace);
    }
    let plaintext = k.to_plaintext();
    let ctx = Ctx::space_epoch(space, k.epoch);
    to.recipient()
        .wrap(&ctx, plaintext.expose(), signer, sk, ent)
        .map_err(KeyError::from)
}

/// Opens the wrap of the space key of `epoch` with a member's keys: the envelope must be for the
/// context `space/<space id>/epoch/<epoch>` and for this member, and its signer must verify
/// through the directory at `at`.
///
/// Caller: INT-S15-1 (a member reads a space), S15c2 (a keyring adopts what a member opens).
pub fn open_space_key(
    env: &Envelope,
    space: &SpaceId,
    epoch: Epoch,
    member: &KeyBundle,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<SpaceKey, KeyError> {
    let plaintext = open(
        env,
        &member.recipient_secret(),
        &Ctx::space_epoch(space, epoch),
        dir,
        at,
    )?;
    SpaceKey::from_plaintext(space, epoch, &plaintext)
}

/// Opens the wrap of a space key that was made to a GROUP, with the group key (P-3: a group key
/// is an X25519 key pair, so a space key wrapped to a group opens with the same sealed-box code
/// as one wrapped to a member).
///
/// Caller: `open_space_key_via_group` (this module), S15c2 (a manager who holds the group key).
pub fn open_space_key_with_group(
    env: &Envelope,
    space: &SpaceId,
    epoch: Epoch,
    group_key: &GroupKey,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<SpaceKey, KeyError> {
    let plaintext = open(
        env,
        &group_key.recipient_secret(),
        &Ctx::space_epoch(space, epoch),
        dir,
        at,
    )?;
    SpaceKey::from_plaintext(space, epoch, &plaintext)
}

/// The group key a space key was wrapped to, as the member reaches it: the group, the epoch of its
/// key, and the member's own wrap of that group key.
///
/// Caller: INT-S15-1 (a member reads a space granted to a group), `open_space_key_via_group`.
#[derive(Clone, Copy, Debug)]
pub struct GroupRoute<'a> {
    /// The group the space is granted to.
    pub group: &'a GroupId,
    /// The epoch of the group key the space key was wrapped to.
    pub group_epoch: Epoch,
    /// The member's wrap of that group key (the `reg_wrap` row of kind `group_key`).
    pub group_wrap: &'a Envelope,
}

/// The whole path of section 5.5's diagram for a space granted to a group: the member's key opens
/// the group key of epoch g, and the group key opens the space key of epoch n. A member who is not
/// in the group cannot open the group key (`NotForMe`); a group key of another epoch is refused
/// as `CtxMismatch`; every envelope's signer is verified through the directory at `at`.
///
/// Caller: INT-S15-1 (a member reads a space granted to a group).
pub fn open_space_key_via_group(
    space_wrap: &Envelope,
    space: &SpaceId,
    epoch: Epoch,
    route: &GroupRoute<'_>,
    member: &KeyBundle,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<SpaceKey, KeyError> {
    let group_key = open_group_key(
        route.group_wrap,
        route.group,
        route.group_epoch,
        member,
        dir,
        at,
    )?;
    open_space_key_with_group(space_wrap, space, epoch, &group_key, dir, at)
}

// ---------------------------------------------------------------------------------------------
// The epoch chain
// ---------------------------------------------------------------------------------------------

/// Whether `newer`'s dedup material can follow from `older`'s by a rotation: either the dedup
/// seed was replaced (the newer dedup epoch is the newer epoch itself) or it was carried (the same
/// dedup epoch and the same seed). Every pair that `SpaceKey::generate` makes satisfies it.
fn dedup_is_consistent(older: &SpaceKey, newer: &SpaceKey) -> bool {
    if newer.dedup.epoch == newer.epoch {
        return true;
    }
    newer.dedup.epoch == older.dedup.epoch && newer.dedup.seed.ct_eq(&older.dedup.seed)
}

/// The checks of a pair of keys that are to be linked: both are the space's, `newer` follows
/// `older` by one epoch, and their dedup material is consistent. DRIFT X-DELTA-W1S15C1-9.
fn check_link_pair(older: &SpaceKey, newer: &SpaceKey, space: &SpaceId) -> Result<(), KeyError> {
    if older.space != *space || newer.space != *space {
        return Err(KeyError::WrongSpace);
    }
    if older.epoch.next() != Some(newer.epoch) {
        return Err(KeyError::NotNextEpoch);
    }
    if !dedup_is_consistent(older, newer) {
        return Err(KeyError::ChainLink {
            epoch: older.epoch,
            fault: LinkFault::Inconsistent,
        });
    }
    Ok(())
}

/// Seals epoch `older` under epoch `newer`: the chain link, an envelope for the context
/// `chain/<space>/epoch/<n>` (n the older epoch) wrapped to [`SpaceKey::chain_recipient`] of
/// `newer` and signed by the device `signer` with `key`. The holder of the newest key then reads
/// every older epoch ([`chain_open`]).
///
/// `newer` must follow `older` by exactly one epoch in the same space and their dedup material
/// must fit a rotation ([`KeyError::NotNextEpoch`], [`KeyError::WrongSpace`],
/// [`LinkFault::Inconsistent`]); all refused before anything is drawn. The envelope layer draws,
/// in this order: the ephemeral X25519 secret, for a hybrid link the ML-KEM message, and last the
/// nonce; a failed draw returns the error and NO envelope.
///
/// A `forward` space writes no chain link (DRIFT X-DELTA-W1S15C1-2): knowing that is the caller's
/// (S15c2's rekey bundle), the link itself does not know the space's history mode.
///
/// Caller: S15c2 (`execute` of a rekey of a `Full` space), INT-S15-1.
pub fn chain_wrap(
    older: &SpaceKey,
    newer: &SpaceKey,
    space: &SpaceId,
    signer: &DeviceId,
    key: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, KeyError> {
    check_link_pair(older, newer, space)?;
    let plaintext = older.to_plaintext();
    let ctx = Ctx::chain(space, older.epoch);
    newer
        .chain_recipient()
        .wrap(&ctx, plaintext.expose(), signer, key, ent)
        .map_err(KeyError::from)
}

/// Opens ONE chain link with the key of the epoch above it: the older epoch's key. The link must
/// be for the context `chain/<space>/epoch/<n>` of the epoch below `newer`, for `newer`'s chain
/// recipient, and its signer must verify through the directory at `at`; the opened key must be a
/// space key of that epoch that fits a rotation into `newer`. Every failure names the link's
/// epoch ([`KeyError::ChainLink`]).
///
/// Caller: `chain_open` (this module), S15c2 (`authorize_epoch` of an epoch read through the
/// chain), INT-S15-1.
pub fn chain_open_link(
    newer: &SpaceKey,
    link: &Envelope,
    space: &SpaceId,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<SpaceKey, KeyError> {
    if newer.space != *space {
        return Err(KeyError::WrongSpace);
    }
    let epoch = newer.epoch.prev().ok_or(KeyError::NotNextEpoch)?;
    let fault = |fault: LinkFault| KeyError::ChainLink { epoch, fault };
    let plaintext = open(
        link,
        &newer.chain_secret(),
        &Ctx::chain(space, epoch),
        dir,
        at,
    )
    .map_err(|e| fault(LinkFault::Open(e)))?;
    let older = SpaceKey::from_plaintext(space, epoch, &plaintext)
        .map_err(|_| fault(LinkFault::Plaintext))?;
    if !dedup_is_consistent(&older, newer) {
        return Err(fault(LinkFault::Inconsistent));
    }
    Ok(older)
}

/// The chain links of `space` among `wraps` whose epoch is in `lowest..below`, by epoch. Everything
/// else (other kinds, other spaces, other epochs) is ignored. Two different envelopes for one
/// epoch are [`LinkFault::Duplicate`]; an identical copy counts once. At most `below - lowest`
/// entries are kept, so the memory is bounded by the walk's own limit.
fn index_links<'a>(
    wraps: &'a [Envelope],
    space: &SpaceId,
    lowest: Epoch,
    below: Epoch,
) -> Result<BTreeMap<Epoch, &'a Envelope>, KeyError> {
    let mut links: BTreeMap<Epoch, &'a Envelope> = BTreeMap::new();
    for env in wraps {
        if env.ctx.kind() != CtxKind::Chain || env.ctx.id_bytes() != space.as_bytes() {
            continue;
        }
        let Some(epoch) = env.ctx.epoch() else {
            continue;
        };
        if epoch < lowest || epoch >= below {
            continue;
        }
        match links.entry(epoch) {
            Entry::Vacant(slot) => {
                slot.insert(env);
            }
            Entry::Occupied(slot) => {
                if **slot.get() != *env {
                    return Err(KeyError::ChainLink {
                        epoch,
                        fault: LinkFault::Duplicate,
                    });
                }
            }
        }
    }
    Ok(links)
}

/// The older epochs of `space`, opened from the newest key `newer`: epoch n-1, n-2, ... down to
/// `upto`, in that order (empty when `upto` is `newer`'s own epoch). The links are looked up in
/// `wraps` by their context, so the order of `wraps` does not matter; anything in it that is not a
/// link of this space between `upto` and `newer` is ignored.
///
/// The walk is bounded: more than [`CHAIN_MAX`] epochs is [`KeyError::ChainTooLong`] and an
/// `upto` above `newer`'s epoch is [`KeyError::UptoAboveNewest`], both before any link is opened.
/// Every link is opened by [`chain_open_link`] (context, recipient, the signer's signature
/// through `dir` at `at`, the cipher), and the walk stops at the first link that is missing,
/// doubled or refused with a [`KeyError::ChainLink`] that names its epoch. It never goes up: no
/// epoch above `newer` is read, and the key of epoch n cannot open n+1.
///
/// One `at` serves every link (DRIFT X-DELTA-W1S15C1-7): a link written by a device that has been
/// removed since fails `VerifyAt::Now` as `SignerRevoked`, so a reader asks at the position of the
/// chain in which the link was written, or walks in segments.
///
/// Caller: INT-S15-1 (a member reads the history of a `Full` space), S15c2 (a joiner's epochs are
/// authorized one by one).
pub fn chain_open(
    newer: &SpaceKey,
    wraps: &[Envelope],
    space: &SpaceId,
    upto: Epoch,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<Vec<SpaceKey>, KeyError> {
    if newer.space != *space {
        return Err(KeyError::WrongSpace);
    }
    let steps = newer
        .epoch
        .get()
        .checked_sub(upto.get())
        .ok_or(KeyError::UptoAboveNewest)?;
    if steps > CHAIN_MAX {
        return Err(KeyError::ChainTooLong);
    }
    let links = index_links(wraps, space, upto, newer.epoch)?;
    let mut opened: Vec<SpaceKey> = Vec::new();
    for _ in 0..steps {
        let current = opened.last().unwrap_or(newer);
        let epoch = current.epoch.prev().ok_or(KeyError::NotNextEpoch)?;
        let link = links.get(&epoch).ok_or(KeyError::ChainLink {
            epoch,
            fault: LinkFault::Missing,
        })?;
        let older = chain_open_link(current, link, space, dir, at)?;
        opened.push(older);
    }
    Ok(opened)
}

// ---------------------------------------------------------------------------------------------
// What a removal costs
// ---------------------------------------------------------------------------------------------

/// The number of key wraps a removal costs (planning/architecture.md section 5.6): one new group
/// key epoch wrapped to each of the `remaining` members, plus one new epoch for each of the
/// `spaces` of the group wrapped to the new group key: `remaining + spaces`, never `remaining *
/// spaces`. `None` when the sum does not fit. The chain wraps of `Full` spaces (at most one per
/// space) are further rows and are not counted here (DRIFT X-DELTA-W1S15C1-14).
///
/// Caller: S15c2 (the rekey planner sizes its output and its limit check with it).
pub fn removal_wrap_count(remaining: usize, spaces: usize) -> Option<usize> {
    remaining.checked_add(spaces)
}
