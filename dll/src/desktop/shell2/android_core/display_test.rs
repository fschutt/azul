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

/// The engine asks Java for the window's first insets when its surface is created: five insets, then the features array. The split gives
/// back what [`decode`] reads, and a snapshot with no features (a platform that refused them) is the insets alone.
///
/// Mutation: the split taken at 4 (the keyboard's inset read as the version of the features).
#[test]
fn the_first_insets_snapshot_is_five_insets_then_the_features_array() {
    let mut snapshot = vec![128, 126, 0, 0, 0];
    snapshot.extend(pixel_8());
    let (insets, features) = split_snapshot(&snapshot).expect("a snapshot");
    assert_eq!(insets, [128, 126, 0, 0, 0]);
    assert_eq!(features, pixel_8().as_slice());
    assert!(decode(features).is_some(), "the rest is the array the decoder reads");

    let (insets, features) = split_snapshot(&[48, 126, 0, 0, 0]).expect("insets alone");
    assert_eq!(insets, [48, 126, 0, 0, 0]);
    assert!(features.is_empty());

    assert!(split_snapshot(&[1, 2, 3, 4]).is_none(), "too short to hold the insets");
    assert!(split_snapshot(&[]).is_none());
}

/// The system applies what the app asked of the system bars in the window's first dispatches: the very first shows the window before the request (the
/// status bar still shown), the next one, a few milliseconds later, the bar hidden. A bar the app asked to hide that the snapshot still shows, or says
/// nothing of, is a request the snapshot does not show yet; an app that asks nothing never waits.
///
/// Mutation: the answer true whenever the app asks for something (the gate that skipped the snapshot for `AzWriter` although it showed the request done).
#[test]
fn a_request_the_snapshot_shows_done_moves_nothing() {
    let bar = |visible| Some(Bar { rect: Px { left: 0, top: 0, right: 1080, bottom: 128 }, visible });
    let features = |status, navigation| DisplayFeaturesPx { cutouts: vec![], corners: [0; 4], status, navigation, fingerprint: false };
    let shown = features(bar(true), bar(true));
    let hidden = features(bar(false), bar(true));
    assert!(!bars_request_moves_insets(Some(&shown), false, false, false), "an app that asks nothing never waits");
    assert!(!bars_request_moves_insets(None, false, false, false), "nor without features");
    assert!(bars_request_moves_insets(Some(&shown), true, false, false), "the status bar the app hides, still shown in the snapshot of the first dispatch");
    assert!(!bars_request_moves_insets(Some(&hidden), true, false, false), "the next dispatch shows it hidden: the request is in the insets");
    assert!(bars_request_moves_insets(Some(&hidden), true, true, false), "the navigation bar the app hides is still shown");
    assert!(!bars_request_moves_insets(Some(&features(bar(false), bar(false))), true, true, false));
    assert!(bars_request_moves_insets(Some(&features(None, bar(true))), true, false, false), "a snapshot that says nothing of the bar does not show it hidden");
    assert!(!bars_request_moves_insets(Some(&shown), false, false, true), "the cutout area: the mode is set before the window's first dispatch, the features show the cutout");
    assert!(bars_request_moves_insets(None, true, false, false), "no features at all: as before, the request is a reason to wait");
    assert!(bars_request_moves_insets(None, false, false, true), "the cutout area without features");
}
