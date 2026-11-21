//! Heads exchanged between devices (acceptance check 7): a peer ahead of this device raises the
//! withheld alarm naming both positions, a peer behind or the same is quiet, a peer on another hash
//! at the same position is a fork, and a peer head that does not verify is ignored without an alarm.
//!
//! Implements planning/architecture.md section 5.9 ("a server could still withhold newer entries
//! from one device, which would keep wrapping keys to a removed member ... devices of one account
//! exchange the heads they have seen whenever they meet peer to peer, so a hidden removal shows
//! quickly").

use azlin_chain::{compare_peer, Alarm, HeadPin, PeerVerdict, PinVerdict};

mod common;
use common::{
    at, epoch, group_head, group_head_in, group_key, key, seq, sign_head, space, team_log,
    FakeHeadVerifier, UnsupportedHeadVerifier,
};

/// A device that has pinned the team log at position 5.
fn device() -> (HeadPin, FakeHeadVerifier) {
    let verifier = FakeHeadVerifier::group();
    let mut pin = HeadPin::new();
    assert_eq!(
        pin.offer(group_head(team_log(1), 5, 0), &verifier),
        PinVerdict::Advanced
    );
    (pin, verifier)
}

#[test]
fn peer_ahead_of_us_raises_withheld_alarm_naming_both_seqs() {
    let (pin, verifier) = device();
    let chain = team_log(1);
    for ahead in [6u64, 7, 50, u64::MAX] {
        let peer = group_head(chain, ahead, 0);
        let verdict = compare_peer(&pin, &peer, &verifier);
        let alarm = Alarm::Withheld {
            chain,
            ours: seq(5),
            peer: seq(ahead),
        };
        assert_eq!(verdict, PeerVerdict::PeerAhead(alarm), "peer at {ahead}");
        assert_eq!(verdict.alarm(), Some(&alarm));
        // The alarm names both positions, in words too.
        let text = alarm.to_string();
        assert!(text.contains(&format!("position {ahead}")), "{text}");
        assert!(text.contains("at 5"), "{text}");
        assert_eq!(alarm.chain(), chain);
    }
    // The comparison changes nothing: the device is still at 5, and may now offer the peer's head to
    // its own pin to move forward.
    assert_eq!(pin.seq(&chain), Some(seq(5)));
    let mut pin = pin;
    let peer = group_head(chain, 8, 0);
    assert!(matches!(
        compare_peer(&pin, &peer, &verifier),
        PeerVerdict::PeerAhead(_)
    ));
    assert_eq!(pin.offer(peer.clone(), &verifier), PinVerdict::Advanced);
    assert_eq!(compare_peer(&pin, &peer, &verifier), PeerVerdict::Same);
}

#[test]
fn peer_behind_is_ignored() {
    let (pin, verifier) = device();
    let chain = team_log(1);
    for behind in [4u64, 3, 1, 0] {
        for variant in [0u8, 1] {
            let verdict = compare_peer(&pin, &group_head(chain, behind, variant), &verifier);
            assert_eq!(verdict, PeerVerdict::PeerBehind, "peer at {behind}");
            assert_eq!(verdict.alarm(), None);
        }
    }
    assert_eq!(pin.seq(&chain), Some(seq(5)));
}

#[test]
fn peer_same_head_is_quiet() {
    let (pin, verifier) = device();
    let chain = team_log(1);
    let verdict = compare_peer(&pin, &group_head(chain, 5, 0), &verifier);
    assert_eq!(verdict, PeerVerdict::Same);
    assert_eq!(verdict.alarm(), None);
    // The same head signed again at another time is the same head.
    let resigned = sign_head(group_head(chain, 5, 0).head, at(12_345), group_key());
    assert_eq!(compare_peer(&pin, &resigned, &verifier), PeerVerdict::Same);
}

