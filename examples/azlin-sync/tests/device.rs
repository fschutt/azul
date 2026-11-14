//! The device half: the head guard (check 5, the five rows of `azul-storage/src/restore.rs` for heads
//! and the rows of S15, V3, F3 and E4) and the validation of a `watch` subscription (check 7) of
//! lane W1-S16a (brief section 6).
//!
//! Implements planning/architecture.md section 5.13 (a head never goes backwards; `watch`), section
//! 6.10 S15, V3 and F3, and section 12 E4.

mod common;

use azlin_chain::{verify_record, StrictTrust, VerifiedRestore};
use azlin_seams::sim::Rng;
use azlin_seams::{Hash32, SpaceId};
use azlin_sync::{
    baseline, judge, validate_subscription, HeadRegister, Level, Seen, Served, SpaceAccess,
    Standing, WatchRefusal, MAX_SPACES,
};
use common::*;

/// The space under test.
const S: u8 = 1;

fn seen(v: Option<u64>, e: u64) -> Seen {
    Seen {
        version: v.map(version),
        restore_epoch: epoch(e),
    }
}

fn served(v: u64, e: u64, fill: u8) -> Served {
    Served {
        version: version(v),
        restore_epoch: epoch(e),
        value_hash: hash_of_fill(fill),
    }
}

fn h(fill: u8) -> Option<Hash32> {
    Some(hash_of_fill(fill))
}

// ---------------------------------------------------------------------------------------------
// Check 5: the five rows of the specification, restated for heads
// ---------------------------------------------------------------------------------------------

#[test]
fn judge_newer_verified_epoch_is_restored_whatever_the_counter_says() {
    let notice = verified(2, 500, &[S]);
    let memory = seen(Some(10), 1);
    // Lower, equal and higher versions: every one is the restored space.
    for v in [3u64, 10, 15] {
        let standing = judge(&memory, &served(v, 2, 7), h(7), Some(&notice), space(S));
        assert_eq!(
            standing,
            Standing::Restored { epoch: epoch(2) },
            "served version {v}"
        );
    }
    // The same version with another value is the restore, not an equivocation.
    let standing = judge(&memory, &served(10, 2, 8), h(7), Some(&notice), space(S));
    assert_eq!(standing, Standing::Restored { epoch: epoch(2) });
    // The standing moves the memory to the served head, down included, and takes the epoch.
    let after = memory.after(&Standing::Restored { epoch: epoch(2) }, &served(3, 2, 7));
    assert_eq!(after, seen(Some(3), 2));
    // A first look is restored too when the announcement is newer than its baseline.
    let fresh = Seen::fresh(None, space(S));
    let standing = judge(&fresh, &served(1, 2, 7), None, Some(&notice), space(S));
    assert_eq!(standing, Standing::Restored { epoch: epoch(2) });
}

#[test]
fn judge_same_version_same_value_is_in_sync() {
    let memory = seen(Some(5), 0);
    let standing = judge(&memory, &served(5, 0, 7), h(7), None, space(S));
    assert_eq!(standing, Standing::InSync);
    // An announcement that is not newer than the memory changes nothing about it.
    let old = verified(0, 100, &[S]);
    let standing = judge(&memory, &served(5, 0, 7), h(7), Some(&old), space(S));
    assert_eq!(standing, Standing::InSync);
    // The memory after InSync is the same.
    assert_eq!(memory.after(&Standing::InSync, &served(5, 0, 7)), memory);
}

#[test]
fn judge_higher_version_is_behind() {
    let memory = seen(Some(5), 0);
    for v in [6u64, 7, 500] {
        let standing = judge(&memory, &served(v, 0, 9), h(7), None, space(S));
        assert_eq!(standing, Standing::Behind, "served version {v}");
    }
    // Behind takes the head.
    let after = memory.after(&Standing::Behind, &served(6, 0, 9));
    assert_eq!(after, seen(Some(6), 0));
}

