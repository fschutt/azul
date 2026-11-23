//! The settings of the web-lift transpiler: every environment variable the lift reads, named in
//! this one file and read ONCE, where the lift starts (`run_web`).
//!
//! The transpiler (`transpiler_remill`, `symbol_table`, `lift_audit`, `run_web`) used to ask
//! `std::env` at the place of use, about 70 times, some of them per function lifted. Now
//! [`WebLiftSettings::from_process`] builds the value once and `run_web` hands it to the
//! transpiler it creates (`RemillTranspiler::new(settings)`), to the symbol table it builds and to
//! the lift audit. The constructor is pure ([`WebLiftSettings::from_lookup`]): a test gives it a
//! `MapEnv`, and [`Default`] is what an EMPTY environment gives: the strict audit, the 8192-function
//! cap of a callback lift, no tracer, no cache.
//!
//! The variables are tool knobs of the web backend (`AZ_BACKEND=web://..`): the toolchain
//! (`REMILL_LIFT_BIN`, `LLC`, `LLVM_OPT`, `LLVM_LINK`, `WASM_LD`, `WASM_OPT`), the lift caches
//! (`AZ_LIFT_CACHE*`), the IR passes (`AZ_NO_FIX_SP`, ...) and the instrumentation that is meant for
//! one debugging run (`AZ_WRITE_TRACE`, `AZ_FUEL`, ...). The web and wasm builds have no process
//! environment; a lift that runs inside wasm gets [`Default`].

use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

use azul_css::env::{EnvSource, NoEnv};

/// The cap on the functions one callback lift may pull in, without `AZ_CB_MAX_DEPTH`.
pub const DEFAULT_CB_MAX_DEPTH: usize = 8192;
/// The cap on the functions the eventloop lift (`azul-mini.wasm`) may pull in, without
/// `AZ_MINI_MAX_DEPTH`.
pub const DEFAULT_MINI_MAX_DEPTH: usize = 4096;
/// Block executions the loop-fuel tracer allows, without `AZ_FUEL_LIMIT`.
pub const DEFAULT_FUEL_LIMIT: u64 = 200_000_000;
/// Upper bound of the address window the store tracer logs, without `AZ_LSWIN_HI`.
pub const DEFAULT_LSWIN_HI: u32 = 196_608;

