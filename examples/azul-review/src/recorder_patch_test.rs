//! The probe's looking: which pixels are the patch's, and when a run has seen the window. No screen.

use super::*;

fn picture(pixels: &[[u8; 4]]) -> Vec<u8> {
    pixels.iter().flatten().copied().collect()
}

const MAGENTA: [u8; 4] = [255, 0, 255, 255];

/// Magenta is counted, with what a capture and a scaling make of it, and nothing else is: not red, not blue, not
/// white, not a dark pixel, not a purple that is a window's shadow.
///
/// Mutation: a threshold that takes red; one that takes white; one that takes only exact magenta.
#[test]
fn magenta_is_counted_with_what_a_scaling_makes_of_it_and_nothing_else_is() {
    let seen = [MAGENTA, [250, 10, 240, 255], [210, 60, 205, 255], [255, 69, 255, 0]];
    assert_eq!(patch_pixels(&picture(&seen)), 4, "magenta, a little off, at the edge of the tolerance, and with any alpha");
    let not_seen = [[255, 0, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255], [0, 0, 0, 255], [120, 40, 120, 255], [199, 0, 255, 255], [255, 0, 199, 255], [255, 70, 255, 255]];
    assert_eq!(patch_pixels(&picture(&not_seen)), 0);
}

/// A picture is counted by its pixels: an empty one has none, and bytes that do not make a whole pixel at the end are
/// not one.
///
/// Mutation: a trailing byte read as a pixel.
#[test]
fn a_picture_is_counted_by_its_whole_pixels() {
    assert_eq!(patch_pixels(&[]), 0);
    let mut bytes = picture(&[MAGENTA, MAGENTA]);
    bytes.extend_from_slice(&[255, 0, 255]);
    assert_eq!(patch_pixels(&bytes), 2);
    assert_eq!(patch_pixels(&vec![0, 0, 0, 255].repeat(1_000)), 0);
}

/// A run has seen the window when it looked at frames and a tenth of them had the patch; no frames is no sight, and
/// a window seen in a few frames of many is not.
///
/// Mutation: no frames counted as seen; a tenth taken as a hundredth.
#[test]
fn a_run_saw_the_window_when_a_tenth_of_its_frames_had_the_patch() {
    assert!(!window_seen(0, 0), "no frames, no sight");
    assert!(!window_seen(0, 5), "a count with no frames is nonsense, not sight");
    assert!(window_seen(100, 100));
    assert!(window_seen(100, 10), "exactly a tenth");
    assert!(!window_seen(100, 9));
    assert!(!window_seen(100, 0), "the window was not in the picture");
    assert!(window_seen(10, 1));
    assert!(window_seen(1, 1) && !window_seen(1, 0));
    assert!(!window_seen(u32::MAX, 1), "no overflow, no sight");
}
