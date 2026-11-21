//! The vocabulary of a signed hash chain: which chain, the entry as the chain machinery sees it, the
//! head of a chain and its signed form, the verifier of a signed head, and the alarms a device
//! raises (planning/architecture.md section 5.9 "The team log and the space chains", section
//! 5.13 "A head never goes backwards", section 6.10 V3 "a restore epoch the metadata group signs";
//! build unit S17, lane W1-S17a).
//!
//! The chain checks take the entry's hash AS THE ENTRY SAYS IT (`ChainEntry::hash`): the byte-exact
//! canonical encoding of a team-log or space-chain entry and the preimage of its signature belong to
//! azlin-cbor and azlin-contract (a later glue unit, CHAIN-GLUE, implements `ChainEntry` for them
//! and runs the golden vectors). This crate decides what a chain is, not how an entry is encoded.
//!
//! THE RESTORE EPOCH (the coordinator's decision on X-DELTA-W1S17A-3): a restore moves positions back,
//! so a position alone cannot tell the lost timeline from the restored one. Every signed head AND
//! every entry carries the [`RestoreEpoch`] it was written in, covered by its signature: the group
//! writes epoch 0 until the first restore and the restore's epoch (one above the last) afterwards.
//! A device that accepted a restore refuses a head or an entry of an older epoch (`HeadRefusal`,
//! `ChainRefusal::BelowRestoreEpoch`): the replay of the lost timeline is no way back.

use core::fmt;

use azlin_seams::{AccountId, ChainSeq, Hash32, KeyId, RestoreEpoch, SpaceId, Stamp, VerifyError};

/// The two kinds of chain: the account's team log and a space's membership chain.
///
/// Implements planning/architecture.md section 5.9 (the team log carries account-level ops only;
/// each space has its own short signed membership chain).
///
/// Caller: `ChainId`; S17c (`TeamLog`), S16b (`Space`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChainKind {
    /// The account's team log: one chain per account.
    TeamLog,
    /// A space chain: one chain per space.
    Space,
}

impl ChainKind {
    /// The kind as a word: `team_log` or `space`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ChainKind::TeamLog => "team_log",
            ChainKind::Space => "space",
        }
    }
}

/// Which chain: its kind and a 16-byte id (the account id of a team log, the space id of a space
/// chain). Ordered and hashable, so it keys a device's pins.
///
/// Implements planning/architecture.md section 5.9 (a chain per account and per space).
///
/// Caller: every `ChainEntry` (S17c `RawEntry`, S16b space entries), `Head`, `HeadPin`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainId {
    /// The kind of chain.
    pub kind: ChainKind,
    /// The id of the account or the space.
    pub id: [u8; 16],
}

impl ChainId {
    /// The team log of an account.
    pub const fn team_log(account: AccountId) -> ChainId {
        ChainId {
            kind: ChainKind::TeamLog,
            id: *account.as_bytes(),
        }
    }

    /// The membership chain of a space.
    pub const fn space(space: SpaceId) -> ChainId {
        ChainId {
            kind: ChainKind::Space,
            id: *space.as_bytes(),
        }
    }
}

/// `team_log:` or `space:` and 32 lowercase hex digits.
impl fmt::Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.kind.as_str())?;
        for byte in &self.id {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChainId({self})")
    }
}

/// One entry of a chain, as the chain machinery sees it: where it belongs, which position it
/// claims, the hash of the entry before it, its own hash, the leader's stamp, and the restore epoch
/// it was written in. The entry's encoding, its signatures and its meaning are its own type's.
///
/// `epoch` is the restore epoch the entry belongs to: 0 until the first restore, afterwards the epoch
/// of the restore the group had applied when it wrote the entry (the coordinator's decision on
/// X-DELTA-W1S17A-3: every entry written after a restore names its epoch, covered by the entry's
/// signature). Along a chain the epoch never goes down, and a device that accepted a restore refuses
/// an entry of an older epoch.
///
/// `is_intact` is the one question the chain cannot answer from the other fields: whether THIS COPY
/// of the entry still is what its hash says (its content hashes to `hash()`, and any signature the
/// type can check without outside state verifies). A copy that is not intact is `Corrupt`: it is
/// fetched again from another voter, and it is never evidence of a fork. A signature that needs
/// state (which device was allowed to sign at this position) is checked by the owner of the type
/// before the entry reaches the chain. DRIFT X-DELTA-W1S17A-2: the brief's trait has no such
/// method, yet its `ChainRefusal::Corrupt` is raised by `admit`; this is the provisional reading.
///
/// Implements planning/architecture.md section 5.9 ("an entry that fails its hash or signature
/// check is fetched again from another voter") and section 6.10 V3 (a restore epoch the metadata
/// group signs).
///
/// Seam: CHAIN-GLUE implements it for the team-log entry (S17c `RawEntry`) and the space-chain
/// entry (S16b); the tests of this crate use a small entry that hashes its fields with BLAKE3.
pub trait ChainEntry {
    /// The chain the entry belongs to.
    fn chain(&self) -> ChainId;

