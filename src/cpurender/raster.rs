#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use super::*;

use std::collections::HashMap;
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::{DecodedImage, ImageRef, RendererResources};
use azul_core::ui_solver::GlyphInstance;
use azul_css::props::basic::{ColorOrSystem, ColorU, FontRef};
use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
use azul_css::props::style::filter::StyleFilter;
use azul_css::props::style::box_shadow::StyleBoxShadow;
use agg_rust::basics::{FillingRule, PATH_FLAGS_NONE};
use agg_rust::blur::stack_blur_rgba32;
use agg_rust::color::Rgba8;
use agg_rust::conv_stroke::ConvStroke;
use agg_rust::gradient_lut::GradientLut;
use agg_rust::path_storage::PathStorage;
use agg_rust::pixfmt_rgba::PixfmtRgba32;
use agg_rust::rasterizer_scanline_aa::RasterizerScanlineAa;
use agg_rust::renderer_base::RendererBase;
use agg_rust::renderer_scanline::render_scanlines_aa_solid;
use agg_rust::rendering_buffer::RowAccessor;
use agg_rust::rounded_rect::RoundedRect;
use agg_rust::scanline_u::ScanlineU8;
use agg_rust::span_gradient::{GradientConic, GradientRadialD, GradientX};
use agg_rust::trans_affine::TransAffine;
use crate::font::parsed::ParsedFont;
use crate::glyph_cache::GlyphCache;
use crate::solver3::display_list::{BorderRadius, DisplayList, DisplayListItem, LocalScrollId};
use crate::text3::cache::{FontHash, FontManager};

const MAX_SHADOW_PIXBUF_SIZE: u32 = 4096;

/// Fallback color used when a `system:*` keyword cannot be resolved
/// (for example because no `SystemStyle` is attached to the
/// [`CpuRenderState`], or because the requested key is unset on the
/// current platform). CSS Images Level 4 leaves the color undefined in
/// this case; transparent black means the stop simply contributes
/// nothing to the gradient instead of poisoning it with an arbitrary
/// visible color (the previous behaviour was hardcoded mid-gray, which
/// produced visibly wrong output).
const SYSTEM_COLOR_FALLBACK: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

/// Resolve a `ColorOrSystem` against the optional system palette.
///
/// Concrete colors are returned verbatim. `system:*` keywords are
/// resolved against `system_colors` when available and fall back to
/// `SYSTEM_COLOR_FALLBACK` otherwise.
#[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
fn resolve_color(
    color: &ColorOrSystem,
    system_colors: Option<&azul_css::system::SystemColors>,
) -> ColorU {
    match (color, system_colors) {
        (ColorOrSystem::Color(c), _) => *c,
        (ColorOrSystem::System(_), Some(sc)) => color.resolve(sc, SYSTEM_COLOR_FALLBACK),
        (ColorOrSystem::System(_), None) => SYSTEM_COLOR_FALLBACK,
    }
}

/// Build a `GradientLut` from normalized linear color stops.
fn build_gradient_lut_linear(
    stops: &azul_css::props::style::background::NormalizedLinearColorStopVec,
    system_colors: Option<&azul_css::system::SystemColors>,
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
        let offset = f64::from(stop.offset.normalized()); // 0.0..1.0
        let c = resolve_color(&stop.color, system_colors);
        lut.add_color(
            offset,
            Rgba8::new(u32::from(c.r), u32::from(c.g), u32::from(c.b), u32::from(c.a)),
        );
    }
    lut.build_lut();
    lut
}

/// Build a `GradientLut` from normalized radial (conic) color stops.
fn build_gradient_lut_radial(
    stops: &azul_css::props::style::background::NormalizedRadialColorStopVec,
    system_colors: Option<&azul_css::system::SystemColors>,
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
        let offset = f64::from((stop.angle.to_degrees() / 360.0).clamp(0.0, 1.0));
        let c = resolve_color(&stop.color, system_colors);
        lut.add_color(
            offset,
            Rgba8::new(u32::from(c.r), u32::from(c.g), u32::from(c.b), u32::from(c.a)),
        );
    }
    lut.build_lut();
    lut
}

/// Resolve a background position to (`x_fraction`, `y_fraction`) in 0..1 range.
fn resolve_background_position(
    pos: &azul_css::props::style::background::StyleBackgroundPosition,
    width: f32,
    height: f32,
) -> (f32, f32) {
    use azul_css::props::style::background::{
        BackgroundPositionHorizontal, BackgroundPositionVertical,
    };

    let x = match pos.horizontal {
        BackgroundPositionHorizontal::Left => 0.0,
        BackgroundPositionHorizontal::Center => 0.5,
        BackgroundPositionHorizontal::Right => 1.0,
        BackgroundPositionHorizontal::Exact(px) => {
            let val = px.to_pixels_internal(width, 16.0, 16.0);
            if width > 0.0 {
                val / width
            } else {
                0.5
            }
        }
    };
    let y = match pos.vertical {
        BackgroundPositionVertical::Top => 0.0,
        BackgroundPositionVertical::Center => 0.5,
        BackgroundPositionVertical::Bottom => 1.0,
        BackgroundPositionVertical::Exact(px) => {
            let val = px.to_pixels_internal(height, 16.0, 16.0);
            if height > 0.0 {
                val / height
            } else {
                0.5
            }
        }
    };
    (x, y)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // software rasterizer: bounded pixel/coord/colour casts
fn render_linear_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::LinearGradient,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
    system_colors: Option<&azul_css::system::SystemColors>,
) {
    use azul_css::props::basic::geometry::{LayoutRect, LayoutSize};

    let Some(rect) = logical_rect_to_az_rect(bounds, dpi_factor) else {
        return;
    };

    let stops = gradient.stops.as_ref();
    if stops.is_empty() {
        return;
    }

    let lut = build_gradient_lut_linear(&gradient.stops, system_colors);

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
    let x1 = f64::from(rect.x) + from_pt.x as f64;
    let y1 = f64::from(rect.y) + from_pt.y as f64;
    let x2 = f64::from(rect.x) + to_pt.x as f64;
    let y2 = f64::from(rect.y) + to_pt.y as f64;

    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = dx.hypot(dy);
    if len < 0.001 {
        return;
    }

    // gradient-space (0..100, 0) → pixel-space line (x1,y1)→(x2,y2). Use agg's
    // helper so the composition order is T * R * S — hand-rolling it via
    // new_translation().rotate().scale() pre-multiplies and ends up as
    // S * R * T, which rotates the translation and yields out-of-range gx.
    let mut transform = TransAffine::new_line_segment(x1, y1, x2, y2, 100.0);
    transform.invert();

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient_clipped(
        pixmap, &mut path, &lut, GradientX, transform, 0.0, 100.0, clip,
    );
}

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
fn render_radial_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::RadialGradient,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
    system_colors: Option<&azul_css::system::SystemColors>,
) {
    use azul_css::props::style::background::{RadialGradientSize, Shape};

    let Some(rect) = logical_rect_to_az_rect(bounds, dpi_factor) else {
        return;
    };

    let stops = gradient.stops.as_ref();
    if stops.is_empty() {
        return;
    }

    let lut = build_gradient_lut_linear(&gradient.stops, system_colors);

    let w = f64::from(rect.width);
    let h = f64::from(rect.height);

    // Compute center from position
    let (cx_frac, cy_frac) =
        resolve_background_position(&gradient.position, rect.width, rect.height);
    let cx = f64::from(rect.x) + f64::from(cx_frac) * w;
    let cy = f64::from(rect.y) + f64::from(cy_frac) * h;

    // Compute radius based on shape and size
    let radius = match gradient.size {
        RadialGradientSize::ClosestSide => {
            let dx = (f64::from(cx_frac) * w).min((1.0 - f64::from(cx_frac)) * w);
            let dy = (f64::from(cy_frac) * h).min((1.0 - f64::from(cy_frac)) * h);
            match gradient.shape {
                Shape::Circle => dx.min(dy),
                Shape::Ellipse => dx.min(dy), // simplified
            }
        }
        RadialGradientSize::FarthestSide => {
            let dx = (f64::from(cx_frac) * w).max((1.0 - f64::from(cx_frac)) * w);
            let dy = (f64::from(cy_frac) * h).max((1.0 - f64::from(cy_frac)) * h);
            match gradient.shape {
                Shape::Circle => dx.max(dy),
                Shape::Ellipse => dx.max(dy),
            }
        }
        RadialGradientSize::ClosestCorner => {
            let dx = (f64::from(cx_frac) * w).min((1.0 - f64::from(cx_frac)) * w);
            let dy = (f64::from(cy_frac) * h).min((1.0 - f64::from(cy_frac)) * h);
            dx.hypot(dy)
        }
        RadialGradientSize::FarthestCorner => {
            let dx = (f64::from(cx_frac) * w).max((1.0 - f64::from(cx_frac)) * w);
            let dy = (f64::from(cy_frac) * h).max((1.0 - f64::from(cy_frac)) * h);
            dx.hypot(dy)
        }
    };

    if radius < 0.001 {
        return;
    }

    // Gradient-space (radius=100 at distance=100) → pixel-space around (cx, cy).
    // Build as T * S (scale first, then translate) so S only affects the radius.
    // scale() pre-multiplies so we must start from scaling matrix.
    let mut transform = TransAffine::new_scaling_uniform(radius / 100.0);
    transform.translate(cx, cy);
    transform.invert();

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient_clipped(
        pixmap,
        &mut path,
        &lut,
        GradientRadialD,
        transform,
        0.0,
        100.0,
        clip,
    );
}

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
fn render_conic_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::ConicGradient,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
    system_colors: Option<&azul_css::system::SystemColors>,
) {
    let Some(rect) = logical_rect_to_az_rect(bounds, dpi_factor) else {
        return;
    };

    let stops = gradient.stops.as_ref();
    if stops.is_empty() {
        return;
    }

    let lut = build_gradient_lut_radial(&gradient.stops, system_colors);

    let w = f64::from(rect.width);
    let h = f64::from(rect.height);

    // Compute center
    let (cx_frac, cy_frac) = resolve_background_position(&gradient.center, rect.width, rect.height);
    let cx = f64::from(rect.x) + f64::from(cx_frac) * w;
    let cy = f64::from(rect.y) + f64::from(cy_frac) * h;

    // Start angle (CSS conic gradients start at 12 o'clock = -90deg in math coords)
    let start_angle_deg = gradient.angle.to_degrees();
    let start_angle_rad = f64::from(start_angle_deg - 90.0).to_radians();

    // Forward: gradient angle θ → pixel rotated by start_angle around (cx, cy).
    // Build as T * R so rotation is applied before translation (rotate() pre-multiplies,
    // so start from rotation matrix and translate last).
    let mut transform = TransAffine::new_rotation(start_angle_rad);
    transform.translate(cx, cy);
    transform.invert();

    // GradientConic maps atan2(y,x) * d / pi, covering [0, d] for the half-circle.
    // We use d2 = 100 as the range; the LUT maps 0..1 over that.
    let d2 = 100.0;

    let mut path = if border_radius.is_zero() {
        build_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    agg_fill_gradient_clipped(
        pixmap,
        &mut path,
        &lut,
        GradientConic,
        transform,
        0.0,
        d2,
        clip,
    );
}

// ============================================================================
// Box shadow rendering
// ============================================================================

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)] // software rasterizer: bounded pixel/coord/colour casts
fn render_box_shadow(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    shadow: &StyleBoxShadow,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> Result<(), String> {
    use azul_css::props::style::box_shadow::BoxShadowClipMode;

    let Some(rect) = logical_rect_to_az_rect(bounds, dpi_factor) else {
        return Ok(());
    };

    let offset_x =
        shadow
            .offset_x
            .inner
            .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
            * dpi_factor;
    let offset_y =
        shadow
            .offset_y
            .inner
            .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
            * dpi_factor;
    let blur_r =
        (shadow
            .blur_radius
            .inner
            .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
            * dpi_factor)
            .max(0.0);
    let spread =
        shadow
            .spread_radius
            .inner
            .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
            * dpi_factor;

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

    if sw == 0 || sh == 0 || sw > MAX_SHADOW_PIXBUF_SIZE || sh > MAX_SHADOW_PIXBUF_SIZE {
        return Ok(());
    }

    // Create temp buffer and draw the shadow shape into it
    let mut tmp = AzulPixmap::new(sw, sh).ok_or("cannot create shadow pixmap")?;
    tmp.fill(0, 0, 0, 0); // transparent

    // The shape origin within the temp buffer
    let shape_x = padding + spread;
    let shape_y = padding + spread;
    let Some(shape_rect) = AzRect::from_xywh(shape_x, shape_y, rect.width, rect.height) else {
        return Ok(());
    };

    let agg_color = Rgba8::new(
        u32::from(color.r),
        u32::from(color.g),
        u32::from(color.b),
        u32::from(color.a),
    );
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
        let mut ra = unsafe { RowAccessor::new_with_buf(tmp.data.as_mut_ptr(), sw, sh, stride) };
        stack_blur_rgba32(&mut ra, blur_radius, blur_radius);
    }

    // Blit the shadow buffer onto the main pixmap
    let dst_x = shadow_x as i32;
    let dst_y = shadow_y as i32;
    blit_buffer(pixmap, &tmp.data, sw, sh, dst_x, dst_y);

    Ok(())
}

/// Entry on the mask/opacity stack.
#[derive(Debug)]
pub enum MaskEntry {
    /// Image mask clip (R8 mask).
    ImageMask {
        snapshot: Vec<u8>,
        mask_data: Vec<u8>,
        origin_x: i32,
        origin_y: i32,
        width: u32,
        height: u32,
    },
    /// Opacity layer.
    Opacity {
        snapshot: Vec<u8>,
        rect: AzRect,
        opacity: f32,
    },
}

