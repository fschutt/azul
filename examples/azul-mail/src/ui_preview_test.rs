//! When the window has a Preview and what a dragged size is worth.

use super::*;

#[test]
fn the_preview_shows_only_in_the_mail_view_with_the_reading_pane_on_and_a_mailbox_to_read() {
    assert!(shows(true, 0, true));
    assert!(!shows(false, 0, true), "View > Reading Pane > Off");
    assert!(!shows(true, 1, true), "the Calendar module is no message");
    assert!(!shows(true, 0, false), "no account and no Local Folders: the list says all there is");
}

#[test]
fn a_dragged_size_is_kept_only_where_it_is_a_size() {
    assert_eq!(clamped_preview_size(320.0), 320.0);
    assert_eq!(clamped_preview_size(0.0), 0.0, "0 is the widget's default");
    assert_eq!(clamped_preview_size(-5.0), 0.0);
    assert_eq!(clamped_preview_size(f32::NAN), 0.0);
    assert_eq!(clamped_preview_size(f32::INFINITY), 0.0);
    assert_eq!(clamped_preview_size(1.0e9), PREVIEW_SIZE_MAX, "a size beyond the limit is the limit");
}