#[test]
fn judge_lower_version_is_rollback() {
    let memory = seen(Some(5), 0);
    for v in [1u64, 4] {
        let standing = judge(&memory, &served(v, 0, 7), h(7), None, space(S));
        assert_eq!(standing, Standing::Rollback, "served version {v}");
        // A refusal changes nothing.
        assert_eq!(memory.after(&standing, &served(v, 0, 7)), memory);
    }
}

#[test]
fn judge_own_counter_drive_decides_by_epoch_only() {
    // DRIFT X-DELTA-W1S16A-2: a head's version is always the register's shared counter, so the old
    // row "a counter that counts this device's own commits says nothing about the drive" is restated
    // for a pairing that remembers no version: the counter says nothing, only the epoch decides.
    let fresh = Seen::fresh(None, space(S));
    assert_eq!(fresh.version, None);
    assert_eq!(fresh.restore_epoch, epoch(0));
    for v in [1u64, 2, 500] {
        let standing = judge(&fresh, &served(v, 0, 7), None, None, space(S));
        assert_eq!(
            standing,
            Standing::Behind,
            "served version {v}: the counter decides nothing"
        );
    }
    // A first look takes a head of a higher epoch as it is: there is nothing of its own to heal.
    assert_eq!(
        judge(&fresh, &served(3, 4, 7), None, None, space(S)),
        Standing::Behind
    );
    // The epoch of the baseline is the announced one: below it is a rollback, at it a first look.
    let notice = verified(3, 100, &[S]);
    let paired = Seen::fresh(Some(&notice), space(S));
    assert_eq!(paired.restore_epoch, epoch(3));
    assert_eq!(
        judge(&paired, &served(9, 2, 7), None, Some(&notice), space(S)),
        Standing::Rollback
    );
    assert_eq!(
        judge(&paired, &served(1, 3, 7), None, Some(&notice), space(S)),
        Standing::Behind
    );
    // A newer announcement that covers the space restores it, whatever the counter.
    let newer = verified(4, 200, &[S]);
    assert_eq!(
        judge(&paired, &served(2, 4, 7), None, Some(&newer), space(S)),
        Standing::Restored { epoch: epoch(4) }
    );
    // The first look takes the head: the memory has a version then.
    let after = fresh.after(&Standing::Behind, &served(3, 4, 7));
    assert_eq!(after, seen(Some(3), 4));
}

// ---------------------------------------------------------------------------------------------
// Check 5: the rows of S15, V3, F3 and E4
// ---------------------------------------------------------------------------------------------

#[test]
fn a_replayed_old_head_is_a_rollback_not_a_reset() {
    // F3: the device has accepted the restore of epoch 2 and stands at version 4 of that epoch.
    let notice = verified(2, 500, &[S]);
    let memory = seen(Some(4), 2);
    // The server replays the lost timeline: its newest head is version 9 of epoch 1. The version is
    // HIGHER than the device's, and still the old history coming back.
    let standing = judge(&memory, &served(9, 1, 7), h(7), Some(&notice), space(S));
    assert_eq!(standing, Standing::Rollback);
    assert_eq!(memory.after(&standing, &served(9, 1, 7)), memory);
    // Without the notice in hand it is the same.
    let standing = judge(&memory, &served(9, 1, 7), h(7), None, space(S));
    assert_eq!(standing, Standing::Rollback);
    // An old head of the new epoch with a lower version is a rollback too.
    let standing = judge(&memory, &served(3, 2, 7), h(7), Some(&notice), space(S));
    assert_eq!(standing, Standing::Rollback);
}

