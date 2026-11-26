//! The viewport arithmetic: zoom about a point, pan, fit, reset, the focus, a whole gesture; the limits; and numbers that
//! are no numbers.

use super::*;

fn view() -> Rect {
    Rect { x: 0.25, y: 0.30, w: 0.5, h: 0.4 }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

fn same(a: Rect, b: Rect) -> bool {
    close(a.x, b.x) && close(a.y, b.y) && close(a.w, b.w) && close(a.h, b.h)
}

fn near(a: (f32, f32), b: (f32, f32)) -> bool {
    close(a.0, b.0) && close(a.1, b.1)
}

/// Where `p` is inside `view`, as fractions of the view.
fn relative(view: Rect, p: (f32, f32)) -> (f32, f32) {
    ((p.0 - view.x) / view.w, (p.1 - view.y) / view.h)
}

// ---- zoom

#[test]
fn the_zoom_of_a_view_is_one_over_its_visible_width() {
    assert!(close(zoom_of(view()), 2.0));
    assert!(close(zoom_of(Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }), 1.0));
    assert!(close(zoom_of(Rect { x: 0.0, y: 0.0, w: 0.0, h: 1.0 }), 1.0), "no width is the fitted zoom");
    assert!(close(zoom_of(Rect { x: 0.0, y: 0.0, w: f32::NAN, h: 1.0 }), 1.0));
}

#[test]
fn zooming_in_by_two_halves_the_view_and_the_point_stays_where_it_is_on_the_screen() {
    let about = (0.4, 0.45);
    let zoomed = zoom_about(view(), 2.0, about);
    assert!(close(zoomed.w, 0.25) && close(zoomed.h, 0.2), "{zoomed:?}");
    let (before, after) = (relative(view(), about), relative(zoomed, about));
    assert!(close(before.0, after.0) && close(before.1, after.1), "{before:?} {after:?}");
}

#[test]
fn zooming_out_makes_the_view_bigger_and_keeps_the_point_when_the_page_has_the_room() {
    let big = Rect { x: 0.4, y: 0.4, w: 0.2, h: 0.2 };
    let about = (0.45, 0.5);
    let zoomed = zoom_about(big, 0.5, about);
    assert!(close(zoomed.w, 0.4) && close(zoomed.h, 0.4), "{zoomed:?}");
    let (before, after) = (relative(big, about), relative(zoomed, about));
    assert!(close(before.0, after.0) && close(before.1, after.1));
}

#[test]
fn a_zoom_keeps_the_shape_of_the_window() {
    for factor in [0.3, 0.7, 1.0, 1.5, 3.0, 9.0] {
        let zoomed = zoom_about(view(), factor, (0.5, 0.5));
        assert!(close(zoomed.h / zoomed.w, view().h / view().w), "factor {factor}: {zoomed:?}");
    }
}

#[test]
fn the_zoom_stops_at_its_limits() {
    let most = zoom_about(view(), 1000.0, (0.5, 0.5));
    assert!(close(zoom_of(most), MAX_ZOOM), "{most:?}");
    let least = zoom_about(view(), 0.0001, (0.5, 0.5));
    assert!(close(zoom_of(least), MIN_ZOOM), "{least:?}");
    assert!(close(least.w, 1.0 / MIN_ZOOM));
}

#[test]
fn a_zoom_with_a_number_that_is_no_number_changes_nothing() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -2.0] {
        assert!(same(zoom_about(view(), bad, (0.5, 0.5)), view()), "factor {bad}");
    }
    for bad in [f32::NAN, f32::INFINITY] {
        assert!(same(zoom_about(view(), 2.0, (bad, 0.5)), view()), "about x {bad}");
        assert!(same(zoom_about(view(), 2.0, (0.5, bad)), view()), "about y {bad}");
    }
}

// ---- pan

#[test]
fn a_pan_moves_the_view_by_page_fractions() {
    let moved = pan(view(), 0.1, -0.1);
    assert!(close(moved.x, 0.35) && close(moved.y, 0.2) && close(moved.w, 0.5) && close(moved.h, 0.4), "{moved:?}");
}

#[test]
fn a_pan_stops_at_the_edges_of_the_page_while_the_view_is_smaller_than_it() {
    let far = pan(view(), 5.0, 5.0);
    assert!(close(far.x, 0.5) && close(far.y, 0.6), "{far:?}");
    let back = pan(view(), -5.0, -5.0);
    assert!(close(back.x, 0.0) && close(back.y, 0.0), "{back:?}");
}

