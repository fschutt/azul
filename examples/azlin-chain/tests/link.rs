//! The link checks: genesis, `prev`, positions, chains, stamps, forks, duplicates and corrupt
//! copies (acceptance checks 4 and 5), on `Chain` and on the entry-less `Tip`, and a differential
//! test of `Chain` against an independent model written from the plan.
//!
//! Implements planning/architecture.md section 5.9 ("the metadata group rejects an entry with a
//! reason for a wrong `prev`", "two different entries at the same position are a fork, refused and
//! alarmed", "an entry that fails its hash or signature check is fetched again from another voter").

use azlin_chain::{
    verify_from_genesis, Admitted, Alarm, Chain, ChainEntry, ChainRefusal, Head, Placement,
    RefusalClass, Tip,
};
use azlin_seams::sim::Rng;
use azlin_seams::{ChainSeq, Hash32, RestoreEpoch, Stamp};

mod common;
use common::{at, build_chain, epoch, hash_of_fill, plain_chain, seq, space, team_log, TestEntry};

/// A chain that has admitted the first `n` entries of `plain_chain`.
fn admitted(n: usize) -> (Chain<TestEntry>, Vec<TestEntry>) {
    let id = team_log(1);
    let entries = plain_chain(id, n + 4);
    let mut chain = Chain::new(id);
    for entry in entries.iter().take(n) {
        assert!(chain.admit(entry.clone()).expect("a valid entry").is_new());
    }
    (chain, entries)
}

/// A hash that is zero but for its last byte: no genesis `prev`.
fn almost_zero() -> Hash32 {
    let mut bytes = [0u8; 32];
    bytes[31] = 1;
    Hash32::from_bytes(bytes)
}

/// The observable state of a chain: head, last stamp, the entries.
fn state(chain: &Chain<TestEntry>) -> (Option<Head>, Option<azlin_seams::Stamp>, Vec<TestEntry>) {
    (chain.head(), chain.last_at(), chain.entries().to_vec())
}

#[test]
fn genesis_must_be_seq_zero_with_zero_prev() {
    let id = team_log(1);
    let genesis = TestEntry::genesis(id, at(100), b"genesis");

    // The control: a genesis is the first entry.
    let mut chain: Chain<TestEntry> = Chain::new(id);
    assert_eq!(chain.head(), None);
    assert!(chain.is_empty());
    assert_eq!(
        chain.admit(genesis.clone()),
        Ok(Admitted::Appended {
            head: genesis.head()
        })
    );
    assert_eq!(chain.head(), Some(genesis.head()));
    assert_eq!(chain.head().map(|h| h.seq), Some(seq(0)));

    // Position 0 with a `prev` that is not zero is no genesis; neither is any other position.
    let bad = [
        TestEntry::new(id, seq(0), hash_of_fill(7), at(100), b"x"),
        TestEntry::new(id, seq(0), almost_zero(), at(100), b"x"),
        TestEntry::new(id, seq(1), Hash32::ZERO, at(100), b"x"),
        TestEntry::new(id, seq(1), hash_of_fill(7), at(100), b"x"),
        TestEntry::new(id, seq(5), Hash32::ZERO, at(100), b"x"),
        TestEntry::new(id, ChainSeq::new(u64::MAX), Hash32::ZERO, at(100), b"x"),
    ];
    for entry in &bad {
        let mut empty: Chain<TestEntry> = Chain::new(id);
        let refusal = empty.admit(entry.clone()).expect_err("not a genesis");
        assert_eq!(refusal, ChainRefusal::NotGenesis, "{entry:?}");
        assert_eq!(refusal.class(), RefusalClass::Refuse);
        assert_eq!(refusal.alarm(id), None);
        assert_eq!(empty.head(), None, "a refusal leaves the chain empty");
        assert!(empty.is_empty());
        // The same on the entry-less state.
        let tip = Tip::new(id);
        assert_eq!(tip.check(entry), Err(ChainRefusal::NotGenesis));
        assert_eq!(tip.head(), None);
    }
    // A genesis checks on a Tip too, and advancing moves it.
    let mut tip = Tip::new(id);
    assert_eq!(tip.check(&genesis), Ok(Placement::Next));
    assert_eq!(tip.next_seq(), Some(seq(0)));
    assert_eq!(
        tip.advance(&genesis),
        Ok(Admitted::Appended {
            head: genesis.head()
        })
    );
    assert_eq!(tip.head(), Some(genesis.head()));
    assert_eq!(tip.last_at(), Some(at(100)));
    assert_eq!(tip.next_seq(), Some(seq(1)));
    assert_eq!(tip.chain(), id);
}