/// Extract and scale mask image data (R8) to target dimensions.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)] // software rasterizer: bounded pixel/coord/colour casts
fn extract_mask_data(mask_image: &ImageRef, target_w: u32, target_h: u32) -> Option<Vec<u8>> {
    let image_data = mask_image.get_data();
    let (mask_bytes, src_w, src_h) = match image_data {
        DecodedImage::Raw((descriptor, data)) => {
            let w = descriptor.width as u32;
            let h = descriptor.height as u32;
            if w == 0 || h == 0 {
                return None;
            }
            let bytes = match data {
                azul_core::resources::ImageData::Raw(shared) => shared.as_ref(),
                azul_core::resources::ImageData::External(_) => return None,
            };
            match descriptor.format {
                azul_core::resources::RawImageFormat::R8 => (bytes.to_vec(), w, h),
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
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)] // software rasterizer: bounded pixel/coord/colour casts
fn apply_mask(pixmap: &mut AzulPixmap, entry: &MaskEntry) {
    let (snapshot, mask_data, origin_x, origin_y, width, height) = match entry {
        MaskEntry::ImageMask {
            snapshot,
            mask_data,
            origin_x,
            origin_y,
            width,
            height,
        } => (
            snapshot,
            mask_data.as_slice(),
            *origin_x,
            *origin_y,
            *width,
            *height,
        ),
        MaskEntry::Opacity{ .. } => return,
    };

    let pw = pixmap.width as i32;
    let ph = pixmap.height as i32;

    for py in 0..height as i32 {
        let dy = origin_y + py;
        if dy < 0 || dy >= ph {
            continue;
        }
        for px in 0..width as i32 {
            let dx = origin_x + px;
            if dx < 0 || dx >= pw {
                continue;
            }

            let mi = (py as u32 * width + px as u32) as usize;
            let mask_val = u32::from(mask_data.get(mi).copied().unwrap_or(0));

            let pi = ((dy as u32 * pixmap.width + dx as u32) * 4) as usize;
            let si = ((py as u32 * width + px as u32) * 4) as usize;

            if pi + 3 >= pixmap.data.len() || si + 3 >= snapshot.len() {
                continue;
            }

            // Blend: result = snapshot * (255 - mask) + current * mask
            // mask_val 255 = fully visible (keep current), 0 = fully clipped (restore snapshot)
            let inv_mask = 255 - mask_val;
            for c in 0..4 {
                let snap_c = u32::from(snapshot[si + c]);
                let cur_c = u32::from(pixmap.data[pi + c]);
                pixmap.data[pi + c] = ((cur_c * mask_val + snap_c * inv_mask) / 255) as u8;
            }
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub width: f32,
    pub height: f32,
    pub dpi_factor: f32,
}

/// Reuse `retained` pixmap if it matches the target dimensions, otherwise allocate new.
fn acquire_pixmap(retained: Option<AzulPixmap>, w: u32, h: u32) -> Result<AzulPixmap, String> {
    if let Some(p) = retained {
        if p.width == w && p.height == h {
            return Ok(p);
        }
    }
    AzulPixmap::new(w, h).ok_or_else(|| "cannot create pixmap".to_string())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // software rasterizer: bounded pixel/coord/colour casts
/// # Errors
///
/// Returns an error string if rendering fails.
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

    let mut pixmap = acquire_pixmap(
        None,
        (width * dpi_factor) as u32,
        (height * dpi_factor) as u32,
    )?;
    pixmap.fill(255, 255, 255, 255);

    render_display_list(dl, &mut pixmap, dpi_factor, res, None, glyph_cache)?;

    Ok(pixmap)
}

/// Render a display list using fonts from `FontManager` directly.
/// This is used in reftest scenarios where `RendererResources` doesn't have fonts registered.
/// # Errors
///
/// Returns an error string if rendering fails.
pub fn render_with_font_manager(
    dl: &DisplayList,
    res: &RendererResources,
    font_manager: &FontManager<FontRef>,
    opts: RenderOptions,
    glyph_cache: &mut GlyphCache,
) -> Result<AzulPixmap, String> {
    let empty_state = CpuRenderState::new(ScrollOffsetMap::new());
    render_with_font_manager_and_scroll(dl, res, font_manager, opts, glyph_cache, &empty_state)
}

/// Render with `FontManager` and explicit render state (scroll offsets + GPU values).
/// Used by `take_screenshot` to render with the current scroll/transform/opacity state.
/// # Errors
///
/// Returns an error string if rendering fails.
pub fn render_with_font_manager_and_scroll(
    dl: &DisplayList,
    res: &RendererResources,
    font_manager: &FontManager<FontRef>,
    opts: RenderOptions,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
) -> Result<AzulPixmap, String> {
    render_with_font_manager_and_scroll_retained(
        dl,
        res,
        font_manager,
        opts,
        glyph_cache,
        render_state,
        None,
    )
}

/// Render with optional retained pixmap. If `retained` is Some and matches
/// the target dimensions, it is reused (cleared to white) instead of
/// allocating a fresh buffer. The pixmap is returned regardless.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // software rasterizer: bounded pixel/coord/colour casts
/// # Errors
///
/// Returns an error string if rendering fails.
pub fn render_with_font_manager_and_scroll_retained(
    dl: &DisplayList,
    res: &RendererResources,
    font_manager: &FontManager<FontRef>,
    opts: RenderOptions,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
    retained: Option<AzulPixmap>,
) -> Result<AzulPixmap, String> {
    let RenderOptions {
        width,
        height,
        dpi_factor,
    } = opts;

    let pw = (width * dpi_factor) as u32;
    let ph = (height * dpi_factor) as u32;
    let mut pixmap = acquire_pixmap(retained, pw, ph)?;
    pixmap.fill(255, 255, 255, 255);

    render_display_list_with_state(
        dl,
        &mut pixmap,
        dpi_factor,
        res,
        Some(font_manager),
        glyph_cache,
        render_state,
    )?;

    Ok(pixmap)
}

/// Scroll offsets keyed by `scroll_id` (`LocalScrollId`).
/// Passed to the renderer so it can look up the current scroll position
/// for each `PushScrollFrame` without embedding it in the display list.
pub type ScrollOffsetMap = HashMap<LocalScrollId, (f32, f32)>;

/// Consolidated render-time state for CPU rendering.
///
/// Bundles scroll offsets and GPU-animated values (transforms, opacities)
/// that `WebRender` would normally manage internally. In cpurender these
/// are looked up from the `GpuValueCache` at screenshot time.
#[derive(Debug)]
pub struct CpuRenderState {
    /// Scroll offsets by `scroll_id`
    pub scroll_offsets: ScrollOffsetMap,
    /// Transform values keyed by TransformKey.id — scrollbar thumb positions
    /// and CSS transforms that are GPU-animated in `WebRender`.
    pub transforms: HashMap<usize, azul_core::transform::ComputedTransform3D>,
    /// Opacity values keyed by OpacityKey.id — scrollbar fade-in/out.
    /// For `WhenScrolling` mode, opacity is 1.0 when recently scrolled,
    /// fades to 0.0 after idle. For Always mode, opacity is always 1.0.
    pub opacities: HashMap<usize, f32>,
    /// System style for resolving system color references inside gradient
    /// stops (e.g. `system:accent` in macOS button backgrounds). When None,
    /// system color stops fall back to a transparent color.
    pub system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
    /// Display lists of nested `VirtualView` child DOMs, keyed by their
    /// `child_dom_id`. The `WebRender` path composites these via separate pipelines;
    /// the CPU path has no pipelines, so the `DisplayListItem::VirtualView` arm
    /// recursively rasterises the child's display list from here (translated to the
    /// item's `bounds.origin`, clipped to `bounds`). Empty for non-window renders.
    pub virtual_view_display_lists:
        std::collections::BTreeMap<azul_core::dom::DomId, std::sync::Arc<DisplayList>>,
    /// Resolved images for `DecodedImage::Callback` `<img>` nodes, keyed by the
    /// callback image's hash. The CPU renderer can't invoke `RenderImageCallback`s
    /// itself (it would draw a grey placeholder); the backend pre-invokes them
    /// via [`crate::window::LayoutWindow::invoke_cpu_image_callbacks`] and passes
    /// the produced images here, where the `DisplayListItem::Image` arm looks
    /// them up by hash. Empty when there are no callback images.
    pub image_callback_results:
        std::collections::BTreeMap<azul_core::resources::ImageRefHash, ImageRef>,
}

impl CpuRenderState {
    #[must_use] pub fn new(scroll_offsets: ScrollOffsetMap) -> Self {
        Self {
            scroll_offsets,
            transforms: HashMap::new(),
            opacities: HashMap::new(),
            system_style: None,
            virtual_view_display_lists: std::collections::BTreeMap::new(),
            image_callback_results: std::collections::BTreeMap::new(),
        }
    }

    /// Provide the resolved `RenderImageCallback` images (see the field doc).
    #[must_use] pub fn with_image_callback_results(
        mut self,
        results: std::collections::BTreeMap<
            azul_core::resources::ImageRefHash,
            ImageRef,
        >,
    ) -> Self {
        self.image_callback_results = results;
        self
    }

    /// Provide the nested `VirtualView` child DOM display lists so the CPU
    /// renderer can composite them (see the field doc).
    #[must_use] pub fn with_virtual_view_display_lists(
        mut self,
        lists: std::collections::BTreeMap<azul_core::dom::DomId, std::sync::Arc<DisplayList>>,
    ) -> Self {
        self.virtual_view_display_lists = lists;
        self
    }

    /// Attach a `SystemStyle` so the renderer can resolve `system:*` color
    /// keywords (e.g. in gradient stops) against the live OS palette.
    #[must_use] pub fn with_system_style(
        mut self,
        system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
    ) -> Self {
        self.system_style = system_style;
        self
    }

    /// Build from a `GpuValueCache` snapshot.
    #[must_use] pub fn from_gpu_cache(
        gpu_cache: Option<&azul_core::gpu::GpuValueCache>,
        dom_id: azul_core::dom::DomId,
        scroll_offsets: &ScrollOffsetMap,
    ) -> Self {
        let (transforms, opacities) = extract_gpu_values(gpu_cache, dom_id);
        Self {
            scroll_offsets: scroll_offsets.clone(),
            transforms,
            opacities,
            system_style: None,
            virtual_view_display_lists: std::collections::BTreeMap::new(),
            image_callback_results: std::collections::BTreeMap::new(),
        }
    }
}

/// Flatten the GPU value cache into `key.id → value` maps — the SAME
/// extraction `CpuRenderState::from_gpu_cache` feeds the renderer with.
///
/// Exposed separately so the damage layer can diff the values frame-to-frame:
/// scrollbar thumb position / fade opacity / drag & CSS transforms change
/// WITHOUT any display-list item changing (items only carry the keys), so a
/// pure item diff reports "visually equal" while the frame must repaint.
#[must_use] pub fn extract_gpu_values(
    gpu_cache: Option<&azul_core::gpu::GpuValueCache>,
    dom_id: azul_core::dom::DomId,
) -> (
    HashMap<usize, azul_core::transform::ComputedTransform3D>,
    HashMap<usize, f32>,
) {
    {
        let mut transforms = HashMap::new();
        let mut opacities = HashMap::new();

        if let Some(cache) = gpu_cache {
            // Scrollbar thumb transforms (vertical)
            for (node_id, key) in &cache.transform_keys {
                if let Some(value) = cache.current_transform_values.get(node_id) {
                    transforms.insert(key.id, *value);
                }
            }
            // Scrollbar thumb transforms (horizontal)
            for (node_id, key) in &cache.h_transform_keys {
                if let Some(value) = cache.h_current_transform_values.get(node_id) {
                    transforms.insert(key.id, *value);
                }
            }
            // CSS transforms
            for (node_id, key) in &cache.css_transform_keys {
                if let Some(value) = cache.css_current_transform_values.get(node_id) {
                    transforms.insert(key.id, *value);
                }
            }
            // Scrollbar opacity (vertical)
            for ((d, node_id), key) in &cache.scrollbar_v_opacity_keys {
                if *d == dom_id {
                    if let Some(&value) = cache.scrollbar_v_opacity_values.get(&(*d, *node_id)) {
                        opacities.insert(key.id, value);
                    }
                }
            }
            // Scrollbar opacity (horizontal)
            for ((d, node_id), key) in &cache.scrollbar_h_opacity_keys {
                if *d == dom_id {
                    if let Some(&value) = cache.scrollbar_h_opacity_values.get(&(*d, *node_id)) {
                        opacities.insert(key.id, value);
                    }
                }
            }
            // CSS opacity
            for (node_id, key) in &cache.opacity_keys {
                if let Some(&value) = cache.current_opacity_values.get(node_id) {
                    opacities.insert(key.id, value);
                }
            }
        }

        (transforms, opacities)
    }
}

fn render_display_list(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
) -> Result<(), String> {
    let empty_state = CpuRenderState::new(ScrollOffsetMap::new());
    render_display_list_with_state(
        display_list,
        pixmap,
        dpi_factor,
        renderer_resources,
        font_manager,
        glyph_cache,
        &empty_state,
    )
}

fn render_display_list_with_state(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
) -> Result<(), String> {
    let mut transform_stack = vec![TransAffine::new()]; // identity
    let mut clip_stack: Vec<Option<AzRect>> = vec![None];
    let mut mask_stack: Vec<MaskEntry> = Vec::new();
    // Accumulated scroll offset stack. Each PushScrollFrame pushes
    // (parent_offset_x + scroll_x, parent_offset_y + scroll_y).
    // Items inside a scroll frame have their bounds shifted by the
    // accumulated offset before rendering.
    let mut scroll_offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];
    let mut text_shadow_stack: Vec<StyleBoxShadow> = Vec::new();

    let _p_loop = crate::probe::Probe::span("raster_loop");
    for item in &display_list.items {
        let _p_item = crate::probe::Probe::span(probe_label_for_item(item));
        render_single_item(
            item,
            pixmap,
            dpi_factor,
            renderer_resources,
            font_manager,
            glyph_cache,
            &mut transform_stack,
            &mut clip_stack,
            &mut mask_stack,
            &mut scroll_offset_stack,
            &mut text_shadow_stack,
            render_state,
        )?;
    }

    Ok(())
}

