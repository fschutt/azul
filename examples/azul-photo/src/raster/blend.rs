//! Blend modes and source-over compositing (W3C Compositing and Blending
//! Level 1, the separable modes).
//!
//! The accumulator is premultiplied f32; the source is straight f32. The
//! result of blending a source with alpha `as` onto a backdrop with alpha `ab`:
//!
//! ```text
//! co = cs * as * (1 - ab) + cb' * (1 - as) + as * ab * B(Cb, Cs)
//! ao = as + ab * (1 - as)
//! ```
//!
//! (`cb'` premultiplied, `Cb` = `cb' / ab`), so a mode acts only where the
//! backdrop is opaque and every mode is source-over on a transparent one.

use serde::{Deserialize, Serialize};

const INV_255: f32 = 1.0 / 255.0;

/// How a layer's colour combines with the colour below it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    Add,
}

impl BlendMode {
    /// Every mode, in the order the layer panel lists them.
    pub const ALL: [Self; 8] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::Difference,
        Self::Add,
    ];

    /// The name the UI shows (and doc.json stores).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
            Self::Overlay => "Overlay",
            Self::Darken => "Darken",
            Self::Lighten => "Lighten",
            Self::Difference => "Difference",
            Self::Add => "Add",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }

    /// The position in [`Self::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }

    /// `B(Cb, Cs)` for one channel, both in 0..1.
    #[inline]
    #[must_use]
    pub fn channel(self, cb: f32, cs: f32) -> f32 {
        match self {
            Self::Normal => cs,
            Self::Multiply => cb * cs,
            Self::Screen => cb + cs - cb * cs,
            // Overlay = HardLight with the layers swapped.
            Self::Overlay => {
                if cb <= 0.5 {
                    2.0 * cb * cs
                } else {
                    1.0 - 2.0 * (1.0 - cb) * (1.0 - cs)
                }
            }
            Self::Darken => cb.min(cs),
            Self::Lighten => cb.max(cs),
            Self::Difference => (cb - cs).abs(),
            Self::Add => (cb + cs).min(1.0),
        }
    }
}

/// Straight RGBA8 to straight f32.
#[inline]
#[must_use]
pub fn to_f32(p: [u8; 4]) -> [f32; 4] {
    [
        f32::from(p[0]) * INV_255,
        f32::from(p[1]) * INV_255,
        f32::from(p[2]) * INV_255,
        f32::from(p[3]) * INV_255,
    ]
}

/// Straight RGBA8 to premultiplied f32.
#[inline]
#[must_use]
pub fn to_premul(p: [u8; 4]) -> [f32; 4] {
    let s = to_f32(p);
    [s[0] * s[3], s[1] * s[3], s[2] * s[3], s[3]]
}

/// A 0..1 value to a byte.
#[inline]
#[must_use]
pub fn unit_to_u8(v: f32) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Premultiplied f32 to straight RGBA8.
#[inline]
#[must_use]
pub fn premul_to_rgba8(p: [f32; 4]) -> [u8; 4] {
    let a = p[3].clamp(0.0, 1.0);
    if a <= 1.0 / 512.0 {
        return [0, 0, 0, 0];
    }
    let inv = 1.0 / a;
    [
        unit_to_u8(p[0] * inv),
        unit_to_u8(p[1] * inv),
        unit_to_u8(p[2] * inv),
        unit_to_u8(a),
    ]
}

/// Composite a straight `src` at `opacity` onto the premultiplied `dst`.
#[inline]
pub fn blend_into(dst: &mut [f32; 4], src: [f32; 4], opacity: f32, mode: BlendMode) {
    let a_s = src[3] * opacity;
    if a_s <= 0.0 {
        return;
    }
    let a_b = dst[3];
    let inv_s = 1.0 - a_s;
    if mode == BlendMode::Normal || a_b <= 0.0 {
        for c in 0..3 {
            dst[c] = src[c] * a_s + dst[c] * inv_s;
        }
    } else {
        for c in 0..3 {
            let cb = (dst[c] / a_b).clamp(0.0, 1.0);
            let mixed = mode.channel(cb, src[c]);
            dst[c] = src[c] * a_s * (1.0 - a_b) + dst[c] * inv_s + a_s * a_b * mixed;
        }
    }
    dst[3] = a_s + a_b * inv_s;
}

/// [`blend_into`] on straight RGBA8 pixels: `source` at `opacity` over
/// `backdrop`.
#[must_use]
pub fn blend_rgba8(backdrop: [u8; 4], source: [u8; 4], opacity: f32, mode: BlendMode) -> [u8; 4] {
    let mut acc = to_premul(backdrop);
    blend_into(&mut acc, to_f32(source), opacity, mode);
    premul_to_rgba8(acc)
}

/// Plain source-over of straight RGBA8 with an extra coverage factor
/// (painting, fills): the paint at `alpha` (0..1) over `base`.
#[inline]
#[must_use]
pub fn paint_over(base: [u8; 4], paint: [u8; 4], alpha: f32) -> [u8; 4] {
    if alpha <= 0.0 {
        return base;
    }
    blend_rgba8(base, paint, alpha.min(1.0), BlendMode::Normal)
}
