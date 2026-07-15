use super::*;

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use agg_rust::basics::{FillingRule, VertexSource};
use agg_rust::color::Rgba8;
use agg_rust::conv_transform::ConvTransform;
use agg_rust::gradient_lut::GradientLut;
use agg_rust::path_storage::PathStorage;
use agg_rust::pixfmt_rgba::PixfmtRgba32;
use agg_rust::rasterizer_scanline_aa::RasterizerScanlineAa;
use agg_rust::renderer_base::RendererBase;
use agg_rust::renderer_scanline::{render_scanlines_aa, render_scanlines_aa_solid};
use agg_rust::rendering_buffer::RowAccessor;
use agg_rust::scanline_u::ScanlineU8;
use agg_rust::span_allocator::SpanAllocator;
use agg_rust::span_gradient::{GradientFunction, SpanGradient};
use agg_rust::span_interpolator_linear::SpanInterpolatorLinear;
use agg_rust::trans_affine::TransAffine;

pub const IDENTITY_EPSILON_F64: f64 = 0.0001;

/// Compute the intersection of two logical rects.
#[must_use] pub fn rect_intersection(a: &LogicalRect, b: &LogicalRect) -> Option<LogicalRect> {
    let x1 = a.origin.x.max(b.origin.x);
    let y1 = a.origin.y.max(b.origin.y);
    let x2 = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let y2 = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    if x2 > x1 && y2 > y1 {
        Some(LogicalRect {
            origin: LogicalPosition { x: x1, y: y1 },
            size: LogicalSize {
                width: x2 - x1,
                height: y2 - y1,
            },
        })
    } else {
        None
    }
}

/// Blit `src` onto `dst` at pixel position (`px_x`, `px_y`) with opacity.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
pub fn blit_pixmap(src: &AzulPixmap, dst: &mut AzulPixmap, px_x: i32, px_y: i32, opacity: f32) {
    let sw = src.width as i32;
    let sh = src.height as i32;
    let dw = dst.width as i32;
    let dh = dst.height as i32;
    let op = (opacity * 255.0).clamp(0.0, 255.0) as u32;

    for sy in 0..sh {
        // saturating: px_y/px_x are caller-supplied device offsets that a large-but-legal
        // CSS transform can push to ~i32::MAX; a plain `+` overflows. A saturated result
        // fails the bounds check below and is skipped, which is the intended outcome.
        let dy = px_y.saturating_add(sy);
        if dy < 0 || dy >= dh {
            continue;
        }
        for sx in 0..sw {
            let dx = px_x.saturating_add(sx);
            if dx < 0 || dx >= dw {
                continue;
            }
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((dy * dw + dx) * 4) as usize;
            if si + 3 >= src.data.len() || di + 3 >= dst.data.len() {
                continue;
            }

            let sr = u32::from(src.data[si]);
            let sg = u32::from(src.data[si + 1]);
            let sb = u32::from(src.data[si + 2]);
            let sa = (u32::from(src.data[si + 3]) * op) / 255;

            if sa == 0 {
                continue;
            }
            if sa == 255 {
                dst.data[di] = sr as u8;
                dst.data[di + 1] = sg as u8;
                dst.data[di + 2] = sb as u8;
                dst.data[di + 3] = 255;
            } else {
                let inv_sa = 255 - sa;
                dst.data[di] = ((sr * sa + u32::from(dst.data[di]) * inv_sa) / 255) as u8;
                dst.data[di + 1] = ((sg * sa + u32::from(dst.data[di + 1]) * inv_sa) / 255) as u8;
                dst.data[di + 2] = ((sb * sa + u32::from(dst.data[di + 2]) * inv_sa) / 255) as u8;
                dst.data[di + 3] = ((sa + u32::from(dst.data[di + 3]) * inv_sa / 255).min(255)) as u8;
            }
        }
    }
}

/// Shift pixel data in a pixmap by (dx, dy) pixels, clearing exposed regions.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
pub fn shift_pixbuf(pixmap: &mut AzulPixmap, dx: i32, dy: i32) {
    use core::cmp::Ordering;
    let w = pixmap.width as i32;
    let h = pixmap.height as i32;
    // `i32::MIN.abs()` panics — MIN has no positive i32 counterpart. `unsigned_abs`
    // is total. `w`/`h` come from unsigned dimensions, so they are never negative.
    if dx.unsigned_abs() >= w.unsigned_abs() || dy.unsigned_abs() >= h.unsigned_abs() {
        // Entire buffer is exposed — just clear it
        pixmap.fill(0, 0, 0, 0);
        return;
    }

    let stride = (w * 4) as usize;
    let data = &mut pixmap.data;

    // Shift rows vertically
    match dy.cmp(&0) {
        Ordering::Greater => {
            // Shift down: copy from top to bottom
            for row in (0..h - dy).rev() {
                let src_start = (row * w * 4) as usize;
                let dst_start = ((row + dy) * w * 4) as usize;
                data.copy_within(src_start..src_start + stride, dst_start);
            }
            // Clear top rows
            for row in 0..dy {
                let start = (row * w * 4) as usize;
                data[start..start + stride].fill(0);
            }
        }
        Ordering::Less => {
            let ady = -dy;
            // Shift up: copy from bottom to top
            for row in ady..h {
                let src_start = (row * w * 4) as usize;
                let dst_start = ((row - ady) * w * 4) as usize;
                data.copy_within(src_start..src_start + stride, dst_start);
            }
            // Clear bottom rows
            for row in (h - ady)..h {
                let start = (row * w * 4) as usize;
                data[start..start + stride].fill(0);
            }
        }
        Ordering::Equal => {}
    }

    // Shift columns horizontally
    match dx.cmp(&0) {
        Ordering::Greater => {
            for row in 0..h {
                let row_start = (row * w * 4) as usize;
                let shift = (dx * 4) as usize;
                // Shift right within the row
                data.copy_within(row_start..row_start + stride - shift, row_start + shift);
                // Clear left columns
                data[row_start..row_start + shift].fill(0);
            }
        }
        Ordering::Less => {
            let adx = (-dx * 4) as usize;
            for row in 0..h {
                let row_start = (row * w * 4) as usize;
                data.copy_within(row_start + adx..row_start + stride, row_start);
                // Clear right columns
                data[row_start + stride - adx..row_start + stride].fill(0);
            }
        }
        Ordering::Equal => {}
    }
}

/// A simple RGBA pixel buffer. Replaces `tiny_skia::Pixmap`.
#[derive(Debug)]
pub struct AzulPixmap {
    pub(crate) data: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl AzulPixmap {
    /// Create a new pixmap filled with opaque white.
    #[must_use] pub fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        // checked: author-controllable dimensions (e.g. an SVG intrinsic size) can make
        // width*height*4 overflow usize — a debug panic, and a silently-undersized
        // buffer in release. Refuse absurd sizes instead.
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))?;
        let data = vec![255u8; len]; // opaque white
        Some(Self {
            data,
            width,
            height,
        })
    }

    /// Fill the entire pixmap with a single color.
    pub fn fill(&mut self, r: u8, g: u8, b: u8, a: u8) {
        for chunk in self.data.chunks_exact_mut(4) {
            chunk[0] = r;
            chunk[1] = g;
            chunk[2] = b;
            chunk[3] = a;
        }
    }

    /// Fill a rectangular region with a single color (pixel coordinates).
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
    #[allow(clippy::many_single_char_names)] // domain-standard coordinate/geometry/short-lived names
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8, a: u8) {
        let pw = self.width as i32;
        let ph = self.height as i32;
        let x0 = x.max(0).min(pw);
        let y0 = y.max(0).min(ph);
        // saturating: a non-finite/huge layout size casts to i32::MAX, and `x + w`
        // would then overflow (debug panic). Clamp instead.
        // `.clamp(x0, ..)` (not just `.max(0)`): a NEGATIVE w gives x1 < x0, and the
        // `data[start..end]` slice below panics on a reversed range. Force x1 >= x0.
        let x1 = x.saturating_add(w).clamp(x0, pw);
        let y1 = y.saturating_add(h).clamp(y0, ph);
        for row in y0..y1 {
            let start = (row * pw + x0) as usize * 4;
            let end = (row * pw + x1) as usize * 4;
            if end <= self.data.len() {
                for chunk in self.data[start..end].chunks_exact_mut(4) {
                    chunk[0] = r;
                    chunk[1] = g;
                    chunk[2] = b;
                    chunk[3] = a;
                }
            }
        }
    }

    /// Raw RGBA pixel data.
    #[must_use] pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable raw RGBA pixel data.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Width in pixels.
    #[must_use] pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use] pub const fn height(&self) -> u32 {
        self.height
    }

    /// Create a clone of this pixmap (for filter application).
    #[must_use] pub fn clone_pixmap(&self) -> Self {
        Self {
            data: self.data.clone(),
            width: self.width,
            height: self.height,
        }
    }

    /// Resize the pixmap preserving existing content in the top-left corner.
    /// New right/bottom strips are filled with the specified color.
    /// Only grows — returns None if new dimensions are smaller (caller should realloc).
    pub fn resize_grow_only(
        &mut self,
        new_width: u32,
        new_height: u32,
        fill_r: u8,
        fill_g: u8,
        fill_b: u8,
        fill_a: u8,
    ) -> Option<()> {
        if new_width < self.width || new_height < self.height {
            return None;
        }
        if new_width == self.width && new_height == self.height {
            return Some(());
        }

        let old_w = self.width as usize;
        let old_h = self.height as usize;
        let new_w = new_width as usize;
        let new_h = new_height as usize;
        let mut new_data = vec![fill_a; new_w * new_h * 4];

        // Fill entire buffer with fill color first (covers right + bottom strips)
        for chunk in new_data.chunks_exact_mut(4) {
            chunk[0] = fill_r;
            chunk[1] = fill_g;
            chunk[2] = fill_b;
            chunk[3] = fill_a;
        }

        // Copy old rows into top-left corner
        let old_stride = old_w * 4;
        let new_stride = new_w * 4;
        for row in 0..old_h {
            let src = row * old_stride;
            let dst = row * new_stride;
            new_data[dst..dst + old_stride].copy_from_slice(&self.data[src..src + old_stride]);
        }

        self.data = new_data;
        self.width = new_width;
        self.height = new_height;
        Some(())
    }

    /// Resize the pixmap, reusing existing content for the overlapping region.
    /// Works for both growing and shrinking. New areas are filled with the given color.
    pub fn resize_reuse(
        &mut self,
        new_width: u32,
        new_height: u32,
        fill_r: u8,
        fill_g: u8,
        fill_b: u8,
        fill_a: u8,
    ) {
        if new_width == self.width && new_height == self.height {
            return;
        }

        let old_w = self.width as usize;
        let old_h = self.height as usize;
        let new_w = new_width as usize;
        let new_h = new_height as usize;
        let new_stride = new_w * 4;
        let old_stride = old_w * 4;

        let mut new_data = vec![0u8; new_w * new_h * 4];

        // Fill entire buffer with fill color
        for chunk in new_data.chunks_exact_mut(4) {
            chunk[0] = fill_r;
            chunk[1] = fill_g;
            chunk[2] = fill_b;
            chunk[3] = fill_a;
        }

        // Copy overlapping region from old to new
        let copy_rows = old_h.min(new_h);
        let copy_cols_bytes = old_stride.min(new_stride);
        for row in 0..copy_rows {
            let src = row * old_stride;
            let dst = row * new_stride;
            new_data[dst..dst + copy_cols_bytes]
                .copy_from_slice(&self.data[src..src + copy_cols_bytes]);
        }

        self.data = new_data;
        self.width = new_width;
        self.height = new_height;
    }

    /// Encode to PNG using the `png` crate.
    /// # Errors
    ///
    /// Returns an error string if PNG encoding fails.
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| format!("PNG header error: {e}"))?;
            writer
                .write_image_data(&self.data)
                .map_err(|e| format!("PNG write error: {e}"))?;
        }
        Ok(buf)
    }

    /// Decode a PNG byte slice into an `AzulPixmap`.
    /// # Errors
    ///
    /// Returns an error string if `png_bytes` is not a valid PNG.
    pub fn decode_png(png_bytes: &[u8]) -> Result<Self, String> {
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("PNG decode error: {e}"))?;
        let buf_size = reader
            .output_buffer_size()
            .ok_or_else(|| "PNG: unknown output buffer size".to_string())?;
        let mut buf = vec![0u8; buf_size];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| format!("PNG frame error: {e}"))?;
        let width = info.width;
        let height = info.height;

        // Convert to RGBA if needed
        let data = match info.color_type {
            png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
            png::ColorType::Rgb => {
                let mut rgba = Vec::with_capacity((width * height * 4) as usize);
                for chunk in buf[..info.buffer_size()].chunks_exact(3) {
                    rgba.push(chunk[0]);
                    rgba.push(chunk[1]);
                    rgba.push(chunk[2]);
                    rgba.push(255);
                }
                rgba
            }
            png::ColorType::Grayscale => {
                let mut rgba = Vec::with_capacity((width * height * 4) as usize);
                for &v in &buf[..info.buffer_size()] {
                    rgba.push(v);
                    rgba.push(v);
                    rgba.push(v);
                    rgba.push(255);
                }
                rgba
            }
            other => return Err(format!("Unsupported PNG color type: {other:?}")),
        };

        Ok(Self {
            data,
            width,
            height,
        })
    }
}

// ============================================================================
// Pixel-diff comparison for regression testing
// ============================================================================

/// Result of comparing two pixmaps pixel-by-pixel.
#[derive(Copy, Debug, Clone)]
pub struct PixelDiffResult {
    /// Number of pixels that differ beyond the threshold.
    pub diff_count: u64,
    /// Total number of pixels compared.
    pub total_pixels: u64,
    /// Maximum per-channel delta found across all pixels.
    pub max_delta: u8,
    /// Whether dimensions matched.
    pub dimensions_match: bool,
    /// Width of the reference image.
    pub ref_width: u32,
    /// Height of the reference image.
    pub ref_height: u32,
    /// Width of the test image.
    pub test_width: u32,
    /// Height of the test image.
    pub test_height: u32,
}

impl PixelDiffResult {
    /// True if the images are identical within tolerance.
    #[must_use] pub const fn is_match(&self) -> bool {
        self.dimensions_match && self.diff_count == 0
    }

    /// Fraction of pixels that differ (0.0 = identical, 1.0 = all different).
    #[allow(clippy::cast_precision_loss)] // bounded pixel/coord/colour/glyph cast
    #[must_use] pub fn diff_ratio(&self) -> f64 {
        if self.total_pixels == 0 {
            0.0
        } else {
            self.diff_count as f64 / self.total_pixels as f64
        }
    }
}

