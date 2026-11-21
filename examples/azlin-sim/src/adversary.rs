//! The adversary (planning/architecture.md section 6.10 S15 "a malicious server", section 5.9 "two
//! different entries at the same position are a fork, refused and alarmed", section 12 E4 "devices
//! accept going back only through a restore signed by the map key").
//!
//! The adversary is the server, the database and the network. It can replay, reorder, corrupt and
//! withhold messages, and, because the voters hold the credential key ("Raft is not
//! Byzantine-tolerant"), it can sign with the GROUP key. It never holds a member, device, map or
//! payments key. So an attack that needs a validly signed entry uses entries the honest devices
//! really signed (replayed, resealed with another stamp, shown to the wrong chain), and a head or a
//! record it signs itself carries the group key only.
//!
//! Each `attack kind=... dev=... chain=...` is one move against one device. [`expected`] says, for
//! each kind and the device's state, which verdict class the device must answer with; the checkers
//! fail on any other answer (an exhaustive match: a new kind needs its row).

use azlin_chain::{
    require_signed, ChainRefusal, Head, RefusalClass, RestoreRecord, RestoreSigner, SignedHead,
};
use azlin_seams::{hash32, ChainSeq, Hash32, RestoreEpoch, UnixTime};

use crate::machine::OpaqueEntry;
use crate::sched::{Job, JobKind};
use crate::step::AttackKind;
use crate::world::{AttackEvent, Observed, World};

/// What a device must answer with.
///
/// Implements planning/architecture.md section 6.10 S15 and section 12 E4.
///
/// Caller: `attack`, `invariants`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Expect {
    /// Refused, with one of these classes; whether an alarm must (`Some(true)`), must not
    /// (`Some(false)`) or need not (`None`) be raised; and the refusal's name when it is demanded.
    Refused {
        classes: &'static [RefusalClass],
        alarm: Option<bool>,
        what: Option<&'static str>,
    },
    /// Ignored quietly: no refusal, no alarm, nothing changed.
    Quiet,
}

impl Expect {
    /// Whether what was observed is what the plan demands.
    pub(crate) fn met(&self, observed: &Observed, state_changed: bool) -> bool {
        if state_changed {
            return false;
        }
        match (self, observed) {
            (
                Expect::Refused {
                    classes,
                    alarm,
                    what,
                },
                Observed::Refused {
                    class,
                    alarm: raised,
                    what: seen,
                },
            ) => {
                classes.contains(class)
                    && alarm.is_none_or(|want| want == *raised)
                    && what.is_none_or(|want| want == seen.as_str())
            }
            (Expect::Quiet, Observed::Quiet(_)) => true,
            (Expect::Refused { .. }, Observed::Accepted(_) | Observed::Quiet(_))
            | (Expect::Quiet, Observed::Accepted(_) | Observed::Refused { .. }) => false,
        }
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            Expect::Refused {
                classes,
                alarm,
                what,
            } => format!("a refusal of class {classes:?}, alarm {alarm:?}, {what:?}"),
            Expect::Quiet => String::from("a quiet refusal: no alarm, nothing changed"),
        }
    }
}

const REFUSE: &[RefusalClass] = &[RefusalClass::Refuse];
const ALARM: &[RefusalClass] = &[RefusalClass::Alarm];
const REFETCH: &[RefusalClass] = &[RefusalClass::Refetch];
const REFUSE_OR_ALARM: &[RefusalClass] = &[RefusalClass::Refuse, RefusalClass::Alarm];

/// The name of a chain refusal as the harness tells them apart.
fn refusal_word(refusal: &ChainRefusal) -> &'static str {
    match refusal {
        ChainRefusal::WrongChain => "WrongChain",
        ChainRefusal::NotGenesis => "NotGenesis",
        ChainRefusal::GenesisAgain { .. } => "GenesisAgain",
        ChainRefusal::WrongPrev { .. } => "WrongPrev",
        ChainRefusal::Gap { .. } => "Gap",
        ChainRefusal::Fork { .. } => "Fork",
        ChainRefusal::StampBackwards => "StampBackwards",
        ChainRefusal::BelowRestoreEpoch { .. } => "BelowRestoreEpoch",
        ChainRefusal::Corrupt => "Corrupt",
        ChainRefusal::Full => "Full",
    }
}

