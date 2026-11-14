//! S1-03: clocks and the leader's stamp (planning/architecture.md section 1.4 rule 2: clock skew
//! and jumps; section 2.4: time is the leader's stamp, checked). Every test enters through the
//! public API.

use azlin_seams::sim::Rng;
use azlin_seams::{Clock, ClockError, Stamp, UnixTime};
use azlin_sim::draw::{below, between};
use azlin_sim::time::{median, Shift, SimClock, StampRefusal, Stamper, SIM_START};

fn at(secs: u64) -> Stamp {
    UnixTime::from_secs(secs)
}

#[test]
fn s1_03_skew_and_jumps_move_what_a_clock_reads() {
    let mut clocks = SimClock::new(4);
    assert_eq!(clocks.len(), 4);
    assert_eq!(clocks.read(0), Ok(at(SIM_START)));
    clocks.skew(1, Shift::from_secs(30));
    assert_eq!(clocks.read(1), Ok(at(SIM_START + 30)));
    clocks.jump(1, Shift::from_secs(-10));
    assert_eq!(
        clocks.read(1),
        Ok(at(SIM_START + 20)),
        "a jump adds to the offset"
    );
    clocks.skew(1, Shift::from_secs(-5));
    assert_eq!(
        clocks.read(1),
        Ok(at(SIM_START - 5)),
        "a skew sets the offset"
    );
    clocks.advance(100);
    assert_eq!(clocks.global(), SIM_START + 100);
    assert_eq!(clocks.read(0), Ok(at(SIM_START + 100)));
    assert_eq!(
        clocks.read(1),
        Ok(at(SIM_START + 95)),
        "the offsets stay when time moves"
    );
    // Another clock is untouched.
    assert_eq!(clocks.read(2), Ok(at(SIM_START + 100)));
    // The clock of a node is the `Clock` seam of the crates.
    let view = clocks.view(1);
    assert_eq!(view.now(), Ok(at(SIM_START + 95)));
    // A clock that does not exist or is off cannot be read, and it is never the time 0.
    assert_eq!(clocks.read(99), Err(ClockError::Unreadable));
    clocks.set_unreadable(3, true);
    assert_eq!(clocks.read(3), Err(ClockError::Unreadable));
    assert_eq!(clocks.view(3).now(), Err(ClockError::Unreadable));
    clocks.set_unreadable(3, false);
    assert!(clocks.read(3).is_ok());
    // A jump to before 1970 is a clock that reads before the epoch.
    clocks.jump(0, Shift::from_secs(i32::MIN));
    clocks.jump(0, Shift::from_secs(i32::MIN));
    assert_eq!(clocks.read(0), Err(ClockError::BeforeEpoch));
}

fn readable(clocks: &SimClock) -> [Result<Stamp, ClockError>; 3] {
    [clocks.read(0), clocks.read(1), clocks.read(2)]
}

/// The median of three clocks as the test sees it: the middle of three readable ones, the smaller
/// of two, nothing for fewer.
fn oracle_median(voters: &[Result<Stamp, ClockError>; 3]) -> Option<Stamp> {
    let mut ok: Vec<Stamp> = voters
        .iter()
        .filter_map(|v| v.as_ref().ok().copied())
        .collect();
    ok.sort();
    match ok.len() {
        3 => Some(ok[1]),
        2 => Some(ok[0]),
        _ => None,
    }
}

#[test]
fn s1_03_the_median_is_the_middle_of_three_and_the_smaller_of_two() {
    let ok = |s| Ok(at(s));
    assert_eq!(median(&[ok(10), ok(30), ok(20)]), Some(at(20)));
    assert_eq!(
        median(&[ok(10), Err(ClockError::Unreadable), ok(20)]),
        Some(at(10))
    );
    assert_eq!(
        median(&[
            ok(10),
            Err(ClockError::Unreadable),
            Err(ClockError::BeforeEpoch)
        ]),
        None
    );
    assert_eq!(median(&[Err(ClockError::Unreadable); 3]), None);
}

