//! Shrinking (feature (c) of the brief; planning/architecture.md section 1.4 rule 2: a failing seed
//! replays exactly, and a shrunk case is what a person reads).
//!
//! [`shrink`] runs `azlin_seams::sim::minimize` with the predicate "the replay fails with the SAME
//! invariant and the same [`FailureKind`]" (a different failure is not the one being shrunk), then
//! tries to lower the configuration (fewer members, devices, spaces, accounts) while it still
//! fails. The result is 1-minimal: removing any one step passes.

use azlin_seams::sim::minimize;

use crate::config::Config;
use crate::scenario::{Failure, FailureKind, Scenario, Verdict};
use crate::step::Step;

/// Whether the replay of `steps` fails the way `failure` did.
fn fails_like(sc: &Scenario, steps: &[Step], failure: &Failure) -> bool {
    let outcome = sc.replay(steps);
    match &outcome.verdict {
        Verdict::Green => false,
        Verdict::Red(found) => {
            if found.kind == FailureKind::Harness {
                return false;
            }
            if found.invariant == failure.invariant && found.kind == failure.kind {
                return true;
            }
            // A known gap that tripped on the way and was kept running past counts too.
            outcome
                .known_gaps
                .iter()
                .any(|gap| gap.invariant == failure.invariant && gap.kind == failure.kind)
        }
    }
}

/// The smallest list of steps (and the smallest world) that still fails the way `failure` did.
///
/// Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays exactly).
///
/// Caller: `run_seeds`, tests/x07, tests/x06.
pub fn shrink_full(sc: &Scenario, steps: Vec<Step>, failure: &Failure) -> (Config, Vec<Step>) {
    let mut steps = minimize(steps, |candidate| fails_like(sc, candidate, failure));
    let mut config = sc.config().clone();
    // Lower the size of the world one notch at a time while the failure stays.
    for round in 0..4 {
        let mut lowered = config.clone();
        match round {
            0 => lowered.members = lowered.members.saturating_sub(1).max(2),
            1 => lowered.devices = lowered.devices.saturating_sub(1).max(1),
            2 => lowered.spaces = 0,
            _ => lowered.accounts = 1,
        }
        if lowered == config {
            continue;
        }
        if let Ok(smaller) = sc.with_config(lowered.clone()) {
            if fails_like(&smaller, &steps, failure) {
                config = lowered;
                let sc = smaller;
                steps = minimize(steps, |candidate| fails_like(&sc, candidate, failure));
            }
        }
    }
    (config, steps)
}

/// The smallest list of steps that still fails the way `failure` did (the configuration of the
/// scenario is kept; see [`shrink_full`] for the lowered one).
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `run_seeds`, tests/x07.
pub fn shrink(sc: &Scenario, steps: Vec<Step>, failure: &Failure) -> Vec<Step> {
    minimize(steps, |candidate| fails_like(sc, candidate, failure))
}

/// The printed form of a shrunk failure: the seed, the stage, the configuration and the steps,
/// pasteable as a seed file body.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: `Report::render`, tests/x06.
pub fn printed(sc: &Scenario, steps: &[Step], failure: &Failure) -> String {
    let mut out = format!(
        "seed: 0x{:016x}\nstage: {}\nconfig: {}\ninvariant: {}\nsteps: {}\n",
        sc.seed(),
        sc.config().stage.number(),
        sc.config(),
        failure.invariant.name(),
        steps.len()
    );
    for step in steps {
        out.push_str(&step.to_line());
        out.push('\n');
    }
    out
}