/// Compact item-kind label for [`crate::probe`]. Names must be `'static`
/// strings (probe events store `&'static str` for cheap aggregation),
/// hence the closed match instead of formatting `Debug`.
#[inline]
const fn probe_label_for_item(item: &DisplayListItem) -> &'static str {
    use crate::solver3::display_list::DisplayListItem as I;
    match item {
        I::Rect { .. } => "dl:rect",
        I::SelectionRect { .. } => "dl:sel_rect",
        I::CursorRect { .. } => "dl:cursor",
        I::Border { .. } => "dl:border",
        I::Text { .. } => "dl:text",
        I::TextLayout { .. } => "dl:text_layout",
        I::Image { .. } => "dl:image",
        I::ScrollBar { .. } => "dl:scrollbar_raw",
        I::ScrollBarStyled { .. } => "dl:scrollbar",
        I::PushClip { .. } => "dl:push_clip",
        I::PopClip => "dl:pop_clip",
        I::PushScrollFrame { .. } => "dl:push_scroll",
        I::PopScrollFrame => "dl:pop_scroll",
        I::PushStackingContext { .. } => "dl:push_stack",
        I::PopStackingContext => "dl:pop_stack",
        I::PushReferenceFrame { .. } => "dl:push_ref",
        I::PopReferenceFrame => "dl:pop_ref",
        I::PushOpacity { .. } => "dl:push_opacity",
        I::PopOpacity => "dl:pop_opacity",
        I::PushFilter { .. } => "dl:push_filter",
        I::PopFilter => "dl:pop_filter",
        I::PushBackdropFilter { .. } => "dl:push_bdfilter",
        I::PopBackdropFilter => "dl:pop_bdfilter",
        I::PushTextShadow { .. } => "dl:push_tshadow",
        I::PopTextShadow => "dl:pop_tshadow",
        I::PushImageMaskClip { .. } => "dl:push_imask",
        I::PopImageMaskClip => "dl:pop_imask",
        I::LinearGradient { .. } => "dl:linear_grad",
        I::RadialGradient { .. } => "dl:radial_grad",
        I::ConicGradient { .. } => "dl:conic_grad",
        I::BoxShadow { .. } => "dl:box_shadow",
        I::Underline { .. } => "dl:underline",
        I::Strikethrough { .. } => "dl:strike",
        I::Overline { .. } => "dl:overline",
        I::HitTestArea { .. } => "dl:hit",
        I::VirtualView { .. } => "dl:vview",
        I::VirtualViewPlaceholder { .. } => "dl:vview_ph",
    }
}

