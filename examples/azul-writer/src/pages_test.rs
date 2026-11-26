//! The document area on a window narrower than a sheet of paper.

use super::shows_as_web;
use crate::app::View;

#[test]
fn a_phone_shows_the_web_layout_because_an_a4_sheet_does_not_fit_it() {
    // The sheet is 794 px at 100 %: it ran out of a 411 dp phone and its text with it.
    assert!(shows_as_web(View::Print, 411.0, 1.0));
    assert!(shows_as_web(View::Print, 360.0, 1.0));
    assert!(shows_as_web(View::Web, 1920.0, 1.0), "the web layout is always the web layout");
}

#[test]
fn a_window_wide_enough_for_the_sheet_shows_the_layout_that_was_asked_for() {
    assert!(!shows_as_web(View::Print, 1280.0, 1.0));
    assert!(!shows_as_web(View::Print, 842.0, 1.0), "794 px of sheet and 48 of canvas");
    assert!(shows_as_web(View::Print, 841.0, 1.0));
}

#[test]
fn zooming_in_makes_the_sheet_wider_and_a_window_that_held_it_may_not() {
    assert!(!shows_as_web(View::Print, 1280.0, 1.0));
    assert!(shows_as_web(View::Print, 1280.0, 1.6), "1270 px of sheet and 48 of canvas");
}
