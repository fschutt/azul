//! Where a cutout goes that was sent to the whiteboard by a button and not dropped by a hand: the first place of the board's
//! view where it overlaps nothing, in reading order (the top row first, the left first in it), a gap away from what is
//! there and from the edge of the view. When the view has no such room the cutout goes under everything else on the board,
//! and the view is moved to show it. Pure: rectangles in, a place out.

use crate::whiteboard_geom::{Pt, Rect, Size};

/// The space kept free round a cutout that is put somewhere, and between it and the edge of the view, board px.
pub const GAP: f32 = 16.0;
/// The most cutouts that are looked at when the view is searched: a view with more in it is full, whatever the gaps
/// between them (and the search is quadratic in what it looks at).
pub const MAX_LOOKED_AT: usize = 96;
/// Less than this is no distance.
const EPS: f32 = 0.01;

/// Where a cutout is put.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spot {
    /// In the view, where nothing else is (the top-left corner of the cutout).
    InView(Pt),
    /// The view is full: under everything on the board (the view is moved to show it).
    Below(Pt),
}

impl Spot {
    /// The top-left corner of the cutout.
    #[must_use]
    pub const fn at(&self) -> Pt {
        match *self {
            Self::InView(p) | Self::Below(p) => p,
        }
    }
}

/// The place for a cutout of `size` among the rectangles `taken`, in the `view` (all in board px), `gap` away from every
/// one of them and from the view's edge. A cutout that is larger than the view cannot be inside it: it is only asked to
/// start inside, and to overlap nothing.
#[must_use]
pub fn free_spot(taken: &[Rect], view: Rect, size: Size, gap: f32) -> Spot {
    let fits_x = size.w + 2.0 * gap <= view.w;
    let fits_y = size.h + 2.0 * gap <= view.h;
    let (x0, y0) = (view.x + gap, view.y + gap);
    // What is out of the view cannot be in the way of a place in it, unless the cutout reaches out of the view.
    let relevant: Vec<Rect> = taken.iter().copied().filter(|t| !(fits_x && fits_y) || t.inflate(gap).intersects(&view)).collect();
    if relevant.len() <= MAX_LOOKED_AT {
        // The places a cutout slid up and to the left until something stops it can be at: a margin, or a gap past the
        // right or the bottom of a cutout.
        let mut xs = vec![x0];
        xs.extend(relevant.iter().map(|t| t.right() + gap));
        let mut ys = vec![y0];
        ys.extend(relevant.iter().map(|t| t.bottom() + gap));
        xs.retain(|x| *x >= x0 - EPS);
        ys.retain(|y| *y >= y0 - EPS);
        for list in [&mut xs, &mut ys] {
            list.sort_by(f32::total_cmp);
            list.dedup_by(|a, b| (*a - *b).abs() < EPS);
        }
        let inside = |x: f32, y: f32| (!fits_x || x + size.w + gap <= view.right() + EPS) && (!fits_y || y + size.h + gap <= view.bottom() + EPS);
        let free = |x: f32, y: f32| {
            let wanted = Rect::new(x - gap + EPS, y - gap + EPS, size.w + 2.0 * gap - 2.0 * EPS, size.h + 2.0 * gap - 2.0 * EPS);
            relevant.iter().all(|t| !t.intersects(&wanted))
        };
        for &y in &ys {
            for &x in &xs {
                if inside(x, y) && free(x, y) {
                    return Spot::InView(Pt::new(x, y));
                }
            }
        }
    }
    let bottom = taken.iter().map(Rect::bottom).fold(f32::NEG_INFINITY, f32::max);
    Spot::Below(Pt::new(x0, if bottom.is_finite() { (bottom + gap).max(y0) } else { y0 }))
}

#[cfg(test)]
#[path = "whiteboard_place_test.rs"]
mod whiteboard_place_test;