/// What a device must answer for an attack of this kind, given what it holds: `holds_genesis` says
/// whether its copy of the chain still holds position 0 (a copy that restarted at a survivor cannot
/// tell a second genesis from an old position).
///
/// Implements planning/architecture.md section 6.10 S15 and section 12 E4.
///
/// Caller: `attack`.
pub(crate) fn expected(kind: AttackKind, holds_genesis: bool) -> Expect {
    match kind {
        AttackKind::ReplayOldHead => Expect::Refused {
            classes: REFUSE,
            alarm: Some(false),
            what: None,
        },
        AttackKind::LostTimelineHead => Expect::Refused {
            classes: ALARM,
            alarm: Some(true),
            what: None,
        },
        AttackKind::StaleHeadToOne => Expect::Refused {
            classes: ALARM,
            alarm: Some(true),
            what: None,
        },
        AttackKind::ForkHead => Expect::Refused {
            classes: ALARM,
            alarm: Some(true),
            what: None,
        },
        AttackKind::ForkEntry => Expect::Refused {
            classes: ALARM,
            alarm: Some(true),
            what: None,
        },
        AttackKind::CorruptEntry => Expect::Refused {
            classes: REFETCH,
            alarm: Some(false),
            what: Some("Corrupt"),
        },
        AttackKind::CorruptHead => Expect::Refused {
            classes: REFUSE,
            alarm: Some(false),
            what: None,
        },
        AttackKind::UnsignedHead | AttackKind::WrongChainHead => Expect::Refused {
            classes: ALARM,
            alarm: Some(true),
            what: None,
        },
        AttackKind::WrongChainEntry => Expect::Refused {
            classes: REFUSE,
            alarm: Some(false),
            what: Some("WrongChain"),
        },
        AttackKind::SecondGenesis => {
            if holds_genesis {
                Expect::Refused {
                    classes: ALARM,
                    alarm: Some(true),
                    what: Some("GenesisAgain"),
                }
            } else {
                Expect::Refused {
                    classes: REFUSE,
                    alarm: Some(false),
                    what: Some("Gap"),
                }
            }
        }
        AttackKind::PrevSwap => Expect::Refused {
            classes: REFUSE,
            alarm: Some(false),
            what: Some("WrongPrev"),
        },
        AttackKind::GapEntry => Expect::Refused {
            classes: REFUSE,
            alarm: Some(false),
            what: Some("Gap"),
        },
        AttackKind::StampBackwardsEntry => Expect::Refused {
            classes: REFUSE,
            alarm: Some(false),
            what: Some("StampBackwards"),
        },
        AttackKind::OldEpochEntry => Expect::Refused {
            classes: ALARM,
            alarm: Some(true),
            what: Some("BelowRestoreEpoch"),
        },
        AttackKind::ReplayOldRestore => Expect::Refused {
            classes: REFUSE_OR_ALARM,
            alarm: None,
            what: None,
        },
        AttackKind::GroupAloneRestore | AttackKind::RestoreWithoutSignature => Expect::Refused {
            classes: ALARM,
            alarm: None,
            what: None,
        },
        AttackKind::PeerJunk => Expect::Quiet,
        AttackKind::PeerFork => Expect::Refused {
            classes: ALARM,
            alarm: Some(true),
            what: None,
        },
    }
}

/// What one attack met.
struct Result3 {
    observed: Observed,
    changed: bool,
    expectation: Expect,
}

fn alarms_of(w: &World, di: usize) -> u64 {
    w.devices.get(di).map_or(0, |d| d.alarm_count)
}

fn digest_of(w: &World, di: usize) -> Hash32 {
    w.devices.get(di).map_or(Hash32::ZERO, |d| d.state_digest())
}

fn run_phases(w: &mut World, job: &mut Job, count: u8) {
    for phase in 0..count {
        job.phase = phase;
        let _ = w.run_phase(job);
    }
}

