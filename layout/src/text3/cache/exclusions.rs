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

/// Computes horizontal line segments where a flattened `path()` shape (a set of
/// `MoveTo`/`LineTo`/`Close` subpaths) intersects a scanline at the given y range. Uses an
/// even-odd fill rule over the union of all subpaths so reversed rings (holes) carve out
/// space. Curves are assumed already flattened to `LineTo`s by `flatten_svg_to_path_segments`.
pub(super) fn path_segments_line_intersection(
    segments: &[PathSegment],
    y: f32,
    line_height: f32,
) -> Vec<(f32, f32)> {
    let line_center_y = y + line_height / 2.0;
    let mut crossings: Vec<f32> = Vec::new();

    // Walk the segments, reconstructing each subpath's vertices and intersecting its
    // (closing) edges with the scanline.
    let mut subpath: Vec<Point> = Vec::new();
    let flush = |subpath: &mut Vec<Point>, crossings: &mut Vec<f32>| {
        if subpath.len() >= 2 {
            for i in 0..subpath.len() {
                let p1 = subpath[i];
                let p2 = subpath[(i + 1) % subpath.len()];
                if (p2.y - p1.y).abs() < f32::EPSILON {
                    continue;
                }
                let crosses = (p1.y <= line_center_y && p2.y > line_center_y)
                    || (p1.y > line_center_y && p2.y <= line_center_y);
                if crosses {
                    let t = (line_center_y - p1.y) / (p2.y - p1.y);
                    crossings.push(t.mul_add(p2.x - p1.x, p1.x));
                }
            }
        }
        subpath.clear();
    };

    for seg in segments {
        match seg {
            PathSegment::MoveTo(p) => {
                flush(&mut subpath, &mut crossings);
                subpath.push(*p);
            }
            PathSegment::LineTo(p) => subpath.push(*p),
            PathSegment::Close => flush(&mut subpath, &mut crossings),
            // CurveTo/QuadTo/Arc should have been flattened to LineTo already; sample the
            // end point as a fallback so an unflattened path still produces a polygon.
            PathSegment::CurveTo { end, .. } | PathSegment::QuadTo { end, .. } => {
                subpath.push(*end);
            }
            PathSegment::Arc { center, .. } => subpath.push(*center),
        }
    }
    flush(&mut subpath, &mut crossings);

    crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mut spans = Vec::new();
    for chunk in crossings.chunks_exact(2) {
        if chunk[1] > chunk[0] {
            spans.push((chunk[0], chunk[1]));
        }
    }
    spans
}

/// Helper function to get the horizontal spans of any shape at a given y-coordinate.
/// Returns a list of (`start_x`, `end_x`) tuples.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma;
                                   // keep explicit a*b+c
pub(super) fn get_shape_horizontal_spans(shape: &ShapeBoundary, y: f32, line_height: f32) -> Vec<(f32, f32)> {
    match shape {
        ShapeBoundary::Rectangle(rect) => {
            // Check for any overlap between the line box [y, y + line_height]
            // and the rectangle's vertical span [rect.y, rect.y + rect.height].
            let line_start = y;
            let line_end = y + line_height;
            let rect_start = rect.y;
            let rect_end = rect.y + rect.height;

            if line_start < rect_end && line_end > rect_start {
                vec![(rect.x, rect.x + rect.width)]
            } else {
                vec![]
            }
        }
        ShapeBoundary::Circle { center, radius } => {
            let line_center_y = y + line_height / 2.0;
            let dy = (line_center_y - center.y).abs();
            if dy <= *radius {
                let dx = (radius.powi(2) - dy.powi(2)).sqrt();
                vec![(center.x - dx, center.x + dx)]
            } else {
                vec![]
            }
        }
        ShapeBoundary::Ellipse { center, radii } => {
            let line_center_y = y + line_height / 2.0;
            let dy = line_center_y - center.y;
            if dy.abs() <= radii.height {
                // Formula: (x-h)^2/a^2 + (y-k)^2/b^2 = 1
                let y_term = dy / radii.height;
                let x_term_squared = 1.0 - y_term.powi(2);
                if x_term_squared >= 0.0 {
                    let dx = radii.width * x_term_squared.sqrt();
                    vec![(center.x - dx, center.x + dx)]
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        }
        ShapeBoundary::Polygon { points } => {
            let segments = polygon_line_intersection(points, y, line_height);
            segments
                .iter()
                .map(|s| (s.start_x, s.start_x + s.width))
                .collect()
        }
        // Scanline intersection for `path()` shapes. `segments` is the flattened
        // (Close-terminated, curves pre-sampled) output of `flatten_svg_to_path_segments`;
        // intersect each subpath polygon with this scanline under an even-odd fill rule so
        // reversed rings (holes) carve out space.
        ShapeBoundary::Path { segments } => {
            path_segments_line_intersection(segments, y, line_height)
        }
    }
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

/// Computes horizontal line segments where a polygon intersects a scanline at the given y range.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma;
                                   // keep explicit a*b+c
pub(super) fn polygon_line_intersection(points: &[Point], y: f32, line_height: f32) -> Vec<LineSegment> {
    if points.len() < 3 {
        return vec![];
    }

    let line_center_y = y + line_height / 2.0;
    let mut intersections = Vec::new();

    // Use winding number algorithm for robustness with complex polygons.
    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];

        // Skip horizontal edges as they don't intersect a horizontal scanline in a meaningful way.
        if (p2.y - p1.y).abs() < f32::EPSILON {
            continue;
        }

        // Check if our horizontal scanline at `line_center_y` crosses this polygon edge.
        let crosses = (p1.y <= line_center_y && p2.y > line_center_y)
            || (p1.y > line_center_y && p2.y <= line_center_y);

        if crosses {
            // Calculate intersection x-coordinate using linear interpolation.
            let t = (line_center_y - p1.y) / (p2.y - p1.y);
            let x = p1.x + t * (p2.x - p1.x);
            intersections.push(x);
        }
    }

    // Sort intersections by x-coordinate to form spans.
    intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    // Build segments from paired intersection points.
    let mut segments = Vec::new();
    for chunk in intersections.chunks_exact(2) {
        let start_x = chunk[0];
        let end_x = chunk[1];
        if end_x > start_x {
            segments.push(LineSegment {
                start_x,
                width: end_x - start_x,
                priority: 0,
            });
        }
    }

    segments
}
