//! The long mode (planning/architecture.md section 12: the deterministic simulation at full size):
//! `cargo test -p azlin-sim --release -- --ignored long`, or the binary `simrun` for a night. The options
//! come from the environment: `AZLIN_SIM_SEEDS` (default 100,000), `AZLIN_SIM_FIRST`, `AZLIN_SIM_STEPS`
//! (default 2,000), `AZLIN_SIM_STAGE`, `AZLIN_SIM_REAL_PERMILLE`, `AZLIN_SIM_OUT` (where new seed files go).

use std::path::PathBuf;

use azlin_sim::{run_seeds, DirSeeds, RunOptions, Stage, LONG_STEPS};

fn env_u64(name: &str) -> Option<u64> {
    // gate:env allow: the night run reads its options
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

#[test]
#[ignore]
fn long() {
    let opts = RunOptions {
        first_seed: env_u64("AZLIN_SIM_FIRST").unwrap_or(0),
        seeds: env_u64("AZLIN_SIM_SEEDS").unwrap_or(100_000),
        stage: env_u64("AZLIN_SIM_STAGE")
            .and_then(|n| u8::try_from(n).ok())
            .and_then(Stage::from_number)
            .unwrap_or(Stage::HIGHEST),
        steps: env_u64("AZLIN_SIM_STEPS").map_or(LONG_STEPS, |v| v as u32),
        real_permille: env_u64("AZLIN_SIM_REAL_PERMILLE").map_or(50, |v| v as u16),
        shrink: true,
        max_failures: 20,
    };
    // gate:env allow: the night run reads its options
    let out = std::env::var("AZLIN_SIM_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("seeds")
        });
    let mut store = DirSeeds::open(&out);
    let report = run_seeds(&opts, &mut store);
    println!("{}", report.render());
    assert!(report.is_green(), "{}", report.render());
}
