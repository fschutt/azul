//! The metadata group, as ONE node: the leader (planning/architecture.md section 5.9 "the metadata
//! group applies each entry once, as it arrives"; section 2.4 "time is the leader's stamp"; section
//! 2.17 "what a restore re-anchors"; section 6.10 V1 "saved means fsynced", V3 "a restore from the
//! off-site copy is announced"). The Raft voters are out of scope (drift row X-DELTA-W1SIM-14): a
//! lying disk of the single replica stands for a lost majority.
//!
//! The leader owns the truth of every chain, signs heads with the GROUP key, stamps entries
//! (glue rule G13), keeps a write-ahead log on its disk, takes off-site copies (a snapshot of its
//! valid WAL prefix) and executes a `disaster` (a restore from a copy) with the OPERATOR's map key
//! signing the record. Its state is a function of its WAL: [`replay_leader`] rebuilds it, and the
//! live path applies exactly what replay applies (glue rule G2).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use azlin_chain::{
    checkpoint_due, Admitted, Chain, ChainId, ChainRefusal, Checkpoint, Head, RefusalClass,
    RestoreRecord, RestoreSigner, SignedHead,
};
use azlin_seams::{ChainSeq, Hash32, RestoreEpoch, Stamp};

use crate::config::{LeaderVariant, MAX_KEPT_RECORDS, MAX_OFFSITE_COPIES};
use crate::disk::{DiskError, SimDisk, Synced};
use crate::machine::{Machine, MachineRefusal, OpaqueEntry, OpaqueMachine, OpaqueReplica};
use crate::sched::{Job, JobKind};
use crate::step::SigChoice;
use crate::time::{StampRefusal, Stamper};
use crate::world::{AckResult, Cause, Msg, Next, Refusal, StampEvent, World};

/// One record of the leader's write-ahead log.
///
/// Implements planning/architecture.md section 5.9 (the registry is replicated and checksummed) and
/// section 6.10 V1.
///
/// Caller: `Leader`, `replay_leader`.
#[derive(Clone, Debug)]
pub(crate) enum LRec {
    /// An entry appended to a chain.
    Entry(OpaqueEntry),
    /// A checkpoint of a chain.
    Checkpoint { chain: ChainId, cp: Checkpoint },
    /// A restore: the chains are cut back to the positions of the record, the epoch is raised.
    Restore(RestoreRecord),
}

/// What the leader keeps of one chain.
#[derive(Clone, Debug)]
pub(crate) struct LChain {
    pub(crate) chain: Chain<OpaqueEntry>,
    pub(crate) replica: OpaqueReplica,
    pub(crate) since_cp: u64,
    pub(crate) last_cp: Option<Stamp>,
    pub(crate) checkpoints: Vec<Checkpoint>,
}

impl LChain {
    pub(crate) fn new(id: ChainId, machine: &OpaqueMachine) -> LChain {
        LChain {
            chain: Chain::new(id),
            replica: machine.empty_replica(),
            since_cp: 0,
            last_cp: None,
            checkpoints: Vec::new(),
        }
    }

    /// Cuts the chain back to the entries at or below `keep`.
    pub(crate) fn truncate(&mut self, keep: ChainSeq, machine: &OpaqueMachine) {
        let id = self.chain.id();
        let kept: Vec<OpaqueEntry> = self
            .chain
            .entries()
            .iter()
            .filter(|e| e.seq <= keep)
            .cloned()
            .collect();
        let mut chain = Chain::new(id);
        for entry in &kept {
            if chain.admit(entry.clone()).is_err() {
                break;
            }
        }
        self.replica = machine.rebuild(&kept);
        self.chain = chain;
        self.checkpoints.retain(|cp| cp.head().seq <= keep);
        let head_seq = self.chain.head().map_or(0, |h| h.seq.get());
        self.since_cp = match self.checkpoints.last() {
            Some(cp) => head_seq.saturating_sub(cp.head().seq.get()),
            None => kept.len() as u64,
        };
        self.last_cp = self.checkpoints.last().map(|cp| cp.signed_head().signed_at);
    }
}

/// The state a WAL replays into.
#[derive(Clone, Debug)]
pub(crate) struct Replayed {
    pub(crate) epoch: RestoreEpoch,
    pub(crate) chains: BTreeMap<ChainId, LChain>,
    pub(crate) records: VecDeque<RestoreRecord>,
    pub(crate) last_stamp: Option<Stamp>,
}

