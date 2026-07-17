/// Integration tests for CSS float positioning (CSS 2.2 §9.5)
///
/// Tests cover:
/// - FloatingContext: adding floats, available space calculation
/// - Clearance offsets for `clear: left|right|both`
/// - Float positioning with overlapping floats
/// - Line box space narrowing with floats present
///
/// CSS Spec references:
/// - CSS 2.2 §9.5 Floats
/// - CSS 2.2 §9.5.1 Positioning the float
/// - CSS 2.2 §9.5.2 Controlling flow next to floats (clear)
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_css::props::layout::{LayoutClear, LayoutFloat, LayoutWritingMode};
use azul_layout::solver3::fc::FloatingContext;
use azul_layout::solver3::geometry::EdgeSizes;

fn zero_edges() -> EdgeSizes {
    EdgeSizes {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    }
}

fn edges(top: f32, right: f32, bottom: f32, left: f32) -> EdgeSizes {
    EdgeSizes {
        top,
        right,
        bottom,
        left,
    }
}

// ============================================================================
// FloatingContext: Basic operations
// ============================================================================

#[test]
fn test_floating_context_default_is_empty() {
    // A default FloatingContext should behave as if no floats exist:
    // available space returns the full container width.
    let ctx = FloatingContext::default();
    let wm = LayoutWritingMode::HorizontalTb;
    let (start, end) = ctx.available_line_box_space(0.0, 50.0, 800.0, wm);
    assert_eq!(start, 0.0);
    assert_eq!(end, 800.0);
}

#[test]
fn test_add_float_left() {
    // CSS 2.2 §9.5: A left float is positioned at the top-left of the
    // containing block's content area.
    let mut ctx = FloatingContext::default();
    let rect = LogicalRect {
        origin: LogicalPosition { x: 0.0, y: 0.0 },
        size: LogicalSize::new(100.0, 50.0),
    };
    ctx.add_float(LayoutFloat::Left, rect, zero_edges());
    // After adding a left float at x=0..100, the available space should narrow
    let wm = LayoutWritingMode::HorizontalTb;
    let (start, _end) = ctx.available_line_box_space(0.0, 50.0, 800.0, wm);
    assert_eq!(start, 100.0); // pushed right by the 100px-wide float
}

#[test]
fn test_add_float_right() {
    // CSS 2.2 §9.5: A right float is positioned at the top-right of the
    // containing block's content area.
    let mut ctx = FloatingContext::default();
    let rect = LogicalRect {
        origin: LogicalPosition { x: 200.0, y: 0.0 },
        size: LogicalSize::new(100.0, 50.0),
    };
    ctx.add_float(LayoutFloat::Right, rect, zero_edges());
    // After adding a right float at x=200..300, the available space should narrow from the right
    let wm = LayoutWritingMode::HorizontalTb;
    let (_start, end) = ctx.available_line_box_space(0.0, 50.0, 800.0, wm);
    assert_eq!(end, 200.0); // narrowed to the float's cross_start
}

#[test]
fn test_add_multiple_floats() {
    let mut ctx = FloatingContext::default();
    let r1 = LogicalRect {
        origin: LogicalPosition { x: 0.0, y: 0.0 },
        size: LogicalSize::new(100.0, 50.0),
    };
    let r2 = LogicalRect {
        origin: LogicalPosition { x: 200.0, y: 0.0 },
        size: LogicalSize::new(100.0, 50.0),
    };
    ctx.add_float(LayoutFloat::Left, r1, zero_edges());
    ctx.add_float(LayoutFloat::Right, r2, zero_edges());
    // Both floats should affect available space: left float narrows from left,
    // right float narrows from right
    let wm = LayoutWritingMode::HorizontalTb;
    let (start, end) = ctx.available_line_box_space(0.0, 50.0, 800.0, wm);
    assert_eq!(start, 100.0); // left float at x=0..100
    assert_eq!(end, 200.0);   // right float at x=200..300
}

// ============================================================================
// available_line_box_space: CSS 2.2 §9.5.1
// ============================================================================

#[test]
fn test_available_space_no_floats() {
    // With no floats, the entire container width is available
    let ctx = FloatingContext::default();
    let wm = LayoutWritingMode::HorizontalTb;
    let (start, end) = ctx.available_line_box_space(0.0, 50.0, 800.0, wm);
    assert_eq!(start, 0.0);
    assert_eq!(end, 800.0);
}

