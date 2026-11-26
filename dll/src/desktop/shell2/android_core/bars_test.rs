//! The icon colour of the system bars from what is rendered behind them. No device.

use super::*;

/// An RGBA frame of one colour, `width` x `height`.
fn frame(width: usize, height: usize, rgb: (u8, u8, u8)) -> Vec<u8> {
    let mut px = Vec::with_capacity(width * height * 4);
    for _ in 0..width * height {
        px.extend_from_slice(&[rgb.0, rgb.1, rgb.2, 255]);
    }
    px
}

#[test]
fn luma_runs_from_black_to_white() {
    assert_eq!(luma(0, 0, 0), 0);
    assert_eq!(luma(255, 255, 255), 255);
    assert!(luma(255, 0, 0) < luma(0, 255, 0), "green is brighter than red");
    assert!((100..140).contains(&luma(236, 236, 236)) == false, "#ECECEC is light");
}

/// A white page behind the status bar: the clock and the battery need DARK icons (the bar is "light").
/// The shell never said, so they were white on white.
///
/// Mutation: a bar that is never told; a dark page read as light.
#[test]
fn a_white_page_behind_a_bar_asks_for_dark_icons_and_a_black_one_for_light() {
    let white = frame(64, 40, (255, 255, 255));
    let black = frame(64, 40, (0, 0, 0));
    let l_white = band_luma(&white, 64 * 4, 64, (0, 10)).unwrap();
    let l_black = band_luma(&black, 64 * 4, 64, (0, 10)).unwrap();
    assert_eq!(l_white, 255);
    assert_eq!(l_black, 0);
    assert!(light_background(None, l_white));
    assert!(!light_background(None, l_black));
}

/// Each bar is read from its own rows: a light page with a dark toolbar at the top has dark
/// icons for the navigation bar and light ones for the status bar.
#[test]
fn each_bar_reads_its_own_rows() {
    let mut px = frame(64, 40, (255, 255, 255));
    for y in 0..8 {
        for x in 0..64 {
            let at = (y * 64 + x) * 4;
            px[at..at + 3].copy_from_slice(&[20, 20, 30]);
        }
    }
    let status = band_luma(&px, 64 * 4, 64, (0, 8)).unwrap();
    let navigation = band_luma(&px, 64 * 4, 64, (34, 40)).unwrap();
    assert!(!light_background(None, status), "a dark toolbar behind the status bar");
    assert!(light_background(None, navigation), "the white page behind the navigation bar");
}

#[test]
fn a_band_outside_the_frame_has_no_luma() {
    let px = frame(16, 4, (255, 255, 255));
    assert_eq!(band_luma(&px, 16 * 4, 16, (10, 20)), None);
    assert_eq!(band_luma(&px, 16 * 4, 16, (3, 3)), None, "an empty band");
    assert_eq!(band_luma(&[], 0, 0, (0, 4)), None);
}

/// A mid grey does not flip the icons from one frame to the next.
///
/// Mutation: one threshold at 128 with no memory.
#[test]
fn a_mid_grey_keeps_what_the_bar_had() {
    assert!(light_background(Some(true), 120));
    assert!(!light_background(Some(false), 140));
    assert!(light_background(None, 140));
    assert!(!light_background(None, 120));
    assert!(light_background(Some(false), 200), "clearly light flips it");
    assert!(!light_background(Some(true), 60), "clearly dark flips it");
}

/// The activity is told when something changed, once, for both bars at a time.
///
/// Mutation: told every frame; told only the bar that changed (the other would go back to its default).
#[test]
fn a_change_is_told_once() {
    let mut bars = BarsState::new();
    assert_eq!(bars.update(Some(255), Some(255)), Some((true, true)), "the first frame tells both");
    assert_eq!(bars.update(Some(250), Some(255)), None, "nothing changed");
    assert_eq!(bars.update(Some(10), Some(255)), Some((false, true)), "the status bar went dark, the other stays");
    assert_eq!(bars.update(Some(10), Some(255)), None);
    assert_eq!(bars.update(None, Some(0)), Some((false, false)), "a bar with no rows keeps what it had");
}

#[test]
fn a_recreated_activity_is_told_again() {
    let mut bars = BarsState::new();
    bars.update(Some(255), Some(255));
    bars.forget();
    assert_eq!(bars.update(Some(255), Some(255)), Some((true, true)));
}
