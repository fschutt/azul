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
