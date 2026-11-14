//! The coverage report (feature (f) of the brief; planning/architecture.md section 1.4 rule 2: a
//! simulator that injects every fault, and a report that says it did).
//!
//! [`Coverage`] counts (1) OPERATION PAIRS: for every two kinds of operation, how often a phase of
//! one ran while the other was open (interleaved) and how often two kinds of step were adjacent;
//! (2) the crash points: (handler, await point) pairs crashed at; (3) the faults fired, by kind; (4)
//! the attacks, by kind and the verdict each met; (5) the refusals by class; (6) the invariant
//! checks by id; (7) the skips; (8) the footprint peaks. [`REQUIRED_PAIRS`] lists the pairs that
//! must be interleaved at least three times in the fast run.

use std::collections::BTreeMap;

use crate::invariants::InvariantId;
use crate::sched::JobKind;
use crate::step::{AttackKind, StepKind};

/// The kinds of operation the interleaving tracker knows: the jobs of the nodes and the faults.
///
/// Implements planning/architecture.md section 1.4 rule 2 (faults interleave with every operation).
///
/// Caller: `sched::Sched`, `Coverage`, `REQUIRED_PAIRS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpKind {
    /// The leader appends an entry (`L.append`).
    Append,
    /// The leader signs and sends heads (`L.head`).
    Head,
    /// The leader answers a fetch (`L.fetch`).
    Fetch,
    /// Two devices meet (`D.meet`).
    Meet,
    /// The leader records a checkpoint (`L.checkpoint`).
    Checkpoint,
    /// The leader restores (`L.restore`).
    Restore,
    /// The leader takes an off-site copy (`L.offsite`).
    Offsite,
    /// A device takes a head (`D.on_head`).
    OnHead,
    /// A device takes entries (`D.on_entries`).
    OnEntries,
    /// A device follows a restore record (`D.on_restore`).
    OnRestore,
    /// A device proposes an entry (`D.propose`).
    Propose,
    /// The adversary acts.
    Attack,
    /// A node crashes.
    Crash,
    /// A partition is drawn.
    Partition,
    /// A clock jumps or skews.
    ClockJump,
    /// A disk fault is armed or fires.
    DiskFault,
}

impl OpKind {
    /// Every kind.
    pub const ALL: [OpKind; 16] = [
        OpKind::Append,
        OpKind::Head,
        OpKind::Fetch,
        OpKind::Meet,
        OpKind::Checkpoint,
        OpKind::Restore,
        OpKind::Offsite,
        OpKind::OnHead,
        OpKind::OnEntries,
        OpKind::OnRestore,
        OpKind::Propose,
        OpKind::Attack,
        OpKind::Crash,
        OpKind::Partition,
        OpKind::ClockJump,
        OpKind::DiskFault,
    ];

    /// The name of the kind in a report.
    pub const fn name(self) -> &'static str {
        match self {
            OpKind::Append => "append",
            OpKind::Head => "head",
            OpKind::Fetch => "fetch",
            OpKind::Meet => "meet",
            OpKind::Checkpoint => "checkpoint",
            OpKind::Restore => "restore",
            OpKind::Offsite => "offsite",
            OpKind::OnHead => "on_head",
            OpKind::OnEntries => "on_entries",
            OpKind::OnRestore => "on_restore",
            OpKind::Propose => "propose",
            OpKind::Attack => "attack",
            OpKind::Crash => "crash",
            OpKind::Partition => "partition",
            OpKind::ClockJump => "clock_jump",
            OpKind::DiskFault => "disk_fault",
        }
    }
}

