//! The zoom model of the review's pages: where the window looks at the pages, in the units the phone talks in, and what a command of
//! the phone (a drag, a pinch, a fit, a page key) makes of it. Pure: numbers in, numbers out, no window.
//!
//! The phone sees the pages as ONE COLUMN of sheets with a break between two (the link crate's `PageSurface`; the units are widths of
//! a sheet, `y` down, sheet `i` starts at `i * (1 / aspect + gap)`). AzReview lays them out as a strip of sheets side by side, each as
//! tall as the tallest page, scrolled sideways from page to page and up and down inside a page. This module is the adapter between the
//! two:
//!
//! - [`Geometry`] is what the layout says (a sheet's size, the gap, the pages, the window of the strip, all in px at zoom 1);
//! - [`Looking`] is where the window is in AzReview's own terms (the zoom, the page at the window's left edge, the place on it), and
//!   [`Looking::view`] / [`Looking::from_view`] turn it into the phone's `Rect` and back;
//! - [`apply`] is what a viewport command makes of the view: the link crate's own arithmetic (`viewport`: a gesture's totals applied to
//!   the view it began on, zoomed about the pen or the cursor and then moved, held inside the column; the fit; the page keys; the
//!   snap to a page top), so that the phone's prediction and the Mac agree to the pixel, and then what AzReview can show of it: the
//!   zoom between [`Z_MIN`] and [`Z_MAX`], the place held on the page, and the page the MIDDLE of the view is on (a drag across a
//!   break goes from the foot of one page to the head of the next: the breaks are respected).
//!
//! THE ZOOM is the factor the pages are drawn at ([`Looking::zoom`], pixels of the screen for a pixel of the page; 1.0 is how the pages
//! have always been drawn). The phone's zoom is the width of the window over the width of what is seen of a sheet, so a zoom of 1.0
//! there is [`Geometry::fit_zoom`] here.

use azul_review_link::{
    frame::{PageSurface, Phase, Rect, ViewportOp},
    viewport::{self, Space},
};

/// The least the pages are drawn at: half their size (two sheets in the window's width and more).
pub const Z_MIN: f32 = 0.5;
/// The most the pages are drawn at. A page is two layers of ink as big as the page times the display's pixel ratio, squared: past
/// this the memory of a page is more than a window may spend on it.
pub const Z_MAX: f32 = 3.0;

/// The size AzReview has when the layout gives none (an A4 sheet at the size the pages have been drawn at).
const DEFAULT_PAGE: (f32, f32) = (554.0, 784.0);

/// What the layout says of the strip of pages, px at zoom 1 (and the window's own px).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// A sheet's width.
    pub page_w: f32,
    /// A sheet's height: the strip is as high as its tallest page.
    pub page_h: f32,
    /// The space between two sheets.
    pub gap: f32,
    /// The pages of the file (at least one).
    pub pages: usize,
    /// What the window shows of the strip: its width and its height.
    pub window: (f32, f32),
}

impl Geometry {
    /// The geometry made safe: a size that is no size is the A4 sheet's, a negative gap none, no pages one, a window of nothing the size
    /// of a sheet. Everything below works on this.
    #[must_use]
    pub fn sane(self) -> Geometry {
        let size = |v: f32, default: f32| if v.is_finite() && v > 1.0 { v } else { default };
        let page_w = size(self.page_w, DEFAULT_PAGE.0);
        let page_h = size(self.page_h, DEFAULT_PAGE.1);
        Geometry {
            page_w,
            page_h,
            gap: if self.gap.is_finite() { self.gap.max(0.0) } else { 0.0 },
            pages: self.pages.max(1),
            window: (size(self.window.0, page_w), size(self.window.1, page_h)),
        }
    }

    /// Where the next sheet starts when this one does, px at zoom 1.
    #[must_use]
    pub fn stride(&self) -> f32 {
        self.page_w + self.gap
    }

    /// The zoom at which a sheet's width is the window's: the phone's zoom of 1.0.
    #[must_use]
    pub fn fit_zoom(&self) -> f32 {
        let g = self.sane();
        (g.window.0 / g.page_w).clamp(Z_MIN, Z_MAX)
    }