#[test]
fn s1_03_two_thousand_sequences_of_jumps_never_make_a_stamp_run_backwards_or_pass_the_voters() {
    let mut refusals = 0u64;
    let mut stamps = 0u64;
    let mut capped = 0u64;
    for seed in 0..2_000u64 {
        let mut rng = Rng::new(0x57A_4B3 + seed);
        let mut clocks = SimClock::new(3);
        let mut stamper = Stamper::new();
        let mut previous: Option<Stamp> = None;
        for _ in 0..60 {
            match below(&mut rng, 6) {
                0 | 1 => clocks.advance(between(&mut rng, 0, 120)),
                2 => {
                    // A small jump of any clock, back or forward.
                    let node = below(&mut rng, 3) as usize;
                    let secs = between(&mut rng, 0, 600) as i32 - 300;
                    clocks.jump(node, Shift::from_secs(secs));
                }
                3 => {
                    // A large jump: hours, days.
                    let node = below(&mut rng, 3) as usize;
                    let secs = between(&mut rng, 0, 400_000) as i32 - 200_000;
                    clocks.jump(node, Shift::from_secs(secs));
                }
                4 => {
                    let node = below(&mut rng, 3) as usize;
                    clocks.skew(
                        node,
                        Shift::from_secs(between(&mut rng, 0, 100) as i32 - 50),
                    );
                }
                _ => {
                    let node = below(&mut rng, 3) as usize;
                    clocks.set_unreadable(node, below(&mut rng, 4) == 0);
                }
            }
            let voters = readable(&clocks);
            match stamper.stamp(clocks.read(0), &voters) {
                Ok(stamp) => {
                    stamps += 1;
                    let median = oracle_median(&voters).expect("a stamp needs a median");
                    assert!(
                        stamp <= median,
                        "seed {seed}: stamp {stamp:?} past the median {median:?}"
                    );
                    if let Some(previous) = previous {
                        assert!(
                            stamp >= previous,
                            "seed {seed}: stamp {stamp:?} below {previous:?}"
                        );
                    }
                    if let Ok(wall) = clocks.read(0) {
                        if stamp < wall {
                            capped += 1;
                        }
                        assert!(
                            stamp >= wall.min(median) || previous.is_some_and(|p| stamp >= p),
                            "seed {seed}"
                        );
                    }
                    previous = Some(stamp);
                    assert_eq!(stamper.last(), Some(stamp));
                }
                Err(StampRefusal::ClockUnreadable(error)) => {
                    refusals += 1;
                    assert_eq!(clocks.read(0), Err(error), "seed {seed}");
                }
                Err(StampRefusal::VotersUnreadable) => {
                    refusals += 1;
                    assert!(oracle_median(&voters).is_none(), "seed {seed}");
                }
                Err(StampRefusal::VotersBehind) => {
                    refusals += 1;
                    let median = oracle_median(&voters).expect("a median");
                    assert!(
                        previous.is_some_and(|p| p > median),
                        "seed {seed}: behind without a reason"
                    );
                }
            }
        }
    }
    assert!(stamps > 20_000, "{stamps} stamps");
    assert!(
        refusals > 100,
        "{refusals} refusals: the rule must be exercised"
    );
    assert!(
        capped > 100,
        "{capped} stamps were held below the leader's clock"
    );
}

#[test]
fn s1_03_a_leader_that_stamps_with_its_raw_clock_runs_backwards_after_a_jump() {
    let mut clocks = SimClock::new(3);
    let mut buggy = Stamper::new();
    let first = buggy.raw(clocks.read(0)).expect("a clock");
    clocks.jump(0, Shift::from_secs(-3_600));
    let second = buggy.raw(clocks.read(0)).expect("a clock");
    assert!(
        second < first,
        "the buggy stamp runs backwards: that is the planted violation"
    );
    // The honest stamper holds its stamp.
    let mut honest = Stamper::new();
    clocks.jump(0, Shift::from_secs(3_600));
    let a = honest
        .stamp(clocks.read(0), &readable(&clocks))
        .expect("stamp");
    clocks.jump(0, Shift::from_secs(-3_600));
    let b = honest
        .stamp(clocks.read(0), &readable(&clocks))
        .expect("stamp");
    assert_eq!(a, b, "the clock went back; the stamp stays");
    // `observe` never lowers the previous stamp.
    honest.observe(at(5));
    assert_eq!(honest.last(), Some(a));
    honest.observe(at(a.as_secs() + 10));
    assert_eq!(honest.last(), Some(at(a.as_secs() + 10)));
}

#[test]
fn s1_03_a_clock_that_cannot_be_read_stops_the_stamp_and_is_never_zero() {
    let mut clocks = SimClock::new(3);
    clocks.set_unreadable(0, true);
    let mut stamper = Stamper::new();
    assert_eq!(
        stamper.stamp(clocks.read(0), &readable(&clocks)),
        Err(StampRefusal::ClockUnreadable(ClockError::Unreadable))
    );
    assert_eq!(stamper.last(), None, "no stamp was made");
    clocks.set_unreadable(0, false);
    clocks.set_unreadable(1, true);
    clocks.set_unreadable(2, true);
    assert_eq!(
        stamper.stamp(clocks.read(0), &readable(&clocks)),
        Err(StampRefusal::VotersUnreadable)
    );
}