#[test]
fn an_announcement_for_another_space_excuses_nothing() {
    // The record covers space 2 only.
    let other = verified(1, 500, &[2]);
    let memory = seen(Some(5), 0);
    // Space 1's head carries epoch 1: nobody announced a restore of space 1.
    let standing = judge(&memory, &served(2, 1, 7), h(7), Some(&other), space(1));
    assert_eq!(standing, Standing::UnannouncedEpoch);
    // A lower version of the same epoch is a rollback, not a restore.
    let standing = judge(&memory, &served(2, 0, 7), h(7), Some(&other), space(1));
    assert_eq!(standing, Standing::Rollback);
    // For the space it does cover, it excuses.
    let standing = judge(&memory, &served(2, 1, 7), h(7), Some(&other), space(2));
    assert_eq!(standing, Standing::Restored { epoch: epoch(1) });
    // The baseline of a pairing ignores it as well.
    assert_eq!(baseline(Some(&other), space(1)), epoch(0));
    assert_eq!(baseline(Some(&other), space(2)), epoch(1));
}

#[test]
fn an_announcement_whose_epoch_is_not_above_the_seen_one_excuses_nothing() {
    // The device has accepted epoch 3. A record of epoch 3 (already applied) or 2 (a replayed old one)
    // that covers the space excuses nothing.
    let memory = seen(Some(5), 3);
    for n in [2u64, 3] {
        let old = verified(n, 100, &[S]);
        let lower = judge(&memory, &served(2, 3, 7), h(7), Some(&old), space(S));
        assert_eq!(lower, Standing::Rollback, "a replayed record of epoch {n}");
        let unannounced = judge(&memory, &served(9, 4, 7), h(7), Some(&old), space(S));
        assert_eq!(
            unannounced,
            Standing::UnannouncedEpoch,
            "a replayed record of epoch {n}"
        );
        // The rollback to epoch 2 or 3 the old record could excuse is refused.
        let back = judge(&memory, &served(1, 2, 7), h(7), Some(&old), space(S));
        assert_eq!(back, Standing::Rollback, "a replayed record of epoch {n}");
    }
}

#[test]
fn an_unverified_announcement_excuses_nothing() {
    // The signature of the map key is what makes a record followed (E4). A record signed by the group
    // alone, an unsigned one and one whose epoch was changed after signing give no `VerifiedRestore`.
    let trust = StrictTrust::new(&FakeRestoreVerifier);
    let group_alone = group_signed(restore_record(1, 500, &[S]));
    assert!(verify_record(&group_alone, &trust).is_err());
    let unsigned = restore_record(1, 500, &[S]);
    assert!(verify_record(&unsigned, &trust).is_err());
    let mut forged = map_signed(restore_record(1, 500, &[S]));
    forged.epoch = epoch(9);
    assert!(verify_record(&forged, &trust).is_err());
    // So the caller has no witness to pass (`judge` takes `Option<&VerifiedRestore>` and nothing
    // else), and the head of an epoch nobody announced is refused.
    let memory = seen(Some(5), 0);
    let standing = judge(&memory, &served(7, 1, 7), h(7), None, space(S));
    assert_eq!(standing, Standing::UnannouncedEpoch);
    // Even with a head that goes back (the rollback the record would have excused).
    let standing = judge(&memory, &served(2, 1, 7), h(7), None, space(S));
    assert_eq!(standing, Standing::UnannouncedEpoch);
}

#[test]
fn same_version_other_value_is_equivocation() {
    let memory = seen(Some(5), 0);
    let standing = judge(&memory, &served(5, 0, 8), h(7), None, space(S));
    assert_eq!(standing, Standing::Equivocation);
    // Refused: nothing changes.
    assert_eq!(memory.after(&standing, &served(5, 0, 8)), memory);
    // The same value is in sync; a value the device did not remember is read again, not trusted.
    assert_eq!(
        judge(&memory, &served(5, 0, 7), h(7), None, space(S)),
        Standing::InSync
    );
    assert_eq!(
        judge(&memory, &served(5, 0, 7), None, None, space(S)),
        Standing::Behind
    );
}

#[test]
fn a_served_epoch_above_seen_without_a_notice_is_unannounced_epoch() {
    let memory = seen(Some(5), 0);
    for v in [2u64, 5, 9] {
        let standing = judge(&memory, &served(v, 1, 7), h(7), None, space(S));
        assert_eq!(standing, Standing::UnannouncedEpoch, "served version {v}");
        assert_eq!(memory.after(&standing, &served(v, 1, 7)), memory);
    }
}

