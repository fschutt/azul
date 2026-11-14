//! Many seeds, and the report of them (feature (d) of the brief; planning/architecture.md section
//! 12: the deterministic simulation proves the logic at full size).
//!
//! [`run_seeds`] runs a range of seeds at a stage. A red run is shrunk (`shrink::shrink_full`) and a
//! seed file `<invariant>-<seed:016x>.seed` is CREATED in the store (never overwritten; at most one
//! file per invariant per run; none when an `expect: fails` file of the same invariant already
//! exists). A known gap is reported and kept the same way, and does not make the report red. The
//! report is one fact per line in a fixed format, so two nights can be diffed.

use std::collections::BTreeSet;

use crate::config::{Config, Stage, DEFAULT_REAL_PERMILLE, FAST_SEEDS, FAST_STEPS};
use crate::coverage::Coverage;
use crate::invariants::InvariantId;
use crate::scenario::{Failure, FailureKind, Scenario, Tallies, Verdict};
use crate::seeds::{Expect, SeedFile, SeedStore, StoreError};
use crate::shrink::shrink_full;

/// What `run_seeds` runs.
///
/// Implements planning/architecture.md section 12.
///
/// Caller: `run_seeds`, tests/fast.rs, tests/long.rs, src/bin/simrun.rs.
#[derive(Clone, Debug)]
pub struct RunOptions {
    /// The first seed.
    pub first_seed: u64,
    /// How many seeds.
    pub seeds: u64,
    /// The stage.
    pub stage: Stage,
    /// The steps of each seed.
    pub steps: u32,
    /// The share of seeds that use real Ed25519, in parts per thousand.
    pub real_permille: u16,
    /// Whether a red run is shrunk and kept as a seed file.
    pub shrink: bool,
    /// The most product failures to collect before the run stops.
    pub max_failures: u32,
}

impl RunOptions {
    /// The fast run: 200 seeds of 250 steps at the highest stage.
    pub fn fast() -> RunOptions {
        RunOptions {
            first_seed: 0,
            seeds: FAST_SEEDS,
            stage: Stage::HIGHEST,
            steps: FAST_STEPS,
            real_permille: DEFAULT_REAL_PERMILLE,
            shrink: true,
            max_failures: 3,
        }
    }
}

/// One failure of a run of seeds.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `Report`.
#[derive(Clone, Debug)]
pub struct FailedSeed {
    /// The seed.
    pub seed: u64,
    /// What failed.
    pub failure: Failure,
    /// The configuration after the shrinker lowered it.
    pub config: Config,
    /// The shrunk steps (empty for a harness failure: it is never shrunk).
    pub steps: Vec<crate::step::Step>,
    /// The name of the seed file created for it, when one was.
    pub file: Option<String>,
}

/// The report of a run of seeds.
///
/// Implements planning/architecture.md section 12.
///
/// Caller: tests/fast.rs, src/bin/simrun.rs.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Seeds run.
    pub seeds: u64,
    /// Steps executed.
    pub steps: u64,
    /// Product failures.
    pub failures: Vec<FailedSeed>,
    /// Failures of known gaps.
    pub known_gaps: Vec<FailedSeed>,
    /// Failures of the harness itself.
    pub harness: Vec<FailedSeed>,
    /// What the seeds exercised, added up.
    pub coverage: Coverage,
    /// The counters, added up.
    pub tallies: Tallies,
}

impl Report {
    /// Whether no seed failed in the product or in the harness (known gaps do not count).
    pub fn is_green(&self) -> bool {
        self.failures.is_empty() && self.harness.is_empty()
    }

    /// The coverage of all seeds.
    pub fn coverage(&self) -> &Coverage {
        &self.coverage
    }

    /// The report text: one fact per line in a fixed format.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("seeds {}\nsteps {}\n", self.seeds, self.steps));
        out.push_str(&format!(
            "tallies steps {} skipped {} acked {} restores {} alarms {} delivered {}\n",
            self.tallies.steps,
            self.tallies.skipped,
            self.tallies.acked,
            self.tallies.restores,
            self.tallies.alarms,
            self.tallies.delivered
        ));
        out.push_str(&format!(
            "failures {} known_gaps {} harness {}\n",
            self.failures.len(),
            self.known_gaps.len(),
            self.harness.len()
        ));
        for (label, list) in [
            ("failure", &self.failures),
            ("known_gap", &self.known_gaps),
            ("harness", &self.harness),
        ] {
            for failed in list {
                out.push_str(&format!(
                    "{label} seed 0x{:016x} invariant {} step {} file {}\n  {}\n",
                    failed.seed,
                    failed.failure.invariant.name(),
                    failed.failure.at_step,
                    failed.file.as_deref().unwrap_or("-"),
                    failed.failure.message
                ));
                if label != "known_gap" {
                    // A failure shows how it ended: the shrunk steps, the end of the trace, the state.
                    for step in &failed.steps {
                        out.push_str(&format!("    step {}\n", step.to_line()));
                    }
                    for line in &failed.failure.trace_tail {
                        out.push_str(&format!("    trace {line}\n"));
                    }
                    for line in &failed.failure.state {
                        out.push_str(&format!("    state {line}\n"));
                    }
                }
            }
        }
        out.push_str(&self.coverage.render());
        out
    }
}

