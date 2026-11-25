//! The lasso: which pen strokes are a loop round some code, and which lines of the page the loop
//! covers. On the strokes every gesture is tested on (`stroke_fixtures`), so a loop and a quick slash
//! are told apart by construction. No window, no azul.

use super::*;
use crate::stroke_fixtures::{self as fx, Fixture, Meaning};

fn pts(f: &Fixture) -> Vec<Pt> {
    f.samples.iter().map(|s| Pt::new(s.x, s.y)).collect()
}

#[test]
fn every_closed_fixture_is_a_lasso_and_no_other_stroke_is() {
    for f in fx::all() {
        let verdict = recognize(&pts(&f));
        if f.meaning == Meaning::ClosedShape {
            assert!(verdict.is_ok(), "{} should be a lasso, got {:?}", f.name, verdict.err());
        } else {
            assert!(verdict.is_err(), "{} ({:?}) is no lasso", f.name, f.meaning);
        }
    }
}

#[test]
fn a_slash_an_underline_and_a_scribble_say_why_they_are_no_lasso() {
    assert_eq!(recognize(&pts(&fx::slash())), Err(NotALasso::Straight));
    assert_eq!(recognize(&pts(&fx::slash_through_a_long_word())), Err(NotALasso::Straight));
    assert_eq!(recognize(&pts(&fx::underline())), Err(NotALasso::Straight));
    assert_eq!(recognize(&pts(&fx::vertical_bar())), Err(NotALasso::TooShort), "40 px: shorter than any loop");
    assert_eq!(recognize(&pts(&fx::scribble())), Err(NotALasso::Scribble));
    assert_eq!(recognize(&pts(&fx::crossing_scribble())), Err(NotALasso::Scribble));
    assert_eq!(recognize(&pts(&fx::open_c())), Err(NotALasso::Open));
    assert_eq!(recognize(&pts(&fx::tap())), Err(NotALasso::TooFew));
}

#[test]
fn a_loop_round_a_word_encloses_the_word_and_nothing_much_else() {
    let lasso = recognize(&pts(&fx::closed_loop())).expect("a lasso");
    let (l, t, r, b) = fx::WORD_BOX;
    for corner in [Pt::new(l, t), Pt::new(r, t), Pt::new(r, b), Pt::new(l, b), Pt::new(fx::WORD_CENTRE.0, fx::WORD_CENTRE.1)] {
        assert!(point_in_polygon(&lasso.polygon, corner), "{corner:?} is in the loop");
    }
    assert!(!point_in_polygon(&lasso.polygon, Pt::new(333.0, 260.0)), "the line below is out");
    assert!(!point_in_polygon(&lasso.polygon, Pt::new(333.0, 160.0)), "the line above is out");
    // About the ellipse of 55 x 24: pi * 55 * 24 = 4147.
    assert!((lasso.area - 4147.0).abs() < 300.0, "{}", lasso.area);
    assert!(lasso.bounds.w > 100.0 && lasso.bounds.h > 40.0 && lasso.bounds.h < 60.0, "{:?}", lasso.bounds);
}

#[test]
fn a_circle_that_crosses_its_own_beginning_is_the_loop_without_its_tail() {
    let lasso = recognize(&pts(&fx::circle_with_overlap())).expect("a lasso");
    assert_eq!(lasso.closure, Closure::Crossing);
    assert!((lasso.area - 4147.0).abs() < 400.0, "{}", lasso.area);
    let gap = recognize(&pts(&fx::closed_loop())).expect("a lasso");
    assert!(matches!(gap.closure, Closure::Gap(g) if g < 20.0), "{:?}", gap.closure);
}

#[test]
fn a_shaky_hand_does_not_make_the_outline_ragged() {
    let lasso = recognize(&pts(&fx::wobbly_loop())).expect("a lasso");
    assert!(lasso.polygon.len() <= LASSO.max_points, "{}", lasso.polygon.len());
    assert!(lasso.polygon.len() >= 8, "still round: {}", lasso.polygon.len());
}

