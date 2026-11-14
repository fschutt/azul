//! A device (planning/architecture.md section 5.9 "each device remembers the newest head it has seen
//! and refuses an older one", "devices of one account exchange the heads they have seen whenever
//! they meet"; section 5.13 "a head never goes backwards"; section 2.17 "devices re-anchor every
//! one of them and send again whatever they hold beyond them"; section 6.10 V3).
//!
//! A device holds a pin of the heads it was offered (`azlin_chain::HeadPin`), a copy of each chain it
//! follows (`azlin_chain::Chain`), a write-ahead log, the entries it must send again after a restore
//! and its alarms. Its state is a function of its log: [`apply_record`] is the ONE function the live
//! path and the restart share (glue rule G2), applied in log order after an fsync returned (glue rule
//! G1). Glue rule G18: a device admits entries into a chain copy only up to the position of the
//! signed head it pinned ("the part of the chain it verified"), and only if the hash at that
//! position is the pinned one.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use azlin_chain::{
    compare_peer, reanchor, rollback_alarm, verify_record, Admitted, Alarm, Chain, ChainId,
    ChainRefusal, GroupAloneTrust, Head, HeadPin, HeadVerifier, PinVerdict, Placement,
    RefusalClass, RestoreRecord, RestoreRefusal, RestoreSigVerifier, RestoreSigner, SignedHead,
    StrictTrust,
};
use azlin_seams::sim::Rng;
use azlin_seams::{hash32, ChainSeq, Hash32, KeyId, RestoreEpoch};

use crate::config::Defect;
use crate::disk::{DiskError, SimDisk};
use crate::draw::below;
use crate::keys::{record_id, Keyring, SimHeadVerifier, SimRestoreVerifier};
use crate::machine::{Machine, MachineRefusal, OpaqueEntry, OpaqueMachine, OpaqueReplica};
use crate::sched::{Job, JobKind, PeerCmp};
use crate::world::{
    AckResult, Cause, ChainInfo, DeviceRestart, HeldBeyond, Ledger, MeetEvent, Msg, Next, PinMove,
    PinView, Refusal, RestoreEvent, World,
};

/// One record of a device's write-ahead log.
///
/// Implements planning/architecture.md section 5.9 ("every device keeps the part of the chain it
/// verified").
///
/// Caller: `Device`, `apply_record`.
#[derive(Clone, Debug)]
pub(crate) enum DRec {
    /// A signed head offered to the pin.
    Head(SignedHead),
    /// A run of entries admitted to the copy of a chain (the chain they were offered for).
    Entries(ChainId, Vec<OpaqueEntry>),
    /// A restore record followed.
    Restore(RestoreRecord),
    /// An entry (by its original hash) that was sent again.
    Resent(Hash32),
}

/// What happened to one entry a device applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EntryVerdict {
    Appended,
    AlreadyHeld,
    BeyondPin,
    ForkWithPin(Alarm),
    Chain(ChainRefusal),
    Machine(MachineRefusal),
}

/// An entry of the device's own that it held beyond the survivors of a restore.
#[derive(Clone, Debug)]
pub(crate) struct OwnItem {
    pub(crate) chain: ChainId,
    pub(crate) payload: u16,
    pub(crate) original: Hash32,
}

/// A device.
///
/// Implements planning/architecture.md section 5.9, section 5.13 and section 2.17.
///
/// Caller: `world::World`.
#[derive(Debug)]
pub(crate) struct Device {
    pub(crate) idx: usize,
    pub(crate) node: usize,
    pub(crate) account: usize,
    pub(crate) member: usize,
    pub(crate) key: KeyId,
    pub(crate) up: bool,
    pub(crate) base_epoch: RestoreEpoch,
    pub(crate) pin: HeadPin,
    pub(crate) copies: BTreeMap<ChainId, Chain<OpaqueEntry>>,
    pub(crate) raw: BTreeMap<ChainId, Vec<OpaqueEntry>>,
    pub(crate) replicas: BTreeMap<ChainId, OpaqueReplica>,
    pub(crate) wal: SimDisk<DRec>,
    pub(crate) applied: usize,
    pub(crate) durable: usize,
    pub(crate) resend: Vec<OwnItem>,
    pub(crate) resent: BTreeSet<Hash32>,
    pub(crate) alarms: VecDeque<Alarm>,
    pub(crate) alarm_count: u64,
    pub(crate) fetch_at: BTreeMap<ChainId, u64>,
    pub(crate) fetch_tries: BTreeMap<(ChainId, ChainSeq), u8>,
    pub(crate) disk_flawed: bool,
    pub(crate) crash_allowed: Option<BTreeSet<Hash32>>,
    pub(crate) chain_ids: Vec<ChainId>,
}

impl Device {
    pub(crate) fn new(
        idx: usize,
        account: usize,
        member: usize,
        key: KeyId,
        epoch: RestoreEpoch,
        chains: &[ChainInfo],
    ) -> Device {
        let chain_ids: Vec<ChainId> = chains
            .iter()
            .filter(|c| c.account == account)
            .map(|c| c.id)
            .collect();
        let mut device = Device {
            idx,
            node: idx + 1,
            account,
            member,
            key,
            up: true,
            base_epoch: epoch,
            pin: HeadPin::starting_at(epoch),
            copies: BTreeMap::new(),
            raw: BTreeMap::new(),
            replicas: BTreeMap::new(),
            wal: SimDisk::new(),
            applied: 0,
            durable: 0,
            resend: Vec::new(),
            resent: BTreeSet::new(),
            alarms: VecDeque::new(),
            alarm_count: 0,
            fetch_at: BTreeMap::new(),
            fetch_tries: BTreeMap::new(),
            disk_flawed: false,
            crash_allowed: None,
            chain_ids,
        };
        device.reset_state(epoch);
        device
    }

    /// The volatile state of a device that has applied nothing, standing at `epoch`.
    pub(crate) fn reset_state(&mut self, epoch: RestoreEpoch) {
        self.base_epoch = epoch;
        self.pin = HeadPin::starting_at(epoch);
        // A fresh device verifies each chain from its genesis, which is of an older restore epoch
        // than the one it was born in: only a restore record it follows raises a copy's floor.
        self.copies = self
            .chain_ids
            .iter()
            .map(|id| (*id, Chain::new(*id)))
            .collect();
        self.raw.clear();
        self.replicas = self
            .chain_ids
            .iter()
            .map(|id| {
                (
                    *id,
                    OpaqueReplica {
                        applied: 0,
                        fold: Hash32::ZERO,
                    },
                )
            })
            .collect();
        self.applied = 0;
        self.durable = 0;
        self.resend.clear();
        self.resent.clear();
        self.fetch_at.clear();
        self.fetch_tries.clear();
    }