/// The names of the `expect: fails` files of a store, by invariant.
fn open_defects(store: &dyn SeedStore) -> BTreeSet<InvariantId> {
    let mut open = BTreeSet::new();
    if let Ok(names) = store.names() {
        for name in names {
            if let Ok(text) = store.read(&name) {
                if let Ok(file) = SeedFile::parse(&text) {
                    if file.expect == Expect::Fails {
                        open.insert(file.invariant);
                    }
                }
            }
        }
    }
    open
}

/// Runs a range of seeds, shrinks and keeps what fails.
///
/// Implements planning/architecture.md section 12 and section 1.4 rule 2.
///
/// Caller: tests/fast.rs, tests/long.rs, tests/x06, src/bin/simrun.rs.
pub fn run_seeds(opts: &RunOptions, store: &mut dyn SeedStore) -> Report {
    let mut report = Report::default();
    let mut written: BTreeSet<InvariantId> = BTreeSet::new();
    let mut open = open_defects(store);
    for offset in 0..opts.seeds {
        let seed = opts.first_seed.wrapping_add(offset);
        let config =
            Config::standard_with(seed, opts.stage, opts.real_permille).with_steps(opts.steps);
        let scenario = match Scenario::new(seed, config) {
            Ok(scenario) => scenario,
            Err(_) => continue,
        };
        let outcome = scenario.run();
        report.seeds += 1;
        report.steps += outcome.tallies.steps;
        report.coverage.merge(&outcome.coverage);
        report.tallies.steps += outcome.tallies.steps;
        report.tallies.skipped += outcome.tallies.skipped;
        report.tallies.acked += outcome.tallies.acked;
        report.tallies.restores += outcome.tallies.restores;
        report.tallies.alarms += outcome.tallies.alarms;
        report.tallies.delivered += outcome.tallies.delivered;
        report.tallies.checkpoints += outcome.tallies.checkpoints;
        // A product failure first; otherwise the known gaps that tripped.
        let mut found: Vec<Failure> = Vec::new();
        if let Verdict::Red(failure) = &outcome.verdict {
            found.push(failure.clone());
        }
        for gap in &outcome.known_gaps {
            if !found.iter().any(|f| f.invariant == gap.invariant) {
                found.push(gap.clone());
            }
        }
        for failure in found {
            // A file is made for the first failure of an invariant that has none; a known gap is
            // shrunk only when it makes one (every later seed of the same gap is just counted).
            let makes_file = failure.kind != FailureKind::Harness
                && opts.shrink
                && !written.contains(&failure.invariant)
                && !open.contains(&failure.invariant);
            let shrinks = makes_file || (failure.kind == FailureKind::Product && opts.shrink);
            let (config, steps) = if shrinks {
                shrink_full(&scenario, outcome.steps.clone(), &failure)
            } else {
                (scenario.config().clone(), Vec::new())
            };
            // The failure of the shrunk list is the one that is shown: its trace and state are the
            // ones of the steps printed with it.
            let failure = match scenario.with_config(config.clone()) {
                Ok(small) if shrinks => match small.replay(&steps).verdict {
                    Verdict::Red(shrunk) if shrunk.invariant == failure.invariant => shrunk,
                    Verdict::Red(_) | Verdict::Green => failure,
                },
                Ok(_) | Err(_) => failure,
            };
            let mut file = None;
            if makes_file {
                let name = format!("{}-{:016x}", failure.invariant.name(), seed);
                let text = SeedFile {
                    name: name.clone(),
                    stage: opts.stage,
                    seed,
                    config: config.clone(),
                    invariant: failure.invariant,
                    expect: Expect::Fails,
                    note: format!(
                        "found by run_seeds at step {}: {}",
                        failure.at_step,
                        one_line(&failure.message)
                    ),
                    steps: steps.clone(),
                }
                .to_text();
                match store.create(&name, &text) {
                    Ok(()) => {
                        written.insert(failure.invariant);
                        open.insert(failure.invariant);
                        file = Some(name);
                    }
                    Err(StoreError::Exists) => {}
                    Err(_) => {}
                }
            }
            let failed = FailedSeed {
                seed,
                failure: failure.clone(),
                config,
                steps,
                file,
            };
            match failure.kind {
                FailureKind::Product => report.failures.push(failed),
                FailureKind::KnownGap(_) => report.known_gaps.push(failed),
                FailureKind::Harness => report.harness.push(failed),
            }
        }
        if report.failures.len() as u32 >= opts.max_failures {
            break;
        }
    }
    report
}

fn one_line(text: &str) -> String {
    text.replace('\n', " ")
}
