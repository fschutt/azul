//! The constants of gesture detection are ONE struct a window holds (`GestureConfig`): what the shells read
//! from the system once per window (Android's `ViewConfiguration`, UIKit's numbers) and hand over, and what
//! the engine keeps usable whatever they hand over.
//!
//! Before it, the touch slop of each shell was a constant of its own (Android 8 dp, iOS 10 pt) and the
//! detection thresholds of the engine were another set; a number a shell got wrong (NaN, a unit mixed up)
//! went straight into every recogniser.

use azul_layout::{managers::gesture::GestureConfig, window::LayoutWindow};
use rust_fontconfig::FcFontCache;

fn window() -> LayoutWindow {
    LayoutWindow::new(FcFontCache::default()).expect("a window")
}

/// The defaults are the numbers the platforms use when nothing is read: Android's `ViewConfiguration` at 1 dp
/// = 1 logical px, the desktop's wheel line of 20 px.
#[test]
fn the_defaults_are_the_numbers_the_platforms_use() {
    let c = GestureConfig::default();
    assert_eq!(c.touch_slop, 8.0, "Android's touch slop in dp");
    assert_eq!(c.paging_slop, 16.0, "twice the touch slop");
    assert_eq!(c.double_tap_slop, 100.0);
    assert_eq!(c.double_tap_timeout_ms, 300);
    assert_eq!(c.tap_timeout_ms, 100);
    assert_eq!(c.long_press_timeout_ms, 500);
    assert_eq!(c.min_fling_velocity, 50.0);
    assert_eq!(c.max_fling_velocity, 8000.0);
    assert_eq!(c.overscroll_distance, 0.0);
    assert_eq!((c.wheel_scroll_factor_x, c.wheel_scroll_factor_y), (20.0, 20.0));
    assert_eq!(c.ambiguous_gesture_multiplier, 2.0);
}

/// A window holds the defaults until a shell sets its own; the old name still names the same struct.
#[test]
fn a_window_has_the_defaults_until_a_shell_sets_its_own() {
    let lw = window();
    assert_eq!(*lw.gesture_config(), GestureConfig::default());
    let _same: azul_layout::managers::gesture::GestureDetectionConfig = *lw.gesture_config();
}

/// The shell hands over what it read, the window keeps it, and the detection fields every recogniser of the
/// manager reads follow the platform's touch numbers.
#[test]
fn a_shell_sets_the_numbers_it_read_and_the_recognisers_see_them() {
    let mut lw = window();
    let read = GestureConfig::default().with_platform_touch(12.0, 120.0, 250, 400);

    let replaced = lw.set_gesture_config(read);

    assert!(replaced.is_empty(), "usable numbers are kept as they are: {replaced:?}");
    assert_eq!(*lw.gesture_config(), read);
    let c = lw.gesture_drag_manager.config;
    assert_eq!(c.touch_slop, 12.0);
    assert_eq!(c.drag_distance_threshold, 12.0, "a drag starts where the touch slop ends");
    assert_eq!(c.long_press_time_threshold_ms, 400, "a long press takes the system's timeout");
    assert_eq!(c.double_click_time_threshold_ms, 250);
    assert_eq!(c.double_click_distance_threshold, 120.0);
}

/// A number that is not a number is the default's, one beyond what a touch screen can mean is cut to the limit
/// - the engine never takes it - and the shell is told which fields it was.
#[test]
fn numbers_that_are_not_usable_are_replaced_or_cut_and_named() {
    let mut lw = window();
    let mut bad = GestureConfig::default();
    bad.touch_slop = f32::NAN;
    bad.double_tap_slop = 1.0e9;
    bad.long_press_timeout_ms = u64::MAX;
    bad.min_samples_for_gesture = 0;
    bad.wheel_scroll_factor_y = f32::INFINITY;
    bad.ambiguous_gesture_multiplier = 0.2;

    let replaced = lw.set_gesture_config(bad);

    assert_eq!(
        replaced,
        vec![
            "touch_slop",
            "double_tap_slop",
            "wheel_scroll_factor_y",
            "ambiguous_gesture_multiplier",
            "long_press_timeout_ms",
            "min_samples_for_gesture",
        ]
    );
    let c = *lw.gesture_config();
    assert_eq!(c.touch_slop, 8.0, "NaN is the default's");
    assert_eq!(c.double_tap_slop, 1000.0, "a slop of a million pixels is a unit mixed up: cut");
    assert_eq!(c.wheel_scroll_factor_y, 20.0, "infinite is the default's");
    assert_eq!(c.ambiguous_gesture_multiplier, 1.0, "a multiplier below 1 would shrink the slop");
    assert_eq!(c.long_press_timeout_ms, 60_000);
    assert_eq!(c.min_samples_for_gesture, 1);
}
