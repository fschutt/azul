//! The planted violations of the invariants (X12), shared by `x12_*` and `x07_*`: for every invariant a
//! harness-side faulty double (a `Defect`, a `LeaderVariant`) or a directed adversary, a list of steps that
//! exposes it, and the invariants that may be the one that goes red (the specific one first; a causal
//! sibling only where the same defect breaks both). A plant may try several salts: the first variant that
//! goes red with an accepted invariant wins.
#![allow(dead_code)]

use azlin_sim::{
    small_world, AttackKind, Config, Defect, InvariantId, LeaderVariant, Outcome, Scenario,
    SigChoice, Step, Steps, Verdict, DIRECTED_SEED,
};

pub struct Planted {
    pub invariant: InvariantId,
    pub defect: Defect,
    pub config: fn() -> Config,
    pub variants: u32,
    pub steps: fn(u32) -> Vec<Step>,
    pub accept: &'static [InvariantId],
}

impl Planted {
    /// The scenario of a variant.
    pub fn scenario(&self, variant: u32) -> Scenario {
        match Scenario::new(DIRECTED_SEED + u64::from(variant), (self.config)()) {
            Ok(scenario) => scenario.with_defect(self.defect),
            Err(error) => panic!(
                "the configuration of {:?} does not validate: {error}",
                self.invariant
            ),
        }
    }

    /// What the first variant did, for the message of a plant that never went red.
    pub fn diagnose(&self) -> String {
        let outcome = self.scenario(0).replay(&(self.steps)(0));
        let verdict = match &outcome.verdict {
            Verdict::Green => String::from("green"),
            Verdict::Red(f) => format!(
                "red with {} at step {}: {}",
                f.invariant.name(),
                f.at_step,
                f.message
            ),
        };
        format!(
            "variant 0 was {verdict}\ntrace:\n{}\nstate:\n{}",
            outcome.trace_tail.join("\n"),
            outcome.state.join("\n")
        )
    }

    /// Runs the variants until one goes red; the variant, the scenario and the outcome.
    pub fn run(&self) -> Option<(u32, Scenario, Outcome)> {
        for variant in 0..self.variants {
            let scenario = self.scenario(variant);
            let outcome = scenario.replay(&(self.steps)(variant));
            if let Verdict::Red(f) = &outcome.verdict {
                if self.accept.contains(&f.invariant) {
                    return Some((variant, scenario, outcome));
                }
            }
        }
        None
    }
}

fn salted(v: u32) -> Steps {
    Steps::with_salt(v.wrapping_mul(7919))
}

fn base() -> Config {
    small_world()
}

fn leader(variant: LeaderVariant) -> Config {
    small_world().with_leader(variant)
}

use InvariantId as I;