#[test]
fn a_view_wider_than_the_page_is_centred_whatever_the_pan() {
    let wide = Rect { x: 0.7, y: 0.1, w: 2.0, h: 1.6 };
    let held = clamp_view(wide);
    assert!(close(held.x, -0.5) && close(held.y, -0.3) && close(held.w, 2.0), "{held:?}");
    let moved = pan(wide, 3.0, -3.0);
    assert!(close(moved.x, -0.5) && close(moved.y, -0.3));
}

#[test]
fn a_view_with_a_number_that_is_no_number_is_the_whole_page() {
    let whole = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(same(clamp_view(Rect { x: bad, y: 0.0, w: 0.5, h: 0.5 }), whole));
        assert!(same(clamp_view(Rect { x: 0.0, y: 0.0, w: bad, h: 0.5 }), whole));
    }
    assert!(same(clamp_view(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.5 }), whole), "an empty view");
    assert!(same(clamp_view(Rect { x: 0.0, y: 0.0, w: -1.0, h: 0.5 }), whole), "a negative view");
    assert!(same(pan(view(), f32::NAN, 0.1), pan(view(), 0.0, 0.1)), "a pan that is no number is no pan");
}

// ---- fit and reset

#[test]
fn fitting_the_width_shows_the_whole_width_and_keeps_the_vertical_centre() {
    let fitted = fit_width(view());
    assert!(close(fitted.w, 1.0) && close(fitted.x, 0.0), "{fitted:?}");
    assert!(close(fitted.h, 0.8), "the shape is kept: {fitted:?}");
    assert!(close(fitted.y + fitted.h / 2.0, 0.5), "the centre stays at {}: {fitted:?}", view().y + view().h / 2.0);
    assert!(same(fit_width(fitted), fitted), "fitting twice is fitting once");
}

#[test]
fn fitting_a_view_that_is_wider_than_the_page_makes_it_the_fitted_zoom_too() {
    let wide = Rect { x: -0.5, y: -0.3, w: 2.0, h: 1.6 };
    let fitted = fit_width(wide);
    assert!(close(fitted.w, 1.0) && close(fitted.h, 0.8), "{fitted:?}");
}

#[test]
fn a_reset_is_the_fitted_zoom_at_the_top_of_the_page() {
    let reset = reset(view());
    assert!(close(reset.x, 0.0) && close(reset.y, 0.0) && close(reset.w, 1.0) && close(reset.h, 0.8), "{reset:?}");
}

// ---- the focus

#[test]
fn the_pen_is_the_focus_where_it_is_in_view_and_the_nearest_point_of_the_view_where_it_is_not() {
    assert!(near(resolve_focus(Focus::Pen, view(), Some((0.4, 0.5))), (0.4, 0.5)));
    assert!(near(resolve_focus(Focus::Pen, view(), Some((0.9, 0.1))), (0.75, 0.3)));
    assert!(near(resolve_focus(Focus::Pen, view(), Some((0.0, 0.99))), (0.25, 0.7)));
}

#[test]
fn without_a_pen_the_focus_is_the_middle_of_the_view() {
    assert!(near(resolve_focus(Focus::Pen, view(), None), (0.5, 0.5)));
    assert!(near(resolve_focus(Focus::Center, view(), Some((0.4, 0.4))), (0.5, 0.5)));
    assert!(near(resolve_focus(Focus::Pen, view(), Some((f32::NAN, 0.4))), (0.5, 0.5)));
}

#[test]
fn a_point_of_the_page_is_the_focus_held_inside_the_view() {
    assert!(near(resolve_focus(Focus::Point { x: 0.3, y: 0.4 }, view(), Some((0.6, 0.6))), (0.3, 0.4)));
    assert!(near(resolve_focus(Focus::Point { x: 0.0, y: 1.0 }, view(), None), (0.25, 0.7)));
    assert!(near(resolve_focus(Focus::Point { x: f32::INFINITY, y: 0.4 }, view(), None), (0.5, 0.5)));
}

// ---- a whole gesture

