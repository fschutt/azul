//! Re-anchoring after an announced restore (acceptance check 9, drift 1: BOTH readings of who signs a
//! restore are built behind `RestoreTrust`, the strict one is the default): a record signed by the
//! group alone is refused under the strict trust (E4), a record signed by the map key re-anchors
//! every chain it lists and no other, a replayed record is refused, an older head without a record
//! stays refused, the device learns what it must send again, and the verified record is a witness.
//!
//! Implements planning/architecture.md section 2.17 ("What a restore re-anchors"), section 5.13
//! ("A head never goes backwards ... The only way back is a restore"), section 6.10 S15 and V3 and
//! section 12 E4 ("devices accept going back only through a restore signed by the map key").

use azlin_chain::{
    reanchor, rollback_alarm, verify_record, Alarm, ChainId, ChainKind, GroupAloneTrust, Head,
    HeadPin, HeadRefusal, Kept, Moved, PinVerdict, RefusalClass, ResendRange, RestoreRecord,
    RestoreRefusal, RestoreSigner, StrictTrust, VerifiedRestore, MAX_PINNED_CHAINS,
};
use azlin_seams::{Hash32, VerifyError};

mod common;
use common::{
    at, epoch, group_head, group_head_in, group_key, group_signed, head_at, map_key, map_signed,
    map_signed_at, restore_record, restore_sig, seq, space, team_log, AmbiguousIfClone,
    AmbiguousIfDefault, FakeHeadVerifier, FakeRestoreVerifier,
};

/// A device that pinned the team log at 10, the space 1 at 4 and the space 2 at 20.
fn device() -> HeadPin {
    let verifier = FakeHeadVerifier::group();
    let mut pin = HeadPin::new();
    for (chain, position) in [(team_log(1), 10u64), (space(1), 4), (space(2), 20)] {
        assert_eq!(
            pin.offer(group_head(chain, position, 0), &verifier),
            PinVerdict::Advanced
        );
    }
    pin
}

/// The surviving positions of a restore: the team log back to 6, the space 1 where it was, the
/// space 2 ahead of the device.
fn survivors() -> Vec<Head> {
    vec![
        head_at(team_log(1), 6, 0),
        head_at(space(1), 4, 0),
        head_at(space(2), 25, 0),
    ]
}

/// The state of a pin that a refusal must leave alone.
fn snapshot(pin: &HeadPin) -> (Vec<(ChainId, Option<Head>)>, azlin_seams::RestoreEpoch) {
    let chains = [team_log(1), space(1), space(2), team_log(2), space(3)];
    (
        chains.iter().map(|c| (*c, pin.head(c))).collect(),
        pin.restore_epoch(),
    )
}

#[test]
fn restore_record_signed_by_group_alone_is_refused_under_strict_trust() {
    let verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&verifier);
    let mut pin = device();
    let before = snapshot(&pin);

    // E4: an announcement signed by the group alone is refused, and the pin is as it was.
    let rec = group_signed(1, survivors());
    let refusal = reanchor(&mut pin, &rec, &strict).expect_err("the group alone");
    assert_eq!(refusal, RestoreRefusal::MapKeySignatureMissing);
    assert_eq!(refusal.class(), RefusalClass::Alarm);
    assert_eq!(snapshot(&pin), before);
    assert!(verify_record(&rec, &strict).is_err());
    // The refusal would have moved the team log back from 10 to 6: that is the alarm to raise.
    assert_eq!(
        rollback_alarm(&pin, &rec),
        Some(Alarm::Rollback {
            chain: team_log(1),
            pinned: seq(10),
            offered: seq(6)
        })
    );
    // The other reading of the plan (the group signs): the same record is followed.
    let lenient = GroupAloneTrust::new(&verifier);
    let (verified, done) = reanchor(&mut pin, &rec, &lenient).expect("the group alone, lenient");
    assert_eq!(verified.epoch(), epoch(1));
    assert_eq!(done.epoch, epoch(1));
    assert_eq!(pin.seq(&team_log(1)), Some(seq(6)));

    // The map key's signature, with or without the group's, is followed under the strict trust.
    for also_group in [false, true] {
        let mut fresh = device();
        let rec = map_signed(1, survivors(), also_group);
        let (_, done) = reanchor(&mut fresh, &rec, &strict).expect("the map key");
        assert_eq!(done.moved.len(), 3);
        assert_eq!(fresh.seq(&team_log(1)), Some(seq(6)));
    }
}

