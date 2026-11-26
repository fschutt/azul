//! Keeping a window out of screen captures: when a shell tells the system, and the values the platforms
//! take. No window, no system call.

use azul_core::window::WindowFlags;

use super::*;

fn flags(excluded: bool) -> WindowFlags {
    WindowFlags { exclude_from_capture: excluded, ..WindowFlags::default() }
}

/// A window that does not ask is left as the system made it (nothing is told at its creation), and one that
/// asks is told.
///
/// Mutation: a new window that is told `false` (a window the system made capturable is made so again); one
/// that asks and is not told.
#[test]
fn a_new_window_that_does_not_ask_is_not_touched_and_one_that_asks_is_told() {
    assert_eq!(exclusion_to_apply(None, &flags(false)), None);
    assert_eq!(exclusion_to_apply(None, &flags(true)), Some(true));
}

/// The system is told when the flag changes, in both directions, and not when it stays.
///
/// Mutation: a change in one direction that is not told; a flag that stays and is told again at every pass.
#[test]
fn a_change_of_the_flag_is_told_in_both_directions_and_no_change_is_not() {
    assert_eq!(exclusion_to_apply(Some(&flags(false)), &flags(true)), Some(true));
    assert_eq!(exclusion_to_apply(Some(&flags(true)), &flags(false)), Some(false));
    assert_eq!(exclusion_to_apply(Some(&flags(true)), &flags(true)), None);
    assert_eq!(exclusion_to_apply(Some(&flags(false)), &flags(false)), None);
}

/// Another flag changing is none of the capture's business.
#[test]
fn another_flag_changing_tells_nothing() {
    let before = flags(true);
    let after = WindowFlags { is_always_on_top: true, ..flags(true) };
    assert_eq!(exclusion_to_apply(Some(&before), &after), None);
    let before = flags(false);
    let after = WindowFlags { is_always_on_top: true, ..flags(false) };
    assert_eq!(exclusion_to_apply(Some(&before), &after), None);
}

/// A window is capturable unless it asks, and the flag is part of what makes two sets of flags equal (a shell
/// that compares the flags before and after would never see the change otherwise).
#[test]
fn the_flag_is_off_by_default_and_part_of_the_flags_identity() {
    assert!(!WindowFlags::default().exclude_from_capture);
    assert_ne!(flags(true), flags(false));
}

/// The values the platforms take: macOS `NSWindowSharingType` (`NSWindowSharingNone` 0, `NSWindowSharingReadOnly` 1,
/// the system's default) and Windows `SetWindowDisplayAffinity` (`WDA_EXCLUDEFROMCAPTURE` 0x11, `WDA_NONE` 0).
///
/// Mutation: the two swapped; a window that is shown again given the affinity that excludes it.
#[test]
fn the_values_the_platforms_take_are_the_systems() {
    assert_eq!(macos_sharing_type(true), 0);
    assert_eq!(macos_sharing_type(false), 1);
    assert_eq!(windows_display_affinity(true), 0x11);
    assert_eq!(windows_display_affinity(false), 0);
}
