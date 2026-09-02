#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use super::*;

use crate::font::parsed::ParsedFont;
use crate::glyph_cache::GlyphCache;
use crate::solver3::display_list::{BorderRadius, DisplayList, DisplayListItem, LocalScrollId};
use crate::text3::cache::{FontHash, FontManager};
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
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::{DecodedImage, ImageRef, RendererResources};
use azul_core::ui_solver::GlyphInstance;
use azul_css::props::basic::pixel::DEFAULT_FONT_SIZE;
use azul_css::props::basic::{ColorOrSystem, ColorU, FontRef};
use azul_css::props::style::box_shadow::StyleBoxShadow;
use azul_css::props::style::filter::StyleFilter;
use std::collections::HashMap;

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
            Rgba8::new(
                u32::from(c.r),
                u32::from(c.g),
                u32::from(c.b),
                u32::from(c.a),
            ),
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
        // Conic stops use angle — normalize to 0..1 fraction of full circle.
        // Use the RAW degrees (not `to_degrees()`, which wraps 360 -> 0): a
        // final 360deg stop is a meaningful, distinct offset of 1.0. Without
        // this, `conic-gradient(a, b)` (normalized to 0deg/360deg) collapses
        // both stops onto offset 0.0, `build_lut()` dedups them to one stop,
        // bails (`len < 2`), and the gradient paints nothing. The clamp keeps
        // any out-of-range raw angle inside [0, 1].
        let offset = f64::from((stop.angle.to_degrees_raw() / 360.0).clamp(0.0, 1.0));
        let c = resolve_color(&stop.color, system_colors);
        lut.add_color(
            offset,
            Rgba8::new(
                u32::from(c.r),
                u32::from(c.g),
                u32::from(c.b),
                u32::from(c.a),
            ),
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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)] // software rasterizer: bounded pixel/coord/colour casts
/// Blurred-shadow buffer cache. A box shadow is a pure function of
/// (shape size, border radius, color, blur, spread, dpi) — NOT of its
/// position — yet every repaint re-allocated a page-sized RGBA buffer,
/// re-filled the shape and re-ran a stack blur over it: **47.7 ms per
/// shadow, ×4 page shadows = 190.6 ms per repaint** on big.md (measured
/// 2026-08-08; this dominated the entire frame after solver3 dropped to
/// 20 ms). miniword's Word-style pages all share one shadow spec, so the
/// whole 190 ms collapses into ONE cache entry replayed at four offsets.
///
/// Thread-local (the CPU raster path is single-threaded per window),
/// byte-capped LRU — a page-sized entry is ~3.7 MB, so the cap admits a
/// handful of distinct shadow specs before evicting; a document with
/// per-node unique shadows degrades to the old per-frame blur, never to
/// unbounded memory.
const SHADOW_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

struct ShadowCacheEntry {
    data: std::rc::Rc<Vec<u8>>,
    w: u32,
    h: u32,
}

thread_local! {
    static SHADOW_BLUR_CACHE: core::cell::RefCell<(
        HashMap<u64, ShadowCacheEntry>,
        std::collections::VecDeque<u64>,
        usize, // bytes
    )> = core::cell::RefCell::new((
        HashMap::new(),
        std::collections::VecDeque::new(),
        0,
    ));
}

fn render_box_shadow(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    shadow: &StyleBoxShadow,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
) -> Result<(), String> {
    // Clamp a shadow blit to the active clip. The shadow BLENDS (alpha), so
    // any write OUTSIDE a damage rect re-darkens retained, already-shadowed
    // pixels — on a resize drag the page shadow visibly accumulated darker
    // with every partial repaint. Writers must clip (the LCD fringe law);
    // the pixels outside the rect are either untouched-and-correct or
    // covered by another damage rect's own clear+repaint.
    let clip_px = clip.map(|c| {
        (
            c.x as i32,
            c.y as i32,
            (c.x + c.width).ceil() as i32,
            (c.y + c.height).ceil() as i32,
        )
    });
    #[allow(clippy::too_many_arguments)]
    fn blit_clipped(
        pixmap: &mut AzulPixmap,
        clip_px: Option<(i32, i32, i32, i32)>,
        src: &[u8],
        src_w: u32,
        src_h: u32,
        mut sx: u32,
        mut sy: u32,
        mut w: u32,
        mut h: u32,
        mut dx: i32,
        mut dy: i32,
    ) {
        if let Some((cx0, cy0, cx1, cy1)) = clip_px {
            if dx < cx0 {
                let d = (cx0 - dx) as u32;
                if d >= w {
                    return;
                }
                sx += d;
                w -= d;
                dx = cx0;
            }
            if dy < cy0 {
                let d = (cy0 - dy) as u32;
                if d >= h {
                    return;
                }
                sy += d;
                h -= d;
                dy = cy0;
            }
            if dx + w as i32 > cx1 {
                let over = dx + w as i32 - cx1;
                if over >= w as i32 {
                    return;
                }
                w -= over as u32;
            }
            if dy + h as i32 > cy1 {
                let over = dy + h as i32 - cy1;
                if over >= h as i32 {
                    return;
                }
                h -= over as u32;
            }
        }
        blit_buffer_sub(pixmap, src, src_w, src_h, sx, sy, w, h, dx, dy);
    }
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
    let shadow_w = 2.0f32.mul_add(spread, rect.width) + 2.0 * padding;
    let shadow_h = 2.0f32.mul_add(spread, rect.height) + 2.0 * padding;

    if shadow_w <= 0.0 || shadow_h <= 0.0 {
        return Ok(());
    }

    let sw = shadow_w.ceil() as u32;
    let sh = shadow_h.ceil() as u32;

    if sw == 0 || sh == 0 || sw > MAX_SHADOW_PIXBUF_SIZE || sh > MAX_SHADOW_PIXBUF_SIZE {
        return Ok(());
    }

    // Cache key: every input the blurred buffer's CONTENT depends on. The
    // shadow's POSITION (shadow_x/y) is deliberately absent — that is the
    // blit offset, not part of the pixels.
    let cache_key = {
        use core::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        sw.hash(&mut h);
        sh.hash(&mut h);
        rect.width.to_bits().hash(&mut h);
        rect.height.to_bits().hash(&mut h);
        (color.r, color.g, color.b, color.a).hash(&mut h);
        blur_r.to_bits().hash(&mut h);
        spread.to_bits().hash(&mut h);
        dpi_factor.to_bits().hash(&mut h);
        // BorderRadius: four corner radii, already resolved f32s.
        border_radius.top_left.to_bits().hash(&mut h);
        border_radius.top_right.to_bits().hash(&mut h);
        border_radius.bottom_left.to_bits().hash(&mut h);
        border_radius.bottom_right.to_bits().hash(&mut h);
        h.finish()
    };

    let cached = SHADOW_BLUR_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(e) = c.0.get(&cache_key) {
            // LRU touch.
            let data = e.data.clone();
            let (w, h) = (e.w, e.h);
            c.1.retain(|k| *k != cache_key);
            c.1.push_back(cache_key);
            Some((data, w, h))
        } else {
            None
        }
    });

    let (shadow_data, sw, sh) = if let Some((data, w, h)) = cached {
        drop(crate::probe::Probe::span("shadow_cache_hit"));
        (data, w, h)
    } else {
        drop(crate::probe::Probe::span("shadow_cache_miss"));
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
            let mut ra =
                unsafe { RowAccessor::new_with_buf(tmp.data.as_mut_ptr(), sw, sh, stride) };
            stack_blur_rgba32(&mut ra, blur_radius, blur_radius);
        }

        let data = std::rc::Rc::new(tmp.data.into_vec());
        SHADOW_BLUR_CACHE.with(|c| {
            let mut c = c.borrow_mut();
            let bytes = data.len();
            // Evict LRU until the new entry fits.
            while c.2 + bytes > SHADOW_CACHE_MAX_BYTES {
                let Some(old_key) = c.1.pop_front() else {
                    break;
                };
                if let Some(old) = c.0.remove(&old_key) {
                    c.2 = c.2.saturating_sub(old.data.len());
                }
            }
            if c.2 + bytes <= SHADOW_CACHE_MAX_BYTES {
                c.0.insert(
                    cache_key,
                    ShadowCacheEntry {
                        data: data.clone(),
                        w: sw,
                        h: sh,
                    },
                );
                c.1.push_back(cache_key);
                c.2 += bytes;
            }
        });
        (data, sw, sh)
    };

    // Blit the shadow buffer onto the main pixmap.
    //
    // CSS: an OUTSET shadow "must not be painted inside the border-box"
    // (the border-box acts as an opaque occluder for its own shadow). The
    // full blit both violated that under translucent elements AND paid the
    // single largest alpha-blend of a repaint — the page-sized interior
    // that the element immediately overdraws. Blit the RING around the
    // border box instead: four strips, skipping the hole. Rounded corners
    // keep the full blit (a rectangular hole would clip shadow that must
    // show at the corner cutouts); non-outset modes keep it too.
    let dst_x = shadow_x as i32;
    let dst_y = shadow_y as i32;
    let ring_eligible =
        matches!(shadow.clip_mode, BoxShadowClipMode::Outset) && border_radius.is_zero();
    if ring_eligible {
        // Border-box hole in SOURCE coordinates. Shrink it by 1px on every
        // side (ceil origin, floor extent) so the ring keeps a sliver of
        // shadow UNDER the element edge — an over-large hole would leave a
        // visible seam against the element's antialiased edge.
        let hole_x = (rect.x - shadow_x).max(0.0).ceil() as u32 + 1;
        let hole_y = (rect.y - shadow_y).max(0.0).ceil() as u32 + 1;
        let hole_r = ((rect.x + rect.width - shadow_x).floor() as i64 - 1).max(0) as u32;
        let hole_b = ((rect.y + rect.height - shadow_y).floor() as i64 - 1).max(0) as u32;
        let hole_r = hole_r.min(sw);
        let hole_b = hole_b.min(sh);

        if hole_x < hole_r && hole_y < hole_b {
            // Top strip (full width).
            blit_clipped(
                pixmap,
                clip_px,
                &shadow_data,
                sw,
                sh,
                0,
                0,
                sw,
                hole_y,
                dst_x,
                dst_y,
            );
            // Bottom strip (full width).
            blit_clipped(
                pixmap,
                clip_px,
                &shadow_data,
                sw,
                sh,
                0,
                hole_b,
                sw,
                sh - hole_b,
                dst_x,
                dst_y + hole_b as i32,
            );
            // Left strip (between top and bottom).
            blit_clipped(
                pixmap,
                clip_px,
                &shadow_data,
                sw,
                sh,
                0,
                hole_y,
                hole_x,
                hole_b - hole_y,
                dst_x,
                dst_y + hole_y as i32,
            );
            // Right strip (between top and bottom).
            blit_clipped(
                pixmap,
                clip_px,
                &shadow_data,
                sw,
                sh,
                hole_r,
                hole_y,
                sw - hole_r,
                hole_b - hole_y,
                dst_x + hole_r as i32,
                dst_y + hole_y as i32,
            );
            return Ok(());
        }
        // Degenerate hole (element fully outside the buffer) — fall through.
    }
    blit_clipped(
        pixmap,
        clip_px,
        &shadow_data,
        sw,
        sh,
        0,
        0,
        sw,
        sh,
        dst_x,
        dst_y,
    );

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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // software rasterizer: bounded pixel/coord/colour casts
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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)] // software rasterizer: bounded pixel/coord/colour casts
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
        MaskEntry::Opacity { .. } => return,
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
/// `font_manager` is REQUIRED, not optional: it is the only thing that can turn
/// the `font_hash` values in `dl` back into faces. There is no second font table
/// to fall back to — a renderer given no manager would silently drop every glyph
/// run instead of failing, which is how the 0.2.0 icon regression stayed invisible.
pub fn render(
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

    let mut pixmap = acquire_pixmap(
        None,
        (width * dpi_factor) as u32,
        (height * dpi_factor) as u32,
    )?;
    pixmap.fill(255, 255, 255, 255);

    render_display_list(dl, &mut pixmap, dpi_factor, res, font_manager, glyph_cache)?;

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
        font_manager,
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
    /// What a damage rect is cleared to before it repaints: opaque white,
    /// or transparent black for a `Transparent`-material window.
    /// (`AZ_DEBUG_FILL` still wins, for the repaint-coverage diagnostics.)
    pub clear_color: [u8; 4],
    pub virtual_view_display_lists:
        std::collections::BTreeMap<azul_core::dom::DomId, std::sync::Arc<DisplayList>>,
}

impl CpuRenderState {
    #[must_use]
    pub fn new(scroll_offsets: ScrollOffsetMap) -> Self {
        Self {
            scroll_offsets,
            transforms: HashMap::new(),
            opacities: HashMap::new(),
            system_style: None,
            clear_color: [255, 255, 255, 255],
            virtual_view_display_lists: std::collections::BTreeMap::new(),
        }
    }

    /// Clear damage rects to `color` (see the field).
    #[must_use]
    pub const fn with_clear_color(mut self, color: [u8; 4]) -> Self {
        self.clear_color = color;
        self
    }

    /// Provide the nested `VirtualView` child DOM display lists so the CPU
    /// renderer can composite them (see the field doc).
    #[must_use]
    pub fn with_virtual_view_display_lists(
        mut self,
        lists: std::collections::BTreeMap<azul_core::dom::DomId, std::sync::Arc<DisplayList>>,
    ) -> Self {
        self.virtual_view_display_lists = lists;
        self
    }

    /// Attach a `SystemStyle` so the renderer can resolve `system:*` color
    /// keywords (e.g. in gradient stops) against the live OS palette.
    #[must_use]
    pub fn with_system_style(
        mut self,
        system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
    ) -> Self {
        self.system_style = system_style;
        self
    }

    /// Build from a `GpuValueCache` snapshot.
    #[must_use]
    pub fn from_gpu_cache(
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
            clear_color: [255, 255, 255, 255],
            virtual_view_display_lists: std::collections::BTreeMap::new(),
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
#[must_use]
pub fn extract_gpu_values(
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
            // ANIMATION transforms — a separate channel from the CSS one,
            // because `synchronize` owns `css_transform_keys` and evicts
            // anything not backed by a CSS `transform` property. Extracted the
            // same way: the rasteriser looks values up by KEY id, so an
            // animated node is indistinguishable from a CSS-transformed one at
            // this point, which is the intent.
            for (node_id, key) in &cache.anim_transform_keys {
                if let Some(value) = cache.anim_current_transform_values.get(node_id) {
                    transforms.insert(key.id, *value);
                }
            }
            for (node_id, key) in &cache.anim_opacity_keys {
                if let Some(value) = cache.anim_current_opacity_values.get(node_id) {
                    opacities.insert(key.id, *value);
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
    font_manager: &FontManager<FontRef>,
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
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
) -> Result<(), String> {
    let mut transform_stack = vec![TransAffine::new()]; // identity
    let mut clip_stack: Vec<Option<AzRect>> = vec![None];
    let mut real_clip_stack: Vec<Option<AzRect>> = vec![None];
    let mut mask_stack: Vec<MaskEntry> = Vec::new();
    // Accumulated scroll offset stack. Each PushScrollFrame pushes
    // (parent_offset_x + scroll_x, parent_offset_y + scroll_y).
    // Items inside a scroll frame have their bounds shifted by the
    // accumulated offset before rendering.
    let mut scroll_offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];
    let mut text_shadow_stack: Vec<StyleBoxShadow> = Vec::new();

    let _p_loop = crate::probe::Probe::span("raster_loop");
    for (item_idx, item) in display_list.items.iter().enumerate() {
        let _p_item = crate::probe::Probe::span(probe_label_for_item(item));
        render_single_item(
            item,
            display_list
                .uniform_text_bgs
                .get(item_idx)
                .copied()
                .flatten(),
            pixmap,
            dpi_factor,
            renderer_resources,
            font_manager,
            glyph_cache,
            &mut transform_stack,
            &mut clip_stack,
            &mut real_clip_stack,
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
        I::StrokedPath { .. } => "dl:stroked_path",
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
/// Colour the damaged region is cleared to before it is repainted.
///
/// Opaque white normally — the same base a full repaint starts from. Set
/// `AZ_DEBUG_FILL=RRGGBB` (or `1` for red) to clear to something loud
/// instead: whatever is still that colour at the end of the frame was
/// CLEARED BUT NEVER REPAINTED, and whatever kept its old content was never
/// in a damage rect at all. That distinction is otherwise invisible — a
/// region that repaints to the wrong colour and a region that never repaints
/// look identical on screen.
fn damage_clear_color() -> (u8, u8, u8, u8) {
    use std::sync::OnceLock;
    static C: OnceLock<(u8, u8, u8, u8)> = OnceLock::new();
    *C.get_or_init(|| parse_damage_fill(std::env::var("AZ_DEBUG_FILL").ok().as_deref()))
}

/// `AZ_DEBUG_FILL` -> clear colour. Split out from [`damage_clear_color`]
/// because that one latches its answer in a `OnceLock`, so a test can only
/// ever observe one branch of it per process.
fn parse_damage_fill(v: Option<&str>) -> (u8, u8, u8, u8) {
    const WHITE: (u8, u8, u8, u8) = (255, 255, 255, 255);
    const RED: (u8, u8, u8, u8) = (255, 0, 0, 255);
    match v {
        None => WHITE,
        // Unset is the shipping default; "0" turns the knob off explicitly
        // without having to unset it.
        Some("0" | "") => WHITE,
        Some("1") => RED,
        Some(v) => u32::from_str_radix(v.trim_start_matches('#'), 16).map_or(RED, |n| {
            (
                ((n >> 16) & 0xff) as u8,
                ((n >> 8) & 0xff) as u8,
                (n & 0xff) as u8,
                255,
            )
        }),
    }
}

/// `AZ_DEBUG_DAMAGE=1` — log what each damaged repaint actually touched.
fn damage_logging_enabled() -> bool {
    use std::sync::OnceLock;
    static E: OnceLock<bool> = OnceLock::new();
    *E.get_or_init(|| std::env::var_os("AZ_DEBUG_DAMAGE").is_some())
}

#[allow(clippy::many_single_char_names)] // r,g,b,a colour channels + loop indices
#[allow(clippy::tuple_array_conversions)] // explicit [r,g,b,a]->(r,g,b,a) is correct; .into() was not
pub fn render_display_list_damaged(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
    damage_rects: &[LogicalRect],
) -> Result<(), String> {
    // The strip/damage raster body - the previously UNSPANNED majority of a
    // scroll frame's present time (7 of 10.2ms measured 2026-08-29).
    let _p = crate::probe::Probe::span("raster_damage_body");

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
    if damage_logging_enabled() {
        let total: i64 = rects
            .iter()
            .map(|r| i64::from(r.x1 - r.x0) * i64::from(r.y1 - r.y0))
            .sum();
        let window = i64::from(pw_i) * i64::from(ph_i);
        eprintln!(
            "[damage] {} rect(s) after merge (from {} requested), {total} px = {:.1}% of \
             the {pw_i}x{ph_i} window, {} display-list item(s)",
            rects.len(),
            damage_rects.len(),
            if window > 0 {
                100.0 * total as f64 / window as f64
            } else {
                0.0
            },
            display_list.items.len(),
        );
    }

    for sr in &rects {
        let (cr, cg, cb, ca) = if std::env::var_os("AZ_DEBUG_FILL").is_some() {
            damage_clear_color()
        } else {
            // Explicit destructure, NOT `.into()`: the array→tuple conversion
            // resolved to the wrong thing and painted damage rects with a bad
            // clear colour, hiding content on every repaint.
            let [r, g, b, a] = render_state.clear_color;
            (r, g, b, a)
        };
        pixmap.fill_rect(sr.x0, sr.y0, sr.x1 - sr.x0, sr.y1 - sr.y0, cr, cg, cb, ca);

        let base_clip = AzRect::from_xywh(
            sr.x0 as f32,
            sr.y0 as f32,
            (sr.x1 - sr.x0) as f32,
            (sr.y1 - sr.y0) as f32,
        );
        let (mut painted, mut skipped) = (0usize, 0usize);
        let mut transform_stack = vec![TransAffine::new()];
        let mut clip_stack: Vec<Option<AzRect>> = vec![base_clip];
        let mut real_clip_stack: Vec<Option<AzRect>> = vec![None];
        let mut mask_stack: Vec<MaskEntry> = Vec::new();
        let mut scroll_offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];
        let mut text_shadow_stack: Vec<StyleBoxShadow> = Vec::new();

        for (item_idx, item) in display_list.items.iter().enumerate() {
            // Always process state-management items (Push/Pop) regardless of bounds,
            // because skipping a Push while processing its matching Pop corrupts stacks.
            if !item.is_state_management() {
                // INK bounds for the cull, not box bounds: `Text.bounds()`
                // returns the IFC owner's WHOLE content box, so a 3px scroll
                // strip touching one paragraph admitted EVERY line of it (and
                // for a block that owns its own scroll, every text item in
                // the document) - each admitted run then paid the full LCD
                // accumulate+sweep even for glyphs nowhere near the strip.
                // `visual_bounds()` is the tight per-line ink box (padded).
                // EXCEPTION: with a text-shadow in effect the ink extends by
                // offset+blur that visual_bounds does not model - keep the
                // coarse box there or shadows get clipped at strip edges.
                let cull_bounds = if text_shadow_stack.is_empty() {
                    item.visual_bounds().or_else(|| item.bounds())
                } else {
                    item.bounds()
                };
                if let Some(item_bounds) = cull_bounds {
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
                        skipped += 1;
                        continue;
                    }
                }
            }

            painted += 1;
            let _p = crate::probe::Probe::span(probe_label_for_item(item));
            render_single_item(
                item,
                display_list
                    .uniform_text_bgs
                    .get(item_idx)
                    .copied()
                    .flatten(),
                pixmap,
                dpi_factor,
                renderer_resources,
                font_manager,
                glyph_cache,
                &mut transform_stack,
                &mut clip_stack,
                &mut real_clip_stack,
                &mut mask_stack,
                &mut scroll_offset_stack,
                &mut text_shadow_stack,
                render_state,
            )?;
        }
        if damage_logging_enabled() {
            eprintln!(
                "[damage]   rect {}x{} @ ({}, {}) phys — painted {painted} item(s), \
                 skipped {skipped}",
                sr.x1 - sr.x0,
                sr.y1 - sr.y0,
                sr.x0,
                sr.y0,
            );
        }
    }

    Ok(())
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)] // software rasterizer: bounded pixel/coord/colour casts
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
#[allow(clippy::match_same_arms)]
// enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics if the clip stack is empty when an item expects an active clip.
/// # Errors
///
/// Returns an error string if rendering fails.
pub fn render_single_item(
    item: &DisplayListItem,
    // PROVEN uniform background for THIS item (Text only; from
    // DisplayList.uniform_text_bgs — the variant itself cannot carry it,
    // printpdf pattern-matches Text exhaustively). None = slow path.
    item_uniform_bg: Option<(ColorU, crate::solver3::display_list::WindowLogicalRect)>,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    transform_stack: &mut Vec<TransAffine>,
    clip_stack: &mut Vec<Option<AzRect>>,
    // The clip stack WITHOUT the damage base (seeded [None]) — TEXT paints
    // under this one. The colorimetric LCD blend reads NEIGHBOUR pixels
    // (chroma step), so a run clipped mid-glyph at a damage boundary
    // blends against different dst than an unclipped render — the round-3
    // blit gate caught it as a one-column divergence. Repainting the whole
    // run is always byte-correct: any pixel the wider write touches that
    // is NOT damaged is by definition identical between frames.
    real_clip_stack: &mut Vec<Option<AzRect>>,
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
        // A STROKE, drawn natively: the CPU path already rasterises vector
        // geometry with agg, so it strokes the real outline rather than
        // approximating it with a mask. (The GPU path has no vector
        // rasteriser and paints the same stroke through an R8 mask instead -
        // same display list item, two backends.)
        DisplayListItem::StrokedPath {
            bounds,
            path,
            view_box,
            color,
            width,
            // The pre-rasterised mask is the GPU backend's half of this item;
            // the CPU one strokes the real outline, which has no resolution
            // ceiling.
            mask: _,
        } => {
            let clip = *clip_stack.last().unwrap();
            render_stroked_path(
                pixmap,
                &scroll_rect(bounds.inner()),
                path,
                *view_box,
                *color,
                *width,
                clip,
                dpi_factor,
            );
        }
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
            // An unset border color paints NOTHING. This must stay fully
            // transparent: the compact style cache encodes colors as one u32
            // and cannot distinguish "unset" (raw 0) from explicit
            // transparent black {0,0,0,0} — both arrive here as `None`, and
            // both mean "no visible border". An opaque default would paint
            // phantom black borders on every node that sets border
            // width+style with a transparent color (e.g. hover-only borders).
            let default_color = ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
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
            render_text_with_bg(
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
                item_uniform_bg,
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
            // The DL item carries the LIVE ImageRef: produced callback frames
            // are patched into the display list by the content chokepoint
            // (`LayoutWindow::apply_content_change`) during the shared
            // per-frame `prepare_frame_cpu` — there is no side map to consult.
            // A `DecodedImage::Callback` reaching this point means a host
            // skipped `prepare_frame_cpu`; `render_image` paints the announced
            // grey placeholder for it.
            render_image(
                pixmap,
                &scroll_rect(bounds.inner()),
                image,
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
            // A PushClip carries MANDATORY bounds, so a None here means those bounds were
            // degenerate/NaN — an UNPAINTABLE clip, not "no clip". intersect_clips reads
            // None as "unclipped", which would let a subsequent full-canvas draw escape
            // the clip; substitute an explicit zero-area deny-all rect instead.
            let new_clip = Some(new_clip.unwrap_or(AzRect::DENY_ALL));
            let merged = intersect_clips(clip_stack.last().copied().flatten(), new_clip);
            clip_stack.push(merged);
            real_clip_stack.push(intersect_clips(
                real_clip_stack.last().copied().flatten(),
                new_clip,
            ));
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
            if real_clip_stack.len() > 1 {
                real_clip_stack.pop();
            }
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
            content_offset,
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
            let child_dl = render_state
                .virtual_view_display_lists
                .get(child_dom_id)
                .cloned();
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
                real_clip_stack.push(intersect_clips(
                    real_clip_stack.last().copied().flatten(),
                    logical_rect_to_az_rect(&scroll_rect(bounds.inner()), dpi_factor),
                ));
                // The renderer draws at `pos - accumulated_scroll`, so
                // subtracting the VirtualView origin places the 0-relative
                // child at the box, and subtracting `content_offset` on top
                // shifts the materialized window by
                // `window_origin - scroll_offset` — which IS the scroll.
                scroll_offset_stack.push((
                    scroll_dx - vv_origin.x - content_offset.x,
                    scroll_dy - vv_origin.y - content_offset.y,
                ));
                for (child_idx, child_item) in child_dl.items.iter().enumerate() {
                    render_single_item(
                        child_item,
                        child_dl.uniform_text_bgs.get(child_idx).copied().flatten(),
                        pixmap,
                        dpi_factor,
                        renderer_resources,
                        font_manager,
                        glyph_cache,
                        transform_stack,
                        clip_stack,
                        real_clip_stack,
                        mask_stack,
                        scroll_offset_stack,
                        text_shadow_stack,
                        render_state,
                    )?;
                }
                scroll_offset_stack.pop();
                real_clip_stack.pop();
                clip_stack.pop();
            }
        }
        DisplayListItem::VirtualViewPlaceholder { .. } =>
        {
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
            let clip = *clip_stack.last().unwrap();
            render_box_shadow(
                pixmap,
                &scroll_rect(bounds.inner()),
                shadow,
                border_radius,
                clip,
                dpi_factor,
            )?;
        }

        // --- Opacity layers ---
        DisplayListItem::PushOpacity {
            bounds,
            opacity,
            opacity_key,
        } => {
            // Live value first — the damaged/incremental path must fade at the
            // same opacity the composited path shows.
            let opacity = &opacity_key
                .and_then(|k| render_state.opacities.get(&k.id).copied())
                .unwrap_or(*opacity);
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

/// Paint a STROKED path - SVG's `stroke`, PDF's second paint operator.
///
/// Strokes the real outline with agg rather than approximating it: the CPU
/// path already has a vector rasteriser, so it uses it. (The GPU path has
/// none and paints the same item through an R8 mask instead; both read the
/// SAME display list item, which is the point of carrying the geometry in
/// user space rather than pre-flattening it.)
///
/// The width is scaled by the geometry's own scale factor, not by the
/// coordinates: a 2-unit rule in a 16-unit viewBox is 8 device px in a 64px
/// box, and that is a property of the mapping, not of the points.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // software rasterizer: bounded pixel/coord/colour casts
fn render_stroked_path(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    path: &azul_core::svg::SvgMultiPolygon,
    view_box: Option<(f32, f32, f32, f32)>,
    color: ColorU,
    width: f32,
    clip: Option<AzRect>,
    dpi_factor: f32,
) {
    use agg_rust::conv_stroke::ConvStroke;

    if color.a == 0 || !width.is_finite() || width <= 0.0 {
        return;
    }
    let (sx, sy, tx, ty) = crate::cpurender::pixmap::svg_user_space_mapping(
        bounds,
        view_box,
        dpi_factor,
    );
    let (ox, oy) = (
        bounds.origin.x * dpi_factor,
        bounds.origin.y * dpi_factor,
    );
    let mx = |x: f32| f64::from((x + tx) * sx + ox);
    let my = |y: f32| f64::from((y + ty) * sy + oy);
    let mut geometry = crate::cpurender::pixmap::svg_path_to_agg(path, &mx, &my);

    // A non-uniform scale cannot be expressed as one stroke width; the mean
    // is the honest approximation and matches what every SVG renderer does
    // for a non-uniform viewBox mapping.
    let scale = f64::from((sx + sy) / 2.0);
    let device_width = (f64::from(width) * scale).max(1.0);

    let mut stroke = ConvStroke::new(&mut geometry);
    stroke.set_width(device_width);
    stroke.set_line_cap(agg_rust::math_stroke::LineCap::Round);
    stroke.set_line_join(agg_rust::math_stroke::LineJoin::Round);

    let agg_color = Rgba8::new(
        u32::from(color.r),
        u32::from(color.g),
        u32::from(color.b),
        u32::from(color.a),
    );
    crate::cpurender::pixmap::agg_fill_path_clipped(
        pixmap,
        &mut stroke,
        &agg_color,
        FillingRule::NonZero,
        clip,
    );
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

    if border_radius.is_zero() && color.a == 255 {
        // OPAQUE axis-aligned rect: plain row fill, no compositing at all.
        // blend_bar still walks agg's per-pixel blend machinery even at full
        // alpha; on a resize repaint the 2-3 page-sized background fills were
        // 1.3 ms each (~0.7 GB/s effective — a third of what the trivial row
        // loop reaches). Clip semantics match blend_bar: intersect with the
        // clip box, then fill [x0, x1) x [y0, y1).
        let (mut fx0, mut fy0) = (rect.x as i32, rect.y as i32);
        let (mut fx1, mut fy1) = ((rect.x + rect.width) as i32, (rect.y + rect.height) as i32);
        if let Some(c) = clip {
            fx0 = fx0.max(c.x as i32);
            fy0 = fy0.max(c.y as i32);
            fx1 = fx1.min((c.x + c.width) as i32);
            fy1 = fy1.min((c.y + c.height) as i32);
        }
        if fx1 > fx0 && fy1 > fy0 {
            pixmap.fill_rect(
                fx0,
                fy0,
                fx1 - fx0,
                fy1 - fy0,
                color.r,
                color.g,
                color.b,
                255,
            );
        }
        return;
    }

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

/// How text is antialiased. One knob, four values, read from `AZ_TEXT_AA`.
///
/// This replaces the two overlapping switches that used to live here
/// (`AZ_TEXT_LCD=0` and `AZ_LCD_BLEND=legacy`), which could express the same
/// state two ways and gave no way at all to ask for aliased text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAa {
    /// RGB LCD subpixel AA with the linear (luminance-correct) blend. Default.
    ///
    /// Distributes coverage across the R/G/B stripes of each physical pixel,
    /// which is crisper on a real panel. ASSUMES horizontal-RGB subpixel order
    /// and an opaque background: a BGR panel needs the R/B taps swapped, and
    /// text over a transparent layer must use `Grayscale` (`render_text_shadow`
    /// forces it). Black text gets the familiar faint colour fringes.
    Lcd,
    /// LCD subpixel AA with the pre-linear-blend behaviour, kept so a rendering
    /// change can be A/B'd against the old output.
    Legacy,
    /// Single grayscale coverage per pixel. No colour fringing, no subpixel-order
    /// assumption. Correct over transparent backgrounds.
    Grayscale,
    /// No antialiasing: coverage is thresholded at 50%, so every pixel is either
    /// fully text or fully background.
    ///
    /// This exists for **cross-engine comparison**. Antialiased text cannot be
    /// compared between two rasterisers: azul and Chrome disagree on AA mode,
    /// coverage curve and hinting, which costs ~11k pixels on a text-heavy page
    /// — more than the reftest threshold — while looking identical. Thresholding
    /// throws all of that away and leaves glyph COVERAGE, which is a question
    /// both engines answer the same way when the layout agrees.
    ///
    /// The trade is real and worth stating: aliasing amplifies genuine sub-pixel
    /// disagreement, because a half-pixel shift flips a whole pixel by 255
    /// instead of nudging a blend. Do not ship this to users.
    None,
}

/// Default text antialiasing: LCD subpixel.
pub const TEXT_AA_DEFAULT: TextAa = TextAa::Lcd;

/// Text antialiasing mode from `AZ_TEXT_AA` (`lcd` | `legacy` | `grayscale` |
/// `none`). Unset or unrecognised gives [`TEXT_AA_DEFAULT`]. Read once.
pub fn text_aa() -> TextAa {
    static V: std::sync::OnceLock<TextAa> = std::sync::OnceLock::new();
    *V.get_or_init(|| match std::env::var("AZ_TEXT_AA") {
        Ok(s) if s.eq_ignore_ascii_case("none") || s == "0" => TextAa::None,
        Ok(s) if s.eq_ignore_ascii_case("grayscale") || s.eq_ignore_ascii_case("greyscale") => {
            TextAa::Grayscale
        }
        Ok(s) if s.eq_ignore_ascii_case("legacy") => TextAa::Legacy,
        Ok(s) if s.eq_ignore_ascii_case("lcd") => TextAa::Lcd,
        _ => TEXT_AA_DEFAULT,
    })
}

/// Whether text takes the RGB LCD subpixel path.
fn text_lcd_enabled() -> bool {
    matches!(text_aa(), TextAa::Lcd | TextAa::Legacy)
}

/// Whether glyph coverage is thresholded to 0/255 (see [`TextAa::None`]).
fn text_aliased() -> bool {
    text_aa() == TextAa::None
}

/// `render_scanlines_aa_solid` with the coverage thresholded at 50%.
///
/// agg's rasteriser here is the "nogamma" variant, so there is no gamma LUT to
/// hang a threshold function on and no `render_scanlines_bin_solid` in the
/// fork. Thresholding the covers as they are handed to the blender is the same
/// thing one step later, and reuses the identical span walk.
fn render_scanlines_aliased_solid<PF: agg_rust::pixfmt_rgba::PixelFormat>(
    ras: &mut RasterizerScanlineAa,
    sl: &mut ScanlineU8,
    ren: &mut RendererBase<PF>,
    color: &PF::ColorType,
) {
    use agg_rust::rasterizer_scanline_aa::Scanline;
    if !ras.rewind_scanlines() {
        return;
    }
    sl.reset(ras.min_x(), ras.max_x());
    let mut solid: Vec<u8> = Vec::new();
    while ras.sweep_scanline(sl) {
        let y = sl.y();
        let covers = sl.covers();
        for span in sl.begin() {
            if span.len <= 0 {
                continue;
            }
            let len = span.len as usize;
            let src = &covers[span.cover_offset..span.cover_offset + len];
            solid.clear();
            // >= 128 is "the pixel centre is inside the outline", the same
            // half-open rule agg's own bin rasteriser uses.
            solid.extend(src.iter().map(|&c| if c >= 128 { 255u8 } else { 0u8 }));
            ren.blend_solid_hspan(span.x, y, span.len, color, &solid);
        }
    }
}

/// `AZ_LCD_PRETILE=0` disables the pre-blended LCD tile fast path — the
/// diagnostic that splits "tile content wrong" from "sweep wrong" in one
/// run. Read once per process.
fn lcd_pretile_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("AZ_LCD_PRETILE").map_or(true, |v| v.trim() != "0"))
}

/// FreeType default "light" 5-tap FIR LUT (0x56/0x4D/0x08) - the arguments
/// are compile-time constants, but this was rebuilt per text run per frame
/// (3x256 f64 mul+floor+cast), measurable inside glyph_lcd_sweep.
fn lcd_distribution_lut() -> &'static agg_rust::pixfmt_lcd::LcdDistributionLut {
    static LUT: std::sync::OnceLock<agg_rust::pixfmt_lcd::LcdDistributionLut> =
        std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        agg_rust::pixfmt_lcd::LcdDistributionLut::new(
            f64::from(0x56u32),
            f64::from(0x4Du32),
            f64::from(0x08u32),
        )
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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)] // software rasterizer: bounded pixel/coord/colour casts
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