/// Replays a write-ahead log into the state of the leader: the one function the live path and the
/// restart share.
///
/// Implements planning/architecture.md section 5.9 ("any copy can restore it") and glue rule G2.
///
/// Caller: `World::restart_leader`, `World::build_genesis`, `l_restore_phase`.
pub(crate) fn replay_leader(
    recs: &[LRec],
    machine: &OpaqueMachine,
    start_epoch: RestoreEpoch,
) -> Replayed {
    let mut state = Replayed {
        epoch: start_epoch,
        chains: BTreeMap::new(),
        records: VecDeque::new(),
        last_stamp: None,
    };
    for rec in recs {
        match rec {
            LRec::Entry(entry) => {
                let lc = state
                    .chains
                    .entry(entry.chain)
                    .or_insert_with(|| LChain::new(entry.chain, machine));
                if let Ok(Admitted::Appended { .. }) = lc.chain.admit(entry.clone()) {
                    machine.apply_trusted(&mut lc.replica, entry);
                    lc.since_cp = lc.since_cp.saturating_add(1);
                    state.last_stamp = state.last_stamp.max(Some(entry.at));
                }
            }
            LRec::Checkpoint { chain, cp } => {
                if let Some(lc) = state.chains.get_mut(chain) {
                    let head_seq = lc.chain.head().map_or(0, |h| h.seq.get());
                    lc.since_cp = head_seq.saturating_sub(cp.head().seq.get());
                    lc.last_cp = Some(cp.signed_head().signed_at);
                    lc.checkpoints.push(cp.clone());
                }
            }
            LRec::Restore(record) => {
                for head in &record.positions {
                    if let Some(lc) = state.chains.get_mut(&head.chain) {
                        lc.truncate(head.seq, machine);
                    }
                }
                for lc in state.chains.values_mut() {
                    lc.chain.raise_floor(record.epoch);
                }
                state.epoch = record.epoch;
                state.records.push_back(record.clone());
                while state.records.len() > MAX_KEPT_RECORDS {
                    state.records.pop_front();
                }
                state.last_stamp = state.last_stamp.max(Some(record.at));
            }
        }
    }
    state
}

/// The leader.
///
/// Implements planning/architecture.md section 5.9, section 2.4, section 2.17 and section 6.10.
///
/// Caller: `world::World`.
#[derive(Debug)]
pub(crate) struct Leader {
    pub(crate) up: bool,
    pub(crate) faulted: bool,
    pub(crate) wal: SimDisk<LRec>,
    pub(crate) epoch: RestoreEpoch,
    pub(crate) start_epoch: RestoreEpoch,
    pub(crate) chains: BTreeMap<ChainId, LChain>,
    pub(crate) reserved: BTreeMap<ChainId, (ChainSeq, Hash32)>,
    pub(crate) stamper: Stamper,
    pub(crate) records: VecDeque<RestoreRecord>,
    pub(crate) offsite: VecDeque<Vec<LRec>>,
    pub(crate) restoring: bool,
    pub(crate) gen: u64,
    pub(crate) variant: LeaderVariant,
    pub(crate) cp_pending: BTreeSet<ChainId>,
}

impl Leader {
    pub(crate) fn new(variant: LeaderVariant, epoch: RestoreEpoch) -> Leader {
        Leader {
            up: true,
            faulted: false,
            wal: SimDisk::new(),
            epoch,
            start_epoch: epoch,
            chains: BTreeMap::new(),
            reserved: BTreeMap::new(),
            stamper: Stamper::new(),
            records: VecDeque::new(),
            offsite: VecDeque::new(),
            restoring: false,
            gen: 0,
            variant,
            cp_pending: BTreeSet::new(),
        }
    }

    /// A process crash: everything volatile is gone.
    pub(crate) fn crash(&mut self) {
        self.up = false;
        self.restoring = false;
        self.reserved.clear();
        self.cp_pending.clear();
        self.chains.clear();
        self.records.clear();
    }
}

fn release(world: &mut World, job: &Job) {
    if let Some(sealed) = &job.sealed {
        if world.leader.reserved.get(&sealed.chain) == Some(&(sealed.seq, sealed.hash)) {
            world.leader.reserved.remove(&sealed.chain);
        }
    }
}

impl World {
    /// The leader's next stamp, from its clock and the three voters' clocks (glue rule G13).
    pub(crate) fn leader_stamp(&mut self) -> Result<Stamp, StampRefusal> {
        let d = self.devices.len();
        let wall = self.clocks.read(0);
        let voters = [
            self.clocks.read(0),
            self.clocks.read(d + 1),
            self.clocks.read(d + 2),
        ];
        if self.leader.variant == LeaderVariant::BuggyStamp {
            self.leader.stamper.raw(wall)
        } else {
            self.leader.stamper.stamp(wall, &voters)
        }
    }

    /// The highest restore epoch the operator ever signed (the operator keeps the counter).
    pub(crate) fn operator_epoch(&self) -> RestoreEpoch {
        self.ledger
            .operator_records
            .values()
            .map(|r| r.epoch)
            .max()
            .unwrap_or(self.leader.start_epoch)
            .max(self.leader.start_epoch)
    }

