//! The zoom model of the pages without a window: the numbers the layout gives, where the window is, what the phone sees of it, and what a
//! drag, a pinch, a fit and a page key make of it.

use azul_review_link::{frame::Focus, viewport};

use super::*;

/// Five pages of 554 x 784 px with 24 px between them, in a window of 620 x 700 px.
fn geometry() -> Geometry {
    Geometry { page_w: 554.0, page_h: 784.0, gap: 24.0, pages: 5, window: (620.0, 700.0) }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-2 * b.abs().max(1.0)
}

fn assert_near(a: f32, b: f32, what: &str) {
    assert!(near(a, b), "{what}: {a} is not {b}");
}

fn rect_near(a: Rect, b: Rect) -> bool {
    near(a.x, b.x) && near(a.y, b.y) && near(a.w, b.w) && near(a.h, b.h)
}

/// Two windows are the same place (the numbers went through a division or two).
fn assert_looking(a: Looking, b: Looking, what: &str) {
    assert_eq!(a.page, b.page, "{what}: the page of {a:?} and {b:?}");
    assert_near(a.zoom, b.zoom, what);
    assert_near(a.at.0, b.at.0, what);
    assert_near(a.at.1, b.at.1, what);
}

fn gesture(id: u32, phase: Phase, pan: (f32, f32), zoom: f32, focus: Focus) -> ViewportOp {
    ViewportOp::Gesture { id, phase, pan_x: pan.0, pan_y: pan.1, zoom, focus }
}

// ---- the numbers the layout gives

#[test]
fn a_geometry_that_is_no_geometry_is_made_safe_and_stays_so() {
    let nothing = Geometry { page_w: f32::NAN, page_h: 0.0, gap: f32::NEG_INFINITY, pages: 0, window: (f32::NAN, -5.0) };
    let sane = nothing.sane();
    assert!(sane.page_w > 1.0 && sane.page_h > 1.0 && sane.gap == 0.0 && sane.pages == 1, "{sane:?}");
    assert_eq!(sane.window, (sane.page_w, sane.page_h), "a window of nothing is the size of a sheet");
    assert_eq!(sane.sane(), sane, "made safe once is safe");
    let negative = Geometry { gap: -8.0, ..geometry() }.sane();
    assert_eq!(negative.gap, 0.0);
    assert_eq!(geometry().sane(), geometry(), "a good geometry is left as it is");
}

#[test]
fn the_pages_are_a_column_of_sheets_one_wide_with_the_break_in_sheet_widths() {
    let g = geometry();
    let surface = g.surface("src/lib.rs", 2, 40, 900);
    assert_eq!((surface.page_index, surface.page_count, surface.line, surface.line_count), (2, 5, 40, 900));
    assert_near(surface.aspect, 554.0 / 784.0, "aspect");
    assert_near(surface.gap, 24.0 / 554.0, "gap");
    assert_near(surface.column_height(), (5.0 * 784.0 + 4.0 * 24.0) / 554.0, "the column's height");
    assert_eq!(g.surface("", 99, 0, 0).page_index, 4, "a page past the last is the last");
    assert_eq!(Geometry { pages: 0, ..g }.surface("", 0, 0, 0).page_count, 1, "a file of no pages is one");
}

#[test]
fn the_fit_zoom_is_the_window_over_the_sheet_and_the_stride_is_a_sheet_and_its_break() {
    let g = geometry();
    assert_near(g.fit_zoom(), 620.0 / 554.0, "fit");
    assert_eq!(g.stride(), 578.0);
    let huge = Geometry { window: (50_000.0, 700.0), ..g };
    assert_eq!(huge.fit_zoom(), Z_MAX, "a window that is far wider than a sheet cannot fit it: the most the pages are drawn at");
}

#[test]
fn how_far_the_window_may_go_down_and_across_a_page_depends_on_the_zoom() {
    let g = geometry();
    assert_near(g.max_down(1.0), 84.0, "a window 700 high over a page 784 high");
    assert_eq!(g.max_down(0.5), 0.0, "zoomed out the whole page is in the window");
    assert_near(g.max_down(2.0), 784.0 - 350.0, "zoomed in");
    assert_eq!(g.max_across(1.0), 0.0, "the window is wider than the page");
    assert_near(g.max_across(2.0), 554.0 - 310.0, "zoomed in");
}

