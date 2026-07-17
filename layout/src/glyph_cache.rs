//! Glyph path and cell cache for CPU rendering.
//!
//! Two-level cache:
//! 1. **Path cache**: `PathStorage` objects keyed by (font, glyph, ppem).
//!    Avoids redundant path construction from font outlines.
//! 2. **Cell cache**: Rasterizer cells keyed by (font, glyph, ppem, scale, sub-pixel).
//!    Avoids the expensive path→cells conversion on every frame.
//!    Cells are computed at position (0,0) and offset at render time.

use std::collections::HashMap;

use agg_rust::basics::{VertexD, VertexSource, PATH_CMD_STOP};
use agg_rust::path_storage::PathStorage;
use agg_rust::rasterizer_cells_aa::CellAa;

use crate::font::parsed::{build_glyph_path, OwnedGlyph, ParsedFont};

/// A `VertexSource` view over an already-built slice of path vertices.
///
/// Replaces the upstream-removed `RasterizerScanlineAa::add_path_vertices_transformed`:
/// wrap this in a `ConvTransform` and feed it to `add_path` to rasterize cached glyph
/// vertices under a transform WITHOUT cloning the (shared, immutable) `PathStorage`.
pub(crate) struct SliceVertexSource<'a> {
    verts: &'a [VertexD],
    pos: usize,
}

impl<'a> SliceVertexSource<'a> {
    pub(crate) fn new(verts: &'a [VertexD]) -> Self {
        Self { verts, pos: 0 }
    }
}

impl VertexSource for SliceVertexSource<'_> {
    fn rewind(&mut self, _path_id: u32) {
        self.pos = 0;
    }

    fn vertex(&mut self, x: &mut f64, y: &mut f64) -> u32 {
        match self.verts.get(self.pos) {
            Some(v) => {
                self.pos += 1;
                *x = v.x;
                *y = v.y;
                v.cmd
            }
            None => PATH_CMD_STOP,
        }
    }
}

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
        // pixel-grid-snapped (rounded placement) as hinting intends.
        let rescale_hinted = is_hinted && (hint_correction - 1.0).abs() > 1e-4;
        let grid_snapped = is_hinted && !rescale_hinted;

        // Sub-pixel HORIZONTAL positioning (default ON): even a grid-snapped
        // (hinted-at-integer-ppem) glyph places its ORIGIN at a 1/4-pixel X
        // bucket, so advances accumulate smoothly and the run lands where
        // CoreText (fractional-x) puts it, instead of each origin rounding to a
        // whole pixel. The grid-fitted OUTLINE is unchanged — only where we drop
        // it horizontally shifts — so vertical stems stay crisp. The Y baseline
        // stays grid-snapped (`subpx_y == 0` for grid_snapped). With
        // `AZ_TEXT_SUBPIXEL=0` the grid_snapped case reverts to integer X
        // (sub-pixel 0, rounded origin), the previous behaviour.
        let subpx_x_snap = grid_snapped && !text_subpixel_enabled();
        let subpx_x = if subpx_x_snap { 0 } else { quantize_subpx(glyph_x) };
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

        // Integer pixel offset — the cells are at sub-pixel origin, offset by int
        // part. `int_x + subpx_x*0.25` must reconstruct `glyph_x`, so the floor
        // pairs with the quantized fraction; only the integer-X-snap case rounds.
        let int_x = if subpx_x_snap { glyph_x.round() as i32 } else { glyph_x.floor() as i32 };
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

                // Feed the cached glyph vertices through the transform via ConvTransform
                // (upstream removed add_path_vertices_transformed).
                let mut src = agg_rust::conv_transform::ConvTransform::new(
                    SliceVertexSource::new(path.vertices()),
                    transform,
                );
                ras.add_path(&mut src, 0);
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
/// table. Mirrors the `FreeType` `TT_Get_HMetrics` lookup used to place phantom
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

    // Optional "light" hinting (CoreText / DirectWrite grayscale style): keep the
    // grid-fitted Y (baseline, x-height and horizontal stems snap crisply to the
    // pixel grid) but restore the UNHINTED fractional X, so vertical stems stay
    // sub-pixel-positioned and anti-alias to soft gray instead of snapping to a
    // hard full-black 1px column (Windows-style full grid-fit). This is what
    // converges our CPU text onto CoreText — full bytecode hinting over-snaps X,
    // rendering stems thinner + darker than CoreText's. On by default; set
    // AZ_HINT_LIGHT=0 to force Windows-style full grid-fit. The interpreter still
    // runs in full (its Y output + FLIP'd on-curve flags are used), so no hinting
    // correctness is lost — only the X axis is left sub-pixel for grayscale AA.
    if hint_light_enabled() {
        let light: Vec<(i32, i32)> = hinted
            .iter()
            .enumerate()
            .map(|(i, &(hx, hy))| (points_f26dot6.get(i).map_or(hx, |p| p.0), hy))
            .collect();
        return build_path_from_contours(&light, &hinted_on_curve, raw_contour_ends);
    }

    // Build path from hinted points using TrueType quadratic contour conventions
    build_path_from_contours(&hinted, &hinted_on_curve, raw_contour_ends)
}

