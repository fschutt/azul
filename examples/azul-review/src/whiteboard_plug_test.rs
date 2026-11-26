//! The whiteboard in the review window without a window: what its glue starts with, how the clock treats what waits on it,
//! and what the settings page offers.

use std::path::PathBuf;

use super::*;
use crate::{
    lib_test::app_with_file,
    whiteboard_drag::{Armed, HOLD_MS},
    whiteboard_geom::{Pt, Size},
};

fn glue() -> Glue {
    Glue::new(&PathBuf::from("/work/space"), &PathBuf::from("/data"))
}

fn square(x: f32, y: f32) -> Vec<Pt> {
    vec![Pt::new(x, y), Pt::new(x + 200.0, y), Pt::new(x + 200.0, y + 60.0), Pt::new(x, y + 60.0)]
}

fn arm(g: &mut Glue, at: u64) -> u64 {
    g.panel.wb.armed.arm(Armed { page: 0, polygon: square(100.0, 200.0), first_line: 12, last_line: 13, stroke_id: 1 }, Size::new(1000.0, 730.0), at)
}

#[test]
fn the_board_of_a_workspace_is_kept_in_the_review_folder_of_the_data_root() {
    let g = glue();
    assert_eq!(g.files, PathBuf::from("/data/review"));
    assert!(g.persist && !g.started, "it is read before it is touched");
    assert_eq!(g.panel.wb.workspace(), "/work/space");
    assert_eq!(g.panel.wb.armed.lifetime_ms(), ARM_MS_DEFAULT, "longer than the board's own 8 s: the first time there are words to read");
}

#[test]
fn a_window_with_no_folder_has_a_board_that_is_not_kept() {
    let g = Glue::new(&PathBuf::new(), &PathBuf::from("/data"));
    assert!(!g.persist);
}

#[test]
fn nothing_waits_on_the_clock_until_a_loop_is_armed_a_press_is_followed_a_board_is_deleted_or_lines_are_lit() {
    let mut g = glue();
    assert!(!g.has_time_in_it());
    arm(&mut g, 0);
    assert!(g.has_time_in_it());
    g.panel.wb.armed.clear();
    assert!(!g.has_time_in_it());
    g.flash = Some(crate::whiteboard_glue::Flash { file: "a.rs".into(), first: 1, last: 2, until_ms: 100 });
    assert!(g.has_time_in_it());
}

#[test]
fn a_loop_is_armed_until_its_time_is_up_and_then_the_clock_has_nothing_to_wait_for() {
    let mut g = glue();
    arm(&mut g, 1_000);
    // the first look: a loop is armed, the pages are built for it
    let first = tick_at(&mut g, 2_000);
    assert!(first.more && first.redraw && first.pages_changed, "{first:?}");
    // the next: the same loop, only its ring is drawn again
    let early = tick_at(&mut g, 2_300);
    assert!(early.more && early.redraw && !early.pages_changed, "{early:?}");
    let late = tick_at(&mut g, 1_000 + ARM_MS_DEFAULT + 1);
    assert!(late.pages_changed, "the loop is gone from the page: its layer and its words go");
    assert!(!late.more, "nothing is left to wait for");
    assert!(g.panel.wb.armed.is_empty());
}

#[test]
fn a_loop_that_is_let_go_of_before_its_time_is_up_takes_its_words_with_it_at_the_next_look() {
    let mut g = glue();
    let id = arm(&mut g, 0);
    let _ = tick_at(&mut g, 500);
    assert!(!tick_at(&mut g, 800).pages_changed, "nothing changed on the page");
    // what armed it was erased or taken back
    g.panel.wb.armed.disarm(id);
    let after = tick_at(&mut g, 1_100);
    assert!(after.pages_changed && !after.more, "the pages are built again, nothing waits: {after:?}");
}

#[test]
fn the_ring_of_a_loop_is_drawn_again_four_times_a_second_and_not_at_every_look_at_the_clock() {
    let mut g = glue();
    arm(&mut g, 0);
    assert!(tick_at(&mut g, 2_000).redraw, "the first look draws");
    assert!(!tick_at(&mut g, 2_100).redraw, "the same quarter of a second");
    assert!(!tick_at(&mut g, 2_249).redraw);
    assert!(tick_at(&mut g, 2_250).redraw, "the next quarter of a second");
    g.panel.wb.armed.clear();
    assert!(!tick_at(&mut g, 3_000).redraw, "nothing waits: nothing is drawn");
}

