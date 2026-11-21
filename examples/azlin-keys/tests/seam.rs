//! C3 (the entropy seam), C4 (the clock seam and the time units) and the space chain view.
//!
//! Most checks use the test doubles (`SeededEntropy`, `FailingEntropy`, `ManualClock`,
//! `MapChainView`), which exist under the feature `test-support`: run this file with
//! `--features test-support` (the lane's hold does).
//!
//! Implements planning/architecture.md section 1.4 rule 1 ("I/O is a parameter") and section 5.5
//! (authorized epochs: the chain view).

#![cfg_attr(not(feature = "test-support"), allow(unused_imports, dead_code))]

mod common;

use azlin_keys::{
    ChainError, ChainSeq, Clock, ClockError, CommitKind, Commitment, DeviceId, Entropy,
    EntropyError, Epoch, KeyId, SpaceChainView, SpaceId, Span, SystemClockSeam, UnixTime,
};
#[cfg(feature = "test-support")]
use azlin_keys::{FailingEntropy, ManualClock, MapChainView, SeededEntropy};

use common::{to_hex, SplitMix64};

/// Negative-trait checks (see tests/secret.rs): compile only while `T` has no `From<u64>`.
trait AmbiguousIfFromU64<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfFromU64<()> for T {}
impl<T: From<u64>> AmbiguousIfFromU64<u8> for T {}

trait AmbiguousIfFromU32<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfFromU32<()> for T {}
impl<T: From<u32>> AmbiguousIfFromU32<u8> for T {}

// ---------------------------------------------------------------------------------------------
// C3: the entropy seam
// ---------------------------------------------------------------------------------------------

/// Draws `total` bytes from `ent` in pieces whose sizes come from `rng`.
fn draw_in_pieces(ent: &mut dyn Entropy, total: usize, rng: &mut SplitMix64) -> Vec<u8> {
    let mut out = Vec::new();
    while out.len() < total {
        let piece = (1 + rng.below(40) as usize).min(total - out.len());
        let mut buf = vec![0u8; piece];
        ent.fill(&mut buf).expect("a seeded source does not fail");
        out.extend_from_slice(&buf);
    }
    out
}

#[cfg(feature = "test-support")]
#[test]
fn c3_entropy_seam() {
    // Same seed, same stream: however the draws are cut.
    let mut whole = SeededEntropy::from_u64(7);
    let mut all = vec![0u8; 500];
    whole.fill(&mut all).unwrap();
    for cut_seed in 0..20u64 {
        let mut pieces = SeededEntropy::from_u64(7);
        let mut rng = SplitMix64::new(cut_seed);
        assert_eq!(
            draw_in_pieces(&mut pieces, 500, &mut rng),
            all,
            "cut seed {cut_seed}"
        );
        assert_eq!(pieces.drawn(), 500);
    }

    // Different seeds, different streams (u64 seeds, 32-byte seeds, a seed that differs in its
    // last byte only).
    let draw32 = |mut e: SeededEntropy| {
        let mut b = [0u8; 32];
        e.fill(&mut b).unwrap();
        b
    };
    assert_ne!(
        draw32(SeededEntropy::from_u64(0)),
        draw32(SeededEntropy::from_u64(1))
    );
    assert_ne!(
        draw32(SeededEntropy::new([1u8; 32])),
        draw32(SeededEntropy::new([2u8; 32]))
    );
    let mut last_byte = [9u8; 32];
    let base = draw32(SeededEntropy::new(last_byte));
    last_byte[31] ^= 1;
    assert_ne!(draw32(SeededEntropy::new(last_byte)), base);
    assert_eq!(
        draw32(SeededEntropy::from_u64(0)),
        draw32(SeededEntropy::new([0u8; 32]))
    );
    assert_ne!(base, [0u8; 32], "a stream is not zeroes");

    // It counts the bytes drawn (empty draws count nothing).
    let mut counted = SeededEntropy::from_u64(3);
    assert_eq!(counted.drawn(), 0);
    counted.fill(&mut []).unwrap();
    assert_eq!(counted.drawn(), 0);
    counted.fill(&mut [0u8; 1]).unwrap();
    counted.fill(&mut [0u8; 31]).unwrap();
    counted.fill(&mut [0u8; 100]).unwrap();
    assert_eq!(counted.drawn(), 132);
    assert_eq!(
        format!("{counted:?}"),
        "SeededEntropy { drawn: 132 }",
        "Debug names the count only"
    );

    // It is object safe, as every generator takes `&mut dyn Entropy`.
    fn through_dyn(e: &mut dyn Entropy) -> [u8; 8] {
        let mut b = [0u8; 8];
        e.fill(&mut b).unwrap();
        b
    }
    let mut boxed: Box<dyn Entropy> = Box::new(SeededEntropy::from_u64(1));
    assert_ne!(through_dyn(boxed.as_mut()), [0u8; 8]);
}