/// The pairs that must be interleaved at least three times in the fast run: an operation X that is
/// open while an operation Y runs, for X in {append, head, fetch, meet, checkpoint, restore} and Y in
/// {append, head, fetch, meet, checkpoint, restore, attack, crash, partition, clock jump, disk
/// fault}, minus [`EXCLUDED_PAIRS`].
///
/// Implements planning/architecture.md section 1.4 rule 2 (the dangerous interleavings are
/// exercised, not hoped for).
///
/// Caller: `Coverage::missing_required`, tests/x05.
pub const REQUIRED_PAIRS: &[(OpKind, OpKind)] = &[
    (OpKind::Append, OpKind::Append),
    (OpKind::Append, OpKind::Head),
    (OpKind::Append, OpKind::Fetch),
    (OpKind::Append, OpKind::Meet),
    (OpKind::Append, OpKind::Checkpoint),
    (OpKind::Append, OpKind::Restore),
    (OpKind::Append, OpKind::Attack),
    (OpKind::Append, OpKind::Crash),
    (OpKind::Append, OpKind::Partition),
    (OpKind::Append, OpKind::ClockJump),
    (OpKind::Append, OpKind::DiskFault),
    (OpKind::Head, OpKind::Append),
    (OpKind::Head, OpKind::Head),
    (OpKind::Head, OpKind::Fetch),
    (OpKind::Head, OpKind::Meet),
    (OpKind::Head, OpKind::Checkpoint),
    (OpKind::Head, OpKind::Restore),
    (OpKind::Head, OpKind::Attack),
    (OpKind::Head, OpKind::Crash),
    (OpKind::Head, OpKind::Partition),
    (OpKind::Head, OpKind::ClockJump),
    (OpKind::Head, OpKind::DiskFault),
    (OpKind::Fetch, OpKind::Append),
    (OpKind::Fetch, OpKind::Head),
    (OpKind::Fetch, OpKind::Fetch),
    (OpKind::Fetch, OpKind::Meet),
    (OpKind::Fetch, OpKind::Checkpoint),
    (OpKind::Fetch, OpKind::Restore),
    (OpKind::Fetch, OpKind::Attack),
    (OpKind::Fetch, OpKind::Crash),
    (OpKind::Fetch, OpKind::Partition),
    (OpKind::Fetch, OpKind::ClockJump),
    (OpKind::Fetch, OpKind::DiskFault),
    (OpKind::Meet, OpKind::Append),
    (OpKind::Meet, OpKind::Head),
    (OpKind::Meet, OpKind::Fetch),
    (OpKind::Meet, OpKind::Meet),
    (OpKind::Meet, OpKind::Checkpoint),
    (OpKind::Meet, OpKind::Restore),
    (OpKind::Meet, OpKind::Attack),
    (OpKind::Meet, OpKind::Crash),
    (OpKind::Meet, OpKind::Partition),
    (OpKind::Meet, OpKind::ClockJump),
    (OpKind::Meet, OpKind::DiskFault),
    (OpKind::Checkpoint, OpKind::Append),
    (OpKind::Checkpoint, OpKind::Head),
    (OpKind::Checkpoint, OpKind::Fetch),
    (OpKind::Checkpoint, OpKind::Meet),
    (OpKind::Checkpoint, OpKind::Checkpoint),
    (OpKind::Checkpoint, OpKind::Restore),
    (OpKind::Checkpoint, OpKind::Attack),
    (OpKind::Checkpoint, OpKind::Crash),
    (OpKind::Checkpoint, OpKind::Partition),
    (OpKind::Checkpoint, OpKind::ClockJump),
    (OpKind::Checkpoint, OpKind::DiskFault),
    (OpKind::Restore, OpKind::Meet),
    (OpKind::Restore, OpKind::Attack),
    (OpKind::Restore, OpKind::Crash),
    (OpKind::Restore, OpKind::Partition),
    (OpKind::Restore, OpKind::ClockJump),
    (OpKind::Restore, OpKind::DiskFault),
];

/// The pairs of the product of the required kinds that are left out, each with its reason: while a
/// restore is open the leader holds its restore lock, so it starts no append, head, checkpoint,
/// fetch or second restore (glue rule G19).
///
/// Implements planning/architecture.md section 6.10 V3 (a restore is one atomic announcement).
///
/// Caller: tests/x05, the report.
pub const EXCLUDED_PAIRS: &[((OpKind, OpKind), &str)] = &[
    (
        (OpKind::Restore, OpKind::Append),
        "the restore lock: no append starts while a restore is open (G19)",
    ),
    (
        (OpKind::Restore, OpKind::Head),
        "the restore lock: no head is signed while a restore is open (G19)",
    ),
    (
        (OpKind::Restore, OpKind::Fetch),
        "the restore lock: no fetch is answered while a restore is open (G19)",
    ),
    (
        (OpKind::Restore, OpKind::Checkpoint),
        "the restore lock: no checkpoint starts while a restore is open (G19)",
    ),
    (
        (OpKind::Restore, OpKind::Restore),
        "the restore lock: one restore at a time (G19)",
    ),
];