/// Compare two pixmaps pixel-by-pixel with a per-channel tolerance.
///
/// `threshold` is the maximum allowed per-channel difference (0 = exact match,
/// 2-3 = anti-aliasing tolerance, 10+ = loose match).
#[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
#[must_use] pub fn pixel_diff(reference: &AzulPixmap, test: &AzulPixmap, threshold: u8) -> PixelDiffResult {
    let dimensions_match = reference.width == test.width && reference.height == test.height;
    if !dimensions_match {
        return PixelDiffResult {
            diff_count: 0,
            total_pixels: 0,
            max_delta: 0,
            dimensions_match: false,
            ref_width: reference.width,
            ref_height: reference.height,
            test_width: test.width,
            test_height: test.height,
        };
    }

    let total_pixels = u64::from(reference.width) * u64::from(reference.height);
    let mut diff_count = 0u64;
    let mut max_delta = 0u8;

    for (ref_chunk, test_chunk) in reference
        .data
        .chunks_exact(4)
        .zip(test.data.chunks_exact(4))
    {
        let mut pixel_differs = false;
        for c in 0..4 {
            let delta = (i16::from(ref_chunk[c]) - i16::from(test_chunk[c])).unsigned_abs() as u8;
            if delta > threshold {
                pixel_differs = true;
            }
            if delta > max_delta {
                max_delta = delta;
            }
        }
        if pixel_differs {
            diff_count += 1;
        }
    }

    PixelDiffResult {
        diff_count,
        total_pixels,
        max_delta,
        dimensions_match: true,
        ref_width: reference.width,
        ref_height: reference.height,
        test_width: test.width,
        test_height: test.height,
    }
}

/// Compare a rendered pixmap against a reference PNG file.
///
/// Returns `Ok(result)` with the diff stats, or `Err` if the reference
/// file cannot be read/decoded.
/// # Errors
///
/// Returns an error string if the images cannot be loaded or compared.
pub fn compare_against_reference(
    rendered: &AzulPixmap,
    reference_png_path: &str,
    threshold: u8,
) -> Result<PixelDiffResult, String> {
    let ref_bytes = std::fs::read(reference_png_path)
        .map_err(|e| format!("Cannot read reference image {reference_png_path}: {e}"))?;
    let reference = AzulPixmap::decode_png(&ref_bytes)?;
    Ok(pixel_diff(&reference, rendered, threshold))
}

// ============================================================================
// Simple rect type (replaces tiny_skia::Rect)
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct AzRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Intersect a freshly-pushed clip with the currently-active one.
///
/// `None`
/// means "no clip". An EMPTY intersection clips everything (zero-area rect) —
/// it must NOT degrade to `None`/unclipped, or nested clips could escape
/// their parents.
#[must_use] pub fn intersect_clips(current: Option<AzRect>, new: Option<AzRect>) -> Option<AzRect> {
    match (current, new) {
        (Some(cur), Some(new)) => {
            let x0 = cur.x.max(new.x);
            let y0 = cur.y.max(new.y);
            let x1 = (cur.x + cur.width).min(new.x + new.width);
            let y1 = (cur.y + cur.height).min(new.y + new.height);
            Some(AzRect {
                x: x0,
                y: y0,
                width: (x1 - x0).max(0.0),
                height: (y1 - y0).max(0.0),
            })
        }
        (Some(cur), None) => Some(cur),
        (None, new) => new,
    }
}

impl AzRect {
    pub(crate) fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        if w <= 0.0
            || h <= 0.0
            || !x.is_finite()
            || !y.is_finite()
            || !w.is_finite()
            || !h.is_finite()
        {
            return None;
        }
        Some(Self {
            x,
            y,
            width: w,
            height: h,
        })
    }

    /// Intersect this rect with a clip rect. Returns None if fully clipped.
    pub(crate) fn clip(&self, clip: &Self) -> Option<Self> {
        let x1 = self.x.max(clip.x);
        let y1 = self.y.max(clip.y);
        let x2 = (self.x + self.width).min(clip.x + clip.width);
        let y2 = (self.y + self.height).min(clip.y + clip.height);
        if x2 > x1 && y2 > y1 {
            Some(Self {
                x: x1,
                y: y1,
                width: x2 - x1,
                height: y2 - y1,
            })
        } else {
            None
        }
    }
}

// ============================================================================
// AGG helper: fill a PathStorage with a solid color into an AzulPixmap
// ============================================================================

pub fn agg_fill_path(
    pixmap: &mut AzulPixmap,
    path: &mut dyn VertexSource,
    color: &Rgba8,
    rule: FillingRule,
) {
    agg_fill_path_clipped(pixmap, path, color, rule, None);
}

/// Fill a path with an optional pixel-level clip box.
///
/// When `clip` is `Some`, `RendererBase::clip_box_i()` restricts all
/// scanline output to the clip region.  This handles scroll-frame clips,
/// border-radius is TODO (would need a mask), transforms are handled by
/// transforming the clip box through the inverse transform before setting it.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // bounded pixel/coord/colour/glyph cast
pub fn agg_fill_path_clipped(
    pixmap: &mut AzulPixmap,
    path: &mut dyn VertexSource,
    color: &Rgba8,
    rule: FillingRule,
    clip: Option<AzRect>,
) {
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;
    let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride) };
    let mut pf = PixfmtRgba32::new(&mut ra);
    let mut rb = RendererBase::new(pf);
    if let Some(c) = clip {
        // A degenerate (non-positive-area) clip means "nothing visible". Bail BEFORE
        // building the integer clip box: `(c.x + c.width) as i32 - 1` for width 0 is
        // `c.x - 1`, an INVERTED box that clip_box_i()'s normalize() silently repairs
        // into a small VALID box — so an empty clip used to paint a few pixels.
        if c.width <= 0.0 || c.height <= 0.0 {
            return;
        }
        rb.clip_box_i(
            c.x as i32,
            c.y as i32,
            (c.x + c.width) as i32 - 1,
            (c.y + c.height) as i32 - 1,
        );
    }
    let mut ras = RasterizerScanlineAa::new();
    ras.filling_rule(rule);
    // Clip GEOMETRY to the target pixmap (intersected with any caller clip) before
    // rasterizing. Without this, the scanline sweep runs once per row the path crosses,
    // so a huge/infinite coordinate — reachable from a large-but-legal CSS transform or
    // SVG attribute — is O(coordinate magnitude), i.e. an effective hang. Clamping the
    // rasterizer's clip box bounds the work to the visible area.
    let (clip_x0, clip_y0, clip_x1, clip_y1) = match clip {
        Some(c) => (
            f64::from(c.x).max(0.0),
            f64::from(c.y).max(0.0),
            f64::from(c.x + c.width).min(f64::from(w)),
            f64::from(c.y + c.height).min(f64::from(h)),
        ),
        None => (0.0, 0.0, f64::from(w), f64::from(h)),
    };
    ras.clip_box(clip_x0, clip_y0, clip_x1, clip_y1);
    ras.add_path(path, 0);
    let mut sl = ScanlineU8::new();
    render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, color);
}

#[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
fn agg_fill_transformed_path(
    pixmap: &mut AzulPixmap,
    path: &mut PathStorage,
    color: &Rgba8,
    rule: FillingRule,
    transform: &TransAffine,
) {
    agg_fill_transformed_path_clipped(pixmap, path, color, rule, transform, None);
}

#[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
fn agg_fill_transformed_path_clipped(
    pixmap: &mut AzulPixmap,
    path: &mut PathStorage,
    color: &Rgba8,
    rule: FillingRule,
    transform: &TransAffine,
    clip: Option<AzRect>,
) {
    if transform.is_identity(IDENTITY_EPSILON_F64) {
        agg_fill_path_clipped(pixmap, path, color, rule, clip);
    } else {
        let mut transformed = ConvTransform::new(path, *transform);
        agg_fill_path_clipped(pixmap, &mut transformed, color, rule, clip);
    }
}

// ============================================================================
// AGG helper: fill a path with a gradient into an AzulPixmap
// ============================================================================

fn agg_fill_gradient<G: GradientFunction>(
    pixmap: &mut AzulPixmap,
    path: &mut dyn VertexSource,
    lut: &GradientLut,
    gradient_fn: G,
    transform: TransAffine,
    d1: f64,
    d2: f64,
) {
    agg_fill_gradient_clipped(pixmap, path, lut, gradient_fn, transform, d1, d2, None);
}

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // bounded pixel/coord/colour/glyph cast
pub fn agg_fill_gradient_clipped<G: GradientFunction>(
    pixmap: &mut AzulPixmap,
    path: &mut dyn VertexSource,
    lut: &GradientLut,
    gradient_fn: G,
    transform: TransAffine,
    d1: f64,
    d2: f64,
    clip: Option<AzRect>,
) {
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;
    let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride) };
    let mut pf = PixfmtRgba32::new(&mut ra);
    let mut rb = RendererBase::new(pf);
    if let Some(c) = clip {
        // Degenerate clip = nothing visible; bail before the inverted-box trap (see
        // agg_fill_path_clipped).
        if c.width <= 0.0 || c.height <= 0.0 {
            return;
        }
        rb.clip_box_i(
            c.x as i32,
            c.y as i32,
            (c.x + c.width) as i32 - 1,
            (c.y + c.height) as i32 - 1,
        );
    }
    let mut ras = RasterizerScanlineAa::new();
    ras.filling_rule(FillingRule::NonZero);
    let (clip_x0, clip_y0, clip_x1, clip_y1) = match clip {
        Some(c) => (
            f64::from(c.x).max(0.0),
            f64::from(c.y).max(0.0),
            f64::from(c.x + c.width).min(f64::from(w)),
            f64::from(c.y + c.height).min(f64::from(h)),
        ),
        None => (0.0, 0.0, f64::from(w), f64::from(h)),
    };
    ras.clip_box(clip_x0, clip_y0, clip_x1, clip_y1);
    ras.add_path(path, 0);
    let mut sl = ScanlineU8::new();

    let interp = SpanInterpolatorLinear::new(transform);
    let mut sg = SpanGradient::new(interp, gradient_fn, lut, d1, d2);
    let mut alloc = SpanAllocator::<Rgba8>::new();
    render_scanlines_aa(&mut ras, &mut sl, &mut rb, &mut alloc, &mut sg);
}

// ============================================================================
// Gradient helpers
// ============================================================================

/// Alpha-blend one premultiplied-alpha RGBA buffer onto another at (dx, dy).
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
pub fn blit_buffer(dst: &mut AzulPixmap, src: &[u8], src_w: u32, src_h: u32, dx: i32, dy: i32) {
    let dw = dst.width as i32;
    let dh = dst.height as i32;

    for py in 0..src_h as i32 {
        // saturating: see blit_pixmap — a saturated offset just fails the bounds check.
        let ty = dy.saturating_add(py);
        if ty < 0 || ty >= dh {
            continue;
        }
        for px in 0..src_w as i32 {
            let tx = dx.saturating_add(px);
            if tx < 0 || tx >= dw {
                continue;
            }

            let si = ((py as u32 * src_w + px as u32) * 4) as usize;
            let di = ((ty as u32 * dst.width + tx as u32) * 4) as usize;

            if si + 3 >= src.len() || di + 3 >= dst.data.len() {
                continue;
            }

            let sa = u32::from(src[si + 3]);
            if sa == 0 {
                continue;
            }
            if sa == 255 {
                dst.data[di] = src[si];
                dst.data[di + 1] = src[si + 1];
                dst.data[di + 2] = src[si + 2];
                dst.data[di + 3] = 255;
            } else {
                // Premultiplied-alpha compositing: src RGB already premultiplied by AGG
                let inv_sa = 255 - sa;
                dst.data[di] =
                    ((u32::from(src[si]) + u32::from(dst.data[di]) * inv_sa / 255).min(255)) as u8;
                dst.data[di + 1] =
                    ((u32::from(src[si + 1]) + u32::from(dst.data[di + 1]) * inv_sa / 255).min(255)) as u8;
                dst.data[di + 2] =
                    ((u32::from(src[si + 2]) + u32::from(dst.data[di + 2]) * inv_sa / 255).min(255)) as u8;
                dst.data[di + 3] = ((sa + u32::from(dst.data[di + 3]) * inv_sa / 255).min(255)) as u8;
            }
        }
    }
}

// ============================================================================
// Image mask clipping
// ============================================================================

/// Take a snapshot of a rectangular region of the pixmap.
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)] // bounded pixel/coord/colour/glyph cast
#[must_use] pub fn snapshot_region(pixmap: &AzulPixmap, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
    let pw = pixmap.width as i32;
    let ph = pixmap.height as i32;
    let mut snap = vec![0u8; (w as usize) * (h as usize) * 4];

    for py in 0..h as i32 {
        // saturating: an extreme snapshot origin would overflow a plain `+`.
        let sy = y.saturating_add(py);
        if sy < 0 || sy >= ph {
            continue;
        }
        for px in 0..w as i32 {
            let sx = x.saturating_add(px);
            if sx < 0 || sx >= pw {
                continue;
            }
            let si = ((sy as u32 * pixmap.width + sx as u32) * 4) as usize;
            let di = ((py as u32 * w + px as u32) * 4) as usize;
            if si + 3 < pixmap.data.len() && di + 3 < snap.len() {
                snap[di] = pixmap.data[si];
                snap[di + 1] = pixmap.data[si + 1];
                snap[di + 2] = pixmap.data[si + 2];
                snap[di + 3] = pixmap.data[si + 3];
            }
        }
    }
    snap
}

/// Overwrite (direct copy, no alpha blending) a `w`×`h` RGBA region of `dst` at
/// `(x, y)` with the pixels in `src`.
///
/// Out-of-bounds pixels are skipped. This is the inverse of [`snapshot_region`]
/// and is used to write a filtered backdrop copy back into the output buffer for
/// `backdrop-filter`.
// bounded image-dimension / non-negative-loop-index coordinate casts
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
pub fn write_region(dst: &mut AzulPixmap, src: &[u8], w: u32, h: u32, x: i32, y: i32) {
    let dw = dst.width as i32;
    let dh = dst.height as i32;
    for py in 0..h as i32 {
        // saturating: an extreme write-region origin would overflow a plain `+`.
        let dy = y.saturating_add(py);
        if dy < 0 || dy >= dh {
            continue;
        }
        for px in 0..w as i32 {
            let dx = x.saturating_add(px);
            if dx < 0 || dx >= dw {
                continue;
            }
            let si = ((py as u32 * w + px as u32) * 4) as usize;
            let di = ((dy as u32 * dst.width + dx as u32) * 4) as usize;
            if si + 3 < src.len() && di + 3 < dst.data.len() {
                dst.data[di] = src[si];
                dst.data[di + 1] = src[si + 1];
                dst.data[di + 2] = src[si + 2];
                dst.data[di + 3] = src[si + 3];
            }
        }
    }
}

