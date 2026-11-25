//! The gestures on the whiteboard itself: what is under the pointer, moving, resizing, drawing an arrow
//! (with the mouse from a rim, with the pen as one stroke), the double click, the wheel. No window.

use std::sync::Arc;

use super::*;
use crate::{
    whiteboard::{CutoutDraft, CutoutImage, Notice, Source},
    whiteboard_geom::{Size, View},
};

fn image() -> Arc<CutoutImage> {
    // 400 x 200 pixels at 2x: 200 x 100 on the board.
    Arc::new(CutoutImage { width: 400, height: 200, scale: 2.0, rgba: vec![255; 400 * 200 * 4], png: vec![] })
}

fn source() -> Source {
    Source {
        file: "src/lib.rs".to_string(),
        page: 0,
        first_line: 10,
        last_line: 20,
        shape: vec![],
        file_hash: 1,
        range_hash: 2,
        lines: vec![],
    }
}

/// Cutouts at (x, y), 200 x 100.
fn put(board: &mut Board, x: f32, y: f32) -> ItemId {
    board.add_cutout(CutoutDraft::ready(Pt::new(x, y), image(), source())).expect("room")
}

/// Two cutouts: `a` at (100, 100) and `b` at (500, 100), the view at its start.
fn two() -> (Board, ItemId, ItemId) {
    let mut board = Board::new();
    let a = put(&mut board, 100.0, 100.0);
    let b = put(&mut board, 500.0, 100.0);
    (board, a, b)
}

fn mouse() -> PointerKind {
    PointerKind::Mouse
}

fn no_mods() -> Mods {
    Mods::default()
}

fn p(x: f32, y: f32) -> Pt {
    Pt::new(x, y)
}

/// A press at the first point, a motion to each of the others, the release at the last.
fn drag(input: &mut BoardInput, board: &mut Board, kind: PointerKind, path: &[Pt]) -> Out {
    input.press(board, path[0], kind, no_mods());
    for q in &path[1..] {
        input.motion(board, *q, no_mods());
    }
    input.release(board, path[path.len() - 1], no_mods())
}

// ---- what is under the pointer

#[test]
fn the_body_the_rim_and_the_outside_of_a_cutout() {
    let (board, a, _) = two();
    let hit = |x, y| hit_test(&board, None, p(x, y), 1.0);
    assert_eq!(hit(200.0, 150.0), Hit::Body(a));
    assert_eq!(hit(103.0, 150.0), Hit::Rim(a, Side::Left), "the inner band along the edge");
    assert_eq!(hit(95.0, 150.0), Hit::Rim(a, Side::Left), "and the outer one");
    assert_eq!(hit(200.0, 103.0), Hit::Rim(a, Side::Top));
    assert_eq!(hit(297.0, 150.0), Hit::Rim(a, Side::Right));
    assert_eq!(hit(200.0, 197.0), Hit::Rim(a, Side::Bottom));
    assert_eq!(hit(400.0, 400.0), Hit::Empty);
    assert_eq!(hit(60.0, 150.0), Hit::Empty, "40 px outside is outside");
}

#[test]
fn the_bands_keep_their_size_on_the_screen_whatever_the_zoom() {
    let (board, a, _) = two();
    // At 2x zoom a rim of 10 screen px is 5 board px: 7 px inside the edge is the body, not the rim.
    assert_eq!(hit_test(&board, None, p(107.0, 150.0), 2.0), Hit::Body(a));
    assert_eq!(hit_test(&board, None, p(103.0, 150.0), 2.0), Hit::Rim(a, Side::Left));
    // At half the zoom it is 20 board px.
    assert_eq!(hit_test(&board, None, p(117.0, 150.0), 0.5), Hit::Rim(a, Side::Left));
}