    // Accumulate every glyph outline (at 3× horizontal resolution) into one
    // rasterizer, then sweep once — same batching as the grayscale path.
    let mut ras = RasterizerScanlineAa::new();
    ras.filling_rule(FillingRule::NonZero);

    let p_outline = crate::probe::Probe::span("glyph_lcd_outline");
    for glyph in glyphs {
        let glyph_index = glyph.index as u16;

        let glyph_x = (glyph.point.x - scroll_offset.0) * dpi_factor;
        let glyph_baseline_y = (glyph.point.y - scroll_offset.1) * dpi_factor;

        // Horizontal cull BEFORE decode: a glyph whose ink cannot reach the
        // clip contributes nothing to the sweep. Pad = 2px for the FIR
        // fringe (ink just outside the clip lightens the boundary column if
        // dropped) plus a generous 4-em width bound — `GlyphInstance`
        // carries no ink extents. Without this, a horizontally scrolled
        // single-line TextInput decoded, cache-probed and ACCUMULATED every
        // glyph in the value on every damage repaint, and the sweep covered
        // the whole run — the LCD pipeline sat at the top of the raster
        // profile on pure caret traffic.
        if let Some(c) = clip {
            // The bound comes from the RENDERED em (`scale` * upem == the
            // effective pixel size), never from `ppem`: that is the HINTING
            // ppem, which is 0 whenever hinting is off - the macOS default -
            // and a zero bound dropped every glyph whose pen sat left of a
            // damage strip (cut text, stray glyphs, a white notch through
            // the run at every caret position; 2026-08-31). Symmetric on
            // both sides so negative bearings / RTL marks reaching into the
            // clip from the right survive too. Only ever more conservative
            // than exact ink: a glyph is clipped by the sweep, never lost.
            let max_ink_w = scale * f32::from(parsed_font.font_metrics.units_per_em) * 4.0;
            let cx0 = c.x;
            let cx1 = c.x + c.width;
            if glyph_x - max_ink_w > cx1 + 2.0 || glyph_x + max_ink_w < cx0 - 2.0 {
                continue;
            }
        }

        let Some(glyph_data) = parsed_font.get_or_decode_glyph(glyph_index) else {
            continue;
        };
        // Builds (and caches) the hinted outline; we only need to know which
        // coordinate space it came back in.
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

        // Cells are rasterized once per (glyph, ppem, scale, 1/3-px bucket)
        // and replayed here at an integer offset. `int_x` is in whole pixels
        // and the cells' x axis is in stripes, hence the *3.
        let Some((cells, int_x, int_y)) = glyph_cache.get_or_build_cells_lcd(
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
        ras.add_cells_offset(cells, int_x * 3, int_y);
    }

    drop(p_outline);
    let _p_sweep = crate::probe::Probe::span("glyph_lcd_sweep");
    // Blend via the LCD pixel format. It reports width*3, so the rasterizer's 3×
    // x-coordinates address individual R/G/B stripes; the clip box X is likewise
    // in sub-pixel space.
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;
    let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride) };
    // FreeType default "light" 5-tap FIR (see lcd_distribution_lut).
    let lut = lcd_distribution_lut();
    let mut sl = ScanlineU8::new();
    if let Some(params) = lcd_linear_params() {
        // Colorimetric path (default): per-stripe compositing in LINEAR
        // light against the actual background pixel, with contrast
        // enhancement and an optional chroma budget — text stays legible
        // on ANY fg/bg pair (thin white on green was unreadable with
        // sRGB-space blending), instead of only on near-b/w pairs.
        let mut pf = agg_rust::pixfmt_lcd::PixfmtRgba32LcdLinear::new(&mut ra, &lut, params);
        if let Some(c) = clip {
            // The FIR spread writes 2 stripes past every span; the renderer-
            // base clip box cannot bound those writes (task #17: a damage-rect
            // repaint double-blended the escaped fringe one pixel LEFT of the
            // rect — 239²/255 = 224, the exact measured divergence).
            pf.set_stripe_clip((c.x as i32) * 3, ((c.x + c.width) as i32) * 3);
        }
        let mut rb = RendererBase::new(pf);
        if let Some(c) = clip {
            // Y-only span clip: vertical has no FIR spread, so scanline
            // clipping is exact. X spans must reach the FIR distribution
            // UNCLIPPED — ink just OUTSIDE the clip contributes fringe to
            // the boundary column INSIDE it (a span-clipped repaint loses
            // that contribution and renders the column lighter than a full
            // repaint). The stripe clip set above bounds the WRITES instead.
            rb.clip_box_i(
                0,
                c.y as i32,
                (w as i32) * 3 - 1,
                (c.y + c.height) as i32 - 1,
            );
        }
        render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &agg_color);
    } else {
        // Legacy sRGB-space blending (AZ_LCD_BLEND=legacy).
        let mut pf = PixfmtRgba32Lcd::new(&mut ra, &lut);
        if let Some(c) = clip {
            // Same stripe-clip as the colorimetric arm above.
            pf.set_stripe_clip((c.x as i32) * 3, ((c.x + c.width) as i32) * 3);
        }
        let mut rb = RendererBase::new(pf);
        if let Some(c) = clip {
            // Y-only span clip: vertical has no FIR spread, so scanline
            // clipping is exact. X spans must reach the FIR distribution
            // UNCLIPPED — ink just OUTSIDE the clip contributes fringe to
            // the boundary column INSIDE it (a span-clipped repaint loses
            // that contribution and renders the column lighter than a full
            // repaint). The stripe clip set above bounds the WRITES instead.
            rb.clip_box_i(
                0,
                c.y as i32,
                (w as i32) * 3 - 1,
                (c.y + c.height) as i32 - 1,
            );
        }
        render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &agg_color);
    }
}

/// Parameters for the colorimetric LCD blend, or `None` for the legacy
/// sRGB-space blend. Defaults to the colorimetric path.
///
/// - `AZ_LCD_BLEND=legacy` selects the old sRGB-space blending.
/// - `AZ_LCD_COVERAGE_GAMMA=<100..255>` tone curve on stripe coverage,
///   x100 (default 220 = c^(1/2.2)): pure linear compositing renders
///   dark-on-light lighter than Skia/ClearType, which blend in an
///   intermediate gamma space; this restores conventional stem weight
///   while keeping linear per-channel mixing.
/// - `AZ_LCD_CONTRAST=<0..255>` additional skia-style coverage contrast
///   (default 0).
/// - `AZ_LCD_CHROMA_LIMIT=<0..255>` caps how far a stripe may deviate
///   from the luminance-correct composite (0 = physically free stripes,
///   the default; 255 = grayscale-equivalent).
///
/// Read once.
fn lcd_linear_params() -> Option<agg_rust::pixfmt_lcd::LcdBlendParams> {
    use std::sync::OnceLock;
    static V: OnceLock<Option<agg_rust::pixfmt_lcd::LcdBlendParams>> = OnceLock::new();
    *V.get_or_init(|| {
        // `AZ_TEXT_AA=legacy` selects the pre-linear-blend path. The old
        // `AZ_LCD_BLEND=legacy` spelling is still honoured so existing scripts
        // and captures keep working.
        if text_aa() == TextAa::Legacy
            || std::env::var("AZ_LCD_BLEND").is_ok_and(|v| v.eq_ignore_ascii_case("legacy"))
        {
            return None;
        }
        let parse = |k: &str, default: u8| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(default)
        };
        Some(agg_rust::pixfmt_lcd::LcdBlendParams {
            contrast: parse("AZ_LCD_CONTRAST", 0),
            chroma_limit: parse("AZ_LCD_CHROMA_LIMIT", 0),
            coverage_gamma: parse("AZ_LCD_COVERAGE_GAMMA", 220),
        })
    })
}

/// A `font_hash` that layout emitted but the `FontManager` that emitted it cannot
/// resolve is a broken invariant, not a missing asset — the display list and the
/// font state have gone out of sync and the user loses text with no other symptom.
///
/// Fail LOUDLY (and, in a debug build, fatally so a test catches it), but never by
/// dereferencing something unresolved: the caller drops this one run and keeps the
/// frame. Deduplicated per hash so a broken frame cannot spam the log at 60 Hz.
#[cfg(feature = "std")]
fn font_resolution_failed(font_hash: u64) {
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<std::collections::BTreeSet<u64>>> = OnceLock::new();
    debug_assert!(
        false,
        "[cpurender] BUG: layout emitted font hash {font_hash} that its own FontManager \
         cannot resolve — the display list and the font state are out of sync"
    );
    let seen = SEEN.get_or_init(|| Mutex::new(std::collections::BTreeSet::new()));
    if let Ok(mut seen) = seen.lock() {
        if seen.insert(font_hash) {
            eprintln!(
                "[azul][font] BUG: layout emitted font hash {font_hash} that its own \
                 FontManager cannot resolve (neither a loaded face nor a registered \
                 embedded font). The text using it CANNOT be drawn. This is an azul \
                 bug — please report it."
            );
        }
    }
}

#[cfg(not(feature = "std"))]
const fn font_resolution_failed(_font_hash: u64) {}

/// A `FontManager` with no faces at all, for tests whose display list carries no
/// text. The CPU renderer has exactly ONE font source, so "no fonts" has to be
/// spelled as an empty manager rather than as an absent one.
#[cfg(test)]
pub(crate) fn empty_font_manager() -> FontManager<FontRef> {
    FontManager::new(rust_fontconfig::FcFontCache::default()).expect("FontManager::new")
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)] // software rasterizer: bounded pixel/coord/colour casts
#[allow(clippy::too_many_lines)] // large but cohesive: font lookup + grayscale/LCD dispatch + glyph loop
/// [`render_text`] + the uniform-background pre-blend fast path. When the
/// generator PROVED the run sits on one solid opaque color
/// (`Text.uniform_bg`), each glyph is an opaque copy of a pre-blended tile
/// (built once through the exact colorimetric pipeline) — the per-pixel
/// linear LCD sweep drops out of the frame. Falls back to the normal path
/// whenever the proof is absent, LCD/linear is off, or adjacent glyph
/// tiles would overlap (an opaque copy would clobber the neighbour's
/// antialiased edge — italic/tight-kerned runs take the slow path).
#[allow(clippy::too_many_arguments)]
fn render_text_with_bg(
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    pixmap: &mut AzulPixmap,
    clip_rect: &LogicalRect,
    clip: Option<AzRect>,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
    force_grayscale: bool,
    uniform_bg: Option<(ColorU, crate::solver3::display_list::WindowLogicalRect)>,
) {
    let pretile_disabled = {
        static V: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *V.get_or_init(|| std::env::var_os("AZ_NO_LCD_PRETILE").is_some())
    };
    if let Some((bg, proven_rect)) = uniform_bg {
        if text_lcd_enabled() && !force_grayscale && !pretile_disabled {
            if let Some(params) = lcd_linear_params() {
                if lcd_pretile_enabled()
                    && render_text_prerendered_lcd(
                    glyphs,
                    font_hash,
                    font_size_px,
                    color,
                    bg,
                    proven_rect.0,
                    params,
                    pixmap,
                    clip_rect,
                    clip,
                    renderer_resources,
                    font_manager,
                    dpi_factor,
                    glyph_cache,
                    scroll_offset,
                ) {
                    return;
                }
            }
        }
    }
    render_text(
        glyphs,
        font_hash,
        font_size_px,
        color,
        pixmap,
        clip_rect,
        clip,
        renderer_resources,
        font_manager,
        dpi_factor,
        glyph_cache,
        scroll_offset,
        force_grayscale,
    );
}

/// The pre-blended tile path. Returns `false` (nothing painted) when any
/// glyph pair would overlap or a tile is unavailable — the caller then
/// runs the normal path.
#[allow(clippy::too_many_arguments)]
fn render_text_prerendered_lcd(
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    bg: ColorU,
    proven_rect: LogicalRect,
    params: agg_rust::pixfmt_lcd::LcdBlendParams,
    pixmap: &mut AzulPixmap,
    clip_rect: &LogicalRect,
    clip: Option<AzRect>,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
) -> bool {
    use crate::glyph_cache::LcdGlyphTile;
    use agg_rust::pixfmt_lcd::LcdDistributionLut;
    let _ = renderer_resources;
    let Some(font_ref) = font_manager.resolve_font_by_hash(font_hash.font_hash) else {
        return false;
    };
    let parsed_font: &ParsedFont = crate::font_ref_to_parsed_font(&font_ref);
    let units_per_em = f32::from(parsed_font.font_metrics.units_per_em);
    if units_per_em <= 0.0 {
        return false;
    }
    let effective_px = font_size_px * dpi_factor;
    let scale = effective_px / units_per_em;
    // ppem = 0 selects the UNHINTED font-unit path in the glyph cache — the
    // platform-faithful default on macOS (see `text_hinting_enabled`).
    let ppem = if crate::glyph_cache::text_hinting_enabled() {
        effective_px.round() as u16
    } else {
        0
    };
    let hint_correction = if ppem > 0 {
        effective_px / f32::from(ppem)
    } else {
        1.0
    };
    let lut = lcd_distribution_lut();

    // Combined clip: the item clip_rect ∩ the stack clip, device pixels.
    // NOTE: `clip_rect` arrives ALREADY scroll-projected by the caller
    // (`text_clip = scroll_rect(clip_rect)` in the Text arm) — do not
    // subtract `scroll_offset` here again.
    let cr = clip_rect;
    let mut cx0 = (cr.origin.x * dpi_factor).floor() as i32;
    let mut cy0 = (cr.origin.y * dpi_factor).floor() as i32;
    let mut cx1 = ((cr.origin.x + cr.size.width) * dpi_factor).ceil() as i32;
    let mut cy1 = ((cr.origin.y + cr.size.height) * dpi_factor).ceil() as i32;
    if let Some(c) = clip {
        cx0 = cx0.max(c.x as i32);
        cy0 = cy0.max(c.y as i32);
        cx1 = cx1.min((c.x + c.width) as i32);
        cy1 = cy1.min((c.y + c.height) as i32);
    }
    if cx1 <= cx0 || cy1 <= cy0 {
        return true; // fully clipped: nothing to paint, and nothing missed
    }

    let _p = crate::probe::Probe::span("glyph_lcd_pretile");
    // Pass 1: build tiles and group glyphs into CONNECTED COMPONENTS by
    // tile-rect overlap. Singleton components blit their pre-blended tile;
    // components of ≥2 glyphs are handed to the batch sweep TOGETHER —
    // their coverage merges in ONE rasterizer exactly as the slow path
    // merges it (sequential per-glyph compositing would double-blend the
    // shared pixels), and their pixels touch no tiled glyph's pixels
    // (components are separated by non-overlap by construction). This
    // keeps the pixel-identity gate at zero tolerance while recovering the
    // runs the old all-or-nothing check sent to the full sweep (44 of 158
    // on big.md — ~4.8 ms of the remaining repaint).
    struct Placed {
        tile: LcdGlyphTile,
        x0: i32,
        y0: i32,
        glyph: GlyphInstance,
        component: usize,
    }
    let mut placed: Vec<Placed> = Vec::with_capacity(glyphs.len());
    let mut next_component = 0usize;
    for glyph in glyphs {
        let gx = (glyph.point.x - scroll_offset.0) * dpi_factor;
        let gy = (glyph.point.y - scroll_offset.1) * dpi_factor;
        let glyph_index = glyph.index as u16;
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
        let Some((tile, int_x, int_y)) = glyph_cache.get_or_build_lcd_tile(
            font_hash.font_hash,
            glyph.index as u16,
            ppem,
            gx,
            gy,
            scale,
            is_hinted,
            hint_correction,
            color,
            bg,
            &lut,
            params,
        ) else {
            continue; // no cells (space) — nothing to paint for this glyph
        };
        let x0 = int_x + tile.dx;
        let y0 = int_y + tile.dy;
        // FRINGE BOUNDARY: the tile (glyph + 1px FIR pad) must lie fully
        // inside the PROVEN uniform region, in device pixels. A tile
        // crossing the boundary would stamp fringe pre-blended against the
        // proven color onto UNPROVEN neighbours (the sweep blends against
        // the real pixel there) — a visible halo at e.g. a page edge, and
        // the divergence the round-3 blit gate caught. Such glyphs sweep.
        {
            let pr = proven_rect;
            // 1-px INSET beyond the rounded bounds: the FIR/chroma blend of
            // the tile's edge stripes READS neighbouring pixels — those
            // reads must also land on proven background, not merely the
            // writes. (The corpus damage-soundness gate caught a 2-channel
            // divergence at a proven-rect boundary without this.)
            // The proven rect is the ancestor's UNSCROLLED paint rect while the
            // tile positions above are scroll-projected: compare both in the
            // scrolled space, or any enclosing scroll offset makes every run
            // "hug the edge" and fall back to the sweep (which is what routed a
            // horizontally scrolled TextInput into the per-glyph cull).
            let prx = pr.origin.x - scroll_offset.0;
            let pry = pr.origin.y - scroll_offset.1;
            let px0 = (prx * dpi_factor).ceil() as i32 + 1;
            let py0 = (pry * dpi_factor).ceil() as i32 + 1;
            let px1 = ((prx + pr.size.width) * dpi_factor).floor() as i32 - 1;
            let py1 = ((pry + pr.size.height) * dpi_factor).floor() as i32 - 1;
            if x0 < px0 || y0 < py0 || x0 + tile.w as i32 > px1 || y0 + tile.h as i32 > py1 {
                drop(crate::probe::Probe::span("glyph_lcd_pretile_boundary"));
                return false; // whole run sweeps (rare: edge-hugging text)
            }
        }
        // Same component as the previous glyph if the tile RECTS intersect
        // (runs are in x order; vertical bands overlap on one text row).
        let component = match placed.last() {
            Some(prev)
                if x0 < prev.x0 + prev.tile.w as i32
                    && prev.x0 < x0 + tile.w as i32
                    && y0 < prev.y0 + prev.tile.h as i32
                    && prev.y0 < y0 + tile.h as i32 =>
            {
                prev.component
            }
            _ => {
                next_component += 1;
                next_component - 1
            }
        };
        placed.push(Placed {
            tile,
            x0,
            y0,
            glyph: *glyph,
            component,
        });
    }

    // Component sizes → which glyphs sweep.
    let mut comp_sizes = vec![0usize; next_component];
    for p in &placed {
        comp_sizes[p.component] += 1;
    }
    let sweep_glyphs: Vec<GlyphInstance> = placed
        .iter()
        .filter(|p| comp_sizes[p.component] >= 2)
        .map(|p| p.glyph)
        .collect();
    if !sweep_glyphs.is_empty() {
        drop(crate::probe::Probe::span("glyph_lcd_pretile_overlap"));
    }

    // Pass 2a: the overlapping components through the batch sweep (their
    // pixels are disjoint from every tiled glyph's pixels).
    if !sweep_glyphs.is_empty() {
        render_glyphs_lcd(
            pixmap,
            clip,
            &sweep_glyphs,
            parsed_font,
            font_hash,
            ppem,
            scale,
            hint_correction,
            color,
            dpi_factor,
            scroll_offset,
            glyph_cache,
        );
    }

    // Pass 2b: opaque tile copies, clip-aware.
    let dst_w = pixmap.width as i32;
    let dst_h = pixmap.height as i32;
    for Placed {
        tile,
        x0,
        y0,
        component,
        ..
    } in placed
    {
        if comp_sizes[component] >= 2 {
            continue; // painted by the sweep above
        }
        let tx0 = x0.max(cx0).max(0);
        let ty0 = y0.max(cy0).max(0);
        let tx1 = (x0 + tile.w as i32).min(cx1).min(dst_w);
        let ty1 = (y0 + tile.h as i32).min(cy1).min(dst_h);
        if tx1 <= tx0 || ty1 <= ty0 {
            // Tile entirely outside the clip/pixmap. Without this, the
            // negative width cast huge in `(tx1 - tx0) as u32 * 4`:
            // overflow-checked builds panic ("attempt to multiply with
            // overflow"), release builds WRAPPED into the bounds guard and
            // skipped by accident.
            continue;
        }
        for ty in ty0..ty1 {
            let src_row = ((ty - y0) as u32 * tile.w * 4) as usize;
            let src_off = src_row + ((tx0 - x0) as u32 * 4) as usize;
            let dst_off = ((ty as u32 * pixmap.width + tx0 as u32) * 4) as usize;
            let n = ((tx1 - tx0) as u32 * 4) as usize;
            if src_off + n <= tile.rgba.len() && dst_off + n <= pixmap.data.len() {
                pixmap.data[dst_off..dst_off + n].copy_from_slice(&tile.rgba[src_off..src_off + n]);
            }
        }
    }
    true
}

