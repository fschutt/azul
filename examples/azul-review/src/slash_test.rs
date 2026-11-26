//! The quick slash: which strokes are one. The strokes are the fixtures every gesture of the pen is tested on
//! (`stroke_fixtures`: a slash, a slash through a long word, loops, an underline, scribbles, a tap, a bar, a tick), their
//! mirrors (drawn the other way, downhill) and strokes of the test's own.

use super::*;
use crate::stroke_fixtures::{self as fx, Fixture, Meaning};

fn samples(f: &Fixture) -> Vec<Sample> {
    f.samples.iter().map(|s| Sample { x: s.x, y: s.y, t_ms: s.t_ms }).collect()
}

/// The same stroke drawn the other way: the last sample first, time running forward again.
fn reversed(s: &[Sample]) -> Vec<Sample> {
    let end = s.last().map_or(0, |l| l.t_ms);
    s.iter().rev().map(|p| Sample { t_ms: end - p.t_ms, ..*p }).collect()
}

/// The stroke mirrored about the level line `y = about`: uphill becomes downhill.
fn flipped(s: &[Sample], about: f32) -> Vec<Sample> {
    s.iter().map(|p| Sample { y: 2.0 * about - p.y, ..*p }).collect()
}

/// The same path over `ms`: drawn more slowly or faster.
fn over(s: &[Sample], ms: u32) -> Vec<Sample> {
    let end = f64::from(s.last().map_or(1, |l| l.t_ms).max(1));
    s.iter()
        .map(|p| Sample { t_ms: (f64::from(p.t_ms) * f64::from(ms) / end).round() as u32, ..*p })
        .collect()
}

/// A straight stroke of `n` samples from (x0, y0) to (x1, y1) over `ms`.
fn straight(x0: f32, y0: f32, x1: f32, y1: f32, n: usize, ms: u32) -> Vec<Sample> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            Sample { x: x0 + (x1 - x0) * t, y: y0 + (y1 - y0) * t, t_ms: (t * ms as f32).round() as u32 }
        })
        .collect()
}

#[test]
fn every_fixture_that_is_a_slash_is_one_and_no_other_fixture_is() {
    for f in fx::all() {
        let said = recognise(&samples(&f));
        assert_eq!(said.is_some(), f.meaning == Meaning::Slash, "{}: {said:?}", f.name);
    }
}

#[test]
fn the_slash_is_the_same_slash_drawn_the_other_way_or_downhill() {
    for f in [fx::slash(), fx::slash_through_a_long_word()] {
        let drawn = samples(&f);
        let level = (drawn[0].y + drawn[drawn.len() - 1].y) / 2.0;
        let forms = [
            ("as drawn", drawn.clone()),
            ("right to left", reversed(&drawn)),
            ("downhill", flipped(&drawn, level)),
            ("downhill, right to left", reversed(&flipped(&drawn, level))),
        ];
        let first = recognise(&drawn).expect("the fixture");
        for (what, form) in forms {
            let line = recognise(&form).unwrap_or_else(|| panic!("{} {what} is a slash", f.name));
            assert!(line.x0 <= line.x1, "{} {what}: left first", f.name);
            assert!((line.x0 - first.x0).abs() < 0.01 && (line.x1 - first.x1).abs() < 0.01, "{} {what}", f.name);
            assert_eq!(line.duration_ms, first.duration_ms, "{} {what}", f.name);
        }
    }
}

#[test]
fn what_a_slash_says_is_where_it_lies_from_left_to_right_and_how_long_it_took() {
    let line = recognise(&samples(&fx::slash())).expect("a slash");
    assert_eq!((line.x0, line.x1), (296.0, 372.0));
    assert!((line.y - 204.5).abs() < 0.01, "the middle of its height: {}", line.y);
    assert_eq!(line.duration_ms, 110);
    assert!((line.centre() - 334.0).abs() < 0.01);
}

#[test]
fn a_stroke_that_is_drawn_slowly_is_no_slash() {
    let quick = samples(&fx::slash());
    assert!(recognise(&over(&quick, 110)).is_some());
    // 76 px in 400 ms is a deliberate swipe of a marker, not a flick
    assert_eq!(recognise(&over(&quick, 400)), None);
    // slower than a flick, though inside the time: 76 px in 250 ms is 0.3 px per ms
    assert_eq!(recognise(&over(&quick, 250)), None, "speed counts as well as time");
    // an underline is drawn in a quarter of a second: slow for its length
    assert_eq!(recognise(&samples(&fx::underline())), None);
}

#[test]
fn a_long_stroke_is_a_slash_only_while_it_is_quick() {
    let long = |ms| straight(100.0, 200.0, 300.0, 202.0, 24, ms);
    assert!(recognise(&long(200)).is_some(), "200 px in 200 ms");
    assert!(recognise(&long(MAX_DURATION_MS)).is_some(), "200 px in the longest time");
    assert_eq!(recognise(&long(MAX_DURATION_MS + 10)), None, "too slow for a slash");
}

