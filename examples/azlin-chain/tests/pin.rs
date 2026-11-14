//! The head a device pins and never lets go backwards (acceptance check 6): newer and same accepted,
//! older refused, a fork alarms and keeps the old head, unsigned and wrongly signed heads are
//! ignored, and the pinned position is the maximum of the validly signed heads offered, over 10,000
//! seeded interleavings.
//!
//! Implements planning/architecture.md section 5.9 ("each device remembers the newest head it has
//! seen and refuses an older one, so history cannot be rolled back"; "two different entries at the
//! same position are a fork, refused and alarmed") and section 5.13 ("a head never goes
//! backwards").

use std::collections::BTreeMap;

use azlin_chain::{
    require_signed, Alarm, ChainId, HeadPin, HeadRefusal, PinVerdict, RefusalClass, SignedHead,
};
use azlin_seams::sim::Rng;
use azlin_seams::{FakeScheme, RestoreEpoch};

mod common;
use common::{
    at, epoch, group_head, group_head_in, group_key, head_at, key, seq, sign_head, space, team_log,
    FakeHeadVerifier, UnsupportedHeadVerifier,
};

#[test]
fn head_pin_accepts_newer_and_same_refuses_older() {
    let verifier = FakeHeadVerifier::group();
    let chain = team_log(1);
    let mut pin = HeadPin::new();
    assert!(pin.is_empty());
    assert_eq!(pin.seq(&chain), None);
    assert_eq!(pin.head(&chain), None);

    // The first head of a chain is pinned.
    let h3 = group_head(chain, 3, 0);
    assert_eq!(pin.offer(h3.clone(), &verifier), PinVerdict::Advanced);
    assert_eq!(pin.seq(&chain), Some(seq(3)));
    assert_eq!(pin.head(&chain), Some(h3.head));
    assert_eq!(pin.signed_head(&chain), Some(&h3));
    // A newer one moves it up.
    let h5 = group_head(chain, 5, 0);
    assert_eq!(pin.offer(h5.clone(), &verifier), PinVerdict::Advanced);
    assert_eq!(pin.seq(&chain), Some(seq(5)));
    // The same one again: accepted, unchanged. Re-signed at another stamp: the same head.
    assert_eq!(pin.offer(h5.clone(), &verifier), PinVerdict::Unchanged);
    let resigned = sign_head(h5.head, at(999), group_key());
    assert_ne!(resigned.sig, h5.sig);
    assert_eq!(pin.offer(resigned, &verifier), PinVerdict::Unchanged);
    assert_eq!(
        pin.signed_head(&chain),
        Some(&h5),
        "the pinned copy is unchanged"
    );
    // Older ones are refused, whatever their hash, and the pin stays.
    for older in [4u64, 3, 2, 1, 0] {
        for variant in [0u8, 1] {
            assert_eq!(
                pin.offer(group_head(chain, older, variant), &verifier),
                PinVerdict::RefusedOlder { newest: seq(5) },
                "position {older}"
            );
        }
    }
    assert_eq!(pin.seq(&chain), Some(seq(5)));
    assert_eq!(pin.head(&chain), Some(h5.head));
    assert_eq!(pin.signed_head(&chain), Some(&h5));
    // Chains are independent: a head of another chain starts its own pin, and a team log is not a
    // space of the same id bytes.
    let other = space(1);
    assert_eq!(
        pin.offer(group_head(other, 2, 0), &verifier),
        PinVerdict::Advanced
    );
    assert_eq!(pin.seq(&other), Some(seq(2)));
    assert_eq!(pin.seq(&chain), Some(seq(5)));
    assert_eq!(pin.len(), 2);
    assert_eq!(pin.restore_epoch(), RestoreEpoch::ZERO);
    // Nothing of this offered an alarm.
    assert_eq!(PinVerdict::Advanced.alarm(&chain), None);
    assert_eq!(PinVerdict::Unchanged.alarm(&chain), None);
    assert_eq!(
        PinVerdict::RefusedOlder { newest: seq(1) }.alarm(&chain),
        None
    );
    // The signed heads are handed to a peer in chain order.
    let heads: Vec<&SignedHead> = pin.signed_heads().collect();
    assert_eq!(heads.len(), 2);
    assert_eq!(heads[0].head.chain, chain);
    assert_eq!(heads[1].head.chain, other);
}