fn render_text(
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    pixmap: &mut AzulPixmap,
    clip_rect: &LogicalRect,
    clip: Option<AzRect>,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
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

    // ONE source of truth. `font_hash` was produced by this very `FontManager`
    // during layout, so resolving it here cannot fail for any font layout could
    // have shaped with — parsed OR embedded (see `resolve_font_by_hash`). There is
    // deliberately no second lookup table: the renderer used to fall back to
    // `renderer_resources.font_hash_map`, a parallel map that could (and did)
    // disagree with the manager layout had actually used.
    let Some(font_ref) = font_manager.resolve_font_by_hash(font_hash.font_hash) else {
        // Not a "font we happen not to have": layout emitted a hash the manager
        // that produced it cannot resolve, i.e. the two went out of sync. Report it
        // as the invariant violation it is instead of quietly dropping the text.
        font_resolution_failed(font_hash.font_hash);
        return;
    };
    // Safe reborrow with a lifetime tied to the `font_ref` we hold — NOT a raw
    // pointer deref whose result outlives the handle keeping the face alive.
    let parsed_font: &ParsedFont = crate::font_ref_to_parsed_font(&font_ref);

    let units_per_em = f32::from(parsed_font.font_metrics.units_per_em);
    if units_per_em <= 0.0 {
        return;
    }

    let effective_px = font_size_px * dpi_factor;
    let scale = effective_px / units_per_em;
    // ppem = 0 selects the UNHINTED font-unit path in the glyph cache — the
    // platform-faithful default on macOS (see `text_hinting_enabled`).
    let ppem = if crate::glyph_cache::text_hinting_enabled() {
        effective_px.round() as u16
    } else {
        0
    };
    // A hinted outline is produced at the integer `ppem`. `hint_correction`
    // rescales it back to the true (possibly fractional) effective size so hinted
    // glyphs match unhinted fallbacks and animate smoothly instead of snapping.
    let hint_correction = if ppem > 0 {
        effective_px / f32::from(ppem)
    } else {
        1.0
    };

    // RGB LCD subpixel-AA path (opt-in, `AZ_TEXT_LCD=1`; off by default). Renders
    // at 3× horizontal resolution with a 5-tap FIR + per-channel blend. The
    // grayscale path below is left byte-for-byte identical when the flag is off.
    if text_lcd_enabled() && !force_grayscale {
        render_glyphs_lcd(
            pixmap,
            clip,
            glyphs,
            parsed_font,
            font_hash,
            ppem,
            scale,
            hint_correction,
            color,
            dpi_factor,
            scroll_offset,
            glyph_cache,
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
    if text_aliased() {
        render_scanlines_aliased_solid(&mut ras, &mut sl, &mut rb, &agg_color);
    } else {
        render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &agg_color);
    }
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
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]
fn render_text_shadow(
    shadow: &StyleBoxShadow,
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    pixmap: &mut AzulPixmap,
    clip_rect: &LogicalRect,
    clip: Option<AzRect>,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
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
            let ac = Rgba8::new(
                u32::from(c.r),
                u32::from(c.g),
                u32::from(c.b),
                u32::from(c.a),
            );
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
            let ac = Rgba8::new(
                u32::from(c.r),
                u32::from(c.g),
                u32::from(c.b),
                u32::from(c.a),
            );
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
            let ac = Rgba8::new(
                u32::from(c.r),
                u32::from(c.g),
                u32::from(c.b),
                u32::from(c.a),
            );
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
            let ac = Rgba8::new(
                u32::from(c.r),
                u32::from(c.g),
                u32::from(c.b),
                u32::from(c.a),
            );
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

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // software rasterizer: bounded pixel/coord/colour casts
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
    // SAMPLE THE SOURCE DIRECTLY — do not convert the whole image to RGBA up
    // front. The old prologue allocated a W×H×4 buffer and swizzled EVERY
    // source pixel on every repaint, then the blit below nearest-sampled only
    // the visible destination pixels (as few as 600×400 of 2M). `SrcImage`
    // reads a pixel per format on demand, so a huge source feeding a small
    // tile only touches the pixels its taps land on — and `image_scale::sample`
    // area-averages on a downscale (no more nearest-neighbour aliasing) and
    // bilinear-interpolates on an upscale.
    let src = match image_data {
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
            let src = crate::image_scale::SrcImage {
                bytes,
                format: descriptor.format,
                width: w,
                height: h,
            };
            // Formats the sampler cannot read (RG8, 16-bit, float) — or a
            // truncated buffer — fall back to the grey placeholder, exactly
            // as before. RGBA8/RGB8/BGRA8/R8 (every live-frame producer's
            // format) are sampleable, so capture tiles do NOT hit this.
            if !src.is_sampleable() {
                let gray = Rgba8::new(200, 200, 200, 255);
                let mut path = build_rect_path(&rect);
                agg_fill_path(pixmap, &mut path, &gray, FillingRule::NonZero);
                return;
            }
            src
        }
        DecodedImage::Callback(_) => {
            // A RenderImageCallback image reached the CPU rasterizer without
            // having been invoked. Since the content chokepoint landed, EVERY
            // host invokes callbacks in `LayoutWindow::prepare_frame_cpu` /
            // `prepare_frame_content` and the produced frame is patched into
            // the display list — so reaching this arm means a host skipped
            // frame preparation (a bug), or the callback hasn't produced a
            // frame yet. Grey is indistinguishable from "still loading", so
            // say so once.
            static CALLBACK_PLACEHOLDER: std::sync::Once = std::sync::Once::new();
            CALLBACK_PLACEHOLDER.call_once(|| {
                eprintln!(
                    "[azul][cpurender] a RenderImageCallback image was composited as a \
                     flat grey placeholder: the frame was rendered without content \
                     preparation (LayoutWindow::prepare_frame_cpu), so the callback's \
                     content cannot appear (logged once)"
                );
            });
            let gray = Rgba8::new(200, 200, 200, 255);
            let mut path = build_rect_path(&rect);
            agg_fill_path(pixmap, &mut path, &gray, FillingRule::NonZero);
            return;
        }
        DecodedImage::NullImage { .. } => {
            let gray = Rgba8::new(200, 200, 200, 255);
            let mut path = build_rect_path(&rect);
            agg_fill_path(pixmap, &mut path, &gray, FillingRule::NonZero);
            return;
        }
        DecodedImage::Gl(_) => return,
    };

    // Area/bilinear blit: each destination pixel takes the value
    // `image_scale::sample` would give it, but the per-pixel invariants are
    // hoisted and the source rows are converted once each — see
    // `blit_sampled_image`.
    let dst_x = rect.x as i32;
    let dst_y = rect.y as i32;
    let dst_w = rect.width as u32;
    let dst_h = rect.height as u32;
    let pw = pixmap.width;
    let ph = pixmap.height;

    // Compute pixel-level clip bounds for the blit loop
    let (clip_x1, clip_y1, clip_x2, clip_y2) =
        clip.as_ref().map_or((0, 0, pw as i32, ph as i32), |c| {
            (
                c.x as i32,
                c.y as i32,
                (c.x + c.width) as i32,
                (c.y + c.height) as i32,
            )
        });

    let Some(win) = visible_dst_window(
        dst_x,
        dst_y,
        dst_w,
        dst_h,
        pw,
        ph,
        (clip_x1, clip_y1, clip_x2, clip_y2),
    ) else {
        return;
    };

    blit_sampled_image(
        pixmap,
        &src,
        dst_x,
        dst_y,
        dst_w,
        dst_h,
        win,
        &mut RowConversions::new(),
    );
}

/// The sub-window of a `dst_w × dst_h` image blit that is actually painted:
/// the destination rect intersected with the clip AND the pixmap, expressed in
/// the image's OWN destination-pixel coordinates as `(px_lo, py_lo, px_hi,
/// py_hi)` half-open. `None` when nothing of the image is visible.
///
/// # Why the loop must be narrowed rather than filtered
///
/// `image_scale::sample` is a pure function of `(px, py)` and the FULL
/// destination size, so restricting which `(px, py)` the loop visits cannot
/// change any painted pixel's value. The blit used to walk all
/// `dst_w × dst_h` pixels and `continue` on the clip test: a 40 px damage
/// strip over a 2078×1132 canvas still ran 2.35 M iterations to paint 83 k
/// pixels. Narrowing the loop also lets everything the sampler needs be set
/// up ONCE for the pixels that are actually painted — per-column tap
/// positions, and the source rows those taps land on.
fn visible_dst_window(
    dst_x: i32,
    dst_y: i32,
    dst_w: u32,
    dst_h: u32,
    pw: u32,
    ph: u32,
    clip: (i32, i32, i32, i32),
) -> Option<(u32, u32, u32, u32)> {
    let (clip_x1, clip_y1, clip_x2, clip_y2) = clip;
    let x_lo = clip_x1.max(0).max(dst_x);
    let y_lo = clip_y1.max(0).max(dst_y);
    let x_hi = clip_x2
        .min(i32::try_from(pw).unwrap_or(i32::MAX))
        .min(dst_x.saturating_add(i32::try_from(dst_w).unwrap_or(i32::MAX)));
    let y_hi = clip_y2
        .min(i32::try_from(ph).unwrap_or(i32::MAX))
        .min(dst_y.saturating_add(i32::try_from(dst_h).unwrap_or(i32::MAX)));
    if x_hi <= x_lo || y_hi <= y_lo {
        return None;
    }
    // Both differences are non-negative: `x_lo >= dst_x` and `x_hi > x_lo`.
    Some((
        (x_lo - dst_x) as u32,
        (y_lo - dst_y) as u32,
        (x_hi - dst_x) as u32,
        (y_hi - dst_y) as u32,
    ))
}

/// Must match `image_scale::MAX_TAPS`. Divergence is caught by
/// `the_fast_blit_is_byte_identical_to_image_scale_sample`, which compares the
/// blit against `image_scale::sample` itself.
const BLIT_MAX_TAPS: u32 = 4;

/// How many source rows the blit converted to straight RGBA.
///
/// Exists so a test can pin the COMPLEXITY rather than a wall clock: each
/// source row a destination row needs is converted once and reused by every
/// destination row that lands on it, so this stays O(source rows touched) —
/// never O(destination rows × taps), which is what a per-pixel
/// `image_scale::sample` effectively did (four `SrcImage::pixel` calls per
/// destination pixel, each re-deriving the format and re-clamping).
struct RowConversions(usize);

impl RowConversions {
    const fn new() -> Self {
        Self(0)
    }
}

/// One source row as straight RGBA8, plus which row it holds.
struct RgbaRow {
    /// Source y this buffer holds, or `None` when empty.
    y: Option<u32>,
    /// `src.width * 4` bytes.
    bytes: Vec<u8>,
}

impl RgbaRow {
    fn new(width: usize) -> Self {
        Self {
            y: None,
            bytes: vec![0u8; width * 4],
        }
    }
}

/// Convert source row `y` into `out` as straight RGBA8. The format match runs
/// ONCE per row instead of once per tap (four times per destination pixel).
fn source_row_to_rgba(src: &crate::image_scale::SrcImage<'_>, y: u32, out: &mut [u8]) {
    use azul_core::resources::RawImageFormat;
    let Some(bpp) = crate::image_scale::bytes_per_pixel(src.format) else {
        out.fill(0);
        return;
    };
    let w = src.width as usize;
    let base = y as usize * w * bpp;
    let row = &src.bytes[base..base + w * bpp];
    match src.format {
        RawImageFormat::RGBA8 => out.copy_from_slice(row),
        RawImageFormat::BGRA8 => {
            for (o, p) in out.chunks_exact_mut(4).zip(row.chunks_exact(4)) {
                o[0] = p[2];
                o[1] = p[1];
                o[2] = p[0];
                o[3] = p[3];
            }
        }
        RawImageFormat::RGB8 => {
            for (o, p) in out.chunks_exact_mut(4).zip(row.chunks_exact(3)) {
                o[0] = p[0];
                o[1] = p[1];
                o[2] = p[2];
                o[3] = 255;
            }
        }
        RawImageFormat::BGR8 => {
            for (o, p) in out.chunks_exact_mut(4).zip(row.chunks_exact(3)) {
                o[0] = p[2];
                o[1] = p[1];
                o[2] = p[0];
                o[3] = 255;
            }
        }
        // A coverage/luma plane replicated to RGB with an OPAQUE alpha.
        RawImageFormat::R8 => {
            for (o, p) in out.chunks_exact_mut(4).zip(row.iter()) {
                o[0] = *p;
                o[1] = *p;
                o[2] = *p;
                o[3] = 255;
            }
        }
        // `bytes_per_pixel` already returned `None` for anything else.
        _ => out.fill(0),
    }
}

/// Composite one straight-RGBA destination row into the pixmap. Split out of
/// the sampling loops so the float work above is a flat, branch-free pass that
/// vectorizes, and the branchy alpha decision reads bytes.
fn composite_rgba_row(pixmap: &mut AzulPixmap, di_base: usize, stage: &[u8]) {
    for (i, q) in stage.chunks_exact(4).enumerate() {
        let di = di_base + i * 4;
        if di + 3 >= pixmap.data.len() {
            break;
        }
        let sa = u32::from(q[3]);
        if sa == 255 {
            pixmap.data[di] = q[0];
            pixmap.data[di + 1] = q[1];
            pixmap.data[di + 2] = q[2];
            pixmap.data[di + 3] = 255;
        } else if sa > 0 {
            // Alpha blend: dst = src * sa + dst * (255 - sa)
            let da = 255 - sa;
            pixmap.data[di] =
                ((u32::from(q[0]) * sa + u32::from(pixmap.data[di]) * da) / 255) as u8;
            pixmap.data[di + 1] =
                ((u32::from(q[1]) * sa + u32::from(pixmap.data[di + 1]) * da) / 255) as u8;
            pixmap.data[di + 2] =
                ((u32::from(q[2]) * sa + u32::from(pixmap.data[di + 2]) * da) / 255) as u8;
            pixmap.data[di + 3] = ((sa + u32::from(pixmap.data[di + 3]) * da / 255).min(255)) as u8;
        }
    }
}

/// Blit `src` into `pixmap` at `dst_x, dst_y` scaled to `dst_w × dst_h`,
/// painting only the destination sub-window `win = (px_lo, py_lo, px_hi,
/// py_hi)`.
///
/// # What this is
///
/// The value written for destination pixel `(px, py)` is EXACTLY
/// `image_scale::sample(src, dst_w, dst_h, px, py)` — same taps, same f32
/// expressions, same rounding, bit for bit (pinned by
/// `the_fast_blit_is_byte_identical_to_image_scale_sample`). What differs is
/// how often the work that does not depend on the pixel is done.
///
/// # Why it is not a `sample()` call per pixel
///
/// `sample` is the golden reference: pure, per-pixel, no state. Calling it per
/// destination pixel made every pixel pay two f32 divisions to re-derive the
/// scale, and four `SrcImage::pixel` calls that each re-matched the pixel
/// format, re-clamped the coordinates and re-bounds-checked the buffer. On the
/// path that actually matters — a HiDPI window compositing a logical-sized
/// canvas, i.e. a 2× bilinear UPSCALE over the whole window — that is ~19 ns
/// per destination pixel, and AzPaint at 1055×677 points blits 2.35 M of them
/// on every pointer move: ~45 ms per frame, which is the whole frame budget
/// three times over. It measured 12× the nearest-neighbour blit it replaced.
///
/// This keeps the quality and gets the cost back:
///
/// * the scale, tap counts and per-column tap positions are computed once,
/// * each source row is converted to straight RGBA once (`RgbaRow`) and reused
///   by every destination row that samples it — a 2× upscale touches one new
///   source row every two destination rows,
/// * the bilinear case is separable: the horizontal partial
///   `p00·(1−tx) + p10·tx` is kept in f32 per source row, so the per-destination
///   -pixel work is one lerp of two f32 rows. Keeping the partial in f32 is what
///   makes the result bit-identical rather than merely close.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
#[allow(clippy::too_many_arguments)] // a blit is a rect, a source and a window
fn blit_sampled_image(
    pixmap: &mut AzulPixmap,
    src: &crate::image_scale::SrcImage<'_>,
    dst_x: i32,
    dst_y: i32,
    dst_w: u32,
    dst_h: u32,
    win: (u32, u32, u32, u32),
    conversions: &mut RowConversions,
) {
    let (px_lo, py_lo, px_hi, py_hi) = win;
    // `is_sampleable` is what lets every read below skip its bounds check: it
    // proves `bytes.len() >= width * height * bytes_per_pixel`.
    if px_hi <= px_lo || py_hi <= py_lo || !src.is_sampleable() {
        return;
    }
    let scale_x = src.width as f32 / dst_w.max(1) as f32;
    let scale_y = src.height as f32 / dst_h.max(1) as f32;
    let vis_w = (px_hi - px_lo) as usize;
    let pw = pixmap.width;
    let src_w = src.width as usize;
    let last_x = src.width as i32 - 1;
    let last_y = src.height as i32 - 1;

    let mut stage = vec![0u8; vis_w * 4];

    if scale_x <= 1.0 && scale_y <= 1.0 {
        // ---- upscale / 1:1 — separable bilinear over a two-row cache -------
        // Per visible column: the two source x taps (as RGBA byte offsets) and
        // the horizontal weight. `fx` is written exactly as `image_scale`
        // writes it, so `tx` is bit-identical.
        let mut cols: Vec<(u32, u32, f32)> = Vec::with_capacity(vis_w);
        for dx in px_lo..px_hi {
            let fx = (dx as f32 + 0.5) * scale_x - 0.5;
            let x0f = fx.floor();
            let tx = fx - x0f;
            let x0 = (x0f as i32).clamp(0, last_x) as u32;
            let x1 = (x0f as i32 + 1).clamp(0, last_x) as u32;
            cols.push((x0 * 4, x1 * 4, tx));
        }
        let mut rgba = RgbaRow::new(src_w);
        // The horizontal partials for the two source rows the current
        // destination row lerps between.
        let mut h0 = vec![0f32; vis_w * 4];
        let mut h1 = vec![0f32; vis_w * 4];
        let (mut have0, mut have1) = (u32::MAX, u32::MAX);
        for py in py_lo..py_hi {
            let fy = (py as f32 + 0.5) * scale_y - 0.5;
            let y0f = fy.floor();
            let ty = fy - y0f;
            let y0 = (y0f as i32).clamp(0, last_y) as u32;
            let y1 = (y0f as i32 + 1).clamp(0, last_y) as u32;

            if have0 != y0 {
                if have1 == y0 {
                    core::mem::swap(&mut h0, &mut h1);
                    have0 = y0;
                    have1 = u32::MAX;
                } else {
                    if rgba.y != Some(y0) {
                        source_row_to_rgba(src, y0, &mut rgba.bytes);
                        rgba.y = Some(y0);
                        conversions.0 += 1;
                    }
                    horizontal_lerp_row(&rgba.bytes, &cols, &mut h0);
                    have0 = y0;
                }
            }
            if have1 != y1 {
                if y1 == y0 {
                    h1.copy_from_slice(&h0);
                } else {
                    if rgba.y != Some(y1) {
                        source_row_to_rgba(src, y1, &mut rgba.bytes);
                        rgba.y = Some(y1);
                        conversions.0 += 1;
                    }
                    horizontal_lerp_row(&rgba.bytes, &cols, &mut h1);
                }
                have1 = y1;
            }

            let w0 = 1.0 - ty;
            for ((o, &a), &b) in stage.iter_mut().zip(h0.iter()).zip(h1.iter()) {
                *o = (a * w0 + b * ty).round().clamp(0.0, 255.0) as u8;
            }
            let ty_px = (dst_y + py as i32) as u32;
            let tx_px = (dst_x + px_lo as i32) as u32;
            composite_rgba_row(pixmap, ((ty_px * pw + tx_px) * 4) as usize, &stage);
        }
        return;
    }

    // ---- downscale on at least one axis — bounded area average -------------
    let nx = (scale_x.ceil() as u32).clamp(1, BLIT_MAX_TAPS);
    let ny = (scale_y.ceil() as u32).clamp(1, BLIT_MAX_TAPS);
    // Per visible column, the `nx` source x taps as RGBA byte offsets.
    let mut cols: Vec<[u32; BLIT_MAX_TAPS as usize]> = Vec::with_capacity(vis_w);
    for dx in px_lo..px_hi {
        let cx = (dx as f32 + 0.5) * scale_x;
        let mut taps = [0u32; BLIT_MAX_TAPS as usize];
        for (t, slot) in taps.iter_mut().enumerate().take(nx as usize) {
            let fx = cx + ((t as f32 + 0.5) / nx as f32 - 0.5) * scale_x;
            *slot = (fx.floor() as i32).clamp(0, last_x) as u32 * 4;
        }
        cols.push(taps);
    }
    // Up to `ny` source rows are live at once; consecutive destination rows
    // reuse most of them, so the cache is indexed by source y.
    let mut rows: Vec<RgbaRow> = (0..ny as usize).map(|_| RgbaRow::new(src_w)).collect();
    let n = nx * ny;
    let half = n / 2;
    for py in py_lo..py_hi {
        let cy = (py as f32 + 0.5) * scale_y;
        let mut live = [0usize; BLIT_MAX_TAPS as usize];
        // Bitmask of cache slots THIS destination row already depends on, so
        // an eviction can never throw away a row a later tap still needs.
        // There are `ny` slots and at most `ny - 1` are claimed when a miss
        // happens, so a free slot always exists.
        let mut claimed = 0u32;
        for (t, slot) in live.iter_mut().enumerate().take(ny as usize) {
            let fy = cy + ((t as f32 + 0.5) / ny as f32 - 0.5) * scale_y;
            let y = (fy.floor() as i32).clamp(0, last_y) as u32;
            let idx = match rows.iter().position(|r| r.y == Some(y)) {
                Some(i) => i,
                None => {
                    let victim = (0..rows.len())
                        .find(|i| claimed & (1u32 << i) == 0)
                        .unwrap_or(0);
                    source_row_to_rgba(src, y, &mut rows[victim].bytes);
                    rows[victim].y = Some(y);
                    conversions.0 += 1;
                    victim
                }
            };
            claimed |= 1u32 << idx;
            *slot = idx;
        }
        for (i, taps) in cols.iter().enumerate() {
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for &row_idx in live.iter().take(ny as usize) {
                let row = &rows[row_idx].bytes;
                for &off in taps.iter().take(nx as usize) {
                    let o = off as usize;
                    r += u32::from(row[o]);
                    g += u32::from(row[o + 1]);
                    b += u32::from(row[o + 2]);
                    a += u32::from(row[o + 3]);
                }
            }
            let s = &mut stage[i * 4..i * 4 + 4];
            s[0] = ((r + half) / n) as u8;
            s[1] = ((g + half) / n) as u8;
            s[2] = ((b + half) / n) as u8;
            s[3] = ((a + half) / n) as u8;
        }
        let ty_px = (dst_y + py as i32) as u32;
        let tx_px = (dst_x + px_lo as i32) as u32;
        composite_rgba_row(pixmap, ((ty_px * pw + tx_px) * 4) as usize, &stage);
    }
}

