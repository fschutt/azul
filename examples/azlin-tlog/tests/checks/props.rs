//! T33: 2,000 seeded random logs (8 tests of 250 runs, so that they run side by side). A run appends random
//! releases, makes checkpoints at random times, lets random subsets of random witnesses cosign them (a
//! witness that skips a round is asked later with a longer proof, one that was never asked starts from
//! nothing), and uses a random quorum. Then:
//!
//! - every checkpoint the quorum cosigned passes the policy, and `logged_since` is the quorum-th smallest
//!   time;
//! - the bundles of random leaves verify, also the bundle of an older checkpoint after the log grew, also
//!   through the text form;
//! - every single-byte mutation (replaced, deleted or inserted byte) of the text of a bundle that carries
//!   exactly the quorum's cosignatures fails (parse or verification);
//! - a mutation of a bundle that carries MORE cosignatures than the quorum fails or, when it only damaged the
//!   name or key hash of a redundant line (an unknown signer is ignored, C2SP `signed-note`), changes
//!   nothing about the claim and never makes `logged_since` earlier (drift row X-DELTA-W1S45C-13);
//! - the monitor follows the whole honest history and never raises an alarm, then indexes every release.
//!
//! The seed is in every failure message.

use azlin_tlog::{
    verify_bundle, Alarm, LeafIndex, Monitor, Note, Observed, PolicyError, ProofBundle, TreeSize,
};

use crate::common::{consistency, expect_of, record, t, Rng, World};

/// The note with the log's signature and its first `cosigs` cosignatures only.
fn restricted(note: &Note, cosigs: usize) -> Note {
    let sigs = note.sigs().iter().take(1 + cosigs).cloned().collect();
    Note::new(note.text().to_string(), sigs).unwrap()
}

/// `text` with one byte replaced, deleted or inserted.
fn mutate(rng: &mut Rng, text: &[u8]) -> Vec<u8> {
    let mut out = text.to_vec();
    let at = rng.pick(out.len());
    match rng.below(3) {
        0 => {
            let mut byte = (rng.next_u64() & 0xFF) as u8;
            if byte == out[at] {
                byte = byte.wrapping_add(1);
            }
            out[at] = byte;
        }
        1 => {
            out.remove(at);
        }
        _ => {
            out.insert(at, (rng.next_u64() & 0xFF) as u8);
        }
    }
    out
}

