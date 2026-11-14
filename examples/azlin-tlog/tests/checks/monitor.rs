//! T30-T32: the monitor.

use azlin_merkle::{MemStore, Tree};
use azlin_tlog::{
    sign_checkpoint, Alarm, Checkpoint, ConsistencyProof, Cosigner, Digest, Hash, LeafIndex,
    Monitor, Note, Observed, Origin, PolicyError, RecordError, TreeSize, MAX_INDEX_RECORDS,
};

use crate::common::{consistency, log_key, record, size_of, t, witness_key, World};

/// A world of three witnesses grown to `size` releases, its note cosigned by witnesses `by` at `time`.
fn noted(world: &mut World, size: usize, by: &[usize], time: u64) -> Note {
    world.grow_to(size);
    let note = world.note();
    let at: Vec<(usize, u64)> = by.iter().map(|&w| (w, time + w as u64)).collect();
    world.cosigned(&note, &at)
}

const NOW: u64 = 10_000;

#[test]
fn t30_the_monitor_accepts_growth_and_alarms_on_a_fork() {
    let mut world = World::new(3);
    let c5 = noted(&mut world, 5, &[0, 1], 100);
    let mut monitor = Monitor::new(world.policy(2));
    assert_eq!(monitor.checkpoint(), None);
    assert_eq!(
        monitor.observe(&c5, t(NOW), None),
        Ok(Observed::First { size: TreeSize(5) })
    );
    assert_eq!(monitor.checkpoint().unwrap().size(), TreeSize(5));
    // the same checkpoint again, with other cosigners, is no news
    assert_eq!(monitor.observe(&c5, t(NOW), None), Ok(Observed::Unchanged));
    let n5 = world.note();
    let other_cosigners = world.cosigned(&n5, &[(0, 150), (2, 160)]);
    assert_ne!(other_cosigners, c5);
    assert_eq!(
        monitor.observe(&other_cosigners, t(NOW), None),
        Ok(Observed::Unchanged)
    );
    // growth: without a proof the monitor asks for one, with the log's proof it follows
    let c9 = noted(&mut world, 9, &[0, 2], 200);
    assert_eq!(
        monitor.observe(&c9, t(NOW), None),
        Ok(Observed::NeedsProof { have: TreeSize(5) })
    );
    assert_eq!(
        monitor.checkpoint().unwrap().size(),
        TreeSize(5),
        "nothing moved"
    );
    let proof = consistency(&world, 5, &c9);
    assert_eq!(
        monitor.observe(&c9, t(NOW), Some(&proof)),
        Ok(Observed::Grew {
            from: TreeSize(5),
            to: TreeSize(9)
        })
    );
    assert_eq!(monitor.checkpoint().unwrap().size(), TreeSize(9));
    assert_eq!(monitor.observe(&c9, t(NOW), None), Ok(Observed::Unchanged));
    // and on: 9 -> 40 -> 41
    let c40 = noted(&mut world, 40, &[1, 2], 300);
    let proof = consistency(&world, 9, &c40);
    assert_eq!(
        monitor.observe(&c40, t(NOW), Some(&proof)),
        Ok(Observed::Grew {
            from: TreeSize(9),
            to: TreeSize(40)
        })
    );
    let c41 = noted(&mut world, 41, &[0, 1, 2], 400);
    let proof = consistency(&world, 40, &c41);
    assert_eq!(
        monitor.observe(&c41, t(NOW), Some(&proof)),
        Ok(Observed::Grew {
            from: TreeSize(40),
            to: TreeSize(41)
        })
    );

    // a fork: the same log key signs a second checkpoint of size 9 with another root, and the witnesses of
    // that history cosign it: both notes are valid under the policy, the monitor alarms and carries both
    let mut a = World::new(3);
    let mut b = World::new(3);
    a.grow_to(5);
    b.grow_to(5);
    for i in 0..4 {
        a.log.append(&record(5 + i)).unwrap();
        b.log.append(&record(500 + i)).unwrap();
    }
    assert_ne!(a.log.root().unwrap(), b.log.root().unwrap());
    let (note_a, note_b) = (a.note(), b.note());
    let note_a = a.cosigned(&note_a, &[(0, 100), (1, 110)]);
    let note_b = b.cosigned(&note_b, &[(1, 120), (2, 130)]);
    assert_eq!(size_of(&note_a), size_of(&note_b));
    let mut monitor = Monitor::new(a.policy(2));
    assert_eq!(
        monitor.observe(&note_a, t(NOW), None),
        Ok(Observed::First { size: TreeSize(9) })
    );
    match monitor.observe(&note_b, t(NOW), None) {
        Err(Alarm::Fork {
            a: first,
            b: second,
        }) => {
            assert_eq!(first, note_a, "the note the monitor held");
            assert_eq!(second, note_b, "the note that disagrees with it");
        }
        other => panic!("{other:?}"),
    }
    // the evidence is kept and the alarm repeats; the held checkpoint is the first one
    assert!(matches!(
        monitor.observe(&note_b, t(NOW), None),
        Err(Alarm::Fork { .. })
    ));
    assert_eq!(
        monitor.observe(&note_a, t(NOW), None),
        Ok(Observed::Unchanged)
    );
    // a fork seen from the other side
    let mut monitor = Monitor::new(a.policy(2));
    monitor.observe(&note_b, t(NOW), None).unwrap();
    assert!(matches!(
        monitor.observe(&note_a, t(NOW), None),
        Err(Alarm::Fork { .. })
    ));
}