#[test]
fn a_stroke_quicker_than_any_hand_is_no_slash_it_is_a_test_or_a_script() {
    let quick = samples(&fx::slash());
    assert_eq!(recognise(&over(&quick, MIN_DURATION_MS - 1)), None);
    assert!(recognise(&over(&quick, MIN_DURATION_MS)).is_some());
    // no time recorded at all (an archive from before the times, a pointer that told none)
    assert_eq!(recognise(&over(&quick, 0)), None);
}

#[test]
fn a_stroke_must_have_some_length_but_not_more_than_a_line_of_code() {
    assert_eq!(recognise(&straight(100.0, 200.0, 100.0 + MIN_LENGTH_PX - 1.0, 200.0, 6, 40)), None);
    assert!(recognise(&straight(100.0, 200.0, 100.0 + MIN_LENGTH_PX, 200.0, 6, 40)).is_some());
    assert!(recognise(&straight(100.0, 200.0, 100.0 + MAX_LENGTH_PX, 200.0, 20, 200)).is_some());
    assert_eq!(recognise(&straight(100.0, 200.0, 100.0 + MAX_LENGTH_PX + 20.0, 200.0, 20, 200)), None);
}

#[test]
fn a_stroke_must_lie_in_the_band_of_one_line_a_slope_is_no_slash() {
    // 60 px long, in 90 ms, at an angle
    let at_deg = |deg: f32| {
        let (dx, dy) = (60.0 * deg.to_radians().cos(), 60.0 * deg.to_radians().sin());
        straight(100.0, 200.0, 100.0 + dx, 200.0 - dy, 8, 90)
    };
    assert!(recognise(&at_deg(0.0)).is_some());
    assert!(recognise(&at_deg(5.0)).is_some(), "5 px of climb stay in the line");
    assert_eq!(recognise(&at_deg(15.0)), None, "16 px of climb are more than a line of glyphs");
    assert_eq!(recognise(&at_deg(45.0)), None);
    assert_eq!(recognise(&samples(&fx::vertical_bar())), None);
    // a gentle slope of 12 degrees over 120 px climbs 25 px: two lines of text, not one; over 40 px it stays in one
    let slope = |len: f32, ms: u32| {
        let (dx, dy) = (len * 12f32.to_radians().cos(), len * 12f32.to_radians().sin());
        straight(100.0, 220.0, 100.0 + dx, 220.0 - dy, 10, ms)
    };
    assert_eq!(recognise(&slope(120.0, 150)), None);
    assert!(recognise(&slope(40.0, 70)).is_some());
}

#[test]
fn a_stroke_that_is_bent_is_no_slash() {
    // a flick that comes back (a hook), a "V", an arc: path much longer than the chord
    let mut hook = straight(100.0, 200.0, 160.0, 200.0, 8, 60);
    hook.extend(straight(160.0, 200.0, 120.0, 204.0, 6, 40).into_iter().skip(1).map(|p| Sample { t_ms: p.t_ms + 60, ..p }));
    assert_eq!(recognise(&hook), None);
    assert_eq!(recognise(&samples(&fx::open_c())), None);
    assert_eq!(recognise(&samples(&fx::check_mark())), None);
}

#[test]
fn a_shaky_line_is_a_slash_while_it_stays_straight_enough_and_not_when_it_wobbles() {
    // 100 px in 150 ms with a wobble of 1.5 px: a hand
    let wobble = |amp: f32| -> Vec<Sample> {
        (0..30)
            .map(|i| {
                let t = i as f32 / 29.0;
                Sample { x: 100.0 + 100.0 * t, y: 205.0 + amp * (t * 30.0).sin(), t_ms: (t * 150.0).round() as u32 }
            })
            .collect()
    };
    assert!(recognise(&wobble(1.5)).is_some());
    assert_eq!(recognise(&wobble(7.0)), None, "a scribble in a hurry: wider than the band, longer than the chord");
}

#[test]
fn too_few_samples_or_samples_that_are_no_numbers_are_no_slash() {
    assert_eq!(recognise(&[]), None);
    assert_eq!(recognise(&samples(&fx::tap())), None, "two samples");
    let mut bad = samples(&fx::slash());
    bad[3].x = f32::NAN;
    assert_eq!(recognise(&bad), None);
    let mut bad = samples(&fx::slash());
    bad[0].y = f32::INFINITY;
    assert_eq!(recognise(&bad), None);
}

#[test]
fn time_that_runs_backwards_is_no_time() {
    let mut s = samples(&fx::slash());
    s[4].t_ms = 5;
    assert_eq!(recognise(&s), None);
}
