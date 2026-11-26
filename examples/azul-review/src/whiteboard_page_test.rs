//! The whiteboard on the pages without a window: what a loop drawn with the pen arms, what an armed loop looks like where its
//! lines are shown, and the words and the key beside it.

use super::*;
use crate::{
    line_map::{Block, LineMap},
    lib_test::{app_with_slash_file, draw_fixture},
    pointer::Up,
    stroke_fixtures,
    whiteboard_drag::Armed,
};

/// The slash file of the other tests, and a block of six lines (18 to 23) whose text reaches the middle of the page, where the
/// fixture of the loop round a block goes round it.
fn app_with_a_block() -> AppState {
    let mut lines: Vec<String> = (0..120).map(|i| format!("fn f{i}() {{}}")).collect();
    lines[11] = format!("{}abcdefghij klm = 1;", " ".repeat(33));
    for line in &mut lines[17..23] {
        *line = format!("{}abcdefghijklmnopqrstuvwxyz0123456789", " ".repeat(33));
    }
    let file = crate::code::SourceFile { path: std::path::PathBuf::from("f.rs"), display: "f.rs".to_string(), lines, big: None };
    AppState::create(std::path::PathBuf::new(), vec![file], RefAny::new(0u8), std::path::PathBuf::new())
}

fn plain() -> LineMap {
    LineMap::new(46, Vec::new(), ui::PAGE_PAD, ui::LINE_H)
}

fn grown_by(rows: usize) -> LineMap {
    let block = Block { after: Some(2), lines: vec!["put in".to_string(); rows], accepted: false, conversation: 1 };
    LineMap::new(46, vec![block], ui::PAGE_PAD, ui::LINE_H)
}

fn region(first: u32, last: u32, at: u64) -> ArmedRegion {
    let mut regions = crate::whiteboard_drag::ArmedRegions::default();
    let polygon = vec![Pt::new(100.0, 200.0), Pt::new(400.0, 200.0), Pt::new(400.0, 230.0), Pt::new(100.0, 230.0)];
    let id = regions.arm(Armed { page: 0, polygon, first_line: first, last_line: last, stroke_id: 7 }, Size::new(1000.0, 730.0), at);
    regions.get(id).cloned().expect("the region that was armed")
}

// ---- the ring

#[test]
fn a_ring_is_as_much_of_a_circle_as_there_is_share_of_it_and_none_at_the_end() {
    let c = Pt::new(50.0, 50.0);
    assert_eq!(ring(c, 16.0, 1.0, 1.0, ACCENT).len(), 48, "a full ring: 48 pieces");
    assert_eq!(ring(c, 16.0, 0.5, 1.0, ACCENT).len(), 24);
    assert_eq!(ring(c, 16.0, 0.01, 1.0, ACCENT).len(), 2, "never fewer than two pieces: it shows a bend");
    assert!(ring(c, 16.0, 0.0, 1.0, ACCENT).is_empty());
    assert!(ring(c, 16.0, f32::NAN, 1.0, ACCENT).is_empty());
    assert_eq!(ring(c, 16.0, 7.0, 1.0, ACCENT).len(), 48, "more than the whole is the whole");
}

#[test]
fn a_ring_starts_at_the_top_and_goes_clockwise() {
    let c = Pt::new(50.0, 50.0);
    let half = ring(c, 10.0, 0.5, 1.0, ACCENT);
    let first = half.first().expect("pieces").a;
    assert!((first.x - 50.0).abs() < 0.01 && (first.y - 40.0).abs() < 0.01, "the top: {first:?}");
    let last = half.last().expect("pieces").b;
    assert!((last.x - 50.0).abs() < 0.01 && (last.y - 60.0).abs() < 0.01, "half a turn clockwise ends at the bottom: {last:?}");
    // a quarter turn is at the right
    let quarter = ring(c, 10.0, 0.25, 1.0, ACCENT);
    let end = quarter.last().expect("pieces").b;
    assert!((end.x - 60.0).abs() < 0.01 && (end.y - 50.0).abs() < 0.01, "{end:?}");
}

// ---- what an armed loop looks like

#[test]
fn an_armed_loop_is_drawn_as_its_outline_its_grip_and_the_ring_of_what_is_left() {
    let r = region(12, 12, 1_000);
    let segs = overlay_segments(&r, 1_000, &plain());
    // the outline is closed: as many pieces as it has corners, the last back to the first
    let outline = &segs[..4];
    assert_eq!(outline[3].b, r.polygon[0]);
    assert!(outline.iter().all(|s| s.rgba == OUTLINE));
    // then the grip (a disc, then the arrow), then the pale ring all round and the part that is left
    let accent_discs = segs.iter().filter(|s| s.a == s.b && s.radius == GRIP_PX * 0.5).count();
    assert_eq!(accent_discs, 1);
    assert_eq!(segs.iter().filter(|s| s.rgba == RING_TRACK).count(), 48);
    assert_eq!(segs.iter().filter(|s| s.rgba == ACCENT && s.radius > 1.5 && s.radius < 2.0).count(), 48, "all of its time is left");
}

