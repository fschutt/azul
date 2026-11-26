//! Which strokes an agent is told of for a conversation, and how each is handed over. Pure: notes and strokes in, strokes out.

use super::*;
use crate::{
    conversation::Conversation,
    dictation::HeardWord,
    model::{InkPoint, NoteState},
};

fn note(page: usize, x: f32, y: f32) -> Note {
    Note { id: 1, page, x, y, text: "this".to_string(), language: "en".to_string(), state: NoteState::Done, talk: Conversation::default() }
}

fn point(x: f32, y: f32, t_ms: u32) -> InkPoint {
    InkPoint { x, y, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0, t_ms }
}

fn stroke(id: u64, page: usize, semantic: Semantic, started: u64, points: &[(f32, f32)]) -> Stroke {
    Stroke {
        page,
        semantic,
        points: points.iter().enumerate().map(|(i, (x, y))| point(*x, *y, i as u32 * 10)).collect(),
        id,
        epoch: 0,
        started_unix_ms: started,
    }
}

fn said_at(note: &mut Note, started: u64, first_ms: u32, last_ms: u32) {
    note.talk.said.push(crate::conversation::Said {
        text: "this loop".to_string(),
        started_unix_ms: started,
        words: vec![
            HeardWord { text: "this".to_string(), start_ms: first_ms, end_ms: first_ms + 200, probability: 0.9, biased: false },
            HeardWord { text: "loop".to_string(), start_ms: last_ms - 200, end_ms: last_ms, probability: 0.9, biased: false },
        ],
    });
    note.talk.understood.push(None);
}

fn ids(picked: &[StrokeInput]) -> Vec<u64> {
    picked.iter().map(|s| s.id).collect()
}

#[test]
fn the_ink_near_the_anchor_on_its_page_is_what_the_agent_is_told_of() {
    let n = note(0, 300.0, 200.0);
    let near = stroke(1, 0, Semantic::Issue, 0, &[(250.0, 190.0), (330.0, 195.0)]);
    let on_it = stroke(2, 0, Semantic::Scope, 0, &[(290.0, 205.0), (320.0, 205.0)]);
    let far = stroke(3, 0, Semantic::Issue, 0, &[(900.0, 600.0), (950.0, 620.0)]);
    let other_page = stroke(4, 1, Semantic::Issue, 0, &[(300.0, 200.0), (310.0, 200.0)]);
    let picked = pick(&n, &[near, on_it, far, other_page]);
    assert_eq!(ids(&picked), vec![1, 2], "the near ones, in the order they were drawn: the far one and the other page's are not");
}

#[test]
fn how_near_is_the_distance_from_the_anchor_to_the_box_of_the_stroke_not_to_its_first_point() {
    let n = note(0, 300.0, 200.0);
    // a long underline that begins far to the left and passes under the anchor
    let long = stroke(1, 0, Semantic::Scope, 0, &[(20.0, 230.0), (200.0, 230.0), (400.0, 230.0), (700.0, 230.0)]);
    assert_eq!(ids(&pick(&n, &[long])), vec![1]);
    // just past the reach
    let past = stroke(2, 0, Semantic::Scope, 0, &[(300.0, 200.0 + NEAR_PX + 5.0), (340.0, 200.0 + NEAR_PX + 5.0)]);
    assert!(pick(&n, &[past]).is_empty());
}

#[test]
fn ink_drawn_while_the_words_were_said_is_about_them_wherever_it_is() {
    let mut n = note(0, 300.0, 200.0);
    // the dictation began at 1_000_000 and said its words between 500 and 3 000 ms into it
    said_at(&mut n, 1_000_000, 500, 3_000);
    let during = stroke(1, 0, Semantic::Issue, 1_001_000, &[(900.0, 600.0), (950.0, 620.0)]);
    let before = stroke(2, 0, Semantic::Issue, 1_000_000 + 500 - SLACK_MS - 2_000, &[(900.0, 500.0), (950.0, 520.0)]);
    let after = stroke(3, 0, Semantic::Issue, 1_000_000 + 3_000 + SLACK_MS + 2_000, &[(900.0, 400.0), (950.0, 420.0)]);
    let slack = stroke(4, 0, Semantic::Issue, 1_000_000 + 3_000 + SLACK_MS - 500, &[(900.0, 300.0), (950.0, 320.0)]);
    let untimed = stroke(5, 0, Semantic::Issue, 0, &[(900.0, 200.0), (950.0, 220.0)]);
    let picked = pick(&n, &[during, before, after, slack, untimed]);
    assert_eq!(ids(&picked), vec![1, 4], "the ones drawn during the words and within the slack after them, not the others");
}

