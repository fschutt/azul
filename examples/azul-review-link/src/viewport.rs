//! Where the Mac's viewport goes when the phone drags, pinches or fits: pure arithmetic on a [`Rect`] of a SURFACE (the
//! pages of the code review or the whiteboard; the units are in `frame.rs`: widths of the surface, `y` down). The phone uses
//! it to show the surface moving under the finger at once; AzReview's zoom model can use it as it is (or as the reference
//! for its own): both then agree to the pixel.
//!
//! The visible width is `1 / zoom` of the surface: zoom 1.0 is its width fitted to the window's width. A zoom keeps the
//! window's SHAPE (a view's height stays the same multiple of its width). Where a view may go is its [`Space`]: over the
//! pages the width of a sheet and the height of the whole column (a drag crosses the breaks, a pan sideways stops at the
//! sheet's edges); over the whiteboard the board and a margin round it.
//!
//! THE PAN SIGN, defined once here ([`finger_to_pan`]): a finger that moves by (dx, dy) on the phone's pad moves the CONTENT
//! the same way, so the viewport moves by the opposite: `pan = -finger / px_per_unit`. That is direct manipulation, as a map
//! on a phone. The Mac's setting "reverse the pan direction" ([`PanDirection::ViewportFollowsFinger`]) flips it: the
//! viewport moves with the finger. The wire's `pan_x`/`pan_y` are always the VIEWPORT's movement (positive: toward the right
//! and the bottom), so the Mac never needs to know which was chosen.

use crate::frame::{Focus, PageSurface, Rect};

/// The least the page may be zoomed (a quarter of the fitted size: four page widths in view).
pub const MIN_ZOOM: f32 = 0.25;
/// The most the page may be zoomed.
pub const MAX_ZOOM: f32 = 16.0;

/// How far beyond the board a view may go, in board widths (so that an item at the edge can be brought to the middle).
pub const BOARD_MARGIN: f32 = 0.25;

/// Where a view may go: the plane's top left corner and its size, in surface units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Space {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Space {
    /// One sheet as wide as it is high: what the viewport functions without `_in` work in.
    pub const UNIT: Space = Space { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };

    /// The column of sheets of the code review: one sheet wide, the whole column high.
    #[must_use]
    pub fn of_pages(page: &PageSurface) -> Space {
        Space { x: 0.0, y: 0.0, width: 1.0, height: page.column_height() }
    }

    /// The whiteboard (its width over its height): the board and a margin round it.
    #[must_use]
    pub fn of_board(aspect: f32) -> Space {
        let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { 1.0 };
        Space { x: -BOARD_MARGIN, y: -BOARD_MARGIN, width: 1.0 + 2.0 * BOARD_MARGIN, height: 1.0 / aspect + 2.0 * BOARD_MARGIN }
    }
}

/// Which way the viewport moves when the finger does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanDirection {
    /// The content follows the finger (the viewport goes the other way): direct manipulation, the default.
    #[default]
    ContentFollowsFinger,
    /// The viewport follows the finger (the Mac's "reverse the pan direction").
    ViewportFollowsFinger,
}

impl PanDirection {
    /// The direction the Mac's setting asks for.
    #[must_use]
    pub fn from_reverse(reverse: bool) -> PanDirection {
        if reverse {
            PanDirection::ViewportFollowsFinger
        } else {
            PanDirection::ContentFollowsFinger
        }
    }
}

/// How far the viewport moves, in surface units, when a finger moves by `finger` pixels on a pad that shows the surface at
/// `px_per_unit` pixels a unit. THE sign of the pan, said once (see the top of this file).
#[must_use]
pub fn finger_to_pan(finger: (f32, f32), px_per_unit: f32, direction: PanDirection) -> (f32, f32) {
    if !px_per_unit.is_finite() || px_per_unit <= 0.0 {
        return (0.0, 0.0);
    }
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    let sign = match direction {
        PanDirection::ContentFollowsFinger => -1.0,
        PanDirection::ViewportFollowsFinger => 1.0,
    };
    (sign * finite(finger.0) / px_per_unit, sign * finite(finger.1) / px_per_unit)
}

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

/// Where a view of `size` starts on the axis that runs from `lo` for `len`: inside it where it is smaller, centred where it is
/// larger.
fn held(start: f32, size: f32, lo: f32, len: f32) -> f32 {
    if size >= len {
        lo + (len - size) / 2.0
    } else {
        start.clamp(lo, lo + len - size)
    }
}

/// `view` held where it may be: zoom between the limits, inside the page where it is smaller than the page, centred
/// where it is larger. A view with a number that is no number is the whole page.
#[must_use]
pub fn clamp_view(view: Rect) -> Rect {
    clamp_view_in(view, Space::UNIT)
}

/// `view` held in `space`: the zoom between its limits (the shape kept), inside the plane on an axis where it is smaller than
/// the plane, centred on it where it is larger. A view with a number that is no number is the whole surface.
#[must_use]
pub fn clamp_view_in(view: Rect, space: Space) -> Rect {
    let finite = view.x.is_finite() && view.y.is_finite() && view.w.is_finite() && view.h.is_finite();
    let view = if !finite || view.w <= 0.0 || view.h <= 0.0 { WHOLE } else { view };
    // the zoom between its limits, the shape kept
    let zoom = (1.0 / view.w).clamp(MIN_ZOOM, MAX_ZOOM);
    let w = 1.0 / zoom;
    let h = view.h * (w / view.w);
    Rect { x: held(view.x, w, space.x, space.width), y: held(view.y, h, space.y, space.height), w, h }
}