#[test]
fn the_two_trusts_by_every_signature_set() {
    let verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&verifier);
    let lenient = GroupAloneTrust::new(&verifier);
    let base = restore_record(1, survivors());
    let sig = |by, k| restore_sig(&base, by, k);
    let bad = |by, k| {
        let mut s = restore_sig(&base, by, k);
        s.sig[0] ^= 1;
        s
    };
    use RestoreSigner::{Group, MapKey};
    type Case = (
        &'static str,
        Vec<azlin_chain::RestoreSig>,
        Result<(), RestoreRefusal>,
        Result<(), RestoreRefusal>,
    );
    let bad_sig = RestoreRefusal::BadSignature(VerifyError::BadSignature);
    let bad_key = RestoreRefusal::BadSignature(VerifyError::BadKey);
    let cases: Vec<Case> = vec![
        // (name, signatures, strict, group-alone)
        (
            "none",
            vec![],
            Err(RestoreRefusal::Unsigned),
            Err(RestoreRefusal::Unsigned),
        ),
        (
            "group",
            vec![sig(Group, group_key())],
            Err(RestoreRefusal::MapKeySignatureMissing),
            Ok(()),
        ),
        ("map key", vec![sig(MapKey, map_key())], Ok(()), Ok(())),
        (
            "both",
            vec![sig(Group, group_key()), sig(MapKey, map_key())],
            Ok(()),
            Ok(()),
        ),
        (
            "group bad, map ok",
            vec![bad(Group, group_key()), sig(MapKey, map_key())],
            Ok(()),
            Ok(()),
        ),
        (
            "group ok, map bad",
            vec![sig(Group, group_key()), bad(MapKey, map_key())],
            Err(bad_sig),
            Ok(()),
        ),
        (
            "map bad",
            vec![bad(MapKey, map_key())],
            Err(bad_sig),
            Err(bad_sig),
        ),
        (
            "group bad",
            vec![bad(Group, group_key())],
            Err(RestoreRefusal::MapKeySignatureMissing),
            Err(bad_sig),
        ),
        // A key claimed for the wrong role is a bad key.
        (
            "map role, group key",
            vec![sig(MapKey, group_key())],
            Err(bad_key),
            Err(bad_key),
        ),
        (
            "group role, map key",
            vec![sig(Group, map_key())],
            Err(RestoreRefusal::MapKeySignatureMissing),
            Err(bad_key),
        ),
        // A signature of another record does not carry over.
        (
            "map key over another record",
            vec![restore_sig(
                &restore_record(2, survivors()),
                MapKey,
                map_key(),
            )],
            Err(bad_sig),
            Err(bad_sig),
        ),
    ];
    for (name, signatures, want_strict, want_lenient) in cases {
        let mut rec = base.clone();
        rec.signatures = signatures;
        assert_eq!(
            verify_record(&rec, &strict).map(|_| ()),
            want_strict,
            "strict trust: {name}"
        );
        assert_eq!(
            verify_record(&rec, &lenient).map(|_| ()),
            want_lenient,
            "group-alone trust: {name}"
        );
    }
    // The classes: refused for want of authority is an alarm.
    assert_eq!(RestoreRefusal::Unsigned.class(), RefusalClass::Alarm);
    assert_eq!(
        RestoreRefusal::MapKeySignatureMissing.class(),
        RefusalClass::Alarm
    );
    assert_eq!(bad_sig.class(), RefusalClass::Alarm);
    assert_eq!(
        RestoreRefusal::TooManyPositions.class(),
        RefusalClass::Refuse
    );
    assert_eq!(
        RestoreRefusal::DuplicatePosition(team_log(1)).class(),
        RefusalClass::Refuse
    );
}

