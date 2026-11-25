//! What a stroke looks like: synthetic strokes drawn over a small page of code (the lines of the
//! page are 16 px high, a column is 8 px wide, line 10 starts at y 100 and the code at x 48), and the
//! rule table that turns a shape into a guess. No window, no model.

use super::*;
use crate::{
    agent_limits::{Limits, Notes},
    agent_review::PageGrid,
    agent_stroke::{shape, Sample},
};

fn grid() -> PageGrid {
    PageGrid {
        first_line: 10,
        origin_x: 48.0,
        origin_y: 100.0,
        line_height: 16.0,
        char_width: 8.0,
    }
}

fn context(with_grid: bool) -> Context {
    Context {
        file: "src/lib.rs".to_string(),
        page: 0,
        first_line: 10,
        lines: vec![
            "fn main() {".to_string(),
            "    let value = foo(1);".to_string(),
            "    // stale comment".to_string(),
            "}".to_string(),
        ],
        total_lines: 40,
        focus: None,
        words_under: Vec::new(),
        grid: with_grid.then(grid),
    }
}

fn stroke(id: u64, points: &[(f32, f32)], pressure: f32, ms_each: u32) -> StrokeInput {
    StrokeInput {
        id,
        page: 0,
        meaning: None,
        pinned: false,
        nib: "pen".to_string(),
        nib_width: 2.0,
        t0_ms: 0,
        samples: points
            .iter()
            .enumerate()
            .map(|(i, (x, y))| Sample {
                x: *x,
                y: *y,
                pressure,
                dt_ms: i as u32 * ms_each,
                tilt: None,
            })
            .collect(),
    }
}

/// `n` samples along the straight line from `a` to `b`.
fn line_pts(a: (f32, f32), b: (f32, f32), n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        })
        .collect()
}

/// `per_edge` samples along every edge of the path through `corners`.
fn poly_pts(corners: &[(f32, f32)], per_edge: usize) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::new();
    for pair in corners.windows(2) {
        let edge = line_pts(pair[0], pair[1], per_edge);
        out.extend(if out.is_empty() {
            edge
        } else {
            edge[1..].to_vec()
        });
    }
    out
}

fn circle_pts(centre: (f32, f32), r: f32, n: usize) -> Vec<(f32, f32)> {
    (0..=n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            (centre.0 + r * a.cos(), centre.1 + r * a.sin())
        })
        .collect()
}

/// A ring round the word "foo": it holds the middle of that word and of no other.
fn ring() -> Vec<(f32, f32)> {
    circle_pts((188.0, 124.0), 14.0, 32)
}

fn features_of(points: &[(f32, f32)], with_grid: bool) -> Features {
    let s = stroke(5, points, 0.5, 15);
    let mut notes = Notes::new();
    let sh = shape(&s, &Limits::default(), &mut notes);
    analyze(&s, &sh, &context(with_grid))
}

fn words(hits: &[WordHit]) -> Vec<&str> {
    hits.iter().map(|h| h.text.as_str()).collect()
}

// y of the line "    let value = foo(1);" (line 11) is 116 to 132; "foo" is at x 176 to 200

#[test]
fn a_level_line_just_under_a_word_is_an_underline_of_that_word() {
    let f = features_of(&line_pts((174.0, 130.4), (202.0, 130.4), 20), true);
    assert_eq!(f.hints, vec![Hint::Underline]);
    assert_eq!(words(&f.covered), vec!["foo"]);
    assert!(f.touched.is_empty());
    assert_eq!(f.lines, Some((11, 11)));
    assert!(f.crosses_text && !f.in_margin && !f.closed);
    assert!(f.straightness > 0.99);
}

#[test]
fn a_level_line_through_the_middle_of_a_word_is_a_strike_through() {
    let f = features_of(&line_pts((174.0, 124.0), (202.0, 124.0), 20), true);
    assert_eq!(f.hints, vec![Hint::StrikeThrough]);
    assert_eq!(words(&f.covered), vec!["foo"]);
}

#[test]
fn a_level_line_between_the_two_is_neither_but_it_touches_the_word() {
    let f = features_of(&line_pts((174.0, 119.2), (202.0, 119.2), 20), true);
    assert!(f.hints.is_empty(), "{:?}", f.hints);
    assert_eq!(words(&f.touched), vec!["foo"]);
    assert!(f.covered.is_empty());
}

#[test]
fn a_line_that_is_just_above_the_next_line_is_the_underline_of_the_one_before() {
    // y 132.8 is the top of line 12, 0.05 of its height: an underline of line 11
    let f = features_of(&line_pts((174.0, 132.8), (202.0, 132.8), 20), true);
    assert!(f.has(Hint::Underline));
    assert_eq!(words(&f.covered), vec!["foo"]);
}

