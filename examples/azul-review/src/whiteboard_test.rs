//! The whiteboard's model: cutouts and arrows, the caps with their notices, the steps Mod+Z takes
//! back, the arrows that follow the boxes they hang on. No window, no azul.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use super::*;

fn image(w: u32, h: u32, scale: f32) -> Arc<CutoutImage> {
    Arc::new(CutoutImage {
        width: w,
        height: h,
        scale,
        rgba: vec![200; (w * h * 4) as usize],
        png: vec![1, 2, 3],
    })
}

fn source(first: u32, last: u32) -> Source {
    Source {
        file: "src/lib.rs".to_string(),
        page: 0,
        first_line: first,
        last_line: last,
        shape: vec![Pt::new(10.0, 10.0), Pt::new(90.0, 10.0), Pt::new(90.0, 50.0), Pt::new(10.0, 50.0)],
        file_hash: 11,
        range_hash: 22,
        lines: vec!["fn a() {}".to_string(), "fn b() {}".to_string()],
    }
}

/// A cutout of 200 x 100 board px (a picture of 400 x 200 drawn at 2x) with its top-left at (x, y).
fn cutout(board: &mut Board, x: f32, y: f32) -> ItemId {
    board
        .add_cutout(CutoutDraft::ready(Pt::new(x, y), image(400, 200, 2.0), source(10, 20)))
        .expect("room for a cutout")
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn close_pt(a: Pt, b: Pt) -> bool {
    close(a.x, b.x) && close(a.y, b.y)
}

fn order(board: &Board) -> Vec<ItemId> {
    board.items().iter().map(Item::id).collect()
}

#[test]
fn a_cutout_has_the_size_of_its_picture_at_the_scale_it_was_drawn_and_lies_on_top() {
    let mut b = Board::new();
    let a = cutout(&mut b, 10.0, 20.0);
    let c = cutout(&mut b, 50.0, 60.0);
    assert_eq!(b.cutout(a).map(|c| c.rect), Some(Rect::new(10.0, 20.0, 200.0, 100.0)));
    assert_eq!(order(&b), vec![a, c], "the newest is on top");
    assert_ne!(a, c);
}

#[test]
fn a_cutout_whose_picture_is_still_being_drawn_takes_the_size_it_was_planned_with() {
    let mut b = Board::new();
    let id = b
        .add_cutout(CutoutDraft::pending(Pt::new(0.0, 0.0), Size::new(300.0, 120.0), source(1, 3)))
        .expect("added");
    assert_eq!(b.cutout(id).map(|c| (c.rect.w, c.rect.h)), Some((300.0, 120.0)));
    assert!(matches!(b.cutout(id).map(|c| &c.image), Some(ImageSlot::Pending)));
    assert!(b.set_image(id, ImageSlot::Ready(image(600, 240, 2.0))));
    assert!(matches!(b.cutout(id).map(|c| &c.image), Some(ImageSlot::Ready(_))));
}

#[test]
fn a_size_past_the_limits_is_fitted_with_its_shape_kept() {
    let mut b = Board::new();
    let huge = b
        .add_cutout(CutoutDraft::pending(Pt::new(0.0, 0.0), Size::new(8000.0, 2000.0), source(1, 3)))
        .expect("added");
    let r = b.cutout(huge).map(|c| c.rect).unwrap_or_default();
    assert!(close(r.w, LIMITS.max_cutout) && close(r.h, LIMITS.max_cutout / 4.0), "{r:?}");
    let thin = b
        .add_cutout(CutoutDraft::pending(Pt::new(0.0, 0.0), Size::new(4.0, 2.0), source(1, 3)))
        .expect("added");
    let r = b.cutout(thin).map(|c| c.rect).unwrap_or_default();
    assert!(close(r.h, LIMITS.min_cutout) && close(r.w, 2.0 * LIMITS.min_cutout), "{r:?}");
}

#[test]
fn an_id_is_never_given_twice_even_after_its_item_is_gone() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0);
    b.remove(a).expect("removed");
    let c = cutout(&mut b, 0.0, 0.0);
    assert!(c > a, "{c} after {a}");
    assert!(b.undo(), "the second cutout is taken back");
    assert!(b.undo(), "and the removal of the first");
    assert!(b.cutout(a).is_some());
    let d = cutout(&mut b, 0.0, 0.0);
    assert!(d > c, "{d} after {c}, which was taken back");
}

