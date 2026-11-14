//! The world and the step interpreter (planning/architecture.md section 1.4 rules 1 and 2, section
//! 12 "the deterministic simulation").
//!
//! One metadata group (the LEADER, node 0), devices (nodes 1..=D), a seeded network, a clock for
//! each node and for the two follower voters, a disk for each node, and the keys of the keyring.
//! [`World::step`] executes one step; the checkers of `invariants` read the world and the step's
//! DELTA (the events the step caused) after every step. The world holds the ORACLES the checkers
//! compare with: small, obviously right ledgers of what the leader signed, acknowledged and
//! announced, and of what each device has been offered and accepted.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use azlin_chain::{
    Alarm, ChainId, ChainRefusal, Head, PinVerdict, RefusalClass, RestoreRecord, SignedHead,
};
use azlin_seams::sim::Rng;
use azlin_seams::{hash32, AccountId, ChainSeq, Hash32, RestoreEpoch, SpaceId, Stamp};

use crate::config::{
    Config, Defect, FaultRates, LeaderVariant, DRAIN_ROUNDS, MAX_ALARMS, MAX_RESTORES,
    MAX_TRACE_TAIL, QUIESCE_ROUNDS,
};
use crate::coverage::{Coverage, FaultKind, Footprint, OpKind};
use crate::device::{DRec, Device};
use crate::draw::{below, step_rng};
use crate::keys::Keyring;
use crate::leader::{replay_leader, LRec, Leader};
use crate::machine::{Machine, MachineRefusal, OpaqueEntry, OpaqueMachine, Proposal};
use crate::net::{Corrupt, SimNet};
use crate::policy_world::PolicyWorld;
use crate::sched::{Job, JobKind, Sched};
use crate::step::{AttackKind, DiskFault, SigChoice, Step, StepKind};
use crate::time::{Shift, SimClock, StampRefusal, Stamper, SIM_START};

/// Why a node refused something, as a typed value with a class.
///
/// Implements planning/architecture.md section 1.4 rule 10 (every error has one class).
///
/// Caller: `AckResult`, `leader`, `device`, `invariants`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The chain crate refused the entry.
    Chain(ChainRefusal),
    /// The proposal is stale: its position or its predecessor is not the leader's tip (retry).
    Stale(ChainRefusal),
    /// The machine refused the entry.
    Machine(MachineRefusal),
    /// The leader could not stamp.
    Stamp(StampRefusal),
    /// The copy of the proposal was damaged.
    Corrupt,
    /// The leader is busy (restoring) or its disk failed: try again later.
    Busy,
    /// The leader's disk lost data: it serves nothing until a restore.
    Faulted,
}

/// The leader's answer to a proposal.
///
/// Implements planning/architecture.md section 5.9 (the group rejects an entry with a reason).
///
/// Caller: `Msg::Ack`, `leader`, `device`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AckResult {
    /// The entry was appended; the head after it.
    Acked(Head),
    /// The leader already held this proposal; the head of its chain.
    Held(Head),
    /// The leader refused.
    Refused(Refusal),
}

/// What travels between the nodes.
///
/// Implements planning/architecture.md section 5.9 and section 5.13 (heads, entries, restore
/// announcements).
///
/// Caller: `net::SimNet`, `leader`, `device`.
#[derive(Clone, Debug)]
pub(crate) enum Msg {
    /// A device proposes an entry.
    Propose(Proposal),
    /// The leader answers a proposal.
    Ack {
        /// The id of the proposal.
        id: Hash32,
        /// The answer.
        result: AckResult,
    },
    /// Entries of a chain, with a signed head over them, and restore records the device lacks.
    Entries {
        /// The chain.
        chain: ChainId,
        /// The entries, in order.
        entries: Vec<OpaqueEntry>,
        /// A head signed over the newest of them.
        head: Option<SignedHead>,
        /// Restore records the device has not been announced.
        records: Vec<RestoreRecord>,
    },
    /// A signed head.
    Head(SignedHead),
    /// A restore announcement.
    Restore(RestoreRecord),
    /// A device asks for what it lacks.
    Fetch {
        /// The chain.
        chain: ChainId,
        /// The position the device holds; the leader sends what follows.
        after: Option<ChainSeq>,
        /// The restore epoch the device stands in.
        floor: RestoreEpoch,
    },
}

impl Corrupt for Msg {
    fn corrupt(&self, rng: &mut Rng) -> Msg {
        match self {
            Msg::Propose(p) => Msg::Propose(p.corrupted(rng)),
            Msg::Entries {
                chain,
                entries,
                head,
                records,
            } => {
                let mut entries = entries.clone();
                let mut head = head.clone();
                if let Some(first) = entries.first_mut() {
                    *first = first.corrupted(rng);
                } else if let Some(h) = head.as_mut() {
                    flip_sig(&mut h.sig, rng);
                }
                Msg::Entries {
                    chain: *chain,
                    entries,
                    head,
                    records: records.clone(),
                }
            }
            Msg::Head(h) => {
                let mut h = h.clone();
                flip_sig(&mut h.sig, rng);
                Msg::Head(h)
            }
            Msg::Restore(r) => {
                let mut r = r.clone();
                if let Some(first) = r.signatures.first_mut() {
                    flip_sig(&mut first.sig, rng);
                }
                Msg::Restore(r)
            }
            Msg::Ack { .. } | Msg::Fetch { .. } => self.clone(),
        }
    }
}

fn flip_sig(sig: &mut [u8], rng: &mut Rng) {
    let index = below(rng, sig.len() as u64) as usize;
    if let Some(byte) = sig.get_mut(index) {
        *byte ^= 0x5A;
    }
}

/// One chain of the world: its id and the account it belongs to.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChainInfo {
    pub(crate) id: ChainId,
    pub(crate) account: usize,
}

/// A pin of one chain, as the checkers see it: the pinned head and the epoch of its signed head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PinView {
    pub(crate) head: Head,
    pub(crate) epoch: Option<RestoreEpoch>,
}

/// How a restore record fared at one device.
#[derive(Clone, Debug)]
pub(crate) struct RestoreEvent {
    pub(crate) dev: usize,
    pub(crate) record: RestoreRecord,
    pub(crate) accepted: bool,
    pub(crate) oracle_valid: bool,
    pub(crate) pins_before: BTreeMap<ChainId, PinView>,
    pub(crate) pins_after: BTreeMap<ChainId, PinView>,
    pub(crate) copies_before: BTreeMap<ChainId, Option<Head>>,
    pub(crate) copies_after: BTreeMap<ChainId, Option<Head>>,
    pub(crate) floor_before: RestoreEpoch,
    pub(crate) floor_after: RestoreEpoch,
}

/// A comparison of a peer's head with a device's pin.
#[derive(Clone, Debug)]
pub(crate) struct MeetEvent {
    pub(crate) dev: usize,
    pub(crate) head: SignedHead,
    pub(crate) verdict: azlin_chain::PeerVerdict,
    pub(crate) ours: Option<PinView>,
    pub(crate) standing: RestoreEpoch,
    pub(crate) alarmed: bool,
    pub(crate) valid: bool,
}

/// What the adversary tried and what it met.
#[derive(Clone, Debug)]
pub(crate) struct AttackEvent {
    pub(crate) kind: AttackKind,
    pub(crate) dev: usize,
    pub(crate) observed: Observed,
    pub(crate) state_changed: bool,
    pub(crate) expectation: crate::adversary::Expect,
}

/// The verdict a hostile input met.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Observed {
    /// A refusal of this class; whether an alarm was raised.
    Refused {
        class: RefusalClass,
        alarm: bool,
        what: String,
    },
    /// Taken: the state changed or no refusal was said.
    Accepted(String),
    /// The input was ignored quietly (a junk head from a peer).
    Quiet(String),
}