#[test]
fn a_round_shape_with_its_ends_together_is_a_circle_that_covers_the_words_it_holds() {
    let f = features_of(&ring(), true);
    assert_eq!(f.hints, vec![Hint::Circle]);
    assert!(f.closed);
    assert_eq!(f.loops, 1);
    assert!((f.aspect - 1.0).abs() < 0.1, "{}", f.aspect);
    assert_eq!(words(&f.covered), vec!["foo"]);
    assert_eq!(f.lines, Some((10, 12)));
}

#[test]
fn a_shape_to_and_fro_over_a_small_place_is_a_scribble() {
    let zigzag: Vec<(f32, f32)> = (0..12)
        .map(|k| (if k % 2 == 0 { 176.0 } else { 200.0 }, 118.0 + k as f32))
        .collect();
    let f = features_of(&zigzag, true);
    assert!(f.has(Hint::Scribble), "{f:?}");
    assert!(f.reversals >= 4, "{}", f.reversals);
    assert!(!f.closed);
    assert_eq!(words(&f.touched), vec!["foo"]);
}

#[test]
fn a_tap_is_a_dot() {
    let f = features_of(&[(100.0, 100.0), (100.3, 100.2), (100.1, 100.4)], true);
    assert_eq!(f.hints, vec![Hint::Dot]);
    assert!(f.length_px < 2.0);
}

#[test]
fn a_tall_stroke_with_two_corners_and_its_ends_on_one_side_is_a_bracket_in_the_margin() {
    let f = features_of(
        &poly_pts(
            &[(44.0, 100.0), (36.0, 100.0), (36.0, 164.0), (44.0, 164.0)],
            8,
        ),
        true,
    );
    assert!(f.has(Hint::Bracket), "{f:?}");
    assert!(!f.has(Hint::VerticalBar));
    assert!(f.in_margin && f.has(Hint::Margin));
    assert_eq!(f.lines, Some((10, 13)), "the lines that were sent, no more");
}

#[test]
fn a_tall_straight_stroke_is_a_vertical_bar() {
    let f = features_of(&line_pts((40.0, 100.0), (40.0, 164.0), 12), true);
    assert!(f.has(Hint::VerticalBar), "{f:?}");
    assert!(f.has(Hint::Margin));
}

#[test]
fn a_line_with_a_head_is_an_arrow() {
    let f = features_of(
        &poly_pts(
            &[
                (100.0, 124.0),
                (160.0, 124.0),
                (150.0, 118.0),
                (160.0, 124.0),
                (150.0, 130.0),
            ],
            6,
        ),
        true,
    );
    assert_eq!(f.hints, vec![Hint::Arrow], "{f:?}");
    assert!(!f.closed);
}

#[test]
fn without_the_pages_grid_the_shapes_that_need_it_are_not_guessed_at() {
    let f = features_of(&line_pts((174.0, 130.4), (202.0, 130.4), 20), false);
    assert!(
        f.hints.is_empty(),
        "no underline without lines to be under: {:?}",
        f.hints
    );
    assert!(f.lines.is_none() && f.covered.is_empty() && !f.crosses_text);
    // the geometric ones stay
    let circle = features_of(&ring(), false);
    assert_eq!(circle.hints, vec![Hint::Circle]);
}

#[test]
fn a_stroke_off_the_lines_that_were_sent_has_no_lines() {
    let f = features_of(&line_pts((174.0, 400.0), (202.0, 400.0), 20), true);
    assert_eq!(f.lines, None);
    assert!(f.covered.is_empty());
}

#[test]
fn the_numbers_are_the_strokes_own() {
    let s = stroke(5, &line_pts((100.0, 200.0), (160.0, 200.0), 20), 0.5, 15);
    let mut notes = Notes::new();
    let sh = shape(&s, &Limits::default(), &mut notes);
    let f = analyze(&s, &sh, &context(true));
    assert_eq!(f.stroke, 5);
    assert!((f.length_px - 60.0).abs() < 1e-3);
    assert_eq!(f.duration_ms, 285);
    assert!((f.mean_pressure - 0.5).abs() < 1e-4 && (f.peak_pressure - 0.5).abs() < 1e-4);
    assert_eq!(f.bbox, [100.0, 200.0, 160.0, 200.0]);
    assert!(f.aspect >= 60.0, "a flat box is very wide: {}", f.aspect);
}

#[test]
fn a_stroke_with_nothing_readable_has_empty_features_and_no_panic() {
    let s = stroke(9, &[(f32::NAN, 1.0)], 0.5, 15);
    let mut notes = Notes::new();
    let sh = shape(&s, &Limits::default(), &mut notes);
    let f = analyze(&s, &sh, &context(true));
    assert!(f.hints.is_empty() && f.covered.is_empty() && f.length_px == 0.0);
}

