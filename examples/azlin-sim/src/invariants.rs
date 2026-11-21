//! The invariants of the plan as named properties, and the checkers (planning/architecture.md
//! section 5.9, section 5.13, section 6.10 S15 V1 V3, section 2.4, section 2.17, section 12 E4,
//! section 1.4 rules 2, 4 and 10).
//!
//! A checker is a function of the world and the LAST STEP's DELTA, never of the machinery under
//! test alone: it compares a crate's answer with an INDEPENDENT oracle the harness keeps (the
//! max-seq watermark of valid heads, the set of heads the group ever signed, the set of restore
//! records the operator ever signed, a ledger of what the leader acknowledged). The oracles are
//! small and obviously right; a checker that re-implements the crate under test finds nothing.
//! [`INVARIANTS`] is the registry: one row for each property, with the plan section it comes from,
//! its status (`Enforced`, or a known gap that a confirmed finding keeps visible without turning
//! every run red) and the test that plants a violation of it.

use std::collections::{BTreeMap, BTreeSet};

use azlin_chain::{
    verify_from_checkpoint, verify_from_genesis, ChainEntry, ChainId, PeerVerdict,
};
use azlin_seams::{ChainSeq, Hash32, RestoreEpoch, Stamp};

use crate::config::{
    Stage, MAX_ALARMS, MAX_DISK_RECORDS, MAX_INFLIGHT, MAX_JOBS_PER_NODE, MAX_OFFSITE_COPIES,
    MAX_OPEN_JOBS, MAX_RESTORES, MAX_TRACE_TAIL,
};
use crate::machine::OpaqueEntry;
use crate::policy_world::PolicyKind;
use crate::world::{StampEvent, World};

/// Whether an invariant is enforced, or a confirmed finding keeps it as a known gap.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a simulator that finds defects).
///
/// Caller: `InvariantInfo`, `Outcome`, tests/seeds.rs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// A failure is a product failure.
    Enforced,
    /// A confirmed finding of the named drift row: the checker runs, the failure is reported and kept
    /// as an `expect: fails` seed, and the fast test does not fail on it.
    KnownGap(&'static str),
}

/// One row of the registry of invariants.
///
/// Implements planning/architecture.md section 1.4 rule 2 (named properties of the plan).
///
/// Caller: `INVARIANTS`, tests/hygiene.rs, tests/x12.
#[derive(Clone, Copy, Debug)]
pub struct InvariantInfo {
    /// The invariant.
    pub id: InvariantId,
    /// Its name.
    pub name: &'static str,
    /// The section numbers of the plan it comes from.
    pub plan: &'static str,
    /// The remark of the table that is not a section number.
    pub note: &'static str,
    /// The stage that introduced it.
    pub stage: Stage,
    /// Whether it is enforced.
    pub status: Status,
    /// The test that plants a violation of it.
    pub planted: &'static str,
}

/// The invariants of stage 1 (the order is the order of the registry).
///
/// Implements planning/architecture.md section 1.4 rule 2 and the sections of `INVARIANTS`.
///
/// Caller: `Checker`, `Failure`, `Coverage`, tests/x12.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InvariantId {
    /// Every chain copy verifies from its start.
    ChainEveryCopyVerifiesFromGenesis,
    /// No fork is ever accepted.
    ChainNoForkIsEverAccepted,
    /// A checkpoint equals the genesis verification.
    ChainCheckpointEqualsGenesis,
    /// A checkpoint is written when due.
    ChainACheckpointIsWrittenWhenDue,
    /// A restore is followed only through the signed epoch path.
    RestoreOnlyThroughTheSignedEpochPath,
    /// The lost timeline stays lost.
    RestoreTheLostTimelineStaysLost,
    /// A late record keeps progress.
    RestoreALateRecordKeepsProgress,
    /// Every listed chain is re-anchored.
    RestoreEveryListedChainIsReanchored,
    /// A pinned head's epoch never goes down.
    PinTheEpochNeverGoesDown,
    /// A pin only holds heads the group signed.
    PinOnlyHoldsHeadsTheGroupSigned,
    /// A pin never goes backwards.
    PinNeverGoesBackwards,
    /// Held entries are sent again.
    RestoreHeldEntriesAreSentAgain,
    /// A restore heals every device.
    RestoreHealsEveryDevice,
    /// A hidden head shows.
    PeerAHiddenHeadShows,
    /// A peer's junk is no alarm.
    PeerAPeersJunkIsNoAlarm,
    /// The stamps never run backwards.
    StampNeverRunsBackwards,
    /// Acknowledged entries survive an honest disk.
    LeaderAcknowledgedEntriesSurviveAnHonestDisk,
    /// No silent rollback window.
    LeaderNoSilentRollbackWindow,
    /// One head per position and epoch.
    LeaderOneHeadPerPositionAndEpoch,
    /// A device's restart recovers a state it had.
    DeviceRestartRecoversAStateItHad,
    /// Every refusal is typed and classed.
    RefusalEveryRefusalIsTypedAndClassed,
    /// Nothing grows without a limit.
    LimitsNothingGrowsWithoutALimit,
    /// A group only tightens.
    PolicyAGroupOnlyTightens,
    /// The policy epoch bumps only on a change.
    PolicyEpochBumpsOnlyOnChange,
    /// A quorum counts distinct people.
    PolicyQuorumCountsDistinctPeople,
}

const fn row(
    id: InvariantId,
    name: &'static str,
    plan: &'static str,
    note: &'static str,
    planted: &'static str,
) -> InvariantInfo {
    InvariantInfo {
        id,
        name,
        plan,
        note,
        stage: Stage::S1,
        status: Status::Enforced,
        planted,
    }
}