#[test]
fn a_note_that_was_typed_has_no_time_and_only_the_place_counts() {
    let n = note(0, 300.0, 200.0);
    let timed = stroke(1, 0, Semantic::Issue, 5_000_000, &[(900.0, 600.0), (950.0, 620.0)]);
    assert!(pick(&n, &[timed]).is_empty());
}

#[test]
fn at_most_a_dozen_strokes_the_ones_drawn_during_the_words_first_and_then_the_nearest_in_the_order_they_were_drawn() {
    let mut n = note(0, 300.0, 200.0);
    said_at(&mut n, 1_000_000, 500, 3_000);
    let mut all = Vec::new();
    // fifteen near strokes, each a little further than the one before
    for i in 0..15u64 {
        let y = 200.0 + 2.0 * i as f32;
        all.push(stroke(10 + i, 0, Semantic::Scope, 0, &[(250.0 + 8.0 * i as f32, y), (270.0 + 8.0 * i as f32, y)]));
    }
    // one far away that was drawn during the words
    all.push(stroke(100, 0, Semantic::Issue, 1_001_500, &[(950.0, 650.0), (960.0, 655.0)]));
    let picked = pick(&n, &all);
    assert_eq!(picked.len(), MAX_STROKES);
    assert!(ids(&picked).contains(&100), "the one drawn during the words is kept: {:?}", ids(&picked));
    let mut sorted = ids(&picked);
    sorted.sort_unstable();
    assert_eq!(ids(&picked), sorted, "in the order they were drawn");
    // of the near ones the nearest are the ones that stay (the first eleven)
    assert!(ids(&picked).contains(&10) && !ids(&picked).contains(&24), "{:?}", ids(&picked));
}

#[test]
fn a_stroke_is_handed_over_with_its_nib_its_meaning_and_every_sample_it_has() {
    let n = note(0, 100.0, 100.0);
    let mut marker = stroke(7, 0, Semantic::Scope, 1_234_567, &[(90.0, 100.0), (120.0, 101.0), (150.0, 102.0)]);
    marker.points[1].tilt_x = 12.0;
    marker.points[1].tilt_y = -3.0;
    let pen = stroke(8, 0, Semantic::Question, 1_234_999, &[(100.0, 120.0), (105.0, 125.0)]);
    let picked = pick(&n, &[marker, pen]);
    let (m, p) = (&picked[0], &picked[1]);
    assert_eq!((m.id, m.page, m.t0_ms), (7, 0, 1_234_567));
    assert_eq!(m.nib, "marker");
    assert_eq!(m.meaning, Some(Meaning::Scope));
    assert!(!m.pinned, "the marker is what the window starts with: nothing says the reviewer chose it");
    assert!(m.nib_width > p.nib_width, "the marker's nib is broader than the pen's");
    assert_eq!(m.samples.len(), 3);
    assert_eq!((m.samples[1].x, m.samples[1].y, m.samples[1].dt_ms), (120.0, 101.0, 10));
    assert_eq!(m.samples[1].tilt, Some((12.0, -3.0)));
    assert_eq!(m.samples[0].tilt, None, "no tilt where the pointer gave none");
    assert!((m.samples[0].pressure - 0.5).abs() < 1e-6);
    assert_eq!(p.nib, "pen");
    assert_eq!(p.meaning, Some(Meaning::Question));
    assert!(p.pinned, "a meaning other than the marker's was chosen: the agent does not override it");
}

#[test]
fn every_meaning_of_a_pen_is_the_agents() {
    for (semantic, meaning) in [
        (Semantic::Scope, Meaning::Scope),
        (Semantic::Issue, Meaning::Issue),
        (Semantic::Question, Meaning::Question),
        (Semantic::Duplicate, Meaning::Duplicate),
        (Semantic::Praise, Meaning::Praise),
    ] {
        assert_eq!(meaning_of(semantic), meaning);
    }
}

#[test]
fn a_stroke_with_nothing_in_it_or_nothing_that_is_a_number_is_never_a_panic() {
    let n = note(0, 100.0, 100.0);
    let empty = stroke(1, 0, Semantic::Issue, 0, &[]);
    let nan = Stroke {
        points: vec![InkPoint { x: f32::NAN, y: f32::NAN, ..point(0.0, 0.0, 0) }, point(100.0, 100.0, 5)],
        ..stroke(2, 0, Semantic::Issue, 0, &[])
    };
    let picked = pick(&n, &[empty, nan]);
    assert_eq!(ids(&picked), vec![2], "the one with a number in it is kept (the agent's own cleaning drops the one that is none)");
    let anchor_nan = note(0, f32::NAN, f32::INFINITY);
    let _ = pick(&anchor_nan, &[stroke(3, 0, Semantic::Issue, 0, &[(1.0, 1.0), (2.0, 2.0)])]);
}