#[test]
fn the_cutout_on_top_is_the_one_that_is_hit() {
    let mut board = Board::new();
    let under = put(&mut board, 100.0, 100.0);
    let over = put(&mut board, 150.0, 120.0);
    assert_eq!(hit_test(&board, None, p(220.0, 160.0), 1.0), Hit::Body(over), "where they overlap");
    assert_eq!(hit_test(&board, None, p(130.0, 130.0), 1.0), Hit::Body(under), "where only the one under is");
    board.raise(under);
    assert_eq!(hit_test(&board, None, p(220.0, 160.0), 1.0), Hit::Body(under));
}

#[test]
fn the_handle_of_the_selected_cutout_is_at_its_bottom_right_corner_and_only_when_it_is_selected() {
    let (board, a, b) = two();
    let corner = p(295.0, 195.0);
    assert!(matches!(hit_test(&board, None, corner, 1.0), Hit::Rim(..) | Hit::Body(_)), "not selected: no handle");
    assert_eq!(hit_test(&board, Some(a), corner, 1.0), Hit::Handle(a));
    assert!(!matches!(hit_test(&board, Some(b), corner, 1.0), Hit::Handle(_)), "another cutout is selected");
}

#[test]
fn an_arrow_is_hit_near_its_line_and_the_selected_one_has_ends_and_a_bend() {
    let (mut board, a, b) = two();
    let arrow = board.add_arrow(End::item(a), End::item(b), 0.0, "").expect("added");
    // The line runs from (300, 150) to (500, 150).
    assert_eq!(hit_test(&board, None, p(400.0, 153.0), 1.0), Hit::Arrow(arrow));
    assert_eq!(hit_test(&board, None, p(400.0, 170.0), 1.0), Hit::Empty);
    assert_eq!(hit_test(&board, Some(arrow), p(301.0, 151.0), 1.0), Hit::ArrowEnd(arrow, Which::From));
    assert_eq!(hit_test(&board, Some(arrow), p(499.0, 149.0), 1.0), Hit::ArrowEnd(arrow, Which::To));
    assert_eq!(hit_test(&board, Some(arrow), p(400.0, 150.0), 1.0), Hit::ArrowBend(arrow));
}

#[test]
fn a_label_has_a_place_under_its_cutout_while_it_says_something_or_the_cutout_is_selected() {
    let (mut board, a, _) = two();
    assert_eq!(hit_test(&board, None, p(200.0, 210.0), 1.0), Hit::Rim(a, Side::Bottom), "no label, not selected: the rim");
    assert_eq!(hit_test(&board, Some(a), p(200.0, 210.0), 1.0), Hit::Label(a), "selected: a place to type");
    board.set_label(a, "the parser").expect("set");
    assert_eq!(hit_test(&board, None, p(200.0, 210.0), 1.0), Hit::Label(a));
    assert_eq!(hit_test(&board, None, p(200.0, 203.0), 1.0), Hit::Label(a), "right under the edge");
    assert_eq!(hit_test(&board, None, p(200.0, 197.0), 1.0), Hit::Rim(a, Side::Bottom), "inside the edge it is still the rim");
    assert_eq!(hit_test(&board, None, p(200.0, 230.0), 1.0), Hit::Empty);
}

// ---- moving, resizing

#[test]
fn dragging_a_cutout_moves_it_as_one_step_and_the_zoom_is_not_in_the_way() {
    let (mut board, a, _) = two();
    board.set_view(View { pan: p(0.0, 0.0), zoom: 2.0 });
    let mut input = BoardInput::default();
    // At zoom 2 the screen point (400, 300) is the board's (200, 150): the body of a.
    drag(&mut input, &mut board, mouse(), &[p(400.0, 300.0), p(420.0, 320.0), p(500.0, 360.0)]);
    // Moved by (100, 60) screen px = (50, 30) board px.
    assert_eq!(board.cutout(a).map(|c| c.rect.origin()), Some(p(150.0, 130.0)));
    assert_eq!(input.selection(), Some(a));
    assert!(board.undo(), "one step...");
    assert_eq!(board.cutout(a).map(|c| c.rect.origin()), Some(p(100.0, 100.0)));
    assert!(board.undo(), "...then the adding of the cutouts");
}

