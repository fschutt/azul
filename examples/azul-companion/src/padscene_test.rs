//! What the pad draws of the surface the pen is over: the viewport frame stays in the middle and the sheets (or the board) move
//! under it, so a finger that moves the content moves what the user sees by the same number of pixels.

use super::*;
use azul_review_link::frame::{PageSurface, WhiteboardSurface};

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// Sheets as wide as they are high, a break of a quarter of a width, five pages.
fn pages() -> Surface {
    Surface::Page(PageSurface { page_count: 5, page_index: 1, aspect: 1.0, gap: 0.25, ..PageSurface::empty() })
}

fn board() -> Surface {
    Surface::Whiteboard(WhiteboardSurface { board_id: 1, name: "Layout".into(), aspect: 2.0, item_count: 4 })
}

const PAD: (f32, f32) = (360.0, 390.0);

fn view_at(y: f32) -> Rect {
    Rect { x: 0.25, y, w: 0.5, h: 0.5 }
}

#[test]
fn the_frame_is_a_share_of_the_pad_and_stays_in_the_middle_of_where_the_surface_is_drawn() {
    let scene = scene(&pages(), view_at(1.0), None, PAD);
    let content = scene.content;
    assert!(close(content.w, PAD.0 - STRIP_W) && close(content.h, PAD.1), "the strip along the edge is not the surface's: {content:?}");
    let centre = (scene.frame.x + scene.frame.w / 2.0, scene.frame.y + scene.frame.h / 2.0);
    assert!(close(centre.0, content.w / 2.0) && close(centre.1, content.h / 2.0), "{:?}", scene.frame);
    assert!(close(scene.frame.w, FRAME_W_SHARE * content.w), "{:?}", scene.frame);
    assert!(close(scene.frame.h / scene.frame.w, 1.0), "the window's shape is the view's");
}

#[test]
fn a_tall_view_is_held_to_the_pads_height_and_the_scale_follows() {
    let tall = Rect { x: 0.25, y: 0.0, w: 0.2, h: 1.6 };
    let scene = scene(&pages(), tall, None, PAD);
    assert!(scene.frame.h <= FRAME_H_SHARE * PAD.1 + 0.01, "{:?}", scene.frame);
    assert!(close(scene.frame.w / tall.w, scene.px_per_unit) && close(scene.frame.h / tall.h, scene.px_per_unit), "square units: one scale on both axes");
}

#[test]
fn the_scale_is_the_frame_over_the_view_so_zooming_in_makes_the_sheets_bigger() {
    let a = scene(&pages(), Rect { x: 0.0, y: 1.0, w: 1.0, h: 0.5 }, None, PAD);
    let b = scene(&pages(), Rect { x: 0.0, y: 1.0, w: 0.5, h: 0.25 }, None, PAD);
    assert!(close(b.px_per_unit, 2.0 * a.px_per_unit), "{} {}", a.px_per_unit, b.px_per_unit);
    assert!(close(b.sheets[0].rect.w, 2.0 * a.sheets[0].rect.w) || b.sheets.iter().any(|s| close(s.rect.w, 2.0 * a.sheets[0].rect.w)));
}

#[test]
fn a_sheet_lies_where_the_view_says_so_and_the_content_moves_with_the_pan_one_pixel_for_one() {
    let first = scene(&pages(), view_at(1.0), None, PAD);
    let panned = scene(&pages(), view_at(1.2), None, PAD);
    let top = |s: &PadScene, index: u32| s.sheets.iter().find(|sheet| sheet.index == index).map(|sheet| sheet.rect.y);
    // the viewport went down by 0.2 units, so the content went up by 0.2 * scale pixels
    let moved = top(&first, 1).unwrap() - top(&panned, 1).unwrap();
    assert!(close(moved, 0.2 * first.px_per_unit), "{moved} vs {}", 0.2 * first.px_per_unit);
    let sideways = scene(&pages(), Rect { x: 0.3, ..view_at(1.0) }, None, PAD);
    let left = |s: &PadScene| s.sheets[0].rect.x;
    assert!(close(left(&first) - left(&sideways), 0.05 * first.px_per_unit));
}

#[test]
fn the_sheets_are_the_pages_in_sight_with_the_break_between_them() {
    let scene = scene(&pages(), view_at(1.0), None, PAD);
    assert!(scene.sheets.len() >= 2, "{:?}", scene.sheets);
    for pair in scene.sheets.windows(2) {
        assert_eq!(pair[1].index, pair[0].index + 1, "in order");
        let between = pair[1].rect.y - pair[0].rect.bottom();
        assert!(close(between, 0.25 * scene.px_per_unit), "the break is {between} px: {pair:?}");
    }
    for sheet in &scene.sheets {
        assert!(sheet.rect.bottom() > 0.0 && sheet.rect.y < PAD.1, "only sheets that show: {sheet:?}");
        assert!(sheet.rect.right() > 0.0 && sheet.rect.x < scene.content.w, "{sheet:?}");
        assert!(close(sheet.rect.w, scene.px_per_unit) && close(sheet.rect.h, scene.px_per_unit), "an aspect of 1 is a square");
    }
}

