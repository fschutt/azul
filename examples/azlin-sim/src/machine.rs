//! The machine seam: everything the world does not know about what a chain carries (planning/
//! architecture.md section 5.9 "every entry is signed by a device whose role allows it"; section
//! 12 E1 "devices run their own check").
//!
//! [`Machine`] owns how a device builds and signs a proposal, how the leader seals it (the stamp and
//! the restore epoch are the leader's), how a replica admits and applies an entry (the leader and
//! every device call the SAME function) and the replica's state hash. Stage 1 plugs
//! [`OpaqueMachine`]: the payload is bytes, authority is "the signer is an enrolled device of the
//! account", and the signature covers the chain, the signer and the payload only, so that the
//! position, the link and the stamp of an entry are the chain crate's business alone (a hostile
//! entry that the chain crate lets through is then accepted by the machine too, and the harness
//! sees it). Stage 2 plugs the real team log.

use std::collections::{BTreeMap, BTreeSet};

use azlin_chain::{ChainEntry, ChainId, Head, RefusalClass};
use azlin_seams::sim::Rng;
use azlin_seams::{hash32, ChainSeq, Hash32, KeyId, RestoreEpoch, Stamp};

use crate::draw::below;
use crate::keys::{Keyring, ENTRY_DOMAIN};

/// The domain of an entry's hash (a test domain).
const ENTRY_HASH_DOMAIN: &str = "azlin-sim-entry-hash-v1";
/// The domain of a proposal's id (a test domain).
const PROPOSAL_ID_DOMAIN: &str = "azlin-sim-proposal-id-v1";
/// The domain of a replica's fold (a test domain).
const FOLD_DOMAIN: &str = "azlin-sim-fold-v1";

/// Why the machine refuses an entry (the chain's own checks come first and have their own
/// refusals).
///
/// Implements planning/architecture.md section 5.9 (the group rejects an entry with a reason for a
/// signer without the role and for a bad signature).
///
/// Caller: `Machine::admit_apply`, `leader`, `device`, `invariants`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MachineRefusal {
    /// The entry belongs to a chain no account of this world owns.
    UnknownChain,
    /// The signer is not an enrolled device of the account.
    NotEnrolled,
    /// The signature does not verify.
    BadSignature,
}

error_text!(MachineRefusal {
    MachineRefusal::UnknownChain => "the chain belongs to no account",
    MachineRefusal::NotEnrolled => "the signer is not an enrolled device of the account",
    MachineRefusal::BadSignature => "the signature does not verify",
});

impl MachineRefusal {
    /// What to do about the refusal: all of them are plain refusals.
    pub fn class(&self) -> RefusalClass {
        match self {
            MachineRefusal::UnknownChain
            | MachineRefusal::NotEnrolled
            | MachineRefusal::BadSignature => RefusalClass::Refuse,
        }
    }
}

/// A proposal: what a device signs and sends to the leader. The leader gives it its stamp and its
/// restore epoch and so makes the entry.
///
/// Implements planning/architecture.md section 5.9 (entry fields: `seq`, `prev`, `signer`, `sig`).
///
/// Caller: `Machine::propose`, `world::Msg`, `leader`, `device`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    /// The chain.
    pub chain: ChainId,
    /// The position the device proposes (its tip's plus one).
    pub seq: ChainSeq,
    /// The hash of the entry the device built on (zero for a genesis).
    pub prev: Hash32,
    /// The payload.
    pub payload: Vec<u8>,
    /// The signing device's key.
    pub signer: KeyId,
    /// The device's signature over the chain, the signer and the payload.
    pub sig: Vec<u8>,
    /// The identity of the proposal: a hash of every field above.
    pub id: Hash32,
}