const INFO: [InvariantInfo; 25] = [
    row(
        InvariantId::ChainEveryCopyVerifiesFromGenesis,
        "inv_chain_every_copy_verifies_from_genesis",
        "5.9",
        "",
        "planted_inv_chain_every_copy_verifies_from_genesis",
    ),
    row(
        InvariantId::ChainNoForkIsEverAccepted,
        "inv_chain_no_fork_is_ever_accepted",
        "5.9",
        "",
        "planted_inv_chain_no_fork_is_ever_accepted",
    ),
    row(
        InvariantId::ChainCheckpointEqualsGenesis,
        "inv_chain_checkpoint_equals_genesis",
        "5.9",
        "",
        "planted_inv_chain_checkpoint_equals_genesis",
    ),
    row(
        InvariantId::ChainACheckpointIsWrittenWhenDue,
        "inv_chain_a_checkpoint_is_written_when_due",
        "5.9",
        "",
        "planted_inv_chain_a_checkpoint_is_written_when_due",
    ),
    row(
        InvariantId::RestoreOnlyThroughTheSignedEpochPath,
        "inv_restore_only_through_the_signed_epoch_path",
        "2.17, 5.13, 6.10, 12",
        "V3, E4",
        "planted_inv_restore_only_through_the_signed_epoch_path",
    ),
    row(
        InvariantId::RestoreTheLostTimelineStaysLost,
        "inv_restore_the_lost_timeline_stays_lost",
        "6.10",
        "V3, S15; D-W1WATCH 1",
        "planted_inv_restore_the_lost_timeline_stays_lost",
    ),
    row(
        InvariantId::RestoreALateRecordKeepsProgress,
        "inv_restore_a_late_record_keeps_progress",
        "5.13",
        "D-W1WATCH 1; the decision is by restore epoch, not by clock (drift row X-DELTA-W1SIM-7)",
        "planted_inv_restore_a_late_record_keeps_progress",
    ),
    row(
        InvariantId::RestoreEveryListedChainIsReanchored,
        "inv_restore_every_listed_chain_is_reanchored",
        "2.17",
        "",
        "planted_inv_restore_every_listed_chain_is_reanchored",
    ),
    row(
        InvariantId::PinTheEpochNeverGoesDown,
        "inv_pin_the_epoch_never_goes_down",
        "6.10",
        "V3, S15; D-W1WATCH 1; a device that saw the new epoch's head before the record (drift row X-DELTA-W1SIM-3)",
        "planted_inv_pin_the_epoch_never_goes_down",
    ),
    row(
        InvariantId::PinOnlyHoldsHeadsTheGroupSigned,
        "inv_pin_only_holds_heads_the_group_signed",
        "5.9",
        "",
        "planted_inv_pin_only_holds_heads_the_group_signed",
    ),
    row(
        InvariantId::PinNeverGoesBackwards,
        "inv_pin_never_goes_backwards",
        "5.13, 6.10",
        "S15",
        "planted_inv_pin_never_goes_backwards",
    ),
    row(
        InvariantId::RestoreHeldEntriesAreSentAgain,
        "inv_restore_held_entries_are_sent_again",
        "2.17, 6.10",
        "V3",
        "planted_inv_restore_held_entries_are_sent_again",
    ),
    row(
        InvariantId::RestoreHealsEveryDevice,
        "inv_restore_heals_every_device",
        "6.10",
        "V3",
        "planted_inv_restore_heals_every_device",
    ),
    row(
        InvariantId::PeerAHiddenHeadShows,
        "inv_peer_a_hidden_head_shows",
        "5.9",
        "",
        "planted_inv_peer_a_hidden_head_shows",
    ),
    row(
        InvariantId::PeerAPeersJunkIsNoAlarm,
        "inv_peer_a_peers_junk_is_no_alarm",
        "5.9, 1.4",
        "rule 10",
        "planted_inv_peer_a_peers_junk_is_no_alarm",
    ),
    row(
        InvariantId::StampNeverRunsBackwards,
        "inv_stamp_never_runs_backwards",
        "2.4",
        "",
        "planted_inv_stamp_never_runs_backwards",
    ),
    row(
        InvariantId::LeaderAcknowledgedEntriesSurviveAnHonestDisk,
        "inv_leader_acknowledged_entries_survive_an_honest_disk",
        "6.10, 1.4",
        "V1, rule 2",
        "planted_inv_leader_acknowledged_entries_survive_an_honest_disk",
    ),
    row(
        InvariantId::LeaderNoSilentRollbackWindow,
        "inv_leader_no_silent_rollback_window",
        "6.10",
        "V3, S15",
        "planted_inv_leader_no_silent_rollback_window",
    ),
    row(
        InvariantId::LeaderOneHeadPerPositionAndEpoch,
        "inv_leader_one_head_per_position_and_epoch",
        "5.9",
        "",
        "planted_inv_leader_one_head_per_position_and_epoch",
    ),
    row(
        InvariantId::DeviceRestartRecoversAStateItHad,
        "inv_device_restart_recovers_a_state_it_had",
        "1.4",
        "rule 2",
        "planted_inv_device_restart_recovers_a_state_it_had",
    ),
    row(
        InvariantId::RefusalEveryRefusalIsTypedAndClassed,
        "inv_refusal_every_refusal_is_typed_and_classed",
        "1.4",
        "rule 10",
        "planted_inv_refusal_every_refusal_is_typed_and_classed",
    ),
    row(
        InvariantId::LimitsNothingGrowsWithoutALimit,
        "inv_limits_nothing_grows_without_a_limit",
        "1.4",
        "rule 4",
        "planted_inv_limits_nothing_grows_without_a_limit",
    ),
    row(
        InvariantId::PolicyAGroupOnlyTightens,
        "inv_policy_a_group_only_tightens",
        "5.10, 7.2",
        "",
        "planted_inv_policy_a_group_only_tightens",
    ),
    row(
        InvariantId::PolicyEpochBumpsOnlyOnChange,
        "inv_policy_epoch_bumps_only_on_change",
        "5.11",
        "",
        "planted_inv_policy_epoch_bumps_only_on_change",
    ),
    row(
        InvariantId::PolicyQuorumCountsDistinctPeople,
        "inv_policy_quorum_counts_distinct_people",
        "12, 5.10",
        "E1",
        "planted_inv_policy_quorum_counts_distinct_people",
    ),
];

