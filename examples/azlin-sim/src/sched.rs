//! Jobs, their await points, the interleaving tracker, crash and restart (planning/architecture.md
//! section 1.4 rule 2: a crash at any point; section 6.10 V1: nothing acknowledged is lost).
//!
//! A handler is not one function call: it is a JOB, a small state machine whose PHASES end at an
//! AWAIT POINT (the places a real node waits for I/O, a lock or a reply). `poll` runs one phase.
//! The scheduler keeps the jobs of each node in a bounded queue, and a bounded set of OPEN jobs
//! (started, not finished); every time a phase of job J runs it records, for each other open job K,
//! the pair (kind of K, kind of J) as INTERLEAVED (the coverage report's definition). A crash lands
//! between two phases of whatever the node was doing: every open job of the node is lost, and each
//! (handler, await point) it stood at is counted.

use std::collections::{BTreeMap, VecDeque};

use azlin_chain::{
    ChainId, ChainRefusal, Checkpoint, Head, PeerVerdict, PinVerdict, RestoreRecord,
    RestoreRefusal, SignedHead,
};
use azlin_seams::{ChainSeq, Hash32, RestoreEpoch, Stamp};

use crate::config::{MAX_JOBS_PER_NODE, MAX_OPEN_JOBS};
use crate::coverage::OpKind;
use crate::leader::LRec;
use crate::machine::{OpaqueEntry, OpaqueReplica, Proposal};
use crate::step::SigChoice;
use crate::world::{AckResult, PinView};

/// The handlers of the nodes (table 4.4 of the brief): one job kind for each.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a crash at any point) and section 6.10
/// V1 (saved means fsynced).
///
/// Caller: `Sched`, `world::World`, `coverage::Coverage`, tests/s1_04.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JobKind {
    /// `L.append`: the leader takes a proposal.
    LAppend,
    /// `L.head`: the leader signs and sends heads.
    LHead,
    /// `L.checkpoint`: the leader records a checkpoint.
    LCheckpoint,
    /// `L.restore`: the leader restores from an off-site copy.
    LRestore,
    /// `L.offsite`: the leader takes an off-site copy.
    LOffsite,
    /// `L.fetch`: the leader answers a fetch.
    LFetch,
    /// `D.on_head`: a device takes a head.
    DOnHead,
    /// `D.on_entries`: a device takes entries.
    DOnEntries,
    /// `D.on_restore`: a device follows a restore record.
    DOnRestore,
    /// `D.propose`: a device proposes an entry.
    DPropose,
    /// `D.meet`: a device meets a peer.
    DMeet,
}

/// One row of table 4.4: a handler, the kind of operation it counts as, and the await points after
/// each of its phases.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a crash at any point).
///
/// Caller: `JobKind::handler`, tests/s1_04.
#[derive(Clone, Copy, Debug)]
pub struct Handler {
    /// The name (`L.append`).
    pub name: &'static str,
    /// The operation kind the interleaving tracker counts it as.
    pub op: OpKind,
    /// The phases, one per await point.
    pub phases: &'static [&'static str],
}

