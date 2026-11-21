//! Checkpoints (acceptance check 8): when one is due, that verifying from a checkpoint is verifying
//! from the genesis (a property over 500 seeded chains, corrupted ones included), that a checkpoint
//! whose head hash does not match the entries after it is refused, and that the entries before it
//! are not needed.
//!
//! Implements planning/architecture.md section 5.9 ("Checkpoints: every 1,000 entries, and monthly,
//! the group records a checkpoint ... A new device verifies from the latest checkpoint whose
//! entries it can check, not from the account's first entry").

use azlin_chain::{
    checkpoint_due, verify_from_checkpoint, verify_from_genesis, Chain, ChainRefusal, Checkpoint,
    RefusalClass, CHECKPOINT_AGE, CHECKPOINT_ENTRIES,
};
use azlin_seams::sim::Rng;
use azlin_seams::{Hash32, Span, Stamp, UnixTime};

mod common;
use common::{
    at, build_chain, epoch, group_key, hash_of_fill, key, plain_chain, seq, sign_head,
    sign_head_in, space, team_log, FakeHeadVerifier, TestEntry, UnsupportedHeadVerifier,
};

/// A checkpoint of the chain at the entry `index`, signed by the group at `signed` in the restore
/// epoch of that entry (the head's epoch is the epoch of its entry unless a restore came between).
fn checkpoint_at(entries: &[TestEntry], index: usize, signed: u64) -> Checkpoint {
    let entry = &entries[index];
    Checkpoint::new(
        sign_head_in(entry.head(), entry.epoch, at(signed), group_key()),
        entry.at,
        hash_of_fill(index as u8),
    )
}

#[test]
fn checkpoint_due_at_1000_entries_and_at_30_days() {
    assert_eq!(CHECKPOINT_ENTRIES, 1_000);
    assert_eq!(CHECKPOINT_AGE, Span::days(30));
    assert_eq!(CHECKPOINT_AGE.as_secs(), 30 * 86_400);
    let last = UnixTime::from_secs(1_800_000_000);
    let plus = |secs: u64| UnixTime::from_secs(1_800_000_000 + secs);

    // By count: 999 are not enough, 1,000 are, more are.
    assert!(!checkpoint_due(0, last, last));
    assert!(!checkpoint_due(1, last, plus(1)));
    assert!(!checkpoint_due(999, last, plus(10)));
    assert!(checkpoint_due(1_000, last, plus(10)));
    assert!(checkpoint_due(1_001, last, plus(10)));
    assert!(checkpoint_due(u64::MAX, last, last));
    // By age, on the leader's stamp: 30 days, to the second.
    let thirty_days = 30 * 86_400;
    assert!(!checkpoint_due(0, last, plus(thirty_days - 1)));
    assert!(!checkpoint_due(999, last, plus(thirty_days - 1)));
    assert!(checkpoint_due(0, last, plus(thirty_days)));
    assert!(checkpoint_due(1, last, plus(thirty_days)));
    assert!(checkpoint_due(0, last, plus(thirty_days + 1)));
    assert!(checkpoint_due(0, last, plus(365 * 86_400)));
    assert!(!checkpoint_due(0, last, plus(29 * 86_400)));
    // A `now` before `last` has no age: only the count decides.
    let earlier = UnixTime::from_secs(1_700_000_000);
    assert!(!checkpoint_due(0, last, earlier));
    assert!(!checkpoint_due(999, last, earlier));
    assert!(checkpoint_due(1_000, last, earlier));
    // The edges of the clock.
    assert!(!checkpoint_due(
        0,
        UnixTime::from_secs(u64::MAX),
        UnixTime::from_secs(0)
    ));
    assert!(checkpoint_due(
        0,
        UnixTime::EPOCH,
        UnixTime::from_secs(thirty_days)
    ));
    assert!(!checkpoint_due(
        0,
        UnixTime::EPOCH,
        UnixTime::from_secs(thirty_days - 1)
    ));
}

