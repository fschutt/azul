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

/// An app that hides a bar or draws into the cutout area gets new insets after its request; one that asks for neither does not, and its first
/// layout needs no wait.
///
/// Mutation: the default answering true (every app would wait for insets that never change).
#[test]
fn only_a_request_that_hides_a_bar_or_draws_into_the_cutout_moves_the_insets() {
    assert!(!bars_request_moves_insets(false, false, false), "the default asks nothing of the bars");
    assert!(bars_request_moves_insets(true, false, false), "a hidden status bar (AzWriter on a phone)");
    assert!(bars_request_moves_insets(false, true, false));
    assert!(bars_request_moves_insets(false, false, true), "the cutout area");
}

/// The wait for the insets after a request: returns the first answer that differs from the one before the request, `None` at the limit, and
/// never takes "nothing yet" for a change.
///
/// Mutation: the answer compared with nothing (the first dispatch ends the wait whatever it says).
#[test]
fn the_wait_ends_at_the_first_answer_that_differs_and_at_the_limit_otherwise() {
    use std::time::Duration;
    let tick = Duration::from_millis(1);
    // nothing yet, the old answer again, then a new one
    let mut answers = vec![None, Some(vec![128]), Some(vec![128]), Some(vec![0])].into_iter();
    let got = wait_for_change(&Some(vec![128]), Duration::from_secs(5), tick, || answers.next().flatten());
    assert_eq!(got, Some(vec![0]));
    // no dispatch before the request: any answer is a change
    let mut answers = vec![None, Some(7)].into_iter();
    assert_eq!(wait_for_change(&None, Duration::from_secs(5), tick, || answers.next().flatten()), Some(7));
    // an answer that never changes: the limit ends the wait
    let started = std::time::Instant::now();
    assert_eq!(wait_for_change(&Some(1), Duration::from_millis(30), tick, || Some(1)), None);
    assert!(started.elapsed() >= Duration::from_millis(30), "it waited for the limit");
    // the question is asked at least once, even with a limit of nothing
    let mut asked = 0;
    let _ = wait_for_change(&Some(1), Duration::ZERO, tick, || {
        asked += 1;
        Some(1)
    });
    assert_eq!(asked, 1);
}