#[cfg(feature = "test-support")]
#[test]
fn c3_failing_entropy_fails_and_keeps_failing() {
    // Serves exactly `n` bytes, then fails; the failing draw takes nothing.
    let mut e = FailingEntropy::after(10);
    let mut first = [0u8; 4];
    e.fill(&mut first).unwrap();
    let mut second = [0u8; 6];
    e.fill(&mut second).unwrap();
    assert_ne!(
        [first.as_slice(), second.as_slice()].concat(),
        vec![0u8; 10],
        "good bytes are not zeroes"
    );
    let mut one_more = [0xEEu8; 1];
    assert_eq!(e.fill(&mut one_more), Err(EntropyError::Exhausted));

    // After a failure every draw fails, whatever its size, empty ones included.
    assert_eq!(e.fill(&mut []), Err(EntropyError::Exhausted));
    assert_eq!(e.fill(&mut [0u8; 1]), Err(EntropyError::Exhausted));
    assert_eq!(e.fill(&mut [0u8; 32]), Err(EntropyError::Exhausted));

    // A draw that does not fit fails whole, takes nothing, and the source stays failed even for
    // a smaller draw that would have fitted.
    let mut e = FailingEntropy::after(10);
    assert_eq!(e.fill(&mut [0u8; 11]), Err(EntropyError::Exhausted));
    assert_eq!(e.fill(&mut [0u8; 1]), Err(EntropyError::Exhausted));

    // after(0) fails at once; an empty draw before the failure is fine.
    let mut none = FailingEntropy::after(0);
    assert_eq!(none.fill(&mut [0u8; 1]), Err(EntropyError::Exhausted));
    let mut empty_ok = FailingEntropy::after(5);
    assert_eq!(empty_ok.fill(&mut []), Ok(()));

    // The error is typed and printable without data.
    assert_eq!(
        EntropyError::Exhausted.to_string(),
        "the random source has no more bytes"
    );
    assert_ne!(EntropyError::Os, EntropyError::Exhausted);
}

#[cfg(feature = "os-rng")]
#[test]
fn c3_os_entropy_fills_non_trivially() {
    use azlin_keys::OsEntropy;
    let mut os = OsEntropy;
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    os.fill(&mut a).unwrap();
    os.fill(&mut b).unwrap();
    assert_ne!(a, b, "two draws differ");
    assert_ne!(a, [0u8; 32], "a draw is not zeroes");
    assert_ne!(b, [0u8; 32]);
    let mut big = vec![0u8; 1 << 16];
    os.fill(&mut big).unwrap();
    assert!(big.iter().any(|x| *x != 0));
    os.fill(&mut []).unwrap();
}

/// The stream of `SeededEntropy` is pinned: the known-answer vectors of the other lanes are made
/// with it. `tests/vectors/seeded_entropy.json` is written with `UPDATE_VECTORS=1` (never when
/// `CI` is set).
#[cfg(feature = "test-support")]
#[test]
fn c3_seeded_stream_is_pinned() {
    let mut rows = Vec::new();
    let seeds: [(&str, [u8; 32]); 4] = [
        ("u64_0", {
            let mut s = [0u8; 32];
            s[..8].copy_from_slice(&0u64.to_le_bytes());
            s
        }),
        ("u64_1", {
            let mut s = [0u8; 32];
            s[..8].copy_from_slice(&1u64.to_le_bytes());
            s
        }),
        ("u64_deadbeef", {
            let mut s = [0u8; 32];
            s[..8].copy_from_slice(&0xDEAD_BEEFu64.to_le_bytes());
            s
        }),
        ("seed_7s", [7u8; 32]),
    ];
    for (name, seed) in seeds {
        // Drawn as 32 + 32 bytes: a cut does not change the stream.
        let mut e = SeededEntropy::new(seed);
        let mut stream = [0u8; 64];
        let (head, tail) = stream.split_at_mut(32);
        e.fill(head).unwrap();
        e.fill(tail).unwrap();
        rows.push(common::Row::new(
            &format!("seeded_{name}"),
            "seeded_entropy",
            "",
            &to_hex(&seed),
            "",
            &to_hex(&stream),
        ));
    }
    // `from_u64` is the seed with the little-endian bytes in front.
    let mut by_u64 = SeededEntropy::from_u64(0xDEAD_BEEF);
    let mut got = [0u8; 64];
    by_u64.fill(&mut got).unwrap();
    assert_eq!(
        to_hex(&got),
        rows.iter()
            .find(|r| r.name == "seeded_u64_deadbeef")
            .unwrap()
            .expect
    );
    common::check_or_update("seeded_entropy.json", &rows);
}