/// A head offered to a device through its own handler, all phases at once.
fn inject_head(
    w: &mut World,
    di: usize,
    chain: azlin_chain::ChainId,
    head: SignedHead,
) -> (Observed, bool) {
    let node = di + 1;
    let (digest, alarms) = (digest_of(w, di), alarms_of(w, di));
    let mut job = Job::new(JobKind::DOnHead, node);
    job.from = usize::MAX;
    job.chain = Some(chain);
    job.head = Some(head);
    run_phases(w, &mut job, 3);
    let changed = digest_of(w, di) != digest;
    let raised = alarms_of(w, di) > alarms;
    let observed = match job.pin_verdict {
        Some(azlin_chain::PinVerdict::Advanced) => Observed::Accepted(String::from("advanced")),
        Some(azlin_chain::PinVerdict::Unchanged) => Observed::Quiet(String::from("unchanged")),
        Some(azlin_chain::PinVerdict::RefusedOlder { .. }) => Observed::Refused {
            class: RefusalClass::Refuse,
            alarm: raised,
            what: String::from("RefusedOlder"),
        },
        Some(azlin_chain::PinVerdict::Fork(_)) => Observed::Refused {
            class: RefusalClass::Alarm,
            alarm: raised,
            what: String::from("Fork"),
        },
        Some(azlin_chain::PinVerdict::BadSignature) => Observed::Refused {
            class: RefusalClass::Refuse,
            alarm: raised,
            what: String::from("BadSignature"),
        },
        Some(azlin_chain::PinVerdict::Refused(refusal)) => Observed::Refused {
            class: refusal.class(),
            alarm: raised,
            what: String::from("BelowRestoreEpoch"),
        },
        Some(azlin_chain::PinVerdict::Full) => Observed::Refused {
            class: RefusalClass::Refuse,
            alarm: raised,
            what: String::from("Full"),
        },
        None => Observed::Quiet(String::from("no verdict")),
    };
    (observed, changed)
}

/// Entries offered to a device through its own handler, all phases at once.
fn inject_entries(
    w: &mut World,
    di: usize,
    chain: azlin_chain::ChainId,
    entries: Vec<OpaqueEntry>,
) -> (Observed, bool) {
    let node = di + 1;
    let (digest, alarms, refusals) = (digest_of(w, di), alarms_of(w, di), w.delta.refusals.len());
    let mut job = Job::new(JobKind::DOnEntries, node);
    job.from = usize::MAX;
    job.chain = Some(chain);
    job.entries = entries;
    run_phases(w, &mut job, 4);
    let changed = digest_of(w, di) != digest;
    let raised = alarms_of(w, di) > alarms;
    let observed = if let Some(refusal) = job.chain_refusals.first() {
        Observed::Refused {
            class: refusal.class(),
            alarm: raised,
            what: String::from(refusal_word(refusal)),
        }
    } else if job.fork_with_pin {
        Observed::Refused {
            class: RefusalClass::Alarm,
            alarm: raised,
            what: String::from("ForkWithPin"),
        }
    } else if w.delta.refusals.len() > refusals {
        let class = w
            .delta
            .refusals
            .last()
            .map_or(RefusalClass::Refuse, |r| r.reported);
        Observed::Refused {
            class,
            alarm: raised,
            what: String::from("refused"),
        }
    } else if changed {
        Observed::Accepted(String::from("admitted"))
    } else {
        Observed::Quiet(String::from("held or dropped"))
    };
    (observed, changed)
}

/// A hostile entry made of an entry the device holds, offered to it. It goes through the handler of
/// the device when its position is below the pinned head; an entry beyond the pin is stopped by the pin
/// before the chain crate sees it (glue rule G18), so then the crate is asked directly, on the copy the
/// device holds: the guards of the link are the crate's, and the attack tests them in either case.
fn offer_hostile(
    w: &mut World,
    di: usize,
    chain: azlin_chain::ChainId,
    hostile: OpaqueEntry,
    pin_seq: Option<ChainSeq>,
) -> (Observed, bool) {
    if pin_seq.is_some_and(|pin| hostile.seq < pin) {
        return inject_entries(w, di, chain, vec![hostile]);
    }
    let verdict = w
        .devices
        .get(di)
        .and_then(|dev| dev.copies.get(&chain))
        .map(|copy| copy.check(&hostile));
    let observed = match verdict {
        Some(Err(refusal)) => Observed::Refused {
            class: refusal.class(),
            alarm: refusal.alarm(chain).is_some(),
            what: String::from(refusal_word(&refusal)),
        },
        Some(Ok(_)) => Observed::Accepted(String::from("the chain crate took it")),
        None => Observed::Quiet(String::from("the device holds no copy")),
    };
    (observed, false)
}