#[test]
fn a_gesture_that_has_done_nothing_leaves_the_view_as_it_is() {
    assert!(same(apply_gesture(view(), (0.0, 0.0), 1.0, (0.5, 0.5)), view()));
}

#[test]
fn a_gesture_zooms_about_its_point_and_then_moves() {
    let about = (0.4, 0.5);
    let made = apply_gesture(view(), (0.05, 0.02), 2.0, about);
    assert!(same(made, pan(zoom_about(view(), 2.0, about), 0.05, 0.02)), "{made:?}");
}

#[test]
fn the_point_of_a_pure_zoom_stays_where_it_is_on_the_screen() {
    let about = (0.55, 0.35);
    let made = apply_gesture(view(), (0.0, 0.0), 1.6, about);
    let (before, after) = (relative(view(), about), relative(made, about));
    assert!(close(before.0, after.0) && close(before.1, after.1));
}

#[test]
fn a_gesture_is_a_function_of_its_base_and_its_totals_so_a_dropped_update_loses_nothing() {
    // the totals of the 3rd update, whether or not the 1st and 2nd arrived
    let direct = apply_gesture(view(), (0.12, -0.04), 1.4, (0.5, 0.5));
    let again = apply_gesture(view(), (0.12, -0.04), 1.4, (0.5, 0.5));
    assert!(same(direct, again));
}

#[test]
fn whatever_the_numbers_the_view_stays_a_view() {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    };
    for _ in 0..5_000 {
        let w = 0.05 + next() * 3.0;
        let base = Rect { x: next() * 2.0 - 0.5, y: next() * 2.0 - 0.5, w, h: w * (0.3 + next()) };
        let made = apply_gesture(base, (next() * 8.0 - 4.0, next() * 8.0 - 4.0), 0.01 + next() * 30.0, (next(), next()));
        assert!(made.x.is_finite() && made.y.is_finite() && made.w.is_finite() && made.h.is_finite(), "{base:?} -> {made:?}");
        assert!(made.w > 0.0 && made.h > 0.0, "{made:?}");
        let zoom = zoom_of(made);
        assert!((MIN_ZOOM - 1e-4..=MAX_ZOOM + 1e-3).contains(&zoom), "{zoom} of {made:?}");
        if made.w <= 1.0 {
            assert!(made.x >= -1e-5 && made.x + made.w <= 1.0 + 1e-5, "{made:?}");
        }
        if made.h <= 1.0 {
            assert!(made.y >= -1e-5 && made.y + made.h <= 1.0 + 1e-5, "{made:?}");
        }
    }
}


// ---- the sign of the pan, said once

#[test]
fn a_finger_that_moves_the_content_moves_the_viewport_the_other_way() {
    // direct manipulation: the finger goes right and down by 100 px on a pad that shows 400 px a unit: the content goes right
    // and down by a quarter, so the viewport goes left and up by a quarter
    let (dx, dy) = finger_to_pan((100.0, 100.0), 400.0, PanDirection::ContentFollowsFinger);
    assert!(close(dx, -0.25) && close(dy, -0.25), "{dx} {dy}");
    let (dx, dy) = finger_to_pan((-40.0, 20.0), 200.0, PanDirection::ContentFollowsFinger);
    assert!(close(dx, 0.2) && close(dy, -0.1), "{dx} {dy}");
}

#[test]
fn reversed_the_viewport_follows_the_finger() {
    let (dx, dy) = finger_to_pan((100.0, -50.0), 400.0, PanDirection::ViewportFollowsFinger);
    assert!(close(dx, 0.25) && close(dy, -0.125), "{dx} {dy}");
    assert_eq!(PanDirection::from_reverse(false), PanDirection::ContentFollowsFinger);
    assert_eq!(PanDirection::from_reverse(true), PanDirection::ViewportFollowsFinger);
    assert_eq!(PanDirection::default(), PanDirection::ContentFollowsFinger, "direct manipulation unless the Mac says otherwise");
}