/// A refusal the world observed: its cause (read from the typed value) and the class it reported.
#[derive(Clone, Debug)]
pub(crate) struct RefusalEvent {
    pub(crate) node: usize,
    pub(crate) cause: Cause,
    pub(crate) reported: RefusalClass,
}

/// The cause of a refusal as the harness reads it from the typed value, which decides the class
/// the plan demands (1.4 rule 10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    /// A fork or a second genesis.
    Fork,
    /// A head or entry of the lost timeline.
    LostTimeline,
    /// A restore record without the authority.
    UnauthorizedRestore,
    /// A copy whose hash or signature failed.
    Corrupt,
    /// Anything else.
    Other,
}

impl Cause {
    /// The class the plan demands for this cause.
    pub(crate) fn demanded(self) -> RefusalClass {
        match self {
            Cause::Fork | Cause::LostTimeline | Cause::UnauthorizedRestore => RefusalClass::Alarm,
            Cause::Corrupt => RefusalClass::Refetch,
            Cause::Other => RefusalClass::Refuse,
        }
    }
}

/// What a step caused (the checkers read it after the step).
#[derive(Clone, Debug, Default)]
pub(crate) struct Delta {
    pub(crate) touched: BTreeSet<usize>,
    pub(crate) pin_moves: Vec<PinMove>,
    pub(crate) restores: Vec<RestoreEvent>,
    pub(crate) meets: Vec<MeetEvent>,
    pub(crate) attacks: Vec<AttackEvent>,
    pub(crate) refusals: Vec<RefusalEvent>,
    pub(crate) heads_signed: Vec<(ChainId, ChainSeq, Hash32, RestoreEpoch)>,
    pub(crate) heads_sent: Vec<(ChainId, ChainSeq, Hash32, RestoreEpoch)>,
    pub(crate) stamps: Vec<StampEvent>,
    pub(crate) acks: Vec<(ChainId, ChainSeq, Hash32)>,
    pub(crate) leader_restarted: bool,
    pub(crate) device_restarts: Vec<DeviceRestart>,
    pub(crate) leader_restore: Option<(RestoreEpoch, RestoreEpoch)>,
    pub(crate) lost_timeline_accepted: Vec<String>,
    pub(crate) notes: Vec<String>,
    pub(crate) policy: Vec<crate::policy_world::PolicyEvent>,
}

/// What the leader's stamps did in a step, in the order it happened: a stamp it gave an entry, and
/// the points where what it holds changed under it (a restart, a restore), with the stamp of the
/// newest entry of the chain at that point.
#[derive(Clone, Copy, Debug)]
pub(crate) enum StampEvent {
    Stamped {
        chain: ChainId,
        seq: ChainSeq,
        at: Stamp,
    },
    Reset {
        chain: ChainId,
        at: Option<Stamp>,
    },
}

/// A device's pin of one chain moved.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PinMove {
    pub(crate) dev: usize,
    pub(crate) chain: ChainId,
    pub(crate) before: Option<Head>,
    pub(crate) after: Option<Head>,
    pub(crate) by_reanchor: bool,
}

/// A device was restarted: the digest it recovered and the digests it may have recovered.
#[derive(Clone, Debug)]
pub(crate) struct DeviceRestart {
    pub(crate) dev: usize,
    pub(crate) recovered: Hash32,
    pub(crate) allowed: BTreeSet<Hash32>,
    pub(crate) wiped: bool,
}

/// What the world remembers independently of the crates.
#[derive(Clone, Debug)]
pub(crate) struct Ledger {
    pub(crate) signed: BTreeSet<(ChainId, ChainSeq, Hash32)>,
    pub(crate) honest_heads: BTreeMap<(ChainId, ChainSeq, RestoreEpoch), Hash32>,
    pub(crate) head_log: VecDeque<SignedHead>,
    pub(crate) entry_log: VecDeque<OpaqueEntry>,
    pub(crate) operator_records: BTreeMap<Hash32, RestoreRecord>,
    pub(crate) record_log: Vec<RestoreRecord>,
    pub(crate) record_positions: BTreeSet<(ChainId, ChainSeq, Hash32)>,
    pub(crate) acked: BTreeMap<(ChainId, ChainSeq), (Hash32, RestoreEpoch)>,
    pub(crate) watermark: BTreeMap<(usize, ChainId), ChainSeq>,
    pub(crate) max_pinned_epoch: BTreeMap<(usize, ChainId), RestoreEpoch>,
    pub(crate) floor: BTreeMap<usize, RestoreEpoch>,
    /// The epoch of the newest restore record a device followed (the floor its chain copies must
    /// have: a device born after a restore verifies the chain from its genesis and has followed none).
    pub(crate) copy_floor: BTreeMap<usize, RestoreEpoch>,
    pub(crate) held_beyond: Vec<HeldBeyond>,
    pub(crate) served: BTreeMap<ChainId, (RestoreEpoch, ChainSeq)>,
    pub(crate) announced_epoch: RestoreEpoch,
    pub(crate) last_stamp: BTreeMap<ChainId, Stamp>,
    pub(crate) degraded: bool,
    pub(crate) lies_told: u64,
    pub(crate) losses: Vec<String>,
    pub(crate) restores: usize,
    pub(crate) restore_refused_exhausted: u64,
    pub(crate) resend_refusals: Vec<(Hash32, String)>,
    pub(crate) first_restore_step: Option<u32>,
    pub(crate) leader_older_seen: BTreeSet<(usize, ChainId)>,
}

impl Default for Ledger {
    fn default() -> Ledger {
        Ledger {
            signed: BTreeSet::new(),
            honest_heads: BTreeMap::new(),
            head_log: VecDeque::new(),
            entry_log: VecDeque::new(),
            operator_records: BTreeMap::new(),
            record_log: Vec::new(),
            record_positions: BTreeSet::new(),
            acked: BTreeMap::new(),
            watermark: BTreeMap::new(),
            max_pinned_epoch: BTreeMap::new(),
            floor: BTreeMap::new(),
            copy_floor: BTreeMap::new(),
            held_beyond: Vec::new(),
            served: BTreeMap::new(),
            announced_epoch: RestoreEpoch::ZERO,
            last_stamp: BTreeMap::new(),
            degraded: false,
            lies_told: 0,
            losses: Vec::new(),
            restores: 0,
            restore_refused_exhausted: 0,
            resend_refusals: Vec::new(),
            first_restore_step: None,
            leader_older_seen: BTreeSet::new(),
        }
    }
}

/// An entry a device held beyond the survivors of a restore and must send again.
#[derive(Clone, Debug)]
pub(crate) struct HeldBeyond {
    pub(crate) dev: usize,
    pub(crate) chain: ChainId,
    pub(crate) payload: u16,
    pub(crate) original: Hash32,
    pub(crate) epoch: RestoreEpoch,
}

/// The trace: the last lines in full, the rest as a rolling hash, and a second hash of the
/// DECISIONS only (no hash of a signed value is in it, so a fake and a real signature scheme give
/// the same one).
#[derive(Clone, Debug)]
pub(crate) struct Trace {
    pub(crate) tail: VecDeque<String>,
    pub(crate) rolling: Hash32,
    pub(crate) decisions: Hash32,
}

impl Trace {
    fn new() -> Trace {
        Trace {
            tail: VecDeque::new(),
            rolling: Hash32::ZERO,
            decisions: Hash32::ZERO,
        }
    }

    /// Adds a line (a fact that may contain hashes).
    pub(crate) fn push(&mut self, line: String) {
        self.rolling = hash32(
            "azlin-sim-trace-v1",
            &[self.rolling.as_bytes(), line.as_bytes()],
        );
        if self.tail.len() >= MAX_TRACE_TAIL {
            self.tail.pop_front();
        }
        self.tail.push_back(line);
    }

