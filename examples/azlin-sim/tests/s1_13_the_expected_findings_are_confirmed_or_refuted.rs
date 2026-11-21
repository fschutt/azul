//! S1-13: the expected findings (section 13 of the brief, items 3 and 7) are confirmed or refuted. The lane
//! runs the Adversarial and RestoreHeavy profiles and prints, for each invariant, how many seeds broke it:
//! (a) a device that saw the new epoch's head BEFORE the record and is then offered a validly signed
//! lost-timeline head with a higher position (`inv_pin_the_epoch_never_goes_down`), and (b) a `disaster`
//! onto a leader whose clock is behind the old leader's (`inv_restore_every_listed_chain_is_reanchored`:
//! the late-record rule of the crate compares two leaders' stamps). A finding is a committed shrunk
//! `expect: fails` seed with its drift row (the registry status and the seed file agree: tests/seeds.rs);
//! this test reports what the profiles find.

use std::collections::BTreeMap;

use azlin_sim::{run_seeds, MemSeeds, Profile, RunOptions, Scenario, Stage, Verdict};

#[test]
fn s1_13_the_profiles_report_what_they_find() {
    let mut found: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for profile in [Profile::RestoreHeavy, Profile::Adversarial] {
        for seed in 0..150u64 {
            let config = azlin_sim::Config::standard(seed * 4 + 2, Stage::S1)
                .with_profile(profile)
                .with_steps(250);
            let Ok(scenario) = Scenario::new(seed * 4 + 2, config) else {
                continue;
            };
            let outcome = scenario.run();
            let mut names: Vec<String> = outcome
                .known_gaps
                .iter()
                .map(|g| g.invariant.name().to_string())
                .collect();
            if let Verdict::Red(f) = &outcome.verdict {
                names.push(f.invariant.name().to_string());
            }
            for name in names {
                found
                    .entry(format!("{} {}", profile.name(), name))
                    .or_default()
                    .push(seed * 4 + 2);
            }
        }
    }
    for (what, seeds) in &found {
        println!("{what}: {} seeds, first {:?}", seeds.len(), seeds.first());
    }
    // The library entry point finds and keeps them the same way: a store takes one file per invariant.
    let mut store = MemSeeds::new();
    let opts = RunOptions {
        seeds: 40,
        ..RunOptions::fast()
    };
    let report = run_seeds(&opts, &mut store);
    println!(
        "{} failures, {} known gaps in 40 standard seeds",
        report.failures.len(),
        report.known_gaps.len()
    );
}