#[test]
fn head_pin_fork_at_same_seq_alarms_and_keeps_the_old_head() {
    let verifier = FakeHeadVerifier::group();
    let chain = team_log(1);
    let mut pin = HeadPin::new();
    let original = group_head(chain, 5, 0);
    assert_eq!(pin.offer(original.clone(), &verifier), PinVerdict::Advanced);

    // Another entry at the pinned position, validly signed: a fork, alarmed, refused.
    let forked = group_head(chain, 5, 1);
    assert_ne!(forked.head.hash, original.head.hash);
    let verdict = pin.offer(forked.clone(), &verifier);
    let alarm = Alarm::Fork {
        chain,
        seq: seq(5),
        have: original.head.hash,
        got: forked.head.hash,
    };
    assert_eq!(verdict, PinVerdict::Fork(alarm));
    assert_eq!(verdict.alarm(&chain), Some(alarm));
    assert_eq!(alarm.chain(), chain);
    // The pin KEEPS the old head: position, hash and signed copy.
    assert_eq!(pin.head(&chain), Some(original.head));
    assert_eq!(pin.signed_head(&chain), Some(&original));
    // And again: every time, the same verdict.
    assert_eq!(pin.offer(forked, &verifier), PinVerdict::Fork(alarm));
    // The old head is still the same head.
    assert_eq!(
        pin.offer(original.clone(), &verifier),
        PinVerdict::Unchanged
    );
    // A newer head still moves on (the pin does not freeze after an alarm).
    assert_eq!(
        pin.offer(group_head(chain, 6, 1), &verifier),
        PinVerdict::Advanced
    );
    assert_eq!(pin.seq(&chain), Some(seq(6)));
}

#[test]
fn head_pin_rejects_unsigned_or_wrongly_signed_head() {
    let verifier = FakeHeadVerifier::group();
    let chain = team_log(1);
    let good = group_head(chain, 4, 0);

    // On an empty pin: nothing is pinned by a head that does not verify.
    let mut pin = HeadPin::new();
    let mut unsigned = good.clone();
    unsigned.sig = Vec::new();
    let mut flipped = good.clone();
    flipped.sig[0] ^= 1;
    let mut truncated = good.clone();
    truncated.sig.truncate(10);
    let mut moved = good.clone();
    moved.head.seq = seq(40); // altered after signing
    let mut renamed = good.clone();
    renamed.head.chain = space(1);
    let mut restamped = good.clone();
    restamped.signed_at = at(1);
    let untrusted = sign_head(good.head, at(40), key(0x99)); // a key the verifier does not know
    let other_chain = sign_head(group_head(space(9), 4, 0).head, at(40), group_key());
    let mut swapped = other_chain.clone();
    swapped.head = good.head; // another head's signature
    for bad in [
        &unsigned, &flipped, &truncated, &moved, &renamed, &restamped, &untrusted, &swapped,
    ] {
        assert_eq!(pin.offer(bad.clone(), &verifier), PinVerdict::BadSignature);
        assert!(pin.is_empty(), "a bad head pins nothing");
        assert_eq!(pin.seq(&chain), None);
    }
    assert_eq!(
        PinVerdict::BadSignature.alarm(&chain),
        None,
        "junk is no alarm"
    );

    // On a pin that holds a head: a bad head at a higher position does not move it, a bad head at
    // the same position with another hash is not a fork, a bad head below is no refusal of age.
    assert_eq!(pin.offer(good.clone(), &verifier), PinVerdict::Advanced);
    let before = pin.head(&chain);
    for position in [1u64, 4, 9] {
        let mut bad = group_head(chain, position, 1);
        bad.sig[3] ^= 0x10;
        assert_eq!(
            pin.offer(bad, &verifier),
            PinVerdict::BadSignature,
            "{position}"
        );
    }
    assert_eq!(pin.head(&chain), before);
    assert_eq!(pin.signed_head(&chain), Some(&good));

    // A verifier that cannot check the algorithm passes nothing.
    let mut fresh = HeadPin::new();
    assert_eq!(
        fresh.offer(good.clone(), &UnsupportedHeadVerifier),
        PinVerdict::BadSignature
    );
    assert!(fresh.is_empty());
    // The fake signature of the right key over the right bytes is what verifies.
    let again = sign_head(good.head, good.signed_at, group_key());
    assert_eq!(
        again.sig,
        FakeScheme::sign(
            &group_key(),
            &common::head_preimage(&good.head, good.epoch, good.signed_at)
        )
    );
    assert_eq!(fresh.offer(again, &verifier), PinVerdict::Advanced);
}