#[test]
fn second_genesis_refused() {
    let (mut chain, entries) = admitted(4);
    let genesis = entries[0].clone();
    let second = TestEntry::genesis(team_log(1), at(105), b"another genesis");
    assert_ne!(second.hash, genesis.hash);
    let before = state(&chain);

    // A different genesis is refused, with both hashes, and it is an alarm (a fork at position 0).
    let refusal = chain.admit(second.clone()).expect_err("a second genesis");
    assert_eq!(
        refusal,
        ChainRefusal::GenesisAgain {
            have: genesis.hash,
            got: second.hash
        }
    );
    assert_eq!(refusal.class(), RefusalClass::Alarm);
    assert_eq!(
        refusal.alarm(team_log(1)),
        Some(Alarm::Fork {
            chain: team_log(1),
            seq: seq(0),
            have: genesis.hash,
            got: second.hash
        })
    );
    assert_eq!(state(&chain), before, "the chain is unchanged");

    // The same genesis again is the genesis it already holds: nothing happens.
    assert_eq!(
        chain.admit(genesis.clone()),
        Ok(Admitted::AlreadyHeld {
            head: entries[3].head()
        })
    );
    assert_eq!(state(&chain), before);

    // The same on a Tip that holds only the genesis (it holds one hash: the head's).
    let mut tip = Tip::new(team_log(1));
    tip.advance(&genesis).expect("genesis");
    assert_eq!(
        tip.check(&second),
        Err(ChainRefusal::GenesisAgain {
            have: genesis.hash,
            got: second.hash
        })
    );
    assert_eq!(
        tip.check(&genesis),
        Ok(Placement::Held {
            head: genesis.head()
        })
    );
    // A Tip further on cannot compare position 0: it is an old position it does not hold.
    tip.advance(&entries[1]).expect("next");
    assert_eq!(
        tip.check(&second),
        Err(ChainRefusal::Gap {
            expected: seq(2),
            got: seq(0)
        })
    );
}

#[test]
fn wrong_prev_names_expected_and_got() {
    let (mut chain, entries) = admitted(3);
    let head = entries[2].clone();
    let before = state(&chain);
    // The right position, the right stamp, another predecessor.
    let wrong = TestEntry::new(head.chain, seq(3), hash_of_fill(9), at(500), b"wrong prev");
    let refusal = chain.admit(wrong).expect_err("wrong prev");
    assert_eq!(
        refusal,
        ChainRefusal::WrongPrev {
            expected: head.hash,
            got: hash_of_fill(9)
        }
    );
    assert_eq!(refusal.class(), RefusalClass::Refuse);
    assert_eq!(refusal.alarm(head.chain), None);
    assert_eq!(state(&chain), before);
    let text = refusal.to_string();
    assert!(text.contains(&head.hash.to_string()), "{text}");
    assert!(text.contains(&hash_of_fill(9).to_string()), "{text}");

    // The predecessor of an older entry is wrong as well: not the head's hash.
    let older = TestEntry::new(head.chain, seq(3), entries[1].hash, at(500), b"old prev");
    assert_eq!(
        chain.admit(older),
        Err(ChainRefusal::WrongPrev {
            expected: head.hash,
            got: entries[1].hash
        })
    );
    // A zero `prev` after a genesis is wrong too.
    let zero = TestEntry::new(head.chain, seq(3), Hash32::ZERO, at(500), b"zero prev");
    assert_eq!(
        chain.admit(zero),
        Err(ChainRefusal::WrongPrev {
            expected: head.hash,
            got: Hash32::ZERO
        })
    );
    // The control: the right predecessor is admitted.
    assert!(chain
        .admit(entries[3].clone())
        .expect("the next entry")
        .is_new());
    // A wrong `prev` is told before a backwards stamp (the position's own check comes first).
    let both = TestEntry::new(head.chain, seq(4), hash_of_fill(1), at(1), b"both");
    assert!(matches!(
        chain.admit(both),
        Err(ChainRefusal::WrongPrev { .. })
    ));
}

