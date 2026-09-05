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

/// The one knob most users set. Everything below is a fine-grained override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiftMode {
    /// Normal serve: lift only what THIS app reaches, no verification. Default.
    App,
    /// Base image: lift the WHOLE api.json surface into the cache, then exit
    /// before serving (same effect as the `web-prelift://` URL). What the
    /// GHCR base image runs so a derived app finds the library warm.
    Full,
    /// App closure PLUS AZ_RELOC_VERIFY - fresh-lift every cache hit and
    /// byte-compare it. The correctness gate; slow, for CI and debugging.
    Verify,
}

impl LiftMode {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "app" | "" => Some(Self::App),
            "full" | "prelift" | "base" => Some(Self::Full),
            "verify" => Some(Self::Verify),
            _ => None,
        }
    }
}

/// Parsed form of the pipeline's environment. Cheap to copy by reference;
/// obtain it with [`lift_env`].
#[derive(Debug)]
pub struct LiftEnv {
    /// The high-level mode (`AZ_LIFT_MODE`); the fields below refine it.
    pub mode: LiftMode,
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
    /// Functions per remill-lift invocation (the --batch_manifest wave size).
    /// 1 disables batching. The point is SPAWN COUNT: a full lift shells out
    /// ~100k times one-per-function, and process creation is both the hang
    /// risk and, with per-process AV scanning, a real cost.
    pub lift_batch: usize,
    /// Mark every lifted function as ENTERED at runtime, so a first paint can
    /// be separated into what actually executes and what is only reachable.
    /// Writes one byte per function into the store-log ring region, so it is
    /// MUTUALLY EXCLUSIVE with AZ_LOG_STORES. See inject_fn_coverage.
    /// Run `wasm-opt -Oz` on the linked wasm. OFF by default: it shrinks the
    /// RAW module ~11% but grows the BROTLI-compressed transfer ~3%, because
    /// it removes the redundancy the compressor was exploiting. Turn it on
    /// when browser compile time matters more than bytes on the wire.
    pub wasm_opt_enable: bool,
    pub fn_coverage: bool,
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
    // ---- transform toggles: each disables (or forces) a DEFAULT-ON transform.
    // They exist to A/B a regression against the transform, and each is folded
    // into the object cache key so a flipped switch never serves a stale object.
    pub no_fix_sp: bool,
    pub no_trap_selfloop: bool,
    pub no_indirect_dispatch: bool,
    pub full_cs_restore: bool,
    pub keep_alias_scope: bool,
    pub no_host_scope: bool,

    // ---- in-wasm recorders: emit tracing code into the lifted output.
    // Never on in a shipped bundle; all excluded from the cache key so an
    // instrumented object is not reused for a clean build.
    pub write_trace: Option<String>,
    pub read_trace: Option<String>,
    pub reg_trace: Option<String>,
    pub reg_trace_nowrap: bool,
    pub sp_trace: bool,
    pub log_stores: Option<String>,
    pub log_selfloop_val: Option<String>,
    pub lswin_lo: Option<u64>,
    pub lswin_hi: Option<u64>,
    pub lsid_lo: Option<u64>,
    pub fuel: Option<String>,
    pub fuel_limit: Option<u64>,
    pub tag_unreachable: bool,
    pub wasm_mirror_trace: bool,

    // ---- opt bisect rig: pins which LLVM pass miscompiles a function.
    pub opt_level: Option<String>,
    pub lowopt_fns: Option<String>,
    pub bisect_fn: Option<String>,
    pub bisect_limit: Option<String>,
    pub lto_level: Option<String>,
    pub wasm_ld_mllvm: Option<String>,

    // ---- pipeline mode ----
    pub native_remill: bool,
    pub merged_compile: bool,
    pub disable_auto_merge: bool,
    pub enable_shards: bool,

    // ---- toolchain overrides (paths, not behaviour) ----
    pub remill_lift_bin: Option<PathBuf>,
    pub llc: Option<PathBuf>,
    pub llvm_opt: Option<PathBuf>,
    pub llvm_link: Option<PathBuf>,
    pub wasm_ld: Option<PathBuf>,
    pub wasm_opt: Option<PathBuf>,
}

