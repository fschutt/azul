//! S1-05: concurrent writers do not fork (planning/architecture.md section 5.9: two different entries at
//! one position are a fork; section 6.10 V1: nothing acknowledged is lost). Two to four devices append
//! to one chain from stale tips with loss, duplication and reordering on; no fork, no gap in any copy,
//! and the retry rule G4 ends in at most five attempts for each proposal (a property over 200 seeds;
//! the invariants check every copy after every step).

use azlin_seams::sim::Rng;
use azlin_sim::draw::below;
use azlin_sim::{Config, Profile, Scenario, Scheme, Stage, Steps, Verdict};

fn writers(seed: u64) -> (Config, Vec<azlin_sim::Step>) {
    let mut rng = Rng::new(0x5_1_05 + seed);
    let members = 2 + below(&mut rng, 2) as u8;
    let devices = 1 + below(&mut rng, 2) as u8;
    let config = Config::standard(seed, Stage::S1)
        .with_profile(Profile::Chaos)
        .with_scheme(Scheme::Fake)
        .with_size(1, members, devices, 0)
        .with_steps(1);
    let mut steps = Steps::new();
    for _ in 0..60 {
        match below(&mut rng, 5) {
            0 | 1 => {
                steps = steps.append(below(&mut rng, 6) as u8, 0, below(&mut rng, 1000) as u16);
            }
            2 => steps = steps.deliver(1 + below(&mut rng, 4) as u8),
            3 => steps = steps.poll(below(&mut rng, 7) as u8, 1 + below(&mut rng, 3) as u8),
            _ => steps = steps.tick(1 + below(&mut rng, 20) as u32),
        }
    }
    (config, steps.quiesce().build())
}

#[test]
fn s1_05_concurrent_writers_do_not_fork() {
    let mut acked = 0u64;
    let mut alarms = 0u64;
    for seed in 0..200u64 {
        let (config, steps) = writers(seed);
        let Ok(scenario) = Scenario::new(seed, config) else {
            panic!("the configuration of seed {seed} must validate");
        };
        let outcome = scenario.replay(&steps);
        acked += outcome.tallies.acked;
        alarms += outcome.tallies.alarms;
        match &outcome.verdict {
            Verdict::Green => {}
            Verdict::Red(f) => panic!(
                "seed {seed}: {} at step {}: {}\n{}",
                f.invariant.name(),
                f.at_step,
                f.message,
                f.trace_tail.join("\n")
            ),
        }
    }
    println!("acked {acked}, alarms {alarms}");
    assert!(
        acked > 400,
        "the writers made progress: {acked} acknowledged entries"
    );
}