impl Proposal {
    /// The id the fields say.
    pub fn compute_id(&self) -> Hash32 {
        let kind = [self.chain.kind as u8];
        let seq = self.seq.get().to_be_bytes();
        hash32(
            PROPOSAL_ID_DOMAIN,
            &[
                &kind,
                &self.chain.id,
                &seq,
                self.prev.as_bytes(),
                &self.payload,
                self.signer.as_bytes(),
                &self.sig,
            ],
        )
    }

    /// Whether this copy is what its id says.
    pub fn is_intact(&self) -> bool {
        self.id == self.compute_id()
    }

    /// A damaged copy: one byte of the signature (or of the payload when the signature is empty)
    /// changes and the id does not.
    #[must_use]
    pub fn corrupted(&self, rng: &mut Rng) -> Proposal {
        let mut copy = self.clone();
        flip_one(&mut copy.sig, rng);
        if copy.sig == self.sig {
            flip_one(&mut copy.payload, rng);
        }
        copy
    }
}

/// Flips one byte of `bytes` (chosen from `rng`); nothing when it is empty.
fn flip_one(bytes: &mut [u8], rng: &mut Rng) {
    let index = below(rng, bytes.len() as u64) as usize;
    if let Some(byte) = bytes.get_mut(index) {
        *byte ^= 0x5A;
    }
}

/// An entry of the opaque machine: the proposal's fields, the leader's stamp and restore epoch, the
/// hash of the proposal it came from and its own hash.
///
/// Implements planning/architecture.md section 5.9 (entry fields) and section 6.10 V3 (every entry
/// names its restore epoch).
///
/// Caller: `Machine::seal`, `leader`, `device`, `adversary`, `invariants`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueEntry {
    /// The chain.
    pub chain: ChainId,
    /// The position.
    pub seq: ChainSeq,
    /// The hash of the entry before it.
    pub prev: Hash32,
    /// The leader's stamp.
    pub at: Stamp,
    /// The restore epoch the entry was written in.
    pub epoch: RestoreEpoch,
    /// The payload.
    pub payload: Vec<u8>,
    /// The signing device's key.
    pub signer: KeyId,
    /// The device's signature.
    pub sig: Vec<u8>,
    /// The id of the proposal the entry was made from.
    pub proposal: Hash32,
    /// The hash the entry claims.
    pub hash: Hash32,
}

impl OpaqueEntry {
    /// The hash the fields say.
    pub fn compute_hash(&self) -> Hash32 {
        let kind = [self.chain.kind as u8];
        let seq = self.seq.get().to_be_bytes();
        let at = self.at.as_secs().to_be_bytes();
        let epoch = self.epoch.get().to_be_bytes();
        hash32(
            ENTRY_HASH_DOMAIN,
            &[
                &kind,
                &self.chain.id,
                &seq,
                self.prev.as_bytes(),
                &at,
                &epoch,
                &self.payload,
                self.signer.as_bytes(),
                &self.sig,
                self.proposal.as_bytes(),
            ],
        )
    }

    /// The same entry with its hash made from its fields (a hostile entry built by hand is
    /// intact: only a damaged copy is not).
    #[must_use]
    pub fn rehashed(mut self) -> OpaqueEntry {
        self.hash = self.compute_hash();
        self
    }

    /// The head this entry makes.
    pub fn head(&self) -> Head {
        Head {
            chain: self.chain,
            seq: self.seq,
            hash: self.hash,
        }
    }

    /// A damaged copy: one byte of the payload (or of the signature when the payload is empty)
    /// changes and the claimed hash does not.
    #[must_use]
    pub fn corrupted(&self, rng: &mut Rng) -> OpaqueEntry {
        let mut copy = self.clone();
        flip_one(&mut copy.payload, rng);
        if copy.payload == self.payload {
            flip_one(&mut copy.sig, rng);
        }
        copy
    }
}

impl ChainEntry for OpaqueEntry {
    fn chain(&self) -> ChainId {
        self.chain
    }

    fn seq(&self) -> ChainSeq {
        self.seq
    }