    /// The position the entry claims.
    fn seq(&self) -> ChainSeq;

    /// The hash of the entry before this one; `Hash32::ZERO` for a genesis entry.
    fn prev(&self) -> Hash32;

    /// The hash of this entry, as the entry says it.
    fn hash(&self) -> Hash32;

    /// The leader's stamp of the entry (planning/architecture.md section 2.4).
    fn at(&self) -> Stamp;

    /// The restore epoch the entry was written in.
    fn epoch(&self) -> RestoreEpoch;

    /// Whether this copy is intact: its content hashes to `hash()` and its self-checkable
    /// signatures verify.
    fn is_intact(&self) -> bool;
}

/// The head of a chain: the position and hash of its newest entry.
///
/// Implements planning/architecture.md section 5.9 (the head a device pins).
///
/// Caller: `Tip`, `Chain`, `HeadPin`, `RestoreRecord` (the positions that survived a restore).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Head {
    /// The chain.
    pub chain: ChainId,
    /// The position of the newest entry.
    pub seq: ChainSeq,
    /// The hash of the newest entry.
    pub hash: Hash32,
}

/// A head the metadata group signed: what `team_log_get` returns. Its AGE is not judged here
/// (that is `azlin_keys::FreshHead`'s, over the plain facts `seq` and `signed_at`): this type owns
/// the head's content and its signature, and it is the ONLY head type of the engine (COMMON-W1R
/// R12). The bytes the signature covers are the signer's and the verifier's: a `HeadVerifier`
/// builds the preimage with the engine's codec and `signing_input`, and the preimage covers EVERY
/// field below, `epoch` included.
///
/// `epoch` is the restore epoch the group was in when it signed the head (X-DELTA-W1S17A-3): a head
/// signed before a restore keeps its old epoch for ever, so once a device has accepted the restore
/// the lost timeline's heads, however validly signed, are refused (`HeadPin::offer`).
///
/// Implements planning/architecture.md section 5.9 ("a device wraps a key only after it holds a head
/// the metadata group signed at most 10 minutes before"), section 5.13 (a head never goes
/// backwards) and section 6.10 V3 (a restore epoch the metadata group signs).
///
/// Caller: `HeadPin::offer`, `compare_peer`, `Checkpoint`; CHAIN-GLUE (`HeadSource` of S15c2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedHead {
    /// The head.
    pub head: Head,
    /// The restore epoch the group was in when it signed the head.
    pub epoch: RestoreEpoch,
    /// The leader's stamp at which the group signed it.
    pub signed_at: Stamp,
    /// The key that signed it (the group's credential key, certified by the identity key).
    pub signer: KeyId,
    /// The signature bytes.
    pub sig: Vec<u8>,
}

/// The seam that checks a signed head: it knows the group's credential key (certified by the
/// identity key, pinned by the map anchors), builds the preimage of the head with the engine's
/// codec (every field of `SignedHead` but `sig`, the epoch included), and verifies `sig`. Every
/// failure is a `VerifyError`; an unsupported algorithm is an error, never a pass.
///
/// Implements planning/architecture.md section 5.9 and section 2.4 (the group's identity key
/// certifies a credential key).
///
/// Seam: CHAIN-GLUE (over the map anchors and `azlin_seams::SignatureVerifier`); the tests of this
/// crate use a verifier over `FakeScheme`.
pub trait HeadVerifier {
    /// `Ok(())` when `h` is a head the group signed; otherwise why not.
    fn verify_head(&self, h: &SignedHead) -> Result<(), VerifyError>;
}

