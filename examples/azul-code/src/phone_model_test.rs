//! When the window is a phone's, how wide its drawer is, when the key row shows.

use super::*;

#[test]
fn a_window_narrower_than_600_or_shorter_than_500_is_a_phones() {
    assert!(is_compact(411.0, 914.0), "a Pixel 8 upright");
    assert!(is_compact(402.0, 874.0), "an iPhone 16 Pro upright");
    assert!(is_compact(360.0, 640.0), "a small phone");
    assert!(is_compact(914.0, 411.0), "a phone on its side is short");
    assert!(is_compact(844.0, 390.0));
    assert!(!is_compact(1280.0, 800.0), "a desktop");
    assert!(!is_compact(1024.0, 768.0), "a tablet");
    assert!(!is_compact(600.0, 500.0), "the limits belong to the desktop");
    assert!(is_compact(599.9, 900.0) && is_compact(900.0, 499.9));
    assert!(is_compact(f32::NAN, 800.0) && is_compact(800.0, f32::INFINITY), "a size that is no number: the layout that fits every screen");
}

#[test]
fn the_drawer_is_most_of_the_width_between_its_limits_and_leaves_scrim_to_tap() {
    assert!((drawer_width(411.0) - 345.24).abs() < 0.1, "84 percent of a Pixel 8");
    assert_eq!(drawer_width(360.0), 302.4_f32.max(DRAWER_MIN).min(320.0));
    assert_eq!(drawer_width(914.0), DRAWER_MAX, "never wider than the limit");
    assert_eq!(drawer_width(300.0), 260.0, "a window of 300 keeps 40 px of scrim: below the minimum, the scrim wins");
    assert_eq!(drawer_width(0.0), 0.0);
    assert_eq!(drawer_width(f32::NAN), 0.0);
    assert_eq!(drawer_width(-5.0), 0.0);
    for w in [200.0, 280.0, 320.0, 360.0, 411.0, 600.0] {
        assert!(drawer_width(w) <= (w - 40.0).max(0.0), "{w}: a strip of scrim stays");
    }
}

#[test]
fn the_key_row_shows_on_a_phone_with_a_file_once_the_editor_is_touched_or_the_keyboard_is_up() {
    assert!(key_row_visible(true, true, true, 0.0), "touched, no keyboard (a window of a phone's size on a desktop)");
    assert!(key_row_visible(true, true, false, 301.0), "the keyboard is up");
    assert!(!key_row_visible(true, true, false, 0.0), "neither");
    assert!(!key_row_visible(true, false, true, 301.0), "no file, nothing to type into");
    assert!(!key_row_visible(false, true, true, 301.0), "a desktop has its keyboard and no key row");
}

#[test]
fn opening_the_drawer_leaves_the_editor_and_closing_it_does_not_bring_the_row_back() {
    let mut p = PhoneState { editing: true, ..PhoneState::default() };
    p.toggle_drawer();
    assert!(p.drawer && !p.editing);
    p.toggle_drawer();
    assert!(!p.drawer && !p.editing);
}

#[test]
fn the_sheet_starts_at_half_and_the_numbers_are_finger_sized() {
    assert_eq!(PhoneState::default().sheet, Detent::Half);
    assert!(TOUCH >= 48.0 && TOP_BAR >= TOUCH && KEY_ROW >= TOUCH);
    assert!(DRAWER_MIN < DRAWER_MAX && STATUS_BAR < TOUCH);
}

#[test]
fn a_keyboard_that_was_up_and_is_gone_ends_the_editing_and_one_never_seen_does_not() {
    // a window of a phone's size on a desktop: touched, no keyboard ever: the row stays
    let mut p = PhoneState { editing: true, ..PhoneState::default() };
    p.observe(0.0);
    assert!(p.editing, "no keyboard was up, so none went away");
    // a phone: the keyboard comes up, then the user puts it away
    p.observe(301.0);
    assert!(p.keyboard_seen && p.editing);
    p.observe(301.0);
    assert!(p.editing, "still up");
    p.observe(0.0);
    assert!(!p.editing && !p.keyboard_seen, "gone: the editor is no longer being typed into");
    // and a touch afterwards starts it again
    p.editing = true;
    p.observe(0.0);
    assert!(p.editing, "the keyboard is not up and was not seen since: the touch stands");
}
