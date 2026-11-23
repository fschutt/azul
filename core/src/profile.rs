//! Unified profiling gate.
//!
//! `AZ_PROFILE` is read once, at the start, by the dll (`ProfileSettings::from_lookup`) and
//! installed here ([`install`]); this module reads no environment. A process that never installs
//! any profiles nothing. Value is a comma-separated list of tokens; unknown tokens are ignored,
//! whitespace is trimmed, matching is case-insensitive.
//!
//! Tokens:
//! - `memory`  — heap-breakdown dumps (StyledDom, LayoutCache, text cache, cascade maps, RSS).
//!   Printed to stderr once per frame.
//! - `cpu`     — per-phase wall-clock timings from `Probe::span` (layout, style, cascade, paint,
//!   callbacks, …), dumped once per frame so stuttering frames are easy to spot.
//! - `cascade` — narrow diagnostic for prop-cache work: top-N CSS properties by cascade-walk count
//!   per frame.
//! - `heap`    — phase-boundary heap probes in `regenerate_layout` (`emit_phase_heap`). By
//!   themselves print nothing — pair with `jsonl` + `AZ_PROFILE_OUT` to persist.
//! - `jsonl`   — format heap probes as JSONL to the file named by `AZ_PROFILE_OUT=<path>`. Requires
//!   `heap` to do anything.
//! - `detail`  — opt-in to the fine-grained per-step probes inside each phase (e.g. `rf_*` labels
//!   inside `rust_fontconfig::request_fonts`, and the `_extra` cache-size payloads). Layered on top
//!   of `heap`.
//!
//! ## Examples
//! - `AZ_PROFILE=cpu` — per-phase CPU timings to stderr.
//! - `AZ_PROFILE=heap,jsonl AZ_PROFILE_OUT=/tmp/run.jsonl` → coarse phase heap probes to JSONL.
//! - `AZ_PROFILE=heap,jsonl,detail AZ_PROFILE_OUT=/tmp/detail.jsonl` → fine-grained (per-step) heap
//!   probes to JSONL.
//! - `AZ_PROFILE=cpu,cascade` — both dumps simultaneously.
//!
//! Tokens are independent flags, not mutually exclusive modes. Unset
//! or empty leaves every quick path silent.
//!
//! ## Path for jsonl output
//! `AZ_PROFILE_OUT` is read separately (not folded into `AZ_PROFILE`
//! because the value can contain `,` and `=` and a path is a different
//! shape from a flag). When `jsonl` is set but `AZ_PROFILE_OUT` is
//! unset, writers silently skip — no stderr fallback so benchmarks
//! don't get polluted.
//!
//! ## Portability
//! - **macOS / Linux**: full support. Span timings via `Instant`; RSS checkpoints via `task_info` /
//!   `/proc/self/statm`.
//! - **Windows**: span timings work. RSS checkpoints silently read 0 (the RSS helpers in
//!   `azul_layout::probe` are `cfg(unix)`-gated).
//! - **WASM (`target_family = "wasm"`)**: `Instant::now()` panics on browser WASM (no monotonic
//!   clock) and `libc::getrusage` isn't available. The probe module detects WASM at compile time
//!   and forces the no-op impl.

use alloc::{string::String, vec::Vec};

/// Set of active `AZ_PROFILE` tokens. Parsed once from the env var.
// independent profile toggles parsed from the env var; a bitflags type would
// not improve this flat set of named booleans.
#[allow(clippy::struct_excessive_bools)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileFlags {
    pub memory: bool,
    pub cpu: bool,
    pub cascade: bool,
    pub heap: bool,
    pub jsonl: bool,
    pub detail: bool,
}