/// What a device raises: something a server or a peer did that no honest party does. An alarm is
/// data for the device's own surfaces (a notice, a log, a refusal to wrap); this crate raises none
/// by itself.
///
/// Implements planning/architecture.md section 5.9 ("two different entries at the same position
/// are a fork, refused and alarmed"; "devices of one account exchange the heads they have seen"),
/// section 5.13 ("a head never goes backwards") and section 6.10 V3 (a restore epoch the metadata
/// group signs; an older head is refused).
///
/// Caller: `PinVerdict::{Fork, Refused}`, `PeerVerdict::{PeerAhead, Fork}`, `ChainRefusal::alarm`,
/// `HeadRefusal::alarm`, `rollback_alarm`, `require_signed`; S16a's head guard (`Alarm`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum Alarm {
    /// Two different entries at one position of one chain.
    #[error("fork of {chain} at position {seq}: held {have}, offered {got}")]
    Fork {
        /// The chain.
        chain: ChainId,
        /// The position.
        seq: ChainSeq,
        /// The hash this device holds there.
        have: Hash32,
        /// The different hash it was offered.
        got: Hash32,
    },
    /// A record or a head that would move a chain back behind what this device has seen, offered
    /// without the authority that may do so.
    #[error(
        "rollback of {chain}: pinned at position {pinned}, offered an older state at {offered}"
    )]
    Rollback {
        /// The chain.
        chain: ChainId,
        /// The newest position this device has pinned.
        pinned: ChainSeq,
        /// The older position it was offered.
        offered: ChainSeq,
    },
    /// A peer holds a newer head of the chain than the server has shown this device (or a head of
    /// a restore epoch this device has not been announced): the server may be withholding entries
    /// (for example the removal of a member) or the restore announcement.
    #[error("a head of {chain} is withheld: a peer is at position {peer}, this device at {ours}")]
    Withheld {
        /// The chain.
        chain: ChainId,
        /// The position this device has pinned.
        ours: ChainSeq,
        /// The newer position the peer holds.
        peer: ChainSeq,
    },
    /// A head that must be signed arrived without a usable signature.
    #[error("an unsigned head was presented for {chain}")]
    UnsignedHead {
        /// The chain.
        chain: ChainId,
    },
    /// A head or an entry of the lost timeline: its restore epoch is below the newest restore this
    /// device accepted. However validly it is signed, it is no way back.
    #[error("{chain}: a head or entry of restore epoch {got} was offered after the restore of epoch {floor}")]
    BelowRestoreEpoch {
        /// The chain.
        chain: ChainId,
        /// The restore epoch the head or entry carries.
        got: RestoreEpoch,
        /// The newest restore epoch this device accepted.
        floor: RestoreEpoch,
    },
}

impl Alarm {
    /// The chain the alarm is about.
    pub fn chain(&self) -> ChainId {
        match *self {
            Alarm::Fork { chain, .. }
            | Alarm::Rollback { chain, .. }
            | Alarm::Withheld { chain, .. }
            | Alarm::UnsignedHead { chain }
            | Alarm::BelowRestoreEpoch { chain, .. } => chain,
        }
    }
}

/// The head a server must send with a signature: `Ok` for a head of `chain` that carries a
/// signature, and `Alarm::UnsignedHead` for a head that is missing, signed with no bytes, or about
/// another chain. It checks presence and shape only; the signature itself is `HeadPin::offer`'s.
///
/// Implements planning/architecture.md section 5.9 (a device wraps a key only after it holds a signed
/// head).
///
/// Caller: the wrap gate (CHAIN-GLUE over `team_log_get`, S15c2 `FreshHead`) and the sync client
/// that reads a head from a server.
pub fn require_signed<'a>(
    chain: &ChainId,
    head: Option<&'a SignedHead>,
) -> Result<&'a SignedHead, Alarm> {
    head.filter(|h| h.head.chain == *chain && !h.sig.is_empty())
        .ok_or(Alarm::UnsignedHead { chain: *chain })
}
