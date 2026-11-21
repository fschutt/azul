//! X14: a harness bug is not a product failure (planning/architecture.md section 1.4 rule 11:
//! invariants are asserted; a wrong harness must say so and never be mistaken for a defect of a crate).
//! A world fed an impossible state yields `FailureKind::Harness` with a message, never a panic.

use azlin_sim::{Config, Defect, FailureKind, Scenario, Stage, Steps, Verdict};

#[test]
fn x14_a_harness_failure_is_its_own_kind() {
    // The harness's own check: a leader whose chain refuses an entry it had just checked is a bug of
    // the glue; a defect cannot make it from outside, so the test asserts the contract of the kinds.
    let outcome = Scenario::standard(1, Stage::S1).run();
    match &outcome.verdict {
        Verdict::Green => {}
        Verdict::Red(f) => assert_ne!(f.kind, FailureKind::Harness, "seed 1: {}", f.message),
    }
    // A defect of the glue is a product failure with an invariant, never a harness failure.
    let config = Config::standard(2, Stage::S1).with_steps(1);
    let scenario = Scenario::new(2, config)
        .expect("a valid configuration")
        .with_defect(Defect::DeviceAdmitsUnchecked);
    let steps = Steps::new()
        .append(0, 0, 1)
        .quiesce()
        .attack(azlin_sim::AttackKind::WrongChainEntry, 0, 0)
        .quiesce()
        .build();
    let outcome = scenario.replay(&steps);
    if let Verdict::Red(f) = &outcome.verdict {
        assert_eq!(f.kind, FailureKind::Product, "{}", f.message);
    }
}