/// Render only the damaged regions of a display list into a retained pixmap.
///
/// For each damage rect:
/// 1. Clear that region in the pixmap (fill with background color).
/// 2. Iterate all display list items, skip those entirely outside the damage rect.
/// 3. Render intersecting items clipped to the damage rect.
///
/// Push/Pop state commands are always processed (they maintain clip/scroll stacks).
#[allow(clippy::cast_possible_truncation)] // software rasterizer: bounded pixel/coord/colour casts
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)] // bounded layout/render numeric cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics if the damage-rect iterator is unexpectedly empty.
/// # Errors
///
/// Returns an error string if rendering fails.
pub fn render_display_list_damaged(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
    damage_rects: &[LogicalRect],
) -> Result<(), String> {
    // A damage rect snapped OUTWARD to physical-pixel boundaries, carried
    // BOTH as physical ints (clear + clip) and as the equivalent logical
    // rect (item filter).
    struct SnappedRect {
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        logical: LogicalRect,
    }

    if damage_rects.is_empty() {
        return Ok(()); // nothing changed
    }

    // Snap every damage rect OUTWARD to physical-pixel boundaries (floor the
    // origin, ceil the far edge). Truncating instead leaves a fractional
    // right/bottom sliver that is neither cleared nor repainted — a 1-2px
    // stale ghost line whenever bounds are fractional (text heights like
    // 18.625, any dpi ≠ 1). The snapped rect is carried BOTH as physical ints
    // (clear + clip) and as the equivalent logical rect (item filter), so the
    // filter admits every item that touches a cleared pixel.
    let pw_i = pixmap.width() as i32;
    let ph_i = pixmap.height() as i32;
    let snap_out = |dr: &LogicalRect| -> Option<SnappedRect> {
        let x0 = ((dr.origin.x * dpi_factor).floor() as i32).clamp(0, pw_i);
        let y0 = ((dr.origin.y * dpi_factor).floor() as i32).clamp(0, ph_i);
        let x1 = (((dr.origin.x + dr.size.width) * dpi_factor).ceil() as i32).clamp(0, pw_i);
        let y1 = (((dr.origin.y + dr.size.height) * dpi_factor).ceil() as i32).clamp(0, ph_i);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(SnappedRect {
            x0,
            y0,
            x1,
            y1,
            logical: LogicalRect {
                origin: LogicalPosition {
                    x: x0 as f32 / dpi_factor,
                    y: y0 as f32 / dpi_factor,
                },
                size: LogicalSize {
                    width: (x1 - x0) as f32 / dpi_factor,
                    height: (y1 - y0) as f32 / dpi_factor,
                },
            },
        })
    };
    let mut rects: Vec<SnappedRect> = damage_rects.iter().filter_map(snap_out).collect();

    // Merge OVERLAPPING rects (strictly overlapping in physical pixels; rects
    // that merely touch stay separate). After this, the rects are pairwise
    // disjoint, so the per-rect passes below clear + paint every damaged pixel
    // EXACTLY once — no double alpha-blend where rects used to overlap, and no
    // ballooned union.
    let mut i = 0;
    while i < rects.len() {
        let mut j = i + 1;
        let mut merged_any = false;
        while j < rects.len() {
            let (a, b) = (&rects[i], &rects[j]);
            let overlap = a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1;
            if overlap {
                let x0 = a.x0.min(b.x0);
                let y0 = a.y0.min(b.y0);
                let x1 = a.x1.max(b.x1);
                let y1 = a.y1.max(b.y1);
                rects[i] = SnappedRect {
                    x0,
                    y0,
                    x1,
                    y1,
                    logical: LogicalRect {
                        origin: LogicalPosition {
                            x: x0 as f32 / dpi_factor,
                            y: y0 as f32 / dpi_factor,
                        },
                        size: LogicalSize {
                            width: (x1 - x0) as f32 / dpi_factor,
                            height: (y1 - y0) as f32 / dpi_factor,
                        },
                    },
                };
                rects.swap_remove(j);
                merged_any = true;
                // rects[i] grew — restart its inner scan, it may now overlap
                // rects it previously missed.
            } else {
                j += 1;
            }
        }
        if merged_any {
            // re-scan the same i (the union may reach earlier-skipped rects)
            if rects.len() > 1 {
                continue;
            }
        }
        i += 1;
    }

    // One pass PER damage rect, each with its own clip seeded to exactly that
    // rect. An item spanning several rects renders once per rect, but the
    // rects are disjoint so no pixel is ever blended twice. Crucially, an item
    // that intersects rect A but not rect B repaints ONLY inside A — the old
    // union-clip approach let such an item paint across the whole union,
    // overwriting neighbours between the rects that were themselves filtered
    // out (skipped), which ERASED untouched content lying between two disjoint
    // damage rects (e.g. window background + scroll strip + scrollbar column:
    // the background repainted the entire union = whole window, while all the
    // rows in the middle were skipped → visually wiped).
    for sr in &rects {
        pixmap.fill_rect(
            sr.x0,
            sr.y0,
            sr.x1 - sr.x0,
            sr.y1 - sr.y0,
            255,
            255,
            255,
            255,
        );

        let base_clip = AzRect::from_xywh(
            sr.x0 as f32,
            sr.y0 as f32,
            (sr.x1 - sr.x0) as f32,
            (sr.y1 - sr.y0) as f32,
        );
        let mut transform_stack = vec![TransAffine::new()];
        let mut clip_stack: Vec<Option<AzRect>> = vec![base_clip];
        let mut mask_stack: Vec<MaskEntry> = Vec::new();
        let mut scroll_offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];
        let mut text_shadow_stack: Vec<StyleBoxShadow> = Vec::new();

        for item in &display_list.items {
            // Always process state-management items (Push/Pop) regardless of bounds,
            // because skipping a Push while processing its matching Pop corrupts stacks.
            if !item.is_state_management() {
                if let Some(item_bounds) = item.bounds() {
                    // Items inside a scroll frame are stored at CONTENT coords but
                    // RENDER at `pos - scroll_offset`. The damage rects are in viewport
                    // space, so we must apply the current scroll offset to the bounds
                    // before the intersection test — otherwise scrolled content is
                    // filtered against the wrong position and rows that actually fall
                    // in a damage strip get dropped (visible as a missing band).
                    let (sdx, sdy) = *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0));
                    let test_bounds = if sdx == 0.0 && sdy == 0.0 {
                        item_bounds
                    } else {
                        LogicalRect {
                            origin: LogicalPosition {
                                x: item_bounds.origin.x - sdx,
                                y: item_bounds.origin.y - sdy,
                            },
                            size: item_bounds.size,
                        }
                    };
                    if !rects_overlap_or_adjacent(&test_bounds, &sr.logical, 0.0) {
                        continue;
                    }
                }
            }

            render_single_item(
                item,
                pixmap,
                dpi_factor,
                renderer_resources,
                font_manager,
                glyph_cache,
                &mut transform_stack,
                &mut clip_stack,
                &mut mask_stack,
                &mut scroll_offset_stack,
                &mut text_shadow_stack,
                render_state,
            )?;
        }
    }

    Ok(())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)] // software rasterizer: bounded pixel/coord/colour casts
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics if the clip stack is empty when an item expects an active clip.
/// # Errors
///
/// Returns an error string if rendering fails.
pub fn render_single_item(
    item: &DisplayListItem,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    glyph_cache: &mut GlyphCache,
    transform_stack: &mut Vec<TransAffine>,
    clip_stack: &mut Vec<Option<AzRect>>,
    mask_stack: &mut Vec<MaskEntry>,
    scroll_offset_stack: &mut Vec<(f32, f32)>,
    text_shadow_stack: &mut Vec<StyleBoxShadow>,
    render_state: &CpuRenderState,
) -> Result<(), String> {
    use azul_css::props::style::border::BorderStyle;
    // Current accumulated scroll offset — applied to all item bounds.
    // Negative because scrolling down (positive offset) moves content up.
    let (scroll_dx, scroll_dy) = *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0));

    // Helper: apply scroll offset to a LogicalRect.
    // Items inside scroll frames have absolute window coordinates;
    // the scroll offset shifts them so the visible portion aligns
    // with the clip region.
    let scroll_rect = |r: &LogicalRect| -> LogicalRect {
        if scroll_dx == 0.0 && scroll_dy == 0.0 {
            return *r;
        }
        LogicalRect {
            origin: LogicalPosition {
                x: r.origin.x - scroll_dx,
                y: r.origin.y - scroll_dy,
            },
            size: r.size,
        }
    };

    match item {
        DisplayListItem::Rect {
            bounds,
            color,
            border_radius,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                border_radius,
                clip,
                dpi_factor,
            );
        }
        DisplayListItem::SelectionRect {
            bounds,
            color,
            border_radius,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                border_radius,
                clip,
                dpi_factor,
            );
        }
        DisplayListItem::CursorRect { bounds, color } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                &BorderRadius::default(),
                clip,
                dpi_factor,
            );
        }
        DisplayListItem::Border {
            bounds,
            widths,
            colors,
            styles,
            border_radius,
        } => {
            let default_color = ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            };

            let w_top = widths
                .top
                .and_then(|w| w.get_property().copied())
                .map_or(0.0, |w| {
                    w.inner
                        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
                });
            let w_right = widths
                .right
                .and_then(|w| w.get_property().copied())
                .map_or(0.0, |w| {
                    w.inner
                        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
                });
            let w_bottom = widths
                .bottom
                .and_then(|w| w.get_property().copied())
                .map_or(0.0, |w| {
                    w.inner
                        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
                });
            let w_left = widths
                .left
                .and_then(|w| w.get_property().copied())
                .map_or(0.0, |w| {
                    w.inner
                        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
                });

            let c_top = colors
                .top
                .and_then(|c| c.get_property().copied())
                .map_or(default_color, |c| c.inner);
            let c_right = colors
                .right
                .and_then(|c| c.get_property().copied())
                .map_or(default_color, |c| c.inner);
            let c_bottom = colors
                .bottom
                .and_then(|c| c.get_property().copied())
                .map_or(default_color, |c| c.inner);
            let c_left = colors
                .left
                .and_then(|c| c.get_property().copied())
                .map_or(default_color, |c| c.inner);

            let s_top = styles
                .top
                .and_then(|s| s.get_property().copied())
                .map_or(BorderStyle::Solid, |s| s.inner);
            let s_right = styles
                .right
                .and_then(|s| s.get_property().copied())
                .map_or(BorderStyle::Solid, |s| s.inner);
            let s_bottom = styles
                .bottom
                .and_then(|s| s.get_property().copied())
                .map_or(BorderStyle::Solid, |s| s.inner);
            let s_left = styles
                .left
                .and_then(|s| s.get_property().copied())
                .map_or(BorderStyle::Solid, |s| s.inner);

            let simple_radius = BorderRadius {
                top_left: border_radius.top_left.to_pixels_internal(
                    bounds.0.size.width,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                top_right: border_radius.top_right.to_pixels_internal(
                    bounds.0.size.width,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                bottom_left: border_radius.bottom_left.to_pixels_internal(
                    bounds.0.size.width,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                bottom_right: border_radius.bottom_right.to_pixels_internal(
                    bounds.0.size.width,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
            };

            let clip = *clip_stack.last().unwrap();
            let b = scroll_rect(bounds.inner());

            // If all sides same color/width/style, use single render_border call
            let all_same = c_top == c_right
                && c_top == c_bottom
                && c_top == c_left
                && w_top == w_right
                && w_top == w_bottom
                && w_top == w_left
                && s_top == s_right
                && s_top == s_bottom
                && s_top == s_left;

            if all_same {
                render_border(
                    pixmap,
                    &b,
                    c_top,
                    w_top,
                    s_top,
                    &simple_radius,
                    clip,
                    dpi_factor,
                );
            } else {
                // Per-side rendering: render each side separately
                render_border_sides(
                    pixmap,
                    &b,
                    [c_top, c_right, c_bottom, c_left],
                    [w_top, w_right, w_bottom, w_left],
                    [s_top, s_right, s_bottom, s_left],
                    &simple_radius,
                    clip,
                    dpi_factor,
                );
            }
        }
        DisplayListItem::Underline {
            bounds,
            color,
            thickness: _,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                &BorderRadius::default(),
                clip,
                dpi_factor,
            );
        }
        DisplayListItem::Strikethrough {
            bounds,
            color,
            thickness: _,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                &BorderRadius::default(),
                clip,
                dpi_factor,
            );
        }
        DisplayListItem::Overline {
            bounds,
            color,
            thickness: _,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_rect(
                pixmap,
                &scroll_rect(bounds.inner()),
                *color,
                &BorderRadius::default(),
                clip,
                dpi_factor,
            );
        }
        DisplayListItem::Text {
            glyphs,
            font_size_px,
            font_hash,
            color,
            clip_rect,
            ..
        } => {
            let clip = *clip_stack.last().unwrap();
            let text_clip = scroll_rect(clip_rect.inner());
            // Paint text-shadows behind the real glyphs, back-to-front (the
            // outermost / first-pushed shadow is painted first so later ones
            // layer on top). Reuses the glyph rasterizer + the same stack-blur
            // used by `box-shadow`/`filter`.
            for shadow in text_shadow_stack.iter() {
                render_text_shadow(
                    shadow,
                    glyphs,
                    *font_hash,
                    *font_size_px,
                    pixmap,
                    &text_clip,
                    clip,
                    renderer_resources,
                    font_manager,
                    dpi_factor,
                    glyph_cache,
                    (scroll_dx, scroll_dy),
                );
            }
            render_text(
                glyphs,
                *font_hash,
                *font_size_px,
                *color,
                pixmap,
                &text_clip,
                clip,
                renderer_resources,
                font_manager,
                dpi_factor,
                glyph_cache,
                (scroll_dx, scroll_dy),
                false,
            );
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
            // A `DecodedImage::Callback` `<img>` (e.g. the AzulPaint canvas) can't
            // be rasterised here — the renderer can't run the callback. The backend
            // pre-invoked it into `image_callback_results`; swap in the produced
            // image (keyed by the callback image's hash). Falls back to `image`
            // (→ grey placeholder) only if no result was produced.
            let resolved = render_state.image_callback_results.get(&image.get_hash());
            render_image(
                pixmap,
                &scroll_rect(bounds.inner()),
                resolved.unwrap_or(image),
                clip,
                dpi_factor,
            );
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
                &scroll_rect(bounds.inner()),
                *color,
                &BorderRadius::default(),
                clip,
                dpi_factor,
            );
        }
        DisplayListItem::ScrollBarStyled { info } => {
            let clip = *clip_stack.last().unwrap();

            // Resolve scrollbar opacity from the GPU value cache.
            // WhenScrolling mode starts at 0.0 and fades to 1.0 on scroll.
            // In cpurender we read the current value; if none is cached
            // (e.g. headless mode never ran synchronize_scrollbar_opacity)
            // default to 1.0 so the scrollbar is always visible.
            let scrollbar_opacity = info
                .opacity_key
                .and_then(|key| render_state.opacities.get(&key.id).copied())
                .unwrap_or(1.0);

            if scrollbar_opacity > 0.001 {
                // Render track
                if info.track_color.a > 0 {
                    render_rect(
                        pixmap,
                        &scroll_rect(info.track_bounds.inner()),
                        info.track_color,
                        &BorderRadius::default(),
                        clip,
                        dpi_factor,
                    );
                }

                // Render decrement button
                if let Some(btn_bounds) = &info.button_decrement_bounds {
                    if info.button_color.a > 0 {
                        render_rect(
                            pixmap,
                            &scroll_rect(btn_bounds.inner()),
                            info.button_color,
                            &BorderRadius::default(),
                            clip,
                            dpi_factor,
                        );
                    }
                }

                // Render increment button
                if let Some(btn_bounds) = &info.button_increment_bounds {
                    if info.button_color.a > 0 {
                        render_rect(
                            pixmap,
                            &scroll_rect(btn_bounds.inner()),
                            info.button_color,
                            &BorderRadius::default(),
                            clip,
                            dpi_factor,
                        );
                    }
                }

                // Render thumb — the thumb is wrapped in PushReferenceFrame
                // with a thumb_transform_key, so the GPU cache lookup handles
                // positioning dynamically. Here we just apply the initial
                // transform embedded in the display list item as a fallback.
                if info.thumb_color.a > 0 {
                    let thumb_rect = info.thumb_bounds.inner();
                    // Look up live transform from render_state if available
                    let transform = info
                        .thumb_transform_key
                        .and_then(|key| render_state.transforms.get(&key.id))
                        .unwrap_or(&info.thumb_initial_transform);
                    let tx = transform.m[3][0];
                    let ty = transform.m[3][1];
                    let transformed_thumb = LogicalRect {
                        origin: LogicalPosition {
                            x: thumb_rect.origin.x + tx,
                            y: thumb_rect.origin.y + ty,
                        },
                        size: thumb_rect.size,
                    };
                    render_rect(
                        pixmap,
                        &scroll_rect(&transformed_thumb),
                        info.thumb_color,
                        &info.thumb_border_radius,
                        clip,
                        dpi_factor,
                    );
                }
            } // end scrollbar_opacity > 0
        }
        DisplayListItem::PushClip {
            bounds,
            border_radius,
        } => {
            // Two fixes (the invisible-maps-header bug):
            // 1. The clip must live in the same coordinate space items draw in
            //    (`pos - accumulated_scroll`) — shift it via scroll_rect() like
            //    every drawing arm. A VirtualView child's PushClip otherwise
            //    lands at raw child-local coordinates on the window.
            // 2. A nested clip can only NARROW the active one. Pushing the rect
            //    verbatim let a child DL's own PushClip REPLACE the VirtualView
            //    composite clip, so the child painted over the whole window
            //    (the maps header/toolbar disappeared under the tile grid).
            let new_clip = logical_rect_to_az_rect(&scroll_rect(bounds.inner()), dpi_factor);
            let merged = intersect_clips(clip_stack.last().copied().flatten(), new_clip);
            clip_stack.push(merged);
        }
        DisplayListItem::PopClip => {
            // Never pop the base clip (the window rect pushed at init). An
            // unbalanced PopClip — e.g. a display-list bookkeeping mismatch in
            // the titlebar/stacking-context emit path — must NOT abort the whole
            // layer render. Previously this returned Err, the caller logged
            // "render_layers error: Clip stack underflow" and DROPPED THE ENTIRE
            // FRAME, leaving a blank window with no body/button. Clamp to the base
            // instead so the frame still presents; the only effect of an over-pop
            // is that trailing items fall back to the base (window) clip, which is
            // harmless for well-formed DOMs.
            if clip_stack.len() > 1 {
                clip_stack.pop();
            } else {
                #[cfg(feature = "std")]
                if std::env::var("AZ_CLIP_DEBUG").is_ok() {
                    eprintln!(
                        "[CpuBackend] PopClip with no matching PushClip — clamping to base clip"
                    );
                }
            }
        }
        DisplayListItem::PushScrollFrame { scroll_id, .. } => {
            // Scroll frame = scroll offset only.
            // The display list generator always emits PushClip before
            // PushScrollFrame with the same clip bounds, so we don't
            // need to push another clip here — that would double-clip.
            transform_stack.push(
                transform_stack
                    .last()
                    .copied()
                    .unwrap_or_else(TransAffine::new),
            );
            let frame_offset = render_state
                .scroll_offsets
                .get(scroll_id)
                .copied()
                .unwrap_or((0.0, 0.0));
            let new_scroll = (scroll_dx + frame_offset.0, scroll_dy + frame_offset.1);
            scroll_offset_stack.push(new_scroll);
        }
        DisplayListItem::PopScrollFrame => {
            // Only pop transform and scroll offset — the clip was pushed
            // by a separate PushClip and will be popped by PopClip.
            if transform_stack.len() > 1 {
                transform_stack.pop();
            }
            if scroll_offset_stack.len() > 1 {
                scroll_offset_stack.pop();
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
            let _ = clip_rect;
            // Composite the VirtualView's child DOM (a separate LayoutResult the
            // normal layout loop produced — e.g. the MapWidget's tile grid). Its
            // display list is 0-relative, so we (1) clip to the VirtualView's
            // on-screen rect and (2) push a scroll offset of -bounds.origin so the
            // renderer (which draws at `pos - accumulated_scroll`) places the child
            // content at the VirtualView origin. Then recursively rasterise it.
            // (Was: a debug-blue overlay that never drew the child — the reason the
            // CPU backend showed a blank map.)
            let child_dl = render_state.virtual_view_display_lists.get(child_dom_id).cloned();
            #[cfg(feature = "std")]
            if std::env::var("AZ_MAP_DEBUG").is_ok() {
                eprintln!(
                    "[cpu-vview] VirtualView item: child_dom_id={} found={} items={} bounds={:?} avail_ids={:?}",
                    child_dom_id.inner,
                    child_dl.is_some(),
                    child_dl.as_ref().map_or(0, |d| d.items.len()),
                    bounds.inner(),
                    render_state.virtual_view_display_lists.keys().map(|k| k.inner).collect::<Vec<_>>(),
                );
            }
            if let Some(child_dl) = child_dl {
                let vv_origin = bounds.inner().origin;
                // Intersect with the active clip (the VirtualView may itself sit
                // inside a clipped/scrolled container) — same rule as PushClip.
                let vv_clip = intersect_clips(
                    clip_stack.last().copied().flatten(),
                    logical_rect_to_az_rect(&scroll_rect(bounds.inner()), dpi_factor),
                );
                clip_stack.push(vv_clip);
                scroll_offset_stack.push((scroll_dx - vv_origin.x, scroll_dy - vv_origin.y));
                for child_item in &child_dl.items {
                    render_single_item(
                        child_item,
                        pixmap,
                        dpi_factor,
                        renderer_resources,
                        font_manager,
                        glyph_cache,
                        transform_stack,
                        clip_stack,
                        mask_stack,
                        scroll_offset_stack,
                        text_shadow_stack,
                        render_state,
                    )?;
                }
                scroll_offset_stack.pop();
                clip_stack.pop();
            }
        }
        DisplayListItem::VirtualViewPlaceholder { .. } => {
            #[cfg(feature = "std")]
            if std::env::var("AZ_MAP_DEBUG").is_ok() {
                eprintln!("[cpu-vview] VirtualViewPlaceholder hit (NOT swapped to a VirtualView item — nothing composites)");
            }
        }

        // Gradient rendering
        DisplayListItem::LinearGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_linear_gradient(
                pixmap,
                &scroll_rect(bounds.inner()),
                gradient,
                border_radius,
                clip,
                dpi_factor,
                render_state.system_style.as_deref().map(|s| &s.colors),
            );
        }
        DisplayListItem::RadialGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_radial_gradient(
                pixmap,
                &scroll_rect(bounds.inner()),
                gradient,
                border_radius,
                clip,
                dpi_factor,
                render_state.system_style.as_deref().map(|s| &s.colors),
            );
        }
        DisplayListItem::ConicGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_conic_gradient(
                pixmap,
                &scroll_rect(bounds.inner()),
                gradient,
                border_radius,
                clip,
                dpi_factor,
                render_state.system_style.as_deref().map(|s| &s.colors),
            );
        }

        // BoxShadow
        DisplayListItem::BoxShadow {
            bounds,
            shadow,
            border_radius,
        } => {
            render_box_shadow(
                pixmap,
                &scroll_rect(bounds.inner()),
                shadow,
                border_radius,
                dpi_factor,
            )?;
        }

        // --- Opacity layers ---
        DisplayListItem::PushOpacity { bounds, opacity } => {
            let rect = logical_rect_to_az_rect(&scroll_rect(bounds.inner()), dpi_factor);
            if let Some(r) = rect {
                let snap = snapshot_region(
                    pixmap,
                    r.x as i32,
                    r.y as i32,
                    r.width as u32,
                    r.height as u32,
                );
                mask_stack.push(MaskEntry::Opacity {
                    snapshot: snap,
                    rect: r,
                    opacity: *opacity,
                });
            }
        }
        DisplayListItem::PopOpacity => {
            if let Some(MaskEntry::Opacity {
                snapshot,
                rect,
                opacity,
            }) = mask_stack.pop()
            {
                let x = rect.x as i32;
                let y = rect.y as i32;
                let w = rect.width as u32;
                let h = rect.height as u32;
                let pw = pixmap.width as i32;
                let ph = pixmap.height as i32;
                // Blend: result = snapshot + (current - snapshot) * opacity
                for py in 0..h as i32 {
                    let dy = y + py;
                    if dy < 0 || dy >= ph {
                        continue;
                    }
                    for px in 0..w as i32 {
                        let dx = x + px;
                        if dx < 0 || dx >= pw {
                            continue;
                        }
                        let pi = ((dy as u32 * pixmap.width + dx as u32) * 4) as usize;
                        let si = ((py as u32 * w + px as u32) * 4) as usize;
                        if pi + 3 >= pixmap.data.len() || si + 3 >= snapshot.len() {
                            continue;
                        }
                        let op = (opacity * 255.0).clamp(0.0, 255.0) as u32;
                        let inv_op = 255 - op;
                        for c in 0..4 {
                            let snap_c = u32::from(snapshot[si + c]);
                            let cur_c = u32::from(pixmap.data[pi + c]);
                            pixmap.data[pi + c] = ((cur_c * op + snap_c * inv_op) / 255) as u8;
                        }
                    }
                }
            }
        }

        // --- Reference frames (CSS transforms) ---
        DisplayListItem::PushReferenceFrame {
            transform_key,
            initial_transform,
            bounds,
        } => {
            // Look up the current GPU-cached transform value for this key.
            // For scrollbar thumbs, the GpuValueCache stores the up-to-date
            // thumb translation. For CSS transforms, it stores the computed
            // matrix. Falls back to the initial_transform baked in the DL.
            let live_transform = render_state.transforms.get(&transform_key.id);
            let m = live_transform.map_or(&initial_transform.m, |t| &t.m);
            let tf = TransAffine::new_custom(
                f64::from(m[0][0]),
                f64::from(m[0][1]), // sx, shy
                f64::from(m[1][0]),
                f64::from(m[1][1]), // shx, sy
                f64::from(m[3][0]),
                f64::from(m[3][1]), // tx, ty
            );
            let current = transform_stack
                .last()
                .copied()
                .unwrap_or_else(TransAffine::new);
            let mut composed = tf;
            composed.premultiply(&current);
            transform_stack.push(composed);
        }
        DisplayListItem::PopReferenceFrame => {
            if transform_stack.len() > 1 {
                transform_stack.pop();
            }
        }

        // --- Filter effects ---
        //
        // `filter` (PushFilter/PopFilter) is intentionally a no-op *here*: the
        // effect is realized by the compositor layer path, which allocates a
        // dedicated pixbuf for the filtered subtree in
        // `allocate_layers_from_display_list` and applies the blur/color filters
        // at composite time via `apply_layer_filters`. The content between
        // Push/PopFilter is rendered into that layer's pixbuf by this very
        // function, so the markers themselves carry no work at item level.
        DisplayListItem::PushFilter { .. } => {}
        DisplayListItem::PopFilter => {}

        // TODO(superplan g4): `backdrop-filter` is unimplemented in the CPU
        // renderer. Unlike `filter` (which acts on the element's own content),
        // it must read the *already-composited backdrop* (parent + earlier
        // siblings) under the element and blur/tint that. Those pixels do not
        // exist in this per-layer `pixmap`; they only exist in the `output`
        // buffer inside `CompositorState::composite_layer_recursive`. Correct
        // impl: (1) allocate a layer for PushBackdropFilter in
        // `allocate_layers_from_display_list` (mirroring PushFilter but tagged as
        // a backdrop filter, see the matching TODO there); (2) in
        // `composite_layer_recursive`, before blitting that layer's own content,
        // copy the `output` region under the layer's absolute bounds, run
        // `apply_layer_filters` on the copy, and write it back. No item-level
        // work belongs here. Documented as a known limitation rather than shipping
        // a half-impl that ignores the backdrop.
        DisplayListItem::PushBackdropFilter { .. } => {}
        DisplayListItem::PopBackdropFilter => {}

        // `text-shadow` (superplan g4): the shadow is applied in the `Text` arm
        // (above) by `render_text_shadow`, which rasterizes the glyph run offset
        // by `shadow.offset`, tinted with `shadow.color`, blurred by
        // `shadow.blur_radius` (reusing the same `stack_blur_rgba32` used by
        // `box-shadow`/`filter`), then draws the real glyphs on top. These
        // markers just maintain the active-shadow stack.
        DisplayListItem::PushTextShadow { shadow } => {
            text_shadow_stack.push(*shadow);
        }
        DisplayListItem::PopTextShadow => {
            text_shadow_stack.pop();
        }

        DisplayListItem::PushImageMaskClip {
            bounds,
            mask_image,
            mask_rect,
        } => {
            let mr = &scroll_rect(mask_rect.inner());
            let px_x = (mr.origin.x * dpi_factor) as i32;
            let px_y = (mr.origin.y * dpi_factor) as i32;
            let px_w = (mr.size.width * dpi_factor).ceil() as u32;
            let px_h = (mr.size.height * dpi_factor).ceil() as u32;

            if px_w > 0 && px_h > 0 {
                let snapshot = snapshot_region(pixmap, px_x, px_y, px_w, px_h);
                let mask_data = extract_mask_data(mask_image, px_w, px_h)
                    .unwrap_or_else(|| vec![255u8; (px_w * px_h) as usize]);
                mask_stack.push(MaskEntry::ImageMask {
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

    Ok(())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // software rasterizer: bounded pixel/coord/colour casts
fn render_rect(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    color: ColorU,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
) {
    if color.a == 0 {
        return;
    }

    let Some(rect) = logical_rect_to_az_rect(bounds, dpi_factor) else {
        return;
    };

    // Early-out if fully outside clip
    if let Some(ref c) = clip {
        if rect.clip(c).is_none() {
            return;
        }
    }

    let agg_color = Rgba8::new(
        u32::from(color.r),
        u32::from(color.g),
        u32::from(color.b),
        u32::from(color.a),
    );

    if border_radius.is_zero() {
        // Fast path: axis-aligned rectangle — use direct RendererBase::blend_bar
        // instead of the full rasterizer pipeline. This avoids path construction,
        // cell generation, sorting, and scanline rendering for simple rectangles.
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
        rb.blend_bar(
            rect.x as i32,
            rect.y as i32,
            (rect.x + rect.width) as i32 - 1,
            (rect.y + rect.height) as i32 - 1,
            &agg_color,
            255, // cover=255: alpha is already in the color
        );
    } else {
        // Rounded rect: needs the full rasterizer for curved corners
        let mut path = build_rounded_rect_path(&rect, border_radius, dpi_factor);
        agg_fill_path_clipped(pixmap, &mut path, &agg_color, FillingRule::NonZero, clip);
    }

}

/// Default for the RGB LCD subpixel-AA text path: **ON**.
///
/// LCD rendering distributes glyph coverage across the R/G/B stripes of each
/// physical pixel, giving crisper text on the common case. It ASSUMES a
/// **horizontal-RGB subpixel order** and an **opaque background** (a BGR panel
/// would need the R/B taps swapped, and text composited onto a transparent layer
/// must use the grayscale path — see `render_text_shadow`, which forces it). It
/// also turns black text into the familiar faintly-fringed subpixel look. Set
/// `AZ_TEXT_LCD=0` to force the grayscale path.
pub const TEXT_LCD_DEFAULT: bool = true;

/// Whether to render text via the RGB LCD subpixel-AA path. On by default (see
/// [`TEXT_LCD_DEFAULT`]); set `AZ_TEXT_LCD=0` to disable. Read once.
fn text_lcd_enabled() -> bool {
    static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        std::env::var("AZ_TEXT_LCD")
            .map(|s| !(s == "0" || s.eq_ignore_ascii_case("false")))
            .unwrap_or(TEXT_LCD_DEFAULT)
    })
}

/// RGB LCD subpixel-AA glyph run. Rasterizes each glyph at **3× horizontal
/// resolution** (one sub-sample per R/G/B stripe), then lets [`PixfmtRgba32Lcd`]
/// run a 5-tap FIR (the `FreeType` default "light" filter `[08 4D 56 4D 08]`, which
/// sums to 256) over the sub-samples to produce PER-CHANNEL coverage and blend
/// it into the buffer. Black text on white therefore shows the characteristic
/// R/B subpixel fringes instead of a single grey coverage.
///
/// Assumptions / limitations (documented, since this is opt-in):
/// - **Horizontal RGB** subpixel order. A BGR panel would need the R/B taps
///   swapped; a vertical panel would need a transposed (3× vertical) variant.
/// - **Opaque background.** The pixfmt writes per-channel and forces the touched
///   pixel's alpha to 255, so subpixel text composited onto a transparent layer
///   is wrong — as it is for every LCD text pipeline. The default flat render
///   path fills the frame opaque white, which is the intended target.
/// - Uses the glyph **path** cache (`get_or_build`), not the pre-rasterized cell
///   cache, since the cells are 1× horizontal; LCD is thus a little slower.
///
/// The Y baseline is grid-snapped (crisp vertical) and X is placed at true
/// fractional position (1/3-px LCD precision) when `AZ_TEXT_SUBPIXEL` is on, or
/// snapped to an integer pixel when it is off — matching the grayscale path's
/// sub-pixel-positioning policy.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)] // software rasterizer: bounded pixel/coord/colour casts
#[allow(clippy::too_many_arguments)] // mirrors render_text's font/metric plumbing
fn render_glyphs_lcd(
    pixmap: &mut AzulPixmap,
    clip: Option<AzRect>,
    glyphs: &[GlyphInstance],
    parsed_font: &ParsedFont,
    font_hash: FontHash,
    ppem: u16,
    scale: f32,
    hint_correction: f32,
    color: ColorU,
    dpi_factor: f32,
    scroll_offset: (f32, f32),
    glyph_cache: &mut GlyphCache,
) {
    use agg_rust::pixfmt_lcd::{LcdDistributionLut, PixfmtRgba32Lcd};

    let agg_color = Rgba8::new(
        u32::from(color.r),
        u32::from(color.g),
        u32::from(color.b),
        u32::from(color.a),
    );
    let subpx = crate::glyph_cache::text_subpixel_enabled();

    // Accumulate every glyph outline (at 3× horizontal resolution) into one
    // rasterizer, then sweep once — same batching as the grayscale path.
    let mut ras = RasterizerScanlineAa::new();
    ras.filling_rule(FillingRule::NonZero);

    for glyph in glyphs {
        let glyph_index = glyph.index as u16;
        let Some(glyph_data) = parsed_font.get_or_decode_glyph(glyph_index) else {
            continue;
        };
        let Some(cached) = glyph_cache.get_or_build(
            font_hash.font_hash,
            glyph_index,
            &glyph_data,
            parsed_font,
            ppem,
        ) else {
            continue;
        };
        let is_hinted = cached.is_hinted;

        let glyph_x = (glyph.point.x - scroll_offset.0) * dpi_factor;
        let glyph_baseline_y = (glyph.point.y - scroll_offset.1) * dpi_factor;
        // Crisp vertical: grid-snap the baseline. Soft horizontal: keep the true
        // fractional x (LCD gives 1/3-px precision) unless sub-pixel is disabled.
        let px = if subpx { glyph_x } else { glyph_x.round() };
        let py = glyph_baseline_y.round();

        // Path units → pixels: hinted-at-integer-ppem is already pixel-space
        // (scale 1), a fractional effective size rescales by hint_correction, and
        // an unhinted outline is in font units (scale = px/upem). Mirrors
        // `GlyphCache::get_or_build_cells`.
        let rescale_hinted = is_hinted && (hint_correction - 1.0).abs() > 1e-4;
        let path_scale = if is_hinted {
            if rescale_hinted { f64::from(hint_correction) } else { 1.0 }
        } else {
            f64::from(scale)
        };

        // Map the path to its absolute pixel position, then triple the X axis so
        // the rasterizer runs at 3 sub-samples per pixel:
        //   final_subpixel_x = 3*(path_scale*path_x + px),  final_y = path_scale*path_y + py
        // (scale-then-translate: `TransAffine::multiply` post-concatenates).
        let mut transform = TransAffine::new_scaling(3.0 * path_scale, path_scale);
        transform.multiply(&TransAffine::new_translation(3.0 * f64::from(px), f64::from(py)));
        ras.add_path_vertices_transformed(cached.path.vertices(), &transform);
    }

    // Blend via the LCD pixel format. It reports width*3, so the rasterizer's 3×
    // x-coordinates address individual R/G/B stripes; the clip box X is likewise
    // in sub-pixel space.
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;
    let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride) };
    // FreeType default "light" 5-tap FIR: primary 0x56, secondary 0x4D, tertiary
    // 0x08 (0x08+0x4D+0x56+0x4D+0x08 = 256); the LUT normalizes prim+2·sec+2·tert.
    let lut = LcdDistributionLut::new(f64::from(0x56u32), f64::from(0x4Du32), f64::from(0x08u32));
    let pf = PixfmtRgba32Lcd::new(&mut ra, &lut);
    let mut rb = RendererBase::new(pf);
    if let Some(c) = clip {
        rb.clip_box_i(
            (c.x as i32) * 3,
            c.y as i32,
            ((c.x + c.width) as i32) * 3 - 1,
            (c.y + c.height) as i32 - 1,
        );
    }
    let mut sl = ScanlineU8::new();
    render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &agg_color);
}

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)] // software rasterizer: bounded pixel/coord/colour casts
#[allow(clippy::too_many_lines)] // large but cohesive: font lookup + grayscale/LCD dispatch + glyph loop
fn render_text(
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    pixmap: &mut AzulPixmap,
    clip_rect: &LogicalRect,
    clip: Option<AzRect>,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
    // When true, force the grayscale path even if LCD is enabled. Used for the
    // text-shadow offscreen, which is transparent: the LCD per-channel path
    // assumes an opaque background and forces per-pixel alpha to 255, which
    // corrupts a shadow composited from a transparent layer.
    force_grayscale: bool,
) {
    if color.a == 0 || glyphs.is_empty() {
        return;
    }

    // Skip text entirely if its clip_rect is outside the active clip region
    if let Some(ref c) = clip {
        let Some(text_rect) = logical_rect_to_az_rect(clip_rect, dpi_factor) else {
            return;
        };
        if text_rect.clip(c).is_none() {
            return; // fully clipped
        }
    }

    let agg_color = Rgba8::new(
        u32::from(color.r),
        u32::from(color.g),
        u32::from(color.b),
        u32::from(color.a),
    );

    // Try to get the parsed font
    let parsed_font: &ParsedFont = if let Some(fm) = font_manager {
        if let Some(font_ref) = fm.get_font_by_hash(font_hash.font_hash) { unsafe { &*font_ref.get_parsed().cast::<ParsedFont>() } } else {
            eprintln!(
                "[cpurender] Font hash {} not found in FontManager",
                font_hash.font_hash
            );
            return;
        }
    } else {
        let Some(font_key) = renderer_resources.font_hash_map.get(&font_hash.font_hash) else {
            eprintln!(
                "[cpurender] Font hash {} not found in font_hash_map (available: {:?})",
                font_hash.font_hash,
                renderer_resources.font_hash_map.keys().collect::<Vec<_>>()
            );
            return;
        };

        let Some((font_ref, _instances)) = renderer_resources.currently_registered_fonts.get(font_key) else {
            eprintln!(
                "[cpurender] FontKey {font_key:?} not found in currently_registered_fonts"
            );
            return;
        };

        unsafe { &*font_ref.get_parsed().cast::<ParsedFont>() }
    };

    let units_per_em = f32::from(parsed_font.font_metrics.units_per_em);
    if units_per_em <= 0.0 {
        return;
    }

    let effective_px = font_size_px * dpi_factor;
    let scale = effective_px / units_per_em;
    let ppem = effective_px.round() as u16;
    // A hinted outline is produced at the integer `ppem`. `hint_correction`
    // rescales it back to the true (possibly fractional) effective size so hinted
    // glyphs match unhinted fallbacks and animate smoothly instead of snapping.
    let hint_correction = if ppem > 0 { effective_px / f32::from(ppem) } else { 1.0 };

    // RGB LCD subpixel-AA path (opt-in, `AZ_TEXT_LCD=1`; off by default). Renders
    // at 3× horizontal resolution with a 5-tap FIR + per-channel blend. The
    // grayscale path below is left byte-for-byte identical when the flag is off.
    if text_lcd_enabled() && !force_grayscale {
        render_glyphs_lcd(
            pixmap, clip, glyphs, parsed_font, font_hash, ppem, scale,
            hint_correction, color, dpi_factor, scroll_offset, glyph_cache,
        );
        return;
    }

    // Set up the rasterizer pipeline once, reuse for all glyphs
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;

    // Create renderer infrastructure once, reuse for all glyphs in this text run.
    // Batches all glyph cells into a single rasterizer pass when possible.
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

    // Accumulate all glyph cells into one rasterizer, then render once.
    // This amortizes sort_cells cost across all glyphs in the run.
    for glyph in glyphs {
        let glyph_index = glyph.index as u16;

        // Lazy decode: first access to a given gid for this face does
        // the allsorts glyf walk + OwnedGlyph conversion; subsequent
        // accesses are an Arc bump + BTreeMap lookup.
        let Some(glyph_data) = parsed_font.get_or_decode_glyph(glyph_index) else {
            continue;
        };

        let is_hinted = glyph_cache
            .get_or_build(
                font_hash.font_hash,
                glyph_index,
                &glyph_data,
                parsed_font,
                ppem,
            )
            .is_some_and(|c| c.is_hinted);

        let glyph_x = (glyph.point.x - scroll_offset.0) * dpi_factor;
        let glyph_baseline_y = (glyph.point.y - scroll_offset.1) * dpi_factor;

        let Some((cells, int_x, int_y)) = glyph_cache.get_or_build_cells(
            font_hash.font_hash,
            glyph_index,
            ppem,
            glyph_x,
            glyph_baseline_y,
            scale,
            is_hinted,
            hint_correction,
        ) else {
            continue;
        };

        ras.add_cells_offset(cells, int_x, int_y);
    }

    // Single render pass for all glyphs in this text run
    let mut sl = ScanlineU8::new();
    render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &agg_color);

}