/// A restore record offered to a device through its own handler.
fn inject_restore(w: &mut World, di: usize, record: RestoreRecord) -> (Observed, bool) {
    let node = di + 1;
    let (digest, alarms) = (digest_of(w, di), alarms_of(w, di));
    let mut job = Job::new(JobKind::DOnRestore, node);
    job.from = usize::MAX;
    job.record = Some(record);
    run_phases(w, &mut job, 4);
    let changed = digest_of(w, di) != digest;
    let raised = alarms_of(w, di) > alarms;
    let observed = match job.restore_refusal {
        Some(refusal) => Observed::Refused {
            class: refusal.class(),
            alarm: raised,
            what: format!("{refusal:?}"),
        },
        None if changed => Observed::Accepted(String::from("followed")),
        None => Observed::Quiet(String::from("nothing")),
    };
    (observed, changed)
}

/// A peer head compared with the device's pin through its own meeting handler.
fn inject_peer(w: &mut World, di: usize, head: SignedHead) -> (Observed, bool) {
    let node = di + 1;
    let (digest, alarms) = (digest_of(w, di), alarms_of(w, di));
    let meets = w.delta.meets.len();
    let mut job = Job::new(JobKind::DMeet, node);
    job.heads = vec![head];
    for phase in 1..3 {
        job.phase = phase;
        let _ = w.run_phase(&mut job);
    }
    let changed = digest_of(w, di) != digest;
    let raised = alarms_of(w, di) > alarms;
    let verdict = w.delta.meets.get(meets).map(|e| e.verdict);
    let observed = match verdict {
        Some(azlin_chain::PeerVerdict::BadSignature) if !raised => {
            Observed::Quiet(String::from("BadSignature"))
        }
        Some(
            azlin_chain::PeerVerdict::Same
            | azlin_chain::PeerVerdict::PeerBehind
            | azlin_chain::PeerVerdict::Unpinned,
        ) if !raised => Observed::Quiet(String::from("nothing to say")),
        Some(_) | None => Observed::Refused {
            class: if raised {
                RefusalClass::Alarm
            } else {
                RefusalClass::Refuse
            },
            alarm: raised,
            what: format!("{verdict:?}"),
        },
    };
    (observed, changed)
}

fn flip_sig(head: &mut SignedHead) {
    match head.sig.first_mut() {
        Some(byte) => *byte ^= 0x5A,
        None => head.sig.push(0x5A),
    }
}

/// One attack. Whether it could be made (`false` is a skip: the world held no material for it).
pub(crate) fn attack(w: &mut World, kind: AttackKind, dev: u8, chain: u8) -> bool {
    let di = w.dev_of(dev);
    let Some(chain_id) = w.chain_of(di, chain) else {
        return false;
    };
    if !w.devices.get(di).is_some_and(|d| d.up) {
        return false;
    }
    w.sched.note_fault(crate::coverage::OpKind::Attack);
    let outcome = build_and_run(w, kind, di, chain_id);
    let Some(result) = outcome else {
        return false;
    };
    let met = result.expectation.met(&result.observed, result.changed);
    let tally = w.cov.attacks.entry(kind).or_default();
    tally.attempted += 1;
    if met {
        tally.met += 1;
    }
    w.trace.decide(&format!(
        "attack {} {}",
        kind.name(),
        if met { "met" } else { "missed" }
    ));
    w.delta.touched.insert(di + 1);
    w.delta.attacks.push(AttackEvent {
        kind,
        dev: di,
        observed: result.observed,
        state_changed: result.changed,
        expectation: result.expectation,
    });
    true
}

/// A held entry of this chain from the log of what the leader acknowledged, by position.
fn logged(
    w: &World,
    chain: &azlin_chain::ChainId,
    seq: ChainSeq,
    epoch_below: Option<RestoreEpoch>,
) -> Option<OpaqueEntry> {
    w.ledger
        .entry_log
        .iter()
        .rev()
        .find(|e| {
            e.chain == *chain && e.seq == seq && epoch_below.is_none_or(|floor| e.epoch < floor)
        })
        .cloned()
}