#[test]
fn baseline_of_a_new_pairing_is_the_current_epoch() {
    let notice = verified(4, 100, &[S, 2]);
    assert_eq!(baseline(Some(&notice), space(S)), epoch(4));
    assert_eq!(baseline(Some(&notice), space(2)), epoch(4));
    assert_eq!(
        baseline(Some(&notice), space(3)),
        epoch(0),
        "a space the record does not cover"
    );
    assert_eq!(baseline(None, space(S)), epoch(0));
    let fresh = Seen::fresh(Some(&notice), space(S));
    assert_eq!(
        fresh,
        Seen {
            version: None,
            restore_epoch: epoch(4)
        }
    );
    // An announcement made before the pairing never excuses a rollback after it: the pairing born
    // after epoch 4 refuses the lost timeline of epoch 3.
    assert_eq!(
        judge(&fresh, &served(30, 3, 7), None, Some(&notice), space(S)),
        Standing::Rollback
    );
}

// ---------------------------------------------------------------------------------------------
// Check 5: more rows of the table
// ---------------------------------------------------------------------------------------------

#[test]
fn a_head_below_the_announced_epoch_is_a_rollback_even_when_an_announcement_applies() {
    // The announcement (epoch 2, covering the space) applies, but the head served is of epoch 1: the
    // lost timeline, or a replica that has not applied the restore: refused, retried at the leader.
    let notice = verified(2, 500, &[S]);
    let memory = seen(Some(5), 0);
    assert_eq!(
        judge(&memory, &served(30, 1, 7), h(7), Some(&notice), space(S)),
        Standing::Rollback
    );
    // A head of epoch 3 is a restore that nobody announced yet.
    assert_eq!(
        judge(&memory, &served(2, 3, 7), h(7), Some(&notice), space(S)),
        Standing::UnannouncedEpoch
    );
    // A head of epoch 2 is the restored space.
    assert_eq!(
        judge(&memory, &served(2, 2, 7), h(7), Some(&notice), space(S)),
        Standing::Restored { epoch: epoch(2) }
    );
}

#[test]
fn a_lost_timeline_head_with_a_higher_version_is_refused_without_any_notice() {
    let memory = seen(Some(4), 2);
    assert_eq!(
        judge(&memory, &served(50, 1, 7), h(7), None, space(S)),
        Standing::Rollback
    );
    assert_eq!(
        judge(&memory, &served(50, 0, 7), h(7), None, space(S)),
        Standing::Rollback
    );
}

#[test]
fn the_memory_after_a_standing() {
    let memory = seen(Some(10), 2);
    // A refusal changes nothing, whatever it was served.
    for standing in [
        Standing::Rollback,
        Standing::Equivocation,
        Standing::UnannouncedEpoch,
    ] {
        assert_eq!(memory.after(&standing, &served(99, 7, 7)), memory);
    }
    // InSync and Behind never lower the version or the epoch (the defence of the one place that
    // updates the memory).
    for standing in [Standing::InSync, Standing::Behind] {
        assert_eq!(memory.after(&standing, &served(3, 2, 7)), memory);
        assert_eq!(memory.after(&standing, &served(3, 1, 7)), memory);
        assert_eq!(
            memory.after(&standing, &served(11, 2, 7)),
            seen(Some(11), 2)
        );
        assert_eq!(
            memory.after(&standing, &served(11, 5, 7)),
            seen(Some(11), 5)
        );
    }
    // Restored takes the head and the epoch only when the epoch is above the remembered one.
    assert_eq!(
        memory.after(&Standing::Restored { epoch: epoch(3) }, &served(4, 3, 7)),
        seen(Some(4), 3)
    );
    assert_eq!(
        memory.after(&Standing::Restored { epoch: epoch(2) }, &served(4, 2, 7)),
        memory,
        "a restore that is not above the epoch moves nothing"
    );
}

