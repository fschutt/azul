//! Internal numeric-cast helpers.
//!
//! Each function isolates one `as` conversion that clippy flags
//! (`cast_precision_loss` / `cast_possible_truncation` / `cast_sign_loss` /
//! `cast_possible_wrap`) behind a single documented `#[allow]`, so call sites
//! stay lint-clean without scattering raw casts or per-file helpers. Every one is
//! a behaviour-preserving wrapper around `as` (float→int saturates, int→int wraps
//! per Rust semantics); they exist to *name the intent*, not to change behaviour.

/// `isize` → `f32`. Loses precision only for magnitudes above 2^24; layout
/// coordinates and CSS dimensions stay far within that range.
#[inline]
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub(crate) const fn isize_to_f32(v: isize) -> f32 {
    v as f32
}

/// `usize` → `f32`. Loses precision only above 2^24 (counts/lengths stay small).
#[inline]
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub(crate) const fn usize_to_f32(v: usize) -> f32 {
    v as f32
}

/// `i32` → `f32`. Loses precision only for magnitudes above 2^24.
#[inline]
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub(crate) const fn i32_to_f32(v: i32) -> f32 {
    v as f32
}

/// `f32` → `isize` (truncating). `as` saturates NaN→0 and out-of-range to the
/// `isize` bounds; callers that want rounding `.round()`/`.floor()` first.
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub(crate) const fn f32_to_isize(v: f32) -> isize {
    v as isize
}

/// `f32` → `i32` (truncating). `as` saturates NaN→0 and out-of-range to `i32`
/// bounds; callers that want rounding `.round()` first.
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub(crate) const fn f32_to_i32(v: f32) -> i32 {
    v as i32
}

/// `f32` → `u32` (truncating, sign-dropping). `as` saturates NaN→0, negatives→0,
/// out-of-range→`u32::MAX`; callers validate non-negative / `.round()` first.
#[inline]
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) const fn f32_to_u32(v: f32) -> u32 {
    v as u32
}
