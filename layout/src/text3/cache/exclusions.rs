//! Shape exclusions: where a line crosses a path or polygon, and the spans left for text.

use std::{
    cmp::Ordering,
    collections::{
        hash_map::{DefaultHasher, HashMap},
        BTreeSet, HashSet,
    },
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::ImageRef,
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::{
    corety::LayoutDebugMessage,
    props::{basic::ColorU, style::StyleBackgroundContent},
};
#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Language as HyphenationLanguage, Load, Standard};
use rust_fontconfig::{
    FcFontCache, FcPattern, FcStretch, FcWeight, FontId, PatternMatch, UnicodeRange,
};
use smallvec::{smallvec, SmallVec};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;
use crate::text3::script::{script_to_language, Language, Script};
#[allow(clippy::wildcard_imports)]
// the text layout cache's items, re-exported from the sibling modules by mod.rs
use super::*;

/// Flattens a parsed SVG multipolygon (from a CSS `path()` shape) into a flat list of
/// `PathSegment`s in absolute coordinates (offset by the reference box origin). Each ring
/// becomes a `MoveTo` + a run of `LineTo`s + `Close`; curve elements are sampled into line
/// segments (~one segment per 4px of arc length, capped) so the scanline intersection can
/// treat each subpath as a polygon.
// bounded curve-sampling geometry casts (step count / arc-length parameter / coords)
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub(super) fn flatten_svg_to_path_segments(
    multipolygon: &azul_core::svg::SvgMultiPolygon,
    reference_box: Rect,
) -> Vec<PathSegment> {
    use azul_core::svg::SvgPathElement;

    let mut out: Vec<PathSegment> = Vec::new();

    for ring in multipolygon.rings.as_ref() {
        let elements = ring.items.as_ref();
        if elements.is_empty() {
            continue;
        }
        let start = elements[0].get_start();
        out.push(PathSegment::MoveTo(Point {
            x: reference_box.x + start.x,
            y: reference_box.y + start.y,
        }));
        for el in elements {
            match el {
                SvgPathElement::Line(l) => {
                    out.push(PathSegment::LineTo(Point {
                        x: reference_box.x + l.end.x,
                        y: reference_box.y + l.end.y,
                    }));
                }
                curve => {
                    // Sample the curve by arc length into line segments.
                    let len = curve.get_length();
                    let steps = ((len / 4.0).ceil() as usize).clamp(1, 64);
                    for i in 1..=steps {
                        let offset = len * (i as f64) / (steps as f64);
                        let t = curve.get_t_at_offset(offset);
                        out.push(PathSegment::LineTo(Point {
                            x: reference_box.x + curve.get_x_at_t(t) as f32,
                            y: reference_box.y + curve.get_y_at_t(t) as f32,
                        }));
                    }
                }
            }
        }
        out.push(PathSegment::Close);
    }

    out
}

/// How a line band meets a shape: the room a line has inside it, or the room
/// the shape takes away from a line.
///
/// A line box is as tall as its band, so a shape is measured over the whole
/// band, never at one height of it: inside a slanted or round shape the line
/// has only the room the shape gives at every height of the band, and beside
/// one it loses whatever the shape reaches at any height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandCover {
    /// Where the shape covers the band from its top edge to its bottom edge:
    /// the room inside a `shape-inside`.
    Whole,
    /// Where the shape reaches into the band at any height: the room an
    /// exclusion takes.
    Any,
}

/// How far inside a band a polygon is sampled next to an edge or a vertex,
/// so the sample sees the edges on that side of it and not those meeting
/// there.
const BAND_SAMPLE_INSET: f32 = 1.0e-3;

