//! Scrollbar geometry computation — single source of truth for the layout solver.
//!
//! Provides [`ScrollbarRequirements`] (whether scrollbars are needed and how much
//! space they reserve) and [`ScrollbarGeometry`] (track, thumb, and button rects).
//!
//! The main entry point is [`compute_scrollbar_geometry`], whose output is consumed by:
//! - Display list painting (`paint_scrollbars`)
//! - GPU transform updates (`update_scrollbar_transforms`)
//! - Hit-testing (`hit_test_component`)
//! - Drag delta conversion (`handle_scrollbar_drag`)

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::dom::ScrollbarOrientation;

/// Information about scrollbar requirements and dimensions
// +spec:overflow:55c244 - scrollbar appearance, size, and edge placement are UA-defined
#[derive(Copy, Debug, Clone, Default)]
#[repr(C)]
pub struct ScrollbarRequirements {
    pub needs_horizontal: bool,
    pub needs_vertical: bool,
    /// Layout-reserved width for a vertical scrollbar (0.0 for overlay)
    pub scrollbar_width: f32,
    /// Layout-reserved height for a horizontal scrollbar (0.0 for overlay)
    pub scrollbar_height: f32,
    /// Visual rendering width of the scrollbar in CSS pixels (e.g. 8.0 for thin).
    /// Non-zero even for overlay scrollbars. Used by GPU state for thumb positioning.
    pub visual_width_px: f32,
}

impl ScrollbarRequirements {
    /// Checks if the presence of scrollbars reduces the available inner size,
    /// which would necessitate a reflow of the content.
    #[must_use] pub fn needs_reflow(&self) -> bool {
        self.scrollbar_width > 0.0 || self.scrollbar_height > 0.0
    }

    // +spec:box-model:20c3c8 - scrollbar space reserved between inner border edge and outer padding edge
    // +spec:box-model:32cd53 - scrollbar space subtracted from containing block dimensions
    // +spec:overflow:30a49c - scrollbar space subtracted from content area
    /// Takes a size (representing a content-box) and returns a new size
    /// reduced by the dimensions of any active scrollbars.
    #[must_use] pub fn shrink_size(&self, size: LogicalSize) -> LogicalSize {
        LogicalSize {
            width: (size.width - self.scrollbar_width).max(0.0),
            height: (size.height - self.scrollbar_height).max(0.0),
        }
    }
}

/// Single source of truth for scrollbar geometry.
///
/// Computed once by [`compute_scrollbar_geometry`], then used by:
/// - Display list painting (`paint_scrollbars`)
/// - GPU transform updates (`update_scrollbar_transforms`)
/// - Hit-testing (`hit_test_component`)
/// - Drag delta conversion (`handle_scrollbar_drag`)
#[derive(Debug, Clone, Copy)]
pub struct ScrollbarGeometry {
    /// Orientation (vertical or horizontal)
    pub orientation: ScrollbarOrientation,
    /// The full track rect in the container's coordinate space
    pub track_rect: LogicalRect,
    /// Button size (square: width = height = `scrollbar_width_px`)
    pub button_size: f32,
    /// Usable track length after subtracting buttons and corner
    /// = `track_total` - 2*`button_size`
    pub usable_track_length: f32,
    /// The thumb length (min-clamped to 2*`width_px`)
    pub thumb_length: f32,
    /// Thumb size as ratio of viewport / content (0.0–1.0)
    pub thumb_size_ratio: f32,
    /// Scroll ratio (0.0 at top/left, 1.0 at bottom/right)
    pub scroll_ratio: f32,
    /// Thumb offset in pixels from the start of the usable track region
    pub thumb_offset: f32,
    /// Max scroll distance in content pixels
    pub max_scroll: f32,
    /// CSS-specified scrollbar thickness (width for vertical, height for horizontal)
    pub width_px: f32,
}

impl Default for ScrollbarGeometry {
    fn default() -> Self {
        Self {
            orientation: ScrollbarOrientation::Vertical,
            track_rect: LogicalRect::zero(),
            button_size: 0.0,
            usable_track_length: 0.0,
            thumb_length: 0.0,
            thumb_size_ratio: 0.0,
            scroll_ratio: 0.0,
            thumb_offset: 0.0,
            max_scroll: 0.0,
            width_px: 0.0,
        }
    }
}