/// The table of handlers and their await points (a new handler or phase needs its row here and its
/// directed crash case in tests/s1_04).
///
/// Implements planning/architecture.md section 1.4 rule 2 (a crash at any point) and section 6.10 V1.
///
/// Caller: `JobKind::handler`, tests/s1_04.
pub const HANDLERS: [Handler; 11] = [
    Handler {
        name: "L.append",
        op: OpKind::Append,
        phases: &[
            "recv",
            "verified and admitted by the machine",
            "WAL write",
            "fsync",
            "applied to the chain",
            "ack and notices sent",
        ],
    },
    Handler {
        name: "L.head",
        op: OpKind::Head,
        phases: &["tips read", "heads signed", "sent"],
    },
    Handler {
        name: "L.checkpoint",
        op: OpKind::Checkpoint,
        phases: &[
            "state hash computed",
            "checkpoint signed",
            "WAL write",
            "fsync",
            "sent",
        ],
    },
    Handler {
        name: "L.restore",
        op: OpKind::Restore,
        phases: &[
            "record built and signed",
            "WAL write of the restore record",
            "fsync",
            "truncated to the survivors and epoch raised",
            "broadcast",
        ],
    },
    Handler {
        name: "L.offsite",
        op: OpKind::Offsite,
        phases: &["WAL prefix copied"],
    },
    Handler {
        name: "L.fetch",
        op: OpKind::Fetch,
        phases: &["recv", "read", "sent"],
    },
    Handler {
        name: "D.on_head",
        op: OpKind::OnHead,
        phases: &[
            "signature and epoch checked and offered to the pin",
            "WAL write of the signed head",
            "fsync",
        ],
    },
    Handler {
        name: "D.on_entries",
        op: OpKind::OnEntries,
        phases: &[
            "each entry admitted to the chain copy",
            "WAL write",
            "fsync",
            "head offered",
        ],
    },
    Handler {
        name: "D.on_restore",
        op: OpKind::OnRestore,
        phases: &[
            "record verified",
            "WAL write of the record",
            "fsync",
            "reanchor",
            "chain copies rebuilt at the survivors",
            "resend queued",
        ],
    },
    Handler {
        name: "D.propose",
        op: OpKind::Propose,
        phases: &[
            "entry built and signed on its tip",
            "sent",
            "ack or refusal received",
            "retry or done",
        ],
    },
    Handler {
        name: "D.meet",
        op: OpKind::Meet,
        phases: &["heads sent", "peer heads compared", "alarms recorded"],
    },
];

impl JobKind {
    /// Every kind, in the order of [`HANDLERS`].
    pub const ALL: [JobKind; 11] = [
        JobKind::LAppend,
        JobKind::LHead,
        JobKind::LCheckpoint,
        JobKind::LRestore,
        JobKind::LOffsite,
        JobKind::LFetch,
        JobKind::DOnHead,
        JobKind::DOnEntries,
        JobKind::DOnRestore,
        JobKind::DPropose,
        JobKind::DMeet,
    ];

    /// The handler row of this kind.
    pub fn handler(self) -> &'static Handler {
        match self {
            JobKind::LAppend => &HANDLERS[0],
            JobKind::LHead => &HANDLERS[1],
            JobKind::LCheckpoint => &HANDLERS[2],
            JobKind::LRestore => &HANDLERS[3],
            JobKind::LOffsite => &HANDLERS[4],
            JobKind::LFetch => &HANDLERS[5],
            JobKind::DOnHead => &HANDLERS[6],
            JobKind::DOnEntries => &HANDLERS[7],
            JobKind::DOnRestore => &HANDLERS[8],
            JobKind::DPropose => &HANDLERS[9],
            JobKind::DMeet => &HANDLERS[10],
        }
    }

    /// The operation kind the tracker counts it as.
    pub fn op(self) -> OpKind {
        self.handler().op
    }

    /// How many phases the handler has.
    pub fn phases(self) -> u8 {
        u8::try_from(self.handler().phases.len()).unwrap_or(u8::MAX)
    }

    /// Whether the job runs on the leader.
    pub fn on_leader(self) -> bool {
        matches!(
            self,
            JobKind::LAppend
                | JobKind::LHead
                | JobKind::LCheckpoint
                | JobKind::LRestore
                | JobKind::LOffsite
                | JobKind::LFetch
        )
    }
}

/// One peer head compared with a device's pin, kept between the comparison and the recording of
/// its alarm.
///
/// Implements planning/architecture.md section 5.9 (devices exchange the heads they have seen).
///
/// Caller: `Job::peer_cmps`, `device`.
#[derive(Clone, Debug)]
pub(crate) struct PeerCmp {
    /// The peer's head.
    pub head: SignedHead,
    /// What `compare_peer` said.
    pub verdict: PeerVerdict,
    /// The device's pin of the chain when it compared.
    pub ours: Option<PinView>,
    /// Whether the harness's own check says the signature is good.
    pub valid: bool,
    /// The restore epoch the device stands in, by the harness's own ledger.
    pub standing: RestoreEpoch,
}

