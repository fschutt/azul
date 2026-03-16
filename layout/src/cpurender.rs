//! CPU rendering for solver3 DisplayList
//!
//! This module renders a flat DisplayList (from solver3) to an AzulPixmap using agg-rust.
//! Unlike the old hierarchical CachedDisplayList, the new DisplayList is a simple
//! flat vector of rendering commands that can be executed sequentially.

use azul_core::{
    dom::ScrollbarOrientation,
    geom::{LogicalPosition, LogicalRect},
    resources::{
        DecodedImage, FontInstanceKey, ImageRef,
        RendererResources,
    },
    ui_solver::GlyphInstance,
};
use azul_css::props::basic::{ColorU, ColorOrSystem, FontRef};

use agg_rust::{
    basics::{FillingRule, VertexSource, PATH_FLAGS_NONE},
    blur::stack_blur_rgba32,
    path_storage::PathStorage,
    color::Rgba8,
    conv_stroke::ConvStroke,
    conv_transform::ConvTransform,
    gradient_lut::GradientLut,
    pixfmt_rgba::{PixfmtRgba32, PixelFormat},
    rasterizer_scanline_aa::RasterizerScanlineAa,
    renderer_base::RendererBase,
    renderer_scanline::{render_scanlines_aa, render_scanlines_aa_solid},
    rendering_buffer::RowAccessor,
    rounded_rect::RoundedRect,
    scanline_u::ScanlineU8,
    span_allocator::SpanAllocator,
    span_gradient::{GradientConic, GradientFunction, GradientRadialD, GradientX, SpanGradient},
    span_interpolator_linear::SpanInterpolatorLinear,
    trans_affine::TransAffine,
};

use crate::{
    font::parsed::ParsedFont,
    glyph_cache::GlyphCache,
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem},
    text3::cache::{FontHash, FontManager},
};

// ============================================================================
// AzulPixmap — replacement for tiny_skia::Pixmap
// ============================================================================

/// A simple RGBA pixel buffer. Replaces tiny_skia::Pixmap.
pub struct AzulPixmap {
    data: Vec<u8>,
    width: u32,
    height: u32,
}

impl AzulPixmap {
    /// Create a new pixmap filled with opaque white.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let len = (width as usize) * (height as usize) * 4;
        let mut data = vec![255u8; len]; // opaque white
        Some(Self { data, width, height })
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

    /// Encode to PNG using the `png` crate.
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut buf, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header()
                .map_err(|e| format!("PNG header error: {}", e))?;
            writer.write_image_data(&self.data)
                .map_err(|e| format!("PNG write error: {}", e))?;
        }
        Ok(buf)
    }
}

// ============================================================================
// Simple rect type (replaces tiny_skia::Rect)
// ============================================================================

#[derive(Debug, Clone, Copy)]
struct AzRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl AzRect {
    fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        if w <= 0.0 || h <= 0.0 || !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
            return None;
        }
        Some(Self { x, y, width: w, height: h })
    }
}

// ============================================================================
// AGG helper: fill a PathStorage with a solid color into an AzulPixmap
// ============================================================================

fn agg_fill_path(
    pixmap: &mut AzulPixmap,
    path: &mut dyn VertexSource,
    color: &Rgba8,
    rule: FillingRule,
) {
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;
    let mut ra = unsafe {
        RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride)
    };
    let mut pf = PixfmtRgba32::new(&mut ra);
    let mut rb = RendererBase::new(pf);
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
    if transform.is_identity(0.0001) {
        agg_fill_path(pixmap, path, color, rule);
    } else {
        let mut transformed = ConvTransform::new(path, transform.clone());
        agg_fill_path(pixmap, &mut transformed, color, rule);
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
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;
    let mut ra = unsafe {
        RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride)
    };
    let mut pf = PixfmtRgba32::new(&mut ra);
    let mut rb = RendererBase::new(pf);
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

/// Resolve a ColorOrSystem to a concrete ColorU (system colors fall back to gray).
fn resolve_color(color: &ColorOrSystem) -> ColorU {
    match color {
        ColorOrSystem::Color(c) => *c,
        ColorOrSystem::System(_) => ColorU { r: 128, g: 128, b: 128, a: 255 },
    }
}

/// Build a GradientLut from normalized linear color stops.
fn build_gradient_lut_linear(
    stops: &azul_css::props::style::background::NormalizedLinearColorStopVec,
) -> GradientLut {
    let mut lut = GradientLut::new_default();
    let stops_slice = stops.as_ref();
    if stops_slice.len() < 2 {
        // Need at least 2 stops; fill with transparent
        lut.add_color(0.0, Rgba8::new(0, 0, 0, 0));
        lut.add_color(1.0, Rgba8::new(0, 0, 0, 0));
        lut.build_lut();
        return lut;
    }
    for stop in stops_slice {
        let offset = stop.offset.normalized() as f64; // 0.0..1.0
        let c = resolve_color(&stop.color);
        lut.add_color(offset, Rgba8::new(c.r as u32, c.g as u32, c.b as u32, c.a as u32));
    }
    lut.build_lut();
    lut
}

/// Build a GradientLut from normalized radial (conic) color stops.
fn build_gradient_lut_radial(
    stops: &azul_css::props::style::background::NormalizedRadialColorStopVec,
) -> GradientLut {
    let mut lut = GradientLut::new_default();
    let stops_slice = stops.as_ref();
    if stops_slice.len() < 2 {
        lut.add_color(0.0, Rgba8::new(0, 0, 0, 0));
        lut.add_color(1.0, Rgba8::new(0, 0, 0, 0));
        lut.build_lut();
        return lut;
    }
    for stop in stops_slice {
        // Conic stops use angle — normalize to 0..1 fraction of full circle
        let offset = (stop.angle.to_degrees() / 360.0).clamp(0.0, 1.0) as f64;
        let c = resolve_color(&stop.color);
        lut.add_color(offset, Rgba8::new(c.r as u32, c.g as u32, c.b as u32, c.a as u32));
    }
    lut.build_lut();
    lut
}

/// Resolve a background position to (x_fraction, y_fraction) in 0..1 range.
fn resolve_background_position(
    pos: &azul_css::props::style::background::StyleBackgroundPosition,
    width: f32,
    height: f32,
) -> (f32, f32) {
    use azul_css::props::style::background::{BackgroundPositionHorizontal, BackgroundPositionVertical};

    let x = match pos.horizontal {
        BackgroundPositionHorizontal::Left => 0.0,
        BackgroundPositionHorizontal::Center => 0.5,
        BackgroundPositionHorizontal::Right => 1.0,
        BackgroundPositionHorizontal::Exact(px) => {
            let val = px.to_pixels_internal(width, 16.0);
            if width > 0.0 { val / width } else { 0.5 }
        }
    };
    let y = match pos.vertical {
        BackgroundPositionVertical::Top => 0.0,
        BackgroundPositionVertical::Center => 0.5,
        BackgroundPositionVertical::Bottom => 1.0,
        BackgroundPositionVertical::Exact(px) => {
            let val = px.to_pixels_internal(height, 16.0);
            if height > 0.0 { val / height } else { 0.5 }
        }
    };
    (x, y)
}

fn render_linear_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::LinearGradient,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> Result<(), String> {
    use azul_css::props::basic::geometry::{LayoutRect, LayoutSize};

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let stops = gradient.stops.as_ref();
    if stops.is_empty() {
        return Ok(());
    }

    let lut = build_gradient_lut_linear(&gradient.stops);

    // Convert Direction to start/end points using the existing to_points method
    let layout_rect = LayoutRect {
        origin: azul_css::props::basic::geometry::LayoutPoint::new(0, 0),
        size: LayoutSize {
            width: (rect.width as isize),
            height: (rect.height as isize),
        },
    };
    let (from_pt, to_pt) = gradient.direction.to_points(&layout_rect);

    // Pixel-space start/end
    let x1 = rect.x as f64 + from_pt.x as f64;
    let y1 = rect.y as f64 + from_pt.y as f64;
    let x2 = rect.x as f64 + to_pt.x as f64;
    let y2 = rect.y as f64 + to_pt.y as f64;

    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return Ok(());
    }

    // Build transform: maps gradient line to X axis
    // We need the inverse: pixel space -> gradient space
    let angle = dy.atan2(dx);
    let mut transform = TransAffine::new_translation(x1, y1);
    transform.rotate(angle);
    transform.scale(len / 100.0, len / 100.0); // scale so d1=0, d2=100 maps to gradient length
    transform.invert();

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient(pixmap, &mut path, &lut, GradientX, transform, 0.0, 100.0);
    Ok(())
}

