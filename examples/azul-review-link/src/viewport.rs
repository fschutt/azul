//! Where the Mac's viewport goes when the phone drags, pinches or fits: pure arithmetic on a [`Rect`] of the page
//! in fractions (see `frame.rs`). The phone uses it to show the rectangle moving under the finger at once; AzReview's
//! zoom model can use it as it is (or as the reference for its own): both then agree to the pixel.
//!
//! The visible width of the page is `1 / zoom` of it: zoom 1.0 is the page fitted to the window's width. A zoom
//! keeps the window's SHAPE (a view's height stays the same multiple of its width).

use crate::frame::{Focus, Rect};

/// The least the page may be zoomed (a quarter of the fitted size: four page widths in view).
pub const MIN_ZOOM: f32 = 0.25;
/// The most the page may be zoomed.
pub const MAX_ZOOM: f32 = 16.0;

/// The zoom of a view (1.0: the whole width of the page in view).
#[must_use]
pub fn zoom_of(view: Rect) -> f32 {
    if view.w.is_finite() && view.w > 0.0 {
        1.0 / view.w
    } else {
        1.0
    }
}

/// The whole page: what a view that is no view becomes.
const WHOLE: Rect = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

/// Where a view of `size` starts on an axis of length 1: inside it where it is smaller, centred where it is larger.
fn held(start: f32, size: f32) -> f32 {
    if size >= 1.0 {
        (1.0 - size) / 2.0
    } else {
        start.clamp(0.0, 1.0 - size)
    }
}

/// `view` held where it may be: zoom between the limits, inside the page where it is smaller than the page, centred
/// where it is larger. A view with a number that is no number is the whole page.
#[must_use]
pub fn clamp_view(view: Rect) -> Rect {
    let finite = view.x.is_finite() && view.y.is_finite() && view.w.is_finite() && view.h.is_finite();
    if !finite || view.w <= 0.0 || view.h <= 0.0 {
        return WHOLE;
    }
    // the zoom between its limits, the shape kept
    let zoom = (1.0 / view.w).clamp(MIN_ZOOM, MAX_ZOOM);
    let w = 1.0 / zoom;
    let h = view.h * (w / view.w);
    Rect { x: held(view.x, w), y: held(view.y, h), w, h }
}

/// `view` zoomed by `factor` (above 1 zooms in) with the point `about` (page fractions) staying where it is on the
/// screen. A factor or point that is no number changes nothing.
#[must_use]
pub fn zoom_about(view: Rect, factor: f32, about: (f32, f32)) -> Rect {
    if !factor.is_finite() || factor <= 0.0 || !about.0.is_finite() || !about.1.is_finite() {
        return view;
    }
    let view = clamp_view(view);
    // the zoom the factor asks for, held between the limits
    let zoom = (zoom_of(view) * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    let scale = view.w * zoom;
    let (w, h) = (view.w / scale, view.h / scale);
    // the point keeps its place inside the view
    let (rx, ry) = ((about.0 - view.x) / view.w, (about.1 - view.y) / view.h);
    clamp_view(Rect { x: about.0 - rx * w, y: about.1 - ry * h, w, h })
}

/// `view` moved by (`dx`, `dy`) page fractions (positive: toward the right and the bottom: the content goes
/// the other way).
#[must_use]
pub fn pan(view: Rect, dx: f32, dy: f32) -> Rect {
    let view = clamp_view(view);
    let dx = if dx.is_finite() { dx } else { 0.0 };
    let dy = if dy.is_finite() { dy } else { 0.0 };
    clamp_view(Rect { x: view.x + dx, y: view.y + dy, ..view })
}

/// `view` fitted to the width of the page, its vertical centre kept (the double tap).
#[must_use]
pub fn fit_width(view: Rect) -> Rect {
    let view = clamp_view(view);
    let h = view.h / view.w;
    let centre = view.y + view.h / 2.0;
    clamp_view(Rect { x: 0.0, y: centre - h / 2.0, w: 1.0, h })
}

/// `view` at the fitted zoom and the top of the page.
#[must_use]
pub fn reset(view: Rect) -> Rect {
    let view = clamp_view(view);
    Rect { x: 0.0, y: 0.0, w: 1.0, h: view.h / view.w }
}

/// The point a zoom is about: the pen (or the cursor) where it is, else the middle of the view; always inside the
/// view (a pen that is elsewhere on the page zooms about the nearest point of what is in view).
#[must_use]
pub fn resolve_focus(focus: Focus, view: Rect, pen: Option<(f32, f32)>) -> (f32, f32) {
    let middle = (view.x + view.w / 2.0, view.y + view.h / 2.0);
    let wanted = match focus {
        Focus::Center => None,
        Focus::Pen => pen,
        Focus::Point { x, y } => Some((x, y)),
    };
    match wanted {
        Some((x, y)) if x.is_finite() && y.is_finite() && view.w > 0.0 && view.h > 0.0 => {
            (x.clamp(view.x, view.x + view.w), y.clamp(view.y, view.y + view.h))
        }
        _ => middle,
    }
}

/// What a gesture has made of the view it began on: zoomed by `zoom` about `about`, then moved by `pan`. Both are
/// CUMULATIVE since the gesture began, so the result depends on `base` and these numbers only (a dropped update
/// loses nothing).
#[must_use]
pub fn apply_gesture(base: Rect, pan_by: (f32, f32), zoom: f32, about: (f32, f32)) -> Rect {
    pan(zoom_about(base, zoom, about), pan_by.0, pan_by.1)
}

#[cfg(test)]
#[path = "viewport_test.rs"]
mod viewport_test;