/// One job: its identity, how far it got, and the data its phases carry. The job is a plain value
/// that holds nothing of the world; a crash drops it.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a crash at any point).
///
/// Caller: `Sched`, `leader`, `device`, `world`.
#[derive(Clone, Debug)]
pub(crate) struct Job {
    /// The identity.
    pub id: u64,
    /// The node it runs on.
    pub node: usize,
    /// The handler.
    pub kind: JobKind,
    /// How many phases it completed.
    pub phase: u8,
    /// The leader's generation when the job read the state it works on (a restore changes it).
    pub gen: u64,
    /// Whether the job waits for a reply (an ack) or a retry time.
    pub waiting: bool,
    /// The node a message came from.
    pub from: usize,
    /// Tries of a proposal.
    pub attempts: u8,
    /// The time (the node's own clock, seconds) at which a waiting job may run again.
    pub wake_at: u64,
    /// The chain the job is about.
    pub chain: Option<ChainId>,
    /// The proposal (`L.append`, `D.propose`).
    pub proposal: Option<Proposal>,
    /// The entry the leader sealed.
    pub sealed: Option<OpaqueEntry>,
    /// The replica after the sealed entry.
    pub replica_after: Option<OpaqueReplica>,
    /// The leader's answer to a proposal, or the answer a device received.
    pub answer: Option<AckResult>,
    /// Entries carried.
    pub entries: Vec<OpaqueEntry>,
    /// A head carried.
    pub head: Option<SignedHead>,
    /// Heads carried (`L.head`, `D.meet`).
    pub heads: Vec<SignedHead>,
    /// Restore records carried.
    pub records: Vec<RestoreRecord>,
    /// The restore record being followed or built.
    pub record: Option<RestoreRecord>,
    /// The WAL the restore will replace the disk with.
    pub staged: Vec<LRec>,
    /// Which off-site copy.
    pub copy: usize,
    /// Who signs the restore record.
    pub sig: SigChoice,
    /// Whether the record is broadcast.
    pub announce: bool,
    /// The peer of a meeting.
    pub peer: usize,
    /// The payload a device proposes.
    pub payload: u16,
    /// The position a fetch asks entries after.
    pub after: Option<ChainSeq>,
    /// The restore epoch a fetching device stands in.
    pub floor: RestoreEpoch,
    /// Whether the phase before this one found nothing to do (the job finishes early).
    pub skip_rest: bool,
    /// Whether the job was aborted (a restore changed what it worked on).
    pub aborted: bool,
    /// Whether the leader sent the answer already.
    pub acked_sent: bool,
    /// The tips a leader job read.
    pub tips: Vec<Head>,
    /// The restore epoch a leader job read.
    pub epoch: RestoreEpoch,
    /// The stamp of the entry at a checkpoint's head.
    pub stamp: Option<Stamp>,
    /// The state hash a checkpoint carries.
    pub state_hash: Option<Hash32>,
    /// The checkpoint a leader job made.
    pub checkpoint: Option<Checkpoint>,
    /// The index of the record a device job wrote to its log.
    pub wal_idx: Option<usize>,
    /// Whether a device job writes a record.
    pub will_log: bool,
    /// The verdict of a device's dry run of a head.
    pub pin_verdict: Option<PinVerdict>,
    /// Why a device refused a restore record.
    pub restore_refusal: Option<RestoreRefusal>,
    /// Entries a device found to be refused when it checked them.
    pub refused: usize,
    /// Whether a proposal was acknowledged (or already held): the job is done.
    pub finished_ok: bool,
    /// Whether the same proposal is sent again (a timeout) rather than a new one built.
    pub resend_same: bool,
    /// The comparisons of a meeting.
    pub peer_cmps: Vec<PeerCmp>,
    /// The original hash of the entry a re-proposal sends again.
    pub original: Option<Hash32>,
    /// The typed refusals a device found when it checked entries.
    pub chain_refusals: Vec<ChainRefusal>,
    /// Whether an entry conflicted with the head the device pinned.
    pub fork_with_pin: bool,
}