/// The registry: one row for each invariant, in the order of [`InvariantId`].
///
/// Implements planning/architecture.md section 1.4 rule 2 and the sections each row names.
///
/// Caller: `InvariantId::info`, tests/hygiene.rs, tests/x12.
pub const INVARIANTS: &[InvariantInfo] = &INFO;

impl InvariantId {
    /// Every invariant, in the order of the registry.
    pub fn all() -> impl Iterator<Item = InvariantId> {
        INFO.iter().map(|info| info.id)
    }

    /// The registry row of this invariant.
    pub fn info(self) -> &'static InvariantInfo {
        match INVARIANTS.get(self as usize) {
            Some(info) => info,
            None => &INFO[0],
        }
    }

    /// The name of this invariant.
    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// The invariant of this name.
    pub fn from_name(name: &str) -> Option<InvariantId> {
        INFO.iter()
            .find(|info| info.name == name)
            .map(|info| info.id)
    }
}

/// A broken invariant: which one and why.
#[derive(Clone, Debug)]
pub(crate) struct Violation {
    pub(crate) id: InvariantId,
    pub(crate) message: String,
}

/// The checkers and the little state they keep between steps.
#[derive(Debug, Default)]
pub(crate) struct Checker {
    tripped: BTreeSet<InvariantId>,
    pub(crate) counts: BTreeMap<InvariantId, u64>,
    last_stamp: BTreeMap<ChainId, Stamp>,
    served: BTreeMap<ChainId, (RestoreEpoch, ChainSeq)>,
    heads: BTreeMap<(ChainId, ChainSeq, RestoreEpoch), Hash32>,
}

impl Checker {
    pub(crate) fn new() -> Checker {
        Checker::default()
    }

    /// Stops checking an invariant (a known gap that tripped keeps the run going without it).
    pub(crate) fn trip(&mut self, id: InvariantId) {
        self.tripped.insert(id);
    }

    fn go(
        &mut self,
        out: &mut Vec<Violation>,
        id: InvariantId,
        check: impl FnOnce(&mut Checker) -> Option<String>,
    ) {
        if self.tripped.contains(&id) {
            return;
        }
        *self.counts.entry(id).or_insert(0) += 1;
        if let Some(message) = check(self) {
            out.push(Violation { id, message });
        }
    }

    /// The checks after every step; `deep` adds the whole-world scans.
    pub(crate) fn after_step(&mut self, w: &World, deep: bool) -> Vec<Violation> {
        let mut out = Vec::new();
        self.go(
            &mut out,
            InvariantId::RestoreOnlyThroughTheSignedEpochPath,
            |_| c_restore_path(w),
        );
        self.go(
            &mut out,
            InvariantId::RestoreALateRecordKeepsProgress,
            |_| c_late_record(w),
        );
        self.go(
            &mut out,
            InvariantId::RestoreEveryListedChainIsReanchored,
            |_| c_listed_chains(w),
        );
        self.go(
            &mut out,
            InvariantId::RestoreTheLostTimelineStaysLost,
            |_| c_lost_timeline(w),
        );
        self.go(
            &mut out,
            InvariantId::ChainEveryCopyVerifiesFromGenesis,
            |_| c_copies(w, deep),
        );
        self.go(&mut out, InvariantId::ChainNoForkIsEverAccepted, |_| {
            c_no_fork(w)
        });
        self.go(&mut out, InvariantId::PinNeverGoesBackwards, |_| {
            c_pin_backwards(w)
        });
        self.go(
            &mut out,
            InvariantId::PinOnlyHoldsHeadsTheGroupSigned,
            |_| c_pin_signed(w),
        );
        self.go(&mut out, InvariantId::PinTheEpochNeverGoesDown, |_| {
            c_pin_epoch(w)
        });
        self.go(&mut out, InvariantId::PeerAHiddenHeadShows, |_| {
            c_peer_hidden(w)
        });
        self.go(&mut out, InvariantId::PeerAPeersJunkIsNoAlarm, |_| {
            c_peer_junk(w)
        });
        self.go(&mut out, InvariantId::StampNeverRunsBackwards, |me| {
            c_stamps(w, me, deep)
        });
        self.go(
            &mut out,
            InvariantId::LeaderAcknowledgedEntriesSurviveAnHonestDisk,
            |_| c_acked(w),
        );
        self.go(&mut out, InvariantId::LeaderNoSilentRollbackWindow, |me| {
            c_silent_rollback(w, me)
        });
        self.go(
            &mut out,
            InvariantId::LeaderOneHeadPerPositionAndEpoch,
            |me| c_one_head(w, me),
        );
        self.go(
            &mut out,
            InvariantId::DeviceRestartRecoversAStateItHad,
            |_| c_restart(w),
        );
        self.go(
            &mut out,
            InvariantId::RefusalEveryRefusalIsTypedAndClassed,
            |_| c_refusals(w),
        );
        self.go(
            &mut out,
            InvariantId::LimitsNothingGrowsWithoutALimit,
            |_| c_limits(w),
        );
        self.go(&mut out, InvariantId::PolicyAGroupOnlyTightens, |_| {
            c_policy(w, PolicyKind::Tightens)
        });
        self.go(&mut out, InvariantId::PolicyEpochBumpsOnlyOnChange, |_| {
            c_policy(w, PolicyKind::EpochBump)
        });
        self.go(
            &mut out,
            InvariantId::PolicyQuorumCountsDistinctPeople,
            |_| c_policy(w, PolicyKind::Quorum),
        );
        if deep {
            self.go(&mut out, InvariantId::ChainCheckpointEqualsGenesis, |_| {
                c_checkpoints(w)
            });
        }
        out.sort_by_key(|v| v.id);
        out
    }

