//! S9A-02, the regbase half: `DayStart` truncates toward the start of the day and has no
//! constructor from a raw integer. (The other half, "every time column of the registry is a day
//! start except `hold_until`", is azlin-registry's.)
//!
//! DRIFT X-DELTA-W1S9A-3: the brief asks for "negative values", but `LeaderTime` is
//! `azlin_keys::UnixTime`, whole seconds since 1970 as a `u64`: there is no time before the epoch,
//! so the negative cases are the edge of the type instead (the epoch itself, and a subtraction
//! that would reach before it).

use azlin_keys::{Clock, Span, UnixTime};
use azlin_regbase::{DayStart, LeaderClock, LeaderTime};

use crate::common::Rng;

const DAY: u64 = 86_400;

fn at(secs: u64) -> LeaderTime {
    LeaderTime::from_secs(secs)
}

#[test]
fn s9a_02_day_start_truncates_toward_the_start_of_the_day() {
    let cases = [
        (0, 0),
        (1, 0),
        (86_399, 0),
        (86_400, 86_400),
        (86_401, 86_400),
        (172_799, 86_400),
        (172_800, 172_800),
        (1_800_000_000, 1_799_971_200),
    ];
    for (t, expected) in cases {
        assert_eq!(DayStart::of(at(t)).start(), at(expected), "t = {t}");
    }
    // the edge of the type: the epoch is the first day start, and nothing is before it
    assert_eq!(DayStart::of(UnixTime::EPOCH).start(), UnixTime::EPOCH);
    assert_eq!(
        DayStart::of(UnixTime::EPOCH).minus_days(Span::days(1)),
        None
    );
    // the largest time still truncates to a multiple of a day
    let last = DayStart::of(at(u64::MAX)).start().as_secs();
    assert_eq!(last % DAY, 0);
    assert!(u64::MAX - last < DAY);
}

#[test]
fn s9a_02_the_day_start_of_a_time_is_at_or_before_it_and_less_than_a_day_before() {
    let mut rng = Rng::new(0xda75_7a27);
    for _ in 0..20_000 {
        let t = at(rng.next());
        let day = DayStart::of(t);
        let start = day.start();
        assert!(start <= t, "{start:?} > {t:?}");
        assert_eq!(start.as_secs() % DAY, 0);
        // t < start + 1 day (the sum cannot overflow for a time that is not within a day of the max)
        if let Some(next) = start.checked_add(Span::days(1)) {
            assert!(t < next, "{t:?} >= {next:?}");
            assert_eq!(day.next().map(DayStart::start), Some(next));
        }
        // idempotent, and a day start is its own day start
        assert_eq!(DayStart::of(start), day);
        assert_eq!(DayStart::from_start(start), Some(day));
    }
    // monotone: a later time never has an earlier day start
    for _ in 0..5_000 {
        let a = rng.below(1 << 40);
        let b = a + rng.below(1 << 20);
        assert!(DayStart::of(at(a)) <= DayStart::of(at(b)));
    }
}

#[test]
fn s9a_02_from_start_refuses_a_time_that_is_not_a_day_start() {
    assert!(DayStart::from_start(at(0)).is_some());
    assert!(DayStart::from_start(at(DAY)).is_some());
    for secs in [1, 59, DAY - 1, DAY + 1, 2 * DAY - 1, 1_800_000_001] {
        assert_eq!(DayStart::from_start(at(secs)), None, "{secs}");
    }
}

#[test]
fn s9a_02_day_arithmetic_moves_by_whole_days_only() {
    let start = DayStart::of(at(1_800_000_000));
    let later = start.plus_days(Span::days(90)).expect("90 days");
    assert_eq!(later.start().as_secs() - start.start().as_secs(), 90 * DAY);
    assert_eq!(later.days_since(start), Some(Span::days(90)));
    assert_eq!(start.days_since(later), None);
    assert_eq!(later.minus_days(Span::days(90)), Some(start));
    assert_eq!(start.plus_days(Span::ZERO), Some(start));
    // a span that is not a whole number of days is refused, never rounded
    for odd in [
        Span::secs(1),
        Span::secs(DAY - 1),
        Span::secs(DAY + 1),
        Span::hours(36),
    ] {
        assert_eq!(start.plus_days(odd), None, "{odd:?}");
        assert_eq!(start.minus_days(odd), None, "{odd:?}");
    }
    // overflow is None, not a wrap
    let top = DayStart::of(at(u64::MAX));
    assert_eq!(top.plus_days(Span::days(1)), None);
    assert_eq!(top.next(), None);
    assert!(start.plus_days(Span::days(u32::MAX)).is_some());
    // a day start that was moved is still a day start
    assert_eq!(DayStart::from_start(later.start()), Some(later));
}

#[test]
fn s9a_02_the_leader_clock_reads_the_stamp_it_was_made_with() {
    let clock = LeaderClock::at(at(1_800_000_123));
    assert_eq!(clock.now(), Ok(at(1_800_000_123)));
    assert_eq!(clock.stamp(), at(1_800_000_123));
    // it can stand where a `Clock` is asked for
    let dynamic: &dyn Clock = &clock;
    assert_eq!(dynamic.now(), Ok(at(1_800_000_123)));
}
