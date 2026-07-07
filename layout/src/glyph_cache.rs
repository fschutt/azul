//! Glyph path and cell cache for CPU rendering.
//!
//! Two-level cache:
//! 1. **Path cache**: `PathStorage` objects keyed by (font, glyph, ppem).
//!    Avoids redundant path construction from font outlines.
//! 2. **Cell cache**: Rasterizer cells keyed by (font, glyph, ppem, scale, sub-pixel).
//!    Avoids the expensive path→cells conversion on every frame.
//!    Cells are computed at position (0,0) and offset at render time.

use std::collections::HashMap;

use agg_rust::path_storage::PathStorage;
use agg_rust::rasterizer_cells_aa::CellAa;

use crate::font::parsed::{build_glyph_path, OwnedGlyph, ParsedFont};

/// Cache key for a glyph path.
/// ppem = 0 means unhinted (font-unit path), ppem > 0 means hinted at that size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphPathKey {
    font_hash: u64,
    glyph_id: u16,
    ppem: u16,
}

/// Cache key for pre-rasterized glyph cells.
/// Includes sub-pixel x/y fractional position quantized to 1/4 pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphCellKey {
    font_hash: u64,
    glyph_id: u16,
    ppem: u16,
    /// Scale factor encoded as fixed-point (scale * 65536) for unhinted glyphs.
    /// 0 for hinted glyphs (already in pixel coords).
    scale_fixed: u32,
    /// Sub-pixel x position quantized to 1/4 pixel (0..3).
    subpx_x: u8,
    /// Sub-pixel y position quantized to 1/4 pixel (0..3).
    subpx_y: u8,
}

/// Result of a cache lookup: the path plus whether it's hinted (pixel coords) or not.
pub struct CachedGlyph<'a> {
    pub path: &'a PathStorage,
    pub is_hinted: bool,
}

impl core::fmt::Debug for CachedGlyph<'_> {
    // `path` is agg_rust's PathStorage (not Debug); show the rest.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CachedGlyph")
            .field("is_hinted", &self.is_hinted)
            .finish_non_exhaustive()
    }
}

/// Pre-rasterized glyph cells at a canonical position.
/// Contains the rasterizer's cell output for a glyph at sub-pixel position (`subpx_x`, `subpx_y`).
/// To render at actual position (x, y), add integer pixel offset to each cell.
struct CachedCells {
    cells: Vec<CellAa>,
}

/// Maximum number of glyph path entries before eviction.
/// ~8K glyphs covers most Latin + CJK pages without unbounded growth.
const MAX_PATH_ENTRIES: usize = 8192;
/// Maximum number of cell cache entries before eviction.
/// Cell entries are larger than paths, so a lower limit is appropriate.
const MAX_CELL_ENTRIES: usize = 16384;

/// Cache of built glyph paths and pre-rasterized cells.
pub struct GlyphCache {
    paths: HashMap<GlyphPathKey, Option<(PathStorage, bool)>>,
    cells: HashMap<GlyphCellKey, Option<CachedCells>>,
}

impl core::fmt::Debug for GlyphCache {
    // Values hold agg_rust PathStorage / CellAa (not Debug); show entry counts.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GlyphCache")
            .field("paths", &self.paths.len())
            .field("cells", &self.cells.len())
            .finish_non_exhaustive()
    }
}

/// Quantize a fractional pixel position to 1/4 pixel (0..3).
#[inline]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
fn quantize_subpx(frac: f32) -> u8 {
    let f = frac - frac.floor();
    (f * 4.0).min(3.0) as u8
}