#[test]
fn gap_and_replay_refused_with_expected_seq() {
    let (mut chain, entries) = admitted(3);
    let head = entries[2].clone();
    let before = state(&chain);
    // A gap: the entry is above the next position. `expected` is the next position.
    for position in [4u64, 5, 100, u64::MAX] {
        let entry = TestEntry::new(head.chain, seq(position), head.hash, at(500), b"gap");
        let refusal = chain.admit(entry).expect_err("a gap");
        assert_eq!(
            refusal,
            ChainRefusal::Gap {
                expected: seq(3),
                got: seq(position)
            }
        );
        assert_eq!(refusal.class(), RefusalClass::Refuse);
    }
    assert_eq!(state(&chain), before);

    // A replay of a position the chain no longer holds: a chain resumed at a head holds nothing
    // before it, so an old entry is refused with the expected position (it cannot be judged).
    let mut resumed: Chain<TestEntry> =
        Chain::resume(entries[2].head(), entries[2].at, RestoreEpoch::ZERO, 100);
    assert_eq!(resumed.head(), Some(entries[2].head()));
    assert!(resumed.is_empty(), "no entry is held, only the head");
    for old in [&entries[0], &entries[1], &entries[2]] {
        assert_eq!(
            resumed.admit(old.clone()),
            Err(ChainRefusal::Gap {
                expected: seq(3),
                got: old.seq
            }),
            "a replay of position {}",
            old.seq
        );
    }
    // It goes on from the head, and holds what it admitted from then on.
    assert!(resumed.admit(entries[3].clone()).expect("next").is_new());
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed.get(seq(3)), Some(&entries[3]));
    assert_eq!(resumed.get(seq(2)), None);
    assert_eq!(
        resumed.admit(entries[3].clone()),
        Ok(Admitted::AlreadyHeld {
            head: entries[3].head()
        })
    );

    // A run that repeats an entry is out of place at the repeat.
    let run = [
        entries[0].clone(),
        entries[1].clone(),
        entries[1].clone(),
        entries[2].clone(),
    ];
    assert_eq!(
        verify_from_genesis(head.chain, &run),
        Err(ChainRefusal::Gap {
            expected: seq(2),
            got: seq(1)
        })
    );
    // The largest position has no next one.
    let mut tip = Tip::resume(
        Head {
            chain: head.chain,
            seq: ChainSeq::new(u64::MAX),
            hash: hash_of_fill(3),
        },
        at(1),
        RestoreEpoch::ZERO,
    );
    assert_eq!(tip.next_seq(), None);
    let entry = TestEntry::new(head.chain, seq(0), Hash32::ZERO, at(2), b"x");
    assert_eq!(tip.advance(&entry), Err(ChainRefusal::Full));
}

#[test]
fn entry_for_another_chain_refused() {
    let id = team_log(1);
    let (mut chain, entries) = admitted(3);
    let before = state(&chain);
    // Another id of the same kind, the same id of another kind, a genesis of another chain.
    let others = [team_log(2), space(1), space(2)];
    for other in others {
        // The entry would be the right next one of its own chain, and a valid genesis.
        let foreign = TestEntry::new(other, seq(3), entries[2].hash, at(500), b"foreign");
        let genesis = TestEntry::genesis(other, at(500), b"foreign genesis");
        for entry in [foreign, genesis] {
            let refusal = chain.admit(entry.clone()).expect_err("another chain");
            assert_eq!(refusal, ChainRefusal::WrongChain, "{other}");
            assert_eq!(refusal.class(), RefusalClass::Refuse);
            assert_eq!(Tip::new(id).check(&entry), Err(ChainRefusal::WrongChain));
        }
    }
    assert_eq!(state(&chain), before);
    // The chain id is told before the corrupt copy and before the position.
    let corrupt_foreign = TestEntry::genesis(team_log(2), at(1), b"x").corrupted();
    assert_eq!(
        Chain::<TestEntry>::new(id).admit(corrupt_foreign),
        Err(ChainRefusal::WrongChain)
    );
    // The ids differ by kind alone: a team log is not a space of the same bytes.
    assert_ne!(team_log(1), space(1));
    assert_eq!(team_log(1).id, space(1).id);
}