#[test]
fn entries_before_the_checkpoint_are_not_needed() {
    let id = team_log(1);
    let entries = plain_chain(id, 30);
    let verifier = FakeHeadVerifier::group();
    let cp = checkpoint_at(&entries, 19, 5_000);
    assert_eq!(cp.head(), entries[19].head());
    assert_eq!(cp.at(), entries[19].at);
    assert_eq!(cp.state_hash(), hash_of_fill(19));
    assert_eq!(cp.signed_head().head, entries[19].head());

    // The entries after the checkpoint alone verify to the head of the whole chain.
    let rest = &entries[20..];
    assert_eq!(
        verify_from_checkpoint(&cp, &verifier, rest),
        Ok(entries[29].head())
    );
    // With nothing after it, the checkpoint's own head.
    assert_eq!(
        verify_from_checkpoint(&cp, &verifier, &entries[..0]),
        Ok(entries[19].head())
    );
    assert_eq!(
        verify_from_checkpoint(&cp, &verifier, &entries[20..21]),
        Ok(entries[20].head())
    );
    // The entries before it are not looked at: with them gone (a run that starts after the
    // checkpoint is all this function takes), a chain verified from the genesis would refuse.
    assert_eq!(verify_from_genesis(id, rest), Err(ChainRefusal::NotGenesis));
    // A chain that holds from the checkpoint on (resumed at the head) admits the same entries.
    let mut resumed: Chain<TestEntry> = Chain::resume(cp.head(), cp.at(), cp.epoch(), 1_000);
    for entry in rest {
        assert!(resumed
            .admit(entry.clone())
            .expect("the next entry")
            .is_new());
    }
    assert_eq!(resumed.head(), Some(entries[29].head()));

    // The checkpoint's head signature is what makes it a checkpoint: a head that does not verify is
    // a corrupt copy (fetch another), whatever the entries are.
    let mut forged = cp.signed_head().clone();
    forged.sig[0] ^= 1;
    let forged_cp = Checkpoint::new(forged, cp.at(), cp.state_hash());
    assert_eq!(
        verify_from_checkpoint(&forged_cp, &verifier, rest),
        Err(ChainRefusal::Corrupt)
    );
    let untrusted = Checkpoint::new(
        sign_head(entries[19].head(), at(5_000), key(0x55)),
        cp.at(),
        cp.state_hash(),
    );
    let error = verify_from_checkpoint(&untrusted, &verifier, rest).expect_err("untrusted");
    assert_eq!(error, ChainRefusal::Corrupt);
    assert_eq!(error.class(), RefusalClass::Refetch);
    assert_eq!(
        verify_from_checkpoint(&cp, &UnsupportedHeadVerifier, rest),
        Err(ChainRefusal::Corrupt)
    );
}

/// X-DELTA-W1S17A-3, the checkpoint: the head of a checkpoint is signed in a restore epoch, and the
/// entries after it must be of that epoch or later. A restore right before the checkpoint (the head
/// is the survivor, written in epoch 0, signed in epoch 1) refuses the entries of the lost timeline
/// that chain correctly from it.
#[test]
fn entries_of_an_older_epoch_than_the_checkpoint_head_are_refused() {
    let id = team_log(1);
    let entries = plain_chain(id, 12); // all of epoch 0
    let verifier = FakeHeadVerifier::group();

    // The group signs the survivor (entry 5, epoch 0) in the epoch 1 of the restore it just applied.
    let head = sign_head_in(entries[5].head(), epoch(1), at(9_000), group_key());
    let cp = Checkpoint::new(head, entries[5].at, hash_of_fill(5));
    assert_eq!(cp.epoch(), epoch(1));
    // The entries of the lost timeline (epoch 0) follow the survivor correctly: refused for their
    // epoch (an alarm), at the first one.
    let refusal =
        verify_from_checkpoint(&cp, &verifier, &entries[6..]).expect_err("the lost timeline");
    assert_eq!(
        refusal,
        ChainRefusal::BelowRestoreEpoch {
            got: epoch(0),
            floor: epoch(1)
        }
    );
    assert_eq!(refusal.class(), RefusalClass::Alarm);
    // The restored timeline (epoch 1 and later, a different history after the survivor) verifies.
    let mut restored: Vec<TestEntry> = Vec::new();
    let mut previous = entries[5].clone();
    for (index, epoch_of) in [1u64, 1, 2, 2].into_iter().enumerate() {
        let next = previous.after_in(
            at(500 + index as u64),
            epoch(epoch_of),
            format!("restored {index}").as_bytes(),
        );
        restored.push(next.clone());
        previous = next;
    }
    assert_eq!(
        verify_from_checkpoint(&cp, &verifier, &restored),
        Ok(previous.head())
    );
    // Along the run the epoch goes up and never down.
    let mut dips = restored.clone();
    dips[3] = restored[2].after_in(at(600), epoch(1), b"dips back");
    assert_eq!(
        verify_from_checkpoint(&cp, &verifier, &dips),
        Err(ChainRefusal::BelowRestoreEpoch {
            got: epoch(1),
            floor: epoch(2)
        })
    );
    // A checkpoint signed in epoch 0 (no restore yet) takes the entries of epoch 0.
    let cp0 = checkpoint_at(&entries, 5, 9_000);
    assert_eq!(cp0.epoch(), epoch(0));
    assert_eq!(
        verify_from_checkpoint(&cp0, &verifier, &entries[6..]),
        Ok(entries[11].head())
    );
}