#[test]
fn peer_fork_alarms() {
    let (pin, verifier) = device();
    let chain = team_log(1);
    let ours = group_head(chain, 5, 0);
    let theirs = group_head(chain, 5, 1);
    let verdict = compare_peer(&pin, &theirs, &verifier);
    let alarm = Alarm::Fork {
        chain,
        seq: seq(5),
        have: ours.head.hash,
        got: theirs.head.hash,
    };
    assert_eq!(verdict, PeerVerdict::Fork(alarm));
    assert_eq!(verdict.alarm(), Some(&alarm));
    // It changed nothing.
    assert_eq!(pin.head(&chain), Some(ours.head));
}

#[test]
fn peer_head_with_bad_signature_is_ignored_not_an_alarm() {
    let (pin, verifier) = device();
    let chain = team_log(1);
    // A peer "ahead", "same", "behind" and "forked" with a signature that does not verify: ignored
    // in every case, never an alarm (anybody can send junk; only a signed head proves anything).
    for (position, variant) in [(9u64, 0u8), (5, 0), (5, 1), (2, 0)] {
        let mut flipped = group_head(chain, position, variant);
        flipped.sig[1] ^= 0x40;
        let mut unsigned = group_head(chain, position, variant);
        unsigned.sig.clear();
        let untrusted = sign_head(group_head(chain, position, variant).head, at(9), key(0x77));
        for peer in [&flipped, &unsigned, &untrusted] {
            let verdict = compare_peer(&pin, peer, &verifier);
            assert_eq!(
                verdict,
                PeerVerdict::BadSignature,
                "({position}, {variant})"
            );
            assert_eq!(verdict.alarm(), None);
        }
    }
    // A verifier that cannot check the algorithm says the same.
    assert_eq!(
        compare_peer(&pin, &group_head(chain, 9, 0), &UnsupportedHeadVerifier),
        PeerVerdict::BadSignature
    );
}

#[test]
fn peer_head_of_a_chain_we_pin_nothing_of_has_nothing_to_compare_with() {
    let (pin, verifier) = device();
    // A space chain this device has not pinned: there is no head of ours to be behind.
    let verdict = compare_peer(&pin, &group_head(space(1), 3, 0), &verifier);
    assert_eq!(verdict, PeerVerdict::Unpinned);
    assert_eq!(verdict.alarm(), None);
    // A team log is not the space of the same bytes.
    assert_eq!(
        compare_peer(&pin, &group_head(space(1), 5, 0), &verifier),
        PeerVerdict::Unpinned
    );
    // An empty memory pins nothing at all.
    let empty = HeadPin::new();
    assert_eq!(
        compare_peer(&empty, &group_head(team_log(1), 5, 0), &verifier),
        PeerVerdict::Unpinned
    );
}

/// X-DELTA-W1S17A-3: heads of different restore epochs are not positions of one timeline. A peer on
/// the lost timeline (an older epoch) is behind whatever its position says (it must not raise a
/// withheld alarm for a head the restore removed); a peer on a later epoch has been announced a
/// restore this device has not, whatever its position says.
#[test]
fn peer_head_of_another_restore_epoch_is_judged_by_the_epoch_before_the_position() {
    let verifier = FakeHeadVerifier::group();
    let chain = team_log(1);
    // A device that accepted the restore of epoch 1 and pinned a head of epoch 1 at position 5.
    let mut pin = HeadPin::starting_at(epoch(1));
    assert_eq!(
        pin.offer(group_head_in(chain, 5, 0, 1, 50), &verifier),
        PinVerdict::Advanced
    );

    // A peer on the lost timeline, at positions above, at and below ours: behind, quiet.
    for position in [2u64, 5, 6, 50] {
        for variant in [0u8, 1] {
            let peer = group_head_in(chain, position, variant, 0, 100);
            let verdict = compare_peer(&pin, &peer, &verifier);
            assert_eq!(
                verdict,
                PeerVerdict::PeerBehind,
                "epoch 0, position {position}"
            );
            assert_eq!(verdict.alarm(), None);
        }
    }
    // A peer on a later epoch: ahead, with the alarm naming both positions, at any position.
    for position in [1u64, 5, 9] {
        let peer = group_head_in(chain, position, 0, 2, 100);
        let alarm = Alarm::Withheld {
            chain,
            ours: seq(5),
            peer: seq(position),
        };
        let verdict = compare_peer(&pin, &peer, &verifier);
        assert_eq!(
            verdict,
            PeerVerdict::PeerAhead(alarm),
            "epoch 2, position {position}"
        );
        assert_eq!(verdict.alarm(), Some(&alarm));
    }
    // The same epoch: by position, as before.
    assert_eq!(
        compare_peer(&pin, &group_head_in(chain, 5, 0, 1, 60), &verifier),
        PeerVerdict::Same
    );
    assert_eq!(
        compare_peer(&pin, &group_head_in(chain, 4, 0, 1, 40), &verifier),
        PeerVerdict::PeerBehind
    );
    assert!(matches!(
        compare_peer(&pin, &group_head_in(chain, 7, 0, 1, 70), &verifier),
        PeerVerdict::PeerAhead(Alarm::Withheld { .. })
    ));
    assert!(matches!(
        compare_peer(&pin, &group_head_in(chain, 5, 1, 1, 50), &verifier),
        PeerVerdict::Fork(_)
    ));
    // A bad signature is still told first.
    let mut junk = group_head_in(chain, 5, 0, 2, 100);
    junk.sig[0] ^= 1;
    assert_eq!(
        compare_peer(&pin, &junk, &verifier),
        PeerVerdict::BadSignature
    );
}