    /// The checks after a quiesce: everything has settled, so convergence can be asked for.
    pub(crate) fn after_quiesce(&mut self, w: &World) -> Vec<Violation> {
        let mut out = self.after_step(w, true);
        self.go(
            &mut out,
            InvariantId::ChainACheckpointIsWrittenWhenDue,
            |_| c_checkpoint_due(w),
        );
        self.go(
            &mut out,
            InvariantId::RestoreHeldEntriesAreSentAgain,
            |_| c_held(w),
        );
        self.go(&mut out, InvariantId::RestoreHealsEveryDevice, |_| {
            c_heal(w)
        });
        self.go(
            &mut out,
            InvariantId::LeaderAcknowledgedEntriesSurviveAnHonestDisk,
            |_| c_loss_shows(w),
        );
        if w.delta.notes.iter().any(|n| n == "quiesce:not-converged") && !w.ledger.degraded {
            out.push(Violation {
                id: InvariantId::RestoreHealsEveryDevice,
                message: String::from("quiesce did not converge: the world never settled"),
            });
        }
        out.sort_by_key(|v| v.id);
        out
    }
}

// -------------------------------------------------------------------------------------------
// The checks
// -------------------------------------------------------------------------------------------

/// Whether a run of entries is a sound copy: every entry of this chain and intact, positions one
/// after the other, every `prev` the hash before it, a genesis with a zero `prev`. The first
/// entry of a copy that starts later is the base.
fn check_run(chain: ChainId, entries: &[OpaqueEntry]) -> Result<(), String> {
    let mut previous: Option<&OpaqueEntry> = None;
    for e in entries {
        if e.chain != chain {
            return Err(format!(
                "an entry of {} sits in the copy of {chain}",
                e.chain
            ));
        }
        if !e.is_intact() {
            return Err(format!(
                "a copy at position {} of {chain} is not what its hash says",
                e.seq
            ));
        }
        match previous {
            None => {
                if e.seq == ChainSeq::MIN && !e.prev.is_zero() {
                    return Err(format!("the genesis of {chain} names a predecessor"));
                }
            }
            Some(p) => {
                if p.seq.next() != Some(e.seq) {
                    return Err(format!(
                        "positions {} and {} of {chain} do not follow each other",
                        p.seq, e.seq
                    ));
                }
                if e.prev != p.hash {
                    return Err(format!(
                        "position {} of {chain} is not chained to the one before it",
                        e.seq
                    ));
                }
            }
        }
        previous = Some(e);
    }
    Ok(())
}

/// Whether the stamps and the epochs of a run never decrease (the crate's own stamp and epoch rules
/// are other invariants' business; the crate's `verify_from_genesis` is only asked of a run that
/// keeps them).
fn monotone(entries: &[OpaqueEntry]) -> bool {
    entries.windows(2).all(|pair| match pair {
        [a, b] => b.at >= a.at && b.epoch >= a.epoch,
        _ => true,
    })
}

fn nodes_to_check(w: &World, deep: bool) -> Vec<usize> {
    if deep {
        (0..w.node_count()).collect()
    } else {
        w.delta.touched.iter().copied().collect()
    }
}

/// How many of the newest entries of a copy every step looks at; the whole copy is looked at on the
/// deep steps (every 25th and after a quiesce).
const SHALLOW_ENTRIES: usize = 8;

fn newest(entries: &[OpaqueEntry], deep: bool) -> &[OpaqueEntry] {
    if deep {
        entries
    } else {
        entries
            .get(entries.len().saturating_sub(SHALLOW_ENTRIES)..)
            .unwrap_or(entries)
    }
}

fn c_copies(w: &World, deep: bool) -> Option<String> {
    for node in nodes_to_check(w, deep) {
        if node == 0 {
            if !w.leader.up {
                continue;
            }
            for (id, lc) in &w.leader.chains {
                let entries = lc.chain.entries();
                if let Err(message) = check_run(*id, newest(entries, deep)) {
                    return Some(format!("the leader: {message}"));
                }
                if deep
                    && entries.first().is_some_and(|e| e.seq == ChainSeq::MIN)
                    && monotone(entries)
                {
                    if let Err(refusal) = verify_from_genesis(*id, entries) {
                        return Some(format!(
                            "the crate refuses the leader's sound chain {id}: {refusal}"
                        ));
                    }
                }
            }
            continue;
        }
        let Some(dev) = w.devices.get(node - 1) else {
            continue;
        };
        if !dev.up {
            continue;
        }
        for (id, copy) in &dev.copies {
            if let Err(message) = check_run(*id, newest(copy.entries(), deep)) {
                return Some(format!("device {}: {message}", dev.idx));
            }
            if deep
                && copy
                    .entries()
                    .first()
                    .is_some_and(|e| e.seq == ChainSeq::MIN)
                && monotone(copy.entries())
            {
                if let Err(refusal) = verify_from_genesis(*id, copy.entries()) {
                    return Some(format!(
                        "the crate refuses device {}'s sound chain {id}: {refusal}",
                        dev.idx
                    ));
                }
            }
        }
        for (id, raw) in &dev.raw {
            if let Err(message) = check_run(*id, raw) {
                return Some(format!("device {}: {message}", dev.idx));
            }
        }
    }
    attack_check(
        w,
        &[
            crate::step::AttackKind::WrongChainEntry,
            crate::step::AttackKind::PrevSwap,
            crate::step::AttackKind::GapEntry,
        ],
    )
}