#[test]
fn restore_record_signed_by_map_key_reanchors_every_listed_chain() {
    let verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&verifier);
    let mut pin = device();
    let before_team_log = pin.head(&team_log(1));
    let rec = map_signed(1, survivors(), true);

    let (verified, done) = reanchor(&mut pin, &rec, &strict).expect("the map key signed it");
    // Every listed chain is SET to the record's position: back (10 to 6), where it was (4), and
    // forward (20 to 25): a restore says where the history stands.
    for head in survivors() {
        assert_eq!(pin.head(&head.chain), Some(head), "{}", head.chain);
    }
    assert_eq!(pin.restore_epoch(), epoch(1));
    assert_eq!(pin.len(), 3);
    // What moved, in the order of the record, with where each came from.
    assert_eq!(done.epoch, epoch(1));
    assert_eq!(
        done.moved,
        vec![
            Moved {
                chain: team_log(1),
                before: before_team_log,
                after: head_at(team_log(1), 6, 0)
            },
            Moved {
                chain: space(1),
                before: Some(head_at(space(1), 4, 0)),
                after: head_at(space(1), 4, 0)
            },
            Moved {
                chain: space(2),
                before: Some(head_at(space(2), 20, 0)),
                after: head_at(space(2), 25, 0)
            },
        ]
    );
    // The witness reads like the record.
    assert_eq!(verified.epoch(), epoch(1));
    assert_eq!(verified.at(), rec.at);
    assert_eq!(verified.positions(), survivors().as_slice());
    assert_eq!(
        verified.position(&space(2)),
        Some(&head_at(space(2), 25, 0))
    );
    assert!(
        verified.covers(&team_log(1)) && verified.covers(&space(1)) && verified.covers(&space(2))
    );
    assert!(!verified.covers(&space(3)));
    assert_eq!(verified.position(&space(3)), None);
    // A position set by a record has no signed head to hand to a peer, until a signed head of it
    // arrives (the same head: unchanged, but it now carries its signature).
    assert_eq!(pin.signed_head(&team_log(1)), None);
    let verifier = FakeHeadVerifier::group();
    assert_eq!(
        pin.offer(group_head_in(team_log(1), 6, 0, 1, 60), &verifier),
        PinVerdict::Unchanged
    );
    assert!(pin.signed_head(&team_log(1)).is_some());
    // Going on from the anchor: newer heads advance, older ones are refused again.
    assert_eq!(
        pin.offer(group_head_in(team_log(1), 7, 0, 1, 70), &verifier),
        PinVerdict::Advanced
    );
    assert_eq!(
        pin.offer(group_head_in(team_log(1), 5, 0, 1, 50), &verifier),
        PinVerdict::RefusedOlder { newest: seq(7) }
    );
    // Another hash at the anchored position of another chain is a fork.
    assert!(matches!(
        pin.offer(group_head_in(space(1), 4, 1, 1, 40), &verifier),
        PinVerdict::Fork(_)
    ));
}

#[test]
fn replayed_restore_record_refused() {
    let verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&verifier);
    let mut pin = device();
    let rec = map_signed(3, survivors(), false);
    reanchor(&mut pin, &rec, &strict).expect("the first time");
    assert_eq!(pin.restore_epoch(), epoch(3));
    let after = snapshot(&pin);

    // The same record again: its epoch is not above the accepted one.
    let again = reanchor(&mut pin, &rec, &strict).expect_err("a replay");
    assert_eq!(
        again,
        RestoreRefusal::Replayed {
            record: epoch(3),
            pinned: epoch(3)
        }
    );
    assert_eq!(
        again.class(),
        RefusalClass::Refuse,
        "the same epoch again is no attack"
    );
    assert_eq!(snapshot(&pin), after);
    // An older record, validly signed by the map key: a replay of an old announcement. Alarmed.
    for old in [2u64, 1, 0] {
        let old_rec = map_signed(old, vec![head_at(team_log(1), 2, 0)], false);
        let refusal = reanchor(&mut pin, &old_rec, &strict).expect_err("older");
        assert_eq!(
            refusal,
            RestoreRefusal::Replayed {
                record: epoch(old),
                pinned: epoch(3)
            }
        );
        assert_eq!(refusal.class(), RefusalClass::Alarm, "epoch {old}");
        assert_eq!(snapshot(&pin), after, "an old record moved the pin");
        assert!(
            rollback_alarm(&pin, &old_rec).is_some(),
            "it would have moved the team log back"
        );
    }
    // A later epoch is followed.
    let later = map_signed(4, vec![head_at(team_log(1), 5, 0)], false);
    reanchor(&mut pin, &later, &strict).expect("a newer epoch");
    assert_eq!(pin.restore_epoch(), epoch(4));
    assert_eq!(pin.seq(&team_log(1)), Some(seq(5)));

    // A device born after a restore starts at its epoch: that restore's record is a replay for it.
    let mut born = HeadPin::starting_at(epoch(3));
    assert_eq!(
        reanchor(&mut born, &rec, &strict).expect_err("born after it"),
        RestoreRefusal::Replayed {
            record: epoch(3),
            pinned: epoch(3)
        }
    );
    assert!(born.is_empty());
    // One epoch above is followed, even by a device with no pins at all.
    let next = map_signed(4, vec![head_at(team_log(1), 5, 0)], false);
    let (_, done) = reanchor(&mut born, &next, &strict).expect("one above");
    assert_eq!(done.moved.len(), 1);
    assert_eq!(done.moved[0].before, None);
    assert_eq!(born.restore_epoch(), epoch(4));
}

