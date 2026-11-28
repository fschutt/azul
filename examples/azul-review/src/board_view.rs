//! The zoom model of the whiteboard for the phone's pad: where the window looks at the board, in the units the phone talks in, and what a
//! drag, a pinch and a fit make of it. Pure: numbers in, numbers out, no window.
//!
//! Over the whiteboard the pad is a free two-dimensional pan and zoom (there are no pages and no breaks). The phone's units are widths of
//! the BOX ROUND THE ITEMS of the board (x 0 to 1, y 0 to 1 / aspect; the box is the units' own, so that a board that grows keeps its
//! items' places in the box relative to its width); the board's own view is the board point at the canvas's top left and how many px of
//! the canvas a board px is ([`View`]). This module is the adapter between the two, over the link crate's arithmetic (`viewport`) in the
//! board's space (the box and a margin round it), as `page_view` is for the pages.

use azul_review_link::{
    frame::{Phase, Rect as Window, ViewportOp, WhiteboardSurface},
    limits::{MAX_ASPECT, MIN_ASPECT},
    viewport::{self, Space},
};

use crate::{
    page_view::Held,
    whiteboard_geom::{Pt, Rect, View},
};

/// The box a board with nothing on it has, board px.
const EMPTY_BOX: Rect = Rect { x: 0.0, y: 0.0, w: 1000.0, h: 750.0 };
/// The canvas a window of no size has, px.
const EMPTY_WINDOW: (f32, f32) = (800.0, 600.0);
/// How much of the box's width is left round it when it is fitted, as a fraction of its width.
pub const FIT_MARGIN: f32 = 0.08;

/// What the board and its canvas say.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// The box round the items, board px (`None`: nothing is on the board).
    pub items: Option<Rect>,
    /// The canvas the board is drawn on, px.
    pub window: (f32, f32),
    /// The zoom the board may be drawn at: the least and the most window px a board px may be.
    pub zoom: (f32, f32),
}

impl Geometry {
    /// The geometry made safe: a box that is no box (or none) is the empty board's, a canvas of nothing a standard one, zoom limits that are no
    /// limits the board's usual.
    #[must_use]
    pub fn sane(self) -> Geometry {
        let items = self.items.filter(|r| [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) && r.w >= 1.0 && r.h >= 1.0);
        let window = (
            if self.window.0.is_finite() && self.window.0 >= 1.0 { self.window.0 } else { EMPTY_WINDOW.0 },
            if self.window.1.is_finite() && self.window.1 >= 1.0 { self.window.1 } else { EMPTY_WINDOW.1 },
        );
        let zoom = if self.zoom.0.is_finite() && self.zoom.1.is_finite() && self.zoom.0 > 0.0 && self.zoom.1 >= self.zoom.0 { self.zoom } else { (0.2, 4.0) };
        Geometry { items: Some(items.unwrap_or(EMPTY_BOX)), window, zoom }
    }

    /// The box round the items (the empty board's when there is none).
    #[must_use]
    pub fn items_box(&self) -> Rect {
        self.sane().items.unwrap_or(EMPTY_BOX)
    }

    /// Board px for a unit of the phone's: the width of the box.
    #[must_use]
    pub fn unit(&self) -> f32 {
        self.items_box().w
    }

    /// The box's width over its height, held to what a surface may be.
    #[must_use]
    pub fn aspect(&self) -> f32 {
        let b = self.items_box();
        (b.w / b.h).clamp(MIN_ASPECT, MAX_ASPECT)
    }

    /// Where the phone's view may go: the box and a margin round it.
    #[must_use]
    pub fn space(&self) -> Space {
        Space::of_board(self.aspect())
    }

    /// The surface as the phone is told of it.
    #[must_use]
    pub fn surface(&self, board_id: u32, name: &str, item_count: usize) -> WhiteboardSurface {
        WhiteboardSurface { board_id, name: name.to_string(), aspect: self.aspect(), item_count: u32::try_from(item_count).unwrap_or(u32::MAX) }
    }

    /// A board point in the phone's units (the pen over the board).
    #[must_use]
    pub fn to_units(&self, p: Pt) -> (f32, f32) {
        let b = self.items_box();
        ((p.x - b.x) / b.w, (p.y - b.y) / b.w)
    }