// ---------------------------------------------------------------------------------------------
// C4: the clock seam and the units
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "test-support")]
#[test]
fn c4_clock_seam_and_units() {
    // A manual clock reads what it was set to, moves forward, jumps back and fails on demand.
    let clock = ManualClock::new(UnixTime::from_secs(1_000));
    assert_eq!(clock.now(), Ok(UnixTime::from_secs(1_000)));
    assert_eq!(
        clock.advance(Span::minutes(11)),
        Some(UnixTime::from_secs(1_660))
    );
    assert_eq!(clock.now(), Ok(UnixTime::from_secs(1_660)));
    assert_eq!(
        clock.jump_back(Span::secs(600)),
        Some(UnixTime::from_secs(1_060))
    );
    assert_eq!(clock.now(), Ok(UnixTime::from_secs(1_060)));
    clock.set(UnixTime::from_secs(5));
    assert_eq!(clock.now(), Ok(UnixTime::from_secs(5)));

    // Jumping back past the epoch and advancing past the end are refused and change nothing.
    assert_eq!(clock.jump_back(Span::secs(6)), None);
    assert_eq!(clock.now(), Ok(UnixTime::from_secs(5)));
    clock.set(UnixTime::from_secs(u64::MAX - 1));
    assert_eq!(clock.advance(Span::secs(2)), None);
    assert_eq!(clock.now(), Ok(UnixTime::from_secs(u64::MAX - 1)));
    assert_eq!(
        clock.advance(Span::secs(1)),
        Some(UnixTime::from_secs(u64::MAX))
    );

    // A failing clock is an error, never the time 0, and recovers when the failure ends.
    clock.set(UnixTime::from_secs(42));
    clock.fail(true);
    assert_eq!(clock.now(), Err(ClockError::Unreadable));
    assert_eq!(clock.now(), Err(ClockError::Unreadable));
    clock.fail(false);
    assert_eq!(clock.now(), Ok(UnixTime::from_secs(42)));

    // It is object safe and shared by reference: the code under test holds `&dyn Clock` while the
    // test moves the clock.
    fn read(c: &dyn Clock) -> Result<UnixTime, ClockError> {
        c.now()
    }
    let before = read(&clock).unwrap();
    clock.advance(Span::hours(1)).unwrap();
    assert_eq!(
        read(&clock).unwrap().elapsed_since(before),
        Some(Span::hours(1))
    );
}

#[test]
fn c4_system_clock_seam_passes_the_callers_answer_through() {
    let ok = SystemClockSeam::new(|| Ok(UnixTime::from_secs(7)));
    assert_eq!(ok.now(), Ok(UnixTime::from_secs(7)));
    let unreadable = SystemClockSeam::new(|| Err(ClockError::Unreadable));
    assert_eq!(unreadable.now(), Err(ClockError::Unreadable));
    let before_epoch = SystemClockSeam::new(|| Err(ClockError::BeforeEpoch));
    assert_eq!(before_epoch.now(), Err(ClockError::BeforeEpoch));
    let as_dyn: &dyn Clock = &ok;
    assert_eq!(as_dyn.now(), Ok(UnixTime::from_secs(7)));
    assert_ne!(
        ClockError::Unreadable.to_string(),
        ClockError::BeforeEpoch.to_string()
    );
}

