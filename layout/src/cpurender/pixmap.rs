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

pub(crate) const IDENTITY_EPSILON_F64: f64 = 0.0001;

/// Compute the intersection of two logical rects.
pub(crate) fn rect_intersection(a: &LogicalRect, b: &LogicalRect) -> Option<LogicalRect> {
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

/// Blit `src` onto `dst` at pixel position (px_x, px_y) with opacity.
pub(crate) fn blit_pixmap(src: &AzulPixmap, dst: &mut AzulPixmap, px_x: i32, px_y: i32, opacity: f32) {
    let sw = src.width as i32;
    let sh = src.height as i32;
    let dw = dst.width as i32;
    let dh = dst.height as i32;
    let op = (opacity * 255.0).clamp(0.0, 255.0) as u32;

    for sy in 0..sh {
        let dy = px_y + sy;
        if dy < 0 || dy >= dh {
            continue;
        }
        for sx in 0..sw {
            let dx = px_x + sx;
            if dx < 0 || dx >= dw {
                continue;
            }
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((dy * dw + dx) * 4) as usize;
            if si + 3 >= src.data.len() || di + 3 >= dst.data.len() {
                continue;
            }

            let sr = src.data[si] as u32;
            let sg = src.data[si + 1] as u32;
            let sb = src.data[si + 2] as u32;
            let sa = (src.data[si + 3] as u32 * op) / 255;

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
                dst.data[di] = ((sr * sa + dst.data[di] as u32 * inv_sa) / 255) as u8;
                dst.data[di + 1] = ((sg * sa + dst.data[di + 1] as u32 * inv_sa) / 255) as u8;
                dst.data[di + 2] = ((sb * sa + dst.data[di + 2] as u32 * inv_sa) / 255) as u8;
                dst.data[di + 3] = ((sa + dst.data[di + 3] as u32 * inv_sa / 255).min(255)) as u8;
            }
        }
    }
}