fn render_radial_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::RadialGradient,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> Result<(), String> {
    use azul_css::props::style::background::{RadialGradientSize, Shape};

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let stops = gradient.stops.as_ref();
    if stops.is_empty() {
        return Ok(());
    }

    let lut = build_gradient_lut_linear(&gradient.stops);

    let w = rect.width as f64;
    let h = rect.height as f64;

    // Compute center from position
    let (cx_frac, cy_frac) = resolve_background_position(&gradient.position, rect.width, rect.height);
    let cx = rect.x as f64 + cx_frac as f64 * w;
    let cy = rect.y as f64 + cy_frac as f64 * h;

    // Compute radius based on shape and size
    let radius = match gradient.size {
        RadialGradientSize::ClosestSide => {
            let dx = (cx_frac as f64 * w).min((1.0 - cx_frac as f64) * w);
            let dy = (cy_frac as f64 * h).min((1.0 - cy_frac as f64) * h);
            match gradient.shape {
                Shape::Circle => dx.min(dy),
                Shape::Ellipse => dx.min(dy), // simplified
            }
        }
        RadialGradientSize::FarthestSide => {
            let dx = (cx_frac as f64 * w).max((1.0 - cx_frac as f64) * w);
            let dy = (cy_frac as f64 * h).max((1.0 - cy_frac as f64) * h);
            match gradient.shape {
                Shape::Circle => dx.max(dy),
                Shape::Ellipse => dx.max(dy),
            }
        }
        RadialGradientSize::ClosestCorner => {
            let dx = (cx_frac as f64 * w).min((1.0 - cx_frac as f64) * w);
            let dy = (cy_frac as f64 * h).min((1.0 - cy_frac as f64) * h);
            (dx * dx + dy * dy).sqrt()
        }
        RadialGradientSize::FarthestCorner => {
            let dx = (cx_frac as f64 * w).max((1.0 - cx_frac as f64) * w);
            let dy = (cy_frac as f64 * h).max((1.0 - cy_frac as f64) * h);
            (dx * dx + dy * dy).sqrt()
        }
    };

    if radius < 0.001 {
        return Ok(());
    }

    // Build transform: maps center to origin, scales radius to 100
    let mut transform = TransAffine::new_translation(cx, cy);
    transform.scale(radius / 100.0, radius / 100.0);
    transform.invert();

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient(pixmap, &mut path, &lut, GradientRadialD, transform, 0.0, 100.0);
    Ok(())
}

fn render_conic_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::ConicGradient,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> Result<(), String> {
    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let stops = gradient.stops.as_ref();
    if stops.is_empty() {
        return Ok(());
    }

    let lut = build_gradient_lut_radial(&gradient.stops);

    let w = rect.width as f64;
    let h = rect.height as f64;

    // Compute center
    let (cx_frac, cy_frac) = resolve_background_position(&gradient.center, rect.width, rect.height);
    let cx = rect.x as f64 + cx_frac as f64 * w;
    let cy = rect.y as f64 + cy_frac as f64 * h;

    // Start angle (CSS conic gradients start at 12 o'clock = -90deg in math coords)
    let start_angle_deg = gradient.angle.to_degrees();
    let start_angle_rad = ((start_angle_deg - 90.0) as f64).to_radians();

    // Build transform: translate center to origin, rotate by start angle
    let mut transform = TransAffine::new_translation(cx, cy);
    transform.rotate(start_angle_rad);
    transform.invert();

    // GradientConic maps atan2(y,x) * d / pi, covering [0, d] for the half-circle.
    // We use d2 = 100 as the range; the LUT maps 0..1 over that.
    let d2 = 100.0;

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient(pixmap, &mut path, &lut, GradientConic, transform, 0.0, d2);
    Ok(())
}

// ============================================================================
// Box shadow rendering
// ============================================================================

fn render_box_shadow(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    shadow: &azul_css::props::style::box_shadow::StyleBoxShadow,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> Result<(), String> {
    use azul_css::props::style::box_shadow::BoxShadowClipMode;

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let offset_x = shadow.offset_x.inner.to_pixels_internal(0.0, 16.0) * dpi_factor;
    let offset_y = shadow.offset_y.inner.to_pixels_internal(0.0, 16.0) * dpi_factor;
    let blur_r = (shadow.blur_radius.inner.to_pixels_internal(0.0, 16.0) * dpi_factor).max(0.0);
    let spread = shadow.spread_radius.inner.to_pixels_internal(0.0, 16.0) * dpi_factor;

    let color = shadow.color;
    if color.a == 0 {
        return Ok(());
    }

    // Compute shadow rect (expanded by spread, padded by blur)
    let padding = blur_r.ceil();
    let shadow_x = rect.x + offset_x - spread - padding;
    let shadow_y = rect.y + offset_y - spread - padding;
    let shadow_w = rect.width + 2.0 * spread + 2.0 * padding;
    let shadow_h = rect.height + 2.0 * spread + 2.0 * padding;

    if shadow_w <= 0.0 || shadow_h <= 0.0 {
        return Ok(());
    }

    let sw = shadow_w.ceil() as u32;
    let sh = shadow_h.ceil() as u32;

    if sw == 0 || sh == 0 || sw > 4096 || sh > 4096 {
        return Ok(());
    }

    // Create temp buffer and draw the shadow shape into it
    let mut tmp = AzulPixmap::new(sw, sh).ok_or("cannot create shadow pixmap")?;
    tmp.fill(0, 0, 0, 0); // transparent

    // The shape origin within the temp buffer
    let shape_x = padding + spread;
    let shape_y = padding + spread;
    let shape_rect = match AzRect::from_xywh(shape_x, shape_y, rect.width, rect.height) {
        Some(r) => r,
        None => return Ok(()),
    };

    let agg_color = Rgba8::new(color.r as u32, color.g as u32, color.b as u32, color.a as u32);
    if border_radius.is_zero() {
        let mut path = build_rect_path(&shape_rect);
        agg_fill_path(&mut tmp, &mut path, &agg_color, FillingRule::NonZero);
    } else {
        let mut path = build_rounded_rect_path(&shape_rect, border_radius, dpi_factor);
        agg_fill_path(&mut tmp, &mut path, &agg_color, FillingRule::NonZero);
    }

    // Apply blur
    if blur_r > 0.5 {
        let blur_radius = (blur_r.ceil() as u32).min(254);
        let stride = (sw * 4) as i32;
        let mut ra = unsafe {
            RowAccessor::new_with_buf(tmp.data.as_mut_ptr(), sw, sh, stride)
        };
        stack_blur_rgba32(&mut ra, blur_radius, blur_radius);
    }

    // Blit the shadow buffer onto the main pixmap
    let dst_x = shadow_x as i32;
    let dst_y = shadow_y as i32;
    blit_buffer(pixmap, &tmp.data, sw, sh, dst_x, dst_y);

    Ok(())
}

/// Alpha-blend one RGBA buffer onto another at (dx, dy).
fn blit_buffer(dst: &mut AzulPixmap, src: &[u8], src_w: u32, src_h: u32, dx: i32, dy: i32) {
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
                let da = 255 - sa;
                dst.data[di] = ((src[si] as u32 * sa + dst.data[di] as u32 * da) / 255) as u8;
                dst.data[di + 1] = ((src[si + 1] as u32 * sa + dst.data[di + 1] as u32 * da) / 255) as u8;
                dst.data[di + 2] = ((src[si + 2] as u32 * sa + dst.data[di + 2] as u32 * da) / 255) as u8;
                dst.data[di + 3] = ((sa + dst.data[di + 3] as u32 * da / 255).min(255)) as u8;
            }
        }
    }
}

// ============================================================================
// Image mask clipping
// ============================================================================

/// Entry on the mask stack for PushImageMaskClip / PopImageMaskClip.
struct MaskEntry {
    /// Snapshot of the pixmap region before mask was pushed.
    snapshot: Vec<u8>,
    /// R8 mask data scaled to target dimensions.
    mask_data: Vec<u8>,
    /// Target region in pixel coordinates.
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
}