#[test]
fn a_segment_touches_a_box_exactly_when_it_enters_it() {
    let bx = [10.0, 10.0, 20.0, 20.0];
    assert!(path_in_box(&[(0.0, 15.0), (30.0, 15.0)], bx), "through it");
    assert!(path_in_box(&[(15.0, 15.0), (16.0, 16.0)], bx), "inside it");
    assert!(path_in_box(&[(0.0, 0.0), (30.0, 30.0)], bx), "the diagonal");
    assert!(path_in_box(&[(15.0, 15.0)], bx), "a point inside");
    assert!(
        path_in_box(&[(20.0, 20.0), (25.0, 25.0)], bx),
        "the corner counts"
    );
    assert!(
        path_in_box(&[(15.0, 0.0), (15.0, 40.0)], bx),
        "a vertical through it"
    );
    assert!(!path_in_box(&[(0.0, 0.0), (30.0, 5.0)], bx), "above it");
    assert!(
        !path_in_box(&[(0.0, 25.0), (9.0, 12.0)], bx),
        "stops short of it"
    );
    assert!(
        !path_in_box(&[(25.0, 5.0), (25.0, 40.0)], bx),
        "a vertical beside it"
    );
    assert!(!path_in_box(&[(5.0, 5.0)], bx), "a point outside");
    assert!(!path_in_box(&[], bx), "nothing");
}

#[test]
fn a_stroke_a_million_pixels_long_costs_no_more_than_a_short_one() {
    // the longest a coordinate may be: both ends of a level line a page-width over
    let f = features_of(&line_pts((-1.0e6, 124.0), (1.0e6, 124.0), 20), true);
    assert!(f.has(Hint::StrikeThrough), "{:?}", f.hints);
    assert_eq!(words(&f.covered), vec!["let", "value", "foo", "1"]);
    assert!((f.length_px - 2.0e6).abs() < 1.0e3, "{}", f.length_px);
}

#[test]
fn a_page_of_many_lines_costs_only_the_lines_the_stroke_is_on() {
    let mut ctx = context(true);
    ctx.lines = (0..20_000)
        .map(|i| format!("    let value_{i} = foo({i});"))
        .collect();
    let s = stroke(5, &line_pts((174.0, 130.4), (202.0, 130.4), 20), 0.5, 15);
    let mut notes = Notes::new();
    let sh = shape(&s, &Limits::default(), &mut notes);
    let f = analyze(&s, &sh, &ctx);
    assert_eq!(f.lines, Some((11, 11)));
    assert!(f.has(Hint::Underline));
}

// ==== in words ====

#[test]
fn the_prompt_line_says_what_the_stroke_looks_like_and_what_it_is_on() {
    let s = stroke(5, &line_pts((174.0, 130.4), (202.0, 130.4), 20), 0.5, 15);
    let mut notes = Notes::new();
    let sh = shape(&s, &Limits::default(), &mut notes);
    let line = describe(&analyze(&s, &sh, &context(true)));
    assert_eq!(
        line,
        "- stroke 5: open; straight (1.00); 28 px long in 0.3 s; looks like: underline; covers \"foo\"; on line 11; pressure mean 0.50, peak 0.50"
    );
}

#[test]
fn the_prompt_line_counts_loops_and_turns_and_names_what_is_touched() {
    let f = features_of(&ring(), true);
    let said = describe(&f);
    assert!(
        said.contains("closed")
            && said.contains("1 loop;")
            && said.contains("looks like: circle")
            && said.contains("on lines 10 to 12"),
        "{said}"
    );
    let zigzag: Vec<(f32, f32)> = (0..12)
        .map(|k| (if k % 2 == 0 { 176.0 } else { 200.0 }, 118.0 + k as f32))
        .collect();
    let said = describe(&features_of(&zigzag, true));
    assert!(
        said.contains("turns back")
            && said.contains("passes through \"foo\"")
            && said.contains("winding"),
        "{said}"
    );
}

// ==== the offline default ====

fn with_hints(stroke: u64, hints: &[Hint]) -> Features {
    let mut f = features_of(&line_pts((0.0, 0.0), (1.0, 0.0), 3), false);
    f.stroke = stroke;
    f.hints = hints.to_vec();
    f
}

#[test]
fn the_rule_table_is_a_guess_never_above_a_half_and_has_each_hint_once() {
    assert!(RULES
        .iter()
        .all(|r| r.confidence > 0.0 && r.confidence <= 0.5));
    for (i, a) in RULES.iter().enumerate() {
        assert!(
            RULES.iter().skip(i + 1).all(|b| b.hint != a.hint),
            "{:?} twice",
            a.hint
        );
    }
}