#[test]
fn stamp_going_backwards_refused() {
    let id = team_log(1);
    let g = TestEntry::genesis(id, at(100), b"g");
    let e1 = g.after(at(110), b"e1");
    let mut chain: Chain<TestEntry> = Chain::new(id);
    chain.admit(g).expect("genesis");
    chain.admit(e1.clone()).expect("e1");
    let before = state(&chain);
    assert_eq!(chain.last_at(), Some(at(110)));

    // One second before the previous entry.
    let backwards = e1.after(at(109), b"e2 early");
    let refusal = chain.admit(backwards).expect_err("backwards");
    assert_eq!(refusal, ChainRefusal::StampBackwards);
    assert_eq!(refusal.class(), RefusalClass::Refuse);
    assert_eq!(state(&chain), before);
    // Far before, and the epoch itself.
    for stamp in [0u64, 1, 99] {
        assert_eq!(
            chain.admit(e1.after(at(stamp), b"e2 early")),
            Err(ChainRefusal::StampBackwards)
        );
    }
    // The same stamp is not backwards (two entries in one second), a later one is fine.
    let same = e1.after(at(110), b"e2 same second");
    assert!(chain.admit(same.clone()).expect("same stamp").is_new());
    assert_eq!(chain.last_at(), Some(at(110)));
    assert!(chain
        .admit(same.after(at(111), b"e3"))
        .expect("later")
        .is_new());
    assert_eq!(chain.last_at(), Some(at(111)));

    // The stamp rule holds on the entry-less state too, and a resumed one starts from its stamp.
    let mut tip = Tip::resume(e1.head(), at(110), RestoreEpoch::ZERO);
    assert_eq!(
        tip.advance(&e1.after(at(109), b"x")),
        Err(ChainRefusal::StampBackwards)
    );
    assert!(tip.advance(&e1.after(at(110), b"x")).is_ok());
}

#[test]
fn same_position_different_entry_is_a_fork_and_alarms() {
    let id = team_log(1);
    let (mut chain, entries) = admitted(5);
    let before = state(&chain);
    // A different entry at positions 1 to 4, each a valid entry of its own (the right `prev`).
    for (position, held) in entries.iter().enumerate().take(5).skip(1) {
        let other = TestEntry::new(
            id,
            held.seq,
            entries[position - 1].hash,
            held.at,
            b"the other entry",
        );
        assert_ne!(other.hash, held.hash);
        let refusal = chain.admit(other.clone()).expect_err("a fork");
        assert_eq!(
            refusal,
            ChainRefusal::Fork {
                at: held.seq,
                have: held.hash,
                got: other.hash
            },
            "position {position}"
        );
        // Refused AND alarmed: the class is Alarm and the alarm names the position and both hashes.
        assert_eq!(refusal.class(), RefusalClass::Alarm);
        assert_eq!(
            refusal.alarm(id),
            Some(Alarm::Fork {
                chain: id,
                seq: held.seq,
                have: held.hash,
                got: other.hash
            })
        );
        assert_eq!(state(&chain), before, "the chain keeps the entry it holds");
    }
    // A fork with a wrong `prev` as well is still a fork: the position decides.
    let odd = TestEntry::new(id, seq(2), hash_of_fill(5), at(1), b"odd");
    assert!(matches!(chain.admit(odd), Err(ChainRefusal::Fork { .. })));
    // The head's own position, on a Tip that holds only the head, is a fork too.
    let mut tip = Tip::resume(entries[4].head(), entries[4].at, RestoreEpoch::ZERO);
    let other_head = TestEntry::new(
        id,
        seq(4),
        entries[3].hash,
        entries[4].at,
        b"the other head",
    );
    assert_eq!(
        tip.check(&other_head),
        Err(ChainRefusal::Fork {
            at: seq(4),
            have: entries[4].hash,
            got: other_head.hash
        })
    );
    assert_eq!(tip.advance(&entries[4]).map(|a| a.is_new()), Ok(false));
    assert_eq!(
        Alarm::Fork {
            chain: id,
            seq: seq(4),
            have: entries[4].hash,
            got: other_head.hash
        }
        .chain(),
        id
    );
    // Its text names the position.
    let text = Alarm::Fork {
        chain: id,
        seq: seq(4),
        have: entries[4].hash,
        got: other_head.hash,
    }
    .to_string();
    assert!(text.contains("position 4"), "{text}");
}

#[test]
fn identical_entry_again_is_not_a_fork_and_changes_nothing() {
    let (mut chain, entries) = admitted(5);
    let before = state(&chain);
    let head = entries[4].head();
    // Every held entry again, as a clone: not a fork, nothing appended, the head unchanged.
    for entry in entries.iter().take(5) {
        let admitted = chain.admit(entry.clone()).expect("a held entry again");
        assert_eq!(admitted, Admitted::AlreadyHeld { head });
        assert!(!admitted.is_new());
        assert_eq!(admitted.head(), head);
        assert_eq!(state(&chain), before, "position {}", entry.seq);
        assert_eq!(chain.check(entry), Ok(Placement::Held { head }));
    }
    // And again and again.
    for _ in 0..10 {
        assert!(!chain.admit(entries[2].clone()).expect("again").is_new());
    }
    assert_eq!(state(&chain), before);
    // After the chain moved on, the old entries are still the same entries.
    assert!(chain.admit(entries[5].clone()).expect("next").is_new());
    let moved = chain.head().expect("a head");
    assert_eq!(
        chain.admit(entries[0].clone()),
        Ok(Admitted::AlreadyHeld { head: moved })
    );
    assert_eq!(chain.len(), 6);
}