#[test]
fn served_reads_the_version_the_epoch_and_the_value_hash_of_a_record() {
    let mut register = HeadRegister::new(space(S));
    fill(&mut register, &["a", "b"]);
    let record = register.get().cloned().expect("head");
    let served = Served::of(&record);
    assert_eq!(served.version, version(2));
    assert_eq!(served.restore_epoch, epoch(0));
    assert_eq!(served.value_hash, record.value.hash());
    assert_ne!(record.value.hash(), value("a").hash());
    assert_eq!(value("b").hash(), record.value.hash());
}

#[test]
fn a_restore_of_the_register_is_healed_by_the_device_and_the_lost_timeline_is_refused() {
    // Server and device together.
    let mut register = HeadRegister::new(space(S));
    fill(&mut register, &["a", "b", "c"]); // v1 at 10, v2 at 20, v3 at 30
    let before_restore = register.clone();

    // The device looks at the space for the first time.
    let mut memory = Seen::fresh(None, space(S));
    let mut hash: Option<Hash32> = None;
    let head3 = Served::of(register.get().expect("head"));
    let standing = judge(&memory, &head3, hash, None, space(S));
    assert_eq!(standing, Standing::Behind);
    memory = memory.after(&standing, &head3);
    hash = Some(head3.value_hash);
    assert_eq!(memory, seen(Some(3), 0));

    // The operator restores the space as of 15 (v1, "a"): the head is v4 of epoch 1.
    let restored = register
        .restore_as_of(at(15), at(40), operator_auth(space(S), at(15), 1))
        .expect("restore");
    let head4 = Served::of(&restored.record);
    assert_eq!(head4.version, version(4));
    assert_eq!(head4.restore_epoch, epoch(1));
    // Before the announcement reaches the device, the new epoch is not followed.
    assert_eq!(
        judge(&memory, &head4, hash, None, space(S)),
        Standing::UnannouncedEpoch
    );
    // Announced (signed by the map key): the device heals.
    let notice = verified(1, 40, &[S]);
    let standing = judge(&memory, &head4, hash, Some(&notice), space(S));
    assert_eq!(standing, Standing::Restored { epoch: epoch(1) });
    memory = memory.after(&standing, &head4);
    hash = Some(head4.value_hash);
    assert_eq!(memory, seen(Some(4), 1));

    // The lost timeline's old head comes back (v3 of epoch 0): refused.
    let old3 = Served::of(before_restore.get().expect("head"));
    assert_eq!(
        judge(&memory, &old3, hash, Some(&notice), space(S)),
        Standing::Rollback
    );
    // So does a head of the lost timeline that went on past the restore: v9 of epoch 0 is a HIGHER
    // version than v4, and only its epoch gives it away.
    let mut lost = before_restore;
    for n in 4..=9u64 {
        let expected = lost.get().map(|r| r.version);
        let outcome = lost.cas(
            expected,
            value(&format!("lost {n}")),
            device(2),
            at(100 + n),
            azlin_sync::LockTerm::Unlocked,
        );
        assert!(matches!(outcome, azlin_sync::CasOutcome::Ok { .. }));
    }
    let old9 = Served::of(lost.get().expect("head"));
    assert_eq!(old9.version, version(9));
    assert_eq!(
        judge(&memory, &old9, hash, Some(&notice), space(S)),
        Standing::Rollback
    );
    // The restored timeline goes on: a write on the restored head is Behind, and the device follows.
    let next = register.cas(
        Some(version(4)),
        value("after restore"),
        device(2),
        at(60),
        azlin_sync::LockTerm::Unlocked,
    );
    let azlin_sync::CasOutcome::Ok { record } = next else {
        panic!("{next:?}");
    };
    let head5 = Served::of(&record);
    assert_eq!(
        judge(&memory, &head5, hash, Some(&notice), space(S)),
        Standing::Behind
    );
}

