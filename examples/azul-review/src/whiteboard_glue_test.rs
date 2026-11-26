//! The whiteboard in the review window as numbers: where its bar is and what a loop on a page is measured on. Pure.

use super::*;
use crate::line_map::{Block, LineMap};
use crate::whiteboard_jump::{Freshness, JumpTarget};

fn panels(open: bool, width: f32) -> Panels {
    Panels { tree_open: false, tree_w: layout::TREE_W_DEFAULT, right_open: open, right_w: width }
}

#[test]
fn an_open_bar_is_the_panel_at_the_window_edge_between_the_top_bar_and_the_bottom_bar() {
    let window = Size::new(1280.0, 820.0);
    let p = panels(true, 360.0);
    let solved = layout::solve(window.w, &p, &layout::Slide::settled(&p), 0.0);
    let bar = bar_of(window, &p, &solved);
    assert!(bar.open);
    assert_eq!(bar.window_w, 1280.0);
    // the panel's own border is the panel's: the board's area, which a drop is measured from, begins inside it
    assert_eq!(
        bar.rect,
        Rect::new(1280.0 - 360.0 + PANEL_BORDER_PX, layout::TOP_H, 360.0 - PANEL_BORDER_PX, 820.0 - layout::TOP_H - layout::BOTTOM_H)
    );
}

#[test]
fn a_closed_bar_is_the_thin_edge_of_the_window_and_still_a_place_to_drop_on() {
    let window = Size::new(1280.0, 820.0);
    let p = panels(false, 360.0);
    let solved = layout::solve(window.w, &p, &layout::Slide::settled(&p), 0.0);
    let bar = bar_of(window, &p, &solved);
    assert!(!bar.open);
    assert_eq!(bar.rect.x, 1280.0 - layout::RIGHT_BAR_W);
    assert_eq!(bar.rect.w, layout::RIGHT_BAR_W);
}

#[test]
fn a_bar_that_is_sliding_is_as_wide_as_it_is_now_and_a_window_that_is_no_number_leaves_no_negative_size() {
    let window = Size::new(1280.0, 820.0);
    let p = panels(true, 360.0);
    let mut slide = layout::Slide::settled(&p);
    slide.right = 0.5;
    let solved = layout::solve(window.w, &p, &slide, 0.0);
    let bar = bar_of(window, &p, &solved);
    assert!(bar.open && bar.rect.w > 0.0 && bar.rect.w < 360.0, "{bar:?}");
    // a window too short for the bars and nothing at all
    let tiny = bar_of(Size::new(300.0, 20.0), &p, &layout::solve(300.0, &p, &layout::Slide::settled(&p), 0.0));
    assert!(tiny.rect.h >= 0.0 && tiny.rect.w >= 0.0);
    let nan = bar_of(Size::new(f32::NAN, f32::NAN), &p, &solved);
    assert!(nan.rect.w >= 0.0 && nan.rect.h >= 0.0 && nan.rect.x.is_finite() && nan.rect.y.is_finite());
}

#[test]
fn what_a_loop_is_measured_on_is_the_page_as_it_is_drawn() {
    let lines = vec!["fn a() {}".to_string(), String::new(), "let é = 1;".to_string()];
    let chars = page_chars(&lines);
    assert_eq!(chars, vec![9, 0, 10], "characters, not bytes");
    let m = page_lines(47, &chars);
    assert_eq!(m.first_number, 47);
    assert_eq!((m.top, m.line_h, m.text_x, m.char_w), (ui::PAGE_PAD, ui::LINE_H, ui::CODE_X, ui::CHAR_W));
    assert_eq!(m.max_w, ui::CODE_W - ui::GUTTER_W);
    assert_eq!(m.chars.len(), 3);
}

fn plain(lines: usize) -> LineMap {
    LineMap::new(lines, Vec::new(), ui::PAGE_PAD, ui::LINE_H)
}

fn grown(lines: usize, after: usize, rows: usize) -> LineMap {
    let block = Block { after: Some(after), lines: vec!["inserted".to_string(); rows], accepted: false, conversation: 1 };
    LineMap::new(lines, vec![block], ui::PAGE_PAD, ui::LINE_H)
}

const PAGE: Size = Size { w: 1000.0, h: 46.0 * 15.0 + 40.0 };

