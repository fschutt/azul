//! The zoom model of the whiteboard without a window: the box round the items, the view of the board and the phone's window of it, and what
//! a drag, a pinch, a fit and a reset make of it.

use azul_review_link::frame::Focus;

use super::*;

/// Items in a box of 2000 x 1000 board px at (100, 50), on a canvas of 800 x 600 px.
fn geometry() -> Geometry {
    Geometry { items: Some(Rect { x: 100.0, y: 50.0, w: 2000.0, h: 1000.0 }), window: (800.0, 600.0), zoom: (0.2, 4.0) }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-2 * b.abs().max(1.0)
}

fn assert_near(a: f32, b: f32, what: &str) {
    assert!(near(a, b), "{what}: {a} is not {b}");
}

fn gesture(id: u32, phase: Phase, pan: (f32, f32), zoom: f32, focus: Focus) -> ViewportOp {
    ViewportOp::Gesture { id, phase, pan_x: pan.0, pan_y: pan.1, zoom, focus }
}

#[test]
fn a_geometry_that_is_no_geometry_is_made_safe_and_stays_so() {
    let nothing = Geometry { items: Some(Rect { x: f32::NAN, y: 0.0, w: 0.0, h: -4.0 }), window: (f32::NAN, 0.0), zoom: (5.0, 1.0) };
    let sane = nothing.sane();
    assert_eq!(sane.items, Some(EMPTY_BOX));
    assert_eq!(sane.window, EMPTY_WINDOW);
    assert_eq!(sane.zoom, (0.2, 4.0));
    assert_eq!(sane.sane(), sane);
    assert_eq!(Geometry { items: None, ..geometry() }.sane().items, Some(EMPTY_BOX), "no items is the empty board's box");
    assert_eq!(geometry().sane(), geometry(), "a good one is left as it is");
}

#[test]
fn the_unit_is_the_width_of_the_box_and_the_aspect_its_shape() {
    let g = geometry();
    assert_eq!(g.unit(), 2000.0);
    assert_near(g.aspect(), 2.0, "aspect");
    let (x, y) = g.to_units(Pt::new(1100.0, 550.0));
    assert_near(x, 0.5, "x");
    assert_near(y, 0.25, "y");
    let wide = Geometry { items: Some(Rect { x: 0.0, y: 0.0, w: 100_000.0, h: 10.0 }), ..g };
    assert_eq!(wide.aspect(), MAX_ASPECT, "a shape past what a surface may be is held");
    let surface = g.surface(3, "Architecture", 7);
    assert_eq!((surface.board_id, surface.name.as_str(), surface.item_count), (3, "Architecture", 7));
    assert_near(surface.aspect, 2.0, "the surface's aspect");
}

#[test]
fn the_boards_view_is_the_phones_window_and_back() {
    let g = geometry();
    let view = View { pan: Pt::new(600.0, 300.0), zoom: 0.5 };
    let window = g.window_of(view);
    assert_near(window.x, 0.25, "x");
    assert_near(window.y, 0.125, "y");
    assert_near(window.w, 800.0 / 0.5 / 2000.0, "w: the canvas over the zoom over the box's width");
    assert_near(window.h, 600.0 / 0.5 / 2000.0, "h");
    let back = g.view_of(window);
    assert_near(back.zoom, 0.5, "zoom");
    assert_near(back.pan.x, 600.0, "pan x");
    assert_near(back.pan.y, 300.0, "pan y");
}

#[test]
fn a_view_that_is_no_view_is_the_boxs_corner_at_a_zoom_of_one() {
    let g = geometry();
    let window = g.window_of(View { pan: Pt::new(f32::NAN, 0.0), zoom: f32::NAN });
    assert!(window.x.is_finite() && window.y.is_finite() && window.w > 0.0 && window.h > 0.0, "{window:?}");
    assert_near(window.x, 0.0, "x");
    assert_near(window.w, 800.0 / 2000.0, "w at zoom 1");
}

#[test]
fn a_window_asking_for_more_zoom_than_the_board_may_be_drawn_at_is_held_about_its_middle() {
    let g = geometry();
    let tiny = Window { x: 0.4, y: 0.1, w: 0.001, h: 0.00075 };
    let view = g.view_of(tiny);
    assert_eq!(view.zoom, 4.0);
    let shown = g.window_of(view);
    // (the link crate holds the zoom it knows first: a window of 0.0625 sheet widths at the most; the board's own limit is held about its middle)
    assert_near(shown.x + shown.w / 2.0, 0.4 + 0.0625 / 2.0, "the same middle across");
    let wide = g.view_of(Window { x: 0.0, y: 0.0, w: 50.0, h: 37.5 });
    assert_eq!(wide.zoom, 0.2);
}

#[test]
fn a_drag_moves_the_view_over_the_board_by_what_the_finger_moved() {
    let g = geometry();
    let now = View { pan: Pt::new(100.0, 50.0), zoom: 0.5 };
    let mut held = Held::default();
    let _ = apply(&g, now, &mut held, gesture(1, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center), None);
    let moved = apply(&g, now, &mut held, gesture(1, Phase::Update, (0.1, 0.05), 1.0, Focus::Center), None);
    assert_near(moved.view.pan.x, 100.0 + 0.1 * 2000.0, "a tenth of the box's width to the right");
    assert_near(moved.view.pan.y, 50.0 + 0.05 * 2000.0, "and down");
    assert_near(moved.view.zoom, 0.5, "the zoom is the same");
    assert!(!moved.zoom_held);
}