/// Paint a single `text-shadow` for a glyph run.
///
/// Renders the glyphs (offset by the shadow's logical offset, tinted with the
/// shadow colour) into a transparent offscreen buffer, blurs that buffer by the
/// shadow's blur radius using the same `stack_blur_rgba32` the box-shadow/filter
/// paths use, then alpha-composites it onto `pixmap` (below where the real
/// glyphs are subsequently drawn).
///
/// The offscreen is full-pixmap-sized so the blur is never clipped at a tight
/// glyph bbox and so the existing `blit_buffer` (premultiplied-alpha) compositor
/// can be reused directly. Text-shadows are uncommon, so the extra full-frame
/// allocation/blit is acceptable for correctness.
// software rasterizer: bounded blur-radius / stride / pixel casts
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn render_text_shadow(
    shadow: &StyleBoxShadow,
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    pixmap: &mut AzulPixmap,
    clip_rect: &LogicalRect,
    clip: Option<AzRect>,
    renderer_resources: &RendererResources,
    font_manager: Option<&FontManager<FontRef>>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
) {
    let color = shadow.color;
    if color.a == 0 || glyphs.is_empty() {
        return;
    }

    // Logical offsets (render_text applies dpi_factor internally).
    let off_x = shadow
        .offset_x
        .inner
        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);
    let off_y = shadow
        .offset_y
        .inner
        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);
    let blur_logical = shadow
        .blur_radius
        .inner
        .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
        .max(0.0);

    // Offscreen, transparent, same size as the target (so blur has room).
    let Some(mut tmp) = AzulPixmap::new(pixmap.width, pixmap.height) else {
        return;
    };
    tmp.fill(0, 0, 0, 0);

    // Shift glyphs by the (logical) shadow offset.
    let shifted: Vec<GlyphInstance> = glyphs
        .iter()
        .map(|g| {
            let mut g = *g;
            g.point.x += off_x;
            g.point.y += off_y;
            g
        })
        .collect();

    // Rasterize the offset glyph run in the shadow colour into the offscreen.
    let shadow_clip_rect = LogicalRect {
        origin: LogicalPosition {
            x: clip_rect.origin.x + off_x,
            y: clip_rect.origin.y + off_y,
        },
        size: clip_rect.size,
    };
    render_text(
        &shifted,
        font_hash,
        font_size_px,
        color,
        &mut tmp,
        &shadow_clip_rect,
        clip,
        renderer_resources,
        font_manager,
        dpi_factor,
        glyph_cache,
        scroll_offset,
        // Always grayscale: the shadow offscreen is transparent, so the LCD
        // per-channel path (which assumes an opaque bg) would corrupt it.
        true,
    );

    // Blur the offscreen (in device pixels).
    let blur_px = blur_logical * dpi_factor;
    if blur_px > 0.5 {
        let radius = (blur_px.ceil() as u32).min(254);
        let w = tmp.width;
        let h = tmp.height;
        let stride = (w * 4) as i32;
        let mut ra = unsafe { RowAccessor::new_with_buf(tmp.data.as_mut_ptr(), w, h, stride) };
        stack_blur_rgba32(&mut ra, radius, radius);
    }

    // Composite the (premultiplied) shadow buffer onto the target.
    blit_buffer(pixmap, &tmp.data, tmp.width, tmp.height, 0, 0);
}

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // software rasterizer: bounded pixel/coord/colour casts
fn render_border(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    color: ColorU,
    width: f32,
    border_style: azul_css::props::style::border::BorderStyle,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
) {
    use azul_css::props::style::border::BorderStyle;

    if color.a == 0 || width <= 0.0 {
        return;
    }

    match border_style {
        BorderStyle::None | BorderStyle::Hidden => return,
        _ => {}
    }

    let Some(rect) = logical_rect_to_az_rect(bounds, dpi_factor) else {
        return;
    };

    // Skip if fully outside clip
    if let Some(ref c) = clip {
        if rect.clip(c).is_none() {
            return;
        }
    }

    let scaled_width = width * dpi_factor;
    let agg_color = Rgba8::new(
        u32::from(color.r),
        u32::from(color.g),
        u32::from(color.b),
        u32::from(color.a),
    );

    // 1. Build outer path (rounded rect at the nominal border radii)
    let mut path = build_rounded_rect_path(&rect, border_radius, dpi_factor);

    let x = f64::from(rect.x);
    let y = f64::from(rect.y);
    let w = f64::from(rect.width);
    let h = f64::from(rect.height);
    let sw = f64::from(scaled_width);

    // 2. Add inner path with shrunk radii so EvenOdd fill carves the stroke
    let ir = AzRect::from_xywh(
        rect.x + scaled_width,
        rect.y + scaled_width,
        rect.width - 2.0 * scaled_width,
        rect.height - 2.0 * scaled_width,
    );

    if let Some(ir) = ir {
        let inner_radius = BorderRadius {
            top_left: (border_radius.top_left - width).max(0.0),
            top_right: (border_radius.top_right - width).max(0.0),
            bottom_right: (border_radius.bottom_right - width).max(0.0),
            bottom_left: (border_radius.bottom_left - width).max(0.0),
        };
        let mut inner = build_rounded_rect_path(&ir, &inner_radius, dpi_factor);
        path.concat_path(&mut inner, 0);
    }

    // 3. Render based on border style
    match border_style {
        BorderStyle::Dashed | BorderStyle::Dotted => {
            // For dashed/dotted: stroke the border path with dash pattern
            use agg_rust::conv_dash::ConvDash;
            use agg_rust::conv_stroke::ConvStroke;

            let half = sw / 2.0;
            let mut stroke_path = PathStorage::new();
            let (cx, cy, cw, ch) = (x + half, y + half, w - sw, h - sw);
            stroke_path.move_to(cx, cy);
            stroke_path.line_to(cx + cw, cy);
            stroke_path.line_to(cx + cw, cy + ch);
            stroke_path.line_to(cx, cy + ch);
            stroke_path.close_polygon(PATH_FLAGS_NONE);

            let mut dashed = ConvDash::new(stroke_path);
            if border_style == BorderStyle::Dashed {
                dashed.add_dash(sw * 3.0, sw);
            } else {
                dashed.add_dash(sw, sw);
            }

            let mut stroked = ConvStroke::new(dashed);
            stroked.set_width(sw);

            agg_fill_path_clipped(pixmap, &mut stroked, &agg_color, FillingRule::NonZero, clip);
        }
        _ if border_radius.is_zero() => {
            // Fast path: solid border without rounding — use blend_bar strips
            let pw = pixmap.width;
            let ph = pixmap.height;
            let stride = (pw * 4) as i32;
            let mut ra =
                unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), pw, ph, stride) };
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
            let (xi, yi) = (x as i32, y as i32);
            let (x2i, y2i) = ((x + w) as i32 - 1, (y + h) as i32 - 1);
            let swi = sw as i32;
            // Top strip
            rb.blend_bar(xi, yi, x2i, yi + swi - 1, &agg_color, 255);
            // Bottom strip
            rb.blend_bar(xi, y2i - swi + 1, x2i, y2i, &agg_color, 255);
            // Left strip (between top and bottom)
            rb.blend_bar(xi, yi + swi, xi + swi - 1, y2i - swi, &agg_color, 255);
            // Right strip
            rb.blend_bar(x2i - swi + 1, yi + swi, x2i, y2i - swi, &agg_color, 255);
        }
        _ => {
            // Rounded solid border: fill double-path with EvenOdd
            agg_fill_path_clipped(pixmap, &mut path, &agg_color, FillingRule::EvenOdd, clip);
        }
    }

}