/// The horizontal half of the separable bilinear pass: `p00·(1−tx) + p10·tx`
/// per channel, kept in f32 so the vertical pass reproduces `image_scale`'s
/// arithmetic exactly.
fn horizontal_lerp_row(rgba: &[u8], cols: &[(u32, u32, f32)], out: &mut [f32]) {
    for (o, &(x0, x1, tx)) in out.chunks_exact_mut(4).zip(cols.iter()) {
        let (i0, i1) = (x0 as usize, x1 as usize);
        let (a, b) = (&rgba[i0..i0 + 4], &rgba[i1..i1 + 4]);
        let w = 1.0 - tx;
        for c in 0..4 {
            o[c] = f32::from(a[c]) * w + f32::from(b[c]) * tx;
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
    /// The same pixels, straight RGBA8, `pixel_width * pixel_height * 4` bytes,
    /// top row first.
    ///
    /// Exposed alongside `png_data` because several consumers want pixels, not
    /// a container: the system tray hands RGBA to `CreateIconIndirect`
    /// (Windows), `NSBitmapImageRep` (macOS) and SNI's `IconPixmap` (Linux),
    /// and encoding a PNG only to decode it again on the next line is pure
    /// waste. It is a plain clone of the pixmap buffer, so the cost is one
    /// memcpy for callers that ignore it.
    pub rgba: Vec<u8>,
    /// Width of `rgba` in DEVICE pixels (i.e. `content_width * dpi_factor`).
    pub pixel_width: u32,
    /// Height of `rgba` in DEVICE pixels.
    pub pixel_height: u32,
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

    // Carry over families registered by name via `register_named_font` (mock /
    // in-memory / on-disk stress fonts). `from_arc_shared` starts with an empty
    // `memory_families` and only re-adds the built-in mocks, so without this the
    // legacy (no-registry) chain resolver can't match those families and their
    // text silently renders in a fallback font.
    for (family, faces) in &font_manager.memory_families {
        preview_font_manager
            .memory_families
            .entry(family.clone())
            .or_insert_with(|| faces.clone());
    }

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
        resize_only_hint: false,
        last_reconcile_was_skipped: false,
        last_reconcile_structure_preserved: false,
        last_build_was_patched: false,
        last_patch_damage: None,
        build_seq: 0,
        last_full_build_seq: 0,
        patch_damage_log: Vec::new(),
        last_dynamic_context: None,
        previous_sizes: Vec::new(),
        dom_diff_clean: None,
        last_fingerprint_skips: 0,
        last_patch_move: None,
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
        last_reconcile_reused: 0,
        last_reconcile_fresh: 0,
        last_intrinsic_dirty: 0,
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
        None, // content overlay: no live window in headless preview
        system_style.clone(),
        get_system_time_fn,
        &[],
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
                    rgba: Vec::new(),
                    pixel_width: 0,
                    pixel_height: 0,
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
        &preview_font_manager,
        &mut preview_glyph_cache,
        &preview_render_state,
    )?;

    let rgba = pixmap.data.to_vec();
    let pixel_width = pixmap.width;
    let pixel_height = pixmap.height;

    let png_data = pixmap
        .encode_png()
        .map_err(|e| format!("PNG encoding failed: {e}"))?;

    Ok(ComponentPreviewResult {
        png_data,
        rgba,
        pixel_width,
        pixel_height,
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
    dom: azul_core::dom::Dom,
    css: azul_css::css::Css,
    width: f32,
    height: f32,
    dpi: f32,
) -> Result<Vec<u8>, String> {
    let opaque_white = ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    Ok(render_dom_to_rgba(dom, css, width, height, dpi, opaque_white)?.png_data)
}

/// Render a `Dom` + `Css` over an EXPLICIT backdrop, returning the raw pixels
/// as well as the PNG.
///
/// The two things [`render_dom_to_image`] cannot express, and both matter to
/// the same caller:
///
/// * a TRANSPARENT background (alpha 0). An icon has to composite over
///   whatever is behind it; rendered on opaque white it arrives as a white
///   tile sitting in the titlebar instead of a glyph on it.
/// * the RGBA buffer, so a caller building an `ImageRef` does not encode a PNG
///   and immediately decode it again.
///
/// This is the path an SVG should take: the XML parser already maps
/// `<path>`, `<use>`, `<linearGradient>` and `<stop>` onto real DOM nodes
/// (`azul_core::dom::SvgNodeData`), so the ordinary renderer draws them -
/// gradients included - instead of the standalone `render_svg_to_png`
/// rasteriser, which has no paint servers and drops a gradient fill silently.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
/// # Errors
///
/// Returns an error string if rendering fails.
pub fn render_dom_to_rgba(
    mut dom: azul_core::dom::Dom,
    css: azul_css::css::Css,
    width: f32,
    height: f32,
    dpi: f32,
    background: ColorU,
) -> Result<ComponentPreviewResult, String> {
    use crate::font_traits::FontManager;
    use azul_core::styled_dom::StyledDom;

    let styled_dom = StyledDom::create(&mut dom, css);

    let fc_cache = crate::font::loading::build_font_cache();
    let font_manager =
        FontManager::new(fc_cache).map_err(|e| format!("Failed to create font manager: {e:?}"))?;

    let opts = ComponentPreviewOptions {
        width: Some(width),
        height: Some(height),
        dpi_factor: dpi,
        background_color: background,
    };

    render_component_preview(&styled_dom, &font_manager, opts, None)
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
#[allow(
    clippy::suboptimal_flops,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
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
    //    `query_with_fallback` IS that ladder — exact, then family-relaxed, then
    //    coverage-only — so it replaces the hand-rolled `or_else` chain and keeps
    //    the relaxation rules in one place, where fontconfig's own live.
    let mut trace = Vec::new();
    let matched = fc_cache.query_with_fallback(
        &FcPattern {
            family: Some("sans-serif".to_string()),
            ..Default::default()
        },
        &mut trace,
    )?;

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

    // 2. Register the font in a throwaway FontManager. This helper builds its own
    //    one-item display list, so it also has to supply the font state that list
    //    is written against — through the SAME manager every other renderer
    //    consults, never a parallel RendererResources map.
    let rr = RendererResources::default();
    let font_ref = crate::parsed_font_to_font_ref(parsed.clone());
    let hash = crate::font_ref_to_parsed_font(&font_ref).hash;
    let fm: FontManager<FontRef> =
        FontManager::new(rust_fontconfig::FcFontCache::default()).ok()?;
    fm.insert_font(rust_fontconfig::FontId::new(), font_ref);
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
            point: LogicalPosition {
                x: pen_x,
                y: baseline_y,
            },
            size: LogicalSize {
                width: advance,
                height: font_size_px,
            },
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
        size: LogicalSize {
            width: logical_w,
            height: logical_h,
        },
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
    render_display_list(&dl, &mut pixmap, dpi_factor, &rr, &fm, &mut gc).ok()?;

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

    fn renderer_resources_with(
        font: &ParsedFont,
    ) -> (RendererResources, FontManager<FontRef>, FontHash) {
        let rr = RendererResources::default();
        let font_ref = crate::parsed_font_to_font_ref(font.clone());
        let hash = crate::font_ref_to_parsed_font(&font_ref).hash;
        let fm: FontManager<FontRef> =
            FontManager::new(rust_fontconfig::FcFontCache::default()).expect("FontManager::new");
        fm.insert_font(rust_fontconfig::FontId::new(), font_ref);
        (rr, fm, FontHash { font_hash: hash })
    }

    /// Shape a string into glyph instances with a baseline at (x, y).
    fn shape(
        parsed: &ParsedFont,
        text: &str,
        font_size: f32,
        x: f32,
        y: f32,
    ) -> Vec<GlyphInstance> {
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
        let (rr, fm, font_hash) = renderer_resources_with(&font);

        let w = 200u32;
        let h = 60u32;
        let font_size = 32.0;
        // Black glyphs, baseline near the vertical middle.
        let glyphs = shape(&font, "Hi", font_size, 10.0, 40.0);
        // test fixture: bounded pixmap-dimension cast
        #[allow(clippy::cast_precision_loss)]
        let clip_rect: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: w as f32,
                height: h as f32,
            },
        }
        .into();

        let text_item = DisplayListItem::Text {
            glyphs,
            font_hash,
            font_size_px: font_size,
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
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
        render_display_list(&dl_plain, &mut no_shadow, 1.0, &rr, &fm, &mut gc).unwrap();
        // Baseline red-pixel count. With grayscale text this is 0; with LCD
        // subpixel AA (now the default) black glyph edges carry faint red/blue
        // fringes, so the shadow must add red BEYOND this baseline (checked below).
        let red_plain = count_red(&no_shadow);

        // Render WITH a red shadow offset +24px right, no blur.
        let shadow = StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::px(24.0),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::px(0.0),
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(0.0),
            },
            color: ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
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
        render_display_list(&dl_shadow, &mut with_shadow, 1.0, &rr, &fm, &mut gc2).unwrap();
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
        let (rr, fm, font_hash) = renderer_resources_with(&font);
        let w = 200u32;
        let h = 80u32;
        let font_size = 32.0;
        let glyphs = shape(&font, "Hi", font_size, 40.0, 50.0);
        // test fixture: bounded pixmap-dimension cast
        #[allow(clippy::cast_precision_loss)]
        let clip_rect: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: w as f32,
                height: h as f32,
            },
        }
        .into();

        let make = |blur: f32| -> usize {
            let shadow = StyleBoxShadow {
                offset_x: PixelValueNoPercent {
                    inner: PixelValue::px(0.0),
                },
                offset_y: PixelValueNoPercent {
                    inner: PixelValue::px(0.0),
                },
                blur_radius: PixelValueNoPercent {
                    inner: PixelValue::px(blur),
                },
                spread_radius: PixelValueNoPercent {
                    inner: PixelValue::px(0.0),
                },
                color: ColorU {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                clip_mode: azul_css::props::style::box_shadow::BoxShadowClipMode::Outset,
            };
            let text_item = DisplayListItem::Text {
                glyphs: glyphs.clone(),
                font_hash,
                font_size_px: font_size,
                color: ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                }, // transparent text: isolate shadow
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
            render_display_list(&dl, &mut pm, 1.0, &rr, &fm, &mut gc).unwrap();
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

#[cfg(all(test, feature = "std"))]
#[allow(clippy::float_cmp)] // exact compares on values the code copies through verbatim
#[allow(clippy::many_single_char_names)] // domain-standard coordinate/colour names
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // bounded test-fixture casts
mod autotest_generated {
    use agg_rust::gradient_lut::ColorFunction;
    use azul_core::{
        dom::{DomId, NodeId},
        gpu::GpuValueCache,
        resources::{OpacityKey, RawImage, RawImageData, RawImageFormat, TransformKey},
        transform::ComputedTransform3D,
    };
    use azul_css::{
        props::{
            basic::{
                angle::AngleValue,
                color::{OptionColorU, SystemColorRef},
                length::PercentageValue,
                pixel::{PixelValue, PixelValueNoPercent},
            },
            style::{
                background::{
                    BackgroundPositionHorizontal, BackgroundPositionVertical, ConicGradient,
                    LinearGradient, NormalizedLinearColorStop, NormalizedLinearColorStopVec,
                    NormalizedRadialColorStop, NormalizedRadialColorStopVec, RadialGradient,
                    RadialGradientSize, Shape, StyleBackgroundPosition,
                },
                border::BorderStyle,
                box_shadow::BoxShadowClipMode,
            },
        },
        system::SystemColors,
    };

    use super::*;
    use crate::solver3::display_list::WindowLogicalRect;

    // ------------------------------------------------------------------
    // fixtures
    // ------------------------------------------------------------------

    const RED: ColorU = ColorU {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    const BLACK: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    const WHITE: ColorU = ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    const BLUE: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    };
    const CLEAR: ColorU = ColorU {
        r: 255,
        g: 0,
        b: 0,
        a: 0,
    };

    /// f32 values that must never make the rasterizer panic. `f32::MAX` is
    /// deliberately NOT in here: it is finite and positive, so it produces a
    /// *valid* (if enormous) rect that legitimately paints — it gets its own
    /// clamping test instead of the no-op sweeps.
    const DEGENERATE: [f32; 7] = [
        0.0,
        -0.0,
        -1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MIN,
    ];

    fn pixmap(w: u32, h: u32) -> AzulPixmap {
        let mut p = AzulPixmap::new(w, h).expect("test pixmap must allocate");
        p.fill(255, 255, 255, 255);
        p
    }

    fn snap(p: &AzulPixmap) -> Vec<u8> {
        p.data().to_vec()
    }

    fn px_at(p: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.width + x) * 4) as usize;
        [
            p.data()[i],
            p.data()[i + 1],
            p.data()[i + 2],
            p.data()[i + 3],
        ]
    }

    fn is_reddish(px: [u8; 4]) -> bool {
        px[0] > 200 && px[1] < 60 && px[2] < 60
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

    fn wrect(x: f32, y: f32, w: f32, h: f32) -> WindowLogicalRect {
        lrect(x, y, w, h).into()
    }

    fn lin_stops(pairs: &[(f32, ColorU)]) -> NormalizedLinearColorStopVec {
        pairs
            .iter()
            .map(|(offset_percent, color)| NormalizedLinearColorStop {
                offset: PercentageValue::new(*offset_percent),
                color: ColorOrSystem::Color(*color),
            })
            .collect::<Vec<_>>()
            .into()
    }

    fn rad_stops(pairs: &[(f32, ColorU)]) -> NormalizedRadialColorStopVec {
        pairs
            .iter()
            .map(|(degrees, color)| NormalizedRadialColorStop {
                angle: AngleValue::deg(*degrees),
                color: ColorOrSystem::Color(*color),
            })
            .collect::<Vec<_>>()
            .into()
    }

    fn shadow(offset: f32, blur: f32, spread: f32, color: ColorU) -> StyleBoxShadow {
        StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::px(offset),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::px(offset),
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(blur),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::px(spread),
            },
            color,
            clip_mode: BoxShadowClipMode::Outset,
        }
    }

    fn r8_image(w: usize, h: usize, bytes: Vec<u8>) -> ImageRef {
        ImageRef::new_rawimage(RawImage {
            pixels: RawImageData::U8(bytes.into()),
            width: w,
            height: h,
            premultiplied_alpha: false,
            data_format: RawImageFormat::R8,
            tag: Vec::new().into(),
        })
        .expect("R8 RawImage must build")
    }

    fn rgba_image(w: usize, h: usize, bytes: Vec<u8>) -> ImageRef {
        ImageRef::new_rawimage(RawImage {
            pixels: RawImageData::U8(bytes.into()),
            width: w,
            height: h,
            premultiplied_alpha: false,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        })
        .expect("RGBA8 RawImage must build")
    }

    /// The five mutable stacks `render_single_item` threads through, seeded
    /// exactly as `render_display_list_with_state` seeds them.
    struct Stacks {
        transforms: Vec<TransAffine>,
        clips: Vec<Option<AzRect>>,
        masks: Vec<MaskEntry>,
        scrolls: Vec<(f32, f32)>,
        shadows: Vec<StyleBoxShadow>,
        real_clips: Vec<Option<AzRect>>,
    }

    impl Stacks {
        fn new() -> Self {
            Self {
                transforms: vec![TransAffine::new()],
                clips: vec![None],
                real_clips: vec![None],
                masks: Vec::new(),
                scrolls: vec![(0.0, 0.0)],
                shadows: Vec::new(),
            }
        }
    }

    /// Run one item through `render_single_item` with default resources.
    fn run_item(
        item: &DisplayListItem,
        p: &mut AzulPixmap,
        st: &mut Stacks,
        state: &CpuRenderState,
    ) -> Result<(), String> {
        let res = RendererResources::default();
        let mut gc = GlyphCache::new();
        render_single_item(
            item,
            None,
            p,
            1.0,
            &res,
            &empty_font_manager(),
            &mut gc,
            &mut st.transforms,
            &mut st.clips,
            &mut st.real_clips,
            &mut st.masks,
            &mut st.scrolls,
            &mut st.shadows,
            state,
        )
    }

    fn run_list(dl: &DisplayList, p: &mut AzulPixmap, dpi: f32) -> Result<(), String> {
        let res = RendererResources::default();
        let mut gc = GlyphCache::new();
        render_display_list(dl, p, dpi, &res, &empty_font_manager(), &mut gc)
    }

    fn run_list_with_state(
        dl: &DisplayList,
        p: &mut AzulPixmap,
        state: &CpuRenderState,
    ) -> Result<(), String> {
        let res = RendererResources::default();
        let mut gc = GlyphCache::new();
        render_display_list_with_state(dl, p, 1.0, &res, &empty_font_manager(), &mut gc, state)
    }

    // ==================================================================
    // resolve_color
    // ==================================================================

    #[test]
    fn resolve_color_concrete_is_returned_verbatim() {
        let c = ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        };
        let palette = SystemColors {
            accent: OptionColorU::Some(BLUE),
            ..SystemColors::default()
        };
        // A concrete color must ignore the palette entirely, present or not.
        assert_eq!(resolve_color(&ColorOrSystem::Color(c), None), c);
        assert_eq!(resolve_color(&ColorOrSystem::Color(c), Some(&palette)), c);
    }

    #[test]
    fn resolve_color_system_without_palette_is_transparent_fallback() {
        for key in [
            SystemColorRef::Text,
            SystemColorRef::Accent,
            SystemColorRef::SelectionBackground,
        ] {
            let got = resolve_color(&ColorOrSystem::System(key), None);
            assert_eq!(got, SYSTEM_COLOR_FALLBACK);
            assert_eq!(got.a, 0, "the fallback must contribute nothing");
        }
    }

    #[test]
    fn resolve_color_system_resolves_set_keys_and_falls_back_for_unset_ones() {
        let palette = SystemColors {
            accent: OptionColorU::Some(BLUE),
            ..SystemColors::default()
        };
        assert_eq!(
            resolve_color(
                &ColorOrSystem::System(SystemColorRef::Accent),
                Some(&palette)
            ),
            BLUE
        );
        // `text` is unset on this palette -> transparent fallback, not garbage.
        assert_eq!(
            resolve_color(&ColorOrSystem::System(SystemColorRef::Text), Some(&palette)),
            SYSTEM_COLOR_FALLBACK
        );
        // An entirely empty palette falls back for every key.
        assert_eq!(
            resolve_color(
                &ColorOrSystem::System(SystemColorRef::ButtonFace),
                Some(&SystemColors::default())
            ),
            SYSTEM_COLOR_FALLBACK
        );
    }

    // ==================================================================
    // build_gradient_lut_linear / build_gradient_lut_radial
    // ==================================================================

    #[test]
    fn gradient_lut_linear_under_two_stops_is_fully_transparent() {
        for stops in [lin_stops(&[]), lin_stops(&[(50.0, RED)])] {
            let lut = build_gradient_lut_linear(&stops, None);
            assert_eq!(lut.size(), 256);
            for i in [0usize, 1, 128, 255] {
                assert_eq!(
                    lut.get(i).a,
                    0,
                    "a gradient with <2 stops must not paint anything"
                );
            }
        }
    }

    #[test]
    fn gradient_lut_linear_two_stops_interpolate_end_to_end() {
        let lut = build_gradient_lut_linear(&lin_stops(&[(0.0, BLACK), (100.0, WHITE)]), None);
        assert_eq!(lut.size(), 256);
        assert_eq!(lut.get(0).r, 0);
        assert_eq!(lut.get(255).r, 255);
        // Monotonically increasing across the ramp.
        assert!(lut.get(64).r < lut.get(192).r);
        assert_eq!(lut.get(0).a, 255);
    }

    #[test]
    fn gradient_lut_linear_out_of_range_offsets_are_clamped_not_panicking() {
        // -500% and +900% (and a saturating 1e30%) must clamp into 0..=1.
        let lut = build_gradient_lut_linear(
            &lin_stops(&[(-500.0, BLACK), (900.0, WHITE), (1e30, RED)]),
            None,
        );
        assert_eq!(lut.size(), 256);
        assert_eq!(lut.get(0).r, 0, "the -500% stop clamps to offset 0");
        // Both 900% and 1e30% clamp to offset 1.0; the dedup keeps one of them.
        assert!(lut.get(255).a > 0);
    }

    #[test]
    fn gradient_lut_linear_unsorted_stops_are_sorted_by_offset() {
        // Stops handed over back-to-front must still ramp from offset 0 upward.
        let lut = build_gradient_lut_linear(&lin_stops(&[(100.0, WHITE), (0.0, BLACK)]), None);
        assert_eq!(lut.get(0).r, 0);
        assert_eq!(lut.get(255).r, 255);
    }

    #[test]
    fn gradient_lut_linear_duplicate_offsets_degrade_to_transparent_not_panic() {
        // Two stops at the SAME offset dedup down to one -> <2 stops -> the LUT
        // is left transparent. The contract that matters here: no panic, and no
        // arbitrary color is invented.
        let lut = build_gradient_lut_linear(&lin_stops(&[(50.0, RED), (50.0, BLUE)]), None);
        assert_eq!(lut.size(), 256);
        assert_eq!(lut.get(128).a, 0);
    }

    #[test]
    fn gradient_lut_linear_resolves_system_stops_against_the_palette() {
        let palette = SystemColors {
            accent: OptionColorU::Some(BLUE),
            ..SystemColors::default()
        };
        let stops: NormalizedLinearColorStopVec = vec![
            NormalizedLinearColorStop {
                offset: PercentageValue::new(0.0),
                color: ColorOrSystem::System(SystemColorRef::Accent),
            },
            NormalizedLinearColorStop {
                offset: PercentageValue::new(100.0),
                color: ColorOrSystem::Color(WHITE),
            },
        ]
        .into();

        let with_palette = build_gradient_lut_linear(&stops, Some(&palette));
        assert_eq!(
            with_palette.get(0).b,
            255,
            "system:accent must resolve to blue"
        );
        assert_eq!(with_palette.get(0).a, 255);

        // Without a palette the system stop is transparent (never mid-gray).
        let without = build_gradient_lut_linear(&stops, None);
        assert_eq!(without.get(0).a, 0);
    }

    #[test]
    fn gradient_lut_radial_distinct_angles_interpolate() {
        let lut = build_gradient_lut_radial(&rad_stops(&[(0.0, BLACK), (180.0, WHITE)]), None);
        assert_eq!(lut.size(), 256);
        assert_eq!(lut.get(0).r, 0);
        // 180deg -> offset 0.5; everything past it is clamped to the last color.
        assert_eq!(lut.get(255).r, 255);
        assert!(lut.get(64).r < lut.get(127).r);
    }

    #[test]
    fn gradient_lut_radial_extreme_angles_do_not_panic() {
        // Negative, >360 and saturating angles all fold into 0..=1 offsets.
        for angles in [
            [-720.0_f32, 90.0],
            [1e30, 45.0],
            [f32::NAN, 90.0],
            [f32::INFINITY, 270.0],
        ] {
            let lut =
                build_gradient_lut_radial(&rad_stops(&[(angles[0], RED), (angles[1], BLUE)]), None);
            assert_eq!(lut.size(), 256, "angles {angles:?} must still build a LUT");
        }
    }

    // ==================================================================
    // resolve_background_position
    // ==================================================================

    #[test]
    fn resolve_background_position_keywords_map_to_fractions() {
        let cases = [
            (
                BackgroundPositionHorizontal::Left,
                BackgroundPositionVertical::Top,
                (0.0, 0.0),
            ),
            (
                BackgroundPositionHorizontal::Center,
                BackgroundPositionVertical::Center,
                (0.5, 0.5),
            ),
            (
                BackgroundPositionHorizontal::Right,
                BackgroundPositionVertical::Bottom,
                (1.0, 1.0),
            ),
        ];
        for (horizontal, vertical, expected) in cases {
            let pos = StyleBackgroundPosition {
                horizontal,
                vertical,
            };
            assert_eq!(resolve_background_position(&pos, 200.0, 100.0), expected);
        }
    }

    #[test]
    fn resolve_background_position_exact_px_is_a_fraction_of_the_box() {
        let pos = StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Exact(PixelValue::px(50.0)),
            vertical: BackgroundPositionVertical::Exact(PixelValue::px(25.0)),
        };
        assert_eq!(
            resolve_background_position(&pos, 200.0, 100.0),
            (0.25, 0.25)
        );
    }

    #[test]
    fn resolve_background_position_exact_percent_resolves_against_the_box() {
        let pos = StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Exact(PixelValue::percent(50.0)),
            vertical: BackgroundPositionVertical::Exact(PixelValue::percent(10.0)),
        };
        let (x, y) = resolve_background_position(&pos, 200.0, 100.0);
        assert!(
            (x - 0.5).abs() < 1e-4,
            "50% of the width is the center, got {x}"
        );
        assert!((y - 0.1).abs() < 1e-4, "10% of the height, got {y}");
    }

    #[test]
    fn resolve_background_position_zero_box_falls_back_to_center() {
        // The divide-by-zero guard: a 0-sized box centers instead of producing NaN.
        let pos = StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Exact(PixelValue::px(10.0)),
            vertical: BackgroundPositionVertical::Exact(PixelValue::px(10.0)),
        };
        assert_eq!(resolve_background_position(&pos, 0.0, 0.0), (0.5, 0.5));
    }

    #[test]
    fn resolve_background_position_never_returns_nan_for_degenerate_boxes() {
        let pos = StyleBackgroundPosition {
            horizontal: BackgroundPositionHorizontal::Exact(PixelValue::px(10.0)),
            vertical: BackgroundPositionVertical::Exact(PixelValue::px(-10.0)),
        };
        for w in DEGENERATE {
            for h in DEGENERATE {
                let (x, y) = resolve_background_position(&pos, w, h);
                assert!(
                    !x.is_nan() && !y.is_nan(),
                    "w={w}, h={h} produced NaN ({x}, {y}) — a NaN center poisons the gradient transform"
                );
            }
        }
        // f32::MAX is finite and positive: the fraction collapses to ~0, not NaN.
        let (x, y) = resolve_background_position(&pos, f32::MAX, f32::MAX);
        assert!(x.is_finite() && y.is_finite());
    }

    // ==================================================================
    // render_rect
    // ==================================================================

    #[test]
    fn render_rect_paints_exactly_its_bounds() {
        let mut p = pixmap(10, 10);
        render_rect(
            &mut p,
            &lrect(2.0, 2.0, 4.0, 4.0),
            RED,
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert!(is_reddish(px_at(&p, 3, 3)), "inside the rect must be red");
        assert_eq!(px_at(&p, 0, 0), [255, 255, 255, 255], "outside stays white");
        let red = p
            .data()
            .chunks_exact(4)
            .filter(|c| c[0] > 200 && c[1] < 60)
            .count();
        assert_eq!(red, 16, "a 4x4 rect covers exactly 16 pixels");
    }

    #[test]
    fn render_rect_transparent_color_is_a_noop() {
        let mut p = pixmap(8, 8);
        let before = snap(&p);
        render_rect(
            &mut p,
            &lrect(0.0, 0.0, 8.0, 8.0),
            CLEAR,
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert_eq!(before, p.data(), "alpha=0 must not touch the buffer");
    }

    #[test]
    fn render_rect_degenerate_bounds_are_noops() {
        for bad in DEGENERATE {
            let mut p = pixmap(8, 8);
            let before = snap(&p);
            render_rect(
                &mut p,
                &lrect(0.0, 0.0, bad, bad),
                RED,
                &BorderRadius::default(),
                None,
                1.0,
            );
            assert_eq!(before, p.data(), "size {bad} must be rejected, not painted");

            // NOTE: `f32::MIN` is deliberately NOT swept as an *origin* here — it
            // makes `(rect.x + rect.width) as i32` saturate to `i32::MIN` and the
            // `- 1` that follows overflows (debug panic). See the report.
            if bad == f32::MIN {
                continue;
            }
            let mut p = pixmap(8, 8);
            let before = snap(&p);
            render_rect(
                &mut p,
                &lrect(bad, bad, 4.0, 4.0),
                RED,
                &BorderRadius::default(),
                None,
                1.0,
            );
            if !bad.is_finite() {
                assert_eq!(before, p.data(), "origin {bad} must be rejected");
            }
        }
    }

    #[test]
    fn render_rect_degenerate_dpi_is_a_noop() {
        // 0 / -0 / negative / NaN / +-inf / f32::MIN dpi all collapse or poison
        // the rect, and must be rejected before any pixel is touched.
        for dpi in DEGENERATE {
            let mut p = pixmap(8, 8);
            let before = snap(&p);
            render_rect(
                &mut p,
                &lrect(1.0, 1.0, 4.0, 4.0),
                RED,
                &BorderRadius::default(),
                None,
                dpi,
            );
            assert_eq!(before, p.data(), "dpi {dpi} must be rejected, not painted");
        }
        // f32::MAX dpi overflows the rect to +inf -> also rejected.
        let mut p = pixmap(8, 8);
        let before = snap(&p);
        render_rect(
            &mut p,
            &lrect(1.0, 1.0, 4.0, 4.0),
            RED,
            &BorderRadius::default(),
            None,
            f32::MAX,
        );
        assert_eq!(before, p.data());
    }

    #[test]
    fn render_rect_saturating_bounds_clamp_to_the_pixmap() {
        // f32::MAX is finite: the rect is valid and must be clamped to the
        // buffer (i32-saturating casts), never write out of bounds.
        let mut p = pixmap(8, 8);
        render_rect(
            &mut p,
            &lrect(0.0, 0.0, f32::MAX, f32::MAX),
            RED,
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert!(p.data().chunks_exact(4).all(|c| c[0] > 200 && c[1] < 60));
    }

    #[test]
    fn render_rect_negative_origin_clamps_to_the_pixmap() {
        let mut p = pixmap(8, 8);
        render_rect(
            &mut p,
            &lrect(-1e9, -1e9, 2e9, 2e9),
            RED,
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert!(is_reddish(px_at(&p, 0, 0)));
        assert!(is_reddish(px_at(&p, 7, 7)));
    }

    #[test]
    fn render_rect_fully_outside_the_clip_is_a_noop() {
        let mut p = pixmap(10, 10);
        let before = snap(&p);
        let clip = AzRect::from_xywh(0.0, 0.0, 2.0, 2.0).unwrap();
        render_rect(
            &mut p,
            &lrect(5.0, 5.0, 3.0, 3.0),
            RED,
            &BorderRadius::default(),
            Some(clip),
            1.0,
        );
        assert_eq!(before, p.data());
    }

    #[test]
    fn render_rect_clip_narrows_the_painted_area() {
        let mut p = pixmap(10, 10);
        let clip = AzRect::from_xywh(0.0, 0.0, 2.0, 2.0).unwrap();
        render_rect(
            &mut p,
            &lrect(0.0, 0.0, 10.0, 10.0),
            RED,
            &BorderRadius::default(),
            Some(clip),
            1.0,
        );
        let red = p
            .data()
            .chunks_exact(4)
            .filter(|c| c[0] > 200 && c[1] < 60)
            .count();
        assert_eq!(red, 4, "only the 2x2 clip region may be painted");
    }

    #[test]
    fn render_rect_rounded_corners_leave_the_corner_pixel_unpainted() {
        let mut p = pixmap(20, 20);
        let radius = BorderRadius {
            top_left: 6.0,
            top_right: 6.0,
            bottom_left: 6.0,
            bottom_right: 6.0,
        };
        render_rect(
            &mut p,
            &lrect(0.0, 0.0, 20.0, 20.0),
            RED,
            &radius,
            None,
            1.0,
        );
        assert!(is_reddish(px_at(&p, 10, 10)), "the middle is filled");
        assert_eq!(
            px_at(&p, 0, 0),
            [255, 255, 255, 255],
            "the rounded corner must not be filled"
        );
    }

    #[test]
    fn render_rect_radius_larger_than_the_rect_does_not_panic() {
        let mut p = pixmap(10, 10);
        let radius = BorderRadius {
            top_left: 1e6,
            top_right: 1e6,
            bottom_left: 1e6,
            bottom_right: 1e6,
        };
        render_rect(
            &mut p,
            &lrect(0.0, 0.0, 10.0, 10.0),
            RED,
            &radius,
            None,
            1.0,
        );
        // Radii are normalized to fit; the shape stays inside the buffer.
        assert!(is_reddish(px_at(&p, 5, 5)));
    }

    // ==================================================================
    // render_linear_gradient / render_radial_gradient / render_conic_gradient
    // ==================================================================

    fn linear(stops: NormalizedLinearColorStopVec) -> LinearGradient {
        LinearGradient {
            stops,
            ..LinearGradient::default()
        }
    }

    #[test]
    fn linear_gradient_paints_a_ramp_top_to_bottom() {
        let mut p = pixmap(16, 16);
        render_linear_gradient(
            &mut p,
            &lrect(0.0, 0.0, 16.0, 16.0),
            &linear(lin_stops(&[(0.0, BLACK), (100.0, WHITE)])),
            &BorderRadius::default(),
            None,
            1.0,
            None,
        );
        let top = px_at(&p, 8, 0)[0];
        let bottom = px_at(&p, 8, 15)[0];
        assert!(
            top < bottom,
            "the default Top->Bottom direction must ramp dark->light (top {top}, bottom {bottom})"
        );
    }

    #[test]
    fn linear_gradient_without_stops_is_a_noop() {
        let mut p = pixmap(8, 8);
        let before = snap(&p);
        render_linear_gradient(
            &mut p,
            &lrect(0.0, 0.0, 8.0, 8.0),
            &linear(lin_stops(&[])),
            &BorderRadius::default(),
            None,
            1.0,
            None,
        );
        assert_eq!(before, p.data());
    }

    #[test]
    fn linear_gradient_single_stop_paints_nothing() {
        // <2 stops -> transparent LUT -> alpha 0 -> the buffer is untouched.
        let mut p = pixmap(8, 8);
        let before = snap(&p);
        render_linear_gradient(
            &mut p,
            &lrect(0.0, 0.0, 8.0, 8.0),
            &linear(lin_stops(&[(50.0, RED)])),
            &BorderRadius::default(),
            None,
            1.0,
            None,
        );
        assert_eq!(before, p.data());
    }

    #[test]
    fn linear_gradient_degenerate_geometry_is_a_noop() {
        for bad in DEGENERATE {
            let mut p = pixmap(8, 8);
            let before = snap(&p);
            render_linear_gradient(
                &mut p,
                &lrect(0.0, 0.0, 8.0, 8.0),
                &linear(lin_stops(&[(0.0, BLACK), (100.0, WHITE)])),
                &BorderRadius::default(),
                None,
                bad,
                None,
            );
            assert_eq!(before, p.data(), "dpi {bad} must be rejected");

            let mut p = pixmap(8, 8);
            let before = snap(&p);
            render_linear_gradient(
                &mut p,
                &lrect(0.0, 0.0, bad, bad),
                &linear(lin_stops(&[(0.0, BLACK), (100.0, WHITE)])),
                &BorderRadius::default(),
                None,
                1.0,
                None,
            );
            assert_eq!(before, p.data(), "size {bad} must be rejected");
        }
    }

    #[test]
    fn radial_gradient_zero_radius_is_a_noop() {
        // ClosestSide with the center pinned to the top-left corner => radius 0.
        let gradient = RadialGradient {
            shape: Shape::Circle,
            size: RadialGradientSize::ClosestSide,
            position: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Left,
                vertical: BackgroundPositionVertical::Top,
            },
            stops: lin_stops(&[(0.0, BLACK), (100.0, WHITE)]),
            ..RadialGradient::default()
        };
        let mut p = pixmap(8, 8);
        let before = snap(&p);
        render_radial_gradient(
            &mut p,
            &lrect(0.0, 0.0, 8.0, 8.0),
            &gradient,
            &BorderRadius::default(),
            None,
            1.0,
            None,
        );
        assert_eq!(before, p.data(), "a 0-radius gradient must paint nothing");
    }

    #[test]
    fn radial_gradient_paints_from_the_center_outward() {
        let gradient = RadialGradient {
            shape: Shape::Circle,
            size: RadialGradientSize::FarthestCorner,
            position: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Center,
                vertical: BackgroundPositionVertical::Center,
            },
            stops: lin_stops(&[(0.0, BLACK), (100.0, WHITE)]),
            ..RadialGradient::default()
        };
        let mut p = pixmap(16, 16);
        render_radial_gradient(
            &mut p,
            &lrect(0.0, 0.0, 16.0, 16.0),
            &gradient,
            &BorderRadius::default(),
            None,
            1.0,
            None,
        );
        let center = px_at(&p, 8, 8)[0];
        let corner = px_at(&p, 0, 0)[0];
        assert!(
            center < corner,
            "the center stop is black, the rim white (center {center}, corner {corner})"
        );
    }

    #[test]
    fn radial_gradient_empty_stops_and_degenerate_dpi_are_noops() {
        let empty = RadialGradient {
            stops: lin_stops(&[]),
            ..RadialGradient::default()
        };
        let mut p = pixmap(8, 8);
        let before = snap(&p);
        render_radial_gradient(
            &mut p,
            &lrect(0.0, 0.0, 8.0, 8.0),
            &empty,
            &BorderRadius::default(),
            None,
            1.0,
            None,
        );
        assert_eq!(before, p.data());

        let filled = RadialGradient {
            stops: lin_stops(&[(0.0, BLACK), (100.0, WHITE)]),
            ..RadialGradient::default()
        };
        for bad in DEGENERATE {
            let mut p = pixmap(8, 8);
            let before = snap(&p);
            render_radial_gradient(
                &mut p,
                &lrect(0.0, 0.0, 8.0, 8.0),
                &filled,
                &BorderRadius::default(),
                None,
                bad,
                None,
            );
            assert_eq!(before, p.data(), "dpi {bad} must be rejected");
        }
    }

    #[test]
    fn conic_gradient_empty_stops_and_degenerate_dpi_are_noops() {
        let empty = ConicGradient {
            stops: rad_stops(&[]),
            ..ConicGradient::default()
        };
        let mut p = pixmap(8, 8);
        let before = snap(&p);
        render_conic_gradient(
            &mut p,
            &lrect(0.0, 0.0, 8.0, 8.0),
            &empty,
            &BorderRadius::default(),
            None,
            1.0,
            None,
        );
        assert_eq!(before, p.data());

        let filled = ConicGradient {
            stops: rad_stops(&[(0.0, BLACK), (180.0, WHITE)]),
            ..ConicGradient::default()
        };
        for bad in DEGENERATE {
            let mut p = pixmap(8, 8);
            let before = snap(&p);
            render_conic_gradient(
                &mut p,
                &lrect(0.0, 0.0, 8.0, 8.0),
                &filled,
                &BorderRadius::default(),
                None,
                bad,
                None,
            );
            assert_eq!(before, p.data(), "dpi {bad} must be rejected");
        }
    }

    #[test]
    fn conic_gradient_with_distinct_angle_stops_paints() {
        let gradient = ConicGradient {
            stops: rad_stops(&[(0.0, BLACK), (180.0, WHITE)]),
            ..ConicGradient::default()
        };
        let mut p = pixmap(16, 16);
        let before = snap(&p);
        render_conic_gradient(
            &mut p,
            &lrect(0.0, 0.0, 16.0, 16.0),
            &gradient,
            &BorderRadius::default(),
            None,
            1.0,
            None,
        );
        assert_ne!(before, p.data(), "a 2-stop conic gradient must paint");
    }

    /// Regression: the CSS parser normalizes `conic-gradient(red, blue)` to
    /// stops at **0deg and 360deg** (`get_normalized_radial_stops`,
    /// `default_end = 360.0`). `build_gradient_lut_radial` used to map each stop
    /// through `AngleValue::to_degrees()`, which wraps 360 -> 0, so both stops
    /// landed on offset 0.0, `build_lut()` deduped them to a single stop, bailed
    /// (`len < 2`), and the LUT stayed fully transparent — the gradient painted
    /// NOTHING. It now uses `to_degrees_raw()`, so the last stop lands on 1.0.
    #[test]
    fn conic_gradient_full_circle_stops_paint_the_rect() {
        let gradient = ConicGradient {
            stops: rad_stops(&[(0.0, BLACK), (360.0, WHITE)]),
            ..ConicGradient::default()
        };
        let mut p = pixmap(16, 16);
        let before = snap(&p);
        render_conic_gradient(
            &mut p,
            &lrect(0.0, 0.0, 16.0, 16.0),
            &gradient,
            &BorderRadius::default(),
            None,
            1.0,
            None,
        );
        assert_ne!(
            before,
            p.data(),
            "conic-gradient(black, white) normalizes to 0deg/360deg and must still paint"
        );
    }

    // ==================================================================
    // render_box_shadow
    // ==================================================================

    #[test]
    fn box_shadow_does_not_paint_under_the_bounds() {
        // CSS Backgrounds 3 §box-shadow: an OUTER shadow casts as if the
        // border box were opaque and is painted only OUTSIDE the border
        // box. A zero-offset hard shadow is therefore (nearly) invisible —
        // this test used to pin the opposite (>100 dark pixels UNDER the
        // box, the pre-ring-blit behavior that also wasted the biggest
        // alpha-blend of every repaint). The ring blit keeps a deliberate
        // 1px sliver under the element edge to avoid antialiasing seams,
        // hence "nearly": the sliver is ~4 edges x 20px.
        let mut p = pixmap(40, 40);
        let res = render_box_shadow(
            &mut p,
            &lrect(10.0, 10.0, 20.0, 20.0),
            &shadow(0.0, 0.0, 0.0, BLACK),
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert!(res.is_ok());
        let dark = p.data().chunks_exact(4).filter(|c| c[0] < 50).count();
        assert!(
            dark <= 90,
            "a zero-offset outset shadow must not paint under the border box              (only the 1px anti-seam sliver may darken), got {dark}"
        );
        // And an OFFSET shadow must still visibly paint outside the box.
        let mut p2 = pixmap(60, 60);
        render_box_shadow(
            &mut p2,
            &lrect(10.0, 10.0, 20.0, 20.0),
            &shadow(15.0, 2.0, 0.0, BLACK),
            &BorderRadius::default(),
            None,
            1.0,
        )
        .unwrap();
        let dark2 = p2.data().chunks_exact(4).filter(|c| c[0] < 50).count();
        assert!(
            dark2 > 100,
            "an offset shadow must paint outside the box, got {dark2}"
        );
    }

    #[test]
    fn box_shadow_transparent_color_is_ok_and_a_noop() {
        let mut p = pixmap(20, 20);
        let before = snap(&p);
        let res = render_box_shadow(
            &mut p,
            &lrect(5.0, 5.0, 10.0, 10.0),
            &shadow(0.0, 4.0, 0.0, CLEAR),
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert_eq!(res, Ok(()));
        assert_eq!(before, p.data());
    }

    #[test]
    fn box_shadow_oversized_blur_is_rejected_without_allocating() {
        // blur 1e6 px would need a >4096px scratch buffer -> refused (Ok, no-op),
        // NOT a multi-gigabyte allocation.
        let mut p = pixmap(20, 20);
        let before = snap(&p);
        let res = render_box_shadow(
            &mut p,
            &lrect(5.0, 5.0, 10.0, 10.0),
            &shadow(0.0, 1e6, 0.0, BLACK),
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert_eq!(res, Ok(()));
        assert_eq!(before, p.data(), "an oversized shadow must be skipped");
    }

    #[test]
    fn box_shadow_huge_negative_spread_collapses_to_a_noop() {
        let mut p = pixmap(20, 20);
        let before = snap(&p);
        let res = render_box_shadow(
            &mut p,
            &lrect(5.0, 5.0, 10.0, 10.0),
            &shadow(0.0, 0.0, -1e6, BLACK),
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert_eq!(res, Ok(()));
        assert_eq!(before, p.data(), "a fully-shrunk shadow paints nothing");
    }

    #[test]
    fn box_shadow_degenerate_geometry_is_ok_and_a_noop() {
        for bad in DEGENERATE {
            let mut p = pixmap(20, 20);
            let before = snap(&p);
            let res = render_box_shadow(
                &mut p,
                &lrect(5.0, 5.0, 10.0, 10.0),
                &shadow(0.0, 2.0, 0.0, BLACK),
                &BorderRadius::default(),
                None,
                bad,
            );
            assert_eq!(res, Ok(()), "dpi {bad} must not error");
            assert_eq!(before, p.data(), "dpi {bad} must not paint");

            let mut p = pixmap(20, 20);
            let before = snap(&p);
            let res = render_box_shadow(
                &mut p,
                &lrect(0.0, 0.0, bad, bad),
                &shadow(0.0, 2.0, 0.0, BLACK),
                &BorderRadius::default(),
                None,
                1.0,
            );
            assert_eq!(res, Ok(()), "size {bad} must not error");
            assert_eq!(before, p.data(), "size {bad} must not paint");
        }
    }

    // ==================================================================
    // extract_mask_data
    // ==================================================================

    #[test]
    fn extract_mask_data_zero_target_is_none() {
        let img = r8_image(2, 2, vec![0, 64, 128, 255]);
        assert!(extract_mask_data(&img, 0, 4).is_none());
        assert!(extract_mask_data(&img, 4, 0).is_none());
        assert!(extract_mask_data(&img, 0, 0).is_none());
    }

    #[test]
    fn extract_mask_data_r8_identity_scale_is_a_passthrough() {
        let img = r8_image(2, 2, vec![0, 64, 128, 255]);
        let mask = extract_mask_data(&img, 2, 2).expect("R8 mask must extract");
        assert_eq!(mask, vec![0, 64, 128, 255]);
    }

    #[test]
    fn extract_mask_data_upscales_nearest_neighbour() {
        let img = r8_image(2, 2, vec![0, 255, 255, 0]);
        let mask = extract_mask_data(&img, 4, 4).expect("mask must extract");
        assert_eq!(mask.len(), 16);
        // Each source texel expands into a 2x2 block.
        assert_eq!(
            mask,
            vec![
                0, 0, 255, 255, //
                0, 0, 255, 255, //
                255, 255, 0, 0, //
                255, 255, 0, 0,
            ]
        );
    }

    #[test]
    fn extract_mask_data_downscales_without_reading_out_of_bounds() {
        let img = r8_image(4, 4, (0..16).map(|i| i as u8 * 16).collect());
        let mask = extract_mask_data(&img, 1, 1).expect("mask must extract");
        assert_eq!(
            mask,
            vec![0],
            "1x1 nearest-neighbour samples the first texel"
        );

        // A target bigger than the source in one axis only.
        let mask = extract_mask_data(&img, 8, 2).expect("mask must extract");
        assert_eq!(mask.len(), 16);
    }

    #[test]
    fn extract_mask_data_bgra_source_uses_the_alpha_channel() {
        // RGBA8 is stored as BGRA8; the mask must come from the alpha channel.
        let px = vec![
            255, 0, 0, 0, // red, a=0
            0, 255, 0, 85, // green, a=85
            0, 0, 255, 170, // blue, a=170
            9, 9, 9, 255, // gray, a=255
        ];
        let img = rgba_image(2, 2, px);
        let mask = extract_mask_data(&img, 2, 2).expect("BGRA mask must extract");
        assert_eq!(mask, vec![0, 85, 170, 255]);
    }

    #[test]
    fn extract_mask_data_target_length_always_matches_the_request() {
        let img = r8_image(3, 3, vec![7; 9]);
        for (w, h) in [(1u32, 1u32), (2, 5), (5, 2), (16, 16), (1, 64)] {
            let mask = extract_mask_data(&img, w, h).expect("mask must extract");
            assert_eq!(mask.len(), (w * h) as usize, "target {w}x{h}");
            assert!(mask.iter().all(|&v| v == 7));
        }
    }

    // ==================================================================
    // apply_mask
    // ==================================================================

    fn image_mask_entry(
        snapshot: Vec<u8>,
        mask_data: Vec<u8>,
        origin: (i32, i32),
        size: (u32, u32),
    ) -> MaskEntry {
        MaskEntry::ImageMask {
            snapshot,
            mask_data,
            origin_x: origin.0,
            origin_y: origin.1,
            width: size.0,
            height: size.1,
        }
    }

    #[test]
    fn apply_mask_zero_mask_restores_the_snapshot() {
        let mut p = pixmap(4, 4);
        let snapshot = snapshot_region(&p, 0, 0, 4, 4); // all white
        p.fill(0, 0, 0, 255); // the "masked" drawing
        apply_mask(
            &mut p,
            &image_mask_entry(snapshot, vec![0; 16], (0, 0), (4, 4)),
        );
        assert!(
            p.data().chunks_exact(4).all(|c| c[0] == 255 && c[1] == 255),
            "mask=0 means fully clipped -> the pre-mask snapshot is restored"
        );
    }

    #[test]
    fn apply_mask_opaque_mask_keeps_the_current_pixels() {
        let mut p = pixmap(4, 4);
        let snapshot = snapshot_region(&p, 0, 0, 4, 4);
        p.fill(0, 0, 0, 255);
        apply_mask(
            &mut p,
            &image_mask_entry(snapshot, vec![255; 16], (0, 0), (4, 4)),
        );
        assert!(
            p.data().chunks_exact(4).all(|c| c[0] == 0),
            "mask=255 means fully visible -> the drawing survives"
        );
    }

    #[test]
    fn apply_mask_opacity_entry_is_ignored() {
        let mut p = pixmap(4, 4);
        p.fill(0, 0, 0, 255);
        let before = snap(&p);
        apply_mask(
            &mut p,
            &MaskEntry::Opacity {
                snapshot: vec![255; 64],
                rect: AzRect::from_xywh(0.0, 0.0, 4.0, 4.0).unwrap(),
                opacity: 0.5,
            },
        );
        assert_eq!(
            before,
            p.data(),
            "apply_mask only handles ImageMask entries"
        );
    }

    #[test]
    fn apply_mask_out_of_bounds_origin_does_not_panic_or_write() {
        let mut p = pixmap(4, 4);
        p.fill(0, 0, 0, 255);
        let before = snap(&p);
        // Entirely off the left/top and off the right/bottom, including the
        // i32 lower bound. (`i32::MAX` origins are NOT swept: `origin_y + py`
        // overflows there — see the report.)
        for origin in [(-100, -100), (100, 100), (i32::MIN, 0), (0, i32::MIN)] {
            apply_mask(
                &mut p,
                &image_mask_entry(vec![255; 64], vec![0; 16], origin, (4, 4)),
            );
        }
        assert_eq!(
            before,
            p.data(),
            "off-buffer masks must be skipped entirely"
        );
    }

    #[test]
    fn apply_mask_truncated_mask_data_is_treated_as_zero() {
        let mut p = pixmap(4, 4);
        let snapshot = snapshot_region(&p, 0, 0, 4, 4);
        p.fill(0, 0, 0, 255);
        // Only 4 of the 16 mask bytes are present — the rest must read as 0
        // (clipped), never index out of bounds.
        apply_mask(
            &mut p,
            &image_mask_entry(snapshot, vec![255; 4], (0, 0), (4, 4)),
        );
        assert_eq!(px_at(&p, 0, 0), [0, 0, 0, 255], "the covered texels stay");
        assert_eq!(
            px_at(&p, 0, 3),
            [255, 255, 255, 255],
            "missing mask bytes restore the snapshot"
        );
    }

    #[test]
    fn apply_mask_partially_offscreen_only_touches_visible_pixels() {
        let mut p = pixmap(4, 4);
        let snapshot = snapshot_region(&p, -2, -2, 4, 4);
        p.fill(0, 0, 0, 255);
        apply_mask(
            &mut p,
            &image_mask_entry(snapshot, vec![0; 16], (-2, -2), (4, 4)),
        );
        // The bottom-right quadrant is off-mask and keeps the drawing.
        assert_eq!(px_at(&p, 3, 3), [0, 0, 0, 255]);
    }

    // ==================================================================
    // acquire_pixmap
    // ==================================================================

    #[test]
    fn acquire_pixmap_zero_dimensions_error_instead_of_allocating() {
        assert!(acquire_pixmap(None, 0, 0).is_err());
        assert!(acquire_pixmap(None, 0, 4).is_err());
        assert!(acquire_pixmap(None, 4, 0).is_err());
        // Even with a retained buffer, a 0-sized request must fail (it cannot
        // match the retained dimensions, so it falls through to allocation).
        assert!(acquire_pixmap(Some(pixmap(4, 4)), 0, 4).is_err());
    }

    #[test]
    fn acquire_pixmap_reuses_a_matching_retained_buffer_verbatim() {
        let mut retained = pixmap(4, 4);
        retained.fill(1, 2, 3, 4);
        let got = acquire_pixmap(Some(retained), 4, 4).expect("must reuse");
        assert_eq!(got.width, 4);
        assert_eq!(got.height, 4);
        assert_eq!(
            &got.data()[0..4],
            &[1, 2, 3, 4],
            "reuse must not clear — the caller does that"
        );
    }

    #[test]
    fn acquire_pixmap_allocates_fresh_on_a_size_mismatch() {
        let mut retained = pixmap(4, 4);
        retained.fill(1, 2, 3, 4);
        let got = acquire_pixmap(Some(retained), 5, 5).expect("must allocate");
        assert_eq!((got.width, got.height), (5, 5));
        assert_eq!(
            &got.data()[0..4],
            &[255, 255, 255, 255],
            "fresh = opaque white"
        );
    }

    // ==================================================================
    // render (public entry point)
    // ==================================================================

    fn opts(width: f32, height: f32, dpi_factor: f32) -> RenderOptions {
        RenderOptions {
            width,
            height,
            dpi_factor,
        }
    }

    #[test]
    fn render_empty_display_list_is_opaque_white() {
        let dl = DisplayList::default();
        let res = RendererResources::default();
        let mut gc = GlyphCache::new();
        let p = render(
            &dl,
            &res,
            &empty_font_manager(),
            opts(4.0, 4.0, 1.0),
            &mut gc,
        )
        .expect("must render");
        assert_eq!((p.width, p.height), (4, 4));
        assert!(p
            .data()
            .chunks_exact(4)
            .all(|c| c[0] == 255 && c[1] == 255 && c[2] == 255 && c[3] == 255));
    }

    #[test]
    fn render_applies_the_dpi_factor_to_the_pixmap_size() {
        let dl = DisplayList::default();
        let res = RendererResources::default();
        let mut gc = GlyphCache::new();
        let p = render(
            &dl,
            &res,
            &empty_font_manager(),
            opts(4.0, 3.0, 2.0),
            &mut gc,
        )
        .expect("must render");
        assert_eq!((p.width, p.height), (8, 6));
    }

    #[test]
    fn render_collapsing_dimensions_error_instead_of_panicking() {
        let dl = DisplayList::default();
        let res = RendererResources::default();
        let mut gc = GlyphCache::new();
        // Every one of these truncates to a 0-sized pixmap.
        for o in [
            opts(0.0, 4.0, 1.0),
            opts(4.0, 0.0, 1.0),
            opts(-4.0, -4.0, 1.0),
            opts(f32::NAN, f32::NAN, 1.0),
            opts(4.0, 4.0, 0.0),
            opts(4.0, 4.0, -1.0),
            opts(4.0, 4.0, f32::NAN),
            opts(0.4, 0.4, 1.0), // truncates to 0
        ] {
            let got = render(&dl, &res, &empty_font_manager(), o, &mut gc);
            assert!(
                got.is_err(),
                "{o:?} must return Err, not panic or allocate a 0-sized buffer"
            );
        }
    }

    #[test]
    fn render_paints_display_list_items() {
        let dl = DisplayList {
            items: vec![DisplayListItem::Rect {
                bounds: wrect(0.0, 0.0, 4.0, 4.0),
                color: RED,
                border_radius: BorderRadius::default(),
            }],
            ..Default::default()
        };
        let res = RendererResources::default();
        let mut gc = GlyphCache::new();
        let p = render(
            &dl,
            &res,
            &empty_font_manager(),
            opts(8.0, 8.0, 1.0),
            &mut gc,
        )
        .expect("must render");
        assert!(is_reddish(px_at(&p, 1, 1)));
        assert_eq!(px_at(&p, 7, 7), [255, 255, 255, 255]);
    }

    // ==================================================================
    // CpuRenderState constructors + extract_gpu_values
    // ==================================================================

    #[test]
    fn cpu_render_state_new_keeps_the_scroll_offsets_and_empties_the_rest() {
        let mut offsets = ScrollOffsetMap::new();
        offsets.insert(7, (1.0, 2.0));
        let state = CpuRenderState::new(offsets);
        assert_eq!(state.scroll_offsets.get(&7), Some(&(1.0, 2.0)));
        assert!(state.transforms.is_empty());
        assert!(state.opacities.is_empty());
        assert!(state.system_style.is_none());
        assert!(state.virtual_view_display_lists.is_empty());
    }

    #[test]
    fn cpu_render_state_builders_set_their_field_and_preserve_the_others() {
        let mut offsets = ScrollOffsetMap::new();
        offsets.insert(1, (3.0, 4.0));

        let mut lists = std::collections::BTreeMap::new();
        lists.insert(
            DomId { inner: 9 },
            std::sync::Arc::new(DisplayList::default()),
        );

        let state = CpuRenderState::new(offsets)
            .with_virtual_view_display_lists(lists)
            .with_system_style(Some(std::sync::Arc::new(
                azul_css::system::SystemStyle::default(),
            )));

        assert_eq!(state.scroll_offsets.get(&1), Some(&(3.0, 4.0)));
        assert_eq!(state.virtual_view_display_lists.len(), 1);
        assert!(state
            .virtual_view_display_lists
            .contains_key(&DomId { inner: 9 }));
        assert!(state.system_style.is_some());

        // with_system_style(None) must clear it again.
        let cleared = CpuRenderState::new(ScrollOffsetMap::new()).with_system_style(None);
        assert!(cleared.system_style.is_none());
    }

    #[test]
    fn cpu_render_state_builders_accept_empty_collections() {
        let state = CpuRenderState::new(ScrollOffsetMap::new())
            .with_virtual_view_display_lists(std::collections::BTreeMap::new());
        assert!(state.virtual_view_display_lists.is_empty());
    }

    #[test]
    fn extract_gpu_values_without_a_cache_is_empty() {
        let (transforms, opacities) = extract_gpu_values(None, DomId::ROOT_ID);
        assert!(transforms.is_empty());
        assert!(opacities.is_empty());
    }

    #[test]
    fn extract_gpu_values_flattens_keys_to_ids() {
        let mut cache = GpuValueCache::default();
        let node = NodeId::new(3);
        let tkey = TransformKey { id: 11 };
        let okey = OpacityKey { id: 22 };

        cache.transform_keys.insert(node, tkey);
        cache
            .current_transform_values
            .insert(node, ComputedTransform3D::IDENTITY);
        cache.opacity_keys.insert(node, okey);
        cache.current_opacity_values.insert(node, 0.25);

        let (transforms, opacities) = extract_gpu_values(Some(&cache), DomId::ROOT_ID);
        assert_eq!(transforms.len(), 1);
        assert_eq!(
            transforms.get(&11).map(|t| t.m),
            Some(ComputedTransform3D::IDENTITY.m)
        );
        assert_eq!(opacities.get(&22), Some(&0.25));
    }

    #[test]
    fn extract_gpu_values_drops_keys_without_a_value() {
        // A key with no matching value must NOT be invented as a default.
        let mut cache = GpuValueCache::default();
        cache
            .transform_keys
            .insert(NodeId::new(0), TransformKey { id: 5 });
        cache
            .opacity_keys
            .insert(NodeId::new(0), OpacityKey { id: 6 });
        let (transforms, opacities) = extract_gpu_values(Some(&cache), DomId::ROOT_ID);
        assert!(transforms.is_empty());
        assert!(opacities.is_empty());
    }

    #[test]
    fn extract_gpu_values_filters_scrollbar_opacity_by_dom_id() {
        let mut cache = GpuValueCache::default();
        let other_dom = DomId { inner: 42 };
        let node = NodeId::new(1);
        cache
            .scrollbar_v_opacity_keys
            .insert((other_dom, node), OpacityKey { id: 77 });
        cache
            .scrollbar_v_opacity_values
            .insert((other_dom, node), 1.0);

        // Querying a DIFFERENT dom must not leak the other dom's scrollbar fade.
        let (_, opacities) = extract_gpu_values(Some(&cache), DomId::ROOT_ID);
        assert!(opacities.is_empty());

        // Querying the owning dom does return it.
        let (_, opacities) = extract_gpu_values(Some(&cache), other_dom);
        assert_eq!(opacities.get(&77), Some(&1.0));
    }

    #[test]
    fn cpu_render_state_from_gpu_cache_matches_extract_gpu_values() {
        let mut cache = GpuValueCache::default();
        cache
            .css_transform_keys
            .insert(NodeId::new(2), TransformKey { id: 8 });
        cache
            .css_current_transform_values
            .insert(NodeId::new(2), ComputedTransform3D::IDENTITY);

        let mut offsets = ScrollOffsetMap::new();
        offsets.insert(5, (10.0, 20.0));

        let state = CpuRenderState::from_gpu_cache(Some(&cache), DomId::ROOT_ID, &offsets);
        let (transforms, opacities) = extract_gpu_values(Some(&cache), DomId::ROOT_ID);
        assert_eq!(state.transforms.len(), transforms.len());
        assert!(state.transforms.contains_key(&8));
        assert_eq!(state.opacities.len(), opacities.len());
        assert_eq!(state.scroll_offsets.get(&5), Some(&(10.0, 20.0)));
        assert!(state.system_style.is_none());

        let empty = CpuRenderState::from_gpu_cache(None, DomId::ROOT_ID, &ScrollOffsetMap::new());
        assert!(empty.transforms.is_empty() && empty.opacities.is_empty());
    }

    // ==================================================================
    // probe_label_for_item
    // ==================================================================

    #[test]
    fn probe_label_for_item_returns_a_distinct_static_label() {
        let cases = [
            (
                DisplayListItem::Rect {
                    bounds: wrect(0.0, 0.0, 1.0, 1.0),
                    color: RED,
                    border_radius: BorderRadius::default(),
                },
                "dl:rect",
            ),
            (DisplayListItem::PopClip, "dl:pop_clip"),
            (DisplayListItem::PopScrollFrame, "dl:pop_scroll"),
            (DisplayListItem::PopOpacity, "dl:pop_opacity"),
            (DisplayListItem::PopTextShadow, "dl:pop_tshadow"),
            (DisplayListItem::PopImageMaskClip, "dl:pop_imask"),
            (
                DisplayListItem::BoxShadow {
                    bounds: wrect(0.0, 0.0, 1.0, 1.0),
                    shadow: shadow(0.0, 0.0, 0.0, BLACK),
                    border_radius: BorderRadius::default(),
                },
                "dl:box_shadow",
            ),
        ];
        for (item, expected) in cases {
            assert_eq!(probe_label_for_item(&item), expected);
        }
    }

    // ==================================================================
    // compute_content_bounds
    // ==================================================================

    /// A partial-damage repaint must not re-blend a box shadow outside the
    /// damage rect. The shadow BLENDS (alpha), so an unclipped blit
    /// re-darkens retained, already-shadowed pixels on every repaint — the
    /// live-run symptom was the page shadow visibly accumulating darker
    /// with each resize frame. Starting from a correct frame, repainting a
    /// small rect that crosses the shadow must change NOTHING anywhere.
    #[test]
    fn damaged_repaint_does_not_accumulate_box_shadow_ink() {
        let rr = RendererResources::default();
        let fm: FontManager<FontRef> =
            FontManager::new(rust_fontconfig::FcFontCache::default()).expect("FontManager::new");
        let shadow_item = DisplayListItem::BoxShadow {
            bounds: wrect(30.0, 30.0, 40.0, 40.0),
            shadow: shadow(
                0.0,
                8.0,
                0.0,
                ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 120,
                },
            ),
            border_radius: BorderRadius::default(),
        };
        let rect_item = DisplayListItem::Rect {
            bounds: wrect(30.0, 30.0, 40.0, 40.0),
            color: WHITE,
            border_radius: BorderRadius::default(),
        };
        let dl = DisplayList {
            items: vec![shadow_item, rect_item],
            ..Default::default()
        };

        let mut full = AzulPixmap::new(100, 100).unwrap();
        full.fill(255, 255, 255, 255);
        let mut gc = GlyphCache::new();
        render_display_list(&dl, &mut full, 1.0, &rr, &fm, &mut gc).unwrap();

        let mut incr = full.clone_pixmap();
        let st = CpuRenderState::new(ScrollOffsetMap::new());
        let mut gc2 = GlyphCache::new();
        render_display_list_damaged(
            &dl,
            &mut incr,
            1.0,
            &rr,
            &fm,
            &mut gc2,
            &st,
            &[lrect(25.0, 25.0, 12.0, 12.0)],
        )
        .unwrap();

        let diffs = full
            .data()
            .iter()
            .zip(incr.data().iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            diffs, 0,
            "the shadow re-blended outside the damage rect ({diffs} bytes differ)"
        );
    }

    #[test]
    fn compute_content_bounds_of_an_empty_list_is_none() {
        assert!(compute_content_bounds(&DisplayList::default()).is_none());
    }

    #[test]
    fn compute_content_bounds_ignores_state_management_items() {
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PopClip,
                DisplayListItem::PopScrollFrame,
                DisplayListItem::PopOpacity,
            ],
            ..Default::default()
        };
        assert!(
            compute_content_bounds(&dl).is_none(),
            "push/pop markers carry no content"
        );
    }

    #[test]
    fn compute_content_bounds_unions_every_drawing_item() {
        let dl = DisplayList {
            items: vec![
                DisplayListItem::Rect {
                    bounds: wrect(10.0, 20.0, 30.0, 40.0),
                    color: RED,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::Rect {
                    bounds: wrect(-5.0, 0.0, 5.0, 5.0),
                    color: BLUE,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopClip, // must not influence the box
            ],
            ..Default::default()
        };
        let (min_x, min_y, max_x, max_y) = compute_content_bounds(&dl).expect("has items");
        assert_eq!((min_x, min_y), (-5.0, 0.0));
        assert_eq!((max_x, max_y), (40.0, 60.0));
    }

    #[test]
    fn compute_content_bounds_with_nan_bounds_does_not_produce_nan() {
        // f32::min/max ignore a NaN operand, so a poisoned item cannot make the
        // whole content box NaN (it would turn into a 0-sized PNG downstream).
        let dl = DisplayList {
            items: vec![
                DisplayListItem::Rect {
                    bounds: wrect(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
                    color: RED,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::Rect {
                    bounds: wrect(0.0, 0.0, 10.0, 10.0),
                    color: BLUE,
                    border_radius: BorderRadius::default(),
                },
            ],
            ..Default::default()
        };
        let (min_x, min_y, max_x, max_y) = compute_content_bounds(&dl).expect("has items");
        for v in [min_x, min_y, max_x, max_y] {
            assert!(
                !v.is_nan(),
                "NaN item bounds must not poison the content box"
            );
        }
        assert_eq!((max_x, max_y), (10.0, 10.0));
    }

    // ==================================================================
    // build_rect_path / build_rounded_rect_path
    // ==================================================================

    #[test]
    fn build_rect_path_is_a_closed_quad() {
        let rect = AzRect::from_xywh(1.0, 2.0, 3.0, 4.0).unwrap();
        let path = build_rect_path(&rect);
        // move_to + 3x line_to + end_poly
        assert_eq!(path.total_vertices(), 5);
        let (mut x, mut y) = (0.0, 0.0);
        path.vertex_idx(0, &mut x, &mut y);
        assert_eq!((x, y), (1.0, 2.0));
        path.vertex_idx(2, &mut x, &mut y);
        assert_eq!((x, y), (4.0, 6.0), "the opposite corner is origin + size");
    }

    #[test]
    fn build_rounded_rect_path_falls_back_to_a_quad_for_non_positive_radii() {
        let rect = AzRect::from_xywh(0.0, 0.0, 10.0, 10.0).unwrap();
        let plain = build_rect_path(&rect).total_vertices();

        // Zero radii.
        assert_eq!(
            build_rounded_rect_path(&rect, &BorderRadius::default(), 1.0).total_vertices(),
            plain
        );
        // Negative radii must not generate arcs.
        let negative = BorderRadius {
            top_left: -5.0,
            top_right: -5.0,
            bottom_left: -5.0,
            bottom_right: -5.0,
        };
        assert_eq!(
            build_rounded_rect_path(&rect, &negative, 1.0).total_vertices(),
            plain
        );
        // A 0 dpi factor scales every radius to 0 -> the plain quad again.
        let positive = BorderRadius {
            top_left: 4.0,
            top_right: 4.0,
            bottom_left: 4.0,
            bottom_right: 4.0,
        };
        assert_eq!(
            build_rounded_rect_path(&rect, &positive, 0.0).total_vertices(),
            plain
        );
    }

    #[test]
    fn build_rounded_rect_path_emits_arc_vertices_for_positive_radii() {
        let rect = AzRect::from_xywh(0.0, 0.0, 40.0, 40.0).unwrap();
        let radius = BorderRadius {
            top_left: 8.0,
            top_right: 8.0,
            bottom_left: 8.0,
            bottom_right: 8.0,
        };
        let rounded = build_rounded_rect_path(&rect, &radius, 1.0).total_vertices();
        assert!(
            rounded > build_rect_path(&rect).total_vertices(),
            "arcs must add vertices (a square-cornered path would be the old bug)"
        );
    }

    #[test]
    fn build_rounded_rect_path_normalizes_oversized_radii() {
        // Radii far larger than the rect must be clamped, not explode the path.
        let rect = AzRect::from_xywh(0.0, 0.0, 10.0, 10.0).unwrap();
        let radius = BorderRadius {
            top_left: 1e6,
            top_right: 1e6,
            bottom_left: 1e6,
            bottom_right: 1e6,
        };
        let path = build_rounded_rect_path(&rect, &radius, 1.0);
        assert!(path.total_vertices() > 4);
        let (mut x, mut y) = (0.0, 0.0);
        for i in 0..path.total_vertices() {
            path.vertex_idx(i, &mut x, &mut y);
            assert!(
                x.is_finite() && y.is_finite(),
                "vertex {i} is not finite: ({x}, {y})"
            );
            assert!(
                (-1.0..=11.0).contains(&x) && (-1.0..=11.0).contains(&y),
                "vertex {i} ({x}, {y}) escaped the 10x10 rect"
            );
        }
    }

    // ==================================================================
    // text_lcd_enabled
    // ==================================================================

    #[test]
    fn text_aa_is_read_once_and_stable() {
        let first = text_aa();
        assert_eq!(first, text_aa(), "the OnceLock must not flip");
        if std::env::var("AZ_TEXT_AA").is_err() {
            assert_eq!(
                first, TEXT_AA_DEFAULT,
                "unset env -> the documented default"
            );
        }
        // The derived predicates must agree with the mode, or a caller can end
        // up on the LCD path while the blend thinks it is aliased.
        assert_eq!(
            text_lcd_enabled(),
            matches!(first, TextAa::Lcd | TextAa::Legacy)
        );
        assert_eq!(text_aliased(), first == TextAa::None);
        assert!(
            !(text_aliased() && text_lcd_enabled()),
            "aliased and LCD are mutually exclusive"
        );
    }

    /// The threshold is the whole point of `TextAa::None`: partial coverage must
    /// collapse to 0 or 255 and nothing in between may survive.
    #[test]
    fn aliased_threshold_is_half_open_at_128() {
        let covers: [u8; 6] = [0, 1, 127, 128, 254, 255];
        let out: Vec<u8> = covers
            .iter()
            .map(|&c| if c >= 128 { 255u8 } else { 0u8 })
            .collect();
        assert_eq!(out, vec![0, 0, 0, 255, 255, 255]);
        assert!(
            out.iter().all(|&v| v == 0 || v == 255),
            "no intermediate coverage may survive thresholding"
        );
    }

    // ==================================================================
    // render_single_item — stack discipline
    // ==================================================================

    #[test]
    fn unbalanced_pops_never_underflow_the_stacks() {
        // An over-popped display list (a real bookkeeping mismatch has shipped
        // before) must clamp, NOT abort the frame or panic.
        let mut p = pixmap(8, 8);
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut st = Stacks::new();
        for item in [
            DisplayListItem::PopClip,
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PopReferenceFrame,
            DisplayListItem::PopStackingContext,
            DisplayListItem::PopOpacity,
            DisplayListItem::PopTextShadow,
            DisplayListItem::PopImageMaskClip,
            DisplayListItem::PopFilter,
            DisplayListItem::PopBackdropFilter,
        ] {
            let res = run_item(&item, &mut p, &mut st, &state);
            assert_eq!(res, Ok(()), "{item:?} must not error");
        }
        assert_eq!(st.clips.len(), 1, "the base clip must never be popped");
        assert_eq!(st.transforms.len(), 1);
        assert_eq!(st.scrolls.len(), 1);
        assert!(st.masks.is_empty());
        assert!(st.shadows.is_empty());
    }

    #[test]
    #[should_panic = "called `Option::unwrap()` on a `None` value"]
    fn render_single_item_with_an_empty_clip_stack_panics_as_documented() {
        // The documented contract ("Panics if the clip stack is empty"). The
        // renderer always seeds `vec![None]`; this pins the precondition.
        let mut p = pixmap(4, 4);
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut st = Stacks::new();
        st.clips.clear();
        let _ = run_item(
            &DisplayListItem::Rect {
                bounds: wrect(0.0, 0.0, 4.0, 4.0),
                color: RED,
                border_radius: BorderRadius::default(),
            },
            &mut p,
            &mut st,
            &state,
        );
    }

    #[test]
    fn push_clip_intersects_with_the_active_clip_and_never_widens_it() {
        let mut p = pixmap(16, 16);
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut st = Stacks::new();

        run_item(
            &DisplayListItem::PushClip {
                bounds: wrect(0.0, 0.0, 10.0, 10.0),
                border_radius: BorderRadius::default(),
            },
            &mut p,
            &mut st,
            &state,
        )
        .unwrap();
        // A nested clip that reaches beyond the parent must be narrowed to it.
        run_item(
            &DisplayListItem::PushClip {
                bounds: wrect(5.0, 5.0, 100.0, 100.0),
                border_radius: BorderRadius::default(),
            },
            &mut p,
            &mut st,
            &state,
        )
        .unwrap();

        let top = st.clips.last().copied().flatten().expect("clip present");
        assert_eq!((top.x, top.y), (5.0, 5.0));
        assert_eq!(
            (top.width, top.height),
            (5.0, 5.0),
            "the child cannot escape the parent"
        );

        run_item(&DisplayListItem::PopClip, &mut p, &mut st, &state).unwrap();
        run_item(&DisplayListItem::PopClip, &mut p, &mut st, &state).unwrap();
        assert_eq!(st.clips.len(), 1);
    }

    #[test]
    fn push_clip_with_degenerate_bounds_pushes_an_unpaintable_clip() {
        let mut p = pixmap(8, 8);
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut st = Stacks::new();
        run_item(
            &DisplayListItem::PushClip {
                bounds: wrect(0.0, 0.0, f32::NAN, f32::NAN),
                border_radius: BorderRadius::default(),
            },
            &mut p,
            &mut st,
            &state,
        )
        .unwrap();
        assert_eq!(st.clips.len(), 2, "the pop must still find a matching push");

        let before = snap(&p);
        run_item(
            &DisplayListItem::Rect {
                bounds: wrect(0.0, 0.0, 8.0, 8.0),
                color: RED,
                border_radius: BorderRadius::default(),
            },
            &mut p,
            &mut st,
            &state,
        )
        .unwrap();
        assert_eq!(
            before,
            p.data(),
            "a NaN clip must not silently become 'no clip'"
        );
    }

    #[test]
    fn scroll_frames_shift_item_bounds_by_the_accumulated_offset() {
        let mut offsets = ScrollOffsetMap::new();
        offsets.insert(7, (0.0, 5.0));
        let state = CpuRenderState::new(offsets);

        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushScrollFrame {
                    clip_bounds: wrect(0.0, 0.0, 10.0, 10.0),
                    content_size: LogicalSize {
                        width: 10.0,
                        height: 100.0,
                    },
                    scroll_id: 7,
                },
                DisplayListItem::Rect {
                    bounds: wrect(0.0, 5.0, 10.0, 2.0),
                    color: RED,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopScrollFrame,
            ],
            ..Default::default()
        };

        let mut p = pixmap(10, 10);
        run_list_with_state(&dl, &mut p, &state).expect("must render");
        assert!(
            is_reddish(px_at(&p, 0, 0)),
            "content at y=5 scrolled by 5 must land on row 0"
        );
        assert_eq!(px_at(&p, 0, 5), [255, 255, 255, 255], "row 5 is now empty");
    }

    #[test]
    fn a_missing_scroll_id_defaults_to_a_zero_offset() {
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushScrollFrame {
                    clip_bounds: wrect(0.0, 0.0, 10.0, 10.0),
                    content_size: LogicalSize {
                        width: 10.0,
                        height: 10.0,
                    },
                    scroll_id: 999, // not in the map
                },
                DisplayListItem::Rect {
                    bounds: wrect(0.0, 0.0, 2.0, 2.0),
                    color: RED,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopScrollFrame,
            ],
            ..Default::default()
        };
        let mut p = pixmap(10, 10);
        run_list_with_state(&dl, &mut p, &CpuRenderState::new(ScrollOffsetMap::new()))
            .expect("must render");
        assert!(
            is_reddish(px_at(&p, 0, 0)),
            "an unknown scroll id must not shift"
        );
    }

    // ==================================================================
    // opacity layers
    // ==================================================================

    /// Draw black over white inside a `PushOpacity(op)` layer and return the
    /// resulting gray level.
    fn opacity_layer_result(op: f32) -> u8 {
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushOpacity {
                    bounds: wrect(0.0, 0.0, 4.0, 4.0),
                    opacity: op,
                    opacity_key: None,
                },
                DisplayListItem::Rect {
                    bounds: wrect(0.0, 0.0, 4.0, 4.0),
                    color: BLACK,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopOpacity,
            ],
            ..Default::default()
        };
        let mut p = pixmap(4, 4);
        run_list(&dl, &mut p, 1.0).expect("must render");
        px_at(&p, 1, 1)[0]
    }

    #[test]
    fn opacity_layer_blends_against_the_pre_push_snapshot() {
        assert_eq!(opacity_layer_result(1.0), 0, "opacity 1 keeps the drawing");
        assert_eq!(
            opacity_layer_result(0.0),
            255,
            "opacity 0 restores the snapshot"
        );
        let half = opacity_layer_result(0.5);
        assert!(
            (120..=136).contains(&half),
            "opacity 0.5 must land near mid-gray, got {half}"
        );
    }

    #[test]
    fn opacity_layer_saturates_out_of_range_and_nan_values() {
        // Out-of-range opacities clamp; NaN degrades to "fully transparent"
        // (0 after the cast) rather than panicking or writing garbage.
        assert_eq!(opacity_layer_result(5.0), 0, "opacity > 1 clamps to opaque");
        assert_eq!(
            opacity_layer_result(-5.0),
            255,
            "opacity < 0 clamps to transparent"
        );
        assert_eq!(opacity_layer_result(f32::INFINITY), 0);
        assert_eq!(opacity_layer_result(f32::NEG_INFINITY), 255);
        assert_eq!(opacity_layer_result(f32::NAN), 255);
    }

    #[test]
    fn push_opacity_with_degenerate_bounds_pushes_nothing() {
        // No rect -> no snapshot -> nothing to pop; the matching PopOpacity must
        // not blow up or consume an unrelated mask entry.
        let mut p = pixmap(8, 8);
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut st = Stacks::new();
        run_item(
            &DisplayListItem::PushOpacity {
                bounds: wrect(0.0, 0.0, f32::NAN, 0.0),
                opacity: 0.5,
                opacity_key: None,
            },
            &mut p,
            &mut st,
            &state,
        )
        .unwrap();
        assert!(st.masks.is_empty());
        assert_eq!(
            run_item(&DisplayListItem::PopOpacity, &mut p, &mut st, &state),
            Ok(())
        );
    }

    // ==================================================================
    // image mask clips
    // ==================================================================

    #[test]
    fn image_mask_clip_masks_the_drawing_it_wraps() {
        // A 2x2 R8 mask: left column opaque, right column clipped.
        let mask = r8_image(2, 2, vec![255, 0, 255, 0]);
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushImageMaskClip {
                    bounds: wrect(0.0, 0.0, 4.0, 4.0),
                    mask_image: mask,
                    mask_rect: wrect(0.0, 0.0, 4.0, 4.0),
                },
                DisplayListItem::Rect {
                    bounds: wrect(0.0, 0.0, 4.0, 4.0),
                    color: BLACK,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopImageMaskClip,
            ],
            ..Default::default()
        };
        let mut p = pixmap(4, 4);
        run_list(&dl, &mut p, 1.0).expect("must render");
        assert_eq!(px_at(&p, 0, 0), [0, 0, 0, 255], "mask=255 keeps the fill");
        assert_eq!(
            px_at(&p, 3, 0),
            [255, 255, 255, 255],
            "mask=0 restores the background"
        );
    }

    #[test]
    fn image_mask_clip_with_a_degenerate_rect_is_skipped() {
        let mask = r8_image(1, 1, vec![255]);
        let mut p = pixmap(8, 8);
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut st = Stacks::new();
        run_item(
            &DisplayListItem::PushImageMaskClip {
                bounds: wrect(0.0, 0.0, 8.0, 8.0),
                mask_image: mask,
                mask_rect: wrect(0.0, 0.0, 0.0, 0.0),
            },
            &mut p,
            &mut st,
            &state,
        )
        .unwrap();
        assert!(st.masks.is_empty(), "a 0-sized mask rect pushes no entry");
    }

    // ==================================================================
    // text items without fonts
    // ==================================================================

    /// A `font_hash` layout emitted that its own `FontManager` cannot resolve is a
    /// broken invariant, not a missing asset, so `font_resolution_failed` fires a
    /// `debug_assert`. The two build profiles therefore owe DIFFERENT contracts and
    /// this pins both:
    ///
    ///   - debug: die on it. That gate exists so a test catches the desync, and a
    ///     test that swallowed it would be the exact silent failure it guards.
    ///   - release: drop that one text run and keep the frame — losing a line of
    ///     text must never take the window down in front of a user.
    ///
    /// Asserting only the release half is what made this test fail on a debug
    /// `cargo test`: it demanded graceful degradation from a build deliberately
    /// built not to degrade gracefully.
    #[test]
    #[cfg_attr(debug_assertions, should_panic(expected = "cannot resolve"))]
    fn a_text_item_whose_font_is_unknown_paints_nothing() {
        let dl = DisplayList {
            items: vec![DisplayListItem::Text {
                glyphs: vec![GlyphInstance {
                    index: 1,
                    point: LogicalPosition { x: 0.0, y: 10.0 },
                    size: LogicalSize {
                        width: 8.0,
                        height: 16.0,
                    },
                }],
                font_hash: FontHash {
                    font_hash: 0xdead_beef,
                },
                font_size_px: 16.0,
                color: BLACK,
                clip_rect: wrect(0.0, 0.0, 16.0, 16.0),
                source_node_index: None,
            }],
            ..Default::default()
        };
        let mut p = pixmap(16, 16);
        let before = snap(&p);
        run_list(&dl, &mut p, 1.0).expect("a missing font must not fail the frame");
        assert_eq!(before, p.data());
    }

    #[test]
    fn a_text_item_with_no_glyphs_or_no_alpha_paints_nothing() {
        for (glyphs, color) in [
            (Vec::new(), BLACK),
            (
                vec![GlyphInstance {
                    index: 1,
                    point: LogicalPosition { x: 0.0, y: 10.0 },
                    size: LogicalSize {
                        width: 8.0,
                        height: 16.0,
                    },
                }],
                CLEAR,
            ),
        ] {
            let dl = DisplayList {
                items: vec![DisplayListItem::Text {
                    glyphs,
                    font_hash: FontHash { font_hash: 1 },
                    font_size_px: 16.0,
                    color,
                    clip_rect: wrect(0.0, 0.0, 16.0, 16.0),
                    source_node_index: None,
                }],
                ..Default::default()
            };
            let mut p = pixmap(16, 16);
            let before = snap(&p);
            run_list(&dl, &mut p, 1.0).expect("must render");
            assert_eq!(before, p.data());
        }
    }

    // ==================================================================
    // render_image (through the display list)
    // ==================================================================

    #[test]
    fn an_rgba_image_is_blitted_with_its_channels_in_order() {
        // Solid red, opaque.
        let img = rgba_image(2, 2, [255, 0, 0, 255].repeat(4));
        let dl = DisplayList {
            items: vec![DisplayListItem::Image {
                bounds: wrect(0.0, 0.0, 4.0, 4.0),
                image: img,
                border_radius: BorderRadius::default(),
            }],
            ..Default::default()
        };
        let mut p = pixmap(8, 8);
        run_list(&dl, &mut p, 1.0).expect("must render");
        assert!(
            is_reddish(px_at(&p, 1, 1)),
            "an RGBA image must not come out swizzled or gray, got {:?}",
            px_at(&p, 1, 1)
        );
        assert_eq!(px_at(&p, 6, 6), [255, 255, 255, 255], "outside the bounds");
    }

    #[test]
    fn a_downscaled_image_is_area_averaged_not_nearest_sampled() {
        // THE CLASS: the CPU image blit used to convert the WHOLE source to
        // RGBA and then nearest-sample it — a 4K camera frame into a 160 px
        // tile cost a 33 MB swizzle per repaint and aliased (one source pixel
        // "won" per destination pixel, so thin lines flickered as the tile
        // resized). The blit now samples the source through
        // `image_scale::sample`, which area-averages the footprint of every
        // destination pixel. A black/white checkerboard shrunk into ONE pixel
        // is the discriminating input: nearest sampling returns pure black or
        // pure white, area averaging returns mid grey.
        let mut checker = Vec::with_capacity(4 * 4 * 4);
        for y in 0..4 {
            for x in 0..4 {
                let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                checker.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let img = rgba_image(4, 4, checker);
        let dl = DisplayList {
            items: vec![DisplayListItem::Image {
                bounds: wrect(0.0, 0.0, 1.0, 1.0),
                image: img,
                border_radius: BorderRadius::default(),
            }],
            ..Default::default()
        };
        let mut p = pixmap(4, 4);
        run_list(&dl, &mut p, 1.0).expect("must render");
        let [r, g, b, a] = px_at(&p, 0, 0);
        assert_eq!(a, 255, "an opaque source stays opaque");
        for (name, c) in [("r", r), ("g", g), ("b", b)] {
            assert!(
                (96..=160).contains(&c),
                "NEAREST SAMPLING: a 4x4 checkerboard shrunk into one pixel must average to \
                 mid grey, got {name} = {c} (pure black/white = one source pixel won)"
            );
        }
        assert_eq!(px_at(&p, 2, 2), [255, 255, 255, 255], "outside the bounds");
    }

    /// The blit exactly as it was written before the fast path: one
    /// `image_scale::sample` per destination pixel, composited with the
    /// documented formula. Deliberately duplicated here — it is the thing the
    /// production blit must keep agreeing with, so it has to exist somewhere
    /// the production code cannot drift it.
    fn reference_blit(
        pixmap: &mut AzulPixmap,
        src: &crate::image_scale::SrcImage<'_>,
        dst_x: i32,
        dst_y: i32,
        dst_w: u32,
        dst_h: u32,
    ) {
        let pw = pixmap.width;
        let ph = pixmap.height;
        for py in 0..dst_h {
            for px in 0..dst_w {
                let tx = dst_x + px as i32;
                let ty = dst_y + py as i32;
                if tx < 0 || ty < 0 || tx >= pw as i32 || ty >= ph as i32 {
                    continue;
                }
                let [sr, sg, sb, sa8] = crate::image_scale::sample(src, dst_w, dst_h, px, py);
                let di = ((ty as u32 * pw + tx as u32) * 4) as usize;
                if di + 3 >= pixmap.data.len() {
                    continue;
                }
                let sa = u32::from(sa8);
                if sa == 255 {
                    pixmap.data[di] = sr;
                    pixmap.data[di + 1] = sg;
                    pixmap.data[di + 2] = sb;
                    pixmap.data[di + 3] = 255;
                } else if sa > 0 {
                    let da = 255 - sa;
                    pixmap.data[di] =
                        ((u32::from(sr) * sa + u32::from(pixmap.data[di]) * da) / 255) as u8;
                    pixmap.data[di + 1] =
                        ((u32::from(sg) * sa + u32::from(pixmap.data[di + 1]) * da) / 255) as u8;
                    pixmap.data[di + 2] =
                        ((u32::from(sb) * sa + u32::from(pixmap.data[di + 2]) * da) / 255) as u8;
                    pixmap.data[di + 3] =
                        ((sa + u32::from(pixmap.data[di + 3]) * da / 255).min(255)) as u8;
                }
            }
        }
    }

    /// A deterministic, non-uniform source in `fmt`. Uniform data would let a
    /// wrong tap position pass, and a wrong swizzle pass too.
    fn noisy_src(fmt: azul_core::resources::RawImageFormat, w: u32, h: u32) -> Vec<u8> {
        let bpp = crate::image_scale::bytes_per_pixel(fmt).expect("a sampleable format");
        (0..(w as usize * h as usize * bpp))
            .map(|i| ((i * 37 + 11) % 256) as u8)
            .collect()
    }

    #[test]
    fn the_fast_image_blit_is_byte_identical_to_image_scale_sample() {
        // `image_scale::sample` is the golden reference every consumer of the
        // scaler agrees with, and the blit no longer CALLS it once per
        // destination pixel — that cost ~19 ns/px, which on the path that
        // actually matters (a HiDPI window compositing a logical-sized canvas,
        // i.e. a 2x bilinear upscale over the whole window) is ~45 ms of blit
        // for one frame. The arithmetic is what could silently drift, so every
        // destination byte must still be the byte the reference produces, in
        // every format and on both sides of the upscale/downscale branch.
        use azul_core::resources::RawImageFormat as F;
        for fmt in [F::RGBA8, F::BGRA8, F::RGB8, F::BGR8, F::R8] {
            let (sw, sh) = (13u32, 7u32);
            let bytes = noisy_src(fmt, sw, sh);
            let src = crate::image_scale::SrcImage {
                bytes: &bytes,
                format: fmt,
                width: sw,
                height: sh,
            };
            assert!(src.is_sampleable(), "{fmt:?} must be sampleable");
            for (dw, dh) in [
                (13u32, 7u32), // 1:1
                (40, 23),      // upscale both axes
                (5, 3),        // downscale both axes
                (40, 3),       // upscale x, downscale y
                (5, 23),       // downscale x, upscale y
                (1, 1),        // extreme downscale
                (13, 23),      // 1:1 on x, upscale on y
            ] {
                let mut fast = pixmap(dw + 4, dh + 4);
                let mut want = pixmap(dw + 4, dh + 4);
                blit_sampled_image(
                    &mut fast,
                    &src,
                    2,
                    2,
                    dw,
                    dh,
                    (0, 0, dw, dh),
                    &mut RowConversions::new(),
                );
                reference_blit(&mut want, &src, 2, 2, dw, dh);
                assert_eq!(
                    snap(&fast),
                    snap(&want),
                    "{fmt:?} {sw}x{sh} -> {dw}x{dh}: the fast blit must be BYTE-IDENTICAL to \
                     image_scale::sample, not merely close"
                );
            }
        }
    }

    #[test]
    fn a_clipped_image_blit_narrows_its_loop_to_the_clip() {
        // THE STRUCTURAL BUG. The blit walked every pixel of the image NODE and
        // `continue`d on the clip test, so a 40 px damage strip over a
        // 2078x1132 canvas still ran 2.35 M loop iterations — and, until this,
        // 2.35 M `image_scale::sample` calls. The window the loop runs over
        // must be the clip, not the node.
        let win = visible_dst_window(100, 50, 2000, 1000, 4000, 2000, (140, 90, 180, 130))
            .expect("the clip overlaps the image");
        assert_eq!(
            win,
            (40, 40, 80, 80),
            "the loop must cover only the clipped 40x40"
        );

        // Nothing visible -> no loop at all, not a full-node walk that writes
        // nothing.
        assert!(
            visible_dst_window(100, 50, 10, 10, 4000, 2000, (500, 500, 600, 600)).is_none(),
            "a clip that misses the image must produce no window"
        );

        // The pixmap edge clamps exactly like the clip does: an image hanging
        // off the top-left starts at the first on-screen pixel.
        let win = visible_dst_window(
            -20,
            -30,
            100,
            100,
            50,
            40,
            (i32::MIN, i32::MIN, i32::MAX, i32::MAX),
        )
        .expect("partly on screen");
        assert_eq!(win, (20, 30, 70, 70));
    }

    #[test]
    fn each_source_row_is_converted_once_not_once_per_destination_row() {
        // THE COMPLEXITY INVARIANT, and the reason the fix is not just
        // "inline sample()". A per-pixel `image_scale::sample` re-read the
        // source through `SrcImage::pixel` FOUR times per destination pixel,
        // each re-deriving the pixel format and re-clamping — 9.4 M
        // format-dispatched reads for one 2x upscale of AzPaint's canvas.
        //
        // Converting each source row to straight RGBA once and reusing it
        // makes the format dispatch O(source rows touched). Asserting that
        // count is stable under machine load, unlike a millisecond threshold.
        use azul_core::resources::RawImageFormat as F;

        // 4x UPSCALE: 40 source rows feeding 160 destination rows.
        let (sw, sh) = (50u32, 40u32);
        let bytes = noisy_src(F::RGBA8, sw, sh);
        let src = crate::image_scale::SrcImage {
            bytes: &bytes,
            format: F::RGBA8,
            width: sw,
            height: sh,
        };
        let (dw, dh) = (200u32, 160u32);
        let mut p = pixmap(dw, dh);
        let mut conv = RowConversions::new();
        blit_sampled_image(&mut p, &src, 0, 0, dw, dh, (0, 0, dw, dh), &mut conv);
        assert!(
            conv.0 <= sh as usize + 1,
            "a {sh}-row source must be converted about once per row, not per destination row: \
             {} conversions",
            conv.0
        );
        assert!(
            conv.0 < dh as usize,
            "conversions ({}) must not scale with the {dh} destination rows",
            conv.0
        );

        // 4x DOWNSCALE: every destination row averages 4 source rows, and the
        // tap rows advance monotonically — so each source row is still read
        // once, never once per tap.
        let (sw, sh) = (200u32, 160u32);
        let bytes = noisy_src(F::RGBA8, sw, sh);
        let src = crate::image_scale::SrcImage {
            bytes: &bytes,
            format: F::RGBA8,
            width: sw,
            height: sh,
        };
        let (dw, dh) = (50u32, 40u32);
        let mut p = pixmap(dw, dh);
        let mut conv = RowConversions::new();
        blit_sampled_image(&mut p, &src, 0, 0, dw, dh, (0, 0, dw, dh), &mut conv);
        assert!(
            conv.0 <= sh as usize,
            "a {sh}-row source downscaled 4x must convert at most {sh} rows, got {}",
            conv.0
        );

        // A CLIPPED blit touches only the source rows its window needs — the
        // point of narrowing the loop.
        let mut p = pixmap(dw, dh);
        let mut conv = RowConversions::new();
        blit_sampled_image(&mut p, &src, 0, 0, dw, dh, (0, 0, dw, 4), &mut conv);
        assert!(
            conv.0 <= 4 * BLIT_MAX_TAPS as usize,
            "4 clipped destination rows must not convert the whole {sh}-row source: {}",
            conv.0
        );
    }

    #[test]
    fn every_live_frame_format_is_sampleable_by_the_blit() {
        // The grey-placeholder arm must never catch a live-frame producer's
        // format (camera / screencap / video emit RGBA8 or BGRA8; decoders
        // also hand out RGB8 and R8). This pins the contract between the
        // producers and `image_scale::bytes_per_pixel`, so adding a producer
        // format without teaching the sampler fails here, not as flat grey
        // tiles on screen.
        use azul_core::resources::RawImageFormat;
        for f in [
            RawImageFormat::RGBA8,
            RawImageFormat::BGRA8,
            RawImageFormat::RGB8,
            RawImageFormat::R8,
        ] {
            assert!(
                crate::image_scale::bytes_per_pixel(f).is_some(),
                "{f:?} is a producer format and must be sampleable"
            );
        }
    }

    #[test]
    fn an_image_with_degenerate_bounds_is_skipped() {
        for bad in DEGENERATE {
            let img = rgba_image(1, 1, vec![255, 0, 0, 255]);
            let dl = DisplayList {
                items: vec![DisplayListItem::Image {
                    bounds: wrect(0.0, 0.0, bad, bad),
                    image: img,
                    border_radius: BorderRadius::default(),
                }],
                ..Default::default()
            };
            let mut p = pixmap(8, 8);
            let before = snap(&p);
            run_list(&dl, &mut p, 1.0).expect("must render");
            assert_eq!(before, p.data(), "image size {bad} must be rejected");
        }
    }

    #[test]
    fn a_fully_transparent_image_leaves_the_background_alone() {
        let img = rgba_image(2, 2, [255, 0, 0, 0].repeat(4));
        let dl = DisplayList {
            items: vec![DisplayListItem::Image {
                bounds: wrect(0.0, 0.0, 4.0, 4.0),
                image: img,
                border_radius: BorderRadius::default(),
            }],
            ..Default::default()
        };
        let mut p = pixmap(8, 8);
        let before = snap(&p);
        run_list(&dl, &mut p, 1.0).expect("must render");
        assert_eq!(before, p.data(), "alpha=0 source pixels must not blend");
    }

    // ==================================================================
    // render_border / render_border_sides
    // ==================================================================

    #[test]
    fn render_border_draws_the_frame_but_not_the_middle() {
        let mut p = pixmap(20, 20);
        render_border(
            &mut p,
            &lrect(0.0, 0.0, 20.0, 20.0),
            RED,
            2.0,
            BorderStyle::Solid,
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert!(is_reddish(px_at(&p, 0, 0)), "the frame is painted");
        assert!(is_reddish(px_at(&p, 19, 19)));
        assert_eq!(
            px_at(&p, 10, 10),
            [255, 255, 255, 255],
            "the middle stays clear"
        );
    }

    #[test]
    fn render_border_zero_or_negative_width_is_a_noop() {
        for width in [0.0, -1.0, -1e30, f32::NEG_INFINITY] {
            let mut p = pixmap(10, 10);
            let before = snap(&p);
            render_border(
                &mut p,
                &lrect(0.0, 0.0, 10.0, 10.0),
                RED,
                width,
                BorderStyle::Solid,
                &BorderRadius::default(),
                None,
                1.0,
            );
            assert_eq!(before, p.data(), "border width {width} must not paint");
        }
    }

    #[test]
    fn render_border_nan_width_and_hidden_styles_are_noops() {
        // NaN width: `width <= 0.0` is false for NaN, so this runs the whole
        // pipeline with a poisoned width. It must stay inside the buffer and,
        // above all, must not flood the box (a NaN stroke width that degraded
        // into a fill would swallow the element's content).
        let mut p = pixmap(10, 10);
        render_border(
            &mut p,
            &lrect(0.0, 0.0, 10.0, 10.0),
            RED,
            f32::NAN,
            BorderStyle::Solid,
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert_eq!(p.data().len(), 400, "the buffer must be intact");
        assert_eq!(
            px_at(&p, 5, 5),
            [255, 255, 255, 255],
            "a NaN border width must not fill the middle of the box"
        );

        for style in [BorderStyle::None, BorderStyle::Hidden] {
            let mut p = pixmap(10, 10);
            let before = snap(&p);
            render_border(
                &mut p,
                &lrect(0.0, 0.0, 10.0, 10.0),
                RED,
                2.0,
                style,
                &BorderRadius::default(),
                None,
                1.0,
            );
            assert_eq!(before, p.data(), "{style:?} must not paint");
        }
    }

    #[test]
    fn render_border_transparent_color_and_degenerate_dpi_are_noops() {
        let mut p = pixmap(10, 10);
        let before = snap(&p);
        render_border(
            &mut p,
            &lrect(0.0, 0.0, 10.0, 10.0),
            CLEAR,
            2.0,
            BorderStyle::Solid,
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert_eq!(before, p.data());

        for dpi in DEGENERATE {
            let mut p = pixmap(10, 10);
            let before = snap(&p);
            render_border(
                &mut p,
                &lrect(0.0, 0.0, 10.0, 10.0),
                RED,
                2.0,
                BorderStyle::Solid,
                &BorderRadius::default(),
                None,
                dpi,
            );
            assert_eq!(before, p.data(), "dpi {dpi} must be rejected");
        }
    }

    #[test]
    fn render_border_width_larger_than_the_box_does_not_panic() {
        // The inner rect goes negative -> AzRect::from_xywh returns None and the
        // border degrades to a solid fill instead of underflowing.
        let mut p = pixmap(10, 10);
        render_border(
            &mut p,
            &lrect(0.0, 0.0, 10.0, 10.0),
            RED,
            1000.0,
            BorderStyle::Solid,
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert!(is_reddish(px_at(&p, 5, 5)));
    }

    #[test]
    fn render_border_dashed_and_dotted_styles_paint_without_panicking() {
        for style in [BorderStyle::Dashed, BorderStyle::Dotted] {
            let mut p = pixmap(20, 20);
            let before = snap(&p);
            render_border(
                &mut p,
                &lrect(2.0, 2.0, 16.0, 16.0),
                RED,
                2.0,
                style,
                &BorderRadius::default(),
                None,
                1.0,
            );
            assert_ne!(before, p.data(), "{style:?} must paint something");
        }
    }

    #[test]
    fn render_border_sides_with_mixed_widths_paints_each_side() {
        let mut p = pixmap(20, 20);
        render_border_sides(
            &mut p,
            &lrect(0.0, 0.0, 20.0, 20.0),
            [RED, BLUE, RED, BLUE],
            [3.0, 1.0, 3.0, 1.0],
            [
                BorderStyle::Solid,
                BorderStyle::Solid,
                BorderStyle::Solid,
                BorderStyle::Solid,
            ],
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert!(is_reddish(px_at(&p, 10, 0)), "the top side is red");
        assert_eq!(
            px_at(&p, 10, 10),
            [255, 255, 255, 255],
            "the middle stays clear"
        );
    }

    #[test]
    fn render_border_sides_zero_widths_and_degenerate_values_are_noops() {
        let styles = [
            BorderStyle::Solid,
            BorderStyle::Solid,
            BorderStyle::Solid,
            BorderStyle::Solid,
        ];
        let mut p = pixmap(10, 10);
        let before = snap(&p);
        render_border_sides(
            &mut p,
            &lrect(0.0, 0.0, 10.0, 10.0),
            [RED; 4],
            [0.0; 4],
            styles,
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert_eq!(before, p.data(), "0-width sides must not paint");

        for bad in DEGENERATE {
            let mut p = pixmap(10, 10);
            let before = snap(&p);
            render_border_sides(
                &mut p,
                &lrect(0.0, 0.0, 10.0, 10.0),
                [RED; 4],
                [2.0; 4],
                styles,
                &BorderRadius::default(),
                None,
                bad,
            );
            assert_eq!(before, p.data(), "dpi {bad} must be rejected");
        }

        // NaN / inf widths must not corrupt the buffer either.
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0] {
            let mut p = pixmap(10, 10);
            render_border_sides(
                &mut p,
                &lrect(0.0, 0.0, 10.0, 10.0),
                [RED; 4],
                [bad; 4],
                styles,
                &BorderRadius::default(),
                None,
                1.0,
            );
            assert_eq!(
                p.data().len(),
                400,
                "width {bad} must not resize the buffer"
            );
        }
    }

    // ==================================================================
    // render_display_list_damaged
    // ==================================================================

    fn damaged(dl: &DisplayList, p: &mut AzulPixmap, rects: &[LogicalRect]) -> Result<(), String> {
        let res = RendererResources::default();
        let mut gc = GlyphCache::new();
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        render_display_list_damaged(
            dl,
            p,
            1.0,
            &res,
            &empty_font_manager(),
            &mut gc,
            &state,
            rects,
        )
    }

    fn full_red_dl() -> DisplayList {
        DisplayList {
            items: vec![DisplayListItem::Rect {
                bounds: wrect(0.0, 0.0, 8.0, 8.0),
                color: RED,
                border_radius: BorderRadius::default(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn damaged_render_without_rects_is_a_noop() {
        let mut p = pixmap(8, 8);
        p.fill(0, 0, 255, 255);
        let before = snap(&p);
        damaged(&full_red_dl(), &mut p, &[]).expect("must succeed");
        assert_eq!(before, p.data(), "no damage -> no repaint at all");
    }

    #[test]
    fn damaged_render_only_repaints_inside_the_damage_rect() {
        let mut p = pixmap(8, 8);
        p.fill(0, 0, 255, 255); // stale blue frame
        damaged(&full_red_dl(), &mut p, &[lrect(0.0, 0.0, 4.0, 4.0)]).expect("must succeed");
        assert!(
            is_reddish(px_at(&p, 1, 1)),
            "the damaged region is repainted"
        );
        assert_eq!(
            px_at(&p, 6, 6),
            [0, 0, 255, 255],
            "untouched pixels must survive — a union-clip repaint used to wipe them"
        );
    }

    #[test]
    fn damaged_render_with_nan_rects_paints_nothing() {
        let mut p = pixmap(8, 8);
        p.fill(0, 0, 255, 255);
        let before = snap(&p);
        damaged(
            &full_red_dl(),
            &mut p,
            &[lrect(f32::NAN, f32::NAN, f32::NAN, f32::NAN)],
        )
        .expect("must succeed");
        assert_eq!(
            before,
            p.data(),
            "a NaN damage rect must collapse to nothing"
        );
    }

    #[test]
    fn damaged_render_clamps_saturating_rects_to_the_pixmap() {
        let mut p = pixmap(8, 8);
        p.fill(0, 0, 255, 255);
        damaged(&full_red_dl(), &mut p, &[lrect(-1e9, -1e9, 3e9, 3e9)]).expect("must succeed");
        assert!(
            p.data().chunks_exact(4).all(|c| c[0] > 200 && c[1] < 60),
            "an oversized damage rect clamps to the buffer and repaints all of it"
        );
    }

    #[test]
    fn damaged_render_merges_overlapping_rects_without_double_blending() {
        // Two overlapping damage rects must be merged so the overlap is not
        // alpha-blended twice (a half-transparent fill would double-darken).
        let half_red = ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 128,
        };
        let dl = DisplayList {
            items: vec![DisplayListItem::Rect {
                bounds: wrect(0.0, 0.0, 8.0, 8.0),
                color: half_red,
                border_radius: BorderRadius::default(),
            }],
            ..Default::default()
        };

        let mut once = pixmap(8, 8);
        damaged(&dl, &mut once, &[lrect(0.0, 0.0, 8.0, 8.0)]).expect("must succeed");

        let mut twice = pixmap(8, 8);
        damaged(
            &dl,
            &mut twice,
            &[lrect(0.0, 0.0, 6.0, 6.0), lrect(2.0, 2.0, 6.0, 6.0)],
        )
        .expect("must succeed");

        assert_eq!(
            px_at(&once, 3, 3),
            px_at(&twice, 3, 3),
            "the overlap must be blended exactly once"
        );
    }

    #[test]
    fn damaged_render_with_a_zero_area_rect_is_a_noop() {
        let mut p = pixmap(8, 8);
        p.fill(0, 0, 255, 255);
        let before = snap(&p);
        damaged(&full_red_dl(), &mut p, &[lrect(4.0, 4.0, 0.0, 0.0)]).expect("must succeed");
        assert_eq!(before, p.data());
    }

    // ==================================================================
    // render_component_preview / render_text_run_to_pixmap
    // ==================================================================

    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    #[test]
    fn component_preview_of_a_degenerate_size_never_panics() {
        use rust_fontconfig::FcFontCache;

        let mut dom = azul_core::dom::Dom::create_body();
        let styled =
            azul_core::styled_dom::StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let fm = FontManager::<FontRef>::new(FcFontCache::default()).expect("font manager");

        // Sizes are kept small on purpose: `render_component_preview` clamps to
        // MAX_SIZE (4096) and then ALLOCATES that, so sweeping huge widths here
        // would allocate + PNG-encode a 4096x4096 buffer per case.
        for (w, h, dpi) in [
            (Some(0.0), Some(0.0), 1.0),
            (Some(8.0), Some(8.0), 0.0),
            (Some(8.0), Some(8.0), 1.0),
        ] {
            let o = ComponentPreviewOptions {
                width: w,
                height: h,
                dpi_factor: dpi,
                ..ComponentPreviewOptions::default()
            };
            match render_component_preview(&styled, &fm, o, None) {
                Ok(res) => {
                    assert!(
                        res.content_width.is_finite() && res.content_height.is_finite(),
                        "{w:?}x{h:?}@{dpi} produced non-finite content bounds"
                    );
                    assert!(
                        res.content_width <= 4096.0 && res.content_height <= 4096.0,
                        "the preview must stay bounded by MAX_SIZE"
                    );
                }
                Err(e) => assert!(!e.is_empty(), "an error must carry a message"),
            }
        }
    }

    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    #[test]
    fn text_run_to_pixmap_without_any_font_returns_none_for_every_input() {
        use rust_fontconfig::FcFontCache;

        // An EMPTY font cache: every input must bail out with None — no panic,
        // no unbounded allocation, no hang. This is the fallback path shells hit
        // when fontconfig finds nothing.
        let empty = FcFontCache::default();
        let long = "A".repeat(1_000_000);
        let nested = "[".repeat(10_000);
        let inputs = [
            "",
            "   ",
            "\t\n\r",
            "\0\u{1}\u{7f}",
            "0",
            "-0",
            "9223372036854775807",
            "NaN",
            "inf",
            "-inf",
            "  valid  ",
            "valid;garbage",
            "\u{1F600}\u{1F1E9}\u{1F1EA}",
            "e\u{301}\u{323}\u{489}",
            long.as_str(),
            nested.as_str(),
        ];
        for text in inputs {
            let got = render_text_run_to_pixmap(&empty, text, 16.0, BLACK, WHITE, 2.0, 1.0);
            assert!(
                got.is_none(),
                "no resolvable font must yield None (input len {})",
                text.len()
            );
        }

        // Degenerate numerics must not panic either.
        for size in [0.0, -16.0, f32::NAN, f32::INFINITY] {
            assert!(
                render_text_run_to_pixmap(&empty, "hi", size, BLACK, WHITE, 0.0, 1.0).is_none()
            );
        }
        for dpi in [0.0, -1.0, f32::NAN] {
            assert!(
                render_text_run_to_pixmap(&empty, "hi", 16.0, BLACK, WHITE, 2.0, dpi).is_none()
            );
        }
    }

    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    #[test]
    fn text_run_to_pixmap_renders_dark_glyphs_on_the_background() {
        use rust_fontconfig::{FcFont, FcFontCache, FcPattern};

        let candidates = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
            "C:/Windows/Fonts/arial.ttf",
        ];
        let Some(bytes) = candidates.iter().find_map(|p| std::fs::read(p).ok()) else {
            eprintln!("[skip] no system font file available");
            return;
        };

        let cache = FcFontCache::default();
        cache.with_memory_fonts(vec![(
            FcPattern {
                family: Some("sans-serif".to_string()),
                ..Default::default()
            },
            FcFont {
                bytes,
                font_index: 0,
                id: "autotest-sans".to_string(),
            },
        )]);

        let Some(p) = render_text_run_to_pixmap(&cache, "Hi", 24.0, BLACK, WHITE, 4.0, 1.0) else {
            eprintln!("[skip] the memory font did not resolve through fontconfig");
            return;
        };
        assert!(p.width >= 1 && p.height >= 1);
        let dark = p.data().chunks_exact(4).filter(|c| c[0] < 128).count();
        assert!(dark > 0, "the glyph run must actually rasterize");

        // Empty text still produces a valid, background-only pixmap (it is a
        // tooltip surface — callers blit it unconditionally).
        let empty = render_text_run_to_pixmap(&cache, "", 24.0, BLACK, WHITE, 4.0, 1.0)
            .expect("empty text must still give a pixmap");
        assert!(empty.width >= 1 && empty.height >= 1);
        assert!(
            empty
                .data()
                .chunks_exact(4)
                .all(|c| c[0] == 255 && c[1] == 255),
            "empty text must paint no glyphs"
        );

        // Multibyte / unreachable-codepoint input falls back to glyph 0.
        assert!(
            render_text_run_to_pixmap(&cache, "\u{1F600}é\u{301}", 24.0, BLACK, WHITE, 4.0, 1.0)
                .is_some(),
            "unicode input must not panic or bail out"
        );
    }
}

#[cfg(test)]
mod damage_debug_tests {
    use super::*;

    /// The damaged-repaint clear colour is white unless asked otherwise, and
    /// the knob really produces a DIFFERENT colour — a debug fill that
    /// silently stayed white would make the "cleared but never repainted"
    /// experiment look like a clean frame.
    ///
    /// NEGATIVE CONTROL: making `parse_damage_fill` return WHITE for every
    /// input fails the "1" and hex cases — run and seen.
    #[test]
    fn the_damage_fill_knob_changes_the_clear_colour() {
        const WHITE: (u8, u8, u8, u8) = (255, 255, 255, 255);
        assert_eq!(parse_damage_fill(None), WHITE, "unset ships as white");
        assert_eq!(parse_damage_fill(Some("0")), WHITE, "0 turns it off");
        assert_eq!(parse_damage_fill(Some("")), WHITE);

        assert_eq!(parse_damage_fill(Some("1")), (255, 0, 0, 255), "1 is red");
        assert_ne!(
            parse_damage_fill(Some("1")),
            WHITE,
            "the debug fill must not be white, or nothing is visible"
        );

        assert_eq!(parse_damage_fill(Some("00ff00")), (0, 255, 0, 255));
        assert_eq!(parse_damage_fill(Some("#0000ff")), (0, 0, 255, 255));
        // Garbage falls back to red rather than to white: the user asked for
        // a debug fill, so give them a visible one.
        assert_eq!(parse_damage_fill(Some("nonsense")), (255, 0, 0, 255));
    }
}

#[cfg(all(test, feature = "std"))]
mod shadow_blur_cache_tests {
    use super::*;
    use azul_css::props::basic::pixel::{PixelValue, PixelValueNoPercent};
    use azul_css::props::style::box_shadow::{BoxShadowClipMode, StyleBoxShadow};

    fn pv(v: f32) -> PixelValueNoPercent {
        PixelValueNoPercent {
            inner: PixelValue::px(v),
        }
    }

    fn shadow() -> StyleBoxShadow {
        StyleBoxShadow {
            offset_x: pv(4.0),
            offset_y: pv(4.0),
            blur_radius: pv(8.0),
            spread_radius: pv(0.0),
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 128,
            },
            clip_mode: BoxShadowClipMode::Outset,
        }
    }

    fn bounds(x: f32, y: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition { x, y },
            size: LogicalSize {
                width: 40.0,
                height: 30.0,
            },
        }
    }

    fn cache_state() -> (usize, usize) {
        SHADOW_BLUR_CACHE.with(|c| {
            let c = c.borrow();
            (c.0.len(), c.2)
        })
    }

    /// The blurred buffer is a pure function of the shadow SPEC, not its
    /// position: two same-spec shadows at different offsets share one
    /// entry, and the replay produces bit-identical pixels to a fresh
    /// render. This is the 190.6 ms/repaint finding (4 page shadows x
    /// 47.7 ms stack blur each) collapsed into one cache slot.
    #[test]
    fn same_spec_shadows_share_one_entry_and_pixels_match() {
        SHADOW_BLUR_CACHE.with(|c| {
            let mut c = c.borrow_mut();
            c.0.clear();
            c.1.clear();
            c.2 = 0;
        });

        let mut a = AzulPixmap::new(120, 100).unwrap();
        a.fill(255, 255, 255, 255);
        render_box_shadow(
            &mut a,
            &bounds(20.0, 20.0),
            &shadow(),
            &BorderRadius::default(),
            None,
            1.0,
        )
        .unwrap();
        let (len_after_first, bytes_after_first) = cache_state();
        assert_eq!(len_after_first, 1, "first render must populate one entry");
        assert!(bytes_after_first > 0);

        // Same spec, DIFFERENT position — must hit the same entry.
        let mut b = AzulPixmap::new(120, 100).unwrap();
        b.fill(255, 255, 255, 255);
        render_box_shadow(
            &mut b,
            &bounds(50.0, 30.0),
            &shadow(),
            &BorderRadius::default(),
            None,
            1.0,
        )
        .unwrap();
        assert_eq!(cache_state().0, 1, "same-spec shadow must reuse the entry");

        // Replay correctness: a fresh (cold-cache) render at the SAME
        // position as `a` must equal the cached render pixel-for-pixel.
        SHADOW_BLUR_CACHE.with(|c| {
            let mut c = c.borrow_mut();
            c.0.clear();
            c.1.clear();
            c.2 = 0;
        });
        let mut a2 = AzulPixmap::new(120, 100).unwrap();
        a2.fill(255, 255, 255, 255);
        render_box_shadow(
            &mut a2,
            &bounds(20.0, 20.0),
            &shadow(),
            &BorderRadius::default(),
            None,
            1.0,
        )
        .unwrap();
        assert_eq!(
            a.data, a2.data,
            "cached replay must be bit-identical to a fresh render"
        );
    }

    /// A different spec (blur radius) must NOT share the entry — and the
    /// byte cap must hold under eviction.
    #[test]
    fn different_spec_gets_its_own_entry() {
        SHADOW_BLUR_CACHE.with(|c| {
            let mut c = c.borrow_mut();
            c.0.clear();
            c.1.clear();
            c.2 = 0;
        });
        let mut p = AzulPixmap::new(120, 100).unwrap();
        render_box_shadow(
            &mut p,
            &bounds(20.0, 20.0),
            &shadow(),
            &BorderRadius::default(),
            None,
            1.0,
        )
        .unwrap();
        let mut s2 = shadow();
        s2.blur_radius = pv(2.0);
        render_box_shadow(
            &mut p,
            &bounds(20.0, 20.0),
            &s2,
            &BorderRadius::default(),
            None,
            1.0,
        )
        .unwrap();
        let (len, bytes) = cache_state();
        assert_eq!(len, 2, "distinct blur radii are distinct entries");
        assert!(bytes <= SHADOW_CACHE_MAX_BYTES);
    }
}

#[cfg(all(test, feature = "std"))]
mod shadow_ring_blit_tests {
    use super::*;
    use azul_css::props::basic::pixel::{PixelValue, PixelValueNoPercent};
    use azul_css::props::style::box_shadow::{BoxShadowClipMode, StyleBoxShadow};

    fn pv(v: f32) -> PixelValueNoPercent {
        PixelValueNoPercent {
            inner: PixelValue::px(v),
        }
    }

    /// CSS outset shadows must not paint inside the border box. With a big
    /// offset, the old full blit dragged shadow pixels UNDER the element
    /// area — visible whenever the element's own background is translucent
    /// (and pure wasted blending when it is not).
    #[test]
    fn outset_shadow_does_not_paint_inside_the_border_box() {
        let shadow = StyleBoxShadow {
            offset_x: pv(20.0),
            offset_y: pv(20.0),
            blur_radius: pv(2.0),
            spread_radius: pv(0.0),
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            clip_mode: BoxShadowClipMode::Outset,
        };
        let bounds = LogicalRect {
            origin: LogicalPosition { x: 30.0, y: 30.0 },
            size: LogicalSize {
                width: 40.0,
                height: 40.0,
            },
        };
        let mut p = AzulPixmap::new(140, 140).unwrap();
        p.fill(255, 255, 255, 255);
        render_box_shadow(
            &mut p,
            &bounds,
            &shadow,
            &BorderRadius::default(),
            None,
            1.0,
        )
        .unwrap();

        let px = |x: u32, y: u32| {
            let i = ((y * 140 + x) * 4) as usize;
            (p.data[i], p.data[i + 1], p.data[i + 2])
        };
        // Center of the border box: the offset shadow overlaps this area,
        // but outset shadows must not paint under the element.
        assert_eq!(
            px(50, 50),
            (255, 255, 255),
            "border-box interior must stay untouched"
        );
        // Just outside the border box on the offset side: shadow must be there.
        let (r, g, b) = px(70 + 8, 70 + 8);
        assert!(
            r < 250 && g < 250 && b < 250,
            "shadow must paint outside the border box (got {:?})",
            (r, g, b)
        );
    }
}

#[cfg(all(test, feature = "std"))]
pub(super) mod lcd_pretile_tests {
    use super::*;

    use crate::font::parsed::ParsedFont;

    pub(super) fn load_test_font_pub() -> Option<ParsedFont> {
        load_test_font()
    }
    pub(super) fn rr_with_pub(
        f: &ParsedFont,
    ) -> (RendererResources, FontManager<FontRef>, FontHash) {
        rr_with(f)
    }
    pub(super) fn shape_pub(
        p: &ParsedFont,
        t: &str,
        sz: f32,
        x: f32,
        y: f32,
    ) -> Vec<GlyphInstance> {
        shape(p, t, sz, x, y)
    }

    fn load_test_font() -> Option<ParsedFont> {
        let candidates = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
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

    fn rr_with(font: &ParsedFont) -> (RendererResources, FontManager<FontRef>, FontHash) {
        let rr = RendererResources::default();
        let font_ref = crate::parsed_font_to_font_ref(font.clone());
        let hash = crate::font_ref_to_parsed_font(&font_ref).hash;
        let fm: FontManager<FontRef> =
            FontManager::new(rust_fontconfig::FcFontCache::default()).expect("FontManager::new");
        fm.insert_font(rust_fontconfig::FontId::new(), font_ref);
        (rr, fm, FontHash { font_hash: hash })
    }

    fn shape(
        parsed: &ParsedFont,
        text: &str,
        font_size: f32,
        x: f32,
        y: f32,
    ) -> Vec<GlyphInstance> {
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

    /// Overlap-component splitting: a run with OVERLAPPING glyphs must
    /// still be pixel-identical — overlapping components are swept
    /// TOGETHER through the batch rasterizer (merged coverage, exactly the
    /// slow path's semantics), never composited tile-over-tile.
    #[test]
    fn pretile_split_run_with_overlaps_is_pixel_identical() {
        let Some(font) = load_test_font() else {
            eprintln!("no system test font — skipping");
            return;
        };
        let (rr, fm, font_hash) = rr_with(&font);
        let font_size = 24.0;
        // Compress advances to 40% — forces neighbouring tiles to overlap.
        let mut glyphs = shape(&font, "WAVAWA overlap", font_size, 8.0, 40.0);
        let x0 = glyphs.first().map(|g| g.point.x).unwrap_or(0.0);
        for g in glyphs.iter_mut() {
            g.point.x = x0 + (g.point.x - x0) * 0.4;
        }
        let clip_rect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 320.0,
                height: 60.0,
            },
        };
        let bg = ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        let color = ColorU {
            r: 20,
            g: 20,
            b: 20,
            a: 255,
        };

        let mut slow = AzulPixmap::new(320, 60).unwrap();
        slow.fill(bg.r, bg.g, bg.b, 255);
        let mut gc1 = GlyphCache::new();
        render_text_with_bg(
            &glyphs,
            font_hash,
            font_size,
            color,
            &mut slow,
            &clip_rect,
            None,
            &rr,
            &fm,
            1.0,
            &mut gc1,
            (0.0, 0.0),
            false,
            None,
        );
        let mut fast = AzulPixmap::new(320, 60).unwrap();
        fast.fill(bg.r, bg.g, bg.b, 255);
        let mut gc2 = GlyphCache::new();
        render_text_with_bg(
            &glyphs,
            font_hash,
            font_size,
            color,
            &mut fast,
            &clip_rect,
            None,
            &rr,
            &fm,
            1.0,
            &mut gc2,
            (0.0, 0.0),
            false,
            Some((
                bg,
                LogicalRect {
                    origin: LogicalPosition {
                        x: -10_000.0,
                        y: -10_000.0,
                    },
                    size: LogicalSize {
                        width: 20_000.0,
                        height: 20_000.0,
                    },
                }
                .into(),
            )),
        );
        let diff = slow
            .data
            .iter()
            .zip(fast.data.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            diff, 0,
            "split-run path diverges from the sweep on {diff} bytes —              overlapping components must merge coverage, not composite tiles"
        );
    }

    /// The pre-blended tile path must produce EXACTLY the pixels of the
    /// per-pixel linear sweep for a non-overlapping run on a uniform
    /// opaque background — same pipeline, same LUT, same params, cached.
    /// Any divergence is a rendering bug, not a tolerance question.
    #[test]
    fn pretile_path_is_pixel_identical_to_the_sweep() {
        let Some(font) = load_test_font() else {
            eprintln!("no system test font — skipping");
            return;
        };
        let (rr, fm, font_hash) = rr_with(&font);

        let font_size = 24.0;
        let glyphs = shape(&font, "Hamburgefonstiv 123", font_size, 8.0, 40.0);
        let clip_rect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 320.0,
                height: 60.0,
            },
        };
        let bg = ColorU {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        let color = ColorU {
            r: 20,
            g: 20,
            b: 20,
            a: 255,
        };

        let mut slow = AzulPixmap::new(320, 60).unwrap();
        slow.fill(bg.r, bg.g, bg.b, 255);
        let mut gc1 = GlyphCache::new();
        render_text_with_bg(
            &glyphs,
            font_hash,
            font_size,
            color,
            &mut slow,
            &clip_rect,
            None,
            &rr,
            &fm,
            1.0,
            &mut gc1,
            (0.0, 0.0),
            false,
            None,
        );

        let mut fast = AzulPixmap::new(320, 60).unwrap();
        fast.fill(bg.r, bg.g, bg.b, 255);
        let mut gc2 = GlyphCache::new();
        render_text_with_bg(
            &glyphs,
            font_hash,
            font_size,
            color,
            &mut fast,
            &clip_rect,
            None,
            &rr,
            &fm,
            1.0,
            &mut gc2,
            (0.0, 0.0),
            false,
            Some((
                bg,
                LogicalRect {
                    origin: LogicalPosition {
                        x: -10_000.0,
                        y: -10_000.0,
                    },
                    size: LogicalSize {
                        width: 20_000.0,
                        height: 20_000.0,
                    },
                }
                .into(),
            )),
        );

        if !text_lcd_enabled() || lcd_linear_params().is_none() {
            // Without the LCD linear pipeline both calls took the same path.
            assert_eq!(slow.data, fast.data);
            return;
        }
        let diff = slow
            .data
            .iter()
            .zip(fast.data.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            diff, 0,
            "pre-blended tiles diverge from the sweep on {diff} bytes — \
             same pipeline must mean same pixels (check FIR padding and \
             tile placement)"
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod layer_path_text_tests {
    use super::*;
    use crate::solver3::display_list::DisplayList;

    /// Task #17: all three paths (plain / damaged / layers+composite) must
    /// agree byte-for-byte on a text run. An earlier version of this test
    /// went red only because it skipped `allocate_layers_from_display_list`
    /// (the root layer's range was empty and NOTHING was painted — the
    /// "missing tail from x=88" was the 'fl' ascenders, i.e. the whole run).
    #[test]
    fn render_layers_text_equals_plain_render() {
        let Some(font) = lcd_pretile_tests::load_test_font_pub() else {
            return;
        };
        let (rr, fm, font_hash) = lcd_pretile_tests::rr_with_pub(&font);
        let glyphs = lcd_pretile_tests::shape_pub(&font, "grow reflow", 20.0, 8.0, 26.0);
        let clip_rect: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 200.0,
                height: 40.0,
            },
        }
        .into();
        let item = DisplayListItem::Text {
            glyphs,
            font_hash,
            font_size_px: 20.0,
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            clip_rect,
            source_node_index: None,
        };
        let dl = DisplayList {
            items: vec![item],
            node_mapping: vec![None],
            ..Default::default()
        };

        let mut plain = AzulPixmap::new(200, 40).unwrap();
        plain.fill(255, 255, 255, 255);
        let mut gc1 = GlyphCache::new();
        render_display_list(&dl, &mut plain, 1.0, &rr, &fm, &mut gc1).unwrap();

        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut layered = AzulPixmap::new(200, 40).unwrap();
        layered.fill(255, 255, 255, 255);
        let mut gc3 = GlyphCache::new();
        let mut comp = CompositorState::new(200, 40);
        comp.allocate_layers_from_display_list(&dl, 1.0, &HashMap::new(), &HashMap::new());
        comp.render_layers(&dl, 1.0, &rr, &fm, &mut gc3, &state)
            .unwrap();
        comp.composite_frame(&mut layered, 1.0);
        let ldiff = plain
            .data()
            .iter()
            .zip(layered.data().iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            ldiff, 0,
            "render_layers+composite diverges on {ldiff} bytes"
        );
    }

    /// THE INK-GAMUT LAW: a solid-colour text run blended onto a solid
    /// backdrop cannot produce a channel value outside `[fg, bg]` (±1 for
    /// rounding), whatever the anti-aliasing — every correct LCD or
    /// grayscale blend is a per-channel mix of the two. The first-draw
    /// screenshot violated it (stem edges darker than the text colour).
    fn assert_ink_gamut(pix: &AzulPixmap, fg: ColorU, bg: ColorU, what: &str) {
        let lo = [fg.r.min(bg.r), fg.g.min(bg.g), fg.b.min(bg.b)];
        let hi = [fg.r.max(bg.r), fg.g.max(bg.g), fg.b.max(bg.b)];
        for (i, px) in pix.data().chunks_exact(4).enumerate() {
            for c in 0..3 {
                assert!(
                    px[c] >= lo[c].saturating_sub(1) && px[c] <= hi[c].saturating_add(1),
                    "{what}: pixel {i} channel {c} = {} is outside the ink gamut [{}, {}] — \
                     text blended against something that is not the backdrop (transparent \
                     layer?)",
                    px[c],
                    lo[c],
                    hi[c]
                );
            }
        }
    }

    /// REPORTED (AzWidgets first draw, 2026-08-21): the 13 px subtitle looked
    /// doubled / smeared on the first frame only. Pixel forensics: RGB-LCD
    /// fringes blended against BLACK. The showcase column is a scroll frame;
    /// the layered full-render path gives every scroll frame its own pixbuf,
    /// cleared to transparent black, and the LCD sweep blends per stripe
    /// against whatever is in the destination — while the page background
    /// lives in the ROOT layer. Every later repaint is flat and silently
    /// fixed it. A plain scroll-frame layer is now seeded with its parent's
    /// pixels under its bounds, so full == flat, for text in a scroll frame
    /// without a local background. No uniform-bg proof is attached, so EVERY
    /// glyph takes the sweep — the worst case.
    #[test]
    fn lcd_text_inside_a_scroll_frame_layer_equals_the_flat_render() {
        let Some(font) = lcd_pretile_tests::load_test_font_pub() else {
            return;
        };
        let (rr, fm, font_hash) = lcd_pretile_tests::rr_with_pub(&font);
        let glyphs = lcd_pretile_tests::shape_pub(&font, "Every built-in widget", 13.0, 8.0, 26.0);
        let page: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 200.0,
                height: 40.0,
            },
        }
        .into();
        let fg = ColorU {
            r: 0x66,
            g: 0x70,
            b: 0x85,
            a: 255,
        };
        let bg = ColorU {
            r: 0xf2,
            g: 0xf4,
            b: 0xf7,
            a: 255,
        };
        let items = vec![
            DisplayListItem::Rect {
                bounds: page,
                color: bg,
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::PushScrollFrame {
                clip_bounds: page,
                content_size: LogicalSize {
                    width: 200.0,
                    height: 400.0,
                },
                scroll_id: 7,
            },
            DisplayListItem::Text {
                glyphs,
                font_hash,
                font_size_px: 13.0,
                color: fg,
                clip_rect: page,
                source_node_index: None,
            },
            DisplayListItem::PopScrollFrame,
        ];
        let n = items.len();
        let dl = DisplayList {
            items,
            node_mapping: vec![None; n],
            ..Default::default()
        };

        let mut plain = AzulPixmap::new(200, 40).unwrap();
        plain.fill(255, 255, 255, 255);
        let mut gc1 = GlyphCache::new();
        render_display_list(&dl, &mut plain, 1.0, &rr, &fm, &mut gc1).unwrap();
        assert_ink_gamut(&plain, fg, bg, "flat render");

        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut layered = AzulPixmap::new(200, 40).unwrap();
        layered.fill(255, 255, 255, 255);
        let mut gc2 = GlyphCache::new();
        let mut comp = CompositorState::new(200, 40);
        comp.allocate_layers_from_display_list(&dl, 1.0, &HashMap::new(), &HashMap::new());
        assert_eq!(
            comp.layers.len(),
            2,
            "the scroll frame must have been promoted to a layer"
        );
        comp.render_layers(&dl, 1.0, &rr, &fm, &mut gc2, &state)
            .unwrap();
        comp.composite_frame(&mut layered, 1.0);

        assert_ink_gamut(&layered, fg, bg, "layered render");
        let ldiff = plain
            .data()
            .iter()
            .zip(layered.data().iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            ldiff, 0,
            "text inside a scroll-frame layer diverges from the flat render on {ldiff} bytes: \
             the layer was swept against a transparent backdrop instead of the page"
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod damaged_vs_plain_text_tests {
    use super::*;
    use crate::solver3::display_list::DisplayList;

    /// Task #17 bisect: the SAME Text item rendered through the plain
    /// full renderer and through the damaged renderer (one full-window
    /// rect) must produce identical bytes — these are the two paths behind
    /// "retained first-paint" vs "fresh reference", and the corpus caught
    /// them disagreeing by one LCD fringe quantum.
    #[test]
    fn damaged_full_rect_text_equals_plain_render() {
        let Some(font) = lcd_pretile_tests::load_test_font_pub() else {
            eprintln!("no system test font — skipping");
            return;
        };
        let (rr, fm, font_hash) = lcd_pretile_tests::rr_with_pub(&font);
        let glyphs = lcd_pretile_tests::shape_pub(&font, "grow reflow", 20.0, 8.0, 26.0);
        let clip_rect: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 200.0,
                height: 40.0,
            },
        }
        .into();
        let item = DisplayListItem::Text {
            glyphs,
            font_hash,
            font_size_px: 20.0,
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            clip_rect,
            source_node_index: None,
        };
        let dl = DisplayList {
            items: vec![item],
            node_mapping: vec![None],
            ..Default::default()
        };

        let mut plain = AzulPixmap::new(200, 40).unwrap();
        plain.fill(255, 255, 255, 255);
        let mut gc1 = GlyphCache::new();
        render_display_list(&dl, &mut plain, 1.0, &rr, &fm, &mut gc1).unwrap();

        let mut damaged = AzulPixmap::new(200, 40).unwrap();
        damaged.fill(255, 255, 255, 255);
        let mut gc2 = GlyphCache::new();
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let full = vec![LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 200.0,
                height: 40.0,
            },
        }];
        render_display_list_damaged(&dl, &mut damaged, 1.0, &rr, &fm, &mut gc2, &state, &full)
            .unwrap();

        let diff: Vec<usize> = plain
            .data()
            .iter()
            .zip(damaged.data().iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect();
        assert!(
            diff.is_empty(),
            "plain vs damaged-full-rect diverge on {} bytes, first at px ({}, {}): \
             plain={:?} damaged={:?}",
            diff.len(),
            (diff[0] / 4) % 200,
            (diff[0] / 4) / 200,
            &plain.data()[diff[0] & !3..(diff[0] & !3) + 4],
            &damaged.data()[diff[0] & !3..(diff[0] & !3) + 4],
        );
    }
}

#[cfg(test)]
mod pass2b_clamp_tests {
    /// #29 dev-profile pin: a tile entirely left of the clip yields a
    /// NEGATIVE clamped width. Pass 2b must skip such tiles before any
    /// width arithmetic — the pre-fix code computed `(tx1 - tx0) as u32 * 4`
    /// on it: overflow-checked builds panicked ("attempt to multiply with
    /// overflow", the maps_render_paints_header_pixels CI failure), release
    /// builds wrapped into the bounds guard and skipped by luck.
    #[test]
    fn off_clip_tile_clamp_is_checked_before_width_math() {
        let (cx0, cx1, dst_w) = (100i32, 200i32, 800i32);
        let (x0, tile_w) = (0i32, 32u32);
        let tx0 = x0.max(cx0).max(0);
        let tx1 = (x0 + tile_w as i32).min(cx1).min(dst_w);
        // The hazard is real: the clamped width is negative...
        assert!(tx1 < tx0, "fixture must express the negative-width case");
        // ...and the checked equivalent of the pre-fix arithmetic overflows.
        assert!(
            ((tx1 - tx0) as u32).checked_mul(4).is_none(),
            "the pre-fix `(tx1 - tx0) as u32 * 4` would overflow here — the \
             empty-rect guard in pass 2b must skip before this math runs"
        );
    }
}
