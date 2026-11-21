//! S1-08: peers and withheld heads (planning/architecture.md section 5.9: devices of one account
//! exchange the heads they have seen, so a hidden removal shows quickly). The directed scenario with the
//! stale-head attack, the junk and the fork of a peer runs green on every seed of 100 (the seeds change
//! the salts: the delays, the order, the keys).

use azlin_sim::{directed_named, small_world, Scenario, Verdict};

#[test]
fn s1_08_a_withheld_head_shows_a_peers_junk_is_quiet_and_a_peer_fork_is_an_alarm() {
    let Some(d) = directed_named("peers_and_withheld_heads") else {
        panic!("the directed scenario exists");
    };
    let mut met = 0u64;
    let mut attempted = 0u64;
    for seed in 0..100u64 {
        let Ok(scenario) = Scenario::new(0x5_1_08 + seed, small_world()) else {
            panic!("the small world must validate");
        };
        let outcome = scenario.replay(&(d.steps)());
        if let Verdict::Red(f) = &outcome.verdict {
            panic!(
                "seed {seed}: {} at step {}: {}\n{}",
                f.invariant.name(),
                f.at_step,
                f.message,
                f.trace_tail.join("\n")
            );
        }
        for kind in [
            azlin_sim::AttackKind::StaleHeadToOne,
            azlin_sim::AttackKind::PeerJunk,
            azlin_sim::AttackKind::PeerFork,
        ] {
            if let Some(tally) = outcome.coverage.attacks.get(&kind) {
                attempted += tally.attempted;
                met += tally.met;
            }
        }
    }
    println!("peer attacks: {attempted} attempted, {met} met");
    assert!(attempted >= 100, "the attacks were made: {attempted}");
    assert_eq!(met, attempted, "every attack met its verdict class");
}