/// The kinds of fault the world counts.
///
/// Implements planning/architecture.md section 1.4 rule 2 (the simulator injects every fault).
///
/// Caller: `Coverage::faults`, tests/x04.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FaultKind {
    /// A message lost.
    Loss,
    /// A message duplicated.
    Duplication,
    /// A message reordered.
    Reordering,
    /// A message corrupted in flight.
    Corruption,
    /// A partition drawn.
    Partition,
    /// A slow node drawn.
    SlowNode,
    /// A clock skew drawn.
    ClockSkew,
    /// A clock jump drawn.
    ClockJump,
    /// A process crash.
    Crash,
    /// A crash that tore a record.
    TornWrite,
    /// An fsync that lied.
    LostFsync,
    /// A read that failed.
    ReadError,
    /// A bit flipped.
    BitFlip,
    /// A disk that vanished.
    Vanish,
}

impl FaultKind {
    /// Every kind.
    pub const ALL: [FaultKind; 14] = [
        FaultKind::Loss,
        FaultKind::Duplication,
        FaultKind::Reordering,
        FaultKind::Corruption,
        FaultKind::Partition,
        FaultKind::SlowNode,
        FaultKind::ClockSkew,
        FaultKind::ClockJump,
        FaultKind::Crash,
        FaultKind::TornWrite,
        FaultKind::LostFsync,
        FaultKind::ReadError,
        FaultKind::BitFlip,
        FaultKind::Vanish,
    ];

    /// The name of the kind in a report.
    pub const fn name(self) -> &'static str {
        match self {
            FaultKind::Loss => "loss",
            FaultKind::Duplication => "duplication",
            FaultKind::Reordering => "reordering",
            FaultKind::Corruption => "corruption",
            FaultKind::Partition => "partition",
            FaultKind::SlowNode => "slow_node",
            FaultKind::ClockSkew => "clock_skew",
            FaultKind::ClockJump => "clock_jump",
            FaultKind::Crash => "crash",
            FaultKind::TornWrite => "torn_write",
            FaultKind::LostFsync => "lost_fsync",
            FaultKind::ReadError => "read_error",
            FaultKind::BitFlip => "bit_flip",
            FaultKind::Vanish => "vanish",
        }
    }
}

/// How often an attack was tried and how often it met the verdict class it must.
///
/// Implements planning/architecture.md section 6.10 S15 (a malicious server meets a refusal).
///
/// Caller: `Coverage::attacks`, tests/x04.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttackTally {
    /// Attempts.
    pub attempted: u64,
    /// Attempts that met the verdict class of section 4.5 of the brief.
    pub met: u64,
}

/// The footprint of a world at one moment: the sizes the limits of rule 4 bound.
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit).
///
/// Caller: `Coverage::peak`, `invariants` (the limits invariant), tests/x11.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Footprint {
    /// Messages in flight.
    pub inflight: usize,
    /// The most jobs of one node.
    pub max_jobs_per_node: usize,
    /// Open jobs.
    pub open_jobs: usize,
    /// The most alarms one node holds.
    pub max_alarms: usize,
    /// The most records one disk holds.
    pub max_disk_records: usize,
    /// Off-site copies.
    pub offsite_copies: usize,
    /// Restores done.
    pub restores: usize,
    /// Trace lines kept in full.
    pub trace_tail: usize,
}

impl Footprint {
    /// The pointwise maximum of two footprints.
    #[must_use]
    pub fn max(self, other: Footprint) -> Footprint {
        Footprint {
            inflight: self.inflight.max(other.inflight),
            max_jobs_per_node: self.max_jobs_per_node.max(other.max_jobs_per_node),
            open_jobs: self.open_jobs.max(other.open_jobs),
            max_alarms: self.max_alarms.max(other.max_alarms),
            max_disk_records: self.max_disk_records.max(other.max_disk_records),
            offsite_copies: self.offsite_copies.max(other.offsite_copies),
            restores: self.restores.max(other.restores),
            trace_tail: self.trace_tail.max(other.trace_tail),
        }
    }
}