#[test]
fn a_loop_round_a_long_thin_line_is_a_lasso_too() {
    let lasso = recognize(&pts(&fx::loop_around_a_line())).expect("a lasso");
    assert!(lasso.bounds.w > 400.0 && lasso.bounds.h < 40.0, "{:?}", lasso.bounds);
}

#[test]
fn what_is_too_small_or_too_big_or_not_a_number_is_told() {
    // A ring of 8 px: a dot with a tail.
    let tiny: Vec<Pt> = (0..20)
        .map(|i| {
            let a = i as f32 / 19.0 * std::f32::consts::TAU;
            Pt::new(100.0 + 4.0 * a.cos(), 100.0 + 4.0 * a.sin())
        })
        .collect();
    assert!(matches!(recognize(&tiny), Err(NotALasso::TooShort | NotALasso::TooSmall)));
    // A ring as large as a page of a thousand px: a lasso, but not past the cap.
    let huge: Vec<Pt> = (0..200)
        .map(|i| {
            let a = i as f32 / 199.0 * std::f32::consts::TAU;
            Pt::new(5000.0 + 3000.0 * a.cos(), 5000.0 + 3000.0 * a.sin())
        })
        .collect();
    assert_eq!(recognize(&huge), Err(NotALasso::TooLong));
    // Samples that are not numbers are left out, not followed.
    let mut hurt = pts(&fx::closed_loop());
    hurt.insert(5, Pt::new(f32::NAN, 3.0));
    hurt.insert(9, Pt::new(1.0, f32::INFINITY));
    assert!(recognize(&hurt).is_ok());
    assert_eq!(recognize(&[]), Err(NotALasso::TooFew));
}

#[test]
fn a_stroke_of_a_hundred_thousand_samples_is_thinned_before_it_is_looked_at() {
    let big: Vec<Pt> = (0..100_000)
        .map(|i| {
            let a = i as f32 / 99_999.0 * std::f32::consts::TAU * 0.98;
            Pt::new(333.0 + 55.0 * a.cos(), 207.0 + 24.0 * a.sin())
        })
        .collect();
    let began = std::time::Instant::now();
    let lasso = recognize(&big).expect("a lasso");
    assert!(lasso.polygon.len() <= LASSO.max_points);
    assert!(began.elapsed().as_secs() < 5, "{:?}", began.elapsed());
}

#[test]
fn a_loop_reads_the_same_drawn_the_other_way_round() {
    let mut backwards = pts(&fx::closed_loop());
    backwards.reverse();
    let (a, b) = (recognize(&pts(&fx::closed_loop())).expect("a lasso"), recognize(&backwards).expect("a lasso"));
    assert!((a.area - b.area).abs() < 0.02 * a.area, "{} and {}", a.area, b.area);
}

// ---- the lines under the loop

/// A page of 46 lines of 40 characters from line 1, the code beginning at x 84, a line 15 px tall from y 32.
fn page_of<'a>(chars: &'a [usize], first_number: u32) -> PageLines<'a> {
    PageLines { first_number, chars, top: 32.0, line_h: 15.0, text_x: 84.0, char_w: 6.6, max_w: 600.0 }
}