fn c_no_fork(w: &World) -> Option<String> {
    // One node never holds two hashes at one position: its pin and its copy agree.
    for node in nodes_to_check(w, false) {
        let Some(dev) = node.checked_sub(1).and_then(|i| w.devices.get(i)) else {
            continue;
        };
        for (id, copy) in &dev.copies {
            if let Some(pinned) = dev.pin.head(id) {
                if let Some(held) = copy.get(pinned.seq) {
                    if held.hash != pinned.hash {
                        return Some(format!(
                            "device {} holds two hashes at position {} of {id}",
                            dev.idx, pinned.seq
                        ));
                    }
                }
            }
        }
        for (id, raw) in &dev.raw {
            let mut seen: BTreeMap<ChainSeq, Hash32> = BTreeMap::new();
            for e in raw {
                if let Some(other) = seen.insert(e.seq, e.hash) {
                    if other != e.hash {
                        return Some(format!(
                            "device {} holds two hashes at position {} of {id}",
                            dev.idx, e.seq
                        ));
                    }
                }
            }
        }
    }
    attack_check(
        w,
        &[
            crate::step::AttackKind::ForkHead,
            crate::step::AttackKind::ForkEntry,
            crate::step::AttackKind::SecondGenesis,
            crate::step::AttackKind::PeerFork,
        ],
    )
}

/// The verdict of the attacks of the given kinds that ran in this step.
fn attack_check(w: &World, kinds: &[crate::step::AttackKind]) -> Option<String> {
    for event in &w.delta.attacks {
        if kinds.contains(&event.kind)
            && !event.expectation.met(&event.observed, event.state_changed)
        {
            return Some(format!(
                "the attack {} on device {} met {:?}, the plan demands {}",
                event.kind.name(),
                event.dev,
                event.observed,
                event.expectation.describe()
            ));
        }
    }
    None
}

fn c_pin_backwards(w: &World) -> Option<String> {
    for m in &w.delta.pin_moves {
        if m.by_reanchor {
            continue;
        }
        if let (Some(before), Some(after)) = (m.before, m.after) {
            if after.seq < before.seq {
                return Some(format!(
                    "device {} went back from position {} to {} on {} without a restore",
                    m.dev, before.seq, after.seq, m.chain
                ));
            }
        }
    }
    for node in nodes_to_check(w, false) {
        let Some(dev) = node.checked_sub(1).and_then(|i| w.devices.get(i)) else {
            continue;
        };
        for id in &dev.chain_ids {
            let pinned = dev.pin.head(id).map(|h| h.seq);
            let mark = w.ledger.watermark.get(&(dev.idx, *id)).copied();
            if pinned != mark && !(pinned.is_none() && mark.is_none()) {
                return Some(format!(
                    "device {}: the pin of {id} stands at {pinned:?}, the watermark of valid heads is {mark:?}",
                    dev.idx
                ));
            }
        }
    }
    attack_check(w, &[crate::step::AttackKind::ReplayOldHead])
}

fn c_pin_signed(w: &World) -> Option<String> {
    for node in nodes_to_check(w, false) {
        let Some(dev) = node.checked_sub(1).and_then(|i| w.devices.get(i)) else {
            continue;
        };
        for id in &dev.chain_ids {
            if let Some(head) = dev.pin.head(id) {
                let key = (head.chain, head.seq, head.hash);
                if !w.ledger.signed.contains(&key) && !w.ledger.record_positions.contains(&key) {
                    return Some(format!(
                        "device {} pinned a head of {id} at position {} that nobody with the group key or the map key signed",
                        dev.idx, head.seq
                    ));
                }
            }
        }
    }
    attack_check(
        w,
        &[
            crate::step::AttackKind::CorruptHead,
            crate::step::AttackKind::UnsignedHead,
            crate::step::AttackKind::WrongChainHead,
        ],
    )
}

fn c_pin_epoch(w: &World) -> Option<String> {
    for node in nodes_to_check(w, false) {
        let Some(dev) = node.checked_sub(1).and_then(|i| w.devices.get(i)) else {
            continue;
        };
        let floor = w
            .ledger
            .floor
            .get(&dev.idx)
            .copied()
            .unwrap_or(RestoreEpoch::ZERO);
        for id in &dev.chain_ids {
            let Some(current) = dev.pin.signed_head(id).map(|s| s.epoch) else {
                continue;
            };
            let seen = w
                .ledger
                .max_pinned_epoch
                .get(&(dev.idx, *id))
                .copied()
                .unwrap_or(RestoreEpoch::ZERO)
                .max(floor);
            if current < seen {
                return Some(format!(
                    "device {} pins a head of epoch {current} on {id} after it pinned epoch {seen}",
                    dev.idx
                ));
            }
        }
    }
    None
}

fn c_restore_path(w: &World) -> Option<String> {
    for e in &w.delta.restores {
        if e.accepted && !e.oracle_valid {
            return Some(format!(
                "device {} followed a restore record of epoch {} that the operator did not sign with the map key, or that is not above its floor",
                e.dev, e.record.epoch
            ));
        }
        if !e.accepted && (e.pins_before != e.pins_after || e.floor_before != e.floor_after) {
            return Some(format!(
                "device {} refused a restore record of epoch {} and still changed",
                e.dev, e.record.epoch
            ));
        }
    }
    if let Some((before, after)) = w.delta.leader_restore {
        if after <= before {
            return Some(format!(
                "the leader's restore epoch went from {before} to {after}: a restore must raise it"
            ));
        }
    }
    for node in nodes_to_check(w, false) {
        let Some(dev) = node.checked_sub(1).and_then(|i| w.devices.get(i)) else {
            continue;
        };
        let oracle = w
            .ledger
            .floor
            .get(&dev.idx)
            .copied()
            .unwrap_or(RestoreEpoch::ZERO);
        if dev.pin.restore_epoch() != oracle {
            return Some(format!(
                "device {}: the pin's floor is {}, the records the operator signed and the device may follow give {oracle}",
                dev.idx,
                dev.pin.restore_epoch()
            ));
        }
    }
    attack_check(
        w,
        &[
            crate::step::AttackKind::ReplayOldRestore,
            crate::step::AttackKind::GroupAloneRestore,
            crate::step::AttackKind::RestoreWithoutSignature,
        ],
    )
}