#[test]
fn a_press_that_hardly_moves_is_a_click_and_selects_without_moving() {
    let (mut board, a, b) = two();
    let mut input = BoardInput::default();
    drag(&mut input, &mut board, mouse(), &[p(200.0, 150.0), p(201.0, 151.0), p(202.0, 150.0)]);
    assert_eq!(board.cutout(a).map(|c| c.rect.origin()), Some(p(100.0, 100.0)));
    assert_eq!(input.selection(), Some(a));
    drag(&mut input, &mut board, mouse(), &[p(600.0, 150.0), p(600.0, 150.0)]);
    assert_eq!(input.selection(), Some(b));
    // A press on empty space with the mouse selects nothing.
    drag(&mut input, &mut board, mouse(), &[p(400.0, 600.0), p(400.0, 600.0)]);
    assert_eq!(input.selection(), None);
}

#[test]
fn the_cutout_pressed_comes_to_the_top() {
    let mut board = Board::new();
    let under = put(&mut board, 100.0, 100.0);
    let over = put(&mut board, 150.0, 120.0);
    let mut input = BoardInput::default();
    drag(&mut input, &mut board, mouse(), &[p(120.0, 160.0), p(120.0, 160.0)]);
    let order: Vec<ItemId> = board.items().iter().map(crate::whiteboard::Item::id).collect();
    assert_eq!(order, vec![over, under], "the one under, pressed in its part that is not covered, is on top now");
}

#[test]
fn the_handle_resizes_a_cutout_keeping_its_shape_and_it_is_one_step() {
    let (mut board, a, _) = two();
    let mut input = BoardInput::default();
    drag(&mut input, &mut board, mouse(), &[p(200.0, 150.0), p(200.0, 150.0)]); // select it
    drag(&mut input, &mut board, mouse(), &[p(295.0, 195.0), p(350.0, 400.0), p(400.0, 260.0)]);
    let r = board.cutout(a).map(|c| c.rect).unwrap_or_default();
    // The corner follows the pointer in width (the shape is kept: 2 to 1), the top-left stays.
    assert_eq!(r.origin(), p(100.0, 100.0));
    assert!((r.w - 300.0 - 5.0).abs() < 6.0 && (r.w / r.h - 2.0).abs() < 0.01, "{r:?}");
    assert!(board.undo());
    assert_eq!(board.cutout(a).map(|c| c.rect.size()), Some(Size::new(200.0, 100.0)));
}

#[test]
fn a_resize_cannot_make_a_cutout_smaller_than_the_least_there_is() {
    let (mut board, a, _) = two();
    let mut input = BoardInput::default();
    drag(&mut input, &mut board, mouse(), &[p(200.0, 150.0), p(200.0, 150.0)]);
    drag(&mut input, &mut board, mouse(), &[p(295.0, 195.0), p(110.0, 110.0)]);
    let r = board.cutout(a).map(|c| c.rect).unwrap_or_default();
    assert!(r.w >= board.limits().min_cutout - 1e-3 && r.h >= board.limits().min_cutout - 1e-3, "{r:?}");
}

// ---- arrows

#[test]
fn dragging_from_a_rim_to_another_cutout_draws_an_arrow_from_that_side_to_where_it_lands() {
    let (mut board, a, b) = two();
    let mut input = BoardInput::default();
    // From the right rim of a to the middle of b.
    let out = drag(&mut input, &mut board, mouse(), &[p(297.0, 150.0), p(400.0, 150.0), p(600.0, 150.0)]);
    assert_eq!(board.arrows().count(), 1);
    let arrow = board.arrows().next().cloned().expect("an arrow");
    assert_eq!(arrow.from, End::Item { id: a, anchor: Anchor::Side(Side::Right) });
    assert_eq!(arrow.to, End::Item { id: b, anchor: Anchor::Auto }, "the middle of the body: the line decides");
    assert!(matches!(out, Out::Redraw | Out::Nothing));
}