#[test]
fn the_ring_runs_out_with_the_time_and_is_gone_with_the_loop() {
    let r = region(12, 12, 1_000);
    let left = |now: u64| overlay_segments(&r, now, &plain()).iter().filter(|s| s.rgba == ACCENT && s.radius > 1.5 && s.radius < 2.0).count();
    let life = r.lifetime_ms;
    assert_eq!(left(1_000), 48);
    assert_eq!(left(1_000 + life / 2), 24);
    assert_eq!(left(1_000 + life), 0);
    assert_eq!(left(1_000 + life * 3), 0);
}

#[test]
fn an_armed_loop_goes_down_with_its_lines_when_lines_are_put_in_above_it() {
    let r = region(12, 12, 0);
    let before = overlay_segments(&r, 0, &plain());
    let after = overlay_segments(&r, 0, &grown_by(4));
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(&after) {
        assert!((b.a.y - a.a.y - 4.0 * ui::LINE_H).abs() < 0.01, "{a:?} -> {b:?}");
        assert_eq!(a.a.x, b.a.x);
    }
}

#[test]
fn a_loop_that_is_carried_is_its_outline_moved_by_the_pointer() {
    let polygon = vec![Pt::new(10.0, 40.0), Pt::new(90.0, 40.0), Pt::new(90.0, 70.0)];
    let segs = ghost_segments(&polygon, Pt::new(300.0, -5.0), &plain());
    assert_eq!(segs.len(), 3, "the three sides, the last back to the first");
    assert_eq!(segs[0].a, Pt::new(310.0, 35.0));
    assert_eq!(segs[2].b, Pt::new(310.0, 35.0));
    assert!(segs.iter().all(|s| s.rgba == GHOST));
}

// ---- what the pen arms

#[test]
fn a_loop_round_a_line_of_the_page_is_armed_and_what_is_not_a_loop_is_not() {
    let mut s = app_with_slash_file();
    let ended = draw_fixture(&mut s, &stroke_fixtures::closed_loop(), 0).expect("the stroke ended");
    assert_eq!(ended.up, Up::Kept);
    let (id, first, last) = arm_last(&s).expect("the loop round the word arms its line");
    assert_eq!((first, last), (12, 12));
    let g = s.wb.borrow();
    let r = g.panel.wb.armed.get(id).expect("armed");
    assert_eq!(r.page, 0);
    assert_eq!(g.panel.wb.armed.len(), 1);
    drop(g);

    // an underline, a bar, a scribble and a "C" arm nothing
    for fixture in [stroke_fixtures::underline(), stroke_fixtures::vertical_bar(), stroke_fixtures::scribble(), stroke_fixtures::open_c()] {
        let mut t = app_with_slash_file();
        let ended = draw_fixture(&mut t, &fixture, 0).expect("ended");
        if ended.up == Up::Kept {
            assert_eq!(arm_last(&t), None, "{} is no loop", fixture.name);
        }
    }
}

#[test]
fn a_loop_round_a_block_of_lines_arms_all_of_them() {
    let mut s = app_with_a_block();
    draw_fixture(&mut s, &stroke_fixtures::loop_around_a_block(), 0).expect("ended");
    let (_, first, last) = arm_last(&s).expect("armed");
    assert!(first >= 17 && last <= 24 && last - first >= 3, "lines {first} to {last}");
}

#[test]
fn a_loop_on_the_second_page_is_armed_with_the_numbers_of_that_pages_lines() {
    let mut s = app_with_slash_file();
    draw_fixture(&mut s, &stroke_fixtures::closed_loop(), 1).expect("ended");
    let (id, first, last) = arm_last(&s).expect("armed");
    assert_eq!((first, last), (crate::code::LINES_PER_PAGE as u32 + 12, crate::code::LINES_PER_PAGE as u32 + 12));
    let g = s.wb.borrow();
    assert_eq!(g.panel.wb.armed.get(id).map(|r| r.page), Some(1));
}