/// The horizontal spans of `shape` in the line band from `top` down `height`,
/// as `cover` reads them, left to right and disjoint.
#[must_use]
pub fn shape_spans_in_band(
    shape: &ShapeBoundary,
    top: f32,
    height: f32,
    cover: BandCover,
) -> Vec<(f32, f32)> {
    let bottom = top + height.max(0.0);
    if !top.is_finite() || !bottom.is_finite() {
        return Vec::new();
    }
    match shape {
        ShapeBoundary::Rectangle(rect) => {
            let (rect_top, rect_bottom) = (rect.y, rect.y + rect.height);
            let meets = match cover {
                BandCover::Any => top < rect_bottom && bottom > rect_top,
                BandCover::Whole => {
                    rect.height > 0.0
                        && top >= rect_top - BAND_SAMPLE_INSET
                        && bottom <= rect_bottom + BAND_SAMPLE_INSET
                }
            };
            if meets && rect.width >= 0.0 {
                vec![(rect.x, rect.x + rect.width)]
            } else {
                Vec::new()
            }
        }
        ShapeBoundary::Circle { center, radius } => conic_spans(
            *center,
            |dy| {
                let squared = radius.mul_add(*radius, -(dy * dy));
                (dy.abs() <= *radius && squared >= 0.0).then(|| squared.sqrt())
            },
            top,
            bottom,
            cover,
        ),
        ShapeBoundary::Ellipse { center, radii } => conic_spans(
            *center,
            |dy| {
                if dy.abs() > radii.height {
                    return None;
                }
                let along = dy / radii.height;
                let squared = along.mul_add(-along, 1.0);
                (squared >= 0.0).then(|| radii.width * squared.sqrt())
            },
            top,
            bottom,
            cover,
        ),
        ShapeBoundary::Polygon { points, fill_rule } => ring_spans_in_band(
            core::slice::from_ref(points),
            *fill_rule,
            top,
            bottom,
            cover,
        ),
        ShapeBoundary::Path {
            segments,
            fill_rule,
        } => ring_spans_in_band(&rings_of_path(segments), *fill_rule, top, bottom, cover),
    }
}

/// The spans an exclusion takes from the line band from `y` down
/// `line_height`: wherever it reaches into the band.
#[must_use]
pub fn get_shape_horizontal_spans(
    shape: &ShapeBoundary,
    y: f32,
    line_height: f32,
) -> Vec<(f32, f32)> {
    shape_spans_in_band(shape, y, line_height, BandCover::Any)
}

/// The span of a circle or an ellipse around `center` in a band, where
/// `half_width` is its half width at a vertical distance from the centre
/// (`None` above or below it).
///
/// Both are convex: the widest span any part of the band meets is at the
/// band's height closest to the centre, the span the whole band keeps at the
/// band's edge farthest from it.
fn conic_spans(
    center: Point,
    half_width: impl Fn(f32) -> Option<f32>,
    top: f32,
    bottom: f32,
    cover: BandCover,
) -> Vec<(f32, f32)> {
    let half = match cover {
        BandCover::Any => half_width(center.y.clamp(top, bottom) - center.y),
        BandCover::Whole => half_width(top - center.y)
            .zip(half_width(bottom - center.y))
            .map(|(at_top, at_bottom)| at_top.min(at_bottom)),
    };
    half.filter(|h| h.is_finite())
        .map(|h| vec![(center.x - h, center.x + h)])
        .unwrap_or_default()
}

/// The closed rings of a flattened `path()`: one per subpath, its vertices in
/// order. Curves were sampled into line segments when the path was made
/// (`flatten_svg_to_path_segments`); a curve still left counts by its end
/// point.
fn rings_of_path(segments: &[PathSegment]) -> Vec<Vec<Point>> {
    let mut rings = Vec::new();
    let mut ring: Vec<Point> = Vec::new();
    for segment in segments {
        match segment {
            PathSegment::MoveTo(p) => {
                if !ring.is_empty() {
                    rings.push(core::mem::take(&mut ring));
                }
                ring.push(*p);
            }
            PathSegment::LineTo(p)
            | PathSegment::CurveTo { end: p, .. }
            | PathSegment::QuadTo { end: p, .. } => ring.push(*p),
            PathSegment::Arc { center, .. } => ring.push(*center),
            PathSegment::Close => {
                if !ring.is_empty() {
                    rings.push(core::mem::take(&mut ring));
                }
            }
        }
    }
    if !ring.is_empty() {
        rings.push(ring);
    }
    rings
}

