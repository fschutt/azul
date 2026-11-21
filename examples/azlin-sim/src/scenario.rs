//! The scenario runner (feature (a) of the brief; planning/architecture.md section 1.4 rules 1 and 2,
//! section 12): a seed and a budget draw steps, the world executes each, the invariants are checked
//! after every step, and the run ends with an automatic `quiesce` that must converge.
//!
//! `Scenario::run` draws one step at a time by READING the world (to pick sensible targets and weights
//! from the profile); it never changes it. `Outcome::steps` is the list of steps drawn and
//! `Scenario::replay` executes exactly that list, so `run()` and `replay(run().steps)` are the same
//! run (X01). `AZLIN_SIM_SEED=0x...` plus the stage is the whole reproduction recipe.

use azlin_seams::sim::Rng;
use azlin_seams::Hash32;

use crate::config::{Config, ConfigError, Defect, Profile, Stage};
use crate::coverage::{Coverage, Footprint};
use crate::draw::{below, between, mix, weighted};
use crate::invariants::{Checker, InvariantId, Status};
use crate::step::{AttackKind, DiskFault, SigChoice, Step, StepKind};
use crate::time::Shift;
use crate::world::World;

/// What kind of failure a run ended in.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a failing seed is a regression test).
///
/// Caller: `Failure`, `run_seeds`, tests/fast.rs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// An invariant of the plan is broken: a defect of a crate or of the glue.
    Product,
    /// An invariant whose status is a known gap (the named drift row) is broken.
    KnownGap(&'static str),
    /// The harness itself is wrong; never shrunk, never mistaken for a product failure.
    Harness,
}

/// A failed run: the invariant, the step, why, and the last lines of the trace.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `Verdict`, `shrink`, `Report`.
#[derive(Clone, Debug)]
pub struct Failure {
    /// The invariant that broke (a harness failure holds the limits invariant by convention).
    pub invariant: InvariantId,
    /// What kind of failure.
    pub kind: FailureKind,
    /// The index of the step after which it was found.
    pub at_step: u32,
    /// What did not hold.
    pub message: String,
    /// The last lines of the trace.
    pub trace_tail: Vec<String>,
    /// The state of the world when it was found (empty for a known gap, which keeps the run going).
    pub state: Vec<String>,
}

/// The verdict of a run.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `Outcome`.
#[derive(Clone, Debug)]
pub enum Verdict {
    /// Every invariant held.
    Green,
    /// A product failure, a known gap or a harness failure.
    Red(Failure),
}

/// The counters of a run.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `Outcome`, `Report`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tallies {
    /// Steps executed.
    pub steps: u64,
    /// Steps that were skips.
    pub skipped: u64,
    /// Entries the leader acknowledged.
    pub acked: u64,
    /// Restores done.
    pub restores: u64,
    /// Alarms the devices raised.
    pub alarms: u64,
    /// Messages the network delivered.
    pub delivered: u64,
    /// Checkpoints the leader holds at the end.
    pub checkpoints: u64,
}

/// What a run produced.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `run_seeds`, `shrink`, every test.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// Green, or the first failure (a product failure before a known gap).
    pub verdict: Verdict,
    /// The steps drawn, the automatic `quiesce` last.
    pub steps: Vec<Step>,
    /// A hash of the whole trace.
    pub trace_hash: Hash32,
    /// A hash of the final state of the world.
    pub world_hash: Hash32,
    /// A hash of the decisions only: no hash of a signed value is in it.
    pub decision_hash: Hash32,
    /// What the run exercised.
    pub coverage: Coverage,
    /// The counters.
    pub tallies: Tallies,
    /// The peak of the footprint.
    pub peak: Footprint,
    /// Every known gap that tripped during the run (the first of each invariant).
    pub known_gaps: Vec<Failure>,
    /// The last lines of the trace, green or red.
    pub trace_tail: Vec<String>,
    /// The state of the world at the end, green or red.
    pub state: Vec<String>,
}

/// A scenario: a seed and a configuration.
///
/// Implements planning/architecture.md section 1.4 rules 1 and 2 and section 12.
///
/// Caller: `run_seeds`, `shrink`, `directed`, every test.
#[derive(Clone, Debug)]
pub struct Scenario {
    seed: u64,
    config: Config,
    defect: Defect,
}

impl Scenario {
    /// A scenario of this seed and configuration; the configuration is validated.
    pub fn new(seed: u64, config: Config) -> Result<Scenario, ConfigError> {
        config.validate()?;
        Ok(Scenario {
            seed,
            config,
            defect: Defect::None,
        })
    }