    fn prev(&self) -> Hash32 {
        self.prev
    }

    fn hash(&self) -> Hash32 {
        self.hash
    }

    fn at(&self) -> Stamp {
        self.at
    }

    fn epoch(&self) -> RestoreEpoch {
        self.epoch
    }

    fn is_intact(&self) -> bool {
        self.hash == self.compute_hash()
    }
}

/// One replica's state under the opaque machine: how many entries it applied and a fold of their
/// hashes.
///
/// Implements planning/architecture.md section 5.9 (the group replays the log in order into its
/// tables; a checkpoint carries a hash of the state).
///
/// Caller: `Machine::admit_apply`, `leader`, `device`, `invariants`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueReplica {
    /// How many entries were applied.
    pub applied: u64,
    /// A fold of the hashes of the entries applied, in order.
    pub fold: Hash32,
}

/// The seam of what a chain carries: see the module.
///
/// Implements planning/architecture.md section 5.9 and section 12 E1.
///
/// Seam: stage 2 plugs the real team log (`TeamLogMachine`); stage 1 plugs [`OpaqueMachine`].
pub trait Machine {
    /// What a chain carries.
    type Entry: ChainEntry + Clone + core::fmt::Debug;
    /// What a device sends the leader.
    type Proposal: Clone + core::fmt::Debug;
    /// One replica's state.
    type Replica: Clone + core::fmt::Debug;

    /// The state of a replica that applied nothing.
    fn empty_replica(&self) -> Self::Replica;

    /// A proposal of device `dev` on `chain`, built on the tip `tip` (`None` for an empty chain);
    /// `None` when there is nothing sensible to propose.
    fn propose(
        &self,
        keys: &Keyring,
        dev: usize,
        chain: ChainId,
        tip: Option<Head>,
        intent: u16,
    ) -> Option<Self::Proposal>;

    /// The entry the leader makes of a proposal: its stamp and its restore epoch.
    fn seal(&self, p: &Self::Proposal, at: Stamp, epoch: RestoreEpoch) -> Self::Entry;

    /// THE check: whether the entry is allowed, and the replica after it. The leader and every device
    /// call this one function.
    fn admit_apply(
        &self,
        keys: &Keyring,
        r: &mut Self::Replica,
        e: &Self::Entry,
    ) -> Result<(), MachineRefusal>;

    /// The replica after one more entry that was admitted before (a replay: nothing is checked).
    fn apply_trusted(&self, r: &mut Self::Replica, e: &Self::Entry);

    /// The replica after these entries, which were admitted before.
    fn rebuild(&self, entries: &[Self::Entry]) -> Self::Replica;

    /// The hash of a replica's state.
    fn state_hash(&self, r: &Self::Replica) -> Hash32;
}

/// The stage 1 machine: opaque payloads, authority is "the signer is an enrolled device of the
/// account".
///
/// Implements planning/architecture.md section 5.9 and section 12 E1.
///
/// Caller: `world::World`.
#[derive(Clone, Debug)]
pub struct OpaqueMachine {
    account_of_chain: BTreeMap<ChainId, usize>,
    enrolled: Vec<BTreeSet<KeyId>>,
}

impl OpaqueMachine {
    /// A machine for these chains (each with the index of its account) and these enrolled devices
    /// (the keys of each account's devices).
    pub fn new(
        account_of_chain: BTreeMap<ChainId, usize>,
        enrolled: Vec<BTreeSet<KeyId>>,
    ) -> OpaqueMachine {
        OpaqueMachine {
            account_of_chain,
            enrolled,
        }
    }

    /// The bytes a device's signature covers: the chain, the signer and the payload.
    pub fn signed_bytes(chain: &ChainId, signer: &KeyId, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![chain.kind as u8];
        out.extend_from_slice(&chain.id);
        out.extend_from_slice(signer.as_bytes());
        out.extend_from_slice(payload);
        out
    }