// ---- where the window is

#[test]
fn the_pages_as_they_have_always_been_drawn_are_a_view_a_little_wider_than_a_sheet_at_the_top_of_the_first() {
    let g = geometry();
    let view = Looking::HOME.view(&g);
    assert_near(view.x, 0.0, "x");
    assert_near(view.y, 0.0, "y");
    assert_near(view.w, 620.0 / 554.0, "w");
    assert_near(view.h, 700.0 / 554.0, "h");
    assert_eq!(Looking::HOME.scroll(&g), (0.0, 0.0));
    assert_looking(Looking::from_view(&g, view), Looking::HOME, "and back");
}

#[test]
fn the_strip_is_scrolled_a_page_and_its_break_for_each_page_and_what_is_between_is_the_place_on_the_page() {
    let g = geometry();
    let looking = Looking { zoom: 2.0, page: 1, at: (10.0, 20.0) };
    assert_eq!(looking.scroll(&g), ((578.0 + 10.0) * 2.0, 40.0));
    let back = Looking::from_scroll(&g, 2.0, looking.scroll(&g));
    assert_eq!(back.page, 1);
    assert_near(back.at.0, 10.0, "across");
    assert_near(back.at.1, 20.0, "down");
    assert_eq!(back.zoom, 2.0);
}

#[test]
fn a_scroll_that_is_no_scroll_is_the_top_of_the_first_page_and_one_past_the_end_is_the_last_page() {
    let g = geometry();
    assert_eq!(Looking::from_scroll(&g, 1.0, (f32::NAN, f32::INFINITY)), Looking::HOME);
    assert_eq!(Looking::from_scroll(&g, 1.0, (-500.0, -3.0)), Looking::HOME);
    let far = Looking::from_scroll(&g, 1.0, (1.0e9, 0.0));
    assert_eq!(far.page, 4, "the last page");
    assert_eq!(Looking::from_scroll(&g, f32::NAN, (0.0, 0.0)).zoom, 1.0, "a zoom that is no number is none");
    assert_eq!(Looking::from_scroll(&g, 99.0, (0.0, 0.0)).zoom, Z_MAX);
    assert_eq!(Looking::from_scroll(&g, 0.001, (0.0, 0.0)).zoom, Z_MIN);
}

#[test]
fn a_view_survives_the_way_to_the_pages_and_back_at_every_zoom_and_page() {
    let g = geometry();
    for zoom in [0.5_f32, 0.8, 1.0, 1.7, 3.0] {
        for page in [0_usize, 2, 4] {
            let at = (g.max_across(zoom) * 0.5, g.max_down(zoom) * 0.5);
            let looking = Looking { zoom, page, at };
            let back = Looking::from_view(&g, looking.view(&g));
            assert_eq!(back.page, page, "zoom {zoom} page {page}");
            assert_near(back.zoom, zoom, "zoom");
            assert_near(back.at.0, at.0, "across");
            assert_near(back.at.1, at.1, "down");
            let scrolled = Looking::from_scroll(&g, zoom, looking.scroll(&g));
            assert_eq!(scrolled.page, page);
            assert_near(scrolled.at.1, at.1, "down by the scroll");
        }
    }
}

#[test]
fn a_view_across_a_break_is_on_the_page_its_middle_is_on_and_a_view_above_that_page_is_at_its_top() {
    let g = geometry();
    let surface = g.surface("", 0, 0, 0);
    let top_of_second = surface.page_top(1);
    // the window's top is above the second page's top (in the break), its middle is well inside the second page
    let view = Rect { x: 0.0, y: top_of_second - 0.2, w: 620.0 / 554.0, h: 700.0 / 554.0 };
    let looking = Looking::from_view(&g, view);
    assert_eq!(looking.page, 1, "the page the middle is on");
    assert_near(looking.at.1, 0.0, "and the window is put at its head: the page is never shown from above its top");
    // the same view, lower down the first page: the first
    let lower = Rect { y: 0.3, ..view };
    assert_eq!(Looking::from_view(&g, lower).page, 0);
}