fn c_lost_timeline(w: &World) -> Option<String> {
    if let Some(first) = w.delta.lost_timeline_accepted.first() {
        return Some(first.clone());
    }
    for node in nodes_to_check(w, false) {
        let Some(dev) = node.checked_sub(1).and_then(|i| w.devices.get(i)) else {
            continue;
        };
        let floor = w
            .ledger
            .copy_floor
            .get(&dev.idx)
            .copied()
            .unwrap_or(RestoreEpoch::ZERO);
        for (id, copy) in &dev.copies {
            if copy.epoch() < floor {
                return Some(format!(
                    "device {}: the copy of {id} would take entries of epoch {}, below the restore it accepted ({floor})",
                    dev.idx,
                    copy.epoch()
                ));
            }
        }
    }
    attack_check(
        w,
        &[
            crate::step::AttackKind::LostTimelineHead,
            crate::step::AttackKind::OldEpochEntry,
        ],
    )
}

fn c_late_record(w: &World) -> Option<String> {
    for e in w
        .delta
        .restores
        .iter()
        .filter(|e| e.accepted && e.oracle_valid)
    {
        for head in &e.record.positions {
            let progressed = e
                .pins_before
                .get(&head.chain)
                .is_some_and(|view| view.epoch.is_some_and(|epoch| epoch >= e.record.epoch));
            if !progressed {
                continue;
            }
            if e.pins_after.get(&head.chain) != e.pins_before.get(&head.chain)
                || e.copies_after.get(&head.chain) != e.copies_before.get(&head.chain)
            {
                return Some(format!(
                    "device {}: a record of epoch {} reached it after it pinned a signed head of the restored timeline on {}, and took the chain back",
                    e.dev, e.record.epoch, head.chain
                ));
            }
        }
    }
    None
}

fn c_listed_chains(w: &World) -> Option<String> {
    for e in w
        .delta
        .restores
        .iter()
        .filter(|e| e.accepted && e.oracle_valid)
    {
        for head in &e.record.positions {
            let progressed = e
                .pins_before
                .get(&head.chain)
                .is_some_and(|view| view.epoch.is_some_and(|epoch| epoch >= e.record.epoch));
            if progressed {
                continue;
            }
            let pinned = e.pins_after.get(&head.chain).map(|view| view.head);
            if pinned != Some(*head) {
                return Some(format!(
                    "device {}: after the record of epoch {} the pin of {} stands at {:?}, the survivor is position {}",
                    e.dev,
                    e.record.epoch,
                    head.chain,
                    pinned.map(|h| h.seq),
                    head.seq
                ));
            }
            if e.copies_after.get(&head.chain).copied().flatten() != Some(*head) {
                return Some(format!(
                    "device {}: after the record of epoch {} its copy of {} was not rebuilt at the survivor (position {})",
                    e.dev, e.record.epoch, head.chain, head.seq
                ));
            }
        }
        let listed: BTreeSet<ChainId> = e.record.positions.iter().map(|h| h.chain).collect();
        for (chain, before) in &e.pins_before {
            if !listed.contains(chain) && e.pins_after.get(chain) != Some(before) {
                return Some(format!(
                    "device {}: a chain the record does not list ({chain}) was touched",
                    e.dev
                ));
            }
        }
        if e.floor_after != e.record.epoch {
            return Some(format!(
                "device {}: its floor is {} after the record of epoch {}",
                e.dev, e.floor_after, e.record.epoch
            ));
        }
    }
    None
}

fn c_peer_hidden(w: &World) -> Option<String> {
    for e in &w.delta.meets {
        let Some(ours) = e.ours else {
            continue;
        };
        if !e.valid || e.head.epoch < e.standing {
            continue;
        }
        let hidden = e.head.epoch > e.standing
            || e.head.head.seq > ours.head.seq
            || (e.head.head.seq == ours.head.seq && e.head.head.hash != ours.head.hash);
        if hidden && !e.alarmed {
            return Some(format!(
                "device {}: a peer holds a newer or different head of {} (position {}, ours {}) and no alarm was raised",
                e.dev, e.head.head.chain, e.head.head.seq, ours.head.seq
            ));
        }
    }
    attack_check(w, &[crate::step::AttackKind::StaleHeadToOne])
}

fn c_peer_junk(w: &World) -> Option<String> {
    for e in &w.delta.meets {
        if !e.valid && (e.alarmed || !matches!(e.verdict, PeerVerdict::BadSignature)) {
            return Some(format!(
                "device {}: a peer's head with a bad signature gave {:?} (alarm: {})",
                e.dev, e.verdict, e.alarmed
            ));
        }
    }
    attack_check(w, &[crate::step::AttackKind::PeerJunk])
}