// ---- the words under the strokes ----

fn grid() -> crate::agent_review::PageGrid {
    crate::ui::page_grid(1)
}

fn input(id: u64, samples: &[(f32, f32)]) -> StrokeInput {
    StrokeInput {
        id,
        page: 0,
        meaning: Some(Meaning::Scope),
        pinned: false,
        nib: "marker".to_string(),
        nib_width: 18.0,
        t0_ms: 0,
        samples: samples.iter().enumerate().map(|(i, (x, y))| Sample { x: *x, y: *y, pressure: 0.5, dt_ms: i as u32 * 10, tilt: None }).collect(),
    }
}

/// The page's x of column `col` (a fraction of a column is a place inside it).
fn x_of(col: f32) -> f32 {
    grid().origin_x + col * grid().char_width
}

/// The page's y of the middle of row `row`.
fn y_of(row: usize) -> f32 {
    grid().origin_y + (row as f32 + 0.5) * grid().line_height
}

fn code() -> Vec<String> {
    vec!["pub fn step_6(x: u32) -> u32 { x + 6 }".to_string(), "{ }".to_string(), "let a = b;".to_string()]
}

fn texts(words: &[WordRef]) -> Vec<(u64, usize, usize, &str)> {
    words.iter().map(|w| (w.stroke, w.line, w.col, w.text.as_str())).collect()
}

#[test]
fn a_stroke_over_a_word_is_on_that_word_and_the_columns_are_the_bytes_of_its_line() {
    let words = words_under(&[input(7, &[(x_of(7.5), y_of(0)), (x_of(12.5), y_of(0))])], &grid(), &code());
    assert_eq!(texts(&words), vec![(7, 1, 7, "step_6")], "line 1, byte 7 of it");
}

#[test]
fn a_stroke_across_several_words_is_on_each_in_the_order_it_passes_them_even_where_its_samples_are_far_apart() {
    // two samples a whole line apart: nothing between them is skipped
    let words = words_under(&[input(1, &[(x_of(0.5), y_of(0)), (x_of(12.5), y_of(0))])], &grid(), &code());
    assert_eq!(texts(&words), vec![(1, 1, 0, "pub"), (1, 1, 4, "fn"), (1, 1, 7, "step_6")]);
}

#[test]
fn what_is_under_a_stroke_is_the_row_it_is_on_and_the_words_come_in_the_order_the_stroke_passes_them() {
    // the third row, from the right to the left: "let a = b;" has b at byte 8 and a at byte 4 (the "=" and ";" are no words)
    let back = words_under(&[input(1, &[(x_of(9.5), y_of(2)), (x_of(4.5), y_of(2))])], &grid(), &code());
    assert_eq!(texts(&back), vec![(1, 3, 8, "b"), (1, 3, 4, "a")]);
}

#[test]
fn signs_with_no_letter_in_them_are_not_words_and_a_stroke_off_the_page_is_on_nothing() {
    // "{ }" has no word in it
    assert!(words_under(&[input(1, &[(x_of(0.2), y_of(1)), (x_of(2.8), y_of(1))])], &grid(), &code()).is_empty());
    // a row the page does not have, above the code and below it, and the margin to its left
    assert!(words_under(&[input(1, &[(x_of(1.0), y_of(0) - 100.0), (x_of(5.0), y_of(0) - 100.0)])], &grid(), &code()).is_empty());
    assert!(words_under(&[input(1, &[(x_of(1.0), y_of(9)), (x_of(5.0), y_of(9))])], &grid(), &code()).is_empty());
    assert!(words_under(&[input(1, &[(grid().origin_x - 40.0, y_of(0)), (grid().origin_x - 5.0, y_of(0))])], &grid(), &code()).is_empty());
}

#[test]
fn each_stroke_has_its_own_words_and_a_word_is_listed_once_for_a_stroke_that_passes_it_twice() {
    let there_and_back = input(1, &[(x_of(7.5), y_of(0)), (x_of(12.5), y_of(0)), (x_of(7.5), y_of(0)), (x_of(12.5), y_of(0))]);
    let other = input(2, &[(x_of(8.5), y_of(0)), (x_of(9.5), y_of(0))]);
    let words = words_under(&[there_and_back, other], &grid(), &code());
    assert_eq!(texts(&words), vec![(1, 1, 7, "step_6"), (2, 1, 7, "step_6")]);
}

