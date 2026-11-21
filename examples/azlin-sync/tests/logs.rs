//! The append logs (check 6 of lane W1-S16a, brief section 6): the server-sequenced records, the 64 KiB
//! limit, the account's cap, compaction behind a pack proof, bounded reads, the author's signature and
//! the determinism of the state.
//!
//! Implements planning/architecture.md section 5.13 (`log_append`, `log_read`: server-sequenced small
//! sealed records, compacted by clients into packs; every record carries `author_device` and a
//! signature; uncompacted bytes are capped per account), section 2.5 (limits) and section 2.16
//! (`logs`).

mod common;

use azlin_seams::sim::Rng;
use azlin_seams::{
    ByteCount, Ed25519Verifier, FakeScheme, SigAlg, SignatureVerifier, VerifyError, VerifyKey,
};
use azlin_sync::{
    record_preimage, AccountLogQuota, AppendLog, AppendRefusal, Appended, CompactNotice,
    CompactRefusal, Freed, LogId, LogResumeRefusal, LoggedRecord, PackCovers, ReadPage, Record,
    Seq, SpaceAccess, LOG_CAP, LOG_RECORD_V1, READ_PAGE_BYTES, READ_PAGE_MAX, RECORD_MAX,
    RECORD_OVERHEAD, SIG_MAX,
};
use common::*;

/// What a record of `body_len` bytes costs with a fake signature (32 bytes).
fn cost(body_len: usize) -> u64 {
    RECORD_OVERHEAD.get() + body_len as u64 + FakeScheme::SIG_LEN as u64
}

fn the_log() -> LogId {
    log_id(1, 1)
}

fn new_log() -> AppendLog {
    AppendLog::new(the_log(), account(1))
}

fn quota() -> AccountLogQuota {
    AccountLogQuota::new(account(1))
}

/// Devices 1 and 2 may append to space 1.
fn access() -> FakeAccess {
    FakeAccess::open(1, &[1, 2])
}

fn add(
    log: &mut AppendLog,
    q: &mut AccountLogQuota,
    who: &FakeAccess,
    device_n: u8,
    body: &[u8],
    at_secs: u64,
) -> Result<Appended, AppendRefusal> {
    let record = signed(&log.id(), device_n, body);
    log.append(record, at(at_secs), q, who, &FakeScheme)
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("hex"))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Check 6
// ---------------------------------------------------------------------------------------------

#[test]
fn append_assigns_consecutive_seq_from_1() {
    let (mut log, mut q, who) = (new_log(), quota(), access());
    assert_eq!(log.last_seq(), None);
    for n in 1..=5u64 {
        let appended = add(
            &mut log,
            &mut q,
            &who,
            1,
            format!("m{n}").as_bytes(),
            100 + n,
        )
        .expect("append");
        assert_eq!(appended.seq, seq(n));
        assert_eq!(appended.seq.get(), n);
    }
    assert_eq!(Seq::FIRST.get(), 1);
    assert_eq!(log.last_seq(), Some(seq(5)));
    assert_eq!(log.len(), 5);
    // The record the log keeps has the sequence number and the leader's stamp, and the author.
    let rows: Vec<&LoggedRecord> = log.records().collect();
    assert_eq!(rows[0].seq, seq(1));
    assert_eq!(rows[0].at, at(101));
    assert_eq!(rows[0].record.author, device(1));
    assert_eq!(rows[4].seq, seq(5));
    // Two devices share the one sequence.
    let appended = add(&mut log, &mut q, &who, 2, b"from two", 200).expect("append");
    assert_eq!(appended.seq, seq(6));
    // The bytes are counted: five records plus one.
    let expected: u64 = (1..=5u64).map(|n| cost(format!("m{n}").len())).sum::<u64>() + cost(8);
    assert_eq!(q.used().get(), expected);
    assert_eq!(log.uncompacted_bytes().get(), expected);
    // A sequence number is never zero.
    assert!(Seq::new(0).is_none());
}

#[test]
fn record_over_64_kib_is_too_large() {
    let (mut log, mut q, who) = (new_log(), quota(), access());
    assert_eq!(RECORD_MAX.get(), 65_536);
    // Exactly 64 KiB is accepted.
    let exact = vec![5u8; 65_536];
    assert!(add(&mut log, &mut q, &who, 1, &exact, 10).is_ok());
    // One byte more is not: nothing changed.
    let before = (log.state_hash(), q.state_hash());
    let big = vec![5u8; 65_537];
    let refused = add(&mut log, &mut q, &who, 1, &big, 11);
    assert_eq!(
        refused,
        Err(AppendRefusal::TooLarge {
            size: ByteCount::new(65_537)
        })
    );
    assert_eq!(
        (log.state_hash(), q.state_hash()),
        before,
        "a refused append changes nothing"
    );
    // A signature over its limit is too large as well.
    let mut record = signed(&log.id(), 1, b"x");
    record.sig = vec![0u8; SIG_MAX.get() as usize + 1];
    let refused = log.append(record, at(12), &mut q, &who, &FakeScheme);
    assert!(
        matches!(refused, Err(AppendRefusal::TooLarge { .. })),
        "{refused:?}"
    );
    assert_eq!((log.state_hash(), q.state_hash()), before);
}