#[test]
fn head_pin_is_bounded_and_starts_at_a_restore_epoch() {
    let verifier = FakeHeadVerifier::group();
    let mut pin = HeadPin::with_limit(2);
    assert_eq!(pin.limit(), 2);
    assert_eq!(
        pin.offer(group_head(team_log(1), 1, 0), &verifier),
        PinVerdict::Advanced
    );
    assert_eq!(
        pin.offer(group_head(space(1), 1, 0), &verifier),
        PinVerdict::Advanced
    );
    // A third chain does not fit; the two it follows still move; a bad head is a bad head first.
    assert_eq!(
        pin.offer(group_head(space(2), 1, 0), &verifier),
        PinVerdict::Full
    );
    assert_eq!(pin.len(), 2);
    assert_eq!(
        pin.offer(group_head(space(1), 2, 0), &verifier),
        PinVerdict::Advanced
    );
    let mut bad = group_head(space(3), 1, 0);
    bad.sig.clear();
    assert_eq!(pin.offer(bad, &verifier), PinVerdict::BadSignature);
    assert_eq!(azlin_chain::MAX_PINNED_CHAINS, 100_000);
    assert_eq!(HeadPin::default().limit(), azlin_chain::MAX_PINNED_CHAINS);

    // A device born after a restore starts at its epoch.
    assert_eq!(HeadPin::starting_at(epoch(7)).restore_epoch(), epoch(7));
    assert_eq!(HeadPin::new().restore_epoch(), RestoreEpoch::ZERO);
    assert!(HeadPin::starting_at(epoch(7)).is_empty());
}

#[test]
fn require_signed_names_the_head_a_server_must_send() {
    let chain = team_log(1);
    let head = group_head(chain, 3, 0);
    assert_eq!(require_signed(&chain, Some(&head)), Ok(&head));
    let alarm = Alarm::UnsignedHead { chain };
    // Missing, signed with no bytes, or a head of another chain: no signed head of this chain.
    assert_eq!(require_signed(&chain, None), Err(alarm));
    let mut empty = head.clone();
    empty.sig.clear();
    assert_eq!(require_signed(&chain, Some(&empty)), Err(alarm));
    assert_eq!(
        require_signed(&space(1), Some(&head)),
        Err(Alarm::UnsignedHead { chain: space(1) })
    );
    assert_eq!(alarm.chain(), chain);
    assert!(alarm.to_string().contains("unsigned head"), "{alarm}");
}