#[test]
fn landing_on_the_rim_of_the_other_cutout_fastens_to_that_side() {
    let (mut board, a, b) = two();
    let mut input = BoardInput::default();
    drag(&mut input, &mut board, mouse(), &[p(297.0, 150.0), p(400.0, 150.0), p(503.0, 150.0)]);
    let arrow = board.arrows().next().cloned().expect("an arrow");
    assert_eq!(arrow.from.item_id(), Some(a));
    assert_eq!(arrow.to, End::Item { id: b, anchor: Anchor::Side(Side::Left) });
}

#[test]
fn an_arrow_let_go_over_nothing_is_loose_at_that_end_and_a_short_drag_or_the_same_cutout_is_no_arrow() {
    let (mut board, a, _) = two();
    let mut input = BoardInput::default();
    drag(&mut input, &mut board, mouse(), &[p(297.0, 150.0), p(350.0, 400.0), p(400.0, 450.0)]);
    let arrow = board.arrows().next().cloned().expect("an arrow");
    assert_eq!(arrow.from, End::Item { id: a, anchor: Anchor::Side(Side::Right) });
    assert_eq!(arrow.to, End::Free(p(400.0, 450.0)));
    // A drag of 10 px from a rim: no arrow.
    drag(&mut input, &mut board, mouse(), &[p(297.0, 120.0), p(305.0, 125.0), p(307.0, 125.0)]);
    assert_eq!(board.arrows().count(), 1);
    // Back onto the same cutout: no arrow.
    drag(&mut input, &mut board, mouse(), &[p(297.0, 170.0), p(350.0, 170.0), p(250.0, 150.0)]);
    assert_eq!(board.arrows().count(), 1);
}

#[test]
fn the_pen_draws_an_arrow_as_one_stroke_between_two_cutouts_and_its_curve_is_kept() {
    let (mut board, a, b) = two();
    let mut input = BoardInput::default();
    // From just outside a to the body of b, bowing downwards (to the right of the way it goes).
    let stroke: Vec<Pt> = (0..=24)
        .map(|i| {
            let t = i as f32 / 24.0;
            p(305.0 + 250.0 * t, 160.0 + 4.0 * 55.0 * t * (1.0 - t))
        })
        .collect();
    drag(&mut input, &mut board, PointerKind::Pen, &stroke);
    let arrow = board.arrows().next().cloned().expect("an arrow");
    assert_eq!(arrow.from.item_id(), Some(a));
    assert_eq!(arrow.to.item_id(), Some(b));
    assert!(arrow.bend > 0.25 && arrow.bend < 0.6, "it bows to the right of its way: {}", arrow.bend);
}

#[test]
fn a_pen_stroke_that_ends_on_a_cutout_from_empty_space_hangs_loose_at_its_start() {
    let (mut board, _, b) = two();
    let mut input = BoardInput::default();
    let stroke: Vec<Pt> = (0..=10).map(|i| p(400.0 + 10.0 * i as f32, 300.0 - 15.0 * i as f32)).collect();
    drag(&mut input, &mut board, PointerKind::Pen, &stroke);
    let arrow = board.arrows().next().cloned().expect("an arrow");
    assert_eq!(arrow.from, End::Free(p(400.0, 300.0)));
    assert_eq!(arrow.to.item_id(), Some(b));
}