fn build_and_run(
    w: &mut World,
    kind: AttackKind,
    di: usize,
    chain: azlin_chain::ChainId,
) -> Option<Result3> {
    let dev = w.devices.get(di)?;
    let pin_head = dev.pin.head(&chain);
    let pin_signed = dev.pin.signed_head(&chain).cloned();
    let floor = dev.pin.restore_epoch();
    let copy_head = dev.copies.get(&chain).and_then(|c| c.head());
    let copy_last_at = dev.copies.get(&chain).and_then(|c| c.last_at());
    let copy_epoch = dev.copies.get(&chain).map(|c| c.epoch());
    let account = dev.account;
    let holds_genesis = dev
        .copies
        .get(&chain)
        .is_some_and(|c| c.get(ChainSeq::MIN).is_some());
    // What the device holds of the chain: its verified copy, and what a device that stores entries
    // without asking the chain (the planted double) kept instead.
    let mut held: Vec<OpaqueEntry> = dev
        .copies
        .get(&chain)
        .map(|c| c.entries().to_vec())
        .unwrap_or_default();
    if let Some(raw) = dev.raw.get(&chain) {
        held.extend(raw.iter().cloned());
    }
    let now = UnixTime::from_secs(w.clocks.global());
    let expectation = expected(kind, holds_genesis);
    let result = |observed: Observed, changed: bool| -> Option<Result3> {
        Some(Result3 {
            observed,
            changed,
            expectation,
        })
    };
    match kind {
        AttackKind::ReplayOldHead => {
            let pin = pin_head?;
            let old = w
                .ledger
                .head_log
                .iter()
                .rev()
                .find(|h| h.head.chain == chain && h.head.seq < pin.seq && h.epoch >= floor)?
                .clone();
            let (observed, changed) = inject_head(w, di, chain, old);
            result(observed, changed)
        }
        AttackKind::LostTimelineHead => {
            // A head the group key signed in the epoch before the restore the device accepted, at a
            // position HIGHER than the pinned one (the adversary holds the group key and signs any head).
            let pin = pin_head?;
            let lost_epoch = RestoreEpoch::new(floor.get().checked_sub(1)?);
            let lost = Head {
                chain,
                seq: pin.seq.next()?,
                hash: hash32(
                    "azlin-sim-lost-head-v1",
                    &[pin.hash.as_bytes(), &w.step_no.to_be_bytes()],
                ),
            };
            let signed = w.keys.sign_head(lost, lost_epoch, now);
            w.ledger.signed.insert((lost.chain, lost.seq, lost.hash));
            let (observed, changed) = inject_head(w, di, chain, signed);
            result(observed, changed)
        }
        AttackKind::StaleHeadToOne => {
            let mine = pin_head?.seq;
            let peer = w
                .devices
                .iter()
                .filter(|p| p.idx != di && p.up && p.account == account)
                .filter(|p| !w.net.is_cut(p.node, di + 1))
                .filter(|p| p.pin.head(&chain).is_some_and(|h| h.seq > mine))
                .map(|p| p.node)
                .next()?;
            let meets = w.delta.meets.len();
            let alarms = alarms_of(w, di);
            let mut job = Job::new(JobKind::DMeet, di + 1);
            job.peer = peer;
            for phase in 0..3 {
                job.phase = phase;
                let _ = w.run_phase(&mut job);
            }
            let event = w
                .delta
                .meets
                .iter()
                .skip(meets)
                .find(|e| {
                    e.head.head.chain == chain
                        && e.ours.is_some_and(|o| e.head.head.seq > o.head.seq)
                })?
                .clone();
            let raised = alarms_of(w, di) > alarms;
            let observed = if raised {
                Observed::Refused {
                    class: RefusalClass::Alarm,
                    alarm: true,
                    what: format!("{:?}", event.verdict),
                }
            } else {
                Observed::Quiet(String::from("no alarm"))
            };
            result(observed, false)
        }
        AttackKind::ForkHead => {
            let pin = pin_head?;
            let epoch = floor.max(pin_signed.as_ref().map_or(RestoreEpoch::ZERO, |s| s.epoch));
            let forked = Head {
                hash: hash32(
                    "azlin-sim-fork-head-v1",
                    &[pin.hash.as_bytes(), &w.step_no.to_be_bytes()],
                ),
                ..pin
            };
            let signed = w.keys.sign_head(forked, epoch, now);
            w.ledger
                .signed
                .insert((forked.chain, forked.seq, forked.hash));
            let (observed, changed) = inject_head(w, di, chain, signed);
            result(observed, changed)
        }
        AttackKind::ForkEntry => {
            let pin = pin_head?;
            let target = held
                .iter()
                .rev()
                .find(|e| e.seq < pin.seq)
                .or_else(|| held.last())?
                .clone();
            let mut forked = target;
            forked.at = UnixTime::from_secs(forked.at.as_secs().saturating_add(1));
            let forked = forked.rehashed();
            let (observed, changed) = inject_entries(w, di, chain, vec![forked]);
            result(observed, changed)
        }
        AttackKind::CorruptEntry => {
            let target = held.last()?.clone();
            let mut rng = azlin_seams::sim::Rng::new(w.rng.next_u64());
            let (observed, changed) =
                inject_entries(w, di, chain, vec![target.corrupted(&mut rng)]);
            result(observed, changed)
        }
        AttackKind::CorruptHead => {
            let pin = pin_head?;
            let latest = w
                .ledger
                .head_log
                .iter()
                .rev()
                .find(|h| h.head.chain == chain)?
                .clone();
            let mut forged = latest;
            forged.head.seq = pin.seq.next()?;
            flip_sig(&mut forged);
            let (observed, changed) = inject_head(w, di, chain, forged);
            result(observed, changed)
        }
        AttackKind::UnsignedHead => {
            let latest = w
                .ledger
                .head_log
                .iter()
                .rev()
                .find(|h| h.head.chain == chain)?
                .clone();
            let mut unsigned = latest;
            unsigned.sig.clear();
            shown_unsigned(w, di, chain, unsigned, result)
        }
        AttackKind::WrongChainHead => {
            let other = w
                .ledger
                .head_log
                .iter()
                .rev()
                .find(|h| h.head.chain != chain)?
                .clone();
            shown_unsigned(w, di, chain, other, result)
        }
        AttackKind::WrongChainEntry => {
            let pin = pin_head?;
            let foreign = w
                .ledger
                .entry_log
                .iter()
                .find(|e| e.chain != chain && e.seq <= pin.seq)?
                .clone();
            let (observed, changed) = inject_entries(w, di, chain, vec![foreign]);
            result(observed, changed)
        }
        AttackKind::SecondGenesis => {
            // A device that pins nothing beyond the genesis would answer with the pin's fork
            // before the chain crate is asked, and a device whose copy is empty takes any genesis
            // (it is the pin that catches a forged one, when the run reaches the pinned position).
            if pin_head?.seq == ChainSeq::MIN || copy_head.is_none() {
                return None;
            }
            let genesis = logged(w, &chain, ChainSeq::MIN, None)?;
            let mut second = genesis;
            second.at = UnixTime::from_secs(second.at.as_secs().saturating_add(1));
            let second = second.rehashed();
            let (observed, changed) = inject_entries(w, di, chain, vec![second]);
            result(observed, changed)
        }
        // The four attacks on the link of an entry make a hostile entry of the newest entry the
        // device holds (its signature covers the chain, the signer and the payload only, so the
        // adversary can put it at another position, link, stamp or epoch).
        AttackKind::PrevSwap => {
            let head = copy_head?;
            let mut hostile = held.last()?.clone();
            hostile.seq = head.seq.next()?;
            hostile.prev = hash32("azlin-sim-prev-swap-v1", &[head.hash.as_bytes()]);
            let hostile = hostile.rehashed();
            let (observed, changed) =
                offer_hostile(w, di, chain, hostile, pin_head.map(|pin| pin.seq));
            result(observed, changed)
        }
        AttackKind::GapEntry => {
            let head = copy_head?;
            let mut hostile = held.last()?.clone();
            hostile.seq = head.seq.next()?.next()?;
            hostile.prev = head.hash;
            let hostile = hostile.rehashed();
            let (observed, changed) =
                offer_hostile(w, di, chain, hostile, pin_head.map(|pin| pin.seq));
            result(observed, changed)
        }
        AttackKind::StampBackwardsEntry => {
            let head = copy_head?;
            let last_at = copy_last_at?;
            let mut hostile = held.last()?.clone();
            hostile.seq = head.seq.next()?;
            hostile.prev = head.hash;
            hostile.at = UnixTime::from_secs(last_at.as_secs().checked_sub(1)?);
            let hostile = hostile.rehashed();
            let (observed, changed) =
                offer_hostile(w, di, chain, hostile, pin_head.map(|pin| pin.seq));
            result(observed, changed)
        }
        AttackKind::OldEpochEntry => {
            let head = copy_head?;
            let mut hostile = held.last()?.clone();
            hostile.seq = head.seq.next()?;
            hostile.prev = head.hash;
            hostile.at = copy_last_at?;
            hostile.epoch = RestoreEpoch::new(copy_epoch?.get().checked_sub(1)?);
            let hostile = hostile.rehashed();
            let (observed, changed) =
                offer_hostile(w, di, chain, hostile, pin_head.map(|pin| pin.seq));
            result(observed, changed)
        }
        AttackKind::ReplayOldRestore => {
            let old = w
                .ledger
                .record_log
                .iter()
                .rev()
                .find(|r| {
                    r.epoch <= floor && r.signatures.iter().any(|s| s.by == RestoreSigner::MapKey)
                })?
                .clone();
            let (observed, changed) = inject_restore(w, di, old);
            result(observed, changed)
        }
        AttackKind::GroupAloneRestore | AttackKind::RestoreWithoutSignature => {
            let record = rollback_record(w, di, floor, now)?;
            let record = if kind == AttackKind::GroupAloneRestore {
                let mut signed = record;
                signed.signatures = vec![w.keys.restore_sig(&signed, RestoreSigner::Group)];
                signed
            } else {
                record
            };
            let (observed, changed) = inject_restore(w, di, record);
            result(observed, changed)
        }
        AttackKind::PeerJunk => {
            let pin = pin_head?;
            let latest = w
                .ledger
                .head_log
                .iter()
                .rev()
                .find(|h| h.head.chain == chain)?
                .clone();
            let mut junk = latest;
            junk.head.seq = pin.seq.next()?;
            flip_sig(&mut junk);
            let (observed, changed) = inject_peer(w, di, junk);
            result(observed, changed)
        }
        AttackKind::PeerFork => {
            let pin = pin_head?;
            let standing = floor.max(pin_signed.as_ref().map_or(RestoreEpoch::ZERO, |s| s.epoch));
            let forked = Head {
                hash: hash32(
                    "azlin-sim-peer-fork-v1",
                    &[pin.hash.as_bytes(), &w.step_no.to_be_bytes()],
                ),
                ..pin
            };
            let signed = w.keys.sign_head(forked, standing, now);
            w.ledger
                .signed
                .insert((forked.chain, forked.seq, forked.hash));
            let (observed, changed) = inject_peer(w, di, signed);
            result(observed, changed)
        }
    }
}