    /// A hash of everything a log record can change: the pin, the copies, the floor, the entries to
    /// send again.
    pub(crate) fn state_digest(&self) -> Hash32 {
        let mut parts: Vec<u8> = Vec::new();
        parts.extend_from_slice(&self.pin.restore_epoch().get().to_be_bytes());
        for id in &self.chain_ids {
            parts.extend_from_slice(&id.id);
            match self.pin.head(id) {
                Some(head) => {
                    parts.push(1);
                    parts.extend_from_slice(&head.seq.get().to_be_bytes());
                    parts.extend_from_slice(head.hash.as_bytes());
                }
                None => parts.push(0),
            }
            if let Some(signed) = self.pin.signed_head(id) {
                parts.push(1);
                parts.extend_from_slice(&signed.epoch.get().to_be_bytes());
            } else {
                parts.push(0);
            }
            if let Some(copy) = self.copies.get(id) {
                parts.extend_from_slice(&copy.epoch().get().to_be_bytes());
                match copy.head() {
                    Some(head) => {
                        parts.push(1);
                        parts.extend_from_slice(&head.seq.get().to_be_bytes());
                        parts.extend_from_slice(head.hash.as_bytes());
                    }
                    None => parts.push(0),
                }
            }
            parts.extend_from_slice(&self.raw.get(id).map_or(0, Vec::len).to_be_bytes());
        }
        for item in &self.resend {
            parts.extend_from_slice(item.original.as_bytes());
        }
        parts.push(0xFE);
        for hash in &self.resent {
            parts.extend_from_slice(hash.as_bytes());
        }
        hash32("azlin-sim-device-digest-v1", &[&parts])
    }
}

/// What applying a record caused, to be told to the world after the apply.
#[derive(Debug, Default)]
pub(crate) struct Effects {
    alarms: Vec<Alarm>,
    refusals: Vec<(Cause, RefusalClass)>,
    pin_verdicts: Vec<(ChainId, PinVerdict)>,
    pin_moves: Vec<PinMove>,
    restores: Vec<RestoreEvent>,
    lost: Vec<String>,
    held: Vec<HeldBeyond>,
}

/// What `apply_record` needs of the world.
pub(crate) struct Env<'a> {
    pub(crate) keys: &'a Keyring,
    pub(crate) machine: &'a OpaqueMachine,
    pub(crate) defect: Defect,
}

fn head_verifier(keys: &Keyring, defect: Defect) -> SimHeadVerifier<'_> {
    if defect == Defect::VerifierAcceptsAll {
        SimHeadVerifier::accepting_everything(keys)
    } else {
        SimHeadVerifier::new(keys)
    }
}

/// The head verifier a world's devices use (the faulty double of its defect, if any).
pub(crate) fn head_verifier_for(w: &World) -> SimHeadVerifier<'_> {
    head_verifier(&w.keys, w.defect)
}

pub(crate) fn pin_view(pin: &HeadPin, chain: &ChainId) -> Option<PinView> {
    pin.head(chain).map(|head| PinView {
        head,
        epoch: pin.signed_head(chain).map(|signed| signed.epoch),
    })
}

fn pin_views(dev: &Device) -> BTreeMap<ChainId, PinView> {
    dev.chain_ids
        .iter()
        .filter_map(|id| pin_view(&dev.pin, id).map(|view| (*id, view)))
        .collect()
}

fn copy_heads(dev: &Device) -> BTreeMap<ChainId, Option<Head>> {
    dev.chain_ids
        .iter()
        .map(|id| (*id, dev.copies.get(id).and_then(Chain::head)))
        .collect()
}

/// Whether a restore record would be followed by this pin: verification and a dry run.
pub(crate) fn restore_check(
    pin: &HeadPin,
    rec: &RestoreRecord,
    keys: &Keyring,
    defect: Defect,
) -> Result<(), RestoreRefusal> {
    let verifier = SimRestoreVerifier::new(keys);
    let verified = if defect == Defect::GroupAloneTrust {
        verify_record(rec, &GroupAloneTrust::new(&verifier))?
    } else {
        verify_record(rec, &StrictTrust::new(&verifier))?
    };
    let mut scratch = pin.clone();
    verified.apply(&mut scratch).map(|_| ())
}

fn intent_of(payload: &[u8]) -> u16 {
    match (payload.first(), payload.get(1)) {
        (Some(a), Some(b)) => u16::from_be_bytes([*a, *b]),
        _ => 0,
    }
}

fn oracle_head(
    led: &mut Ledger,
    dev: usize,
    h: &SignedHead,
    keys: &Keyring,
    before: Option<PinView>,
    after: Option<PinView>,
    fx: &mut Effects,
) {
    let chain = h.head.chain;
    let floor = led.floor.get(&dev).copied().unwrap_or(RestoreEpoch::ZERO);
    let valid = SimHeadVerifier::new(keys).verify_head(h).is_ok() && h.epoch >= floor;
    if valid {
        let watermark = led.watermark.entry((dev, chain)).or_insert(h.head.seq);
        if h.head.seq > *watermark {
            *watermark = h.head.seq;
        }
    }
    if h.epoch < floor
        && before != after
        && after.is_some_and(|a| a.head == h.head && a.epoch == Some(h.epoch))
    {
        fx.lost.push(format!(
            "device {dev} pinned a head of epoch {} below its floor {} on {chain}",
            h.epoch, floor
        ));
    }
    if let Some(epoch) = after.and_then(|a| a.epoch) {
        let max = led.max_pinned_epoch.entry((dev, chain)).or_insert(epoch);
        if epoch > *max {
            *max = epoch;
        }
    }
}