#[test]
fn restore_older_than_newest_head_without_record_stays_refused_as_rollback() {
    // S15: a head older than the newest this device has seen is refused unless a signed restore
    // announcement raised the restore epoch (V3).
    let verifier = FakeHeadVerifier::group();
    let restore_verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&restore_verifier);
    let mut pin = device();
    let chain = team_log(1);
    // Without any record: refused, whatever is offered, however often.
    for older in [9u64, 8, 7, 6, 0] {
        assert_eq!(
            pin.offer(group_head(chain, older, 0), &verifier),
            PinVerdict::RefusedOlder { newest: seq(10) }
        );
    }
    assert_eq!(pin.seq(&chain), Some(seq(10)));
    // With a record that is not the authority's (the group alone under the strict trust): refused,
    // and still a rollback: the pin stays and the alarm names the positions.
    let unauthorised = group_signed(1, vec![head_at(chain, 7, 0)]);
    assert_eq!(
        reanchor(&mut pin, &unauthorised, &strict).expect_err("not the map key"),
        RestoreRefusal::MapKeySignatureMissing
    );
    assert_eq!(pin.seq(&chain), Some(seq(10)));
    assert_eq!(pin.restore_epoch(), epoch(0));
    assert_eq!(
        rollback_alarm(&pin, &unauthorised),
        Some(Alarm::Rollback {
            chain,
            pinned: seq(10),
            offered: seq(7)
        })
    );
    assert_eq!(
        pin.offer(group_head(chain, 7, 0), &verifier),
        PinVerdict::RefusedOlder { newest: seq(10) },
        "the head is still refused after a refused record"
    );
    // With the map key's record the way back is open: exactly to the survivors, and no further.
    let record = map_signed(1, vec![head_at(chain, 7, 0)], false);
    reanchor(&mut pin, &record, &strict).expect("the map key");
    assert_eq!(pin.seq(&chain), Some(seq(7)));
    assert_eq!(
        pin.offer(group_head_in(chain, 6, 0, 1, 60), &verifier),
        PinVerdict::RefusedOlder { newest: seq(7) },
        "below the survivor is still refused"
    );
    assert_eq!(
        pin.offer(group_head_in(chain, 7, 0, 1, 70), &verifier),
        PinVerdict::Unchanged
    );
    assert_eq!(
        pin.offer(group_head_in(chain, 8, 0, 1, 80), &verifier),
        PinVerdict::Advanced
    );
    // No alarm for a record that moves nothing back.
    assert_eq!(
        rollback_alarm(&pin, &restore_record(9, vec![head_at(chain, 20, 0)])),
        None
    );
    assert_eq!(
        rollback_alarm(&pin, &restore_record(9, vec![head_at(team_log(9), 1, 0)])),
        None
    );
    assert_eq!(rollback_alarm(&pin, &restore_record(9, Vec::new())), None);
}