#[test]
fn corrupt_entry_class_is_refetch_not_fork() {
    let id = team_log(1);
    let (mut chain, entries) = admitted(4);
    let before = state(&chain);

    // A corrupt copy of an entry the chain holds: its claimed hash is the held one, its content is
    // not. It is NOT a fork (and not "already held"): the copy is bad, fetch it again.
    for held in &entries[..4] {
        let copy = held.corrupted();
        assert_eq!(copy.hash, held.hash);
        assert!(!copy.is_intact());
        let refusal = chain.admit(copy).expect_err("corrupt");
        assert_eq!(refusal, ChainRefusal::Corrupt, "position {}", held.seq);
        assert_eq!(refusal.class(), RefusalClass::Refetch);
        assert_ne!(refusal.class(), RefusalClass::Alarm);
        assert_eq!(refusal.alarm(id), None, "no alarm for a bad copy");
    }
    // A corrupt copy that claims ANOTHER hash at a held position is corrupt, not a fork.
    let mut other = TestEntry::new(id, seq(2), entries[1].hash, at(120), b"other");
    other.hash = hash_of_fill(0xAB);
    assert_eq!(chain.admit(other), Err(ChainRefusal::Corrupt));
    // A corrupt copy of the NEXT entry is corrupt before anything is said about its position.
    assert_eq!(
        chain.admit(entries[4].corrupted()),
        Err(ChainRefusal::Corrupt)
    );
    let mut gapped = entries[6].clone();
    gapped.payload.push(1);
    assert_eq!(chain.admit(gapped), Err(ChainRefusal::Corrupt));
    // A corrupt genesis on an empty chain: corrupt, not "not a genesis".
    let mut empty: Chain<TestEntry> = Chain::new(id);
    assert_eq!(
        empty.admit(entries[0].corrupted()),
        Err(ChainRefusal::Corrupt)
    );
    assert_eq!(empty.head(), None);
    assert_eq!(state(&chain), before, "no refusal changed the chain");
    // The control: the intact next entry is admitted.
    assert!(chain.admit(entries[4].clone()).expect("intact").is_new());
    // On a Tip as well.
    let tip = Tip::resume(entries[3].head(), entries[3].at, RestoreEpoch::ZERO);
    assert_eq!(
        tip.check(&entries[4].corrupted()),
        Err(ChainRefusal::Corrupt)
    );
    assert_eq!(
        tip.check(&entries[3].corrupted()),
        Err(ChainRefusal::Corrupt)
    );
}

#[test]
fn a_chain_keeps_a_bounded_number_of_entries() {
    let id = team_log(1);
    let entries = plain_chain(id, 6);
    let mut chain: Chain<TestEntry> = Chain::with_limit(id, 3);
    for entry in entries.iter().take(3) {
        assert!(chain
            .admit(entry.clone())
            .expect("within the limit")
            .is_new());
    }
    let before = state(&chain);
    let refusal = chain.admit(entries[3].clone()).expect_err("full");
    assert_eq!(refusal, ChainRefusal::Full);
    assert_eq!(refusal.class(), RefusalClass::Refuse);
    assert_eq!(state(&chain), before, "a full chain is unchanged");
    // What it holds is still answered: an identical entry again, a fork.
    assert!(!chain.admit(entries[1].clone()).expect("held").is_new());
    let other = TestEntry::new(id, seq(1), entries[0].hash, entries[1].at, b"other");
    assert!(matches!(chain.admit(other), Err(ChainRefusal::Fork { .. })));
    assert_eq!(Chain::<TestEntry>::DEFAULT_MAX_ENTRIES, 100_000);
    let zero: Chain<TestEntry> = Chain::with_limit(id, 0);
    assert!(zero.is_empty());
    let mut zero = zero;
    assert_eq!(
        zero.admit(entries[0].clone()),
        Err(ChainRefusal::Full),
        "a bound of 0 holds nothing"
    );
}