#[test]
fn checkpoint_with_wrong_head_hash_refused() {
    let id = team_log(1);
    let entries = plain_chain(id, 12);
    let verifier = FakeHeadVerifier::group();
    let rest = &entries[6..];

    // A checkpoint whose (validly signed) head names another hash at position 5: the first entry
    // after it does not continue it.
    let mut wrong_head = entries[5].head();
    wrong_head.hash = hash_of_fill(0xAA);
    let cp = Checkpoint::new(
        sign_head(wrong_head, at(9_000), group_key()),
        entries[5].at,
        hash_of_fill(5),
    );
    assert_eq!(
        verify_from_checkpoint(&cp, &verifier, rest),
        Err(ChainRefusal::WrongPrev {
            expected: hash_of_fill(0xAA),
            got: entries[5].hash
        })
    );
    // A checkpoint at another position than the entries continue: a gap.
    let early = checkpoint_at(&entries, 3, 9_000);
    assert_eq!(
        verify_from_checkpoint(&early, &verifier, rest),
        Err(ChainRefusal::Gap {
            expected: seq(4),
            got: seq(6)
        })
    );
    let late = checkpoint_at(&entries, 9, 9_000);
    assert_eq!(
        verify_from_checkpoint(&late, &verifier, rest),
        Err(ChainRefusal::Gap {
            expected: seq(10),
            got: seq(6)
        })
    );
    // A checkpoint of another chain: the entries are not its entries.
    let foreign = Checkpoint::new(
        sign_head(plain_chain(space(1), 6)[5].head(), at(9_000), group_key()),
        entries[5].at,
        hash_of_fill(5),
    );
    assert_eq!(
        verify_from_checkpoint(&foreign, &verifier, rest),
        Err(ChainRefusal::WrongChain)
    );
    // The control: the honest checkpoint continues.
    let honest = checkpoint_at(&entries, 5, 9_000);
    assert_eq!(
        verify_from_checkpoint(&honest, &verifier, rest),
        Ok(entries[11].head())
    );
    // The stamp rule holds across the boundary: an entry earlier than the checkpoint's entry is
    // refused even though the checkpoint's signature is good.
    let mut early_entry = entries[6].clone();
    early_entry.at = at(entries[5].at.as_secs() - 1);
    let early_entry = TestEntry::new(id, seq(6), entries[5].hash, early_entry.at, b"early");
    assert_eq!(
        verify_from_checkpoint(&honest, &verifier, &[early_entry]),
        Err(ChainRefusal::StampBackwards)
    );
    // A run with a corrupt entry and a repeated entry is refused where it goes wrong.
    let mut corrupt: Vec<TestEntry> = rest.to_vec();
    corrupt[2] = corrupt[2].corrupted();
    assert_eq!(
        verify_from_checkpoint(&honest, &verifier, &corrupt),
        Err(ChainRefusal::Corrupt)
    );
    let mut repeated: Vec<TestEntry> = rest.to_vec();
    repeated.insert(3, repeated[2].clone());
    assert_eq!(
        verify_from_checkpoint(&honest, &verifier, &repeated),
        Err(ChainRefusal::Gap {
            expected: seq(9),
            got: seq(8)
        })
    );
}

