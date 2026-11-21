//! Helpers shared by the test files of this crate: the seed budget, a runner that spreads the seeds over a
//! few threads, and the one cached run that two tests read (planning/architecture.md section 12 E1).

#![allow(dead_code)]

use std::ops::Range;
use std::sync::OnceLock;

use azlin_escalate_e1::compare::{differential_plan, Failure, Outcome, Plan, Stats};

pub mod cells;
/// The authority table v0 as data (the file the brief names, `tests/table_data.rs`).
#[path = "../table_data.rs"]
pub mod table_data;

/// The number of random entries after the setup of each sequence.
pub const STEPS: usize = 28;

/// The number of seeds of a run of `cargo test`: `AZLIN_ESCALATE_SEEDS`, default 3,000 (200,000 for the
/// nightly).
pub fn seed_count() -> u64 {
    std::env::var("AZLIN_ESCALATE_SEEDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(3_000)
}

/// What a run of many seeds found.
pub struct Run {
    /// The counts of the passing sequences.
    pub stats: Stats,
    /// The failing sequences, by seed.
    pub failures: Vec<Failure>,
    /// How many seeds were run.
    pub seeds: u64,
}

/// Runs the seeds on up to four threads.
pub fn run_seeds(seeds: Range<u64>, steps: usize, plan: Plan) -> Run {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 4) as u64;
    let total = seeds.end.saturating_sub(seeds.start);
    let results: Vec<(Stats, Vec<Failure>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let seeds = seeds.clone();
                scope.spawn(move || {
                    let mut stats = Stats::default();
                    let mut failures: Vec<Failure> = Vec::new();
                    for seed in seeds {
                        if seed % threads != t {
                            continue;
                        }
                        match differential_plan(seed, steps, plan) {
                            Outcome::Pass(s) => stats.merge(&s),
                            Outcome::Fail(f) => failures.push(*f),
                        }
                    }
                    (stats, failures)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a worker thread panicked"))
            .collect()
    });
    let mut stats = Stats::default();
    let mut failures: Vec<Failure> = Vec::new();
    for (s, f) in results {
        stats.merge(&s);
        failures.extend(f);
    }
    failures.sort_by_key(|f| f.seed);
    Run {
        stats,
        failures,
        seeds: total,
    }
}

/// The message of a failed run: the failing seeds (the first fifty) and the first failure with its minimal
/// sequence.
pub fn fail_report(run: &Run) -> String {
    let seeds: Vec<String> = run
        .failures
        .iter()
        .take(50)
        .map(|f| f.seed.to_string())
        .collect();
    let first = run
        .failures
        .first()
        .map(|f| f.message())
        .unwrap_or_default();
    format!(
        "{} of {} sequences failed; failing seeds: {}\n{}",
        run.failures.len(),
        run.seeds,
        seeds.join(", "),
        first
    )
}

/// The run of the default budget that the differential test and the coverage test both read.
pub fn main_run() -> &'static Run {
    static RUN: OnceLock<Run> = OnceLock::new();
    RUN.get_or_init(|| run_seeds(0..seed_count(), STEPS, Plan::Mixed))
}