fn one_run(seed: u64) {
    let mut rng = Rng::new(seed);
    let witnesses = 1 + rng.pick(3);
    let quorum = 1 + rng.pick(witnesses);
    let mut world = World::new(witnesses);
    let policy = world.policy(quorum as u8);
    let mut monitor = Monitor::new(world.policy(quorum as u8));
    let mut now = 1_000u64;
    let mut size = 0usize;
    // the checkpoints the quorum cosigned: the note, its size, the cosignature times in the note's order
    let mut witnessed: Vec<(Note, usize, Vec<u64>)> = Vec::new();
    let mut held: Option<usize> = None;
    for round in 0..1 + rng.pick(3) {
        size += 1 + rng.pick(8);
        world.grow_to(size);
        let note = world.note();
        let mut at: Vec<(usize, u64)> = Vec::new();
        for w in 0..witnesses {
            if rng.chance(3, 4) {
                at.push((w, now + rng.below(50)));
            }
        }
        let cosigned = world.cosigned(&note, &at);
        now += 100;
        let result = policy.check(&cosigned, t(now));
        if at.len() < quorum {
            assert!(
                matches!(result, Err(PolicyError::QuorumNotMet { .. })),
                "seed {seed} round {round}: {result:?}"
            );
            continue;
        }
        let ok = result.unwrap_or_else(|e| panic!("seed {seed} round {round}: {e:?}"));
        let times: Vec<u64> = at.iter().map(|&(_, time)| time).collect();
        let mut sorted = times.clone();
        sorted.sort_unstable();
        assert_eq!(
            ok.logged_since(),
            t(sorted[quorum - 1]),
            "seed {seed}: the quorum-th smallest time"
        );
        assert_eq!(ok.cosigners().len(), at.len(), "seed {seed}");
        // the monitor follows an honest history without an alarm
        let seen = match held {
            None => monitor.observe(&cosigned, t(now), None),
            Some(old) => {
                let proof = consistency(&world, old as u64, &cosigned);
                monitor.observe(&cosigned, t(now), Some(&proof))
            }
        };
        match (held, seen) {
            (None, Ok(Observed::First { size: s })) => {
                assert_eq!(s, TreeSize(size as u64), "seed {seed}")
            }
            (Some(old), Ok(Observed::Grew { from, to })) => {
                assert_eq!(
                    (from, to),
                    (TreeSize(old as u64), TreeSize(size as u64)),
                    "seed {seed}"
                );
            }
            (_, other) => panic!("seed {seed} round {round}: the monitor said {other:?}"),
        }
        held = Some(size);
        witnessed.push((cosigned, size, times));
    }
    let Some((last_note, last_size, last_times)) = witnessed.last().cloned() else {
        return;
    };

    // the monitor indexes every release of the final checkpoint
    let records: Vec<Vec<u8>> = (0..last_size).map(|i| record(i).to_bytes()).collect();
    monitor
        .ingest(&records)
        .unwrap_or_else(|e: Alarm| panic!("seed {seed}: ingest: {e:?}"));
    for i in 0..last_size {
        assert_eq!(
            monitor.knows(&record(i).digest),
            Some(&record(i)),
            "seed {seed}: release {i}"
        );
    }

    // bundles of random leaves of the last checkpoint verify, also through the text form
    let mut leaves = vec![rng.pick(last_size), last_size - 1];
    leaves.dedup();
    let mut sorted = last_times.clone();
    sorted.sort_unstable();
    for &i in &leaves {
        let bundle = world.log.bundle(LeafIndex(i as u64), &last_note).unwrap();
        let logged = verify_bundle(&bundle, &policy, t(now), &expect_of(i))
            .unwrap_or_else(|e| panic!("seed {seed}: leaf {i}: {e:?}"));
        assert_eq!(logged.record(), &record(i), "seed {seed}");
        assert_eq!(logged.index(), LeafIndex(i as u64), "seed {seed}");
        assert_eq!(logged.logged_since(), t(sorted[quorum - 1]), "seed {seed}");
        let text = bundle.to_text().unwrap();
        assert_eq!(
            ProofBundle::parse(text.as_bytes()).unwrap(),
            bundle,
            "seed {seed}"
        );
    }
    // the bundle of an older checkpoint verifies after the log grew
    if witnessed.len() > 1 {
        let (old_note, old_size, old_times) = &witnessed[0];
        let i = rng.pick(*old_size);
        let bundle = world.log.bundle(LeafIndex(i as u64), old_note).unwrap();
        let logged = verify_bundle(&bundle, &policy, t(now), &expect_of(i))
            .unwrap_or_else(|e| panic!("seed {seed}: old leaf {i}: {e:?}"));
        let mut times = old_times.clone();
        times.sort_unstable();
        assert_eq!(logged.logged_since(), t(times[quorum - 1]), "seed {seed}");
        assert_eq!(
            logged.checkpoint().size(),
            TreeSize(*old_size as u64),
            "seed {seed}"
        );
    }

    // the bundle with exactly the quorum's cosignatures: every single-byte mutation fails
    let leaf = rng.pick(last_size);
    let exact_note = restricted(&last_note, quorum);
    let exact = world
        .log
        .bundle(LeafIndex(leaf as u64), &exact_note)
        .unwrap();
    verify_bundle(&exact, &policy, t(now), &expect_of(leaf))
        .unwrap_or_else(|e| panic!("seed {seed}: the exact bundle: {e:?}"));
    let exact_text = exact.to_text().unwrap().into_bytes();
    for k in 0..5 {
        let mutated = mutate(&mut rng, &exact_text);
        let accepted = match ProofBundle::parse(&mutated) {
            Err(_) => false,
            Ok(parsed) => verify_bundle(&parsed, &policy, t(now), &expect_of(leaf)).is_ok(),
        };
        assert!(
            !accepted,
            "seed {seed} mutation {k}: a mutated bundle was accepted: {:?}",
            String::from_utf8_lossy(&mutated)
        );
    }

    // the bundle with every cosignature: a mutation fails, or it changed nothing that the claim rests on
    let full = world
        .log
        .bundle(LeafIndex(leaf as u64), &last_note)
        .unwrap();
    let original = verify_bundle(&full, &policy, t(now), &expect_of(leaf)).unwrap();
    let full_text = full.to_text().unwrap().into_bytes();
    for k in 0..5 {
        let mutated = mutate(&mut rng, &full_text);
        let Ok(parsed) = ProofBundle::parse(&mutated) else {
            continue;
        };
        if let Ok(logged) = verify_bundle(&parsed, &policy, t(now), &expect_of(leaf)) {
            assert_eq!(
                logged.record(),
                original.record(),
                "seed {seed} mutation {k}"
            );
            assert_eq!(logged.index(), original.index(), "seed {seed} mutation {k}");
            assert_eq!(
                logged.checkpoint(),
                original.checkpoint(),
                "seed {seed} mutation {k}"
            );
            assert!(
                logged.logged_since() >= original.logged_since(),
                "seed {seed} mutation {k}: an earlier time"
            );
            assert!(
                logged.cosigners().len() <= original.cosigners().len(),
                "seed {seed} mutation {k}"
            );
        }
    }
}