#[test]
fn a_pen_stroke_between_nothing_and_nothing_is_no_arrow_and_says_why_and_a_tap_says_nothing() {
    let (mut board, _, _) = two();
    let mut input = BoardInput::default();
    let far: Vec<Pt> = (0..=10).map(|i| p(100.0 + 20.0 * i as f32, 500.0)).collect();
    let out = drag(&mut input, &mut board, PointerKind::Pen, &far);
    assert_eq!(out, Out::Notice(Notice::ArrowNeedsACutout));
    assert_eq!(board.arrows().count(), 0);
    let tap = [p(100.0, 500.0), p(101.0, 500.0), p(101.0, 501.0)];
    assert_eq!(drag(&mut input, &mut board, PointerKind::Pen, &tap), Out::Nothing);
}

#[test]
fn the_arrow_being_drawn_is_shown_as_it_goes_with_the_cutout_it_would_land_on() {
    let (mut board, a, b) = two();
    let mut input = BoardInput::default();
    input.press(&mut board, p(297.0, 150.0), mouse(), no_mods());
    input.motion(&mut board, p(380.0, 250.0), no_mods());
    let free = input.preview(&board).expect("a preview");
    assert_eq!(free.target, None);
    assert_eq!(free.to, p(380.0, 250.0));
    input.motion(&mut board, p(560.0, 150.0), no_mods());
    let over = input.preview(&board).expect("a preview");
    assert_eq!(over.target, Some(b), "it would land on b");
    assert_ne!(over.from, over.to);
    let _ = a;
    input.release(&mut board, p(560.0, 150.0), no_mods());
    assert!(input.preview(&board).is_none(), "nothing is being drawn any more");
}

#[test]
fn a_stroke_in_progress_can_be_shown_and_escape_throws_it_away() {
    let (mut board, _, _) = two();
    let mut input = BoardInput::default();
    input.press(&mut board, p(100.0, 500.0), PointerKind::Pen, no_mods());
    input.motion(&mut board, p(150.0, 510.0), no_mods());
    input.motion(&mut board, p(200.0, 520.0), no_mods());
    assert_eq!(input.stroke().len(), 3);
    input.cancel(&mut board);
    assert!(input.stroke().is_empty());
    assert_eq!(input.release(&mut board, p(200.0, 520.0), no_mods()), Out::Nothing);
    assert_eq!(board.arrows().count(), 0);
}

#[test]
fn an_end_of_the_selected_arrow_can_be_dragged_to_another_cutout_or_let_loose() {
    let mut board = Board::new();
    let a = put(&mut board, 100.0, 100.0);
    let b = put(&mut board, 500.0, 100.0);
    let c = put(&mut board, 500.0, 400.0);
    let arrow = board.add_arrow(End::item(a), End::item(b), 0.0, "").expect("added");
    let mut input = BoardInput::default();
    // Select the arrow by its middle.
    drag(&mut input, &mut board, mouse(), &[p(400.0, 152.0), p(400.0, 152.0)]);
    assert_eq!(input.selection(), Some(arrow));
    // Take its head (at b's edge, (500, 150)) to c.
    drag(&mut input, &mut board, mouse(), &[p(499.0, 150.0), p(520.0, 300.0), p(600.0, 450.0)]);
    assert_eq!(board.arrow(arrow).map(|x| x.to.item_id()), Some(Some(c)));
    // And let its tail go loose: where it is now is where the arrow leaves a on its way to c.
    let tail = board.place_arrow(arrow).map(|x| x.from).unwrap_or_default();
    drag(&mut input, &mut board, mouse(), &[tail, p(250.0, 300.0), p(200.0, 350.0)]);
    assert_eq!(board.arrow(arrow).map(|x| x.from), Some(End::Free(p(200.0, 350.0))));
    assert!(board.undo() && board.undo(), "each a step of its own");
    assert_eq!(board.arrow(arrow).map(|x| x.to.item_id()), Some(Some(b)));
}