/// The spans of the shape the `rings` fill under `fill_rule` in a band.
///
/// Between two vertex heights every edge is straight, so each span's ends
/// move linearly with the height and are widest and narrowest at the
/// heights where the band or an edge ends. The shape is read just inside
/// the band's top and bottom edges and on both sides of every vertex within
/// the band, and the band keeps what all of those readings cover
/// ([`BandCover::Whole`]) or loses what any of them does ([`BandCover::Any`]).
fn ring_spans_in_band(
    rings: &[Vec<Point>],
    fill_rule: FillRule,
    top: f32,
    bottom: f32,
    cover: BandCover,
) -> Vec<(f32, f32)> {
    let inset = BAND_SAMPLE_INSET.min((bottom - top) / 4.0);
    let mut heights = Vec::new();
    if inset <= 0.0 {
        heights.push(top);
    } else {
        heights.push(top + inset);
        heights.push(bottom - inset);
        for vertex in rings.iter().flatten() {
            if vertex.y > top && vertex.y < bottom {
                heights.push((vertex.y - inset).max(top + inset));
                heights.push((vertex.y + inset).min(bottom - inset));
            }
        }
    }
    let mut readings = heights
        .iter()
        .map(|&y| scanline_spans(rings, y, fill_rule));
    let Some(first) = readings.next() else {
        return Vec::new();
    };
    readings.fold(first, |acc, spans| match cover {
        BandCover::Whole => intersect_spans(&acc, &spans),
        BandCover::Any => union_spans(acc, spans),
    })
}

/// How narrow a span may be and still be read as part of a shape: a narrower
/// one is the rounding of two edges that meet, not an area.
const MIN_SPAN_WIDTH: f32 = 1.0e-3;

/// Where a horizontal line at `y` is inside the shape the `rings` fill
/// under `fill_rule`, left to right.
///
/// An edge counts from its upper end up to (and not including) its lower
/// one, so a line through a vertex crosses the two edges meeting there once
/// between them. A ring of fewer than three points encloses nothing.
fn scanline_spans(rings: &[Vec<Point>], y: f32, fill_rule: FillRule) -> Vec<(f32, f32)> {
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for ring in rings.iter().filter(|ring| ring.len() >= 3) {
        for (index, &from) in ring.iter().enumerate() {
            let to = ring[(index + 1) % ring.len()];
            let downward = from.y <= y && y < to.y;
            let upward = to.y <= y && y < from.y;
            if downward || upward {
                let along = (y - from.y) / (to.y - from.y);
                let x = along.mul_add(to.x - from.x, from.x);
                crossings.push((x, if downward { 1 } else { -1 }));
            }
        }
    }
    crossings.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    let mut spans = Vec::new();
    match fill_rule {
        FillRule::EvenOdd => {
            for pair in crossings.chunks_exact(2) {
                if pair[1].0 - pair[0].0 > MIN_SPAN_WIDTH {
                    spans.push((pair[0].0, pair[1].0));
                }
            }
        }
        FillRule::NonZero => {
            let mut winding = 0;
            let mut start = 0.0;
            for (x, direction) in crossings {
                let was_outside = winding == 0;
                winding += direction;
                if was_outside && winding != 0 {
                    start = x;
                } else if !was_outside && winding == 0 && x - start > MIN_SPAN_WIDTH {
                    spans.push((start, x));
                }
            }
        }
    }
    spans
}

/// The parts both span lists cover, left to right.
#[must_use]
pub fn intersect_spans(a: &[(f32, f32)], b: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let start = a[i].0.max(b[j].0);
        let end = a[i].1.min(b[j].1);
        if end > start {
            out.push((start, end));
        }
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// What either span list covers, left to right, overlapping spans merged.
#[must_use]
pub fn union_spans(mut a: Vec<(f32, f32)>, b: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    a.extend(b);
    a.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(Ordering::Equal));
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(a.len());
    for (start, end) in a {
        match out.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
}

/// Merges overlapping or adjacent line segments into larger ones.
pub(super) fn merge_segments(mut segments: Vec<LineSegment>) -> Vec<LineSegment> {
    if segments.len() <= 1 {
        return segments;
    }
    segments.sort_by(|a, b| a.start_x.partial_cmp(&b.start_x).unwrap_or(Ordering::Equal));
    let mut merged = vec![segments[0]];
    for next_seg in segments.iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if next_seg.start_x <= last.start_x + last.width {
            let new_width = (next_seg.start_x + next_seg.width) - last.start_x;
            last.width = last.width.max(new_width);
        } else {
            merged.push(*next_seg);
        }
    }
    merged
}
