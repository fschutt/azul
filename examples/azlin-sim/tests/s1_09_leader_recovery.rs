//! S1-09: leader recovery (planning/architecture.md section 6.10 V1 "a node lost during an upload loses
//! nothing acknowledged", V3; section 1.4 rule 2). With an honest disk, 300 seeded crash and restart
//! schedules lose no acknowledged entry and re-broadcast every restore record; with a lying disk the loss
//! is detected on the devices that hold the lost entries (an alarm or a refusal) and healed only by a
//! `disaster` with a map-key record, never by a silent continuation. The invariants decide: every
//! schedule must run green.

use azlin_seams::sim::Rng;
use azlin_sim::draw::below;
use azlin_sim::{small_world, DiskFault, Profile, Scenario, SigChoice, Steps, Verdict};

fn schedule(seed: u64, lying: bool) -> Vec<azlin_sim::Step> {
    let mut rng = Rng::new(0x5_1_09 + seed);
    let mut steps = Steps::with_salt(seed as u32);
    for _ in 0..50 {
        match below(&mut rng, 14) {
            0 | 1 | 2 => {
                steps = steps.append(
                    below(&mut rng, 2) as u8,
                    below(&mut rng, 2) as u8,
                    below(&mut rng, 500) as u16,
                )
            }
            3 | 4 => steps = steps.poll(below(&mut rng, 3) as u8, 1 + below(&mut rng, 4) as u8),
            5 => steps = steps.tick(1 + below(&mut rng, 6) as u32),
            6 | 7 => steps = steps.deliver(1 + below(&mut rng, 4) as u8),
            8 => steps = steps.crash(0),
            9 => steps = steps.restart(0, false),
            10 => steps = steps.head_tick(),
            11 => steps = steps.offsite(),
            12 if lying => steps = steps.disk(0, DiskFault::Lie),
            12 => steps = steps.poll(0, 6),
            _ => steps = steps.disaster(below(&mut rng, 4) as u8, SigChoice::Map, true),
        }
    }
    steps.quiesce().build()
}

fn run(profile_lying: bool) -> (u64, u64) {
    let mut acked = 0;
    let mut alarms = 0;
    for seed in 0..300u64 {
        let config = small_world().with_profile(Profile::Calm);
        let Ok(scenario) = Scenario::new(0x5_1_09 + seed, config) else {
            panic!("the small world must validate");
        };
        let outcome = scenario.replay(&schedule(seed, profile_lying));
        acked += outcome.tallies.acked;
        alarms += outcome.tallies.alarms;
        if let Verdict::Red(f) = &outcome.verdict {
            panic!(
                "seed {seed} (lying disk: {profile_lying}): {} at step {}: {}\n{}\nstate:\n{}",
                f.invariant.name(),
                f.at_step,
                f.message,
                f.trace_tail.join("\n"),
                f.state.join("\n")
            );
        }
    }
    (acked, alarms)
}

#[test]
fn s1_09_with_an_honest_disk_no_acknowledged_entry_is_lost_and_restore_records_are_announced_again()
{
    let (acked, _) = run(false);
    assert!(acked > 300, "{acked} acknowledged entries");
}

#[test]
fn s1_09_with_a_lying_disk_the_loss_shows_and_is_healed_only_by_a_restore() {
    let (acked, alarms) = run(true);
    println!("acked {acked}, alarms {alarms}");
    assert!(acked > 100, "{acked} acknowledged entries");
}