fn rect_poly(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Pt> {
    vec![Pt::new(x0, y0), Pt::new(x1, y0), Pt::new(x1, y1), Pt::new(x0, y1)]
}

#[test]
fn a_loop_covers_the_lines_it_has_most_of_inside() {
    let chars = vec![40_usize; 46];
    let page = page_of(&chars, 1);
    // Lines 3, 4, 5 are y 62 to 107.
    let range = covered_lines(&rect_poly(70.0, 61.0, 400.0, 108.0), &page).expect("lines");
    assert_eq!((range.first, range.last), (3, 5));
}

#[test]
fn a_line_counts_when_half_of_its_text_is_inside() {
    let chars = vec![40_usize; 46];
    let page = page_of(&chars, 1);
    // Lines 3 to 5, and the top 45% of line 6 (y 107 to 122): not enough for line 6...
    let under = covered_lines(&rect_poly(70.0, 61.0, 400.0, 107.0 + 15.0 * 0.45), &page).expect("lines");
    assert_eq!(under.last, 5);
    // ... and 55% of it is.
    let over = covered_lines(&rect_poly(70.0, 61.0, 400.0, 107.0 + 15.0 * 0.55), &page).expect("lines");
    assert_eq!(over.last, 6);
    // The same for the width: a loop that stops half way along the text of every line.
    let half = covered_lines(&rect_poly(70.0, 61.0, 84.0 + 40.0 * 6.6 * 0.45, 108.0), &page);
    assert_eq!(half.map(|r| (r.first, r.last)), Some((3, 5)), "no line has half, so the lines with the characters wholly inside");
}

#[test]
fn a_loop_round_a_few_words_of_one_line_covers_that_line() {
    let chars = vec![80_usize; 46];
    let page = page_of(&chars, 1);
    // A loop round 12 characters in the middle of line 10 (y 167 to 182): 15% of the line's text.
    let range = covered_lines(&rect_poly(200.0, 165.0, 280.0, 184.0), &page).expect("a line");
    assert_eq!((range.first, range.last, range.best), (10, 10, 10));
}

#[test]
fn the_numbers_are_those_of_the_file_not_of_the_page() {
    let chars = vec![40_usize; 46];
    let page = page_of(&chars, 47);
    let range = covered_lines(&rect_poly(70.0, 61.0, 400.0, 108.0), &page).expect("lines");
    assert_eq!((range.first, range.last), (49, 51));
}

#[test]
fn blank_lines_at_the_edges_of_the_loop_are_not_part_of_what_it_shows() {
    let mut chars = vec![40_usize; 46];
    chars[2] = 0; // line 3
    chars[5] = 0; // line 6
    let page = page_of(&chars, 1);
    // Lines 3 to 6 all inside: the blank ones at both ends are left out; a blank one inside would stay.
    let range = covered_lines(&rect_poly(70.0, 61.0, 400.0, 123.0), &page).expect("lines");
    assert_eq!((range.first, range.last), (4, 5));
    chars[3] = 0; // line 4 blank too: only line 5 is left
    let page = page_of(&chars, 1);
    let range = covered_lines(&rect_poly(70.0, 61.0, 400.0, 123.0), &page).expect("lines");
    assert_eq!((range.first, range.last), (5, 5));
}

#[test]
fn a_loop_in_the_margin_or_below_the_last_line_covers_nothing() {
    let chars = vec![40_usize; 10];
    let page = page_of(&chars, 1);
    assert_eq!(covered_lines(&rect_poly(900.0, 40.0, 980.0, 100.0), &page), None, "right of the text");
    assert_eq!(covered_lines(&rect_poly(10.0, 40.0, 60.0, 100.0), &page), None, "in the gutter");
    assert_eq!(covered_lines(&rect_poly(70.0, 400.0, 400.0, 600.0), &page), None, "under the 10 lines");
    assert_eq!(covered_lines(&[], &page), None);
}

#[test]
fn a_loop_over_the_end_of_the_page_stops_at_the_last_line() {
    let chars = vec![40_usize; 10];
    let page = page_of(&chars, 1);
    let range = covered_lines(&rect_poly(70.0, 150.0, 400.0, 600.0), &page).expect("lines");
    assert_eq!((range.first, range.last), (9, 10));
}

#[test]
fn a_real_loop_round_a_word_covers_the_words_line() {
    let chars = vec![55_usize; 46];
    let page = page_of(&chars, 1);
    let lasso = recognize(&pts(&fx::closed_loop())).expect("a lasso");
    let range = covered_lines(&lasso.polygon, &page).expect("lines");
    assert_eq!((range.first, range.last), (fx::WORD_LINE.0, fx::WORD_LINE.0), "the loop shows parts of the lines around, but is about the word's line");
    let block = recognize(&pts(&fx::loop_around_a_block())).expect("a lasso");
    let range = covered_lines(&block.polygon, &page).expect("lines");
    // The ellipse is 124 px tall (lines 15 to 22) and narrows at its ends: the lines with half their text inside
    // are the five in the middle.
    assert!((15..=17).contains(&range.first) && (19..=21).contains(&range.last), "{range:?}");
}