#[test]
fn the_board_refuses_an_item_past_its_cap_and_says_which() {
    let mut b = Board::with_limits(Limits { max_items: 3, ..LIMITS });
    let a = cutout(&mut b, 0.0, 0.0);
    let c = cutout(&mut b, 300.0, 0.0);
    b.add_arrow(End::item(a), End::item(c), 0.0, "").expect("an arrow counts as an item");
    let refused = b.add_cutout(CutoutDraft::ready(Pt::default(), image(40, 40, 1.0), source(1, 1)));
    assert_eq!(refused, Err(Notice::TooManyItems { limit: 3 }));
    assert_eq!(b.len(), 3, "nothing was added");
}

#[test]
fn one_picture_and_all_the_pictures_together_have_a_pixel_cap() {
    let mut b = Board::with_limits(Limits { max_image_px: 1_000, max_total_px: 3_000, ..LIMITS });
    let big = b.add_cutout(CutoutDraft::ready(Pt::default(), image(40, 40, 1.0), source(1, 1)));
    assert_eq!(big, Err(Notice::PictureTooBig { px: 1_600, limit_px: 1_000 }));
    let ok = |b: &mut Board| b.add_cutout(CutoutDraft::ready(Pt::default(), image(30, 30, 1.0), source(1, 1)));
    let first = ok(&mut b).expect("900 px");
    ok(&mut b).expect("1800 px");
    ok(&mut b).expect("2700 px");
    assert_eq!(ok(&mut b), Err(Notice::PicturesTooBig { limit_px: 3_000 }), "3600 px");
    assert_eq!(b.picture_pixels(), 2_700);
    b.remove(first).expect("removed");
    assert!(ok(&mut b).is_ok(), "room again");
}

#[test]
fn a_picture_whose_bytes_do_not_fit_its_size_is_refused() {
    let mut b = Board::new();
    let mut bad = (*image(10, 10, 1.0)).clone();
    bad.rgba.truncate(10);
    assert_eq!(
        b.add_cutout(CutoutDraft::ready(Pt::default(), Arc::new(bad), source(1, 1))),
        Err(Notice::PictureInvalid)
    );
    let mut flat = (*image(10, 10, 1.0)).clone();
    flat.scale = 0.0;
    assert_eq!(
        b.add_cutout(CutoutDraft::ready(Pt::default(), Arc::new(flat), source(1, 1))),
        Err(Notice::PictureInvalid)
    );
}

#[test]
fn a_label_is_cut_to_its_length_by_characters_and_the_board_says_so() {
    let mut b = Board::with_limits(Limits { max_label_chars: 5, ..LIMITS });
    let id = cutout(&mut b, 0.0, 0.0);
    b.set_label(id, "äöüßéxyz").expect("set");
    assert_eq!(b.cutout(id).map(|c| c.label.as_str()), Some("äöüßé"));
    assert_eq!(b.take_notices(), vec![Notice::LabelShortened { limit: 5 }]);
    assert!(b.take_notices().is_empty(), "told once");
}

#[test]
fn a_label_change_is_a_step_unless_the_label_is_the_same() {
    let mut b = Board::new();
    let id = cutout(&mut b, 0.0, 0.0);
    b.set_label(id, "the parser").expect("set");
    b.set_label(id, "the parser").expect("set again");
    assert!(b.undo());
    assert_eq!(b.cutout(id).map(|c| c.label.clone()), Some(String::new()));
    assert!(b.redo());
    assert_eq!(b.cutout(id).map(|c| c.label.clone()), Some("the parser".to_string()));
    assert!(b.undo() && b.undo(), "the label and then the cutout: two steps, not three");
    assert!(!b.undo());
}