#[test]
fn a_view_that_asks_for_more_zoom_than_the_pages_can_be_drawn_at_is_held_about_its_middle() {
    let g = geometry();
    let tiny = Rect { x: 1.0, y: 1.0, w: 0.01, h: 0.01 * 700.0 / 620.0 };
    let looking = Looking::from_view(&g, tiny);
    assert_eq!(looking.zoom, Z_MAX);
    let view = looking.view(&g);
    assert_near(view.w, 620.0 / (Z_MAX * 554.0), "w at the limit");
    let wide = Looking::from_view(&g, Rect { x: 0.0, y: 0.0, w: 40.0, h: 40.0 * 700.0 / 620.0 });
    assert_eq!(wide.zoom, Z_MIN);
}

// ---- what the phone's commands make of it

#[test]
fn a_drag_down_moves_the_window_down_the_page_by_what_the_finger_moved() {
    let g = geometry();
    let mut held = Held::default();
    let begin = apply(&g, Looking::HOME, &mut held, gesture(1, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center), Context::default());
    assert_looking(begin.looking, Looking::HOME, "a begin moves nothing");
    let step = apply(&g, Looking::HOME, &mut held, gesture(1, Phase::Update, (0.0, 0.1), 1.0, Focus::Center), Context::default());
    assert_eq!(step.looking.page, 0);
    assert_near(step.looking.at.1, 0.1 * 554.0, "a tenth of a sheet width down");
    assert!(!step.zoom_held);
    assert!(rect_near(step.view, step.looking.view(&g)), "what the phone is told is what the window shows");
}

#[test]
fn a_drag_across_a_break_goes_from_the_foot_of_a_page_to_the_head_of_the_next() {
    let g = geometry();
    let mut held = Held::default();
    let begin = gesture(2, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center);
    let _ = apply(&g, Looking::HOME, &mut held, begin, Context::default());
    // a little way: the foot of the first page is as far as the window can go down it
    let foot = apply(&g, Looking::HOME, &mut held, gesture(2, Phase::Update, (0.0, 0.3), 1.0, Focus::Center), Context::default());
    assert_eq!(foot.looking.page, 0);
    assert_near(foot.looking.at.1, g.max_down(1.0), "held at the foot of the page");
    // far enough that the middle of the window is on the second page: its head
    let next = apply(&g, Looking::HOME, &mut held, gesture(2, Phase::Update, (0.0, 0.95), 1.0, Focus::Center), Context::default());
    assert_eq!(next.looking.page, 1);
    assert_near(next.looking.at.1, 0.0, "the head of the page");
}

#[test]
fn the_totals_of_a_gesture_are_applied_to_the_view_it_began_on_so_a_dropped_update_loses_nothing() {
    let g = geometry();
    let ctx = Context::default();
    let mut one = Held::default();
    let _ = apply(&g, Looking::HOME, &mut one, gesture(3, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center), ctx);
    let mut now = Looking::HOME;
    for pan in [0.02, 0.05, 0.08] {
        now = apply(&g, now, &mut one, gesture(3, Phase::Update, (0.0, pan), 1.0, Focus::Center), ctx).looking;
    }
    // the same gesture with the first two updates lost
    let mut other = Held::default();
    let _ = apply(&g, Looking::HOME, &mut other, gesture(3, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center), ctx);
    let lost = apply(&g, Looking::HOME, &mut other, gesture(3, Phase::Update, (0.0, 0.08), 1.0, Focus::Center), ctx).looking;
    assert_eq!(now.page, lost.page);
    assert_near(now.at.1, lost.at.1, "the same place");
}

#[test]
fn an_update_with_no_begin_starts_from_the_view_there_is_and_a_gesture_of_another_id_starts_again() {
    let g = geometry();
    let ctx = Context::default();
    let mut held = Held::default();
    let first = apply(&g, Looking::HOME, &mut held, gesture(7, Phase::Update, (0.0, 0.05), 1.0, Focus::Center), ctx).looking;
    assert_near(first.at.1, 0.05 * 554.0, "begun by the first update it saw");
    // another gesture begins where the window is now, whatever the first one's totals were
    let second = apply(&g, first, &mut held, gesture(8, Phase::Update, (0.0, 0.05), 1.0, Focus::Center), ctx).looking;
    assert_near(second.at.1, 2.0 * 0.05 * 554.0, "from where the first left it");
}

