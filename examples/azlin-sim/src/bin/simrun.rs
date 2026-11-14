//! The night binary: many seeds of the simulation, the report on standard output (planning/architecture.md
//! section 12: the deterministic simulation proves the logic at full size).
//!
//! `cargo run -p azlin-sim --release --bin simrun -- --stage 1 --seeds 100000 --out <dir>`. The exit code is
//! 0 for a green run, 1 for a product failure, 2 for a harness bug and 3 for a usage error. The options of
//! the environment (`AZLIN_SIM_SEEDS`, `AZLIN_SIM_FIRST`, `AZLIN_SIM_STEPS`, `AZLIN_SIM_STAGE`,
//! `AZLIN_SIM_REAL_PERMILLE`, `AZLIN_SIM_OUT`) are read here and only here: the library takes plain data.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use azlin_sim::{canary_files, run_seeds, DirSeeds, RunOptions, SeedStore, Stage, LONG_STEPS};

// gate:env allow: the night run reads its options
fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn main() -> ExitCode {
    let mut opts = RunOptions {
        first_seed: env_u64("AZLIN_SIM_FIRST").unwrap_or(0),
        seeds: env_u64("AZLIN_SIM_SEEDS").unwrap_or(100_000),
        stage: Stage::HIGHEST,
        steps: env_u64("AZLIN_SIM_STEPS").map_or(LONG_STEPS, |v| v as u32),
        real_permille: env_u64("AZLIN_SIM_REAL_PERMILLE").map_or(50, |v| v as u16),
        shrink: true,
        max_failures: 20,
    };
    if let Some(stage) = env_u64("AZLIN_SIM_STAGE")
        .and_then(|n| u8::try_from(n).ok())
        .and_then(Stage::from_number)
    {
        opts.stage = stage;
    }
    let mut out = std::env::var("AZLIN_SIM_OUT").ok().map(PathBuf::from);
    let mut canaries: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next();
        match (arg.as_str(), value) {
            ("--stage", Some(v)) => match v.parse::<u8>().ok().and_then(Stage::from_number) {
                Some(stage) => opts.stage = stage,
                None => return usage(),
            },
            ("--seeds", Some(v)) => match v.parse() {
                Ok(n) => opts.seeds = n,
                Err(_) => return usage(),
            },
            ("--first", Some(v)) => match v.parse() {
                Ok(n) => opts.first_seed = n,
                Err(_) => return usage(),
            },
            ("--steps", Some(v)) => match v.parse() {
                Ok(n) => opts.steps = n,
                Err(_) => return usage(),
            },
            ("--out", Some(v)) => out = Some(PathBuf::from(v)),
            ("--write-canaries", Some(v)) => canaries = Some(PathBuf::from(v)),
            _ => return usage(),
        }
    }
    if let Some(dir) = canaries {
        return write_canaries(&dir);
    }
    let dir = out.unwrap_or_else(|| PathBuf::from("tests/seeds"));
    let mut store = DirSeeds::open(&dir);
    let report = run_seeds(&opts, &mut store);
    println!("{}", report.render());
    if !report.harness.is_empty() {
        ExitCode::from(2)
    } else if !report.failures.is_empty() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Writes the canary seed files that the directory does not hold yet (a store never overwrites).
fn write_canaries(dir: &Path) -> ExitCode {
    let mut store = DirSeeds::open(dir);
    for file in canary_files() {
        match store.create(&file.name, &file.to_text()) {
            Ok(()) => println!("wrote {}", file.name),
            Err(error) => println!("{}: {error}", file.name),
        }
    }
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: simrun [--stage N] [--seeds N] [--first N] [--steps N] [--out DIR] | --write-canaries DIR"
    );
    ExitCode::from(3)
}