#[test]
fn a_sample_that_is_no_number_and_a_page_that_is_no_grid_are_never_a_panic() {
    let bad = input(1, &[(f32::NAN, 10.0), (x_of(8.0), f32::INFINITY), (x_of(8.5), y_of(0)), (x_of(9.5), y_of(0))]);
    assert_eq!(texts(&words_under(&[bad], &grid(), &code())), vec![(1, 1, 7, "step_6")], "the samples that are numbers are read");
    let mut flat = grid();
    flat.line_height = 0.0;
    assert!(words_under(&[input(1, &[(x_of(8.0), y_of(0)), (x_of(9.0), y_of(0))])], &flat, &code()).is_empty());
    assert!(words_under(&[input(1, &[(1.0, 1.0)])], &grid(), &[]).is_empty());
}

// ---- the lines a loop goes round

/// The page of the stroke fixtures: line 12 has its word where the fixtures' loop goes round it (columns 33 to 43), and lines 18 to 23
/// have text that reaches the middle of the page, where the loop round a block goes.
fn page_of_the_fixtures() -> Vec<String> {
    let mut lines: Vec<String> = (0..46).map(|i| format!("fn f{i}() {{}}")).collect();
    lines[11] = format!("{}abcdefghij klm = 1;", " ".repeat(33));
    for line in &mut lines[17..23] {
        *line = format!("{}abcdefghijklmnopqrstuvwxyz0123456789", " ".repeat(33));
    }
    lines
}

fn from_fixture(id: u64, fixture: &crate::stroke_fixtures::Fixture) -> Stroke {
    Stroke {
        page: 0,
        semantic: Semantic::Scope,
        points: fixture.samples.iter().map(|s| InkPoint { x: s.x, y: s.y, pressure: s.pressure, tilt_x: 0.0, tilt_y: 0.0, t_ms: s.t_ms }).collect(),
        id,
        epoch: 0,
        started_unix_ms: 0,
    }
}

#[test]
fn the_lines_a_loop_goes_round_are_what_the_reviewer_is_talking_about() {
    let picked = pick(&note(0, 333.0, 204.0), &[from_fixture(1, &crate::stroke_fixtures::closed_loop())]);
    assert_eq!(circled_lines(&picked, 1, &page_of_the_fixtures()), Some((12, 12)));
}

#[test]
fn a_loop_on_a_later_page_is_numbered_by_the_lines_of_that_page() {
    let picked = pick(&note(0, 333.0, 204.0), &[from_fixture(1, &crate::stroke_fixtures::closed_loop())]);
    // the page that begins at line 47: its twelfth line is line 58
    assert_eq!(circled_lines(&picked, 47, &page_of_the_fixtures()), Some((58, 58)));
}

#[test]
fn strokes_that_are_no_loop_go_round_nothing() {
    use crate::stroke_fixtures as f;
    for fixture in [f::underline(), f::slash(), f::scribble(), f::open_c(), f::vertical_bar(), f::tap()] {
        let picked = pick(&note(0, 333.0, 204.0), &[from_fixture(1, &fixture)]);
        assert_eq!(circled_lines(&picked, 1, &page_of_the_fixtures()), None, "{} is no loop", fixture.name);
    }
    assert_eq!(circled_lines(&[], 1, &page_of_the_fixtures()), None, "no strokes");
    let picked = pick(&note(0, 333.0, 204.0), &[from_fixture(1, &crate::stroke_fixtures::closed_loop())]);
    assert_eq!(circled_lines(&picked, 1, &[]), None, "a page with no lines");
}

#[test]
fn two_loops_make_the_range_from_the_first_line_of_the_one_to_the_last_line_of_the_other() {
    let strokes = [from_fixture(1, &crate::stroke_fixtures::closed_loop()), from_fixture(2, &crate::stroke_fixtures::loop_around_a_block())];
    let picked = pick(&note(0, 333.0, 204.0), &strokes);
    assert_eq!(picked.len(), 2, "both are about the conversation");
    let (first, last) = circled_lines(&picked, 1, &page_of_the_fixtures()).expect("two loops");
    assert_eq!(first, 12, "the first line of the nearer loop");
    assert!(last >= 20 && last <= 24, "the last line of the loop round the block: {last}");
}

#[test]
fn no_more_words_than_a_request_lists_however_many_a_stroke_crosses() {
    let line: Vec<String> = vec!["a ".repeat(200)];
    let words = words_under(&[input(1, &[(x_of(0.1), y_of(0)), (x_of(399.0), y_of(0))])], &grid(), &line);
    assert_eq!(words.len(), MAX_WORDS_UNDER);
    assert_eq!(words[0].col, 0);
}