#[test]
fn the_two_directions_are_exact_opposites_and_a_scale_that_is_no_scale_pans_nothing() {
    for finger in [(10.0, 0.0), (0.0, -33.0), (7.5, 7.5)] {
        let a = finger_to_pan(finger, 123.0, PanDirection::ContentFollowsFinger);
        let b = finger_to_pan(finger, 123.0, PanDirection::ViewportFollowsFinger);
        assert!(close(a.0, -b.0) && close(a.1, -b.1), "{finger:?}");
    }
    for bad in [0.0, -5.0, f32::NAN, f32::INFINITY] {
        assert_eq!(finger_to_pan((100.0, 100.0), bad, PanDirection::ContentFollowsFinger), (0.0, 0.0), "scale {bad}");
    }
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(finger_to_pan((bad, 5.0), 100.0, PanDirection::ContentFollowsFinger).0, 0.0, "a finger that is no number goes nowhere");
        assert!(close(finger_to_pan((bad, 5.0), 100.0, PanDirection::ContentFollowsFinger).1, -0.05), "the other axis still counts");
    }
}

// ---- the pages as one column

/// A sheet twice as high as wide, a break of half a width, four pages: the tops are 0, 2.5, 5 and 7.5, the column 9.5 high.
fn column() -> PageSurface {
    PageSurface { page_count: 4, aspect: 0.5, gap: 0.5, ..PageSurface::empty() }
}

#[test]
fn the_column_of_pages_is_one_sheet_wide_and_as_high_as_its_sheets_and_breaks() {
    let space = Space::of_pages(&column());
    assert_eq!((space.x, space.y, space.width), (0.0, 0.0, 1.0));
    assert!(close(space.height, 9.5), "{space:?}");
    assert!(close(Space::of_pages(&PageSurface { page_count: 0, ..column() }).height, 2.0), "no page is one sheet");
    assert!(close(Space::of_pages(&PageSurface { page_count: 1, ..column() }).height, 2.0));
}

#[test]
fn the_board_is_its_box_and_a_margin_round_it() {
    let space = Space::of_board(2.0);
    assert!(close(space.x, -BOARD_MARGIN) && close(space.y, -BOARD_MARGIN), "{space:?}");
    assert!(close(space.width, 1.0 + 2.0 * BOARD_MARGIN) && close(space.height, 0.5 + 2.0 * BOARD_MARGIN), "{space:?}");
    let bad = Space::of_board(f32::NAN);
    assert!(bad.width.is_finite() && bad.height.is_finite() && bad.height > 0.0, "a board of no shape is a square: {bad:?}");
}

#[test]
fn a_drag_down_the_column_crosses_the_breaks_and_stops_at_the_last_page() {
    let space = Space::of_pages(&column());
    let view = Rect { x: 0.25, y: 0.0, w: 0.5, h: 0.8 };
    let across = pan_in(view, 0.0, 3.0, space);
    assert!(close(across.y, 3.0), "past the first break and into the second page: {across:?}");
    let end = pan_in(view, 0.0, 99.0, space);
    assert!(close(end.y + end.h, 9.5), "the last page's bottom edge is the end: {end:?}");
    let top = pan_in(view, 0.0, -99.0, space);
    assert!(close(top.y, 0.0), "{top:?}");
}

#[test]
fn a_pan_sideways_over_the_pages_stops_at_the_edges_of_the_sheet() {
    let space = Space::of_pages(&column());
    let view = Rect { x: 0.25, y: 1.0, w: 0.5, h: 0.8 };
    assert!(close(pan_in(view, 9.0, 0.0, space).x, 0.5) && close(pan_in(view, -9.0, 0.0, space).x, 0.0));
    let wide = Rect { x: 0.0, y: 1.0, w: 2.0, h: 3.0 };
    assert!(close(clamp_view_in(wide, space).x, -0.5), "a view wider than the sheet is centred on it");
}

#[test]
fn over_the_board_a_view_may_go_a_margin_beyond_its_edge_and_no_further() {
    let space = Space::of_board(1.0);
    let view = Rect { x: 0.2, y: 0.2, w: 0.4, h: 0.4 };
    let left = pan_in(view, -9.0, -9.0, space);
    assert!(close(left.x, -BOARD_MARGIN) && close(left.y, -BOARD_MARGIN), "{left:?}");
    let right = pan_in(view, 9.0, 9.0, space);
    assert!(close(right.x + right.w, 1.0 + BOARD_MARGIN) && close(right.y + right.h, 1.0 + BOARD_MARGIN), "{right:?}");
}