    /// The board's view as the phone sees it: the part of the box's plane the canvas shows.
    #[must_use]
    pub fn window_of(&self, view: View) -> Window {
        let g = self.sane();
        let b = g.items_box();
        let zoom = if view.zoom.is_finite() && view.zoom > 0.0 { view.zoom.clamp(g.zoom.0, g.zoom.1) } else { 1.0 };
        let pan = if view.pan.x.is_finite() && view.pan.y.is_finite() { view.pan } else { Pt::new(b.x, b.y) };
        Window { x: (pan.x - b.x) / b.w, y: (pan.y - b.y) / b.w, w: g.window.0 / zoom / b.w, h: g.window.1 / zoom / b.w }
    }

    /// The board's view for a window of the phone's: the zoom between the limits (the window seen from the same middle when it was held), the
    /// pan from the window's corner.
    #[must_use]
    pub fn view_of(&self, window: Window) -> View {
        let g = self.sane();
        let b = g.items_box();
        let window = viewport::clamp_view_in(window, g.space());
        let zoom = (g.window.0 / (window.w * b.w)).clamp(g.zoom.0, g.zoom.1);
        let (w, h) = (g.window.0 / (zoom * b.w), g.window.1 / (zoom * b.w));
        let (x, y) = (window.x + (window.w - w) / 2.0, window.y + (window.h - h) / 2.0);
        View { pan: Pt::new(b.x + x * b.w, b.y + y * b.w), zoom }
    }

    /// The window that shows the whole box with a margin round it, centred on it (a fit of everything on the board).
    #[must_use]
    pub fn whole(&self) -> Window {
        let g = self.sane();
        let b = g.items_box();
        let shape = g.window.1 / g.window.0;
        let box_h = b.h / b.w;
        let w = (1.0 + 2.0 * FIT_MARGIN).max((box_h + 2.0 * FIT_MARGIN) / shape);
        let h = w * shape;
        Window { x: 0.5 - w / 2.0, y: box_h / 2.0 - h / 2.0, w, h }
    }
}

/// What a command of the phone made of the board's view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Applied {
    /// The view the board is to be drawn with.
    pub view: View,
    /// What the phone is told: the window of that view (not the one the command asked for, which the board may not be able to show).
    pub window: Window,
    /// The command asked for more zoom than the board may be drawn at (or less): it was held at the limit.
    pub zoom_held: bool,
}

/// What `op` makes of the board's view `now`: the link crate's arithmetic on the window the view has (or, for a gesture, on the one it had when
/// the gesture began) in the board's space, then what the board can show of it. A fit and a reset show everything on the board; a page key
/// has no pages here and changes nothing.
#[must_use]
pub fn apply(g: &Geometry, now: View, held: &mut Held, op: ViewportOp, pen: Option<(f32, f32)>) -> Applied {
    let g = g.sane();
    let space = g.space();
    let current = g.window_of(now);
    let asked = match op {
        ViewportOp::Gesture { id, phase, pan_x, pan_y, zoom, focus } => {
            if phase == Phase::Begin || held.gesture.is_none_or(|(known, _)| known != id) {
                held.gesture = Some((id, current));
            }
            match held.gesture {
                Some((_, base)) => {
                    let about = viewport::resolve_focus(focus, base, pen);
                    let window = viewport::apply_gesture_in(base, (pan_x, pan_y), zoom, about, space);
                    if phase == Phase::End {
                        held.gesture = None;
                    }
                    window
                }
                None => current,
            }
        }
        ViewportOp::Fit | ViewportOp::Reset => g.whole(),
        ViewportOp::PageStep(_) => current,
    };
    let view = g.view_of(asked);
    let wanted = g.window.0 / (viewport::clamp_view_in(asked, space).w * g.unit());
    let held_at_limit = (wanted - view.zoom).abs() > 1e-3 * wanted.max(1.0);
    Applied { view, window: g.window_of(view), zoom_held: held_at_limit }
}

/// The box round a board's items: the union of their rectangles (`None`: no items).
#[must_use]
pub fn box_round(rects: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    rects.into_iter().filter(|r| [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite())).fold(None, |acc: Option<Rect>, r| {
        Some(match acc {
            None => r,
            Some(a) => {
                let (left, top) = (a.x.min(r.x), a.y.min(r.y));
                let (right, bottom) = ((a.x + a.w).max(r.x + r.w), (a.y + a.h).max(r.y + r.h));
                Rect { x: left, y: top, w: right - left, h: bottom - top }
            }
        })
    })
}

#[cfg(test)]
#[path = "board_view_test.rs"]
mod board_view_test;