/// The whole bundle, every byte: each position replaced two ways, incremented, deleted, and a byte inserted in
/// front of it. The bundle carries exactly the quorum's cosignatures, so none of these is a redundant byte.
#[test]
fn t33_every_single_byte_mutation_of_a_bundle_fails() {
    let mut world = World::new(3);
    world.grow_to(12);
    let note = world.note();
    let note = world.cosigned(&note, &[(0, 100), (1, 150), (2, 200)]);
    let policy = world.policy(2);
    let exact = restricted(&note, 2);
    let bundle = world.log.bundle(LeafIndex(7), &exact).unwrap();
    let text = bundle.to_text().unwrap().into_bytes();
    let accepts = |bytes: &[u8]| match ProofBundle::parse(bytes) {
        Err(_) => false,
        Ok(parsed) => verify_bundle(&parsed, &policy, t(1_000), &expect_of(7)).is_ok(),
    };
    assert!(accepts(&text), "the bundle itself verifies");
    assert!(text.len() > 1_000, "{} bytes", text.len());
    let mut tried = 0usize;
    for i in 0..text.len() {
        for edit in 0..5 {
            let mut mutated = text.clone();
            match edit {
                0 => mutated[i] ^= 0x01,
                1 => mutated[i] ^= 0x20,
                2 => mutated[i] = mutated[i].wrapping_add(1),
                3 => {
                    mutated.remove(i);
                }
                _ => mutated.insert(i, b'A'),
            }
            if mutated == text {
                continue;
            }
            tried += 1;
            assert!(
                !accepts(&mutated),
                "edit {edit} of byte {i} was accepted: {:?}",
                String::from_utf8_lossy(&mutated)
            );
        }
    }
    assert!(tried > 5_000, "{tried} mutations tried");
}

fn run(from: u64, to: u64) {
    for seed in from..to {
        one_run(seed);
    }
}

#[test]
fn t33_property_random_logs_a() {
    run(0, 250);
}

#[test]
fn t33_property_random_logs_b() {
    run(250, 500);
}

#[test]
fn t33_property_random_logs_c() {
    run(500, 750);
}

#[test]
fn t33_property_random_logs_d() {
    run(750, 1_000);
}

#[test]
fn t33_property_random_logs_e() {
    run(1_000, 1_250);
}

#[test]
fn t33_property_random_logs_f() {
    run(1_250, 1_500);
}

#[test]
fn t33_property_random_logs_g() {
    run(1_500, 1_750);
}

#[test]
fn t33_property_random_logs_h() {
    run(1_750, 2_000);
}