#[test]
fn reanchor_returns_the_positions_the_device_must_send_again() {
    let verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&verifier);
    let mut pin = device();
    // Back from 10 to 6 on the team log (positions 7 to 10 are beyond the survivors); where it was
    // on the space 1; the space 2 ahead; the space 4 never pinned; the space 3 back to 1 from a
    // pin that the record lists.
    let extra = {
        let verifier = FakeHeadVerifier::group();
        assert_eq!(
            pin.offer(group_head(space(3), 9, 0), &verifier),
            PinVerdict::Advanced
        );
        space(3)
    };
    let mut positions = survivors();
    positions.push(head_at(space(4), 2, 0));
    positions.push(head_at(extra, 1, 0));
    let rec = map_signed(1, positions, false);
    let (_, done) = reanchor(&mut pin, &rec, &strict).expect("the map key");

    assert_eq!(
        done.resend_ranges(),
        vec![
            ResendRange {
                chain: team_log(1),
                from: seq(7),
                through: seq(10)
            },
            ResendRange {
                chain: space(3),
                from: seq(2),
                through: seq(9)
            },
        ]
    );
    // Per chain: ahead of the survivor, level with it, behind it, not pinned before.
    assert_eq!(
        done.moved[0].resend().map(|r| (r.from, r.through)),
        Some((seq(7), seq(10)))
    );
    assert_eq!(done.moved[1].resend(), None, "level with the survivor");
    assert_eq!(done.moved[2].resend(), None, "behind the survivor");
    assert_eq!(done.moved[3].resend(), None, "pinned nothing before");
    // Nothing to send again for a record that moves no pin back.
    let mut quiet = device();
    let rec = map_signed(
        1,
        vec![head_at(team_log(1), 10, 0), head_at(space(1), 5, 0)],
        false,
    );
    let (_, done) = reanchor(&mut quiet, &rec, &strict).expect("level or ahead");
    assert!(done.resend_ranges().is_empty());
    // The largest position has no position after it to send again.
    let mut top = HeadPin::new();
    let verifier_h = FakeHeadVerifier::group();
    assert_eq!(
        top.offer(group_head_in(space(5), u64::MAX, 0, 0, 10), &verifier_h),
        PinVerdict::Advanced
    );
    let rec = map_signed(1, vec![head_at(space(5), 7, 0)], false);
    let (_, done) = reanchor(&mut top, &rec, &strict).expect("back");
    assert_eq!(
        done.resend_ranges(),
        vec![ResendRange {
            chain: space(5),
            from: seq(8),
            through: seq(u64::MAX)
        }]
    );
}

#[test]
fn reanchor_never_moves_a_chain_the_record_does_not_list() {
    let verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&verifier);
    let mut pin = device();
    let unlisted = [space(1), space(2)];
    let before: Vec<_> = unlisted
        .iter()
        .map(|c| (pin.head(c), pin.signed_head(c).cloned()))
        .collect();
    // The record lists the team log only (and a chain the device never pinned).
    let rec = map_signed(
        1,
        vec![head_at(team_log(1), 6, 0), head_at(space(9), 3, 0)],
        false,
    );
    let (verified, done) = reanchor(&mut pin, &rec, &strict).expect("the map key");
    assert_eq!(done.moved.len(), 2);
    assert_eq!(pin.seq(&team_log(1)), Some(seq(6)));
    assert_eq!(pin.seq(&space(9)), Some(seq(3)));
    // The unlisted chains: same head, same signed copy, untouched.
    let after: Vec<_> = unlisted
        .iter()
        .map(|c| (pin.head(c), pin.signed_head(c).cloned()))
        .collect();
    assert_eq!(after, before);
    assert!(after.iter().all(|(_, signed)| signed.is_some()));
    assert!(!verified.covers(&space(1)) && !verified.covers(&space(2)));
    assert_eq!(pin.len(), 4);
    // The epoch is the device's, not a chain's: it moved once.
    assert_eq!(pin.restore_epoch(), epoch(1));
}