    /// The scenario of a seed at a stage: the configuration is a pure function of the seed.
    pub fn standard(seed: u64, stage: Stage) -> Scenario {
        Scenario {
            seed,
            config: Config::standard(seed, stage),
            defect: Defect::None,
        }
    }

    /// The same scenario with a faulty double in the glue (a planted violation; never part of a
    /// seed file).
    #[must_use]
    pub fn with_defect(mut self, defect: Defect) -> Scenario {
        self.defect = defect;
        self
    }

    /// The same seed and faulty double under another configuration (the shrinker lowers the size
    /// of the world with it).
    pub fn with_config(&self, config: Config) -> Result<Scenario, ConfigError> {
        config.validate()?;
        Ok(Scenario {
            seed: self.seed,
            config,
            defect: self.defect,
        })
    }

    /// The faulty double of this scenario.
    pub fn defect(&self) -> Defect {
        self.defect
    }

    /// The seed.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Draws and executes `config.steps` steps, checking the invariants after each, and ends with
    /// an automatic `quiesce` that is the last element of `Outcome::steps`.
    pub fn run(&self) -> Outcome {
        let mut world = World::new(self.seed, self.config.clone(), self.defect);
        let mut run = Run::new();
        let mut gen = Rng::new(mix(self.seed, 0x6E4E_5A11));
        for index in 0..self.config.steps {
            let step = self.draw_step(&world, &mut gen);
            if run.execute(&mut world, &step, index) {
                break;
            }
        }
        if run.failure.is_none() {
            let last = Step::Quiesce {
                salt: gen.next_u64() as u32,
            };
            let at = self.config.steps;
            run.execute(&mut world, &last, at);
        }
        run.finish(world)
    }

    /// Executes exactly this list of steps.
    pub fn replay(&self, steps: &[Step]) -> Outcome {
        let mut world = World::new(self.seed, self.config.clone(), self.defect);
        let mut run = Run::new();
        for (index, step) in steps.iter().enumerate() {
            if run.execute(&mut world, step, index as u32) {
                break;
            }
        }
        run.finish(world)
    }