    /// Adds a decision token (no signed value in it).
    pub(crate) fn decide(&mut self, token: &str) {
        self.decisions = hash32(
            "azlin-sim-decision-v1",
            &[self.decisions.as_bytes(), token.as_bytes()],
        );
    }
}

/// How a phase function wants the job to go on.
pub(crate) enum Next {
    /// Run the next phase next time.
    Advance,
    /// Go to this phase.
    Goto(u8),
}

/// The world.
///
/// Implements planning/architecture.md section 1.4 rules 1 and 2 and section 12.
///
/// Caller: `scenario::Scenario`.
pub struct World {
    pub(crate) seed: u64,
    pub(crate) cfg: Config,
    pub(crate) defect: Defect,
    pub(crate) keys: Keyring,
    pub(crate) machine: OpaqueMachine,
    pub(crate) clocks: SimClock,
    pub(crate) net: SimNet<Msg>,
    pub(crate) sched: Sched,
    pub(crate) leader: Leader,
    pub(crate) devices: Vec<Device>,
    pub(crate) chains: Vec<ChainInfo>,
    pub(crate) ledger: Ledger,
    pub(crate) delta: Delta,
    pub(crate) trace: Trace,
    pub(crate) cov: Coverage,
    pub(crate) rng: Rng,
    pub(crate) step_no: u32,
    pub(crate) last_kind: Option<StepKind>,
    pub(crate) harness_error: Option<String>,
    pub(crate) quiet: bool,
    pub(crate) policy: PolicyWorld,
    pub(crate) node_refused: u64,
    pub(crate) msgs_to_dead: u64,
}

/// A chain in a few characters, for the state dump.
fn short_chain(id: &ChainId) -> String {
    let hex: String = id.id.iter().take(3).map(|b| format!("{b:02x}")).collect();
    format!("{}:{hex}", id.kind.as_str())
}