fn c_stamps(w: &World, me: &mut Checker, deep: bool) -> Option<String> {
    for event in &w.delta.stamps {
        match *event {
            // A stamp the leader gave to an entry it never wrote (a crash, a restore) is no stamp
            // of the chain: from the reset on, what counts is what the chain holds.
            StampEvent::Reset { chain, at } => match at {
                Some(at) => {
                    me.last_stamp.insert(chain, at);
                }
                None => {
                    me.last_stamp.remove(&chain);
                }
            },
            StampEvent::Stamped { chain, seq, at } => {
                if let Some(last) = me.last_stamp.get(&chain) {
                    if at < *last {
                        return Some(format!(
                            "the leader stamped position {seq} of {chain} with {} after {}",
                            at.as_secs(),
                            last.as_secs()
                        ));
                    }
                }
                me.last_stamp.insert(chain, at);
            }
        }
    }
    for node in nodes_to_check(w, deep) {
        let runs: Vec<(ChainId, &[OpaqueEntry])> = if node == 0 {
            w.leader
                .chains
                .iter()
                .map(|(id, lc)| (*id, lc.chain.entries()))
                .collect()
        } else {
            match w.devices.get(node - 1) {
                Some(dev) => dev
                    .copies
                    .iter()
                    .map(|(id, c)| (*id, c.entries()))
                    .collect(),
                None => Vec::new(),
            }
        };
        for (id, entries) in runs {
            for pair in entries.windows(2) {
                if let [a, b] = pair {
                    if b.at < a.at {
                        return Some(format!(
                            "node {node}: position {} of {id} is stamped {}, before position {} ({})",
                            b.seq,
                            b.at.as_secs(),
                            a.seq,
                            a.at.as_secs()
                        ));
                    }
                }
            }
        }
        if let Some(dev) = node.checked_sub(1).and_then(|i| w.devices.get(i)) {
            for (id, raw) in &dev.raw {
                for pair in raw.windows(2) {
                    if let [a, b] = pair {
                        if b.seq > a.seq && b.at < a.at {
                            return Some(format!(
                                "device {}: stamps of {id} run backwards",
                                dev.idx
                            ));
                        }
                    }
                }
            }
        }
    }
    attack_check(w, &[crate::step::AttackKind::StampBackwardsEntry])
}

fn c_acked(w: &World) -> Option<String> {
    if !w.delta.leader_restarted || w.leader.faulted || !w.leader.up {
        return None;
    }
    let loss_allowed = w.ledger.lies_told > 0 || w.ledger.degraded;
    for ((chain, seq), (hash, _)) in &w.ledger.acked {
        let held = w.leader.chains.get(chain).and_then(|lc| lc.chain.get(*seq));
        if held.map(|e| e.hash) != Some(*hash) {
            if loss_allowed {
                return None;
            }
            return Some(format!(
                "the leader acknowledged position {seq} of {chain} and does not hold it after a restart, with an honest disk"
            ));
        }
    }
    None
}

/// After a quiesce on a degraded world: a loss must SHOW. A device that holds what the leader no
/// longer has refused the leader's older head or raised an alarm.
fn c_loss_shows(w: &World) -> Option<String> {
    if !w.ledger.degraded && w.ledger.lies_told == 0 {
        return None;
    }
    if w.leader.faulted || !w.leader.up {
        return None;
    }
    for dev in &w.devices {
        for id in &dev.chain_ids {
            let (Some(leader_head), Some(pinned)) = (
                w.leader.chains.get(id).and_then(|lc| lc.chain.head()),
                dev.pin.head(id),
            ) else {
                continue;
            };
            let diverges = pinned.seq > leader_head.seq
                || (pinned.seq <= leader_head.seq
                    && w.leader
                        .chains
                        .get(id)
                        .and_then(|lc| lc.chain.get(pinned.seq))
                        .is_some_and(|e| e.hash != pinned.hash));
            if diverges
                && dev.alarm_count == 0
                && !w.ledger.leader_older_seen.contains(&(dev.idx, *id))
            {
                return Some(format!(
                    "device {} holds position {} of {id} that the leader lost, and neither refused the leader's older head nor raised an alarm",
                    dev.idx, pinned.seq
                ));
            }
        }
    }
    None
}

fn c_silent_rollback(w: &World, me: &mut Checker) -> Option<String> {
    let honest_disk = w.ledger.lies_told == 0 && !w.ledger.degraded;
    for (chain, seq, _, epoch) in &w.delta.heads_signed {
        if let Some((served_epoch, served_seq)) = me.served.get(chain).copied() {
            if honest_disk && *seq < served_seq {
                if *epoch == served_epoch {
                    return Some(format!(
                        "the leader served position {seq} of {chain} in epoch {epoch} after position {served_seq} in the same epoch"
                    ));
                }
                if *epoch > served_epoch && w.ledger.announced_epoch < *epoch {
                    return Some(format!(
                        "the leader serves position {seq} of {chain} in epoch {epoch}, behind position {served_seq}, and has not announced a restore record of epoch {epoch}"
                    ));
                }
            }
            if (*epoch, *seq) > (served_epoch, served_seq) {
                me.served.insert(*chain, (*epoch, *seq));
            }
        } else {
            me.served.insert(*chain, (*epoch, *seq));
        }
    }
    None
}

fn c_one_head(w: &World, me: &mut Checker) -> Option<String> {
    let excused = w.ledger.lies_told > 0 || w.ledger.degraded;
    for (chain, seq, hash, epoch) in &w.delta.heads_signed {
        match me.heads.get(&(*chain, *seq, *epoch)) {
            Some(other) if other != hash && !excused => {
                return Some(format!(
                    "the leader signed two different heads at position {seq} of {chain} in epoch {epoch}"
                ));
            }
            Some(_) => {}
            None => {
                me.heads.insert((*chain, *seq, *epoch), *hash);
            }
        }
    }
    None
}

fn c_restart(w: &World) -> Option<String> {
    for r in &w.delta.device_restarts {
        if !r.wiped && !r.allowed.is_empty() && !r.allowed.contains(&r.recovered) {
            return Some(format!(
                "device {} recovered a state that no prefix of its log gives: pins, floor and copies are an invention",
                r.dev
            ));
        }
    }
    None
}

fn c_refusals(w: &World) -> Option<String> {
    for r in &w.delta.refusals {
        if r.reported != r.cause.demanded() {
            return Some(format!(
                "node {}: a refusal for {:?} was classed {:?}, the plan demands {:?}",
                r.node,
                r.cause,
                r.reported,
                r.cause.demanded()
            ));
        }
    }
    attack_check(w, &[crate::step::AttackKind::CorruptEntry])
}