/// What the web-lift transpiler takes from the environment, read once.
///
/// A flag the old code asked with `var_os(..).is_some()` is true for any value, a flag it asked
/// with `var(..).is_ok()` is true for a UTF-8 value only, and a number that does not parse is the
/// default, as before.
#[derive(Debug, Clone, PartialEq)]
pub struct WebLiftSettings {
    // ---- the run (`run_web`) ----
    /// `AZ_SKIP_LAYOUT_LIFT`: skip the lift of the layout callbacks (fast iteration on callbacks).
    pub skip_layout_lift: bool,
    /// `AZ_LIFT_STRICT`: the lift audit refuses to serve a build with fatal findings; `0` serves
    /// anyway. On unless the value is `0`.
    pub lift_strict: bool,
    /// `AZ_ENABLE_SHARDS` set and `AZ_BUNDLED_LEGACY` not: the framework symbols lift into
    /// per-function shards instead of being bundled into every module.
    pub shards: bool,
    /// `AZ_PREFLIGHT`: print the report of functions that lifted with `__remill_error`.
    pub preflight: bool,
    // ---- the toolchain ----
    /// `REMILL_LIFT_BIN`: the `remill-lift-17` binary (kept as the OS gave it; the discovery
    /// reads its text).
    pub remill_lift_bin: Option<OsString>,
    /// `LLC`.
    pub llc: Option<String>,
    /// `LLVM_OPT`.
    pub llvm_opt: Option<String>,
    /// `LLVM_LINK`.
    pub llvm_link: Option<String>,
    /// `WASM_LD`.
    pub wasm_ld: Option<String>,
    /// `WASM_OPT`.
    pub wasm_opt: Option<String>,
    /// `AZ_REMILL_SKIP_WASM_OPT`: leave the linked modules as `wasm-ld` wrote them.
    pub skip_wasm_opt: bool,
    /// `AZ_NATIVE_REMILL`: lift in-process (needs the `web-transpiler-static` feature).
    pub native_remill: bool,
    /// `AZ_LIFT_JOBS`: worker threads of the transitive lift (at least 1), `None` = by the cores.
    pub lift_jobs: Option<usize>,
    // ---- scratch and caches ----
    /// The directory scratch files and the default lift cache go to: the platform's answer in the
    /// process ([`WebLiftSettings::from_process`]), `TMPDIR` or `/tmp` from a lookup.
    pub temp_dir: PathBuf,
    /// `AZ_REMILL_KEEP_SCRATCH`: keep the per-function IR and objects after the transpiler drops.
    pub keep_scratch: bool,
    /// `AZ_LIFT_CACHE`: opt in to the on-disk lift caches.
    pub lift_cache: bool,
    /// `AZ_NO_LIFT_CACHE`: no object cache even with `AZ_LIFT_CACHE`.
    pub no_lift_cache: bool,
    /// `AZ_LIFT_CACHE_DIR`: the cache root instead of `<temp_dir>/az-lift-cache` (not when empty).
    pub lift_cache_dir: Option<PathBuf>,
    /// `AZ_RELOC_VERIFY`: lift fresh as well and compare with a cache translation.
    pub reloc_verify: bool,
    // ---- IR passes and link ----
    /// `AZ_KEEP_ALIAS_SCOPE`: keep the alias-scope metadata after the link.
    pub keep_alias_scope: bool,
    /// `AZ_NO_FIX_SP`: skip the SP-preservation pass.
    pub no_fix_sp: bool,
    /// `AZ_NO_TRAP_SELFLOOP`: leave the empty self-loops.
    pub no_trap_selfloop: bool,
    /// `AZ_NO_INDIRECT_DISPATCH`: no indirect-call dispatcher object.
    pub no_indirect_dispatch: bool,
    /// `AZ_FULL_CS_RESTORE`: restore the callee-saved registers unconditionally.
    pub full_cs_restore: bool,
    /// `AZ_NO_HOST_SCOPE` (a UTF-8 value): no host/guest alias-scope separation.
    pub no_host_scope: bool,
    /// `AZ_UNK_TRAP` (not `0`): unmatched indirect dispatches trap instead of returning.
    pub unk_trap: bool,
    /// `AZ_LTO_LEVEL`: the LTO opt level of the link (not when empty).
    pub lto_level: Option<String>,
    /// `AZ_WASM_LD_MLLVM`: comma-separated flags forwarded as `-mllvm <flag>` to `wasm-ld`.
    pub wasm_ld_mllvm: Option<String>,
    /// `AZ_WASM_DEBUG`: link with the names and without the size passes.
    pub wasm_debug: bool,
    /// `AZ_CB_MAX_DEPTH`: the function cap of a callback lift.
    pub cb_max_depth: usize,
    /// `AZ_MINI_MAX_DEPTH`: the function cap of the eventloop lift.
    pub mini_max_depth: usize,
    /// `AZ_REMILL_DEBUG`: duplicate-target and duplicate-object diagnostics.
    pub remill_debug: bool,
    /// `AZ_REMILL_MERGED_COMPILE`: force the merged native compile on.
    pub merged_compile: bool,
    /// `AZ_REMILL_DISABLE_AUTO_MERGE`: force the merged native compile off.
    pub disable_auto_merge: bool,
    /// `AZ_REMILL_MERGED_ALWAYSINLINE`: force `alwaysinline` on every function of a merged compile.
    pub merged_alwaysinline: bool,
    /// The LLVM `-O<level>` flag of `opt` and `llc` (`AZ_OPT_LEVEL`: `0`, `1`, `s`, `z`; else `-O2`).
    pub opt_flag: &'static str,
    /// `AZ_LOWOPT_FNS`: stems (comma-separated) of the functions that compile at `-O0`.
    pub lowopt_fns: Vec<String>,
    /// `AZ_BISECT_FN`: the function whose `opt` passes are bisected.
    pub bisect_fn: Option<String>,
    /// `AZ_BISECT_LIMIT`: the `-opt-bisect-limit` of that function.
    pub bisect_limit: Option<String>,
    // ---- data mirror ----
    /// `AZ_FORCE_MIRROR_VMADDRS`: comma-separated vmaddrs of pages to mirror.
    pub force_mirror_vmaddrs: Option<String>,
    /// `AZ_FORCE_WHOLE_PAGE`: mirror whole pages, not the precise ranges.
    pub force_whole_page: bool,
    /// `AZ_EMPTY_GROUP_TRACE`: log the hashbrown empty-group runs.
    pub empty_group_trace: bool,
    /// `AZ_WASM_MIRROR_TRACE`: log the pages that are mirrored or skipped.
    pub wasm_mirror_trace: bool,
    /// `AZ_TRACE_STALE_PTR`: log pointer-like values that are not in a tracked image.
    pub trace_stale_ptr: bool,
    // ---- instrumentation of one debugging run ----
    /// `AZ_WRITE_TRACE`: `ALL` or comma-separated stems; trace the guest writes.
    pub write_trace: Option<String>,
    /// `AZ_READ_TRACE`: the same for the guest double reads.
    pub read_trace: Option<String>,
    /// `AZ_REG_TRACE`: the same for the register stores.
    pub reg_trace: Option<String>,
    /// `AZ_REG_TRACE_NOWRAP`: clamp the register-trace ring instead of wrapping it.
    pub reg_trace_nowrap: bool,
    /// `AZ_LOG_SELFLOOP_VAL`: the same for the values that route into empty self-loops.
    pub log_selfloop_val: Option<String>,
    /// `AZ_LOG_STORES`: the same for the store-address tracer.
    pub log_stores: Option<String>,
    /// `AZ_TAG_UNREACHABLE`: the same for the unreachable tagger.
    pub tag_unreachable: Option<String>,
    /// `AZ_FUEL`: the same for the loop-fuel hang finder.
    pub fuel: Option<String>,
    /// `AZ_FUEL_LIMIT`: block executions before the fuel tracer traps.
    pub fuel_limit: u64,
    /// `AZ_LSWIN_HI`: upper bound of the logged store address (`0x..` or decimal).
    pub lswin_hi: u32,
    /// `AZ_LSWIN_LO`: lower bound of the logged store address.
    pub lswin_lo: u32,
    /// `AZ_LSID_LO`: the first store id that is logged.
    pub lsid_lo: u32,
    /// `AZ_SP_TRACE`: the SP trajectory tracer (aarch64 only).
    pub sp_trace: bool,
}