#[test]
fn the_words_of_an_armed_region_are_put_under_its_loop_at_its_left() {
    let loop_box = Rect::new(120.0, 100.0, 300.0, 60.0);
    let at = chip_place(loop_box, &plain(46), PAGE);
    assert_eq!(at, Pt::new(120.0, 160.0 + CHIP_GAP));
}

#[test]
fn the_words_go_over_a_loop_that_has_no_room_under_it_and_stay_on_the_page() {
    let low = Rect::new(120.0, PAGE.h - 40.0, 300.0, 30.0);
    let at = chip_place(low, &plain(46), PAGE);
    assert_eq!(at.y, (PAGE.h - 40.0) - CHIP_H - CHIP_GAP, "over the loop, not under it: {at:?}");
    // a loop at the very top with no room anywhere is at the page's top, never above it
    let tiny = Size::new(1000.0, 50.0);
    assert_eq!(chip_place(Rect::new(10.0, 2.0, 30.0, 30.0), &plain(3), tiny).y, 0.0);
    // far right: the words are inside the page
    let right = chip_place(Rect::new(900.0, 100.0, 90.0, 40.0), &plain(46), PAGE);
    assert!(right.x + CHIP_W <= PAGE.w, "{right:?}");
    // a point that is no number is a place
    let nan = chip_place(Rect::new(f32::NAN, f32::NAN, 1.0, 1.0), &plain(46), PAGE);
    assert!(nan.x.is_finite() && nan.y.is_finite(), "{nan:?}");
}

#[test]
fn the_words_follow_the_loop_down_when_lines_are_put_in_above_it() {
    let loop_box = Rect::new(120.0, 200.0, 300.0, 40.0);
    let before = chip_place(loop_box, &plain(46), PAGE);
    let after = chip_place(loop_box, &grown(46, 2, 3), Size::new(PAGE.w, PAGE.h + 45.0));
    assert_eq!(after.y, before.y + 3.0 * ui::LINE_H, "three rows were put in above it");
}

#[test]
fn a_region_that_is_sent_lands_in_the_upper_part_of_the_board_and_the_next_one_is_not_on_it() {
    let first = send_point(340.0, 0);
    assert!(first.y >= crate::whiteboard_app::HEADER_PX, "under the header: {first:?}");
    assert!(first.x > 0.0 && first.x < 340.0, "inside the bar: {first:?}");
    let second = send_point(340.0, 1);
    assert_ne!(first, second);
    // the places go round (the board is not left): the seventh is the first again
    assert_eq!(send_point(340.0, 6), first);
    // a bar with no width (or none that is a number) has the points at its left edge, never outside it
    assert_eq!(send_point(f32::NAN, 0).x, 0.0);
    assert!((0..12).all(|n| (0.0..=20.0).contains(&send_point(20.0, n).x)), "a narrow bar holds them all");
}

fn jump(file: &str, first: u32, last: u32) -> JumpTarget {
    JumpTarget { id: 1, file: file.to_string(), first_line: first, last_line: last, freshness: Freshness::Fresh }
}

#[test]
fn the_lines_of_a_jump_are_lit_for_a_while_and_then_not() {
    let f = Flash::of(&jump("src/lib.rs", 50, 52), 10_000);
    assert_eq!((f.first, f.last, f.until_ms), (50, 52, 10_000 + FLASH_MS));
    assert!(!f.is_over(10_000 + FLASH_MS - 1));
    assert!(f.is_over(10_000 + FLASH_MS));
    // lines given the wrong way round are the same lines
    let swapped = Flash::of(&jump("a", 9, 3), 0);
    assert_eq!((swapped.first, swapped.last), (3, 9));
}

#[test]
fn a_flash_lights_the_rows_of_the_page_it_reaches_and_no_other_page_or_file() {
    let f = Flash::of(&jump("src/lib.rs", 44, 49), 0);
    // page 1 holds lines 1..=46: rows 43 to 45
    assert_eq!(f.rows_on("src/lib.rs", 1, 46), Some((43, 45)));
    // page 2 holds lines 47..=92: rows 0 to 2
    assert_eq!(f.rows_on("src/lib.rs", 47, 46), Some((0, 2)));
    // page 3 and another file have none
    assert_eq!(f.rows_on("src/lib.rs", 93, 46), None);
    assert_eq!(f.rows_on("src/other.rs", 1, 46), None);
    // a page with no lines has none
    assert_eq!(f.rows_on("src/lib.rs", 1, 0), None);
    // the last page of a file is shorter
    let g = Flash::of(&jump("src/lib.rs", 100, 120), 0);
    assert_eq!(g.rows_on("src/lib.rs", 93, 10), Some((7, 9)));
}

