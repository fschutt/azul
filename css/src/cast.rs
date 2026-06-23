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
