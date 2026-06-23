//! Unified profiling gate.
//!
//! Reads `AZ_PROFILE` once on first access, caches the result forever.
//! Value is a comma-separated list of tokens; unknown tokens are ignored,
//! whitespace is trimmed, matching is case-insensitive.
//!
//! Tokens:
//! - `memory`  — heap-breakdown dumps (StyledDom, LayoutCache, text cache,
//!               cascade maps, RSS). Printed to stderr once per frame.
//! - `cpu`     — per-phase wall-clock timings from `Probe::span` (layout,
//!               style, cascade, paint, callbacks, …), dumped once per
//!               frame so stuttering frames are easy to spot.
//! - `cascade` — narrow diagnostic for prop-cache work: top-N CSS
//!               properties by cascade-walk count per frame.
//! - `heap`    — phase-boundary heap probes in `regenerate_layout`
//!               (`emit_phase_heap`). By themselves print nothing —
//!               pair with `jsonl` + `AZ_PROFILE_OUT` to persist.
//! - `jsonl`   — format heap probes as JSONL to the file named by
//!               `AZ_PROFILE_OUT=<path>`. Requires `heap` to do anything.
//! - `detail`  — opt-in to the fine-grained per-step probes inside each
//!               phase (e.g. `rf_*` labels inside
//!               `rust_fontconfig::request_fonts`, and the `_extra`
//!               cache-size payloads). Layered on top of `heap`.
//!
//! ## Examples
//! - `AZ_PROFILE=cpu` — per-phase CPU timings to stderr.
//! - `AZ_PROFILE=heap,jsonl AZ_PROFILE_OUT=/tmp/run.jsonl`
//!     → coarse phase heap probes to JSONL.
//! - `AZ_PROFILE=heap,jsonl,detail AZ_PROFILE_OUT=/tmp/detail.jsonl`
//!     → fine-grained (per-step) heap probes to JSONL.
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
//! - **macOS / Linux**: full support. Span timings via `Instant`; RSS
//!   checkpoints via `task_info` / `/proc/self/statm`.
//! - **Windows**: span timings work. RSS checkpoints silently read 0
//!   (the RSS helpers in `azul_layout::probe` are `cfg(unix)`-gated).
//! - **WASM (`target_family = "wasm"`)**: `Instant::now()` panics on
//!   browser WASM (no monotonic clock) and `libc::getrusage` isn't
//!   available. The probe module detects WASM at compile time and
//!   forces the no-op impl.

#[cfg(feature = "std")]
use std::sync::OnceLock;

/// Set of active `AZ_PROFILE` tokens. Parsed once from the env var.
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
    fn parse(value: &str) -> Self {
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

#[cfg(feature = "std")]
#[inline]
pub fn flags() -> ProfileFlags {
    static FLAGS: OnceLock<ProfileFlags> = OnceLock::new();
    *FLAGS.get_or_init(|| {
        std::env::var("AZ_PROFILE")
            .map(|v| ProfileFlags::parse(&v))
            .unwrap_or_default()
    })
}

/// `no_std` builds have no environment; profiling is always off.
#[cfg(not(feature = "std"))]
#[inline]
pub fn flags() -> ProfileFlags {
    let _ = ProfileFlags::parse;
    ProfileFlags::default()
}

/// `AZ_PROFILE_OUT=<path>` — destination for JSONL heap probes.
/// Returns `None` if unset. Cached on first access.
#[cfg(feature = "std")]
#[inline]
pub fn out_path() -> Option<&'static str> {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var("AZ_PROFILE_OUT").ok())
        .as_deref()
}

/// `no_std` builds have no environment; no output path.
#[cfg(not(feature = "std"))]
#[inline]
pub fn out_path() -> Option<&'static str> {
    None
}

#[inline]
#[must_use] pub fn memory_enabled() -> bool { flags().memory }

#[inline]
#[must_use] pub fn cpu_enabled() -> bool { flags().cpu }

#[inline]
#[must_use] pub fn cascade_enabled() -> bool { flags().cascade }

#[inline]
#[must_use] pub fn heap_enabled() -> bool { flags().heap }

#[inline]
#[must_use] pub fn jsonl_enabled() -> bool { flags().jsonl }

#[inline]
#[must_use] pub fn detail_enabled() -> bool { flags().detail }

#[cfg(test)]
mod tests {
    use super::ProfileFlags;

    #[test]
    fn parse_single_token() {
        let f = ProfileFlags::parse("cpu");
        assert!(f.cpu && !f.memory && !f.heap);
    }

    #[test]
    fn parse_multiple_tokens() {
        let f = ProfileFlags::parse("heap,jsonl,detail");
        assert!(f.heap && f.jsonl && f.detail);
        assert!(!f.cpu && !f.memory);
    }

    #[test]
    fn parse_is_case_insensitive_and_trims() {
        let f = ProfileFlags::parse(" Heap , JSONL ");
        assert!(f.heap && f.jsonl);
    }

    #[test]
    fn parse_ignores_unknown_tokens() {
        let f = ProfileFlags::parse("cpu,bogus,heap");
        assert!(f.cpu && f.heap);
    }

    #[test]
    fn parse_accepts_aliases() {
        let f = ProfileFlags::parse("mem,perf,css");
        assert!(f.memory && f.cpu && f.cascade);
    }
}