#[test]
fn adding_a_cutout_is_taken_back_and_brought_again_in_its_place() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0);
    let c = cutout(&mut b, 300.0, 0.0);
    assert!(b.undo());
    assert_eq!(order(&b), vec![a]);
    assert!(b.redo());
    assert_eq!(order(&b), vec![a, c]);
    assert_eq!(b.cutout(c).map(|c| c.rect.x), Some(300.0));
    assert!(!b.redo(), "nothing more to bring again");
}

#[test]
fn a_new_step_ends_what_could_have_been_brought_again() {
    let mut b = Board::new();
    cutout(&mut b, 0.0, 0.0);
    assert!(b.undo());
    assert!(b.can_redo());
    cutout(&mut b, 5.0, 5.0);
    assert!(!b.can_redo());
}

#[test]
fn a_picture_that_arrives_while_its_cutout_is_taken_back_is_there_when_it_comes_again() {
    let mut b = Board::new();
    let id = b
        .add_cutout(CutoutDraft::pending(Pt::default(), Size::new(200.0, 100.0), source(1, 2)))
        .expect("added");
    assert!(b.undo());
    assert!(b.cutout(id).is_none());
    assert!(b.set_image(id, ImageSlot::Ready(image(400, 200, 2.0))), "found in the history");
    assert!(b.redo());
    assert!(matches!(b.cutout(id).map(|c| &c.image), Some(ImageSlot::Ready(_))));
    assert!(!b.set_image(9_999, ImageSlot::Pending), "no such cutout");
}

#[test]
fn a_drag_is_one_step_whatever_the_number_of_moves() {
    let mut b = Board::new();
    let id = cutout(&mut b, 10.0, 10.0);
    for i in 1..=5 {
        b.move_live(id, Pt::new(10.0 + 20.0 * i as f32, 10.0));
    }
    assert_eq!(b.cutout(id).map(|c| c.rect.x), Some(110.0), "the cutout follows at once");
    assert!(b.commit_live());
    assert!(b.undo());
    assert_eq!(b.cutout(id).map(|c| c.rect.x), Some(10.0));
    assert!(b.redo());
    assert_eq!(b.cutout(id).map(|c| c.rect.x), Some(110.0));
    assert!(b.undo() && b.undo(), "the move and the adding: two steps");
    assert!(!b.undo());
}

#[test]
fn a_drag_that_ends_where_it_began_is_no_step_and_a_cancelled_one_puts_the_cutout_back() {
    let mut b = Board::new();
    let id = cutout(&mut b, 10.0, 10.0);
    b.move_live(id, Pt::new(50.0, 50.0));
    b.move_live(id, Pt::new(10.0, 10.0));
    assert!(!b.commit_live(), "no change, no step");
    b.move_live(id, Pt::new(70.0, 70.0));
    b.cancel_live();
    assert_eq!(b.cutout(id).map(|c| c.rect.origin()), Some(Pt::new(10.0, 10.0)));
    assert!(!b.commit_live(), "nothing left to commit");
}

#[test]
fn a_resize_is_kept_inside_the_size_limits_and_is_one_step() {
    let mut b = Board::new();
    let id = cutout(&mut b, 10.0, 10.0);
    b.resize_live(id, Rect::new(10.0, 10.0, 5.0, 5000.0));
    let r = b.cutout(id).map(|c| c.rect).unwrap_or_default();
    assert!(close(r.w, LIMITS.min_cutout) && close(r.h, LIMITS.max_cutout), "{r:?}");
    b.resize_live(id, Rect::new(10.0, 10.0, 300.0, 150.0));
    assert!(b.commit_live());
    assert!(b.undo());
    assert_eq!(b.cutout(id).map(|c| c.rect), Some(Rect::new(10.0, 10.0, 200.0, 100.0)));
}

#[test]
fn a_position_that_is_no_number_or_far_outside_is_pulled_in() {
    let mut b = Board::new();
    let id = cutout(&mut b, f32::NAN, 1e12);
    let r = b.cutout(id).map(|c| c.rect).unwrap_or_default();
    assert_eq!(r.x, 0.0);
    assert_eq!(r.y, LIMITS.max_coord);
    b.move_live(id, Pt::new(f32::INFINITY, -1e12));
    let r = b.cutout(id).map(|c| c.rect).unwrap_or_default();
    assert_eq!((r.x, r.y), (LIMITS.max_coord, -LIMITS.max_coord));
}