#[test]
fn the_sheet_the_middle_of_the_view_is_on_is_the_current_one() {
    let scene = scene(&pages(), view_at(1.4), None, PAD); // the middle (1.65) is on page 1 (1.25 .. 2.25)
    let current: Vec<u32> = scene.sheets.iter().filter(|s| s.current).map(|s| s.index).collect();
    assert_eq!(current, [1], "{:?}", scene.sheets);
    let scene = scene_of(view_at(2.6));
    assert_eq!(scene.sheets.iter().filter(|s| s.current).map(|s| s.index).collect::<Vec<_>>(), [2]);
}

fn scene_of(view: Rect) -> PadScene {
    scene(&pages(), view, None, PAD)
}

#[test]
fn the_ends_of_the_column_show_no_sheets_that_do_not_exist() {
    let top = scene_of(view_at(0.0));
    assert_eq!(top.sheets.first().map(|s| s.index), Some(0), "{:?}", top.sheets);
    let bottom = scene(&Surface::Page(PageSurface { page_count: 5, aspect: 1.0, gap: 0.25, ..PageSurface::empty() }), view_at(5.5), None, PAD);
    assert_eq!(bottom.sheets.last().map(|s| s.index), Some(4), "{:?}", bottom.sheets);
    let empty = scene(&Surface::Page(PageSurface::empty()), Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, None, PAD);
    assert_eq!(empty.sheets.len(), 1, "no page is one sheet");
}

#[test]
fn the_pen_is_drawn_where_it_is_on_the_surface() {
    let pen = PenMark { x: 0.5, y: 1.25, down: true }; // the middle of the view
    let scene = scene(&pages(), view_at(1.0), Some(pen), PAD);
    let (x, y, down) = scene.pen.expect("a pen");
    assert!(down && close(x, scene.content.w / 2.0) && close(y, PAD.1 / 2.0), "{x} {y}");
    assert!(self::scene(&pages(), view_at(1.0), None, PAD).pen.is_none());
}

#[test]
fn over_the_board_there_is_no_strip_and_the_board_box_and_the_overview_inset_are_drawn() {
    let view = Rect { x: 0.1, y: 0.05, w: 0.4, h: 0.25 };
    let scene = scene(&board(), view, None, PAD);
    assert!(close(scene.content.w, PAD.0), "no page strip on a board: {:?}", scene.content);
    assert!(scene.sheets.is_empty());
    let boxed = scene.board.expect("the board's box");
    assert!(close(boxed.w, scene.px_per_unit) && close(boxed.h, scene.px_per_unit / 2.0), "{boxed:?}");
    let mini = scene.minimap.expect("the overview inset");
    assert!(mini.board.w <= 0.4 * PAD.0 + 0.01 && close(mini.board.w / mini.board.h, 2.0), "{mini:?}");
    assert!(mini.board.x >= 0.0 && mini.board.right() <= PAD.0 && mini.board.bottom() <= PAD.1, "inside the pad: {mini:?}");
    // the frame in the overview is where the view is on the board
    let (fx, fy) = ((mini.frame.x - mini.board.x) / mini.board.w, (mini.frame.y - mini.board.y) / mini.board.h);
    assert!(close(fx, view.x / 1.0) && close(fy, view.y / 0.5), "{fx} {fy}");
    assert!(close(mini.frame.w / mini.board.w, view.w) && close(mini.frame.h / mini.board.h, view.h / 0.5));
}

#[test]
fn a_view_that_is_no_view_still_draws_a_frame_and_never_panics_or_goes_non_finite() {
    for bad in [Rect { x: f32::NAN, y: 0.0, w: 0.5, h: 0.5 }, Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.5 }, Rect { x: 0.0, y: 0.0, w: -1.0, h: f32::INFINITY }] {
        for surface in [pages(), board()] {
            let scene = scene(&surface, bad, Some(PenMark { x: f32::NAN, y: 1.0, down: false }), PAD);
            let all = [scene.frame.x, scene.frame.y, scene.frame.w, scene.frame.h, scene.px_per_unit];
            assert!(all.iter().all(|v| v.is_finite()), "{all:?}");
            assert!(scene.px_per_unit > 0.0);
        }
    }
    let tiny = scene(&pages(), view_at(1.0), None, (0.0, 0.0));
    assert!(tiny.px_per_unit.is_finite() && tiny.px_per_unit >= 0.0);
}

#[test]
fn the_scale_for_a_gesture_is_the_one_of_the_view_it_began_on() {
    let view = view_at(1.0);
    let s = px_per_unit(PAD, true, view);
    assert!(close(s, scene(&pages(), view, None, PAD).px_per_unit));
    assert!(px_per_unit(PAD, false, view) > s, "a board has the strip's width too");
}