/// Shift pixel data in a pixmap by (dx, dy) pixels, clearing exposed regions.
pub(crate) fn shift_pixbuf(pixmap: &mut AzulPixmap, dx: i32, dy: i32) {
    let w = pixmap.width as i32;
    let h = pixmap.height as i32;
    if dx.abs() >= w || dy.abs() >= h {
        // Entire buffer is exposed — just clear it
        pixmap.fill(0, 0, 0, 0);
        return;
    }

    let stride = (w * 4) as usize;
    let data = &mut pixmap.data;

    // Shift rows vertically
    if dy > 0 {
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
    } else if dy < 0 {
        let ady = (-dy) as i32;
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

    // Shift columns horizontally
    if dx > 0 {
        for row in 0..h {
            let row_start = (row * w * 4) as usize;
            let shift = (dx * 4) as usize;
            // Shift right within the row
            data.copy_within(row_start..row_start + stride - shift, row_start + shift);
            // Clear left columns
            data[row_start..row_start + shift].fill(0);
        }
    } else if dx < 0 {
        let adx = (-dx * 4) as usize;
        for row in 0..h {
            let row_start = (row * w * 4) as usize;
            data.copy_within(row_start + adx..row_start + stride, row_start);
            // Clear right columns
            data[row_start + stride - adx..row_start + stride].fill(0);
        }
    }
}

/// A simple RGBA pixel buffer. Replaces tiny_skia::Pixmap.
pub struct AzulPixmap {
    pub(crate) data: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl AzulPixmap {
    /// Create a new pixmap filled with opaque white.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let len = (width as usize) * (height as usize) * 4;
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
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8, a: u8) {
        let pw = self.width as i32;
        let ph = self.height as i32;
        let x0 = x.max(0).min(pw);
        let y0 = y.max(0).min(ph);
        // saturating: a non-finite/huge layout size casts to i32::MAX, and `x + w`
        // would then overflow (debug panic). Clamp instead.
        let x1 = x.saturating_add(w).max(0).min(pw);
        let y1 = y.saturating_add(h).max(0).min(ph);
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
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Mutable raw RGBA pixel data.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Create a clone of this pixmap (for filter application).
    pub fn clone_pixmap(&self) -> Self {
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
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|e| format!("PNG header error: {}", e))?;
            writer
                .write_image_data(&self.data)
                .map_err(|e| format!("PNG write error: {}", e))?;
        }
        Ok(buf)
    }

    /// Decode a PNG byte slice into an AzulPixmap.
    pub fn decode_png(png_bytes: &[u8]) -> Result<Self, String> {
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("PNG decode error: {}", e))?;
        let buf_size = reader
            .output_buffer_size()
            .ok_or_else(|| "PNG: unknown output buffer size".to_string())?;
        let mut buf = vec![0u8; buf_size];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| format!("PNG frame error: {}", e))?;
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
            other => return Err(format!("Unsupported PNG color type: {:?}", other)),
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
#[derive(Debug, Clone)]
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
    pub fn is_match(&self) -> bool {
        self.dimensions_match && self.diff_count == 0
    }

    /// Fraction of pixels that differ (0.0 = identical, 1.0 = all different).
    pub fn diff_ratio(&self) -> f64 {
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
pub fn pixel_diff(reference: &AzulPixmap, test: &AzulPixmap, threshold: u8) -> PixelDiffResult {
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

    let total_pixels = (reference.width as u64) * (reference.height as u64);
    let mut diff_count = 0u64;
    let mut max_delta = 0u8;

    for (ref_chunk, test_chunk) in reference
        .data
        .chunks_exact(4)
        .zip(test.data.chunks_exact(4))
    {
        let mut pixel_differs = false;
        for c in 0..4 {
            let delta = (ref_chunk[c] as i16 - test_chunk[c] as i16).unsigned_abs() as u8;
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
pub fn compare_against_reference(
    rendered: &AzulPixmap,
    reference_png_path: &str,
    threshold: u8,
) -> Result<PixelDiffResult, String> {
    let ref_bytes = std::fs::read(reference_png_path)
        .map_err(|e| format!("Cannot read reference image {}: {}", reference_png_path, e))?;
    let reference = AzulPixmap::decode_png(&ref_bytes)?;
    Ok(pixel_diff(&reference, rendered, threshold))
}

// ============================================================================
// Simple rect type (replaces tiny_skia::Rect)
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub(crate) struct AzRect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Intersect a freshly-pushed clip with the currently-active one. `None`
/// means "no clip". An EMPTY intersection clips everything (zero-area rect) —
/// it must NOT degrade to `None`/unclipped, or nested clips could escape
/// their parents.
pub(crate) fn intersect_clips(current: Option<AzRect>, new: Option<AzRect>) -> Option<AzRect> {
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
    pub(crate) fn clip(&self, clip: &AzRect) -> Option<AzRect> {
        let x1 = self.x.max(clip.x);
        let y1 = self.y.max(clip.y);
        let x2 = (self.x + self.width).min(clip.x + clip.width);
        let y2 = (self.y + self.height).min(clip.y + clip.height);
        if x2 > x1 && y2 > y1 {
            Some(AzRect {
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

pub(crate) fn agg_fill_path(
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
pub(crate) fn agg_fill_path_clipped(
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
        rb.clip_box_i(
            c.x as i32,
            c.y as i32,
            (c.x + c.width) as i32 - 1,
            (c.y + c.height) as i32 - 1,
        );
    }
    let mut ras = RasterizerScanlineAa::new();
    ras.filling_rule(rule);
    ras.add_path(path, 0);
    let mut sl = ScanlineU8::new();
    render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, color);
}

fn agg_fill_transformed_path(
    pixmap: &mut AzulPixmap,
    path: &mut PathStorage,
    color: &Rgba8,
    rule: FillingRule,
    transform: &TransAffine,
) {
    agg_fill_transformed_path_clipped(pixmap, path, color, rule, transform, None);
}

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
        let mut transformed = ConvTransform::new(path, transform.clone());
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

pub(crate) fn agg_fill_gradient_clipped<G: GradientFunction>(
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
        rb.clip_box_i(
            c.x as i32,
            c.y as i32,
            (c.x + c.width) as i32 - 1,
            (c.y + c.height) as i32 - 1,
        );
    }
    let mut ras = RasterizerScanlineAa::new();
    ras.filling_rule(FillingRule::NonZero);
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
pub(crate) fn blit_buffer(dst: &mut AzulPixmap, src: &[u8], src_w: u32, src_h: u32, dx: i32, dy: i32) {
    let dw = dst.width as i32;
    let dh = dst.height as i32;

    for py in 0..src_h as i32 {
        let ty = dy + py;
        if ty < 0 || ty >= dh {
            continue;
        }
        for px in 0..src_w as i32 {
            let tx = dx + px;
            if tx < 0 || tx >= dw {
                continue;
            }

            let si = ((py as u32 * src_w + px as u32) * 4) as usize;
            let di = ((ty as u32 * dst.width + tx as u32) * 4) as usize;

            if si + 3 >= src.len() || di + 3 >= dst.data.len() {
                continue;
            }

            let sa = src[si + 3] as u32;
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
                    ((src[si] as u32 + dst.data[di] as u32 * inv_sa / 255).min(255)) as u8;
                dst.data[di + 1] =
                    ((src[si + 1] as u32 + dst.data[di + 1] as u32 * inv_sa / 255).min(255)) as u8;
                dst.data[di + 2] =
                    ((src[si + 2] as u32 + dst.data[di + 2] as u32 * inv_sa / 255).min(255)) as u8;
                dst.data[di + 3] = ((sa + dst.data[di + 3] as u32 * inv_sa / 255).min(255)) as u8;
            }
        }
    }
}

// ============================================================================
// Image mask clipping
// ============================================================================

/// Take a snapshot of a rectangular region of the pixmap.
pub(crate) fn snapshot_region(pixmap: &AzulPixmap, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
    let pw = pixmap.width as i32;
    let ph = pixmap.height as i32;
    let mut snap = vec![0u8; (w as usize) * (h as usize) * 4];

    for py in 0..h as i32 {
        let sy = y + py;
        if sy < 0 || sy >= ph {
            continue;
        }
        for px in 0..w as i32 {
            let sx = x + px;
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

pub fn union_rect(a: &LogicalRect, b: &LogicalRect) -> LogicalRect {
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

pub(crate) fn logical_rect_to_az_rect(bounds: &LogicalRect, dpi_factor: f32) -> Option<AzRect> {
    let x = bounds.origin.x * dpi_factor;
    let y = bounds.origin.y * dpi_factor;
    let width = bounds.size.width * dpi_factor;
    let height = bounds.size.height * dpi_factor;

    AzRect::from_xywh(x, y, width, height)
}

