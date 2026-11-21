//! S1-10: the policy world (planning/architecture.md section 5.10 "groups inherit and may only tighten",
//! section 5.11 `policy_epoch`, section 12 E1 "a quorum counts distinct people"). 2,000 random trees and
//! patch sequences: the policy invariants hold; every field is reachable by `random_patch`; the quorum
//! oracle agrees with `quorum_met`.

use std::collections::BTreeSet;

use azlin_policy::Field;
use azlin_seams::sim::Rng;
use azlin_sim::{random_patch, Defect, PolicyKind, PolicyWorld};

#[test]
fn s1_10_every_field_is_reachable_by_a_random_patch() {
    let mut rng = Rng::new(0x51_10);
    let mut seen: BTreeSet<Field> = BTreeSet::new();
    for _ in 0..5_000 {
        for field in random_patch(&mut rng).sets() {
            seen.insert(field);
        }
    }
    let missing: Vec<&str> = Field::ALL
        .iter()
        .filter(|f| !seen.contains(*f))
        .map(|f| f.name())
        .collect();
    assert!(
        missing.is_empty(),
        "fields never drawn in 5,000 patches: {missing:?}"
    );
}

#[test]
fn s1_10_two_thousand_random_trees_keep_the_policy_invariants() {
    let mut quorum_checks = 0u64;
    for seed in 0..2_000u64 {
        let mut world = PolicyWorld::new(seed, Defect::None);
        let mut rng = Rng::new(0xB01 + seed);
        for step in 0..12u32 {
            let events = world.step(
                (step % 3 == 0) as u8,
                (step % 8) as u8,
                rng.next_u64() as u32,
                &mut rng,
            );
            assert_eq!(events.len(), 3);
            for event in events {
                assert!(
                    event.ok,
                    "seed {seed} step {step}: {:?} broke: {}",
                    event.kind, event.message
                );
                if event.kind == PolicyKind::Quorum {
                    quorum_checks += 1;
                }
            }
        }
    }
    assert!(quorum_checks >= 24_000);
}

#[test]
fn s1_10_each_faulty_double_of_the_policy_world_is_caught() {
    for (defect, kind) in [
        (Defect::PolicySkipsCheck, PolicyKind::Tightens),
        (Defect::PolicyAlwaysBumps, PolicyKind::EpochBump),
        (Defect::QuorumCountsDevices, PolicyKind::Quorum),
    ] {
        let mut caught = false;
        'seeds: for seed in 0..300u64 {
            let mut world = PolicyWorld::new(seed, defect);
            let mut rng = Rng::new(0xB01 + seed);
            for step in 0..40u32 {
                let events = world.step(
                    (step % 3 == 0) as u8,
                    (step % 8) as u8,
                    rng.next_u64() as u32,
                    &mut rng,
                );
                if events.iter().any(|e| e.kind == kind && !e.ok) {
                    caught = true;
                    break 'seeds;
                }
            }
        }
        assert!(caught, "{defect:?} was never caught as a broken {kind:?}");
    }
}