    /// The pages as the phone's column: one sheet wide, the shape and the break in sheet widths.
    #[must_use]
    pub fn surface(&self, file: &str, page_index: usize, line: usize, line_count: usize) -> PageSurface {
        let g = self.sane();
        let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        PageSurface {
            file: file.to_string(),
            page_index: to_u32(page_index.min(g.pages - 1)),
            page_count: to_u32(g.pages),
            line: to_u32(line),
            line_count: to_u32(line_count),
            aspect: g.page_w / g.page_h,
            gap: g.gap / g.page_w,
        }
    }

    /// Where the view may go: one sheet wide and the whole column high.
    #[must_use]
    pub fn space(&self) -> Space {
        Space::of_pages(&self.surface("", 0, 0, 0))
    }

    /// The farthest the window's top may be down a page, px at zoom 1, when the pages are drawn at `zoom` (a window taller than the page
    /// shows all of it: 0).
    #[must_use]
    pub fn max_down(&self, zoom: f32) -> f32 {
        (self.page_h - self.window.1 / zoom).max(0.0)
    }

    /// The farthest the window's left edge may be across a page, px at zoom 1, when the pages are drawn at `zoom` (a window wider than
    /// the page starts at the page: 0).
    #[must_use]
    pub fn max_across(&self, zoom: f32) -> f32 {
        (self.page_w - self.window.0 / zoom).max(0.0)
    }
}

/// Where the window is, in AzReview's own terms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Looking {
    /// The factor the pages are drawn at, between [`Z_MIN`] and [`Z_MAX`].
    pub zoom: f32,
    /// The page the window's left edge is on, from 0.
    pub page: usize,
    /// Where on that page the window's top left corner is, px at zoom 1 from the page's own top left.
    pub at: (f32, f32),
}

impl Looking {
    /// The pages as they have always been drawn: the first page, its top left, no zoom.
    pub const HOME: Looking = Looking { zoom: 1.0, page: 0, at: (0.0, 0.0) };

    /// The strip's scroll offset, in the px of the screen (the strip is `stride * zoom` wide for a page and `page_h * zoom` high).
    #[must_use]
    pub fn scroll(&self, g: &Geometry) -> (f32, f32) {
        let g = g.sane();
        ((self.page as f32 * g.stride() + self.at.0) * self.zoom, self.at.1 * self.zoom)
    }

    /// The window of a strip scrolled to `scroll` (px of the screen) with the pages drawn at `zoom`.
    #[must_use]
    pub fn from_scroll(g: &Geometry, zoom: f32, scroll: (f32, f32)) -> Looking {
        let g = g.sane();
        let zoom = held_zoom(zoom);
        let finite = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let across = finite(scroll.0) / zoom;
        let page = ((across / g.stride()).floor() as usize).min(g.pages - 1);
        let x = (across - page as f32 * g.stride()).max(0.0);
        Looking { zoom, page, at: (x.min(g.max_across(zoom)), (finite(scroll.1) / zoom).min(g.max_down(zoom))) }
    }

    /// What the phone sees: the part of the column the window shows, in sheet widths (the width of the window over the width a sheet is
    /// drawn at, and the same for the height; the page's top in the column is `page * (1 / aspect + gap)`).
    #[must_use]
    pub fn view(&self, g: &Geometry) -> Rect {
        let g = g.sane();
        let sheet = self.zoom * g.page_w;
        let column_top = self.page as f32 * (g.page_h + g.gap) / g.page_w;
        Rect { x: self.at.0 / g.page_w, y: column_top + self.at.1 / g.page_w, w: g.window.0 / sheet, h: g.window.1 / sheet }
    }