impl ProfileFlags {
    pub(crate) fn parse(value: &str) -> Self {
        let mut f = Self::default();
        for tok in value.split(',') {
            let t = tok.trim();
            if t.eq_ignore_ascii_case("memory") || t.eq_ignore_ascii_case("mem") {
                f.memory = true;
            } else if t.eq_ignore_ascii_case("cpu") || t.eq_ignore_ascii_case("perf") {
                f.cpu = true;
            } else if t.eq_ignore_ascii_case("cascade") || t.eq_ignore_ascii_case("css") {
                f.cascade = true;
            } else if t.eq_ignore_ascii_case("heap") {
                f.heap = true;
            } else if t.eq_ignore_ascii_case("jsonl") {
                f.jsonl = true;
            } else if t.eq_ignore_ascii_case("detail") {
                f.detail = true;
            }
        }
        f
    }
}

/// What the environment asked of the profiling gate.
///
/// The tokens of `AZ_PROFILE`, the destination of the JSONL heap probes (`AZ_PROFILE_OUT`), and
/// the lines worth saying about them. [`ProfileSettings::from_lookup`] (module
/// `crate::settings`) reads it; [`install`] puts it where [`flags`] and [`out_path`] find it.
/// `Default` is the empty environment's: everything off, no destination.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileSettings {
    /// The active modes.
    pub flags: ProfileFlags,
    /// `AZ_PROFILE_OUT`: where the JSONL heap probes go. `None`: nowhere.
    pub out_path: Option<String>,
    /// What [`install`] says on stderr once: an unknown token, a mode that would emit nothing.
    pub warnings: Vec<String>,
}

// The profiling gate of the process: installed once at the start, then only read. It is asked
// from per-property and per-frame code that carries no window (`prop_cache::get_property`), which
// is why it is not a field of one.
#[cfg(feature = "std")]
use std::sync::OnceLock;

#[cfg(feature = "std")]
static SETTINGS: OnceLock<ProfileSettings> = OnceLock::new();

/// Installs the settings of this process and says what is worth saying about them, once.
///
/// The words go to stderr. The first install wins: the gate never changes while the process
/// runs. Without `std` profiling is always off and nothing is kept.
pub fn install(settings: ProfileSettings) {
    #[cfg(feature = "std")]
    {
        for line in &settings.warnings {
            eprintln!("{line}");
        }
        drop(SETTINGS.set(settings));
    }
    #[cfg(not(feature = "std"))]
    {
        drop(settings);
    }
}

#[cfg(feature = "std")]
#[inline]
#[must_use]
pub fn flags() -> ProfileFlags {
    SETTINGS.get().map_or_else(ProfileFlags::default, |s| s.flags)
}

/// `no_std` builds have no environment; profiling is always off.
#[cfg(not(feature = "std"))]
#[inline]
#[must_use]
pub fn flags() -> ProfileFlags {
    ProfileFlags::default()
}

/// `AZ_PROFILE_OUT=<path>` — destination for JSONL heap probes.
/// Returns `None` if unset.
#[cfg(feature = "std")]
#[inline]
#[must_use]
pub fn out_path() -> Option<&'static str> {
    SETTINGS.get().and_then(|s| s.out_path.as_deref())
}

/// `no_std` builds have no environment; no output path.
#[cfg(not(feature = "std"))]
#[inline]
#[must_use]
pub fn out_path() -> Option<&'static str> {
    None
}

#[inline]
#[must_use]
pub fn memory_enabled() -> bool {
    flags().memory
}

#[inline]
#[must_use]
pub fn cpu_enabled() -> bool {
    flags().cpu
}

#[inline]
#[must_use]
pub fn cascade_enabled() -> bool {
    flags().cascade
}

#[inline]
#[must_use]
pub fn heap_enabled() -> bool {
    flags().heap
}

#[inline]
#[must_use]
pub fn jsonl_enabled() -> bool {
    flags().jsonl
}

#[inline]
#[must_use]
pub fn detail_enabled() -> bool {
    flags().detail
}

#[cfg(test)]
#[path = "profile_test.rs"]
mod profile_test;