    /// The group signs a head of a chain at its tip, in the leader's current epoch.
    fn sign_tip(&mut self, head: Head, epoch: RestoreEpoch, stamp: Stamp) -> SignedHead {
        let signed = self.keys.sign_head(head, epoch, stamp);
        self.ledger.signed.insert((head.chain, head.seq, head.hash));
        if self.ledger.head_log.len() >= 4096 {
            self.ledger.head_log.pop_front();
        }
        self.ledger.head_log.push_back(signed.clone());
        self.delta
            .heads_signed
            .push((head.chain, head.seq, head.hash, epoch));
        signed
    }

    fn l_refuse(&mut self, job: &mut Job, refusal: Refusal) {
        job.answer = Some(AckResult::Refused(refusal));
        job.skip_rest = true;
    }

    // ---------------------------------------------------------------------------------------
    // L.append
    // ---------------------------------------------------------------------------------------

    pub(crate) fn l_append_phase(&mut self, job: &mut Job) -> Next {
        match job.phase {
            0 => job.gen = self.leader.gen,
            1 => self.l_append_admit(job),
            2 => self.l_append_write(job),
            3 => self.l_append_sync(job),
            4 => self.l_append_apply(job),
            _ => self.l_append_ack(job),
        }
        Next::Advance
    }

    fn l_append_admit(&mut self, job: &mut Job) {
        let Some(prop) = job.proposal.clone() else {
            job.skip_rest = true;
            return;
        };
        if self.leader.faulted {
            return self.l_refuse(job, Refusal::Faulted);
        }
        if self.leader.restoring {
            return self.l_refuse(job, Refusal::Busy);
        }
        if !prop.is_intact() {
            self.note_refusal(0, Cause::Corrupt, RefusalClass::Refetch);
            return self.l_refuse(job, Refusal::Corrupt);
        }
        let Some((existing, tip)) = self
            .leader
            .chains
            .get(&prop.chain)
            .map(|lc| (lc.chain.get(prop.seq).cloned(), lc.chain.head()))
        else {
            return self.l_refuse(job, Refusal::Machine(MachineRefusal::UnknownChain));
        };
        // The same proposal again: held already, never a fork (glue rule G3).
        if let Some(existing) = existing {
            if existing.proposal == prop.id {
                job.answer = Some(AckResult::Held(existing.head()));
                job.skip_rest = true;
                return;
            }
        }
        let tip = match self.leader.reserved.get(&prop.chain) {
            Some((seq, hash)) => Some(Head {
                chain: prop.chain,
                seq: *seq,
                hash: *hash,
            }),
            None => tip,
        };
        let (expected, prev) = match tip {
            None => (ChainSeq::MIN, Hash32::ZERO),
            Some(head) => match head.seq.next() {
                Some(next) => (next, head.hash),
                None => return self.l_refuse(job, Refusal::Chain(ChainRefusal::Full)),
            },
        };
        // The leader judges a PROPOSAL against its own tip; the chain crate judges the sealed
        // entry (glue rule G17: a stale proposal is a retry, never a fork alarm).
        if prop.seq != expected {
            return self.l_refuse(
                job,
                Refusal::Stale(ChainRefusal::Gap {
                    expected,
                    got: prop.seq,
                }),
            );
        }
        if prop.prev != prev {
            return self.l_refuse(
                job,
                Refusal::Stale(ChainRefusal::WrongPrev {
                    expected: prev,
                    got: prop.prev,
                }),
            );
        }
        let stamp = match self.leader_stamp() {
            Ok(stamp) => stamp,
            Err(error) => return self.l_refuse(job, Refusal::Stamp(error)),
        };
        let epoch = if self.leader.variant == LeaderVariant::BuggyEpoch {
            RestoreEpoch::new(self.leader.epoch.get().saturating_sub(1))
        } else {
            self.leader.epoch
        };
        // The stamp is the leader's own act: it is told to the checkers even if the entry is refused later.
        self.delta.stamps.push(StampEvent::Stamped {
            chain: prop.chain,
            seq: prop.seq,
            at: stamp,
        });
        let sealed = self.machine.seal(&prop, stamp, epoch);
        let Some(mut replica) = self
            .leader
            .chains
            .get(&prop.chain)
            .map(|lc| lc.replica.clone())
        else {
            return self.l_refuse(job, Refusal::Machine(MachineRefusal::UnknownChain));
        };
        if let Err(refusal) = self.machine.admit_apply(&self.keys, &mut replica, &sealed) {
            self.note_refusal(0, Cause::Other, refusal.class());
            return self.l_refuse(job, Refusal::Machine(refusal));
        }
        let placement = self
            .leader
            .chains
            .get(&prop.chain)
            .map(|lc| lc.chain.check(&sealed));
        match placement {
            Some(Ok(_)) => {}
            Some(Err(refusal)) => {
                self.note_chain_refusal(0, &refusal, prop.chain);
                return self.l_refuse(job, Refusal::Chain(refusal));
            }
            None => return self.l_refuse(job, Refusal::Machine(MachineRefusal::UnknownChain)),
        }
        self.leader
            .reserved
            .insert(prop.chain, (sealed.seq, sealed.hash));
        job.sealed = Some(sealed);
        job.replica_after = Some(replica);
    }

