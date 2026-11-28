use azul::widgets::{FormFactor, PreviewDock};

use super::*;

/// `--form phone` and `--form desktop` are what they say, whatever the window and the platform.
#[test]
fn a_chosen_form_is_the_form() {
    for (width, touch) in [(300.0, true), (300.0, false), (1400.0, true), (1400.0, false)] {
        assert!(is_phone(FormFactor::Phone, width, touch), "{width} {touch}");
        assert!(!is_phone(FormFactor::Desktop, width, touch), "{width} {touch}");
    }
}

/// `Auto` is a phone's only on a touch platform in a narrow window: a desktop window narrowed
/// stays a desktop's.
#[test]
fn auto_is_a_phone_only_on_a_touch_platform_in_a_narrow_window() {
    assert!(is_phone(FormFactor::Auto, 411.0, true));
    assert!(!is_phone(FormFactor::Auto, 411.0, false), "a narrow desktop window is no phone");
    assert!(!is_phone(FormFactor::Auto, 1000.0, true), "a wide touch window is no phone");
    assert!(
        !is_phone(FormFactor::Auto, PHONE_MAX_WIDTH, true),
        "the phone's width is under the line"
    );
    assert!(!is_phone(FormFactor::Auto, 0.0, true), "a window not measured yet is not narrow");
}

/// The window is made for the right hand for now: the preview docks at the right of a desktop and at
/// the foot of a phone, and the decision is one function.
#[test]
fn the_window_is_right_handed_and_the_dock_is_decided_in_one_place() {
    assert_eq!(HANDEDNESS, Handedness::Right);
    assert_eq!(Handedness::default(), Handedness::Right);
    assert_eq!(HANDEDNESS.preview_dock(false), PreviewDock::Right);
    assert_eq!(HANDEDNESS.preview_dock(true), PreviewDock::Bottom);
}
