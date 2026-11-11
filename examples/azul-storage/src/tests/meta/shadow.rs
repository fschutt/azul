//! The switch to deleting and the record of the shadow rounds it waits for (feature `encryption`;
//! X-DELTA-AZDRIVE-SYNC-6, ARCH29 C8): `go` is a pure judge with a truth table, and the ledger is
//! what makes its evidence something a round can fail to produce.

use crate::meta::{
    go,
    reclaim::{GO_CLEAN_ROUNDS, GO_DAYS, GO_DEVICES},
    shadow::{Footing, Ledger, Round, EVERY_SECS, KEEP_ROUNDS},
    Go, GoEvidence, Hold, MemoryBucket, ReclaimMode, TestSealer,
};

const DAY: u64 = 86_400;
const T0: u64 = 1_800_000_000;

/// Evidence in which nothing is lacking: the run may delete.
fn enough() -> GoEvidence {
    GoEvidence {
        clean_rounds: GO_CLEAN_ROUNDS,
        days: GO_DAYS,
        devices: 3,
        drive_devices: 3,
        rederived_live_equal: Some(true),
        byte_budget_ok: true,
        floor_known: true,
        restore_source: true,
    }
}

fn lacking(change: impl FnOnce(&mut GoEvidence)) -> GoEvidence {
    let mut e = enough();
    change(&mut e);
    e
}

/// X-DELTA-AZDRIVE-SYNC-6. The truth table of the switch. `Delete` is an answer only to a run
/// configured to delete AND shown by every line of the evidence; the order of the holds is the order
/// of the table.
#[test]
fn x_delta_azdrive_sync_6_go_truth_table() {
    // The kill switch and the shadow run answer the same whatever the evidence is.
    for e in [enough(), GoEvidence::default()] {
        assert_eq!(go(ReclaimMode::Off, &e), Go::Off);
        assert_eq!(go(ReclaimMode::Shadow, &e), Go::Shadow(Hold::Configured));
    }
    // Everything there: it deletes.
    assert_eq!(go(ReclaimMode::Delete, &enough()), Go::Delete);
    // Nothing shown: the first thing lacking.
    assert_eq!(
        go(ReclaimMode::Delete, &GoEvidence::default()),
        Go::Shadow(Hold::NoRestoreSource)
    );

    let rows: Vec<(&str, GoEvidence, Hold)> = vec![
        (
            "no restore source",
            lacking(|e| e.restore_source = false),
            Hold::NoRestoreSource,
        ),
        (
            "no floor",
            lacking(|e| e.floor_known = false),
            Hold::NoFloor,
        ),
        (
            "over the budget",
            lacking(|e| e.byte_budget_ok = false),
            Hold::TooBig,
        ),
        (
            "the head was not listed",
            lacking(|e| e.rederived_live_equal = None),
            Hold::NotRederived,
        ),
        (
            "the listing names what the live set lacks",
            lacking(|e| e.rederived_live_equal = Some(false)),
            Hold::Differs,
        ),
        (
            "one clean round short",
            lacking(|e| e.clean_rounds = GO_CLEAN_ROUNDS - 1),
            Hold::Rounds {
                have: GO_CLEAN_ROUNDS - 1,
                need: GO_CLEAN_ROUNDS,
            },
        ),
        (
            "one day short",
            lacking(|e| e.days = GO_DAYS - 1),
            Hold::Days {
                have: GO_DAYS - 1,
                need: GO_DAYS,
            },
        ),
        (
            "two of three devices",
            lacking(|e| e.devices = 2),
            Hold::Devices { have: 2, need: 3 },
        ),
    ];
    for (what, evidence, hold) in &rows {
        assert_eq!(
            go(ReclaimMode::Delete, evidence),
            Go::Shadow(*hold),
            "{what}"
        );
    }

    // The order: when two things are lacking the earlier one is said.
    assert_eq!(
        go(
            ReclaimMode::Delete,
            &lacking(|e| {
                e.restore_source = false;
                e.floor_known = false;
            })
        ),
        Go::Shadow(Hold::NoRestoreSource)
    );
    assert_eq!(
        go(
            ReclaimMode::Delete,
            &lacking(|e| {
                e.floor_known = false;
                e.byte_budget_ok = false;
            })
        ),
        Go::Shadow(Hold::NoFloor)
    );
    assert_eq!(
        go(
            ReclaimMode::Delete,
            &lacking(|e| {
                e.rederived_live_equal = Some(false);
                e.clean_rounds = 0;
            })
        ),
        Go::Shadow(Hold::Differs),
        "a listing that disagrees is said before the rounds that are missing"
    );
}