fn c_limits(w: &World) -> Option<String> {
    let f = w.footprint();
    let checks = [
        ("messages in flight", f.inflight, MAX_INFLIGHT),
        ("jobs of a node", f.max_jobs_per_node, MAX_JOBS_PER_NODE),
        ("open jobs", f.open_jobs, MAX_OPEN_JOBS),
        ("alarms of a node", f.max_alarms, MAX_ALARMS),
        ("records of a disk", f.max_disk_records, MAX_DISK_RECORDS),
        ("off-site copies", f.offsite_copies, MAX_OFFSITE_COPIES),
        ("restores", f.restores, MAX_RESTORES),
        ("trace lines", f.trace_tail, MAX_TRACE_TAIL),
    ];
    for (what, have, limit) in checks {
        if have > limit {
            return Some(format!("{what}: {have} is past the limit {limit}"));
        }
    }
    None
}

fn c_policy(w: &World, kind: PolicyKind) -> Option<String> {
    w.delta
        .policy
        .iter()
        .find(|e| e.kind == kind && !e.ok)
        .map(|e| e.message.clone())
}

fn c_checkpoints(w: &World) -> Option<String> {
    if !w.leader.up {
        return None;
    }
    let verifier = crate::device::head_verifier_for(w);
    for (id, lc) in &w.leader.chains {
        let all = lc.chain.entries();
        for cp in &lc.checkpoints {
            let rest: Vec<OpaqueEntry> = all
                .iter()
                .filter(|e| e.seq > cp.head().seq)
                .cloned()
                .collect();
            if all
                .get(usize::try_from(cp.head().seq.get()).unwrap_or(usize::MAX))
                .map(|e| e.hash)
                != Some(cp.head().hash)
                && all.first().is_some_and(|e| e.seq == ChainSeq::MIN)
            {
                return Some(format!(
                    "a checkpoint of {id} names a head the chain does not hold"
                ));
            }
            let from_checkpoint = verify_from_checkpoint(cp, &verifier, &rest);
            let from_genesis = verify_from_genesis(*id, all).map(|head| head.unwrap_or(cp.head()));
            if from_checkpoint != from_genesis && monotone(all) {
                return Some(format!(
                    "a checkpoint of {id} gives {from_checkpoint:?}, verification from the start gives {from_genesis:?}"
                ));
            }
            // A checkpoint whose head signature is damaged is refused.
            let mut damaged = cp.signed_head().clone();
            if let Some(byte) = damaged.sig.first_mut() {
                *byte ^= 0x5A;
            }
            let damaged = azlin_chain::Checkpoint::new(damaged, cp.at(), cp.state_hash());
            if verify_from_checkpoint(&damaged, &verifier, &rest).is_ok() {
                return Some(format!(
                    "a checkpoint of {id} with a damaged head signature was accepted"
                ));
            }
        }
    }
    None
}

fn c_checkpoint_due(w: &World) -> Option<String> {
    if !w.leader.up || w.leader.faulted || w.ledger.degraded {
        return None;
    }
    let now = w.leader.stamper.last()?;
    for (id, lc) in &w.leader.chains {
        let head = lc.chain.head()?;
        let (since, last) = match lc.checkpoints.last() {
            Some(cp) => (
                head.seq.get().saturating_sub(cp.head().seq.get()),
                cp.signed_head().signed_at,
            ),
            None => (
                head.seq.get().saturating_add(1),
                lc.chain.entries().first().map_or(now, |e| e.at),
            ),
        };
        let age = now.as_secs().saturating_sub(last.as_secs());
        let due = since >= azlin_chain::CHECKPOINT_ENTRIES
            || age >= azlin_chain::CHECKPOINT_AGE.as_secs();
        if due {
            return Some(format!(
                "a checkpoint of {id} is due ({since} entries, {age} seconds since the last) and the leader has not written one"
            ));
        }
    }
    None
}

fn c_held(w: &World) -> Option<String> {
    if w.ledger.degraded || w.leader.faulted {
        return None;
    }
    for h in &w.ledger.held_beyond {
        let Some(dev) = w.devices.get(h.dev) else {
            continue;
        };
        if w.ledger
            .resend_refusals
            .iter()
            .any(|(original, _)| *original == h.original)
        {
            continue;
        }
        let present = w.leader.chains.get(&h.chain).is_some_and(|lc| {
            lc.chain.entries().iter().any(|e| {
                e.signer == dev.key && e.epoch >= h.epoch && e.payload == h.payload.to_be_bytes()
            })
        });
        if !present {
            return Some(format!(
                "device {} held an entry (payload {}) of {} beyond the survivors of a restore and the leader has neither it nor a typed refusal after resend and quiesce",
                h.dev, h.payload, h.chain
            ));
        }
    }
    None
}

fn c_heal(w: &World) -> Option<String> {
    if w.ledger.degraded || w.leader.faulted || !w.leader.up {
        return None;
    }
    for dev in &w.devices {
        if !dev.up {
            continue;
        }
        if dev.pin.restore_epoch() != w.leader.epoch {
            return Some(format!(
                "after quiesce device {} stands at restore epoch {}, the leader at {}",
                dev.idx,
                dev.pin.restore_epoch(),
                w.leader.epoch
            ));
        }
        for id in &dev.chain_ids {
            let leader_head = w.leader.chains.get(id).and_then(|lc| lc.chain.head());
            let pinned = dev.pin.head(id);
            let copied = dev.copies.get(id).and_then(|c| c.head());
            if pinned != leader_head || copied != leader_head {
                return Some(format!(
                    "after quiesce device {} holds {id} at pin {:?} and copy {:?}, the leader at {:?}",
                    dev.idx,
                    pinned.map(|h| h.seq),
                    copied.map(|h| h.seq),
                    leader_head.map(|h| h.seq)
                ));
            }
        }
    }
    None
}
