//! The insets of a phone's window: cleaned numbers, the room above the keyboard.

use super::*;

#[test]
fn a_side_that_is_no_number_or_is_negative_is_no_inset() {
    let i = Insets::new(f32::NAN, -4.0, f32::INFINITY, 0.0, f32::NEG_INFINITY);
    assert_eq!(i, Insets::default());
    assert!(i.is_none());
}

#[test]
fn real_numbers_are_kept_as_they_are() {
    let i = Insets::new(48.0, 8.0, 24.0, 6.0, 312.5);
    assert_eq!((i.top, i.right, i.bottom, i.left, i.keyboard), (48.0, 8.0, 24.0, 6.0, 312.5));
    assert!(!i.is_none());
}

#[test]
fn a_keyboard_alone_is_an_inset() {
    assert!(!Insets::new(0.0, 0.0, 0.0, 0.0, 280.0).is_none());
}

#[test]
fn what_stands_at_the_bottom_rises_by_the_keyboard_and_keeps_the_home_indicators_room_only_without_one() {
    let down = Insets::new(0.0, 0.0, 24.0, 0.0, 0.0);
    assert_eq!((down.lift(), down.bottom_clear()), (0.0, 24.0));
    let up = Insets::new(0.0, 0.0, 24.0, 0.0, 300.0);
    assert_eq!((up.lift(), up.bottom_clear()), (300.0, 0.0), "the keyboard covers the home indicator's strip");
}

#[test]
fn an_app_whose_root_the_engine_insets_has_only_the_keyboard_left_less_the_bar_under_the_root() {
    // 48 px of status bar, 48 of navigation bar, 300 of keyboard (the keyboard's number includes the bar's area)
    let raw = Insets::new(48.0, 0.0, 48.0, 0.0, 300.0);
    let inside = raw.inside_root();
    assert_eq!((inside.top, inside.right, inside.bottom, inside.left), (0.0, 0.0, 0.0, 0.0), "the engine's root already excludes the bars");
    assert_eq!(inside.keyboard, 252.0, "the root's lower edge is 48 px above the window's: the keyboard covers 252 of the root");
    assert_eq!(inside.lift(), 252.0);
    assert_eq!(inside.bottom_clear(), 0.0);
}

#[test]
fn no_keyboard_leaves_nothing_and_a_keyboard_shorter_than_the_bar_cannot_make_a_negative_lift() {
    assert!(Insets::new(24.0, 0.0, 34.0, 0.0, 0.0).inside_root().is_none());
    assert_eq!(Insets::new(0.0, 0.0, 48.0, 0.0, 30.0).inside_root().keyboard, 0.0);
    assert!(Insets::default().inside_root().is_none());
}