#[test]
fn t31_the_monitor_alarms_on_shrinking_and_a_bad_consistency_proof() {
    let mut world = World::new(3);
    let c5 = noted(&mut world, 5, &[0, 1], 100);
    let c9 = noted(&mut world, 9, &[0, 1], 200);
    let good = consistency(&world, 5, &c9);
    let policy = world.policy(2);

    // shrinking: a validly cosigned older checkpoint after a newer one
    let mut monitor = Monitor::new(policy.clone());
    monitor.observe(&c9, t(NOW), None).unwrap();
    assert_eq!(
        monitor.observe(&c5, t(NOW), None),
        Err(Alarm::Shrunk {
            have: TreeSize(9),
            seen: TreeSize(5)
        })
    );
    assert_eq!(
        monitor.checkpoint().unwrap().size(),
        TreeSize(9),
        "the held checkpoint is the larger"
    );

    // a consistency proof that does not verify: every damage, the monitor keeps the checkpoint it holds
    let mut fork_world = World::new(3);
    fork_world.grow_to(5);
    for i in 0..4 {
        fork_world.log.append(&record(700 + i)).unwrap();
    }
    let fork_note = fork_world.note();
    let fork_proof = consistency(&fork_world, 5, &fork_note);
    let mut flipped = good.clone();
    flipped.path[0].0[7] ^= 1;
    let mut dropped = good.clone();
    dropped.path.pop();
    let mut extra = good.clone();
    extra.path.push(Hash([1; 32]));
    let wrong_old = ConsistencyProof {
        old: TreeSize(4),
        ..good.clone()
    };
    let wrong_new = ConsistencyProof {
        new: TreeSize(10),
        ..good.clone()
    };
    let empty = ConsistencyProof {
        path: vec![],
        ..good.clone()
    };
    let for_another_pair = consistency(&world, 3, &c9);
    let mut monitor = Monitor::new(policy.clone());
    monitor.observe(&c5, t(NOW), None).unwrap();
    for (what, proof) in [
        ("a flipped hash", flipped),
        ("a hash dropped", dropped),
        ("a hash added", extra),
        ("the wrong old size", wrong_old),
        ("the wrong new size", wrong_new),
        ("no hashes", empty),
        ("a proof from 3", for_another_pair),
        (
            "the proof of a forked history",
            ConsistencyProof {
                old: TreeSize(5),
                new: TreeSize(9),
                path: fork_proof.path,
            },
        ),
    ] {
        assert_eq!(
            monitor.observe(&c9, t(NOW), Some(&proof)),
            Err(Alarm::BadConsistency),
            "{what}"
        );
        assert_eq!(
            monitor.checkpoint().unwrap().size(),
            TreeSize(5),
            "{what}: nothing moved"
        );
    }
    // the forked history's own checkpoint with the honest log's proof: not consistent either
    let fork_cosigned = fork_world.cosigned(&fork_note, &[(0, 300), (1, 310)]);
    assert_eq!(
        monitor.observe(&fork_cosigned, t(NOW), Some(&good)),
        Err(Alarm::BadConsistency)
    );
    // the right proof is still accepted afterwards
    assert_eq!(
        monitor.observe(&c9, t(NOW), Some(&good)),
        Ok(Observed::Grew {
            from: TreeSize(5),
            to: TreeSize(9)
        })
    );

    // a note that is not witnessed raises NotWitnessed, whatever its size
    let mut monitor = Monitor::new(policy.clone());
    let one = noted(&mut world, 9, &[2], 500);
    assert_eq!(
        monitor.observe(&one, t(NOW), None),
        Err(Alarm::NotWitnessed(PolicyError::QuorumNotMet {
            have: 1,
            need: 2
        }))
    );
    assert_eq!(
        monitor.checkpoint(),
        None,
        "a note that is not witnessed is not remembered"
    );
    // a cosignature from the future of the monitor's clock
    let late = world.cosigned(&one, &[(0, 5_000), (1, 5_010)]);
    assert!(matches!(
        monitor.observe(&late, t(1_000), None),
        Err(Alarm::NotWitnessed(PolicyError::FutureCosignature { .. }))
    ));
    assert_eq!(monitor.checkpoint(), None);
}

