//! Small geometry types used by the layout solver and text shaping pipeline.
//!
//! Default font / text constants live in [`azul_css::defaults`].

use crate::geom::{LogicalPosition, LogicalSize};

/// Resolved top/right/bottom/left offsets in logical pixels (used for
/// margins, padding, and borders after CSS resolution).
#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ResolvedOffsets {
    pub top: f32,
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
}

impl ResolvedOffsets {
    #[must_use] pub const fn zero() -> Self {
        Self {
            top: 0.0,
            left: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }
    #[must_use]
    pub fn total_vertical(&self) -> f32 {
        self.top + self.bottom
    }
    #[must_use]
    pub fn total_horizontal(&self) -> f32 {
        self.left + self.right
    }
}

/// Index into a font's glyph table.
type GlyphIndex = u32;

/// A single positioned glyph with its index, screen position, and size.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd)]
pub struct GlyphInstance {
    pub index: GlyphIndex,
    pub point: LogicalPosition,
    pub size: LogicalSize,
}

impl GlyphInstance {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        self.point.scale_for_dpi(scale_factor);
        self.size.scale_for_dpi(scale_factor);
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    fn offsets(top: f32, left: f32, right: f32, bottom: f32) -> ResolvedOffsets {
        ResolvedOffsets {
            top,
            left,
            right,
            bottom,
        }
    }

    fn glyph(index: u32, x: f32, y: f32, w: f32, h: f32) -> GlyphInstance {
        GlyphInstance {
            index,
            point: LogicalPosition::new(x, y),
            size: LogicalSize::new(w, h),
        }
    }

    // --- ResolvedOffsets::zero (constructor) ---

    #[test]
    fn zero_is_all_zeroes_and_usable_in_const_context() {
        const Z: ResolvedOffsets = ResolvedOffsets::zero();
        assert_eq!(Z.top, 0.0);
        assert_eq!(Z.left, 0.0);
        assert_eq!(Z.right, 0.0);
        assert_eq!(Z.bottom, 0.0);
        // Positive zero, not -0.0: sign leaks through `f32::signum` in layout math.
        assert!(Z.top.is_sign_positive());
        assert!(Z.left.is_sign_positive());
        assert!(Z.right.is_sign_positive());
        assert!(Z.bottom.is_sign_positive());
    }

    #[test]
    fn zero_matches_default_and_is_neutral_for_totals() {
        assert_eq!(ResolvedOffsets::zero(), ResolvedOffsets::default());
        assert_eq!(ResolvedOffsets::zero().total_vertical(), 0.0);
        assert_eq!(ResolvedOffsets::zero().total_horizontal(), 0.0);
    }

    // --- ResolvedOffsets::total_vertical / total_horizontal (getters) ---

    #[test]
    fn totals_sum_only_their_own_axis() {
        let o = offsets(1.0, 20.0, 300.0, 4000.0);
        assert_eq!(o.total_vertical(), 4001.0); // top + bottom
        assert_eq!(o.total_horizontal(), 320.0); // left + right
        // Perturbing one axis must not move the other.
        let o2 = offsets(1.0, -20.0, -300.0, 4000.0);
        assert_eq!(o2.total_vertical(), o.total_vertical());
        assert_eq!(o2.total_horizontal(), -320.0);
    }

    #[test]
    fn totals_handle_negative_and_cancelling_offsets() {
        let o = offsets(-5.0, -2.5, 2.5, 5.0);
        assert_eq!(o.total_vertical(), 0.0);
        assert_eq!(o.total_horizontal(), 0.0);
    }

    #[test]
    fn totals_saturate_to_infinity_instead_of_wrapping() {
        let o = offsets(f32::MAX, f32::MAX, f32::MAX, f32::MAX);
        assert!(o.total_vertical().is_infinite() && o.total_vertical().is_sign_positive());
        assert!(o.total_horizontal().is_infinite() && o.total_horizontal().is_sign_positive());

        let o = offsets(f32::MIN, f32::MIN, f32::MIN, f32::MIN);
        assert!(o.total_vertical().is_infinite() && o.total_vertical().is_sign_negative());
        assert!(o.total_horizontal().is_infinite() && o.total_horizontal().is_sign_negative());
    }