#[test]
fn test_available_space_left_float_narrows_from_left() {
    // CSS 2.2 §9.5: A left float pushes the available space start to the right
    //
    // Container: 800px wide
    // Left float: 0,0 -> 200x100
    // Query line at y=0..50 (overlaps float) → available 200..800
    let mut ctx = FloatingContext::default();
    let rect = LogicalRect {
        origin: LogicalPosition { x: 0.0, y: 0.0 },
        size: LogicalSize::new(200.0, 100.0),
    };
    ctx.add_float(LayoutFloat::Left, rect, zero_edges());

    let wm = LayoutWritingMode::HorizontalTb;
    let (start, end) = ctx.available_line_box_space(0.0, 50.0, 800.0, wm);

    // In horizontal-tb, main=y, cross=x
    // Float rect x=0..200, y=0..100
    // Query main_start=0, main_end=50 → overlaps float
    // Left float: available_cross_start = max(0, float_cross_end=200) = 200
    assert_eq!(start, 200.0);
    assert_eq!(end, 800.0);
}

#[test]
fn test_available_space_right_float_narrows_from_right() {
    // CSS 2.2 §9.5: A right float pushes the available space end to the left
    //
    // Container: 800px wide
    // Right float at x=600, 200px wide → occupies 600..800
    let mut ctx = FloatingContext::default();
    let rect = LogicalRect {
        origin: LogicalPosition { x: 600.0, y: 0.0 },
        size: LogicalSize::new(200.0, 100.0),
    };
    ctx.add_float(LayoutFloat::Right, rect, zero_edges());

    let wm = LayoutWritingMode::HorizontalTb;
    let (start, end) = ctx.available_line_box_space(0.0, 50.0, 800.0, wm);

    // Right float: available_cross_end = min(800, float_cross_start=600) = 600
    assert_eq!(start, 0.0);
    assert_eq!(end, 600.0);
}

