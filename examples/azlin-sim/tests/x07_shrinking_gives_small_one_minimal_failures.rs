//! X07: shrinking gives small, 1-minimal failures (planning/architecture.md section 1.4 rule 2: a shrunk case
//! is what a person reads). For 12 planted failures (across the chain, pin, restore, peer, leader and
//! device groups) the shrunk list has at most 12 steps, still fails with the same invariant, and is
//! 1-minimal: the test removes each step in turn and the run no longer fails the same way.

mod common;

use azlin_sim::{shrink, Failure, FailureKind, Scenario, Step, Verdict};

fn fails_like(scenario: &Scenario, steps: &[Step], failure: &Failure) -> bool {
    match &scenario.replay(steps).verdict {
        Verdict::Red(f) => f.invariant == failure.invariant && f.kind == failure.kind,
        Verdict::Green => false,
    }
}

#[test]
fn x07_shrinking_gives_small_one_minimal_failures() {
    let mut shrunk_cases = 0;
    for plant in common::planted()
        .iter()
        .filter(|p| p.variants == 1)
        .take(12)
    {
        let Some((_, scenario, outcome)) = plant.run() else {
            panic!(
                "the plant of {} never went red\n{}",
                plant.invariant.name(),
                plant.diagnose()
            );
        };
        let Verdict::Red(failure) = &outcome.verdict else {
            panic!("a plant is red");
        };
        assert_eq!(failure.kind, FailureKind::Product);
        let steps = shrink(&scenario, outcome.steps.clone(), failure);
        assert!(
            steps.len() <= 12,
            "{}: {} steps after shrinking",
            plant.invariant.name(),
            steps.len()
        );
        assert!(
            fails_like(&scenario, &steps, failure),
            "{}: the shrunk list no longer fails the same way",
            plant.invariant.name()
        );
        for index in 0..steps.len() {
            let mut without = steps.clone();
            without.remove(index);
            assert!(
                !fails_like(&scenario, &without, failure),
                "{}: step {index} ({}) can be removed: the list is not 1-minimal",
                plant.invariant.name(),
                steps.get(index).map(Step::to_line).unwrap_or_default()
            );
        }
        shrunk_cases += 1;
    }
    assert_eq!(shrunk_cases, 12);
}