/// `view` zoomed by `factor` (above 1 zooms in) with the point `about` (page fractions) staying where it is on the
/// screen. A factor or point that is no number changes nothing.
#[must_use]
pub fn zoom_about(view: Rect, factor: f32, about: (f32, f32)) -> Rect {
    zoom_about_in(view, factor, about, Space::UNIT)
}

/// [`zoom_about`] in `space`.
#[must_use]
pub fn zoom_about_in(view: Rect, factor: f32, about: (f32, f32), space: Space) -> Rect {
    if !factor.is_finite() || factor <= 0.0 || !about.0.is_finite() || !about.1.is_finite() {
        return view;
    }
    let view = clamp_view_in(view, space);
    // the zoom the factor asks for, held between the limits
    let zoom = (zoom_of(view) * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    let scale = view.w * zoom;
    let (w, h) = (view.w / scale, view.h / scale);
    // the point keeps its place inside the view
    let (rx, ry) = ((about.0 - view.x) / view.w, (about.1 - view.y) / view.h);
    clamp_view_in(Rect { x: about.0 - rx * w, y: about.1 - ry * h, w, h }, space)
}

/// `view` moved by (`dx`, `dy`) page fractions (positive: toward the right and the bottom: the content goes
/// the other way).
#[must_use]
pub fn pan(view: Rect, dx: f32, dy: f32) -> Rect {
    pan_in(view, dx, dy, Space::UNIT)
}

/// [`pan`] in `space`: the viewport's movement (see [`finger_to_pan`] for what the finger makes of it).
#[must_use]
pub fn pan_in(view: Rect, dx: f32, dy: f32, space: Space) -> Rect {
    let view = clamp_view_in(view, space);
    let dx = if dx.is_finite() { dx } else { 0.0 };
    let dy = if dy.is_finite() { dy } else { 0.0 };
    clamp_view_in(Rect { x: view.x + dx, y: view.y + dy, ..view }, space)
}

/// `view` fitted to the width of the page, its vertical centre kept (the double tap).
#[must_use]
pub fn fit_width(view: Rect) -> Rect {
    fit_width_in(view, Space::UNIT)
}

/// [`fit_width`] in `space`.
#[must_use]
pub fn fit_width_in(view: Rect, space: Space) -> Rect {
    let view = clamp_view_in(view, space);
    let h = view.h / view.w;
    let centre = view.y + view.h / 2.0;
    clamp_view_in(Rect { x: 0.0, y: centre - h / 2.0, w: 1.0, h }, space)
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
    apply_gesture_in(base, pan_by, zoom, about, Space::UNIT)
}

/// [`apply_gesture`] in `space`.
#[must_use]
pub fn apply_gesture_in(base: Rect, pan_by: (f32, f32), zoom: f32, about: (f32, f32), space: Space) -> Rect {
    pan_in(zoom_about_in(base, zoom, about, space), pan_by.0, pan_by.1, space)
}

// ---- the pages as one column

/// The page whose band holds `y`: a page's band runs from half the break before it to half the break after it. Before the
/// first page it is the first, after the last the last.
#[must_use]
pub fn page_at(page: &PageSurface, y: f32) -> u32 {
    if !y.is_finite() {
        return 0;
    }
    let step = page.page_height() + page.break_height();
    let index = ((y + page.break_height() / 2.0) / step).floor();
    let last = page.page_count.max(1) - 1;
    if index <= 0.0 {
        0
    } else {
        (index as u32).min(last)
    }
}

/// `view` with the top of the nearest page put at its top (the release of a drag that snaps to the page top).
#[must_use]
pub fn snap_to_page_top(view: Rect, page: &PageSurface) -> Rect {
    let space = Space::of_pages(page);
    let view = clamp_view_in(view, space);
    let step = page.page_height() + page.break_height();
    let last = page.page_count.max(1) - 1;
    let nearest = (view.y / step).round().clamp(0.0, last as f32) as u32;
    clamp_view_in(Rect { y: page.page_top(nearest), ..view }, space)
}

/// `view` moved to the top of the page `steps` away from the one its middle is on (the page keys); the width and the shape
/// are kept, the first and last page are the ends.
#[must_use]
pub fn step_page(view: Rect, page: &PageSurface, steps: i32) -> Rect {
    let space = Space::of_pages(page);
    let view = clamp_view_in(view, space);
    let current = i64::from(page_at(page, view.y + view.h / 2.0));
    let last = i64::from(page.page_count.max(1) - 1);
    let target = (current + i64::from(steps)).clamp(0, last) as u32;
    clamp_view_in(Rect { y: page.page_top(target), ..view }, space)
}

/// `view` at the fitted zoom and the top of the page its middle is on (a reset over the pages).
#[must_use]
pub fn reset_to_page_top(view: Rect, page: &PageSurface) -> Rect {
    let space = Space::of_pages(page);
    let view = clamp_view_in(view, space);
    let current = page_at(page, view.y + view.h / 2.0);
    clamp_view_in(Rect { x: 0.0, y: page.page_top(current), w: 1.0, h: view.h / view.w }, space)
}

#[cfg(test)]
#[path = "viewport_test.rs"]
mod viewport_test;