#[test]
fn the_totals_of_a_gesture_are_applied_to_the_view_it_began_on() {
    let g = geometry();
    let now = View { pan: Pt::new(100.0, 50.0), zoom: 0.5 };
    let mut held = Held::default();
    let _ = apply(&g, now, &mut held, gesture(2, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center), None);
    let mut latest = now;
    for step in [0.02, 0.05, 0.09] {
        latest = apply(&g, latest, &mut held, gesture(2, Phase::Update, (step, 0.0), 1.0, Focus::Center), None).view;
    }
    assert_near(latest.pan.x, 100.0 + 0.09 * 2000.0, "the last total is the place, whatever came before");
}

#[test]
fn a_pinch_about_the_pen_keeps_the_board_point_under_the_pen_where_it_was_on_the_canvas() {
    let g = geometry();
    let now = View { pan: Pt::new(100.0, 50.0), zoom: 0.5 };
    let base = g.window_of(now);
    let pen = (base.x + base.w * 0.25, base.y + base.h * 0.5);
    let mut held = Held::default();
    let _ = apply(&g, now, &mut held, gesture(3, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen), Some(pen));
    let zoomed = apply(&g, now, &mut held, gesture(3, Phase::Update, (0.0, 0.0), 2.0, Focus::Pen), Some(pen));
    assert_near(zoomed.view.zoom, 1.0, "twice the zoom");
    let after = zoomed.window;
    assert_near((pen.0 - after.x) / after.w, 0.25, "across the canvas");
    assert_near((pen.1 - after.y) / after.h, 0.5, "down the canvas");
}

#[test]
fn a_zoom_past_the_limits_is_held_and_said() {
    let g = geometry();
    let now = View { pan: Pt::new(100.0, 50.0), zoom: 3.0 };
    let mut held = Held::default();
    let _ = apply(&g, now, &mut held, gesture(4, Phase::Begin, (0.0, 0.0), 1.0, Focus::Center), None);
    let far = apply(&g, now, &mut held, gesture(4, Phase::Update, (0.0, 0.0), 10.0, Focus::Center), None);
    assert_eq!(far.view.zoom, 4.0);
    assert!(far.zoom_held);
    let out = apply(&g, now, &mut held, gesture(4, Phase::Update, (0.0, 0.0), 0.05, Focus::Center), None);
    assert_eq!(out.view.zoom, 0.2);
    assert!(out.zoom_held);
}

#[test]
fn a_fit_and_a_reset_show_everything_on_the_board_with_a_margin_and_a_page_key_changes_nothing() {
    let g = geometry();
    let now = View { pan: Pt::new(1500.0, 800.0), zoom: 2.0 };
    let mut held = Held::default();
    for op in [ViewportOp::Fit, ViewportOp::Reset] {
        let fit = apply(&g, now, &mut held, op, None);
        let shown = g.window_of(fit.view);
        assert!(shown.x <= 0.0 && shown.x + shown.w >= 1.0, "the box's width is in view: {shown:?}");
        assert!(shown.y <= 0.0 && shown.y + shown.h >= 0.5, "and its height (the box is 0.5 of its width high): {shown:?}");
    }
    let step = apply(&g, now, &mut held, ViewportOp::PageStep(1), None);
    assert_near(step.view.zoom, 2.0, "zoom");
    assert_near(step.view.pan.x, 1500.0, "pan x");
    assert_near(step.view.pan.y, 800.0, "pan y");
}

#[test]
fn a_board_taller_than_the_canvas_is_fitted_by_its_height() {
    let tall = Geometry { items: Some(Rect { x: 0.0, y: 0.0, w: 400.0, h: 2000.0 }), ..geometry() };
    let fit = tall.whole();
    let box_h = 2000.0 / 400.0;
    assert!(fit.y <= 0.0 && fit.y + fit.h >= box_h, "the whole height is in view: {fit:?}");
    assert!(fit.x <= 0.0 && fit.x + fit.w >= 1.0, "and the whole width: {fit:?}");
}

#[test]
fn the_box_round_items_is_their_union() {
    let items = [Rect { x: 10.0, y: 20.0, w: 100.0, h: 50.0 }, Rect { x: 300.0, y: -40.0, w: 20.0, h: 20.0 }];
    assert_eq!(box_round(items), Some(Rect { x: 10.0, y: -40.0, w: 310.0, h: 110.0 }));
    assert_eq!(box_round(Vec::new()), None);
    assert_eq!(box_round([Rect { x: f32::NAN, y: 0.0, w: 1.0, h: 1.0 }]), None, "an item that is no rectangle is left out");
}

#[test]
fn nothing_the_board_or_the_phone_says_makes_a_number_that_is_not_one() {
    let nothing = Geometry { items: Some(Rect { x: f32::NAN, y: f32::NAN, w: f32::NAN, h: f32::NAN }), window: (0.0, 0.0), zoom: (f32::NAN, f32::NAN) };
    let ops = [
        ViewportOp::Fit,
        ViewportOp::Reset,
        ViewportOp::PageStep(3),
        gesture(1, Phase::Begin, (f32::NAN, 1.0e30), f32::INFINITY, Focus::Point { x: f32::NAN, y: 1.0e30 }),
        gesture(1, Phase::End, (1.0e30, -1.0e30), 0.0, Focus::Pen),
    ];
    for op in ops {
        let mut held = Held::default();
        let applied = apply(&nothing, View { pan: Pt::new(f32::NAN, f32::INFINITY), zoom: f32::NAN }, &mut held, op, Some((f32::NAN, 0.0)));
        let w = applied.window;
        assert!(w.x.is_finite() && w.y.is_finite() && w.w.is_finite() && w.h.is_finite() && w.w > 0.0 && w.h > 0.0, "{op:?}: {w:?}");
        assert!(applied.view.pan.x.is_finite() && applied.view.pan.y.is_finite(), "{op:?}");
        assert!((0.2..=4.0).contains(&applied.view.zoom), "{op:?}: {}", applied.view.zoom);
    }
}
