//! The first frame of a window runs no layout of its own where one has run.

use super::*;

/// The window the constructor laid out: its first frame builds the display list from that layout
/// and asks for none. The rule before 2026-11-26 asked whenever the display list was not
/// initialised yet, which on macOS ran the user's layout callback a second time before the first
/// present.
///
/// Mutation: the old rule (`!display_list_initialized` alone); the layout flag ignored.
#[test]
fn a_window_that_was_laid_out_at_creation_asks_for_no_second_layout() {
    assert!(!first_frame_owes_layout(false, true));
}

/// A window that never laid out (the constructor's layout failed) still owes the layout its first
/// frame needs: there is nothing to build a display list from.
///
/// Mutation: a rule that never asks.
#[test]
fn a_window_without_any_layout_asks_for_one() {
    assert!(first_frame_owes_layout(false, false));
}

/// After the first frame the question is closed: later layouts come from the requests that raise
/// them, whether or not a layout exists.
///
/// Mutation: a rule that asks again once the display list exists.
#[test]
fn a_window_past_its_first_frame_never_asks() {
    assert!(!first_frame_owes_layout(true, true));
    assert!(!first_frame_owes_layout(true, false));
}