/// X-DELTA-W1S17A-3, the floor: a head signed in a restore epoch below the newest restore the device
/// accepted is refused as an alarm whatever its position; a head of the floor's epoch or a later one
/// is an ordinary head (a later one is a restore the device has not been announced yet: the floor does
/// not move until the record arrives).
#[test]
fn a_head_below_the_restore_epoch_is_refused_as_an_alarm_whatever_its_position() {
    let verifier = FakeHeadVerifier::group();
    let chain = team_log(1);
    let mut pin = HeadPin::starting_at(epoch(2));
    let refusal = HeadRefusal::BelowRestoreEpoch {
        head: epoch(1),
        floor: epoch(2),
    };
    let alarm = Alarm::BelowRestoreEpoch {
        chain,
        got: epoch(1),
        floor: epoch(2),
    };
    // Even the first head of a chain, at any position, signed in epoch 0 or 1: refused, and the pin
    // stays empty.
    for position in [0u64, 5, 1_000, u64::MAX / 20] {
        let verdict = pin.offer(group_head_in(chain, position, 0, 1, 10), &verifier);
        assert_eq!(verdict, PinVerdict::Refused(refusal), "position {position}");
        assert_eq!(verdict.alarm(&chain), Some(alarm));
        assert!(pin.is_empty());
    }
    assert_eq!(refusal.class(), RefusalClass::Alarm);
    assert_eq!(refusal.alarm(chain), alarm);
    assert_eq!(alarm.chain(), chain);
    assert!(alarm.to_string().contains("restore epoch 1"), "{alarm}");
    assert!(alarm.to_string().contains("epoch 2"), "{alarm}");
    assert!(
        refusal.to_string().contains("below the epoch 2"),
        "{refusal}"
    );
    assert_eq!(
        pin.offer(group_head_in(chain, 4, 0, 0, 10), &verifier),
        PinVerdict::Refused(HeadRefusal::BelowRestoreEpoch {
            head: epoch(0),
            floor: epoch(2)
        })
    );
    // A bad signature is told before the epoch: junk is junk.
    let mut junk = group_head_in(chain, 4, 0, 1, 10);
    junk.sig.clear();
    assert_eq!(pin.offer(junk, &verifier), PinVerdict::BadSignature);

    // The floor's own epoch is an ordinary head; so is a later one, and the floor stays.
    assert_eq!(
        pin.offer(group_head_in(chain, 5, 0, 2, 50), &verifier),
        PinVerdict::Advanced
    );
    assert_eq!(
        pin.offer(group_head_in(chain, 8, 0, 3, 80), &verifier),
        PinVerdict::Advanced,
        "a head of a restore the device has not been announced yet"
    );
    assert_eq!(
        pin.restore_epoch(),
        epoch(2),
        "only a record moves the floor"
    );
    assert_eq!(pin.seq(&chain), Some(seq(8)));
    // The epoch is judged before the position: an older head of the lost timeline is a refusal for
    // its epoch (an alarm), not just 'older'.
    assert_eq!(
        pin.offer(group_head_in(chain, 3, 0, 1, 30), &verifier),
        PinVerdict::Refused(HeadRefusal::BelowRestoreEpoch {
            head: epoch(1),
            floor: epoch(2)
        })
    );
    assert_eq!(
        pin.offer(group_head_in(chain, 3, 0, 2, 30), &verifier),
        PinVerdict::RefusedOlder { newest: seq(8) },
        "an older head of the floor's own epoch is only older"
    );
    assert_eq!(pin.seq(&chain), Some(seq(8)));
}