#[test]
fn test_available_space_both_floats_narrow_both_sides() {
    // CSS 2.2 §9.5: Left and right floats both narrow the available space
    //
    // Container: 800px wide
    // Left float: 0,0 → 200x100
    // Right float: 600,0 → 200x100
    // Available space: 200..600 (400px)
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(200.0, 100.0),
        },
        zero_edges(),
    );
    ctx.add_float(
        LayoutFloat::Right,
        LogicalRect {
            origin: LogicalPosition { x: 600.0, y: 0.0 },
            size: LogicalSize::new(200.0, 100.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let (start, end) = ctx.available_line_box_space(0.0, 50.0, 800.0, wm);
    assert_eq!(start, 200.0);
    assert_eq!(end, 600.0);
}

#[test]
fn test_available_space_no_overlap_on_main_axis() {
    // If the query range doesn't overlap the float on the main axis,
    // the full width is available
    //
    // Float at y=0..100, query at y=150..200 → no overlap
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(200.0, 100.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let (start, end) = ctx.available_line_box_space(150.0, 200.0, 800.0, wm);
    assert_eq!(start, 0.0);
    assert_eq!(end, 800.0);
}

#[test]
fn test_available_space_partial_overlap() {
    // Query partially overlaps float on main axis
    // Float y=0..100, query y=80..120 → overlap
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(200.0, 100.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let (start, end) = ctx.available_line_box_space(80.0, 120.0, 800.0, wm);
    assert_eq!(start, 200.0); // Narrowed by left float
    assert_eq!(end, 800.0);
}

#[test]
fn test_available_space_stacked_left_floats() {
    // CSS 2.2 §9.5.1: Multiple left floats at the same vertical position
    // stack horizontally: float1 at x=0..100, float2 at x=100..250
    // Available start = max of all float cross_ends
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(100.0, 100.0),
        },
        zero_edges(),
    );
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 100.0, y: 0.0 },
            size: LogicalSize::new(150.0, 80.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let (start, end) = ctx.available_line_box_space(0.0, 50.0, 800.0, wm);
    // Both floats overlap at y=0..50
    // Float 1: cross_end = 0 + 100 = 100
    // Float 2: cross_end = 100 + 150 = 250
    // available_cross_start = max(100, 250) = 250
    assert_eq!(start, 250.0);
    assert_eq!(end, 800.0);
}

// ============================================================================
// clearance_offset: CSS 2.2 §9.5.2
// ============================================================================

#[test]
fn test_clearance_offset_no_floats() {
    // No floats → clearance_offset returns current position
    let ctx = FloatingContext::default();
    let wm = LayoutWritingMode::HorizontalTb;
    let offset = ctx.clearance_offset(LayoutClear::Both, 50.0, wm);
    assert_eq!(offset, 50.0);
}

#[test]
fn test_clearance_none_does_nothing() {
    // clear: none → doesn't clear any floats, returns current position
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(200.0, 100.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let offset = ctx.clearance_offset(LayoutClear::None, 0.0, wm);
    assert_eq!(offset, 0.0); // No clearing, stays at 0
}

#[test]
fn test_clear_left_clears_left_float() {
    // CSS 2.2 §9.5.2: clear:left → "top border edge below bottom outer edge of left floats"
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(200.0, 100.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let offset = ctx.clearance_offset(LayoutClear::Left, 0.0, wm);
    // Float occupies y=0..100, clear:left should push to y=100
    assert_eq!(offset, 100.0);
}

#[test]
fn test_clear_right_clears_right_float() {
    // CSS 2.2 §9.5.2: clear:right → clears right floats only
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Right,
        LogicalRect {
            origin: LogicalPosition { x: 600.0, y: 0.0 },
            size: LogicalSize::new(200.0, 120.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let offset = ctx.clearance_offset(LayoutClear::Right, 0.0, wm);
    assert_eq!(offset, 120.0);
}

#[test]
fn test_clear_left_ignores_right_float() {
    // clear:left should NOT clear right floats
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Right,
        LogicalRect {
            origin: LogicalPosition { x: 600.0, y: 0.0 },
            size: LogicalSize::new(200.0, 120.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let offset = ctx.clearance_offset(LayoutClear::Left, 0.0, wm);
    // Right float not cleared by clear:left
    assert_eq!(offset, 0.0);
}

#[test]
fn test_clear_right_ignores_left_float() {
    // clear:right should NOT clear left floats
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(200.0, 100.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let offset = ctx.clearance_offset(LayoutClear::Right, 0.0, wm);
    assert_eq!(offset, 0.0);
}

#[test]
fn test_clear_both_clears_all_floats() {
    // CSS 2.2 §9.5.2: clear:both → clears both left and right floats
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(200.0, 100.0),
        },
        zero_edges(),
    );
    ctx.add_float(
        LayoutFloat::Right,
        LogicalRect {
            origin: LogicalPosition { x: 600.0, y: 0.0 },
            size: LogicalSize::new(200.0, 150.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let offset = ctx.clearance_offset(LayoutClear::Both, 0.0, wm);
    // Must clear past the tallest float (right float at 150px)
    assert_eq!(offset, 150.0);
}

#[test]
fn test_clearance_with_margin() {
    // CSS 2.2 §9.5.2: Clearance considers the float's margin-box outer edge
    // "below the bottom outer edge" = content+padding+border+margin
    let mut ctx = FloatingContext::default();
    let margin = edges(0.0, 0.0, 20.0, 0.0); // 20px bottom margin
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(200.0, 100.0),
        },
        margin,
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let offset = ctx.clearance_offset(LayoutClear::Left, 0.0, wm);
    // Float content goes to y=100, plus bottom margin of 20 → outer edge at 120
    assert_eq!(offset, 120.0);
}

#[test]
fn test_clearance_already_past_float() {
    // If current position is already past the float, clearance doesn't move backward
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(200.0, 100.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let offset = ctx.clearance_offset(LayoutClear::Left, 200.0, wm);
    // Already at 200, float ends at 100 → stay at 200
    assert_eq!(offset, 200.0);
}

#[test]
fn test_clearance_multiple_stacked_floats() {
    // Multiple floats at different y positions, clear:both should clear past all
    let mut ctx = FloatingContext::default();
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize::new(100.0, 50.0),
        },
        zero_edges(),
    );
    ctx.add_float(
        LayoutFloat::Left,
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 50.0 },
            size: LogicalSize::new(100.0, 100.0),
        },
        zero_edges(),
    );
    ctx.add_float(
        LayoutFloat::Right,
        LogicalRect {
            origin: LogicalPosition { x: 700.0, y: 0.0 },
            size: LogicalSize::new(100.0, 80.0),
        },
        zero_edges(),
    );

    let wm = LayoutWritingMode::HorizontalTb;
    let offset = ctx.clearance_offset(LayoutClear::Both, 0.0, wm);
    // Left floats: 0..50 and 50..150  → max end = 150
    // Right float: 0..80             → max end = 80
    // clear:both → max(150, 80) = 150
    assert_eq!(offset, 150.0);
}

// ============================================================================
// OverflowBehavior helpers
// ============================================================================

#[test]
fn test_overflow_behavior_is_clipped() {
    use azul_layout::solver3::fc::OverflowBehavior;

    assert!(!OverflowBehavior::Visible.is_clipped());
    assert!(OverflowBehavior::Hidden.is_clipped());
    assert!(OverflowBehavior::Clip.is_clipped());
    assert!(OverflowBehavior::Scroll.is_clipped());
    assert!(OverflowBehavior::Auto.is_clipped());
}

#[test]
fn test_overflow_behavior_is_scroll() {
    use azul_layout::solver3::fc::OverflowBehavior;

    assert!(!OverflowBehavior::Visible.is_scroll());
    assert!(!OverflowBehavior::Hidden.is_scroll());
    assert!(!OverflowBehavior::Clip.is_scroll());
    assert!(OverflowBehavior::Scroll.is_scroll());
    assert!(OverflowBehavior::Auto.is_scroll());
}

// ============================================================================
// check_scrollbar_necessity: CSS Overflow Level 3 §3
// ============================================================================

#[test]
fn test_scrollbar_visible_never_needs_scrollbar() {
    // overflow: visible → no scrollbars regardless of content size
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};
    let result = check_scrollbar_necessity(
        LogicalSize::new(2000.0, 2000.0), // huge content
        LogicalSize::new(800.0, 600.0),   // small container
        OverflowBehavior::Visible,
        OverflowBehavior::Visible,
        16.0,
    );
    assert!(!result.needs_horizontal);
    assert!(!result.needs_vertical);
}

#[test]
fn test_scrollbar_hidden_never_needs_scrollbar() {
    // overflow: hidden → no scrollbars (content clipped)
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};
    let result = check_scrollbar_necessity(
        LogicalSize::new(2000.0, 2000.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Hidden,
        OverflowBehavior::Hidden,
        16.0,
    );
    assert!(!result.needs_horizontal);
    assert!(!result.needs_vertical);
}

#[test]
fn test_scrollbar_scroll_always_shows() {
    // overflow: scroll → always show scrollbars
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};
    let result = check_scrollbar_necessity(
        LogicalSize::new(100.0, 100.0), // small content
        LogicalSize::new(800.0, 600.0), // large container
        OverflowBehavior::Scroll,
        OverflowBehavior::Scroll,
        16.0,
    );
    assert!(result.needs_horizontal);
    assert!(result.needs_vertical);
}

#[test]
fn test_scrollbar_auto_only_when_overflowing() {
    // overflow: auto → scrollbar only when content exceeds container
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};

    // Content fits → no scrollbars
    let result = check_scrollbar_necessity(
        LogicalSize::new(400.0, 300.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
        16.0,
    );
    assert!(!result.needs_horizontal);
    assert!(!result.needs_vertical);

    // Content overflows vertically only
    let result = check_scrollbar_necessity(
        LogicalSize::new(400.0, 1000.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
        16.0,
    );
    assert!(!result.needs_horizontal);
    assert!(result.needs_vertical);

    // Content overflows horizontally only
    let result = check_scrollbar_necessity(
        LogicalSize::new(1200.0, 300.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
        16.0,
    );
    assert!(result.needs_horizontal);
    assert!(!result.needs_vertical);
}

#[test]
fn test_scrollbar_auto_cascade_vertical_triggers_horizontal() {
    // CSS Overflow L3: A vertical scrollbar can reduce horizontal space,
    // triggering a horizontal scrollbar
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};

    // Content is 790px wide (fits in 800px), but 1000px tall (triggers vertical scrollbar).
    // With 16px scrollbar, horizontal space reduced to 784px.
    // 790px > 784px → now also needs horizontal scrollbar.
    let result = check_scrollbar_necessity(
        LogicalSize::new(790.0, 1000.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
        16.0,
    );
    assert!(result.needs_vertical);
    assert!(result.needs_horizontal); // cascade effect
}

#[test]
fn test_scrollbar_auto_cascade_horizontal_triggers_vertical() {
    // Same as above but horizontal → vertical cascade
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};

    // Content is 1200px wide (triggers horizontal scrollbar), 590px tall (fits in 600px).
    // With 16px scrollbar height, vertical space reduced to 584px.
    // 590px > 584px → now also needs vertical scrollbar.
    let result = check_scrollbar_necessity(
        LogicalSize::new(1200.0, 590.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
        16.0,
    );
    assert!(result.needs_horizontal);
    assert!(result.needs_vertical); // cascade effect
}

#[test]
fn test_scrollbar_overlay_zero_width() {
    // macOS overlay scrollbars: scrollbar_width_px = 0
    // Should still register scroll nodes, but reserve no space
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};

    let result = check_scrollbar_necessity(
        LogicalSize::new(400.0, 1000.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
        0.0, // overlay scrollbar
    );
    assert!(result.needs_vertical);
    assert!(!result.needs_horizontal); // No cascade with 0-width scrollbar
    assert_eq!(result.scrollbar_width, 0.0);
    assert_eq!(result.scrollbar_height, 0.0);
}

#[test]
fn test_scrollbar_clip_never_needs_scrollbar() {
    // overflow: clip → like hidden, no scrollbars
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};
    let result = check_scrollbar_necessity(
        LogicalSize::new(2000.0, 2000.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Clip,
        OverflowBehavior::Clip,
        16.0,
    );
    assert!(!result.needs_horizontal);
    assert!(!result.needs_vertical);
}

#[test]
fn test_scrollbar_mixed_overflow() {
    // overflow-x: hidden, overflow-y: auto
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};
    let result = check_scrollbar_necessity(
        LogicalSize::new(2000.0, 2000.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Hidden,
        OverflowBehavior::Auto,
        16.0,
    );
    assert!(!result.needs_horizontal); // hidden = no scrollbar
    assert!(result.needs_vertical); // auto + overflow = scrollbar
}

#[test]
fn test_scrollbar_content_exactly_fits_epsilon() {
    // Content exactly fits (within EPSILON=1.0) → no scrollbar
    // This tests the epsilon tolerance for float comparison
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};
    let result = check_scrollbar_necessity(
        LogicalSize::new(800.5, 600.5), // within 1.0 epsilon
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
        16.0,
    );
    assert!(!result.needs_horizontal);
    assert!(!result.needs_vertical);
}

#[test]
fn test_scrollbar_content_just_overflows() {
    // Content exceeds container by more than EPSILON → scrollbar needed
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};
    let result = check_scrollbar_necessity(
        LogicalSize::new(802.0, 602.0), // > 1.0 epsilon
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
        16.0,
    );
    assert!(result.needs_horizontal);
    assert!(result.needs_vertical);
}

#[test]
fn test_scrollbar_width_values() {
    // Verify scrollbar_width and scrollbar_height are set correctly
    use azul_layout::solver3::fc::{check_scrollbar_necessity, OverflowBehavior};

    // Only vertical scrollbar
    let result = check_scrollbar_necessity(
        LogicalSize::new(400.0, 1000.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
        16.0,
    );
    assert_eq!(result.scrollbar_width, 16.0); // vertical scrollbar reserves 16px width
    assert_eq!(result.scrollbar_height, 0.0); // no horizontal scrollbar

    // Only horizontal scrollbar
    let result = check_scrollbar_necessity(
        LogicalSize::new(1200.0, 300.0),
        LogicalSize::new(800.0, 600.0),
        OverflowBehavior::Auto,
        OverflowBehavior::Auto,
        16.0,
    );
    assert_eq!(result.scrollbar_width, 0.0);
    assert_eq!(result.scrollbar_height, 16.0);
}