    /// A proposal signed by device `dev` with a payload of its own choosing, on any position and
    /// link (the hostile attacks of the adversary use it with the honest devices' signatures that
    /// the adversary has seen; it is never used by an honest device).
    pub fn proposal_with(
        keys: &Keyring,
        dev: usize,
        chain: ChainId,
        seq: ChainSeq,
        prev: Hash32,
        payload: Vec<u8>,
    ) -> Option<Proposal> {
        let signer = keys.device_key(dev)?;
        let sig = keys.sign_device(
            dev,
            &ENTRY_DOMAIN,
            &Self::signed_bytes(&chain, &signer, &payload),
        )?;
        let mut proposal = Proposal {
            chain,
            seq,
            prev,
            payload,
            signer,
            sig,
            id: Hash32::ZERO,
        };
        proposal.id = proposal.compute_id();
        Some(proposal)
    }
}

impl Machine for OpaqueMachine {
    type Entry = OpaqueEntry;
    type Proposal = Proposal;
    type Replica = OpaqueReplica;

    fn empty_replica(&self) -> OpaqueReplica {
        OpaqueReplica {
            applied: 0,
            fold: Hash32::ZERO,
        }
    }

    fn propose(
        &self,
        keys: &Keyring,
        dev: usize,
        chain: ChainId,
        tip: Option<Head>,
        intent: u16,
    ) -> Option<Proposal> {
        let (seq, prev) = match tip {
            None => (ChainSeq::MIN, Hash32::ZERO),
            Some(head) => (head.seq.next()?, head.hash),
        };
        let payload = intent.to_be_bytes().to_vec();
        OpaqueMachine::proposal_with(keys, dev, chain, seq, prev, payload)
    }

    fn seal(&self, p: &Proposal, at: Stamp, epoch: RestoreEpoch) -> OpaqueEntry {
        OpaqueEntry {
            chain: p.chain,
            seq: p.seq,
            prev: p.prev,
            at,
            epoch,
            payload: p.payload.clone(),
            signer: p.signer,
            sig: p.sig.clone(),
            proposal: p.id,
            hash: Hash32::ZERO,
        }
        .rehashed()
    }

    fn admit_apply(
        &self,
        keys: &Keyring,
        r: &mut OpaqueReplica,
        e: &OpaqueEntry,
    ) -> Result<(), MachineRefusal> {
        let account = self
            .account_of_chain
            .get(&e.chain)
            .ok_or(MachineRefusal::UnknownChain)?;
        let enrolled = self
            .enrolled
            .get(*account)
            .ok_or(MachineRefusal::UnknownChain)?;
        if !enrolled.contains(&e.signer) {
            return Err(MachineRefusal::NotEnrolled);
        }
        keys.verify_device(
            &e.signer,
            &ENTRY_DOMAIN,
            &OpaqueMachine::signed_bytes(&e.chain, &e.signer, &e.payload),
            &e.sig,
        )
        .map_err(|_| MachineRefusal::BadSignature)?;
        r.applied = r.applied.saturating_add(1);
        r.fold = hash32(FOLD_DOMAIN, &[r.fold.as_bytes(), e.hash.as_bytes()]);
        Ok(())
    }

    fn apply_trusted(&self, r: &mut OpaqueReplica, e: &OpaqueEntry) {
        r.applied = r.applied.saturating_add(1);
        r.fold = hash32(FOLD_DOMAIN, &[r.fold.as_bytes(), e.hash.as_bytes()]);
    }

    fn rebuild(&self, entries: &[OpaqueEntry]) -> OpaqueReplica {
        let mut replica = self.empty_replica();
        for e in entries {
            self.apply_trusted(&mut replica, e);
        }
        replica
    }

    fn state_hash(&self, r: &OpaqueReplica) -> Hash32 {
        hash32(FOLD_DOMAIN, &[&r.applied.to_be_bytes(), r.fold.as_bytes()])
    }
}