/// `0x..` as hex, else decimal.
fn hex_or_decimal(s: &str) -> Option<u32> {
    s.strip_prefix("0x")
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .or_else(|| s.parse().ok())
}

impl WebLiftSettings {
    /// The settings `env` asks for; an unset variable is the default (`from_lookup(&NoEnv)` is
    /// [`WebLiftSettings::default`]).
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let set = |name: &str| env.is_set(name);
        let text = |name: &str| env.var(name);
        Self {
            skip_layout_lift: set("AZ_SKIP_LAYOUT_LIFT"),
            lift_strict: env.var("AZ_LIFT_STRICT").is_none_or(|v| v != "0"),
            shards: set("AZ_ENABLE_SHARDS") && !set("AZ_BUNDLED_LEGACY"),
            preflight: set("AZ_PREFLIGHT"),
            remill_lift_bin: env.var_os("REMILL_LIFT_BIN"),
            llc: text("LLC"),
            llvm_opt: text("LLVM_OPT"),
            llvm_link: text("LLVM_LINK"),
            wasm_ld: text("WASM_LD"),
            wasm_opt: text("WASM_OPT"),
            skip_wasm_opt: set("AZ_REMILL_SKIP_WASM_OPT"),
            native_remill: set("AZ_NATIVE_REMILL"),
            lift_jobs: text("AZ_LIFT_JOBS")
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|n| *n >= 1),
            temp_dir: env
                .var_os("TMPDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("/tmp")),
            keep_scratch: set("AZ_REMILL_KEEP_SCRATCH"),
            lift_cache: set("AZ_LIFT_CACHE"),
            no_lift_cache: set("AZ_NO_LIFT_CACHE"),
            lift_cache_dir: env
                .var_os("AZ_LIFT_CACHE_DIR")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
            reloc_verify: set("AZ_RELOC_VERIFY"),
            keep_alias_scope: set("AZ_KEEP_ALIAS_SCOPE"),
            no_fix_sp: set("AZ_NO_FIX_SP"),
            no_trap_selfloop: set("AZ_NO_TRAP_SELFLOOP"),
            no_indirect_dispatch: set("AZ_NO_INDIRECT_DISPATCH"),
            full_cs_restore: set("AZ_FULL_CS_RESTORE"),
            no_host_scope: text("AZ_NO_HOST_SCOPE").is_some(),
            unk_trap: text("AZ_UNK_TRAP").is_some_and(|v| v != "0"),
            lto_level: text("AZ_LTO_LEVEL").filter(|s| !s.is_empty()),
            wasm_ld_mllvm: text("AZ_WASM_LD_MLLVM"),
            wasm_debug: set("AZ_WASM_DEBUG"),
            cb_max_depth: text("AZ_CB_MAX_DEPTH")
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_CB_MAX_DEPTH),
            mini_max_depth: text("AZ_MINI_MAX_DEPTH")
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_MINI_MAX_DEPTH),
            remill_debug: set("AZ_REMILL_DEBUG"),
            merged_compile: set("AZ_REMILL_MERGED_COMPILE"),
            disable_auto_merge: set("AZ_REMILL_DISABLE_AUTO_MERGE"),
            merged_alwaysinline: set("AZ_REMILL_MERGED_ALWAYSINLINE"),
            opt_flag: match text("AZ_OPT_LEVEL").as_deref() {
                Some("0") => "-O0",
                Some("1") => "-O1",
                Some("s" | "S") => "-Os",
                Some("z" | "Z") => "-Oz",
                _ => "-O2",
            },
            lowopt_fns: text("AZ_LOWOPT_FNS")
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
            bisect_fn: text("AZ_BISECT_FN"),
            bisect_limit: text("AZ_BISECT_LIMIT"),
            force_mirror_vmaddrs: text("AZ_FORCE_MIRROR_VMADDRS"),
            force_whole_page: set("AZ_FORCE_WHOLE_PAGE"),
            empty_group_trace: set("AZ_EMPTY_GROUP_TRACE"),
            wasm_mirror_trace: set("AZ_WASM_MIRROR_TRACE"),
            trace_stale_ptr: set("AZ_TRACE_STALE_PTR"),
            write_trace: text("AZ_WRITE_TRACE"),
            read_trace: text("AZ_READ_TRACE"),
            reg_trace: text("AZ_REG_TRACE"),
            reg_trace_nowrap: set("AZ_REG_TRACE_NOWRAP"),
            log_selfloop_val: text("AZ_LOG_SELFLOOP_VAL"),
            log_stores: text("AZ_LOG_STORES"),
            tag_unreachable: text("AZ_TAG_UNREACHABLE"),
            fuel: text("AZ_FUEL"),
            fuel_limit: text("AZ_FUEL_LIMIT")
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_FUEL_LIMIT),
            lswin_hi: text("AZ_LSWIN_HI")
                .and_then(|s| hex_or_decimal(&s))
                .unwrap_or(DEFAULT_LSWIN_HI),
            lswin_lo: text("AZ_LSWIN_LO")
                .and_then(|s| hex_or_decimal(&s))
                .unwrap_or(0),
            lsid_lo: text("AZ_LSID_LO")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            sp_trace: set("AZ_SP_TRACE"),
        }
    }

    /// The settings of THIS process, read once: the variables through the dll's one reader
    /// ([`crate::env::ProcessEnv`], made once here) and the scratch directory the platform names
    /// (`TMPDIR` on Unix, `TMP`/`TEMP` and the profile on Windows).
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            temp_dir: std::env::temp_dir(),
            ..Self::from_lookup(&crate::env::ProcessEnv::new())
        }
    }

    /// The text of `REMILL_LIFT_BIN`: `None` when it is unset or not UTF-8, as
    /// `std::env::var("REMILL_LIFT_BIN").ok()`.
    #[must_use]
    pub fn remill_lift_bin_text(&self) -> Option<&str> {
        self.remill_lift_bin.as_deref().and_then(OsStr::to_str)
    }

    /// The seven tracer targets whose match with a function changes that function's IR
    /// (`AZ_WRITE_TRACE`, `AZ_READ_TRACE`, `AZ_REG_TRACE`, `AZ_LOG_SELFLOOP_VAL`, `AZ_LOG_STORES`,
    /// `AZ_TAG_UNREACHABLE`, `AZ_FUEL`): an instrumented object must never come out of a cache.
    #[must_use]
    pub fn instrumentation_targets(&self) -> [Option<&str>; 7] {
        [
            self.write_trace.as_deref(),
            self.read_trace.as_deref(),
            self.reg_trace.as_deref(),
            self.log_selfloop_val.as_deref(),
            self.log_stores.as_deref(),
            self.tag_unreachable.as_deref(),
            self.fuel.as_deref(),
        ]
    }
}

impl Default for WebLiftSettings {
    /// The behaviour of an EMPTY environment.
    fn default() -> Self {
        Self::from_lookup(&NoEnv)
    }
}

#[cfg(test)]
#[path = "lift_settings_test.rs"]
mod tests;