    /// Draws one step from the profile, reading the world to pick targets.
    fn draw_step(&self, world: &World, rng: &mut Rng) -> Step {
        let rates = &self.config.faults;
        let profile = self.config.profile;
        let salt = rng.next_u64() as u32;
        let nodes = world.node_count() as u64;
        let devices = world.devices.len().max(1) as u64;
        let up_devices: Vec<usize> = world
            .devices
            .iter()
            .filter(|d| d.up)
            .map(|d| d.idx)
            .collect();
        let pick_dev = |rng: &mut Rng| -> u8 {
            match rng.pick(&up_devices) {
                Some(index) => *index as u8,
                None => below(rng, devices) as u8,
            }
        };
        // Faults come first, each with the probability its rate says.
        if rates.partition.hit(rng) {
            let mask = (rng.next_u64() & ((1u64 << nodes.min(16)) - 1)) as u16;
            return Step::Partition { mask, salt };
        }
        if rates.slow.hit(rng) {
            return Step::Slow {
                node: below(rng, nodes) as u8,
                factor: 2 + below(rng, 6) as u8,
                salt,
            };
        }
        if rates.clock.hit(rng) {
            let node = below(rng, nodes + 2) as u8;
            let secs = match below(rng, 10) {
                0 => -2_000_000_000,
                1 | 2 => {
                    between(rng, 3_600, 200_000) as i32 * if below(rng, 2) == 0 { 1 } else { -1 }
                }
                _ => between(rng, 1, 300) as i32 * if below(rng, 2) == 0 { 1 } else { -1 },
            };
            let shift = Shift::from_secs(secs);
            return if below(rng, 2) == 0 {
                Step::Skew { node, shift, salt }
            } else {
                Step::Jump { node, shift, salt }
            };
        }
        for _ in 0..3 {
            if rates.crash.hit(rng) {
                let busy = world.sched.busy_nodes();
                let node = match (below(rng, 10) < 7, rng.pick(&busy)) {
                    (true, Some(node)) => *node as u8,
                    _ => below(rng, nodes) as u8,
                };
                return Step::Crash { node, salt };
            }
        }
        let down: Vec<usize> = (0..world.node_count())
            .filter(|n| !world.is_up(*n))
            .collect();
        if !down.is_empty() && below(rng, 100) < 14 {
            if let Some(node) = rng.pick(&down) {
                return Step::Restart {
                    node: *node as u8,
                    wipe: below(rng, 20) == 0,
                    salt,
                };
            }
        }
        if rates.read_error.hit(rng) {
            return Step::Disk {
                node: below(rng, nodes) as u8,
                fault: DiskFault::ReadErr,
                salt,
            };
        }
        if rates.bit_flip.hit(rng) {
            return Step::Disk {
                node: below(rng, nodes) as u8,
                fault: DiskFault::Flip,
                salt,
            };
        }
        if rates.vanish.hit(rng) {
            // The leader's disk vanishes far more rarely than a device's.
            let node = if below(rng, 6) == 0 {
                0
            } else {
                1 + below(rng, devices) as u8
            };
            return Step::Disk {
                node,
                fault: DiskFault::Vanish,
                salt,
            };
        }
        if rates.lost_fsync.get() > 0 && below(rng, 100) < 3 {
            return Step::Disk {
                node: below(rng, nodes) as u8,
                fault: DiskFault::Lie,
                salt,
            };
        }
        if rates.torn.get() > 0 && below(rng, 100) < 2 {
            return Step::Disk {
                node: below(rng, nodes) as u8,
                fault: DiskFault::Tear,
                salt,
            };
        }
        if world.net.cut_mask() != 0 && below(rng, 100) < 12 {
            return Step::Heal { mask: 0xffff, salt };
        }
        let restore_weight = match profile {
            Profile::Calm => 0,
            Profile::Chaos => 2,
            Profile::RestoreHeavy => 8,
            Profile::Adversarial => 2,
        };
        let attack_weight = match profile {
            Profile::Calm => 1,
            Profile::Chaos => 3,
            Profile::RestoreHeavy => 3,
            Profile::Adversarial => 30,
        };
        let busy = world.sched.busy_nodes();
        let table: [(u32, StepKind); 14] = [
            (22, StepKind::Append),
            (20, StepKind::Deliver),
            (16, StepKind::Poll),
            (8, StepKind::Tick),
            (6, StepKind::HeadTick),
            (3, StepKind::Fetch),
            (6, StepKind::Meet),
            (2, StepKind::Checkpoint),
            (3, StepKind::Offsite),
            (restore_weight, StepKind::Disaster),
            (3, StepKind::Resend),
            (attack_weight, StepKind::Attack),
            (5, StepKind::Policy),
            (1, StepKind::Quiesce),
        ];
        let kind = match weighted(rng, &table) {
            Some(kind) => *kind,
            None => StepKind::Tick,
        };
        match kind {
            StepKind::Append => Step::Append {
                dev: pick_dev(rng),
                chain: below(rng, 256) as u8,
                payload: below(rng, 65_536) as u16,
                salt,
            },
            StepKind::Deliver => Step::Deliver {
                n: 1 + below(rng, 4) as u8,
                salt,
            },
            StepKind::Poll => {
                let node = match (below(rng, 10) < 7, rng.pick(&busy)) {
                    (true, Some(node)) => *node as u8,
                    _ => below(rng, nodes) as u8,
                };
                Step::Poll {
                    node,
                    n: 1 + below(rng, 3) as u8,
                    salt,
                }
            }
            StepKind::Tick => Step::Tick {
                secs: match profile {
                    Profile::Calm | Profile::Adversarial => 1 + below(rng, 4) as u32,
                    Profile::Chaos | Profile::RestoreHeavy => 1 + below(rng, 45) as u32,
                },
                salt,
            },
            StepKind::HeadTick => Step::HeadTick { salt },
            StepKind::Fetch => Step::Fetch {
                dev: pick_dev(rng),
                chain: below(rng, 256) as u8,
                salt,
            },
            StepKind::Meet => Step::Meet {
                a: pick_dev(rng),
                b: pick_dev(rng),
                salt,
            },
            StepKind::Checkpoint => Step::Checkpoint {
                chain: below(rng, 256) as u8,
                salt,
            },
            StepKind::Offsite => Step::Offsite { salt },
            StepKind::Disaster => Step::Disaster {
                copy: below(rng, 256) as u8,
                // The operator holds the map key and signs the restore it does; a record of any other
                // signature is what the adversary sends (`Step::Attack`), never what a restore is.
                sig: SigChoice::Map,
                // A restore that is never announced is the planted violation of
                // `inv_leader_no_silent_rollback_window` (`LeaderVariant::SilentRollback`), never a
                // draw: the plan asks for the announcement (6.10 V3).
                announce: true,
                salt,
            },
            StepKind::Resend => Step::Resend {
                dev: pick_dev(rng),
                salt,
            },
            StepKind::Attack => Step::Attack {
                kind: match AttackKind::ALL.get(below(rng, AttackKind::ALL.len() as u64) as usize) {
                    Some(kind) => *kind,
                    None => AttackKind::PeerJunk,
                },
                dev: pick_dev(rng),
                chain: below(rng, 256) as u8,
                salt,
            },
            StepKind::Policy => Step::Policy {
                scope: below(rng, 2) as u8,
                group: below(rng, 8) as u8,
                patch: rng.next_u64() as u32,
                salt,
            },
            StepKind::Quiesce => Step::Quiesce { salt },
            StepKind::Partition
            | StepKind::Heal
            | StepKind::Slow
            | StepKind::Skew
            | StepKind::Jump
            | StepKind::Crash
            | StepKind::Restart
            | StepKind::Disk => Step::Tick { secs: 1, salt },
        }
    }
}