#[test]
fn append_at_the_account_cap_is_quota_exceeded_with_a_compact_notice() {
    let who = access();
    let mut log = new_log();
    // A cap of exactly four records of 100 bytes.
    let cap = ByteCount::new(4 * cost(100));
    let mut q = AccountLogQuota::with_cap(account(1), cap);
    for n in 1..=4u64 {
        add(&mut log, &mut q, &who, 1, &[n as u8; 100], n)
            .expect("a record that fits the cap exactly");
    }
    assert_eq!(q.used(), cap, "the cap is reached exactly");
    let before = (log.state_hash(), q.state_hash());
    let refused = add(&mut log, &mut q, &who, 1, &[9u8; 100], 5);
    assert_eq!(
        refused,
        Err(AppendRefusal::QuotaExceeded {
            notice: CompactNotice {
                account: account(1),
                used: cap,
                cap
            }
        })
    );
    assert_eq!(
        q.notice(),
        CompactNotice {
            account: account(1),
            used: cap,
            cap
        }
    );
    assert_eq!(
        (log.state_hash(), q.state_hash()),
        before,
        "a refused append changes nothing"
    );
    assert_eq!(
        log.last_seq(),
        Some(seq(4)),
        "the refused record took no sequence number"
    );
    // A smaller record does not fit either (no room at all), and the notice is about the account.
    let refused = add(&mut log, &mut q, &who, 2, b"s", 6);
    assert!(matches!(refused, Err(AppendRefusal::QuotaExceeded { .. })));
    // The default cap is 1 GiB.
    assert_eq!(LOG_CAP.get(), 1 << 30);
    assert_eq!(quota().cap(), LOG_CAP);
}

#[test]
fn cap_is_per_account_across_logs_not_per_log() {
    // §5.13 "a bot appending all day": ONE quota for the account, whatever logs it spreads over.
    let who = access();
    let cap = ByteCount::new(3 * cost(100));
    let mut q = AccountLogQuota::with_cap(account(1), cap);
    let mut first = AppendLog::new(log_id(1, 1), account(1));
    let mut second = AppendLog::new(log_id(1, 2), account(1));
    add(&mut first, &mut q, &who, 1, &[1u8; 100], 1).expect("first log");
    add(&mut first, &mut q, &who, 1, &[2u8; 100], 2).expect("first log");
    add(&mut second, &mut q, &who, 1, &[3u8; 100], 3)
        .expect("second log: the third record of the account");
    // The fourth record of the account is refused in EITHER log, although each log alone is below
    // the cap.
    assert!(second.uncompacted_bytes() < cap && first.uncompacted_bytes() < cap);
    let refused = add(&mut second, &mut q, &who, 1, &[4u8; 100], 4);
    assert!(
        matches!(refused, Err(AppendRefusal::QuotaExceeded { .. })),
        "{refused:?}"
    );
    let refused = add(&mut first, &mut q, &who, 1, &[5u8; 100], 5);
    assert!(
        matches!(refused, Err(AppendRefusal::QuotaExceeded { .. })),
        "{refused:?}"
    );
    // Another account has a quota of its own.
    let mut other_q = AccountLogQuota::with_cap(account(2), cap);
    let mut other_log = AppendLog::new(log_id(2, 1), account(2));
    let other_access = FakeAccess::open(2, &[1]);
    add(
        &mut other_log,
        &mut other_q,
        &other_access,
        1,
        &[6u8; 100],
        6,
    )
    .expect("another account");
    // A log wired to another account's quota is refused, not counted.
    let before = (first.state_hash(), q.state_hash(), other_q.state_hash());
    let refused = add(&mut first, &mut other_q, &who, 1, b"x", 7);
    assert_eq!(refused, Err(AppendRefusal::WrongQuota));
    assert_eq!(
        (first.state_hash(), q.state_hash(), other_q.state_hash()),
        before
    );
}