#[test]
fn c4_units_are_checked_and_typed() {
    // Span constructors: seconds, minutes, hours, days.
    assert_eq!(Span::days(1).as_secs(), 86_400);
    assert_eq!(Span::hours(1).as_secs(), 3_600);
    assert_eq!(Span::minutes(1).as_secs(), 60);
    assert_eq!(Span::secs(90).as_secs(), 90);
    assert_eq!(Span::days(1), Span::hours(24));
    assert_eq!(Span::hours(1), Span::minutes(60));
    assert_eq!(Span::minutes(10).as_secs(), 600);
    assert_eq!(Span::ZERO.as_secs(), 0);
    // The largest constructors cannot overflow.
    assert_eq!(Span::days(u32::MAX).as_secs(), u64::from(u32::MAX) * 86_400);
    assert_eq!(Span::hours(u32::MAX).as_secs(), u64::from(u32::MAX) * 3_600);

    // Ordering.
    assert!(Span::minutes(59) < Span::hours(1));
    assert!(Span::days(1) > Span::hours(23));
    assert!(UnixTime::from_secs(1) < UnixTime::from_secs(2));
    assert!(UnixTime::EPOCH < UnixTime::from_secs(1));

    // Time arithmetic is checked.
    let t = UnixTime::from_secs(1_800_000_000);
    assert_eq!(
        t.checked_add(Span::days(1)),
        Some(UnixTime::from_secs(1_800_086_400))
    );
    assert_eq!(
        t.checked_sub(Span::days(1)),
        Some(UnixTime::from_secs(1_799_913_600))
    );
    assert_eq!(
        UnixTime::from_secs(u64::MAX).checked_add(Span::secs(1)),
        None
    );
    assert_eq!(
        UnixTime::from_secs(u64::MAX).checked_add(Span::ZERO),
        Some(UnixTime::from_secs(u64::MAX))
    );
    assert_eq!(UnixTime::EPOCH.checked_sub(Span::secs(1)), None);
    assert_eq!(
        UnixTime::from_secs(10).checked_sub(Span::secs(10)),
        Some(UnixTime::EPOCH)
    );
    assert_eq!(
        t.elapsed_since(UnixTime::from_secs(1_799_999_990)),
        Some(Span::secs(10))
    );
    assert_eq!(
        UnixTime::from_secs(5).elapsed_since(UnixTime::from_secs(6)),
        None
    );
    assert_eq!(t.elapsed_since(t), Some(Span::ZERO));

    // Span arithmetic is checked.
    assert_eq!(Span::secs(u64::MAX).checked_add(Span::secs(1)), None);
    assert_eq!(
        Span::secs(1).checked_add(Span::secs(2)),
        Some(Span::secs(3))
    );
    assert_eq!(Span::secs(1).checked_sub(Span::secs(2)), None);
    assert_eq!(
        Span::secs(5).checked_sub(Span::secs(2)),
        Some(Span::secs(3))
    );
    assert_eq!(Span::days(1).checked_mul(7), Some(Span::days(7)));
    assert_eq!(Span::secs(u64::MAX).checked_mul(2), None);
    assert_eq!(
        Span::secs(u64::MAX).checked_mul(1),
        Some(Span::secs(u64::MAX))
    );
    assert_eq!(Span::days(u32::MAX).checked_mul(u32::MAX), None);

    // No implicit conversion from a bare number: neither type implements From<u64> or From<u32>.
    <UnixTime as AmbiguousIfFromU64<_>>::check();
    <Span as AmbiguousIfFromU64<_>>::check();
    <UnixTime as AmbiguousIfFromU32<_>>::check();
    <Span as AmbiguousIfFromU32<_>>::check();
}

// ---------------------------------------------------------------------------------------------
// The space chain view
// ---------------------------------------------------------------------------------------------

fn commitment(space: SpaceId, epoch: u32, seq: u64, kind: CommitKind) -> Commitment {
    Commitment {
        space,
        epoch: Epoch::new(epoch).unwrap(),
        key_id: KeyId::from_bytes([epoch as u8; 32]),
        signer: DeviceId::from_bytes([0xD0; 16]),
        seq: ChainSeq::new(seq),
        kind,
    }
}