fn apply_entry(
    dev: &mut Device,
    chain: ChainId,
    e: &OpaqueEntry,
    env: &Env<'_>,
    floor: RestoreEpoch,
    fx: &mut Effects,
) -> EntryVerdict {
    if env.defect == Defect::DeviceAdmitsUnchecked {
        dev.raw.entry(chain).or_default().push(e.clone());
        return EntryVerdict::Appended;
    }
    let pinned = dev.pin.head(&chain);
    // An entry of another chain is the chain crate's to refuse (it names the wrong chain): the pin
    // of THIS chain says nothing of it.
    let allowed = e.chain != chain
        || match pinned {
            None => false,
            Some(p) => e.seq < p.seq || (e.seq == p.seq && e.hash == p.hash),
        };
    if !allowed {
        return match pinned {
            Some(p) if e.seq == p.seq => EntryVerdict::ForkWithPin(Alarm::Fork {
                chain,
                seq: p.seq,
                have: p.hash,
                got: e.hash,
            }),
            Some(_) | None => EntryVerdict::BeyondPin,
        };
    }
    let Some(copy) = dev.copies.get_mut(&chain) else {
        return EntryVerdict::Chain(ChainRefusal::WrongChain);
    };
    match copy.check(e) {
        Err(refusal) => EntryVerdict::Chain(refusal),
        Ok(Placement::Held { .. }) => EntryVerdict::AlreadyHeld,
        Ok(Placement::Next) => {
            let replica = dev.replicas.entry(chain).or_insert_with(|| OpaqueReplica {
                applied: 0,
                fold: Hash32::ZERO,
            });
            let mut next = replica.clone();
            if let Err(refusal) = env.machine.admit_apply(env.keys, &mut next, e) {
                return EntryVerdict::Machine(refusal);
            }
            match copy.admit(e.clone()) {
                Ok(Admitted::Appended { .. }) => {
                    *replica = next;
                    if e.epoch < floor {
                        fx.lost.push(format!(
                            "device {} admitted an entry of epoch {} below its floor {floor} on {chain}",
                            dev.idx, e.epoch
                        ));
                    }
                    EntryVerdict::Appended
                }
                Ok(Admitted::AlreadyHeld { .. }) => EntryVerdict::AlreadyHeld,
                Err(refusal) => EntryVerdict::Chain(refusal),
            }
        }
    }
}

/// Applies one record of the log to a device: the one function the live path and the restart share.
/// The oracle (`ledger`) is told only when it is given (the live path).
pub(crate) fn apply_record(
    dev: &mut Device,
    rec: &DRec,
    env: &Env<'_>,
    ledger: &mut Option<&mut Ledger>,
    fx: &mut Effects,
) {
    match rec {
        DRec::Head(h) => {
            let chain = h.head.chain;
            let before = pin_view(&dev.pin, &chain);
            if env.defect == Defect::PinForgets {
                dev.pin = HeadPin::starting_at(dev.pin.restore_epoch());
            }
            let verdict = {
                let verifier = head_verifier(env.keys, env.defect);
                dev.pin.offer(h.clone(), &verifier)
            };
            let after = pin_view(&dev.pin, &chain);
            if let Some(led) = ledger.as_deref_mut() {
                oracle_head(led, dev.idx, h, env.keys, before, after, fx);
            }
            if before.map(|v| v.head) != after.map(|v| v.head) {
                fx.pin_moves.push(PinMove {
                    dev: dev.idx,
                    chain,
                    before: before.map(|v| v.head),
                    after: after.map(|v| v.head),
                    by_reanchor: false,
                });
            }
            fx.pin_verdicts.push((chain, verdict));
        }
        DRec::Entries(declared, entries) => {
            let floor = ledger
                .as_deref()
                .and_then(|led| led.copy_floor.get(&dev.idx).copied())
                .unwrap_or(RestoreEpoch::ZERO);
            for e in entries {
                apply_entry(dev, *declared, e, env, floor, fx);
            }
        }
        DRec::Resent(hash) => {
            dev.resent.insert(*hash);
            dev.resend.retain(|item| item.original != *hash);
        }
        DRec::Restore(record) => apply_restore(dev, record, env, ledger, fx),
    }
}