    #[test]
    fn totals_of_opposing_infinities_are_nan_not_a_panic() {
        let o = offsets(f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        assert!(o.total_vertical().is_nan());
        assert!(o.total_horizontal().is_nan());
    }

    #[test]
    fn totals_propagate_nan() {
        let o = offsets(f32::NAN, 1.0, 2.0, 10.0);
        assert!(o.total_vertical().is_nan());
        assert_eq!(o.total_horizontal(), 3.0); // unaffected axis stays finite

        let o = offsets(1.0, f32::NAN, 2.0, 10.0);
        assert_eq!(o.total_vertical(), 11.0);
        assert!(o.total_horizontal().is_nan());
    }

    #[test]
    fn totals_on_subnormals_do_not_flush_to_a_wrong_value() {
        let o = offsets(f32::MIN_POSITIVE, f32::MIN_POSITIVE, 0.0, 0.0);
        assert_eq!(o.total_vertical(), f32::MIN_POSITIVE);
        assert_eq!(o.total_horizontal(), f32::MIN_POSITIVE);
    }

    #[test]
    fn totals_are_pure_getters() {
        let o = offsets(3.0, 7.0, 11.0, 13.0);
        let before = o;
        let _ = o.total_vertical();
        let _ = o.total_horizontal();
        assert_eq!(o, before);
        // Repeated calls are deterministic.
        let (v1, v2) = (o.total_vertical(), o.total_vertical());
        let (h1, h2) = (o.total_horizontal(), o.total_horizontal());
        assert_eq!(v1, v2);
        assert_eq!(h1, h2);
    }

    // --- GlyphInstance::scale_for_dpi (numeric) ---

    #[test]
    fn scale_for_dpi_by_one_is_identity_and_never_touches_the_glyph_index() {
        let mut g = glyph(u32::MAX, 1.5, -2.5, 3.5, 4.5);
        g.scale_for_dpi(1.0);
        assert_eq!(g.index, u32::MAX);
        assert_eq!(g.point.x, 1.5);
        assert_eq!(g.point.y, -2.5);
        assert_eq!(g.size.width, 3.5);
        assert_eq!(g.size.height, 4.5);
    }

    #[test]
    fn scale_for_dpi_by_zero_collapses_to_zero_and_preserves_sign() {
        let mut g = glyph(7, 10.0, -10.0, 20.0, -20.0);
        g.scale_for_dpi(0.0);
        assert_eq!(g.index, 7);
        assert_eq!(g.point.x, 0.0);
        assert_eq!(g.point.y, 0.0);
        assert!(g.point.x.is_sign_positive());
        assert!(g.point.y.is_sign_negative()); // -10.0 * 0.0 == -0.0
        assert_eq!(g.size.width, 0.0);
        assert_eq!(g.size.height, 0.0);
    }

    #[test]
    fn scale_for_dpi_negative_factor_mirrors_deterministically() {
        let mut g = glyph(1, 2.0, -4.0, 8.0, -16.0);
        g.scale_for_dpi(-2.0);
        assert_eq!(g.point.x, -4.0);
        assert_eq!(g.point.y, 8.0);
        assert_eq!(g.size.width, -16.0);
        assert_eq!(g.size.height, 32.0);
    }

    #[test]
    fn scale_for_dpi_round_trips_for_exact_binary_factors() {
        let original = glyph(42, 12.0, -6.5, 100.0, 0.25);
        let mut g = original;
        g.scale_for_dpi(4.0);
        g.scale_for_dpi(0.25);
        assert_eq!(g.index, original.index);
        assert_eq!(g.point.x, original.point.x);
        assert_eq!(g.point.y, original.point.y);
        assert_eq!(g.size.width, original.size.width);
        assert_eq!(g.size.height, original.size.height);
    }

    #[test]
    fn scale_for_dpi_overflows_to_infinity_rather_than_wrapping() {
        let mut g = glyph(0, f32::MAX, -f32::MAX, f32::MAX, f32::MAX);
        g.scale_for_dpi(2.0);
        assert!(g.point.x.is_infinite() && g.point.x.is_sign_positive());
        assert!(g.point.y.is_infinite() && g.point.y.is_sign_negative());
        assert!(g.size.width.is_infinite());
        assert!(g.size.height.is_infinite());
    }

    #[test]
    fn scale_for_dpi_underflows_to_zero_rather_than_panicking() {
        let mut g = glyph(0, f32::MIN_POSITIVE, f32::MIN_POSITIVE, f32::MIN_POSITIVE, 1.0);
        g.scale_for_dpi(f32::MIN_POSITIVE);
        assert_eq!(g.point.x, 0.0);
        assert_eq!(g.point.y, 0.0);
        assert_eq!(g.size.width, 0.0);
        assert_eq!(g.size.height, f32::MIN_POSITIVE);
    }

    #[test]
    fn scale_for_dpi_with_nan_factor_poisons_all_coordinates_without_panicking() {
        let mut g = glyph(3, 1.0, 2.0, 3.0, 4.0);
        g.scale_for_dpi(f32::NAN);
        assert_eq!(g.index, 3);
        assert!(g.point.x.is_nan());
        assert!(g.point.y.is_nan());
        assert!(g.size.width.is_nan());
        assert!(g.size.height.is_nan());
    }

    #[test]
    fn scale_for_dpi_with_infinite_factor_is_defined_at_zero_and_nonzero_coords() {
        // finite non-zero * inf == inf (sign-correct)
        let mut g = glyph(0, 1.0, -1.0, 2.0, -2.0);
        g.scale_for_dpi(f32::INFINITY);
        assert!(g.point.x.is_infinite() && g.point.x.is_sign_positive());
        assert!(g.point.y.is_infinite() && g.point.y.is_sign_negative());
        assert!(g.size.width.is_infinite() && g.size.width.is_sign_positive());
        assert!(g.size.height.is_infinite() && g.size.height.is_sign_negative());

        // 0 * inf == NaN, per IEEE-754 — must not panic, must not silently become 0.
        let mut g = glyph(0, 0.0, 0.0, 0.0, 0.0);
        g.scale_for_dpi(f32::INFINITY);
        assert!(g.point.x.is_nan());
        assert!(g.size.width.is_nan());

        // -inf flips signs.
        let mut g = glyph(0, 1.0, 1.0, 1.0, 1.0);
        g.scale_for_dpi(f32::NEG_INFINITY);
        assert!(g.point.x.is_infinite() && g.point.x.is_sign_negative());
        assert!(g.size.height.is_infinite() && g.size.height.is_sign_negative());
    }

    #[test]
    fn scale_for_dpi_on_default_glyph_is_a_no_op_for_finite_factors() {
        let mut g = GlyphInstance::default();
        g.scale_for_dpi(1_000_000.0);
        assert_eq!(g.index, 0);
        assert_eq!(g.point.x, 0.0);
        assert_eq!(g.point.y, 0.0);
        assert_eq!(g.size.width, 0.0);
        assert_eq!(g.size.height, 0.0);
    }

    #[test]
    fn glyph_equality_stays_reflexive_after_a_nan_scale() {
        // `LogicalPosition`/`LogicalSize` quantize NaN to a single sentinel, so
        // `GlyphInstance: Eq` must remain reflexive even with poisoned coords.
        let mut g = glyph(9, 1.0, 2.0, 3.0, 4.0);
        g.scale_for_dpi(f32::NAN);
        let same = g;
        assert_eq!(g, same);
        // A NaN glyph must not compare equal to the origin glyph.
        assert_ne!(g, glyph(9, 0.0, 0.0, 0.0, 0.0));
    }
}