/// X-DELTA-AZDRIVE-SYNC-6. The devices wanted are the drive's own up to three: one device for a
/// drive that has one (the recommended default: 14 days and 3 devices, or 1 when the drive has 1).
#[test]
fn x_delta_azdrive_sync_6_the_devices_wanted_are_those_the_drive_has_up_to_three() {
    let with = |devices: u32, drive_devices: u32| {
        go(
            ReclaimMode::Delete,
            &lacking(|e| {
                e.devices = devices;
                e.drive_devices = drive_devices;
            }),
        )
    };
    assert_eq!(with(1, 1), Go::Delete, "a drive of one device: one");
    assert_eq!(with(2, 2), Go::Delete, "a drive of two: two");
    assert_eq!(with(1, 2), Go::Shadow(Hold::Devices { have: 1, need: 2 }));
    assert_eq!(with(3, 5), Go::Delete, "a drive of five: three are enough");
    assert_eq!(with(2, 5), Go::Shadow(Hold::Devices { have: 2, need: 3 }));
    assert_eq!(
        with(0, 0),
        Go::Shadow(Hold::Devices { have: 0, need: 1 }),
        "a drive that says no devices has one, and nobody showed it"
    );
    assert_eq!(with(1, 0), Go::Delete, "at least one");
    assert_eq!(GO_DEVICES, 3);
}

/// X-DELTA-AZDRIVE-SYNC-6. The property behind the table: for each line of the evidence, a copy that
/// lacks it is never `Delete`.
#[test]
fn x_delta_azdrive_sync_6_no_single_missing_line_of_the_evidence_deletes() {
    let each: Vec<GoEvidence> = vec![
        lacking(|e| e.restore_source = false),
        lacking(|e| e.floor_known = false),
        lacking(|e| e.byte_budget_ok = false),
        lacking(|e| e.rederived_live_equal = None),
        lacking(|e| e.rederived_live_equal = Some(false)),
        lacking(|e| e.clean_rounds = 0),
        lacking(|e| e.days = 0),
        lacking(|e| e.devices = 0),
    ];
    for e in each {
        assert_ne!(go(ReclaimMode::Delete, &e), Go::Delete, "{e:?}");
    }
}

fn round(day: u64, device: &str, clean: bool) -> Round {
    Round {
        at: T0 + day * DAY,
        device: device.to_string(),
        revision: day + 1,
        digest: format!("digest-{day}"),
        candidates: 3,
        candidate_bytes: 300,
        live: 10,
        clean,
        unclean: if clean {
            String::new()
        } else {
            String::from("the head was not listed")
        },
    }
}

fn footing() -> Footing {
    Footing {
        drive_devices: 3,
        rederived_live_equal: Some(true),
        byte_budget_ok: true,
        floor_known: true,
        restore_source: true,
    }
}

fn ledger_of(rounds: &[Round]) -> Ledger {
    let mut ledger = Ledger::default();
    for r in rounds {
        ledger.record(r.clone());
    }
    ledger
}

/// X-DELTA-AZDRIVE-SYNC-6. What the record shows is the clean streak AT ITS END: the rounds since
/// the last one that was not clean, the days they span and the devices that ran them. A round that
/// was not clean is not forgotten by the ones after it: it ends the streak.
#[test]
fn x_delta_azdrive_sync_6_the_evidence_is_the_clean_streak_at_the_end_of_the_ledger() {
    let ledger = ledger_of(&[
        round(1, "a", true),
        round(2, "b", true),
        round(3, "a", false),
        round(4, "a", true),
        round(6, "b", true),
    ]);
    let e = ledger.evidence(T0 + 7 * DAY, &footing());
    assert_eq!(
        (e.clean_rounds, e.days, e.devices),
        (2, 2, 2),
        "the rounds of day 4 and day 6, by two devices"
    );

    // Fifteen days of daily rounds by three devices in turn.
    let all: Vec<Round> = (0..=14)
        .map(|day| round(day, ["a", "b", "c"][(day % 3) as usize], true))
        .collect();
    let e = ledger_of(&all).evidence(T0 + 15 * DAY, &footing());
    assert_eq!((e.clean_rounds, e.days, e.devices), (15, 14, 3));
    assert_eq!(
        go(ReclaimMode::Delete, &e),
        Go::Delete,
        "fifteen clean rounds over fourteen days by three devices"
    );

    // The last round was not clean: nothing is shown.
    let mut broken = all.clone();
    broken.push(round(15, "a", false));
    let e = ledger_of(&broken).evidence(T0 + 15 * DAY, &footing());
    assert_eq!((e.clean_rounds, e.days, e.devices), (0, 0, 0));
    assert!(matches!(
        go(ReclaimMode::Delete, &e),
        Go::Shadow(Hold::Rounds { have: 0, .. })
    ));

    // An empty ledger shows nothing, and says the drive has a device.
    let e = Ledger::default().evidence(
        T0,
        &Footing {
            drive_devices: 0,
            ..footing()
        },
    );
    assert_eq!(
        (e.clean_rounds, e.days, e.devices, e.drive_devices),
        (0, 0, 0, 1)
    );
}