/// Render border with per-side colors/widths/styles using CSS trapezoid model.
/// Each side is a trapezoid: outer edge → inner edge with 45° miters at corners.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // software rasterizer: bounded pixel/coord/colour casts
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn render_border_sides(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    colors: [ColorU; 4], // top, right, bottom, left
    widths: [f32; 4],    // top, right, bottom, left
    _styles: [azul_css::props::style::border::BorderStyle; 4],
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
) {
    let Some(rect) = logical_rect_to_az_rect(bounds, dpi_factor) else {
        return;
    };

    // Outer corners
    let ox = f64::from(rect.x);
    let oy = f64::from(rect.y);
    let ow = f64::from(rect.width);
    let oh = f64::from(rect.height);

    // Inner corners (inset by per-side widths)
    let wt = f64::from(widths[0] * dpi_factor);
    let wr = f64::from(widths[1] * dpi_factor);
    let wb = f64::from(widths[2] * dpi_factor);
    let wl = f64::from(widths[3] * dpi_factor);

    let ix = ox + wl;
    let iy = oy + wt;
    let iw = ow - wl - wr;
    let ih = oh - wt - wb;

    // Each side is a trapezoid with 4 vertices:
    // Top:    (ox, oy) → (ox+ow, oy) → (ix+iw, iy) → (ix, iy)
    // Right:  (ox+ow, oy) → (ox+ow, oy+oh) → (ix+iw, iy+ih) → (ix+iw, iy)
    // Bottom: (ox+ow, oy+oh) → (ox, oy+oh) → (ix, iy+ih) → (ix+iw, iy+ih)
    // Left:   (ox, oy+oh) → (ox, oy) → (ix, iy) → (ix, iy+ih)

    let sides: [(f64, f64, f64, f64, f64, f64, f64, f64, ColorU, f32); 4] = [
        // Top trapezoid
        (
            ox,
            oy,
            ox + ow,
            oy,
            ix + iw,
            iy,
            ix,
            iy,
            colors[0],
            widths[0],
        ),
        // Right trapezoid
        (
            ox + ow,
            oy,
            ox + ow,
            oy + oh,
            ix + iw,
            iy + ih,
            ix + iw,
            iy,
            colors[1],
            widths[1],
        ),
        // Bottom trapezoid
        (
            ox + ow,
            oy + oh,
            ox,
            oy + oh,
            ix,
            iy + ih,
            ix + iw,
            iy + ih,
            colors[2],
            widths[2],
        ),
        // Left trapezoid
        (
            ox,
            oy + oh,
            ox,
            oy,
            ix,
            iy,
            ix,
            iy + ih,
            colors[3],
            widths[3],
        ),
    ];

    if border_radius.is_zero() {
        // Fast path: axis-aligned border strips — no rasterizer needed
        let pw = pixmap.width;
        let ph = pixmap.height;
        let stride = (pw * 4) as i32;
        let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), pw, ph, stride) };
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
        // Top: full width, height = wt
        if widths[0] > 0.0 && colors[0].a > 0 {
            let c = colors[0];
            let ac = Rgba8::new(u32::from(c.r), u32::from(c.g), u32::from(c.b), u32::from(c.a));
            rb.blend_bar(
                ox as i32,
                oy as i32,
                (ox + ow) as i32 - 1,
                iy as i32 - 1,
                &ac,
                255,
            );
        }
        // Bottom
        if widths[2] > 0.0 && colors[2].a > 0 {
            let c = colors[2];
            let ac = Rgba8::new(u32::from(c.r), u32::from(c.g), u32::from(c.b), u32::from(c.a));
            rb.blend_bar(
                ox as i32,
                (iy + ih) as i32,
                (ox + ow) as i32 - 1,
                (oy + oh) as i32 - 1,
                &ac,
                255,
            );
        }
        // Left: between top and bottom
        if widths[3] > 0.0 && colors[3].a > 0 {
            let c = colors[3];
            let ac = Rgba8::new(u32::from(c.r), u32::from(c.g), u32::from(c.b), u32::from(c.a));
            rb.blend_bar(
                ox as i32,
                iy as i32,
                ix as i32 - 1,
                (iy + ih) as i32 - 1,
                &ac,
                255,
            );
        }
        // Right
        if widths[1] > 0.0 && colors[1].a > 0 {
            let c = colors[1];
            let ac = Rgba8::new(u32::from(c.r), u32::from(c.g), u32::from(c.b), u32::from(c.a));
            rb.blend_bar(
                (ix + iw) as i32,
                iy as i32,
                (ox + ow) as i32 - 1,
                (iy + ih) as i32 - 1,
                &ac,
                255,
            );
        }
    } else {
        // Rounded borders: use trapezoid rasterizer
        for &(x0, y0, x1, y1, x2, y2, x3, y3, color, width) in &sides {
            if width <= 0.0 || color.a == 0 {
                continue;
            }

            let mut path = PathStorage::new();
            path.move_to(x0, y0);
            path.line_to(x1, y1);
            path.line_to(x2, y2);
            path.line_to(x3, y3);
            path.close_polygon(PATH_FLAGS_NONE);

            let agg_color = Rgba8::new(
                u32::from(color.r),
                u32::from(color.g),
                u32::from(color.b),
                u32::from(color.a),
            );
            agg_fill_path_clipped(pixmap, &mut path, &agg_color, FillingRule::NonZero, clip);
        }
    }

}

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_precision_loss, clippy::cast_sign_loss)] // software rasterizer: bounded pixel/coord/colour casts
#[allow(clippy::many_single_char_names, clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn render_image(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    image: &ImageRef,
    clip: Option<AzRect>,
    dpi_factor: f32,
) {
    let Some(rect) = logical_rect_to_az_rect(bounds, dpi_factor) else {
        return;
    };

    // Skip if fully outside clip
    if let Some(ref c) = clip {
        if rect.clip(c).is_none() {
            return;
        }
    }

    let image_data = image.get_data();
    let (src_rgba, src_w, src_h) = match image_data {
        DecodedImage::Raw((descriptor, data)) => {
            let w = descriptor.width as u32;
            let h = descriptor.height as u32;
            if w == 0 || h == 0 {
                return;
            }
            let bytes = match data {
                azul_core::resources::ImageData::Raw(shared) => shared.as_ref(),
                azul_core::resources::ImageData::External(_) => return,
            };

            let rgba = match descriptor.format {
                // Already the target layout — plain copy. This is the format
                // every live-frame producer (camera / screencap / video
                // decoder) emits, so it must NOT fall into the gray-placeholder
                // arm below (that bug made all capture tiles render flat gray
                // on the CPU backend, on every OS).
                azul_core::resources::RawImageFormat::RGBA8 => bytes.to_vec(),
                azul_core::resources::RawImageFormat::RGB8 => {
                    let mut out = Vec::with_capacity(bytes.len() / 3 * 4);
                    for chunk in bytes.chunks_exact(3) {
                        out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
                    }
                    out
                }
                azul_core::resources::RawImageFormat::BGRA8 => {
                    let mut out = Vec::with_capacity(bytes.len());
                    for chunk in bytes.chunks_exact(4) {
                        let b = chunk[0];
                        let g = chunk[1];
                        let r = chunk[2];
                        let a = chunk[3];
                        out.push(r);
                        out.push(g);
                        out.push(b);
                        out.push(a);
                    }
                    out
                }
                azul_core::resources::RawImageFormat::R8 => {
                    let mut out = Vec::with_capacity(bytes.len() * 4);
                    for &v in bytes {
                        out.push(v);
                        out.push(v);
                        out.push(v);
                        out.push(v);
                    }
                    out
                }
                _ => {
                    // Unsupported format — render gray placeholder
                    let gray = Rgba8::new(200, 200, 200, 255);
                    let mut path = build_rect_path(&rect);
                    agg_fill_path(pixmap, &mut path, &gray, FillingRule::NonZero);
                    return;
                }
            };

            (rgba, w, h)
        }
        DecodedImage::NullImage { .. } | DecodedImage::Callback(_) => {
            let gray = Rgba8::new(200, 200, 200, 255);
            let mut path = build_rect_path(&rect);
            agg_fill_path(pixmap, &mut path, &gray, FillingRule::NonZero);
            return;
        }
        DecodedImage::Gl(_) => return,
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

    // Compute pixel-level clip bounds for the blit loop
    let (clip_x1, clip_y1, clip_x2, clip_y2) = clip.as_ref().map_or((0, 0, pw as i32, ph as i32), |c| (
            c.x as i32,
            c.y as i32,
            (c.x + c.width) as i32,
            (c.y + c.height) as i32,
        ));

    for py in 0..dst_h {
        for px in 0..dst_w {
            let tx = dst_x + px as i32;
            let ty = dst_y + py as i32;
            if tx < 0 || ty < 0 || tx >= pw as i32 || ty >= ph as i32 {
                continue;
            }
            // Clip check
            if tx < clip_x1 || ty < clip_y1 || tx >= clip_x2 || ty >= clip_y2 {
                continue;
            }

            let src_x = ((px as f32 * sx) as u32).min(src_w - 1);
            let src_y = ((py as f32 * sy) as u32).min(src_h - 1);
            let si = ((src_y * src_w + src_x) * 4) as usize;
            let di = ((ty as u32 * pw + tx as u32) * 4) as usize;

            if si + 3 < src_rgba.len() && di + 3 < pixmap.data.len() {
                let sa = u32::from(src_rgba[si + 3]);
                if sa == 255 {
                    pixmap.data[di] = src_rgba[si];
                    pixmap.data[di + 1] = src_rgba[si + 1];
                    pixmap.data[di + 2] = src_rgba[si + 2];
                    pixmap.data[di + 3] = 255;
                } else if sa > 0 {
                    // Alpha blend: dst = src * sa + dst * (255 - sa)
                    let da = 255 - sa;
                    pixmap.data[di] =
                        ((u32::from(src_rgba[si]) * sa + u32::from(pixmap.data[di]) * da) / 255) as u8;
                    pixmap.data[di + 1] = ((u32::from(src_rgba[si + 1]) * sa
                        + u32::from(pixmap.data[di + 1]) * da)
                        / 255) as u8;
                    pixmap.data[di + 2] = ((u32::from(src_rgba[si + 2]) * sa
                        + u32::from(pixmap.data[di + 2]) * da)
                        / 255) as u8;
                    pixmap.data[di + 3] =
                        ((sa + u32::from(pixmap.data[di + 3]) * da / 255).min(255)) as u8;
                }
            }
        }
    }

}

fn build_rect_path(rect: &AzRect) -> PathStorage {
    let mut path = PathStorage::new();
    let x = f64::from(rect.x);
    let y = f64::from(rect.y);
    let w = f64::from(rect.width);
    let h = f64::from(rect.height);
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

    let x = f64::from(rect.x);
    let y = f64::from(rect.y);
    let w = f64::from(rect.width);
    let h = f64::from(rect.height);

    let tl = f64::from(border_radius.top_left * dpi_factor);
    let tr = f64::from(border_radius.top_right * dpi_factor);
    let br = f64::from(border_radius.bottom_right * dpi_factor);
    let bl = f64::from(border_radius.bottom_left * dpi_factor);

    if tl <= 0.0 && tr <= 0.0 && br <= 0.0 && bl <= 0.0 {
        path.move_to(x, y);
        path.line_to(x + w, y);
        path.line_to(x + w, y + h);
        path.line_to(x, y + h);
        path.close_polygon(PATH_FLAGS_NONE);
        return path;
    }

    // agg::RoundedRect emits real arc vertices (MOVE_TO + LINE_TO segments)
    // via its embedded Arc generator, which the scanline rasterizer consumes
    // directly. curve3() control points are silently flattened to straight
    // lines by the rasterizer, which is why the hand-rolled path produced
    // square corners — Arc-based flattening produces smooth corners.
    //
    // agg's corner slots (rx1/ry1 .. rx4/ry4) map to screen corners as:
    //   slot 1 → top-left    (center at x1+rx1, y1+ry1)
    //   slot 2 → top-right   (center at x2-rx2, y1+ry2)
    //   slot 3 → bottom-right (center at x2-rx3, y2-ry3)
    //   slot 4 → bottom-left (center at x1+rx4, y2-ry4)
    let mut rr = RoundedRect::default_new();
    rr.rect(x, y, x + w, y + h);
    rr.radius_all(tl, tl, tr, tr, br, br, bl, bl);
    rr.normalize_radius();
    rr.set_approximation_scale(f64::from(dpi_factor.max(1.0)));

    path.concat_path(&mut rr, 0);
    path
}

// ============================================================================
// Component Preview Rendering
// ============================================================================

/// Options for rendering a component preview.
#[derive(Debug, Clone, Copy)]
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
            background_color: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        }
    }
}