#[test]
fn verify_from_genesis_checks_a_whole_run() {
    let id = team_log(1);
    let entries = plain_chain(id, 10);
    assert_eq!(verify_from_genesis(id, &entries[..0]), Ok(None));
    assert_eq!(
        verify_from_genesis(id, &entries),
        Ok(Some(entries[9].head()))
    );
    assert_eq!(
        verify_from_genesis(id, &entries[..1]),
        Ok(Some(entries[0].head()))
    );
    // A run that does not start at the genesis, with a gap, a corrupt entry, another chain's entry.
    assert_eq!(
        verify_from_genesis(id, &entries[1..]),
        Err(ChainRefusal::NotGenesis)
    );
    let mut gapped = entries.clone();
    gapped.remove(5);
    assert_eq!(
        verify_from_genesis(id, &gapped),
        Err(ChainRefusal::Gap {
            expected: seq(5),
            got: seq(6)
        })
    );
    let mut corrupt = entries.clone();
    corrupt[7] = corrupt[7].corrupted();
    assert_eq!(
        verify_from_genesis(id, &corrupt),
        Err(ChainRefusal::Corrupt)
    );
    assert_eq!(
        verify_from_genesis(team_log(2), &entries),
        Err(ChainRefusal::WrongChain)
    );
}

/// What the plan says of one offered entry, written independently of the crate: the model keeps the
/// accepted entries as `(hash, stamp, restore epoch)` and answers by the rules of section 5.9 and
/// 6.10 V3.
fn model_expect(
    model: &[(Hash32, Stamp, RestoreEpoch)],
    id: azlin_chain::ChainId,
    e: &TestEntry,
) -> Result<bool, ChainRefusal> {
    if e.chain != id {
        return Err(ChainRefusal::WrongChain);
    }
    if !e.is_intact() {
        return Err(ChainRefusal::Corrupt);
    }
    let Some((head_hash, head_at, head_epoch)) = model.last() else {
        return if e.seq == seq(0) && e.prev == Hash32::ZERO {
            Ok(true)
        } else {
            Err(ChainRefusal::NotGenesis)
        };
    };
    let next = model.len() as u64;
    let position = e.seq.get();
    if position > next {
        Err(ChainRefusal::Gap {
            expected: seq(next),
            got: e.seq,
        })
    } else if position == next {
        if e.prev != *head_hash {
            Err(ChainRefusal::WrongPrev {
                expected: *head_hash,
                got: e.prev,
            })
        } else if e.at < *head_at {
            Err(ChainRefusal::StampBackwards)
        } else if e.epoch < *head_epoch {
            Err(ChainRefusal::BelowRestoreEpoch {
                got: e.epoch,
                floor: *head_epoch,
            })
        } else {
            Ok(true)
        }
    } else {
        let have = model[position as usize].0;
        if have == e.hash {
            Ok(false)
        } else if position == 0 {
            Err(ChainRefusal::GenesisAgain { have, got: e.hash })
        } else {
            Err(ChainRefusal::Fork {
                at: e.seq,
                have,
                got: e.hash,
            })
        }
    }
}