/// Whether to apply CoreText-style "light" hinting (grid-fit Y only, fractional X).
/// ON by default (matches CoreText / modern browser grayscale rendering); set
/// `AZ_HINT_LIGHT=0` (or `false`) to force Windows-style full grid-fit. Read once.
fn hint_light_enabled() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("AZ_HINT_LIGHT")
            .map(|s| !(s == "0" || s.eq_ignore_ascii_case("false")))
            .unwrap_or(true)
    })
}

/// Whether to place each glyph ORIGIN at a sub-pixel HORIZONTAL position (a
/// 1/4-pixel X bucket) instead of snapping it to a whole pixel.
///
/// ON by default. This is the horizontal half of the light-hinting philosophy
/// (crisp vertical, soft/sub-pixel horizontal): the grid-fitted glyph *outline*
/// still snaps its stems and baseline to the pixel grid, but the pen advances
/// accumulate at fractional precision so a run of glyphs lands where CoreText
/// (which positions glyphs at fractional x) puts them, rather than each origin
/// rounding to an integer pixel and drifting the whole line. Set
/// `AZ_TEXT_SUBPIXEL=0` (or `false`) to force integer X placement. Read once.
pub(crate) fn text_subpixel_enabled() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("AZ_TEXT_SUBPIXEL")
            .map(|s| !(s == "0" || s.eq_ignore_ascii_case("false")))
            .unwrap_or(true)
    })
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

#[cfg(test)]
#[allow(
    // exact float comparisons are intentional: power-of-two / midpoint
    // arithmetic with exactly representable results
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_lossless
)]
mod autotest_generated {
    use std::{
        panic::{catch_unwind, AssertUnwindSafe},
        sync::Arc,
    };

    use agg_rust::basics::{
        PATH_CMD_CURVE3, PATH_CMD_END_POLY, PATH_CMD_LINE_TO, PATH_CMD_MOVE_TO, PATH_FLAGS_CLOSE,
    };
    use azul_core::resources::OwnedGlyphBoundingBox;