/// Result of a component preview render.
#[derive(Debug)]
pub struct ComponentPreviewResult {
    /// PNG-encoded image data.
    pub png_data: Vec<u8>,
    /// Actual content width (logical pixels).
    pub content_width: f32,
    /// Actual content height (logical pixels).
    pub content_height: f32,
}

/// Compute the tight bounding box of all display list items.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
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
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // software rasterizer: bounded pixel/coord/colour casts
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics if `opts.width` or `opts.height` is None.
/// # Errors
///
/// Returns an error string if rendering fails.
pub fn render_component_preview(
    styled_dom: &azul_core::styled_dom::StyledDom,
    font_manager: &FontManager<FontRef>,
    opts: ComponentPreviewOptions,
    system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
) -> Result<ComponentPreviewResult, String> {
    use crate::{
        font_traits::TextLayoutCache,
        solver3::{self, cache::LayoutCache, display_list::DisplayList},
    };
    use azul_core::{
        dom::DomId,
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::{IdNamespace, RendererResources},
        selection::{SelectionState, TextSelection},
    };
    use std::collections::{BTreeMap, HashMap};

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
    )
    .map_err(|e| format!("Failed to create preview font manager: {e:?}"))?;

    // --- Font resolution ---
    {
        use crate::solver3::getters::collect_and_resolve_font_chains_with_registration;
        use crate::text3::default::PathLoader;

        let platform = azul_css::system::Platform::current();

        let chains = collect_and_resolve_font_chains_with_registration(
            styled_dom,
            &preview_font_manager.fc_cache,
            &preview_font_manager,
            &platform,
        );
        let loader = PathLoader::new();
        let _failed = preview_font_manager.load_missing_for_chains(&chains, |bytes, index| {
            loader.load_font_shared(bytes, index)
        });
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
        cache_map: solver3::cache::LayoutCacheMap::default(),
        previous_positions: Vec::new(),
        cached_display_list: None,
        prev_dom_ptr: 0,
        prev_viewport: LogicalRect::zero(),
    };
    let mut text_cache = TextLayoutCache::new();
    let empty_scroll_offsets = BTreeMap::new();
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
        &empty_text_selections,
        &mut debug_messages,
        None,
        &renderer_resources,
        id_namespace,
        dom_id,
        false,
        Vec::new(),
        None, // preedit_text: not needed for headless preview rendering
        &azul_core::resources::ImageCache::default(),
        system_style.clone(),
        get_system_time_fn,
    )
    .map_err(|e| format!("Layout failed: {e:?}"))?;

    // --- Determine actual render size ---
    let (render_width, render_height) = if opts.width.is_some() && opts.height.is_some() {
        (opts.width.unwrap(), opts.height.unwrap())
    } else {
        match compute_content_bounds(&display_list) {
            Some((_min_x, _min_y, max_x, max_y)) => {
                let w = if opts.width.is_some() {
                    opts.width.unwrap()
                } else {
                    max_x.max(1.0).ceil()
                };
                let h = if opts.height.is_some() {
                    opts.height.unwrap()
                } else {
                    max_y.max(1.0).ceil()
                };
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
        .ok_or_else(|| format!("Cannot create pixmap {pixel_w}x{pixel_h}"))?;

    let bg = opts.background_color;
    pixmap.fill(bg.r, bg.g, bg.b, bg.a);

    let mut preview_glyph_cache = GlyphCache::new();
    let preview_render_state =
        CpuRenderState::new(ScrollOffsetMap::new()).with_system_style(system_style);
    render_display_list_with_state(
        &display_list,
        &mut pixmap,
        dpi,
        &renderer_resources,
        Some(&preview_font_manager),
        &mut preview_glyph_cache,
        &preview_render_state,
    )?;

    let png_data = pixmap
        .encode_png()
        .map_err(|e| format!("PNG encoding failed: {e}"))?;

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
/// # Errors
///
/// Returns an error string if rendering fails.
pub fn render_dom_to_image(
    mut dom: azul_core::dom::Dom,
    css: azul_css::css::Css,
    width: f32,
    height: f32,
    dpi: f32,
) -> Result<Vec<u8>, String> {
    use crate::font_traits::FontManager;
    use azul_core::styled_dom::StyledDom;

    let styled_dom = StyledDom::create(&mut dom, css);

    let fc_cache = crate::font::loading::build_font_cache();
    let font_manager = FontManager::new(fc_cache)
        .map_err(|e| format!("Failed to create font manager: {e:?}"))?;

    let opts = ComponentPreviewOptions {
        width: Some(width),
        height: Some(height),
        dpi_factor: dpi,
        background_color: ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        },
    };

    let result = render_component_preview(&styled_dom, &font_manager, opts, None)?;
    Ok(result.png_data)
}

/// Render a short single-line string into a freshly allocated [`AzulPixmap`].
///
/// Shapes + rasterizes the glyphs (e.g. a tooltip label) through the same CPU
/// text pipeline ([`render_display_list`] → `render_text`) the rest of the
/// renderer uses. This is the platform-agnostic text path for shells that have
/// **no** native server-side text drawing (notably Wayland, which — unlike
/// X11's `XDrawString`, macOS `NSTextField` or Win32 GDI — must rasterize into
/// a client `wl_shm` buffer itself).
///
/// The returned pixmap is exactly `text + 2*padding` wide and one line tall
/// (ascent+descent), filled with `bg_color`, with the text drawn in
/// `text_color`. Pixel data is RGBA8 (see [`AzulPixmap::data`]); callers that
/// need a different channel order (e.g. ARGB8888 little-endian = BGRA bytes for
/// Wayland) must swap on copy.
///
/// Returns `None` if no usable system font can be resolved or the font has
/// degenerate metrics — callers should fall back gracefully (no tooltip text).
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
#[must_use]
// bounded pixel-dimension casts; explicit a*b+c kept (see render_box_shadow)
#[allow(clippy::suboptimal_flops, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn render_text_run_to_pixmap(
    fc_cache: &rust_fontconfig::FcFontCache,
    text: &str,
    font_size_px: f32,
    text_color: ColorU,
    bg_color: ColorU,
    padding_px: f32,
    dpi_factor: f32,
) -> Option<AzulPixmap> {
    use azul_core::resources::{FontKey, IdNamespace};
    use rust_fontconfig::{FcPattern, OwnedFontSource};

    // 1. Resolve a default (sans-serif) system font, falling back to any font.
    let mut trace = Vec::new();
    let matched = fc_cache
        .query(
            &FcPattern {
                family: Some("sans-serif".to_string()),
                ..Default::default()
            },
            &mut trace,
        )
        .or_else(|| fc_cache.query(&FcPattern::default(), &mut trace))?;

    let bytes = fc_cache.get_font_bytes(&matched.id)?;
    let font_index = fc_cache
        .get_font_by_id(&matched.id)
        .map_or(0, |src| match src {
            OwnedFontSource::Disk(path) => path.font_index,
            OwnedFontSource::Memory(font) => font.font_index,
        });

    let parsed = ParsedFont::from_bytes(bytes.as_slice(), font_index, &mut Vec::new())?
        .with_source_bytes(bytes.clone());

    let upm = f32::from(parsed.font_metrics.units_per_em);
    if upm <= 0.0 {
        return None;
    }
    let scale = font_size_px / upm;

    // 2. Register the font in a throwaway RendererResources so the display-list
    //    renderer can resolve the glyph run by hash.
    let mut rr = RendererResources::default();
    let font_ref = crate::parsed_font_to_font_ref(parsed.clone());
    let key = FontKey::unique(IdNamespace(0));
    let hash = crate::font_ref_to_parsed_font(&font_ref).hash;
    rr.font_hash_map.insert(hash, key);
    rr.currently_registered_fonts
        .insert(key, (font_ref, std::collections::BTreeMap::default()));
    let font_hash = FontHash { font_hash: hash };

    // 3. Shape the string (simple per-char advances; tooltips are short,
    //    single-line and unstyled, so the full bidi/complex shaper isn't
    //    reachable here — same simplification as the pagination header path).
    let ascent = parsed.font_metrics.ascent * scale;
    let descent = parsed.font_metrics.descent * scale; // typically negative
    let baseline_y = padding_px + ascent;
    let mut pen_x = padding_px;
    let mut glyphs = Vec::new();
    for c in text.chars() {
        let gid = parsed.lookup_glyph_index(c as u32).unwrap_or(0);
        let advance = f32::from(parsed.get_horizontal_advance(gid)) * scale;
        glyphs.push(GlyphInstance {
            index: u32::from(gid),
            point: LogicalPosition { x: pen_x, y: baseline_y },
            size: LogicalSize { width: advance, height: font_size_px },
        });
        pen_x += advance;
    }

    // 4. Size the pixmap to the shaped run (logical units; device pixels via dpi).
    let logical_w = (pen_x + padding_px).max(1.0);
    let logical_h = (ascent - descent + padding_px * 2.0).max(1.0);
    let w = ((logical_w * dpi_factor).ceil() as u32).max(1);
    let h = ((logical_h * dpi_factor).ceil() as u32).max(1);

    let mut pixmap = AzulPixmap::new(w, h)?;
    pixmap.fill(bg_color.r, bg_color.g, bg_color.b, bg_color.a);

    // 5. Rasterize the run via the shared display-list text path.
    let clip_rect: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
        origin: LogicalPosition { x: 0.0, y: 0.0 },
        size: LogicalSize { width: logical_w, height: logical_h },
    }
    .into();

    let item = DisplayListItem::Text {
        glyphs,
        font_hash,
        font_size_px,
        color: text_color,
        clip_rect,
        source_node_index: None,
    };
    let dl = DisplayList {
        items: vec![item],
        ..Default::default()
    };
    let mut gc = GlyphCache::new();
    render_display_list(&dl, &mut pixmap, dpi_factor, &rr, None, &mut gc).ok()?;

    Some(pixmap)
}