impl LiftEnv {
    fn from_process() -> Self {
        let mode = std::env::var("AZ_LIFT_MODE")
            .ok()
            .and_then(|s| LiftMode::parse(&s))
            .unwrap_or(LiftMode::App);
        Self {
            mode,
            // Verify is OFF unless the mode asks for it (or the raw flag is set
            // as an override): the cache is trusted by default now that its
            // correctness is established, so a normal run reuses it instead of
            // re-lifting every hit.
            reloc_verify: mode == LiftMode::Verify || flag("AZ_RELOC_VERIFY"),
            // Default ON: serving a bundle known to be broken is worse than
            // refusing to start.
            lift_strict: std::env::var("AZ_LIFT_STRICT").map(|v| v != "0").unwrap_or(true),
            unk_trap: flag("AZ_UNK_TRAP"),

            lift_cache: flag("AZ_LIFT_CACHE"),
            no_lift_cache: flag("AZ_NO_LIFT_CACHE"),
            lift_cache_dir: std::env::var_os("AZ_LIFT_CACHE_DIR").map(PathBuf::from),

            lift_jobs: num("AZ_LIFT_JOBS").filter(|n: &usize| *n >= 1),
            wasm_opt_enable: flag("AZ_WASM_OPT"),
            fn_coverage: flag("AZ_FN_COVERAGE"),
            lift_batch: num("AZ_LIFT_BATCH").filter(|n: &usize| *n >= 1).unwrap_or(64),
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
            no_fix_sp: flag("AZ_NO_FIX_SP"),
            no_trap_selfloop: flag("AZ_NO_TRAP_SELFLOOP"),
            no_indirect_dispatch: flag("AZ_NO_INDIRECT_DISPATCH"),
            full_cs_restore: flag("AZ_FULL_CS_RESTORE"),
            keep_alias_scope: flag("AZ_KEEP_ALIAS_SCOPE"),
            no_host_scope: flag("AZ_NO_HOST_SCOPE"),

            write_trace: std::env::var("AZ_WRITE_TRACE").ok(),
            read_trace: std::env::var("AZ_READ_TRACE").ok(),
            reg_trace: std::env::var("AZ_REG_TRACE").ok(),
            reg_trace_nowrap: flag("AZ_REG_TRACE_NOWRAP"),
            sp_trace: flag("AZ_SP_TRACE"),
            log_stores: std::env::var("AZ_LOG_STORES").ok(),
            log_selfloop_val: std::env::var("AZ_LOG_SELFLOOP_VAL").ok(),
            lswin_lo: num("AZ_LSWIN_LO"),
            lswin_hi: num("AZ_LSWIN_HI"),
            lsid_lo: num("AZ_LSID_LO"),
            fuel: std::env::var("AZ_FUEL").ok(),
            fuel_limit: num("AZ_FUEL_LIMIT"),
            tag_unreachable: flag("AZ_TAG_UNREACHABLE"),
            wasm_mirror_trace: flag("AZ_WASM_MIRROR_TRACE"),

            opt_level: std::env::var("AZ_OPT_LEVEL").ok(),
            lowopt_fns: std::env::var("AZ_LOWOPT_FNS").ok(),
            bisect_fn: std::env::var("AZ_BISECT_FN").ok(),
            bisect_limit: std::env::var("AZ_BISECT_LIMIT").ok(),
            lto_level: std::env::var("AZ_LTO_LEVEL").ok().filter(|s| !s.is_empty()),
            wasm_ld_mllvm: std::env::var("AZ_WASM_LD_MLLVM").ok(),

            native_remill: flag("AZ_NATIVE_REMILL"),
            merged_compile: flag("AZ_REMILL_MERGED_COMPILE"),
            disable_auto_merge: flag("AZ_REMILL_DISABLE_AUTO_MERGE"),
            enable_shards: flag("AZ_ENABLE_SHARDS"),

            remill_lift_bin: std::env::var_os("REMILL_LIFT_BIN").map(PathBuf::from),
            llc: std::env::var_os("LLC").map(PathBuf::from),
            llvm_opt: std::env::var_os("LLVM_OPT").map(PathBuf::from),
            llvm_link: std::env::var_os("LLVM_LINK").map(PathBuf::from),
            wasm_ld: std::env::var_os("WASM_LD").map(PathBuf::from),
            wasm_opt: std::env::var_os("WASM_OPT").map(PathBuf::from),
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
        let mode = match self.mode {
            LiftMode::App => "app",
            LiftMode::Full => "full",
            LiftMode::Verify => "verify",
        };
        if on.is_empty() {
            format!("mode={mode}")
        } else {
            format!("mode={mode} {}", on.join(" "))
        }
    }
}

/// The process-wide parsed environment.
pub fn lift_env() -> &'static LiftEnv {
    static ENV: OnceLock<LiftEnv> = OnceLock::new();
    ENV.get_or_init(LiftEnv::from_process)
}