#[test]
fn an_arrow_between_two_cutouts_runs_from_edge_to_edge_on_the_line_between_their_centres() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0); // centre (100, 50)
    let c = cutout(&mut b, 400.0, 0.0); // centre (500, 50)
    let arrow = b.add_arrow(End::item(a), End::item(c), 0.0, "calls").expect("added");
    let p = b.place_arrow(arrow).expect("placed");
    assert!(close_pt(p.from, Pt::new(200.0, 50.0)), "{:?}", p.from);
    assert!(close_pt(p.to, Pt::new(400.0, 50.0)), "{:?}", p.to);
    assert!(close_pt(p.curve.at(0.5), Pt::new(300.0, 50.0)));
}

#[test]
fn an_arrow_follows_the_cutout_it_hangs_on() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0);
    let c = cutout(&mut b, 400.0, 0.0);
    let arrow = b.add_arrow(End::item(a), End::item(c), 0.0, "").expect("added");
    b.move_live(c, Pt::new(400.0, 300.0)); // centre (500, 350)
    let p = b.place_arrow(arrow).expect("placed");
    // Leaves the first box on the way to the second one's centre, which is down and to the right:
    // through its bottom side, a third of the way along the half of its width.
    assert!(close(p.from.y, 100.0) && p.from.x > 100.0 && p.from.x < 200.0, "{:?}", p.from);
    assert!(close(p.to.y, 300.0), "it ends on the top of the second box: {:?}", p.to);
    assert!(b.cutout(c).is_some_and(|c| c.rect.contains(p.to)));
}

#[test]
fn an_anchor_on_a_side_starts_from_the_middle_of_that_side_and_a_free_end_stays_put() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0);
    let from = End::Item { id: a, anchor: Anchor::Side(Side::Bottom) };
    let arrow = b.add_arrow(from, End::Free(Pt::new(100.0, 400.0)), 0.0, "").expect("added");
    let p = b.place_arrow(arrow).expect("placed");
    assert!(close_pt(p.from, Pt::new(100.0, 100.0)));
    assert!(close_pt(p.to, Pt::new(100.0, 400.0)));
    let at = End::Item { id: a, anchor: Anchor::Point { u: 0.25, v: 0.5 } };
    let arrow = b.add_arrow(at, End::Free(Pt::new(100.0, 400.0)), 0.0, "").expect("added");
    assert!(close_pt(b.place_arrow(arrow).map(|p| p.from).unwrap_or_default(), Pt::new(50.0, 50.0)));
}

#[test]
fn an_arrow_may_not_loop_onto_its_own_cutout_nor_float_free_nor_hang_on_nothing_nor_repeat() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0);
    let c = cutout(&mut b, 400.0, 0.0);
    assert_eq!(b.add_arrow(End::item(a), End::item(a), 0.0, ""), Err(Notice::ArrowToItself));
    assert_eq!(
        b.add_arrow(End::Free(Pt::new(1.0, 1.0)), End::Free(Pt::new(9.0, 9.0)), 0.0, ""),
        Err(Notice::ArrowNeedsACutout)
    );
    assert_eq!(b.add_arrow(End::item(a), End::item(777), 0.0, ""), Err(Notice::NoSuchItem { id: 777 }));
    b.add_arrow(End::item(a), End::item(c), 0.0, "").expect("the first");
    assert_eq!(b.add_arrow(End::item(a), End::item(c), 0.0, ""), Err(Notice::ArrowRepeated));
    b.add_arrow(End::item(c), End::item(a), 0.0, "").expect("the other way is another arrow");
    assert_eq!(b.len(), 4);
}