#[test]
fn a_verified_restore_is_a_witness_and_apply_checks_the_epoch_of_each_pin() {
    // No Clone, no Default: the only way to hold one is `verify_record`.
    <VerifiedRestore as AmbiguousIfClone<_>>::check();
    <VerifiedRestore as AmbiguousIfDefault<_>>::check();

    let verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&verifier);
    let rec = map_signed(2, survivors(), false);
    let verified = verify_record(&rec, &strict).expect("sound and signed by the map key");
    assert_eq!(verified.epoch(), epoch(2));
    assert!(format!("{verified:?}").contains("epoch: RestoreEpoch(2)"));
    // Verifying changes no pin. Applying to two devices works for each; to the same one twice does
    // not.
    let mut one = device();
    let mut two = device();
    assert_eq!(one.restore_epoch(), epoch(0));
    assert!(verified.apply(&mut one).is_ok());
    assert!(verified.apply(&mut two).is_ok());
    assert_eq!(
        verified.apply(&mut one).expect_err("twice"),
        RestoreRefusal::Replayed {
            record: epoch(2),
            pinned: epoch(2)
        }
    );
    // A device that already accepted a later epoch refuses it.
    let mut ahead = HeadPin::starting_at(epoch(5));
    assert_eq!(
        verified.apply(&mut ahead).expect_err("older"),
        RestoreRefusal::Replayed {
            record: epoch(2),
            pinned: epoch(5)
        }
    );
}

#[test]
fn a_malformed_record_is_refused_before_its_signatures_are_looked_at() {
    let verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&verifier);
    // The same chain twice, even with the same head, is ambiguous.
    let twice = map_signed(
        1,
        vec![head_at(team_log(1), 6, 0), head_at(team_log(1), 6, 0)],
        false,
    );
    assert_eq!(
        verify_record(&twice, &strict).expect_err("twice"),
        RestoreRefusal::DuplicatePosition(team_log(1))
    );
    // It is told even when the record is unsigned: the structure comes first.
    let mut unsigned = twice.clone();
    unsigned.signatures.clear();
    assert_eq!(
        verify_record(&unsigned, &strict).expect_err("twice"),
        RestoreRefusal::DuplicatePosition(team_log(1))
    );
    // Two chains of different kinds with the same id bytes are two chains.
    let both = map_signed(
        1,
        vec![
            head_at(
                ChainId {
                    kind: ChainKind::TeamLog,
                    id: [4; 16],
                },
                1,
                0,
            ),
            head_at(
                ChainId {
                    kind: ChainKind::Space,
                    id: [4; 16],
                },
                1,
                0,
            ),
        ],
        false,
    );
    assert!(verify_record(&both, &strict).is_ok());

    // More chains than a device follows.
    let many: Vec<Head> = (0..=MAX_PINNED_CHAINS as u32)
        .map(|i| {
            let mut id = [0u8; 16];
            id[..4].copy_from_slice(&i.to_be_bytes());
            Head {
                chain: ChainId {
                    kind: ChainKind::Space,
                    id,
                },
                seq: seq(1),
                hash: Hash32::ZERO,
            }
        })
        .collect();
    assert_eq!(many.len(), MAX_PINNED_CHAINS + 1);
    let huge = RestoreRecord {
        epoch: epoch(1),
        at: at(1),
        positions: many,
        signatures: Vec::new(),
    };
    assert_eq!(
        verify_record(&huge, &strict).expect_err("too many"),
        RestoreRefusal::TooManyPositions
    );

    // A pin with a smaller bound than the record needs: the whole record is refused, nothing moves.
    let mut small = HeadPin::with_limit(2);
    let hv = FakeHeadVerifier::group();
    assert_eq!(
        small.offer(group_head(team_log(1), 5, 0), &hv),
        PinVerdict::Advanced
    );
    let before = snapshot(&small);
    let rec = map_signed(
        1,
        vec![
            head_at(team_log(1), 3, 0),
            head_at(space(1), 1, 0),
            head_at(space(2), 1, 0),
        ],
        false,
    );
    assert_eq!(
        reanchor(&mut small, &rec, &strict).expect_err("does not fit"),
        RestoreRefusal::TooManyPositions
    );
    assert_eq!(snapshot(&small), before, "an atomic refusal");
    // The record's epoch is not consumed by a refusal: the same record fits a larger pin.
    let mut roomy = HeadPin::with_limit(3);
    assert!(reanchor(&mut roomy, &rec, &strict).is_ok());
    // Messages.
    assert!(RestoreRefusal::Replayed {
        record: epoch(1),
        pinned: epoch(2)
    }
    .to_string()
    .contains("not above"));
    assert!(RestoreRefusal::DuplicatePosition(team_log(1))
        .to_string()
        .contains("team_log:"));
}

