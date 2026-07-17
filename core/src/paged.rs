//! Paged media layout primitives.
//!
//! Provides the [`FragmentationContext`] that the layout solver threads through to
//! distinguish continuous (screen) from paged (print) media.
//!
//! For continuous media (screens), content flows into a single infinitely tall
//! container. For paged media (print), content is laid out on a continuous canvas
//! and afterwards sliced into fixed-size pages by the display-list slicer
//! (`paginate_display_list_with_slicer_and_breaks` in `azul_layout::solver3::display_list`).
//! This lets the layout engine make break decisions while respecting CSS properties
//! like `break-before`, `break-after`, and `break-inside`.
//!
//! Page *decoration* (headers, footers, margin boxes, counters) lives in
//! `azul_layout::solver3::pagination`.

use crate::geom::LogicalSize;

/// Selects how content is fragmented during layout.
///
/// This is the core abstraction for fragmentation support:
/// - Screen rendering: [`Continuous`](Self::Continuous) — a single infinite container.
/// - Print rendering: [`Paged`](Self::Paged) — a series of fixed-size page containers.
#[derive(Debug, Clone, Copy)]
pub enum FragmentationContext {
    /// Continuous media (screen): a single, infinitely tall container.
    ///
    /// Used for normal screen rendering where content can scroll indefinitely;
    /// breaks are never forced.
    Continuous {
        /// Width of the viewport.
        width: f32,
    },

    /// Paged media (print): fixed-size pages.
    ///
    /// Used for PDF generation and print preview. Content flows from one page to
    /// the next when a page is full.
    Paged {
        /// Size of each page.
        page_size: LogicalSize,
    },
}

impl FragmentationContext {
    /// Create a continuous fragmentation context for screen rendering.
    #[must_use] pub const fn new_continuous(width: f32) -> Self {
        Self::Continuous { width }
    }

    /// Create a paged fragmentation context for print rendering.
    #[must_use] pub const fn new_paged(page_size: LogicalSize) -> Self {
        Self::Paged { page_size }
    }

    /// Get the page content height (page height for paged media).
    ///
    /// For continuous media, returns `f32::MAX`.
    #[must_use] pub const fn page_content_height(&self) -> f32 {
        match self {
            Self::Continuous { .. } => f32::MAX,
            Self::Paged { page_size, .. } => page_size.height,
        }
    }

    /// Check if this is paged media.
    #[must_use] pub const fn is_paged(&self) -> bool {
        matches!(self, Self::Paged { .. })
    }
}

/// Page margins in points.
///
/// Canonical paged-media margin type (formerly defined in the now-removed
/// `crate::fragmentation` module). Re-exported from the crate root as
/// `azul_layout::PageMargins`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PageMargins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl PageMargins {
    #[must_use] pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    #[must_use] pub const fn uniform(margin: f32) -> Self {
        Self {
            top: margin,
            right: margin,
            bottom: margin,
            left: margin,
        }
    }

    #[must_use] pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    #[must_use] pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

#[cfg(test)]
mod autotest_generated {
    #![allow(clippy::float_cmp)]

    use super::*;