#[test]
fn an_underline_the_reviewer_did_not_choose_a_meaning_for_means_scope_with_a_low_confidence() {
    let intent = offline_intent(&[with_hints(7, &[Hint::Underline])], &[]).expect("a rule fits");
    assert_eq!(intent.meaning, Some(Meaning::Scope));
    assert!((intent.confidence - 0.45).abs() < 1e-6);
    assert!(
        intent.rationale.contains("stroke 7") && intent.rationale.contains("underline"),
        "{}",
        intent.rationale
    );
}

#[test]
fn a_stroke_the_reviewer_chose_a_meaning_for_is_never_guessed_at() {
    assert_eq!(
        offline_intent(&[with_hints(7, &[Hint::Underline])], &[7]),
        None
    );
    let both = [
        with_hints(7, &[Hint::Underline]),
        with_hints(8, &[Hint::StrikeThrough]),
    ];
    let intent = offline_intent(&both, &[7]).expect("the other one");
    assert_eq!(intent.meaning, Some(Meaning::Issue));
}

#[test]
fn the_best_rule_among_the_hints_wins_and_no_hint_is_no_guess() {
    let both = [
        with_hints(1, &[Hint::StrikeThrough, Hint::Dot]),
        with_hints(2, &[Hint::Underline]),
    ];
    assert_eq!(
        offline_intent(&both, &[]).and_then(|i| i.meaning),
        Some(Meaning::Scope)
    );
    assert_eq!(offline_intent(&[with_hints(1, &[Hint::Margin])], &[]), None);
    assert_eq!(offline_intent(&[], &[]), None);
}

#[test]
fn every_hint_has_a_word() {
    for hint in [
        Hint::Dot,
        Hint::Underline,
        Hint::StrikeThrough,
        Hint::Circle,
        Hint::Scribble,
        Hint::Bracket,
        Hint::VerticalBar,
        Hint::Arrow,
        Hint::Margin,
    ] {
        assert!(!hint.word().is_empty());
    }
}

// ==== in one call ====

fn pinned_stroke(id: u64, points: &[(f32, f32)]) -> StrokeInput {
    StrokeInput {
        pinned: true,
        meaning: Some(Meaning::Praise),
        ..stroke(id, points, 0.5, 15)
    }
}

#[test]
fn the_offline_default_of_some_strokes_is_the_best_guess_of_those_the_reviewer_did_not_choose() {
    let underline = stroke(1, &line_pts((174.0, 130.4), (202.0, 130.4), 20), 0.5, 15);
    let strike = stroke(2, &line_pts((174.0, 124.0), (202.0, 124.0), 20), 0.5, 15);
    let intent = offline_intent_for(&[underline, strike], &context(true), &Limits::default())
        .expect("a rule fits");
    assert_eq!(
        intent.meaning,
        Some(Meaning::Scope),
        "the underline's rule is the stronger"
    );
    // the reviewer chose a meaning for the underline: only the strike-through is guessed at
    let chosen = pinned_stroke(1, &line_pts((174.0, 130.4), (202.0, 130.4), 20));
    let strike = stroke(2, &line_pts((174.0, 124.0), (202.0, 124.0), 20), 0.5, 15);
    let intent = offline_intent_for(&[chosen, strike], &context(true), &Limits::default())
        .expect("a rule fits");
    assert_eq!(intent.meaning, Some(Meaning::Issue));
}

#[test]
fn nothing_to_guess_at_is_no_guess() {
    let ctx = context(true);
    assert_eq!(offline_intent_for(&[], &ctx, &Limits::default()), None);
    let chosen = pinned_stroke(1, &line_pts((174.0, 130.4), (202.0, 130.4), 20));
    assert_eq!(
        offline_intent_for(&[chosen], &ctx, &Limits::default()),
        None
    );
    // a slanted stroke is no shape of the table
    let slanted = stroke(3, &line_pts((100.0, 100.0), (140.0, 150.0), 20), 0.5, 15);
    assert_eq!(
        offline_intent_for(&[slanted], &ctx, &Limits::default()),
        None
    );
}

#[test]
fn only_the_newest_strokes_are_read_as_a_request_sends_them() {
    let strike = |id| stroke(id, &line_pts((174.0, 124.0), (202.0, 124.0), 20), 0.5, 15);
    let slanted = |id| stroke(id, &line_pts((100.0, 100.0), (140.0, 150.0), 20), 0.5, 15);
    let limits = Limits {
        max_strokes: 2,
        ..Limits::default()
    };
    // the strike-through is the oldest of three: left out
    let strokes = [strike(1), slanted(2), slanted(3)];
    assert_eq!(offline_intent_for(&strokes, &context(true), &limits), None);
    let strokes = [slanted(1), slanted(2), strike(3)];
    assert!(offline_intent_for(&strokes, &context(true), &limits).is_some());
}