/// Compute scrollbar geometry for one axis.
///
/// This is the **single source of truth** for all scrollbar calculations.
/// All consumers (display list painting, GPU transforms, hit-testing, drag)
/// must use this function to ensure consistent geometry.
///
/// # Parameters
/// - `orientation`: Vertical or horizontal scrollbar
/// - `inner_rect`: The padding-box (border-box minus borders) of the scroll container,
///   in the container's coordinate space (absolute window coordinates)
/// - `content_size`: Total content size (from `get_content_size()` or `virtual_scroll_size`)
/// - `scroll_offset`: Current scroll offset (y for vertical, x for horizontal; positive = scrolled)
/// - `scrollbar_width_px`: CSS-resolved scrollbar thickness in pixels
/// - `has_other_scrollbar`: Whether the perpendicular scrollbar is also visible
///   (reduces track length by one `scrollbar_width_px` for the corner)
#[must_use] pub fn compute_scrollbar_geometry(
    orientation: ScrollbarOrientation,
    inner_rect: LogicalRect,
    content_size: LogicalSize,
    scroll_offset: f32,
    scrollbar_width_px: f32,
    has_other_scrollbar: bool,
) -> ScrollbarGeometry {
    // For macOS-style overlay scrollbars, callers should pass button_size=0.
    // For legacy scrollbars with arrow buttons, button_size=scrollbar_width_px.
    compute_scrollbar_geometry_with_button_size(
        orientation,
        inner_rect,
        content_size,
        scroll_offset,
        scrollbar_width_px,
        has_other_scrollbar,
        scrollbar_width_px, // default: reserve button space
    )
}