#[test]
fn a_pinch_about_the_pen_keeps_the_point_under_the_pen_where_it_was_on_the_screen() {
    let g = geometry();
    let ctx = Context { snap_to_page_top: false, pen: Some((0.5, 0.6)) };
    let mut held = Held::default();
    // the view the gesture began on as the link crate holds it (a view wider than a sheet is centred on it)
    let base = viewport::clamp_view_in(Looking::HOME.view(&g), g.space());
    let _ = apply(&g, Looking::HOME, &mut held, gesture(4, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen), ctx);
    let zoomed = apply(&g, Looking::HOME, &mut held, gesture(4, Phase::Update, (0.0, 0.0), 2.0, Focus::Pen), ctx);
    assert!(zoomed.looking.zoom > 1.9, "zoomed in: {:?}", zoomed.looking);
    let before = ((0.5 - base.x) / base.w, (0.6 - base.y) / base.h);
    let after = ((0.5 - zoomed.view.x) / zoomed.view.w, (0.6 - zoomed.view.y) / zoomed.view.h);
    assert_near(before.0, after.0, "across");
    assert_near(before.1, after.1, "down");
}

#[test]
fn a_zoom_the_pages_cannot_be_drawn_at_is_held_at_the_limit_and_said() {
    let g = geometry();
    let ctx = Context::default();
    let mut held = Held::default();
    let _ = apply(&g, Looking::HOME, &mut held, gesture(5, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center), ctx);
    let in_far = apply(&g, Looking::HOME, &mut held, gesture(5, Phase::Update, (0.0, 0.0), 20.0, Focus::Center), ctx);
    assert_eq!(in_far.looking.zoom, Z_MAX);
    assert!(in_far.zoom_held, "the phone asked for more than the pages can be drawn at");
    assert_near(in_far.view.w, 620.0 / (Z_MAX * 554.0), "the view the phone is told is the one that is shown");
    let out_far = apply(&g, Looking::HOME, &mut held, gesture(5, Phase::Update, (0.0, 0.0), 0.05, Focus::Center), ctx);
    assert_eq!(out_far.looking.zoom, Z_MIN);
    assert!(out_far.zoom_held);
}

#[test]
fn a_pan_or_a_zoom_that_is_no_number_changes_nothing() {
    let g = geometry();
    let ctx = Context::default();
    let mut held = Held::default();
    let now = Looking { zoom: 1.5, page: 2, at: (30.0, 100.0) };
    let _ = apply(&g, now, &mut held, gesture(6, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center), ctx);
    let bad = apply(&g, now, &mut held, gesture(6, Phase::Update, (f32::NAN, f32::INFINITY), f32::NAN, Focus::Center), ctx);
    assert_eq!(bad.looking.page, 2);
    assert_near(bad.looking.zoom, 1.5, "zoom");
    assert_near(bad.looking.at.1, 100.0, "down");
}

#[test]
fn the_snap_after_a_drag_puts_the_nearest_page_top_at_the_top_of_the_window_only_when_the_setting_is_on() {
    let g = geometry();
    let on = Context { snap_to_page_top: true, pen: None };
    let off = Context::default();
    let mut held = Held::default();
    let _ = apply(&g, Looking::HOME, &mut held, gesture(9, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center), on);
    // a drag that ends near the second page's head (the window's top a little above it, in the break)
    let end = gesture(9, Phase::End, (0.0, 1.4), 1.0, Focus::Center);
    let snapped = apply(&g, Looking::HOME, &mut held, end, on);
    assert_eq!(snapped.looking.page, 1);
    assert_near(snapped.looking.at.1, 0.0, "the head of the page");
    assert_eq!(held.gesture, None, "the gesture is over");
    let mut kept = Held::default();
    let _ = apply(&g, Looking::HOME, &mut kept, gesture(10, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center), off);
    let plain = apply(&g, Looking::HOME, &mut kept, gesture(10, Phase::End, (0.0, 0.1), 1.0, Focus::Center), off);
    assert_near(plain.looking.at.1, 0.1 * 554.0, "no snap: where the finger left it");
}