pub fn planted() -> Vec<Planted> {
    vec![
        Planted {
            invariant: I::ChainEveryCopyVerifiesFromGenesis,
            defect: Defect::DeviceAdmitsUnchecked,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .quiesce()
                    .attack(AttackKind::WrongChainEntry, 0, 0)
                    .build()
            },
            accept: &[I::ChainEveryCopyVerifiesFromGenesis],
        },
        Planted {
            invariant: I::ChainNoForkIsEverAccepted,
            defect: Defect::DeviceAdmitsUnchecked,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .append(1, 0, 2)
                    .quiesce()
                    .attack(AttackKind::ForkEntry, 0, 0)
                    .build()
            },
            accept: &[I::ChainNoForkIsEverAccepted],
        },
        Planted {
            invariant: I::ChainCheckpointEqualsGenesis,
            defect: Defect::VerifierAcceptsAll,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .quiesce()
                    .tick(31 * 86_400)
                    .head_tick()
                    .quiesce()
                    .build()
            },
            accept: &[I::ChainCheckpointEqualsGenesis],
        },
        Planted {
            invariant: I::ChainACheckpointIsWrittenWhenDue,
            defect: Defect::None,
            config: || leader(LeaderVariant::NoCheckpoints),
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .quiesce()
                    .tick(31 * 86_400)
                    .head_tick()
                    .quiesce()
                    .build()
            },
            accept: &[I::ChainACheckpointIsWrittenWhenDue],
        },
        Planted {
            invariant: I::RestoreOnlyThroughTheSignedEpochPath,
            defect: Defect::GroupAloneTrust,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .append(1, 0, 2)
                    .quiesce()
                    .attack(AttackKind::GroupAloneRestore, 0, 0)
                    .build()
            },
            accept: &[I::RestoreOnlyThroughTheSignedEpochPath],
        },
        Planted {
            invariant: I::RestoreTheLostTimelineStaysLost,
            defect: Defect::NoChainFloor,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .quiesce()
                    .offsite()
                    .append(0, 0, 2)
                    .quiesce()
                    .disaster(0, SigChoice::Map, true)
                    .quiesce()
                    .build()
            },
            accept: &[I::RestoreTheLostTimelineStaysLost],
        },
        Planted {
            invariant: I::RestoreALateRecordKeepsProgress,
            defect: Defect::LateRecordRebuildsAll,
            config: base,
            variants: 1,
            steps: |v| {
                azlin_sim::directed_named("restore_late_record_keeps_progress").map_or_else(
                    Vec::new,
                    |d| {
                        let _ = v;
                        (d.steps)()
                    },
                )
            },
            accept: &[I::RestoreALateRecordKeepsProgress],
        },
        Planted {
            invariant: I::RestoreEveryListedChainIsReanchored,
            defect: Defect::ReanchorSkipsChain,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .append(1, 0, 2)
                    .quiesce()
                    .offsite()
                    .append(0, 0, 3)
                    .append(1, 1, 4)
                    .quiesce()
                    .disaster(0, SigChoice::Map, true)
                    .quiesce()
                    .build()
            },
            accept: &[I::RestoreEveryListedChainIsReanchored],
        },
        Planted {
            invariant: I::PinOnlyHoldsHeadsTheGroupSigned,
            defect: Defect::VerifierAcceptsAll,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .quiesce()
                    .attack(AttackKind::CorruptHead, 0, 0)
                    .build()
            },
            accept: &[I::PinOnlyHoldsHeadsTheGroupSigned],
        },
        Planted {
            invariant: I::PinNeverGoesBackwards,
            defect: Defect::PinForgets,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .append(1, 0, 2)
                    .quiesce()
                    .attack(AttackKind::ReplayOldHead, 0, 0)
                    .build()
            },
            accept: &[I::PinNeverGoesBackwards],
        },
        Planted {
            invariant: I::RestoreHeldEntriesAreSentAgain,
            defect: Defect::ResendDropped,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .quiesce()
                    .offsite()
                    .append(0, 0, 2)
                    .quiesce()
                    .disaster(0, SigChoice::Map, true)
                    .quiesce()
                    .build()
            },
            accept: &[I::RestoreHeldEntriesAreSentAgain],
        },
        Planted {
            invariant: I::RestoreHealsEveryDevice,
            defect: Defect::DeviceIgnoresRestore,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .quiesce()
                    .offsite()
                    .append(0, 0, 2)
                    .quiesce()
                    .disaster(0, SigChoice::Map, true)
                    .quiesce()
                    .build()
            },
            accept: &[I::RestoreHealsEveryDevice],
        },
        Planted {
            invariant: I::PeerAHiddenHeadShows,
            defect: Defect::MeetDropsAlarms,
            config: base,
            variants: 1,
            steps: |v| {
                azlin_sim::directed_named("peers_and_withheld_heads").map_or_else(Vec::new, |d| {
                    let _ = v;
                    (d.steps)()
                })
            },
            accept: &[I::PeerAHiddenHeadShows],
        },
        Planted {
            invariant: I::PeerAPeersJunkIsNoAlarm,
            defect: Defect::AlarmOnJunk,
            config: base,
            variants: 1,
            steps: |v| {
                azlin_sim::directed_named("peers_and_withheld_heads").map_or_else(Vec::new, |d| {
                    let _ = v;
                    (d.steps)()
                })
            },
            accept: &[I::PeerAPeersJunkIsNoAlarm],
        },
        Planted {
            invariant: I::StampNeverRunsBackwards,
            defect: Defect::None,
            config: || leader(LeaderVariant::BuggyStamp),
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .quiesce()
                    .jump(0, -3_600)
                    .append(1, 0, 2)
                    .quiesce()
                    .build()
            },
            accept: &[I::StampNeverRunsBackwards],
        },
        Planted {
            invariant: I::LeaderAcknowledgedEntriesSurviveAnHonestDisk,
            defect: Defect::None,
            config: || leader(LeaderVariant::AckBeforeFsync),
            variants: 40,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .poll(1, 1)
                    .tick(3)
                    .deliver(1)
                    .poll(0, 2)
                    .crash(0)
                    .restart(0, false)
                    .build()
            },
            accept: &[I::LeaderAcknowledgedEntriesSurviveAnHonestDisk],
        },
        Planted {
            invariant: I::LeaderNoSilentRollbackWindow,
            defect: Defect::None,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .quiesce()
                    .offsite()
                    .append(0, 0, 2)
                    .quiesce()
                    .disaster(0, SigChoice::Map, false)
                    .poll(0, 4)
                    .head_tick()
                    .poll(0, 3)
                    .build()
            },
            accept: &[I::LeaderNoSilentRollbackWindow],
        },
        Planted {
            invariant: I::LeaderOneHeadPerPositionAndEpoch,
            defect: Defect::None,
            config: || leader(LeaderVariant::DoubleSign),
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .quiesce()
                    .head_tick()
                    .poll(0, 3)
                    .build()
            },
            accept: &[I::LeaderOneHeadPerPositionAndEpoch],
        },
        Planted {
            invariant: I::DeviceRestartRecoversAStateItHad,
            defect: Defect::RecoveryReorders,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .append(1, 0, 2)
                    .quiesce()
                    .crash(1)
                    .restart(1, false)
                    .build()
            },
            accept: &[I::DeviceRestartRecoversAStateItHad],
        },
        Planted {
            invariant: I::RefusalEveryRefusalIsTypedAndClassed,
            defect: Defect::MisclassifiesFork,
            config: base,
            variants: 1,
            steps: |v| {
                salted(v)
                    .append(0, 0, 1)
                    .append(1, 0, 2)
                    .quiesce()
                    .attack(AttackKind::ForkEntry, 0, 0)
                    .build()
            },
            accept: &[I::RefusalEveryRefusalIsTypedAndClassed],
        },
        Planted {
            invariant: I::LimitsNothingGrowsWithoutALimit,
            defect: Defect::UnboundedNet,
            config: base,
            variants: 1,
            steps: |v| {
                let mut steps = salted(v);
                for _ in 0..160 {
                    steps = steps.head_tick().poll(0, 3);
                }
                steps.build()
            },
            accept: &[I::LimitsNothingGrowsWithoutALimit],
        },
        Planted {
            invariant: I::PolicyAGroupOnlyTightens,
            defect: Defect::PolicySkipsCheck,
            config: base,
            variants: 20,
            steps: |v| policy_steps(v),
            accept: &[I::PolicyAGroupOnlyTightens],
        },
        Planted {
            invariant: I::PolicyEpochBumpsOnlyOnChange,
            defect: Defect::PolicyAlwaysBumps,
            config: base,
            variants: 20,
            steps: |v| policy_steps(v),
            accept: &[I::PolicyEpochBumpsOnlyOnChange],
        },
        Planted {
            invariant: I::PolicyQuorumCountsDistinctPeople,
            defect: Defect::QuorumCountsDevices,
            config: base,
            variants: 20,
            steps: |v| policy_steps(v),
            accept: &[I::PolicyQuorumCountsDistinctPeople],
        },
    ]
}

fn policy_steps(variant: u32) -> Vec<Step> {
    let mut steps = salted(variant);
    for i in 0..60u32 {
        steps = steps.policy(
            (i % 3 == 0) as u8,
            (i % 8) as u8,
            i.wrapping_mul(0x9E37_79B1) ^ variant,
        );
    }
    steps.build()
}