#[test]
fn the_middle_of_the_selected_arrow_bends_it_and_the_bend_is_one_step() {
    let (mut board, a, b) = two();
    let arrow = board.add_arrow(End::item(a), End::item(b), 0.0, "").expect("added");
    let mut input = BoardInput::default();
    drag(&mut input, &mut board, mouse(), &[p(400.0, 152.0), p(400.0, 152.0)]);
    // Pull the middle 50 px down (to the right of its way): a sagitta of 50 over a chord of 200 is a bend of 0.5.
    drag(&mut input, &mut board, mouse(), &[p(400.0, 150.0), p(400.0, 175.0), p(400.0, 200.0)]);
    let bend = board.arrow(arrow).map_or(0.0, |x| x.bend);
    assert!((bend - 0.5).abs() < 0.05, "{bend}");
    assert!(board.undo());
    assert_eq!(board.arrow(arrow).map(|x| x.bend), Some(0.0));
}

// ---- the rest

#[test]
fn the_mouse_on_empty_space_pans_the_view_and_the_view_is_no_step() {
    let (mut board, _, _) = two();
    let mut input = BoardInput::default();
    drag(&mut input, &mut board, mouse(), &[p(300.0, 600.0), p(250.0, 580.0), p(200.0, 560.0)]);
    assert_eq!(board.view().pan, p(100.0, 40.0), "the board moved with the hand: the pan is the other way");
    assert_eq!(board.view().zoom, 1.0);
    assert!(board.undo(), "the only step is the adding of the first cutout...");
    assert!(board.undo());
    assert!(!board.undo());
}

#[test]
fn the_wheel_pans_and_with_the_command_key_zooms_around_the_pointer_within_the_range() {
    let (mut board, _, _) = two();
    let mut input = BoardInput::default();
    input.wheel(&mut board, p(100.0, 100.0), p(0.0, 30.0), no_mods());
    assert_eq!(board.view().pan, p(0.0, 30.0));
    let before = board.view().to_board(p(300.0, 200.0));
    input.wheel(&mut board, p(300.0, 200.0), p(0.0, -100.0), Mods { cmd: true, ..Mods::default() });
    assert!(board.view().zoom > 1.0, "{}", board.view().zoom);
    let after = board.view().to_board(p(300.0, 200.0));
    assert!((before.x - after.x).abs() < 1e-2 && (before.y - after.y).abs() < 1e-2, "the point under the pointer stays");
    for _ in 0..200 {
        input.wheel(&mut board, p(300.0, 200.0), p(0.0, -400.0), Mods { cmd: true, ..Mods::default() });
    }
    assert_eq!(board.view().zoom, board.limits().zoom_max);
}

#[test]
fn a_double_click_on_a_cutout_jumps_to_it_and_on_its_label_edits_it_and_on_nothing_does_nothing() {
    let (mut board, a, b) = two();
    let mut input = BoardInput::default();
    assert_eq!(input.double_click(&mut board, p(200.0, 150.0)), Out::Jump(a));
    assert_eq!(input.double_click(&mut board, p(103.0, 150.0)), Out::Jump(a), "also on the rim");
    assert_eq!(input.double_click(&mut board, p(600.0, 210.0)), Out::Nothing, "no label there yet: only the outer band of the rim");
    board.set_label(b, "x").expect("set");
    assert_eq!(input.double_click(&mut board, p(600.0, 210.0)), Out::EditLabel(b));
    assert_eq!(input.double_click(&mut board, p(400.0, 600.0)), Out::Nothing);
}

#[test]
fn delete_takes_the_selection_off_with_its_arrows_in_one_step() {
    let (mut board, a, b) = two();
    board.add_arrow(End::item(a), End::item(b), 0.0, "").expect("added");
    let mut input = BoardInput::default();
    drag(&mut input, &mut board, mouse(), &[p(200.0, 150.0), p(200.0, 150.0)]);
    assert_eq!(input.delete_selection(&mut board), Out::Redraw);
    assert_eq!(board.len(), 1);
    assert_eq!(input.selection(), None);
    assert!(board.undo());
    assert_eq!(board.len(), 3);
    assert_eq!(input.delete_selection(&mut board), Out::Nothing, "nothing selected");
}

