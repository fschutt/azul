//! The fast run (planning/architecture.md section 12: the deterministic simulation proves the logic):
//! 200 seeds of 250 steps at the highest stage, plus every directed scenario and every crash case.
//! X03 (green and fast), X04 (every fault, every await point, every attack) and X05 (the required
//! operation pairs) read the same run, so it is made once.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use azlin_sim::sched::{JobKind, HANDLERS};
use azlin_sim::{
    crash_cases, run_directed, run_seeds, Coverage, DirectedExpect, FaultKind, MemSeeds, Report,
    RunOptions, Scenario, Verdict, DIRECTED, DIRECTED_SEED, EXCLUDED_PAIRS, REQUIRED_PAIRS,
};

struct Fast {
    report: Report,
    seed_coverage: Coverage,
    directed_coverage: Coverage,
    elapsed: Duration,
    directed_failures: Vec<String>,
    crash_failures: Vec<String>,
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn fast() -> &'static Fast {
    static RUN: OnceLock<Fast> = OnceLock::new();
    RUN.get_or_init(|| {
        let mut opts = RunOptions::fast();
        if let Some(n) = env_u64("AZLIN_SIM_SEEDS") {
            opts.seeds = n;
        }
        if let Some(n) = env_u64("AZLIN_SIM_FIRST") {
            opts.first_seed = n;
        }
        if let Some(n) = env_u64("AZLIN_SIM_STEPS") {
            opts.steps = n as u32;
        }
        let started = Instant::now();
        let mut store = MemSeeds::new();
        let report = run_seeds(&opts, &mut store);
        let elapsed = started.elapsed();
        let seed_coverage = report.coverage.clone();
        let mut directed_coverage = Coverage::default();
        let mut directed_failures = Vec::new();
        for d in DIRECTED {
            match run_directed(d) {
                Some(outcome) => {
                    directed_coverage.merge(&outcome.coverage);
                    let ok = match (d.expect, &outcome.verdict) {
                        (DirectedExpect::Green, Verdict::Green) => true,
                        (DirectedExpect::Red(id), Verdict::Red(f)) => f.invariant == id,
                        _ => false,
                    };
                    if !ok {
                        directed_failures.push(format!("{}: {:?}", d.name, outcome.verdict));
                    }
                }
                None => directed_failures.push(format!("{}: could not be built", d.name)),
            }
        }
        let mut crash_failures = Vec::new();
        for case in crash_cases() {
            let Ok(scenario) = Scenario::new(DIRECTED_SEED, azlin_sim::small_world()) else {
                crash_failures.push(String::from("the small world does not validate"));
                continue;
            };
            let outcome = scenario.replay(&case.steps);
            directed_coverage.merge(&outcome.coverage);
            if let Verdict::Red(f) = &outcome.verdict {
                crash_failures.push(format!(
                    "{} after phase {}: {} at step {}: {}",
                    case.kind.handler().name,
                    case.phase,
                    f.invariant.name(),
                    f.at_step,
                    f.message
                ));
            }
        }
        for (label, text) in [
            ("failures", &report.failures),
            ("known gaps", &report.known_gaps),
            ("harness", &report.harness),
        ] {
            for failed in text {
                println!(
                    "{label}: seed 0x{:016x} {} at step {}: {}",
                    failed.seed,
                    failed.failure.invariant.name(),
                    failed.failure.at_step,
                    failed.failure.message
                );
                println!("    config: {}", failed.config);
                println!("    steps: {}", failed.steps.len());
                for step in &failed.steps {
                    println!("    {}", step.to_line());
                }
            }
        }
        Fast {
            report,
            seed_coverage,
            directed_coverage,
            elapsed,
            directed_failures,
            crash_failures,
        }
    })
}

#[test]
fn x03_the_fast_run_is_green_and_fast() {
    let run = fast();
    println!(
        "fast run: {} seeds, {} steps, {:.1} s",
        run.report.seeds,
        run.report.steps,
        run.elapsed.as_secs_f64()
    );
    assert!(
        run.report.harness.is_empty(),
        "harness failures:\n{}",
        run.report.render()
    );
    assert!(
        run.report.failures.is_empty(),
        "product failures:\n{}",
        run.report.render()
    );
    assert!(
        run.directed_failures.is_empty(),
        "directed scenarios: {:?}",
        run.directed_failures
    );
    assert!(
        run.crash_failures.is_empty(),
        "crash cases: {:#?}",
        run.crash_failures
    );
    assert!(
        run.elapsed < Duration::from_secs(120),
        "the fast run took {:?}",
        run.elapsed
    );
    if run.elapsed > Duration::from_secs(60) {
        println!(
            "WARNING: the fast run took {:?}, over the 60 s of H10",
            run.elapsed
        );
    }
    let tallies = run.report.tallies;
    println!(
        "steps {} skipped {} acked {} restores {} alarms {} delivered {}",
        tallies.steps,
        tallies.skipped,
        tallies.acked,
        tallies.restores,
        tallies.alarms,
        tallies.delivered
    );
    assert!(
        tallies.skipped * 5 <= tallies.steps,
        "skips are {} of {} steps: over 20 %",
        tallies.skipped,
        tallies.steps
    );
}

#[test]
fn x04_every_fault_every_await_point_and_every_attack_fired() {
    let run = fast();
    let mut all = run.seed_coverage.clone();
    all.merge(&run.directed_coverage);
    let mut missing = Vec::new();
    for kind in FaultKind::ALL {
        let count = all.fault(kind);
        println!("fault {} {}", kind.name(), count);
        if count < 20 {
            missing.push(format!("fault {} fired {count} times", kind.name()));
        }
    }
    for kind in JobKind::ALL {
        for phase in 1..=kind.phases() {
            let count = all.crash_points.get(&(kind, phase)).copied().unwrap_or(0);
            if count < 3 {
                missing.push(format!(
                    "crash at {} after phase {phase}: {count} times",
                    kind.handler().name
                ));
            }
        }
    }
    for kind in azlin_sim::AttackKind::ALL {
        // The fast run is the seeds and the directed scenarios.
        let tally = all.attacks.get(&kind).copied().unwrap_or_default();
        println!(
            "attack {} attempted {} met {}",
            kind.name(),
            tally.attempted,
            tally.met
        );
        if tally.attempted < 5 {
            missing.push(format!(
                "attack {} attempted {} times",
                kind.name(),
                tally.attempted
            ));
        }
        if tally.met != tally.attempted {
            missing.push(format!(
                "attack {}: {} of {} attempts met the verdict class",
                kind.name(),
                tally.met,
                tally.attempted
            ));
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
    assert_eq!(HANDLERS.len(), JobKind::ALL.len());
}

#[test]
fn x05_the_required_operation_pairs_were_interleaved() {
    let run = fast();
    let mut all = run.seed_coverage.clone();
    all.merge(&run.directed_coverage);
    let missing = all.missing_required();
    for (open, running) in &missing {
        println!(
            "missing pair: {} open while {} ran",
            open.name(),
            running.name()
        );
    }
    println!(
        "{} required pairs, {} excluded, {} missing",
        REQUIRED_PAIRS.len(),
        EXCLUDED_PAIRS.len(),
        missing.len()
    );
    assert!(
        missing.len() * 100 <= REQUIRED_PAIRS.len() * 5,
        "more than 5 % of the required pairs were not interleaved three times: {missing:?}"
    );
}