// ---------------------------------------------------------------------------------------------
// Check 5: the property
// ---------------------------------------------------------------------------------------------

#[test]
fn the_seen_version_never_decreases_over_10000_seeded_sequences() {
    // Twelve verified records: epochs 1 to 4, each covering the space 1, the space 2, or both.
    let mut notices: Vec<(u64, Vec<u8>, VerifiedRestore)> = Vec::new();
    for e in 1..=4u64 {
        for covers in [vec![1u8], vec![2u8], vec![1u8, 2u8]] {
            let witness = verified(e, 100 * e, &covers);
            notices.push((e, covers, witness));
        }
    }
    for seed in 0..10_000u64 {
        let mut rng = Rng::new(seed);
        let mut memory = Seen::fresh(None, space(1));
        let mut hash: Option<Hash32> = None;
        for step in 0..30 {
            let served = Served {
                version: version(1 + rng.below(12).expect("range")),
                restore_epoch: epoch(rng.below(5).expect("range")),
                value_hash: hash_of_fill(
                    u8::try_from(rng.below(3).expect("range")).expect("small"),
                ),
            };
            let notice = if rng.chance(1, 2) {
                let pick = usize::try_from(rng.below(notices.len() as u64).expect("range"))
                    .expect("small");
                notices.get(pick)
            } else {
                None
            };
            let witness = notice.map(|(_, _, witness)| witness);
            let standing = judge(&memory, &served, hash, witness, space(1));
            let next = memory.after(&standing, &served);

            // The central property (E4: "one older than a device's newest head"): the remembered
            // version goes down only through a Restored standing, and the epoch never goes down.
            if next.version < memory.version {
                assert!(
                    matches!(standing, Standing::Restored { .. }),
                    "seed {seed} step {step}: {memory:?} -> {next:?} through {standing:?}"
                );
            }
            assert!(
                next.restore_epoch >= memory.restore_epoch,
                "seed {seed} step {step}"
            );
            match standing {
                Standing::Restored { epoch: announced } => {
                    let (_, covers, witness) = notice.expect("a restore needs a record");
                    assert!(
                        covers.contains(&1),
                        "seed {seed}: a record for another space excused a rollback"
                    );
                    assert!(
                        announced > memory.restore_epoch,
                        "seed {seed}: a replayed record"
                    );
                    assert_eq!(announced, witness.epoch());
                    assert_eq!(
                        served.restore_epoch, announced,
                        "seed {seed}: the head is of the announced epoch"
                    );
                    assert_eq!(
                        next,
                        Seen {
                            version: Some(served.version),
                            restore_epoch: announced
                        }
                    );
                }
                Standing::InSync | Standing::Behind => {
                    // Accepted heads are never older than the memory.
                    assert!(
                        served.restore_epoch >= memory.restore_epoch,
                        "seed {seed} step {step}"
                    );
                    if let Some(v) = memory.version {
                        if served.restore_epoch == memory.restore_epoch {
                            assert!(
                                served.version >= v,
                                "seed {seed} step {step}: an older head was accepted"
                            );
                        }
                    }
                    assert!(next.version >= memory.version, "seed {seed} step {step}");
                }
                Standing::Rollback | Standing::Equivocation | Standing::UnannouncedEpoch => {
                    assert_eq!(
                        next, memory,
                        "seed {seed} step {step}: a refusal moved the memory"
                    );
                }
            }
            if matches!(
                standing,
                Standing::InSync | Standing::Behind | Standing::Restored { .. }
            ) {
                hash = Some(served.value_hash);
            }
            memory = next;
        }
    }
}