/// X-DELTA-W1S17A-3, the attack on the way forward: the restore moved the team log back from 10 to 6
/// and raised the floor to the epoch 1. The server replays the head of the lost timeline: validly
/// signed by the group BEFORE the restore (epoch 0), at position 10, above the survivor. Without the
/// floor it would advance the pin again (the old timeline's entries 7 to 10 chain correctly from the
/// survivor); with it, it is refused as an alarm, whatever its position.
#[test]
fn a_head_signed_before_the_restore_cannot_advance_the_pin() {
    let verifier = FakeHeadVerifier::group();
    let restore_verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&restore_verifier);
    let chain = team_log(1);
    let mut pin = device();
    // The head of the lost timeline, signed before the restore: the one the server kept.
    let lost = group_head(chain, 10, 0);
    assert_eq!(lost.epoch, epoch(0));

    let rec = map_signed(1, vec![head_at(chain, 6, 0)], false);
    reanchor(&mut pin, &rec, &strict).expect("the map key signed it");
    assert_eq!(pin.seq(&chain), Some(seq(6)));
    assert_eq!(pin.restore_epoch(), epoch(1));

    // The replay: position 10 is above the survivor 6, the signature is the group's and good.
    let refusal = HeadRefusal::BelowRestoreEpoch {
        head: epoch(0),
        floor: epoch(1),
    };
    let verdict = pin.offer(lost.clone(), &verifier);
    assert_eq!(verdict, PinVerdict::Refused(refusal));
    assert_eq!(refusal.class(), RefusalClass::Alarm);
    assert_eq!(
        verdict.alarm(&chain),
        Some(Alarm::BelowRestoreEpoch {
            chain,
            got: epoch(0),
            floor: epoch(1)
        })
    );
    assert_eq!(pin.seq(&chain), Some(seq(6)), "the pin did not move");
    assert_eq!(pin.head(&chain), Some(head_at(chain, 6, 0)));
    // Not a head of the lost timeline at any position, in any variant, advances or even pins.
    for position in [0u64, 3, 6, 7, 10, 11, 500] {
        for variant in [0u8, 1] {
            assert_eq!(
                pin.offer(group_head(chain, position, variant), &verifier),
                PinVerdict::Refused(refusal),
                "position {position}, variant {variant}"
            );
        }
    }
    assert_eq!(pin.seq(&chain), Some(seq(6)));
    // The restored timeline goes on from the survivor: heads of the epoch 1 advance the pin.
    assert_eq!(
        pin.offer(group_head_in(chain, 7, 0, 1, 7_000), &verifier),
        PinVerdict::Advanced
    );
    assert_eq!(pin.seq(&chain), Some(seq(7)));
    // A second restore raises the floor again: the heads of the first restored timeline (epoch 1)
    // are now the lost ones.
    // (Stamped after the head the device pinned from the restored timeline, signed at 7,000.)
    let second = map_signed_at(2, 8_000, vec![head_at(chain, 5, 0)]);
    reanchor(&mut pin, &second, &strict).expect("a second restore");
    assert_eq!(pin.restore_epoch(), epoch(2));
    assert_eq!(pin.seq(&chain), Some(seq(5)));
    assert_eq!(
        pin.offer(group_head_in(chain, 7, 0, 1, 7_000), &verifier),
        PinVerdict::Refused(HeadRefusal::BelowRestoreEpoch {
            head: epoch(1),
            floor: epoch(2)
        })
    );
    assert_eq!(
        pin.offer(group_head_in(chain, 6, 0, 2, 9_000), &verifier),
        PinVerdict::Advanced
    );
    // A device born after the first restore refuses the lost timeline from its first head.
    let mut born = HeadPin::starting_at(epoch(1));
    assert_eq!(
        born.offer(lost, &verifier),
        PinVerdict::Refused(refusal),
        "a device born after the restore never pins a head of the lost timeline"
    );
    assert!(born.is_empty());
}