#[test]
fn the_zoom_does_not_depend_on_the_space_only_on_the_width_of_the_view() {
    let pages = Space::of_pages(&column());
    let board = Space::of_board(1.5);
    let view = Rect { x: 0.0, y: 1.0, w: 0.5, h: 0.5 };
    assert!(close(zoom_about_in(view, 2.0, (0.2, 1.2), pages).w, 0.25));
    assert!(close(zoom_about_in(view, 2.0, (0.2, 1.2), board).w, 0.25));
    assert!(close(zoom_of(zoom_about_in(view, 1e6, (0.2, 1.2), pages)), MAX_ZOOM));
}

#[test]
fn a_gesture_over_the_column_is_a_zoom_and_then_a_pan_in_its_space() {
    let space = Space::of_pages(&column());
    let base = Rect { x: 0.25, y: 4.0, w: 0.5, h: 0.8 };
    let made = apply_gesture_in(base, (0.0, 1.2), 1.0, (0.5, 4.4), space);
    assert!(close(made.y, 5.2) && close(made.x, 0.25), "{made:?}");
    // the same numbers on the unit square would have been held to it
    assert!(apply_gesture_in(base, (0.0, 1.2), 1.0, (0.5, 4.4), Space::UNIT).y <= 1.0);
}

#[test]
fn the_page_a_height_belongs_to_is_decided_in_the_middle_of_the_break() {
    let page = column();
    assert_eq!(page_at(&page, 0.0), 0);
    assert_eq!(page_at(&page, 2.2), 0, "still the first page's side of the break");
    assert_eq!(page_at(&page, 2.3), 1, "the second page's side");
    assert_eq!(page_at(&page, 7.2), 2, "the third page's side of the break before the fourth");
    assert_eq!(page_at(&page, 7.3), 3, "the fourth page's side");
    assert_eq!(page_at(&page, -5.0), 0);
    assert_eq!(page_at(&page, 99.0), 3);
    assert_eq!(page_at(&page, f32::NAN), 0, "a height that is no number is the first page");
    assert_eq!(page_at(&PageSurface { page_count: 0, ..column() }, 5.0), 0, "no pages: page 0");
}

#[test]
fn a_release_snaps_the_nearest_page_top_to_the_top_of_the_view() {
    let page = column();
    let view = Rect { x: 0.25, y: 2.9, w: 0.5, h: 0.8 };
    assert!(close(snap_to_page_top(view, &page).y, 2.5), "closer to page 1's top (2.5) than to page 2's (5.0)");
    assert!(close(snap_to_page_top(Rect { y: 4.0, ..view }, &page).y, 5.0), "closer to page 2's top");
    assert!(close(snap_to_page_top(Rect { y: 0.3, ..view }, &page).y, 0.0));
    let kept = snap_to_page_top(view, &page);
    assert!(close(kept.x, 0.25) && close(kept.w, 0.5) && close(kept.h, 0.8), "only the height moves: {kept:?}");
    assert!(close(snap_to_page_top(Rect { y: 50.0, ..view }, &page).y, 7.5), "never past the last page");
}

#[test]
fn the_page_keys_go_to_the_top_of_the_neighbouring_page_and_stop_at_the_ends() {
    let page = column();
    let view = Rect { x: 0.25, y: 2.6, w: 0.5, h: 0.8 }; // the middle (3.0) is on page 1
    assert!(close(step_page(view, &page, 1).y, 5.0));
    assert!(close(step_page(view, &page, -1).y, 0.0));
    assert!(close(step_page(view, &page, 0).y, 2.5), "no step lands on the top of this page");
    assert!(close(step_page(view, &page, 9).y, 7.5), "the last page is the end");
    assert!(close(step_page(view, &page, -9).y, 0.0));
    let kept = step_page(view, &page, 1);
    assert!(close(kept.x, 0.25) && close(kept.w, 0.5) && close(kept.h, 0.8), "{kept:?}");
}

#[test]
fn a_reset_over_the_pages_is_the_fitted_width_at_the_top_of_the_page_in_view() {
    let page = column();
    let view = Rect { x: 0.25, y: 5.6, w: 0.5, h: 0.8 }; // the middle (6.0) is on page 2, which starts at 5.0
    let made = reset_to_page_top(view, &page);
    assert!(close(made.x, 0.0) && close(made.w, 1.0) && close(made.y, 5.0), "{made:?}");
    assert!(close(made.h / made.w, view.h / view.w), "the window's shape is kept: {made:?}");
}
