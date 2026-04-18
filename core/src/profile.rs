//! Unified profiling gate.
//!
//! Reads `AZ_PROFILE` once on first access, caches the result forever.
//! One mode at a time — so profiling one concern doesn't drown out another.
//!
//! Modes:
//! - `AZ_PROFILE=memory`  — heap-breakdown dumps (StyledDom, LayoutCache,
//!                          text cache, cascade maps, RSS).
//! - `AZ_PROFILE=cpu`     — per-phase wall-clock timings from `Probe::span`
//!                          (layout, style, cascade, paint, callbacks, …),
//!                          dumped once per frame so stuttering frames
//!                          are easy to spot.
//! - `AZ_PROFILE=cascade` — narrow diagnostic for prop-cache work: top-N
//!                          CSS properties by cascade-walk count per frame.
//!
//! Anything else (or unset) leaves the quick path silent.
//!
//! ## Portability
//! - **macOS / Linux**: full support. Span timings via `Instant`; RSS
//!   checkpoints via `task_info` / `/proc/self/statm`.
//! - **Windows**: span timings work. RSS checkpoints silently read 0
//!   (the RSS helpers in `azul_layout::probe` are `cfg(unix)`-gated).
//!   `AZ_PROFILE=cpu` and `AZ_PROFILE=cascade` are fully functional;
//!   `AZ_PROFILE=memory` prints what it can and skips the unix-only bits.
//! - **WASM (`target_family = "wasm"`)**: `Instant::now()` panics on
//!   browser WASM (no monotonic clock) and `libc::getrusage` isn't
//!   available. The probe module detects WASM at compile time and
//!   forces the no-op impl (spans record nothing, `Probe::drain()`
//!   returns empty). `AZ_PROFILE=cpu` then prints
//!   `"probe unavailable on this target (timings = ???)"` instead
//!   of crashing. `AZ_PROFILE=cascade` and `AZ_PROFILE=memory` are
//!   unaffected — they don't depend on `Instant` or libc.

use std::sync::OnceLock;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ProfileMode {
    None,
    Memory,
    Cpu,
    Cascade,
}

#[inline]
pub fn mode() -> ProfileMode {
    static MODE: OnceLock<ProfileMode> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("AZ_PROFILE").as_deref() {
        Ok("memory") | Ok("mem") => ProfileMode::Memory,
        Ok("cpu") | Ok("perf") => ProfileMode::Cpu,
        Ok("cascade") | Ok("css") => ProfileMode::Cascade,
        _ => ProfileMode::None,
    })
}

#[inline]
pub fn memory_enabled() -> bool {
    matches!(mode(), ProfileMode::Memory)
}

#[inline]
pub fn cpu_enabled() -> bool {
    matches!(mode(), ProfileMode::Cpu)
}

#[inline]
pub fn cascade_enabled() -> bool {
    matches!(mode(), ProfileMode::Cascade)
}
