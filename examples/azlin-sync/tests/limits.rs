//! Check 8 of lane W1-S16a: every structure of the crate has a limit stated up front, and a test that
//! drives each one to its limit and asserts that it stops growing (planning/architecture.md section
//! 1.4 rule 4 "Everything has a limit, stated up front ... At a limit the code refuses; no queue, table
//! or thread count grows without bound", section 2.5 limits, section 5.13).

mod common;

use azlin_seams::sim::Rng;
use azlin_seams::{ByteCount, FakeScheme, Span};
use azlin_sync::{
    validate_subscription, AccountLogQuota, AppendLog, AppendRefusal, CasOutcome, HeadLimits,
    HeadRegister, HeadValue, Level, LockTerm, ReadPage, Version, WatchRefusal, HEAD_MAX,
    HEAD_RECORD_OVERHEAD, HISTORY_PAGE_BYTES, HISTORY_PAGE_MAX, MAX_HISTORY_BYTES, MAX_SPACES,
    MAX_VERSIONS, READ_PAGE_BYTES, READ_PAGE_MAX, RECORD_MAX, RECORD_OVERHEAD,
};
use common::*;

fn write(register: &mut HeadRegister, n: u64, len: usize) -> CasOutcome {
    let expected = register.get().map(|r| r.version);
    register.cas(
        expected,
        HeadValue::new(vec![(n % 251) as u8; len]),
        device(1),
        at(n),
        LockTerm::Unlocked,
    )
}

