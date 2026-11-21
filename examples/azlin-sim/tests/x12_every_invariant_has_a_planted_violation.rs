//! X12: every invariant has a planted violation (planning/architecture.md section 1.4 rule 2: a simulator
//! that finds what a test of one crate cannot, and proves it can fail). For every invariant of stage 1
//! `planted_<name>` plants a violation with a harness-side faulty double or a directed adversary and the run
//! is RED with exactly that invariant (a causal sibling only where the table of `common/mod.rs` says so),
//! with `FailureKind::Product`; an invariant whose status is a known gap is planted by its own seed file.

mod common;

use azlin_sim::{FailureKind, InvariantId, Status, Verdict, INVARIANTS};

fn planted(invariant: InvariantId) {
    let table = common::planted();
    let Some(plant) = table.iter().find(|p| p.invariant == invariant) else {
        panic!("no planted violation of {}", invariant.name());
    };
    let Some((variant, _, outcome)) = plant.run() else {
        panic!(
            "the plant of {} (variants 0..{}) never went red with an accepted invariant\n{}",
            invariant.name(),
            plant.variants,
            plant.diagnose()
        );
    };
    let Verdict::Red(failure) = &outcome.verdict else {
        panic!("unreachable: a plant is red");
    };
    println!(
        "{} planted at variant {variant}: {} at step {}: {}",
        invariant.name(),
        failure.invariant.name(),
        failure.at_step,
        failure.message
    );
    assert_eq!(
        failure.invariant,
        invariant,
        "the plant of {} went red with {}",
        invariant.name(),
        failure.invariant.name()
    );
    assert_eq!(failure.kind, FailureKind::Product);
}

macro_rules! plant {
    ($($name:ident => $id:ident),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                planted(InvariantId::$id);
            }
        )+
    };
}

plant! {
    planted_inv_chain_every_copy_verifies_from_genesis => ChainEveryCopyVerifiesFromGenesis,
    planted_inv_chain_no_fork_is_ever_accepted => ChainNoForkIsEverAccepted,
    planted_inv_chain_checkpoint_equals_genesis => ChainCheckpointEqualsGenesis,
    planted_inv_chain_a_checkpoint_is_written_when_due => ChainACheckpointIsWrittenWhenDue,
    planted_inv_pin_never_goes_backwards => PinNeverGoesBackwards,
    planted_inv_pin_only_holds_heads_the_group_signed => PinOnlyHoldsHeadsTheGroupSigned,
    planted_inv_restore_only_through_the_signed_epoch_path => RestoreOnlyThroughTheSignedEpochPath,
    planted_inv_restore_the_lost_timeline_stays_lost => RestoreTheLostTimelineStaysLost,
    planted_inv_restore_a_late_record_keeps_progress => RestoreALateRecordKeepsProgress,
    planted_inv_restore_every_listed_chain_is_reanchored => RestoreEveryListedChainIsReanchored,
    planted_inv_restore_held_entries_are_sent_again => RestoreHeldEntriesAreSentAgain,
    planted_inv_restore_heals_every_device => RestoreHealsEveryDevice,
    planted_inv_peer_a_hidden_head_shows => PeerAHiddenHeadShows,
    planted_inv_peer_a_peers_junk_is_no_alarm => PeerAPeersJunkIsNoAlarm,
    planted_inv_stamp_never_runs_backwards => StampNeverRunsBackwards,
    planted_inv_leader_acknowledged_entries_survive_an_honest_disk => LeaderAcknowledgedEntriesSurviveAnHonestDisk,
    planted_inv_leader_no_silent_rollback_window => LeaderNoSilentRollbackWindow,
    planted_inv_leader_one_head_per_position_and_epoch => LeaderOneHeadPerPositionAndEpoch,
    planted_inv_device_restart_recovers_a_state_it_had => DeviceRestartRecoversAStateItHad,
    planted_inv_refusal_every_refusal_is_typed_and_classed => RefusalEveryRefusalIsTypedAndClassed,
    planted_inv_limits_nothing_grows_without_a_limit => LimitsNothingGrowsWithoutALimit,
    planted_inv_policy_a_group_only_tightens => PolicyAGroupOnlyTightens,
    planted_inv_policy_epoch_bumps_only_on_change => PolicyEpochBumpsOnlyOnChange,
    planted_inv_policy_quorum_counts_distinct_people => PolicyQuorumCountsDistinctPeople,
}

#[test]
fn x12_every_invariant_of_the_registry_names_its_planted_test() {
    let table = common::planted();
    for info in INVARIANTS {
        assert!(info.planted.starts_with("planted_"), "{}", info.name);
        let in_table = table.iter().any(|p| p.invariant == info.id);
        let known_gap = matches!(info.status, Status::KnownGap(_));
        // The invariant of the pin's epoch is planted by the seed file of its known gap (a finding of the
        // crate itself), every other one by the table.
        if info.id == InvariantId::PinTheEpochNeverGoesDown {
            continue;
        }
        assert!(
            in_table || known_gap,
            "{} has no planted violation",
            info.name
        );
        let source = include_str!("x12_every_invariant_has_a_planted_violation.rs");
        assert!(
            source.contains(&format!("fn {}", info.planted))
                || source.contains(&format!("{} =>", info.planted)),
            "the test {} is missing",
            info.planted
        );
    }
}