/// A differential test: 300 seeded offer streams (valid entries in order, duplicates, forks,
/// corrupt copies, gaps, wrong chains, backwards stamps) against the model, one offer at a time. The
/// chain's verdict equals the model's, a refusal changes nothing, and an accepted entry moves the
/// head to it.
#[test]
fn chain_agrees_with_an_independent_model_on_seeded_offer_streams() {
    let id = team_log(1);
    let mut offers_seen = 0usize;
    let mut refusals_seen = std::collections::HashSet::new();
    for seed in 0..300u64 {
        let mut rng = Rng::new(0xC4A1_0000 + seed);
        let valid = build_chain(id, 12, &mut rng);
        let mut chain: Chain<TestEntry> = Chain::new(id);
        let mut model: Vec<(Hash32, Stamp, RestoreEpoch)> = Vec::new();
        let mut next_valid = 0usize;
        for _ in 0..60 {
            // One offer: mostly the next valid entry, sometimes a mess.
            let next_position = seq(model.len() as u64);
            let entry = match rng.below(14).unwrap_or(0) {
                0..=4 => valid.get(next_valid).cloned(),
                5 => rng.pick(&valid).cloned(), // a replay of anything
                6 => rng
                    .pick(&valid)
                    .map(|e| TestEntry::new(e.chain, e.seq, e.prev, e.at, b"a fork")), // a fork
                7 => rng.pick(&valid).map(TestEntry::corrupted),
                8 => rng.pick(&valid).map(|e| {
                    let mut moved = e.clone();
                    moved.chain = space(1);
                    moved
                }),
                9 => {
                    let skip = rng.below(3).unwrap_or(0) as usize;
                    valid.get(next_valid + 1 + skip).cloned() // a gap
                }
                // The next position with the right `prev` and a stamp one second early.
                10 => model.last().map(|(hash, stamp, restores)| {
                    let early = at(stamp.as_secs().saturating_sub(1));
                    TestEntry::new_in(id, next_position, *hash, early, *restores, b"backwards")
                }),
                // The next position with another predecessor.
                11 => model.last().map(|(_, stamp, restores)| {
                    TestEntry::new_in(
                        id,
                        next_position,
                        hash_of_fill(0x55),
                        *stamp,
                        *restores,
                        b"wrong prev",
                    )
                }),
                // The next position, linked and stamped right, in an OLDER restore epoch than the
                // chain stands in (the lost timeline of a restore); no such entry while the chain
                // is still in epoch 0.
                12 => model.last().and_then(|(hash, stamp, restores)| {
                    restores.get().checked_sub(1).map(|older| {
                        TestEntry::new_in(id, next_position, *hash, *stamp, epoch(older), b"lost")
                    })
                }),
                // The next position, linked and stamped right, in a NEWER epoch (a restore the
                // chain admits: the epoch only goes up).
                _ => model.last().map(|(hash, stamp, restores)| {
                    TestEntry::new_in(
                        id,
                        next_position,
                        *hash,
                        *stamp,
                        epoch(restores.get() + 1),
                        b"after a restore",
                    )
                }),
            };
            let Some(entry) = entry else { continue };
            let before = state(&chain);
            let expected = model_expect(&model, id, &entry);
            let got = chain.admit(entry.clone());
            offers_seen += 1;
            match (&expected, &got) {
                (Ok(true), Ok(admitted)) => {
                    assert!(admitted.is_new(), "seed {seed}: {entry:?}");
                    assert_eq!(admitted.head(), entry.head());
                    assert_eq!(chain.head(), Some(entry.head()));
                    model.push((entry.hash, entry.at, entry.epoch));
                    if valid.get(next_valid).map(|v| v.hash) == Some(entry.hash) {
                        next_valid += 1;
                    }
                }
                (Ok(false), Ok(admitted)) => {
                    assert!(!admitted.is_new(), "seed {seed}: {entry:?}");
                    assert_eq!(state(&chain), before);
                }
                (Err(want), Err(have)) => {
                    assert_eq!(want, have, "seed {seed}: {entry:?}");
                    assert_eq!(
                        state(&chain),
                        before,
                        "seed {seed}: a refusal changed the chain"
                    );
                    refusals_seen.insert(std::mem::discriminant(have));
                }
                _ => panic!("seed {seed}: model {expected:?}, chain {got:?} for {entry:?}"),
            }
            // The head the chain reports is the model's last entry, always.
            assert_eq!(
                chain.head().map(|h| (h.seq.get() + 1, h.hash)),
                model.last().map(|(hash, _, _)| (model.len() as u64, *hash))
            );
        }
    }
    assert!(offers_seen > 10_000, "{offers_seen} offers");
    // The stream met every kind of refusal that a chain can give but `Full`.
    assert!(
        refusals_seen.len() >= 9,
        "the streams met only {} kinds of refusal",
        refusals_seen.len()
    );
}