impl Job {
    /// A new job of this kind on this node, with nothing in it.
    pub fn new(kind: JobKind, node: usize) -> Job {
        Job {
            id: 0,
            node,
            kind,
            phase: 0,
            gen: 0,
            waiting: false,
            from: 0,
            attempts: 0,
            wake_at: 0,
            chain: None,
            proposal: None,
            sealed: None,
            replica_after: None,
            answer: None,
            entries: Vec::new(),
            head: None,
            heads: Vec::new(),
            records: Vec::new(),
            record: None,
            staged: Vec::new(),
            copy: 0,
            sig: SigChoice::Map,
            announce: true,
            peer: 0,
            payload: 0,
            after: None,
            floor: RestoreEpoch::ZERO,
            skip_rest: false,
            aborted: false,
            acked_sent: false,
            tips: Vec::new(),
            epoch: RestoreEpoch::ZERO,
            stamp: None,
            state_hash: None,
            checkpoint: None,
            wal_idx: None,
            will_log: false,
            pin_verdict: None,
            restore_refusal: None,
            refused: 0,
            finished_ok: false,
            resend_same: false,
            peer_cmps: Vec::new(),
            original: None,
            chain_refusals: Vec::new(),
            fork_with_pin: false,
        }
    }

    /// Whether the job has run every phase and waits to be retired.
    pub fn finished(&self) -> bool {
        self.phase >= self.kind.phases()
    }
}

/// Why a job could not be queued.
///
/// Implements planning/architecture.md section 1.4 rule 4 (a limit is a typed refusal).
///
/// Caller: `Sched::spawn`, `world::World`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchedRefusal {
    /// The node holds as many jobs as it may.
    NodeFull,
}

/// The jobs of every node and the interleaving tracker.
///
/// Implements planning/architecture.md section 1.4 rule 2 and rule 4.
///
/// Caller: `world::World`.
#[derive(Clone, Debug, Default)]
pub(crate) struct Sched {
    queues: BTreeMap<usize, VecDeque<Job>>,
    open: Vec<(u64, OpKind)>,
    next_id: u64,
    pairs: BTreeMap<(OpKind, OpKind), u64>,
    crash_points: BTreeMap<(JobKind, u8), u64>,
}

impl Sched {
    /// A scheduler with no job.
    pub fn new() -> Sched {
        Sched::default()
    }

    /// Queues a job (it gets its id here); a node at its bound refuses it (the world counts the
    /// refusal). Past `MAX_OPEN_JOBS` open jobs a job runs but its interleavings are not followed.
    pub fn spawn(&mut self, mut job: Job) -> Result<u64, SchedRefusal> {
        let queue = self.queues.entry(job.node).or_default();
        if queue.len() >= MAX_JOBS_PER_NODE {
            return Err(SchedRefusal::NodeFull);
        }
        self.next_id = self.next_id.saturating_add(1);
        job.id = self.next_id;
        if self.open.len() < MAX_OPEN_JOBS {
            self.open.push((job.id, job.kind.op()));
        }
        let id = job.id;
        queue.push_back(job);
        Ok(id)
    }

    /// How many jobs the busiest node holds.
    pub fn max_jobs(&self) -> usize {
        self.queues.values().map(VecDeque::len).max().unwrap_or(0)
    }

    /// How many open jobs the tracker follows.
    pub fn open_count(&self) -> usize {
        self.open.len()
    }

    /// The pairs counted so far: (X open, Y running).
    pub fn pairs(&self) -> &BTreeMap<(OpKind, OpKind), u64> {
        &self.pairs
    }

    /// The crash points counted so far.
    pub fn crash_points(&self) -> &BTreeMap<(JobKind, u8), u64> {
        &self.crash_points
    }