#[test]
fn lines_that_a_jump_lit_are_dark_again_when_their_moment_is_over() {
    let mut g = glue();
    g.flash = Some(crate::whiteboard_glue::Flash { file: "a.rs".into(), first: 3, last: 4, until_ms: 1_500 });
    let during = tick_at(&mut g, 1_000);
    assert!(during.more && !during.pages_changed && g.flash.is_some());
    let after = tick_at(&mut g, 1_500);
    assert!(after.pages_changed && !after.more && g.flash.is_none());
}

#[test]
fn a_press_inside_a_loop_that_is_held_is_told_once_by_the_clock() {
    let mut g = glue();
    arm(&mut g, 0);
    // a press inside the loop, away from its grip (the grip is at the right, at the top)
    let at = Pt::new(130.0, 230.0);
    let press = g.panel.wb.page_press(crate::whiteboard_drag::Pointer { page: 0, in_page: at, in_window: at }, 10);
    assert!(matches!(press, crate::whiteboard_drag::Press::Inside(_)), "{press:?}");
    let early = tick_at(&mut g, 10 + HOLD_MS - 1);
    assert!(!early.held);
    let held = tick_at(&mut g, 10 + HOLD_MS);
    assert!(held.held, "held long enough: the stroke it began is no stroke");
    let again = tick_at(&mut g, 10 + HOLD_MS + 100);
    assert!(!again.held, "told once");
}

#[test]
fn a_loop_that_is_held_in_the_hand_is_not_taken_away_under_it() {
    let mut g = glue();
    let id = arm(&mut g, 0);
    let grip = g.panel.wb.armed.get(id).expect("armed").grip.center();
    let press = g.panel.wb.page_press(crate::whiteboard_drag::Pointer { page: 0, in_page: grip, in_window: grip }, 100);
    assert!(matches!(press, crate::whiteboard_drag::Press::Grip(_)), "{press:?}");
    let way_past = tick_at(&mut g, ARM_MS_DEFAULT * 3);
    assert!(g.panel.wb.armed.get(id).is_some(), "the owner keeps what the pointer holds: {way_past:?}");
}

#[test]
fn what_was_done_is_kept_oldest_first_not_twice_and_never_more_than_the_most() {
    let mut g = glue();
    g.note_patch(200, "b".to_string());
    g.note_patch(100, "a".to_string());
    g.note_patch(200, "b".to_string());
    assert_eq!(g.patches, vec![(100, "a".to_string()), (200, "b".to_string())]);
    for i in 0..(MAX_PATCH_LINES as u64 + 20) {
        g.note_patch(1_000 + i, format!("line {i}"));
    }
    assert_eq!(g.patches.len(), MAX_PATCH_LINES);
    assert_eq!(g.patches.last().map(|(_, l)| l.as_str()), Some(format!("line {}", MAX_PATCH_LINES + 19).as_str()), "the newest are kept");
}

#[test]
fn the_panel_is_offered_what_was_done_since_the_last_snapshot() {
    let mut g = glue();
    assert!(g.panel.history_ui.material.is_empty());
    g.note_patch(300, "later".to_string());
    g.note_patch(100, "earlier".to_string());
    g.refresh_material();
    // no snapshot was taken yet: everything is offered, by the time
    assert_eq!(g.panel.history_ui.material, vec!["earlier".to_string(), "later".to_string()]);
}

#[test]
fn the_settings_page_offers_the_times_a_loop_may_wait_and_five_lines_of_help() {
    crate::l10n::in_english();
    let s = app_with_file();
    let section = settings_section(&s, &RefAny::new(0u8));
    // the band's title is the whiteboard's, in the person's language
    assert_eq!(section.title.as_str(), "Whiteboard");
    // a row for the time, a box for the help's title and then its five lines
    let children = section.content.children.as_slice();
    assert_eq!(children.len(), 2 + HELP_LINES.len(), "the time, the help's title, and the five lines");
}

#[test]
fn the_times_on_offer_are_inside_what_the_board_allows_and_the_one_in_use_is_chosen() {
    use crate::whiteboard_drag::{ARM_MAX_MS, ARM_MIN_MS};
    for ms in ARM_PRESETS {
        assert!((ARM_MIN_MS..=ARM_MAX_MS).contains(&ms), "{ms} ms is a time the board takes");
    }
    assert!(ARM_PRESETS.windows(2).all(|w| w[0] < w[1]), "ascending");
    assert!(ARM_PRESETS.contains(&ARM_MS_DEFAULT), "the default is one of them");
    assert_eq!(nearest(&ARM_PRESETS, ARM_MS_DEFAULT), 1);
}