    fn l_abort(&mut self, job: &mut Job) {
        release(self, job);
        job.sealed = None;
        job.replica_after = None;
        job.answer = None;
        job.skip_rest = true;
        job.aborted = true;
    }

    fn l_append_write(&mut self, job: &mut Job) {
        if job.skip_rest || job.sealed.is_none() {
            return;
        }
        if job.gen != self.leader.gen {
            return self.l_abort(job);
        }
        let Some(sealed) = job.sealed.clone() else {
            return;
        };
        if self.leader.wal.append(LRec::Entry(sealed)).is_err() {
            release(self, job);
            job.sealed = None;
            return self.l_refuse(job, Refusal::Busy);
        }
        if self.leader.variant == LeaderVariant::AckBeforeFsync {
            self.l_send_ack(job);
        }
    }

    fn l_append_sync(&mut self, job: &mut Job) {
        if job.skip_rest || job.sealed.is_none() {
            return;
        }
        if job.gen != self.leader.gen {
            return self.l_abort(job);
        }
        let lies = self.rates().lost_fsync.hit(&mut self.rng);
        match self.leader.wal.fsync(lies) {
            Ok(Synced::Durable) => {}
            Ok(Synced::Lied) => self.ledger.lies_told += 1,
            Err(_) => {
                release(self, job);
                job.sealed = None;
                self.l_refuse(job, Refusal::Busy);
            }
        }
    }

    fn l_append_apply(&mut self, job: &mut Job) {
        if job.skip_rest {
            return;
        }
        let (Some(sealed), Some(replica)) = (job.sealed.clone(), job.replica_after.take()) else {
            return;
        };
        if job.gen != self.leader.gen {
            return self.l_abort(job);
        }
        let applied = match self.leader.chains.get_mut(&sealed.chain) {
            Some(lc) => match lc.chain.admit(sealed.clone()) {
                Ok(Admitted::Appended { .. }) => {
                    lc.replica = replica;
                    lc.since_cp = lc.since_cp.saturating_add(1);
                    Ok(())
                }
                Ok(Admitted::AlreadyHeld { .. }) => Ok(()),
                Err(refusal) => Err(refusal),
            },
            None => Err(ChainRefusal::WrongChain),
        };
        release(self, job);
        if let Err(refusal) = applied {
            self.harness(format!(
                "the leader's chain refused an entry it had checked: {refusal}"
            ));
        }
    }

    /// The leader acknowledges: the ack to the proposer (once), and the ledger of what it
    /// acknowledged.
    fn l_send_ack(&mut self, job: &mut Job) {
        if job.acked_sent {
            return;
        }
        let Some(proposal) = job.proposal.clone() else {
            return;
        };
        let result = match (&job.answer, &job.sealed) {
            (Some(answer), _) => answer.clone(),
            (None, Some(sealed)) => AckResult::Acked(sealed.head()),
            (None, None) => return,
        };
        job.acked_sent = true;
        if let (AckResult::Acked(head), Some(sealed)) = (&result, &job.sealed) {
            self.ledger
                .acked
                .insert((head.chain, head.seq), (head.hash, sealed.epoch));
            if self.ledger.entry_log.len() >= 4096 {
                self.ledger.entry_log.pop_front();
            }
            self.ledger.entry_log.push_back(sealed.clone());
            self.delta.acks.push((head.chain, head.seq, head.hash));
        }
        let to = job.from;
        self.send(
            0,
            to,
            Msg::Ack {
                id: proposal.id,
                result,
            },
        );
    }

    fn l_append_ack(&mut self, job: &mut Job) {
        if job.aborted {
            return;
        }
        self.l_send_ack(job);
        let (None, Some(sealed)) = (&job.answer, job.sealed.clone()) else {
            return;
        };
        // The notices: the new entry and a signed head over it, to every device of the account.
        let stamp = match self.leader_stamp() {
            Ok(stamp) => stamp,
            Err(_) => return,
        };
        let epoch = self.leader.epoch;
        let head = self.sign_tip(sealed.head(), epoch, stamp);
        let nodes = match self.account_of(&sealed.chain) {
            Some(account) => self.account_nodes(account),
            None => Vec::new(),
        };
        for node in nodes {
            self.send(
                0,
                node,
                Msg::Entries {
                    chain: sealed.chain,
                    entries: vec![sealed.clone()],
                    head: Some(head.clone()),
                    records: Vec::new(),
                },
            );
        }
    }