#[cfg(feature = "test-support")]
#[test]
fn c4_space_chain_view_is_object_safe_and_tells_none_from_unreadable() {
    let space = SpaceId::from_bytes([1; 16]);
    let other_space = SpaceId::from_bytes([2; 16]);
    let device = DeviceId::from_bytes([0xD0; 16]);
    let stranger = DeviceId::from_bytes([0xD1; 16]);

    let mut chain = MapChainView::new();
    let created = commitment(space, 1, 0, CommitKind::Create);
    let rekeyed = commitment(space, 2, 9, CommitKind::Rekey);
    chain.commit(created);
    chain.commit(rekeyed);
    chain.authorize(space, device, ChainSeq::new(0), Some(ChainSeq::new(20)));

    {
        let view: &dyn SpaceChainView = &chain; // object safe
                                                // A commitment that exists, one that does not (Ok(None)), and another space's.
        assert_eq!(view.commitment(&space, Epoch::FIRST), Ok(Some(created)));
        assert_eq!(
            view.commitment(&space, Epoch::new(2).unwrap()),
            Ok(Some(rekeyed))
        );
        assert_eq!(view.commitment(&space, Epoch::new(3).unwrap()), Ok(None));
        assert_eq!(view.commitment(&other_space, Epoch::FIRST), Ok(None));
        assert_eq!(created.kind, CommitKind::Create);
        assert_ne!(created.kind, rekeyed.kind);

        // The authorized window is [from, until): inclusive at the start, exclusive at the end.
        assert_eq!(
            view.is_authorized_signer(&space, &device, ChainSeq::new(0)),
            Ok(true)
        );
        assert_eq!(
            view.is_authorized_signer(&space, &device, ChainSeq::new(19)),
            Ok(true)
        );
        assert_eq!(
            view.is_authorized_signer(&space, &device, ChainSeq::new(20)),
            Ok(false)
        );
        assert_eq!(
            view.is_authorized_signer(&space, &stranger, ChainSeq::new(5)),
            Ok(false)
        );
        assert_eq!(
            view.is_authorized_signer(&other_space, &device, ChainSeq::new(5)),
            Ok(false)
        );
    }

    // Two windows for one device (removed, then added again), and an open-ended one.
    chain.authorize(space, stranger, ChainSeq::new(5), Some(ChainSeq::new(8)));
    chain.authorize(space, stranger, ChainSeq::new(12), None);
    for (seq, want) in [
        (4, false),
        (5, true),
        (7, true),
        (8, false),
        (11, false),
        (12, true),
        (u64::MAX, true),
    ] {
        assert_eq!(
            chain.is_authorized_signer(&space, &stranger, ChainSeq::new(seq)),
            Ok(want),
            "position {seq}"
        );
    }

    // An unreadable chain is an error and not "no commitment" or "not authorized": every answer
    // is the error, for every kind of question.
    for error in [
        ChainError::Unreadable,
        ChainError::UnknownSpace,
        ChainError::Corrupt,
    ] {
        chain.fail_with(Some(error));
        let view: &dyn SpaceChainView = &chain;
        assert_eq!(view.commitment(&space, Epoch::FIRST), Err(error));
        assert_eq!(view.commitment(&other_space, Epoch::FIRST), Err(error));
        assert_eq!(
            view.is_authorized_signer(&space, &device, ChainSeq::new(0)),
            Err(error)
        );
        assert_eq!(
            view.is_authorized_signer(&space, &stranger, ChainSeq::new(0)),
            Err(error)
        );
    }
    chain.fail_with(None);
    assert_eq!(chain.commitment(&space, Epoch::FIRST), Ok(Some(created)));
    assert_ne!(
        ChainError::Unreadable.to_string(),
        ChainError::Corrupt.to_string()
    );
    assert_ne!(
        ChainError::Unreadable.to_string(),
        ChainError::UnknownSpace.to_string()
    );
}

#[test]
fn c4_commitment_is_plain_data() {
    let space = SpaceId::from_bytes([1; 16]);
    let a = commitment(space, 1, 0, CommitKind::Create);
    let b = a; // Copy
    assert_eq!(a, b);
    let mut c = a;
    c.seq = ChainSeq::new(1);
    assert_ne!(a, c);
    let mut d = a;
    d.kind = CommitKind::Rekey;
    assert_ne!(a, d);
    let mut e = a;
    e.key_id = KeyId::from_bytes([9; 32]);
    assert_ne!(a, e);
    let mut f = a;
    f.signer = DeviceId::from_bytes([9; 16]);
    assert_ne!(a, f);
}