/// One way to damage the part of a run after the checkpoint.
fn damage(rest: &mut Vec<TestEntry>, rng: &mut Rng) {
    if rest.is_empty() {
        return;
    }
    let index = rng.below(rest.len() as u64).unwrap_or(0) as usize;
    match rng.below(8).unwrap_or(0) {
        0 => rest[index] = rest[index].corrupted(),
        1 => {
            // A wrong predecessor, with a valid hash of its own.
            let e = &rest[index];
            rest[index] = TestEntry::new(e.chain, e.seq, hash_of_fill(0x42), e.at, &e.payload);
        }
        2 => {
            rest.remove(index);
        }
        3 => {
            let copy = rest[index].clone();
            rest.insert(index, copy);
        }
        4 => {
            // A stamp that runs backwards (the entry stays intact and linked).
            let e = &rest[index];
            let earlier = at(e.at.as_secs().saturating_sub(1_000_000));
            rest[index] = TestEntry::new(e.chain, e.seq, e.prev, earlier, &e.payload);
        }
        5 => {
            let e = &rest[index];
            rest[index] = TestEntry::new(space(7), e.seq, e.prev, e.at, &e.payload);
        }
        6 => {
            let other = rng.below(rest.len() as u64).unwrap_or(0) as usize;
            rest.swap(index, other);
        }
        _ => rest.truncate(index),
    }
}

/// The property: verifying the entries after a checkpoint gives the head, or the refusal, that
/// verifying the whole chain from its genesis gives, for 500 seeded chains with a checkpoint at a
/// random position and damage (corrupt, wrong prev, missing, repeated, backwards, foreign, swapped,
/// truncated) only after the checkpoint: the entries before a checkpoint are sound by construction
/// (the group signed it).
#[test]
fn verify_from_checkpoint_equals_verify_from_genesis() {
    let id = team_log(1);
    let verifier = FakeHeadVerifier::group();
    let mut damaged = 0usize;
    let mut refused = std::collections::HashSet::new();
    for seed in 0..500u64 {
        let mut rng = Rng::new(0x0C4E_C000 + seed);
        let len = 2 + rng.below(40).unwrap_or(0) as usize;
        let mut entries = build_chain(id, len, &mut rng);
        let k = rng.below(len as u64 - 1).unwrap_or(0) as usize; // the checkpoint entry, not the last
        let cp = checkpoint_at(&entries, k, 9_000 + seed);
        let mut rest: Vec<TestEntry> = entries.split_off(k + 1);
        let before_checkpoint = entries; // positions 0..=k
        if rng.chance(2, 5) {
            damage(&mut rest, &mut rng);
            damaged += 1;
        }
        // Both paths over the same run: the whole chain, and the part after the checkpoint.
        let mut whole = before_checkpoint.clone();
        whole.extend(rest.iter().cloned());
        let from_genesis =
            verify_from_genesis(id, &whole).map(|head| head.expect("a non-empty run"));
        let from_checkpoint = verify_from_checkpoint(&cp, &verifier, &rest);
        assert_eq!(
            from_genesis,
            from_checkpoint,
            "seed {seed}: checkpoint at {k} of {len}, {} entries after it",
            rest.len()
        );
        if let Err(refusal) = &from_checkpoint {
            refused.insert(std::mem::discriminant(refusal));
        }
    }
    assert!(damaged > 150, "{damaged} damaged runs");
    assert!(
        refused.len() >= 5,
        "the damage met only {} kinds of refusal",
        refused.len()
    );
    // A checkpoint the whole group agrees on is a head like any other: the hash of the state is
    // carried, not checked, here.
    let none: Hash32 = Hash32::ZERO;
    let stamp: Stamp = at(1);
    assert_eq!(
        Checkpoint::new(
            sign_head(plain_chain(id, 1)[0].head(), at(2), group_key()),
            stamp,
            none
        )
        .state_hash(),
        none
    );
}