#[test]
fn the_box_of_lit_rows_is_where_the_page_shows_them_and_takes_lines_put_in_between() {
    let (top, height) = rows_box(&plain(46), 3, 5);
    assert_eq!((top, height), (ui::PAGE_PAD + 3.0 * ui::LINE_H, 3.0 * ui::LINE_H));
    // two rows put in after line 3: line 4 and 5 are two rows lower, the box is lit through the lines between
    let (top, height) = rows_box(&grown(46, 3, 2), 3, 5);
    assert_eq!(top, ui::PAGE_PAD + 3.0 * ui::LINE_H);
    assert_eq!(height, 5.0 * ui::LINE_H, "rows 3, the two put in, and 4 and 5");
}

// ---- what was done since the last snapshot

fn header(conversation: u64, outcome: Option<crate::patch::Outcome>, said: &str, rationale: &str) -> crate::patch::ReadHeader {
    crate::patch::ReadHeader { conversation, outcome, said: said.to_string(), rationale: rationale.to_string(), ..Default::default() }
}

#[test]
fn a_resolved_conversation_is_a_line_for_the_reason_of_a_snapshot_in_the_words_of_how_it_ended() {
    crate::l10n::in_english();
    use crate::patch::Outcome;
    let has = |line: &str, pieces: &[&str]| pieces.iter().all(|p| line.contains(p));
    let taken = material_line(&header(3, Some(Outcome::Accepted), "rename this loop", ""));
    assert!(has(&taken, &["Conversation", "3", "suggestions taken", "rename this loop"]), "{taken}");
    let refused = material_line(&header(4, Some(Outcome::Rejected), "drop it", ""));
    assert!(has(&refused, &["4", "suggestions refused", "drop it"]), "{refused}");
    let answered = material_line(&header(5, Some(Outcome::Answered), "why is this here", ""));
    assert!(has(&answered, &["5", "answered in words", "why is this here"]), "{answered}");
    let closed = material_line(&header(6, Some(Outcome::Closed), "", ""));
    assert!(has(&closed, &["6", "closed"]), "{closed}");
    // an outcome this version does not know is a closed one
    assert!(material_line(&header(7, None, "x", "")).contains("closed"));
    // why the agent answered as it did follows in brackets
    let why = material_line(&header(8, Some(Outcome::Accepted), "make it shorter", "the loop is the same"));
    assert!(has(&why, &["8", "make it shorter", "(the loop is the same)"]), "{why}");
}

#[test]
fn a_long_word_said_or_a_long_reason_is_cut_to_what_a_line_of_a_reason_can_hold() {
    crate::l10n::in_english();
    let line = material_line(&header(1, Some(crate::patch::Outcome::Accepted), &"a".repeat(500), &"b".repeat(500)));
    assert!(line.chars().count() < 200, "{} characters", line.chars().count());
    assert!(line.contains('\u{2026}'), "cut where something was cut: {line}");
}

#[test]
fn only_what_is_newer_than_the_snapshot_is_material_oldest_first_and_never_more_than_the_most() {
    let entries = vec![(300, "c".to_string()), (100, "a".to_string()), (200, "b".to_string())];
    assert_eq!(lines_since(&entries, 0), vec!["a", "b", "c"], "no snapshot yet: everything, by the time");
    assert_eq!(lines_since(&entries, 100), vec!["b", "c"], "a snapshot at the very moment of a line is after it");
    assert!(lines_since(&entries, 300).is_empty());
    let many: Vec<(u64, String)> = (1..=MAX_MATERIAL as u64 + 30).map(|i| (i, format!("line {i}"))).collect();
    let kept = lines_since(&many, 0);
    assert_eq!(kept.len(), MAX_MATERIAL);
    assert_eq!(kept.last().map(String::as_str), Some(format!("line {}", MAX_MATERIAL + 30).as_str()), "the newest are kept");
    assert_eq!(kept.first().map(String::as_str), Some("line 31"));
}