/// The property: whatever interleaving of heads of a few chains is offered (some validly signed, some
/// not, some at a position another hash already holds), the pinned position of a chain is the
/// maximum position of the validly signed heads offered for it, it never decreases, a head that
/// does not verify changes nothing, and the pinned head is one the offers made.
#[test]
fn head_pin_never_goes_backwards_over_10000_seeded_interleavings() {
    let verifier = FakeHeadVerifier::group();
    let chains: [ChainId; 3] = [team_log(1), space(1), space(2)];
    let mut offers = 0usize;
    let mut refusals = 0usize;
    let mut forks = 0usize;
    for interleaving in 0..10_000u64 {
        let mut rng = Rng::new(0x9100_0000 + interleaving);
        // A device that accepted the restore of epoch 0, 1 or 2 (the floor), offered heads signed in
        // the epochs 0 to 3: those below the floor are the lost timeline.
        let floor = rng.below(3).unwrap_or(0);
        let mut pin = HeadPin::starting_at(epoch(floor));
        // The model: per chain, the maximum position of the validly signed heads offered, and the
        // hashes offered validly at it.
        let mut model: BTreeMap<ChainId, (u64, Vec<azlin_seams::Hash32>)> = BTreeMap::new();
        let mut last_seen: BTreeMap<ChainId, u64> = BTreeMap::new();
        let count = 5 + rng.below(16).unwrap_or(0);
        for _ in 0..count {
            let chain = *rng.pick(&chains).unwrap_or(&chains[0]);
            let position = rng.below(25).unwrap_or(0);
            let variant = u8::from(rng.chance(1, 4));
            let head_epoch = rng.below(4).unwrap_or(0);
            let mut head = group_head_in(chain, position, variant, head_epoch, position * 10);
            let valid = !rng.chance(1, 5);
            if !valid {
                match rng.below(3).unwrap_or(0) {
                    0 => head.sig.clear(),
                    1 => head.sig[0] ^= 0x80,
                    _ => head.head.seq = seq(position + 1 + rng.below(5).unwrap_or(0)),
                }
            }
            let before = pin.head(&chain);
            let verdict = pin.offer(head.clone(), &verifier);
            offers += 1;
            if !valid {
                assert_eq!(
                    verdict,
                    PinVerdict::BadSignature,
                    "interleaving {interleaving}"
                );
                assert_eq!(pin.head(&chain), before, "a bad head changed the pin");
                refusals += 1;
            } else if head_epoch < floor {
                // The lost timeline: validly signed, refused for its epoch, however new its position.
                assert_eq!(
                    verdict,
                    PinVerdict::Refused(HeadRefusal::BelowRestoreEpoch {
                        head: epoch(head_epoch),
                        floor: epoch(floor)
                    }),
                    "interleaving {interleaving}"
                );
                assert_eq!(
                    pin.head(&chain),
                    before,
                    "a head of the lost timeline changed the pin"
                );
                refusals += 1;
            } else {
                let entry = model.entry(chain).or_insert((position, Vec::new()));
                if position > entry.0 {
                    *entry = (position, Vec::new());
                }
                if position == entry.0 {
                    entry.1.push(head.head.hash);
                }
                // The verdict, by the rules.
                let pinned_before = before.map(|h| h.seq.get());
                match pinned_before {
                    None => assert_eq!(verdict, PinVerdict::Advanced),
                    Some(p) if position > p => assert_eq!(verdict, PinVerdict::Advanced),
                    Some(p) if position < p => {
                        assert_eq!(verdict, PinVerdict::RefusedOlder { newest: seq(p) });
                        refusals += 1;
                    }
                    Some(_) => {
                        let have = before.map(|h| h.hash);
                        if have == Some(head.head.hash) {
                            assert_eq!(verdict, PinVerdict::Unchanged);
                        } else {
                            assert!(matches!(verdict, PinVerdict::Fork(_)), "{verdict:?}");
                            forks += 1;
                        }
                    }
                }
            }
            // The pinned position is the model's maximum, and it never decreased.
            let pinned = pin.seq(&chain).map(|s| s.get());
            assert_eq!(
                pinned,
                model.get(&chain).map(|(max, _)| *max),
                "interleaving {interleaving}: the pin is the maximum of the valid heads"
            );
            if let (Some(now), Some(then)) = (pinned, last_seen.get(&chain)) {
                assert!(now >= *then, "the pin went backwards: {then} to {now}");
            }
            if let Some(now) = pinned {
                last_seen.insert(chain, now);
            }
            // The pinned head is one a valid offer made at that position (the first one seen
            // there, in fact: a fork keeps the old head).
            if let (Some(pinned_head), Some((max, hashes))) = (pin.head(&chain), model.get(&chain))
            {
                assert_eq!(pinned_head.seq.get(), *max);
                assert!(hashes.contains(&pinned_head.hash));
                assert_eq!(pinned_head.hash, hashes[0], "the first hash at the maximum");
            }
        }
    }
    assert!(offers > 50_000, "{offers} offers");
    assert!(
        refusals > 5_000 && forks > 100,
        "refusals {refusals}, forks {forks}"
    );
    // The property is about the generator too: head_at differs by variant.
    assert_ne!(
        head_at(team_log(1), 3, 0).hash,
        head_at(team_log(1), 3, 1).hash
    );
}