    use super::*;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    /// A real system/repo font, or `None` — font-dependent tests skip rather
    /// than guess. Source bytes are retained so `glyph_lsb` can read `hmtx`.
    fn test_font() -> Option<ParsedFont> {
        let candidates = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
            "C:/Windows/Fonts/arial.ttf",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../examples/assets/fonts/SourceSerifPro-Regular.ttf"
            ),
        ];
        for path in candidates {
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let arc = Arc::new(rust_fontconfig::FontBytes::Owned(Arc::from(
                bytes.as_slice(),
            )));
            if let Some(font) =
                ParsedFont::from_bytes(&bytes, 0, &mut Vec::new()).map(|f| f.with_source_bytes(arc))
            {
                return Some(font);
            }
        }
        None
    }

    /// A glyph with no outline at all (the "space character" shape the
    /// `get_or_build` doc mentions) — buildable without a font.
    fn empty_glyph() -> OwnedGlyph {
        OwnedGlyph {
            bounding_box: OwnedGlyphBoundingBox {
                max_x: 0,
                max_y: 0,
                min_x: 0,
                min_y: 0,
            },
            horz_advance: 0,
            outline: Vec::new(),
            phantom_points: None,
            raw_points: None,
            raw_on_curve: None,
            raw_contour_ends: None,
            instructions: None,
        }
    }

    /// `('A', decoded glyph)` from `font`, if the font has one with an outline.
    fn glyph_a(font: &ParsedFont) -> Option<(u16, Arc<OwnedGlyph>)> {
        let gid = font.lookup_glyph_index('A' as u32)?;
        let glyph = font.get_or_decode_glyph(gid)?;
        Some((gid, glyph))
    }

    // ---------------------------------------------------------------------
    // quantize_subpx — numeric
    // ---------------------------------------------------------------------

    #[test]
    fn quantize_subpx_zero_and_quarter_buckets() {
        assert_eq!(quantize_subpx(0.0), 0);
        assert_eq!(quantize_subpx(0.24), 0);
        assert_eq!(quantize_subpx(0.25), 1);
        assert_eq!(quantize_subpx(0.5), 2);
        assert_eq!(quantize_subpx(0.75), 3);
        // 0.999 * 4 == 3.996, clamped by `.min(3.0)` — must not reach bucket 4.
        assert_eq!(quantize_subpx(0.999), 3);
        // Only the fractional part matters: whole pixels drop out.
        assert_eq!(quantize_subpx(1.0), 0);
        assert_eq!(quantize_subpx(2.25), 1);
        assert_eq!(quantize_subpx(1024.5), 2);
    }

    #[test]
    fn quantize_subpx_negative_inputs_use_floor_not_truncation() {
        // frac - frac.floor() is always in [0, 1), so a negative x lands in the
        // bucket of its positive fractional remainder (-0.25 => 0.75 => 3).
        assert_eq!(quantize_subpx(-0.25), 3);
        assert_eq!(quantize_subpx(-0.5), 2);
        assert_eq!(quantize_subpx(-0.75), 1);
        assert_eq!(quantize_subpx(-1.0), 0);
        assert_eq!(quantize_subpx(-0.0), 0);
        assert_eq!(quantize_subpx(-7.25), 3);
    }

    #[test]
    fn quantize_subpx_nan_and_infinities_are_defined() {
        // NaN.floor() == NaN, NaN - NaN == NaN, and f32::min ignores NaN, so the
        // clamp yields 3.0. Defined and non-panicking, if arguably surprising:
        // a NaN position buckets as if it were 3/4 of a pixel.
        assert_eq!(quantize_subpx(f32::NAN), 3);
        assert_eq!(quantize_subpx(f32::INFINITY), 3);
        assert_eq!(quantize_subpx(f32::NEG_INFINITY), 3);
    }

    #[test]
    fn quantize_subpx_never_exceeds_three() {
        let extremes = [
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            f32::EPSILON,
            1e30,
            -1e30,
            16_777_216.0, // 2^24: f32 loses the fractional bit entirely
            -16_777_217.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];
        for v in extremes {
            assert!(
                quantize_subpx(v) <= 3,
                "quantize_subpx({v}) escaped the 0..=3 bucket range"
            );
        }
        for i in -2000..2000 {
            let v = f64_to_f32(f64::from(i) * 0.017);
            assert!(quantize_subpx(v) <= 3, "quantize_subpx({v}) > 3");
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn f64_to_f32(v: f64) -> f32 {
        v as f32
    }

    // ---------------------------------------------------------------------
    // f26_to_px — numeric
    // ---------------------------------------------------------------------

    #[test]
    fn f26_to_px_zero_and_exact_fractions() {
        assert_eq!(f26_to_px(0), 0.0);
        assert_eq!(f26_to_px(64), 1.0);
        assert_eq!(f26_to_px(-64), -1.0);
        assert_eq!(f26_to_px(32), 0.5);
        assert_eq!(f26_to_px(16), 0.25);
        assert_eq!(f26_to_px(1), 1.0 / 64.0);
        assert_eq!(f26_to_px(-1), -1.0 / 64.0);
    }

    #[test]
    fn f26_to_px_min_max_stay_finite() {
        // i32::MAX rounds up to 2^31 in f32; both ends are exactly ±2^25 px.
        assert_eq!(f26_to_px(i32::MAX), 33_554_432.0);
        assert_eq!(f26_to_px(i32::MIN), -33_554_432.0);
        assert!(f26_to_px(i32::MAX).is_finite());
        assert!(f26_to_px(i32::MIN).is_finite());
    }

    #[test]
    fn f26_to_px_round_trips_for_exactly_representable_inputs() {
        // /64 is a power-of-two scaling: exact (no rounding) for |v| <= 2^24.
        for v in [
            0,
            1,
            -1,
            63,
            64,
            -64,
            4096,
            -4096,
            8_388_607,
            -8_388_607,
            16_777_216,
            -16_777_216,
        ] {
            let px = f26_to_px(v);
            assert_eq!(px * 64.0, v as f32, "f26_to_px({v}) did not round-trip");
        }
    }

    #[test]
    fn f26_to_px_is_monotonic() {
        let ladder = [
            i32::MIN,
            -1_000_000,
            -64,
            -1,
            0,
            1,
            64,
            1_000_000,
            i32::MAX,
        ];
        for w in ladder.windows(2) {
            assert!(
                f26_to_px(w[0]) <= f26_to_px(w[1]),
                "f26_to_px is not monotonic between {} and {}",
                w[0],
                w[1]
            );
        }
    }

    // ---------------------------------------------------------------------
    // build_path_from_contours — structure / round-trip of the TrueType rules
    // ---------------------------------------------------------------------

    #[test]
    fn build_path_from_contours_empty_inputs_return_none() {
        assert!(build_path_from_contours(&[], &[], &[]).is_none());
        // Points but no contours => nothing emitted.
        assert!(build_path_from_contours(&[(0, 0), (64, 0)], &[true, true], &[]).is_none());
        // Contour end but no points => out-of-range end, skipped.
        assert!(build_path_from_contours(&[], &[], &[0]).is_none());
    }

    #[test]
    fn build_path_from_contours_single_point_contour_is_skipped() {
        // n < 2 => degenerate contour, no path ops at all.
        assert!(build_path_from_contours(&[(0, 0)], &[true], &[0]).is_none());
    }

    #[test]
    fn build_path_from_contours_out_of_range_contour_end_is_skipped_not_indexed() {
        let pts = [(0, 0), (64, 64)];
        let oc = [true, true];
        assert!(build_path_from_contours(&pts, &oc, &[10]).is_none());
        // u16::MAX end: `end + 1` must not overflow the usize cursor either.
        assert!(build_path_from_contours(&pts, &oc, &[u16::MAX]).is_none());
        // Skipping a bogus end still advances the cursor to `end + 1`, so every
        // later contour falls into the `contour_start > end` branch too. The
        // whole glyph fails closed (None) instead of indexing out of bounds.
        assert!(
            build_path_from_contours(&pts, &oc, &[99, 1]).is_none(),
            "a bogus contour end poisons the cursor for the rest of the glyph"
        );
    }

    #[test]
    fn build_path_from_contours_line_contour_emits_move_line_close() {
        let path = build_path_from_contours(&[(0, 0), (128, 64)], &[true, true], &[1])
            .expect("two on-curve points form a contour");
        let v = path.vertices();
        assert_eq!(v.len(), 3, "expected move_to + line_to + close");
        assert_eq!(v[0].cmd, PATH_CMD_MOVE_TO);
        assert_eq!(v[0].x, 0.0);
        assert_eq!(v[0].y, 0.0);
        assert_eq!(v[1].cmd, PATH_CMD_LINE_TO);
        assert_eq!(v[1].x, 2.0, "128 F26Dot6 units == 2 px");
        assert_eq!(v[1].y, -1.0, "Y must be negated for screen coords");
        assert_eq!(v[2].cmd, PATH_CMD_END_POLY | PATH_FLAGS_CLOSE);
    }

    #[test]
    fn build_path_from_contours_offcurve_point_becomes_curve3() {
        let path = build_path_from_contours(
            &[(0, 0), (64, 64), (128, 0)],
            &[true, false, true],
            &[2],
        )
        .expect("on/off/on contour");
        let v = path.vertices();
        assert_eq!(v.len(), 4, "move_to + curve3(ctrl,to) + close");
        assert_eq!(v[0].cmd, PATH_CMD_MOVE_TO);
        assert_eq!(v[1].cmd, PATH_CMD_CURVE3);
        assert_eq!((v[1].x, v[1].y), (1.0, -1.0), "control point");
        assert_eq!(v[2].cmd, PATH_CMD_CURVE3);
        assert_eq!((v[2].x, v[2].y), (2.0, 0.0), "curve endpoint");
        assert_eq!(v[3].cmd, PATH_CMD_END_POLY | PATH_FLAGS_CLOSE);
    }

    #[test]
    fn build_path_from_contours_two_offcurve_points_insert_implicit_midpoint() {
        let path = build_path_from_contours(
            &[(0, 0), (64, 64), (128, 64), (192, 0)],
            &[true, false, false, true],
            &[3],
        )
        .expect("on/off/off/on contour");
        let v = path.vertices();
        assert_eq!(v.len(), 6, "move_to + 2 curve3 + close");
        // First quad ends at the implicit midpoint of the two control points:
        // mid((1,-1), (2,-1)) == (1.5, -1).
        assert_eq!((v[1].x, v[1].y), (1.0, -1.0));
        assert_eq!((v[2].x, v[2].y), (1.5, -1.0), "implicit on-curve midpoint");
        assert_eq!((v[3].x, v[3].y), (2.0, -1.0));
        assert_eq!((v[4].x, v[4].y), (3.0, 0.0));
    }

    #[test]
    fn build_path_from_contours_all_offcurve_closes_back_to_the_synthetic_origin() {
        // No on-curve point anywhere: origin is the midpoint of first & last,
        // and the final curve must return to it.
        let path = build_path_from_contours(
            &[(0, 0), (64, 64), (128, 0)],
            &[false, false, false],
            &[2],
        )
        .expect("all-off-curve contour");
        let v = path.vertices();
        assert_eq!(v.len(), 8, "move_to + 3 curve3 + close");
        assert_eq!(v[0].cmd, PATH_CMD_MOVE_TO);
        assert_eq!((v[0].x, v[0].y), (1.0, 0.0), "origin = mid(first, last)");
        let last_pt = v[6];
        assert_eq!(
            (last_pt.x, last_pt.y),
            (v[0].x, v[0].y),
            "final curve3 must land back on the origin"
        );
        assert_eq!(v[7].cmd, PATH_CMD_END_POLY | PATH_FLAGS_CLOSE);
    }

    #[test]
    fn build_path_from_contours_leading_offcurve_uses_trailing_oncurve_as_origin() {
        // flags[0] off, flags[n-1] on => origin is the LAST point, range [0, n-1).
        let path = build_path_from_contours(&[(64, 64), (128, 0)], &[false, true], &[1])
            .expect("off/on contour");
        let v = path.vertices();
        assert_eq!(v.len(), 4);
        assert_eq!(v[0].cmd, PATH_CMD_MOVE_TO);
        assert_eq!((v[0].x, v[0].y), (2.0, 0.0), "origin is the on-curve point");
        assert_eq!(v[1].cmd, PATH_CMD_CURVE3);
        assert_eq!((v[2].x, v[2].y), (2.0, 0.0), "curve returns to the origin");
    }

    #[test]
    fn build_path_from_contours_multiple_contours_emit_multiple_subpaths() {
        let pts = [(0, 0), (64, 0), (0, 64), (64, 64)];
        let oc = [true; 4];
        let path = build_path_from_contours(&pts, &oc, &[1, 3]).expect("two contours");
        let v = path.vertices();
        assert_eq!(v.len(), 6, "two × (move_to + line_to + close)");
        let moves = v.iter().filter(|x| x.cmd == PATH_CMD_MOVE_TO).count();
        assert_eq!(moves, 2, "each contour must start its own subpath");
    }

    #[test]
    fn build_path_from_contours_non_monotonic_contour_ends_do_not_panic() {
        let pts = [(0, 0), (64, 0), (0, 64), (64, 64)];
        let oc = [true; 4];
        // Descending ends: the second contour has contour_start > end => skipped.
        let path = build_path_from_contours(&pts, &oc, &[1, 0]).expect("first contour builds");
        assert_eq!(path.vertices().len(), 3);
        // Duplicate ends: likewise skipped, not re-emitted.
        let dup = build_path_from_contours(&pts, &oc, &[1, 1]).expect("first contour builds");
        assert_eq!(dup.vertices().len(), 3);
    }

    #[test]
    fn build_path_from_contours_extra_on_curve_flags_are_harmless() {
        // on_curve longer than points: the extra flags must simply go unread.
        let path = build_path_from_contours(&[(0, 0), (64, 0)], &[true; 8], &[1])
            .expect("contour builds with a too-long flag slice");
        assert_eq!(path.vertices().len(), 3);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn build_path_from_contours_short_on_curve_slice_panics() {
        // BUG (documented, not weakened): the bounds check only compares the
        // contour end against `points.len()`, then slices `on_curve` with the
        // same range. A caller passing a shorter `on_curve` than `points` gets
        // an index-out-of-bounds panic out of a `pub fn` that otherwise
        // signals failure by returning `None`.
        let _ = build_path_from_contours(&[(0, 0), (64, 0)], &[true], &[1]);
    }

    #[test]
    fn build_path_from_contours_extreme_coordinates_stay_finite() {
        let pts = [(i32::MIN, i32::MAX), (i32::MAX, i32::MIN), (0, 0)];
        let oc = [true, false, true];
        let path = build_path_from_contours(&pts, &oc, &[2]).expect("extreme contour still builds");
        for v in path.vertices() {
            assert!(
                v.x.is_finite() && v.y.is_finite(),
                "F26Dot6 extremes produced a non-finite vertex: ({}, {})",
                v.x,
                v.y
            );
        }
    }

    // ---------------------------------------------------------------------
    // env-backed predicates
    // ---------------------------------------------------------------------

    #[test]
    fn hint_light_enabled_is_stable_across_calls() {
        // OnceLock-backed: whatever the env says, every call must agree.
        let first = hint_light_enabled();
        assert_eq!(first, hint_light_enabled());
        assert_eq!(first, hint_light_enabled());
    }

    #[test]
    fn text_subpixel_enabled_is_stable_across_calls() {
        let first = text_subpixel_enabled();
        assert_eq!(first, text_subpixel_enabled());
        assert_eq!(first, text_subpixel_enabled());
    }

    // ---------------------------------------------------------------------
    // GlyphCache::new / paths_len / cells_len
    // ---------------------------------------------------------------------

    #[test]
    fn new_cache_is_empty_and_default_matches_new() {
        let cache = GlyphCache::new();
        assert_eq!(cache.paths_len(), 0);
        assert_eq!(cache.cells_len(), 0);

        let def = GlyphCache::default();
        assert_eq!(def.paths_len(), 0);
        assert_eq!(def.cells_len(), 0);
    }

    #[test]
    fn debug_impl_reports_entry_counts_without_touching_agg_types() {
        let cache = GlyphCache::new();
        let s = format!("{cache:?}");
        assert!(s.contains("GlyphCache"), "{s}");
        assert!(s.contains("paths"), "{s}");
        assert!(s.contains("cells"), "{s}");
    }

    // ---------------------------------------------------------------------
    // get_or_build_cells — numeric (needs no font: a path-cache miss is a
    // legitimate, reachable state)
    // ---------------------------------------------------------------------

    #[test]
    fn get_or_build_cells_without_a_cached_path_returns_none_and_negative_caches() {
        let mut cache = GlyphCache::new();
        let got = cache
            .get_or_build_cells(1, 2, 16, 0.0, 0.0, 1.0, false, 1.0)
            .map(|(cells, x, y)| (cells.len(), x, y));
        assert_eq!(got, None, "no path cached => no cells");
        assert_eq!(cache.cells_len(), 1, "the miss must be negative-cached");

        // Idempotent: a repeat lookup does not add a second entry.
        let again = cache
            .get_or_build_cells(1, 2, 16, 0.0, 0.0, 1.0, false, 1.0)
            .map(|(cells, _, _)| cells.len());
        assert_eq!(again, None);
        assert_eq!(cache.cells_len(), 1);
    }

    #[test]
    fn get_or_build_cells_keys_on_the_quarter_pixel_bucket_not_the_raw_position() {
        let mut cache = GlyphCache::new();
        // All of these have a fractional part < 0.25 => same sub-pixel bucket.
        for x in [0.0_f32, 0.1, 0.2, 5.24, 100.0] {
            let _ = cache.get_or_build_cells(7, 3, 16, x, 0.0, 1.0, false, 1.0);
        }
        assert_eq!(cache.cells_len(), 1, "same bucket must reuse one entry");

        // A different bucket (0.5 => bucket 2) is a different key.
        let _ = cache.get_or_build_cells(7, 3, 16, 0.5, 0.0, 1.0, false, 1.0);
        assert_eq!(cache.cells_len(), 2);

        // A different scale is a different key too (scale_fixed is in the key).
        let _ = cache.get_or_build_cells(7, 3, 16, 0.5, 0.0, 2.0, false, 1.0);
        assert_eq!(cache.cells_len(), 3);
    }

    #[test]
    fn get_or_build_cells_extreme_arguments_do_not_panic() {
        let mut cache = GlyphCache::new();
        // is_hinted + scale 0.0 keeps the fixed-point debug_assert satisfied
        // while pushing every *other* argument to its limit.
        for x in [
            0.0_f32,
            f32::MAX,
            f32::MIN,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.75,
        ] {
            let got = cache
                .get_or_build_cells(u64::MAX, u16::MAX, u16::MAX, x, x, 0.0, true, 1.0)
                .map(|(cells, _, _)| cells.len());
            assert_eq!(got, None, "empty path cache must yield None for x = {x}");
        }
        // Zero everywhere.
        let zeroed = cache
            .get_or_build_cells(0, 0, 0, 0.0, 0.0, 0.0, false, 0.0)
            .map(|(cells, _, _)| cells.len());
        assert_eq!(zeroed, None);
        // Largest in-range scale (the debug_assert's exclusive upper bound - 1).
        let big = cache
            .get_or_build_cells(0, 0, 0, 0.0, 0.0, 65_535.0, false, 1.0)
            .map(|(cells, _, _)| cells.len());
        assert_eq!(big, None);
    }

    #[test]
    fn get_or_build_cells_nan_hint_correction_falls_back_to_grid_snapped() {
        // (NaN - 1.0).abs() > 1e-4 is FALSE, so a NaN hint_correction is treated
        // exactly like the no-rescale case (scale_fixed = 0) rather than being
        // cast to a garbage fixed-point key. Same key => no extra entry.
        let mut cache = GlyphCache::new();
        let _ = cache.get_or_build_cells(9, 1, 12, 3.5, 4.5, 0.0, true, 1.0);
        assert_eq!(cache.cells_len(), 1);
        let _ = cache.get_or_build_cells(9, 1, 12, 3.5, 4.5, 0.0, true, f32::NAN);
        assert_eq!(
            cache.cells_len(),
            1,
            "NaN hint_correction must collapse onto the grid-snapped key"
        );
    }

    #[test]
    fn get_or_build_cells_negative_scale_is_debug_asserted() {
        let mut cache = GlyphCache::new();
        let res = catch_unwind(AssertUnwindSafe(|| {
            cache
                .get_or_build_cells(1, 1, 16, 0.0, 0.0, -1.0, false, 1.0)
                .map(|(cells, _, _)| cells.len())
        }));
        if cfg!(debug_assertions) {
            assert!(
                res.is_err(),
                "a negative scale must trip the fixed-point range debug_assert"
            );
        } else {
            // Release: the f32 -> u32 cast saturates to 0 instead of wrapping.
            assert_eq!(res.ok().flatten(), None);
        }
    }

    #[test]
    fn get_or_build_cells_evicts_wholesale_at_the_entry_limit() {
        let mut cache = GlyphCache::new();
        for i in 0..MAX_CELL_ENTRIES as u64 {
            let _ = cache.get_or_build_cells(i, 0, 16, 0.0, 0.0, 1.0, false, 1.0);
        }
        assert_eq!(cache.cells_len(), MAX_CELL_ENTRIES);
        // One past the limit: the whole map is cleared, then the new key lands.
        let _ = cache.get_or_build_cells(u64::MAX, 0, 16, 0.0, 0.0, 1.0, false, 1.0);
        assert_eq!(cache.cells_len(), 1, "cell cache must not grow unbounded");
    }

    // ---------------------------------------------------------------------
    // get_or_build — needs a ParsedFont (skipped when no font is available)
    // ---------------------------------------------------------------------

    #[test]
    fn get_or_build_outlineless_glyph_returns_none_and_caches_the_miss() {
        let Some(font) = test_font() else {
            return; // no font on this machine: skip rather than guess
        };
        let mut cache = GlyphCache::new();
        let glyph = empty_glyph();
        let got = cache
            .get_or_build(1, 0, &glyph, &font, 0)
            .map(|c| c.is_hinted);
        assert_eq!(got, None, "a glyph with no outline must yield None");
        assert_eq!(cache.paths_len(), 1, "the miss must be negative-cached");
    }

    #[test]
    fn get_or_build_extreme_ids_and_ppem_do_not_panic() {
        let Some(font) = test_font() else {
            return;
        };
        let mut cache = GlyphCache::new();
        let glyph = empty_glyph();
        for (hash, gid, ppem) in [
            (0_u64, 0_u16, 0_u16),
            (u64::MAX, u16::MAX, u16::MAX),
            (u64::MAX, 0, 1),
            (0, u16::MAX, u16::MAX),
        ] {
            let got = cache
                .get_or_build(hash, gid, &glyph, &font, ppem)
                .map(|c| c.is_hinted);
            assert_eq!(got, None, "outline-less glyph at ppem {ppem}");
        }
        assert_eq!(cache.paths_len(), 4, "each (hash, gid, ppem) is its own key");
    }

    #[test]
    fn get_or_build_is_idempotent_and_ppem_is_part_of_the_key() {
        let Some(font) = test_font() else {
            return;
        };
        let Some((gid, glyph)) = glyph_a(&font) else {
            return;
        };
        let mut cache = GlyphCache::new();

        // ppem == 0 => unhinted path, in font units.
        let first = cache
            .get_or_build(font.hash, gid, &glyph, &font, 0)
            .map(|c| (c.is_hinted, c.path.total_vertices()));
        let Some((is_hinted, verts)) = first else {
            return; // 'A' has no outline in this font: nothing to assert
        };
        assert!(!is_hinted, "ppem == 0 must not produce a hinted path");
        assert!(verts > 0, "an outlined glyph must emit vertices");
        assert_eq!(cache.paths_len(), 1);

        // Second lookup is a cache hit: same result, no new entry.
        let second = cache
            .get_or_build(font.hash, gid, &glyph, &font, 0)
            .map(|c| (c.is_hinted, c.path.total_vertices()));
        assert_eq!(second, Some((is_hinted, verts)));
        assert_eq!(cache.paths_len(), 1, "a hit must not insert a second entry");

        // A different ppem is a different key.
        let _ = cache.get_or_build(font.hash, gid, &glyph, &font, 16);
        assert_eq!(cache.paths_len(), 2);
    }

    #[test]
    fn get_or_build_evicts_wholesale_at_the_entry_limit() {
        let Some(font) = test_font() else {
            return;
        };
        let mut cache = GlyphCache::new();
        let glyph = empty_glyph(); // no outline => cheap negative entries
        for i in 0..MAX_PATH_ENTRIES as u64 {
            let _ = cache.get_or_build(i, 0, &glyph, &font, 0);
        }
        assert_eq!(cache.paths_len(), MAX_PATH_ENTRIES);
        let _ = cache.get_or_build(u64::MAX, 0, &glyph, &font, 0);
        assert_eq!(cache.paths_len(), 1, "path cache must not grow unbounded");
    }

    // ---------------------------------------------------------------------
    // glyph_lsb — numeric / bounds
    // ---------------------------------------------------------------------

    #[test]
    fn glyph_lsb_without_source_bytes_is_none() {
        let Some(font) = test_font() else {
            return;
        };
        let mut stripped = font;
        stripped.original_bytes = None;
        assert_eq!(
            glyph_lsb(&stripped, 0),
            None,
            "no retained font bytes => no hmtx to read"
        );
    }

    #[test]
    fn glyph_lsb_zero_h_metrics_is_none() {
        let Some(mut font) = test_font() else {
            return;
        };
        font.hhea_table.num_h_metrics = 0;
        assert_eq!(glyph_lsb(&font, 0), None, "num_h_metrics == 0 must bail out");
    }

    #[test]
    fn glyph_lsb_reads_gid_zero_and_rejects_out_of_range_gids() {
        let Some(font) = test_font() else {
            return;
        };
        let (_, len) = font.hmtx_range;
        let num = usize::from(font.hhea_table.num_h_metrics);
        if len > 0 && num > 0 && font.original_bytes.is_some() {
            assert!(
                glyph_lsb(&font, 0).is_some(),
                "gid 0 sits inside hmtx and must read back"
            );
        }

        // A gid whose lsb offset lands past the table must be bounds-rejected,
        // not indexed. (Mirrors the impl's offset arithmetic to know it is out
        // of range for THIS font rather than assuming a glyph count.)
        let gid = usize::from(u16::MAX);
        let lsb_off = if gid < num {
            gid * 4 + 2
        } else {
            num * 4 + (gid - num) * 2
        };
        if lsb_off + 2 > len {
            assert_eq!(
                glyph_lsb(&font, u16::MAX),
                None,
                "an out-of-table gid must return None, not panic"
            );
        }

        // Sweep the boundary around num_h_metrics (long metrics -> trailing
        // lsb-only array) — none of these may panic.
        for gid in [0_u16, 1, u16::MAX] {
            let _ = glyph_lsb(&font, gid);
        }
    }

    // ---------------------------------------------------------------------
    // build_hinted_path — guard clauses (the interpreter path itself is only
    // smoke-tested at a realistic ppem)
    // ---------------------------------------------------------------------

    #[test]
    fn build_hinted_path_without_raw_hinting_data_is_none() {
        let Some(font) = test_font() else {
            return;
        };
        let glyph = empty_glyph(); // raw_points / instructions all None
        assert!(build_hinted_path(0, &glyph, &font, 16).is_none());
        assert!(
            build_hinted_path(u16::MAX, &glyph, &font, u16::MAX).is_none(),
            "missing raw data must short-circuit before any hinting arithmetic"
        );
    }

    #[test]
    fn build_hinted_path_with_empty_contours_is_none() {
        let Some(font) = test_font() else {
            return;
        };
        let mut glyph = empty_glyph();
        glyph.raw_points = Some(Vec::new());
        glyph.raw_on_curve = Some(Vec::new());
        glyph.raw_contour_ends = Some(Vec::new());
        glyph.instructions = Some(Vec::new());
        assert!(
            build_hinted_path(0, &glyph, &font, 16).is_none(),
            "an empty point/contour list must bail out, not hint an empty glyph"
        );
    }

    #[test]
    fn build_hinted_path_without_a_hint_instance_is_none() {
        let Some(mut font) = test_font() else {
            return;
        };
        let Some((gid, glyph)) = glyph_a(&font) else {
            return;
        };
        if glyph.raw_points.is_none() || glyph.instructions.is_none() {
            return; // CFF / composite glyph: nothing to hint, test not applicable
        }
        font.hint_instance = None;
        assert!(
            build_hinted_path(gid, &glyph, &font, 16).is_none(),
            "no interpreter => no hinted path"
        );
    }

    #[test]
    fn build_hinted_path_with_zero_upem_is_none() {
        let Some(mut font) = test_font() else {
            return;
        };
        let Some((gid, glyph)) = glyph_a(&font) else {
            return;
        };
        if font.hint_instance.is_none() || glyph.raw_points.is_none() {
            return; // the upem guard sits behind the hint-instance guard
        }
        font.font_metrics.units_per_em = 0;
        assert!(
            build_hinted_path(gid, &glyph, &font, 16).is_none(),
            "upem == 0 must bail out before the divide-by-upem scale"
        );
    }

    #[test]
    fn build_hinted_path_at_a_realistic_ppem_produces_a_finite_path() {
        let Some(font) = test_font() else {
            return;
        };
        let Some((gid, glyph)) = glyph_a(&font) else {
            return;
        };
        let Some(path) = build_hinted_path(gid, &glyph, &font, 16) else {
            return; // unhinted font (CFF / no instructions): nothing to assert
        };
        assert!(path.total_vertices() > 0, "hinted path must have vertices");
        for v in path.vertices() {
            assert!(
                v.x.is_finite() && v.y.is_finite(),
                "hinting produced a non-finite vertex: ({}, {})",
                v.x,
                v.y
            );
        }
    }
}
