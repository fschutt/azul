//! Glyph path cache for CPU rendering.
//!
//! Caches built tiny-skia Path objects keyed by (font_hash, glyph_id, ppem) so that
//! repeated rendering of the same glyph avoids redundant path construction.
//! When ppem > 0 and the font has hinting data, the path is hinted (grid-fitted)
//! and in pixel coordinates. Otherwise the path is in font units.

use std::collections::HashMap;

use crate::font::parsed::{build_glyph_path, OwnedGlyph, ParsedFont};

/// Cache key for a glyph path.
/// ppem = 0 means unhinted (font-unit path), ppem > 0 means hinted at that size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphPathKey {
    pub font_hash: u64,
    pub glyph_id: u16,
    pub ppem: u16,
}

/// Result of a cache lookup: the path plus whether it's hinted (pixel coords) or not.
pub struct CachedGlyph<'a> {
    pub path: &'a tiny_skia::Path,
    pub is_hinted: bool,
}

/// Cache of built glyph paths.
pub struct GlyphCache {
    paths: HashMap<GlyphPathKey, Option<(tiny_skia::Path, bool)>>,
}

impl GlyphCache {
    pub fn new() -> Self {
        Self {
            paths: HashMap::new(),
        }
    }

    /// Get a cached path, or build it on cache miss.
    /// Returns `None` if the glyph has no outline (e.g. space character).
    ///
    /// When `ppem > 0` and the font has hinting data for this glyph,
    /// the returned path is hinted and in pixel coordinates (1 unit = 1 pixel).
    /// Otherwise, the path is in font units (unhinted).
    pub fn get_or_build(
        &mut self,
        font_hash: u64,
        glyph_id: u16,
        glyph_data: &OwnedGlyph,
        parsed_font: &ParsedFont,
        ppem: u16,
    ) -> Option<CachedGlyph<'_>> {
        let key = GlyphPathKey { font_hash, glyph_id, ppem };
        let entry = self
            .paths
            .entry(key)
            .or_insert_with(|| {
                // Try hinted path first if ppem > 0
                if ppem > 0 {
                    if let Some(path) = build_hinted_path(glyph_data, parsed_font, ppem) {
                        return Some((path, true));
                    }
                }
                // Fall back to unhinted path
                build_glyph_path(glyph_data).map(|p| (p, false))
            });
        entry.as_ref().map(|(path, is_hinted)| CachedGlyph {
            path,
            is_hinted: *is_hinted,
        })
    }

    /// Evict all cached paths.
    pub fn clear(&mut self) {
        self.paths.clear();
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.paths.len()
    }
}

/// Build a hinted glyph path using TrueType bytecode hinting.
///
/// The returned path is in pixel coordinates (1 unit = 1 pixel at the given ppem).
/// Returns `None` if the glyph has no raw hinting data or hinting fails.
fn build_hinted_path(
    glyph: &OwnedGlyph,
    parsed_font: &ParsedFont,
    ppem: u16,
) -> Option<tiny_skia::Path> {
    let raw_points = glyph.raw_points.as_ref()?;
    let raw_on_curve = glyph.raw_on_curve.as_ref()?;
    let raw_contour_ends = glyph.raw_contour_ends.as_ref()?;
    let instructions = glyph.instructions.as_ref()?;

    if raw_points.is_empty() || raw_contour_ends.is_empty() {
        return None;
    }

    let hint_mutex = parsed_font.hint_instance.as_ref()?;
    let mut hint = hint_mutex.lock().ok()?;

    let upem = parsed_font.font_metrics.units_per_em;
    if upem == 0 {
        return None;
    }

    // Set up hinting for this ppem (scales CVT, runs prep)
    if hint.set_ppem(ppem, ppem as f64).is_err() {
        return None;
    }

    // Scale raw points from font units to F26Dot6
    let scale = allsorts::hinting::f26dot6::compute_scale(ppem, upem);
    use allsorts::hinting::f26dot6::F26Dot6;

    let points_f26dot6: Vec<(i32, i32)> = raw_points
        .iter()
        .map(|&(x, y)| {
            let sx = F26Dot6::from_funits(x as i32, scale);
            let sy = F26Dot6::from_funits(y as i32, scale);
            (sx.to_bits(), sy.to_bits())
        })
        .collect();

    // Scale advance width to F26Dot6 for phantom points
    let adv_f26dot6 = F26Dot6::from_funits(glyph.horz_advance as i32, scale).to_bits();

    // Run hinting with unscaled orus for precise IUP interpolation
    let hinted = match hint.hint_glyph_with_orus(
        &points_f26dot6,
        Some(raw_points.as_slice()),
        raw_on_curve,
        raw_contour_ends,
        instructions,
        adv_f26dot6,
    ) {
        Ok(h) => h,
        Err(_) => return None,
    };

    // Build path from hinted points using TrueType quadratic contour conventions
    build_path_from_contours(&hinted, raw_on_curve, raw_contour_ends)
}