    /// Every hostile `f32` a caller can hand to these constructors.
    const HOSTILE_F32: [f32; 12] = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        f32::MAX,
        f32::MIN,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        f32::EPSILON,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];

    /// Bit-exact float compare: distinguishes `0.0` from `-0.0`, and treats any
    /// NaN as equal to any other NaN (so it can be used on the hostile list).
    fn same_f32(a: f32, b: f32) -> bool {
        if a.is_nan() && b.is_nan() {
            return true;
        }
        a.to_bits() == b.to_bits()
    }

    // --- FragmentationContext::new_continuous / new_paged (constructor) -------

    #[test]
    fn new_continuous_stores_width_bit_exactly_for_hostile_input() {
        for w in HOSTILE_F32 {
            let ctx = FragmentationContext::new_continuous(w);
            match ctx {
                FragmentationContext::Continuous { width } => {
                    assert!(same_f32(width, w), "width mangled: {w:?} -> {width:?}");
                }
                FragmentationContext::Paged { .. } => {
                    panic!("new_continuous produced a Paged variant for width {w:?}")
                }
            }
        }
    }

    #[test]
    fn new_paged_stores_page_size_bit_exactly_for_hostile_input() {
        for w in HOSTILE_F32 {
            for h in HOSTILE_F32 {
                let ctx = FragmentationContext::new_paged(LogicalSize::new(w, h));
                match ctx {
                    FragmentationContext::Paged { page_size } => {
                        assert!(same_f32(page_size.width, w));
                        assert!(same_f32(page_size.height, h));
                    }
                    FragmentationContext::Continuous { .. } => {
                        panic!("new_paged produced a Continuous variant for {w:?}x{h:?}")
                    }
                }
            }
        }
    }

    /// The `const fn` marker is part of the public contract: a caller may put
    /// these in a `const` item. If constness regresses, this stops compiling.
    #[test]
    fn constructors_and_accessors_are_usable_in_const_context() {
        const CONT: FragmentationContext = FragmentationContext::new_continuous(1024.0);
        const A4: FragmentationContext =
            FragmentationContext::new_paged(LogicalSize::new(595.0, 842.0));
        const CONT_H: f32 = CONT.page_content_height();
        const A4_H: f32 = A4.page_content_height();
        const CONT_PAGED: bool = CONT.is_paged();
        const A4_PAGED: bool = A4.is_paged();
        const UNIFORM: PageMargins = PageMargins::uniform(10.0);
        const EXPLICIT: PageMargins = PageMargins::new(1.0, 2.0, 3.0, 4.0);

        assert_eq!(CONT_H, f32::MAX);
        assert_eq!(A4_H, 842.0);
        const _: () = assert!(!CONT_PAGED && A4_PAGED);
        assert_eq!(UNIFORM.top, 10.0);
        assert_eq!(EXPLICIT.left, 4.0);
    }

    // --- FragmentationContext::page_content_height (getter) ------------------

    #[test]
    fn page_content_height_is_f32_max_for_every_continuous_width() {
        // Continuous ignores `width` entirely — even NaN/inf must not leak out.
        for w in HOSTILE_F32 {
            let h = FragmentationContext::new_continuous(w).page_content_height();
            assert!(
                same_f32(h, f32::MAX),
                "continuous width {w:?} leaked into page_content_height: {h:?}"
            );
        }
    }

    #[test]
    fn page_content_height_returns_paged_height_verbatim_and_ignores_width() {
        for h in HOSTILE_F32 {
            // Width must never contaminate the result, however hostile it is.
            for w in HOSTILE_F32 {
                let got = FragmentationContext::new_paged(LogicalSize::new(w, h))
                    .page_content_height();
                assert!(
                    same_f32(got, h),
                    "page {w:?}x{h:?} -> page_content_height {got:?}, expected {h:?}"
                );
            }
        }
    }

    #[test]
    fn page_content_height_does_not_saturate_or_clamp_degenerate_pages() {
        // A zero/negative page height is nonsense for print, but the getter is a
        // plain accessor: it must report what it was given, not silently repair it.
        let zero = FragmentationContext::new_paged(LogicalSize::zero());
        assert_eq!(zero.page_content_height(), 0.0);

        let negative = FragmentationContext::new_paged(LogicalSize::new(595.0, -842.0));
        assert_eq!(negative.page_content_height(), -842.0);

        let nan = FragmentationContext::new_paged(LogicalSize::new(595.0, f32::NAN));
        assert!(nan.page_content_height().is_nan());
    }

    /// `f32::MAX` is the sentinel for "infinitely tall". A paged context whose page
    /// is exactly `f32::MAX` tall is therefore indistinguishable from a continuous
    /// one *by height alone* — `is_paged()` is the only safe discriminator.
    #[test]
    fn f32_max_tall_page_collides_with_continuous_sentinel_but_is_paged_disambiguates() {
        let continuous = FragmentationContext::new_continuous(595.0);
        let max_page =
            FragmentationContext::new_paged(LogicalSize::new(595.0, f32::MAX));

        assert_eq!(
            continuous.page_content_height(),
            max_page.page_content_height()
        );
        assert!(!continuous.is_paged());
        assert!(max_page.is_paged());
    }

    // --- FragmentationContext::is_paged (predicate) --------------------------

    #[test]
    fn is_paged_is_true_only_for_paged_regardless_of_hostile_fields() {
        for v in HOSTILE_F32 {
            assert!(
                !FragmentationContext::new_continuous(v).is_paged(),
                "continuous({v:?}) reported as paged"
            );
            assert!(
                FragmentationContext::new_paged(LogicalSize::new(v, v)).is_paged(),
                "paged({v:?}x{v:?}) reported as continuous"
            );
        }
        // Fully degenerate page: still paged. The predicate inspects the variant,
        // never the payload.
        assert!(FragmentationContext::new_paged(LogicalSize::zero()).is_paged());
    }

    #[test]
    fn is_paged_is_deterministic_across_repeated_calls_and_copies() {
        let ctx = FragmentationContext::new_paged(LogicalSize::new(f32::NAN, f32::NAN));
        let copied = ctx; // Copy
        assert!(ctx.is_paged());
        assert!(ctx.is_paged());
        assert!(copied.is_paged());
    }

    // --- PageMargins::new (constructor) --------------------------------------

    #[test]
    fn new_assigns_each_argument_to_its_own_field_no_transposition() {
        // Distinct values in every slot: catches any top/right/bottom/left swap.
        let m = PageMargins::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(m.top, 1.0);
        assert_eq!(m.right, 2.0);
        assert_eq!(m.bottom, 3.0);
        assert_eq!(m.left, 4.0);
    }

    #[test]
    fn new_stores_hostile_floats_bit_exactly() {
        for v in HOSTILE_F32 {
            // Rotate the hostile value through each slot; the other three stay sane
            // so a misassignment shows up as a mismatch rather than cancelling out.
            let m = PageMargins::new(v, 2.0, 3.0, 4.0);
            assert!(same_f32(m.top, v));
            assert_eq!(m.right, 2.0);

            let m = PageMargins::new(1.0, v, 3.0, 4.0);
            assert!(same_f32(m.right, v));
            assert_eq!(m.bottom, 3.0);

            let m = PageMargins::new(1.0, 2.0, v, 4.0);
            assert!(same_f32(m.bottom, v));
            assert_eq!(m.left, 4.0);

            let m = PageMargins::new(1.0, 2.0, 3.0, v);
            assert!(same_f32(m.left, v));
            assert_eq!(m.top, 1.0);
        }
    }

    #[test]
    fn default_margins_are_positive_zero_and_sum_to_zero() {
        let d = PageMargins::default();
        for f in [d.top, d.right, d.bottom, d.left] {
            assert!(same_f32(f, 0.0), "default field is not +0.0: {f:?}");
        }
        assert_eq!(d.horizontal(), 0.0);
        assert_eq!(d.vertical(), 0.0);
    }

    // --- PageMargins::uniform (numeric) --------------------------------------

    #[test]
    fn uniform_zero_and_negative_zero_preserve_sign() {
        let z = PageMargins::uniform(0.0);
        assert!(same_f32(z.top, 0.0) && same_f32(z.left, 0.0));
        assert_eq!(z.horizontal(), 0.0);
        assert_eq!(z.vertical(), 0.0);

        // -0.0 + -0.0 == -0.0, so the sign must survive all the way through.
        let nz = PageMargins::uniform(-0.0);
        assert!(same_f32(nz.top, -0.0), "uniform(-0.0) lost the sign bit");
        assert!(same_f32(nz.horizontal(), -0.0));
        assert!(same_f32(nz.vertical(), -0.0));
    }

    #[test]
    fn uniform_replicates_hostile_value_into_all_four_fields() {
        for v in HOSTILE_F32 {
            let m = PageMargins::uniform(v);
            assert!(same_f32(m.top, v), "top != {v:?}");
            assert!(same_f32(m.right, v), "right != {v:?}");
            assert!(same_f32(m.bottom, v), "bottom != {v:?}");
            assert!(same_f32(m.left, v), "left != {v:?}");
        }
    }

    #[test]
    fn uniform_negative_margins_are_kept_not_clamped() {
        // Negative margins are legal CSS (`margin: -10pt`); nothing here may clamp.
        let m = PageMargins::uniform(-10.0);
        assert_eq!(m.top, -10.0);
        assert_eq!(m.horizontal(), -20.0);
        assert_eq!(m.vertical(), -20.0);
    }

    #[test]
    fn uniform_min_max_do_not_panic_and_overflow_to_infinity_not_wrap() {
        // f32::MAX + f32::MAX overflows. IEEE-754 says +inf; it must not panic,
        // wrap to a negative, or saturate back to f32::MAX.
        let hi = PageMargins::uniform(f32::MAX);
        assert_eq!(hi.horizontal(), f32::INFINITY);
        assert_eq!(hi.vertical(), f32::INFINITY);

        let lo = PageMargins::uniform(f32::MIN);
        assert_eq!(lo.horizontal(), f32::NEG_INFINITY);
        assert_eq!(lo.vertical(), f32::NEG_INFINITY);
    }

    #[test]
    fn uniform_exactly_at_the_overflow_boundary_stays_finite() {
        // MAX/2 is exactly representable, so doubling it must land on MAX exactly
        // — the last finite step before the overflow tested above.
        let edge = PageMargins::uniform(f32::MAX / 2.0);
        assert_eq!(edge.horizontal(), f32::MAX);
        assert!(edge.horizontal().is_finite());
    }

    #[test]
    fn uniform_smallest_normal_doubles_exactly_without_flushing_to_zero() {
        let tiny = PageMargins::uniform(f32::MIN_POSITIVE);
        assert_eq!(tiny.horizontal(), f32::MIN_POSITIVE * 2.0);
        assert!(tiny.horizontal() > 0.0, "tiny margin flushed to zero");
    }

    #[test]
    fn uniform_nan_and_inf_propagate_without_panicking() {
        let nan = PageMargins::uniform(f32::NAN);
        assert!(nan.horizontal().is_nan());
        assert!(nan.vertical().is_nan());

        let inf = PageMargins::uniform(f32::INFINITY);
        assert_eq!(inf.horizontal(), f32::INFINITY);
        assert_eq!(inf.vertical(), f32::INFINITY);

        let neg_inf = PageMargins::uniform(f32::NEG_INFINITY);
        assert_eq!(neg_inf.horizontal(), f32::NEG_INFINITY);
        assert_eq!(neg_inf.vertical(), f32::NEG_INFINITY);
    }

    #[test]
    fn uniform_agrees_with_new_given_the_same_value() {
        for v in HOSTILE_F32 {
            let u = PageMargins::uniform(v);
            let n = PageMargins::new(v, v, v, v);
            assert!(same_f32(u.top, n.top));
            assert!(same_f32(u.right, n.right));
            assert!(same_f32(u.bottom, n.bottom));
            assert!(same_f32(u.left, n.left));
            assert!(same_f32(u.horizontal(), n.horizontal()));
            assert!(same_f32(u.vertical(), n.vertical()));
        }
    }

    // --- PageMargins::horizontal / vertical (getters) ------------------------

    #[test]
    fn horizontal_sums_left_and_right_vertical_sums_top_and_bottom() {
        // new(top, right, bottom, left) = (1, 2, 3, 4)
        //   horizontal = left + right   = 4 + 2 = 6
        //   vertical   = top  + bottom  = 1 + 3 = 4
        // Distinct values, so picking the wrong pair cannot produce these sums.
        let m = PageMargins::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(m.horizontal(), 6.0);
        assert_eq!(m.vertical(), 4.0);
    }

    #[test]
    fn horizontal_and_vertical_are_axis_independent() {
        // Loading one axis must not move the other.
        let h_only = PageMargins::new(0.0, 7.0, 0.0, 11.0);
        assert_eq!(h_only.horizontal(), 18.0);
        assert_eq!(h_only.vertical(), 0.0);

        let v_only = PageMargins::new(7.0, 0.0, 11.0, 0.0);
        assert_eq!(v_only.vertical(), 18.0);
        assert_eq!(v_only.horizontal(), 0.0);
    }

    #[test]
    fn opposing_infinities_yield_nan_not_a_panic() {
        // +inf + -inf is NaN by IEEE-754. The getter must return it, not trap.
        let m = PageMargins::new(f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        assert!(m.vertical().is_nan(), "inf + -inf should be NaN");
        assert!(m.horizontal().is_nan(), "-inf + inf should be NaN");
    }

    #[test]
    fn opposing_extremes_cancel_to_zero_rather_than_overflowing() {
        // MAX + MIN == MAX + (-MAX) == 0.0, exactly. No overflow, no panic.
        let m = PageMargins::new(f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        assert_eq!(m.vertical(), 0.0);
        assert_eq!(m.horizontal(), 0.0);
    }

    #[test]
    fn getters_absorb_a_tiny_operand_next_to_a_huge_one_without_error() {
        // Classic float-precision trap: 1e30 + 1.0 == 1e30. Documented, not a bug —
        // pin it so nobody "fixes" the sum into something lossier.
        // horizontal() = left + right, vertical() = top + bottom -- so each axis has
        // to MIX magnitudes for absorption to happen at all. (top, right, bottom, left)
        let m = PageMargins::new(1.0e30, 1.0, 1.0, 1.0e30);
        assert_eq!(m.horizontal(), 1.0e30);
        assert_eq!(m.vertical(), 1.0e30);
    }

    #[test]
    fn getters_do_not_panic_on_any_hostile_field_combination() {
        for a in HOSTILE_F32 {
            for b in HOSTILE_F32 {
                // new(top=a, right=b, bottom=b, left=a): both axes sum the same pair,
                // so horizontal() and vertical() must agree bit-for-bit for every one
                // of the 144 hostile combinations — and neither may panic.
                let m = PageMargins::new(a, b, b, a);
                assert!(
                    same_f32(m.horizontal(), m.vertical()),
                    "axes disagree for {a:?}/{b:?}: h={:?} v={:?}",
                    m.horizontal(),
                    m.vertical()
                );
            }
        }
    }

    #[test]
    fn getters_are_pure_and_do_not_mutate_the_receiver() {
        let m = PageMargins::new(1.0, 2.0, 3.0, 4.0);
        let first_h = m.horizontal();
        let first_v = m.vertical();
        // Repeat calls: same answer, fields untouched.
        assert_eq!(m.horizontal(), first_h);
        assert_eq!(m.vertical(), first_v);
        assert_eq!(m.top, 1.0);
        assert_eq!(m.right, 2.0);
        assert_eq!(m.bottom, 3.0);
        assert_eq!(m.left, 4.0);
    }

    #[test]
    fn page_margins_copy_does_not_alias_the_original() {
        let original = PageMargins::uniform(5.0);
        let mut copy = original; // Copy, not a move
        copy.top = 99.0;
        assert_eq!(original.top, 5.0, "mutating a copy wrote through to the original");
        assert_eq!(original.vertical(), 10.0);
        assert_eq!(copy.vertical(), 104.0);
    }

    // --- round-trip ----------------------------------------------------------

    #[test]
    fn round_trip_new_fields_reconstruct_an_identical_margin() {
        for v in HOSTILE_F32 {
            let a = PageMargins::new(v, 1.0, 2.0, 3.0);
            let b = PageMargins::new(a.top, a.right, a.bottom, a.left);
            assert!(same_f32(a.top, b.top));
            assert!(same_f32(a.right, b.right));
            assert!(same_f32(a.bottom, b.bottom));
            assert!(same_f32(a.left, b.left));
        }
    }

    #[test]
    fn round_trip_paged_context_returns_the_height_it_was_built_from() {
        for h in [0.0_f32, 1.0, 842.0, -842.0, f32::MAX, f32::MIN, f32::MIN_POSITIVE] {
            let ctx = FragmentationContext::new_paged(LogicalSize::new(595.0, h));
            assert!(same_f32(ctx.page_content_height(), h));
            assert!(ctx.is_paged());
        }
    }
}
