//! X01: the same seed gives the same run (planning/architecture.md section 1.4 rule 1: a failing
//! seed replays exactly).

use azlin_sim::{Scenario, Stage, Verdict};

#[test]
fn x01_the_same_seed_gives_the_same_run() {
    for seed in 0..20u64 {
        let first = Scenario::standard(seed, Stage::S1).run();
        let second = Scenario::standard(seed, Stage::S1).run();
        assert_eq!(first.steps, second.steps, "seed {seed}: the steps differ");
        assert_eq!(
            first.trace_hash, second.trace_hash,
            "seed {seed}: the trace differs"
        );
        assert_eq!(
            first.world_hash, second.world_hash,
            "seed {seed}: the world differs"
        );
        assert_eq!(first.decision_hash, second.decision_hash, "seed {seed}");
        // run() and replay(run().steps) are the same run.
        let replayed = Scenario::standard(seed, Stage::S1).replay(&first.steps);
        assert_eq!(
            replayed.trace_hash, first.trace_hash,
            "seed {seed}: replay differs"
        );
        assert_eq!(
            replayed.world_hash, first.world_hash,
            "seed {seed}: replay differs"
        );
        assert_eq!(
            matches!(replayed.verdict, Verdict::Green),
            matches!(first.verdict, Verdict::Green),
            "seed {seed}: replay verdict differs"
        );
        // A run that ends in a failure stops where it was found; every other ends in the quiesce.
        if matches!(first.verdict, Verdict::Green) {
            assert!(
                matches!(first.steps.last(), Some(azlin_sim::Step::Quiesce { .. })),
                "seed {seed}: the automatic quiesce is the last step"
            );
        }
    }
}