/// A device that holds a head of a restore it has not been announced yet (the floor is still the
/// older epoch) stands in the epoch of that head: a peer of that epoch is compared by position, not
/// alarmed as ahead.
#[test]
fn a_device_holding_a_head_of_a_later_epoch_compares_a_peer_of_that_epoch_by_position() {
    let verifier = FakeHeadVerifier::group();
    let chain = team_log(1);
    let mut pin = HeadPin::new(); // the floor is epoch 0
    assert_eq!(
        pin.offer(group_head_in(chain, 7, 0, 1, 70), &verifier),
        PinVerdict::Advanced
    );
    assert_eq!(pin.restore_epoch(), epoch(0));
    // The peer is on the same (not announced) epoch: same position, same hash: quiet.
    assert_eq!(
        compare_peer(&pin, &group_head_in(chain, 7, 0, 1, 71), &verifier),
        PeerVerdict::Same
    );
    assert_eq!(
        compare_peer(&pin, &group_head_in(chain, 3, 0, 1, 30), &verifier),
        PeerVerdict::PeerBehind
    );
    // A peer of the older epoch is behind (the lost timeline); of a still later one, ahead.
    assert_eq!(
        compare_peer(&pin, &group_head_in(chain, 9, 0, 0, 90), &verifier),
        PeerVerdict::PeerBehind
    );
    assert!(matches!(
        compare_peer(&pin, &group_head_in(chain, 2, 0, 2, 20), &verifier),
        PeerVerdict::PeerAhead(_)
    ));
    // A device with a position set by a record only (no signed head) stands in the floor's epoch.
    let mut anchored = HeadPin::starting_at(epoch(0));
    let rec_pin_head = head_of(chain, 4);
    anchored_set(&mut anchored, rec_pin_head);
    assert_eq!(anchored.restore_epoch(), epoch(1));
    assert_eq!(
        compare_peer(&anchored, &group_head_in(chain, 4, 0, 1, 40), &verifier),
        PeerVerdict::Same
    );
    assert_eq!(
        compare_peer(&anchored, &group_head_in(chain, 9, 0, 0, 90), &verifier),
        PeerVerdict::PeerBehind,
        "the lost timeline, whatever its position"
    );
}

/// The head of `chain` at `n`, variant 0.
fn head_of(chain: azlin_chain::ChainId, n: u64) -> azlin_chain::Head {
    common::head_at(chain, n, 0)
}

/// Sets a position by a verified restore record of epoch 1 (the only way a pin gets an anchored
/// position).
fn anchored_set(pin: &mut HeadPin, head: azlin_chain::Head) {
    let restore_verifier = common::FakeRestoreVerifier;
    let strict = azlin_chain::StrictTrust::new(&restore_verifier);
    let rec = common::map_signed(1, vec![head], false);
    azlin_chain::reanchor(pin, &rec, &strict).expect("a record signed by the map key");
}