    /// Takes the first job of `node` that may run now: not waiting, or waiting with its wake time
    /// reached on the node's own clock `now`. A finished job is retired on the way (it ran all its
    /// phases; it only waited to be a crash point).
    pub fn take_runnable(&mut self, node: usize, now: u64) -> Option<Job> {
        let queue = self.queues.get_mut(&node)?;
        let retired: Vec<u64> = queue
            .iter()
            .filter(|job| job.finished())
            .map(|job| job.id)
            .collect();
        let found = queue
            .iter()
            .position(|job| !job.finished() && (!job.waiting || now >= job.wake_at));
        let job = found.and_then(|index| queue.remove(index));
        queue.retain(|job| !job.finished());
        self.open.retain(|(id, _)| !retired.contains(id));
        job
    }

    /// Puts a job back after a phase; a job that ran its last phase stays queued until the next
    /// poll retires it (a crash then still counts the await point after its last phase), but it is
    /// no longer open: nothing of it is left to interleave with.
    pub fn put_back(&mut self, job: Job) {
        if job.finished() {
            self.open.retain(|(id, _)| *id != job.id);
        }
        self.queues.entry(job.node).or_default().push_back(job);
    }

    /// Counts the interleaving of the phase of `running` with every other open job.
    pub fn note_phase(&mut self, running: &Job) {
        let y = running.kind.op();
        for (id, x) in &self.open {
            if *id != running.id {
                *self.pairs.entry((*x, y)).or_insert(0) += 1;
            }
        }
    }

    /// Counts a fault of kind `y` against every open job.
    pub fn note_fault(&mut self, y: OpKind) {
        for (_, x) in &self.open {
            *self.pairs.entry((*x, y)).or_insert(0) += 1;
        }
    }

    /// A process crash: every job of the node is lost, and the await point each stood at is
    /// counted. The number of jobs lost.
    pub fn crash_node(&mut self, node: usize) -> usize {
        let Some(queue) = self.queues.get_mut(&node) else {
            return 0;
        };
        let lost: Vec<(u64, JobKind, u8)> = queue.iter().map(|j| (j.id, j.kind, j.phase)).collect();
        queue.clear();
        for (id, kind, phase) in &lost {
            self.open.retain(|(open, _)| open != id);
            if *phase > 0 {
                *self.crash_points.entry((*kind, *phase)).or_insert(0) += 1;
            }
        }
        lost.len()
    }

    /// The jobs of a node that are not finished, as (kind, phase), for the state dump of a failure.
    pub fn open_of(&self, node: usize) -> Vec<(JobKind, u8)> {
        self.queues
            .get(&node)
            .map(|q| {
                q.iter()
                    .filter(|j| !j.finished())
                    .map(|j| (j.kind, j.phase))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the node has a job that is not finished and that `pred` says yes to.
    pub fn any_job(&self, node: usize, pred: impl Fn(&Job) -> bool) -> bool {
        self.queues
            .get(&node)
            .is_some_and(|q| q.iter().any(|j| !j.finished() && pred(j)))
    }

    /// Hands a message to the job of this kind that waits for it: the first waiting job of the
    /// node of this kind for which `matches` says yes. Whether one was found.
    pub fn deliver_to_waiting(
        &mut self,
        node: usize,
        kind: JobKind,
        matches: impl Fn(&Job) -> bool,
        answer: AckResult,
    ) -> bool {
        let Some(queue) = self.queues.get_mut(&node) else {
            return false;
        };
        for job in queue.iter_mut() {
            if job.kind == kind && job.waiting && !job.finished() && matches(job) {
                job.answer = Some(answer);
                job.wake_at = 0;
                return true;
            }
        }
        false
    }

    /// Whether any queue holds a job that is not finished.
    pub fn any_unfinished(&self) -> bool {
        self.queues
            .values()
            .any(|q| q.iter().any(|j| !j.finished()))
    }

    /// The nodes that hold a job that is not finished.
    pub fn busy_nodes(&self) -> Vec<usize> {
        self.queues
            .iter()
            .filter(|(_, q)| q.iter().any(|j| !j.finished()))
            .map(|(node, _)| *node)
            .collect()
    }
}