#[test]
fn memory_is_bounded_by_the_caps() {
    // --- A register stops at its version limit. ---
    let limits = HeadLimits::new(50, ByteCount::mib(1)).expect("limits");
    let mut register = HeadRegister::with_limits(space(1), limits);
    let mut landed = 0;
    for n in 1..=1_000u64 {
        match write(&mut register, n, 20) {
            CasOutcome::Ok { .. } => landed += 1,
            CasOutcome::HistoryFull => {}
            other => panic!("{other:?}"),
        }
        assert!(
            register.versions_held() <= 50,
            "the version limit holds at write {n}"
        );
    }
    assert_eq!(landed, 50);
    assert_eq!(register.versions_held(), 50);
    let held = register.bytes_held();
    for n in 1_001..=2_000u64 {
        assert_eq!(write(&mut register, n, 20), CasOutcome::HistoryFull);
    }
    assert_eq!(
        register.bytes_held(),
        held,
        "a register at its limit stopped growing"
    );
    assert_eq!(register.versions_held(), 50);

    // --- A register stops at its byte limit, with the largest values. ---
    let limits = HeadLimits::new(1_000_000, ByteCount::kib(64)).expect("limits");
    let mut big = HeadRegister::with_limits(space(1), limits);
    let record_size = HEAD_MAX.get() + HEAD_RECORD_OVERHEAD.get();
    let fits = (ByteCount::kib(64).get() / record_size) as usize;
    let mut landed = 0usize;
    for n in 1..=200u64 {
        if matches!(
            write(&mut big, n, HEAD_MAX.get() as usize),
            CasOutcome::Ok { .. }
        ) {
            landed += 1;
        }
        assert!(
            big.bytes_held() <= ByteCount::kib(64),
            "the byte limit holds at write {n}"
        );
    }
    assert_eq!(landed, fits);
    assert_eq!(big.versions_held(), fits);
    // Pruning frees room as versions pass their retention, and the limit holds afterwards too.
    let pruned = big.prune(at(10_000), Span::secs(10));
    assert_eq!(pruned.removed.len(), fits - 1);
    let again = write(&mut big, 10_001, HEAD_MAX.get() as usize);
    assert!(matches!(again, CasOutcome::Ok { .. }));
    assert!(big.bytes_held() <= ByteCount::kib(64));

    // --- The standard limits are the stated ones. ---
    assert_eq!(MAX_VERSIONS, 100_000);
    assert_eq!(MAX_HISTORY_BYTES, ByteCount::mib(64));
    assert_eq!(HeadRegister::new(space(1)).limits(), HeadLimits::standard());
    assert_eq!(HeadLimits::standard().versions(), MAX_VERSIONS);
    assert_eq!(HeadLimits::standard().bytes(), MAX_HISTORY_BYTES);

    // --- A page of history never passes its bounds. ---
    let mut pages = HeadRegister::new(space(1));
    for n in 1..=700u64 {
        assert!(matches!(write(&mut pages, n, 3_000), CasOutcome::Ok { .. }));
    }
    let mut from = Some(Version::FIRST);
    let mut count = 0usize;
    while let Some(start) = from {
        let page = pages.history(start, u16::MAX);
        let bytes: u64 = page.records.iter().map(|r| r.size().get()).sum();
        assert!(page.records.len() <= usize::from(HISTORY_PAGE_MAX));
        assert!(bytes <= HISTORY_PAGE_BYTES.get());
        count += page.records.len();
        from = page.next;
    }
    assert_eq!(count, 700);

    // --- The append log: the record, the account's cap, and a refused append adds nothing. ---
    let who = FakeAccess::open(1, &[1, 2]);
    let id = log_id(1, 1);
    let cap = ByteCount::kib(10);
    let mut q = AccountLogQuota::with_cap(account(1), cap);
    let mut log = AppendLog::new(id, account(1));
    let mut rng = Rng::new(99);
    let mut refused_for_quota = 0u32;
    for n in 0..3_000u64 {
        let len = usize::try_from(rng.below(900).expect("range")).expect("small");
        let record = signed(&id, 1, &rng.bytes(len));
        match log.append(record, at(n), &mut q, &who, &FakeScheme) {
            Ok(_) => {}
            Err(AppendRefusal::QuotaExceeded { .. }) => refused_for_quota += 1,
            Err(other) => panic!("{other:?}"),
        }
        assert!(q.used() <= cap, "the account's cap holds at append {n}");
        assert!(log.uncompacted_bytes() <= cap);
        assert_eq!(log.uncompacted_bytes(), q.used());
    }
    assert!(
        refused_for_quota > 2_000,
        "the cap was reached early and held: {refused_for_quota}"
    );
    // The most records a cap can hold is bounded by the cost of the smallest record, overhead
    // included: nothing is free.
    let smallest = RECORD_OVERHEAD.get() + FakeScheme::SIG_LEN as u64;
    assert!(log.len() as u64 <= cap.get() / smallest);
    // A log at its cap does not grow, whatever is tried: a record whose body is as long as the room
    // that is left costs more than the room (its overhead and signature come on top).
    let hash = (log.state_hash(), q.state_hash());
    let remaining = usize::try_from(cap.get() - q.used().get()).expect("small");
    for n in 0..500u64 {
        let record = signed(&id, 2, &vec![n as u8; remaining]);
        let refused = log.append(record, at(5_000 + n), &mut q, &who, &FakeScheme);
        assert!(
            matches!(refused, Err(AppendRefusal::QuotaExceeded { .. })),
            "{refused:?}"
        );
    }
    assert_eq!(
        (log.state_hash(), q.state_hash()),
        hash,
        "a log at its cap does not grow"
    );
    // The record limit holds on its own, whatever the quota: 64 KiB.
    let mut roomy = AccountLogQuota::new(account(1));
    let mut roomy_log = AppendLog::new(id, account(1));
    let over = signed(&id, 1, &vec![0u8; RECORD_MAX.get() as usize + 1]);
    assert!(matches!(
        roomy_log.append(over, at(1), &mut roomy, &who, &FakeScheme),
        Err(AppendRefusal::TooLarge { .. })
    ));
    assert!(roomy_log.is_empty());
    assert_eq!(roomy.used(), ByteCount::ZERO);

    // --- A page of a read never passes its bounds. ---
    let mut wide = AppendLog::new(id, account(1));
    let mut wide_q = AccountLogQuota::new(account(1));
    for n in 1..=400u64 {
        let record = signed(&id, 1, &vec![n as u8; 5_000]);
        wide.append(record, at(n), &mut wide_q, &who, &FakeScheme)
            .expect("fill");
    }
    let mut from = Some(common::seq(1));
    let mut read = 0usize;
    while let Some(start) = from {
        match wide.read(start, u16::MAX, ByteCount::gib(1)) {
            ReadPage::Records { records, next } => {
                let bytes: u64 = records.iter().map(|r| r.record.size().get()).sum();
                assert!(records.len() <= usize::from(READ_PAGE_MAX));
                assert!(bytes <= READ_PAGE_BYTES.get());
                read += records.len();
                from = next;
            }
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(read, 400);

    // --- A subscription of more than 20,000 spaces is refused; an accepted one never has more. ---
    struct Everyone;
    impl azlin_sync::SpaceAccess for Everyone {
        fn may_head_get(&self, _: azlin_seams::DeviceId, _: azlin_seams::SpaceId) -> bool {
            true
        }
        fn may_append(&self, _: azlin_seams::DeviceId, _: azlin_seams::SpaceId) -> bool {
            true
        }
        fn author_key(&self, _: azlin_seams::DeviceId) -> Option<azlin_seams::VerifyKey> {
            None
        }
    }
    let make = |count: usize| -> Vec<(azlin_seams::SpaceId, Level)> {
        (0..count)
            .map(|i| {
                let mut bytes = [0u8; 16];
                bytes[..8].copy_from_slice(&(i as u64).to_be_bytes());
                (azlin_seams::SpaceId::from_bytes(bytes), Level::Sync)
            })
            .collect()
    };
    let ok = validate_subscription(&make(MAX_SPACES), device(1), &Everyone).expect("at the limit");
    assert!(ok.len() <= MAX_SPACES);
    assert_eq!(
        validate_subscription(&make(MAX_SPACES + 1), device(1), &Everyone),
        Err(WatchRefusal::TooMany {
            got: MAX_SPACES + 1,
            max: MAX_SPACES
        })
    );
}

#[test]
fn the_stated_limits_are_the_plans() {
    // planning/architecture.md section 2.5: the `log_append` record is 64 KiB, the uncompacted
    // append-log bytes per account 1 GiB, the spaces in one `watch` subscription 20,000.
    assert_eq!(RECORD_MAX, ByteCount::kib(64));
    assert_eq!(azlin_sync::LOG_CAP, ByteCount::gib(1));
    assert_eq!(MAX_SPACES, 20_000);
    // The head value: 4 KiB (DRIFT X-DELTA-W1S16A-4).
    assert_eq!(HEAD_MAX, ByteCount::kib(4));
    // The pages: 256 records or 1 MiB (DRIFT X-DELTA-W1S16A-5).
    assert_eq!(HISTORY_PAGE_MAX, 256);
    assert_eq!(READ_PAGE_MAX, 256);
    assert_eq!(HISTORY_PAGE_BYTES, ByteCount::mib(1));
    assert_eq!(READ_PAGE_BYTES, ByteCount::mib(1));
}