fn apply_restore(
    dev: &mut Device,
    record: &RestoreRecord,
    env: &Env<'_>,
    ledger: &mut Option<&mut Ledger>,
    fx: &mut Effects,
) {
    if env.defect == Defect::DeviceIgnoresRestore {
        return;
    }
    let pins_before = pin_views(dev);
    let copies_before = copy_heads(dev);
    let floor_before = dev.pin.restore_epoch();
    let verifier = SimRestoreVerifier::new(env.keys);
    let result = if env.defect == Defect::GroupAloneTrust {
        reanchor(&mut dev.pin, record, &GroupAloneTrust::new(&verifier))
    } else {
        reanchor(&mut dev.pin, record, &StrictTrust::new(&verifier))
    };
    // The oracle's own judgement of the record, independent of the crate: the operator signed these
    // bytes, a signature of the map key verifies, and the epoch is above the device's floor.
    let oracle_valid = ledger.as_deref().is_some_and(|led| {
        let floor = led
            .floor
            .get(&dev.idx)
            .copied()
            .unwrap_or(RestoreEpoch::ZERO);
        record.epoch > floor
            && led.operator_records.contains_key(&record_id(record))
            && record.signatures.iter().any(|s| {
                s.by == RestoreSigner::MapKey && verifier.verify_signature(record, s).is_ok()
            })
    });
    let accepted = match result {
        Ok((_, reanchored)) => {
            // The entries of its own that the device holds beyond the survivors are queued to be
            // sent again, from the ranges the crate computed.
            for moved in &reanchored.moved {
                let Some(range) = moved.resend() else {
                    continue;
                };
                let copy_head = dev
                    .copies
                    .get(&moved.chain)
                    .and_then(Chain::head)
                    .map(|h| h.seq);
                let through = match copy_head {
                    Some(h) if h > range.through => h,
                    Some(_) | None => range.through,
                };
                let own: Vec<OpaqueEntry> = dev
                    .copies
                    .get(&moved.chain)
                    .map(|copy| {
                        copy.entries()
                            .iter()
                            .filter(|e| {
                                e.signer == dev.key && e.seq >= range.from && e.seq <= through
                            })
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                for e in own {
                    if dev.resent.contains(&e.hash)
                        || dev.resend.iter().any(|i| i.original == e.hash)
                    {
                        continue;
                    }
                    fx.held.push(HeldBeyond {
                        dev: dev.idx,
                        chain: moved.chain,
                        payload: intent_of(&e.payload),
                        original: e.hash,
                        epoch: record.epoch,
                    });
                    if env.defect != Defect::ResendDropped {
                        dev.resend.push(OwnItem {
                            chain: moved.chain,
                            payload: intent_of(&e.payload),
                            original: e.hash,
                        });
                    }
                }
            }
            // The chain copies: rebuilt at the survivors, and every floor raised (glue rule G8).
            let resume_epoch = if env.defect == Defect::NoChainFloor {
                RestoreEpoch::ZERO
            } else {
                record.epoch
            };
            let mut skipped = false;
            for moved in &reanchored.moved {
                if env.defect == Defect::ReanchorSkipsChain && !skipped {
                    skipped = true;
                    continue;
                }
                dev.copies.insert(
                    moved.chain,
                    Chain::resume(
                        moved.after,
                        record.at,
                        resume_epoch,
                        Chain::<OpaqueEntry>::DEFAULT_MAX_ENTRIES,
                    ),
                );
                dev.replicas.insert(
                    moved.chain,
                    OpaqueReplica {
                        applied: 0,
                        fold: Hash32::ZERO,
                    },
                );
            }
            for k in &reanchored.kept {
                if env.defect == Defect::LateRecordRebuildsAll {
                    dev.copies.insert(
                        k.chain,
                        Chain::resume(
                            k.listed,
                            record.at,
                            resume_epoch,
                            Chain::<OpaqueEntry>::DEFAULT_MAX_ENTRIES,
                        ),
                    );
                }
            }
            if env.defect != Defect::NoChainFloor {
                for copy in dev.copies.values_mut() {
                    copy.raise_floor(record.epoch);
                }
            }
            true
        }
        Err(refusal) => {
            if refusal.class() == RefusalClass::Alarm {
                if let Some(alarm) = rollback_alarm(&dev.pin, record) {
                    fx.alarms.push(alarm);
                }
                fx.refusals
                    .push((Cause::UnauthorizedRestore, refusal.class()));
            } else {
                fx.refusals.push((Cause::Other, refusal.class()));
            }
            fx.restores.push(RestoreEvent {
                dev: dev.idx,
                record: record.clone(),
                accepted: false,
                oracle_valid,
                pins_before: pins_before.clone(),
                pins_after: pin_views(dev),
                copies_before: copies_before.clone(),
                copies_after: copy_heads(dev),
                floor_before,
                floor_after: dev.pin.restore_epoch(),
            });
            false
        }
    };
    if accepted {
        fx.restores.push(RestoreEvent {
            dev: dev.idx,
            record: record.clone(),
            accepted: true,
            oracle_valid,
            pins_before: pins_before.clone(),
            pins_after: pin_views(dev),
            copies_before,
            copies_after: copy_heads(dev),
            floor_before,
            floor_after: dev.pin.restore_epoch(),
        });
        for (chain, before) in &pins_before {
            let after = pin_view(&dev.pin, chain);
            if after.map(|v| v.head) != Some(before.head) {
                fx.pin_moves.push(PinMove {
                    dev: dev.idx,
                    chain: *chain,
                    before: Some(before.head),
                    after: after.map(|v| v.head),
                    by_reanchor: true,
                });
            }
        }
    }
    if let Some(led) = ledger.as_deref_mut() {
        if oracle_valid {
            led.floor.insert(dev.idx, record.epoch);
            led.copy_floor.insert(dev.idx, record.epoch);
            for head in &record.positions {
                let kept_by_epoch = pins_before
                    .get(&head.chain)
                    .is_some_and(|view| view.epoch.is_some_and(|e| e >= record.epoch));
                if !kept_by_epoch {
                    led.watermark.insert((dev.idx, head.chain), head.seq);
                }
            }
        }
    }
}

impl World {
    // ---------------------------------------------------------------------------------------
    // The log
    // ---------------------------------------------------------------------------------------

    fn dev_log(&mut self, di: usize, rec: DRec) -> Option<usize> {
        let dev = self.devices.get_mut(di)?;
        dev.wal.append(rec).ok()?;
        Some(dev.wal.len().saturating_sub(1))
    }

    fn dev_sync(&mut self, di: usize) {
        let lies = self.rates().lost_fsync.hit(&mut self.rng);
        if let Some(dev) = self.devices.get_mut(di) {
            if dev.wal.fsync(lies).is_ok() {
                dev.durable = dev.wal.len();
            }
        }
    }

    /// Applies every record covered by an fsync that returned, in log order (glue rule G1).
    pub(crate) fn dev_apply_pending(&mut self, di: usize, live: bool) {
        for _ in 0..4096 {
            let (idx, rec) = {
                let Some(dev) = self.devices.get(di) else {
                    return;
                };
                if dev.applied >= dev.durable {
                    return;
                }
                match dev.wal.record_at(dev.applied) {
                    Some(rec) => (dev.applied, rec),
                    None => return,
                }
            };
            let mut fx = Effects::default();
            {
                let World {
                    devices,
                    keys,
                    machine,
                    defect,
                    ledger,
                    ..
                } = &mut *self;
                let Some(dev) = devices.get_mut(di) else {
                    return;
                };
                let env = Env {
                    keys: &*keys,
                    machine: &*machine,
                    defect: *defect,
                };
                let mut led = if live { Some(&mut *ledger) } else { None };
                apply_record(dev, &rec, &env, &mut led, &mut fx);
                dev.applied = idx.saturating_add(1);
            }
            if live {
                self.dev_absorb(di, fx);
            }
        }
    }

    fn dev_absorb(&mut self, di: usize, fx: Effects) {
        let node = di + 1;
        self.delta.touched.insert(node);
        for alarm in fx.alarms {
            self.push_alarm(node, alarm);
        }
        for (cause, class) in fx.refusals {
            self.note_refusal(node, cause, class);
        }
        for (chain, verdict) in fx.pin_verdicts {
            self.note_pin_verdict(node, &verdict, chain);
        }
        self.delta.pin_moves.extend(fx.pin_moves);
        self.delta.restores.extend(fx.restores);
        self.delta.lost_timeline_accepted.extend(fx.lost);
        self.ledger.held_beyond.extend(fx.held);
    }

    // ---------------------------------------------------------------------------------------
    // Fetching and sending again
    // ---------------------------------------------------------------------------------------

    /// Asks the leader for what the device lacks of a chain (at most once in twenty seconds of the
    /// device's own clock).
    pub(crate) fn dev_send_fetch(&mut self, di: usize, chain: ChainId) {
        self.dev_send_fetch_inner(di, chain, false);
    }

    /// The same, without the pause.
    pub(crate) fn dev_send_fetch_forced(&mut self, di: usize, chain: ChainId) {
        self.dev_send_fetch_inner(di, chain, true);
    }

    fn dev_send_fetch_inner(&mut self, di: usize, chain: ChainId, force: bool) {
        let now = self.node_secs(di + 1);
        let Some(dev) = self.devices.get_mut(di) else {
            return;
        };
        if !dev.up {
            return;
        }
        if !force {
            if let Some(last) = dev.fetch_at.get(&chain) {
                if now.saturating_sub(*last) < 20 && now >= *last {
                    return;
                }
            }
        }
        dev.fetch_at.insert(chain, now);
        let after = dev.copies.get(&chain).and_then(Chain::head).map(|h| h.seq);
        let floor = dev.pin.restore_epoch();
        let node = dev.node;
        self.send(
            node,
            0,
            Msg::Fetch {
                chain,
                after,
                floor,
            },
        );
    }

    /// Sends again what the device holds beyond the survivors (glue rule G10): each entry as a new
    /// entry of the new epoch, one proposal at a time for each. The entry is logged as sent again
    /// only when its proposal is over (acknowledged, or refused with a typed reason): a crash before
    /// that leaves it queued, so the restart sends it again. Whether anything was queued.
    pub(crate) fn dev_resend(&mut self, di: usize) -> bool {
        let (items, node) = match self.devices.get(di) {
            Some(dev) if dev.up => (
                dev.resend
                    .iter()
                    .filter(|i| !dev.resent.contains(&i.original))
                    .cloned()
                    .collect::<Vec<OwnItem>>(),
                dev.node,
            ),
            Some(_) | None => return false,
        };
        let mut any = false;
        for item in items {
            let running = self.sched.any_job(node, |j| {
                j.kind == JobKind::DPropose && j.original == Some(item.original)
            });
            if running {
                continue;
            }
            let mut job = Job::new(JobKind::DPropose, node);
            job.chain = Some(item.chain);
            job.payload = item.payload;
            job.original = Some(item.original);
            if self.sched.spawn(job).is_ok() {
                any = true;
            }
        }
        any
    }

    /// The proposal that sent an entry again is over: the entry is logged as sent again.
    fn dev_mark_resent(&mut self, di: usize, original: Hash32) {
        if self.dev_log(di, DRec::Resent(original)).is_some() {
            self.dev_sync(di);
            self.dev_apply_pending(di, true);
        }
    }

    // ---------------------------------------------------------------------------------------
    // D.on_head
    // ---------------------------------------------------------------------------------------

    pub(crate) fn d_on_head_phase(&mut self, job: &mut Job) -> Next {
        let Some(di) = job.node.checked_sub(1) else {
            return Next::Advance;
        };
        match job.phase {
            0 => {
                let Some(head) = job.head.clone() else {
                    job.skip_rest = true;
                    return Next::Advance;
                };
                let chain = head.head.chain;
                let judged = self.devices.get(di).map(|d| {
                    let verifier = head_verifier(&self.keys, self.defect);
                    let mut scratch = d.pin.clone();
                    let verdict = scratch.offer(head.clone(), &verifier);
                    let anchored =
                        d.pin.head(&chain).is_some() && d.pin.signed_head(&chain).is_none();
                    (verdict, anchored, d.pin.restore_epoch())
                });
                let Some((verdict, anchored, floor)) = judged else {
                    job.skip_rest = true;
                    return Next::Advance;
                };
                job.pin_verdict = Some(verdict);
                job.will_log = matches!(verdict, PinVerdict::Advanced)
                    || (matches!(verdict, PinVerdict::Unchanged) && anchored);
                if matches!(verdict, PinVerdict::RefusedOlder { .. }) && job.from == 0 {
                    self.ledger.leader_older_seen.insert((di, chain));
                }
                if !job.will_log {
                    self.note_pin_verdict(job.node, &verdict, chain);
                    job.skip_rest = true;
                }
                if head.epoch > floor {
                    // A head of a restore the device was not told of: fetch the records (G6).
                    self.dev_send_fetch(di, chain);
                }
            }
            1 => {
                if job.will_log && !job.skip_rest {
                    if let Some(head) = job.head.clone() {
                        job.wal_idx = self.dev_log(di, DRec::Head(head));
                    }
                }
            }
            _ => {
                if job.will_log && !job.skip_rest {
                    self.dev_sync(di);
                    self.dev_apply_pending(di, true);
                    if let Some(chain) = job.chain {
                        self.dev_catch_up(di, chain);
                    }
                }
            }
        }
        Next::Advance
    }

    /// A device whose pin is beyond its chain copy fetches what it lacks (glue rule G6).
    fn dev_catch_up(&mut self, di: usize, chain: ChainId) {
        let behind = self.devices.get(di).is_some_and(|d| {
            let pinned = d.pin.head(&chain).map(|h| h.seq);
            let held = d.copies.get(&chain).and_then(Chain::head).map(|h| h.seq);
            match (pinned, held) {
                (Some(p), Some(h)) => p > h,
                (Some(_), None) => true,
                (None, _) => false,
            }
        });
        if behind {
            self.dev_send_fetch(di, chain);
        }
    }

    // ---------------------------------------------------------------------------------------
    // D.on_entries
    // ---------------------------------------------------------------------------------------

    pub(crate) fn d_on_entries_phase(&mut self, job: &mut Job) -> Next {
        let Some(di) = job.node.checked_sub(1) else {
            return Next::Advance;
        };
        match job.phase {
            0 => self.d_entries_check(job, di),
            1 => {
                if job.will_log && !job.skip_rest {
                    if let Some(head) = job.head.take() {
                        job.wal_idx = self.dev_log(di, DRec::Head(head));
                    }
                    if !job.entries.is_empty() {
                        if let Some(chain) = job.chain {
                            job.wal_idx =
                                self.dev_log(di, DRec::Entries(chain, job.entries.clone()));
                        }
                    }
                }
            }
            2 => {
                if job.will_log && !job.skip_rest {
                    self.dev_sync(di);
                    self.dev_apply_pending(di, true);
                }
            }
            _ => {
                if let Some(chain) = job.chain {
                    self.dev_catch_up(di, chain);
                    if job.refused > 0 {
                        // Glue rule G5: a copy that was damaged is fetched again, a bounded number of times.
                        let first = job.entries.first().map_or(ChainSeq::MIN, |e| e.seq);
                        let tries = self
                            .devices
                            .get_mut(di)
                            .map(|d| {
                                let t = d.fetch_tries.entry((chain, first)).or_insert(0);
                                *t = t.saturating_add(1);
                                *t
                            })
                            .unwrap_or(u8::MAX);
                        if tries <= 3 {
                            self.dev_send_fetch_forced(di, chain);
                        }
                    }
                }
            }
        }
        Next::Advance
    }

    fn pin_is_anchored(&self, di: usize, chain: &ChainId) -> bool {
        self.devices
            .get(di)
            .is_some_and(|d| d.pin.head(chain).is_some() && d.pin.signed_head(chain).is_none())
    }

    /// The dry run of what a message of entries and a head would do: nothing is changed.
    fn d_entries_check(&mut self, job: &mut Job, di: usize) {
        let Some(chain) = job.chain else {
            job.skip_rest = true;
            return;
        };
        let head = job.head.clone();
        let mut forks: Vec<Alarm> = Vec::new();
        let mut refusals: Vec<ChainRefusal> = Vec::new();
        let mut admitted: Vec<OpaqueEntry> = Vec::new();
        let mut head_verdict: Option<PinVerdict> = None;
        let mut floor = RestoreEpoch::ZERO;
        {
            let Some(dev) = self.devices.get(di) else {
                job.skip_rest = true;
                return;
            };
            floor = floor.max(dev.pin.restore_epoch());
            let verifier = head_verifier(&self.keys, self.defect);
            let mut scratch_pin = dev.pin.clone();
            if let Some(h) = &head {
                head_verdict = Some(scratch_pin.offer(h.clone(), &verifier));
            }
            let pinned = scratch_pin.head(&chain);
            if let Some(copy) = dev.copies.get(&chain) {
                let mut tip = copy.tip().clone();
                let held_up_to = copy.head().map(|h| h.seq);
                for e in &job.entries {
                    if self.defect == Defect::DeviceAdmitsUnchecked {
                        admitted.push(e.clone());
                        continue;
                    }
                    let allowed = e.chain != chain
                        || pinned
                            .is_some_and(|p| e.seq < p.seq || (e.seq == p.seq && e.hash == p.hash));
                    if !allowed {
                        if let Some(p) = pinned {
                            if e.seq == p.seq {
                                forks.push(Alarm::Fork {
                                    chain,
                                    seq: p.seq,
                                    have: p.hash,
                                    got: e.hash,
                                });
                            }
                        }
                        continue;
                    }
                    let verdict = if held_up_to.is_some_and(|h| e.seq <= h) {
                        copy.check(e)
                    } else {
                        tip.check(e)
                    };
                    match verdict {
                        Ok(Placement::Next) => {
                            if tip.advance(e).is_ok() {
                                admitted.push(e.clone());
                            }
                        }
                        Ok(Placement::Held { .. }) => {}
                        Err(refusal) => {
                            refusals.push(refusal);
                            break;
                        }
                    }
                }
            } else {
                refusals.push(ChainRefusal::WrongChain);
            }
        }
        let node = job.node;
        job.fork_with_pin = !forks.is_empty();
        job.chain_refusals = refusals.clone();
        if matches!(head_verdict, Some(PinVerdict::RefusedOlder { .. })) && job.from == 0 {
            self.ledger.leader_older_seen.insert((di, chain));
        }
        for alarm in forks {
            self.note_refusal(node, Cause::Fork, RefusalClass::Alarm);
            self.push_alarm(node, alarm);
            job.refused += 1;
        }
        for refusal in &refusals {
            self.note_chain_refusal(node, refusal, chain);
            job.refused += 1;
        }
        let head_logged = head_verdict.is_some_and(|v| {
            matches!(v, PinVerdict::Advanced)
                || (matches!(v, PinVerdict::Unchanged) && self.pin_is_anchored(di, &chain))
        });
        if let (Some(v), Some(h)) = (head_verdict, &head) {
            if !head_logged {
                self.note_pin_verdict(node, &v, h.head.chain);
            }
            if h.epoch > floor {
                self.dev_send_fetch(di, chain);
            }
        }
        job.pin_verdict = head_verdict;
        job.entries = admitted;
        job.will_log = head_logged || !job.entries.is_empty();
        if !job.will_log {
            job.skip_rest = true;
        }
        if !head_logged {
            job.head = None;
        }
    }

    // ---------------------------------------------------------------------------------------
    // D.on_restore
    // ---------------------------------------------------------------------------------------

    pub(crate) fn d_on_restore_phase(&mut self, job: &mut Job) -> Next {
        let Some(di) = job.node.checked_sub(1) else {
            return Next::Advance;
        };
        match job.phase {
            0 => self.d_restore_verify(job, di),
            1 => {
                if !job.skip_rest {
                    if let Some(record) = job.record.clone() {
                        job.wal_idx = self.dev_log(di, DRec::Restore(record));
                    }
                }
            }
            2 => {
                if !job.skip_rest {
                    self.dev_sync(di);
                }
            }
            3 => {
                if !job.skip_rest {
                    self.dev_apply_pending(di, true);
                }
            }
            4 => {
                if !job.skip_rest {
                    for chain in self
                        .devices
                        .get(di)
                        .map(|d| d.chain_ids.clone())
                        .unwrap_or_default()
                    {
                        self.dev_send_fetch(di, chain);
                    }
                }
            }
            _ => {
                if !job.skip_rest {
                    self.dev_resend(di);
                }
            }
        }
        Next::Advance
    }

    fn d_restore_verify(&mut self, job: &mut Job, di: usize) {
        let Some(record) = job.record.clone() else {
            job.skip_rest = true;
            return;
        };
        let Some(dev) = self.devices.get(di) else {
            job.skip_rest = true;
            return;
        };
        if self.defect == Defect::DeviceIgnoresRestore {
            job.will_log = true;
            return;
        }
        match restore_check(&dev.pin, &record, &self.keys, self.defect) {
            Ok(()) => job.will_log = true,
            Err(refusal) => {
                // Refused before anything was written: told to the world at once.
                let mut fx = Effects::default();
                let pins = pin_views(dev);
                let copies = copy_heads(dev);
                let floor = dev.pin.restore_epoch();
                if refusal.class() == RefusalClass::Alarm {
                    if let Some(alarm) = rollback_alarm(&dev.pin, &record) {
                        fx.alarms.push(alarm);
                    }
                    fx.refusals
                        .push((Cause::UnauthorizedRestore, refusal.class()));
                } else {
                    fx.refusals.push((Cause::Other, refusal.class()));
                }
                let oracle_valid = {
                    let verifier = SimRestoreVerifier::new(&self.keys);
                    let led_floor = self
                        .ledger
                        .floor
                        .get(&di)
                        .copied()
                        .unwrap_or(RestoreEpoch::ZERO);
                    record.epoch > led_floor
                        && self
                            .ledger
                            .operator_records
                            .contains_key(&record_id(&record))
                        && record.signatures.iter().any(|s| {
                            s.by == RestoreSigner::MapKey
                                && verifier.verify_signature(&record, s).is_ok()
                        })
                };
                fx.restores.push(RestoreEvent {
                    dev: di,
                    record,
                    accepted: false,
                    oracle_valid,
                    pins_before: pins.clone(),
                    pins_after: pins,
                    copies_before: copies.clone(),
                    copies_after: copies,
                    floor_before: floor,
                    floor_after: floor,
                });
                job.restore_refusal = Some(refusal);
                job.skip_rest = true;
                self.dev_absorb(di, fx);
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // D.propose
    // ---------------------------------------------------------------------------------------

    pub(crate) fn d_propose_phase(&mut self, job: &mut Job) -> Next {
        let Some(di) = job.node.checked_sub(1) else {
            return Next::Advance;
        };
        match job.phase {
            0 => {
                job.waiting = false;
                job.skip_rest = false;
                let (Some(chain), Some(dev)) = (job.chain, self.devices.get(di)) else {
                    job.skip_rest = true;
                    return Next::Advance;
                };
                let tip = dev.copies.get(&chain).and_then(Chain::head);
                match self
                    .machine
                    .propose(&self.keys, di, chain, tip, job.payload)
                {
                    Some(proposal) => job.proposal = Some(proposal),
                    None => job.skip_rest = true,
                }
                Next::Advance
            }
            1 => {
                if job.skip_rest {
                    return Next::Advance;
                }
                if let Some(proposal) = job.proposal.clone() {
                    self.send(job.node, 0, Msg::Propose(proposal));
                    job.waiting = true;
                    job.wake_at = self.node_secs(job.node).saturating_add(60);
                    job.attempts = job.attempts.saturating_add(1);
                }
                Next::Advance
            }
            2 => {
                job.waiting = false;
                match job.answer.take() {
                    Some(AckResult::Acked(_)) | Some(AckResult::Held(_)) => job.finished_ok = true,
                    Some(AckResult::Refused(refusal)) => {
                        job.resend_same = matches!(refusal, Refusal::Corrupt);
                        if matches!(refusal, Refusal::Machine(_) | Refusal::Chain(_)) {
                            // A typed refusal that no retry can change: the proposal is given up.
                            if let Some(original) = job.original {
                                self.ledger
                                    .resend_refusals
                                    .push((original, format!("refused: {refusal:?}")));
                            }
                            job.finished_ok = true;
                        }
                    }
                    None => job.resend_same = true,
                }
                Next::Advance
            }
            _ => {
                if job.skip_rest || job.finished_ok {
                    if let Some(original) = job.original {
                        if !job.finished_ok {
                            self.ledger
                                .resend_refusals
                                .push((original, String::from("nothing to propose")));
                        }
                        self.dev_mark_resent(di, original);
                    }
                    return Next::Advance;
                }
                if job.attempts >= 5 {
                    // Glue rule G4: at most five attempts; then the proposal is given up.
                    self.node_refused = self.node_refused.saturating_add(1);
                    if let Some(original) = job.original {
                        self.ledger
                            .resend_refusals
                            .push((original, String::from("gave up after five attempts")));
                        self.dev_mark_resent(di, original);
                    }
                    job.finished_ok = true;
                    return Next::Advance;
                }
                let wait = (1u64 << job.attempts.min(4)) + below(&mut self.rng, 3);
                job.waiting = true;
                job.wake_at = self.node_secs(job.node).saturating_add(wait);
                if job.resend_same {
                    return Next::Goto(1);
                }
                if let Some(chain) = job.chain {
                    self.dev_send_fetch_forced(di, chain);
                }
                job.proposal = None;
                Next::Goto(0)
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // D.meet
    // ---------------------------------------------------------------------------------------

    pub(crate) fn d_meet_phase(&mut self, job: &mut Job) -> Next {
        let Some(di) = job.node.checked_sub(1) else {
            return Next::Advance;
        };
        match job.phase {
            0 => {
                let peer = job.peer.checked_sub(1);
                let heads = peer
                    .and_then(|p| self.devices.get(p))
                    .filter(|p| p.up && !self.net.is_cut(p.node, job.node))
                    .map(|p| p.pin.signed_heads().cloned().collect::<Vec<SignedHead>>());
                match heads {
                    Some(heads) => job.heads = heads,
                    None => job.skip_rest = true,
                }
            }
            1 => {
                if job.skip_rest {
                    return Next::Advance;
                }
                let mut cmps = Vec::new();
                if let Some(dev) = self.devices.get(di) {
                    let verifier = head_verifier(&self.keys, self.defect);
                    let strict = SimHeadVerifier::new(&self.keys);
                    let floor = self
                        .ledger
                        .floor
                        .get(&di)
                        .copied()
                        .unwrap_or(RestoreEpoch::ZERO);
                    for head in &job.heads {
                        let verdict = compare_peer(&dev.pin, head, &verifier);
                        let ours = pin_view(&dev.pin, &head.head.chain);
                        cmps.push(PeerCmp {
                            head: head.clone(),
                            verdict,
                            ours,
                            valid: strict.verify_head(head).is_ok(),
                            standing: floor
                                .max(ours.and_then(|v| v.epoch).unwrap_or(RestoreEpoch::ZERO)),
                        });
                    }
                }
                job.peer_cmps = cmps;
            }
            _ => {
                if job.skip_rest {
                    return Next::Advance;
                }
                for cmp in std::mem::take(&mut job.peer_cmps) {
                    let chain = cmp.head.head.chain;
                    let mut alarm = cmp.verdict.alarm().copied();
                    if self.defect == Defect::MeetDropsAlarms {
                        alarm = None;
                    }
                    if self.defect == Defect::AlarmOnJunk
                        && matches!(cmp.verdict, azlin_chain::PeerVerdict::BadSignature)
                    {
                        alarm = Some(Alarm::UnsignedHead { chain });
                    }
                    if let Some(alarm) = alarm {
                        self.push_alarm(job.node, alarm);
                        if matches!(cmp.verdict, azlin_chain::PeerVerdict::Fork(_)) {
                            self.note_refusal(job.node, Cause::Fork, RefusalClass::Alarm);
                        }
                        if matches!(cmp.verdict, azlin_chain::PeerVerdict::PeerAhead(_)) {
                            // Glue rule G6: a withheld head makes the device ask for what it lacks.
                            self.dev_send_fetch(di, chain);
                        }
                    }
                    self.delta.meets.push(MeetEvent {
                        dev: di,
                        head: cmp.head,
                        verdict: cmp.verdict,
                        ours: cmp.ours,
                        standing: cmp.standing,
                        alarmed: alarm.is_some(),
                        valid: cmp.valid,
                    });
                }
            }
        }
        Next::Advance
    }

    // ---------------------------------------------------------------------------------------
    // Crash and restart
    // ---------------------------------------------------------------------------------------

    /// Replays records into a fresh state of the device; the digest after each prefix is collected.
    fn replay_device(
        &self,
        di: usize,
        records: &[DRec],
        scratch: &mut Option<&mut Ledger>,
        digests: &mut Vec<Hash32>,
        base: RestoreEpoch,
    ) -> Option<(Device, Vec<HeldBeyond>)> {
        let source = self.devices.get(di)?;
        let mut fresh = Device::new(
            source.idx,
            source.account,
            source.member,
            source.key,
            base,
            &self.chains,
        );
        let env = Env {
            keys: &self.keys,
            machine: &self.machine,
            defect: self.defect,
        };
        let mut held = Vec::new();
        digests.push(fresh.state_digest());
        for rec in records {
            let mut fx = Effects::default();
            apply_record(&mut fresh, rec, &env, scratch, &mut fx);
            held.extend(fx.held);
            digests.push(fresh.state_digest());
        }
        Some((fresh, held))
    }

    /// A device crashes: the digests it may recover are computed from what its log holds now, then
    /// the disk applies its crash rule and the volatile state is gone.
    pub(crate) fn dev_crash(&mut self, di: usize, rng: &mut Rng, tear: crate::config::Ppm) {
        let (records, synced, flawed_before, base) = match self.devices.get(di) {
            Some(d) => (
                d.wal.peek(),
                d.wal.synced_len(),
                d.disk_flawed || d.wal.stats().lies > 0,
                d.base_epoch,
            ),
            None => return,
        };
        let mut digests = Vec::new();
        let mut scratch_ledger = Ledger {
            operator_records: self.ledger.operator_records.clone(),
            ..Ledger::default()
        };
        scratch_ledger.floor.insert(di, base);
        let mut scratch = Some(&mut scratch_ledger);
        let _ = self.replay_device(di, &records, &mut scratch, &mut digests, base);
        let lo = if flawed_before {
            0
        } else {
            synced.min(records.len())
        };
        let allowed: BTreeSet<Hash32> = digests.iter().skip(lo).copied().collect();
        if let Some(dev) = self.devices.get_mut(di) {
            let report = dev.wal.crash(rng, tear);
            if report.lost_by_lie > 0 {
                dev.disk_flawed = true;
            }
            dev.up = false;
            dev.crash_allowed = Some(allowed);
        }
    }

    /// Restarts a device: from its log (glue rule G2), or as a new device that starts at the
    /// leader's current restore epoch (glue rule G14). Whether it is up.
    pub(crate) fn restart_device(&mut self, di: usize, wipe: bool) -> bool {
        let leader_epoch = self.leader.epoch;
        let (records, wiped) = match self.devices.get_mut(di) {
            None => return false,
            Some(dev) => {
                if wipe {
                    dev.wal.reset();
                    (Vec::new(), true)
                } else {
                    let prefix = dev.wal.valid_prefix();
                    match prefix.cut {
                        Some(DiskError::Read) => return false,
                        Some(DiskError::Gone) => {
                            dev.wal.reset();
                            (Vec::new(), true)
                        }
                        Some(DiskError::Corrupt { .. }) => {
                            let _ = dev.wal.replace(prefix.records.clone(), false);
                            (prefix.records, false)
                        }
                        Some(DiskError::Full) | None => (prefix.records, false),
                    }
                }
            }
        };
        let records = if self.defect == Defect::RecoveryReorders {
            records.into_iter().rev().collect()
        } else {
            records
        };
        let base = if wiped {
            leader_epoch
        } else {
            self.devices
                .get(di)
                .map_or(RestoreEpoch::ZERO, |d| d.base_epoch)
        };
        let mut scratch_ledger = Ledger {
            operator_records: self.ledger.operator_records.clone(),
            ..Ledger::default()
        };
        scratch_ledger.floor.insert(di, base);
        let mut digests = Vec::new();
        let replayed = {
            let mut scratch = Some(&mut scratch_ledger);
            self.replay_device(di, &records, &mut scratch, &mut digests, base)
        };
        let Some((fresh, held)) = replayed else {
            return false;
        };
        let recovered = digests.last().copied().unwrap_or(Hash32::ZERO);
        let allowed = self
            .devices
            .get_mut(di)
            .and_then(|d| d.crash_allowed.take())
            .unwrap_or_default();
        // The replayed state replaces the device's state; the identity and the disk stay.
        if let Some(dev) = self.devices.get_mut(di) {
            dev.base_epoch = base;
            dev.pin = fresh.pin;
            dev.copies = fresh.copies;
            dev.raw = fresh.raw;
            dev.replicas = fresh.replicas;
            dev.resend = fresh.resend;
            dev.resent = fresh.resent;
            dev.applied = records.len();
            dev.durable = records.len();
            dev.fetch_at.clear();
            dev.fetch_tries.clear();
            dev.up = true;
        }
        // The oracle's per-device entries follow the recovered state.
        self.ledger.watermark.retain(|(dev, _), _| *dev != di);
        self.ledger
            .max_pinned_epoch
            .retain(|(dev, _), _| *dev != di);
        for ((dev, chain), seq) in scratch_ledger.watermark {
            if dev == di {
                self.ledger.watermark.insert((dev, chain), seq);
            }
        }
        for ((dev, chain), epoch) in scratch_ledger.max_pinned_epoch {
            if dev == di {
                self.ledger.max_pinned_epoch.insert((dev, chain), epoch);
            }
        }
        self.ledger
            .floor
            .insert(di, scratch_ledger.floor.get(&di).copied().unwrap_or(base));
        self.ledger.copy_floor.insert(
            di,
            scratch_ledger
                .copy_floor
                .get(&di)
                .copied()
                .unwrap_or(RestoreEpoch::ZERO),
        );
        self.ledger.held_beyond.retain(|h| h.dev != di);
        self.ledger.held_beyond.extend(held);
        self.delta.device_restarts.push(DeviceRestart {
            dev: di,
            recovered,
            allowed,
            wiped,
        });
        true
    }
}
