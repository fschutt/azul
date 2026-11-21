//! The head register: compare-and-swap (check 1), history and retention (check 2), the lost reply
//! (check 3) and the server half of a restore (check 4) of lane W1-S16a (brief section 6).
//!
//! Implements planning/architecture.md section 5.13 (`head_get`, `head_cas`, `head_history`,
//! `restore_as_of`), section 5.8 (retention and locks), section 2.5 (the client first reads whether
//! the write landed) and section 6.10 A7 and V3.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use azlin_seams::sim::Rng;
use azlin_seams::{ByteCount, Span};
use azlin_sync::{
    resolve_lost_cas, CasOutcome, HeadLimits, HeadRecord, HeadRegister, HeadValue, LockTerm,
    LostCas, Requester, RestoreAsOfRefusal, RestoreAuth, ResumeRefusal, Version, Writer, HEAD_MAX,
    HEAD_RECORD_OVERHEAD, HISTORY_PAGE_BYTES, HISTORY_PAGE_MAX,
};
use common::*;

fn new_register() -> HeadRegister {
    HeadRegister::new(space(1))
}

fn ok(outcome: CasOutcome) -> HeadRecord {
    match outcome {
        CasOutcome::Ok { record } => record,
        other => panic!("expected a landed write, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Check 1: CAS
// ---------------------------------------------------------------------------------------------

#[test]
fn first_cas_with_none_creates_version_1() {
    let mut register = new_register();
    assert!(register.get().is_none());
    let record = ok(register.cas(None, value("a"), device(1), at(100), LockTerm::Unlocked));
    assert_eq!(record.version, Version::FIRST);
    assert_eq!(record.version.get(), 1);
    assert_eq!(record.value, value("a"));
    assert_eq!(record.at, at(100));
    assert_eq!(record.writer, Writer::Device(device(1)));
    assert_eq!(record.restore_epoch, epoch(0));
    assert_eq!(record.retain_until, None);
    assert_eq!(record.restored_from, None);
    assert_eq!(register.get(), Some(&record));
    assert_eq!(register.versions_held(), 1);
}

#[test]
fn cas_with_the_current_version_advances_by_one() {
    let mut register = new_register();
    fill(&mut register, &["a"]);
    let second = ok(register.cas(
        Some(version(1)),
        value("b"),
        device(2),
        at(200),
        LockTerm::Unlocked,
    ));
    assert_eq!(second.version, version(2));
    assert_eq!(second.writer, Writer::Device(device(2)));
    let third = ok(register.cas(
        Some(version(2)),
        value("c"),
        device(1),
        at(300),
        LockTerm::Unlocked,
    ));
    assert_eq!(third.version, version(3));
    assert_eq!(register.get(), Some(&third));
    // Every version is kept.
    let held: Vec<u64> = register.records().map(|r| r.version.get()).collect();
    assert_eq!(held, vec![1, 2, 3]);
}

#[test]
fn cas_with_a_stale_version_conflicts_and_returns_the_current_head() {
    let mut register = new_register();
    fill(&mut register, &["a", "b", "c"]);
    let before = register.state_hash();
    let head = register.get().cloned();
    // A stale version, and a version the register has not reached: both lose.
    for expected in [version(1), version(2), version(9)] {
        let outcome = register.cas(
            Some(expected),
            value("late"),
            device(2),
            at(500),
            LockTerm::Unlocked,
        );
        assert_eq!(
            outcome,
            CasOutcome::Conflict {
                current: head.clone()
            }
        );
    }
    assert_eq!(register.state_hash(), before, "a lost CAS changes nothing");
    assert_eq!(register.get(), head.as_ref());
}

#[test]
fn cas_expecting_none_on_an_existing_register_conflicts() {
    let mut register = new_register();
    fill(&mut register, &["a"]);
    let before = register.state_hash();
    let head = register.get().cloned();
    let outcome = register.cas(None, value("again"), device(2), at(500), LockTerm::Unlocked);
    assert_eq!(outcome, CasOutcome::Conflict { current: head });
    assert_eq!(register.state_hash(), before);
}

#[test]
fn cas_expecting_a_version_on_an_empty_register_conflicts() {
    let mut register = new_register();
    let outcome = register.cas(
        Some(version(1)),
        value("a"),
        device(1),
        at(100),
        LockTerm::Unlocked,
    );
    assert_eq!(outcome, CasOutcome::Conflict { current: None });
    assert!(register.get().is_none());
}

#[test]
fn head_value_over_head_max_is_too_large() {
    assert_eq!(HEAD_MAX.get(), 4096, "the head value limit of the brief");
    let mut register = new_register();
    // 4,097 bytes: one over.
    let too_big = HeadValue::new(vec![7u8; 4097]);
    let outcome = register.cas(None, too_big, device(1), at(100), LockTerm::Unlocked);
    assert_eq!(
        outcome,
        CasOutcome::TooLarge {
            size: ByteCount::new(4097)
        }
    );
    assert!(register.get().is_none(), "nothing was written");
    // Exactly HEAD_MAX is accepted.
    let exact = HeadValue::new(vec![7u8; 4096]);
    let record = ok(register.cas(None, exact, device(1), at(100), LockTerm::Unlocked));
    assert_eq!(record.value.size(), HEAD_MAX);
    // The size is judged before the version: a too-large value with a stale expected version says so.
    let before = register.state_hash();
    let outcome = register.cas(
        Some(version(7)),
        HeadValue::new(vec![1u8; 5000]),
        device(1),
        at(101),
        LockTerm::Unlocked,
    );
    assert!(
        matches!(outcome, CasOutcome::TooLarge { .. }),
        "{outcome:?}"
    );
    assert_eq!(register.state_hash(), before);
}

#[test]
fn a_write_keeps_the_restore_epoch_of_the_head_and_stores_its_lock() {
    let mut register = new_register();
    fill(&mut register, &["a", "b"]);
    register
        .restore_as_of(at(10), at(100), operator_auth(space(1), at(10), 3))
        .expect("restore");
    let head = register.get().cloned().expect("head");
    assert_eq!(head.restore_epoch, epoch(3));
    let record = ok(register.cas(
        Some(head.version),
        value("after"),
        device(1),
        at(200),
        LockTerm::Until(at(9_999)),
    ));
    assert_eq!(
        record.restore_epoch,
        epoch(3),
        "a write carries the epoch on"
    );
    assert_eq!(record.retain_until, Some(at(9_999)));
    assert_eq!(record.restored_from, None);
}

#[test]
fn the_version_counter_never_wraps() {
    let last = Version::new(u64::MAX).expect("the largest version");
    assert_eq!(last.next(), None);
    let record = HeadRecord {
        version: last,
        value: value("x"),
        at: at(1),
        writer: Writer::Device(device(1)),
        restore_epoch: epoch(0),
        retain_until: None,
        restored_from: None,
    };
    let mut register = HeadRegister::resume(space(1), HeadLimits::standard(), false, vec![record])
        .expect("a register at the largest version");
    let before = register.state_hash();
    let outcome = register.cas(Some(last), value("y"), device(1), at(2), LockTerm::Unlocked);
    assert_eq!(outcome, CasOutcome::VersionExhausted);
    assert_eq!(register.state_hash(), before);
    assert!(Version::new(0).is_none(), "there is no version 0");
}

#[test]
fn a_register_resumes_from_its_records_and_refuses_records_that_do_not_hold_together() {
    let mut register = new_register();
    fill(&mut register, &["a", "b", "c"]);
    let rows: Vec<HeadRecord> = register.records().cloned().collect();
    let again = HeadRegister::resume(space(1), HeadLimits::standard(), false, rows.clone())
        .expect("rows of a real register");
    assert_eq!(again.state_hash(), register.state_hash());
    assert_eq!(again, register);

    // Versions not ascending.
    let mut swapped = rows.clone();
    swapped.swap(0, 1);
    assert_eq!(
        HeadRegister::resume(space(1), HeadLimits::standard(), false, swapped),
        Err(ResumeRefusal::NotAscending {
            version: version(1)
        })
    );
    // An epoch that goes down.
    let mut down = rows.clone();
    down[1].restore_epoch = epoch(5);
    assert_eq!(
        HeadRegister::resume(space(1), HeadLimits::standard(), false, down),
        Err(ResumeRefusal::EpochGoesDown {
            version: version(3)
        })
    );
    // A value over the limit.
    let mut big = rows.clone();
    big[2].value = HeadValue::new(vec![0u8; 5000]);
    assert_eq!(
        HeadRegister::resume(space(1), HeadLimits::standard(), false, big),
        Err(ResumeRefusal::TooLarge {
            version: version(3)
        })
    );
    // A restore record that names a version that is not below it.
    let mut odd = rows.clone();
    odd[1].restored_from = Some(version(2));
    assert_eq!(
        HeadRegister::resume(space(1), HeadLimits::standard(), false, odd),
        Err(ResumeRefusal::BadRestoredFrom {
            version: version(2)
        })
    );
    // More than the limits allow.
    let tight = HeadLimits::new(2, ByteCount::mib(1)).expect("limits");
    assert_eq!(
        HeadRegister::resume(space(1), tight, false, rows),
        Err(ResumeRefusal::OverLimit)
    );
}

#[test]
fn the_state_hash_sees_every_field_of_every_record() {
    let mut base = new_register();
    fill(&mut base, &["a", "b"]);
    let hash = base.state_hash();
    let rows: Vec<HeadRecord> = base.records().cloned().collect();
    let rebuild = |rows: Vec<HeadRecord>, hold: bool| {
        HeadRegister::resume(space(1), HeadLimits::standard(), hold, rows)
            .expect("rows")
            .state_hash()
    };
    assert_eq!(rebuild(rows.clone(), false), hash);
    assert_ne!(rebuild(rows.clone(), true), hash, "the legal hold");
    let mutate = |change: &dyn Fn(&mut HeadRecord)| {
        let mut changed = rows.clone();
        change(&mut changed[1]);
        rebuild(changed, false)
    };
    assert_ne!(mutate(&|r| r.value = value("z")), hash, "value");
    assert_ne!(mutate(&|r| r.at = at(99_999)), hash, "stamp");
    assert_ne!(mutate(&|r| r.writer = Writer::Operator), hash, "writer");
    assert_ne!(
        mutate(&|r| r.writer = Writer::Device(device(9))),
        hash,
        "writer device"
    );
    assert_ne!(mutate(&|r| r.restore_epoch = epoch(1)), hash, "epoch");
    assert_ne!(mutate(&|r| r.retain_until = Some(at(1))), hash, "lock");
    assert_ne!(
        mutate(&|r| r.restored_from = Some(version(1))),
        hash,
        "restored from"
    );
    let other_space = HeadRegister::resume(space(2), HeadLimits::standard(), false, rows)
        .expect("rows")
        .state_hash();
    assert_ne!(other_space, hash, "the space");
}

// ---------------------------------------------------------------------------------------------
// Check 1: the property, with lost replies, against an independent model
// ---------------------------------------------------------------------------------------------

/// The retention of the property's prunes, in seconds.
const RETENTION: u64 = 100;

/// The model's record: the test's own, written independently of the register.
#[derive(Debug)]
struct Entry {
    at: u64,
    value: Vec<u8>,
    writer: u8,
}

/// A write whose reply was lost: what the device expected and what it wrote.
struct Pending {
    expected: Option<Version>,
    mine: HeadValue,
    text: Vec<u8>,
}

/// What really happened to the write that wanted version `want`, from the model alone.
enum Truth {
    /// The head has not reached `want`.
    NotYet,
    /// `want` is this device's write of this value.
    Mine,
    /// `want` is somebody else's.
    Other,
    /// `want` was pruned: the model cannot say.
    Pruned,
}

fn truth(model: &BTreeMap<u64, Entry>, want: u64, text: &[u8], writer: u8) -> Truth {
    let head = model.keys().next_back().copied().unwrap_or(0);
    if want > head {
        return Truth::NotYet;
    }
    match model.get(&want) {
        Some(entry) if entry.value == text && entry.writer == writer => Truth::Mine,
        Some(_) => Truth::Other,
        None => Truth::Pruned,
    }
}

struct Sim {
    rng: Rng,
    register: HeadRegister,
    model: BTreeMap<u64, Entry>,
    views: Vec<Option<Version>>,
    pending: Vec<Option<Pending>>,
    /// (the expected version, the version) of every write that landed, in the order they landed.
    winners: Vec<(Option<u64>, u64)>,
    counter: u64,
    now: u64,
    seed: u64,
}

impl Sim {
    fn pick(&mut self, n: usize) -> usize {
        usize::try_from(self.rng.below(n as u64).expect("a non-empty range")).expect("small")
    }

    fn model_head(&self) -> Option<u64> {
        self.model.keys().next_back().copied()
    }

    /// One CAS by device index `d` (the device number is `d + 1`), applied to the register and
    /// checked against the model at once. `lost`: the reply is lost.
    fn write(&mut self, d: usize, expected: Option<Version>, lost: bool) {
        self.counter += 1;
        let text = format!("v{}", self.counter).into_bytes();
        let mine = HeadValue::new(text.clone());
        let writer = d as u8 + 1;
        let outcome = self.register.cas(
            expected,
            mine.clone(),
            device(writer),
            at(self.now),
            LockTerm::Unlocked,
        );
        let head = self.model_head();
        let agrees = expected.map(|v| v.get()) == head;
        match outcome {
            CasOutcome::Ok { record } => {
                assert!(
                    agrees,
                    "seed {}: the register let a stale write win",
                    self.seed
                );
                let won = head.map_or(1, |h| h + 1);
                assert_eq!(record.version.get(), won, "seed {}", self.seed);
                self.model.insert(
                    won,
                    Entry {
                        at: self.now,
                        value: text.clone(),
                        writer,
                    },
                );
                self.winners.push((expected.map(|v| v.get()), won));
                if !lost {
                    self.views[d] = Some(record.version);
                }
            }
            CasOutcome::Conflict { current } => {
                assert!(
                    !agrees,
                    "seed {}: the register refused the current version",
                    self.seed
                );
                let model_head = head.and_then(|h| self.model.get(&h).map(|e| (h, e)));
                match (current.as_ref(), model_head) {
                    (None, None) => {}
                    (Some(record), Some((h, entry))) => {
                        assert_eq!(record.version.get(), h, "seed {}", self.seed);
                        assert_eq!(record.value.as_bytes(), entry.value.as_slice());
                    }
                    other => panic!(
                        "seed {}: the head differs from the model: {other:?}",
                        self.seed
                    ),
                }
                if !lost {
                    self.views[d] = current.map(|r| r.version);
                }
            }
            other => panic!("seed {}: unexpected outcome {other:?}", self.seed),
        }
        if lost {
            self.pending[d] = Some(Pending {
                expected,
                mine,
                text,
            });
        }
    }

    /// Device `d` resolves its lost reply and the result is compared with the model.
    fn resolve(&mut self, d: usize) {
        let Some(p) = self.pending[d].take() else {
            return;
        };
        let writer = d as u8 + 1;
        let want = match p.expected {
            None => 1,
            Some(v) => v.get() + 1,
        };
        let expect = match truth(&self.model, want, &p.text, writer) {
            Truth::NotYet | Truth::Other => LostCas::NotLanded,
            Truth::Mine => LostCas::Landed(version(want)),
            Truth::Pruned => LostCas::Unknown,
        };
        if let Some(current) = self.register.get().cloned() {
            let page = self.register.history(version(want), HISTORY_PAGE_MAX);
            let got = resolve_lost_cas(p.expected, &p.mine, device(writer), &current, &page);
            assert_eq!(
                got, expect,
                "seed {}: device {writer} wanted version {want}",
                self.seed
            );
        } else {
            // An empty register: nothing can have landed.
            assert!(matches!(expect, LostCas::NotLanded), "seed {}", self.seed);
        }
        self.views[d] = self.register.get().map(|r| r.version);
    }

    fn prune(&mut self) {
        let pruned = self.register.prune(at(self.now), Span::secs(RETENTION));
        let head = self.model_head();
        let now = self.now;
        let doomed: Vec<u64> = self
            .model
            .iter()
            .filter(|(v, e)| {
                Some(**v) != head && now.checked_sub(e.at).is_some_and(|age| age > RETENTION)
            })
            .map(|(v, _)| *v)
            .collect();
        for v in &doomed {
            self.model.remove(v);
        }
        let removed: Vec<u64> = pruned.removed.iter().map(|v| v.get()).collect();
        assert_eq!(
            removed, doomed,
            "seed {}: prune differs from the model",
            self.seed
        );
    }

    /// Every device that has no write in flight sends a CAS that expects the head, in a random
    /// order: exactly one of them wins.
    fn burst(&mut self) {
        let expected = self.register.get().map(|r| r.version);
        let mut order: Vec<usize> = (0..self.views.len())
            .filter(|d| self.pending[*d].is_none())
            .collect();
        self.rng.shuffle(&mut order);
        let before = self.winners.len();
        for d in &order {
            self.write(*d, expected, false);
        }
        let landed = self.winners.len() - before;
        if order.is_empty() {
            assert_eq!(landed, 0);
        } else {
            assert_eq!(
                landed,
                1,
                "seed {}: of {} writes that expected {:?} exactly one wins",
                self.seed,
                order.len(),
                expected
            );
        }
    }
}

fn schedule(seed: u64) {
    let mut rng = Rng::new(seed);
    let devices = 2 + usize::try_from(rng.below(4).expect("range")).expect("small");
    let steps = 20 + rng.below(40).expect("range");
    let mut sim = Sim {
        rng,
        register: new_register(),
        model: BTreeMap::new(),
        views: vec![None; devices],
        pending: (0..devices).map(|_| None).collect(),
        winners: Vec::new(),
        counter: 0,
        now: 1_000,
        seed,
    };
    for _ in 0..steps {
        sim.now += sim.rng.below(30).expect("range");
        match sim.rng.below(10).expect("range") {
            0 => sim.prune(),
            1 => sim.burst(),
            _ => {
                let d = sim.pick(devices);
                if sim.pending[d].is_some() {
                    sim.resolve(d);
                } else {
                    if sim.rng.chance(1, 2) {
                        sim.views[d] = sim.register.get().map(|r| r.version);
                    }
                    let lost = sim.rng.chance(1, 3);
                    let expected = sim.views[d];
                    sim.write(d, expected, lost);
                }
            }
        }
    }
    // Every device resolves what it still has in flight.
    for d in 0..devices {
        sim.resolve(d);
    }
    // The winners' versions are 1, 2, 3, ...: strictly increasing by one, each expecting the one
    // before it, and no two share an expected version.
    let mut expected_seen = BTreeSet::new();
    for (i, (expected, won)) in sim.winners.iter().enumerate() {
        assert_eq!(*won, i as u64 + 1, "seed {seed}: the winners' versions");
        let wanted = if i == 0 { None } else { Some(i as u64) };
        assert_eq!(
            *expected, wanted,
            "seed {seed}: winner {i} expected the head"
        );
        assert!(
            expected_seen.insert(*expected),
            "seed {seed}: two winners expected {expected:?}"
        );
    }
    // The register holds exactly what the model holds.
    let held: Vec<(u64, Vec<u8>)> = sim
        .register
        .records()
        .map(|r| (r.version.get(), r.value.as_bytes().to_vec()))
        .collect();
    let modelled: Vec<(u64, Vec<u8>)> = sim
        .model
        .iter()
        .map(|(v, e)| (*v, e.value.clone()))
        .collect();
    assert_eq!(
        held, modelled,
        "seed {seed}: the register against the model"
    );
}

#[test]
fn of_concurrent_cas_with_one_expected_version_exactly_one_wins() {
    for seed in 0..5_000u64 {
        schedule(seed);
    }
}

// ---------------------------------------------------------------------------------------------
// Check 2: history and retention
// ---------------------------------------------------------------------------------------------

fn register_of(count: u64) -> HeadRegister {
    let mut register = new_register();
    for n in 1..=count {
        let expected = register.get().map(|r| r.version);
        let outcome = register.cas(
            expected,
            value(&format!("value {n}")),
            device(1),
            at(10 * n),
            LockTerm::Unlocked,
        );
        assert!(matches!(outcome, CasOutcome::Ok { .. }));
    }
    register
}

#[test]
fn history_is_oldest_first_bounded_and_resumable() {
    let register = register_of(600);
    // The first page: the oldest records, 256 of them, whatever limit was asked for.
    let page = register.history(Version::FIRST, 1000);
    assert_eq!(page.records.len(), usize::from(HISTORY_PAGE_MAX));
    assert_eq!(page.records[0].version, version(1));
    assert_eq!(page.records[255].version, version(256));
    assert_eq!(page.next, Some(version(257)));
    assert_eq!(page.oldest, Some(version(1)));
    assert_eq!(page.newest, Some(version(600)));
    // Following `next` reads everything, once, in order.
    let mut seen = Vec::new();
    let mut from = Some(Version::FIRST);
    let mut pages = 0;
    while let Some(start) = from {
        let page = register.history(start, 100);
        assert!(page.records.len() <= 100);
        assert!(!page.records.is_empty(), "a page always makes progress");
        seen.extend(page.records.iter().map(|r| r.version.get()));
        from = page.next;
        pages += 1;
    }
    assert_eq!(pages, 6);
    assert_eq!(seen, (1..=600u64).collect::<Vec<_>>());
    // The last page reaches the head: no `next`.
    let last = register.history(version(599), 10);
    assert_eq!(last.records.len(), 2);
    assert_eq!(last.next, None);
    // A `from` above the head: nothing.
    let beyond = register.history(version(601), 10);
    assert!(beyond.records.is_empty());
    assert_eq!(beyond.next, None);
    // A limit of 0 counts as 1 so that a reader advances.
    let one = register.history(version(5), 0);
    assert_eq!(one.records.len(), 1);
    assert_eq!(one.next, Some(version(6)));
}

#[test]
fn history_page_is_bounded_by_bytes_and_a_big_record_still_makes_progress() {
    let mut register = new_register();
    for n in 1..=300u64 {
        let expected = register.get().map(|r| r.version);
        let outcome = register.cas(
            expected,
            HeadValue::new(vec![(n % 250) as u8; 4096]),
            device(1),
            at(n),
            LockTerm::Unlocked,
        );
        assert!(matches!(outcome, CasOutcome::Ok { .. }), "{n}: {outcome:?}");
    }
    let page = register.history(Version::FIRST, HISTORY_PAGE_MAX);
    let bytes: u64 = page.records.iter().map(|r| r.size().get()).sum();
    assert!(
        bytes <= HISTORY_PAGE_BYTES.get(),
        "a page of {} records is {bytes} bytes",
        page.records.len()
    );
    assert!(
        page.records.len() < usize::from(HISTORY_PAGE_MAX),
        "the byte bound cut the page"
    );
    assert!(page.next.is_some());
    // Following the pages still reads every version.
    let mut count = 0usize;
    let mut from = Some(Version::FIRST);
    while let Some(start) = from {
        let page = register.history(start, HISTORY_PAGE_MAX);
        assert!(!page.records.is_empty());
        count += page.records.len();
        from = page.next;
    }
    assert_eq!(count, 300);
}

#[test]
fn history_from_below_the_oldest_starts_at_the_oldest() {
    let mut register = register_of(10);
    let pruned = register.prune(at(500), Span::secs(250));
    assert!(!pruned.removed.is_empty());
    let oldest = register.records().next().map(|r| r.version).expect("kept");
    assert!(oldest > Version::FIRST);
    let page = register.history(Version::FIRST, 5);
    assert_eq!(page.records[0].version, oldest);
    assert_eq!(page.oldest, Some(oldest));
}

#[test]
fn prune_drops_versions_older_than_retention_but_never_the_current_head() {
    // Ten versions stamped 10, 20, ..., 100.
    let mut register = register_of(10);
    // At now = 200 with a retention of 105 s: ages are 190, 180, ..., 100: those above 105 go (the
    // stamps 10 to 90, nine versions); the head, stamped 100, is 100 s old and stays anyway.
    let pruned = register.prune(at(200), Span::secs(105));
    let removed: Vec<u64> = pruned.removed.iter().map(|v| v.get()).collect();
    assert_eq!(removed, (1..=9u64).collect::<Vec<_>>());
    assert_eq!(register.versions_held(), 1);
    assert_eq!(register.get().map(|r| r.version), Some(version(10)));
    assert!(pruned.freed.get() > 0);
    assert_eq!(
        register.bytes_held().get(),
        register.get().expect("head").size().get()
    );
    // Far in the future everything is past its retention, but the head is never dropped.
    let pruned = register.prune(at(1_000_000), Span::secs(1));
    assert!(pruned.removed.is_empty());
    assert_eq!(register.get().map(|r| r.version), Some(version(10)));
}

#[test]
fn prune_boundary_is_exclusive_and_future_stamps_are_kept() {
    let mut register = register_of(3); // stamps 10, 20, 30
                                       // Version 1 is exactly 50 s old at now = 60 with a retention of 50: kept (not strictly older).
    let pruned = register.prune(at(60), Span::secs(50));
    assert!(pruned.removed.is_empty(), "{:?}", pruned.removed);
    // One second later it is older than the retention.
    let pruned = register.prune(at(61), Span::secs(50));
    assert_eq!(pruned.removed, vec![version(1)]);
    // A version stamped after `now` has no age and is kept.
    let mut future = new_register();
    fill(&mut future, &["a", "b", "c"]);
    let pruned = future.prune(at(5), Span::secs(0));
    assert!(pruned.removed.is_empty());
    assert_eq!(future.versions_held(), 3);
}

#[test]
fn a_version_under_a_lock_survives_prune_until_retain_until() {
    let mut register = new_register();
    // v1: no lock, stamped 10. v2: locked until 500, stamped 20. v3: locked until 150, stamped 30.
    // v4 (head): stamped 40.
    for (n, lock) in [
        (1u64, LockTerm::Unlocked),
        (2, LockTerm::Until(at(500))),
        (3, LockTerm::Until(at(150))),
        (4, LockTerm::Unlocked),
    ] {
        let expected = register.get().map(|r| r.version);
        let outcome = register.cas(
            expected,
            value(&format!("v{n}")),
            device(1),
            at(10 * n),
            lock,
        );
        assert!(matches!(outcome, CasOutcome::Ok { .. }));
    }
    // At now = 200 with a retention of 50 every version is past it. v1 goes; v2 is held by its lock
    // (until 500); v3's lock ended at 150, so it goes; the head stays.
    let pruned = register.prune(at(200), Span::secs(50));
    assert_eq!(pruned.removed, vec![version(1), version(3)]);
    assert_eq!(pruned.kept_locked, 1);
    let held: Vec<u64> = register.records().map(|r| r.version.get()).collect();
    assert_eq!(held, vec![2, 4]);
    // Still locked a second before the end of the lock, gone at the end of it.
    let pruned = register.prune(at(499), Span::secs(50));
    assert!(pruned.removed.is_empty());
    assert_eq!(pruned.kept_locked, 1);
    let pruned = register.prune(at(500), Span::secs(50));
    assert_eq!(pruned.removed, vec![version(2)]);
    assert_eq!(register.versions_held(), 1);
}

#[test]
fn prune_is_deterministic_for_the_same_now() {
    let mut a = register_of(40);
    let mut b = register_of(40);
    assert_eq!(a.state_hash(), b.state_hash());
    let pa = a.prune(at(350), Span::secs(100));
    let pb = b.prune(at(350), Span::secs(100));
    assert_eq!(pa, pb);
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a, b);
    // A second prune at the same now drops nothing more.
    let again = a.prune(at(350), Span::secs(100));
    assert!(again.removed.is_empty());
    // A different now gives a different state.
    let mut c = register_of(40);
    c.prune(at(400), Span::secs(100));
    assert_ne!(c.state_hash(), a.state_hash());
}

#[test]
fn a_legal_hold_stops_every_prune() {
    let mut register = register_of(10);
    register.set_legal_hold(true);
    assert!(register.legal_hold());
    let held_hash = register.state_hash();
    let pruned = register.prune(at(1_000_000), Span::secs(1));
    assert!(pruned.removed.is_empty());
    assert!(pruned.held);
    assert_eq!(register.versions_held(), 10);
    assert_eq!(register.state_hash(), held_hash);
    // Lifting the hold lets the prune go on (the hold is part of the state hash).
    register.set_legal_hold(false);
    assert_ne!(register.state_hash(), held_hash);
    let pruned = register.prune(at(1_000_000), Span::secs(1));
    assert_eq!(pruned.removed.len(), 9);
    assert!(!pruned.held);
}

// ---------------------------------------------------------------------------------------------
// Check 3: the lost reply
// ---------------------------------------------------------------------------------------------

fn page_from(register: &HeadRegister, from: u64) -> azlin_sync::HistoryPage {
    register.history(version(from), HISTORY_PAGE_MAX)
}

#[test]
fn lost_reply_finds_my_write_as_the_head() {
    let mut register = new_register();
    fill(&mut register, &["base"]);
    // Device 2 writes v2 and loses the reply.
    let mine = value("mine");
    let _lost = ok(register.cas(
        Some(version(1)),
        mine.clone(),
        device(2),
        at(50),
        LockTerm::Unlocked,
    ));
    let head = register.get().cloned().expect("head");
    let got = resolve_lost_cas(
        Some(version(1)),
        &mine,
        device(2),
        &head,
        &page_from(&register, 2),
    );
    assert_eq!(got, LostCas::Landed(version(2)));
    // The same for a create that was lost.
    let mut created = new_register();
    let first = value("first");
    let _lost = ok(created.cas(None, first.clone(), device(3), at(10), LockTerm::Unlocked));
    let head = created.get().cloned().expect("head");
    let got = resolve_lost_cas(None, &first, device(3), &head, &page_from(&created, 1));
    assert_eq!(got, LostCas::Landed(version(1)));
}

#[test]
fn lost_reply_finds_my_write_in_history_after_others_advanced() {
    let mut register = new_register();
    fill(&mut register, &["base"]);
    let mine = value("mine");
    let _lost = ok(register.cas(
        Some(version(1)),
        mine.clone(),
        device(2),
        at(50),
        LockTerm::Unlocked,
    ));
    // Others advance the head three more times.
    for (n, text) in ["x", "y", "z"].iter().enumerate() {
        let expected = register.get().map(|r| r.version);
        let _ = ok(register.cas(
            expected,
            value(text),
            device(3),
            at(60 + n as u64),
            LockTerm::Unlocked,
        ));
    }
    let head = register.get().cloned().expect("head");
    assert_eq!(head.version, version(5));
    // The device asks for history from the version its write would have created.
    let got = resolve_lost_cas(
        Some(version(1)),
        &mine,
        device(2),
        &head,
        &page_from(&register, 2),
    );
    assert_eq!(got, LostCas::Landed(version(2)));
    // A page that starts earlier shows it too.
    let got = resolve_lost_cas(
        Some(version(1)),
        &mine,
        device(2),
        &head,
        &page_from(&register, 1),
    );
    assert_eq!(got, LostCas::Landed(version(2)));
}

#[test]
fn lost_reply_not_landed_when_history_shows_no_such_value() {
    let mut register = new_register();
    fill(&mut register, &["base"]);
    let mine = value("mine");
    // Device 3 won the race for v2; device 2's write (also expecting v1) lost.
    let _won = ok(register.cas(
        Some(version(1)),
        value("theirs"),
        device(3),
        at(50),
        LockTerm::Unlocked,
    ));
    let _more = ok(register.cas(
        Some(version(2)),
        value("more"),
        device(3),
        at(60),
        LockTerm::Unlocked,
    ));
    let head = register.get().cloned().expect("head");
    let got = resolve_lost_cas(
        Some(version(1)),
        &mine,
        device(2),
        &head,
        &page_from(&register, 2),
    );
    assert_eq!(got, LostCas::NotLanded);

    // The head has not moved at all: the write did not land.
    let mut still = new_register();
    fill(&mut still, &["base"]);
    let head = still.get().cloned().expect("head");
    let got = resolve_lost_cas(
        Some(version(1)),
        &mine,
        device(2),
        &head,
        &page_from(&still, 2),
    );
    assert_eq!(got, LostCas::NotLanded);

    // The same device wrote ANOTHER value at that version: not this write.
    let mut other = new_register();
    fill(&mut other, &["base"]);
    let _ = ok(other.cas(
        Some(version(1)),
        value("other value"),
        device(2),
        at(50),
        LockTerm::Unlocked,
    ));
    let head = other.get().cloned().expect("head");
    let got = resolve_lost_cas(
        Some(version(1)),
        &mine,
        device(2),
        &head,
        &page_from(&other, 2),
    );
    assert_eq!(got, LostCas::NotLanded);

    // Another device wrote the SAME value at that version: not my write either.
    let mut same_value = new_register();
    fill(&mut same_value, &["base"]);
    let _ = ok(same_value.cas(
        Some(version(1)),
        mine.clone(),
        device(4),
        at(50),
        LockTerm::Unlocked,
    ));
    let head = same_value.get().cloned().expect("head");
    let got = resolve_lost_cas(
        Some(version(1)),
        &mine,
        device(2),
        &head,
        &page_from(&same_value, 2),
    );
    assert_eq!(got, LostCas::NotLanded);

    // A create that lost to another device's create.
    let mut created = new_register();
    let _ = ok(created.cas(None, value("theirs"), device(3), at(10), LockTerm::Unlocked));
    let head = created.get().cloned().expect("head");
    let got = resolve_lost_cas(None, &mine, device(2), &head, &page_from(&created, 1));
    assert_eq!(got, LostCas::NotLanded);
}

#[test]
fn lost_reply_with_too_short_history_is_unknown_never_not_landed() {
    // Device 2's write took v2 and landed; the head moved on to v6; the history before v4 was pruned.
    let mut register = new_register();
    fill(&mut register, &["base"]); // v1 at 10
    let mine = value("mine");
    let _ = ok(register.cas(
        Some(version(1)),
        mine.clone(),
        device(2),
        at(20),
        LockTerm::Unlocked,
    ));
    for (n, text) in ["c", "d", "e", "f"].iter().enumerate() {
        let expected = register.get().map(|r| r.version);
        let _ = ok(register.cas(
            expected,
            value(text),
            device(3),
            at(1_450 + n as u64),
            LockTerm::Unlocked,
        ));
    }
    assert_eq!(register.get().map(|r| r.version), Some(version(6)));
    let pruned = register.prune(at(1_500), Span::secs(100));
    assert_eq!(
        pruned.removed,
        vec![version(1), version(2)],
        "v1 and v2 are past the retention"
    );
    let head = register.get().cloned().expect("head");
    // The page the device reads starts at the oldest the register holds: it does not show v2.
    let page = page_from(&register, 2);
    assert_eq!(page.oldest, Some(version(3)));
    let got = resolve_lost_cas(Some(version(1)), &mine, device(2), &head, &page);
    assert_eq!(got, LostCas::Unknown);
    // A page that was cut short before v2 is no better: the answer is still "I cannot tell".
    let mut fresh = new_register();
    fill(&mut fresh, &["base"]);
    let _ = ok(fresh.cas(
        Some(version(1)),
        mine.clone(),
        device(2),
        at(20),
        LockTerm::Unlocked,
    ));
    for (n, text) in ["c", "d", "e"].iter().enumerate() {
        let expected = fresh.get().map(|r| r.version);
        let _ = ok(fresh.cas(
            expected,
            value(text),
            device(3),
            at(30 + n as u64),
            LockTerm::Unlocked,
        ));
    }
    let head = fresh.get().cloned().expect("head");
    let late_page = page_from(&fresh, 4); // asked from the wrong version
    let got = resolve_lost_cas(Some(version(1)), &mine, device(2), &head, &late_page);
    assert_eq!(got, LostCas::Unknown);
    // With the right page it is decided.
    let got = resolve_lost_cas(
        Some(version(1)),
        &mine,
        device(2),
        &head,
        &page_from(&fresh, 2),
    );
    assert_eq!(got, LostCas::Landed(version(2)));
    // And an empty page cannot say either.
    let empty = azlin_sync::HistoryPage {
        records: Vec::new(),
        next: None,
        oldest: None,
        newest: None,
    };
    let got = resolve_lost_cas(Some(version(1)), &mine, device(2), &head, &empty);
    assert_eq!(got, LostCas::Unknown);
}

#[test]
fn a_restore_record_is_never_my_landed_write() {
    let mut register = new_register();
    fill(&mut register, &["a", "b"]);
    // An owner restores through device 2; device 2's own CAS (expecting v2) lost the race, and its
    // value happens to equal the restored one: the restore record is not its write.
    let restored = register
        .restore_as_of(at(10), at(100), owner_auth(space(1), at(10), 1, 2))
        .expect("restore");
    assert_eq!(restored.record.version, version(3));
    let mine = value("a"); // the very value the restore carries
    let head = register.get().cloned().expect("head");
    assert!(!head.is_write_by(device(2)));
    let got = resolve_lost_cas(
        Some(version(2)),
        &mine,
        device(2),
        &head,
        &page_from(&register, 3),
    );
    assert_eq!(got, LostCas::NotLanded);
}

#[test]
fn a_write_that_expected_the_largest_version_cannot_have_landed() {
    let last = Version::new(u64::MAX).expect("largest");
    let head = HeadRecord {
        version: last,
        value: value("x"),
        at: at(1),
        writer: Writer::Device(device(1)),
        restore_epoch: epoch(0),
        retain_until: None,
        restored_from: None,
    };
    let page = azlin_sync::HistoryPage {
        records: vec![head.clone()],
        next: None,
        oldest: Some(last),
        newest: Some(last),
    };
    let got = resolve_lost_cas(Some(last), &value("y"), device(1), &head, &page);
    assert_eq!(got, LostCas::NotLanded);
}

// ---------------------------------------------------------------------------------------------
// Check 4: restore, the server half
// ---------------------------------------------------------------------------------------------

/// Three versions stamped 10, 20, 30 holding "a", "b", "c".
fn three() -> HeadRegister {
    let mut register = new_register();
    fill(&mut register, &["a", "b", "c"]);
    register
}

#[test]
fn restore_as_of_writes_a_new_higher_version_with_the_old_value_and_epoch_plus_one() {
    let mut register = three();
    let before: Vec<HeadRecord> = register.records().cloned().collect();
    // As of 25 the head was v2 ("b"): the restore writes v4 with that value.
    let restored = register
        .restore_as_of(at(25), at(40), operator_auth(space(1), at(25), 1))
        .expect("restore");
    assert_eq!(restored.replaced, version(3));
    assert_eq!(restored.restored_from, version(2));
    let head = register.get().cloned().expect("head");
    assert_eq!(head, restored.record);
    assert_eq!(
        head.version,
        version(4),
        "a NEW, higher version: the number never goes back"
    );
    assert_eq!(head.value, value("b"), "the old value");
    assert_eq!(
        head.restore_epoch,
        epoch(1),
        "the epoch of the head (0) plus one"
    );
    assert_eq!(head.restored_from, Some(version(2)));
    assert_eq!(head.writer, Writer::Operator);
    assert_eq!(head.at, at(40));
    // Nothing of the history changed: the lost versions are still there.
    let after: Vec<HeadRecord> = register.records().take(3).cloned().collect();
    assert_eq!(after, before);
    assert_eq!(register.versions_held(), 4);
    // A restore by an owner names the owner's device.
    let again = register
        .restore_as_of(at(15), at(50), owner_auth(space(1), at(15), 2, 7))
        .expect("restore");
    assert_eq!(again.record.version, version(5));
    assert_eq!(again.record.value, value("a"));
    assert_eq!(again.record.restore_epoch, epoch(2));
    assert_eq!(again.record.writer, Writer::Device(device(7)));
    assert_eq!(again.restored_from, version(1));
}

#[test]
fn restore_as_of_picks_the_newest_version_at_or_before_the_moment() {
    // Stamps 10, 20, 30: a restore as of exactly 20 takes v2; as of 29 also v2; as of 30 takes v3.
    for (as_of, from) in [(20u64, 2u64), (29, 2), (30, 3), (999, 3), (10, 1)] {
        let mut register = three();
        let restored = register
            .restore_as_of(at(as_of), at(100), operator_auth(space(1), at(as_of), 1))
            .expect("restore");
        assert_eq!(restored.restored_from, version(from), "as of {as_of}");
    }
}

#[test]
fn restore_to_the_head_itself_stamps_the_epoch() {
    // After the off-site copy replaced the register, the node restores "as of the snapshot" to stamp
    // the epoch: a restore to a moment at or after the head's own stamp restores the head to itself.
    let mut register = three();
    let restored = register
        .restore_as_of(at(500), at(600), operator_auth(space(1), at(500), 4))
        .expect("restore");
    assert_eq!(restored.restored_from, version(3));
    assert_eq!(restored.record.version, version(4));
    assert_eq!(restored.record.value, value("c"));
    assert_eq!(restored.record.restore_epoch, epoch(4));
}

#[test]
fn restore_as_of_before_the_first_version_is_refused() {
    let mut register = three();
    let before = register.state_hash();
    let refused = register.restore_as_of(at(5), at(40), operator_auth(space(1), at(5), 1));
    assert_eq!(refused, Err(RestoreAsOfRefusal::BeforeFirstVersion));
    assert_eq!(
        register.state_hash(),
        before,
        "a refused restore changes nothing"
    );
    // An empty register has nothing to restore.
    let mut empty = new_register();
    let refused = empty.restore_as_of(at(5), at(40), operator_auth(space(1), at(5), 1));
    assert_eq!(refused, Err(RestoreAsOfRefusal::Empty));
}

#[test]
fn restore_beyond_the_retention_is_told_apart_from_before_the_first_version() {
    let mut register = three();
    let pruned = register.prune(at(1_000), Span::secs(100));
    assert_eq!(pruned.removed.len(), 2, "v1 and v2 are gone");
    let before = register.state_hash();
    // As of 15 would have been v1, which is pruned: not "before the first version".
    let refused = register.restore_as_of(at(15), at(2_000), operator_auth(space(1), at(15), 1));
    assert_eq!(refused, Err(RestoreAsOfRefusal::BeyondRetention));
    assert_eq!(register.state_hash(), before);
}

#[test]
fn after_a_restore_a_cas_on_the_old_expected_version_conflicts() {
    let mut register = three();
    let _ = register
        .restore_as_of(at(15), at(40), operator_auth(space(1), at(15), 1))
        .expect("restore");
    let head = register.get().cloned().expect("head");
    assert_eq!(head.version, version(4));
    // A device that still expects v3 loses and is shown the restored head.
    let outcome = register.cas(
        Some(version(3)),
        value("late"),
        device(2),
        at(50),
        LockTerm::Unlocked,
    );
    assert_eq!(
        outcome,
        CasOutcome::Conflict {
            current: Some(head.clone())
        }
    );
    // A write that expects the restored head lands, in the restored epoch.
    let record = ok(register.cas(
        Some(version(4)),
        value("next"),
        device(2),
        at(60),
        LockTerm::Unlocked,
    ));
    assert_eq!(record.version, version(5));
    assert_eq!(record.restore_epoch, epoch(1));
}

#[test]
fn restore_as_of_needs_the_authorization_witness() {
    // The witness is made by `RestoreAuth::verify` and nothing else (the compile_fail doctests of
    // src/restore.rs show that it cannot be built by hand or copied). A request the verifier refuses
    // gives no witness at all.
    let stranger = signed_request(space(1), at(25), device_key(9));
    let verifier = FakeRequestVerifier { owners: Vec::new() };
    assert!(RestoreAuth::verify(&stranger, epoch(1), &verifier).is_err());
    // A request signed for another moment does not verify for this one.
    let mut tampered = signed_request(space(1), at(25), operator_key());
    tampered.as_of = at(26);
    assert!(RestoreAuth::verify(&tampered, epoch(1), &verifier).is_err());
    // A witness for another space or another moment is refused by the register.
    let mut register = three();
    let before = register.state_hash();
    let other_space = register.restore_as_of(at(25), at(40), operator_auth(space(2), at(25), 1));
    assert_eq!(other_space, Err(RestoreAsOfRefusal::WrongSpace));
    let other_moment = register.restore_as_of(at(25), at(40), operator_auth(space(1), at(15), 1));
    assert_eq!(other_moment, Err(RestoreAsOfRefusal::RequestMismatch));
    assert_eq!(register.state_hash(), before);
    // The witness says who asked.
    let operator = operator_auth(space(1), at(25), 1);
    assert_eq!(operator.requester(), Requester::Operator);
    assert_eq!(operator.epoch(), epoch(1));
    assert_eq!(operator.space(), space(1));
    assert_eq!(operator.as_of(), at(25));
    let owner = owner_auth(space(1), at(25), 1, 7);
    assert_eq!(owner.requester(), Requester::Owner(device(7)));
    // Neither Clone nor Default (a witness is one verified request).
    <RestoreAuth as AmbiguousIfClone<_>>::check();
    <RestoreAuth as AmbiguousIfDefault<_>>::check();
}

#[test]
fn restore_needs_an_epoch_above_the_heads_and_keeps_the_lock() {
    let mut register = new_register();
    fill(&mut register, &["a"]);
    let _ = ok(register.cas(
        Some(version(1)),
        value("b"),
        device(1),
        at(20),
        LockTerm::Until(at(9_000)),
    ));
    // Epoch 0 (not above the head's 0) is refused.
    let before = register.state_hash();
    let refused = register.restore_as_of(at(10), at(30), operator_auth(space(1), at(10), 0));
    assert_eq!(
        refused,
        Err(RestoreAsOfRefusal::EpochNotAbove {
            current: epoch(0),
            got: epoch(0)
        })
    );
    assert_eq!(register.state_hash(), before);
    // Epoch 5 is accepted (the group's counter may be above the register's own), and the restore
    // record keeps the lock of the head it replaces.
    let restored = register
        .restore_as_of(at(10), at(30), operator_auth(space(1), at(10), 5))
        .expect("restore");
    assert_eq!(restored.record.restore_epoch, epoch(5));
    assert_eq!(restored.record.retain_until, Some(at(9_000)));
    // The next restore must be above 5.
    let refused = register.restore_as_of(at(10), at(40), operator_auth(space(1), at(10), 5));
    assert_eq!(
        refused,
        Err(RestoreAsOfRefusal::EpochNotAbove {
            current: epoch(5),
            got: epoch(5)
        })
    );
}

#[test]
fn a_refused_cas_or_restore_at_the_limits_changes_nothing() {
    let limits = HeadLimits::new(3, ByteCount::mib(1)).expect("limits");
    let mut register = HeadRegister::with_limits(space(1), limits);
    fill(&mut register, &["a", "b", "c"]);
    let before = register.state_hash();
    let outcome = register.cas(
        Some(version(3)),
        value("d"),
        device(1),
        at(50),
        LockTerm::Unlocked,
    );
    assert_eq!(outcome, CasOutcome::HistoryFull);
    assert_eq!(register.state_hash(), before);
    let refused = register.restore_as_of(at(10), at(60), operator_auth(space(1), at(10), 1));
    assert_eq!(refused, Err(RestoreAsOfRefusal::HistoryFull));
    assert_eq!(register.state_hash(), before);
    // Dropping a version makes room (the oldest two are past a retention of 5 at now = 100).
    let pruned = register.prune(at(100), Span::secs(5));
    assert_eq!(pruned.removed.len(), 2);
    let record = ok(register.cas(
        Some(version(3)),
        value("d"),
        device(1),
        at(101),
        LockTerm::Unlocked,
    ));
    assert_eq!(record.version, version(4));
    // The sizes the limits are made of.
    assert_eq!(
        HeadLimits::new(1, HEAD_MAX),
        None,
        "bytes below the largest record cannot hold one"
    );
    assert!(HeadLimits::new(1, HEAD_MAX.saturating_add(HEAD_RECORD_OVERHEAD)).is_some());
    assert!(HeadLimits::new(0, ByteCount::mib(1)).is_none());
}