/// Build a tiny-skia Path from TrueType contour data (points in F26Dot6).
///
/// Uses the standard TrueType quadratic contour algorithm:
/// - On-curve points are endpoints of line/curve segments
/// - Off-curve points are quadratic Bézier control points
/// - Two consecutive off-curve points have an implicit on-curve midpoint
/// - Y is negated for screen coordinates (font Y-up → screen Y-down)
pub fn build_path_from_contours(
    points: &[(i32, i32)],
    on_curve: &[bool],
    contour_ends: &[u16],
) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    let mut has_ops = false;
    let mut contour_start = 0usize;

    for &end_idx in contour_ends {
        let end = end_idx as usize;
        if end >= points.len() || contour_start > end {
            contour_start = end + 1;
            continue;
        }

        let pts = &points[contour_start..=end];
        let flags = &on_curve[contour_start..=end];
        let n = pts.len();
        if n < 2 {
            contour_start = end + 1;
            continue;
        }

        // Helper: get point as (f32, f32) with Y negated
        let px = |i: usize| -> (f32, f32) {
            (f26_to_px(pts[i].0), -f26_to_px(pts[i].1))
        };
        let mid = |a: (f32, f32), b: (f32, f32)| -> (f32, f32) {
            ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
        };

        // Determine the starting on-curve point.
        // If first point is on-curve, start there.
        // If first is off-curve but last is on-curve, start at last.
        // If both are off-curve, start at their midpoint (virtual on-curve).
        let (start_pt, process_from) = if flags[0] {
            (px(0), 1) // start at point 0, process from 1
        } else if flags[n - 1] {
            (px(n - 1), 0) // start at last point, process from 0
        } else {
            (mid(px(0), px(n - 1)), 0) // virtual start, process from 0
        };

        pb.move_to(start_pt.0, start_pt.1);
        has_ops = true;

        // Walk through all points in the contour.
        // We track the "last on-curve" position and accumulate off-curve control points.
        let mut i = process_from;
        let mut pending_offcurve: Option<(f32, f32)> = None;

        // Process all n points (wrapping: after n-1 comes the closing back to start)
        let total = n; // we process exactly n points starting from process_from
        for _ in 0..total {
            let idx = i % n;
            let p = px(idx);

            if flags[idx] {
                // On-curve point
                if let Some(ctrl) = pending_offcurve.take() {
                    pb.quad_to(ctrl.0, ctrl.1, p.0, p.1);
                } else {
                    pb.line_to(p.0, p.1);
                }
            } else {
                // Off-curve point
                if let Some(ctrl) = pending_offcurve.take() {
                    // Two consecutive off-curve: implicit on-curve midpoint
                    let m = mid(ctrl, p);
                    pb.quad_to(ctrl.0, ctrl.1, m.0, m.1);
                }
                pending_offcurve = Some(p);
            }
            i += 1;
        }

        // Close the contour: handle any remaining off-curve → start_pt
        if let Some(ctrl) = pending_offcurve {
            pb.quad_to(ctrl.0, ctrl.1, start_pt.0, start_pt.1);
        }
        pb.close();

        contour_start = end + 1;
    }

    if !has_ops {
        return None;
    }
    pb.finish()
}

/// Convert F26Dot6 value to pixel coordinate (f32).
#[inline]
fn f26_to_px(v: i32) -> f32 {
    v as f32 / 64.0
}
