//! Azlin sim: the seeded simulation harness that runs the new engine's crates TOGETHER
//! (planning/architecture.md section 1.4 rules 1 to 4 and section 12 "the deterministic
//! simulation"; lane W1-SIM, owner decision D-MAJORS 2c: "the 325 hand mutants of the lanes do not
//! test the crates TOGETHER").
//!
//! A deterministic, single-threaded world: one metadata group (the LEADER, with a disk), devices of
//! members of accounts (each with a disk and a clock), a seeded network between them, an operator
//! who holds the map key, and an adversary (the server, the database, the network) that holds the
//! group key but never a member, map or payments key. A SCENARIO is a seed and a budget; it draws
//! steps (append, deliver, tick, head, fetch, meet, restore, crash, partition, clock jump, disk
//! fault, attack, ...), executes each against the world and, after every step, checks the
//! INVARIANTS of the plan, written as named properties. A red run is shrunk with
//! `azlin_seams::sim::minimize` and kept as a regression test under `tests/seeds/`.
//!
//! The world is a function of (seed, config, steps): no clock is read, no file outside the seed
//! store, no thread, no network. The harness calls the crates through their public items only; what
//! a node does around the pure crates (persist before use, retry, fetch after an alarm, resend after
//! a restore) is the GLUE of later integration units: the smallest version the plan's text requires
//! is written here as numbered glue rules (`GLUE_RULES`).
//!
//! | module | what | stage |
//! |---|---|---|
//! | [`config`] | `Config`, `Profile`, `Scheme`, `FaultRates`, `Ppm`, `Stage`, the limits | 1 |
//! | [`step`] | the `Step` enum and its strict one-line text form | 1 |
//! | [`draw`] | seeded draws, the per-step generator | 1 |
//! | [`time`] | simulated clocks, the leader's stamp | 1 |
//! | [`net`] | the faulty network | 1 |
//! | [`disk`] | the faulty disk | 1 |
//! | `sched`, `keys`, `machine`, `leader`, `device`, `world` | jobs, keys, the machine seam, the nodes, the interpreter | 1 |
//! | `invariants`, `adversary`, `policy_world` | the named properties and what each attack must meet | 1 |
//! | `scenario`, `shrink`, `seeds`, `dirstore`, `coverage`, `report`, `mutants_table`, `directed` | the features (a) to (f) | 1 |
//! | the other modules | stubs of stages 2 to 4 | 2..4 |

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
#![warn(missing_docs)]

/// `Display` from a table of patterns and format strings, and `Error`: the harness has no
/// third-party dependency, so the typed errors write their own text.
macro_rules! error_text {
    ($ty:ty { $($pat:pat => $msg:literal),+ $(,)? }) => {
        impl core::fmt::Display for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                match self {
                    $($pat => write!(f, $msg)),+
                }
            }
        }

        impl std::error::Error for $ty {}
    };
}

pub mod config;
pub mod disk;
pub mod draw;
pub mod net;
pub mod step;
pub mod time;

// The files of the later units of stage 1 and of the later stages: one-line stubs that the units
// fill (launch rule 10: the file exists before its `mod` line does).
pub mod adversary;
pub mod coverage;
pub mod creds_world;
pub mod device;
pub mod directed;
pub mod dirstore;
pub mod enroll_world;
pub mod entitle_world;
pub mod glue;
pub mod invariants;
pub mod keys;
pub mod leader;
pub mod links_world;
pub mod machine;
pub mod mutants_table;
pub mod policy_world;
pub mod report;
pub mod reports_world;
pub mod scenario;
pub mod sched;
pub mod seeds;
pub mod shrink;
pub mod sync_world;
pub mod teamlog_world;
pub mod tlog_world;
pub mod world;

pub use config::{
    Config, ConfigError, Defect, FaultRates, LeaderVariant, Ppm, Profile, Scheme, Stage,
    DEFAULT_REAL_PERMILLE, FAST_SEEDS, FAST_STEPS, LONG_STEPS,
};
pub use coverage::{Coverage, FaultKind, Footprint, OpKind, EXCLUDED_PAIRS, REQUIRED_PAIRS};
pub use directed::{
    canary_files, crash_cases, directed_named, run_directed, small_world, CrashCase, Directed,
    DirectedExpect, Steps, DIRECTED, DIRECTED_SEED,
};
pub use dirstore::DirSeeds;
pub use invariants::{InvariantId, InvariantInfo, Status, INVARIANTS};
pub use mutants_table::{run_child, Killer, MutantRow, MUTANT_TABLE, WAIVERS};
pub use policy_world::{random_patch, PolicyKind, PolicyWorld};
pub use report::{run_seeds, FailedSeed, Report, RunOptions};
pub use scenario::{Failure, FailureKind, Outcome, Scenario, Tallies, Verdict};
pub use seeds::{
    replay_store, Expect, MemSeeds, ReplayReport, SeedFile, SeedParseError, SeedStore, SeedVerdict,
    StoreError,
};
pub use shrink::{printed, shrink, shrink_full};
pub use step::{arbitrary, AttackKind, DiskFault, SigChoice, Step, StepKind, StepParseError};
pub use world::{GlueRule, World, GLUE_RULES};
