//! Where the system takes the swipes of its own gestures, and the rects an app asks it to leave alone.
//!
//! A touch that starts inside one of the system's gesture regions (the back swipe from the side edges, the
//! home swipe at the bottom) may be taken by the system, and the app then gets a cancel. A window holds the
//! regions its shell reports (`SystemGestureInsets`: Android's `WindowInsets` system, mandatory and tappable
//! insets) and the rects the app excludes; the system honours at most 200 dp of height of rects at each
//! side edge, so the engine keeps what the platform will keep and says how many rects it dropped or cut.

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_layout::{
    managers::gesture::{EdgeInsetsPx, SystemGestureInsets, SYSTEM_GESTURE_EXCLUSION_LIMIT_PX},
    window::LayoutWindow,
};
use rust_fontconfig::FcFontCache;

/// A window 400 px wide.
fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a window");
    lw.current_window_state.size.dimensions = LogicalSize::new(400.0, 800.0);
    lw
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

#[test]
fn a_window_nobody_filled_has_no_regions_and_no_exclusions() {
    let lw = window();
    assert_eq!(*lw.system_gesture_insets(), SystemGestureInsets::default());
    assert!(lw.system_gesture_exclusion_rects().is_empty());
}

/// The shell reports the three kinds of region; the engine keeps numbers that mean something: a side that
/// is not a number or is negative is no region, and none is kept beyond 10,000 px.
#[test]
fn a_shell_reports_the_regions_and_the_engine_keeps_usable_numbers() {
    let mut lw = window();
    let reported = SystemGestureInsets {
        normal: EdgeInsetsPx { left: 24.0, top: 0.0, right: 24.0, bottom: 48.0 },
        mandatory: EdgeInsetsPx { left: 0.0, top: 0.0, right: 0.0, bottom: 32.0 },
        tappable: EdgeInsetsPx { left: f32::NAN, top: -5.0, right: 20_000.0, bottom: 48.0 },
    };

    lw.set_system_gesture_insets(reported);

    let kept = *lw.system_gesture_insets();
    assert_eq!(kept.normal, reported.normal, "usable numbers stay as they were");
    assert_eq!(kept.mandatory, reported.mandatory);
    assert_eq!(
        kept.tappable,
        EdgeInsetsPx { left: 0.0, top: 0.0, right: 10_000.0, bottom: 48.0 },
        "not a number and negative are no region, a unit mixed up is cut"
    );
}

/// Rects away from the edges are no business of the system's swipes: kept as asked, however many.
#[test]
fn rects_away_from_the_edges_are_kept_as_asked() {
    let mut lw = window();
    let asked: Vec<LogicalRect> = (0..10).map(|i| rect(100.0, 50.0 * i as f32, 150.0, 400.0)).collect();

    let changed = lw.set_system_gesture_exclusion_rects(asked.clone());

    assert_eq!(changed, 0);
    assert_eq!(lw.system_gesture_exclusion_rects(), asked.as_slice());
}

/// At the left edge the system honours 200 px of height: three rects of 100 px, the first two stand, the
/// third finds no room and is dropped.
#[test]
fn the_height_of_rects_at_an_edge_is_kept_to_what_the_system_honours() {
    let mut lw = window();
    assert_eq!(SYSTEM_GESTURE_EXCLUSION_LIMIT_PX, 200.0);

    let changed = lw.set_system_gesture_exclusion_rects(vec![
        rect(0.0, 0.0, 40.0, 100.0),
        rect(0.0, 300.0, 40.0, 100.0),
        rect(0.0, 600.0, 40.0, 100.0),
    ]);

    assert_eq!(changed, 1, "one rect was dropped");
    assert_eq!(
        lw.system_gesture_exclusion_rects(),
        &[rect(0.0, 0.0, 40.0, 100.0), rect(0.0, 300.0, 40.0, 100.0)]
    );
}

/// A rect that only partly fits is cut to the height that fits; its top part stays.
#[test]
fn a_rect_that_only_partly_fits_is_cut_to_the_height_that_fits() {
    let mut lw = window();

    let changed = lw.set_system_gesture_exclusion_rects(vec![
        rect(0.0, 0.0, 40.0, 150.0),
        rect(0.0, 300.0, 40.0, 100.0),
    ]);

    assert_eq!(changed, 1, "one rect was cut");
    assert_eq!(
        lw.system_gesture_exclusion_rects(),
        &[rect(0.0, 0.0, 40.0, 150.0), rect(0.0, 300.0, 40.0, 50.0)]
    );
}

/// The two edges have a budget of their own: 200 px at the left and 200 px at the right all stand; a rect
/// across the whole width uses both.
#[test]
fn the_two_edges_have_budgets_of_their_own() {
    let mut lw = window();

    let changed = lw.set_system_gesture_exclusion_rects(vec![
        rect(0.0, 0.0, 40.0, 200.0),
        rect(360.0, 0.0, 40.0, 200.0),
    ]);
    assert_eq!(changed, 0, "200 px at each side edge all stand");

    let changed = lw.set_system_gesture_exclusion_rects(vec![
        rect(0.0, 0.0, 400.0, 150.0),
        rect(0.0, 300.0, 40.0, 100.0),
        rect(360.0, 300.0, 40.0, 100.0),
    ]);
    assert_eq!(changed, 2, "the full-width rect used 150 px of both edges: 50 px are left at each");
    assert_eq!(
        lw.system_gesture_exclusion_rects(),
        &[
            rect(0.0, 0.0, 400.0, 150.0),
            rect(0.0, 300.0, 40.0, 50.0),
            rect(360.0, 300.0, 40.0, 50.0),
        ]
    );
}

/// A rect that is not one excludes nothing: not a number, no size.
#[test]
fn a_rect_that_is_not_a_rect_is_dropped() {
    let mut lw = window();

    let changed = lw.set_system_gesture_exclusion_rects(vec![
        rect(f32::NAN, 0.0, 10.0, 10.0),
        rect(0.0, 0.0, 0.0, 10.0),
        rect(0.0, 0.0, 10.0, -5.0),
        rect(100.0, 100.0, 10.0, 10.0),
    ]);

    assert_eq!(changed, 3);
    assert_eq!(lw.system_gesture_exclusion_rects(), &[rect(100.0, 100.0, 10.0, 10.0)]);
}

/// A call replaces what the one before asked: the budgets are the call's own.
#[test]
fn a_second_call_replaces_the_first() {
    let mut lw = window();
    let _ = lw.set_system_gesture_exclusion_rects(vec![rect(0.0, 0.0, 40.0, 200.0)]);

    let changed = lw.set_system_gesture_exclusion_rects(vec![rect(0.0, 500.0, 40.0, 200.0)]);

    assert_eq!(changed, 0);
    assert_eq!(lw.system_gesture_exclusion_rects(), &[rect(0.0, 500.0, 40.0, 200.0)]);
    let _ = lw.set_system_gesture_exclusion_rects(Vec::new());
    assert!(lw.system_gesture_exclusion_rects().is_empty());
}