#[test]
fn escape_during_a_drag_puts_the_cutout_back() {
    let (mut board, a, _) = two();
    let mut input = BoardInput::default();
    input.press(&mut board, p(200.0, 150.0), mouse(), no_mods());
    input.motion(&mut board, p(260.0, 220.0), no_mods());
    assert_eq!(board.cutout(a).map(|c| c.rect.origin()), Some(p(160.0, 170.0)));
    input.cancel(&mut board);
    assert_eq!(board.cutout(a).map(|c| c.rect.origin()), Some(p(100.0, 100.0)));
    input.release(&mut board, p(260.0, 220.0), no_mods());
    assert!(board.undo(), "only the adding of the cutouts is in the history");
}

#[test]
fn what_the_pointer_is_over_says_which_cursor_to_show() {
    let (mut board, a, b) = two();
    board.add_arrow(End::item(a), End::item(b), 0.0, "").expect("added");
    let mut input = BoardInput::default();
    assert_eq!(input.hover(&board, p(200.0, 150.0)), Cursor::Move);
    assert_eq!(input.hover(&board, p(297.0, 150.0)), Cursor::Connect);
    assert_eq!(input.hover(&board, p(400.0, 600.0)), Cursor::Default);
    assert_eq!(input.hover(&board, p(400.0, 152.0)), Cursor::Pointer, "an arrow");
    drag(&mut input, &mut board, mouse(), &[p(200.0, 150.0), p(200.0, 150.0)]);
    assert_eq!(input.hover(&board, p(295.0, 195.0)), Cursor::Resize);
}

// ---- a full board

/// 250 cutouts in a grid and 250 arrows between neighbours: the biggest board the caps allow, nearly.
fn full_board() -> Board {
    let mut board = Board::new();
    let mut ids = Vec::new();
    for i in 0..250 {
        let (col, row) = (i % 25, i / 25);
        ids.push(put(&mut board, col as f32 * 260.0, row as f32 * 160.0));
    }
    for i in 0..249 {
        board.add_arrow(End::item(ids[i]), End::item(ids[i + 1]), 0.1, "").expect("an arrow");
    }
    board
}

#[test]
fn what_is_under_the_pointer_is_found_in_well_under_a_millisecond_on_a_full_board() {
    let board = full_board();
    assert!(board.len() >= 499);
    let began = std::time::Instant::now();
    let mut found = 0;
    for i in 0..2_000 {
        let p = p(50.0 + (i % 100) as f32 * 61.0, 40.0 + (i / 100) as f32 * 190.0);
        if hit_test(&board, None, p, 1.0) != Hit::Empty {
            found += 1;
        }
    }
    let per = began.elapsed().as_secs_f64() * 1000.0 / 2_000.0;
    eprintln!("hit test on {} items: {per:.4} ms, {found} of 2000 points over something", board.len());
    assert!(per < 1.0, "{per} ms for one hit test");
    assert!(found > 100);
}

#[test]
fn a_gesture_on_a_full_board_costs_no_more_than_a_frame_of_it() {
    let mut board = full_board();
    let first = board.cutouts().next().map(|c| c.id).expect("a cutout");
    let mut input = BoardInput::default();
    let began = std::time::Instant::now();
    input.press(&mut board, p(120.0, 140.0), mouse(), no_mods());
    for i in 1..=60 {
        input.motion(&mut board, p(120.0 + i as f32 * 3.0, 140.0 + i as f32), no_mods());
    }
    input.release(&mut board, p(300.0, 200.0), no_mods());
    let per = began.elapsed().as_secs_f64() * 1000.0 / 62.0;
    eprintln!("a drag on {} items: {per:.4} ms per event", board.len());
    assert!(per < 2.0, "{per} ms per event");
    assert!(board.cutout(first).is_some());
}