/// Like [`compute_scrollbar_geometry`] but allows overriding the button size.
/// Pass `button_size = 0.0` for macOS-style overlay scrollbars (no arrow buttons).
#[must_use] pub fn compute_scrollbar_geometry_with_button_size(
    orientation: ScrollbarOrientation,
    inner_rect: LogicalRect,
    content_size: LogicalSize,
    scroll_offset: f32,
    scrollbar_width_px: f32,
    has_other_scrollbar: bool,
    button_size: f32,
) -> ScrollbarGeometry {
    let (track_total, viewport_length, content_length, track_rect) = match orientation {
        ScrollbarOrientation::Vertical => {
            let track_total = if has_other_scrollbar {
                inner_rect.size.height - scrollbar_width_px
            } else {
                inner_rect.size.height
            };
            let track_rect = LogicalRect {
                origin: LogicalPosition::new(
                    inner_rect.origin.x + inner_rect.size.width - scrollbar_width_px,
                    inner_rect.origin.y,
                ),
                size: LogicalSize::new(scrollbar_width_px, track_total),
            };
            (track_total, inner_rect.size.height, content_size.height, track_rect)
        }
        ScrollbarOrientation::Horizontal => {
            let track_total = if has_other_scrollbar {
                inner_rect.size.width - scrollbar_width_px
            } else {
                inner_rect.size.width
            };
            let track_rect = LogicalRect {
                origin: LogicalPosition::new(
                    inner_rect.origin.x,
                    inner_rect.origin.y + inner_rect.size.height - scrollbar_width_px,
                ),
                size: LogicalSize::new(track_total, scrollbar_width_px),
            };
            (track_total, inner_rect.size.width, content_size.width, track_rect)
        }
    };

    compute_thumb_geometry(
        orientation,
        track_rect,
        track_total,
        viewport_length,
        content_length,
        button_size,
        scrollbar_width_px,
        scroll_offset,
    )
}

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
fn compute_thumb_geometry(
    orientation: ScrollbarOrientation,
    track_rect: LogicalRect,
    track_total: f32,
    viewport_length: f32,
    content_length: f32,
    button_size: f32,
    scrollbar_width_px: f32,
    scroll_offset: f32,
) -> ScrollbarGeometry {
    let usable_track_length = (track_total - 2.0 * button_size).max(0.0);

    let thumb_size_ratio = if content_length > 0.0 {
        (viewport_length / content_length).min(1.0)
    } else {
        1.0
    };
    let thumb_length = (usable_track_length * thumb_size_ratio)
        .max(scrollbar_width_px * 2.0)
        .min(usable_track_length);

    let max_scroll = (content_length - viewport_length).max(0.0);
    let scroll_ratio = if max_scroll > 0.0 {
        (scroll_offset.abs() / max_scroll).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let thumb_offset = (usable_track_length - thumb_length) * scroll_ratio;

    ScrollbarGeometry {
        orientation,
        track_rect,
        button_size,
        usable_track_length,
        thumb_length,
        thumb_size_ratio,
        scroll_ratio,
        thumb_offset,
        max_scroll,
        width_px: scrollbar_width_px,
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    fn reqs(width: f32, height: f32) -> ScrollbarRequirements {
        ScrollbarRequirements {
            needs_horizontal: height > 0.0,
            needs_vertical: width > 0.0,
            scrollbar_width: width,
            scrollbar_height: height,
            visual_width_px: 15.0,
        }
    }

    #[track_caller]
    fn approx(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-3,
            "expected {expected}, got {actual}"
        );
    }

    // ---------------------------------------------------------------------
    // ScrollbarRequirements::needs_reflow  (getter / predicate)
    // ---------------------------------------------------------------------

    #[test]
    fn needs_reflow_default_instance_is_false() {
        assert!(!ScrollbarRequirements::default().needs_reflow());
    }

    #[test]
    fn needs_reflow_true_when_either_axis_reserves_space() {
        assert!(reqs(15.0, 0.0).needs_reflow());
        assert!(reqs(0.0, 15.0).needs_reflow());
        assert!(reqs(15.0, 15.0).needs_reflow());
        assert!(!reqs(0.0, 0.0).needs_reflow());
    }

    #[test]
    fn needs_reflow_is_false_for_overlay_scrollbars() {
        // Overlay scrollbars are visible (visual_width_px > 0) but reserve no
        // layout space, so they must never trigger a reflow.
        let overlay = ScrollbarRequirements {
            needs_horizontal: true,
            needs_vertical: true,
            scrollbar_width: 0.0,
            scrollbar_height: 0.0,
            visual_width_px: 12.0,
        };
        assert!(!overlay.needs_reflow());
    }

    #[test]
    fn needs_reflow_does_not_panic_on_extreme_values() {
        // NaN compares false against every bound, so it reads as "no space reserved".
        assert!(!reqs(f32::NAN, f32::NAN).needs_reflow());
        // Negative / -inf reservations are nonsense but must not report a reflow.
        assert!(!reqs(-1.0, -1.0).needs_reflow());
        assert!(!reqs(f32::NEG_INFINITY, f32::NEG_INFINITY).needs_reflow());
        assert!(!reqs(f32::MIN, f32::MIN).needs_reflow());
        assert!(!reqs(-0.0, -0.0).needs_reflow());
        // Anything strictly positive, however small or large, does.
        assert!(reqs(f32::MIN_POSITIVE, 0.0).needs_reflow());
        assert!(reqs(0.0, f32::MAX).needs_reflow());
        assert!(reqs(f32::INFINITY, 0.0).needs_reflow());
    }

    // ---------------------------------------------------------------------
    // ScrollbarRequirements::shrink_size  (numeric)
    // ---------------------------------------------------------------------

    #[test]
    fn shrink_size_zero_reservation_is_the_identity() {
        let out = reqs(0.0, 0.0).shrink_size(LogicalSize::new(800.0, 600.0));
        approx(out.width, 800.0);
        approx(out.height, 600.0);
    }

    #[test]
    fn shrink_size_subtracts_each_axis_independently() {
        let out = reqs(15.0, 0.0).shrink_size(LogicalSize::new(100.0, 200.0));
        approx(out.width, 85.0);
        approx(out.height, 200.0);

        let out = reqs(0.0, 15.0).shrink_size(LogicalSize::new(100.0, 200.0));
        approx(out.width, 100.0);
        approx(out.height, 185.0);
    }

    #[test]
    fn shrink_size_clamps_at_zero_and_never_returns_negative() {
        let out = reqs(100.0, 100.0).shrink_size(LogicalSize::new(10.0, 10.0));
        approx(out.width, 0.0);
        approx(out.height, 0.0);
        assert!(out.width >= 0.0 && out.height >= 0.0);

        // Zero-sized content box stays at zero.
        let out = reqs(15.0, 15.0).shrink_size(LogicalSize::new(0.0, 0.0));
        approx(out.width, 0.0);
        approx(out.height, 0.0);
    }

    #[test]
    fn shrink_size_at_float_min_max_does_not_panic() {
        // MAX - MAX == 0.0
        let out = reqs(f32::MAX, f32::MAX).shrink_size(LogicalSize::new(f32::MAX, f32::MAX));
        approx(out.width, 0.0);
        approx(out.height, 0.0);

        // MAX with nothing reserved survives unchanged (no overflow).
        let out = reqs(0.0, 0.0).shrink_size(LogicalSize::new(f32::MAX, f32::MAX));
        assert!(out.width.is_finite() && out.height.is_finite());
        approx(out.width / f32::MAX, 1.0);

        // A negative (MIN) input size is clamped up to zero.
        let out = reqs(0.0, 0.0).shrink_size(LogicalSize::new(f32::MIN, f32::MIN));
        approx(out.width, 0.0);
        approx(out.height, 0.0);
    }

    #[test]
    fn shrink_size_with_nan_or_infinite_inputs_is_defined() {
        // NaN propagates into the subtraction, but f32::max(NaN, 0.0) == 0.0,
        // so the result is a defined (zero) size rather than a NaN size.
        let out = reqs(f32::NAN, f32::NAN).shrink_size(LogicalSize::new(100.0, 100.0));
        approx(out.width, 0.0);
        approx(out.height, 0.0);

        let out = reqs(0.0, 0.0).shrink_size(LogicalSize::new(f32::NAN, f32::NAN));
        approx(out.width, 0.0);
        approx(out.height, 0.0);

        // inf - inf == NaN -> clamped to 0.0
        let out =
            reqs(f32::INFINITY, f32::INFINITY).shrink_size(LogicalSize::new(f32::INFINITY, f32::INFINITY));
        approx(out.width, 0.0);
        approx(out.height, 0.0);

        // An infinite content box with a finite reservation stays infinite.
        let out = reqs(15.0, 15.0).shrink_size(LogicalSize::new(f32::INFINITY, f32::INFINITY));
        assert!(out.width.is_infinite() && out.width.is_sign_positive());
        assert!(out.height.is_infinite() && out.height.is_sign_positive());
    }

    #[test]
    fn shrink_size_with_negative_reservation_grows_the_box() {
        // Characterization: shrink_size does not clamp the *reservation*, only the
        // result. A negative reserved width therefore enlarges the content box.
        // Negative reservations are not producible by the CSS resolver today; this
        // pins the behaviour so a future clamp is a deliberate, visible change.
        let out = reqs(-10.0, -10.0).shrink_size(LogicalSize::new(100.0, 100.0));
        approx(out.width, 110.0);
        approx(out.height, 110.0);
    }

    #[test]
    fn shrink_size_is_identity_exactly_when_no_reflow_is_needed() {
        // Property: for the non-negative reservations the solver can actually
        // produce, !needs_reflow() <=> shrink_size() leaves the size untouched.
        for w in [0.0_f32, 1.0, 8.0, 15.0, 100.0] {
            for h in [0.0_f32, 1.0, 8.0, 15.0, 100.0] {
                let r = reqs(w, h);
                let size = LogicalSize::new(500.0, 500.0);
                let out = r.shrink_size(size);
                let unchanged = (out.width - size.width).abs() < f32::EPSILON
                    && (out.height - size.height).abs() < f32::EPSILON;
                assert_eq!(!r.needs_reflow(), unchanged, "w={w} h={h}");
            }
        }
    }

    // ---------------------------------------------------------------------
    // compute_scrollbar_geometry  (numeric)
    // ---------------------------------------------------------------------

    #[test]
    fn vertical_geometry_places_the_track_on_the_right_inner_edge() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(10.0, 20.0, 100.0, 200.0),
            LogicalSize::new(100.0, 400.0),
            0.0,
            15.0,
            false,
        );
        assert_eq!(g.orientation, ScrollbarOrientation::Vertical);
        approx(g.track_rect.origin.x, 10.0 + 100.0 - 15.0);
        approx(g.track_rect.origin.y, 20.0);
        approx(g.track_rect.size.width, 15.0);
        approx(g.track_rect.size.height, 200.0);
        approx(g.button_size, 15.0);
        approx(g.usable_track_length, 200.0 - 2.0 * 15.0);
        approx(g.thumb_size_ratio, 0.5);
        approx(g.thumb_length, 85.0);
        approx(g.max_scroll, 200.0);
        approx(g.scroll_ratio, 0.0);
        approx(g.thumb_offset, 0.0);
        approx(g.width_px, 15.0);
    }

    #[test]
    fn horizontal_geometry_places_the_track_on_the_bottom_inner_edge() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Horizontal,
            rect(10.0, 20.0, 100.0, 200.0),
            LogicalSize::new(300.0, 200.0),
            100.0,
            15.0,
            false,
        );
        assert_eq!(g.orientation, ScrollbarOrientation::Horizontal);
        approx(g.track_rect.origin.x, 10.0);
        approx(g.track_rect.origin.y, 20.0 + 200.0 - 15.0);
        approx(g.track_rect.size.width, 100.0);
        approx(g.track_rect.size.height, 15.0);
        approx(g.usable_track_length, 70.0);
        approx(g.thumb_size_ratio, 1.0 / 3.0);
        // 70 * 0.333 = 23.3 -> lifted to the 2*width minimum (30)
        approx(g.thumb_length, 30.0);
        approx(g.max_scroll, 200.0);
        approx(g.scroll_ratio, 0.5);
        approx(g.thumb_offset, (70.0 - 30.0) * 0.5);
    }

    #[test]
    fn the_other_scrollbar_steals_exactly_one_width_from_the_track() {
        let without = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, 400.0),
            0.0,
            15.0,
            false,
        );
        let with = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, 400.0),
            0.0,
            15.0,
            true,
        );
        approx(without.track_rect.size.height - with.track_rect.size.height, 15.0);
        approx(without.usable_track_length - with.usable_track_length, 15.0);
        approx(with.usable_track_length, 200.0 - 15.0 - 30.0);
    }

    #[test]
    fn compute_scrollbar_geometry_defaults_to_button_size_equal_to_width() {
        for orientation in [ScrollbarOrientation::Vertical, ScrollbarOrientation::Horizontal] {
            let a = compute_scrollbar_geometry(
                orientation,
                rect(5.0, 7.0, 120.0, 240.0),
                LogicalSize::new(500.0, 900.0),
                33.0,
                17.0,
                true,
            );
            let b = compute_scrollbar_geometry_with_button_size(
                orientation,
                rect(5.0, 7.0, 120.0, 240.0),
                LogicalSize::new(500.0, 900.0),
                33.0,
                17.0,
                true,
                17.0,
            );
            approx(a.button_size, b.button_size);
            approx(a.usable_track_length, b.usable_track_length);
            approx(a.thumb_length, b.thumb_length);
            approx(a.thumb_offset, b.thumb_offset);
            approx(a.max_scroll, b.max_scroll);
        }
    }

    #[test]
    fn everything_zero_yields_an_all_zero_geometry() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            LogicalRect::zero(),
            LogicalSize::new(0.0, 0.0),
            0.0,
            0.0,
            false,
        );
        approx(g.usable_track_length, 0.0);
        approx(g.thumb_length, 0.0);
        approx(g.thumb_offset, 0.0);
        approx(g.max_scroll, 0.0);
        approx(g.scroll_ratio, 0.0);
        // Zero content means "everything fits" -> the thumb covers the whole track.
        approx(g.thumb_size_ratio, 1.0);
    }

    #[test]
    fn content_smaller_than_viewport_is_not_scrollable() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 400.0),
            LogicalSize::new(100.0, 50.0),
            9999.0, // bogus scroll offset on a non-scrollable box
            15.0,
            false,
        );
        approx(g.thumb_size_ratio, 1.0);
        approx(g.max_scroll, 0.0);
        approx(g.scroll_ratio, 0.0);
        approx(g.thumb_offset, 0.0);
        approx(g.thumb_length, g.usable_track_length);
    }

    #[test]
    fn overscroll_clamps_the_thumb_to_the_end_of_the_track() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, 400.0),
            1.0e9, // far past max_scroll (200)
            15.0,
            false,
        );
        approx(g.scroll_ratio, 1.0);
        approx(g.thumb_offset, g.usable_track_length - g.thumb_length);
        assert!(g.thumb_offset + g.thumb_length <= g.usable_track_length + 1e-3);
    }

    #[test]
    fn negative_scroll_offsets_are_taken_by_absolute_value() {
        let pos = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, 400.0),
            50.0,
            15.0,
            false,
        );
        let neg = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, 400.0),
            -50.0,
            15.0,
            false,
        );
        approx(neg.scroll_ratio, pos.scroll_ratio);
        approx(neg.thumb_offset, pos.thumb_offset);
        assert!(neg.thumb_offset >= 0.0);
    }

    #[test]
    fn very_long_content_lifts_the_thumb_to_the_minimum_length() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, 1.0e6),
            0.0,
            15.0,
            false,
        );
        // proportional thumb would be ~0.03px; the 2*width floor kicks in
        approx(g.thumb_length, 30.0);
        assert!(g.thumb_length <= g.usable_track_length);
    }

    #[test]
    fn the_minimum_thumb_length_is_capped_by_the_usable_track() {
        // Track (40px) is barely bigger than the two buttons (2*15) -> 10px usable.
        // The 30px minimum thumb must be clamped down, never overflow the track.
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 40.0),
            LogicalSize::new(100.0, 1000.0),
            500.0,
            15.0,
            false,
        );
        approx(g.usable_track_length, 10.0);
        approx(g.thumb_length, 10.0);
        approx(g.thumb_offset, 0.0);
        assert!(g.thumb_offset + g.thumb_length <= g.usable_track_length + 1e-3);
    }

    #[test]
    fn a_container_thinner_than_its_scrollbar_still_produces_safe_lengths() {
        // 10px tall container, 15px scrollbar, corner reserved => track_total = -5.
        // Characterization: the *track rect* is allowed to go negative here (it is
        // clipped by the painter), but every derived length stays non-negative.
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 10.0),
            LogicalSize::new(100.0, 400.0),
            10.0,
            15.0,
            true,
        );
        assert!(g.track_rect.size.height < 0.0, "track rect goes negative");
        approx(g.usable_track_length, 0.0);
        approx(g.thumb_length, 0.0);
        approx(g.thumb_offset, 0.0);
    }

    #[test]
    fn zero_button_size_gives_the_whole_track_to_the_thumb() {
        let g = compute_scrollbar_geometry_with_button_size(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, 400.0),
            0.0,
            8.0,
            false,
            0.0, // overlay scrollbar: no arrow buttons
        );
        approx(g.button_size, 0.0);
        approx(g.usable_track_length, 200.0);
        approx(g.thumb_length, 100.0);
        approx(g.width_px, 8.0);
    }

    #[test]
    fn zero_width_scrollbar_does_not_panic() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Horizontal,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(400.0, 200.0),
            100.0,
            0.0,
            false,
        );
        approx(g.usable_track_length, 100.0);
        approx(g.width_px, 0.0);
        approx(g.thumb_size_ratio, 0.25);
        approx(g.thumb_length, 25.0);
        assert!(g.thumb_offset >= 0.0);
    }

    #[test]
    fn negative_content_size_is_treated_as_unscrollable() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, -400.0),
            50.0,
            15.0,
            false,
        );
        // `content_length > 0.0` is false -> ratio 1.0, no scrolling.
        approx(g.thumb_size_ratio, 1.0);
        approx(g.max_scroll, 0.0);
        approx(g.scroll_ratio, 0.0);
        approx(g.thumb_offset, 0.0);
        approx(g.thumb_length, g.usable_track_length);
    }

    #[test]
    fn a_negative_viewport_still_produces_non_negative_lengths() {
        // Degenerate inner rect (negative height). The size *ratio* goes negative,
        // but the lengths that reach the painter must not.
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, -100.0),
            LogicalSize::new(100.0, 200.0),
            50.0,
            15.0,
            false,
        );
        approx(g.thumb_size_ratio, -0.5);
        approx(g.usable_track_length, 0.0);
        approx(g.thumb_length, 0.0);
        approx(g.thumb_offset, 0.0);
        assert!(g.max_scroll >= 0.0);
    }

    #[test]
    fn float_max_inputs_stay_finite() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, f32::MAX, f32::MAX),
            LogicalSize::new(f32::MAX, f32::MAX),
            f32::MAX,
            15.0,
            true,
        );
        assert!(g.usable_track_length.is_finite());
        assert!(g.thumb_length.is_finite());
        assert!(g.thumb_offset.is_finite());
        assert!(g.max_scroll.is_finite());
        assert!(g.scroll_ratio.is_finite());
        assert!(g.thumb_size_ratio.is_finite());
        approx(g.thumb_size_ratio, 1.0);
        approx(g.max_scroll, 0.0);
        approx(g.thumb_offset, 0.0);
    }

    #[test]
    fn infinite_content_length_does_not_panic() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, f32::INFINITY),
            100.0,
            15.0,
            false,
        );
        approx(g.thumb_size_ratio, 0.0);
        approx(g.thumb_length, 30.0); // floored at 2 * width
        assert!(g.max_scroll.is_infinite());
        // finite_offset / inf == 0 -> the thumb parks at the start
        approx(g.scroll_ratio, 0.0);
        approx(g.thumb_offset, 0.0);
    }

    #[test]
    fn nan_scrollbar_width_does_not_panic_and_collapses_the_track() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, 400.0),
            0.0,
            f32::NAN,
            false,
        );
        // NaN width => NaN button size => the usable track clamps to 0 and the
        // thumb collapses. Nothing paints, but nothing panics either.
        approx(g.usable_track_length, 0.0);
        approx(g.thumb_length, 0.0);
        approx(g.thumb_offset, 0.0);
        assert!(g.width_px.is_nan());
        assert!(g.track_rect.origin.x.is_nan());
    }

    #[test]
    fn nan_content_size_does_not_panic() {
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, f32::NAN),
            50.0,
            15.0,
            false,
        );
        // `NaN > 0.0` is false -> ratio 1.0; `NaN - v` is NaN -> max_scroll 0.
        approx(g.thumb_size_ratio, 1.0);
        approx(g.max_scroll, 0.0);
        approx(g.scroll_ratio, 0.0);
        approx(g.thumb_offset, 0.0);
        approx(g.thumb_length, g.usable_track_length);
        assert!(g.thumb_length.is_finite());
    }

    #[test]
    fn infinite_scroll_offsets_saturate_the_scroll_ratio() {
        for offset in [f32::INFINITY, f32::NEG_INFINITY] {
            let g = compute_scrollbar_geometry(
                ScrollbarOrientation::Vertical,
                rect(0.0, 0.0, 100.0, 200.0),
                LogicalSize::new(100.0, 400.0),
                offset,
                15.0,
                false,
            );
            approx(g.scroll_ratio, 1.0);
            approx(g.thumb_offset, g.usable_track_length - g.thumb_length);
        }
    }

    #[test]
    fn nan_scroll_offset_leaks_nan_into_scroll_ratio_and_thumb_offset() {
        // FINDING (characterization, not a panic): `f32::clamp` returns NaN for a
        // NaN input, so a NaN scroll offset survives into `scroll_ratio` and then
        // `thumb_offset`. Every other field stays well-defined. A NaN offset would
        // paint the thumb at an undefined position rather than clamping to 0.
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, 200.0),
            LogicalSize::new(100.0, 400.0),
            f32::NAN,
            15.0,
            false,
        );
        assert!(g.scroll_ratio.is_nan());
        assert!(g.thumb_offset.is_nan());
        approx(g.usable_track_length, 170.0);
        approx(g.thumb_length, 85.0);
        approx(g.max_scroll, 200.0);
    }

    #[test]
    fn infinite_viewport_leaks_nan_into_thumb_offset() {
        // FINDING (characterization, not a panic): an infinite inner rect makes both
        // `usable_track_length` and `thumb_length` infinite, so `usable - thumb` is
        // NaN and the offset follows. No panic; the value is simply undefined.
        let g = compute_scrollbar_geometry(
            ScrollbarOrientation::Vertical,
            rect(0.0, 0.0, 100.0, f32::INFINITY),
            LogicalSize::new(100.0, 1000.0),
            0.0,
            15.0,
            false,
        );
        assert!(g.usable_track_length.is_infinite());
        assert!(g.thumb_length.is_infinite());
        assert!(g.thumb_offset.is_nan());
        approx(g.max_scroll, 0.0);
        approx(g.scroll_ratio, 0.0);
    }

    // ---------------------------------------------------------------------
    // compute_thumb_geometry  (private, numeric)
    // ---------------------------------------------------------------------

    #[test]
    fn thumb_geometry_math_is_exact_for_a_known_case() {
        let track = rect(1.0, 2.0, 3.0, 4.0);
        let g = compute_thumb_geometry(
            ScrollbarOrientation::Horizontal,
            track,
            200.0, // track_total
            100.0, // viewport_length
            200.0, // content_length
            10.0,  // button_size
            10.0,  // scrollbar_width_px
            50.0,  // scroll_offset
        );
        approx(g.usable_track_length, 180.0); // 200 - 2*10
        approx(g.thumb_size_ratio, 0.5); // 100 / 200
        approx(g.thumb_length, 90.0); // 180 * 0.5
        approx(g.max_scroll, 100.0); // 200 - 100
        approx(g.scroll_ratio, 0.5); // 50 / 100
        approx(g.thumb_offset, 45.0); // (180 - 90) * 0.5
        // the track rect is passed straight through, never recomputed
        approx(g.track_rect.origin.x, track.origin.x);
        approx(g.track_rect.size.height, track.size.height);
        assert_eq!(g.orientation, ScrollbarOrientation::Horizontal);
    }

    #[test]
    fn thumb_geometry_buttons_larger_than_the_track_collapse_the_usable_length() {
        let g = compute_thumb_geometry(
            ScrollbarOrientation::Vertical,
            LogicalRect::zero(),
            20.0,   // track_total
            100.0,  // viewport_length
            1000.0, // content_length
            500.0,  // button_size >> track
            15.0,
            250.0,
        );
        approx(g.usable_track_length, 0.0);
        approx(g.thumb_length, 0.0);
        approx(g.thumb_offset, 0.0);
        assert!(g.scroll_ratio >= 0.0 && g.scroll_ratio <= 1.0);
    }

    #[test]
    fn thumb_geometry_ignores_scroll_offset_when_content_fits() {
        let g = compute_thumb_geometry(
            ScrollbarOrientation::Vertical,
            LogicalRect::zero(),
            200.0,
            200.0, // viewport == content
            200.0,
            10.0,
            10.0,
            1.0e9, // absurd offset
        );
        approx(g.max_scroll, 0.0);
        approx(g.scroll_ratio, 0.0);
        approx(g.thumb_offset, 0.0);
        approx(g.thumb_size_ratio, 1.0);
        approx(g.thumb_length, g.usable_track_length);
    }

    #[test]
    fn thumb_geometry_clamps_an_oversized_viewport_ratio_to_one() {
        let g = compute_thumb_geometry(
            ScrollbarOrientation::Vertical,
            LogicalRect::zero(),
            200.0,
            400.0, // viewport bigger than content
            100.0,
            10.0,
            10.0,
            0.0,
        );
        approx(g.thumb_size_ratio, 1.0);
        approx(g.thumb_length, g.usable_track_length);
        approx(g.max_scroll, 0.0);
    }

    #[test]
    fn thumb_geometry_survives_an_all_nan_call() {
        let g = compute_thumb_geometry(
            ScrollbarOrientation::Vertical,
            LogicalRect::zero(),
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
            f32::NAN,
        );
        // max()/min() drop NaN in favour of the other operand, and the
        // `content_length > 0.0` / `max_scroll > 0.0` guards both read false.
        approx(g.usable_track_length, 0.0);
        approx(g.thumb_length, 0.0);
        approx(g.thumb_size_ratio, 1.0);
        approx(g.max_scroll, 0.0);
        approx(g.scroll_ratio, 0.0);
        approx(g.thumb_offset, 0.0);
    }

    // ---------------------------------------------------------------------
    // round-trip + invariants
    // ---------------------------------------------------------------------

    #[test]
    fn thumb_offset_round_trips_back_to_the_scroll_offset() {
        // This is the inverse used by `handle_scrollbar_drag`: dragging the thumb to
        // `thumb_offset` must map back to the scroll offset that produced it.
        for &offset in &[0.0_f32, 25.0, 50.0, 100.0, 150.0, 200.0] {
            let g = compute_scrollbar_geometry(
                ScrollbarOrientation::Vertical,
                rect(0.0, 0.0, 100.0, 200.0),
                LogicalSize::new(100.0, 400.0),
                offset,
                15.0,
                false,
            );
            let drag_range = g.usable_track_length - g.thumb_length;
            assert!(drag_range > 0.0);
            let recovered = (g.thumb_offset / drag_range) * g.max_scroll;
            approx(recovered, offset);
        }
    }

    #[test]
    fn thumb_offset_is_monotonic_in_the_scroll_offset() {
        let mut previous = f32::NEG_INFINITY;
        for step in 0..=20 {
            let offset = step as f32 * 15.0; // 0 .. 300, past max_scroll (200)
            let g = compute_scrollbar_geometry(
                ScrollbarOrientation::Horizontal,
                rect(0.0, 0.0, 200.0, 100.0),
                LogicalSize::new(400.0, 100.0),
                offset,
                15.0,
                false,
            );
            assert!(
                g.thumb_offset >= previous - 1e-4,
                "thumb went backwards at offset {offset}: {} < {previous}",
                g.thumb_offset
            );
            previous = g.thumb_offset;
        }
    }

    #[test]
    fn geometry_invariants_hold_across_a_finite_input_grid() {
        let orientations = [ScrollbarOrientation::Vertical, ScrollbarOrientation::Horizontal];
        let rects = [
            rect(0.0, 0.0, 0.0, 0.0),
            rect(0.0, 0.0, 1.0, 1.0),
            rect(-50.0, -50.0, 100.0, 200.0),
            rect(10.0, 20.0, 800.0, 600.0),
            rect(0.0, 0.0, f32::MAX, f32::MAX),
        ];
        let contents = [
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(1.0, 1.0),
            LogicalSize::new(100.0, 200.0),
            LogicalSize::new(1.0e9, 1.0e9),
            LogicalSize::new(f32::MAX, f32::MAX),
        ];
        let offsets = [0.0_f32, -1.0, 1.0, 12_345.0, -12_345.0, f32::MAX];
        let widths = [0.0_f32, 1.0, 8.0, 15.0, 1000.0];
        let buttons = [0.0_f32, 1.0, 15.0, 1000.0];

        for &orientation in &orientations {
            for &inner in &rects {
                for &content in &contents {
                    for &offset in &offsets {
                        for &width in &widths {
                            for &button in &buttons {
                                for &other in &[false, true] {
                                    let g = compute_scrollbar_geometry_with_button_size(
                                        orientation,
                                        inner,
                                        content,
                                        offset,
                                        width,
                                        other,
                                        button,
                                    );
                                    let ctx = format!(
                                        "inner={inner:?} content={content:?} offset={offset} \
                                         width={width} button={button} other={other}"
                                    );
                                    assert!(g.usable_track_length.is_finite(), "{ctx}");
                                    assert!(g.thumb_length.is_finite(), "{ctx}");
                                    assert!(g.thumb_offset.is_finite(), "{ctx}");
                                    assert!(g.max_scroll.is_finite(), "{ctx}");
                                    assert!(g.usable_track_length >= 0.0, "{ctx}");
                                    assert!(g.max_scroll >= 0.0, "{ctx}");
                                    assert!(g.thumb_offset >= 0.0, "{ctx}");
                                    assert!(
                                        g.thumb_length >= 0.0
                                            && g.thumb_length <= g.usable_track_length,
                                        "thumb escapes the track: {ctx}"
                                    );
                                    assert!(
                                        (0.0..=1.0).contains(&g.thumb_size_ratio),
                                        "size ratio out of range: {ctx}"
                                    );
                                    assert!(
                                        (0.0..=1.0).contains(&g.scroll_ratio),
                                        "scroll ratio out of range: {ctx}"
                                    );
                                    let slack = g.usable_track_length
                                        + g.usable_track_length.abs() * 1e-5
                                        + 1e-3;
                                    assert!(
                                        g.thumb_offset + g.thumb_length <= slack,
                                        "thumb overruns the track end: {ctx}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn default_geometry_is_inert() {
        let g = ScrollbarGeometry::default();
        assert_eq!(g.orientation, ScrollbarOrientation::Vertical);
        approx(g.button_size, 0.0);
        approx(g.usable_track_length, 0.0);
        approx(g.thumb_length, 0.0);
        approx(g.thumb_size_ratio, 0.0);
        approx(g.scroll_ratio, 0.0);
        approx(g.thumb_offset, 0.0);
        approx(g.max_scroll, 0.0);
        approx(g.width_px, 0.0);
        assert_eq!(g.track_rect, LogicalRect::zero());
    }
}
