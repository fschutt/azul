//! The display features array of the Java bridge, read back. No device.

use super::*;

/// A Pixel 8 in portrait: a punch hole at the top middle, round corners, a visible status bar, a gesture navigation pill, an
/// under-display fingerprint sensor (its kind is not told).
fn pixel_8() -> Vec<i32> {
    let mut a = vec![VERSION, 1];
    a.extend([501, 24, 579, 102]); // the hole
    a.extend([120, 120, 120, 120]); // the corners
    a.extend([1, 0, 0, 1080, 118, 1]); // the status bar
    a.extend([1, 0, 2316, 1080, 2400, 1]); // the navigation bar
    a.push(0);
    a
}

#[test]
fn a_pixel_8_is_read_back_whole() {
    let f = decode(&pixel_8()).expect("a valid array");
    assert_eq!(f.cutouts, vec![Px { left: 501, top: 24, right: 579, bottom: 102 }]);
    assert_eq!(f.corners, [120; 4]);
    assert_eq!(f.status, Some(Bar { rect: Px { left: 0, top: 0, right: 1080, bottom: 118 }, visible: true }));
    assert_eq!(f.navigation, Some(Bar { rect: Px { left: 0, top: 2316, right: 1080, bottom: 2400 }, visible: true }));
    assert!(f.fingerprint);
}

/// A hidden status bar keeps its rect (an app that takes its place needs the band), an absent bar has none, no fingerprint sensor is
/// `-1`.
#[test]
fn a_hidden_bar_keeps_its_rect_and_an_absent_one_has_none() {
    let mut a = vec![VERSION, 0, 0, 0, 0, 0];
    a.extend([1, 0, 0, 1080, 118, 0]); // hidden
    a.extend([0, 0, 0, 0, 0, 0]); // absent
    a.push(-1);
    let f = decode(&a).unwrap();
    assert!(f.cutouts.is_empty());
    assert_eq!(f.status, Some(Bar { rect: Px { left: 0, top: 0, right: 1080, bottom: 118 }, visible: false }));
    assert_eq!(f.navigation, None);
    assert!(!f.fingerprint);
}

/// An array that is cut short, of another version, or with a count it cannot hold is no report at all.
///
/// Mutation: `decode` reading past the end (a panic in the loop thread on a half-written dispatch).
#[test]
fn a_cut_array_or_another_version_is_no_report() {
    let whole = pixel_8();
    for len in 0..whole.len() {
        assert_eq!(decode(&whole[..len]), None, "cut at {len}");
    }
    let mut other = pixel_8();
    other[0] = 2;
    assert_eq!(decode(&other), None);
    assert_eq!(decode(&[VERSION, i32::MAX]), None);
    assert_eq!(decode(&[VERSION, -1]), None);
}

/// A cutout with no area is dropped, and a negative radius is no radius.
#[test]
fn empty_cutouts_and_negative_radii_are_dropped() {
    let mut a = vec![VERSION, 2];
    a.extend([10, 10, 10, 50]); // no width
    a.extend([501, 24, 579, 102]);
    a.extend([-1, 5, 0, 7]);
    a.extend([0, 0, 0, 0, 0, 0]);
    a.extend([0, 0, 0, 0, 0, 0]);
    a.push(-1);
    let f = decode(&a).unwrap();
    assert_eq!(f.cutouts.len(), 1);
    assert_eq!(f.corners, [0, 5, 0, 7]);
}

/// Physical px become logical px with the window's scale; a punch hole is a circle, a wide slot a rounded rect.
#[test]
fn logical_px_and_the_shape_of_a_hole() {
    let hole = Px { left: 501, top: 24, right: 579, bottom: 102 };
    assert_eq!(hole.to_logical(2.625), (501.0 / 2.625, 24.0 / 2.625, 78.0 / 2.625, 78.0 / 2.625));
    assert_eq!(hole.to_logical(0.0), (501.0, 24.0, 78.0, 78.0), "no scale is scale 1");
    assert_eq!(guess_shape(hole), ShapeGuess::Circle);
    assert_eq!(guess_shape(Px { left: 400, top: 0, right: 680, bottom: 90 }), ShapeGuess::RoundedRect);
}