/// A cosigned checkpoint over the leaf data `datas` (not records of a `Log`): signed by the log key, cosigned
/// by witnesses 0 and 1.
fn manual_note(datas: &[Vec<u8>]) -> Note {
    let mut tree = Tree::new(MemStore::new()).unwrap();
    for data in datas {
        tree.append(data).unwrap();
    }
    let cp = Checkpoint::new(Origin::azlin(), tree.size(), tree.root().unwrap(), vec![]).unwrap();
    let note = sign_checkpoint(&cp, &log_key()).unwrap();
    let first = witness_key(0).cosign(note.text(), t(100)).unwrap();
    let second = witness_key(1).cosign(note.text(), t(110)).unwrap();
    note.with_sig(first).unwrap().with_sig(second).unwrap()
}

#[test]
fn t32_the_digest_index() {
    let mut world = World::new(3);
    let c12 = noted(&mut world, 12, &[0, 1], 100);
    let records: Vec<Vec<u8>> = (0..12).map(|i| record(i).to_bytes()).collect();
    let mut monitor = Monitor::new(world.policy(2));
    // nothing is known before a checkpoint was observed
    assert_eq!(monitor.ingest(&records), Err(Alarm::NoCheckpoint));
    assert_eq!(monitor.indexed(), None);
    monitor.observe(&c12, t(NOW), None).unwrap();
    assert_eq!(
        monitor.knows(&record(3).digest),
        None,
        "observing is not ingesting"
    );
    monitor.ingest(&records).unwrap();
    assert_eq!(monitor.indexed(), Some(TreeSize(12)));
    for i in 0..12 {
        assert_eq!(
            monitor.knows(&record(i).digest),
            Some(&record(i)),
            "release {i}"
        );
    }
    // a digest the log does not hold
    assert_eq!(
        monitor.knows(&Digest::of(b"a build made for some users")),
        None
    );
    assert_eq!(monitor.knows(&record(12).digest), None);
    // a list one record short, one record too long, and the empty list: refused, the index is as it was
    assert_eq!(
        monitor.ingest(&records[..11]),
        Err(Alarm::RecordCount {
            want: TreeSize(12),
            have: 11
        })
    );
    let mut longer = records.clone();
    longer.push(record(12).to_bytes());
    assert_eq!(
        monitor.ingest(&longer),
        Err(Alarm::RecordCount {
            want: TreeSize(12),
            have: 13
        })
    );
    assert_eq!(
        monitor.ingest(&[]),
        Err(Alarm::RecordCount {
            want: TreeSize(12),
            have: 0
        })
    );
    // a list that does not hash to the checkpoint's root: one record replaced, two swapped
    let mut replaced = records.clone();
    replaced[5] = record(50).to_bytes();
    assert_eq!(monitor.ingest(&replaced), Err(Alarm::RecordsMismatch));
    let mut swapped = records.clone();
    swapped.swap(2, 9);
    assert_eq!(monitor.ingest(&swapped), Err(Alarm::RecordsMismatch));
    let mut altered = records.clone();
    altered[0][20] ^= 0x01;
    assert_eq!(monitor.ingest(&altered), Err(Alarm::RecordsMismatch));
    for i in 0..12 {
        assert!(
            monitor.knows(&record(i).digest).is_some(),
            "the refusals left the index alone"
        );
    }
    // the log grows: the index covers the old size until the list of the whole tree is ingested again
    let c15 = noted(&mut world, 15, &[0, 1], 200);
    let proof = consistency(&world, 12, &c15);
    monitor.observe(&c15, t(NOW), Some(&proof)).unwrap();
    assert_eq!(monitor.checkpoint().unwrap().size(), TreeSize(15));
    assert_eq!(monitor.indexed(), Some(TreeSize(12)));
    assert_eq!(monitor.knows(&record(13).digest), None);
    assert_eq!(
        monitor.ingest(&records),
        Err(Alarm::RecordCount {
            want: TreeSize(15),
            have: 12
        })
    );
    let all: Vec<Vec<u8>> = (0..15).map(|i| record(i).to_bytes()).collect();
    monitor.ingest(&all).unwrap();
    assert_eq!(monitor.indexed(), Some(TreeSize(15)));
    assert_eq!(monitor.knows(&record(13).digest), Some(&record(13)));

    // a leaf that is no release record, in a log whose root does match: the log is misbehaving
    let garbage = vec![
        record(0).to_bytes(),
        b"not a release record\n".to_vec(),
        record(2).to_bytes(),
    ];
    let mut monitor = Monitor::new(world.policy(2));
    monitor
        .observe(&manual_note(&garbage), t(NOW), None)
        .unwrap();
    assert_eq!(
        monitor.ingest(&garbage),
        Err(Alarm::BadRecord {
            index: LeafIndex(1),
            error: RecordError::Header
        })
    );
    // the same release twice: a build for some users beside the one for everyone
    let mut tailored = record(1);
    tailored.digest = Digest::of(b"tailored");
    let double = vec![
        record(1).to_bytes(),
        record(2).to_bytes(),
        tailored.to_bytes(),
    ];
    let mut monitor = Monitor::new(world.policy(2));
    monitor
        .observe(&manual_note(&double), t(NOW), None)
        .unwrap();
    assert_eq!(
        monitor.ingest(&double),
        Err(Alarm::Equivocation {
            first: LeafIndex(0),
            second: LeafIndex(2)
        })
    );
    assert_eq!(monitor.indexed(), None);
    let repeated = vec![record(4).to_bytes(), record(4).to_bytes()];
    let mut monitor = Monitor::new(world.policy(2));
    monitor
        .observe(&manual_note(&repeated), t(NOW), None)
        .unwrap();
    assert_eq!(
        monitor.ingest(&repeated),
        Err(Alarm::Equivocation {
            first: LeafIndex(0),
            second: LeafIndex(1)
        })
    );
    // a list that is not authentic accuses nobody: the same garbage under a root that does not match
    let mut monitor = Monitor::new(world.policy(2));
    monitor
        .observe(
            &manual_note(&[
                record(0).to_bytes(),
                record(1).to_bytes(),
                record(2).to_bytes(),
            ]),
            t(NOW),
            None,
        )
        .unwrap();
    assert_eq!(monitor.ingest(&garbage), Err(Alarm::RecordsMismatch));
    assert_eq!(monitor.ingest(&double), Err(Alarm::RecordsMismatch));
    // one digest under two names: the earlier record answers
    let mut same_digest = record(6);
    same_digest.digest = record(5).digest;
    let twins = vec![record(5).to_bytes(), same_digest.to_bytes()];
    let mut monitor = Monitor::new(world.policy(2));
    monitor.observe(&manual_note(&twins), t(NOW), None).unwrap();
    monitor.ingest(&twins).unwrap();
    assert_eq!(monitor.knows(&record(5).digest), Some(&record(5)));
    // the limit: 5 records in an index of 5, never an eviction at 6
    let mut small = Monitor::with_limit(world.policy(2), 5);
    let five: Vec<Vec<u8>> = (0..5).map(|i| record(i).to_bytes()).collect();
    small.observe(&manual_note(&five), t(NOW), None).unwrap();
    small.ingest(&five).unwrap();
    assert_eq!(small.indexed(), Some(TreeSize(5)));
    let six: Vec<Vec<u8>> = (0..6).map(|i| record(i).to_bytes()).collect();
    let six_note = manual_note(&six);
    let mut probe = Monitor::with_limit(world.policy(2), 5);
    probe.observe(&six_note, t(NOW), None).unwrap();
    assert_eq!(probe.ingest(&six), Err(Alarm::TooMany));
    let proof = ConsistencyProof {
        old: TreeSize(5),
        new: TreeSize(6),
        path: {
            let mut tree = Tree::new(MemStore::new()).unwrap();
            for r in &six {
                tree.append(r).unwrap();
            }
            tree.consistency_proof(TreeSize(5), TreeSize(6))
                .unwrap()
                .path
        },
    };
    small.observe(&six_note, t(NOW), Some(&proof)).unwrap();
    assert_eq!(small.ingest(&six), Err(Alarm::TooMany));
    assert_eq!(
        small.indexed(),
        Some(TreeSize(5)),
        "the index kept what it had"
    );
    for i in 0..5 {
        assert!(
            small.knows(&record(i).digest).is_some(),
            "no record was evicted"
        );
    }
    assert_eq!(MAX_INDEX_RECORDS, 1_000_000);
}