#[test]
fn the_bend_of_an_arrow_is_kept_inside_what_a_curve_can_do() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0);
    let c = cutout(&mut b, 400.0, 0.0);
    let arrow = b.add_arrow(End::item(a), End::item(c), 9.0, "").expect("added");
    assert!(b.arrow(arrow).is_some_and(|a| a.bend <= crate::whiteboard_geom::MAX_BEND + 1e-4));
    let nan = b.add_arrow(End::item(c), End::item(a), f32::NAN, "").expect("added");
    assert_eq!(b.arrow(nan).map(|a| a.bend), Some(0.0));
}

#[test]
fn removing_a_cutout_takes_its_arrows_and_one_step_brings_everything_back_in_place() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0);
    let c = cutout(&mut b, 400.0, 0.0);
    let d = cutout(&mut b, 800.0, 0.0);
    let ac = b.add_arrow(End::item(a), End::item(c), 0.0, "ac").expect("added");
    let cd = b.add_arrow(End::item(c), End::item(d), 0.0, "cd").expect("added");
    let ad = b.add_arrow(End::item(a), End::item(d), 0.0, "ad").expect("added");
    let before = order(&b);
    let gone = b.remove(c).expect("removed");
    assert_eq!(gone, vec![c, ac, cd], "the cutout and the arrows that hung on it");
    assert_eq!(order(&b), vec![a, d, ad]);
    assert!(b.undo(), "one step");
    assert_eq!(order(&b), before, "every item where it was");
    assert!(b.arrow(ac).is_some_and(|x| x.label == "ac"));
    assert!(b.redo());
    assert_eq!(order(&b), vec![a, d, ad]);
}

#[test]
fn removing_an_arrow_leaves_the_cutouts_and_something_that_is_not_there_is_told() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0);
    let c = cutout(&mut b, 400.0, 0.0);
    let arrow = b.add_arrow(End::item(a), End::item(c), 0.0, "").expect("added");
    assert_eq!(b.remove(arrow), Ok(vec![arrow]));
    assert_eq!(order(&b), vec![a, c]);
    assert_eq!(b.remove(arrow), Err(Notice::NoSuchItem { id: arrow }));
}

#[test]
fn rewiring_an_end_hangs_the_arrow_on_another_cutout_and_undo_puts_it_back() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0);
    let c = cutout(&mut b, 400.0, 0.0);
    let d = cutout(&mut b, 800.0, 0.0);
    let arrow = b.add_arrow(End::item(a), End::item(c), 0.0, "").expect("added");
    b.rewire(arrow, Which::To, End::item(d)).expect("rewired");
    assert_eq!(b.arrow(arrow).map(|a| a.to), Some(End::item(d)));
    assert_eq!(b.rewire(arrow, Which::To, End::item(a)), Err(Notice::ArrowToItself));
    assert_eq!(b.rewire(arrow, Which::From, End::item(555)), Err(Notice::NoSuchItem { id: 555 }));
    assert!(b.undo());
    assert_eq!(b.arrow(arrow).map(|a| a.to), Some(End::item(c)));
    b.set_bend(arrow, 0.3).expect("bent");
    assert!(b.arrow(arrow).is_some_and(|a| close(a.bend, 0.3)));
    assert!(b.undo());
    assert!(b.arrow(arrow).is_some_and(|a| a.bend == 0.0));
}

#[test]
fn raising_a_cutout_puts_it_on_top_without_being_a_step() {
    let mut b = Board::new();
    let a = cutout(&mut b, 0.0, 0.0);
    let c = cutout(&mut b, 5.0, 5.0);
    b.raise(a);
    assert_eq!(order(&b), vec![c, a]);
    assert!(b.undo(), "the undo is for the adding of the second cutout");
    assert_eq!(order(&b), vec![a]);
}

#[test]
fn the_history_keeps_the_last_steps_only() {
    let mut b = Board::with_limits(Limits { history: 3, ..LIMITS });
    for i in 0..6 {
        cutout(&mut b, i as f32 * 10.0, 0.0);
    }
    assert!(b.undo() && b.undo() && b.undo());
    assert!(!b.undo(), "the first three are no longer to be taken back");
    assert_eq!(b.len(), 3);
}