impl Default for GlyphCache {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            paths: HashMap::new(),
            cells: HashMap::new(),
        }
    }

    /// Entry count of the glyph-path cache (for leak probes).
    #[must_use] pub fn paths_len(&self) -> usize { self.paths.len() }

    /// Entry count of the pre-rasterized cell cache (for leak probes).
    #[must_use] pub fn cells_len(&self) -> usize { self.cells.len() }

    /// Get a cached path, or build it on cache miss.
    /// Returns `None` if the glyph has no outline (e.g. space character).
    pub fn get_or_build(
        &mut self,
        font_hash: u64,
        glyph_id: u16,
        glyph_data: &OwnedGlyph,
        parsed_font: &ParsedFont,
        ppem: u16,
    ) -> Option<CachedGlyph<'_>> {
        if self.paths.len() >= MAX_PATH_ENTRIES {
            self.paths.clear();
        }
        let key = GlyphPathKey { font_hash, glyph_id, ppem };
        let entry = self
            .paths
            .entry(key)
            .or_insert_with(|| {
                // Try hinted path first if ppem > 0
                if ppem > 0 {
                    if let Some(path) = build_hinted_path(glyph_id, glyph_data, parsed_font, ppem) {
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

    /// Get cached rasterizer cells for a glyph, or build them from the path.
    ///
    /// - `glyph_x`, `glyph_y`: final pixel position (used for sub-pixel quantization)
    /// - `scale`: font-unit→pixel scale (0.0 for hinted glyphs)
    /// - `is_hinted`: whether the path is in pixel coords (hinted) or font units
    /// - `hint_correction`: `effective_px / ppem` for hinted glyphs (1.0 otherwise).
    ///   A hinted outline is built at the *integer* ppem; when the requested
    ///   effective size (`font_size * dpi`) is fractional this rescales it back to
    ///   the true target size so hinted glyphs match their unhinted neighbours and
    ///   animate smoothly instead of snapping between integer ppems. When the
    ///   effective size is already integral this is 1.0 and the hinted glyph keeps
    ///   its pixel-grid-snapped placement.
    ///
    /// Returns the cached cells and the integer pixel offset to apply.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    pub fn get_or_build_cells(
        &mut self,
        font_hash: u64,
        glyph_id: u16,
        ppem: u16,
        glyph_x: f32,
        glyph_y: f32,
        scale: f32,
        is_hinted: bool,
        hint_correction: f32,
    ) -> Option<(&[CellAa], i32, i32)> {
        if self.cells.len() >= MAX_CELL_ENTRIES {
            self.cells.clear();
        }
        // Hinted outline built at integer ppem needs rescaling only when the
        // effective size is fractional (hint_correction != 1). Otherwise it stays
        // pixel-grid-snapped (sub-pixel 0, rounded placement) as hinting intends.
        let rescale_hinted = is_hinted && (hint_correction - 1.0).abs() > 1e-4;
        let grid_snapped = is_hinted && !rescale_hinted;

        let subpx_x = if grid_snapped { 0 } else { quantize_subpx(glyph_x) };
        let subpx_y = if grid_snapped { 0 } else { quantize_subpx(glyph_y) };
        debug_assert!((0.0..65536.0).contains(&scale), "scale out of range for fixed-point: {scale}");
        let scale_fixed = if is_hinted {
            if rescale_hinted { (hint_correction * 65536.0) as u32 } else { 0 }
        } else {
            (scale * 65536.0) as u32
        };

        let cell_key = GlyphCellKey {
            font_hash, glyph_id, ppem, scale_fixed, subpx_x, subpx_y,
        };

        // Integer pixel offset — the cells are at sub-pixel origin, offset by int part
        let int_x = if grid_snapped { glyph_x.round() as i32 } else { glyph_x.floor() as i32 };
        let int_y = if grid_snapped { glyph_y.round() as i32 } else { glyph_y.floor() as i32 };

        if !self.cells.contains_key(&cell_key) {
            // Build cells from cached path
            let path_key = GlyphPathKey { font_hash, glyph_id, ppem };
            let path_entry = self.paths.get(&path_key);
            let cached_cells = path_entry.and_then(|entry| {
                use agg_rust::trans_affine::TransAffine;
                use agg_rust::basics::FillingRule;
                use agg_rust::rasterizer_scanline_aa::RasterizerScanlineAa;
                let (path, _) = entry.as_ref()?;
                let frac_x = f64::from(subpx_x) * 0.25;
                let frac_y = f64::from(subpx_y) * 0.25;

                let mut ras = RasterizerScanlineAa::new();
                ras.filling_rule(FillingRule::NonZero);

                let transform = if is_hinted {
                    if rescale_hinted {
                        let mut t = TransAffine::new_scaling_uniform(f64::from(hint_correction));
                        t.multiply(&TransAffine::new_translation(frac_x, frac_y));
                        t
                    } else {
                        TransAffine::new_translation(frac_x, frac_y)
                    }
                } else {
                    let mut t = TransAffine::new_scaling_uniform(f64::from(scale));
                    t.multiply(&TransAffine::new_translation(frac_x, frac_y));
                    t
                };

                let verts = path.vertices();
                ras.add_path_vertices_transformed(verts, &transform);
                let cells = ras.outline_cells();
                if cells.is_empty() { None } else { Some(CachedCells { cells }) }
            });
            self.cells.insert(cell_key, cached_cells);
        }

        let entry = self.cells.get(&cell_key)?;
        entry.as_ref().map(|cc| (cc.cells.as_slice(), int_x, int_y))
    }
}

/// Build a hinted glyph path using TrueType bytecode hinting.
///
/// The returned path is in pixel coordinates (1 unit = 1 pixel at the given ppem).
/// Returns `None` if the glyph has no raw hinting data or hinting fails.
/// Read a glyph's left side bearing (font units) straight from the `hmtx`
/// table. Mirrors the FreeType `TT_Get_HMetrics` lookup used to place phantom
/// point pp1 at `xMin - lsb`. Returns `None` if hmtx is unavailable.
fn glyph_lsb(parsed_font: &ParsedFont, glyph_id: u16) -> Option<i16> {
    let (off, len) = parsed_font.hmtx_range;
    if len == 0 {
        return None;
    }
    let bytes = parsed_font.original_bytes.as_ref()?;
    let hmtx = bytes.as_ref().get(off..off + len)?;
    let num = usize::from(parsed_font.hhea_table.num_h_metrics);
    if num == 0 {
        return None;
    }
    let gid = usize::from(glyph_id);
    // longHorMetric[i] = { advanceWidth: u16, lsb: i16 } (4 bytes) for i < num;
    // trailing leftSideBearing: i16 array for the remaining glyphs.
    let lsb_off = if gid < num {
        gid * 4 + 2
    } else {
        num * 4 + (gid - num) * 2
    };
    let b = hmtx.get(lsb_off..lsb_off + 2)?;
    Some(i16::from_be_bytes([b[0], b[1]]))
}

fn build_hinted_path(
    glyph_id: u16,
    glyph: &OwnedGlyph,
    parsed_font: &ParsedFont,
    ppem: u16,
) -> Option<PathStorage> {
    use allsorts::hinting::f26dot6::F26Dot6;
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
    if hint.set_ppem(ppem, f64::from(ppem)).is_err() {
        return None;
    }

    // Scale raw points from font units to F26Dot6
    let scale = allsorts::hinting::f26dot6::compute_scale(ppem, upem);

    let points_f26dot6: Vec<(i32, i32)> = raw_points
        .iter()
        .map(|&(x, y)| {
            let sx = F26Dot6::from_funits(i32::from(x), scale);
            let sy = F26Dot6::from_funits(i32::from(y), scale);
            (sx.to_bits(), sy.to_bits())
        })
        .collect();

    // Scale advance width to F26Dot6 for phantom points
    let adv_f26dot6 = F26Dot6::from_funits(i32::from(glyph.horz_advance), scale).to_bits();

    // Phantom point pp1.x = (xMin - lsb) scaled (FreeType tt_loader_set_pp).
    // Threading the real lsb makes left-side-bearing grid-fitting match FreeType
    // for fonts where lsb != xMin; when lsb is unavailable, fall back to xMin
    // (i.e. lsb == xMin => pp1.x = 0, the previous hardcoded behaviour).
    let x_min = i32::from(glyph.bounding_box.min_x);
    let lsb = glyph_lsb(parsed_font, glyph_id).map_or(x_min, i32::from);
    let pp1_x_f26dot6 = F26Dot6::from_funits(x_min - lsb, scale).to_bits();

    // Run hinting and capture the POST-hinting on-curve flags. FLIPPT/FLIPRGON/
    // FLIPRGOFF can flip a point between on-curve and off-curve during the glyph
    // program; the contour builder must use the updated flags, not the original
    // raw_on_curve, or it treats a flipped control point as a line endpoint (and
    // vice versa), kinking the outline.
    let Ok((hinted, hinted_on_curve)) = hint.hint_glyph_with_flags_pp1(
        &points_f26dot6,
        raw_on_curve,
        raw_contour_ends,
        instructions,
        adv_f26dot6,
        pp1_x_f26dot6,
    ) else {
        return None;
    };
    drop(hint);

    // Build path from hinted points using TrueType quadratic contour conventions
    build_path_from_contours(&hinted, &hinted_on_curve, raw_contour_ends)
}

/// Build an agg `PathStorage` from TrueType contour data (points in `F26Dot6`).
///
/// Matches allsorts' `visit_simple_glyph_outline` algorithm exactly:
/// - On-curve points are endpoints of line/curve segments
/// - Off-curve points are quadratic Bézier control points
/// - Two consecutive off-curve points have an implicit on-curve midpoint
/// - Y is negated for screen coordinates (font Y-up → screen Y-down)
/// - The origin point is NOT revisited in the loop; `close()` handles the final segment
#[must_use] pub fn build_path_from_contours(
    points: &[(i32, i32)],
    on_curve: &[bool],
    contour_ends: &[u16],
) -> Option<PathStorage> {
    use agg_rust::basics::PATH_FLAGS_NONE;

    let mut path = PathStorage::new();
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

        // Helper: get point as (f64, f64) with Y negated
        let px = |i: usize| -> (f64, f64) {
            (f64::from(f26_to_px(pts[i].0)), f64::from(-f26_to_px(pts[i].1)))
        };
        let mid = |a: (f64, f64), b: (f64, f64)| -> (f64, f64) {
            ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
        };

        // Determine origin and processing range (matching allsorts' calculate_origin)
        let (origin, start, until) = if flags[0] {
            (px(0), 1usize, n)
        } else if flags[n - 1] {
            (px(n - 1), 0usize, n - 1)
        } else {
            (mid(px(0), px(n - 1)), 0usize, n)
        };

        path.move_to(origin.0, origin.1);
        has_ops = true;

        let mut i = start;
        while i < until {
            if flags[i] {
                // On-curve: line segment
                let to = px(i);
                path.line_to(to.0, to.1);
                i += 1;
            } else {
                // Off-curve control point
                let ctrl = px(i);
                let next = i + 1;
                if next < until {
                    if flags[next] {
                        // Next is on-curve: quad to it, consume both
                        let to = px(next);
                        path.curve3(ctrl.0, ctrl.1, to.0, to.1);
                        i = next + 1;
                    } else {
                        // Next is also off-curve: quad to implicit midpoint
                        let m = mid(ctrl, px(next));
                        path.curve3(ctrl.0, ctrl.1, m.0, m.1);
                        i = next;
                    }
                } else {
                    // End of range: curve back to origin
                    path.curve3(ctrl.0, ctrl.1, origin.0, origin.1);
                    i = next;
                }
            }
        }
        path.close_polygon(PATH_FLAGS_NONE);

        contour_start = end + 1;
    }

    if !has_ops {
        return None;
    }
    Some(path)
}

/// Convert `F26Dot6` value to pixel coordinate (f32).
#[inline]
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
fn f26_to_px(v: i32) -> f32 {
    v as f32 / 64.0
}