    /// The window a view of the column asks for, as far as the pages can be shown: the zoom between the limits, the page the middle of
    /// the view is on (a view that is across a break is on the page its middle is on), the place held on that page.
    #[must_use]
    pub fn from_view(g: &Geometry, view: Rect) -> Looking {
        let g = g.sane();
        let view = viewport::clamp_view_in(view, g.space());
        let zoom = held_zoom(g.window.0 / (view.w * g.page_w));
        // what the window shows at that zoom: when the zoom was held the view is the same one seen from the same middle
        let (w, h) = (g.window.0 / (zoom * g.page_w), g.window.1 / (zoom * g.page_w));
        let (x, y) = (view.x + (view.w - w) / 2.0, view.y + (view.h - h) / 2.0);
        let surface = g.surface("", 0, 0, 0);
        let page = (viewport::page_at(&surface, y + h / 2.0) as usize).min(g.pages - 1);
        let top = surface.page_top(page as u32);
        let down = ((y - top) * g.page_w).clamp(0.0, g.max_down(zoom));
        let across = (x * g.page_w).clamp(0.0, g.max_across(zoom));
        Looking { zoom, page, at: (across, down) }
    }
}

/// A zoom held between the limits; one that is no number is the pages as they have been drawn.
fn held_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() && zoom > 0.0 {
        zoom.clamp(Z_MIN, Z_MAX)
    } else {
        1.0
    }
}

/// What a command of the phone made of the view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Applied {
    /// Where the window is now.
    pub looking: Looking,
    /// What the phone is told: the view of that window (not the view the command asked for, which the pages may not be able to show).
    pub view: Rect,
    /// The command asked for more than the pages can be drawn at (or less): the zoom was held at the limit.
    pub zoom_held: bool,
}

/// The gesture a command belongs to, remembered from its Begin: the view it began on (the totals of its updates are totals since it
/// began, so each update is applied to this one and a dropped update loses nothing).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Held {
    pub gesture: Option<(u32, Rect)>,
}

/// The options of a command: the setting "snap to the top of a page after a drag", and where the pen is (the zoom's centre).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Context {
    pub snap_to_page_top: bool,
    /// The pen (or the cursor) in the column's units; `None` when it is away from the pages.
    pub pen: Option<(f32, f32)>,
}

/// What `op` makes of the window `now`: the link crate's arithmetic on the view the window has (or, for a gesture, on the view it had
/// when the gesture began), then what AzReview can show of it.
#[must_use]
pub fn apply(g: &Geometry, now: Looking, held: &mut Held, op: ViewportOp, context: Context) -> Applied {
    let g = g.sane();
    let space = g.space();
    let surface = g.surface("", 0, 0, 0);
    let current = now.view(&g);
    let asked = match op {
        ViewportOp::Gesture { id, phase, pan_x, pan_y, zoom, focus } => {
            let begins = phase == Phase::Begin || held.gesture.is_none_or(|(known, _)| known != id);
            if begins {
                held.gesture = Some((id, current));
            }
            match held.gesture {
                Some((_, base)) => {
                    let about = viewport::resolve_focus(focus, base, context.pen);
                    let mut view = viewport::apply_gesture_in(base, (pan_x, pan_y), zoom, about, space);
                    if phase == Phase::End {
                        if context.snap_to_page_top {
                            view = viewport::snap_to_page_top(view, &surface);
                        }
                        held.gesture = None;
                    }
                    view
                }
                None => current,
            }
        }
        ViewportOp::Fit => viewport::fit_width_in(current, space),
        ViewportOp::PageStep(steps) => viewport::step_page(current, &surface, i32::from(steps)),
        ViewportOp::Reset => viewport::reset_to_page_top(current, &surface),
    };
    let looking = Looking::from_view(&g, asked);
    // the zoom the command asked for, and whether the pages could be drawn at it
    let wanted = g.window.0 / (viewport::clamp_view_in(asked, space).w * g.page_w);
    let zoom_held = (wanted - looking.zoom).abs() > 1e-3 * wanted.max(1.0);
    Applied { looking, view: looking.view(&g), zoom_held }
}

/// Where the pen is in the column's units, when it is on `page` at `at` (px at zoom 1 from the page's top left): what a zoom about the
/// pen needs.
#[must_use]
pub fn pen_in_column(g: &Geometry, page: usize, at: (f32, f32)) -> (f32, f32) {
    let g = g.sane();
    let column_top = page.min(g.pages - 1) as f32 * (g.page_h + g.gap) / g.page_w;
    (at.0 / g.page_w, column_top + at.1 / g.page_w)
}

#[cfg(test)]
#[path = "page_view_test.rs"]
mod page_view_test;