#[test]
fn judge_is_total_over_a_grid() {
    // Every combination of memory, head and announcement has an answer, and the answers obey the
    // table: no panic, and Restored only for a verified, covering, newer record of the head's epoch.
    let covering = verified(2, 100, &[S]);
    let elsewhere = verified(2, 100, &[2]);
    for sv in [None, Some(1u64), Some(5)] {
        for se in 0..4u64 {
            for v in [1u64, 5, 9] {
                for e in 0..4u64 {
                    for fill in [7u8, 8] {
                        for notice in [None, Some(&covering), Some(&elsewhere)] {
                            let memory = seen(sv, se);
                            let head = served(v, e, fill);
                            let standing = judge(&memory, &head, h(7), notice, space(S));
                            if let Standing::Restored { epoch: announced } = standing {
                                let record = notice.expect("a record");
                                assert!(record.covers(&azlin_chain::ChainId::space(space(S))));
                                assert!(announced > memory.restore_epoch);
                                assert_eq!(head.restore_epoch, announced);
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Check 7: watch
// ---------------------------------------------------------------------------------------------

/// A distinct space for every index.
fn space_n(i: usize) -> SpaceId {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&(i as u64).to_be_bytes());
    SpaceId::from_bytes(bytes)
}

/// A registry that lets every device read every space.
struct AllowAll;

impl SpaceAccess for AllowAll {
    fn may_head_get(&self, _device: azlin_seams::DeviceId, _space: SpaceId) -> bool {
        true
    }
    fn may_append(&self, _device: azlin_seams::DeviceId, _space: SpaceId) -> bool {
        true
    }
    fn author_key(&self, _device: azlin_seams::DeviceId) -> Option<azlin_seams::VerifyKey> {
        None
    }
}

#[test]
fn subscription_over_20000_spaces_is_too_many_bad_request() {
    assert_eq!(MAX_SPACES, 20_000);
    // 20,000 distinct spaces are accepted...
    let entries: Vec<(SpaceId, Level)> =
        (0..MAX_SPACES).map(|i| (space_n(i), Level::Sync)).collect();
    let accepted =
        validate_subscription(&entries, device(1), &AllowAll).expect("exactly the limit");
    assert_eq!(accepted.len(), MAX_SPACES);
    // ...one more is not.
    let mut one_more = entries;
    one_more.push((space_n(MAX_SPACES), Level::Sync));
    let refused = validate_subscription(&one_more, device(1), &AllowAll);
    assert_eq!(
        refused,
        Err(WatchRefusal::TooMany {
            got: 20_001,
            max: 20_000
        })
    );
    // The limit counts the entries as sent: 20,001 copies of one space are too many too, so the work
    // is bounded by the limit and not by what a client sends.
    let copies: Vec<(SpaceId, Level)> = (0..=MAX_SPACES)
        .map(|_| (space_n(1), Level::Live))
        .collect();
    assert_eq!(
        validate_subscription(&copies, device(1), &AllowAll),
        Err(WatchRefusal::TooMany {
            got: 20_001,
            max: 20_000
        })
    );
}

#[test]
fn an_empty_subscription_is_accepted() {
    let accepted = validate_subscription(&[], device(1), &AllowAll).expect("empty");
    assert!(accepted.is_empty());
    assert_eq!(accepted.len(), 0);
    assert_eq!(accepted.iter().count(), 0);
}

#[test]
fn duplicate_spaces_collapse_to_the_highest_level() {
    let orders: [Vec<(SpaceId, Level)>; 3] = [
        vec![
            (space(1), Level::Notify),
            (space(1), Level::Live),
            (space(1), Level::Sync),
            (space(2), Level::Sync),
            (space(2), Level::Notify),
        ],
        vec![
            (space(2), Level::Notify),
            (space(1), Level::Live),
            (space(2), Level::Sync),
            (space(1), Level::Sync),
            (space(1), Level::Notify),
        ],
        vec![
            (space(1), Level::Sync),
            (space(2), Level::Sync),
            (space(1), Level::Notify),
            (space(2), Level::Notify),
            (space(1), Level::Live),
        ],
    ];
    let mut results = Vec::new();
    for entries in &orders {
        let accepted = validate_subscription(entries, device(1), &AllowAll).expect("valid");
        assert_eq!(accepted.len(), 2);
        assert_eq!(accepted.level(space(1)), Some(Level::Live));
        assert_eq!(accepted.level(space(2)), Some(Level::Sync));
        results.push(accepted);
    }
    // The order of the entries does not matter.
    assert_eq!(results[0], results[1]);
    assert_eq!(results[1], results[2]);
    // The order of the iteration is the order of the space ids.
    let listed: Vec<(SpaceId, Level)> = results[0].iter().collect();
    assert_eq!(
        listed,
        vec![(space(1), Level::Live), (space(2), Level::Sync)]
    );
}

#[test]
fn no_access_and_unknown_spaces_are_dropped_identically() {
    // Device 1 may read space 1. Space 2 EXISTS but device 1 has no grant to it; space 3 does not
    // exist. The registry answers `false` for both: the same drop, no word.
    let mut forbidden_registry = FakeAccess::default();
    forbidden_registry.readers.insert((device(1), space(1)));
    forbidden_registry.readers.insert((device(9), space(2))); // another device may read space 2
                                                              // A registry that has never heard of space 2: nobody may read it.
    let mut unknown_registry = FakeAccess::default();
    unknown_registry.readers.insert((device(1), space(1)));

    let entries = [
        (space(1), Level::Live),
        (space(2), Level::Live),
        (space(3), Level::Sync),
    ];
    let a = validate_subscription(&entries, device(1), &forbidden_registry).expect("valid");
    let b = validate_subscription(&entries, device(1), &unknown_registry).expect("valid");
    assert_eq!(
        a, b,
        "an unreadable space and an unknown one are one answer"
    );
    assert_eq!(a.len(), 1);
    assert_eq!(a.level(space(1)), Some(Level::Live));
    assert_eq!(a.level(space(2)), None);
    assert_eq!(a.level(space(3)), None);
    // Subscribing to nothing readable is no error either: the answer is an empty subscription, the
    // same for every reason.
    let nothing =
        validate_subscription(&entries[1..], device(1), &forbidden_registry).expect("valid");
    let also_nothing =
        validate_subscription(&entries[1..], device(1), &unknown_registry).expect("valid");
    assert_eq!(nothing, also_nothing);
    assert!(nothing.is_empty());
    // The answer depends on the device that asks: device 9 may read space 2.
    let nine = validate_subscription(&entries, device(9), &forbidden_registry).expect("valid");
    assert_eq!(nine.len(), 1);
    assert_eq!(nine.level(space(2)), Some(Level::Live));
    // A level is only about what is shown, not about access: a `notify` of a space the device cannot
    // read is dropped like a `live` one.
    let notify =
        validate_subscription(&[(space(2), Level::Notify)], device(1), &forbidden_registry)
            .expect("valid");
    assert!(notify.is_empty());
}

#[test]
fn levels_live_sync_notify_are_the_only_ones() {
    assert_eq!(Level::ALL, [Level::Notify, Level::Sync, Level::Live]);
    assert_eq!(Level::ALL.len(), 3);
    for level in Level::ALL {
        assert_eq!(Level::from_word(level.as_str()), Some(level));
        // An exhaustive match: a fourth level does not compile.
        match level {
            Level::Notify | Level::Sync | Level::Live => {}
        }
    }
    assert_eq!(Level::Live.as_str(), "live");
    assert_eq!(Level::Sync.as_str(), "sync");
    assert_eq!(Level::Notify.as_str(), "notify");
    // "none" is the absence of an entry, not a level; every other word is refused, case included.
    for word in [
        "none", "", "Live", "LIVE", " live", "live ", "sync\n", "all", "open",
    ] {
        assert_eq!(Level::from_word(word), None, "{word:?}");
    }
    // Live is the highest level, notify the lowest.
    assert!(Level::Notify < Level::Sync && Level::Sync < Level::Live);
    let unique: std::collections::BTreeSet<Level> = Level::ALL.into_iter().collect();
    assert_eq!(unique.len(), 3);
}
