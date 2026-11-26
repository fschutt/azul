//! What the pad draws of the surface the pen is over, as numbers: the viewport frame stays in the middle of the pad, the
//! sheets of the code review (or the whiteboard) move under it. A finger that moves the content therefore moves what the user
//! sees by the same number of pixels, and the scale (pixels a unit) is the one place the pad's drag and its drawing agree on.
//! No azul in here: `ui/remote.rs` turns the numbers into nodes.

use azul_review_link::frame::{PenMark, Rect, Surface};
use azul_review_link::viewport::page_at;

use crate::layout::{Zone, STRIP_W};

/// How much of the width of the drawn area the frame takes.
pub const FRAME_W_SHARE: f32 = 0.55;
/// How much of the height of the pad the frame takes at most.
pub const FRAME_H_SHARE: f32 = 0.80;
/// How wide the overview inset of a whiteboard is, at most, as a share of the pad.
pub const MINIMAP_SHARE: f32 = 0.30;
/// The room between the overview inset and the pad's edge.
pub const INSET_MARGIN: f32 = 8.0;

/// One sheet in sight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sheet {
    pub index: u32,
    /// Where it is in the pad's pixels (may reach beyond the pad).
    pub rect: Zone,
    /// The sheet the middle of the view is on.
    pub current: bool,
}

/// The overview of a whiteboard in a corner of the pad: the whole board and where the view is on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MiniMap {
    pub board: Zone,
    pub frame: Zone,
}

/// What the pad shows.
#[derive(Debug, Clone, PartialEq)]
pub struct PadScene {
    /// Pixels a surface unit takes on the pad.
    pub px_per_unit: f32,
    /// The part of the pad the surface is drawn in (over the pages the strip along the edge is left out).
    pub content: Zone,
    /// The viewport: what the Mac shows, fixed in the middle of the content.
    pub frame: Zone,
    /// Over the pages: the sheets that show.
    pub sheets: Vec<Sheet>,
    /// Over a whiteboard: the box of the board.
    pub board: Option<Zone>,
    /// Where the pen is, in the pad's pixels, and whether it is down.
    pub pen: Option<(f32, f32, bool)>,
    /// Over a whiteboard: the overview inset.
    pub minimap: Option<MiniMap>,
}

/// A view that is a view: finite, with a size. Anything else is the whole surface.
fn sane(view: Rect) -> Rect {
    let finite = view.x.is_finite() && view.y.is_finite() && view.w.is_finite() && view.h.is_finite();
    if finite && view.w > 0.0 && view.h > 0.0 {
        view
    } else {
        Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }
    }
}

/// The part of the pad the surface is drawn in: all of it, but over the pages the strip along the edge.
fn content_of(pad: (f32, f32), pages: bool) -> Zone {
    let width = if pad.0.is_finite() { pad.0.max(0.0) } else { 0.0 };
    let height = if pad.1.is_finite() { pad.1.max(0.0) } else { 0.0 };
    Zone { x: 0.0, y: 0.0, w: if pages { (width - STRIP_W).max(0.0) } else { width }, h: height }
}

/// The pixels a unit takes when `view` is shown in a pad of `pad` pixels: the frame is a share of the pad (`pages`: the strip
/// is not the surface's). A view that is no view is the whole page.
#[must_use]
pub fn px_per_unit(pad: (f32, f32), pages: bool, view: Rect) -> f32 {
    let content = content_of(pad, pages);
    let view = sane(view);
    (FRAME_W_SHARE * content.w / view.w).min(FRAME_H_SHARE * content.h / view.h).max(0.0)
}

/// The scene of `view` over `surface` in a pad of `pad` pixels.
#[must_use]
pub fn scene(surface: &Surface, view: Rect, pen: Option<PenMark>, pad: (f32, f32)) -> PadScene {
    let pages = matches!(surface, Surface::Page(_));
    let content = content_of(pad, pages);
    let view = sane(view);
    let s = px_per_unit(pad, pages, view);
    let (cx, cy) = (content.w / 2.0, content.h / 2.0);
    let (vcx, vcy) = (view.x + view.w / 2.0, view.y + view.h / 2.0);
    let px = |u: f32| cx + (u - vcx) * s;
    let py = |v: f32| cy + (v - vcy) * s;
    let frame = Zone { x: cx - view.w * s / 2.0, y: cy - view.h * s / 2.0, w: view.w * s, h: view.h * s };
    let pen = pen.filter(|m| m.x.is_finite() && m.y.is_finite()).map(|m| (px(m.x), py(m.y), m.down));
    let mut scene = PadScene { px_per_unit: s, content, frame, sheets: Vec::new(), board: None, pen, minimap: None };
    match surface {
        Surface::Page(page) => {
            let current = page_at(page, vcy);
            let shows = |rect: &Zone| rect.bottom() > 0.0 && rect.y < content.h && rect.right() > 0.0 && rect.x < content.w;
            for index in 0..page.page_count.max(1) {
                let rect = Zone { x: px(0.0), y: py(page.page_top(index)), w: s, h: s * page.page_height() };
                if shows(&rect) {
                    scene.sheets.push(Sheet { index, rect, current: index == current });
                }
            }
        }
        Surface::Whiteboard(board) => {
            let aspect = if board.aspect.is_finite() && board.aspect > 0.0 { board.aspect } else { 1.0 };
            scene.board = Some(Zone { x: px(0.0), y: py(0.0), w: s, h: s / aspect });
            // the overview: the whole board in a corner, and the frame where the view is on it (held inside the board's box)
            let width = (MINIMAP_SHARE * content.w).min(content.w - 2.0 * INSET_MARGIN).max(0.0);
            let height = width / aspect;
            let box_ = Zone { x: content.w - INSET_MARGIN - width, y: content.h - INSET_MARGIN - height, w: width, h: height };
            let (x0, y0) = (box_.x + view.x * width, box_.y + view.y * width);
            let (x1, y1) = (x0 + view.w * width, y0 + view.h * width);
            let held = |v: f32, lo: f32, hi: f32| v.clamp(lo, hi.max(lo));
            let (fx0, fy0) = (held(x0, box_.x, box_.right()), held(y0, box_.y, box_.bottom()));
            let (fx1, fy1) = (held(x1, box_.x, box_.right()), held(y1, box_.y, box_.bottom()));
            scene.minimap = Some(MiniMap { board: box_, frame: Zone { x: fx0, y: fy0, w: (fx1 - fx0).max(0.0), h: (fy1 - fy0).max(0.0) } });
        }
    }
    scene
}

#[cfg(test)]
#[path = "padscene_test.rs"]
mod padscene_test;