/// Take a snapshot of a rectangular region of the pixmap.
fn snapshot_region(pixmap: &AzulPixmap, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
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

/// Extract and scale mask image data (R8) to target dimensions.
fn extract_mask_data(mask_image: &ImageRef, target_w: u32, target_h: u32) -> Option<Vec<u8>> {
    let image_data = mask_image.get_data();
    let (mask_bytes, src_w, src_h) = match &*image_data {
        DecodedImage::Raw((descriptor, data)) => {
            let w = descriptor.width as u32;
            let h = descriptor.height as u32;
            if w == 0 || h == 0 {
                return None;
            }
            let bytes = match data {
                azul_core::resources::ImageData::Raw(shared) => shared.as_ref(),
                _ => return None,
            };
            match descriptor.format {
                azul_core::resources::RawImageFormat::R8 => {
                    (bytes.to_vec(), w, h)
                }
                azul_core::resources::RawImageFormat::BGRA8 => {
                    // Use alpha channel as mask
                    let mut r8 = Vec::with_capacity((w * h) as usize);
                    for chunk in bytes.chunks_exact(4) {
                        r8.push(chunk[3]); // alpha
                    }
                    (r8, w, h)
                }
                _ => {
                    // Use first channel as grayscale mask
                    let chan_count = bytes.len() / (w * h) as usize;
                    if chan_count == 0 {
                        return None;
                    }
                    let mut r8 = Vec::with_capacity((w * h) as usize);
                    for i in 0..(w * h) as usize {
                        r8.push(bytes[i * chan_count]);
                    }
                    (r8, w, h)
                }
            }
        }
        _ => return None,
    };

    if target_w == 0 || target_h == 0 {
        return None;
    }

    // Scale mask to target dimensions via nearest-neighbor
    let mut scaled = vec![0u8; (target_w * target_h) as usize];
    let sx = src_w as f32 / target_w as f32;
    let sy = src_h as f32 / target_h as f32;
    for py in 0..target_h {
        for px in 0..target_w {
            let mx = ((px as f32 * sx) as u32).min(src_w - 1);
            let my = ((py as f32 * sy) as u32).min(src_h - 1);
            scaled[(py * target_w + px) as usize] = mask_bytes[(my * src_w + mx) as usize];
        }
    }
    Some(scaled)
}

/// Apply a mask: for each pixel in the mask region, blend between the snapshot
/// (pre-mask state) and the current pixmap state using the mask value.
fn apply_mask(pixmap: &mut AzulPixmap, entry: &MaskEntry) {
    let pw = pixmap.width as i32;
    let ph = pixmap.height as i32;

    for py in 0..entry.height as i32 {
        let dy = entry.origin_y + py;
        if dy < 0 || dy >= ph {
            continue;
        }
        for px in 0..entry.width as i32 {
            let dx = entry.origin_x + px;
            if dx < 0 || dx >= pw {
                continue;
            }

            let mi = (py as u32 * entry.width + px as u32) as usize;
            let mask_val = entry.mask_data.get(mi).copied().unwrap_or(0) as u32;

            let pi = ((dy as u32 * pixmap.width + dx as u32) * 4) as usize;
            let si = ((py as u32 * entry.width + px as u32) * 4) as usize;

            if pi + 3 >= pixmap.data.len() || si + 3 >= entry.snapshot.len() {
                continue;
            }

            // Blend: result = snapshot * (255 - mask) + current * mask
            // mask_val 255 = fully visible (keep current), 0 = fully clipped (restore snapshot)
            let inv_mask = 255 - mask_val;
            for c in 0..4 {
                let snap_c = entry.snapshot[si + c] as u32;
                let cur_c = pixmap.data[pi + c] as u32;
                pixmap.data[pi + c] = ((cur_c * mask_val + snap_c * inv_mask) / 255) as u8;
            }
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

pub struct RenderOptions {
    pub width: f32,
    pub height: f32,
    pub dpi_factor: f32,
}

pub fn render(
    dl: &DisplayList,
    res: &RendererResources,
    opts: RenderOptions,
    glyph_cache: &mut GlyphCache,
) -> Result<AzulPixmap, String> {
    let RenderOptions {
        width,
        height,
        dpi_factor,
    } = opts;

    let mut pixmap = AzulPixmap::new((width * dpi_factor) as u32, (height * dpi_factor) as u32)
        .ok_or_else(|| "cannot create pixmap".to_string())?;

    pixmap.fill(255, 255, 255, 255);

    render_display_list(dl, &mut pixmap, dpi_factor, res, None, glyph_cache)?;

    Ok(pixmap)
}

/// Render a display list using fonts from FontManager directly
/// This is used in reftest scenarios where RendererResources doesn't have fonts registered
pub fn render_with_font_manager(
    dl: &DisplayList,
    res: &RendererResources,
    font_manager: &FontManager<FontRef>,
    opts: RenderOptions,
    glyph_cache: &mut GlyphCache,
) -> Result<AzulPixmap, String> {
    let RenderOptions {
        width,
        height,
        dpi_factor,
    } = opts;

    let mut pixmap = AzulPixmap::new((width * dpi_factor) as u32, (height * dpi_factor) as u32)
        .ok_or_else(|| "cannot create pixmap".to_string())?;

    pixmap.fill(255, 255, 255, 255);

    render_display_list(dl, &mut pixmap, dpi_factor, res, Some(font_manager), glyph_cache)?;

    Ok(pixmap)
}

fn render_display_list(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
) -> Result<(), String> {
    let mut transform_stack = vec![TransAffine::new()]; // identity
    let mut clip_stack: Vec<Option<AzRect>> = vec![None];
    let mut mask_stack: Vec<MaskEntry> = Vec::new();

    for item in &display_list.items {
        match item {
            DisplayListItem::Rect {
                bounds,
                color,
                border_radius,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    *color,
                    border_radius,
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::SelectionRect {
                bounds,
                color,
                border_radius,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    *color,
                    border_radius,
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::CursorRect { bounds, color } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    *color,
                    &BorderRadius::default(),
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Border {
                bounds,
                widths,
                colors,
                styles,
                border_radius,
            } => {
                use azul_css::{css::CssPropertyValue, props::basic::pixel::DEFAULT_FONT_SIZE};

                let width = widths
                    .top
                    .and_then(|w| w.get_property().cloned())
                    .map(|w| w.inner.to_pixels_internal(0.0, DEFAULT_FONT_SIZE))
                    .unwrap_or(0.0);

                let color = colors
                    .top
                    .and_then(|c| c.get_property().cloned())
                    .map(|c| c.inner)
                    .unwrap_or(ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    });

                let simple_radius = BorderRadius {
                    top_left: border_radius
                        .top_left
                        .to_pixels_internal(bounds.0.size.width, DEFAULT_FONT_SIZE),
                    top_right: border_radius
                        .top_right
                        .to_pixels_internal(bounds.0.size.width, DEFAULT_FONT_SIZE),
                    bottom_left: border_radius
                        .bottom_left
                        .to_pixels_internal(bounds.0.size.width, DEFAULT_FONT_SIZE),
                    bottom_right: border_radius
                        .bottom_right
                        .to_pixels_internal(bounds.0.size.width, DEFAULT_FONT_SIZE),
                };

                let clip = *clip_stack.last().unwrap();
                render_border(
                    pixmap,
                    bounds.inner(),
                    color,
                    width,
                    &simple_radius,
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Underline {
                bounds,
                color,
                thickness,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    *color,
                    &BorderRadius::default(),
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Strikethrough {
                bounds,
                color,
                thickness,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    *color,
                    &BorderRadius::default(),
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Overline {
                bounds,
                color,
                thickness,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    *color,
                    &BorderRadius::default(),
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::Text {
                glyphs,
                font_size_px,
                font_hash,
                color,
                clip_rect,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_text(
                    glyphs,
                    *font_hash,
                    *font_size_px,
                    *color,
                    pixmap,
                    clip_rect.inner(),
                    clip,
                    renderer_resources,
                    font_manager,
                    dpi_factor,
                    glyph_cache,
                )?;
            }
            DisplayListItem::TextLayout {
                layout,
                bounds,
                font_hash,
                font_size_px,
                color,
            } => {
                // TextLayout is metadata for PDF/accessibility - skip in CPU rendering
            }
            DisplayListItem::Image { bounds, image, .. } => {
                let clip = *clip_stack.last().unwrap();
                render_image(
                    pixmap,
                    bounds.inner(),
                    image,
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::ScrollBar {
                bounds,
                color,
                orientation,
                opacity_key: _,
                hit_id: _,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    *color,
                    &BorderRadius::default(),
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::ScrollBarStyled { info } => {
                let clip = *clip_stack.last().unwrap();

                // Render track
                if info.track_color.a > 0 {
                    render_rect(
                        pixmap,
                        info.track_bounds.inner(),
                        info.track_color,
                        &BorderRadius::default(),
                        clip,
                        dpi_factor,
                    )?;
                }

                // Render decrement button
                if let Some(btn_bounds) = &info.button_decrement_bounds {
                    if info.button_color.a > 0 {
                        render_rect(
                            pixmap,
                            btn_bounds.inner(),
                            info.button_color,
                            &BorderRadius::default(),
                            clip,
                            dpi_factor,
                        )?;
                    }
                }

                // Render increment button
                if let Some(btn_bounds) = &info.button_increment_bounds {
                    if info.button_color.a > 0 {
                        render_rect(
                            pixmap,
                            btn_bounds.inner(),
                            info.button_color,
                            &BorderRadius::default(),
                            clip,
                            dpi_factor,
                        )?;
                    }
                }

                // Render thumb
                if info.thumb_color.a > 0 {
                    render_rect(
                        pixmap,
                        info.thumb_bounds.inner(),
                        info.thumb_color,
                        &info.thumb_border_radius,
                        clip,
                        dpi_factor,
                    )?;
                }
            }
            DisplayListItem::PushClip {
                bounds,
                border_radius,
            } => {
                let new_clip = logical_rect_to_az_rect(bounds.inner(), dpi_factor);
                clip_stack.push(new_clip);
            }
            DisplayListItem::PopClip => {
                clip_stack.pop();
                if clip_stack.is_empty() {
                    return Err("Clip stack underflow".to_string());
                }
            }
            DisplayListItem::PushScrollFrame {
                clip_bounds,
                content_size,
                scroll_id,
            } => {
                let new_clip = logical_rect_to_az_rect(clip_bounds.inner(), dpi_factor);
                clip_stack.push(new_clip);
            }
            DisplayListItem::PopScrollFrame => {
                clip_stack.pop();
                if clip_stack.is_empty() {
                    return Err("Clip stack underflow from scroll frame".to_string());
                }
            }
            DisplayListItem::HitTestArea { bounds, tag } => {
                // Hit test areas don't render anything
            }
            DisplayListItem::PushStackingContext { z_index, bounds } => {
                // For CPU rendering, stacking contexts are already handled by display list order
            }
            DisplayListItem::PopStackingContext => {}
            DisplayListItem::VirtualView {
                child_dom_id,
                bounds,
                clip_rect,
            } => {
                let clip = *clip_stack.last().unwrap();
                render_rect(
                    pixmap,
                    bounds.inner(),
                    ColorU {
                        r: 200,
                        g: 200,
                        b: 255,
                        a: 128,
                    },
                    &BorderRadius::default(),
                    clip,
                    dpi_factor,
                )?;
            }
            DisplayListItem::VirtualViewPlaceholder { .. } => {}

            // Gradient rendering
            DisplayListItem::LinearGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                render_linear_gradient(
                    pixmap,
                    bounds.inner(),
                    gradient,
                    border_radius,
                    dpi_factor,
                )?;
            }
            DisplayListItem::RadialGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                render_radial_gradient(
                    pixmap,
                    bounds.inner(),
                    gradient,
                    border_radius,
                    dpi_factor,
                )?;
            }
            DisplayListItem::ConicGradient {
                bounds,
                gradient,
                border_radius,
            } => {
                render_conic_gradient(
                    pixmap,
                    bounds.inner(),
                    gradient,
                    border_radius,
                    dpi_factor,
                )?;
            }

            // BoxShadow
            DisplayListItem::BoxShadow {
                bounds,
                shadow,
                border_radius,
            } => {
                render_box_shadow(
                    pixmap,
                    bounds.inner(),
                    shadow,
                    border_radius,
                    dpi_factor,
                )?;
            }

            // Filter effects - not supported in CPU render
            DisplayListItem::PushFilter { .. } => {}
            DisplayListItem::PopFilter => {}
            DisplayListItem::PushBackdropFilter { .. } => {}
            DisplayListItem::PopBackdropFilter => {}
            DisplayListItem::PushOpacity { bounds, opacity } => {}
            DisplayListItem::PopOpacity => {}
            DisplayListItem::PushReferenceFrame { .. } => {}
            DisplayListItem::PopReferenceFrame => {}
            DisplayListItem::PushTextShadow { .. } => {}
            DisplayListItem::PopTextShadow => {}

            DisplayListItem::PushImageMaskClip {
                bounds,
                mask_image,
                mask_rect,
            } => {
                let mr = mask_rect.inner();
                let px_x = (mr.origin.x * dpi_factor) as i32;
                let px_y = (mr.origin.y * dpi_factor) as i32;
                let px_w = (mr.size.width * dpi_factor).ceil() as u32;
                let px_h = (mr.size.height * dpi_factor).ceil() as u32;

                if px_w > 0 && px_h > 0 {
                    let snapshot = snapshot_region(pixmap, px_x, px_y, px_w, px_h);
                    let mask_data = extract_mask_data(mask_image, px_w, px_h)
                        .unwrap_or_else(|| vec![255u8; (px_w * px_h) as usize]);
                    mask_stack.push(MaskEntry {
                        snapshot,
                        mask_data,
                        origin_x: px_x,
                        origin_y: px_y,
                        width: px_w,
                        height: px_h,
                    });
                }
            }
            DisplayListItem::PopImageMaskClip => {
                if let Some(entry) = mask_stack.pop() {
                    apply_mask(pixmap, &entry);
                }
            }
        }
    }

    Ok(())
}

fn render_rect(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    color: ColorU,
    border_radius: &BorderRadius,
    _clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    if color.a == 0 {
        return Ok(());
    }

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let agg_color = Rgba8::new(color.r as u32, color.g as u32, color.b as u32, color.a as u32);

    if border_radius.is_zero() {
        let mut path = build_rect_path(&rect);
        agg_fill_path(pixmap, &mut path, &agg_color, FillingRule::NonZero);
    } else {
        let mut path = build_rounded_rect_path(&rect, border_radius, dpi_factor);
        agg_fill_path(pixmap, &mut path, &agg_color, FillingRule::NonZero);
    }

    Ok(())
}

fn render_text(
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    pixmap: &mut AzulPixmap,
    clip_rect: &LogicalRect,
    _clip: Option<AzRect>,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
) -> Result<(), String> {
    if color.a == 0 || glyphs.is_empty() {
        return Ok(());
    }

    let agg_color = Rgba8::new(color.r as u32, color.g as u32, color.b as u32, color.a as u32);

    // Try to get the parsed font
    let parsed_font: &ParsedFont = if let Some(fm) = font_manager {
        match fm.get_font_by_hash(font_hash.font_hash) {
            Some(font_ref) => unsafe { &*(font_ref.get_parsed() as *const ParsedFont) },
            None => {
                eprintln!(
                    "[cpurender] Font hash {} not found in FontManager",
                    font_hash.font_hash
                );
                return Ok(());
            }
        }
    } else {
        let font_key = match renderer_resources.font_hash_map.get(&font_hash.font_hash) {
            Some(k) => k,
            None => {
                eprintln!(
                    "[cpurender] Font hash {} not found in font_hash_map (available: {:?})",
                    font_hash.font_hash,
                    renderer_resources.font_hash_map.keys().collect::<Vec<_>>()
                );
                return Ok(());
            }
        };

        let font_ref = match renderer_resources.currently_registered_fonts.get(font_key) {
            Some((font_ref, _instances)) => font_ref,
            None => {
                eprintln!(
                    "[cpurender] FontKey {:?} not found in currently_registered_fonts",
                    font_key
                );
                return Ok(());
            }
        };

        unsafe { &*(font_ref.get_parsed() as *const ParsedFont) }
    };

    let units_per_em = parsed_font.font_metrics.units_per_em as f32;
    if units_per_em <= 0.0 {
        return Ok(());
    }

    let scale = (font_size_px * dpi_factor) / units_per_em;
    let ppem = (font_size_px * dpi_factor).round() as u16;

    // Draw each glyph using cached paths
    for glyph in glyphs {
        let glyph_index = glyph.index as u16;

        let glyph_data = match parsed_font.glyph_records_decoded.get(&glyph_index) {
            Some(d) => d,
            None => continue,
        };

        let cached = match glyph_cache.get_or_build(
            font_hash.font_hash, glyph_index, glyph_data, parsed_font, ppem,
        ) {
            Some(c) => c,
            None => continue,
        };

        let glyph_x = glyph.point.x * dpi_factor;
        let glyph_baseline_y = glyph.point.y * dpi_factor;

        let glyph_transform = if cached.is_hinted {
            // Hinted path is in pixel coordinates — snap to pixel grid
            TransAffine::new_translation(glyph_x.round() as f64, glyph_baseline_y.round() as f64)
        } else {
            // Unhinted path is in font units — apply scale + translate
            let mut t = TransAffine::new_scaling_uniform(scale as f64);
            t.multiply(&TransAffine::new_translation(glyph_x as f64, glyph_baseline_y as f64));
            t
        };

        let mut path_clone = cached.path.clone();
        agg_fill_transformed_path(
            pixmap,
            &mut path_clone,
            &agg_color,
            FillingRule::NonZero,
            &glyph_transform,
        );
    }

    Ok(())
}

fn render_border(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    color: ColorU,
    width: f32,
    border_radius: &BorderRadius,
    _clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    if color.a == 0 || width <= 0.0 {
        return Ok(());
    }

    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let scaled_width = width * dpi_factor;
    let agg_color = Rgba8::new(color.r as u32, color.g as u32, color.b as u32, color.a as u32);

    let mut path = PathStorage::new();

    // 1. Add Outer Path
    let x = rect.x as f64;
    let y = rect.y as f64;
    let w = rect.width as f64;
    let h = rect.height as f64;

    if border_radius.is_zero() {
        path.move_to(x, y);
        path.line_to(x + w, y);
        path.line_to(x + w, y + h);
        path.line_to(x, y + h);
        path.close_polygon(PATH_FLAGS_NONE);
    } else {
        let tl = (border_radius.top_left * dpi_factor) as f64;
        let tr = (border_radius.top_right * dpi_factor) as f64;
        let br = (border_radius.bottom_right * dpi_factor) as f64;
        let bl = (border_radius.bottom_left * dpi_factor) as f64;

        path.move_to(x + tl, y);
        path.line_to(x + w - tr, y);
        if tr > 0.0 {
            path.curve3(x + w, y, x + w, y + tr);
        }
        path.line_to(x + w, y + h - br);
        if br > 0.0 {
            path.curve3(x + w, y + h, x + w - br, y + h);
        }
        path.line_to(x + bl, y + h);
        if bl > 0.0 {
            path.curve3(x, y + h, x, y + h - bl);
        }
        path.line_to(x, y + tl);
        if tl > 0.0 {
            path.curve3(x, y, x + tl, y);
        }
        path.close_polygon(PATH_FLAGS_NONE);
    }

    // 2. Add Inner Path (same winding — EvenOdd fill creates the hole)
    let sw = scaled_width as f64;
    let ir = AzRect::from_xywh(
        rect.x + scaled_width,
        rect.y + scaled_width,
        rect.width - 2.0 * scaled_width,
        rect.height - 2.0 * scaled_width,
    );

    if let Some(ir) = ir {
        let ix = ir.x as f64;
        let iy = ir.y as f64;
        let iw = ir.width as f64;
        let ih = ir.height as f64;

        if border_radius.is_zero() {
            path.move_to(ix, iy);
            path.line_to(ix + iw, iy);
            path.line_to(ix + iw, iy + ih);
            path.line_to(ix, iy + ih);
            path.close_polygon(PATH_FLAGS_NONE);
        } else {
            let tl = ((border_radius.top_left * dpi_factor - scaled_width).max(0.0)) as f64;
            let tr = ((border_radius.top_right * dpi_factor - scaled_width).max(0.0)) as f64;
            let br = ((border_radius.bottom_right * dpi_factor - scaled_width).max(0.0)) as f64;
            let bl = ((border_radius.bottom_left * dpi_factor - scaled_width).max(0.0)) as f64;

            path.move_to(ix + tl, iy);
            path.line_to(ix + iw - tr, iy);
            if tr > 0.0 {
                path.curve3(ix + iw, iy, ix + iw, iy + tr);
            }
            path.line_to(ix + iw, iy + ih - br);
            if br > 0.0 {
                path.curve3(ix + iw, iy + ih, ix + iw - br, iy + ih);
            }
            path.line_to(ix + bl, iy + ih);
            if bl > 0.0 {
                path.curve3(ix, iy + ih, ix, iy + ih - bl);
            }
            path.line_to(ix, iy + tl);
            if tl > 0.0 {
                path.curve3(ix, iy, ix + tl, iy);
            }
            path.close_polygon(PATH_FLAGS_NONE);
        }
    }

    // 3. Fill with EvenOdd to create the hole
    agg_fill_path(pixmap, &mut path, &agg_color, FillingRule::EvenOdd);

    Ok(())
}

fn logical_rect_to_az_rect(
    bounds: &LogicalRect,
    dpi_factor: f32,
) -> Option<AzRect> {
    let x = bounds.origin.x * dpi_factor;
    let y = bounds.origin.y * dpi_factor;
    let width = bounds.size.width * dpi_factor;
    let height = bounds.size.height * dpi_factor;

    AzRect::from_xywh(x, y, width, height)
}

fn render_image(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    image: &ImageRef,
    _clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    let rect = match logical_rect_to_az_rect(bounds, dpi_factor) {
        Some(r) => r,
        None => return Ok(()),
    };

    let image_data = image.get_data();
    let (src_rgba, src_w, src_h) = match &*image_data {
        DecodedImage::Raw((descriptor, data)) => {
            let w = descriptor.width as u32;
            let h = descriptor.height as u32;
            if w == 0 || h == 0 { return Ok(()); }
            let bytes = match data {
                azul_core::resources::ImageData::Raw(shared) => shared.as_ref(),
                _ => return Ok(()),
            };

            let rgba = match descriptor.format {
                azul_core::resources::RawImageFormat::BGRA8 => {
                    let mut out = Vec::with_capacity(bytes.len());
                    for chunk in bytes.chunks_exact(4) {
                        let b = chunk[0]; let g = chunk[1]; let r = chunk[2]; let a = chunk[3];
                        out.push(r); out.push(g); out.push(b); out.push(a);
                    }
                    out
                }
                azul_core::resources::RawImageFormat::R8 => {
                    let mut out = Vec::with_capacity(bytes.len() * 4);
                    for &v in bytes {
                        out.push(v); out.push(v); out.push(v); out.push(v);
                    }
                    out
                }
                _ => {
                    // Unsupported format — render gray placeholder
                    let gray = Rgba8::new(200, 200, 200, 255);
                    let mut path = build_rect_path(&rect);
                    agg_fill_path(pixmap, &mut path, &gray, FillingRule::NonZero);
                    return Ok(());
                }
            };

            (rgba, w, h)
        }
        DecodedImage::NullImage { .. } | DecodedImage::Callback(_) => {
            let gray = Rgba8::new(200, 200, 200, 255);
            let mut path = build_rect_path(&rect);
            agg_fill_path(pixmap, &mut path, &gray, FillingRule::NonZero);
            return Ok(());
        }
        _ => return Ok(()),
    };

    // Simple nearest-neighbor blit with scaling
    let dst_x = rect.x as i32;
    let dst_y = rect.y as i32;
    let dst_w = rect.width as u32;
    let dst_h = rect.height as u32;
    let pw = pixmap.width;
    let ph = pixmap.height;

    let sx = src_w as f32 / dst_w.max(1) as f32;
    let sy = src_h as f32 / dst_h.max(1) as f32;

    for py in 0..dst_h {
        for px in 0..dst_w {
            let tx = dst_x + px as i32;
            let ty = dst_y + py as i32;
            if tx < 0 || ty < 0 || tx >= pw as i32 || ty >= ph as i32 {
                continue;
            }

            let src_x = ((px as f32 * sx) as u32).min(src_w - 1);
            let src_y = ((py as f32 * sy) as u32).min(src_h - 1);
            let si = ((src_y * src_w + src_x) * 4) as usize;
            let di = ((ty as u32 * pw + tx as u32) * 4) as usize;

            if si + 3 < src_rgba.len() && di + 3 < pixmap.data.len() {
                let sa = src_rgba[si + 3] as u32;
                if sa == 255 {
                    pixmap.data[di]     = src_rgba[si];
                    pixmap.data[di + 1] = src_rgba[si + 1];
                    pixmap.data[di + 2] = src_rgba[si + 2];
                    pixmap.data[di + 3] = 255;
                } else if sa > 0 {
                    // Alpha blend: dst = src * sa + dst * (255 - sa)
                    let da = 255 - sa;
                    pixmap.data[di]     = ((src_rgba[si] as u32 * sa + pixmap.data[di] as u32 * da) / 255) as u8;
                    pixmap.data[di + 1] = ((src_rgba[si + 1] as u32 * sa + pixmap.data[di + 1] as u32 * da) / 255) as u8;
                    pixmap.data[di + 2] = ((src_rgba[si + 2] as u32 * sa + pixmap.data[di + 2] as u32 * da) / 255) as u8;
                    pixmap.data[di + 3] = ((sa + pixmap.data[di + 3] as u32 * da / 255).min(255)) as u8;
                }
            }
        }
    }

    Ok(())
}

fn build_rect_path(rect: &AzRect) -> PathStorage {
    let mut path = PathStorage::new();
    let x = rect.x as f64;
    let y = rect.y as f64;
    let w = rect.width as f64;
    let h = rect.height as f64;
    path.move_to(x, y);
    path.line_to(x + w, y);
    path.line_to(x + w, y + h);
    path.line_to(x, y + h);
    path.close_polygon(PATH_FLAGS_NONE);
    path
}

fn build_rounded_rect_path(
    rect: &AzRect,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> PathStorage {
    let mut path = PathStorage::new();

    let x = rect.x as f64;
    let y = rect.y as f64;
    let w = rect.width as f64;
    let h = rect.height as f64;

    let tl = (border_radius.top_left * dpi_factor) as f64;
    let tr = (border_radius.top_right * dpi_factor) as f64;
    let br = (border_radius.bottom_right * dpi_factor) as f64;
    let bl = (border_radius.bottom_left * dpi_factor) as f64;

    // Start at top-left corner (after radius)
    path.move_to(x + tl, y);

    // Top edge
    path.line_to(x + w - tr, y);

    // Top-right corner
    if tr > 0.0 {
        path.curve3(x + w, y, x + w, y + tr);
    }

    // Right edge
    path.line_to(x + w, y + h - br);

    // Bottom-right corner
    if br > 0.0 {
        path.curve3(x + w, y + h, x + w - br, y + h);
    }

    // Bottom edge
    path.line_to(x + bl, y + h);

    // Bottom-left corner
    if bl > 0.0 {
        path.curve3(x, y + h, x, y + h - bl);
    }

    // Left edge
    path.line_to(x, y + tl);

    // Top-left corner
    if tl > 0.0 {
        path.curve3(x, y, x + tl, y);
    }

    path.close_polygon(PATH_FLAGS_NONE);
    path
}

// ============================================================================
// Component Preview Rendering
// ============================================================================

/// Options for rendering a component preview.
pub struct ComponentPreviewOptions {
    /// Optional width constraint. If None, size to content (uses 4096px max).
    pub width: Option<f32>,
    /// Optional height constraint. If None, size to content (uses 4096px max).
    pub height: Option<f32>,
    /// DPI scale factor. Default 1.0.
    pub dpi_factor: f32,
    /// Background color. Default white.
    pub background_color: ColorU,
}

impl Default for ComponentPreviewOptions {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            dpi_factor: 1.0,
            background_color: ColorU { r: 255, g: 255, b: 255, a: 255 },
        }
    }
}

/// Result of a component preview render.
pub struct ComponentPreviewResult {
    /// PNG-encoded image data.
    pub png_data: Vec<u8>,
    /// Actual content width (logical pixels).
    pub content_width: f32,
    /// Actual content height (logical pixels).
    pub content_height: f32,
}

/// Compute the tight bounding box of all display list items.
fn compute_content_bounds(dl: &DisplayList) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut has_items = false;

    for item in &dl.items {
        let bounds = match item {
            DisplayListItem::Rect { bounds, .. } => Some(*bounds),
            DisplayListItem::SelectionRect { bounds, .. } => Some(*bounds),
            DisplayListItem::Border { bounds, .. } => Some(*bounds),
            DisplayListItem::Text { clip_rect, .. } => Some(*clip_rect),
            DisplayListItem::Image { bounds, .. } => Some(*bounds),
            DisplayListItem::BoxShadow { bounds, .. } => Some(*bounds),
            DisplayListItem::PushClip { bounds, .. } => Some(*bounds),
            DisplayListItem::LinearGradient { bounds, .. } => Some(*bounds),
            DisplayListItem::RadialGradient { bounds, .. } => Some(*bounds),
            DisplayListItem::ConicGradient { bounds, .. } => Some(*bounds),
            DisplayListItem::VirtualView { bounds, .. } => Some(*bounds),
            DisplayListItem::ScrollBar { bounds, .. } => Some(*bounds),
            _ => None,
        };
        if let Some(b) = bounds {
            has_items = true;
            min_x = min_x.min(b.0.origin.x);
            min_y = min_y.min(b.0.origin.y);
            max_x = max_x.max(b.0.origin.x + b.0.size.width);
            max_y = max_y.max(b.0.origin.y + b.0.size.height);
        }
    }

    if has_items {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

/// Render a `StyledDom` to a PNG image for component preview.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
pub fn render_component_preview(
    styled_dom: azul_core::styled_dom::StyledDom,
    font_manager: &FontManager<azul_css::props::basic::FontRef>,
    opts: ComponentPreviewOptions,
    system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
) -> Result<ComponentPreviewResult, String> {
    use std::collections::{BTreeMap, HashMap};
    use azul_core::{
        dom::DomId,
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::{IdNamespace, RendererResources},
        selection::{SelectionState, TextSelection},
    };
    use crate::{
        solver3::{
            self,
            cache::LayoutCache,
            display_list::DisplayList,
        },
        font_traits::TextLayoutCache,
    };

    const MAX_SIZE: f32 = 4096.0;

    let layout_width = opts.width.unwrap_or(MAX_SIZE);
    let layout_height = opts.height.unwrap_or(MAX_SIZE);

    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize {
            width: layout_width,
            height: layout_height,
        },
    };

    let mut preview_font_manager = FontManager::from_arc_shared(
        font_manager.fc_cache.clone(),
        font_manager.parsed_fonts.clone(),
    ).map_err(|e| format!("Failed to create preview font manager: {:?}", e))?;

    // --- Font resolution ---
    {
        use crate::solver3::getters::{
            collect_and_resolve_font_chains, collect_font_ids_from_chains,
            compute_fonts_to_load, load_fonts_from_disk, register_embedded_fonts_from_styled_dom,
        };

        let platform = azul_css::system::Platform::current();

        register_embedded_fonts_from_styled_dom(&styled_dom, &preview_font_manager, &platform);

        let chains = collect_and_resolve_font_chains(&styled_dom, &preview_font_manager.fc_cache, &platform);
        let required_fonts = collect_font_ids_from_chains(&chains);
        let already_loaded = preview_font_manager.get_loaded_font_ids();
        let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);

        if !fonts_to_load.is_empty() {
            use crate::text3::default::PathLoader;
            let loader = PathLoader::new();
            let load_result = load_fonts_from_disk(
                &fonts_to_load,
                &preview_font_manager.fc_cache,
                |bytes, index| loader.load_font(bytes, index),
            );
            preview_font_manager.insert_fonts(load_result.loaded);
        }

        preview_font_manager.set_font_chain_cache(chains.into_fontconfig_chains());
    }

    // --- Layout ---
    let mut layout_cache = LayoutCache {
        tree: None,
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: HashMap::new(),
        scroll_id_to_node_id: HashMap::new(),
        counters: HashMap::new(),
        float_cache: HashMap::new(),
        cache_map: Default::default(),
    };
    let mut text_cache = TextLayoutCache::new();
    let empty_scroll_offsets = BTreeMap::new();
    let empty_selections = BTreeMap::new();
    let empty_text_selections = BTreeMap::new();
    let renderer_resources = RendererResources::default();
    let id_namespace = IdNamespace(0xFFFF);
    let dom_id = DomId::ROOT_ID;
    let mut debug_messages = None;
    let get_system_time_fn = azul_core::task::GetSystemTimeCallback {
        cb: azul_core::task::get_system_time_libstd,
    };

    let display_list = solver3::layout_document(
        &mut layout_cache,
        &mut text_cache,
        styled_dom,
        viewport,
        &preview_font_manager,
        &empty_scroll_offsets,
        &empty_selections,
        &empty_text_selections,
        &mut debug_messages,
        None,
        &renderer_resources,
        id_namespace,
        dom_id,
        false,
        None,
        &azul_core::resources::ImageCache::default(),
        system_style,
        get_system_time_fn,
    ).map_err(|e| format!("Layout failed: {:?}", e))?;

    // --- Determine actual render size ---
    let (render_width, render_height) = if opts.width.is_some() && opts.height.is_some() {
        (opts.width.unwrap(), opts.height.unwrap())
    } else {
        match compute_content_bounds(&display_list) {
            Some((_min_x, _min_y, max_x, max_y)) => {
                let w = if opts.width.is_some() { opts.width.unwrap() } else { max_x.max(1.0).ceil() };
                let h = if opts.height.is_some() { opts.height.unwrap() } else { max_y.max(1.0).ceil() };
                (w, h)
            }
            None => {
                return Ok(ComponentPreviewResult {
                    png_data: Vec::new(),
                    content_width: 0.0,
                    content_height: 0.0,
                });
            }
        }
    };

    let render_width = render_width.min(MAX_SIZE);
    let render_height = render_height.min(MAX_SIZE);

    // --- Render ---
    let dpi = opts.dpi_factor;
    let pixel_w = ((render_width * dpi) as u32).max(1);
    let pixel_h = ((render_height * dpi) as u32).max(1);

    let mut pixmap = AzulPixmap::new(pixel_w, pixel_h)
        .ok_or_else(|| format!("Cannot create pixmap {}x{}", pixel_w, pixel_h))?;

    let bg = opts.background_color;
    pixmap.fill(bg.r, bg.g, bg.b, bg.a);

    let mut preview_glyph_cache = GlyphCache::new();
    render_display_list(
        &display_list,
        &mut pixmap,
        dpi,
        &renderer_resources,
        Some(&preview_font_manager),
        &mut preview_glyph_cache,
    )?;

    let png_data = pixmap.encode_png()
        .map_err(|e| format!("PNG encoding failed: {}", e))?;

    Ok(ComponentPreviewResult {
        png_data,
        content_width: render_width,
        content_height: render_height,
    })
}

/// Render a `Dom` + `Css` to a PNG image at the given dimensions.
///
/// This is a convenience API that creates a `StyledDom`, lays it out,
/// and rasterizes via the CPU renderer.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
pub fn render_dom_to_image(
    mut dom: azul_core::dom::Dom,
    css: azul_css::css::Css,
    width: f32,
    height: f32,
    dpi: f32,
) -> Result<Vec<u8>, String> {
    use azul_core::styled_dom::StyledDom;
    use crate::font_traits::FontManager;

    let styled_dom = StyledDom::create(&mut dom, css);

    let fc_cache = crate::font::loading::build_font_cache();
    let font_manager = FontManager::new(fc_cache)
        .map_err(|e| format!("Failed to create font manager: {:?}", e))?;

    let opts = ComponentPreviewOptions {
        width: Some(width),
        height: Some(height),
        dpi_factor: dpi,
        background_color: azul_css::props::basic::ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    };

    let result = render_component_preview(styled_dom, &font_manager, opts, None)?;
    Ok(result.png_data)
}

// ============================================================================
// Direct SVG-to-image renderer (bypasses CSS layout)
// ============================================================================

/// Render raw SVG bytes to a PNG image.
///
/// Parses the SVG XML, walks the element tree, extracts path geometry +
/// fill/stroke attributes, and rasterizes via agg-rust directly (no CSS
/// layout involved).
#[cfg(all(feature = "std", feature = "xml"))]
pub fn render_svg_to_png(
    svg_data: &[u8],
    target_width: u32,
    target_height: u32,
) -> Result<Vec<u8>, String> {
    let svg_str = core::str::from_utf8(svg_data)
        .map_err(|e| format!("SVG is not valid UTF-8: {e}"))?;

    let nodes = crate::xml::parse_xml_string(svg_str)
        .map_err(|e| format!("XML parse error: {e}"))?;

    // Find the <svg> root
    let node_slice: &[azul_core::xml::XmlNodeChild] = nodes.as_ref();
    let svg_node = node_slice.iter().find_map(|n| {
        if let azul_core::xml::XmlNodeChild::Element(e) = n {
            let tag = e.node_type.as_str().to_lowercase();
            if tag == "svg" { Some(e) } else { None }
        } else { None }
    }).ok_or_else(|| "No <svg> root element found".to_string())?;

    // Parse viewBox for coordinate mapping
    let vb = parse_viewbox(svg_node);
    let (vb_x, vb_y, vb_w, vb_h) = vb.unwrap_or((0.0, 0.0, target_width as f64, target_height as f64));

    let sx = target_width as f64 / vb_w;
    let sy = target_height as f64 / vb_h;
    let scale = sx.min(sy);

    let root_transform = TransAffine::new_custom(scale, 0.0, 0.0, scale, -vb_x * scale, -vb_y * scale);

    let mut pixmap = AzulPixmap::new(target_width, target_height)
        .ok_or_else(|| "Failed to create pixmap".to_string())?;
    pixmap.fill(255, 255, 255, 255);

    render_svg_group(svg_node, &mut pixmap, &root_transform);

    pixmap.encode_png().map_err(|e| format!("PNG encode error: {e}"))
}

#[cfg(all(feature = "std", feature = "xml"))]
fn parse_viewbox(node: &azul_core::xml::XmlNode) -> Option<(f64, f64, f64, f64)> {
    let vb = node.attributes.get_key("viewbox")
        .or_else(|| node.attributes.get_key("viewBox"))?;
    let nums: Vec<f64> = vb.as_str()
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() == 4 { Some((nums[0], nums[1], nums[2], nums[3])) } else { None }
}

/// Inherited SVG style (fill, stroke, stroke-width) that cascades from parent groups.
#[cfg(all(feature = "std", feature = "xml"))]
#[derive(Clone)]
struct SvgInheritedStyle {
    fill: Option<String>,       // None = not set (inherit default black)
    stroke: Option<String>,     // None = not set (inherit default none)
    stroke_width: Option<f64>,
}

#[cfg(all(feature = "std", feature = "xml"))]
impl Default for SvgInheritedStyle {
    fn default() -> Self {
        Self { fill: None, stroke: None, stroke_width: None }
    }
}

#[cfg(all(feature = "std", feature = "xml"))]
fn render_svg_group(
    node: &azul_core::xml::XmlNode,
    pixmap: &mut AzulPixmap,
    parent_transform: &TransAffine,
) {
    render_svg_group_with_style(node, pixmap, parent_transform, &SvgInheritedStyle::default());
}

#[cfg(all(feature = "std", feature = "xml"))]
fn render_svg_group_with_style(
    node: &azul_core::xml::XmlNode,
    pixmap: &mut AzulPixmap,
    parent_transform: &TransAffine,
    parent_style: &SvgInheritedStyle,
) {
    use azul_core::xml::{XmlNodeChild, XmlNode};
    use agg_rust::math_stroke::{LineCap, LineJoin};

    let group_transform = if let Some(t) = node.attributes.get_key("transform") {
        let mut tf = parse_svg_transform(t.as_str());
        tf.premultiply(parent_transform);
        tf
    } else {
        parent_transform.clone()
    };

    // Inherit style from this group's attributes
    let group_style = SvgInheritedStyle {
        fill: node.attributes.get_key("fill")
            .map(|s| s.as_str().to_string())
            .or_else(|| parent_style.fill.clone()),
        stroke: node.attributes.get_key("stroke")
            .map(|s| s.as_str().to_string())
            .or_else(|| parent_style.stroke.clone()),
        stroke_width: node.attributes.get_key("stroke-width")
            .and_then(|s| s.as_str().parse().ok())
            .or(parent_style.stroke_width),
    };

    for child in node.children.as_ref().iter() {
        let child_node = match child {
            XmlNodeChild::Element(e) => e,
            _ => continue,
        };

        let tag = child_node.node_type.as_str().to_lowercase();

        match tag.as_str() {
            "g" | "svg" => {
                render_svg_group_with_style(child_node, pixmap, &group_transform, &group_style);
            }
            "path" | "circle" | "rect" | "ellipse" | "line" | "polygon" | "polyline" => {
                let mut path = build_agg_path(child_node);
                if path.is_none() { continue; }
                let path = path.as_mut().unwrap();

                // Per-element transform
                let elem_transform = if let Some(t) = child_node.attributes.get_key("transform") {
                    let mut tf = parse_svg_transform(t.as_str());
                    tf.premultiply(&group_transform);
                    tf
                } else {
                    group_transform.clone()
                };

                // Fill: element overrides group
                let fill_attr = child_node.attributes.get_key("fill")
                    .map(|s| s.as_str().to_string())
                    .or_else(|| group_style.fill.clone());
                let fill_color = match fill_attr.as_deref() {
                    Some("none") => None,
                    Some(c) => parse_svg_color(c),
                    None => Some(Rgba8 { r: 0, g: 0, b: 0, a: 255 }), // SVG default
                };

                let fill_opacity = child_node.attributes.get_key("fill-opacity")
                    .and_then(|s| s.as_str().parse::<f64>().ok())
                    .unwrap_or(1.0);

                let opacity = child_node.attributes.get_key("opacity")
                    .and_then(|s| s.as_str().parse::<f64>().ok())
                    .unwrap_or(1.0);

                if let Some(mut color) = fill_color {
                    color.a = ((color.a as f64) * fill_opacity * opacity).min(255.0) as u8;

                    let fill_rule_str = child_node.attributes.get_key("fill-rule")
                        .map(|s| s.as_str().to_string());
                    let rule = match fill_rule_str.as_deref() {
                        Some("evenodd") => FillingRule::EvenOdd,
                        _ => FillingRule::NonZero,
                    };

                    agg_fill_transformed_path(pixmap, path, &color, rule, &elem_transform);
                }

                // Stroke: element overrides group
                let stroke_attr = child_node.attributes.get_key("stroke")
                    .map(|s| s.as_str().to_string())
                    .or_else(|| group_style.stroke.clone());
                let stroke_color = match stroke_attr.as_deref() {
                    Some("none") | None => None,
                    Some(c) => parse_svg_color(c),
                };

                if let Some(mut color) = stroke_color {
                    let stroke_opacity = child_node.attributes.get_key("stroke-opacity")
                        .and_then(|s| s.as_str().parse::<f64>().ok())
                        .unwrap_or(1.0);
                    color.a = ((color.a as f64) * stroke_opacity * opacity).min(255.0) as u8;

                    let stroke_width = child_node.attributes.get_key("stroke-width")
                        .and_then(|s| s.as_str().parse::<f64>().ok())
                        .or(group_style.stroke_width)
                        .unwrap_or(1.0);

                    let mut conv_stroke = ConvStroke::new(path.clone());
                    conv_stroke.set_width(stroke_width);
                    conv_stroke.set_line_cap(LineCap::Round);
                    conv_stroke.set_line_join(LineJoin::Round);

                    let mut transformed = ConvTransform::new(&mut conv_stroke, elem_transform.clone());
                    agg_fill_path(pixmap, &mut transformed, &color, FillingRule::NonZero);
                }
            }
            _ => {
                // Recurse into unknown containers (defs, symbol, etc.)
                render_svg_group_with_style(child_node, pixmap, &group_transform, &group_style);
            }
        }
    }
}

/// Build an agg PathStorage from an SVG shape element's attributes.
#[cfg(all(feature = "std", feature = "xml"))]
fn build_agg_path(node: &azul_core::xml::XmlNode) -> Option<PathStorage> {
    let tag = node.node_type.as_str().to_lowercase();
    match tag.as_str() {
        "path" => {
            let d = node.attributes.get_key("d")?;
            let mp = azul_core::svg_path_parser::parse_svg_path_d(d.as_str()).ok()?;
            Some(svg_multi_polygon_to_path_storage(&mp))
        }
        "circle" => {
            let cx = attr_f64(node, "cx");
            let cy = attr_f64(node, "cy");
            let r = attr_f64(node, "r");
            if r <= 0.0 { return None; }
            let mp = azul_core::svg_path_parser::svg_circle_to_paths(cx as f32, cy as f32, r as f32);
            let multi = azul_core::svg::SvgMultiPolygon {
                rings: azul_core::svg::SvgPathVec::from_vec(vec![mp]),
            };
            Some(svg_multi_polygon_to_path_storage(&multi))
        }
        "rect" => {
            let x = attr_f64(node, "x");
            let y = attr_f64(node, "y");
            let w = attr_f64(node, "width");
            let h = attr_f64(node, "height");
            let rx = attr_f64(node, "rx");
            let ry = if let Some(v) = node.attributes.get_key("ry") {
                v.as_str().parse().unwrap_or(rx)
            } else { rx };
            if w <= 0.0 || h <= 0.0 { return None; }
            let mp = azul_core::svg_path_parser::svg_rect_to_path(x as f32, y as f32, w as f32, h as f32, rx as f32, ry as f32);
            let multi = azul_core::svg::SvgMultiPolygon {
                rings: azul_core::svg::SvgPathVec::from_vec(vec![mp]),
            };
            Some(svg_multi_polygon_to_path_storage(&multi))
        }
        "ellipse" => {
            let cx = attr_f64(node, "cx");
            let cy = attr_f64(node, "cy");
            let rx = attr_f64(node, "rx");
            let ry = attr_f64(node, "ry");
            if rx <= 0.0 || ry <= 0.0 { return None; }
            // Use circle path with scaling
            let mp = azul_core::svg_path_parser::svg_circle_to_paths(cx as f32, cy as f32, 1.0);
            let multi = azul_core::svg::SvgMultiPolygon {
                rings: azul_core::svg::SvgPathVec::from_vec(vec![mp]),
            };
            let mut ps = svg_multi_polygon_to_path_storage(&multi);
            // Scale ellipse: we'll just build it directly instead
            let mut path = PathStorage::new();
            const KAPPA: f64 = 0.5522847498;
            let kx = rx * KAPPA;
            let ky = ry * KAPPA;
            path.move_to(cx, cy - ry);
            path.curve4(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
            path.curve4(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
            path.curve4(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
            path.curve4(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
            path.close_polygon(PATH_FLAGS_NONE);
            Some(path)
        }
        "line" => {
            let x1 = attr_f64(node, "x1");
            let y1 = attr_f64(node, "y1");
            let x2 = attr_f64(node, "x2");
            let y2 = attr_f64(node, "y2");
            let mut path = PathStorage::new();
            path.move_to(x1, y1);
            path.line_to(x2, y2);
            Some(path)
        }
        "polygon" | "polyline" => {
            let pts_str = node.attributes.get_key("points")?;
            let nums: Vec<f64> = pts_str.as_str()
                .split(|c: char| c == ',' || c.is_ascii_whitespace())
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect();
            if nums.len() < 4 { return None; }
            let mut path = PathStorage::new();
            path.move_to(nums[0], nums[1]);
            for chunk in nums[2..].chunks_exact(2) {
                path.line_to(chunk[0], chunk[1]);
            }
            if tag == "polygon" {
                path.close_polygon(PATH_FLAGS_NONE);
            }
            Some(path)
        }
        _ => None,
    }
}

#[cfg(all(feature = "std", feature = "xml"))]
fn attr_f64(node: &azul_core::xml::XmlNode, key: &str) -> f64 {
    node.attributes.get_key(key)
        .and_then(|s| s.as_str().parse().ok())
        .unwrap_or(0.0)
}

/// Convert SvgMultiPolygon to agg PathStorage.
#[cfg(all(feature = "std", feature = "xml"))]
fn svg_multi_polygon_to_path_storage(mp: &azul_core::svg::SvgMultiPolygon) -> PathStorage {
    let mut path = PathStorage::new();
    for ring in mp.rings.as_ref().iter() {
        let mut first = true;
        for item in ring.items.as_ref().iter() {
            match item {
                azul_core::svg::SvgPathElement::Line(l) => {
                    if first {
                        path.move_to(l.start.x as f64, l.start.y as f64);
                        first = false;
                    }
                    path.line_to(l.end.x as f64, l.end.y as f64);
                }
                azul_core::svg::SvgPathElement::QuadraticCurve(q) => {
                    if first {
                        path.move_to(q.start.x as f64, q.start.y as f64);
                        first = false;
                    }
                    path.curve3(q.ctrl.x as f64, q.ctrl.y as f64, q.end.x as f64, q.end.y as f64);
                }
                azul_core::svg::SvgPathElement::CubicCurve(c) => {
                    if first {
                        path.move_to(c.start.x as f64, c.start.y as f64);
                        first = false;
                    }
                    path.curve4(
                        c.ctrl_1.x as f64, c.ctrl_1.y as f64,
                        c.ctrl_2.x as f64, c.ctrl_2.y as f64,
                        c.end.x as f64, c.end.y as f64,
                    );
                }
            }
        }
        path.close_polygon(PATH_FLAGS_NONE);
    }
    path
}

/// Parse SVG transform attribute (supports matrix, translate, scale, rotate).
#[cfg(all(feature = "std", feature = "xml"))]
fn parse_svg_transform(s: &str) -> TransAffine {
    let s = s.trim();
    if s.starts_with("matrix(") {
        let inner = &s[7..s.len()-1];
        let nums: Vec<f64> = inner
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        if nums.len() == 6 {
            return TransAffine::new_custom(nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]);
        }
    } else if s.starts_with("translate(") {
        let inner = &s[10..s.len()-1];
        let nums: Vec<f64> = inner
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        let tx = nums.first().copied().unwrap_or(0.0);
        let ty = nums.get(1).copied().unwrap_or(0.0);
        return TransAffine::new_custom(1.0, 0.0, 0.0, 1.0, tx, ty);
    } else if s.starts_with("scale(") {
        let inner = &s[6..s.len()-1];
        let nums: Vec<f64> = inner
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        let sx = nums.first().copied().unwrap_or(1.0);
        let sy = nums.get(1).copied().unwrap_or(sx);
        return TransAffine::new_custom(sx, 0.0, 0.0, sy, 0.0, 0.0);
    } else if s.starts_with("rotate(") {
        let inner = &s[7..s.len()-1];
        let nums: Vec<f64> = inner
            .split(|c: char| c == ',' || c.is_ascii_whitespace())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        let angle = nums.first().copied().unwrap_or(0.0).to_radians();
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        return TransAffine::new_custom(cos_a, sin_a, -sin_a, cos_a, 0.0, 0.0);
    }
    TransAffine::new()
}

/// Parse SVG color string (#RRGGBB, #RGB, named colors).
#[cfg(all(feature = "std", feature = "xml"))]
fn parse_svg_color(s: &str) -> Option<Rgba8> {
    let s = s.trim();
    if s.starts_with('#') {
        let hex = &s[1..];
        return match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Rgba8 { r, g, b, a: 255 })
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Rgba8 { r, g, b, a: 255 })
            }
            _ => None,
        };
    }
    match s.to_lowercase().as_str() {
        "black" => Some(Rgba8 { r: 0, g: 0, b: 0, a: 255 }),
        "white" => Some(Rgba8 { r: 255, g: 255, b: 255, a: 255 }),
        "red" => Some(Rgba8 { r: 255, g: 0, b: 0, a: 255 }),
        "green" => Some(Rgba8 { r: 0, g: 128, b: 0, a: 255 }),
        "blue" => Some(Rgba8 { r: 0, g: 0, b: 255, a: 255 }),
        "yellow" => Some(Rgba8 { r: 255, g: 255, b: 0, a: 255 }),
        "orange" => Some(Rgba8 { r: 255, g: 165, b: 0, a: 255 }),
        "gold" => Some(Rgba8 { r: 255, g: 215, b: 0, a: 255 }),
        _ => None,
    }
}