/// X-DELTA-W1S17A-3, the entries: a device that re-anchored a chain to the survivor of a restore
/// refuses the entries of the lost timeline, which chain correctly from the survivor but carry the old
/// restore epoch.
#[test]
fn an_entry_written_before_the_restore_is_refused() {
    let id = team_log(1);
    let entries = plain_chain(id, 8);
    // The restore of epoch 1 left the positions up to 3. The device re-anchored the chain there.
    let mut chain: Chain<TestEntry> =
        Chain::resume(entries[3].head(), entries[3].at, epoch(1), 100);
    assert_eq!(chain.epoch(), epoch(1));
    assert_eq!(chain.tip().epoch(), epoch(1));

    // Entry 4 of the lost timeline: the right `prev`, the right position, a good stamp, epoch 0.
    let lost = entries[4].clone();
    assert_eq!(lost.prev, entries[3].hash);
    let refusal = chain.admit(lost.clone()).expect_err("the lost timeline");
    assert_eq!(
        refusal,
        ChainRefusal::BelowRestoreEpoch {
            got: epoch(0),
            floor: epoch(1)
        }
    );
    assert_eq!(refusal.class(), RefusalClass::Alarm);
    assert_eq!(
        refusal.alarm(id),
        Some(Alarm::BelowRestoreEpoch {
            chain: id,
            got: epoch(0),
            floor: epoch(1)
        })
    );
    assert!(chain.is_empty() && chain.head() == Some(entries[3].head()));
    assert!(refusal.to_string().contains("epoch 0"), "{refusal}");
    // The same on the entry-less state.
    let tip = Tip::resume(entries[3].head(), entries[3].at, epoch(1));
    assert_eq!(
        tip.check(&lost),
        Err(ChainRefusal::BelowRestoreEpoch {
            got: epoch(0),
            floor: epoch(1)
        })
    );

    // Entry 4' of the restored timeline (epoch 1, a different entry) is admitted; so is a later
    // epoch (another restore); the epoch only goes up.
    let restored = entries[3].after_in(at(500), epoch(1), b"after the restore");
    assert!(chain.admit(restored.clone()).expect("restored").is_new());
    let again = restored.after_in(at(510), epoch(1), b"epoch 1 again");
    assert!(chain.admit(again.clone()).expect("same epoch").is_new());
    let second = again.after_in(at(520), epoch(3), b"a second restore");
    assert!(chain.admit(second.clone()).expect("a later epoch").is_new());
    assert_eq!(chain.epoch(), epoch(3));
    // After epoch 3, an entry of epoch 2 or 1 is refused, though the floor the device raised was 1.
    for older in [2u64, 1, 0] {
        let entry = second.after_in(at(530), epoch(older), b"older");
        assert_eq!(
            chain.admit(entry),
            Err(ChainRefusal::BelowRestoreEpoch {
                got: epoch(older),
                floor: epoch(3)
            }),
            "epoch {older}"
        );
    }
    // The order of the checks: a wrong `prev` and a backwards stamp are told before the epoch.
    let wrong_prev = TestEntry::new_in(id, seq(7), hash_of_fill(9), at(540), epoch(0), b"x");
    assert!(matches!(
        chain.admit(wrong_prev),
        Err(ChainRefusal::WrongPrev { .. })
    ));
    let early = second.after_in(at(1), epoch(0), b"x");
    assert_eq!(chain.admit(early), Err(ChainRefusal::StampBackwards));
}

#[test]
fn history_is_not_refused_for_its_epoch_only_new_entries_are() {
    // A chain from the genesis holds entries of epoch 0 and 1. The device then raises the floor to
    // 2 (a restore it accepted). The entries it holds are history: offered again they are
    // `AlreadyHeld`, a different one at a held position is still a fork, and only the NEXT entry is
    // judged by the epoch.
    let id = team_log(1);
    let mut chain: Chain<TestEntry> = Chain::new(id);
    let g = TestEntry::genesis(id, at(100), b"g");
    let e1 = g.after_in(at(110), epoch(1), b"e1");
    let e2 = e1.after_in(at(120), epoch(1), b"e2");
    for entry in [&g, &e1, &e2] {
        assert!(chain.admit(entry.clone()).expect("history").is_new());
    }
    chain.raise_floor(epoch(2));
    assert_eq!(chain.epoch(), epoch(2));
    for entry in [&g, &e1, &e2] {
        assert!(!chain.admit(entry.clone()).expect("held").is_new());
    }
    let other = TestEntry::new_in(id, seq(1), g.hash, at(110), epoch(1), b"other e1");
    assert!(matches!(chain.admit(other), Err(ChainRefusal::Fork { .. })));
    assert_eq!(
        chain.admit(e2.after_in(at(130), epoch(1), b"e3 of epoch 1")),
        Err(ChainRefusal::BelowRestoreEpoch {
            got: epoch(1),
            floor: epoch(2)
        })
    );
    assert!(chain
        .admit(e2.after_in(at(130), epoch(2), b"e3"))
        .expect("epoch 2")
        .is_new());

    // `raise_floor` never lowers; a genesis is judged by the floor as well.
    let mut tip = Tip::new(id);
    assert_eq!(tip.epoch(), RestoreEpoch::ZERO);
    tip.raise_floor(epoch(5));
    tip.raise_floor(epoch(2));
    assert_eq!(tip.epoch(), epoch(5));
    let low = TestEntry::new_in(id, seq(0), Hash32::ZERO, at(1), epoch(4), b"old genesis");
    assert_eq!(
        tip.check(&low),
        Err(ChainRefusal::BelowRestoreEpoch {
            got: epoch(4),
            floor: epoch(5)
        })
    );
    let right = TestEntry::new_in(id, seq(0), Hash32::ZERO, at(1), epoch(5), b"genesis");
    assert_eq!(tip.check(&right), Ok(Placement::Next));
    assert!(tip.advance(&right).expect("genesis").is_new());
    assert_eq!(tip.epoch(), epoch(5));
}
