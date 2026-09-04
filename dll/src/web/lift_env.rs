//! Every environment knob the lift pipeline reads, parsed once.
//!
//! These used to be ~67 scattered `env::var` calls, several of them inside
//! per-function or per-8-byte-slot loops, with no single place that listed what
//! the pipeline actually responds to. That made the set impossible to audit:
//! knobs nothing set survived for months, one was documented but read nowhere,
//! and two more had been deleted from the code while docs still described them.
//!
//! Everything is read here, once, and reached through [`lift_env`]. A knob that
//! is not a field of [`LiftEnv`] does not exist - which is the property that
//! makes the set reviewable, and makes a stale one obvious.
//!
//! Reading once also fixes a real cost: probes like `AZ_TRACE_STALE_PTR` sat in
//! the pointer-mirror loop and were evaluated millions of times per run.

use std::{path::PathBuf, sync::OnceLock};

fn flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| !v.is_empty() && v != "0")
}

fn num<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.trim().parse().ok()
}

/// Parsed form of the pipeline's environment. Cheap to copy by reference;
/// obtain it with [`lift_env`].
#[derive(Debug)]
pub struct LiftEnv {
    // ---- correctness gates (never remove these without a replacement) ----
    /// Fresh-lift every translated function and byte-compare it, healing bad
    /// templates in place. The instrument that found the reloc-cache bugs.
    pub reloc_verify: bool,
    /// Refuse to serve a bundle whose lift audit found fatal problems.
    /// Default ON; `AZ_LIFT_STRICT=0` downgrades to warnings.
    pub lift_strict: bool,
    /// Turn an unmatched indirect dispatch into a trap, so the stack names the
    /// caller instead of the call silently vanishing.
    pub unk_trap: bool,

    // ---- caching ----
    pub lift_cache: bool,
    pub no_lift_cache: bool,
    pub lift_cache_dir: Option<PathBuf>,

    // ---- scheduling / limits ----
    /// Object-pool worker count. `None` = derive from available parallelism.
    pub lift_jobs: Option<usize>,
    /// Seconds before a wedged `CreateProcess` aborts the run; 0 disables.
    pub spawn_watchdog_secs: u64,
    /// Seconds a single tool invocation may take, including its pipe reads.
    pub tool_timeout_secs: u64,
    pub mini_max_depth: Option<usize>,
    pub cb_max_depth: Option<usize>,

    // ---- diagnostics ----
    pub keep_scratch: bool,
    pub wasm_debug: bool,
    pub trace_stale_ptr: bool,
    pub preflight: bool,
    pub remill_debug: bool,
    pub skip_wasm_opt: bool,
}

impl LiftEnv {
    fn from_process() -> Self {
        Self {
            reloc_verify: flag("AZ_RELOC_VERIFY"),
            // Default ON: serving a bundle known to be broken is worse than
            // refusing to start.
            lift_strict: std::env::var("AZ_LIFT_STRICT").map(|v| v != "0").unwrap_or(true),
            unk_trap: flag("AZ_UNK_TRAP"),

            lift_cache: flag("AZ_LIFT_CACHE"),
            no_lift_cache: flag("AZ_NO_LIFT_CACHE"),
            lift_cache_dir: std::env::var_os("AZ_LIFT_CACHE_DIR").map(PathBuf::from),

            lift_jobs: num("AZ_LIFT_JOBS").filter(|n: &usize| *n >= 1),
            spawn_watchdog_secs: num("AZ_SPAWN_WATCHDOG_SECS").unwrap_or(300),
            tool_timeout_secs: num("AZ_TOOL_TIMEOUT_SECS").unwrap_or(900),
            mini_max_depth: num("AZ_MINI_MAX_DEPTH"),
            cb_max_depth: num("AZ_CB_MAX_DEPTH"),

            keep_scratch: flag("AZ_REMILL_KEEP_SCRATCH"),
            wasm_debug: flag("AZ_WASM_DEBUG"),
            trace_stale_ptr: flag("AZ_TRACE_STALE_PTR"),
            preflight: flag("AZ_PREFLIGHT"),
            remill_debug: flag("AZ_REMILL_DEBUG"),
            skip_wasm_opt: flag("AZ_REMILL_SKIP_WASM_OPT"),
        }
    }

    /// One line naming every non-default setting, for the startup log. Makes a
    /// run self-describing: what a bundle was built with is otherwise
    /// unrecoverable after the fact.
    pub fn summary(&self) -> String {
        let mut on: Vec<&str> = Vec::new();
        for (name, set) in [
            ("AZ_RELOC_VERIFY", self.reloc_verify),
            ("AZ_UNK_TRAP", self.unk_trap),
            ("AZ_LIFT_CACHE", self.lift_cache),
            ("AZ_NO_LIFT_CACHE", self.no_lift_cache),
            ("AZ_REMILL_KEEP_SCRATCH", self.keep_scratch),
            ("AZ_WASM_DEBUG", self.wasm_debug),
            ("AZ_TRACE_STALE_PTR", self.trace_stale_ptr),
            ("AZ_PREFLIGHT", self.preflight),
            ("AZ_REMILL_DEBUG", self.remill_debug),
            ("AZ_REMILL_SKIP_WASM_OPT", self.skip_wasm_opt),
        ] {
            if set {
                on.push(name);
            }
        }
        if !self.lift_strict {
            on.push("AZ_LIFT_STRICT=0");
        }
        if on.is_empty() {
            "defaults".to_string()
        } else {
            on.join(" ")
        }
    }
}

/// The process-wide parsed environment.
pub fn lift_env() -> &'static LiftEnv {
    static ENV: OnceLock<LiftEnv> = OnceLock::new();
    ENV.get_or_init(LiftEnv::from_process)
}