#[test]
fn a_loop_whose_stroke_is_taken_back_or_erased_is_not_armed_any_more() {
    let mut s = app_with_slash_file();
    draw_fixture(&mut s, &stroke_fixtures::closed_loop(), 0).expect("ended");
    arm_last(&s).expect("armed");
    assert_eq!(s.wb.borrow().panel.wb.armed.len(), 1);
    assert!(s.undo(), "the loop is taken back");
    assert_eq!(s.wb.borrow().panel.wb.armed.len(), 0, "its stroke is not on the page: nor is the loop armed");
    // the stroke comes back, the loop is not armed again by it; the pen arms it when it is drawn
    assert!(s.redo());
    assert_eq!(s.wb.borrow().panel.wb.armed.len(), 0);
    arm_last(&s).expect("armed again");
    let at = s.strokes.len() - 1;
    s.erase(at).expect("the stroke is erased");
    assert_eq!(s.wb.borrow().panel.wb.armed.len(), 0, "an erased loop is no loop");
}

#[test]
fn a_page_shows_the_layer_of_loops_only_while_a_loop_waits_on_it() {
    let mut s = app_with_slash_file();
    assert!(!page_has_regions(&s, 0));
    draw_fixture(&mut s, &stroke_fixtures::closed_loop(), 0).expect("ended");
    arm_last(&s).expect("armed");
    assert!(page_has_regions(&s, 0));
    assert!(!page_has_regions(&s, 1), "the other page has none");
    s.wb.borrow_mut().panel.wb.armed.clear();
    assert!(!page_has_regions(&s, 0));
}

// ---- the words and the key

#[test]
fn each_armed_loop_of_a_page_has_its_words_and_key_and_a_loop_that_is_gone_has_none() {
    let mut s = app_with_a_block();
    crate::l10n::in_english();
    let app = RefAny::new(0u8);
    assert!(chips(&s, &app, 0, &plain(), ui::page_h()).is_empty());
    draw_fixture(&mut s, &stroke_fixtures::closed_loop(), 0).expect("ended");
    arm_last(&s).expect("armed");
    draw_fixture(&mut s, &stroke_fixtures::loop_around_a_block(), 0).expect("ended");
    arm_last(&s).expect("armed");
    assert_eq!(chips(&s, &app, 0, &plain(), ui::page_h()).len(), 2);
    assert!(chips(&s, &app, 1, &plain(), ui::page_h()).is_empty());
    s.wb.borrow_mut().panel.wb.armed.clear();
    assert!(chips(&s, &app, 0, &plain(), ui::page_h()).is_empty());
}

#[test]
fn the_ids_of_the_pieces_of_a_page_name_their_page_and_their_loop() {
    assert_eq!(overlay_id(3), "__azreview_wb-overlay-3");
    assert_eq!(chip_id(5), "__azreview_wb-chip-5");
    assert_eq!(send_id(5), "__azreview_wb-send-5");
}

// ---- a rest that is for the board

#[test]
fn a_pen_that_rests_on_the_grip_of_a_loop_is_reaching_for_it_and_is_not_asking_to_be_listened_to() {
    let mut s = app_with_slash_file();
    draw_fixture(&mut s, &stroke_fixtures::closed_loop(), 0).expect("ended");
    let (id, ..) = arm_last(&s).expect("armed");
    let grip = s.wb.borrow().panel.wb.armed.get(id).expect("armed").grip;
    // on the grip, and a little way round it
    assert!(rest_is_for_the_board(&s, 0, grip.center()));
    assert!(rest_is_for_the_board(&s, 0, Pt::new(grip.right() + 5.0, grip.center().y)));
    // far from it, on the loop's far side, on another page
    assert!(!rest_is_for_the_board(&s, 0, Pt::new(grip.center().x - 150.0, grip.center().y)));
    assert!(!rest_is_for_the_board(&s, 1, grip.center()));
    // a loop that is gone has no grip
    s.wb.borrow_mut().panel.wb.armed.clear();
    assert!(!rest_is_for_the_board(&s, 0, grip.center()));
}

#[test]
fn while_a_press_on_a_loop_is_followed_no_rest_asks_to_be_listened_to_anywhere() {
    let mut s = app_with_slash_file();
    draw_fixture(&mut s, &stroke_fixtures::closed_loop(), 0).expect("ended");
    let (id, ..) = arm_last(&s).expect("armed");
    let inside = {
        let g = s.wb.borrow();
        let r = g.panel.wb.armed.get(id).expect("armed");
        Pt::new(r.bounds.x + 20.0, r.bounds.y + r.bounds.h / 2.0)
    };
    assert!(!rest_is_for_the_board(&s, 0, Pt::new(10.0, 10.0)));
    let press = s.wb.borrow_mut().panel.wb.page_press(Pointer { page: 0, in_page: inside, in_window: inside }, 5);
    assert!(matches!(press, Press::Inside(_)), "{press:?}");
    assert!(rest_is_for_the_board(&s, 0, Pt::new(10.0, 10.0)), "the press is followed: the hand is at the board");
}