    // ---------------------------------------------------------------------------------------
    // L.head
    // ---------------------------------------------------------------------------------------

    pub(crate) fn l_head_phase(&mut self, job: &mut Job) -> Next {
        match job.phase {
            0 => {
                job.gen = self.leader.gen;
                if self.leader.faulted || self.leader.restoring {
                    job.skip_rest = true;
                } else {
                    job.epoch = self.leader.epoch;
                    job.tips = self
                        .leader
                        .chains
                        .values()
                        .filter_map(|lc| lc.chain.head())
                        .collect();
                }
            }
            1 => self.l_head_sign(job),
            _ => {
                if !job.skip_rest {
                    for signed in job.heads.clone() {
                        let nodes = match self.account_of(&signed.head.chain) {
                            Some(account) => self.account_nodes(account),
                            None => Vec::new(),
                        };
                        self.delta.heads_sent.push((
                            signed.head.chain,
                            signed.head.seq,
                            signed.head.hash,
                            signed.epoch,
                        ));
                        for node in nodes {
                            self.send(0, node, Msg::Head(signed.clone()));
                        }
                    }
                }
            }
        }
        Next::Advance
    }

    fn l_head_sign(&mut self, job: &mut Job) {
        if job.skip_rest {
            return;
        }
        if job.gen != self.leader.gen {
            job.skip_rest = true;
            return;
        }
        let stamp = match self.leader_stamp() {
            Ok(stamp) => stamp,
            Err(_) => {
                job.skip_rest = true;
                return;
            }
        };
        for (index, tip) in job.tips.clone().into_iter().enumerate() {
            let signed = self.sign_tip(tip, job.epoch, stamp);
            job.heads.push(signed);
            if self.leader.variant == LeaderVariant::DoubleSign && index == 0 {
                let other = Head {
                    hash: azlin_seams::hash32("azlin-sim-double-sign-v1", &[tip.hash.as_bytes()]),
                    ..tip
                };
                let second = self.sign_tip(other, job.epoch, stamp);
                job.heads.push(second);
            }
        }
        if self.leader.variant == LeaderVariant::NoCheckpoints {
            return;
        }
        // The checkpoint rule: whenever one is due, the leader writes it with its next head.
        let due: Vec<ChainId> = self
            .leader
            .chains
            .iter()
            .filter(|(id, lc)| {
                let last = lc
                    .last_cp
                    .or_else(|| lc.chain.entries().first().map(|e| e.at))
                    .unwrap_or(stamp);
                checkpoint_due(lc.since_cp, last, stamp) && !self.leader.cp_pending.contains(*id)
            })
            .map(|(id, _)| *id)
            .collect();
        for chain in due {
            let mut cp_job = Job::new(JobKind::LCheckpoint, 0);
            cp_job.chain = Some(chain);
            if self.sched.spawn(cp_job).is_ok() {
                self.leader.cp_pending.insert(chain);
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // L.checkpoint
    // ---------------------------------------------------------------------------------------

    pub(crate) fn l_checkpoint_phase(&mut self, job: &mut Job) -> Next {
        let chain = job.chain;
        match job.phase {
            0 => {
                job.gen = self.leader.gen;
                if self.leader.faulted || self.leader.restoring {
                    job.skip_rest = true;
                } else if let Some(lc) = chain.and_then(|id| self.leader.chains.get(&id)) {
                    job.tips = lc.chain.head().into_iter().collect();
                    job.state_hash = Some(self.machine.state_hash(&lc.replica));
                    job.stamp = lc.chain.last_at();
                    job.epoch = self.leader.epoch;
                } else {
                    job.skip_rest = true;
                }
            }
            1 => {
                if !job.skip_rest {
                    if job.gen != self.leader.gen {
                        job.skip_rest = true;
                    } else if let (Some(tip), Some(state), Some(at)) =
                        (job.tips.first().copied(), job.state_hash, job.stamp)
                    {
                        match self.leader_stamp() {
                            Ok(now) => {
                                let signed = self.sign_tip(tip, job.epoch, now);
                                job.checkpoint = Some(Checkpoint::new(signed, at, state));
                            }
                            Err(_) => job.skip_rest = true,
                        }
                    } else {
                        job.skip_rest = true;
                    }
                }
            }
            2 => {
                if !job.skip_rest {
                    if let (Some(cp), Some(id)) = (job.checkpoint.clone(), chain) {
                        if self
                            .leader
                            .wal
                            .append(LRec::Checkpoint { chain: id, cp })
                            .is_err()
                        {
                            job.skip_rest = true;
                        }
                    }
                }
            }
            3 => {
                if !job.skip_rest {
                    let lies = self.rates().lost_fsync.hit(&mut self.rng);
                    match self.leader.wal.fsync(lies) {
                        Ok(Synced::Lied) => self.ledger.lies_told += 1,
                        Ok(Synced::Durable) => {}
                        Err(_) => job.skip_rest = true,
                    }
                }
            }
            _ => {
                if let Some(id) = chain {
                    self.leader.cp_pending.remove(&id);
                }
                if !job.skip_rest && job.gen == self.leader.gen {
                    if let (Some(cp), Some(id)) = (job.checkpoint.clone(), chain) {
                        let signed = cp.signed_head().clone();
                        if let Some(lc) = self.leader.chains.get_mut(&id) {
                            let head_seq = lc.chain.head().map_or(0, |h| h.seq.get());
                            lc.since_cp = head_seq.saturating_sub(cp.head().seq.get());
                            lc.last_cp = Some(signed.signed_at);
                            lc.checkpoints.push(cp);
                        }
                        let nodes = match self.account_of(&id) {
                            Some(account) => self.account_nodes(account),
                            None => Vec::new(),
                        };
                        for node in nodes {
                            self.send(0, node, Msg::Head(signed.clone()));
                        }
                    }
                }
            }
        }
        Next::Advance
    }

    // ---------------------------------------------------------------------------------------
    // L.offsite
    // ---------------------------------------------------------------------------------------

    pub(crate) fn l_offsite_phase(&mut self, _job: &mut Job) -> Next {
        let prefix = self.leader.wal.valid_prefix();
        if prefix.cut.is_none() {
            if self.leader.offsite.len() >= MAX_OFFSITE_COPIES {
                self.leader.offsite.pop_front();
            }
            self.leader.offsite.push_back(prefix.records);
        }
        Next::Advance
    }

    // ---------------------------------------------------------------------------------------
    // L.fetch
    // ---------------------------------------------------------------------------------------

    pub(crate) fn l_fetch_phase(&mut self, job: &mut Job) -> Next {
        match job.phase {
            0 => {
                if self.leader.faulted || self.leader.restoring {
                    job.skip_rest = true;
                }
            }
            1 => {
                if !job.skip_rest {
                    self.l_fetch_read(job);
                }
            }
            _ => {
                if !job.skip_rest {
                    if let Some(chain) = job.chain {
                        let to = job.from;
                        self.send(
                            0,
                            to,
                            Msg::Entries {
                                chain,
                                entries: job.entries.clone(),
                                head: job.head.clone(),
                                records: job.records.clone(),
                            },
                        );
                    }
                }
            }
        }
        Next::Advance
    }

    fn l_fetch_read(&mut self, job: &mut Job) {
        let Some(chain) = job.chain else {
            job.skip_rest = true;
            return;
        };
        let (entries, tip) = match self.leader.chains.get(&chain) {
            Some(lc) => (
                lc.chain
                    .entries()
                    .iter()
                    .filter(|e| job.after.is_none_or(|after| e.seq > after))
                    .take(64)
                    .cloned()
                    .collect::<Vec<OpaqueEntry>>(),
                lc.chain.head(),
            ),
            None => {
                job.skip_rest = true;
                return;
            }
        };
        job.entries = entries;
        // The records of the account of this chain that the device has not been announced.
        let account = self.account_of(&chain);
        job.records = self
            .leader
            .records
            .iter()
            .filter(|r| r.epoch > job.floor && self.record_account(r) == account)
            .cloned()
            .collect();
        if let Some(tip) = tip {
            if let Ok(stamp) = self.leader_stamp() {
                let epoch = self.leader.epoch;
                job.head = Some(self.sign_tip(tip, epoch, stamp));
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // L.restore
    // ---------------------------------------------------------------------------------------

    pub(crate) fn l_restore_phase(&mut self, job: &mut Job) -> Next {
        match job.phase {
            0 => self.l_restore_build(job),
            1 => {}
            2 => self.l_restore_sync(job),
            3 => self.l_restore_apply(job),
            _ => self.l_restore_broadcast(job),
        }
        Next::Advance
    }

    fn l_restore_build(&mut self, job: &mut Job) {
        self.leader.restoring = true;
        job.gen = self.leader.gen;
        let copies = self.leader.offsite.len();
        let Some(copy) = self.leader.offsite.get(job.copy % copies.max(1)).cloned() else {
            self.leader.restoring = false;
            job.skip_rest = true;
            return;
        };
        let restored = replay_leader(&copy, &self.machine, self.leader.start_epoch);
        let base = restored
            .epoch
            .max(self.leader.epoch)
            .max(self.operator_epoch());
        let Some(epoch) = base.next() else {
            // The counter of restores cannot count another one: a typed refusal.
            self.ledger.restore_refused_exhausted += 1;
            self.leader.restoring = false;
            job.skip_rest = true;
            return;
        };
        // The record is stamped by the leader that is restored: its clock, never below the
        // survivors' stamps, never past the voters.
        let mut stamper = Stamper::resume(restored.last_stamp);
        let d = self.devices.len();
        let voters = [
            self.clocks.read(0),
            self.clocks.read(d + 1),
            self.clocks.read(d + 2),
        ];
        let at = match stamper.stamp(self.clocks.read(0), &voters) {
            Ok(at) => at,
            Err(_) => {
                self.leader.restoring = false;
                job.skip_rest = true;
                return;
            }
        };
        let signers: &[RestoreSigner] = match job.sig {
            SigChoice::Map => &[RestoreSigner::MapKey],
            SigChoice::Group => &[RestoreSigner::Group],
            SigChoice::Both => &[RestoreSigner::MapKey, RestoreSigner::Group],
            SigChoice::None => &[],
        };
        // One record for each account, listing the survivors of ITS chains only (a device learns
        // nothing of the chains of another account, and pins none of them).
        let accounts: BTreeSet<usize> = self.chains.iter().map(|info| info.account).collect();
        let mut staged = copy;
        let mut made = Vec::new();
        for account in accounts {
            let positions: Vec<Head> = restored
                .chains
                .iter()
                .filter(|(id, _)| self.account_of(id) == Some(account))
                .filter_map(|(_, lc)| lc.chain.head())
                .collect();
            if positions.is_empty() {
                continue;
            }
            let mut record = RestoreRecord {
                epoch,
                at,
                positions,
                signatures: Vec::new(),
            };
            record.signatures = signers
                .iter()
                .map(|by| self.keys.restore_sig(&record, *by))
                .collect();
            if signers.contains(&RestoreSigner::MapKey) {
                self.ledger
                    .operator_records
                    .insert(crate::keys::record_id(&record), record.clone());
                for head in &record.positions {
                    self.ledger
                        .record_positions
                        .insert((head.chain, head.seq, head.hash));
                }
            }
            self.ledger.record_log.push(record.clone());
            staged.push(LRec::Restore(record.clone()));
            made.push(record);
        }
        job.staged = staged;
        job.records = made;
    }

    /// The account a restore record is about: the account of the first chain it lists.
    pub(crate) fn record_account(&self, record: &RestoreRecord) -> Option<usize> {
        record
            .positions
            .first()
            .and_then(|head| self.account_of(&head.chain))
    }

    /// Sends a restore record to every device of its account.
    fn announce_record(&mut self, record: &RestoreRecord) {
        let account = self.record_account(record);
        for dev in 0..self.devices.len() {
            let Some((node, of)) = self.devices.get(dev).map(|d| (d.node, d.account)) else {
                continue;
            };
            if Some(of) == account {
                self.send(0, node, Msg::Restore(record.clone()));
            }
        }
    }

    fn l_restore_sync(&mut self, job: &mut Job) {
        if job.skip_rest {
            return;
        }
        let lies = self.rates().lost_fsync.hit(&mut self.rng);
        match self.leader.wal.replace(job.staged.clone(), lies) {
            Ok(Synced::Durable) => {}
            Ok(Synced::Lied) => self.ledger.lies_told += 1,
            Err(_) => {
                self.leader.restoring = false;
                job.skip_rest = true;
            }
        }
    }

    fn l_restore_apply(&mut self, job: &mut Job) {
        if job.skip_rest {
            return;
        }
        let Some(record) = job.records.first().cloned() else {
            return;
        };
        let before = self.leader.epoch;
        let replayed = replay_leader(&job.staged, &self.machine, self.leader.start_epoch);
        self.leader.chains = replayed.chains;
        self.leader.epoch = record.epoch;
        self.leader.gen = self.leader.gen.saturating_add(1);
        self.leader.reserved.clear();
        self.leader.cp_pending.clear();
        self.leader.faulted = false;
        self.leader.records = replayed.records;
        self.leader.stamper = Stamper::resume(replayed.last_stamp.max(Some(record.at)));
        let records = job.records.clone();
        self.restore_took_effect(&records, before);
    }

    /// What a restore that took effect means to the oracles, whether it was applied live or found
    /// durable in the log at a restart: the leader's chains lost what lay beyond the survivors (it is
    /// no longer acknowledged state), and the stamps start again from what the chains hold.
    fn restore_took_effect(&mut self, records: &[RestoreRecord], before: RestoreEpoch) {
        let Some(first) = records.first() else {
            return;
        };
        self.ledger.restores += 1;
        if self.ledger.first_restore_step.is_none() {
            self.ledger.first_restore_step = Some(self.step_no);
        }
        self.cov_restore();
        self.delta.leader_restore = Some((before, first.epoch));
        let survivors: BTreeMap<ChainId, ChainSeq> = records
            .iter()
            .flat_map(|r| r.positions.iter())
            .map(|h| (h.chain, h.seq))
            .collect();
        self.ledger
            .acked
            .retain(|(chain, seq), _| survivors.get(chain).is_none_or(|s| *seq <= *s));
        self.reset_stamp_floors();
    }

    /// The stamps start again from what the leader's chains hold (after a restart or a restore):
    /// a stamp given to an entry that was never written is no stamp of the chain.
    fn reset_stamp_floors(&mut self) {
        let events: Vec<StampEvent> = self
            .leader
            .chains
            .iter()
            .map(|(id, lc)| StampEvent::Reset {
                chain: *id,
                at: lc.chain.last_at(),
            })
            .collect();
        self.delta.stamps.extend(events);
    }

    fn cov_restore(&mut self) {
        self.trace
            .push(format!("leader restored to epoch {}", self.leader.epoch));
    }

    fn l_restore_broadcast(&mut self, job: &mut Job) {
        self.leader.restoring = false;
        if job.skip_rest || !job.announce {
            return;
        }
        let Some(epoch) = job.records.first().map(|r| r.epoch) else {
            return;
        };
        self.ledger.announced_epoch = self.ledger.announced_epoch.max(epoch);
        for record in job.records.clone() {
            self.announce_record(&record);
        }
    }

    // ---------------------------------------------------------------------------------------
    // Restart
    // ---------------------------------------------------------------------------------------

    /// Restarts the leader from its disk (glue rule G2) or from nothing; whether it is up.
    pub(crate) fn restart_leader(&mut self, wipe: bool) -> bool {
        if wipe {
            self.leader.wal.reset();
            self.leader.chains.clear();
            self.leader.records.clear();
            self.leader.epoch = self.leader.start_epoch;
            self.leader.stamper = Stamper::new();
            self.leader.faulted = true;
            self.leader.up = true;
            self.leader.restoring = false;
            self.ledger.degraded = true;
            self.ledger.losses.push(format!(
                "step {}: the leader was restarted with an empty disk",
                self.step_no
            ));
            self.delta.leader_restarted = true;
            return true;
        }
        let prefix = self.leader.wal.valid_prefix();
        let mut faulted = false;
        match prefix.cut {
            Some(DiskError::Read) => return false,
            Some(DiskError::Gone) => {
                faulted = true;
                self.ledger
                    .losses
                    .push(format!("step {}: the leader's disk is gone", self.step_no));
            }
            Some(DiskError::Corrupt { index }) => {
                // A torn tail is the usual end of a crash; a corrupt record in the middle is data lost.
                if index.saturating_add(1) < self.leader.wal.len() {
                    faulted = true;
                    self.ledger.losses.push(format!(
                        "step {}: the leader's log is corrupt at record {index} of {}",
                        self.step_no,
                        self.leader.wal.len()
                    ));
                }
                if self
                    .leader
                    .wal
                    .replace(prefix.records.clone(), false)
                    .is_err()
                {
                    faulted = true;
                }
            }
            Some(DiskError::Full) | None => {}
        }
        if faulted {
            self.ledger.degraded = true;
            self.leader.faulted = true;
        }
        let before = self.leader.epoch;
        let replayed = replay_leader(&prefix.records, &self.machine, self.leader.start_epoch);
        // A restore that was durable when the leader crashed, before it was applied, takes effect now.
        let fresh: Vec<RestoreRecord> = replayed
            .records
            .iter()
            .filter(|r| r.epoch > before)
            .cloned()
            .collect();
        self.leader.chains = replayed.chains;
        self.leader.epoch = replayed.epoch;
        self.leader.records = replayed.records;
        self.leader.stamper = Stamper::resume(replayed.last_stamp);
        self.leader.up = true;
        self.leader.restoring = false;
        self.leader.reserved.clear();
        self.leader.gen = self.leader.gen.saturating_add(1);
        self.delta.leader_restarted = true;
        if fresh.is_empty() {
            self.reset_stamp_floors();
        } else {
            self.restore_took_effect(&fresh, before);
        }
        // Glue rule G11: the leader announces every restore record it holds again.
        if !self.leader.faulted {
            let records: Vec<RestoreRecord> = self.leader.records.iter().cloned().collect();
            for record in records {
                self.announce_record(&record);
            }
            if !self.leader.records.is_empty() {
                self.ledger.announced_epoch = self.ledger.announced_epoch.max(self.leader.epoch);
            }
        }
        true
    }
}