#[must_use] pub fn union_rect(a: &LogicalRect, b: &LogicalRect) -> LogicalRect {
    let x = a.origin.x.min(b.origin.x);
    let y = a.origin.y.min(b.origin.y);
    let right = (a.origin.x + a.size.width).max(b.origin.x + b.size.width);
    let bottom = (a.origin.y + a.size.height).max(b.origin.y + b.size.height);
    LogicalRect {
        origin: LogicalPosition { x, y },
        size: LogicalSize {
            width: right - x,
            height: bottom - y,
        },
    }
}

#[must_use] pub fn logical_rect_to_az_rect(bounds: &LogicalRect, dpi_factor: f32) -> Option<AzRect> {
    let x = bounds.origin.x * dpi_factor;
    let y = bounds.origin.y * dpi_factor;
    let width = bounds.size.width * dpi_factor;
    let height = bounds.size.height * dpi_factor;

    AzRect::from_xywh(x, y, width, height)
}

#[cfg(test)]
#[allow(clippy::many_single_char_names, clippy::float_cmp)]
mod autotest_generated {
    use agg_rust::span_gradient::GradientX;

    use super::*;

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    fn pm(w: u32, h: u32) -> AzulPixmap {
        AzulPixmap::new(w, h).expect("AzulPixmap::new failed for a valid size")
    }

    fn filled(w: u32, h: u32, c: [u8; 4]) -> AzulPixmap {
        let mut p = pm(w, h);
        p.fill(c[0], c[1], c[2], c[3]);
        p
    }

    /// A pixmap whose R channel encodes `y * width + x` — makes shifts/copies verifiable.
    fn marked(w: u32, h: u32) -> AzulPixmap {
        let mut p = pm(w, h);
        for y in 0..h {
            for x in 0..w {
                let idx = u8::try_from(y * w + x).expect("marker fits in u8");
                set(&mut p, x, y, [idx, 0, 0, 255]);
            }
        }
        p
    }

    /// A pixmap with width == height == 0 (only reachable via `resize_reuse`, never `new`).
    fn zero_sized() -> AzulPixmap {
        let mut p = pm(2, 2);
        p.resize_reuse(0, 0, 0, 0, 0, 0);
        p
    }

    /// Number of pixels that are no longer opaque white.
    fn painted_count(p: &AzulPixmap) -> usize {
        p.data()
            .chunks_exact(4)
            .filter(|c| c[0] != 255 || c[1] != 255 || c[2] != 255 || c[3] != 255)
            .count()
    }