/// X-DELTA-W1S17A-3, the attack on the way back: a record that arrives late (the device missed it) is
/// not applied to a chain whose pinned signed head was signed AFTER the record: the device already
/// follows the restored timeline there, and the record's survivors are behind that progress. The
/// other chains of the record are re-anchored, and the floor is raised either way.
#[test]
fn a_late_record_cannot_reanchor_behind_a_newer_head() {
    let verifier = FakeHeadVerifier::group();
    let restore_verifier = FakeRestoreVerifier;
    let strict = StrictTrust::new(&restore_verifier);
    // A device that pinned the team log at 10 (signed at 100), the space 1 at 4 (40) and the space 2
    // at 20 (200) before the restore, then missed the record of epoch 1 (stamped 500) and met a head
    // of the restored timeline of the team log from a peer: position 12, epoch 1, signed at 600.
    let mut pin = device();
    assert_eq!(
        pin.offer(group_head_in(team_log(1), 12, 0, 1, 600), &verifier),
        PinVerdict::Advanced,
        "a head of a restore the device has not been announced yet is an ordinary head"
    );
    assert_eq!(
        pin.restore_epoch(),
        epoch(0),
        "the floor moves only with a record"
    );

    // The late record lists the survivors of all three chains.
    let rec = map_signed_at(
        1,
        500,
        vec![
            head_at(team_log(1), 6, 0),
            head_at(space(1), 3, 0),
            head_at(space(2), 18, 0),
        ],
    );
    let (verified, done) =
        reanchor(&mut pin, &rec, &strict).expect("a record signed by the map key");

    // The team log is KEPT where it is: its pinned head was signed at 600, after the record at 500.
    assert_eq!(
        done.kept,
        vec![Kept {
            chain: team_log(1),
            pinned: head_at(team_log(1), 12, 0),
            listed: head_at(team_log(1), 6, 0)
        }]
    );
    assert_eq!(pin.head(&team_log(1)), Some(head_at(team_log(1), 12, 0)));
    assert!(
        pin.signed_head(&team_log(1)).is_some(),
        "its signed head is still held"
    );
    // The other two were signed before the record (40 and 200): re-anchored to the survivors.
    assert_eq!(done.moved.len(), 2);
    assert_eq!(pin.head(&space(1)), Some(head_at(space(1), 3, 0)));
    assert_eq!(pin.head(&space(2)), Some(head_at(space(2), 18, 0)));
    assert_eq!(
        done.resend_ranges(),
        vec![
            ResendRange {
                chain: space(1),
                from: seq(4),
                through: seq(4)
            },
            ResendRange {
                chain: space(2),
                from: seq(19),
                through: seq(20)
            },
        ]
    );
    // The floor is the record's epoch in any case, and the witness still lists all three chains.
    assert_eq!(pin.restore_epoch(), epoch(1));
    assert_eq!(done.epoch, epoch(1));
    assert!(
        verified.covers(&team_log(1)) && verified.covers(&space(1)) && verified.covers(&space(2))
    );
    // Heads of the lost timeline are refused from now on, for the chain that was kept too.
    assert_eq!(
        pin.offer(group_head_in(team_log(1), 13, 0, 0, 130), &verifier),
        PinVerdict::Refused(HeadRefusal::BelowRestoreEpoch {
            head: epoch(0),
            floor: epoch(1)
        })
    );

    // The comparison is strict: a pinned head signed AT the record's stamp is not after it.
    let mut level = device();
    assert_eq!(
        level.offer(group_head_in(team_log(1), 12, 0, 0, 500), &verifier),
        PinVerdict::Advanced
    );
    let rec = map_signed_at(1, 500, vec![head_at(team_log(1), 6, 0)]);
    let (_, done) = reanchor(&mut level, &rec, &strict).expect("the map key");
    assert!(done.kept.is_empty());
    assert_eq!(level.seq(&team_log(1)), Some(seq(6)));
    // A pin that holds a position set by an earlier record has no signed head to compare: a later
    // record (epoch 2) re-anchors it.
    let second = map_signed_at(2, 400, vec![head_at(team_log(1), 4, 0)]);
    let (_, done) = reanchor(&mut level, &second, &strict).expect("a second record");
    assert!(done.kept.is_empty());
    assert_eq!(level.seq(&team_log(1)), Some(seq(4)));
    // Nothing is kept for a chain the device pinned nothing of.
    let mut empty = HeadPin::new();
    let rec = map_signed_at(1, 500, vec![head_at(team_log(1), 6, 0)]);
    let (_, done) = reanchor(&mut empty, &rec, &strict).expect("the map key");
    assert!(done.kept.is_empty());
    assert_eq!(done.moved.len(), 1);
}
