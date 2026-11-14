//! X13: the fake and the real signature scheme decide alike (planning/architecture.md section 5.9: every
//! entry and head is signed). For 10 seeds the run with `Scheme::Fake` and with `Scheme::Ed25519` give
//! the same DECISION hash (every verdict, class and tally: no signature byte is in it) and the same
//! step list.

use azlin_sim::{Config, Scenario, Scheme, Stage};

#[test]
fn x13_fake_and_real_signatures_decide_alike() {
    for seed in 0..10u64 {
        let base = Config::standard(seed, Stage::S1).with_steps(120);
        let fake = Scenario::new(seed, base.clone().with_scheme(Scheme::Fake))
            .expect("a valid configuration")
            .run();
        let real = Scenario::new(seed, base.with_scheme(Scheme::Ed25519))
            .expect("a valid configuration")
            .run();
        assert_eq!(fake.steps, real.steps, "seed {seed}: the step lists differ");
        assert_eq!(
            fake.decision_hash, real.decision_hash,
            "seed {seed}: the decisions differ"
        );
        assert_eq!(
            matches!(fake.verdict, azlin_sim::Verdict::Green),
            matches!(real.verdict, azlin_sim::Verdict::Green),
            "seed {seed}: the verdicts differ"
        );
    }
}