/// A head the device must refuse before it reaches the pin: no signature, or another chain's.
fn shown_unsigned(
    w: &mut World,
    di: usize,
    chain: azlin_chain::ChainId,
    head: SignedHead,
    result: impl Fn(Observed, bool) -> Option<Result3>,
) -> Option<Result3> {
    let alarms = alarms_of(w, di);
    match require_signed(&chain, Some(&head)) {
        Err(alarm) => {
            w.push_alarm(di + 1, alarm);
            let raised = alarms_of(w, di) > alarms;
            result(
                Observed::Refused {
                    class: RefusalClass::Alarm,
                    alarm: raised,
                    what: String::from("UnsignedHead"),
                },
                false,
            )
        }
        Ok(accepted) => {
            let accepted = accepted.clone();
            let (observed, changed) = inject_head(w, di, chain, accepted);
            result(observed, changed)
        }
    }
}

/// A record that would take the device back: epoch above its floor, positions one behind its pins.
fn rollback_record(
    w: &World,
    di: usize,
    floor: RestoreEpoch,
    now: UnixTime,
) -> Option<RestoreRecord> {
    let dev = w.devices.get(di)?;
    let mut positions = Vec::new();
    for id in &dev.chain_ids {
        let pin = dev.pin.head(id)?;
        let Some(previous) = pin.seq.get().checked_sub(1) else {
            continue;
        };
        let entry = w
            .ledger
            .entry_log
            .iter()
            .rev()
            .find(|e| e.chain == *id && e.seq.get() == previous)?;
        positions.push(entry.head());
    }
    if positions.is_empty() {
        return None;
    }
    Some(RestoreRecord {
        epoch: floor.next()?,
        at: now,
        positions,
        signatures: Vec::new(),
    })
}