/// X-DELTA-AZDRIVE-SYNC-6. A round stamped in the future (a device whose clock was ahead) counts as
/// now: it makes no days.
#[test]
fn x_delta_azdrive_sync_6_a_round_stamped_in_the_future_makes_no_days() {
    let ledger = ledger_of(&[round(1, "a", true), round(400, "b", true)]);
    let e = ledger.evidence(T0 + 2 * DAY, &footing());
    assert_eq!(e.clean_rounds, 2);
    assert_eq!(e.days, 1, "day 400 is read as day 2: one day, not 399");
}

/// X-DELTA-AZDRIVE-SYNC-5. One round per drive per day, whatever the device: a round is due when none
/// of any device ran in the last day, and a stamp in the future is stale, not fresh.
#[test]
fn x_delta_azdrive_sync_5_a_round_is_due_a_day_after_the_last_of_any_device() {
    assert!(Ledger::default().due(T0), "no round yet");
    let ledger = ledger_of(&[round(0, "a", true)]);
    assert!(!ledger.due(T0), "just ran");
    assert!(
        !ledger.due(T0 + EVERY_SECS - 1),
        "a second short of the day"
    );
    assert!(ledger.due(T0 + EVERY_SECS), "a day after");
    let ahead = ledger_of(&[round(400, "a", true)]);
    assert!(
        ahead.due(T0),
        "a round stamped far ahead holds nothing back"
    );
    let two = ledger_of(&[round(0, "a", true), round(1, "b", true)]);
    assert!(
        !two.due(T0 + DAY + 3_600),
        "the newest round of any device counts"
    );
}

/// The ledger keeps the newest rounds only.
#[test]
fn x_delta_azdrive_sync_6_the_ledger_keeps_the_newest_rounds() {
    let mut ledger = Ledger::default();
    for day in 0..(KEEP_ROUNDS as u64 + 10) {
        ledger.record(round(day, "a", true));
    }
    assert_eq!(ledger.rounds.len(), KEEP_ROUNDS);
    assert_eq!(
        ledger.rounds[0].at,
        T0 + 10 * DAY,
        "the oldest ten are gone"
    );
}

/// X-DELTA-AZDRIVE-SYNC-6. The ledger is one sealed object in the bucket; each device appends its
/// round under the compare-and-swap, and a lost swap reads again and appends to what the other wrote.
#[test]
fn x_delta_azdrive_sync_6_devices_append_to_one_ledger_and_a_lost_swap_keeps_both_rounds() {
    let bucket = MemoryBucket::new();
    let sealer = TestSealer::new([7; 32]);
    let (empty, version) = Ledger::read(&bucket, &sealer).unwrap();
    assert!(
        empty.rounds.is_empty() && version.is_none(),
        "no ledger yet"
    );

    Ledger::append(&bucket, &sealer, &round(0, "a", true))
        .unwrap()
        .expect("recorded");

    // Another device's turn comes first, just before this one's swap.
    let other = bucket.clone();
    let other_sealer = TestSealer::new([7; 32]);
    bucket.before_next_replace(crate::meta::keys::SHADOW, move || {
        Ledger::append(&other, &other_sealer, &round(1, "b", true)).unwrap();
    });
    let written = Ledger::append(&bucket, &sealer, &round(2, "a", true))
        .unwrap()
        .expect("recorded after reading again");
    let devices: Vec<&str> = written.rounds.iter().map(|r| r.device.as_str()).collect();
    assert_eq!(devices, ["a", "b", "a"], "nobody's round was lost");
    let (read, _) = Ledger::read(&bucket, &sealer).unwrap();
    assert_eq!(read, written);
}

/// X-DELTA-AZDRIVE-SYNC-6. A ledger that does not open (another key, damage) is an empty one at the
/// version it has: fewer clean rounds, never more, and the next round replaces it.
#[test]
fn x_delta_azdrive_sync_6_a_ledger_that_does_not_open_starts_again_and_is_replaced() {
    let bucket = MemoryBucket::new();
    let sealer = TestSealer::new([7; 32]);
    bucket.overwrite(crate::meta::keys::SHADOW, b"not a sealed ledger at all");
    let (ledger, version) = Ledger::read(&bucket, &sealer).unwrap();
    assert!(ledger.rounds.is_empty(), "it shows nothing");
    assert!(version.is_some(), "and it is replaced, not created");
    let after = Ledger::append(&bucket, &sealer, &round(0, "a", true))
        .unwrap()
        .expect("replaced");
    assert_eq!(after.rounds.len(), 1);
    assert_eq!(Ledger::read(&bucket, &sealer).unwrap().0, after);
}