#[test]
fn the_board_knows_when_it_has_to_be_saved_and_the_view_alone_does_not_ask() {
    let mut b = Board::new();
    assert!(!b.needs_save());
    let id = cutout(&mut b, 0.0, 0.0);
    assert!(b.needs_save());
    b.mark_saved();
    assert!(!b.needs_save());
    b.set_view(View { pan: Pt::new(50.0, 20.0), zoom: 2.0 });
    assert!(!b.needs_save(), "panning is kept with the next save, it does not cause one");
    assert!(b.view_unsaved());
    b.move_live(id, Pt::new(9.0, 9.0));
    assert!(b.needs_save(), "a moving cutout is a change");
    b.mark_saved();
    assert!(!b.view_unsaved());
    let before = b.content_revision();
    b.set_view(View::default());
    assert_eq!(b.content_revision(), before, "the view is no content");
}

#[test]
fn the_zoom_of_the_view_stays_inside_its_range() {
    let mut b = Board::new();
    b.set_view(View { pan: Pt::new(f32::NAN, 5.0), zoom: 100.0 });
    assert_eq!(b.view().zoom, LIMITS.zoom_max);
    assert_eq!(b.view().pan, Pt::new(0.0, 5.0));
    b.set_view(View { pan: Pt::default(), zoom: -1.0 });
    assert_eq!(b.view().zoom, LIMITS.zoom_min);
}

struct Log(Rc<RefCell<Vec<BoardEvent>>>);

impl BoardObserver for Log {
    fn on_event(&mut self, event: &BoardEvent) {
        self.0.borrow_mut().push(event.clone());
    }
}

#[test]
fn an_observer_hears_what_happens_on_the_board_and_with_none_nothing_is_needed() {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut b = Board::new();
    b.set_observer(Box::new(Log(heard.clone())));
    let a = cutout(&mut b, 0.0, 0.0);
    let c = cutout(&mut b, 400.0, 0.0);
    let arrow = b.add_arrow(End::item(a), End::item(c), 0.0, "").expect("added");
    b.move_live(c, Pt::new(500.0, 0.0));
    b.commit_live();
    b.resize_live(c, Rect::new(500.0, 0.0, 100.0, 50.0));
    b.commit_live();
    b.set_label(a, "x").expect("set");
    b.remove(a).expect("removed");
    b.undo();
    let events = heard.borrow().clone();
    assert_eq!(
        events,
        vec![
            BoardEvent::CutoutCreated { id: a, file: "src/lib.rs".to_string(), first_line: 10, last_line: 20 },
            BoardEvent::CutoutCreated { id: c, file: "src/lib.rs".to_string(), first_line: 10, last_line: 20 },
            BoardEvent::ArrowDrawn { id: arrow, from: Some(a), to: Some(c) },
            BoardEvent::CutoutMoved { id: c },
            BoardEvent::CutoutResized { id: c },
            BoardEvent::Labelled { id: a },
            BoardEvent::Removed { ids: vec![a, arrow] },
            BoardEvent::Undone,
        ]
    );
    // The default observer hears nothing and costs nothing.
    let mut quiet = Board::new();
    cutout(&mut quiet, 0.0, 0.0);
}

#[test]
fn the_hash_of_lines_is_stable_and_tells_different_lines_apart() {
    let a = lines_hash(&["fn a() {}".to_string(), "fn b() {}".to_string()]);
    assert_eq!(a, lines_hash(&["fn a() {}".to_string(), "fn b() {}".to_string()]));
    assert_ne!(a, lines_hash(&["fn a() {}fn b() {}".to_string()]), "where the lines break matters");
    assert_ne!(a, lines_hash(&["fn a() {}".to_string()]));
    // The empty text is FNV-1a's offset basis, a fixed number the saved boards can rely on.
    assert_eq!(lines_hash(&[]), 0xcbf2_9ce4_8422_2325);
}