/// A hash in a few characters, for the state dump.
fn short_hash(hash: &Hash32) -> String {
    hash.as_bytes()
        .iter()
        .take(4)
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn id16(seed: u64, kind: u8, a: u64, b: u64) -> [u8; 16] {
    let digest = hash32(
        "azlin-sim-id-v1",
        &[
            &seed.to_le_bytes(),
            &[kind],
            &a.to_le_bytes(),
            &b.to_le_bytes(),
        ],
    );
    let mut out = [0u8; 16];
    for (dst, src) in out.iter_mut().zip(digest.as_bytes().iter()) {
        *dst = *src;
    }
    out
}

impl World {
    /// A world at the genesis of every chain: the leader holds one genesis entry on each chain, every
    /// device holds the same and has pinned the head of it, every log is synced.
    pub fn new(seed: u64, cfg: Config, defect: Defect) -> World {
        let devices_total = cfg.device_count();
        let accounts = usize::from(cfg.accounts);
        let members = usize::from(cfg.members);
        let per_member = usize::from(cfg.devices);
        let keys = Keyring::new(seed, cfg.scheme, devices_total);
        // The chains: the team log of each account, then its spaces.
        let mut chains = Vec::new();
        for account in 0..accounts {
            chains.push(ChainInfo {
                id: ChainId::team_log(AccountId::from_bytes(id16(seed, 1, account as u64, 0))),
                account,
            });
            for space in 0..usize::from(cfg.spaces) {
                chains.push(ChainInfo {
                    id: ChainId::space(SpaceId::from_bytes(id16(
                        seed,
                        2,
                        account as u64,
                        space as u64,
                    ))),
                    account,
                });
            }
        }
        // The machine: which account each chain belongs to and which devices are enrolled.
        let account_of_chain: BTreeMap<ChainId, usize> =
            chains.iter().map(|c| (c.id, c.account)).collect();
        let mut enrolled: Vec<BTreeSet<azlin_seams::KeyId>> = vec![BTreeSet::new(); accounts];
        for index in 0..devices_total {
            let account = index / (members * per_member).max(1);
            if let (Some(set), Some(key)) = (enrolled.get_mut(account), keys.device_key(index)) {
                set.insert(key);
            }
        }
        let machine = OpaqueMachine::new(account_of_chain, enrolled);
        let start_epoch = RestoreEpoch::new(cfg.start_epoch);
        let net = if defect == Defect::UnboundedNet {
            SimNet::with_limit(usize::MAX / 2)
        } else {
            SimNet::new()
        };
        let mut world = World {
            seed,
            leader: Leader::new(cfg.leader, start_epoch),
            devices: Vec::new(),
            clocks: SimClock::new(devices_total + 3),
            net,
            sched: Sched::new(),
            chains,
            ledger: Ledger::default(),
            delta: Delta::default(),
            trace: Trace::new(),
            cov: Coverage::default(),
            rng: Rng::new(0),
            step_no: 0,
            last_kind: None,
            harness_error: None,
            quiet: false,
            policy: PolicyWorld::new(seed, defect),
            node_refused: 0,
            msgs_to_dead: 0,
            keys,
            machine,
            defect,
            cfg,
        };
        world.build_genesis(start_epoch);
        world
    }

    /// Writes the genesis of every chain into the leader and into every device.
    fn build_genesis(&mut self, epoch: RestoreEpoch) {
        let per_account = usize::from(self.cfg.members) * usize::from(self.cfg.devices);
        let stamp = azlin_seams::UnixTime::from_secs(SIM_START);
        // The devices.
        for account in 0..usize::from(self.cfg.accounts) {
            for local in 0..per_account {
                let idx = account * per_account + local;
                let key = self.keys.device_key(idx);
                if let Some(key) = key {
                    self.devices.push(Device::new(
                        idx,
                        account,
                        local / usize::from(self.cfg.devices).max(1),
                        key,
                        epoch,
                        &self.chains,
                    ));
                }
            }
        }
        // The leader's WAL: the genesis entry of every chain.
        let mut records = Vec::new();
        let mut heads = Vec::new();
        for info in self.chains.clone() {
            let first = info.account * per_account;
            let proposal = self
                .machine
                .propose(&self.keys, first, info.id, None, 0)
                .map(|p| self.machine.seal(&p, stamp, epoch));
            if let Some(entry) = proposal {
                heads.push(entry.head());
                self.ledger
                    .acked
                    .insert((info.id, entry.seq), (entry.hash, epoch));
                self.ledger.last_stamp.insert(info.id, stamp);
                self.ledger.entry_log.push_back(entry.clone());
                records.push(LRec::Entry(entry));
            }
        }
        let replayed = replay_leader(&records, &self.machine, epoch);
        self.leader.chains = replayed.chains;
        self.leader.stamper = Stamper::resume(replayed.last_stamp);
        for record in records {
            // The genesis is written before any fault can exist.
            let _ = self.leader.wal.append(record);
        }
        let _ = self.leader.wal.fsync(false);
        // The first heads, signed by the group, pinned by every device.
        let mut signed_heads = Vec::new();
        for head in heads {
            let signed = self.keys.sign_head(head, epoch, stamp);
            self.ledger.signed.insert((head.chain, head.seq, head.hash));
            self.ledger
                .honest_heads
                .insert((head.chain, head.seq, epoch), head.hash);
            self.ledger.head_log.push_back(signed.clone());
            self.ledger.served.insert(head.chain, (epoch, head.seq));
            signed_heads.push(signed);
        }
        for dev in 0..self.devices.len() {
            let account = self.devices.get(dev).map_or(0, |d| d.account);
            let mut recs = Vec::new();
            for signed in &signed_heads {
                let belongs = self
                    .chains
                    .iter()
                    .any(|c| c.id == signed.head.chain && c.account == account);
                if !belongs {
                    continue;
                }
                recs.push(DRec::Head(signed.clone()));
                if let Some(entry) = self.leader_entry(&signed.head.chain, signed.head.seq) {
                    recs.push(DRec::Entries(signed.head.chain, vec![entry]));
                }
            }
            for rec in recs {
                if let Some(d) = self.devices.get_mut(dev) {
                    let _ = d.wal.append(rec);
                }
            }
            if let Some(d) = self.devices.get_mut(dev) {
                let _ = d.wal.fsync(false);
                d.durable = d.wal.len();
            }
            self.dev_apply_pending(dev, false);
            self.ledger.floor.insert(dev, epoch);
        }
        // The oracle's watermark and epochs start where the genesis put the pins.
        for dev in 0..self.devices.len() {
            let pins: Vec<(ChainId, ChainSeq, RestoreEpoch)> = self
                .devices
                .get(dev)
                .map(|d| {
                    d.pin
                        .signed_heads()
                        .map(|h| (h.head.chain, h.head.seq, h.epoch))
                        .collect()
                })
                .unwrap_or_default();
            for (chain, seq, e) in pins {
                self.ledger.watermark.insert((dev, chain), seq);
                self.ledger.max_pinned_epoch.insert((dev, chain), e);
            }
        }
        self.trace.push(format!(
            "genesis: {} chains, {} devices, scheme {}",
            self.chains.len(),
            self.devices.len(),
            self.cfg.scheme.name()
        ));
    }

    /// The entry the leader holds at a position.
    pub(crate) fn leader_entry(&self, chain: &ChainId, seq: ChainSeq) -> Option<OpaqueEntry> {
        self.leader.chains.get(chain)?.chain.get(seq).cloned()
    }

    // ---------------------------------------------------------------------------------------
    // Addressing
    // ---------------------------------------------------------------------------------------

    /// The number of nodes (the leader and the devices).
    pub(crate) fn node_count(&self) -> usize {
        1 + self.devices.len()
    }

    /// The node a step's node number names (modulo the population).
    pub(crate) fn node_of(&self, n: u8) -> usize {
        usize::from(n) % self.node_count().max(1)
    }

    /// The device a step's device number names (modulo the population).
    pub(crate) fn dev_of(&self, n: u8) -> usize {
        usize::from(n) % self.devices.len().max(1)
    }

    /// The clock a step's node number names: a node, or one of the two voters.
    pub(crate) fn clock_of(&self, n: u8) -> usize {
        usize::from(n) % (self.devices.len() + 3)
    }

    /// The chains of an account, as indices into `chains`.
    pub(crate) fn account_chain_ids(&self, account: usize) -> Vec<ChainId> {
        self.chains
            .iter()
            .filter(|c| c.account == account)
            .map(|c| c.id)
            .collect()
    }

    /// The chain a device's step names: the `n`-th chain of its account.
    pub(crate) fn chain_of(&self, dev: usize, n: u8) -> Option<ChainId> {
        let account = self.devices.get(dev)?.account;
        let ids = self.account_chain_ids(account);
        ids.get(usize::from(n) % ids.len().max(1)).copied()
    }

    /// The account of a chain.
    pub(crate) fn account_of(&self, chain: &ChainId) -> Option<usize> {
        self.chains
            .iter()
            .find(|c| c.id == *chain)
            .map(|c| c.account)
    }

    /// The nodes of the devices of an account.
    pub(crate) fn account_nodes(&self, account: usize) -> Vec<usize> {
        self.devices
            .iter()
            .filter(|d| d.account == account)
            .map(|d| d.node)
            .collect()
    }

    /// Whether a node is up.
    pub(crate) fn is_up(&self, node: usize) -> bool {
        if node == 0 {
            self.leader.up
        } else {
            self.devices.get(node - 1).is_some_and(|d| d.up)
        }
    }

    /// The fault rates in force: the configured ones, or none while the world is quiet.
    pub(crate) fn rates(&self) -> FaultRates {
        if self.quiet {
            FaultRates::none()
        } else {
            self.cfg.faults.clone()
        }
    }

    /// The clock reading of a node in seconds (the node's own clock), `0` when it cannot be read.
    pub(crate) fn node_secs(&self, node: usize) -> u64 {
        self.clocks.read(node).map_or(0, |t| t.as_secs())
    }

    // ---------------------------------------------------------------------------------------
    // Sending, alarms, refusals
    // ---------------------------------------------------------------------------------------

    /// Sends a message; a full network refuses it and counts.
    pub(crate) fn send(&mut self, from: usize, to: usize, msg: Msg) {
        let rates = self.rates();
        let now = self.clocks.global();
        if self
            .net
            .send(now, from, to, msg, &rates, &mut self.rng)
            .is_err()
        {
            self.node_refused = self.node_refused.saturating_add(1);
        }
    }

    /// Records an alarm a node raised.
    pub(crate) fn push_alarm(&mut self, node: usize, alarm: Alarm) {
        if node == 0 {
            self.trace.push(format!("leader alarm: {alarm}"));
            return;
        }
        if let Some(dev) = self.devices.get_mut(node - 1) {
            dev.alarm_count = dev.alarm_count.saturating_add(1);
            if dev.alarms.len() >= MAX_ALARMS {
                dev.alarms.pop_front();
            }
            dev.alarms.push_back(alarm);
        }
        let kind = match alarm {
            Alarm::Fork { .. } => "fork",
            Alarm::Rollback { .. } => "rollback",
            Alarm::Withheld { .. } => "withheld",
            Alarm::UnsignedHead { .. } => "unsigned",
            Alarm::BelowRestoreEpoch { .. } => "below-epoch",
        };
        self.trace.decide(&format!("alarm {node} {kind}"));
        self.trace.push(format!("node {node} alarm: {alarm}"));
    }

    /// Records a typed refusal a node produced: its cause and the class it reported.
    pub(crate) fn note_refusal(&mut self, node: usize, cause: Cause, reported: RefusalClass) {
        let reported = if self.defect == Defect::MisclassifiesFork && cause == Cause::Fork {
            RefusalClass::Refuse
        } else {
            reported
        };
        let word = match reported {
            RefusalClass::Refuse => "refuse",
            RefusalClass::Alarm => "alarm",
            RefusalClass::Refetch => "refetch",
        };
        *self.cov.refusals.entry(word).or_insert(0) += 1;
        self.trace.decide(&format!("refusal {node} {word}"));
        self.delta.refusals.push(RefusalEvent {
            node,
            cause,
            reported,
        });
    }

    /// Records a typed chain refusal (the cause read from the variant, the class from the crate).
    pub(crate) fn note_chain_refusal(
        &mut self,
        node: usize,
        refusal: &ChainRefusal,
        chain: ChainId,
    ) {
        let cause = match refusal {
            ChainRefusal::Fork { .. } | ChainRefusal::GenesisAgain { .. } => Cause::Fork,
            ChainRefusal::BelowRestoreEpoch { .. } => Cause::LostTimeline,
            ChainRefusal::Corrupt => Cause::Corrupt,
            ChainRefusal::WrongChain
            | ChainRefusal::NotGenesis
            | ChainRefusal::WrongPrev { .. }
            | ChainRefusal::Gap { .. }
            | ChainRefusal::StampBackwards
            | ChainRefusal::Full => Cause::Other,
        };
        self.note_refusal(node, cause, refusal.class());
        if let Some(alarm) = refusal.alarm(chain) {
            self.push_alarm(node, alarm);
        }
    }

    /// Notes a typed pin verdict that is a refusal.
    pub(crate) fn note_pin_verdict(&mut self, node: usize, verdict: &PinVerdict, chain: ChainId) {
        match verdict {
            PinVerdict::Fork(alarm) => {
                self.note_refusal(node, Cause::Fork, RefusalClass::Alarm);
                self.push_alarm(node, *alarm);
            }
            PinVerdict::Refused(refusal) => {
                self.note_refusal(node, Cause::LostTimeline, refusal.class());
                self.push_alarm(node, refusal.alarm(chain));
            }
            PinVerdict::RefusedOlder { .. } => {
                self.note_refusal(node, Cause::Other, RefusalClass::Refuse);
            }
            PinVerdict::BadSignature => {
                self.note_refusal(node, Cause::Other, RefusalClass::Refuse);
            }
            PinVerdict::Advanced | PinVerdict::Unchanged | PinVerdict::Full => {}
        }
    }

    /// Records that the harness itself is wrong (the first message wins).
    pub(crate) fn harness(&mut self, message: String) {
        if self.harness_error.is_none() {
            self.harness_error = Some(message);
        }
    }

    // ---------------------------------------------------------------------------------------
    // The interpreter
    // ---------------------------------------------------------------------------------------

    /// The footprint now.
    pub(crate) fn footprint(&self) -> Footprint {
        Footprint {
            inflight: self.net.len(),
            max_jobs_per_node: self.sched.max_jobs(),
            open_jobs: self.sched.open_count(),
            max_alarms: self
                .devices
                .iter()
                .map(|d| d.alarms.len())
                .max()
                .unwrap_or(0),
            max_disk_records: self
                .devices
                .iter()
                .map(|d| d.wal.len())
                .chain(core::iter::once(self.leader.wal.len()))
                .max()
                .unwrap_or(0),
            offsite_copies: self.leader.offsite.len(),
            restores: self.ledger.restores,
            trace_tail: self.trace.tail.len(),
        }
    }

    /// Executes one step. Whether it applied (`false` is a skip).
    pub fn step(&mut self, step: &Step) -> bool {
        self.rng = step_rng(self.seed, step.salt());
        self.delta = Delta::default();
        self.step_no = self.step_no.saturating_add(1);
        let kind = step.kind();
        let applied = match *step {
            Step::Append {
                dev,
                chain,
                payload,
                ..
            } => self.do_append(dev, chain, payload),
            Step::Deliver { n, .. } => self.do_deliver(n),
            Step::Poll { node, n, .. } => self.do_poll(node, n),
            Step::Tick { secs, .. } => {
                self.clocks.advance(u64::from(secs));
                true
            }
            Step::HeadTick { .. } => self.do_head_tick(),
            Step::Fetch { dev, chain, .. } => self.do_fetch(dev, chain),
            Step::Meet { a, b, .. } => self.do_meet(a, b),
            Step::Checkpoint { chain, .. } => self.do_checkpoint(chain),
            Step::Offsite { .. } => self.do_offsite(),
            Step::Disaster {
                copy,
                sig,
                announce,
                ..
            } => self.do_disaster(copy, sig, announce),
            Step::Resend { dev, .. } => self.do_resend_step(dev),
            Step::Partition { mask, .. } => self.do_partition(mask),
            Step::Heal { mask, .. } => {
                self.net.heal(mask);
                true
            }
            Step::Slow { node, factor, .. } => {
                let node = self.node_of(node);
                self.sched.note_fault(OpKind::Partition);
                self.net.set_slow(node, factor);
                *self.cov.faults.entry(FaultKind::SlowNode).or_insert(0) += 1;
                true
            }
            Step::Skew { node, shift, .. } => self.do_clock(node, shift, true),
            Step::Jump { node, shift, .. } => self.do_clock(node, shift, false),
            Step::Crash { node, .. } => self.do_crash_step(node),
            Step::Restart { node, wipe, .. } => self.do_restart_step(node, wipe),
            Step::Disk { node, fault, .. } => self.do_disk(node, fault),
            Step::Attack {
                kind, dev, chain, ..
            } => crate::adversary::attack(self, kind, dev, chain),
            Step::Policy {
                scope,
                group,
                patch,
                ..
            } => self.do_policy(scope, group, patch),
            Step::Quiesce { .. } => {
                if !self.quiesce() {
                    self.delta.notes.push(String::from("quiesce:not-converged"));
                }
                true
            }
        };
        self.cov.steps += 1;
        if !applied {
            self.cov.skipped += 1;
            *self.cov.skips.entry(kind).or_insert(0) += 1;
        }
        if let Some(previous) = self.last_kind {
            *self.cov.adjacent.entry((previous, kind)).or_insert(0) += 1;
        }
        self.last_kind = Some(kind);
        self.trace.decide(&format!("{} {}", kind.verb(), applied));
        self.trace.push(format!(
            "{}: {} -> {}",
            self.step_no,
            step.to_line(),
            if applied { "ok" } else { "skip" }
        ));
        self.cov.peak = self.cov.peak.max(self.footprint());
        applied
    }

    fn do_append(&mut self, dev: u8, chain: u8, payload: u16) -> bool {
        let dev = self.dev_of(dev);
        let Some(chain) = self.chain_of(dev, chain) else {
            return false;
        };
        let Some(node) = self.devices.get(dev).filter(|d| d.up).map(|d| d.node) else {
            return false;
        };
        let mut job = Job::new(JobKind::DPropose, node);
        job.chain = Some(chain);
        job.payload = payload;
        self.spawn_and_run_first(job)
    }

    /// Queues a job and runs its first phase at once ("to its first await").
    pub(crate) fn spawn_and_run_first(&mut self, job: Job) -> bool {
        let node = job.node;
        match self.sched.spawn(job) {
            Ok(_) => {
                self.run_phases(node, 1);
                true
            }
            Err(_) => {
                self.node_refused = self.node_refused.saturating_add(1);
                false
            }
        }
    }

    /// Runs up to `n` phases of the jobs of one node; the number run.
    pub(crate) fn run_phases(&mut self, node: usize, n: usize) -> usize {
        let mut ran = 0;
        for _ in 0..n {
            let now = self.node_secs(node);
            let Some(mut job) = self.sched.take_runnable(node, now) else {
                break;
            };
            self.sched.note_phase(&job);
            self.delta.touched.insert(node);
            let next = self.run_phase(&mut job);
            match next {
                Next::Advance => job.phase = job.phase.saturating_add(1),
                Next::Goto(phase) => job.phase = phase,
            }
            self.sched.put_back(job);
            ran += 1;
        }
        ran
    }

    pub(crate) fn run_phase(&mut self, job: &mut Job) -> Next {
        match job.kind {
            JobKind::LAppend => self.l_append_phase(job),
            JobKind::LHead => self.l_head_phase(job),
            JobKind::LCheckpoint => self.l_checkpoint_phase(job),
            JobKind::LRestore => self.l_restore_phase(job),
            JobKind::LOffsite => self.l_offsite_phase(job),
            JobKind::LFetch => self.l_fetch_phase(job),
            JobKind::DOnHead => self.d_on_head_phase(job),
            JobKind::DOnEntries => self.d_on_entries_phase(job),
            JobKind::DOnRestore => self.d_on_restore_phase(job),
            JobKind::DPropose => self.d_propose_phase(job),
            JobKind::DMeet => self.d_meet_phase(job),
        }
    }

    fn do_deliver(&mut self, n: u8) -> bool {
        let mut any = false;
        for _ in 0..n.max(1) {
            let now = self.clocks.global();
            let Some(envelope) = self.net.pop_due(now) else {
                break;
            };
            any = true;
            self.on_message(envelope.from, envelope.to, envelope.msg);
        }
        any
    }

    fn do_poll(&mut self, node: u8, n: u8) -> bool {
        let node = self.node_of(node);
        self.run_phases(node, usize::from(n.max(1))) > 0
    }

    /// A message reaches a node: a job is made of it and runs to its first await.
    pub(crate) fn on_message(&mut self, from: usize, to: usize, msg: Msg) {
        if !self.is_up(to) {
            self.msgs_to_dead = self.msgs_to_dead.saturating_add(1);
            return;
        }
        self.delta.touched.insert(to);
        match msg {
            Msg::Propose(proposal) => {
                let mut job = Job::new(JobKind::LAppend, 0);
                job.from = from;
                job.chain = Some(proposal.chain);
                job.proposal = Some(proposal);
                self.spawn_and_run_first(job);
            }
            Msg::Fetch {
                chain,
                after,
                floor,
            } => {
                let mut job = Job::new(JobKind::LFetch, 0);
                job.from = from;
                job.chain = Some(chain);
                job.after = after;
                job.floor = floor;
                self.spawn_and_run_first(job);
            }
            Msg::Ack { id, result } => {
                self.sched.deliver_to_waiting(
                    to,
                    JobKind::DPropose,
                    |j| j.proposal.as_ref().is_some_and(|p| p.id == id),
                    result,
                );
            }
            Msg::Head(head) => {
                let mut job = Job::new(JobKind::DOnHead, to);
                job.from = from;
                job.chain = Some(head.head.chain);
                job.head = Some(head);
                self.spawn_and_run_first(job);
            }
            Msg::Restore(record) => {
                let mut job = Job::new(JobKind::DOnRestore, to);
                job.from = from;
                job.record = Some(record);
                self.spawn_and_run_first(job);
            }
            Msg::Entries {
                chain,
                entries,
                head,
                records,
            } => {
                for record in records {
                    let mut job = Job::new(JobKind::DOnRestore, to);
                    job.from = from;
                    job.record = Some(record);
                    self.spawn_and_run_first(job);
                }
                let mut job = Job::new(JobKind::DOnEntries, to);
                job.from = from;
                job.chain = Some(chain);
                job.entries = entries;
                job.head = head;
                self.spawn_and_run_first(job);
            }
        }
    }

    fn do_head_tick(&mut self) -> bool {
        if !self.leader.up {
            return false;
        }
        let job = Job::new(JobKind::LHead, 0);
        self.spawn_and_run_first(job)
    }

    fn do_fetch(&mut self, dev: u8, chain: u8) -> bool {
        let dev = self.dev_of(dev);
        let Some(chain) = self.chain_of(dev, chain) else {
            return false;
        };
        if !self.devices.get(dev).is_some_and(|d| d.up) {
            return false;
        }
        self.dev_send_fetch(dev, chain);
        true
    }

    fn do_meet(&mut self, a: u8, b: u8) -> bool {
        let (a, b) = (self.dev_of(a), self.dev_of(b));
        if a == b {
            return false;
        }
        let (Some(da), Some(db)) = (self.devices.get(a), self.devices.get(b)) else {
            return false;
        };
        if !da.up || !db.up || self.net.is_cut(da.node, db.node) {
            return false;
        }
        let (na, nb) = (da.node, db.node);
        for (me, peer) in [(na, nb), (nb, na)] {
            let mut job = Job::new(JobKind::DMeet, me);
            job.peer = peer;
            self.spawn_and_run_first(job);
        }
        true
    }

    fn do_checkpoint(&mut self, chain: u8) -> bool {
        if !self.leader.up || self.leader.faulted {
            return false;
        }
        let ids: Vec<ChainId> = self.chains.iter().map(|c| c.id).collect();
        let Some(id) = ids.get(usize::from(chain) % ids.len().max(1)).copied() else {
            return false;
        };
        let mut job = Job::new(JobKind::LCheckpoint, 0);
        job.chain = Some(id);
        self.spawn_and_run_first(job)
    }

    fn do_offsite(&mut self) -> bool {
        // A copy taken while a restore is open would be the state the restore is replacing.
        if !self.leader.up || self.leader.faulted || self.leader.restoring {
            return false;
        }
        let job = Job::new(JobKind::LOffsite, 0);
        self.spawn_and_run_first(job)
    }

    fn do_disaster(&mut self, copy: u8, sig: SigChoice, announce: bool) -> bool {
        if self.ledger.restores >= MAX_RESTORES || self.leader.offsite.is_empty() {
            return false;
        }
        if self.leader.restoring {
            return false;
        }
        if !self.leader.up {
            // A restore onto a replacement server: it starts with an empty disk.
            self.restart_node(0, true);
        }
        let mut job = Job::new(JobKind::LRestore, 0);
        job.copy = usize::from(copy);
        job.sig = sig;
        job.announce = announce && self.cfg.leader != LeaderVariant::SilentRollback;
        self.sched.note_fault(OpKind::Restore);
        self.spawn_and_run_first(job)
    }

    fn do_resend_step(&mut self, dev: u8) -> bool {
        let dev = self.dev_of(dev);
        self.dev_resend(dev)
    }

    fn do_partition(&mut self, mask: u16) -> bool {
        let nodes = self.node_count();
        let valid = if nodes >= 16 {
            u16::MAX
        } else {
            (1u16 << nodes) - 1
        };
        let mask = mask & valid;
        if mask == 0 || mask == valid {
            return false;
        }
        self.sched.note_fault(OpKind::Partition);
        self.net.partition(mask);
        *self.cov.faults.entry(FaultKind::Partition).or_insert(0) += 1;
        true
    }

    fn do_clock(&mut self, node: u8, shift: Shift, set: bool) -> bool {
        let clock = self.clock_of(node);
        self.sched.note_fault(OpKind::ClockJump);
        if set {
            self.clocks.skew(clock, shift);
            *self.cov.faults.entry(FaultKind::ClockSkew).or_insert(0) += 1;
        } else {
            self.clocks.jump(clock, shift);
            *self.cov.faults.entry(FaultKind::ClockJump).or_insert(0) += 1;
        }
        true
    }

    fn do_crash_step(&mut self, node: u8) -> bool {
        let node = self.node_of(node);
        if !self.is_up(node) {
            return false;
        }
        self.crash_node(node);
        true
    }

    fn do_restart_step(&mut self, node: u8, wipe: bool) -> bool {
        let node = self.node_of(node);
        if self.is_up(node) {
            return false;
        }
        self.restart_node(node, wipe)
    }

    fn do_disk(&mut self, node: u8, fault: DiskFault) -> bool {
        let node = self.node_of(node);
        self.sched.note_fault(OpKind::DiskFault);
        let lies = 1 + below(&mut self.rng, 3) as u32;
        let mut rng = Rng::new(self.rng.next_u64());
        let flaw = !matches!(fault, DiskFault::Tear | DiskFault::Lie);
        if node == 0 {
            match fault {
                DiskFault::Lie => self.leader.wal.arm_lie(lies),
                DiskFault::Tear => self.leader.wal.arm_tear(),
                DiskFault::ReadErr => self.leader.wal.arm_read_error(),
                DiskFault::Flip => self.leader.wal.flip(&mut rng),
                DiskFault::Vanish => self.leader.wal.vanish(),
            }
        } else if let Some(dev) = self.devices.get_mut(node - 1) {
            match fault {
                DiskFault::Lie => dev.wal.arm_lie(lies),
                DiskFault::Tear => dev.wal.arm_tear(),
                DiskFault::ReadErr => dev.wal.arm_read_error(),
                DiskFault::Flip => dev.wal.flip(&mut rng),
                DiskFault::Vanish => dev.wal.vanish(),
            }
        } else {
            return false;
        }
        if node == 0 && flaw && matches!(fault, DiskFault::Flip | DiskFault::Vanish) {
            self.ledger.losses.push(format!(
                "step {}: the leader's disk took a {}",
                self.step_no,
                fault.name()
            ));
            self.ledger.degraded = true;
        }
        if node != 0 {
            if let Some(dev) = self.devices.get_mut(node - 1) {
                if flaw {
                    dev.disk_flawed = true;
                    // A flaw after the crash: any prefix of the log may be what the restart finds.
                    dev.crash_allowed = None;
                }
            }
        }
        match fault {
            DiskFault::ReadErr => {
                *self.cov.faults.entry(FaultKind::ReadError).or_insert(0) += 1;
            }
            DiskFault::Flip => {
                *self.cov.faults.entry(FaultKind::BitFlip).or_insert(0) += 1;
            }
            DiskFault::Vanish => {
                *self.cov.faults.entry(FaultKind::Vanish).or_insert(0) += 1;
            }
            DiskFault::Lie | DiskFault::Tear => {}
        }
        true
    }

    fn do_policy(&mut self, scope: u8, group: u8, patch: u32) -> bool {
        let mut rng = Rng::new(self.rng.next_u64());
        let events = self.policy.step(scope, group, patch, &mut rng);
        let applied = !events.is_empty();
        self.delta.policy = events;
        applied
    }

    // ---------------------------------------------------------------------------------------
    // Crash and restart
    // ---------------------------------------------------------------------------------------

    /// A process crash at the current await point: every job of the node is lost, its volatile
    /// state is gone, its disk is crashed by the disk's rule.
    pub(crate) fn crash_node(&mut self, node: usize) {
        self.sched.note_fault(OpKind::Crash);
        self.sched.crash_node(node);
        *self.cov.faults.entry(FaultKind::Crash).or_insert(0) += 1;
        let tear = self.rates().torn;
        let mut rng = Rng::new(self.rng.next_u64());
        self.delta.touched.insert(node);
        if node == 0 {
            let report = self.leader.wal.crash(&mut rng, tear);
            self.leader.crash();
            if report.lost_by_lie > 0 {
                self.ledger.degraded = true;
                self.ledger.losses.push(format!(
                    "step {}: a crash lost {} leader records a lying fsync had reported durable",
                    self.step_no, report.lost_by_lie
                ));
            }
        } else {
            self.dev_crash(node - 1, &mut rng, tear);
        }
        self.trace.push(format!("node {node} crashed"));
    }

    /// A restart: from the disk alone, or from nothing (`wipe`). Whether the node is up.
    pub(crate) fn restart_node(&mut self, node: usize, wipe: bool) -> bool {
        self.delta.touched.insert(node);
        let up = if node == 0 {
            self.restart_leader(wipe)
        } else {
            self.restart_device(node - 1, wipe)
        };
        self.trace
            .push(format!("node {node} restart wipe={wipe} up={up}"));
        up
    }

    // ---------------------------------------------------------------------------------------
    // Quiesce
    // ---------------------------------------------------------------------------------------

    /// Heals every partition, stops the faults, restarts what is down and drains everything, then
    /// the convergence checks run (the caller: `invariants`). `false` when it did not converge
    /// within [`QUIESCE_ROUNDS`] rounds.
    pub(crate) fn quiesce(&mut self) -> bool {
        self.net.heal_all();
        self.net.clear_slow();
        self.quiet = true;
        let mut stable_rounds = 0;
        let mut previous = self.fingerprint();
        for round in 0..QUIESCE_ROUNDS {
            // A node whose disk vanished cannot write: it stops, and comes back from what the disk
            // holds (nothing), like every node that is down.
            if self.leader.up && self.leader.wal.is_gone() {
                self.crash_node(0);
            }
            if !self.leader.up {
                self.restart_node(0, false);
            }
            for dev in 0..self.devices.len() {
                if self.devices.get(dev).is_some_and(|d| d.up && d.wal.is_gone()) {
                    self.crash_node(dev + 1);
                }
                if !self.devices.get(dev).is_some_and(|d| d.up) {
                    self.restart_node(dev + 1, false);
                }
            }
            // The leader signs heads; the devices send again what they hold and fetch.
            if self.leader.up && !self.leader.faulted {
                let job = Job::new(JobKind::LHead, 0);
                let _ = self.sched.spawn(job);
            }
            for dev in 0..self.devices.len() {
                self.dev_resend(dev);
                let account = self.devices.get(dev).map_or(0, |d| d.account);
                for chain in self.account_chain_ids(account) {
                    self.dev_send_fetch_forced(dev, chain);
                }
            }
            self.drain();
            let now = self.fingerprint();
            if now == previous && self.net.is_empty() && !self.sched.any_unfinished() {
                stable_rounds += 1;
            } else {
                stable_rounds = 0;
            }
            previous = now;
            if stable_rounds >= 2 {
                self.quiet = false;
                return true;
            }
            // Give the timers of the devices time to expire.
            self.clocks.advance(30 * (1 + (round as u64).min(40)));
        }
        self.quiet = false;
        false
    }

    /// Delivers and polls until nothing is due and no job can run.
    fn drain(&mut self) {
        for _ in 0..DRAIN_ROUNDS {
            let mut progress = false;
            while let Some(envelope) = self.net.pop_due(self.clocks.global()) {
                progress = true;
                self.on_message(envelope.from, envelope.to, envelope.msg);
            }
            for node in self.sched.busy_nodes() {
                if self.run_phases(node, 64) > 0 {
                    progress = true;
                }
            }
            if !progress {
                match self.net.next_deliverable() {
                    Some(at) if at > self.clocks.global() => {
                        self.clocks.advance(at - self.clocks.global());
                    }
                    Some(_) => {}
                    None => return,
                }
            }
        }
    }

    /// A hash of what quiesce watches: the leader's chains and every device's pins and copies.
    pub(crate) fn fingerprint(&self) -> Hash32 {
        let mut parts: Vec<u8> = Vec::new();
        parts.extend_from_slice(&self.leader.epoch.get().to_be_bytes());
        for (id, lc) in &self.leader.chains {
            parts.extend_from_slice(&id.id);
            if let Some(head) = lc.chain.head() {
                parts.extend_from_slice(&head.seq.get().to_be_bytes());
                parts.extend_from_slice(head.hash.as_bytes());
            }
        }
        for dev in &self.devices {
            parts.extend_from_slice(dev.state_digest().as_bytes());
        }
        hash32("azlin-sim-fingerprint-v1", &[&parts])
    }

    /// The hash of the world's final state (leader, devices, ledger counts).
    pub fn world_hash(&self) -> Hash32 {
        hash32(
            "azlin-sim-world-v1",
            &[
                self.fingerprint().as_bytes(),
                &self.ledger.restores.to_be_bytes(),
                &(self.devices.len() as u64).to_be_bytes(),
                &self.leader.wal.len().to_be_bytes(),
            ],
        )
    }

    /// The state of the world in a few lines: what a failure report shows next to the trace.
    pub(crate) fn dump(&self) -> Vec<String> {
        let mut out = Vec::new();
        out.push(format!(
            "world: clock +{} s, {} in flight, cut {:#06x}, {} restores, degraded {}, announced epoch {}, refused sends and jobs {}",
            self.clocks.global().saturating_sub(SIM_START),
            self.net.len(),
            self.net.cut_mask(),
            self.ledger.restores,
            self.ledger.degraded,
            self.ledger.announced_epoch,
            self.node_refused
        ));
        out.push(format!(
            "leader: up {} faulted {} restoring {} epoch {} generation {} wal {} (synced {}) off-site copies {} records held {}",
            self.leader.up,
            self.leader.faulted,
            self.leader.restoring,
            self.leader.epoch,
            self.leader.gen,
            self.leader.wal.len(),
            self.leader.wal.synced_len(),
            self.leader.offsite.len(),
            self.leader.records.len()
        ));
        for (id, lc) in &self.leader.chains {
            out.push(format!(
                "  leader {}: head {} entries {} newest stamp +{} checkpoints {}",
                short_chain(id),
                lc.chain.head().map_or(String::from("-"), |h| h.seq.to_string()),
                lc.chain.entries().len(),
                lc.chain
                    .last_at()
                    .map_or(0, |at| at.as_secs().saturating_sub(SIM_START)),
                lc.checkpoints.len()
            ));
        }
        for dev in &self.devices {
            out.push(format!(
                "device {} (node {}, account {}): up {} floor {} wal {} (synced {}) alarms {} to send again {} sent again {} disk flawed {}",
                dev.idx,
                dev.node,
                dev.account,
                dev.up,
                dev.pin.restore_epoch(),
                dev.wal.len(),
                dev.wal.synced_len(),
                dev.alarm_count,
                dev.resend.len(),
                dev.resent.len(),
                dev.disk_flawed
            ));
            for id in &dev.chain_ids {
                let pin = match (dev.pin.head(id), dev.pin.signed_head(id)) {
                    (Some(head), Some(signed)) => {
                        format!("{}/{} signed in epoch {}", head.seq, short_hash(&head.hash), signed.epoch)
                    }
                    (Some(head), None) => format!("{}/{} anchored", head.seq, short_hash(&head.hash)),
                    (None, _) => String::from("-"),
                };
                let copy = dev.copies.get(id).map_or(String::from("-"), |c| {
                    format!(
                        "{} (floor {})",
                        c.head().map_or(String::from("-"), |h| h.seq.to_string()),
                        c.epoch()
                    )
                });
                out.push(format!("  device {} {}: pin {pin} copy {copy}", dev.idx, short_chain(id)));
            }
        }
        for node in 0..self.node_count() {
            let open = self.sched.open_of(node);
            if !open.is_empty() {
                out.push(format!("jobs of node {node}: {open:?}"));
            }
        }
        out
    }

    /// Merges what the net and the disks counted into the coverage.
    pub fn finish_coverage(&mut self) {
        let net = self.net.stats();
        let mut add = |kind: FaultKind, n: u64| {
            *self.cov.faults.entry(kind).or_insert(0) += n;
        };
        add(FaultKind::Loss, net.lost);
        add(FaultKind::Duplication, net.duplicated);
        add(FaultKind::Reordering, net.reordered);
        add(FaultKind::Corruption, net.corrupted);
        let mut torn = self.leader.wal.stats().torn;
        let mut lies = self.leader.wal.stats().lies;
        for dev in &self.devices {
            torn += dev.wal.stats().torn;
            lies += dev.wal.stats().lies;
        }
        add(FaultKind::TornWrite, torn);
        add(FaultKind::LostFsync, lies);
        for ((open, running), count) in self.sched.pairs() {
            *self.cov.interleaved.entry((*open, *running)).or_insert(0) += count;
        }
        for (key, count) in self.sched.crash_points() {
            *self.cov.crash_points.entry(*key).or_insert(0) += count;
        }
    }
}

/// One rule of what a node does around the pure crates, with the plan quote or the drift row that backs
/// it (rule H7 of the harness).
///
/// Implements planning/architecture.md section 1.4 rule 3 (protocol code is a pure codec: what a node
/// does around it is glue).
///
/// Caller: `GLUE_RULES`, the report, tests/hygiene.rs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlueRule {
    /// The id (`G1`).
    pub id: &'static str,
    /// The rule in one sentence.
    pub rule: &'static str,
    /// The plan quote or the drift row that backs it.
    pub backing: &'static str,
}

