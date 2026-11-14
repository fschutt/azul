//! The mutation guard of the engine's pure crates: [`mutant!`] (planning/architecture.md section
//! 1.4 rule 1 and the mutation standard of the QUALITY rules, rule 12: a guard that a fix added is
//! proved by turning it OFF and watching a test fail). The contract is the one of azlin-proto's
//! `mutants.rs` (the iso crate cannot be a dependency of an azul crate, so the macro is copied
//! here, in the one crate that declares the feature).
//!
//! ```ignore
//! if !azlin_seams::mutant!("chain_checks_prev") {
//!     if entry.prev() != head.hash { return Err(ChainRefusal::WrongPrev { .. }); }
//! }
//! ```
//!
//! The macro is `false` unless the crate is built with the feature `mutants` AND the environment
//! variable [`ENV`] (`AZLIN_MUTANT`) names the id: one mutant per process, read once. Without the
//! feature, `mutant!` is the constant `false` and the guarded code is the product. Only this
//! crate declares the feature (COMMON-W1R R19): a crate that carries guards depends on
//! `azlin-seams`, and its mutant hold runs
//! `AZLIN_MUTANT=<id> cargo test -p <crate> --features azlin-seams/mutants` once per id (it must
//! fail) and once without the variable (it must pass). No shipped build turns the feature on.

/// The environment variable that names the one mutant of a process.
///
/// Implements planning/architecture.md section 1.4 rule 1 (the guard reads its switch as a
/// parameter of the process, not of the code).
///
/// Caller: `on` below, the mutant holds of every crate that carries guards.
pub const ENV: &str = "AZLIN_MUTANT";

/// Whether the mutant `id` is the one this process runs with: always `false` without the feature
/// `mutants`, so a shipped build carries a constant `false` at every guard. With the feature, the
/// variable [`ENV`] is read once; an unset or empty variable names no mutant, and the id must match
/// exactly (no prefix, no case folding).
///
/// Implements planning/architecture.md section 1.4 rule 1 and QUALITY rule 12.
///
/// Caller: the `mutant!` macro, and through it the guards of azlin-seams and azlin-chain.
#[inline(always)]
#[must_use]
#[cfg_attr(not(feature = "mutants"), allow(unused_variables))]
pub fn on(id: &str) -> bool {
    #[cfg(feature = "mutants")]
    {
        use std::ffi::OsString;
        use std::sync::OnceLock;
        static ACTIVE: OnceLock<Option<OsString>> = OnceLock::new();
        let active = ACTIVE.get_or_init(|| std::env::var_os(ENV).filter(|value| !value.is_empty()));
        active.as_deref() == Some(std::ffi::OsStr::new(id))
    }
    #[cfg(not(feature = "mutants"))]
    {
        false
    }
}

/// `mutant!("id")`: whether the guard `id` is turned off in this process (see the module).
///
/// Implements planning/architecture.md section 1.4 rule 1 and QUALITY rule 12.
///
/// Caller: the guards of azlin-seams (`hash`, `bytes`, `verify`, `sim`) and azlin-chain.
#[macro_export]
macro_rules! mutant {
    ($id:literal) => {
        $crate::mutants::on($id)
    };
}