#[test]
fn a_source_is_cut_to_what_the_board_keeps() {
    let limits = Limits { max_shape_points: 10, max_source_lines: 3, max_source_line_chars: 8, ..LIMITS };
    let raw = Source {
        file: "src\\deep\\lib.rs".to_string(),
        page: 2,
        first_line: 30,
        last_line: 12,
        shape: (0..100).map(|i| Pt::new(i as f32, (i % 7) as f32)).chain([Pt::new(f32::NAN, 1.0)]).collect(),
        file_hash: 1,
        range_hash: 2,
        lines: (0..9).map(|i| format!("line number {i}")).collect(),
    };
    let s = raw.sanitized(&limits);
    assert_eq!(s.file, "src/deep/lib.rs");
    assert_eq!((s.first_line, s.last_line), (12, 30), "the lines run the right way round");
    assert!(s.shape.len() <= 10 && s.shape.len() >= 3, "{}", s.shape.len());
    assert!(s.shape.iter().all(|p| p.is_finite()));
    assert_eq!(s.lines.len(), 3);
    assert!(s.lines.iter().all(|l| l.chars().count() <= 8), "{:?}", s.lines);
}

fn dangling(board_ids: &[ItemId]) -> BoardData {
    let mk = |id: ItemId, x: f32| {
        Item::Cutout(Cutout {
            id,
            rect: Rect::new(x, 0.0, 100.0, 50.0),
            image: ImageSlot::Failed("not drawn".to_string()),
            label: String::new(),
            source: source(1, 2),
        })
    };
    let arrow = |id: ItemId, from: ItemId, to: ItemId| {
        Item::Arrow(Arrow { id, from: End::item(from), to: End::item(to), bend: 0.0, label: String::new() })
    };
    let mut items = vec![mk(board_ids[0], 0.0), mk(board_ids[1], 200.0)];
    items.push(arrow(10, board_ids[0], board_ids[1]));
    items.push(arrow(11, board_ids[0], 999)); // hangs on nothing
    items.push(mk(board_ids[0], 400.0)); // an id taken twice
    items.push(arrow(12, board_ids[1], board_ids[1])); // loops
    BoardData { items, view: View::default(), next_id: 3 }
}

#[test]
fn a_board_read_from_a_file_is_made_consistent_and_what_was_left_out_is_counted() {
    let (b, notices) = Board::from_data(dangling(&[1, 2]), LIMITS);
    assert_eq!(order(&b), vec![1, 2, 10]);
    assert_eq!(notices, vec![Notice::BoardRepaired { dropped: 3 }]);
    let next = cutout(&mut { b }, 0.0, 0.0);
    assert!(next > 12, "the next id is past every id the file used, got {next}");
}

#[test]
fn a_board_read_from_a_file_keeps_to_the_caps() {
    let limits = Limits { max_items: 2, ..LIMITS };
    let (b, notices) = Board::from_data(dangling(&[1, 2]), limits);
    assert_eq!(b.len(), 2);
    assert!(notices.iter().any(|n| matches!(n, Notice::BoardRepaired { .. })), "{notices:?}");
}

#[test]
fn the_sentences_of_the_notices_say_what_to_do() {
    let all = [
        Notice::TooManyItems { limit: 500 },
        Notice::PicturesTooBig { limit_px: 40_000_000 },
        Notice::PictureTooBig { px: 2_000_000, limit_px: 1_500_000 },
        Notice::PictureInvalid,
        Notice::ScaleLowered { wanted_pct: 200, used_pct: 140 },
        Notice::LabelShortened { limit: 120 },
        Notice::NoSuchItem { id: 4 },
        Notice::ArrowToItself,
        Notice::ArrowNeedsACutout,
        Notice::ArrowRepeated,
        Notice::BoardRepaired { dropped: 2 },
    ];
    for n in all {
        let said = n.english();
        assert!(said.len() > 15 && !said.contains("{"), "{n:?}: {said}");
        assert!(n.key().contains("whiteboard-notice-"), "{:?}", n.key());
    }
    assert!(Notice::TooManyItems { limit: 500 }.english().contains("500"));
    assert_eq!(Notice::TooManyItems { limit: 500 }.args(), vec![("limit", 500)]);
    assert_eq!(Notice::PictureTooBig { px: 7, limit_px: 5 }.args(), vec![("px", 7), ("limit", 5)]);
}