#[test]
fn fit_makes_the_width_of_a_sheet_the_width_of_the_window() {
    let g = geometry();
    let zoomed = Looking { zoom: 2.4, page: 1, at: (50.0, 300.0) };
    let mut held = Held::default();
    let fit = apply(&g, zoomed, &mut held, ViewportOp::Fit, Context::default());
    assert_near(fit.looking.zoom, g.fit_zoom(), "the zoom");
    assert_near(fit.view.w, 1.0, "a sheet's width in view");
    assert_eq!(fit.looking.page, 1, "the page it was on");
    assert_near(fit.looking.at.0, 0.0, "the left edge of the sheet");
}

#[test]
fn the_page_keys_go_to_the_head_of_the_next_and_the_previous_page_and_stop_at_the_ends() {
    let g = geometry();
    let mut held = Held::default();
    let ctx = Context::default();
    let next = apply(&g, Looking::HOME, &mut held, ViewportOp::PageStep(1), ctx);
    assert_eq!(next.looking.page, 1);
    assert_near(next.looking.at.1, 0.0, "the head of the page");
    let back = apply(&g, next.looking, &mut held, ViewportOp::PageStep(-1), ctx);
    assert_eq!(back.looking.page, 0);
    let before_first = apply(&g, Looking::HOME, &mut held, ViewportOp::PageStep(-3), ctx);
    assert_eq!(before_first.looking.page, 0, "the first page is the end");
    let past_last = apply(&g, Looking::HOME, &mut held, ViewportOp::PageStep(8), ctx);
    assert_eq!(past_last.looking.page, 4, "the last page is the end");
    let way_past = apply(&g, Looking::HOME, &mut held, ViewportOp::PageStep(i8::MAX), ctx);
    assert_eq!(way_past.looking.page, 4);
}

#[test]
fn reset_is_the_fitted_zoom_at_the_head_of_the_page_the_window_is_on() {
    let g = geometry();
    let zoomed = Looking { zoom: 2.4, page: 3, at: (80.0, 300.0) };
    let mut held = Held::default();
    let reset = apply(&g, zoomed, &mut held, ViewportOp::Reset, Context::default());
    assert_eq!(reset.looking.page, 3);
    assert_near(reset.looking.at.0, 0.0, "across");
    assert_near(reset.looking.at.1, 0.0, "down");
    assert_near(reset.looking.zoom, g.fit_zoom(), "the zoom");
}

#[test]
fn where_the_pen_is_in_the_column_follows_the_page_it_is_on() {
    let g = geometry();
    let (x, y) = pen_in_column(&g, 1, (277.0, 392.0));
    assert_near(x, 0.5, "across");
    assert_near(y, (784.0 + 24.0) / 554.0 + 392.0 / 554.0, "down");
    assert_eq!(pen_in_column(&g, 99, (0.0, 0.0)), pen_in_column(&g, 4, (0.0, 0.0)), "a page past the last is the last");
}

#[test]
fn nothing_the_layout_or_the_phone_says_makes_a_number_that_is_not_one() {
    let nothing = Geometry { page_w: 0.0, page_h: f32::NAN, gap: f32::NAN, pages: 0, window: (0.0, 0.0) };
    let ctx = Context { snap_to_page_top: true, pen: Some((f32::NAN, 0.0)) };
    let ops = [
        ViewportOp::Fit,
        ViewportOp::Reset,
        ViewportOp::PageStep(3),
        gesture(1, Phase::Begin, (f32::NAN, 1.0e30), f32::INFINITY, Focus::Point { x: f32::NAN, y: 1.0e30 }),
        gesture(1, Phase::End, (1.0e30, -1.0e30), 0.0, Focus::Pen),
    ];
    for op in ops {
        let mut held = Held::default();
        let applied = apply(&nothing, Looking { zoom: f32::NAN, page: usize::MAX, at: (f32::NAN, -1.0) }, &mut held, op, ctx);
        let v = applied.view;
        assert!(v.x.is_finite() && v.y.is_finite() && v.w.is_finite() && v.h.is_finite() && v.w > 0.0 && v.h > 0.0, "{op:?}: {v:?}");
        assert!((Z_MIN..=Z_MAX).contains(&applied.looking.zoom), "{op:?}: {:?}", applied.looking);
        assert!(applied.looking.at.0.is_finite() && applied.looking.at.1.is_finite(), "{op:?}");
    }
}
