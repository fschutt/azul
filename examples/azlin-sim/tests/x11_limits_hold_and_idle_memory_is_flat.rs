//! X11: limits hold and idle memory is flat (planning/architecture.md section 1.4 rule 4: everything has a
//! limit, stated up front). At every step of 50 seeds the footprint is at or below the ceilings (the
//! invariant checks it after every step; the test reads the peak); after a quiesce plus 1,000 ticks of
//! nothing the world is the world after the quiesce (no leak while idle); a node at its job bound refuses.

use azlin_sim::config::{
    MAX_ALARMS, MAX_DISK_RECORDS, MAX_INFLIGHT, MAX_JOBS_PER_NODE, MAX_OFFSITE_COPIES,
    MAX_OPEN_JOBS, MAX_RESTORES, MAX_TRACE_TAIL,
};
use azlin_sim::{small_world, Scenario, Stage, Steps, Verdict};

#[test]
fn x11_the_peak_of_the_footprint_stays_at_or_below_the_ceilings() {
    for seed in 0..50u64 {
        let outcome = Scenario::standard(seed, Stage::S1).run();
        if let Verdict::Red(f) = &outcome.verdict {
            panic!(
                "seed {seed}: {} at step {}: {}",
                f.invariant.name(),
                f.at_step,
                f.message
            );
        }
        let p = outcome.peak;
        assert!(
            p.inflight <= MAX_INFLIGHT,
            "seed {seed}: {} messages in flight",
            p.inflight
        );
        assert!(
            p.max_jobs_per_node <= MAX_JOBS_PER_NODE,
            "seed {seed}: {} jobs",
            p.max_jobs_per_node
        );
        assert!(
            p.open_jobs <= MAX_OPEN_JOBS,
            "seed {seed}: {} open jobs",
            p.open_jobs
        );
        assert!(p.max_alarms <= MAX_ALARMS);
        assert!(p.max_disk_records <= MAX_DISK_RECORDS);
        assert!(p.offsite_copies <= MAX_OFFSITE_COPIES);
        assert!(p.restores <= MAX_RESTORES);
        assert!(p.trace_tail <= MAX_TRACE_TAIL);
    }
}

#[test]
fn x11_idle_memory_is_flat_after_a_quiesce() {
    for seed in 0..10u64 {
        let first = Scenario::standard(seed, Stage::S1).run();
        let mut steps = first.steps.clone();
        let mut idle = Steps::with_salt(77);
        for _ in 0..1_000 {
            idle = idle.tick(7);
        }
        steps.extend(idle.build());
        let second = Scenario::standard(seed, Stage::S1).replay(&steps);
        assert_eq!(
            first.world_hash, second.world_hash,
            "seed {seed}: a thousand idle ticks changed the world"
        );
        assert_eq!(
            first.peak, second.peak,
            "seed {seed}: the footprint grew while idle"
        );
    }
}

#[test]
fn x11_a_node_at_its_job_bound_refuses_and_counts() {
    let mut steps = Steps::new();
    for i in 0..(MAX_JOBS_PER_NODE as u16 + 13) {
        steps = steps.append(0, 0, i);
    }
    let Ok(scenario) = Scenario::new(1, small_world()) else {
        panic!("the small world must validate");
    };
    let outcome = scenario.replay(&steps.quiesce().build());
    if let Verdict::Red(f) = &outcome.verdict {
        panic!(
            "{} at step {}: {}",
            f.invariant.name(),
            f.at_step,
            f.message
        );
    }
    assert_eq!(
        outcome.peak.max_jobs_per_node, MAX_JOBS_PER_NODE,
        "the node filled to its bound and no further"
    );
    assert!(
        outcome.coverage.skipped >= 13,
        "{} steps were skips",
        outcome.coverage.skipped
    );
}