/// What a run (or many) exercised.
///
/// Implements planning/architecture.md section 1.4 rule 2 (the report says what was injected).
///
/// Caller: `Outcome`, `Report`, tests/x04, tests/x05.
#[derive(Clone, Debug, Default)]
pub struct Coverage {
    /// For (X, Y): a phase of Y ran, or a fault Y fired, while X was open.
    pub interleaved: BTreeMap<(OpKind, OpKind), u64>,
    /// How often a step of one kind followed a step of the other.
    pub adjacent: BTreeMap<(StepKind, StepKind), u64>,
    /// The (handler, await point) pairs crashed at: the number of phases the job had completed.
    pub crash_points: BTreeMap<(JobKind, u8), u64>,
    /// The faults fired.
    pub faults: BTreeMap<FaultKind, u64>,
    /// The attacks by kind.
    pub attacks: BTreeMap<AttackKind, AttackTally>,
    /// The refusals by class word (`refuse`, `alarm`, `refetch`).
    pub refusals: BTreeMap<&'static str, u64>,
    /// The invariant checks by id.
    pub checks: BTreeMap<InvariantId, u64>,
    /// The skipped steps by verb.
    pub skips: BTreeMap<StepKind, u64>,
    /// The steps executed (skips included).
    pub steps: u64,
    /// The steps that were skips.
    pub skipped: u64,
    /// The peak of the footprint.
    pub peak: Footprint,
}

impl Coverage {
    /// Adds the counts of another run.
    pub fn merge(&mut self, other: &Coverage) {
        for (key, value) in &other.interleaved {
            *self.interleaved.entry(*key).or_insert(0) += value;
        }
        for (key, value) in &other.adjacent {
            *self.adjacent.entry(*key).or_insert(0) += value;
        }
        for (key, value) in &other.crash_points {
            *self.crash_points.entry(*key).or_insert(0) += value;
        }
        for (key, value) in &other.faults {
            *self.faults.entry(*key).or_insert(0) += value;
        }
        for (key, value) in &other.attacks {
            let tally = self.attacks.entry(*key).or_default();
            tally.attempted += value.attempted;
            tally.met += value.met;
        }
        for (key, value) in &other.refusals {
            *self.refusals.entry(*key).or_insert(0) += value;
        }
        for (key, value) in &other.checks {
            *self.checks.entry(*key).or_insert(0) += value;
        }
        for (key, value) in &other.skips {
            *self.skips.entry(*key).or_insert(0) += value;
        }
        self.steps += other.steps;
        self.skipped += other.skipped;
        self.peak = self.peak.max(other.peak);
    }

    /// How often X was open while Y ran.
    pub fn pair(&self, open: OpKind, running: OpKind) -> u64 {
        self.interleaved.get(&(open, running)).copied().unwrap_or(0)
    }

    /// The required pairs that were interleaved fewer than three times.
    pub fn missing_required(&self) -> Vec<(OpKind, OpKind)> {
        REQUIRED_PAIRS
            .iter()
            .copied()
            .filter(|(open, running)| self.pair(*open, *running) < 3)
            .collect()
    }

    /// How often a fault of this kind fired.
    pub fn fault(&self, kind: FaultKind) -> u64 {
        self.faults.get(&kind).copied().unwrap_or(0)
    }

    /// The report text: one fact per line, in a fixed format (two nights can be diffed).
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("steps {} skipped {}\n", self.steps, self.skipped));
        for kind in FaultKind::ALL {
            out.push_str(&format!("fault {} {}\n", kind.name(), self.fault(kind)));
        }
        for (kind, tally) in &self.attacks {
            out.push_str(&format!(
                "attack {} attempted {} met {}\n",
                kind.name(),
                tally.attempted,
                tally.met
            ));
        }
        for ((kind, phase), count) in &self.crash_points {
            out.push_str(&format!(
                "crash_point {} {} {}\n",
                kind.handler().name,
                phase,
                count
            ));
        }
        for (class, count) in &self.refusals {
            out.push_str(&format!("refusal {class} {count}\n"));
        }
        for (id, count) in &self.checks {
            out.push_str(&format!("check {} {}\n", id.name(), count));
        }
        let missing = self.missing_required();
        out.push_str(&format!("required_pairs_missing {}\n", missing.len()));
        for (open, running) in missing {
            out.push_str(&format!("missing {} {}\n", open.name(), running.name()));
        }
        out
    }
}