    fn get(p: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.width() + x) * 4) as usize;
        let d = p.data();
        [d[i], d[i + 1], d[i + 2], d[i + 3]]
    }

    fn set(p: &mut AzulPixmap, x: u32, y: u32, c: [u8; 4]) {
        let i = ((y * p.width() + x) * 4) as usize;
        p.data_mut()[i..i + 4].copy_from_slice(&c);
    }

    fn lrect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition { x, y },
            size: LogicalSize {
                width: w,
                height: h,
            },
        }
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn rect_path(x: f64, y: f64, w: f64, h: f64) -> PathStorage {
        let mut p = PathStorage::new();
        p.move_to(x, y);
        p.line_to(x + w, y);
        p.line_to(x + w, y + h);
        p.line_to(x, y + h);
        p.close_polygon(0);
        p
    }

    fn red() -> Rgba8 {
        Rgba8::new(255, 0, 0, 255)
    }

    fn two_stop_lut() -> GradientLut {
        let mut lut = GradientLut::new(256);
        lut.add_color(0.0, Rgba8::new(255, 0, 0, 255));
        lut.add_color(1.0, Rgba8::new(0, 0, 255, 255));
        lut.build_lut();
        lut
    }

    /// Encode a PNG with an arbitrary colour type, so `decode_png`'s non-RGBA
    /// branches get a positive (and a negative) control.
    fn encode_custom_png(w: u32, h: u32, ct: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut buf, w, h);
            enc.set_color(ct);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().expect("test PNG header");
            wr.write_image_data(data).expect("test PNG data");
        }
        buf
    }

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "azul_autotest_pixmap_{}_{tag}.bin",
            std::process::id()
        ))
    }

    // ==================================================================
    // rect_intersection (numeric)
    // ==================================================================

    #[test]
    fn rect_intersection_overlap_is_the_common_area() {
        let a = lrect(0.0, 0.0, 10.0, 10.0);
        let b = lrect(5.0, 5.0, 10.0, 10.0);
        let i = rect_intersection(&a, &b).expect("rects overlap");
        assert!(approx(i.origin.x, 5.0));
        assert!(approx(i.origin.y, 5.0));
        assert!(approx(i.size.width, 5.0));
        assert!(approx(i.size.height, 5.0));
    }

    #[test]
    fn rect_intersection_is_commutative() {
        let a = lrect(-3.0, 2.0, 8.0, 4.0);
        let b = lrect(1.0, 1.0, 9.0, 9.0);
        let ab = rect_intersection(&a, &b).expect("overlap");
        let ba = rect_intersection(&b, &a).expect("overlap");
        assert!(approx(ab.origin.x, ba.origin.x));
        assert!(approx(ab.origin.y, ba.origin.y));
        assert!(approx(ab.size.width, ba.size.width));
        assert!(approx(ab.size.height, ba.size.height));
    }

    #[test]
    fn rect_intersection_zero_sized_rect_is_none() {
        let a = lrect(0.0, 0.0, 0.0, 0.0);
        let b = lrect(0.0, 0.0, 10.0, 10.0);
        // A zero-area rect can never satisfy `x2 > x1 && y2 > y1`.
        assert!(rect_intersection(&a, &b).is_none());
        assert!(rect_intersection(&b, &a).is_none());
    }

    #[test]
    fn rect_intersection_touching_edges_is_none() {
        // a's right edge == b's left edge: zero-width overlap, not an intersection.
        let a = lrect(0.0, 0.0, 5.0, 5.0);
        let b = lrect(5.0, 0.0, 5.0, 5.0);
        assert!(rect_intersection(&a, &b).is_none());
    }

    #[test]
    fn rect_intersection_disjoint_is_none() {
        let a = lrect(0.0, 0.0, 1.0, 1.0);
        let b = lrect(100.0, 100.0, 1.0, 1.0);
        assert!(rect_intersection(&a, &b).is_none());
    }

    #[test]
    fn rect_intersection_negative_coordinates() {
        let a = lrect(-10.0, -10.0, 5.0, 5.0);
        let b = lrect(-7.0, -7.0, 5.0, 5.0);
        let i = rect_intersection(&a, &b).expect("overlap in negative quadrant");
        assert!(approx(i.origin.x, -7.0));
        assert!(approx(i.origin.y, -7.0));
        assert!(approx(i.size.width, 2.0));
        assert!(approx(i.size.height, 2.0));
    }

    #[test]
    fn rect_intersection_result_never_exceeds_either_input() {
        let a = lrect(-1.0, -1.0, 3.0, 100.0);
        let b = lrect(0.0, 0.0, 100.0, 3.0);
        let i = rect_intersection(&a, &b).expect("overlap");
        assert!(i.size.width <= a.size.width && i.size.width <= b.size.width);
        assert!(i.size.height <= a.size.height && i.size.height <= b.size.height);
    }

    #[test]
    fn rect_intersection_f32_max_does_not_panic() {
        let a = lrect(0.0, 0.0, f32::MAX, f32::MAX);
        let b = lrect(0.0, 0.0, f32::MAX, f32::MAX);
        let i = rect_intersection(&a, &b).expect("both cover the same huge area");
        assert!(i.size.width > 0.0 && i.size.height > 0.0);
    }

    #[test]
    fn rect_intersection_nan_does_not_panic_and_stays_finite() {
        let nan = lrect(f32::NAN, f32::NAN, f32::NAN, f32::NAN);
        let ok = lrect(0.0, 0.0, 10.0, 10.0);
        // f32::max/min ignore NaN, so the NaN rect degrades to "the other rect".
        // The only hard requirement: no panic, and no NaN leaking into the result.
        for r in [
            rect_intersection(&nan, &ok),
            rect_intersection(&ok, &nan),
            rect_intersection(&nan, &nan),
        ] {
            if let Some(r) = r {
                assert!(!r.size.width.is_nan(), "NaN width leaked out: {r:?}");
                assert!(!r.size.height.is_nan(), "NaN height leaked out: {r:?}");
                assert!(r.size.width >= 0.0 && r.size.height >= 0.0);
            }
        }
    }

    #[test]
    fn rect_intersection_infinite_size_does_not_panic() {
        let inf = lrect(0.0, 0.0, f32::INFINITY, f32::INFINITY);
        let ok = lrect(2.0, 2.0, 4.0, 4.0);
        let i = rect_intersection(&inf, &ok).expect("infinite rect contains the finite one");
        assert!(approx(i.size.width, 4.0));
        assert!(approx(i.size.height, 4.0));
    }

    // ==================================================================
    // union_rect (numeric)
    // ==================================================================

    #[test]
    fn union_rect_covers_both_inputs() {
        let a = lrect(0.0, 0.0, 2.0, 2.0);
        let b = lrect(8.0, 8.0, 2.0, 2.0);
        let u = union_rect(&a, &b);
        assert!(approx(u.origin.x, 0.0));
        assert!(approx(u.origin.y, 0.0));
        assert!(approx(u.size.width, 10.0));
        assert!(approx(u.size.height, 10.0));
    }

    #[test]
    fn union_rect_with_self_is_identity() {
        let a = lrect(3.0, 4.0, 5.0, 6.0);
        let u = union_rect(&a, &a);
        assert!(approx(u.origin.x, 3.0) && approx(u.origin.y, 4.0));
        assert!(approx(u.size.width, 5.0) && approx(u.size.height, 6.0));
    }

    #[test]
    fn union_rect_negative_origins() {
        let a = lrect(-5.0, -5.0, 1.0, 1.0);
        let b = lrect(5.0, 5.0, 1.0, 1.0);
        let u = union_rect(&a, &b);
        assert!(approx(u.origin.x, -5.0));
        assert!(approx(u.size.width, 11.0));
        assert!(approx(u.size.height, 11.0));
    }

    #[test]
    fn union_rect_zero_size_still_extends_bounds() {
        // A zero-area rect at (20, 20) must still push the union's extent out to 20.
        let a = lrect(0.0, 0.0, 1.0, 1.0);
        let b = lrect(20.0, 20.0, 0.0, 0.0);
        let u = union_rect(&a, &b);
        assert!(approx(u.size.width, 20.0));
        assert!(approx(u.size.height, 20.0));
    }

    #[test]
    fn union_rect_never_shrinks_below_its_inputs() {
        let a = lrect(1.0, 1.0, 4.0, 4.0);
        let b = lrect(2.0, 2.0, 1.0, 1.0); // fully inside a
        let u = union_rect(&a, &b);
        assert!(u.size.width >= a.size.width);
        assert!(u.size.height >= a.size.height);
    }

    #[test]
    fn union_rect_infinite_inputs_do_not_panic() {
        let a = lrect(0.0, 0.0, f32::INFINITY, f32::INFINITY);
        let b = lrect(1.0, 1.0, 1.0, 1.0);
        let u = union_rect(&a, &b);
        assert!(u.size.width.is_infinite());
        assert!(u.size.height.is_infinite());
    }

    // ==================================================================
    // logical_rect_to_az_rect + AzRect::from_xywh (constructor / numeric)
    // ==================================================================

    #[test]
    fn logical_rect_to_az_rect_scales_by_dpi() {
        let r = lrect(1.0, 2.0, 3.0, 4.0);
        let a = logical_rect_to_az_rect(&r, 2.0).expect("positive size");
        assert!(approx(a.x, 2.0));
        assert!(approx(a.y, 4.0));
        assert!(approx(a.width, 6.0));
        assert!(approx(a.height, 8.0));
    }

    #[test]
    fn logical_rect_to_az_rect_zero_dpi_is_none() {
        // dpi 0 collapses the rect to zero area -> from_xywh rejects it.
        let r = lrect(1.0, 2.0, 3.0, 4.0);
        assert!(logical_rect_to_az_rect(&r, 0.0).is_none());
    }

    #[test]
    fn logical_rect_to_az_rect_negative_dpi_is_none() {
        let r = lrect(1.0, 2.0, 3.0, 4.0);
        assert!(logical_rect_to_az_rect(&r, -1.0).is_none());
    }

    #[test]
    fn logical_rect_to_az_rect_zero_size_is_none() {
        assert!(logical_rect_to_az_rect(&lrect(0.0, 0.0, 0.0, 10.0), 1.0).is_none());
        assert!(logical_rect_to_az_rect(&lrect(0.0, 0.0, 10.0, 0.0), 1.0).is_none());
    }

    #[test]
    fn logical_rect_to_az_rect_nan_dpi_is_none() {
        let r = lrect(1.0, 2.0, 3.0, 4.0);
        assert!(logical_rect_to_az_rect(&r, f32::NAN).is_none());
    }

    #[test]
    fn logical_rect_to_az_rect_infinite_dpi_is_none() {
        let r = lrect(1.0, 2.0, 3.0, 4.0);
        assert!(logical_rect_to_az_rect(&r, f32::INFINITY).is_none());
        assert!(logical_rect_to_az_rect(&r, f32::NEG_INFINITY).is_none());
    }

    #[test]
    fn logical_rect_to_az_rect_overflow_to_inf_is_none() {
        // f32::MAX * 2.0 saturates to +inf, which from_xywh must reject.
        let r = lrect(0.0, 0.0, f32::MAX, f32::MAX);
        assert!(logical_rect_to_az_rect(&r, 2.0).is_none());
    }

    #[test]
    fn logical_rect_to_az_rect_nan_bounds_is_none() {
        let r = lrect(f32::NAN, 0.0, 10.0, 10.0);
        assert!(logical_rect_to_az_rect(&r, 1.0).is_none());
    }

    #[test]
    fn az_rect_from_xywh_rejects_nonpositive_size() {
        assert!(AzRect::from_xywh(0.0, 0.0, 0.0, 1.0).is_none());
        assert!(AzRect::from_xywh(0.0, 0.0, 1.0, 0.0).is_none());
        assert!(AzRect::from_xywh(0.0, 0.0, -1.0, 1.0).is_none());
        assert!(AzRect::from_xywh(0.0, 0.0, 1.0, -1.0).is_none());
    }

    #[test]
    fn az_rect_from_xywh_rejects_nonfinite() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(AzRect::from_xywh(bad, 0.0, 1.0, 1.0).is_none(), "x={bad}");
            assert!(AzRect::from_xywh(0.0, bad, 1.0, 1.0).is_none(), "y={bad}");
            assert!(AzRect::from_xywh(0.0, 0.0, bad, 1.0).is_none(), "w={bad}");
            assert!(AzRect::from_xywh(0.0, 0.0, 1.0, bad).is_none(), "h={bad}");
        }
    }

    #[test]
    fn az_rect_from_xywh_keeps_fields_verbatim() {
        let r = AzRect::from_xywh(-3.5, 7.25, 1.5, 2.5).expect("valid");
        assert!(approx(r.x, -3.5));
        assert!(approx(r.y, 7.25));
        assert!(approx(r.width, 1.5));
        assert!(approx(r.height, 2.5));
    }

    #[test]
    fn az_rect_from_xywh_smallest_positive_size_is_accepted() {
        assert!(AzRect::from_xywh(0.0, 0.0, f32::MIN_POSITIVE, f32::MIN_POSITIVE).is_some());
    }

    // ==================================================================
    // AzRect::clip (other)
    // ==================================================================

    #[test]
    fn az_rect_clip_contained_returns_self() {
        let inner = AzRect::from_xywh(2.0, 2.0, 2.0, 2.0).expect("valid");
        let outer = AzRect::from_xywh(0.0, 0.0, 10.0, 10.0).expect("valid");
        let c = inner.clip(&outer).expect("inner is fully inside outer");
        assert!(approx(c.x, 2.0) && approx(c.y, 2.0));
        assert!(approx(c.width, 2.0) && approx(c.height, 2.0));
    }

    #[test]
    fn az_rect_clip_partial_overlap() {
        let a = AzRect::from_xywh(0.0, 0.0, 10.0, 10.0).expect("valid");
        let b = AzRect::from_xywh(5.0, 5.0, 10.0, 10.0).expect("valid");
        let c = a.clip(&b).expect("overlap");
        assert!(approx(c.x, 5.0) && approx(c.width, 5.0));
    }

    #[test]
    fn az_rect_clip_disjoint_is_none() {
        let a = AzRect::from_xywh(0.0, 0.0, 1.0, 1.0).expect("valid");
        let b = AzRect::from_xywh(50.0, 50.0, 1.0, 1.0).expect("valid");
        assert!(a.clip(&b).is_none());
    }

    #[test]
    fn az_rect_clip_touching_edge_is_none() {
        let a = AzRect::from_xywh(0.0, 0.0, 5.0, 5.0).expect("valid");
        let b = AzRect::from_xywh(5.0, 0.0, 5.0, 5.0).expect("valid");
        assert!(a.clip(&b).is_none());
    }

    #[test]
    fn az_rect_clip_huge_rect_does_not_panic() {
        let a = AzRect::from_xywh(0.0, 0.0, f32::MAX, f32::MAX).expect("valid");
        let b = AzRect::from_xywh(1.0, 1.0, 2.0, 2.0).expect("valid");
        let c = a.clip(&b).expect("b is inside a");
        assert!(approx(c.width, 2.0) && approx(c.height, 2.0));
    }

    // ==================================================================
    // intersect_clips (numeric) — the "empty clip must not become unclipped" contract
    // ==================================================================

    #[test]
    fn intersect_clips_none_none_is_none() {
        assert!(intersect_clips(None, None).is_none());
    }

    #[test]
    fn intersect_clips_none_current_adopts_new() {
        let new = AzRect::from_xywh(1.0, 2.0, 3.0, 4.0).expect("valid");
        let r = intersect_clips(None, Some(new)).expect("adopts new");
        assert!(approx(r.x, 1.0) && approx(r.width, 3.0));
    }

    #[test]
    fn intersect_clips_none_new_keeps_current() {
        let cur = AzRect::from_xywh(1.0, 2.0, 3.0, 4.0).expect("valid");
        let r = intersect_clips(Some(cur), None).expect("keeps current");
        assert!(approx(r.x, 1.0) && approx(r.width, 3.0));
    }

    #[test]
    fn intersect_clips_nested_shrinks_to_inner() {
        let outer = AzRect::from_xywh(0.0, 0.0, 10.0, 10.0).expect("valid");
        let inner = AzRect::from_xywh(2.0, 2.0, 3.0, 3.0).expect("valid");
        let r = intersect_clips(Some(outer), Some(inner)).expect("overlap");
        assert!(approx(r.x, 2.0) && approx(r.y, 2.0));
        assert!(approx(r.width, 3.0) && approx(r.height, 3.0));
    }

    #[test]
    fn intersect_clips_never_grows() {
        let a = AzRect::from_xywh(0.0, 0.0, 10.0, 4.0).expect("valid");
        let b = AzRect::from_xywh(3.0, 0.0, 10.0, 10.0).expect("valid");
        let r = intersect_clips(Some(a), Some(b)).expect("overlap");
        assert!(r.width <= a.width && r.width <= b.width);
        assert!(r.height <= a.height && r.height <= b.height);
    }

    #[test]
    fn intersect_clips_disjoint_is_some_zero_area_not_none() {
        // Documented invariant: an EMPTY intersection clips everything and must
        // NOT degrade to `None` (which means "unclipped").
        let a = AzRect::from_xywh(0.0, 0.0, 4.0, 4.0).expect("valid");
        let b = AzRect::from_xywh(10.0, 10.0, 4.0, 4.0).expect("valid");
        let r = intersect_clips(Some(a), Some(b)).expect("must stay Some, not unclipped");
        assert!(approx(r.width, 0.0), "empty clip must have zero width");
        assert!(approx(r.height, 0.0), "empty clip must have zero height");
    }

    #[test]
    fn intersect_clips_never_yields_negative_extent() {
        let a = AzRect::from_xywh(0.0, 0.0, 1.0, 1.0).expect("valid");
        let b = AzRect::from_xywh(100.0, 100.0, 1.0, 1.0).expect("valid");
        let r = intersect_clips(Some(a), Some(b)).expect("some");
        assert!(r.width >= 0.0 && r.height >= 0.0);
    }

    #[test]
    fn intersect_clips_with_self_is_idempotent() {
        let a = AzRect::from_xywh(2.0, 3.0, 4.0, 5.0).expect("valid");
        let r = intersect_clips(Some(a), Some(a)).expect("some");
        assert!(approx(r.x, 2.0) && approx(r.y, 3.0));
        assert!(approx(r.width, 4.0) && approx(r.height, 5.0));
    }

    #[test]
    fn intersect_clips_huge_rects_do_not_panic() {
        let a = AzRect::from_xywh(0.0, 0.0, f32::MAX, f32::MAX).expect("valid");
        let b = AzRect::from_xywh(-1.0e30, -1.0e30, f32::MAX, f32::MAX).expect("valid");
        let r = intersect_clips(Some(a), Some(b)).expect("some");
        assert!(!r.width.is_nan() && !r.height.is_nan());
        assert!(r.width >= 0.0 && r.height >= 0.0);
    }

    // ==================================================================
    // AzulPixmap::new (constructor) + getters
    // ==================================================================

    #[test]
    fn pixmap_new_zero_dimension_is_none() {
        assert!(AzulPixmap::new(0, 0).is_none());
        assert!(AzulPixmap::new(0, 16).is_none());
        assert!(AzulPixmap::new(16, 0).is_none());
    }

    #[test]
    fn pixmap_new_invariants_hold() {
        let p = pm(3, 5);
        assert_eq!(p.width(), 3);
        assert_eq!(p.height(), 5);
        assert_eq!(p.data().len(), 3 * 5 * 4);
        assert!(
            p.data().iter().all(|&b| b == 255),
            "new() is documented to be opaque white"
        );
    }

    #[test]
    fn pixmap_new_1x1_is_a_single_white_pixel() {
        let p = pm(1, 1);
        assert_eq!(p.data(), &WHITE);
    }

    #[test]
    fn pixmap_new_absurd_dimensions_return_none_instead_of_aborting() {
        // `new` returns Option, so the documented failure mode for a size that
        // cannot be allocated is None — not a `capacity overflow` panic.
        assert!(AzulPixmap::new(u32::MAX, u32::MAX).is_none());
    }

    #[test]
    fn pixmap_getters_on_zero_sized_instance_do_not_panic() {
        let p = zero_sized();
        assert_eq!(p.width(), 0);
        assert_eq!(p.height(), 0);
        assert!(p.data().is_empty());
    }

    #[test]
    fn data_mut_writes_are_visible_through_data() {
        let mut p = pm(2, 1);
        p.data_mut()[0] = 7;
        assert_eq!(p.data()[0], 7);
        assert_eq!(p.data().len(), 8);
    }

    #[test]
    fn data_mut_on_zero_sized_instance_is_empty_not_panic() {
        let mut p = zero_sized();
        assert!(p.data_mut().is_empty());
    }

    #[test]
    fn clone_pixmap_is_a_deep_copy() {
        let src = filled(2, 2, [1, 2, 3, 4]);
        let mut cloned = src.clone_pixmap();
        assert_eq!(cloned.width(), src.width());
        assert_eq!(cloned.height(), src.height());
        assert_eq!(cloned.data(), src.data());

        set(&mut cloned, 0, 0, [9, 9, 9, 9]);
        assert_eq!(get(&src, 0, 0), [1, 2, 3, 4], "clone must not alias source");
    }

    #[test]
    fn clone_pixmap_of_zero_sized_instance_does_not_panic() {
        let p = zero_sized();
        let c = p.clone_pixmap();
        assert_eq!(c.width(), 0);
        assert!(c.data().is_empty());
    }

    // ==================================================================
    // AzulPixmap::fill (numeric)
    // ==================================================================

    #[test]
    fn fill_sets_every_channel_of_every_pixel() {
        let p = filled(4, 3, [10, 20, 30, 40]);
        for y in 0..3 {
            for x in 0..4 {
                assert_eq!(get(&p, x, y), [10, 20, 30, 40]);
            }
        }
    }

    #[test]
    fn fill_with_u8_extremes() {
        let p = filled(2, 2, [0, 0, 0, 0]);
        assert!(p.data().iter().all(|&b| b == 0));
        let p = filled(2, 2, [255, 255, 255, 255]);
        assert!(p.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn fill_on_zero_sized_pixmap_does_not_panic() {
        let mut p = zero_sized();
        p.fill(1, 2, 3, 4);
        assert!(p.data().is_empty());
    }

    // ==================================================================
    // AzulPixmap::fill_rect (numeric)
    // ==================================================================

    #[test]
    fn fill_rect_fills_exactly_the_requested_box() {
        let mut p = filled(5, 5, CLEAR);
        p.fill_rect(1, 1, 2, 2, 1, 2, 3, 4);
        assert_eq!(get(&p, 1, 1), [1, 2, 3, 4]);
        assert_eq!(get(&p, 2, 2), [1, 2, 3, 4]);
        assert_eq!(get(&p, 0, 0), CLEAR, "outside the box must be untouched");
        assert_eq!(get(&p, 3, 3), CLEAR, "x1/y1 are exclusive");
    }

    #[test]
    fn fill_rect_zero_size_is_a_noop() {
        let mut p = filled(4, 4, CLEAR);
        p.fill_rect(1, 1, 0, 0, 255, 0, 0, 255);
        assert!(p.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn fill_rect_negative_origin_clips_to_the_pixmap() {
        let mut p = filled(4, 4, CLEAR);
        p.fill_rect(-2, -2, 4, 4, 9, 9, 9, 9);
        // Only the on-screen quadrant (0..2, 0..2) is written.
        assert_eq!(get(&p, 0, 0), [9, 9, 9, 9]);
        assert_eq!(get(&p, 1, 1), [9, 9, 9, 9]);
        assert_eq!(get(&p, 2, 2), CLEAR);
    }

    #[test]
    fn fill_rect_fully_offscreen_is_a_noop() {
        let mut p = filled(4, 4, CLEAR);
        p.fill_rect(100, 100, 10, 10, 255, 0, 0, 255);
        p.fill_rect(-100, -100, 10, 10, 255, 0, 0, 255);
        assert!(p.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn fill_rect_negative_width_does_not_panic() {
        // A negative width must be treated as an empty rect (a no-op), not turned
        // into a reversed slice range.
        let mut p = filled(8, 8, CLEAR);
        p.fill_rect(3, 0, -5, 2, 255, 0, 0, 255);
        assert!(
            p.data().iter().all(|&b| b == 0),
            "a negative-width rect must paint nothing"
        );
    }

    #[test]
    fn fill_rect_negative_height_is_a_noop() {
        let mut p = filled(8, 8, CLEAR);
        p.fill_rect(0, 3, 2, -5, 255, 0, 0, 255);
        assert!(p.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn fill_rect_i32_extremes_do_not_panic() {
        let mut p = filled(4, 4, CLEAR);
        p.fill_rect(i32::MIN, i32::MIN, i32::MAX, i32::MAX, 1, 1, 1, 1);
        p.fill_rect(i32::MAX, i32::MAX, i32::MAX, i32::MAX, 2, 2, 2, 2);
        p.fill_rect(0, 0, i32::MAX, i32::MAX, 3, 3, 3, 3);
        // The last call saturates to the full pixmap.
        assert_eq!(get(&p, 0, 0), [3, 3, 3, 3]);
        assert_eq!(get(&p, 3, 3), [3, 3, 3, 3]);
    }

    #[test]
    fn fill_rect_on_zero_sized_pixmap_does_not_panic() {
        let mut p = zero_sized();
        p.fill_rect(0, 0, 10, 10, 1, 2, 3, 4);
        assert!(p.data().is_empty());
    }

    // ==================================================================
    // resize_grow_only (numeric)
    // ==================================================================

    #[test]
    fn resize_grow_only_rejects_shrinking() {
        let mut p = filled(4, 4, [1, 2, 3, 4]);
        assert!(p.resize_grow_only(2, 4, 0, 0, 0, 0).is_none());
        assert!(p.resize_grow_only(4, 2, 0, 0, 0, 0).is_none());
        assert!(p.resize_grow_only(2, 2, 0, 0, 0, 0).is_none());
        // Mixed grow/shrink is still a rejection.
        assert!(p.resize_grow_only(8, 2, 0, 0, 0, 0).is_none());
        assert_eq!(p.width(), 4, "a rejected resize must not mutate");
        assert_eq!(p.height(), 4);
        assert_eq!(p.data().len(), 4 * 4 * 4);
    }

    #[test]
    fn resize_grow_only_same_dimensions_is_a_noop_some() {
        let mut p = filled(3, 3, [7, 7, 7, 7]);
        assert!(p.resize_grow_only(3, 3, 0, 0, 0, 0).is_some());
        assert_eq!(p.width(), 3);
        assert!(p.data().iter().all(|&b| b == 7));
    }

    #[test]
    fn resize_grow_only_preserves_topleft_and_fills_the_new_strips() {
        let mut p = marked(2, 2);
        p.resize_grow_only(4, 4, 9, 8, 7, 6).expect("growing is allowed");
        assert_eq!(p.width(), 4);
        assert_eq!(p.height(), 4);
        assert_eq!(p.data().len(), 4 * 4 * 4);
        // old content stays in the top-left
        assert_eq!(get(&p, 0, 0), [0, 0, 0, 255]);
        assert_eq!(get(&p, 1, 0), [1, 0, 0, 255]);
        assert_eq!(get(&p, 0, 1), [2, 0, 0, 255]);
        assert_eq!(get(&p, 1, 1), [3, 0, 0, 255]);
        // new right/bottom strips carry the fill colour
        assert_eq!(get(&p, 3, 0), [9, 8, 7, 6]);
        assert_eq!(get(&p, 0, 3), [9, 8, 7, 6]);
        assert_eq!(get(&p, 3, 3), [9, 8, 7, 6]);
    }

    #[test]
    fn resize_grow_only_grow_one_axis_only() {
        let mut p = marked(2, 2);
        p.resize_grow_only(2, 4, 1, 1, 1, 1).expect("height-only growth");
        assert_eq!(p.width(), 2);
        assert_eq!(p.height(), 4);
        assert_eq!(get(&p, 1, 1), [3, 0, 0, 255]);
        assert_eq!(get(&p, 0, 3), [1, 1, 1, 1]);
    }

    #[test]
    fn resize_grow_only_from_zero_sized_pixmap_does_not_panic() {
        let mut p = zero_sized();
        p.resize_grow_only(2, 2, 5, 5, 5, 5)
            .expect("0x0 -> 2x2 is a growth");
        assert_eq!(p.width(), 2);
        assert_eq!(p.data().len(), 16);
        assert_eq!(get(&p, 0, 0), [5, 5, 5, 5]);
    }

    // ==================================================================
    // resize_reuse (numeric)
    // ==================================================================

    #[test]
    fn resize_reuse_same_dimensions_is_a_noop() {
        let mut p = filled(3, 3, [4, 4, 4, 4]);
        p.resize_reuse(3, 3, 0, 0, 0, 0);
        assert!(p.data().iter().all(|&b| b == 4));
    }

    #[test]
    fn resize_reuse_grow_preserves_overlap_and_fills_the_rest() {
        let mut p = marked(2, 2);
        p.resize_reuse(4, 3, 1, 2, 3, 4);
        assert_eq!(p.width(), 4);
        assert_eq!(p.height(), 3);
        assert_eq!(p.data().len(), 4 * 3 * 4);
        assert_eq!(get(&p, 0, 0), [0, 0, 0, 255]);
        assert_eq!(get(&p, 1, 1), [3, 0, 0, 255]);
        assert_eq!(get(&p, 3, 0), [1, 2, 3, 4]);
        assert_eq!(get(&p, 0, 2), [1, 2, 3, 4]);
    }

    #[test]
    fn resize_reuse_shrink_crops_to_the_topleft() {
        let mut p = marked(4, 4);
        p.resize_reuse(2, 2, 0, 0, 0, 0);
        assert_eq!(p.width(), 2);
        assert_eq!(p.height(), 2);
        assert_eq!(p.data().len(), 2 * 2 * 4);
        // markers were y*4 + x on the old 4x4 grid
        assert_eq!(get(&p, 0, 0), [0, 0, 0, 255]);
        assert_eq!(get(&p, 1, 0), [1, 0, 0, 255]);
        assert_eq!(get(&p, 0, 1), [4, 0, 0, 255]);
        assert_eq!(get(&p, 1, 1), [5, 0, 0, 255]);
    }

    #[test]
    fn resize_reuse_shrink_width_only_keeps_rows_aligned() {
        let mut p = marked(4, 2);
        p.resize_reuse(2, 2, 0, 0, 0, 0);
        assert_eq!(get(&p, 0, 1), [4, 0, 0, 255], "row 1 must not be smeared");
        assert_eq!(get(&p, 1, 1), [5, 0, 0, 255]);
    }

    #[test]
    fn resize_reuse_to_zero_yields_an_empty_buffer() {
        let mut p = filled(4, 4, [1, 1, 1, 1]);
        p.resize_reuse(0, 0, 2, 2, 2, 2);
        assert_eq!(p.width(), 0);
        assert_eq!(p.height(), 0);
        assert!(p.data().is_empty());
    }

    #[test]
    fn resize_reuse_from_zero_sized_pixmap_does_not_panic() {
        let mut p = zero_sized();
        p.resize_reuse(2, 2, 3, 3, 3, 3);
        assert_eq!(p.width(), 2);
        assert_eq!(get(&p, 0, 0), [3, 3, 3, 3]);
    }

    #[test]
    fn resize_reuse_data_len_always_matches_the_new_dimensions() {
        let mut p = marked(3, 3);
        for (w, h) in [(1u32, 1u32), (5, 2), (2, 5), (7, 7), (1, 9)] {
            p.resize_reuse(w, h, 0, 0, 0, 0);
            assert_eq!(p.width(), w);
            assert_eq!(p.height(), h);
            assert_eq!(p.data().len(), (w as usize) * (h as usize) * 4);
        }
    }

    // ==================================================================
    // encode_png / decode_png (round-trip + parser)
    // ==================================================================

    #[test]
    fn png_round_trip_preserves_dimensions_and_pixels() {
        let src = marked(5, 3);
        let bytes = src.encode_png().expect("encode");
        let back = AzulPixmap::decode_png(&bytes).expect("decode");
        assert_eq!(back.width(), src.width());
        assert_eq!(back.height(), src.height());
        assert_eq!(back.data(), src.data());
    }

    #[test]
    fn png_round_trip_1x1() {
        let mut src = pm(1, 1);
        set(&mut src, 0, 0, [1, 2, 3, 4]);
        let bytes = src.encode_png().expect("encode");
        let back = AzulPixmap::decode_png(&bytes).expect("decode");
        assert_eq!(back.data(), &[1, 2, 3, 4]);
    }

    #[test]
    fn png_round_trip_preserves_full_alpha_range() {
        let mut src = pm(4, 1);
        set(&mut src, 0, 0, [0, 0, 0, 0]);
        set(&mut src, 1, 0, [255, 255, 255, 255]);
        set(&mut src, 2, 0, [255, 0, 0, 1]);
        set(&mut src, 3, 0, [0, 255, 0, 254]);
        let bytes = src.encode_png().expect("encode");
        let back = AzulPixmap::decode_png(&bytes).expect("decode");
        assert_eq!(back.data(), src.data(), "PNG must not premultiply alpha");
    }

    #[test]
    fn encode_png_starts_with_the_png_signature() {
        let bytes = pm(2, 2).encode_png().expect("encode");
        assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    }

    #[test]
    fn encode_png_of_a_zero_sized_pixmap_is_err_not_panic() {
        let p = zero_sized();
        let e = p.encode_png().expect_err("PNG forbids zero dimensions");
        assert!(e.contains("PNG header error"), "unexpected message: {e}");
    }

    #[test]
    fn decode_png_empty_input_is_err() {
        assert!(AzulPixmap::decode_png(&[]).is_err());
    }

    #[test]
    fn decode_png_whitespace_only_is_err() {
        assert!(AzulPixmap::decode_png(b"   ").is_err());
        assert!(AzulPixmap::decode_png(b"\t\n\r\n").is_err());
    }

    #[test]
    fn decode_png_garbage_is_err() {
        assert!(AzulPixmap::decode_png(b"not a png at all").is_err());
        assert!(AzulPixmap::decode_png(&[0x00, 0x01, 0x02, 0x03]).is_err());
    }

    #[test]
    fn decode_png_invalid_utf8_bytes_are_err() {
        assert!(AzulPixmap::decode_png(&[0xFF, 0xFE, 0x00]).is_err());
        assert!(AzulPixmap::decode_png(&[0xC0, 0x80, 0xED, 0xA0, 0x80]).is_err());
    }

    #[test]
    fn decode_png_unicode_text_is_err() {
        assert!(AzulPixmap::decode_png("\u{1F600} héllo n\u{0303}".as_bytes()).is_err());
    }

    #[test]
    fn decode_png_boundary_number_strings_are_err() {
        for s in ["0", "-0", "9223372036854775807", "NaN", "inf", "-inf", "1e999"] {
            assert!(
                AzulPixmap::decode_png(s.as_bytes()).is_err(),
                "{s:?} is not a PNG"
            );
        }
    }

    #[test]
    fn decode_png_extremely_long_garbage_is_err_and_terminates() {
        let junk = vec![0x41u8; 1_000_000];
        assert!(AzulPixmap::decode_png(&junk).is_err());
    }

    #[test]
    fn decode_png_deeply_nested_brackets_is_err() {
        let mut nested = vec![b'['; 10_000];
        nested.extend(std::iter::repeat(b']').take(10_000));
        assert!(AzulPixmap::decode_png(&nested).is_err());
    }

    #[test]
    fn decode_png_signature_without_chunks_is_err() {
        let sig = [0x89u8, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        assert!(AzulPixmap::decode_png(&sig).is_err());
    }

    #[test]
    fn decode_png_truncated_valid_png_is_err() {
        let full = pm(4, 4).encode_png().expect("encode");
        for cut in [9, full.len() / 2, full.len() - 1] {
            assert!(
                AzulPixmap::decode_png(&full[..cut]).is_err(),
                "a PNG truncated to {cut} bytes must not decode"
            );
        }
    }

    #[test]
    fn decode_png_leading_junk_is_rejected() {
        let full = pm(2, 2).encode_png().expect("encode");
        let mut with_junk = b"garbage".to_vec();
        with_junk.extend_from_slice(&full);
        assert!(
            AzulPixmap::decode_png(&with_junk).is_err(),
            "the decoder must not scan forward for a signature"
        );
    }

    #[test]
    fn decode_png_trailing_junk_after_iend_still_decodes() {
        let src = marked(2, 2);
        let mut bytes = src.encode_png().expect("encode");
        bytes.extend_from_slice(b"garbage;after;iend");
        let back = AzulPixmap::decode_png(&bytes).expect("bytes past IEND are outside the stream");
        assert_eq!(back.data(), src.data());
    }

    #[test]
    fn decode_png_rgb_expands_to_opaque_rgba() {
        let bytes = encode_custom_png(2, 1, png::ColorType::Rgb, &[10, 20, 30, 40, 50, 60]);
        let p = AzulPixmap::decode_png(&bytes).expect("RGB is supported");
        assert_eq!(p.width(), 2);
        assert_eq!(p.height(), 1);
        assert_eq!(p.data(), &[10, 20, 30, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn decode_png_grayscale_expands_to_rgba() {
        let bytes = encode_custom_png(2, 1, png::ColorType::Grayscale, &[7, 9]);
        let p = AzulPixmap::decode_png(&bytes).expect("grayscale is supported");
        assert_eq!(p.data(), &[7, 7, 7, 255, 9, 9, 9, 255]);
    }

    #[test]
    fn decode_png_unsupported_color_type_is_err_not_panic() {
        let bytes = encode_custom_png(1, 1, png::ColorType::GrayscaleAlpha, &[7, 128]);
        let e = AzulPixmap::decode_png(&bytes).expect_err("gray+alpha is not handled");
        assert!(
            e.contains("Unsupported PNG color type"),
            "unexpected message: {e}"
        );
    }

    // ==================================================================
    // PixelDiffResult (predicate + getter)
    // ==================================================================

    fn diff_result(diff_count: u64, total_pixels: u64, dimensions_match: bool) -> PixelDiffResult {
        PixelDiffResult {
            diff_count,
            total_pixels,
            max_delta: 0,
            dimensions_match,
            ref_width: 1,
            ref_height: 1,
            test_width: 1,
            test_height: 1,
        }
    }

    #[test]
    fn is_match_is_true_only_when_dimensions_match_and_no_pixel_differs() {
        assert!(diff_result(0, 100, true).is_match());
        assert!(!diff_result(1, 100, true).is_match());
        assert!(!diff_result(0, 100, false).is_match());
        assert!(!diff_result(u64::MAX, 100, true).is_match());
    }

    #[test]
    fn is_match_on_an_empty_comparison_is_deterministic() {
        // Zero pixels compared but dimensions agree -> vacuously a match.
        assert!(diff_result(0, 0, true).is_match());
        assert!(!diff_result(0, 0, false).is_match());
    }

    #[test]
    fn diff_ratio_of_zero_total_is_zero_not_nan() {
        let r = diff_result(0, 0, true).diff_ratio();
        assert!(r.is_finite() && r == 0.0, "0/0 must not become NaN");
    }

    #[test]
    fn diff_ratio_basic_values() {
        assert!((diff_result(0, 100, true).diff_ratio() - 0.0).abs() < 1e-12);
        assert!((diff_result(50, 100, true).diff_ratio() - 0.5).abs() < 1e-12);
        assert!((diff_result(100, 100, true).diff_ratio() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn diff_ratio_at_u64_max_stays_finite() {
        let r = diff_result(u64::MAX, u64::MAX, true).diff_ratio();
        assert!(r.is_finite(), "u64::MAX/u64::MAX must not overflow to inf");
        assert!((r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn diff_ratio_on_a_dimension_mismatch_is_zero_yet_not_a_match() {
        // A caller that only checks diff_ratio() would think a size mismatch is
        // a perfect match — pin the (surprising but documented) behaviour down.
        let r = diff_result(0, 0, false);
        assert!((r.diff_ratio() - 0.0).abs() < 1e-12);
        assert!(!r.is_match());
    }

    // ==================================================================
    // pixel_diff (numeric)
    // ==================================================================

    #[test]
    fn pixel_diff_identical_images_match() {
        let a = filled(4, 4, [1, 2, 3, 4]);
        let b = filled(4, 4, [1, 2, 3, 4]);
        let r = pixel_diff(&a, &b, 0);
        assert!(r.is_match());
        assert_eq!(r.diff_count, 0);
        assert_eq!(r.max_delta, 0);
        assert_eq!(r.total_pixels, 16);
    }

    #[test]
    fn pixel_diff_threshold_is_exclusive_at_the_boundary() {
        let a = filled(2, 2, [10, 10, 10, 255]);
        let b = filled(2, 2, [13, 10, 10, 255]); // delta of exactly 3 on R
        assert!(
            pixel_diff(&a, &b, 3).is_match(),
            "delta == threshold is within tolerance"
        );
        assert!(
            !pixel_diff(&a, &b, 2).is_match(),
            "delta > threshold must be reported"
        );
        assert_eq!(pixel_diff(&a, &b, 2).diff_count, 4);
        assert_eq!(pixel_diff(&a, &b, 3).max_delta, 3, "max_delta ignores the threshold");
    }

    #[test]
    fn pixel_diff_threshold_zero_catches_a_single_bit() {
        let a = filled(2, 2, [0, 0, 0, 0]);
        let mut b = filled(2, 2, [0, 0, 0, 0]);
        set(&mut b, 1, 1, [0, 0, 0, 1]);
        let r = pixel_diff(&a, &b, 0);
        assert!(!r.is_match());
        assert_eq!(r.diff_count, 1);
        assert_eq!(r.max_delta, 1);
    }

    #[test]
    fn pixel_diff_threshold_255_always_matches() {
        // The largest possible per-channel delta is 255, and the check is `>`.
        let a = filled(3, 3, [0, 0, 0, 0]);
        let b = filled(3, 3, [255, 255, 255, 255]);
        let r = pixel_diff(&a, &b, 255);
        assert!(r.is_match(), "threshold 255 tolerates every possible delta");
        assert_eq!(r.diff_count, 0);
        assert_eq!(r.max_delta, 255);
    }

    #[test]
    fn pixel_diff_max_delta_is_unsigned_and_direction_independent() {
        let a = filled(1, 1, [0, 0, 0, 0]);
        let b = filled(1, 1, [255, 0, 0, 0]);
        assert_eq!(pixel_diff(&a, &b, 0).max_delta, 255, "no wraparound");
        assert_eq!(
            pixel_diff(&b, &a, 0).max_delta,
            255,
            "reference/test order must not change |delta|"
        );
    }

    #[test]
    fn pixel_diff_all_pixels_different_ratio_is_one() {
        let a = filled(4, 4, [0, 0, 0, 0]);
        let b = filled(4, 4, [255, 255, 255, 255]);
        let r = pixel_diff(&a, &b, 0);
        assert_eq!(r.diff_count, r.total_pixels);
        assert!((r.diff_ratio() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn pixel_diff_dimension_mismatch_reports_both_sizes_and_no_match() {
        let a = filled(4, 4, CLEAR);
        let b = filled(2, 8, CLEAR);
        let r = pixel_diff(&a, &b, 0);
        assert!(!r.is_match());
        assert!(!r.dimensions_match);
        assert_eq!(r.total_pixels, 0);
        assert_eq!(r.diff_count, 0);
        assert_eq!((r.ref_width, r.ref_height), (4, 4));
        assert_eq!((r.test_width, r.test_height), (2, 8));
    }

    #[test]
    fn pixel_diff_ratio_is_always_within_0_and_1() {
        let a = marked(4, 4);
        let b = filled(4, 4, [0, 0, 0, 0]);
        for t in [0u8, 1, 127, 254, 255] {
            let r = pixel_diff(&a, &b, t);
            let ratio = r.diff_ratio();
            assert!((0.0..=1.0).contains(&ratio), "ratio {ratio} out of range at t={t}");
            assert!(r.diff_count <= r.total_pixels);
        }
    }

    #[test]
    fn pixel_diff_on_zero_sized_pixmaps_does_not_panic() {
        let a = zero_sized();
        let b = zero_sized();
        let r = pixel_diff(&a, &b, 0);
        assert!(r.is_match());
        assert_eq!(r.total_pixels, 0);
        assert!((r.diff_ratio() - 0.0).abs() < 1e-12);
    }

    // ==================================================================
    // compare_against_reference (parser / IO)
    // ==================================================================

    #[test]
    fn compare_against_reference_missing_file_is_err() {
        let p = pm(2, 2);
        let e = compare_against_reference(&p, "/nonexistent/azul/does_not_exist.png", 0)
            .expect_err("missing file");
        assert!(e.contains("Cannot read reference image"), "got: {e}");
    }

    #[test]
    fn compare_against_reference_empty_path_is_err() {
        let p = pm(2, 2);
        assert!(compare_against_reference(&p, "", 0).is_err());
    }

    #[test]
    fn compare_against_reference_whitespace_path_is_err() {
        let p = pm(2, 2);
        assert!(compare_against_reference(&p, "   ", 0).is_err());
        assert!(compare_against_reference(&p, "\t\n", 0).is_err());
    }

    #[test]
    fn compare_against_reference_nul_byte_path_is_err_not_panic() {
        let p = pm(2, 2);
        assert!(compare_against_reference(&p, "bad\0path.png", 0).is_err());
    }

    #[test]
    fn compare_against_reference_unicode_path_is_err() {
        let p = pm(2, 2);
        assert!(compare_against_reference(&p, "/tmp/\u{1F600}/nope.png", 0).is_err());
    }

    #[test]
    fn compare_against_reference_extremely_long_path_is_err_not_panic() {
        let p = pm(2, 2);
        let long = format!("/tmp/{}.png", "a".repeat(100_000));
        assert!(compare_against_reference(&p, &long, 0).is_err());
    }

    #[test]
    fn compare_against_reference_boundary_number_paths_are_err() {
        let p = pm(2, 2);
        for s in ["0", "-0", "NaN", "inf", "9223372036854775807"] {
            assert!(compare_against_reference(&p, s, 0).is_err(), "{s:?}");
        }
    }

    #[test]
    fn compare_against_reference_non_png_file_is_err() {
        let path = temp_path("not_a_png");
        std::fs::write(&path, b"definitely not a png").expect("write temp file");
        let p = pm(2, 2);
        let res = compare_against_reference(&p, path.to_str().expect("utf8 path"), 0);
        let _ = std::fs::remove_file(&path);
        let e = res.expect_err("a non-PNG file must not decode");
        assert!(e.contains("PNG decode error"), "got: {e}");
    }

    #[test]
    fn compare_against_reference_valid_png_matches_itself() {
        let src = marked(3, 2);
        let path = temp_path("valid_ref");
        std::fs::write(&path, src.encode_png().expect("encode")).expect("write temp file");
        let res = compare_against_reference(&src, path.to_str().expect("utf8 path"), 0);
        let _ = std::fs::remove_file(&path);
        let r = res.expect("a freshly written reference must decode");
        assert!(r.is_match(), "an image must match its own PNG: {r:?}");
        assert_eq!(r.total_pixels, 6);
    }

    #[test]
    fn compare_against_reference_size_mismatch_is_ok_but_not_a_match() {
        let src = marked(3, 2);
        let path = temp_path("size_mismatch");
        std::fs::write(&path, src.encode_png().expect("encode")).expect("write temp file");
        let other = pm(4, 4);
        let res = compare_against_reference(&other, path.to_str().expect("utf8 path"), 0);
        let _ = std::fs::remove_file(&path);
        let r = res.expect("decoding succeeds; only the comparison fails");
        assert!(!r.is_match());
        assert!(!r.dimensions_match);
    }

    // ==================================================================
    // blit_pixmap (numeric)
    // ==================================================================

    #[test]
    fn blit_pixmap_full_opacity_opaque_src_overwrites_dst() {
        let src = filled(2, 2, [10, 20, 30, 255]);
        let mut dst = filled(4, 4, CLEAR);
        blit_pixmap(&src, &mut dst, 1, 1, 1.0);
        assert_eq!(get(&dst, 1, 1), [10, 20, 30, 255]);
        assert_eq!(get(&dst, 2, 2), [10, 20, 30, 255]);
        assert_eq!(get(&dst, 0, 0), CLEAR);
        assert_eq!(get(&dst, 3, 3), CLEAR);
    }

    #[test]
    fn blit_pixmap_zero_opacity_leaves_dst_untouched() {
        let src = filled(2, 2, [10, 20, 30, 255]);
        let mut dst = filled(2, 2, WHITE);
        blit_pixmap(&src, &mut dst, 0, 0, 0.0);
        assert!(dst.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn blit_pixmap_transparent_src_leaves_dst_untouched() {
        let src = filled(2, 2, [10, 20, 30, 0]);
        let mut dst = filled(2, 2, WHITE);
        blit_pixmap(&src, &mut dst, 0, 0, 1.0);
        assert!(dst.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn blit_pixmap_nan_opacity_does_not_panic_and_blits_nothing() {
        // (NaN * 255).clamp(..) is NaN, and `NaN as u32` saturates to 0.
        let src = filled(2, 2, [10, 20, 30, 255]);
        let mut dst = filled(2, 2, WHITE);
        blit_pixmap(&src, &mut dst, 0, 0, f32::NAN);
        assert!(
            dst.data().iter().all(|&b| b == 255),
            "a NaN opacity must not paint anything"
        );
    }

    #[test]
    fn blit_pixmap_infinite_opacity_clamps_to_fully_opaque() {
        let src = filled(1, 1, [10, 20, 30, 255]);
        let mut dst = filled(1, 1, WHITE);
        blit_pixmap(&src, &mut dst, 0, 0, f32::INFINITY);
        assert_eq!(get(&dst, 0, 0), [10, 20, 30, 255]);
    }

    #[test]
    fn blit_pixmap_negative_infinite_opacity_clamps_to_transparent() {
        let src = filled(1, 1, [10, 20, 30, 255]);
        let mut dst = filled(1, 1, WHITE);
        blit_pixmap(&src, &mut dst, 0, 0, f32::NEG_INFINITY);
        assert_eq!(get(&dst, 0, 0), WHITE);
    }

    #[test]
    fn blit_pixmap_opacity_above_one_clamps_to_one() {
        let src = filled(1, 1, [10, 20, 30, 255]);
        let mut a = filled(1, 1, CLEAR);
        let mut b = filled(1, 1, CLEAR);
        blit_pixmap(&src, &mut a, 0, 0, 1.0);
        blit_pixmap(&src, &mut b, 0, 0, 1.0e30);
        assert_eq!(a.data(), b.data(), "opacity is clamped to [0, 1]");
    }

    #[test]
    fn blit_pixmap_half_alpha_blend_is_exact() {
        let src = filled(1, 1, [200, 100, 50, 128]);
        let mut dst = filled(1, 1, CLEAR);
        blit_pixmap(&src, &mut dst, 0, 0, 1.0);
        // sa = 128, inv = 127, dst starts at 0 => (c * 128) / 255
        assert_eq!(get(&dst, 0, 0), [100, 50, 25, 128]);
    }

    #[test]
    fn blit_pixmap_negative_position_clips_to_dst() {
        let src = marked(2, 2);
        let mut dst = filled(4, 4, CLEAR);
        blit_pixmap(&src, &mut dst, -1, -1, 1.0);
        // Only src(1,1) (marker 3) lands on dst(0,0).
        assert_eq!(get(&dst, 0, 0), [3, 0, 0, 255]);
        assert_eq!(get(&dst, 1, 1), CLEAR);
    }

    #[test]
    fn blit_pixmap_fully_offscreen_is_a_noop() {
        let src = filled(2, 2, [1, 2, 3, 255]);
        let mut dst = filled(4, 4, CLEAR);
        blit_pixmap(&src, &mut dst, 100, 100, 1.0);
        blit_pixmap(&src, &mut dst, -100, -100, 1.0);
        assert!(dst.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn blit_pixmap_into_smaller_dst_clips_instead_of_panicking() {
        let src = filled(8, 8, [1, 2, 3, 255]);
        let mut dst = filled(2, 2, CLEAR);
        blit_pixmap(&src, &mut dst, 0, 0, 1.0);
        assert_eq!(get(&dst, 1, 1), [1, 2, 3, 255]);
    }

    #[test]
    fn blit_pixmap_extreme_positions_do_not_panic() {
        // Every source pixel is off-screen, so this must be a no-op — not an
        // `px_x + sx` overflow.
        let src = filled(2, 2, [1, 2, 3, 255]);
        let mut dst = filled(4, 4, CLEAR);
        blit_pixmap(&src, &mut dst, i32::MAX, 0, 1.0);
        blit_pixmap(&src, &mut dst, 0, i32::MAX, 1.0);
        blit_pixmap(&src, &mut dst, i32::MIN, i32::MIN, 1.0);
        assert!(dst.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn blit_pixmap_zero_sized_src_is_a_noop() {
        let src = zero_sized();
        let mut dst = filled(2, 2, WHITE);
        blit_pixmap(&src, &mut dst, 0, 0, 1.0);
        assert!(dst.data().iter().all(|&b| b == 255));
    }

    // ==================================================================
    // shift_pixbuf (numeric)
    // ==================================================================

    #[test]
    fn shift_pixbuf_zero_delta_is_a_noop() {
        let mut p = marked(3, 3);
        let before = p.data().to_vec();
        shift_pixbuf(&mut p, 0, 0);
        assert_eq!(p.data(), &before[..]);
    }

    #[test]
    fn shift_pixbuf_right_moves_columns_and_clears_the_left() {
        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, 1, 0);
        assert_eq!(get(&p, 0, 0), CLEAR, "the exposed column must be cleared");
        assert_eq!(get(&p, 1, 0), [0, 0, 0, 255]);
        assert_eq!(get(&p, 2, 0), [1, 0, 0, 255]);
        assert_eq!(get(&p, 1, 2), [6, 0, 0, 255]);
    }

    #[test]
    fn shift_pixbuf_left_moves_columns_and_clears_the_right() {
        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, -1, 0);
        assert_eq!(get(&p, 0, 0), [1, 0, 0, 255]);
        assert_eq!(get(&p, 1, 0), [2, 0, 0, 255]);
        assert_eq!(get(&p, 2, 0), CLEAR);
        assert_eq!(get(&p, 0, 2), [7, 0, 0, 255]);
    }

    #[test]
    fn shift_pixbuf_down_moves_rows_and_clears_the_top() {
        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, 0, 1);
        assert_eq!(get(&p, 0, 0), CLEAR);
        assert_eq!(get(&p, 0, 1), [0, 0, 0, 255]);
        assert_eq!(get(&p, 1, 1), [1, 0, 0, 255]);
        assert_eq!(get(&p, 0, 2), [3, 0, 0, 255]);
    }

    #[test]
    fn shift_pixbuf_up_moves_rows_and_clears_the_bottom() {
        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, 0, -1);
        assert_eq!(get(&p, 0, 0), [3, 0, 0, 255]);
        assert_eq!(get(&p, 0, 1), [6, 0, 0, 255]);
        assert_eq!(get(&p, 0, 2), CLEAR);
        assert_eq!(get(&p, 2, 2), CLEAR);
    }

    #[test]
    fn shift_pixbuf_diagonal_composes_both_axes() {
        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, 1, 1);
        assert_eq!(get(&p, 1, 1), [0, 0, 0, 255]);
        assert_eq!(get(&p, 2, 2), [4, 0, 0, 255]);
        assert_eq!(get(&p, 0, 0), CLEAR);
        assert_eq!(get(&p, 2, 0), CLEAR);
        assert_eq!(get(&p, 0, 2), CLEAR);
    }

    #[test]
    fn shift_pixbuf_by_exactly_the_size_clears_everything() {
        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, 3, 0);
        assert!(p.data().iter().all(|&b| b == 0));

        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, 0, -3);
        assert!(p.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn shift_pixbuf_beyond_the_size_clears_everything() {
        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, 100, 100);
        assert!(p.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn shift_pixbuf_i32_max_clears_everything() {
        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, i32::MAX, i32::MAX);
        assert!(p.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn shift_pixbuf_i32_min_does_not_panic() {
        // `dx.abs()` on i32::MIN overflows; the buffer is entirely exposed, so
        // the documented outcome is "clear everything".
        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, i32::MIN, 0);
        assert!(p.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn shift_pixbuf_i32_min_dy_does_not_panic() {
        let mut p = marked(3, 3);
        shift_pixbuf(&mut p, 0, i32::MIN);
        assert!(p.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn shift_pixbuf_on_zero_sized_pixmap_does_not_panic() {
        let mut p = zero_sized();
        shift_pixbuf(&mut p, 1, 1);
        assert!(p.data().is_empty());
    }

    #[test]
    fn shift_pixbuf_preserves_the_buffer_length() {
        let mut p = marked(4, 4);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (3, 3), (-3, -3)] {
            shift_pixbuf(&mut p, dx, dy);
            assert_eq!(p.data().len(), 4 * 4 * 4);
        }
    }

    // ==================================================================
    // blit_buffer (numeric)
    // ==================================================================

    #[test]
    fn blit_buffer_opaque_src_overwrites_dst() {
        let src = vec![10u8, 20, 30, 255, 40, 50, 60, 255];
        let mut dst = filled(4, 1, CLEAR);
        blit_buffer(&mut dst, &src, 2, 1, 1, 0);
        assert_eq!(get(&dst, 0, 0), CLEAR);
        assert_eq!(get(&dst, 1, 0), [10, 20, 30, 255]);
        assert_eq!(get(&dst, 2, 0), [40, 50, 60, 255]);
        assert_eq!(get(&dst, 3, 0), CLEAR);
    }

    #[test]
    fn blit_buffer_transparent_src_is_a_noop() {
        let src = vec![10u8, 20, 30, 0];
        let mut dst = filled(1, 1, WHITE);
        blit_buffer(&mut dst, &src, 1, 1, 0, 0);
        assert_eq!(get(&dst, 0, 0), WHITE);
    }

    #[test]
    fn blit_buffer_premultiplied_half_alpha_blend_is_exact() {
        // src RGB is already premultiplied, so dst = src + dst * (255 - sa) / 255.
        let src = vec![100u8, 50, 25, 128];
        let mut dst = filled(1, 1, CLEAR);
        blit_buffer(&mut dst, &src, 1, 1, 0, 0);
        assert_eq!(get(&dst, 0, 0), [100, 50, 25, 128]);
    }

    #[test]
    fn blit_buffer_premultiplied_blend_saturates_instead_of_wrapping() {
        // src is (illegally) not premultiplied: 255 + white*127/255 would exceed
        // u8 — it must clamp to 255, not wrap to a dark pixel.
        let src = vec![255u8, 255, 255, 128];
        let mut dst = filled(1, 1, WHITE);
        blit_buffer(&mut dst, &src, 1, 1, 0, 0);
        assert_eq!(get(&dst, 0, 0), WHITE);
    }

    #[test]
    fn blit_buffer_negative_offset_clips() {
        let src = vec![1u8, 1, 1, 255, 2, 2, 2, 255, 3, 3, 3, 255, 4, 4, 4, 255];
        let mut dst = filled(2, 2, CLEAR);
        blit_buffer(&mut dst, &src, 2, 2, -1, -1);
        // Only src(1,1) lands on dst(0,0).
        assert_eq!(get(&dst, 0, 0), [4, 4, 4, 255]);
        assert_eq!(get(&dst, 1, 1), CLEAR);
    }

    #[test]
    fn blit_buffer_fully_offscreen_is_a_noop() {
        let src = vec![9u8, 9, 9, 255];
        let mut dst = filled(2, 2, CLEAR);
        blit_buffer(&mut dst, &src, 1, 1, 50, 50);
        blit_buffer(&mut dst, &src, 1, 1, -50, -50);
        assert!(dst.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn blit_buffer_src_shorter_than_its_declared_size_is_skipped_not_panic() {
        // Claims 4x4 but only carries 2 pixels — the bounds guard must skip the rest.
        let src = vec![7u8, 7, 7, 255, 8, 8, 8, 255];
        let mut dst = filled(4, 4, CLEAR);
        blit_buffer(&mut dst, &src, 4, 4, 0, 0);
        assert_eq!(get(&dst, 0, 0), [7, 7, 7, 255]);
        assert_eq!(get(&dst, 1, 0), [8, 8, 8, 255]);
        assert_eq!(get(&dst, 2, 0), CLEAR);
        assert_eq!(get(&dst, 3, 3), CLEAR);
    }

    #[test]
    fn blit_buffer_empty_src_is_a_noop() {
        let mut dst = filled(2, 2, WHITE);
        blit_buffer(&mut dst, &[], 2, 2, 0, 0);
        assert!(dst.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn blit_buffer_zero_src_dimensions_are_a_noop() {
        let src = vec![9u8, 9, 9, 255];
        let mut dst = filled(2, 2, WHITE);
        blit_buffer(&mut dst, &src, 0, 0, 0, 0);
        blit_buffer(&mut dst, &src, 0, 1, 0, 0);
        blit_buffer(&mut dst, &src, 1, 0, 0, 0);
        assert!(dst.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn blit_buffer_u32_max_src_dimensions_are_a_noop() {
        // `src_w as i32` is -1, so both loops are empty.
        let src = vec![9u8, 9, 9, 255];
        let mut dst = filled(2, 2, WHITE);
        blit_buffer(&mut dst, &src, u32::MAX, u32::MAX, 0, 0);
        assert!(dst.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn blit_buffer_extreme_offsets_do_not_panic() {
        let src = vec![9u8; 2 * 2 * 4];
        let mut dst = filled(4, 4, CLEAR);
        blit_buffer(&mut dst, &src, 2, 2, i32::MAX, 0);
        blit_buffer(&mut dst, &src, 2, 2, 0, i32::MAX);
        blit_buffer(&mut dst, &src, 2, 2, i32::MIN, i32::MIN);
        assert!(dst.data().iter().all(|&b| b == 0));
    }

    // ==================================================================
    // snapshot_region / write_region (numeric + round-trip)
    // ==================================================================

    #[test]
    fn snapshot_region_copies_the_requested_box() {
        let p = marked(4, 4);
        let snap = snapshot_region(&p, 1, 1, 2, 2);
        assert_eq!(snap.len(), 2 * 2 * 4);
        assert_eq!(&snap[0..4], &[5, 0, 0, 255]); // (1,1)
        assert_eq!(&snap[4..8], &[6, 0, 0, 255]); // (2,1)
        assert_eq!(&snap[8..12], &[9, 0, 0, 255]); // (1,2)
        assert_eq!(&snap[12..16], &[10, 0, 0, 255]); // (2,2)
    }

    #[test]
    fn snapshot_region_zero_size_is_an_empty_vec() {
        let p = marked(4, 4);
        assert!(snapshot_region(&p, 0, 0, 0, 0).is_empty());
        assert!(snapshot_region(&p, 0, 0, 4, 0).is_empty());
        assert!(snapshot_region(&p, 0, 0, 0, 4).is_empty());
    }

    #[test]
    fn snapshot_region_out_of_bounds_pixels_are_zero_filled() {
        let p = filled(2, 2, WHITE);
        let snap = snapshot_region(&p, 1, 1, 2, 2);
        assert_eq!(snap.len(), 16);
        assert_eq!(&snap[0..4], &WHITE, "only (1,1) is inside the pixmap");
        assert_eq!(&snap[4..8], &CLEAR);
        assert_eq!(&snap[8..12], &CLEAR);
        assert_eq!(&snap[12..16], &CLEAR);
    }

    #[test]
    fn snapshot_region_negative_origin_is_partially_zero_filled() {
        let p = marked(2, 2);
        let snap = snapshot_region(&p, -1, -1, 2, 2);
        assert_eq!(&snap[0..4], &CLEAR);
        assert_eq!(&snap[4..8], &CLEAR);
        assert_eq!(&snap[8..12], &CLEAR);
        assert_eq!(&snap[12..16], &[0, 0, 0, 255], "pixel (0,0) of the source");
    }

    #[test]
    fn snapshot_region_fully_offscreen_is_all_zeroes() {
        let p = filled(4, 4, WHITE);
        let snap = snapshot_region(&p, 100, 100, 2, 2);
        assert!(snap.iter().all(|&b| b == 0));
    }

    #[test]
    fn snapshot_region_extreme_origin_does_not_panic() {
        // Every sampled pixel is off-screen, so the result must be all zeroes —
        // not an `x + px` overflow.
        let p = filled(4, 4, WHITE);
        let snap = snapshot_region(&p, i32::MAX, 0, 2, 2);
        assert!(snap.iter().all(|&b| b == 0));
        let snap = snapshot_region(&p, 0, i32::MAX, 2, 2);
        assert!(snap.iter().all(|&b| b == 0));
        let snap = snapshot_region(&p, i32::MIN, i32::MIN, 2, 2);
        assert!(snap.iter().all(|&b| b == 0));
    }

    #[test]
    fn write_region_overwrites_without_blending() {
        // Unlike blit_buffer, a fully transparent src must still clobber dst.
        let src = vec![0u8; 4];
        let mut dst = filled(2, 2, WHITE);
        write_region(&mut dst, &src, 1, 1, 0, 0);
        assert_eq!(get(&dst, 0, 0), CLEAR, "write_region is a direct copy");
        assert_eq!(get(&dst, 1, 1), WHITE);
    }

    #[test]
    fn write_region_places_pixels_at_the_offset() {
        let src = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut dst = filled(4, 1, CLEAR);
        write_region(&mut dst, &src, 2, 1, 2, 0);
        assert_eq!(get(&dst, 2, 0), [1, 2, 3, 4]);
        assert_eq!(get(&dst, 3, 0), [5, 6, 7, 8]);
        assert_eq!(get(&dst, 0, 0), CLEAR);
    }

    #[test]
    fn write_region_out_of_bounds_pixels_are_skipped() {
        let src = vec![9u8; 2 * 2 * 4];
        let mut dst = filled(2, 2, CLEAR);
        write_region(&mut dst, &src, 2, 2, 1, 1);
        assert_eq!(get(&dst, 1, 1), [9, 9, 9, 9]);
        assert_eq!(get(&dst, 0, 0), CLEAR);
    }

    #[test]
    fn write_region_negative_coords_clip() {
        let src = vec![1u8, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
        let mut dst = filled(2, 2, CLEAR);
        write_region(&mut dst, &src, 2, 2, -1, -1);
        assert_eq!(get(&dst, 0, 0), [4, 4, 4, 4]);
        assert_eq!(get(&dst, 1, 1), CLEAR);
    }

    #[test]
    fn write_region_short_src_is_skipped_not_panic() {
        let src = vec![7u8, 7, 7, 7];
        let mut dst = filled(4, 4, CLEAR);
        write_region(&mut dst, &src, 4, 4, 0, 0);
        assert_eq!(get(&dst, 0, 0), [7, 7, 7, 7]);
        assert_eq!(get(&dst, 1, 0), CLEAR);
    }

    #[test]
    fn write_region_zero_dimensions_are_a_noop() {
        let src = vec![9u8; 16];
        let mut dst = filled(2, 2, WHITE);
        write_region(&mut dst, &src, 0, 0, 0, 0);
        assert!(dst.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn write_region_u32_max_dimensions_are_a_noop() {
        let src = vec![9u8; 16];
        let mut dst = filled(2, 2, WHITE);
        write_region(&mut dst, &src, u32::MAX, u32::MAX, 0, 0);
        assert!(dst.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn write_region_extreme_coords_do_not_panic() {
        let src = vec![9u8; 2 * 2 * 4];
        let mut dst = filled(4, 4, CLEAR);
        write_region(&mut dst, &src, 2, 2, i32::MAX, 0);
        write_region(&mut dst, &src, 2, 2, 0, i32::MAX);
        write_region(&mut dst, &src, 2, 2, i32::MIN, i32::MIN);
        assert!(dst.data().iter().all(|&b| b == 0));
    }

    #[test]
    fn snapshot_region_then_write_region_round_trips() {
        let mut p = marked(6, 6);
        let snap = snapshot_region(&p, 1, 1, 3, 3);
        let expected = p.data().to_vec();
        p.fill_rect(1, 1, 3, 3, 0, 0, 0, 0); // destroy the region
        assert_eq!(get(&p, 2, 2), CLEAR);
        write_region(&mut p, &snap, 3, 3, 1, 1); // restore it
        assert_eq!(p.data(), &expected[..], "write_region must invert snapshot_region");
    }

    // ==================================================================
    // agg_fill_path / agg_fill_path_clipped (other + numeric)
    // ==================================================================

    #[test]
    fn agg_fill_path_paints_the_path_interior_only() {
        let mut p = filled(10, 10, WHITE);
        let mut path = rect_path(2.0, 2.0, 6.0, 6.0);
        agg_fill_path(&mut p, &mut path, &red(), FillingRule::NonZero);
        assert_eq!(get(&p, 5, 5), [255, 0, 0, 255], "interior is fully covered");
        assert_eq!(get(&p, 0, 0), WHITE, "outside the path is untouched");
        assert_eq!(get(&p, 9, 9), WHITE);
    }

    #[test]
    fn agg_fill_path_empty_path_paints_nothing() {
        let mut p = filled(8, 8, WHITE);
        let mut path = PathStorage::new();
        agg_fill_path(&mut p, &mut path, &red(), FillingRule::NonZero);
        assert!(p.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn agg_fill_path_on_a_1x1_pixmap_does_not_panic() {
        let mut p = filled(1, 1, WHITE);
        let mut path = rect_path(0.0, 0.0, 1.0, 1.0);
        agg_fill_path(&mut p, &mut path, &red(), FillingRule::NonZero);
        assert_eq!(get(&p, 0, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn agg_fill_path_offscreen_path_paints_nothing() {
        let mut p = filled(8, 8, WHITE);
        let mut path = rect_path(100.0, 100.0, 10.0, 10.0);
        agg_fill_path(&mut p, &mut path, &red(), FillingRule::NonZero);
        assert!(p.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn agg_fill_path_nan_coordinates_do_not_panic() {
        let mut p = filled(8, 8, WHITE);
        let mut path = PathStorage::new();
        path.move_to(f64::NAN, f64::NAN);
        path.line_to(4.0, f64::NAN);
        path.line_to(f64::NAN, 4.0);
        path.close_polygon(0);
        agg_fill_path(&mut p, &mut path, &red(), FillingRule::NonZero);
        assert_eq!(p.data().len(), 8 * 8 * 4, "the buffer must stay intact");
    }

    #[test]
    fn agg_fill_path_huge_coordinates_do_not_panic() {
        let mut p = filled(8, 8, WHITE);
        let mut path = rect_path(-1.0e30, -1.0e30, 2.0e30, 2.0e30);
        agg_fill_path(&mut p, &mut path, &red(), FillingRule::NonZero);
        assert_eq!(p.data().len(), 8 * 8 * 4);
    }

    #[test]
    fn agg_fill_path_infinite_coordinates_do_not_panic() {
        let mut p = filled(8, 8, WHITE);
        let mut path = PathStorage::new();
        path.move_to(f64::NEG_INFINITY, f64::NEG_INFINITY);
        path.line_to(f64::INFINITY, f64::NEG_INFINITY);
        path.line_to(f64::INFINITY, f64::INFINITY);
        path.close_polygon(0);
        agg_fill_path(&mut p, &mut path, &red(), FillingRule::NonZero);
        assert_eq!(p.data().len(), 8 * 8 * 4);
    }

    #[test]
    fn agg_fill_path_clipped_none_clip_equals_unclipped() {
        let mut a = filled(10, 10, WHITE);
        let mut b = filled(10, 10, WHITE);
        let mut pa = rect_path(1.0, 1.0, 5.0, 5.0);
        let mut pb = rect_path(1.0, 1.0, 5.0, 5.0);
        agg_fill_path(&mut a, &mut pa, &red(), FillingRule::NonZero);
        agg_fill_path_clipped(&mut b, &mut pb, &red(), FillingRule::NonZero, None);
        assert_eq!(a.data(), b.data());
    }

    #[test]
    fn agg_fill_path_clipped_restricts_output_to_the_clip_box() {
        let mut p = filled(10, 10, WHITE);
        let mut path = rect_path(0.0, 0.0, 10.0, 10.0); // the whole canvas
        let clip = AzRect::from_xywh(2.0, 2.0, 3.0, 3.0).expect("valid");
        agg_fill_path_clipped(&mut p, &mut path, &red(), FillingRule::NonZero, Some(clip));
        assert_eq!(get(&p, 3, 3), [255, 0, 0, 255], "inside the clip");
        assert_eq!(get(&p, 6, 6), WHITE, "outside the clip");
        assert_eq!(get(&p, 0, 0), WHITE);
    }

    #[test]
    fn agg_fill_path_clipped_offscreen_clip_paints_nothing() {
        let mut p = filled(10, 10, WHITE);
        let mut path = rect_path(0.0, 0.0, 10.0, 10.0);
        let clip = AzRect::from_xywh(100.0, 100.0, 5.0, 5.0).expect("valid");
        agg_fill_path_clipped(&mut p, &mut path, &red(), FillingRule::NonZero, Some(clip));
        assert!(
            p.data().iter().all(|&b| b == 255),
            "a clip box outside the buffer must reject everything"
        );
    }

    #[test]
    fn agg_fill_path_clipped_empty_clip_paints_nothing() {
        // `intersect_clips` returns a zero-area rect for non-overlapping nested
        // clips, and documents that it must clip EVERYTHING. Nothing may leak.
        let outer = AzRect::from_xywh(0.0, 0.0, 4.0, 4.0).expect("valid");
        let inner = AzRect::from_xywh(10.0, 10.0, 4.0, 4.0).expect("valid");
        let empty = intersect_clips(Some(outer), Some(inner)).expect("stays Some");
        assert!(approx(empty.width, 0.0) && approx(empty.height, 0.0));

        let mut p = filled(20, 20, WHITE);
        let mut path = rect_path(0.0, 0.0, 20.0, 20.0);
        agg_fill_path_clipped(&mut p, &mut path, &red(), FillingRule::NonZero, Some(empty));
        let painted = painted_count(&p);
        assert_eq!(painted, 0, "an empty clip leaked {painted} painted pixel(s)");
    }

    #[test]
    fn agg_fill_path_evenodd_leaves_the_hole_of_a_donut_unpainted() {
        let mut p = filled(12, 12, WHITE);
        let mut path = PathStorage::new();
        // outer ring
        path.move_to(1.0, 1.0);
        path.line_to(11.0, 1.0);
        path.line_to(11.0, 11.0);
        path.line_to(1.0, 11.0);
        path.close_polygon(0);
        // inner ring (same winding — only EvenOdd punches a hole)
        path.move_to(4.0, 4.0);
        path.line_to(8.0, 4.0);
        path.line_to(8.0, 8.0);
        path.line_to(4.0, 8.0);
        path.close_polygon(0);
        agg_fill_path(&mut p, &mut path, &red(), FillingRule::EvenOdd);
        assert_eq!(get(&p, 2, 2), [255, 0, 0, 255], "the ring is painted");
        assert_eq!(get(&p, 6, 6), WHITE, "the hole is not");
    }

    // ==================================================================
    // agg_fill_transformed_path / _clipped (other + numeric)
    // ==================================================================

    #[test]
    fn agg_fill_transformed_path_identity_matches_the_untransformed_fill() {
        let mut a = filled(10, 10, WHITE);
        let mut b = filled(10, 10, WHITE);
        let mut pa = rect_path(2.0, 2.0, 4.0, 4.0);
        let mut pb = rect_path(2.0, 2.0, 4.0, 4.0);
        agg_fill_path(&mut a, &mut pa, &red(), FillingRule::NonZero);
        agg_fill_transformed_path(&mut b, &mut pb, &red(), FillingRule::NonZero, &TransAffine::new());
        assert_eq!(a.data(), b.data(), "an identity transform must be a no-op");
    }

    #[test]
    fn agg_fill_transformed_path_translation_moves_the_output() {
        let mut p = filled(12, 12, WHITE);
        let mut path = rect_path(0.0, 0.0, 3.0, 3.0);
        let t = TransAffine::new_translation(6.0, 0.0);
        agg_fill_transformed_path(&mut p, &mut path, &red(), FillingRule::NonZero, &t);
        assert_eq!(get(&p, 7, 1), [255, 0, 0, 255], "moved right by 6");
        assert_eq!(get(&p, 1, 1), WHITE, "the original position is empty");
    }

    #[test]
    fn agg_fill_transformed_path_zero_scale_does_not_panic() {
        let mut p = filled(8, 8, WHITE);
        let mut path = rect_path(1.0, 1.0, 4.0, 4.0);
        let t = TransAffine::new_scaling(0.0, 0.0);
        agg_fill_transformed_path(&mut p, &mut path, &red(), FillingRule::NonZero, &t);
        assert!(
            p.data().iter().all(|&b| b == 255),
            "a degenerate transform collapses the path to a point"
        );
    }

    #[test]
    fn agg_fill_transformed_path_nan_transform_does_not_panic() {
        let mut p = filled(8, 8, WHITE);
        let mut path = rect_path(1.0, 1.0, 4.0, 4.0);
        let t = TransAffine::new_scaling(f64::NAN, 1.0);
        agg_fill_transformed_path(&mut p, &mut path, &red(), FillingRule::NonZero, &t);
        assert_eq!(p.data().len(), 8 * 8 * 4);
    }

    #[test]
    fn agg_fill_transformed_path_clipped_applies_the_clip_after_the_transform() {
        let mut p = filled(12, 12, WHITE);
        let mut path = rect_path(0.0, 0.0, 3.0, 3.0);
        let t = TransAffine::new_translation(6.0, 0.0);
        // The clip box covers the ORIGINAL position, not the transformed one.
        let clip = AzRect::from_xywh(0.0, 0.0, 3.0, 3.0).expect("valid");
        agg_fill_transformed_path_clipped(
            &mut p,
            &mut path,
            &red(),
            FillingRule::NonZero,
            &t,
            Some(clip),
        );
        assert!(
            p.data().iter().all(|&b| b == 255),
            "the translated path lies outside the clip box"
        );
    }

    #[test]
    fn agg_fill_transformed_path_clipped_empty_clip_paints_nothing() {
        let outer = AzRect::from_xywh(0.0, 0.0, 4.0, 4.0).expect("valid");
        let inner = AzRect::from_xywh(10.0, 10.0, 4.0, 4.0).expect("valid");
        let empty = intersect_clips(Some(outer), Some(inner)).expect("stays Some");

        let mut p = filled(20, 20, WHITE);
        let mut path = rect_path(0.0, 0.0, 20.0, 20.0);
        agg_fill_transformed_path_clipped(
            &mut p,
            &mut path,
            &red(),
            FillingRule::NonZero,
            &TransAffine::new(),
            Some(empty),
        );
        assert!(
            p.data().iter().all(|&b| b == 255),
            "an empty clip must clip everything, even on the identity fast path"
        );
    }

    // ==================================================================
    // agg_fill_gradient / agg_fill_gradient_clipped (numeric)
    // ==================================================================

    #[test]
    fn agg_fill_gradient_paints_the_path() {
        let mut p = filled(10, 10, WHITE);
        let mut path = rect_path(0.0, 0.0, 10.0, 10.0);
        let lut = two_stop_lut();
        agg_fill_gradient(
            &mut p,
            &mut path,
            &lut,
            GradientX,
            TransAffine::new(),
            0.0,
            10.0,
        );
        assert_ne!(get(&p, 5, 5), WHITE, "the gradient must paint something");
        assert_eq!(get(&p, 5, 5)[3], 255, "opaque stops produce opaque pixels");
    }

    #[test]
    fn agg_fill_gradient_empty_path_paints_nothing() {
        let mut p = filled(8, 8, WHITE);
        let mut path = PathStorage::new();
        let lut = two_stop_lut();
        agg_fill_gradient(
            &mut p,
            &mut path,
            &lut,
            GradientX,
            TransAffine::new(),
            0.0,
            8.0,
        );
        assert!(p.data().iter().all(|&b| b == 255));
    }

    #[test]
    fn agg_fill_gradient_zero_length_d1_eq_d2_does_not_panic() {
        // d2 - d1 == 0 must not divide by zero.
        let mut p = filled(8, 8, WHITE);
        let mut path = rect_path(0.0, 0.0, 8.0, 8.0);
        let lut = two_stop_lut();
        agg_fill_gradient_clipped(
            &mut p,
            &mut path,
            &lut,
            GradientX,
            TransAffine::new(),
            5.0,
            5.0,
            None,
        );
        assert_eq!(p.data().len(), 8 * 8 * 4);
    }

    #[test]
    fn agg_fill_gradient_reversed_d2_lt_d1_does_not_panic() {
        let mut p = filled(8, 8, WHITE);
        let mut path = rect_path(0.0, 0.0, 8.0, 8.0);
        let lut = two_stop_lut();
        agg_fill_gradient_clipped(
            &mut p,
            &mut path,
            &lut,
            GradientX,
            TransAffine::new(),
            8.0,
            0.0,
            None,
        );
        assert_eq!(p.data().len(), 8 * 8 * 4);
    }

    #[test]
    fn agg_fill_gradient_nan_and_inf_distances_do_not_panic() {
        let lut = two_stop_lut();
        for (d1, d2) in [
            (f64::NAN, f64::NAN),
            (0.0, f64::NAN),
            (f64::NEG_INFINITY, f64::INFINITY),
            (0.0, f64::MAX),
            (f64::MIN, f64::MAX),
        ] {
            let mut p = filled(8, 8, WHITE);
            let mut path = rect_path(0.0, 0.0, 8.0, 8.0);
            agg_fill_gradient_clipped(
                &mut p,
                &mut path,
                &lut,
                GradientX,
                TransAffine::new(),
                d1,
                d2,
                None,
            );
            assert_eq!(p.data().len(), 8 * 8 * 4, "d1={d1}, d2={d2}");
        }
    }

    #[test]
    fn agg_fill_gradient_nan_transform_does_not_panic() {
        let mut p = filled(8, 8, WHITE);
        let mut path = rect_path(0.0, 0.0, 8.0, 8.0);
        let lut = two_stop_lut();
        agg_fill_gradient_clipped(
            &mut p,
            &mut path,
            &lut,
            GradientX,
            TransAffine::new_scaling(f64::NAN, f64::NAN),
            0.0,
            8.0,
            None,
        );
        assert_eq!(p.data().len(), 8 * 8 * 4);
    }

    #[test]
    fn agg_fill_gradient_clipped_restricts_output_to_the_clip_box() {
        let mut p = filled(10, 10, WHITE);
        let mut path = rect_path(0.0, 0.0, 10.0, 10.0);
        let lut = two_stop_lut();
        let clip = AzRect::from_xywh(0.0, 0.0, 3.0, 3.0).expect("valid");
        agg_fill_gradient_clipped(
            &mut p,
            &mut path,
            &lut,
            GradientX,
            TransAffine::new(),
            0.0,
            10.0,
            Some(clip),
        );
        assert_ne!(get(&p, 1, 1), WHITE, "inside the clip");
        assert_eq!(get(&p, 8, 8), WHITE, "outside the clip");
    }

    #[test]
    fn agg_fill_gradient_clipped_empty_clip_paints_nothing() {
        let outer = AzRect::from_xywh(0.0, 0.0, 4.0, 4.0).expect("valid");
        let inner = AzRect::from_xywh(10.0, 10.0, 4.0, 4.0).expect("valid");
        let empty = intersect_clips(Some(outer), Some(inner)).expect("stays Some");

        let mut p = filled(20, 20, WHITE);
        let mut path = rect_path(0.0, 0.0, 20.0, 20.0);
        let lut = two_stop_lut();
        agg_fill_gradient_clipped(
            &mut p,
            &mut path,
            &lut,
            GradientX,
            TransAffine::new(),
            0.0,
            20.0,
            Some(empty),
        );
        let painted = painted_count(&p);
        assert_eq!(painted, 0, "an empty clip leaked {painted} painted pixel(s)");
    }

    #[test]
    fn agg_fill_gradient_on_a_1x1_pixmap_does_not_panic() {
        let mut p = filled(1, 1, WHITE);
        let mut path = rect_path(0.0, 0.0, 1.0, 1.0);
        let lut = two_stop_lut();
        agg_fill_gradient(&mut p, &mut path, &lut, GradientX, TransAffine::new(), 0.0, 1.0);
        assert_eq!(p.data().len(), 4);
    }
}