/// The glue rules of stage 1: the smallest version the plan's text requires.
///
/// Implements planning/architecture.md section 1.4 rule 3 and section 5.9 (the glue is later units').
///
/// Caller: tests/hygiene.rs, `Report::render`.
pub const GLUE_RULES: &[GlueRule] = &[
    GlueRule {
        id: "G1",
        rule: "a node persists a record and fsyncs BEFORE it uses it: the leader before it acks, a device before it applies a head, an entry or a restore record",
        backing: "6.10 V1 \"saved means fsynced\"; 1.4 rule 2",
    },
    GlueRule {
        id: "G2",
        rule: "a restart rebuilds a node from its log alone: replay the valid prefix, truncate at the first corrupt record",
        backing: "5.9 \"any copy can restore it\"",
    },
    GlueRule {
        id: "G3",
        rule: "the leader acks a proposal only after its fsync; the same proposal again is acked as already held, never a fork",
        backing: "5.9; azlin-chain identical_entry_again_is_not_a_fork",
    },
    GlueRule {
        id: "G4",
        rule: "a proposal refused as stale: the device fetches, rebuilds on the new tip, re-signs and retries at most five times, with a jittered backoff on its own clock",
        backing: "1.4 rule 10; drift row X-DELTA-W1SIM-13",
    },
    GlueRule {
        id: "G5",
        rule: "a refusal of the class refetch: fetch the entry again from the leader (the plan says another voter), at most three times",
        backing: "5.9 \"fetched again from another voter\"",
    },
    GlueRule {
        id: "G6",
        rule: "on a peer that is ahead, or a head of an epoch above the device's floor with no record, the device fetches the restore records since its floor and the entries it lacks",
        backing: "5.13 \"devices heal from that announcement only\"; drift row X-DELTA-W1SIM-3",
    },
    GlueRule {
        id: "G7",
        rule: "a meeting: both devices send their signed heads and run compare_peer on each",
        backing: "5.9 \"devices of one account exchange the heads they have seen\"",
    },
    GlueRule {
        id: "G8",
        rule: "a restore record: persist it before applying; rebuild the chain copies at the survivors and raise every floor",
        backing: "azlin-chain Caller: list (D-W1S17A item 3)",
    },
    GlueRule {
        id: "G9",
        rule: "a device that follows a record keeps its OWN entries beyond the survivor, for resending",
        backing: "2.17 \"send again whatever they hold beyond them\"",
    },
    GlueRule {
        id: "G10",
        rule: "resend: re-propose the own entries beyond the survivor as NEW entries of the new epoch, once for each original hash",
        backing: "2.17 \"send again\"; drift row X-DELTA-W1SIM-4",
    },
    GlueRule {
        id: "G11",
        rule: "the leader announces every restore record it holds again after a restart, and keeps the last 64",
        backing: "drift row X-DELTA-W1SIM-3",
    },
    GlueRule {
        id: "G12",
        rule: "the leader signs one head for each (chain, position, epoch), in the epoch it stands in",
        backing: "5.9",
    },
    GlueRule {
        id: "G13",
        rule: "the leader's stamp is the larger of its clock and its previous stamp, and at most the median of three voter clocks; when the voters are behind its previous stamp it refuses",
        backing: "2.4; drift row X-DELTA-W1SIM-8",
    },
    GlueRule {
        id: "G14",
        rule: "a new or wiped device starts at the leader's current restore epoch and trusts it once",
        backing: "azlin-chain HeadPin::starting_at docs",
    },
    GlueRule {
        id: "G15",
        rule: "an off-site copy is a snapshot of the leader's valid log; a disaster rebuilds from it",
        backing: "2.17",
    },
    GlueRule {
        id: "G17",
        rule: "the leader judges a PROPOSAL against its own tip before it seals it: a stale proposal is a retry, never a fork alarm; the chain crate judges the sealed entry",
        backing: "5.9 \"rejects an entry with a reason for a wrong prev\"; drift row X-DELTA-W1SIM-18",
    },
    GlueRule {
        id: "G18",
        rule: "a device admits entries into a chain copy only up to the position of the signed head it pinned, and only when the hash at that position is the pinned one",
        backing: "5.9 \"every device keeps the part of the chain it verified\"; drift row X-DELTA-W1SIM-19",
    },
    GlueRule {
        id: "G19",
        rule: "the leader holds a restore lock: while a restore is open it starts no append, head, checkpoint, fetch or off-site copy",
        backing: "6.10 V3 (a restore is one announcement); drift row X-DELTA-W1SIM-20",
    },
];