/// The state of one run: the checkers and the failures found so far.
struct Run {
    checker: Checker,
    steps: Vec<Step>,
    failure: Option<Failure>,
    gaps: Vec<Failure>,
}

impl Run {
    fn new() -> Run {
        Run {
            checker: Checker::new(),
            steps: Vec::new(),
            failure: None,
            gaps: Vec::new(),
        }
    }

    /// Executes a step and checks the invariants; whether the run must stop (a product or harness
    /// failure was found).
    fn execute(&mut self, world: &mut World, step: &Step, index: u32) -> bool {
        self.steps.push(*step);
        world.step(step);
        if let Some(message) = world.harness_error.take() {
            self.failure = Some(Failure {
                invariant: InvariantId::LimitsNothingGrowsWithoutALimit,
                kind: FailureKind::Harness,
                at_step: index,
                message,
                trace_tail: world.trace.tail.iter().cloned().collect(),
                state: world.dump(),
            });
            return true;
        }
        let deep = index % 25 == 24;
        let violations = if matches!(step, Step::Quiesce { .. }) {
            self.checker.after_quiesce(world)
        } else {
            self.checker.after_step(world, deep)
        };
        for violation in violations {
            match violation.id.info().status {
                Status::Enforced => {
                    self.failure = Some(Failure {
                        invariant: violation.id,
                        kind: FailureKind::Product,
                        at_step: index,
                        message: violation.message,
                        trace_tail: world.trace.tail.iter().cloned().collect(),
                        state: world.dump(),
                    });
                    return true;
                }
                Status::KnownGap(row) => {
                    self.checker.trip(violation.id);
                    self.gaps.push(Failure {
                        invariant: violation.id,
                        kind: FailureKind::KnownGap(row),
                        at_step: index,
                        message: violation.message,
                        trace_tail: world.trace.tail.iter().cloned().collect(),
                        state: Vec::new(),
                    });
                }
            }
        }
        false
    }

    fn finish(mut self, mut world: World) -> Outcome {
        world.finish_coverage();
        for (id, count) in &self.checker.counts {
            *world.cov.checks.entry(*id).or_insert(0) += count;
        }
        let verdict = match (self.failure.take(), self.gaps.first()) {
            (Some(failure), _) => Verdict::Red(failure),
            (None, Some(gap)) => Verdict::Red(gap.clone()),
            (None, None) => Verdict::Green,
        };
        let net = world.net.stats();
        let tallies = Tallies {
            steps: world.cov.steps,
            skipped: world.cov.skipped,
            acked: world.ledger.acked.len() as u64,
            restores: world.ledger.restores as u64,
            alarms: world.devices.iter().map(|d| d.alarm_count).sum(),
            delivered: net.delivered,
            checkpoints: world
                .leader
                .chains
                .values()
                .map(|lc| lc.checkpoints.len() as u64)
                .sum(),
        };
        Outcome {
            verdict,
            steps: self.steps,
            trace_hash: world.trace.rolling,
            world_hash: world.world_hash(),
            decision_hash: world.trace.decisions,
            peak: world.cov.peak,
            trace_tail: world.trace.tail.iter().cloned().collect(),
            state: world.dump(),
            coverage: world.cov,
            tallies,
            known_gaps: self.gaps,
        }
    }
}