#[test]
fn compaction_frees_quota_and_read_reports_compacted_through() {
    let who = access();
    let mut log = new_log();
    let cap = ByteCount::new(6 * cost(100));
    let mut q = AccountLogQuota::with_cap(account(1), cap);
    for n in 1..=6u64 {
        add(&mut log, &mut q, &who, 1, &[n as u8; 100], n).expect("fill");
    }
    assert!(
        add(&mut log, &mut q, &who, 1, &[7u8; 100], 7).is_err(),
        "full"
    );
    // A pack covers the first four records: compact through 4.
    let proof = PackCovers::from_commit(the_log(), seq(4));
    let freed = log
        .compact_through(seq(4), &proof, &mut q)
        .expect("compact");
    assert_eq!(
        freed,
        Freed {
            records: 4,
            bytes: ByteCount::new(4 * cost(100))
        }
    );
    assert_eq!(
        q.used().get(),
        2 * cost(100),
        "the quota got the bytes back"
    );
    assert_eq!(log.uncompacted_bytes().get(), 2 * cost(100));
    assert_eq!(log.len(), 2);
    assert_eq!(log.compacted_through(), Some(seq(4)));
    assert_eq!(log.last_seq(), Some(seq(6)));
    // Reading inside the compacted prefix says so; reading after it gives the records.
    for from in [1u64, 2, 4] {
        assert_eq!(
            log.read(seq(from), 10, ByteCount::mib(1)),
            ReadPage::Compacted { through: seq(4) },
            "from {from}"
        );
    }
    match log.read(seq(5), 10, ByteCount::mib(1)) {
        ReadPage::Records { records, next } => {
            let seqs: Vec<u64> = records.iter().map(|r| r.seq.get()).collect();
            assert_eq!(seqs, vec![5, 6]);
            assert_eq!(next, None);
        }
        other => panic!("{other:?}"),
    }
    // The room is back: an append fits again and takes the next sequence number.
    let appended = add(&mut log, &mut q, &who, 1, &[7u8; 100], 8).expect("room again");
    assert_eq!(appended.seq, seq(7));
    // A repeated batch changes nothing.
    let hash = (log.state_hash(), q.state_hash());
    let again = log
        .compact_through(seq(4), &proof, &mut q)
        .expect("idempotent");
    assert_eq!(
        again,
        Freed {
            records: 0,
            bytes: ByteCount::ZERO
        }
    );
    let lower = log
        .compact_through(seq(2), &PackCovers::from_commit(the_log(), seq(2)), &mut q)
        .expect("idempotent");
    assert_eq!(
        lower,
        Freed {
            records: 0,
            bytes: ByteCount::ZERO
        }
    );
    assert_eq!((log.state_hash(), q.state_hash()), hash);
    // The whole log can go (through the last record): the log is empty, the quota is back at 0.
    let proof = PackCovers::from_commit(the_log(), seq(7));
    let freed = log.compact_through(seq(7), &proof, &mut q).expect("all");
    assert_eq!(freed.records, 3);
    assert!(log.is_empty());
    assert_eq!(q.used(), ByteCount::ZERO);
    assert_eq!(log.compacted_through(), Some(seq(7)));
    // The sequence goes on from the last, never reused.
    let appended = add(&mut log, &mut q, &who, 1, b"after", 9).expect("append");
    assert_eq!(appended.seq, seq(8));
}

#[test]
fn compaction_needs_a_pack_proof_and_never_drops_beyond_it() {
    // (The compile_fail doctest of src/compact.rs shows that a proof cannot be built by hand; it is
    // not Clone, Default or From.)
    <PackCovers as AmbiguousIfClone<_>>::check();
    <PackCovers as AmbiguousIfDefault<_>>::check();
    let who = access();
    let (mut log, mut q) = (new_log(), quota());
    for n in 1..=6u64 {
        add(&mut log, &mut q, &who, 1, &[n as u8; 20], n).expect("fill");
    }
    let before = (log.state_hash(), q.state_hash());
    // The pack covers through 3; compacting through 4 is beyond it.
    let proof = PackCovers::from_commit(the_log(), seq(3));
    assert_eq!(proof.through(), seq(3));
    assert_eq!(proof.log(), the_log());
    assert_eq!(
        log.compact_through(seq(4), &proof, &mut q),
        Err(CompactRefusal::BeyondProof)
    );
    // A proof for another log of the same space.
    let other_log = PackCovers::from_commit(log_id(1, 2), seq(9));
    assert_eq!(
        log.compact_through(seq(3), &other_log, &mut q),
        Err(CompactRefusal::WrongLog)
    );
    // A proof for another space.
    let other_space = PackCovers::from_commit(log_id(2, 1), seq(9));
    assert_eq!(
        log.compact_through(seq(3), &other_space, &mut q),
        Err(CompactRefusal::WrongLog)
    );
    // Records the log has not got.
    let wide = PackCovers::from_commit(the_log(), seq(9));
    assert_eq!(
        log.compact_through(seq(7), &wide, &mut q),
        Err(CompactRefusal::BeyondEnd)
    );
    // A quota of another account.
    let mut stranger = AccountLogQuota::with_cap(account(2), ByteCount::mib(1));
    assert_eq!(
        log.compact_through(seq(3), &proof, &mut stranger),
        Err(CompactRefusal::WrongQuota)
    );
    // A quota that holds fewer bytes than the log (it is not this log's quota): refused before
    // anything changes.
    let mut light = AccountLogQuota::resume(account(1), ByteCount::mib(1), ByteCount::new(10));
    assert_eq!(
        log.compact_through(seq(3), &proof, &mut light),
        Err(CompactRefusal::QuotaUnderflow)
    );
    assert_eq!(light.used(), ByteCount::new(10));
    // Nothing of that changed anything.
    assert_eq!((log.state_hash(), q.state_hash()), before);
    // A compaction of an empty log has nothing to compact.
    let mut empty = new_log();
    assert_eq!(
        empty.compact_through(seq(1), &PackCovers::from_commit(the_log(), seq(1)), &mut q),
        Err(CompactRefusal::BeyondEnd)
    );
    // With the proof it is exactly the records the pack covers that go.
    let freed = log
        .compact_through(seq(3), &proof, &mut q)
        .expect("within the proof");
    assert_eq!(freed.records, 3);
    assert_eq!(log.compacted_through(), Some(seq(3)));
    assert_eq!(log.len(), 3);
}