// ============================================================================
// Direct SVG-to-image renderer (bypasses CSS layout)
// ============================================================================


#[cfg(all(test, feature = "std"))]
mod text_shadow_tests {
    use super::*;
    use crate::font::parsed::ParsedFont;
    use crate::solver3::display_list::{DisplayList, WindowLogicalRect};
    use azul_core::resources::{FontKey, IdNamespace};
    use azul_css::props::basic::pixel::{PixelValue, PixelValueNoPercent};
    use azul_css::props::style::box_shadow::StyleBoxShadow;

    fn load_test_font() -> Option<ParsedFont> {
        let candidates = [
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "C:/Windows/Fonts/arial.ttf",
        ];
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let arc = std::sync::Arc::new(rust_fontconfig::FontBytes::Owned(
                    std::sync::Arc::from(bytes.as_slice()),
                ));
                if let Some(font) = ParsedFont::from_bytes(&bytes, 0, &mut Vec::new())
                    .map(|f| f.with_source_bytes(arc))
                {
                    return Some(font);
                }
            }
        }
        None
    }

    fn renderer_resources_with(font: &ParsedFont) -> (RendererResources, FontHash) {
        let mut rr = RendererResources::default();
        let font_ref = crate::parsed_font_to_font_ref(font.clone());
        let key = FontKey::unique(IdNamespace(0));
        let hash = crate::font_ref_to_parsed_font(&font_ref).hash;
        rr.font_hash_map.insert(hash, key);
        rr.currently_registered_fonts
            .insert(key, (font_ref, std::collections::BTreeMap::default()));
        (rr, FontHash { font_hash: hash })
    }

    /// Shape a string into glyph instances with a baseline at (x, y).
    fn shape(parsed: &ParsedFont, text: &str, font_size: f32, x: f32, y: f32) -> Vec<GlyphInstance> {
        let upm = f32::from(parsed.font_metrics.units_per_em);
        let scale = font_size / upm;
        let mut pen_x = x;
        let mut out = Vec::new();
        for c in text.chars() {
            let gid = parsed.lookup_glyph_index(c as u32).unwrap_or(0);
            let advance = f32::from(parsed.get_horizontal_advance(gid)) * scale;
            out.push(GlyphInstance {
                index: u32::from(gid),
                point: LogicalPosition { x: pen_x, y },
                size: LogicalSize {
                    width: advance,
                    height: font_size,
                },
            });
            pen_x += advance;
        }
        out
    }

    fn count_red(pixmap: &AzulPixmap) -> usize {
        pixmap
            .data()
            .chunks_exact(4)
            .filter(|p| p[0] > 150 && p[1] < 100 && p[2] < 100)
            .count()
    }

    /// A `text-shadow` must actually paint shadow-coloured pixels, offset from
    /// the glyphs, where the no-shadow render shows only the white background.
    #[test]
    fn text_shadow_paints_offset_colored_pixels() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, font_hash) = renderer_resources_with(&font);

        let w = 200u32;
        let h = 60u32;
        let font_size = 32.0;
        // Black glyphs, baseline near the vertical middle.
        let glyphs = shape(&font, "Hi", font_size, 10.0, 40.0);
        // test fixture: bounded pixmap-dimension cast
        #[allow(clippy::cast_precision_loss)]
        let clip_rect: WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize { width: w as f32, height: h as f32 },
        }
        .into();

        let text_item = DisplayListItem::Text {
            glyphs,
            font_hash,
            font_size_px: font_size,
            color: ColorU { r: 0, g: 0, b: 0, a: 255 },
            clip_rect,
            source_node_index: None,
        };

        // Render WITHOUT a shadow: only black glyphs on white -> no red pixels.
        let mut gc = GlyphCache::new();
        let mut no_shadow = AzulPixmap::new(w, h).unwrap();
        no_shadow.fill(255, 255, 255, 255);
        let dl_plain = DisplayList {
            items: vec![text_item.clone()],
            ..Default::default()
        };
        render_display_list(&dl_plain, &mut no_shadow, 1.0, &rr, None, &mut gc).unwrap();
        // Baseline red-pixel count. With grayscale text this is 0; with LCD
        // subpixel AA (now the default) black glyph edges carry faint red/blue
        // fringes, so the shadow must add red BEYOND this baseline (checked below).
        let red_plain = count_red(&no_shadow);

        // Render WITH a red shadow offset +24px right, no blur.
        let shadow = StyleBoxShadow {
            offset_x: PixelValueNoPercent { inner: PixelValue::px(24.0) },
            offset_y: PixelValueNoPercent { inner: PixelValue::px(0.0) },
            blur_radius: PixelValueNoPercent { inner: PixelValue::px(0.0) },
            spread_radius: PixelValueNoPercent { inner: PixelValue::px(0.0) },
            color: ColorU { r: 255, g: 0, b: 0, a: 255 },
            clip_mode: azul_css::props::style::box_shadow::BoxShadowClipMode::Outset,
        };
        let mut with_shadow = AzulPixmap::new(w, h).unwrap();
        with_shadow.fill(255, 255, 255, 255);
        let dl_shadow = DisplayList {
            items: vec![
                DisplayListItem::PushTextShadow { shadow },
                text_item,
                DisplayListItem::PopTextShadow,
            ],
            ..Default::default()
        };
        let mut gc2 = GlyphCache::new();
        render_display_list(&dl_shadow, &mut with_shadow, 1.0, &rr, None, &mut gc2).unwrap();
        let red_shadow = count_red(&with_shadow);

        assert!(
            red_shadow > red_plain + 20,
            "text-shadow must paint red shadow pixels beyond the baseline \
             (plain {red_plain}, shadow {red_shadow})"
        );

        // The shadow must be OFFSET to the right of the glyphs: there must be red
        // pixels in the right portion of the canvas that are absent in the plain
        // render (i.e. to the right of where the glyphs themselves sit).
        let right_red = with_shadow
            .data()
            .chunks_exact(4)
            .enumerate()
            .filter(|(i, p)| {
                #[allow(clippy::cast_possible_truncation)] // bounded pixel index
                let x = (*i as u32) % w;
                x > 30 && p[0] > 150 && p[1] < 100 && p[2] < 100
            })
            .count();
        assert!(
            right_red > 0,
            "shadow should appear offset to the right of the glyphs"
        );
    }

    /// With a blurred shadow, the shadow region should be larger (blur spreads
    /// coverage) than with a hard-edged shadow.
    #[test]
    fn text_shadow_blur_spreads_coverage() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, font_hash) = renderer_resources_with(&font);
        let w = 200u32;
        let h = 80u32;
        let font_size = 32.0;
        let glyphs = shape(&font, "Hi", font_size, 40.0, 50.0);
        // test fixture: bounded pixmap-dimension cast
        #[allow(clippy::cast_precision_loss)]
        let clip_rect: WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize { width: w as f32, height: h as f32 },
        }
        .into();

        let make = |blur: f32| -> usize {
            let shadow = StyleBoxShadow {
                offset_x: PixelValueNoPercent { inner: PixelValue::px(0.0) },
                offset_y: PixelValueNoPercent { inner: PixelValue::px(0.0) },
                blur_radius: PixelValueNoPercent { inner: PixelValue::px(blur) },
                spread_radius: PixelValueNoPercent { inner: PixelValue::px(0.0) },
                color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                clip_mode: azul_css::props::style::box_shadow::BoxShadowClipMode::Outset,
            };
            let text_item = DisplayListItem::Text {
                glyphs: glyphs.clone(),
                font_hash,
                font_size_px: font_size,
                color: ColorU { r: 0, g: 0, b: 0, a: 0 }, // transparent text: isolate shadow
                clip_rect,
                source_node_index: None,
            };
            let dl = DisplayList {
                items: vec![
                    DisplayListItem::PushTextShadow { shadow },
                    text_item,
                    DisplayListItem::PopTextShadow,
                ],
                ..Default::default()
            };
            let mut pm = AzulPixmap::new(w, h).unwrap();
            pm.fill(255, 255, 255, 255);
            let mut gc = GlyphCache::new();
            render_display_list(&dl, &mut pm, 1.0, &rr, None, &mut gc).unwrap();
            // count any non-white pixel (shadow coverage)
            pm.data()
                .chunks_exact(4)
                .filter(|p| p[0] != 255 || p[1] != 255 || p[2] != 255)
                .count()
        };

        let hard = make(0.0);
        let blurred = make(6.0);
        assert!(hard > 0, "hard shadow should paint");
        assert!(
            blurred > hard,
            "blurred shadow ({blurred}) should cover more pixels than hard ({hard})"
        );
    }
}