#[test]
fn read_is_bounded_by_count_and_bytes_and_resumable() {
    let who = access();
    let (mut log, mut q) = (new_log(), quota());
    for n in 1..=600u64 {
        add(&mut log, &mut q, &who, 1, &[(n % 200) as u8; 10], n).expect("fill");
    }
    // The count: 256 whatever is asked for.
    match log.read(seq(1), 1000, ByteCount::mib(10)) {
        ReadPage::Records { records, next } => {
            assert_eq!(records.len(), usize::from(READ_PAGE_MAX));
            assert_eq!(records[0].seq, seq(1));
            assert_eq!(records[255].seq, seq(256));
            assert_eq!(next, Some(seq(257)));
        }
        other => panic!("{other:?}"),
    }
    // Following `next` reads everything once, in order, in pages of the limit asked for.
    let mut got = Vec::new();
    let mut from = Some(seq(1));
    let mut pages = 0;
    while let Some(start) = from {
        match log.read(start, 100, ByteCount::mib(1)) {
            ReadPage::Records { records, next } => {
                assert!(records.len() <= 100 && !records.is_empty());
                got.extend(records.iter().map(|r| r.seq.get()));
                from = next;
                pages += 1;
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(pages, 6);
    assert_eq!(got, (1..=600u64).collect::<Vec<_>>());
    // Reading past the end: nothing, and nothing to continue from.
    assert_eq!(
        log.read(seq(601), 10, ByteCount::mib(1)),
        ReadPage::Records {
            records: Vec::new(),
            next: None
        }
    );
    // A limit of 0 counts as 1.
    match log.read(seq(10), 0, ByteCount::mib(1)) {
        ReadPage::Records { records, next } => {
            assert_eq!(records.len(), 1);
            assert_eq!(next, Some(seq(11)));
        }
        other => panic!("{other:?}"),
    }

    // The bytes: records of 400 bytes cost 464; 1,000 bytes hold two; a budget below one record still
    // returns one, so that the reader advances.
    let (mut big, mut bq) = (new_log(), quota());
    for n in 1..=10u64 {
        add(&mut big, &mut bq, &who, 1, &[n as u8; 400], n).expect("fill");
    }
    for (budget, expect) in [
        (1_000u64, 2usize),
        (10, 1),
        (464, 1),
        (928, 2),
        (929, 2),
        (1_392, 3),
    ] {
        match big.read(seq(1), 100, ByteCount::new(budget)) {
            ReadPage::Records { records, next } => {
                assert_eq!(records.len(), expect, "budget {budget}");
                assert_eq!(next, Some(seq(expect as u64 + 1)));
            }
            other => panic!("{other:?}"),
        }
    }
    // Whatever budget is asked for, a page never passes READ_PAGE_BYTES (1 MiB) unless one record
    // does: 64 KiB records, 20 of them.
    let (mut huge, mut hq) = (new_log(), quota());
    for n in 1..=20u64 {
        add(&mut huge, &mut hq, &who, 1, &vec![n as u8; 65_536], n).expect("fill");
    }
    match huge.read(seq(1), 256, ByteCount::gib(1)) {
        ReadPage::Records { records, next } => {
            let bytes: u64 = records.iter().map(|r| r.record.size().get()).sum();
            assert!(
                bytes <= READ_PAGE_BYTES.get(),
                "{} records, {bytes} bytes",
                records.len()
            );
            assert_eq!(records.len(), 15, "15 records of 65,632 bytes fit a MiB");
            assert_eq!(next, Some(seq(16)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn author_signature_is_verified() {
    let who = access();
    let (mut log, mut q) = (new_log(), quota());
    add(&mut log, &mut q, &who, 1, b"fine", 1).expect("a signed record");
    let before = (log.state_hash(), q.state_hash());
    let bad = |record: Record| {
        let mut log = log.clone();
        let mut q = q.clone();
        log.append(record, at(9), &mut q, &who, &FakeScheme)
    };
    // The body was changed after signing.
    let mut tampered = signed(&log.id(), 1, b"original");
    tampered.body = b"changed".to_vec();
    assert_eq!(
        bad(tampered),
        Err(AppendRefusal::BadSignature(VerifyError::BadSignature))
    );
    // Signed with another device's key.
    let mut forged = signed(&log.id(), 2, b"body");
    forged.author = device(1);
    assert_eq!(
        bad(forged),
        Err(AppendRefusal::BadSignature(VerifyError::BadSignature))
    );
    // Signed for another log: a record cannot be moved into this one.
    let moved = signed(&log_id(1, 2), 1, b"body");
    assert_eq!(
        bad(moved),
        Err(AppendRefusal::BadSignature(VerifyError::BadSignature))
    );
    let moved_space = signed(&log_id(2, 1), 1, b"body");
    assert_eq!(
        bad(moved_space),
        Err(AppendRefusal::BadSignature(VerifyError::BadSignature))
    );
    // A signature of the wrong length.
    let mut short = signed(&log.id(), 1, b"body");
    short.sig.pop();
    assert_eq!(
        bad(short),
        Err(AppendRefusal::BadSignature(VerifyError::Malformed))
    );
    // No signature at all.
    let mut none = signed(&log.id(), 1, b"body");
    none.sig.clear();
    assert_eq!(
        bad(none),
        Err(AppendRefusal::BadSignature(VerifyError::Malformed))
    );
    // The same body signed properly is accepted (the control).
    let good = signed(&log.id(), 1, b"body");
    assert!(bad(good).is_ok());
    // None of the refusals changed the real log.
    assert_eq!((log.state_hash(), q.state_hash()), before);
}

#[test]
fn author_without_write_access_refused() {
    let mut who = access(); // devices 1 and 2 may append
    who.keys.insert(device(3), device_key(3)); // device 3 has a key but no grant
    who.readers.insert((device(3), space(1))); // it may even read the space
    let (mut log, mut q) = (new_log(), quota());
    let before = (log.state_hash(), q.state_hash());
    // A well-signed record of a device that may not append.
    let refused = add(&mut log, &mut q, &who, 3, b"hello", 1);
    assert_eq!(refused, Err(AppendRefusal::AuthorNotAllowed));
    // A device the registry does not know (no key, no grant).
    let refused = add(&mut log, &mut q, &who, 9, b"hello", 2);
    assert_eq!(refused, Err(AppendRefusal::AuthorNotAllowed));
    // A device with the grant but no usable key.
    who.writers.insert((device(4), space(1)));
    let refused = add(&mut log, &mut q, &who, 4, b"hello", 3);
    assert_eq!(refused, Err(AppendRefusal::AuthorNotAllowed));
    // The grant is checked before the signature: a badly signed record of a non-writer is refused for
    // its author, not for its signature (no word about keys to a stranger).
    let mut bad = signed(&log.id(), 3, b"hello");
    bad.sig = vec![0u8; 32];
    assert_eq!(
        log.append(bad, at(4), &mut q, &who, &FakeScheme),
        Err(AppendRefusal::AuthorNotAllowed)
    );
    // A grant to another space is no grant to this one.
    let mut elsewhere = FakeAccess::open(2, &[1]);
    elsewhere.keys.insert(device(1), device_key(1));
    let refused = add(&mut log, &mut q, &elsewhere, 1, b"hello", 5);
    assert_eq!(refused, Err(AppendRefusal::AuthorNotAllowed));
    assert_eq!(
        (log.state_hash(), q.state_hash()),
        before,
        "nothing was written"
    );
    // The device that may append can.
    assert!(add(&mut log, &mut q, &who, 1, b"hello", 6).is_ok());
}

#[test]
fn append_is_deterministic_across_two_replicas_state_hash() {
    // Two replicas apply the same entries in the same order and agree on the state at every step,
    // refusals included; a replica that diverged in anything a record carries does not.
    let who = access();
    let cap = ByteCount::new(8 * cost(60));
    let mut a = (new_log(), AccountLogQuota::with_cap(account(1), cap));
    let mut b = (new_log(), AccountLogQuota::with_cap(account(1), cap));
    assert_eq!(a.0.state_hash(), b.0.state_hash());
    let mut rng = Rng::new(7);
    let mut applied = 0;
    for step in 0..40u64 {
        let device_n = if rng.chance(1, 2) { 1 } else { 2 };
        let body = rng.bytes(60);
        let at_secs = 100 + step;
        let ra = add(&mut a.0, &mut a.1, &who, device_n, &body, at_secs);
        let rb = add(&mut b.0, &mut b.1, &who, device_n, &body, at_secs);
        assert_eq!(ra, rb, "step {step}");
        assert_eq!(a.0.state_hash(), b.0.state_hash(), "step {step}");
        assert_eq!(a.1.state_hash(), b.1.state_hash(), "step {step}");
        if ra.is_ok() {
            applied += 1;
        }
        if step % 11 == 10 && applied > 2 {
            let through = seq(2);
            let pa = a.0.compact_through(
                through,
                &PackCovers::from_commit(the_log(), through),
                &mut a.1,
            );
            let pb = b.0.compact_through(
                through,
                &PackCovers::from_commit(the_log(), through),
                &mut b.1,
            );
            assert_eq!(pa, pb);
            assert_eq!(a.0.state_hash(), b.0.state_hash());
        }
    }
    assert!(
        applied >= 8,
        "the cap was reached and refusals happened too: {applied} appended"
    );
    // A replica that applied one entry less, or with another stamp, or with another body, differs.
    let mut base = (new_log(), quota());
    add(&mut base.0, &mut base.1, &who, 1, b"one", 10).expect("append");
    let baseline = base.0.state_hash();
    let variants: Vec<(&str, (AppendLog, AccountLogQuota))> = {
        let mut stamp = (new_log(), quota());
        add(&mut stamp.0, &mut stamp.1, &who, 1, b"one", 11).expect("append");
        let mut body = (new_log(), quota());
        add(&mut body.0, &mut body.1, &who, 1, b"two", 10).expect("append");
        let mut author = (new_log(), quota());
        add(&mut author.0, &mut author.1, &who, 2, b"one", 10).expect("append");
        let mut more = (new_log(), quota());
        add(&mut more.0, &mut more.1, &who, 1, b"one", 10).expect("append");
        add(&mut more.0, &mut more.1, &who, 1, b"one", 10).expect("append");
        let none = (new_log(), quota());
        vec![
            ("stamp", stamp),
            ("body", body),
            ("author", author),
            ("one more", more),
            ("none", none),
        ]
    };
    for (what, (log, _)) in &variants {
        assert_ne!(log.state_hash(), baseline, "{what}");
    }
    // The log's own id and account are part of the state.
    let other_name = {
        let mut log = AppendLog::new(log_id(1, 2), account(1));
        let mut q = quota();
        add(&mut log, &mut q, &who, 1, b"one", 10).expect("append");
        log.state_hash()
    };
    assert_ne!(other_name, baseline);
    // The quota's state hash sees the figures.
    assert_ne!(
        AccountLogQuota::with_cap(account(1), ByteCount::new(1)).state_hash(),
        quota().state_hash()
    );
    assert_ne!(
        AccountLogQuota::with_cap(account(2), LOG_CAP).state_hash(),
        quota().state_hash()
    );
    assert_ne!(base.1.state_hash(), quota().state_hash());
}

// ---------------------------------------------------------------------------------------------
// The signature: its preimage, its domain, and the real verifier
// ---------------------------------------------------------------------------------------------

#[test]
fn the_record_preimage_layout_and_domain_are_pinned() {
    assert_eq!(LOG_RECORD_V1.as_str(), "azlin-log-record-v1");
    assert!(LOG_RECORD_V1.is_wellformed());
    let log = LogId {
        space: space(1),
        name: [2; 16],
    };
    let preimage = record_preimage(&log, device(3), b"hello log");
    // domain || 0x00 || space || name || author || body
    let expected = hex("617a6c696e2d6c6f672d7265636f72642d76310001010101010101010101010101010101020202020202020202020202020202020303030303030303030303030303030368656c6c6f206c6f67");
    assert_eq!(preimage, expected);
    // The three ids and the body are all in it: a change of any one is another preimage.
    let base = record_preimage(&log, device(3), b"hello log");
    assert_ne!(
        base,
        record_preimage(
            &LogId {
                space: space(9),
                name: [2; 16]
            },
            device(3),
            b"hello log"
        )
    );
    assert_ne!(
        base,
        record_preimage(
            &LogId {
                space: space(1),
                name: [9; 16]
            },
            device(3),
            b"hello log"
        )
    );
    assert_ne!(base, record_preimage(&log, device(4), b"hello log"));
    assert_ne!(base, record_preimage(&log, device(3), b"hello lop"));
}

#[test]
fn a_record_signed_with_a_real_ed25519_key_is_appended_by_the_real_verifier() {
    // The vector was made by an independent pure-Python RFC 8032 implementation over the preimage
    // above: secret 07 x 32, author 03 x 16, space 01 x 16, log name 02 x 16, body "hello log".
    let public = hex("ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c");
    let sig = hex("7f162e8378b0ceee9660451c9d7d680c7ef366f66cf3fc5916be5da2c97b5a798193f8a939733b458cd3025a37de7e92bd62a2f8fe135ef7ff614be97eb38b0f");
    let log = LogId {
        space: space(1),
        name: [2; 16],
    };
    let preimage = record_preimage(&log, device(3), b"hello log");
    assert_eq!(
        Ed25519Verifier.verify(SigAlg::Ed25519, &public, &preimage, &sig),
        Ok(())
    );
    // A real registry view: device 3 may append and its key is the public key above.
    let mut who = FakeAccess::default();
    who.writers.insert((device(3), space(1)));
    struct RealKeys {
        inner: FakeAccess,
        key: [u8; 32],
    }
    impl SpaceAccess for RealKeys {
        fn may_head_get(&self, d: azlin_seams::DeviceId, s: azlin_seams::SpaceId) -> bool {
            self.inner.may_head_get(d, s)
        }
        fn may_append(&self, d: azlin_seams::DeviceId, s: azlin_seams::SpaceId) -> bool {
            self.inner.may_append(d, s)
        }
        fn author_key(&self, d: azlin_seams::DeviceId) -> Option<VerifyKey> {
            (d == device(3)).then(|| VerifyKey::from_bytes(self.key))
        }
    }
    let key: [u8; 32] = public.clone().try_into().expect("32 bytes");
    let registry = RealKeys { inner: who, key };
    let mut appended_log = AppendLog::new(log, account(1));
    let mut q = quota();
    let record = Record {
        author: device(3),
        body: b"hello log".to_vec(),
        sig: sig.clone(),
    };
    let appended = appended_log
        .append(record, at(5), &mut q, &registry, &Ed25519Verifier)
        .expect("a record with a real signature");
    assert_eq!(appended.seq, seq(1));
    // The same signature over another body does not verify.
    let forged = Record {
        author: device(3),
        body: b"hello lop".to_vec(),
        sig,
    };
    assert_eq!(
        appended_log.append(forged, at(6), &mut q, &registry, &Ed25519Verifier),
        Err(AppendRefusal::BadSignature(VerifyError::BadSignature))
    );
}

// ---------------------------------------------------------------------------------------------
// Persistence: a log and a quota rebuilt from their rows
// ---------------------------------------------------------------------------------------------

#[test]
fn a_log_resumes_from_its_rows_and_refuses_rows_that_do_not_hold_together() {
    let who = access();
    let (mut log, mut q) = (new_log(), quota());
    for n in 1..=5u64 {
        add(&mut log, &mut q, &who, 1, &[n as u8; 30], n).expect("fill");
    }
    let rows: Vec<LoggedRecord> = log.records().cloned().collect();
    let again =
        AppendLog::resume(the_log(), account(1), None, rows.clone()).expect("rows of a real log");
    assert_eq!(again.state_hash(), log.state_hash());
    assert_eq!(again, log);
    // After a compaction: the rows start after the compacted prefix.
    let proof = PackCovers::from_commit(the_log(), seq(2));
    log.compact_through(seq(2), &proof, &mut q)
        .expect("compact");
    let rows: Vec<LoggedRecord> = log.records().cloned().collect();
    let again = AppendLog::resume(the_log(), account(1), Some(seq(2)), rows.clone()).expect("rows");
    assert_eq!(again.state_hash(), log.state_hash());
    assert_eq!(again.last_seq(), Some(seq(5)));
    // A fully compacted log keeps its last sequence number through the compacted mark.
    let empty = AppendLog::resume(the_log(), account(1), Some(seq(5)), Vec::new()).expect("empty");
    assert_eq!(empty.last_seq(), Some(seq(5)));
    assert!(empty.is_empty());
    // Rows that do not start where the compaction ended, with a gap, or out of order.
    assert_eq!(
        AppendLog::resume(the_log(), account(1), None, rows.clone()),
        Err(LogResumeRefusal::NotConsecutive { seq: seq(3) })
    );
    let mut gap = rows.clone();
    gap.remove(1);
    assert_eq!(
        AppendLog::resume(the_log(), account(1), Some(seq(2)), gap),
        Err(LogResumeRefusal::NotConsecutive { seq: seq(5) })
    );
    let mut swapped = rows.clone();
    swapped.swap(0, 1);
    assert!(AppendLog::resume(the_log(), account(1), Some(seq(2)), swapped).is_err());
    // A body over its limit.
    let mut huge = rows;
    huge[0].record.body = vec![0u8; 65_537];
    assert_eq!(
        AppendLog::resume(the_log(), account(1), Some(seq(2)), huge),
        Err(LogResumeRefusal::TooLarge { seq: seq(3) })
    );
    // The quota resumes from its figures; a `used` above a lowered cap is allowed and refuses appends.
    let low = AccountLogQuota::resume(account(1), ByteCount::new(10), ByteCount::new(500));
    assert_eq!(low.used(), ByteCount::new(500));
    let mut low = low;
    let mut log = new_log();
    let refused = add(&mut log, &mut low, &who, 1, b"x", 1);
    assert!(matches!(refused, Err(AppendRefusal::QuotaExceeded { .. })));
}

// ---------------------------------------------------------------------------------------------
// The property: a log against a model, over seeded schedules
// ---------------------------------------------------------------------------------------------

/// The model of one log: the test's own, written independently of the crate.
#[derive(Default)]
struct Model {
    /// (sequence number, body, stamp) of the uncompacted records.
    records: Vec<(u64, Vec<u8>, u64)>,
    last: u64,
    compacted: u64,
}

impl Model {
    fn bytes(&self) -> u64 {
        self.records
            .iter()
            .map(|(_, body, _)| cost(body.len()))
            .sum()
    }
}

/// What a read must answer, from the model.
fn model_read(
    model: &Model,
    from: u64,
    limit: u16,
    max_bytes: u64,
) -> Option<(Vec<u64>, Option<u64>)> {
    if model.compacted > 0 && from <= model.compacted {
        return None;
    }
    let count = usize::from(limit.clamp(1, 256));
    let budget = max_bytes.min(1 << 20);
    let mut taken = Vec::new();
    let mut spent = 0u64;
    let mut next = None;
    for (seq_n, body, _) in model.records.iter().filter(|(s, _, _)| *s >= from) {
        let size = cost(body.len());
        if taken.len() >= count || (!taken.is_empty() && spent + size > budget) {
            next = Some(*seq_n);
            break;
        }
        spent += size;
        taken.push(*seq_n);
    }
    Some((taken, next))
}

fn schedule(seed: u64) {
    let mut rng = Rng::new(seed);
    let cap = 500 + rng.below(2_500).expect("range");
    let mut q = AccountLogQuota::with_cap(account(1), ByteCount::new(cap));
    let mut logs = [
        AppendLog::new(log_id(1, 1), account(1)),
        AppendLog::new(log_id(1, 2), account(1)),
    ];
    let mut models = [Model::default(), Model::default()];
    let who = access();
    let mut now = 100u64;
    for step in 0..40u32 {
        now += 1 + rng.below(5).expect("range");
        let l = usize::try_from(rng.below(2).expect("range")).expect("small");
        let id = logs[l].id();
        match rng.below(10).expect("range") {
            // An append: a valid one, one by a device with no grant, or one with a damaged body.
            0..=5 => {
                let body_len = usize::try_from(rng.below(300).expect("range")).expect("small");
                let body = rng.bytes(body_len);
                let kind = rng.below(10).expect("range");
                let size = cost(body_len);
                let model_used: u64 = models.iter().map(Model::bytes).sum();
                let (record, expect): (Record, Result<(), &str>) = match kind {
                    0 => (signed(&id, 3, &body), Err("not allowed")),
                    1 => {
                        let mut record = signed(&id, 1, &body);
                        record.body.push(0);
                        (record, Err("bad signature"))
                    }
                    _ => {
                        let device_n = if rng.chance(1, 2) { 1 } else { 2 };
                        let verdict = if model_used + size > cap {
                            Err("quota")
                        } else {
                            Ok(())
                        };
                        (signed(&id, device_n, &body), verdict)
                    }
                };
                let got = logs[l].append(record, at(now), &mut q, &who, &FakeScheme);
                match (expect, got) {
                    (Ok(()), Ok(appended)) => {
                        let model = &mut models[l];
                        model.last += 1;
                        assert_eq!(appended.seq.get(), model.last, "seed {seed} step {step}");
                        model.records.push((model.last, body, now));
                    }
                    (Err("not allowed"), Err(AppendRefusal::AuthorNotAllowed)) => {}
                    (Err("bad signature"), Err(AppendRefusal::BadSignature(_))) => {}
                    (Err("quota"), Err(AppendRefusal::QuotaExceeded { notice })) => {
                        assert_eq!(notice.used.get(), model_used, "seed {seed} step {step}");
                        assert_eq!(notice.cap.get(), cap);
                    }
                    (expect, got) => {
                        panic!("seed {seed} step {step}: expected {expect:?}, got {got:?}")
                    }
                }
            }
            // A read.
            6 | 7 => {
                let from = 1 + rng.below(models[l].last + 2).expect("range");
                let limit = u16::try_from(rng.below(300).expect("range")).expect("small");
                let max_bytes = rng.below(3_000).expect("range");
                let got = logs[l].read(seq(from), limit, ByteCount::new(max_bytes));
                match (model_read(&models[l], from, limit, max_bytes), got) {
                    (None, ReadPage::Compacted { through }) => {
                        assert_eq!(
                            through.get(),
                            models[l].compacted,
                            "seed {seed} step {step}"
                        );
                    }
                    (
                        Some((seqs, next)),
                        ReadPage::Records {
                            records,
                            next: got_next,
                        },
                    ) => {
                        let got_seqs: Vec<u64> = records.iter().map(|r| r.seq.get()).collect();
                        assert_eq!(got_seqs, seqs, "seed {seed} step {step}: read from {from}");
                        assert_eq!(got_next.map(Seq::get), next, "seed {seed} step {step}");
                        for record in &records {
                            let modelled = models[l]
                                .records
                                .iter()
                                .find(|(s, _, _)| *s == record.seq.get())
                                .expect("a record the model has");
                            assert_eq!(record.record.body, modelled.1);
                            assert_eq!(record.at.as_secs(), modelled.2);
                        }
                    }
                    (expect, got) => panic!(
                        "seed {seed} step {step}: read from {from}: {expect:?} against {got:?}"
                    ),
                }
            }
            // A compaction behind a pack proof.
            _ => {
                if models[l].last == 0 {
                    continue;
                }
                let through = 1 + rng.below(models[l].last).expect("range");
                let proof_through = through + rng.below(3).expect("range");
                let before_used = q.used().get();
                let got = logs[l].compact_through(
                    seq(through),
                    &PackCovers::from_commit(id, seq(proof_through)),
                    &mut q,
                );
                let model = &mut models[l];
                if through <= model.compacted {
                    assert_eq!(
                        got,
                        Ok(Freed {
                            records: 0,
                            bytes: ByteCount::ZERO
                        }),
                        "seed {seed} step {step}"
                    );
                } else {
                    let dropped: Vec<&(u64, Vec<u8>, u64)> = model
                        .records
                        .iter()
                        .filter(|(s, _, _)| *s <= through)
                        .collect();
                    let freed: u64 = dropped.iter().map(|(_, body, _)| cost(body.len())).sum();
                    let count = dropped.len() as u64;
                    assert_eq!(
                        got,
                        Ok(Freed {
                            records: count,
                            bytes: ByteCount::new(freed)
                        }),
                        "seed {seed} step {step}"
                    );
                    model.records.retain(|(s, _, _)| *s > through);
                    model.compacted = through;
                    assert_eq!(q.used().get(), before_used - freed);
                }
            }
        }
        // After every step: the quota is exactly the bytes the two logs hold, and each log holds
        // exactly what its model holds.
        let model_used: u64 = models.iter().map(Model::bytes).sum();
        assert_eq!(
            q.used().get(),
            model_used,
            "seed {seed} step {step}: the quota"
        );
        assert!(
            q.used().get() <= cap,
            "seed {seed} step {step}: the cap holds"
        );
        for (log, model) in logs.iter().zip(&models) {
            assert_eq!(
                log.uncompacted_bytes().get(),
                model.bytes(),
                "seed {seed} step {step}"
            );
            assert_eq!(log.len(), model.records.len());
        }
    }
    // The rows round-trip: a log rebuilt from them is the same state.
    for (log, model) in logs.iter().zip(&models) {
        let rows: Vec<LoggedRecord> = log.records().cloned().collect();
        let compacted = Seq::new(model.compacted);
        let again =
            AppendLog::resume(log.id(), account(1), compacted, rows).expect("rows round-trip");
        assert_eq!(again.state_hash(), log.state_hash(), "seed {seed}");
        assert_eq!(
            again.last_seq().map(Seq::get).unwrap_or(0),
            model.last.max(model.compacted)
        );
    }
}

#[test]
fn logs_match_a_model_over_seeded_schedules() {
    for seed in 0..2_000u64 {
        schedule(seed);
    }
}
