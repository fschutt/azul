use std::collections::HashMap;

use agg_rust::{
    basics::{FillingRule, PATH_FLAGS_NONE},
    blur::stack_blur_rgba32,
    color::Rgba8,
    conv_stroke::ConvStroke,
    gradient_lut::GradientLut,
    path_storage::PathStorage,
    pixfmt_rgba::PixfmtRgba32,
    rasterizer_scanline_aa::RasterizerScanlineAa,
    renderer_base::RendererBase,
    renderer_scanline::render_scanlines_aa_solid,
    rendering_buffer::RowAccessor,
    rounded_rect::RoundedRect,
    scanline_u::ScanlineU8,
    span_gradient::{GradientRadialD, GradientRepeatAdaptor, GradientX},
    trans_affine::TransAffine,
};
use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::{DecodedImage, ImageRef, RendererResources},
    ui_solver::GlyphInstance,
};
use azul_css::props::{
    basic::{pixel::DEFAULT_FONT_SIZE, ColorOrSystem, ColorU, FontRef},
    style::{box_shadow::StyleBoxShadow, filter::StyleFilter},
};

#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use super::*;
use crate::{
    font::parsed::ParsedFont,
    glyph_cache::GlyphCache,
    solver3::display_list::{
        BorderRadius, DisplayList, DisplayListItem, LocalScrollId,
    },
    text3::cache::{FontHash, FontManager},
};

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
#[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot
                                             // pixel/coord path or to avoid churning call sites for
                                             // a perf-neutral change)
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

/// The default gradient LUT resolution (agg's own default).
const GRADIENT_LUT_MIN: usize = 256;
/// The finest gradient LUT, and the longest gradient space (px) agg is handed.
/// agg measures gradient distances in 1/16 units and computes
/// `(d - d1) * lut_size` in `i32`: 4096 * 16 * 4096 is ~2.7e8, well inside.
const GRADIENT_LUT_MAX: usize = 4096;
/// How far the second stop of a hard stop is nudged past the first: agg's
/// `build_lut` keeps ONE stop per offset (closer than 1e-10), so the pair
/// must differ - by less than any LUT entry, so the change stays hard.
const HARD_STOP_EPSILON: f64 = 1e-9;

/// The LUT resolution for a gradient line `line_px` device pixels long: an
/// entry per pixel (a 3 px quote bar in a 600 px block is not smeared over
/// 256 entries), within `GRADIENT_LUT_MIN..=GRADIENT_LUT_MAX`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // finite, clamped
fn gradient_lut_size(line_px: f64) -> usize {
    if line_px.is_finite() && line_px > 0.0 {
        (line_px.ceil() as usize).clamp(GRADIENT_LUT_MIN, GRADIENT_LUT_MAX)
    } else {
        GRADIENT_LUT_MIN
    }
}

/// Build a `size`-entry `GradientLut` from `(offset, color)` stops - the one
/// LUT builder of every CPU gradient.
///
/// The stops are ordered by offset and cut to the line (`0.0..=1.0`) by the
/// shared [`color_stops_on_the_line`], so a stop beyond an end colors that
/// end by interpolation. A hard stop (two stops at one offset) keeps both
/// colors: agg would drop the second, which painted nothing at all for
/// `red 50%, blue 50%` (one stop left), so it is nudged past the first by
/// [`HARD_STOP_EPSILON`]. Fewer than two stops paint nothing.
///
/// Colours between two stops are mixed in premultiplied space (CSS Images 3,
/// what Chrome and `WebRender` paint): red fading to a transparent blue stays
/// red while it fades. agg mixes its stops straight, channel by channel, so
/// [`premultiplied_steps`] gives it a stop for every entry where the alpha
/// changes, and agg's straight steps between them are exact.
///
/// [`color_stops_on_the_line`]: azul_css::props::style::background::color_stops_on_the_line
fn build_gradient_lut(stops: &[(f32, ColorU)], size: usize) -> GradientLut {
    let size = size.clamp(GRADIENT_LUT_MIN, GRADIENT_LUT_MAX);
    let mut lut = GradientLut::new(size);
    let mut ordered: Vec<(f32, ColorU)> = stops
        .iter()
        .copied()
        .filter(|(t, _)| t.is_finite())
        .collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0)); // stable: a hard stop keeps its order
    if ordered.len() < 2 {
        lut.add_color(0.0, Rgba8::new(0, 0, 0, 0));
        lut.add_color(1.0, Rgba8::new(0, 0, 0, 0));
        lut.build_lut();
        return lut;
    }
    let on_line = azul_css::props::style::background::color_stops_on_the_line(
        &ordered,
        ColorU::interpolate_premultiplied,
    );
    let mut previous = f64::NEG_INFINITY;
    for (t, c) in premultiplied_steps(&on_line, size) {
        let mut offset = f64::from(t);
        if offset <= previous {
            offset = previous + HARD_STOP_EPSILON;
        }
        previous = offset;
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

/// `stops` (ordered, on the line) with a stop of its own for every one of
/// the `size` LUT entries inside a segment whose ends differ in alpha, its
/// colour mixed in premultiplied space. A segment of one alpha mixes the same
/// either way and is left to agg, and so is a hard stop (no entries between).
fn premultiplied_steps(stops: &[(f32, ColorU)], size: usize) -> Vec<(f32, ColorU)> {
    let mut out = Vec::with_capacity(stops.len());
    for (i, &(t, color)) in stops.iter().enumerate() {
        if let Some(&(from_t, from)) = i.checked_sub(1).and_then(|p| stops.get(p)) {
            let span = t - from_t;
            if from.a != color.a && span > 0.0 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                #[allow(clippy::cast_precision_loss)] // size <= GRADIENT_LUT_MAX
                let steps = ((span * size as f32).ceil() as usize).clamp(1, size);
                for k in 1..steps {
                    #[allow(clippy::cast_precision_loss)] // k < steps <= GRADIENT_LUT_MAX
                    let f = k as f32 / steps as f32;
                    out.push((
                        span.mul_add(f, from_t),
                        from.interpolate_premultiplied(color, f),
                    ));
                }
            }
        }
        out.push((t, color));
    }
    out
}

/// Build a `GradientLut` from the stops of a radial gradient, which sit at a
/// percentage of the gradient ray (the parser refuses radial stops at a
/// length).
fn build_gradient_lut_linear(
    stops: &azul_css::props::style::background::NormalizedLinearColorStopVec,
    system_colors: Option<&azul_css::system::SystemColors>,
) -> GradientLut {
    let stops: Vec<(f32, ColorU)> = stops
        .as_ref()
        .iter()
        .map(|stop| {
            (
                stop.offset.normalized(),
                resolve_color(&stop.color, system_colors),
            )
        })
        .collect();
    build_gradient_lut(&stops, GRADIENT_LUT_MIN)
}

/// Build a `GradientLut` from normalized radial (conic) color stops.
fn build_gradient_lut_radial(
    stops: &azul_css::props::style::background::NormalizedRadialColorStopVec,
    system_colors: Option<&azul_css::system::SystemColors>,
) -> GradientLut {
    let stops: Vec<(f32, ColorU)> = stops
        .as_ref()
        .iter()
        .map(|stop| {
            // Conic stops use angle — normalize to 0..1 fraction of full circle.
            // Use the RAW degrees (not `to_degrees()`, which wraps 360 -> 0): a
            // final 360deg stop is a meaningful, distinct offset of 1.0. Without
            // this, `conic-gradient(a, b)` (normalized to 0deg/360deg) collapses
            // both stops onto offset 0.0 and paints a single color. The clamp
            // keeps any out-of-range raw angle inside [0, 1].
            let offset = (stop.angle.to_degrees_raw() / 360.0).clamp(0.0, 1.0);
            (offset, resolve_color(&stop.color, system_colors))
        })
        .collect();
    build_gradient_lut(&stops, GRADIENT_LUT_MIN)
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

#[allow(clippy::suboptimal_flops)] // explicit origin + point * dpi; mul_add is no faster here
fn render_linear_gradient(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    gradient: &azul_css::props::style::background::LinearGradient,
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
    system_colors: Option<&azul_css::system::SystemColors>,
) {
    use azul_css::props::style::background::ExtendMode;

    let Some(rect) = logical_rect_to_az_rect(bounds, dpi_factor) else {
        return;
    };
    if gradient.stops.as_ref().is_empty() {
        return;
    }

    // The gradient line and every stop on it, from the resolver all
    // renderers share (CSS Images 3: `90deg` runs like `to right`, a stop at
    // a length sits at that length, a hard stop stays hard). It works in the
    // box's CSS px; the line is scaled to device px here.
    let dpi = f64::from(dpi_factor);
    // (x1, y1, x2, y2, length) of a resolved line in device px.
    let device_line = |r: &azul_css::props::style::background::ResolvedLinearGradient| {
        let x1 = f64::from(rect.x) + f64::from(r.start.0) * dpi;
        let y1 = f64::from(rect.y) + f64::from(r.start.1) * dpi;
        let x2 = f64::from(rect.x) + f64::from(r.end.0) * dpi;
        let y2 = f64::from(rect.y) + f64::from(r.end.1) * dpi;
        (x1, y1, x2, y2, (x2 - x1).hypot(y2 - y1))
    };
    let mut resolved = gradient.resolve_in_box(bounds.size.width, bounds.size.height);
    // `repeating-linear-gradient` repeats the first..last stop span. A period
    // under a pixel cannot repeat (agg's repeat divides by its length in
    // 1/16 px): it paints clamped instead.
    let mut repeating = false;
    if gradient.extend_mode == ExtendMode::Repeat {
        let period = resolved.to_repeat_period();
        if device_line(&period).4 >= 1.0 {
            resolved = period;
            repeating = true;
        }
    }
    let (x1, y1, x2, y2, len) = device_line(&resolved);
    if !len.is_finite() || len < 0.001 {
        return;
    }

    let stops: Vec<(f32, ColorU)> = resolved
        .stops
        .iter()
        .map(|(t, color)| (*t, resolve_color(color, system_colors)))
        .collect();
    let lut = build_gradient_lut(&stops, gradient_lut_size(len));

    // Gradient space runs 0..`d2` along the line (x1,y1)→(x2,y2): one unit
    // per device pixel, capped so agg's i32 distance math cannot overflow.
    // Use agg's helper so the composition order is T * R * S — hand-rolling
    // it via new_translation().rotate().scale() pre-multiplies and ends up as
    // S * R * T, which rotates the translation and yields out-of-range gx.
    #[allow(clippy::cast_precision_loss)] // GRADIENT_LUT_MAX is far below 2^52
    let d2 = len.min(GRADIENT_LUT_MAX as f64);
    let mut transform = TransAffine::new_line_segment(x1, y1, x2, y2, d2);
    transform.invert();

    let mut path = if border_radius.is_zero() {
        build_pixel_snapped_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    if repeating {
        agg_fill_gradient_clipped(
            pixmap,
            &mut path,
            &lut,
            GradientRepeatAdaptor::new(GradientX),
            transform,
            0.0,
            d2,
            clip,
        );
    } else {
        agg_fill_gradient_clipped(pixmap, &mut path, &lut, GradientX, transform, 0.0, d2, clip);
    }
}

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant
                                  // (or cross-type bindings that can't merge)
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

    let w = f64::from(rect.width);
    let h = f64::from(rect.height);

    // Compute center from position
    let (cx_frac, cy_frac) =
        resolve_background_position(&gradient.position, rect.width, rect.height);
    let cx = f64::from(rect.x) + f64::from(cx_frac) * w;
    let cy = f64::from(rect.y) + f64::from(cy_frac) * h;

    // The gradient's radii (CSS Images 3, 3.2.1). A circle has one. An ELLIPSE has one per
    // axis: to a side, the centre's distance to that side on each axis; to a corner, the
    // side's ellipse grown by sqrt(2) - the ellipse of the same proportions through the
    // corner. It was painted as a circle of the smaller (or larger) distance: the default
    // `radial-gradient(a, b)` - an ellipse to the farthest corner - on a box that is not
    // square was a circle (A1-51).
    let (near_x, far_x) = {
        let (left, right) = (f64::from(cx_frac) * w, (1.0 - f64::from(cx_frac)) * w);
        (left.min(right), left.max(right))
    };
    let (near_y, far_y) = {
        let (top, bottom) = (f64::from(cy_frac) * h, (1.0 - f64::from(cy_frac)) * h);
        (top.min(bottom), top.max(bottom))
    };
    let both = |r: f64| (r, r);
    let (rx, ry) = match (&gradient.shape, &gradient.size) {
        (Shape::Circle, RadialGradientSize::ClosestSide) => both(near_x.min(near_y)),
        (Shape::Circle, RadialGradientSize::FarthestSide) => both(far_x.max(far_y)),
        (Shape::Circle, RadialGradientSize::ClosestCorner) => both(near_x.hypot(near_y)),
        (Shape::Circle, RadialGradientSize::FarthestCorner) => both(far_x.hypot(far_y)),
        (Shape::Ellipse, RadialGradientSize::ClosestSide) => (near_x, near_y),
        (Shape::Ellipse, RadialGradientSize::FarthestSide) => (far_x, far_y),
        (Shape::Ellipse, RadialGradientSize::ClosestCorner) => {
            (near_x * core::f64::consts::SQRT_2, near_y * core::f64::consts::SQRT_2)
        }
        (Shape::Ellipse, RadialGradientSize::FarthestCorner) => {
            (far_x * core::f64::consts::SQRT_2, far_y * core::f64::consts::SQRT_2)
        }
    };

    if !(rx >= 0.001 && ry >= 0.001) {
        return;
    }

    let mut path = if border_radius.is_zero() {
        build_pixel_snapped_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    // `repeating-radial-gradient`: the span of the ray between the first and the last stop
    // repeats outwards and inwards (it was painted once and clamped, A1-52). The stops are
    // spread over that span, gradient space gets 100 units per period, and the distance is
    // counted from where the span begins. A period under a pixel cannot repeat: it paints
    // clamped, as a linear one does.
    if gradient.extend_mode == azul_css::props::style::background::ExtendMode::Repeat {
        let resolved: Vec<(f32, ColorU)> = stops
            .iter()
            .map(|stop| (stop.offset.normalized(), resolve_color(&stop.color, system_colors)))
            .filter(|(t, _)| t.is_finite())
            .collect();
        let first = resolved.iter().map(|(t, _)| *t).fold(f32::INFINITY, f32::min);
        let last = resolved.iter().map(|(t, _)| *t).fold(f32::NEG_INFINITY, f32::max);
        let span = f64::from(last - first);
        if span.is_finite() && span * rx.min(ry) >= 1.0 {
            let over_span: Vec<(f32, ColorU)> =
                resolved.iter().map(|(t, c)| ((t - first) / (last - first), *c)).collect();
            let lut = build_gradient_lut(&over_span, GRADIENT_LUT_MIN);
            let mut transform = TransAffine::new_scaling(rx * span / 100.0, ry * span / 100.0);
            transform.translate(cx, cy);
            transform.invert();
            // agg's gradient distances are in sixteenths of a unit.
            #[allow(clippy::cast_possible_truncation)] // a start beyond i32 saturates: far outside
            let start = (f64::from(first) / span * 100.0 * 16.0).round() as i32;
            agg_fill_gradient_clipped(
                pixmap,
                &mut path,
                &lut,
                RadialRepeatFrom { start },
                transform,
                0.0,
                100.0,
                clip,
            );
            return;
        }
    }

    let lut = build_gradient_lut_linear(&gradient.stops, system_colors);

    // Gradient-space (radius=100 at distance=100) → pixel-space around (cx, cy).
    // Build as T * S (scale first, then translate) so S only affects the radii.
    // scale() pre-multiplies so we must start from scaling matrix.
    let mut transform = TransAffine::new_scaling(rx / 100.0, ry / 100.0);
    transform.translate(cx, cy);
    transform.invert();

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

/// The distance from the centre counted from `start` and wrapped into one period (`d`, the
/// gradient's far end): a `repeating-radial-gradient` whose stops begin `start` sixteenths of a
/// unit out. `GradientRepeatAdaptor` wraps from the centre, which is right only for a first
/// stop at 0.
struct RadialRepeatFrom {
    start: i32,
}

impl agg_rust::span_gradient::GradientFunction for RadialRepeatFrom {
    fn calculate(&self, x: i32, y: i32, d: i32) -> i32 {
        use agg_rust::span_gradient::GradientFunction;
        if d <= 0 {
            return 0;
        }
        GradientRadialD.calculate(x, y, d).saturating_sub(self.start).rem_euclid(d)
    }
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

    // The stops run over the whole turn (CSS Images 4, 3.3: a stop at an angle `a` is at
    // `a / 360` of it); gradient space is 100 units a turn and the LUT maps 0..1 over that.
    // agg's `GradientConic` is `|atan2(y, x)| d / pi`: the ramp over HALF a turn, mirrored on
    // the other half. `conic-gradient(red, blue)` was red at 12 o'clock, blue at 6 and the
    // same purple at 3 and at 9 (A1-58, found by A3-solver).
    let d2 = 100.0;

    let mut path = if border_radius.is_zero() {
        build_pixel_snapped_rect_path(&rect)
    } else {
        build_rounded_rect_path(&rect, border_radius, dpi_factor)
    };

    // `repeating-conic-gradient`: the part of the turn between the first and the last stop
    // repeats (A1-59; `extend_mode` was not read). The LUT is built over that part.
    if gradient.extend_mode == azul_css::props::style::background::ExtendMode::Repeat {
        let resolved: Vec<(f32, ColorU)> = stops
            .iter()
            .map(|stop| {
                (
                    (stop.angle.to_degrees_raw() / 360.0).clamp(0.0, 1.0),
                    resolve_color(&stop.color, system_colors),
                )
            })
            .filter(|(t, _)| t.is_finite())
            .collect();
        let first = resolved.iter().map(|(t, _)| *t).fold(f32::INFINITY, f32::min);
        let last = resolved.iter().map(|(t, _)| *t).fold(f32::NEG_INFINITY, f32::max);
        // A part under a thousandth of the turn cannot repeat: it paints clamped.
        if (last - first).is_finite() && last - first >= 1.0e-3 {
            let over_part: Vec<(f32, ColorU)> =
                resolved.iter().map(|(t, c)| ((t - first) / (last - first), *c)).collect();
            let lut = build_gradient_lut(&over_part, GRADIENT_LUT_MIN);
            let turn = ConicTurn { repeat: Some((f64::from(first), f64::from(last - first))) };
            agg_fill_gradient_clipped(pixmap, &mut path, &lut, turn, transform, 0.0, d2, clip);
            return;
        }
    }

    let lut = build_gradient_lut_radial(&gradient.stops, system_colors);
    agg_fill_gradient_clipped(
        pixmap,
        &mut path,
        &lut,
        ConicTurn { repeat: None },
        transform,
        0.0,
        d2,
        clip,
    );
}

/// The angle of a point about the gradient's centre as a part of the whole turn, from the
/// local +x axis towards +y (after `render_conic_gradient`'s transform: from the gradient's
/// start angle, clockwise on screen), scaled to the gradient's far end `d`. With `repeat`
/// `(first, part)`: the angle counted from `first` and wrapped into `part` of a turn, as a
/// fraction of that part.
struct ConicTurn {
    repeat: Option<(f64, f64)>,
}

impl agg_rust::span_gradient::GradientFunction for ConicTurn {
    #[allow(clippy::cast_possible_truncation)] // 0 ..= d
    fn calculate(&self, x: i32, y: i32, d: i32) -> i32 {
        let mut turn = f64::from(y).atan2(f64::from(x)) / core::f64::consts::TAU;
        if turn < 0.0 {
            turn += 1.0;
        }
        let t = match self.repeat {
            Some((first, part)) if part > 0.0 => (turn - first).rem_euclid(part) / part,
            _ => turn,
        };
        (t * f64::from(d) + 0.5) as i32
    }
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

pub(crate) struct ShadowCacheEntry {
    data: std::sync::Arc<Vec<u8>>,
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

/// The blurred box shadows a thread keeps: by key, their order of use, their bytes.
pub(crate) type ShadowCache = (
    HashMap<u64, ShadowCacheEntry>,
    std::collections::VecDeque<u64>,
    usize,
);

/// The frame thread's blurred box shadows while the bands of a repaint run (`bands`), behind
/// a lock its band threads share: a band thread starts with no thread-locals of its own, and
/// one that blurred every shadow of its band again on every frame would lose what the cache
/// is for (as for the kept text halos, `text_halo::SharedCpuHalos`).
pub(crate) type SharedShadowCache = std::sync::Arc<std::sync::Mutex<ShadowCache>>;

thread_local! {
    /// Set on a thread while it paints a band: the frame's shared shadows, read and kept
    /// instead of this thread's own [`SHADOW_BLUR_CACHE`].
    static FRAME_SHADOW_CACHE: core::cell::RefCell<Option<SharedShadowCache>> =
        const { core::cell::RefCell::new(None) };
}

/// This thread's blurred box shadows, moved out to be shared with the band threads of the
/// frame it paints; [`restore_shadow_cache`] moves them back.
pub(crate) fn share_shadow_cache() -> SharedShadowCache {
    let cache = SHADOW_BLUR_CACHE.with(|c| core::mem::take(&mut *c.borrow_mut()));
    std::sync::Arc::new(std::sync::Mutex::new(cache))
}

/// Paint on this thread with the frame's shared box shadows (`Some`), or its own (`None`).
pub(crate) fn adopt_shadow_cache(shared: Option<SharedShadowCache>) {
    FRAME_SHADOW_CACHE.with(|frame| *frame.borrow_mut() = shared);
}

/// Move the shadows shared for a frame's bands back into this thread's own cache, with what
/// the bands kept.
pub(crate) fn restore_shadow_cache(shared: SharedShadowCache) {
    adopt_shadow_cache(None);
    if let Ok(lock) = std::sync::Arc::try_unwrap(shared) {
        let cache = lock.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
        SHADOW_BLUR_CACHE.with(|c| *c.borrow_mut() = cache);
    }
}

thread_local! {
    /// Set on a thread while it paints a band: the window's glyph cache, which every band
    /// reads and none writes while the bands run (`bands::paint_bands`). A band looks a
    /// glyph's cells up there first ([`GlyphCache::cached_cells`]) and builds into the cache
    /// of its own only what the window's lacks; the window's cache takes those back when the
    /// bands are done (`GlyphCache::absorb`).
    static SHARED_GLYPHS: core::cell::RefCell<Option<std::sync::Arc<GlyphCache>>> =
        const { core::cell::RefCell::new(None) };
}

/// Paint on this thread with the window's glyph cache to look cells up in (`Some`: in a band),
/// or with the cache it is handed alone (`None`). A band thread sets `None` before it ends, so
/// that the frame's thread is the only holder when it takes the cache back.
pub(crate) fn adopt_shared_glyphs(shared: Option<std::sync::Arc<GlyphCache>>) {
    SHARED_GLYPHS.with(|frame| *frame.borrow_mut() = shared);
}

/// The window's glyph cache while this thread paints a band; `None` outside bands, where the
/// cache a text function is handed IS the window's.
fn shared_glyphs() -> Option<std::sync::Arc<GlyphCache>> {
    SHARED_GLYPHS.with(|frame| frame.borrow().clone())
}

/// Count the glyph cell sets `glyph_cache` has built since it had built `before`
/// (`Counter::CellsBuilt`), on this thread: the cache is the window's, or in a band the band's
/// own, whose count nobody reads once the window's cache has absorbed it. The three glyph loops
/// are the only callers of the cell builders.
fn count_cells_built(glyph_cache: &GlyphCache, before: u64) {
    let built = glyph_cache.cells_built().saturating_sub(before);
    if built != 0 {
        crate::cpu_counters::count(crate::cpu_counters::Counter::CellsBuilt, built);
    }
}

/// The buffer kept under `key`, with its size, and `key` is the most recently used from here
/// on.
///
/// The order of use (`cache.1`) may hold a key at several places: its LAST one counts
/// ([`keep_shadow`] skips the others when it evicts). A use is then a push. It was a `retain`
/// over the whole order to take the key's old place out - every kept shadow visited for every
/// shadow of every pass, inside the lock the band threads share (1,600 small shadows kept:
/// 2.5 million visits a frame). The order is compacted to one place per key when it holds
/// more than twice the kept shadows and 64.
fn kept_shadow(cache: &mut ShadowCache, key: u64) -> Option<(std::sync::Arc<Vec<u8>>, u32, u32)> {
    let entry = cache.0.get(&key)?;
    let found = (std::sync::Arc::clone(&entry.data), entry.w, entry.h);
    if cache.1.back() != Some(&key) {
        cache.1.push_back(key);
        if cache.1.len() > 2 * cache.0.len() + 64 {
            let mut seen = std::collections::HashSet::with_capacity(cache.0.len());
            let mut last_places: Vec<u64> =
                cache.1.iter().rev().copied().filter(|k| seen.insert(*k)).collect();
            last_places.reverse();
            cache.1 = last_places.into();
        }
    }
    Some(found)
}

/// Keep `data` (`w` x `h` px) under `key`, as the most recently used; the least recently used
/// buffers go until it fits under [`SHADOW_CACHE_MAX_BYTES`], and a buffer larger than that
/// is not kept.
fn keep_shadow(cache: &mut ShadowCache, key: u64, data: &std::sync::Arc<Vec<u8>>, w: u32, h: u32) {
    let bytes = data.len();
    if bytes > SHADOW_CACHE_MAX_BYTES {
        // It can never fit: making room for it emptied the whole cache, on every draw of it.
        return;
    }
    if let Some(replaced) = cache.0.remove(&key) {
        cache.2 = cache.2.saturating_sub(replaced.data.len());
    }
    while cache.2 + bytes > SHADOW_CACHE_MAX_BYTES {
        let Some(oldest) = cache.1.pop_front() else {
            break;
        };
        // Used again since this place in the order: its later place decides.
        if cache.1.contains(&oldest) {
            continue;
        }
        if let Some(old) = cache.0.remove(&oldest) {
            cache.2 = cache.2.saturating_sub(old.data.len());
        }
    }
    if cache.2 + bytes <= SHADOW_CACHE_MAX_BYTES {
        cache.0.insert(key, ShadowCacheEntry { data: std::sync::Arc::clone(data), w, h });
        cache.1.push_back(key);
        cache.2 += bytes;
    }
}

/// The box-shadow cache this thread paints with: the frame's shared one in a band, else its own.
fn with_shadow_cache<R>(f: impl FnOnce(&mut ShadowCache) -> R) -> R {
    let shared = FRAME_SHADOW_CACHE.with(|frame| frame.borrow().clone());
    match shared {
        Some(shared) => {
            let mut cache = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            f(&mut cache)
        }
        None => SHADOW_BLUR_CACHE.with(|c| f(&mut c.borrow_mut())),
    }
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
    // The pixels a paint under the clip may write (`AzRect::pixel_box`, the one clip rule of
    // the CPU path, A2-cpu-06): the far edges were rounded UP here, so under a clip that ends
    // between two pixels a shadow wrote the column and the row no fill under that clip writes.
    let clip_px = clip.map(|c| c.pixel_box());
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

    // CSS Backgrounds 3 s7.2: an INNER shadow - `bounds` is then the padding
    // box (the display list hands it over, above the background) - is cast
    // inside the padding box only, as if everything outside the padding edge
    // were opaque: the padding box moved by the offset and shrunk by the
    // spread is the hole, the rest of the padding box is shadow. It used to
    // take the outer path below: a filled box at the offset.
    if matches!(shadow.clip_mode, BoxShadowClipMode::Inset) {
        let pad = blur_r.ceil() + 1.0;
        let buf_w = 2.0f32.mul_add(pad, rect.width).ceil();
        let buf_h = 2.0f32.mul_add(pad, rect.height).ceil();
        if !(buf_w > 0.0 && buf_h > 0.0) {
            return Ok(());
        }
        let (sw, sh) = (buf_w as u32, buf_h as u32);
        if sw == 0 || sh == 0 || sw > MAX_SHADOW_PIXBUF_SIZE || sh > MAX_SHADOW_PIXBUF_SIZE {
            return Ok(());
        }
        // The finished buffer - filled, blurred, cut to the padding box's curve - depends on
        // nothing but these (the box's PLACE is the blit's offset below), and was made again
        // on every draw: for every pass, and for every band the box reaches (A1-54).
        let inset_key = {
            use core::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            0x49_u8.hash(&mut h);
            (sw, sh).hash(&mut h);
            for v in [rect.width, rect.height, blur_r, spread, offset_x, offset_y, dpi_factor] {
                v.to_bits().hash(&mut h);
            }
            (color.r, color.g, color.b, color.a).hash(&mut h);
            for r in [
                border_radius.top_left,
                border_radius.top_right,
                border_radius.bottom_left,
                border_radius.bottom_right,
            ] {
                r.to_bits().hash(&mut h);
            }
            h.finish()
        };
        let kept = with_shadow_cache(|c| kept_shadow(c, inset_key));
        let finished = if let Some((data, _, _)) = kept {
            data
        } else {
        let mut tmp = AzulPixmap::new_transparent(sw, sh).ok_or("cannot create shadow pixmap")?;
        let agg_color = Rgba8::new(
            u32::from(color.r),
            u32::from(color.g),
            u32::from(color.b),
            u32::from(color.a),
        );
        let Some(whole) = AzRect::from_xywh(0.0, 0.0, buf_w, buf_h) else {
            return Ok(());
        };
        // Everything (even-odd) minus the hole.
        let mut path = build_rect_path(&whole);
        let hole_w = 2.0f32.mul_add(-spread, rect.width);
        let hole_h = 2.0f32.mul_add(-spread, rect.height);
        if hole_w > 0.0 && hole_h > 0.0 {
            if let Some(hole) = AzRect::from_xywh(
                pad + offset_x + spread,
                pad + offset_y + spread,
                hole_w,
                hole_h,
            ) {
                // The hole's corners shrink with the spread (logical radii,
                // device spread).
                let shrink = |r: f32| {
                    if r > 0.0 {
                        (r - spread / dpi_factor.max(f32::EPSILON)).max(0.0)
                    } else {
                        0.0
                    }
                };
                let hole_radius = BorderRadius {
                    top_left: shrink(border_radius.top_left),
                    top_right: shrink(border_radius.top_right),
                    bottom_left: shrink(border_radius.bottom_left),
                    bottom_right: shrink(border_radius.bottom_right),
                };
                let mut hole_path = build_rounded_rect_path(&hole, &hole_radius, dpi_factor);
                path.concat_path(&mut hole_path, 0);
            }
        }
        agg_fill_path(&mut tmp, &mut path, &agg_color, FillingRule::EvenOdd);
        if blur_r > 0.5 {
            let blur_radius = (blur_r.ceil() as u32).min(254);
            let stride = (sw * 4) as i32;
            let mut ra =
                unsafe { RowAccessor::new_with_buf(tmp.data_mut().as_mut_ptr(), sw, sh, stride) };
            stack_blur_rgba32(&mut ra, blur_radius, blur_radius);
        }
        // A rounded padding box masks the shadow to its curve (premultiplied:
        // every channel scales with the coverage).
        if !border_radius.is_zero() {
            let mut mask = AzulPixmap::new_transparent(sw, sh).ok_or("cannot create shadow mask")?;
            if let Some(padding_box) = AzRect::from_xywh(pad, pad, rect.width, rect.height) {
                let mut padding_path =
                    build_rounded_rect_path(&padding_box, border_radius, dpi_factor);
                agg_fill_path(
                    &mut mask,
                    &mut padding_path,
                    &Rgba8::new(255, 255, 255, 255),
                    FillingRule::NonZero,
                );
            }
            for (px, m) in tmp
                .data_mut()
                .chunks_exact_mut(4)
                .zip(mask.data().chunks_exact(4))
            {
                let coverage = u32::from(m[3]);
                for channel in px.iter_mut() {
                    *channel = ((u32::from(*channel) * coverage + 127) / 255) as u8;
                }
            }
        }
            let data = std::sync::Arc::new(tmp.data.into_vec());
            with_shadow_cache(|c| keep_shadow(c, inset_key, &data, sw, sh));
            data
        };
        // Never outside the padding box (nor the active clip).
        let padding_clip = (
            rect.x.floor() as i32,
            rect.y.floor() as i32,
            (rect.x + rect.width).ceil() as i32,
            (rect.y + rect.height).ceil() as i32,
        );
        let clip = match clip_px {
            Some((x0, y0, x1, y1)) => (
                x0.max(padding_clip.0),
                y0.max(padding_clip.1),
                x1.min(padding_clip.2),
                y1.min(padding_clip.3),
            ),
            None => padding_clip,
        };
        if clip.0 >= clip.2 || clip.1 >= clip.3 {
            return Ok(());
        }
        blit_clipped(
            pixmap,
            Some(clip),
            &finished,
            sw,
            sh,
            0,
            0,
            sw,
            sh,
            (rect.x - pad) as i32,
            (rect.y - pad) as i32,
        );
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

    let cached = with_shadow_cache(|c| kept_shadow(c, cache_key));

    let (shadow_data, sw, sh) = if let Some((data, w, h)) = cached {
        drop(crate::probe::Probe::span("shadow_cache_hit"));
        (data, w, h)
    } else {
        drop(crate::probe::Probe::span("shadow_cache_miss"));
        // Create temp buffer and draw the shadow shape into it
        let mut tmp = AzulPixmap::new_transparent(sw, sh).ok_or("cannot create shadow pixmap")?;

        // The shadow SHAPE is the border box grown by the spread on every
        // side (CSS Backgrounds 3 §7.1), placed after the blur padding. It
        // used to be drawn at the spread OFFSET but at the border box's own
        // size, so a spread never widened anything: the ring blit below cut
        // the box out and left only its 1px anti-seam sliver - a `0 0 0 6px`
        // ring painted as two hairlines.
        let Some(shape_rect) = AzRect::from_xywh(
            padding,
            padding,
            2.0f32.mul_add(spread, rect.width),
            2.0f32.mul_add(spread, rect.height),
        ) else {
            return Ok(());
        };
        // The shadow's corners follow the spread too: radius + spread, never
        // below zero (a square corner stays square). `spread` is in device
        // pixels, the radii in logical ones.
        let grow = |r: f32| {
            if r > 0.0 {
                (r + spread / dpi_factor.max(f32::EPSILON)).max(0.0)
            } else {
                0.0
            }
        };
        let border_radius = &BorderRadius {
            top_left: grow(border_radius.top_left),
            top_right: grow(border_radius.top_right),
            bottom_left: grow(border_radius.bottom_left),
            bottom_right: grow(border_radius.bottom_right),
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

        let data = std::sync::Arc::new(tmp.data.into_vec());
        with_shadow_cache(|c| keep_shadow(c, cache_key, &data, sw, sh));
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
        // Border-box hole in SOURCE coordinates. A box off the pixel grid
        // has an antialiased edge: shrink the hole by 1px on every side
        // (ceil origin, floor extent) so the ring keeps a sliver of shadow
        // UNDER that edge - an over-large hole would leave a visible seam. A
        // box ON the grid has no such edge, and the sliver would show
        // through a transparent box (CSS: never inside the border box; WPT
        // box-shadow-outset-without-border-radius-001): its hole is exact.
        let on_the_grid = [rect.x, rect.y, rect.width, rect.height]
            .iter()
            .all(|v| v.fract() == 0.0);
        let sliver = u32::from(!on_the_grid);
        let hole_x = (rect.x - shadow_x).max(0.0).ceil() as u32 + sliver;
        let hole_y = (rect.y - shadow_y).max(0.0).ceil() as u32 + sliver;
        let hole_r =
            ((rect.x + rect.width - shadow_x).floor() as i64 - i64::from(sliver)).max(0) as u32;
        let hole_b =
            ((rect.y + rect.height - shadow_y).floor() as i64 - i64::from(sliver)).max(0) as u32;
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
    // An outer shadow is drawn outside the border box only (CSS Backgrounds
    // 3, 7.1): a rounded box is cut out of it through its own curve, or a
    // translucent box would show its shadow through it.
    if matches!(shadow.clip_mode, BoxShadowClipMode::Outset) && !border_radius.is_zero() {
        // The shadow with the box cut out depends on the blurred buffer (`cache_key`) and on
        // where the box lies in it - which is the shadow's offset and the fraction of a pixel
        // the shadow starts at, not the box's place. It was made again on every draw (a mask
        // of the buffer's size rasterised, the buffer copied, every pixel multiplied): for
        // every pass and every band a rounded, shadowed box reaches (A1-54). A box that
        // stands still or moves by whole pixels finds it kept.
        let (box_x, box_y) = (rect.x - dst_x as f32, rect.y - dst_y as f32);
        let cut_key = {
            use core::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            0x52_u8.hash(&mut h);
            cache_key.hash(&mut h);
            box_x.to_bits().hash(&mut h);
            box_y.to_bits().hash(&mut h);
            h.finish()
        };
        if let Some((cut, _, _)) = with_shadow_cache(|c| kept_shadow(c, cut_key)) {
            blit_clipped(pixmap, clip_px, &cut, sw, sh, 0, 0, sw, sh, dst_x, dst_y);
            return Ok(());
        }
        let mut mask = AzulPixmap::new_transparent(sw, sh).ok_or("cannot create shadow mask")?;
        if let Some(border_box) = AzRect::from_xywh(
            box_x,
            box_y,
            rect.width,
            rect.height,
        ) {
            let mut box_path = build_rounded_rect_path(&border_box, border_radius, dpi_factor);
            agg_fill_path(
                &mut mask,
                &mut box_path,
                &Rgba8::new(255, 255, 255, 255),
                FillingRule::NonZero,
            );
        }
        // Premultiplied: every channel scales with what the box leaves.
        let mut outside_the_box = shadow_data.as_slice().to_vec();
        for (px, m) in outside_the_box
            .chunks_exact_mut(4)
            .zip(mask.data().chunks_exact(4))
        {
            let left = 255 - u32::from(m[3]);
            for channel in px.iter_mut() {
                *channel = ((u32::from(*channel) * left + 127) / 255) as u8;
            }
        }
        let outside_the_box = std::sync::Arc::new(outside_the_box);
        with_shadow_cache(|c| keep_shadow(c, cut_key, &outside_the_box, sw, sh));
        blit_clipped(
            pixmap,
            clip_px,
            &outside_the_box,
            sw,
            sh,
            0,
            0,
            sw,
            sh,
            dst_x,
            dst_y,
        );
        return Ok(());
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
    /// Opacity layer: an isolated group put onto `snapshot` (what was
    /// below it) at `opacity` with `blend` (`mix-blend-mode`).
    Opacity {
        snapshot: Vec<u8>,
        rect: AzRect,
        opacity: f32,
        blend: azul_css::props::style::effects::StyleMixBlendMode,
    },
    /// The rounded corners of a `PushClip` with a border radius
    /// (`overflow: hidden` on a `border-radius` box).
    ///
    /// The clip STACK is rectangles only, so the rectangle clips the bulk of
    /// the content; what it cannot express is the four quarter-circles. Each
    /// corner box is snapshotted when the clip is pushed and blended back on
    /// the matching `PopClip` through the rounded-rect coverage, which restores
    /// whatever sat outside the arc before the clipped content painted over it.
    RoundedClip {
        corners: Vec<RoundedCorner>,
        /// `clip_stack.len()` right after the push, so `PopClip` restores only
        /// the corners that belong to the clip it is popping.
        clip_depth: usize,
    },
    /// A `filter` group (`PushFilter`); `Some` when it is isolated (see
    /// [`FilterGroup`]), `None` when the push carries no work here. Every
    /// push has one, so a `PopFilter` always closes its own push.
    Filter(Option<Box<FilterGroup>>),
    /// A reference frame (`PushReferenceFrame`): how its content reaches the
    /// pixmap, so the `PopReferenceFrame` undoes exactly what its push did.
    /// Every push has one.
    Transform(ReferenceFrameGroup),
}

/// How the flat walk paints a reference frame's content (a CSS `transform`,
/// a drag, the animation channel's slide) - moved by the frame's live
/// matrix, as the layered compositor paints the frame's layer.
#[derive(Debug)]
pub enum ReferenceFrameGroup {
    /// The matrix is the identity: nothing was opened.
    Identity,
    /// A 2D translation, folded into the scroll-offset stack (the walk
    /// paints at `pos - offset`): one entry was pushed there. The items are
    /// painted moved and nothing is resampled.
    Translated,
    /// Any other matrix: an isolated group, see [`TransformGroup`].
    Isolated(Box<TransformGroup>),
}

/// A reference frame whose matrix is not a translation, painted in place.
///
/// The push opens a surface of its own the size of the frame's device-pixel
/// box (the compositor's layer for the frame is that box, too), and the
/// frame's content paints into it at its LAYOUT place, clipped to it. The pop
/// composites the surface through the matrix - about the frame's origin, as
/// the compositor places its layer - into the surface below, inside the clip
/// that was open around the push. The content never touches the pixels
/// below at its layout place: a box laid out past the window's edge and
/// turned into view keeps the part the window would have cut off.
#[derive(Debug, Clone, Copy)]
pub struct TransformGroup {
    width: u32,
    height: u32,
    /// Group pixel -> pixmap pixel, a column-vector homography (`Mat3`).
    matrix: Mat3,
    /// The clip open around the push, device pixels, half-open.
    outer_clip: Option<(i32, i32, i32, i32)>,
    /// Whether any of the group can land in the pixels below. A group that cannot has a
    /// one-pixel stand-in for a surface and composites nothing.
    lands: bool,
}

impl ReferenceFrameGroup {
    /// Opens the reference frame whose live matrix is `m`, around content
    /// laid out in `bounds` (device-logical: already moved by the scroll
    /// frames around it).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded pixel box
    fn begin(
        m: &[[f32; 4]; 4],
        bounds: &LogicalRect,
        extent: &LogicalRect,
        surfaces: &mut Vec<AzulPixmap>,
        dpi_factor: f32,
        clip_stack: &mut Vec<Option<AzRect>>,
        real_clip_stack: &mut Vec<Option<AzRect>>,
        scroll_offset_stack: &mut Vec<(f32, f32)>,
        below: (u32, u32),
        // (A10-06) The frame the shell shows as a layer (`CpuRenderState::promote`): an
        // isolated group whatever its matrix is, so that its picture is a surface.
        promoted: Option<PromoteFrame>,
    ) -> Self {
        let isolate = promoted.is_some();
        // As the layer builder places the frame: the identity paints where it
        // is laid out, a translation paints moved into the pixels below (the
        // compositor paints it in its parent layer, too), and any other
        // matrix is a layer there and an isolated group here.
        if !isolate && is_identity_2d(m) {
            return Self::Identity;
        }
        if let Some((tx, ty)) = translation_2d(m).filter(|_| !isolate) {
            let (sx, sy) = scroll_offset_stack.last().copied().unwrap_or((0.0, 0.0));
            scroll_offset_stack.push((sx - tx, sy - ty));
            return Self::Translated;
        }

        let GroupGeometry {
            x,
            y,
            width,
            height,
            matrix,
            outer_clip,
        } = GroupGeometry::of(
            m,
            bounds,
            extent,
            dpi_factor,
            clip_stack.last().copied().flatten(),
        );
        // Of the group's box, only the part that can land in the pixels below is painted: the
        // clip around the push, mapped back through the matrix (and the walk culls the items
        // outside it). Where the matrix cannot be inverted, or has a perspective part, all of it.
        // (A10-06) For the frame shown as a layer, as far around that part as the frame's
        // render state says (`PromoteFrame::paint_margin`; `retained_candidate` offers the
        // same box).
        let part = match promoted.and_then(|p| p.paint_margin) {
            Some(margin) => group_part_within(&matrix, outer_clip, below, (width, height), margin),
            None => group_visible_part(&matrix, outer_clip, below, (width, height)),
        };
        let lands = width > 0
            && height > 0
            && part.is_none_or(|(x0, y0, x1, y1)| x1 > x0 && y1 > y0);

        // The group's surface: its pixel (0, 0) is the box's device pixel
        // (x, y), so the walk paints at its position less the box's origin.
        // Transparent from the allocator (`new_transparent`): a page of it costs memory once
        // something is painted on it, and only the part that lands is painted.
        let surface: Option<AzulPixmap> = if lands {
            use crate::cpu_counters::{count, Counter};
            count(Counter::GroupsOpened, 1);
            count(Counter::GroupPx, u64::from(width) * u64::from(height));
            count(
                Counter::GroupPxPainted,
                part.map_or(u64::from(width) * u64::from(height), |(x0, y0, x1, y1)| {
                    u64::from((x1 - x0) as u32) * u64::from((y1 - y0) as u32)
                }),
            );
            AzulPixmap::new_transparent(width, height)
        } else {
            // A group whose box lands nowhere (a label turned outside the damage, the view or
            // the part of the picture a zoom shows) gets no surface of its size: one pixel
            // stands in, the clip pushed below denies every paint into it, and its pop
            // composites nothing. It used to be allocated, cleared and blitted like any other.
            crate::cpu_counters::count(crate::cpu_counters::Counter::GroupsEmpty, 1);
            AzulPixmap::new_transparent(1, 1)
        };
        surfaces.push(surface.unwrap_or_else(|| AzulPixmap {
            data: Vec::new().into(),
            width: 0,
            height: 0,
        }));
        let (sx, sy) = scroll_offset_stack.last().copied().unwrap_or((0.0, 0.0));
        scroll_offset_stack.push((sx + x as f32 / dpi_factor, sy + y as f32 / dpi_factor));

        // The content paints inside the group's box only, whatever was open
        // around it: that clip applies where the group lands, at the pop.
        let group_clip = AzRect::from_xywh(0.0, 0.0, width as f32, height as f32)
            .unwrap_or(AzRect::DENY_ALL);
        real_clip_stack.push(Some(if lands { group_clip } else { AzRect::DENY_ALL }));
        let painted_clip = match part {
            None => group_clip,
            Some((x0, y0, x1, y1)) => AzRect::from_xywh(
                x0 as f32,
                y0 as f32,
                (x1 - x0).max(0) as f32,
                (y1 - y0).max(0) as f32,
            )
            .unwrap_or(AzRect::DENY_ALL),
        };
        clip_stack.push(Some(painted_clip));

        Self::Isolated(Box::new(TransformGroup {
            width,
            height,
            matrix,
            outer_clip,
            lands,
        }))
    }

    /// Closes what [`Self::begin`] opened: the scroll-offset entry of a
    /// translation, or the isolated group - its surface composited through
    /// its matrix into the surface below (`pixmap` when no other is open).
    fn finish(
        self,
        pixmap: &mut AzulPixmap,
        surfaces: &mut Vec<AzulPixmap>,
        clip_stack: &mut Vec<Option<AzRect>>,
        real_clip_stack: &mut Vec<Option<AzRect>>,
        scroll_offset_stack: &mut Vec<(f32, f32)>,
    ) {
        drop(self.finish_keeping(
            pixmap,
            surfaces,
            clip_stack,
            real_clip_stack,
            scroll_offset_stack,
        ));
    }

    /// [`Self::finish`], handing back the surface of an isolated group after it was
    /// composited: the walk keeps the picture of a frame that may only be moved next frame
    /// (`retained`). `None` for a frame that had no surface.
    fn finish_keeping(
        self,
        pixmap: &mut AzulPixmap,
        surfaces: &mut Vec<AzulPixmap>,
        clip_stack: &mut Vec<Option<AzRect>>,
        real_clip_stack: &mut Vec<Option<AzRect>>,
        scroll_offset_stack: &mut Vec<(f32, f32)>,
    ) -> Option<AzulPixmap> {
        match self {
            Self::Identity => None,
            Self::Translated => {
                if scroll_offset_stack.len() > 1 {
                    scroll_offset_stack.pop();
                }
                None
            }
            Self::Isolated(group) => {
                if clip_stack.len() > 1 {
                    clip_stack.pop();
                }
                if real_clip_stack.len() > 1 {
                    real_clip_stack.pop();
                }
                if scroll_offset_stack.len() > 1 {
                    scroll_offset_stack.pop();
                }
                let surface = surfaces.pop()?;
                let below = match surfaces.last_mut() {
                    Some(below) => below,
                    None => pixmap,
                };
                group.finish(&surface, below);
                Some(surface)
            }
        }
    }
}

/// Widest or tallest box a [`TransformGroup`] sets aside, in device pixels
/// (a degenerate layout must not become a giant allocation).
const MAX_TRANSFORM_GROUP_SIDE: i32 = 16_384;

/// Where the surface of an isolated group lies and how it lands: everything
/// [`ReferenceFrameGroup::begin`] derives from the frame's matrix, box and
/// extent before it opens anything. Pure, so the walk can ask it for a frame
/// it is about to open (is a retained surface of this geometry kept?) and the
/// push that follows opens exactly that.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GroupGeometry {
    /// The device pixel of the surface below that the group's pixel (0, 0) is.
    pub(crate) x: i32,
    pub(crate) y: i32,
    /// The surface's size in device pixels.
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Group pixel -> pixel of the surface below, a column-vector homography (`Mat3`).
    pub(crate) matrix: Mat3,
    /// The clip open around the push, device pixels of the surface below, half-open.
    pub(crate) outer_clip: Option<(i32, i32, i32, i32)>,
}

impl GroupGeometry {
    /// The geometry of the group of the frame whose live matrix is `m`, around content laid
    /// out in `bounds` and painting `extent` (both device-logical: already moved by the scroll
    /// frames around the push), under `clip` (the innermost clip of the walk, device px).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded pixel box
    pub(crate) fn of(
        m: &[[f32; 4]; 4],
        bounds: &LogicalRect,
        extent: &LogicalRect,
        dpi_factor: f32,
        clip: Option<AzRect>,
    ) -> Self {
        // The surface covers what the frame paints, not only its box: a
        // shadow or an overflowing child turns with the frame.
        let x = (extent.origin.x * dpi_factor).floor() as i32;
        let y = (extent.origin.y * dpi_factor).floor() as i32;
        let x1 = ((extent.origin.x + extent.size.width) * dpi_factor).ceil() as i32;
        let y1 = ((extent.origin.y + extent.size.height) * dpi_factor).ceil() as i32;
        let width = x1.saturating_sub(x).clamp(0, MAX_TRANSFORM_GROUP_SIDE) as u32;
        let height = y1.saturating_sub(y).clamp(0, MAX_TRANSFORM_GROUP_SIDE) as u32;

        // The pixels the group may land on are the pixels a direct paint under the same
        // clip may write (`AzRect::pixel_box`): rounded to the nearest edge instead, a group
        // under a clip that starts at x = 62.5 began at column 63 where its content, painted
        // in place the frame before, began at column 62.
        let outer_clip = clip.map(|c| c.pixel_box());

        // Group pixel s -> pixmap: p = (x, y) + s, then about the frame's
        // origin o: o + M (p - o), M's translation in logical units scaled
        // to device pixels (the compositor's `layer_h`).
        let dpi = f64::from(dpi_factor);
        let (ox, oy) = (
            f64::from(bounds.origin.x) * dpi,
            f64::from(bounds.origin.y) * dpi,
        );
        let layer_h: Mat3 = [
            f64::from(m[0][0]),
            f64::from(m[1][0]),
            f64::from(m[3][0]) * dpi,
            f64::from(m[0][1]),
            f64::from(m[1][1]),
            f64::from(m[3][1]) * dpi,
            f64::from(m[0][3]) / dpi,
            f64::from(m[1][3]) / dpi,
            f64::from(m[3][3]),
        ];
        let matrix = mat3_mul(
            &mat3_translation(ox, oy),
            &mat3_mul(
                &layer_h,
                &mat3_translation(f64::from(x) - ox, f64::from(y) - oy),
            ),
        );
        Self {
            x,
            y,
            width,
            height,
            matrix,
            outer_clip,
        }
    }
}

impl TransformGroup {
    /// Composites the group's `surface` through its matrix into `pixmap`.
    fn finish(&self, surface: &AzulPixmap, pixmap: &mut AzulPixmap) {
        if self.width == 0 || self.height == 0 || !self.lands {
            return;
        }
        if mat3_is_affine(&self.matrix) {
            blit_pixmap_affine_clipped(
                surface,
                pixmap,
                &mat3_affine_part(&self.matrix),
                1.0,
                self.outer_clip,
            );
        } else {
            blit_pixmap_projective_clipped(surface, pixmap, &self.matrix, 1.0, self.outer_clip);
        }
    }
}

/// How far past the part of a group that can land the walk still paints, in group pixels: the
/// composite's bilinear taps (two) and what a content's own effects (a blur, a halo) reach into it.
const GROUP_VISIBLE_MARGIN: i32 = 64;

thread_local! {
    /// Tests: paint every item of a group, whatever lands (the way groups were painted before
    /// the walk culled inside them).
    #[cfg(test)]
    static GROUP_CULL_OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Tests: test every item of every level against the walk's cull, as before a level had
    /// a grid (`item_grid`).
    #[cfg(test)]
    static GRID_OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Tests: the items a walk on this thread hands to the painter (the list's address and
    /// the item's index), in order, pushes and pops left out; `None`: not recorded.
    #[cfg(test)]
    static PAINT_TRACE: std::cell::RefCell<Option<Vec<(usize, usize)>>> = const { std::cell::RefCell::new(None) };
}

/// Tests: the switches of this thread's walk, for the band threads it starts (a thread starts
/// with its own thread-locals): whether the walk culls by the clip, whether halos are kept,
/// whether the levels' grids are left unused.
#[cfg(test)]
pub(crate) fn walk_test_flags() -> (bool, bool, bool) {
    (
        GROUP_CULL_OFF.with(std::cell::Cell::get),
        super::text_halo::cpu_halos_are_kept(),
        GRID_OFF.with(std::cell::Cell::get),
    )
}

/// Tests: paint on this thread with the switches of [`walk_test_flags`].
#[cfg(test)]
pub(crate) fn set_walk_test_flags((cull_off, halos_kept, grid_off): (bool, bool, bool)) {
    GROUP_CULL_OFF.with(|c| c.set(cull_off));
    super::text_halo::keep_cpu_halos(halos_kept);
    GRID_OFF.with(|c| c.set(grid_off));
}

/// Does a walk ask the grids of a list's levels which children can paint (`item_grid`; the
/// default), or test every item (`AZ_CPU_GRID=off`)?
fn walk_uses_grids() -> bool {
    #[cfg(test)]
    if GRID_OFF.with(std::cell::Cell::get) {
        return false;
    }
    super::item_grid::grid_is_used()
}

/// The rect outside which a walk culls every direct child of the level it is entering, in the
/// coordinates the items are stored in, and whether it culls by its clip (`true`) or by its
/// damage rect; `None` when it culls nothing there, or not by a rect.
///
/// A level is the top of a list or the inside of one scope (`item_grid`), and the walk's state
/// is the same at all of its direct children, so this is asked once, when the walk enters.
/// The walk's own tests, per item (`walk_range`):
///
/// - inside an isolated frame, and in a view's list, by the innermost clip
///   (`outside_active_clip`): `bounds - offset`, widened by the text shadows' reach, against
///   `clip / dpi`; nothing is culled without a clip, or while a filter or a mask is open;
/// - else, under a damage rect: `bounds - offset`, widened the same, against the rect;
/// - else nothing.
///
/// Here the rect is moved instead of the bounds: `[x0 + ox - g, y0 + oy - g, x1 + ox + g,
/// y1 + oy + g]`, with `g` the reach of the text shadows open now, one px, and 1e-5 of the
/// largest coordinate (the two subtractions round apart by an ulp or two; 1e-5 is eighty).
/// What meets the walk's test meets this rect; the walk still makes its test on what it visits.
///
/// Example: a damage rect (40, 200) 100 x 30 at the offset (0, 1500) of a scrolled page, no
/// shadow: [38.98, 1698.98, 141.02, 1731.02] (g = 1 + 1730 x 1e-5 = 1.02).
fn level_cull_rect(
    cull: &WalkCull,
    mask_stack: &[MaskEntry],
    clip: Option<AzRect>,
    scroll_offset: (f32, f32),
    text_shadow_stack: &[StyleBoxShadow],
    dpi_factor: f32,
) -> Option<(super::item_grid::Box4, bool)> {
    // (Tests that paint "unculled" walk every item.)
    if !walk_culls_by_clip() {
        return None;
    }
    let by_clip = inside_isolated_transform(mask_stack) || matches!(cull, WalkCull::ActiveClip);
    let [x0, y0, x1, y1] = if by_clip {
        if !(dpi_factor > 0.0) {
            return None;
        }
        let clip = clip?;
        if mask_stack.iter().any(|e| match e {
            MaskEntry::Filter(_) => true,
            MaskEntry::ImageMask { width, height, .. } => *width > 0 && *height > 0,
            _ => false,
        }) {
            return None;
        }
        [
            clip.x / dpi_factor,
            clip.y / dpi_factor,
            clip.x / dpi_factor + clip.width / dpi_factor,
            clip.y / dpi_factor + clip.height / dpi_factor,
        ]
    } else if let WalkCull::Damage(damage) = cull {
        [
            damage.origin.x,
            damage.origin.y,
            damage.origin.x + damage.size.width,
            damage.origin.y + damage.size.height,
        ]
    } else {
        return None;
    };
    let (ox, oy) = scroll_offset;
    let moved = [x0 + ox, y0 + oy, x1 + ox, y1 + oy];
    let largest = moved
        .iter()
        .chain([x0, y0, x1, y1, ox, oy].iter())
        .fold(0.0_f32, |m, v| m.max(v.abs()));
    let grow = text_shadow_reach(text_shadow_stack).max(0.0) + 1.0 + largest * 1e-5;
    let rect = [moved[0] - grow, moved[1] - grow, moved[2] + grow, moved[3] + grow];
    rect.iter().all(|v| v.is_finite()).then_some((rect, by_clip))
}

/// Does the walk cull by the clip it paints under (always, outside tests)?
fn walk_culls_by_clip() -> bool {
    #[cfg(test)]
    {
        !GROUP_CULL_OFF.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        true
    }
}

/// The part of an isolated group's surface that can land in the pixels below it, as a box
/// `(x0, y0, x1, y1)` of group pixels, half-open, widened by [`GROUP_VISIBLE_MARGIN`] and cut to
/// the surface of `surface` pixels. `matrix` takes a group pixel to a pixel of the surface below,
/// `outer` is the clip open around the push in that surface's pixels and `below` its size. The
/// box is empty when nothing lands. `None` when the matrix has a perspective part or cannot be
/// inverted: the whole surface is kept.
fn group_visible_part(
    matrix: &Mat3,
    outer: Option<(i32, i32, i32, i32)>,
    below: (u32, u32),
    surface: (u32, u32),
) -> Option<(i32, i32, i32, i32)> {
    group_part_within(matrix, outer, below, surface, GROUP_VISIBLE_MARGIN)
}

/// [`group_visible_part`] widened by `margin` group pixels instead of [`GROUP_VISIBLE_MARGIN`]:
/// with the two pixels of the composite's bilinear taps it is the part of a kept surface a
/// composite READS (`retained`), with the full margin the part a walk PAINTS.
#[allow(clippy::cast_possible_truncation, clippy::many_single_char_names)]
pub(crate) fn group_part_within(
    matrix: &Mat3,
    outer: Option<(i32, i32, i32, i32)>,
    below: (u32, u32),
    surface: (u32, u32),
    margin: i32,
) -> Option<(i32, i32, i32, i32)> {
    if !walk_culls_by_clip() || !mat3_is_affine(matrix) {
        return None;
    }
    let (a, b, tx, c, d, ty) = (matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5]);
    let det = a * d - b * c;
    if !det.is_finite() || det.abs() < 1e-9 || ![tx, ty].iter().all(|v| v.is_finite()) {
        return None;
    }
    let (below_w, below_h) = (below.0 as i32, below.1 as i32);
    let (bx0, by0, bx1, by1) = match outer {
        Some((x0, y0, x1, y1)) => (x0.max(0), y0.max(0), x1.min(below_w), y1.min(below_h)),
        None => (0, 0, below_w, below_h),
    };
    if bx1 <= bx0 || by1 <= by0 {
        return Some((0, 0, 0, 0));
    }
    // The four corners of what lands, back to group pixels.
    let back = |px: i32, py: i32| {
        let (dx, dy) = (f64::from(px) - tx, f64::from(py) - ty);
        ((d * dx - b * dy) / det, (-c * dx + a * dy) / det)
    };
    let corners = [back(bx0, by0), back(bx1, by0), back(bx1, by1), back(bx0, by1)];
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (x, y) in corners {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
        return None;
    }
    let margin = f64::from(margin);
    let clamp = |v: f64, hi: u32| (v.clamp(-1.0e9, 1.0e9) as i32).clamp(0, hi as i32);
    Some((
        clamp((x0 - margin).floor(), surface.0),
        clamp((y0 - margin).floor(), surface.1),
        clamp((x1 + margin).ceil(), surface.0),
        clamp((y1 + margin).ceil(), surface.1),
    ))
}

/// How far past a text item's box the text shadows in effect can paint, logical px: the largest
/// of each shadow's offset plus its blur and its spread. What every cull of a text item under a
/// shadow widens the item's box by, and what the item diff widens a run's damage by when its
/// shadow changed (`compositor::compute_display_list_damage_impl`).
pub(super) fn text_shadow_reach(text_shadow_stack: &[StyleBoxShadow]) -> f32 {
    let length = |v: &azul_css::props::basic::pixel::PixelValue| {
        v.to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
    };
    text_shadow_stack
        .iter()
        .map(|sh| {
            length(&sh.offset_x.inner).abs().max(length(&sh.offset_y.inner).abs())
                + length(&sh.blur_radius.inner).max(0.0)
                + length(&sh.spread_radius.inner).max(0.0)
        })
        .fold(0.0_f32, f32::max)
}

/// Does `item` paint nothing under `clip`, the innermost clip the walk paints under (device px
/// of the surface being painted)? The items of a view or of an isolated group are laid out far
/// past the pixels they can land on, and every one used to be painted to a clip that cut it.
/// `scroll_offset` is the walk's offset: items are stored at layout coordinates and paint at
/// `pos - offset`. State items are never skipped (a push skipped under its pop corrupts the
/// stacks), nor is anything while a `filter` group is open (a blur reaches in from outside), and
/// a text shadow widens what an item can paint by its offset, blur and spread.
fn outside_active_clip(
    item: &DisplayListItem,
    text_shadow_stack: &[StyleBoxShadow],
    scroll_offset: (f32, f32),
    clip: Option<AzRect>,
    mask_stack: &[MaskEntry],
    dpi_factor: f32,
) -> bool {
    if !walk_culls_by_clip() || item.is_state_management() || dpi_factor <= 0.0 {
        return false;
    }
    let Some(clip) = clip else {
        return false;
    };
    // Nothing is painted under a clip of no area. A clip pushed outside the clip around it
    // leaves such a rect at the far corner of the two (`intersect_clips`: the larger near
    // edges, a width and height of 0), and the test below - closed, so that what touches the
    // clip is painted - took an item that happened to hold that corner for one that paints: it
    // was handed to the painter, which drew nothing of it (A1-65: found by the grids' test,
    // which steps over such an item).
    if !(clip.width > 0.0 && clip.height > 0.0) {
        return true;
    }
    // (A mask that reaches no pixel of the pixmap leaves an empty entry: it holds nothing back.)
    if mask_stack.iter().any(|e| match e {
        MaskEntry::Filter(_) => true,
        MaskEntry::ImageMask { width, height, .. } => *width > 0 && *height > 0,
        _ => false,
    }) {
        return false;
    }
    let bounds = if text_shadow_stack.is_empty() {
        item.visual_bounds().or_else(|| item.bounds())
    } else {
        item.bounds()
    };
    let Some(bounds) = bounds else {
        return false;
    };
    let reach = text_shadow_reach(text_shadow_stack);
    let on_surface = LogicalRect {
        origin: LogicalPosition {
            x: bounds.origin.x - scroll_offset.0 - reach,
            y: bounds.origin.y - scroll_offset.1 - reach,
        },
        size: LogicalSize {
            width: bounds.size.width + 2.0 * reach,
            height: bounds.size.height + 2.0 * reach,
        },
    };
    let clip_logical = LogicalRect {
        origin: LogicalPosition { x: clip.x / dpi_factor, y: clip.y / dpi_factor },
        size: LogicalSize { width: clip.width / dpi_factor, height: clip.height / dpi_factor },
    };
    !rects_overlap_or_adjacent(&on_surface, &clip_logical, 0.0)
}

/// Is the walk inside an isolated reference frame ([`TransformGroup`])?
/// Its items paint at their layout place and are moved at the pop, so the
/// damaged walk cannot cull them by where they are laid out.
fn inside_isolated_transform(mask_stack: &[MaskEntry]) -> bool {
    mask_stack.iter().any(|entry| {
        matches!(
            entry,
            MaskEntry::Transform(ReferenceFrameGroup::Isolated(_))
        )
    })
}

/// Is the walk inside a group whose pixels start transparent and are blended into the pixels
/// below at its pop: an opacity group that fades or blends, or an isolated `filter` group?
/// Text in it is grayscale wherever its run is cut (see the `Text` arm).
fn inside_blended_group(mask_stack: &[MaskEntry]) -> bool {
    use azul_css::props::style::effects::StyleMixBlendMode;
    mask_stack.iter().any(|entry| match entry {
        MaskEntry::Opacity { opacity, blend, .. } => {
            *opacity < 1.0 || !matches!(blend, StyleMixBlendMode::Normal)
        }
        MaskEntry::Filter(group) => group.is_some(),
        _ => false,
    })
}

/// Does a `filter` list need its group's own pixels apart from the backdrop?
/// `flood()` fills the group and `composite()` reads the group's own alpha,
/// so both do; the colour filters that act per pixel do not.
///
/// Only a list made of those two alone: a list that also holds a filter the walk does not
/// apply (a blur) is the layered compositor's whole (`filters_need_a_layer`), which applies
/// every filter of it to the group's layer - isolating it here as well flooded the parent
/// under that layer a second time.
fn filter_group_needs_isolation(filters: &[StyleFilter]) -> bool {
    !filters_need_a_layer(filters)
        && filters
            .iter()
            .any(|f| matches!(f, StyleFilter::Flood(_) | StyleFilter::Composite(_)))
}

/// Does a `filter` list hold a filter the flat walk does not apply: anything but `flood()`
/// and `composite()`? Such a group is painted through a layer of the layered compositor,
/// which applies the list when the layer is composited (`apply_layer_filters`).
#[must_use]
pub(crate) fn filters_need_a_layer(filters: &[StyleFilter]) -> bool {
    filters
        .iter()
        .any(|f| !matches!(f, StyleFilter::Flood(_) | StyleFilter::Composite(_)))
}

/// An isolated `filter` group of the direct (layer-less) render path.
///
/// [`Self::begin`] sets the pixels under the group aside and clears them to
/// transparent, so the group paints onto nothing; [`Self::finish`] filters
/// what the group painted (the SOURCE graphic of `composite()`) and
/// composites the result back over the saved backdrop.
#[derive(Debug)]
pub struct FilterGroup {
    backdrop: Vec<u8>,
    filters: Vec<StyleFilter>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl FilterGroup {
    /// Set aside and clear the device-pixel region `r` covers (its bounding
    /// box: the near edges floored, the far ones ceiled).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded pixel box
    fn begin(pixmap: &mut AzulPixmap, r: &AzRect, filters: &[StyleFilter]) -> Self {
        let x = r.x.floor() as i32;
        let y = r.y.floor() as i32;
        let width = ((r.x + r.width).ceil() as i32).saturating_sub(x).max(0) as u32;
        let height = ((r.y + r.height).ceil() as i32).saturating_sub(y).max(0) as u32;
        let backdrop = snapshot_region(pixmap, x, y, width, height);
        let clear = vec![0u8; backdrop.len()];
        write_region(pixmap, &clear, width, height, x, y);
        Self {
            backdrop,
            filters: filters.to_vec(),
            x,
            y,
            width,
            height,
        }
    }

    /// Filter what the group painted and composite it over the backdrop.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // colour math
    fn finish(&self, pixmap: &mut AzulPixmap, dpi_factor: f32) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        let painted = snapshot_region(pixmap, self.x, self.y, self.width, self.height);
        let mut group = AzulPixmap {
            data: painted.into(),
            width: self.width,
            height: self.height,
        };
        apply_layer_filters(&mut group, &self.filters, dpi_factor);

        // straight-alpha source-over: the filtered group over the backdrop
        let mut out = self.backdrop.clone();
        for (dst, src) in out.chunks_exact_mut(4).zip(group.data.chunks_exact(4)) {
            let sa = f32::from(src[3]) / 255.0;
            let da = f32::from(dst[3]) / 255.0;
            let oa = sa + da * (1.0 - sa);
            if oa <= 0.0 {
                dst.copy_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            for c in 0..3 {
                let v = (f32::from(src[c]) * sa + f32::from(dst[c]) * da * (1.0 - sa)) / oa;
                dst[c] = v.round().clamp(0.0, 255.0) as u8;
            }
            dst[3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        write_region(pixmap, &out, self.width, self.height, self.x, self.y);
    }
}

/// One corner box of a [`MaskEntry::RoundedClip`], in device pixels.
#[derive(Debug)]
pub struct RoundedCorner {
    snapshot: Vec<u8>,
    /// Coverage of the rounded rectangle: 255 inside, 0 outside the arc.
    mask: Vec<u8>,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

/// Build the corner masks for a rounded clip over `rect` (device pixels).
///
/// Only the part of each corner box beyond the arc's centre is curved; the
/// rest of the box is fully inside the rounded rectangle and keeps coverage
/// 255. A one-pixel analytic edge (`r + 0.5 - d`) anti-aliases the arc.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
fn rounded_clip_corners(
    pixmap: &AzulPixmap,
    rect: AzRect,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> Vec<RoundedCorner> {
    let (x0, y0) = (rect.x, rect.y);
    let (x1, y1) = (rect.x + rect.width, rect.y + rect.height);
    // CSS clamps radii that would overlap; half the short side is the bound.
    let max_r = (rect.width.min(rect.height) / 2.0).max(0.0);
    let spec = [
        (border_radius.top_left, true, true),
        (border_radius.top_right, false, true),
        (border_radius.bottom_left, true, false),
        (border_radius.bottom_right, false, false),
    ];
    let mut corners = Vec::new();
    for (radius, left, top) in spec {
        let r = (radius * dpi_factor).min(max_r);
        // Not positive, or NaN from a degenerate radius: nothing to round.
        if r.is_nan() || r <= 0.0 {
            continue;
        }
        let cx = if left { x0 + r } else { x1 - r };
        let cy = if top { y0 + r } else { y1 - r };
        let bx0 = if left { x0.floor() } else { cx.floor() };
        let bx1 = if left { cx.ceil() } else { x1.ceil() };
        let by0 = if top { y0.floor() } else { cy.floor() };
        let by1 = if top { cy.ceil() } else { y1.ceil() };
        let w = (bx1 - bx0).max(0.0) as u32;
        let h = (by1 - by0).max(0.0) as u32;
        if w == 0 || h == 0 {
            continue;
        }
        let mut mask = vec![255u8; (w as usize) * (h as usize)];
        for j in 0..h {
            for i in 0..w {
                let px = bx0 + i as f32 + 0.5;
                let py = by0 + j as f32 + 0.5;
                let beyond_x = if left { px < cx } else { px > cx };
                let beyond_y = if top { py < cy } else { py > cy };
                if beyond_x && beyond_y {
                    let d = (px - cx).hypot(py - cy);
                    let coverage = (r + 0.5 - d).clamp(0.0, 1.0);
                    mask[(j * w + i) as usize] = (coverage * 255.0).round() as u8;
                }
            }
        }
        let (bx, by) = (bx0 as i32, by0 as i32);
        corners.push(RoundedCorner {
            snapshot: snapshot_region(pixmap, bx, by, w, h),
            mask,
            x: bx,
            y: by,
            w,
            h,
        });
    }
    corners
}

/// Extract and scale mask image data (R8) to target dimensions.
fn extract_mask_data(mask_image: &ImageRef, target_w: u32, target_h: u32) -> Option<Vec<u8>> {
    extract_mask_window(mask_image, target_w, target_h, (0, 0, target_w, target_h))
}

/// The window `(x, y, w, h)` of the mask image scaled to `target_w` x
/// `target_h`, `w * h` bytes: only the part of a large mask rect a region
/// sees is resampled.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // software rasterizer: bounded pixel/coord/colour casts
fn extract_mask_window(
    mask_image: &ImageRef,
    target_w: u32,
    target_h: u32,
    (win_x, win_y, win_w, win_h): (u32, u32, u32, u32),
) -> Option<Vec<u8>> {
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

    if target_w == 0 || target_h == 0 || win_w == 0 || win_h == 0 {
        return None;
    }

    // BILINEAR, not nearest. The mask is rasterised in LOGICAL pixels and
    // applied in DEVICE pixels, so on any HiDPI display (or any zoom) it is
    // upscaled - and nearest-neighbour turns the antialiased edge the
    // rasteriser worked out into hard staircase steps. A circle came out
    // visibly blocky: the coverage values were right in the source and thrown
    // away on the way to the screen.
    //
    // 1:1 is the common case and still costs one sample per pixel, because
    // both weights land on 0.
    let mut scaled = vec![0u8; (win_w as usize) * (win_h as usize)];
    let sx = src_w as f32 / target_w as f32;
    let sy = src_h as f32 / target_h as f32;
    let sample = |x: u32, y: u32| -> f32 {
        f32::from(mask_bytes[(y.min(src_h - 1) * src_w + x.min(src_w - 1)) as usize])
    };
    for (row, py) in (win_y..win_y.saturating_add(win_h)).enumerate() {
        // Sample at the pixel CENTRE, or the image shifts half a texel.
        let fy = ((py as f32 + 0.5) * sy - 0.5).max(0.0);
        let y0 = fy as u32;
        let wy = fy - y0 as f32;
        let out = &mut scaled[row * win_w as usize..(row + 1) * win_w as usize];
        for (slot, px) in out.iter_mut().zip(win_x..win_x.saturating_add(win_w)) {
            let fx = ((px as f32 + 0.5) * sx - 0.5).max(0.0);
            let x0 = fx as u32;
            let wx = fx - x0 as f32;
            let top = sample(x0, y0) * (1.0 - wx) + sample(x0 + 1, y0) * wx;
            let bottom = sample(x0, y0 + 1) * (1.0 - wx) + sample(x0 + 1, y0 + 1) * wx;
            let value = top * (1.0 - wy) + bottom * wy;
            *slot = value.round().clamp(0.0, 255.0) as u8;
        }
    }
    Some(scaled)
}

/// The box `(x, y, w, h)` in device pixels cut to the pixmap's area; empty
/// (`w` or `h` 0) when they do not meet.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // inside the pixmap
fn on_pixmap((x, y, w, h): (i32, i32, u32, u32), pixmap: &AzulPixmap) -> (i32, i32, u32, u32) {
    let x0 = i64::from(x).max(0);
    let y0 = i64::from(y).max(0);
    let x1 = (i64::from(x) + i64::from(w)).min(i64::from(pixmap.width));
    let y1 = (i64::from(y) + i64::from(h)).min(i64::from(pixmap.height));
    if x1 <= x0 || y1 <= y0 {
        return (0, 0, 0, 0);
    }
    (x0 as i32, y0 as i32, (x1 - x0) as u32, (y1 - y0) as u32)
}

/// The part of the mask rect `mask` inside `region` (device-pixel boxes),
/// in the mask rect's own pixels; `None` when they do not meet.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // inside the mask rect
fn mask_window(
    (mask_x, mask_y, mask_w, mask_h): (i32, i32, u32, u32),
    (region_x, region_y, region_w, region_h): (i32, i32, u32, u32),
) -> Option<(u32, u32, u32, u32)> {
    let x0 = (i64::from(region_x) - i64::from(mask_x)).max(0);
    let y0 = (i64::from(region_y) - i64::from(mask_y)).max(0);
    let x1 = (i64::from(region_x) + i64::from(region_w) - i64::from(mask_x)).min(i64::from(mask_w));
    let y1 = (i64::from(region_y) + i64::from(region_h) - i64::from(mask_y)).min(i64::from(mask_h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

/// Apply a mask: for each pixel in the mask region, blend between the snapshot
/// (pre-mask state) and the current pixmap state using the mask value.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)] // software rasterizer: bounded pixel/coord/colour casts
fn apply_mask(pixmap: &mut AzulPixmap, entry: &MaskEntry) {
    match entry {
        MaskEntry::ImageMask {
            snapshot,
            mask_data,
            origin_x,
            origin_y,
            width,
            height,
        } => blend_masked_region(
            pixmap,
            snapshot,
            mask_data,
            *origin_x,
            *origin_y,
            *width,
            *height,
        ),
        MaskEntry::RoundedClip { corners, .. } => {
            for c in corners {
                blend_masked_region(pixmap, &c.snapshot, &c.mask, c.x, c.y, c.w, c.h);
            }
        }
        MaskEntry::Opacity { .. } | MaskEntry::Filter(_) | MaskEntry::Transform(_) => {}
    }
}

/// `result = snapshot * (255 - mask) + current * mask` over one region.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)]
fn blend_masked_region(
    pixmap: &mut AzulPixmap,
    snapshot: &[u8],
    mask_data: &[u8],
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
) {
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
    /// Colour values keyed by `ColorKey.id`: the backgrounds whose colour
    /// fades, bound by `DisplayList::color_bindings` - what `WebRender`
    /// gets as its colour bindings.
    pub colors: HashMap<usize, ColorU>,
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
    /// The most threads a repaint may paint its bands on (`bands`): `0` is the default
    /// (`bands::max_threads`), `1` paints every damage rect on the calling thread as one pass.
    /// The picture does not depend on it.
    pub paint_threads: usize,
    /// The group surfaces kept from frame to frame (`retained`): the picture of a turned or
    /// scaled reference frame whose content did not change is composited again, not painted
    /// again. `None` (a one-off render, a test of the walk) keeps nothing.
    pub retained: Option<std::sync::Arc<RetainedFrames>>,
    /// The window's own list, when the caller holds it shared: a kept surface holds the list
    /// it was painted from, and a walk is handed a `&DisplayList`. (A view's list is found in
    /// `virtual_view_display_lists`.) `None`: the frames of the window's own list are not kept.
    pub root_display_list: Option<std::sync::Arc<DisplayList>>,
    /// (A10-06) The reference frame the shell shows as a layer of its own ([`PromoteFrame`]);
    /// `None` (every caller but a backend asked for the layer present): no frame is treated
    /// differently and every pixel is the one painted before this field existed.
    pub promote: Option<PromoteFrame>,
}

/// A reference frame the shell shows as a layer of its own (A10-06, `present_layer`): the
/// frame whose push carries the transform key `transform_key`.
///
/// The walk paints it as an isolated group whatever its matrix is - also the identity and a
/// translation, which it otherwise paints in place - so that its picture is a kept surface
/// (`retained`). With `composite` the walk puts that surface into the pixels below it, as it
/// does for every group; without, it leaves those pixels as what is painted BEFORE the frame
/// left them, and the shell composites the surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromoteFrame {
    /// `TransformKey::id` of the frame's push.
    pub transform_key: usize,
    /// Whether the walk composites the frame's surface into the pixels below.
    pub composite: bool,
    /// How far around the part of the surface that lands a pass paints, surface px; `None`:
    /// as for every group (64 px). A frame that completes a surface strip by strip (a pan
    /// of a layer) paints further ahead: every strip is one more copy of the surface handed
    /// to the window system.
    pub paint_margin: Option<i32>,
}

impl CpuRenderState {
    #[must_use]
    pub fn new(scroll_offsets: ScrollOffsetMap) -> Self {
        Self {
            scroll_offsets,
            transforms: HashMap::new(),
            opacities: HashMap::new(),
            colors: HashMap::new(),
            system_style: None,
            clear_color: [255, 255, 255, 255],
            virtual_view_display_lists: std::collections::BTreeMap::new(),
            paint_threads: 0,
            retained: None,
            root_display_list: None,
            promote: None,
        }
    }

    /// The window's own list, shared (see the field).
    #[must_use]
    pub fn with_root_display_list(mut self, list: std::sync::Arc<DisplayList>) -> Self {
        self.root_display_list = Some(list);
        self
    }

    /// Paint a repaint's bands on at most `threads` threads (see the field).
    #[must_use]
    pub const fn with_paint_threads(mut self, threads: usize) -> Self {
        self.paint_threads = threads;
        self
    }

    /// Keep group surfaces from frame to frame in `retained` (see the field).
    #[must_use]
    pub fn with_retained_frames(mut self, retained: std::sync::Arc<RetainedFrames>) -> Self {
        self.retained = Some(retained);
        self
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

    /// Paint with `values`: the transforms, opacities and colours of every DOM of the
    /// window ([`WindowGpuValues`]), whichever list a key is bound in.
    #[must_use]
    pub fn with_gpu_values(mut self, values: WindowGpuValues) -> Self {
        self.transforms = values.transforms;
        self.opacities = values.opacities;
        self.colors = values.colors;
        self
    }

    /// Build from a `GpuValueCache` snapshot (ONE DOM's: a window paints with
    /// [`Self::with_gpu_values`]).
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
            colors: extract_gpu_colors(gpu_cache),
            system_style: None,
            clear_color: [255, 255, 255, 255],
            virtual_view_display_lists: std::collections::BTreeMap::new(),
            paint_threads: 0,
            retained: None,
            root_display_list: None,
            promote: None,
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

        // THE shared source (`GpuValueCache::for_each_bound_value`): the
        // WebRender path hands the same values to its dynamic properties.
        if let Some(cache) = gpu_cache {
            cache.for_each_bound_value(
                dom_id,
                |key, value| {
                    transforms.insert(key, *value);
                },
                |key, value| {
                    opacities.insert(key, value);
                },
            );
        }

        (transforms, opacities)
    }
}

/// Bound background colours by `ColorKey.id` ([`extract_gpu_colors`]).
pub type GpuColors = HashMap<usize, ColorU>;

/// The GPU values the display lists of EVERY DOM of a window bind, by key id: what a CPU
/// frame paints with and what its damage layer diffs from frame to frame.
///
/// Each DOM has a value cache of its own (`GpuStateManager::get_cache`), and a `VirtualView`'s
/// child is a DOM: a node of it that carries a CSS `transform` or `opacity` binds a key in
/// the CHILD's cache, and a write of the value alone (`set_css_property` on that node) moves
/// it there and rebuilds no list. `WebRender` reads every DOM's cache
/// (`synchronize_gpu_values`); the CPU paths read the root's alone, so a value-only write
/// inside a view drew nothing until the view rendered again. The keys are unique across the
/// window (one counter for every `TransformKey`, `OpacityKey` and `ColorKey`), so the DOMs'
/// maps merge without a collision.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WindowGpuValues {
    /// Transform values by `TransformKey.id`.
    pub transforms: HashMap<usize, azul_core::transform::ComputedTransform3D>,
    /// Opacity values by `OpacityKey.id`.
    pub opacities: HashMap<usize, f32>,
    /// Background colours by `ColorKey.id`.
    pub colors: GpuColors,
}

impl WindowGpuValues {
    /// The values `doms` bind in `gpu_state`: [`extract_gpu_values`] and
    /// [`extract_gpu_colors`] of each DOM's cache, merged. A DOM without a cache binds
    /// nothing.
    #[must_use]
    pub fn of_doms(
        gpu_state: &crate::managers::gpu_state::GpuStateManager,
        doms: impl IntoIterator<Item = azul_core::dom::DomId>,
    ) -> Self {
        let mut values = Self::default();
        // A9-03: straight into the merged maps. A map per DOM, copied into these and
        // dropped, hashed and stored every value twice per frame (2,119 transforms of the
        // editor's view: two maps of ~299 KB).
        let Self {
            transforms,
            opacities,
            colors,
        } = &mut values;
        for dom in doms {
            let Some(cache) = gpu_state.get_cache(dom) else {
                continue;
            };
            transforms.reserve(cache.css_transform_keys.len());
            cache.for_each_bound_value(
                dom,
                |key, value| {
                    transforms.insert(key, *value);
                },
                |key, value| {
                    opacities.insert(key, value);
                },
            );
            cache.for_each_bound_color(|key, color| {
                colors.insert(key, color);
            });
        }
        values
    }
}

/// The bound background colours of the GPU value cache, `key.id -> colour`
/// (`GpuValueCache::for_each_bound_color`): what the renderer paints a
/// bound rect in, and what the damage layer diffs frame to frame.
#[must_use]
pub fn extract_gpu_colors(gpu_cache: Option<&azul_core::gpu::GpuValueCache>) -> GpuColors {
    let mut colors = HashMap::new();
    if let Some(cache) = gpu_cache {
        cache.for_each_bound_color(|key, color| {
            colors.insert(key, color);
        });
    }
    colors
}

/// `item` as the renderer paints it: a rect bound to a colour key
/// (`DisplayList::color_bindings`) in the key's current value, every other
/// item as it is.
#[must_use]
pub fn item_with_bound_color<'a>(
    item: &'a DisplayListItem,
    binding: Option<azul_core::resources::ColorKey>,
    render_state: &CpuRenderState,
) -> alloc::borrow::Cow<'a, DisplayListItem> {
    match (item, binding.and_then(|key| render_state.colors.get(&key.id))) {
        (
            DisplayListItem::Rect {
                bounds,
                border_radius,
                ..
            },
            Some(color),
        ) => alloc::borrow::Cow::Owned(DisplayListItem::Rect {
            bounds: *bounds,
            color: *color,
            border_radius: *border_radius,
        }),
        _ => alloc::borrow::Cow::Borrowed(item),
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
    let mut surfaces: Vec<AzulPixmap> = Vec::new();
    // Accumulated scroll offset stack. Each PushScrollFrame pushes
    // (parent_offset_x + scroll_x, parent_offset_y + scroll_y).
    // Items inside a scroll frame have their bounds shifted by the
    // accumulated offset before rendering.
    let mut scroll_offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];
    let mut text_shadow_stack: Vec<StyleBoxShadow> = Vec::new();

    let _p_loop = crate::probe::Probe::span("raster_loop");
    walk_items(
        display_list,
        WalkCull::InGroups,
        true,
        pixmap,
        dpi_factor,
        renderer_resources,
        font_manager,
        glyph_cache,
        &mut transform_stack,
        &mut clip_stack,
        &mut real_clip_stack,
        &mut mask_stack,
        &mut surfaces,
        &mut scroll_offset_stack,
        &mut text_shadow_stack,
        render_state,
    )?;

    Ok(())
}

/// How a walk over a list culls the items it steps over. State items (a push, a pop) are never
/// culled: one skipped under its partner corrupts the stacks.
#[derive(Debug, Clone, Copy)]
pub(crate) enum WalkCull {
    /// A list painted in full: an item is culled only inside an isolated frame, by the clip of
    /// the frame's surface ([`outside_active_clip`]).
    InGroups,
    /// One damage rect (or a band of one) of a list: outside isolated frames an item is also
    /// culled by the rect, given in the logical px the items are stored in.
    Damage(LogicalRect),
    /// A view's child list: every item is culled by the clip it paints under (the list is a
    /// map or a page far larger than the box that shows it).
    ActiveClip,
}

/// ONE walk over the items of `list`, in list order (painter's order): the loop of the full
/// repaint, of each damage rect, of each band of one, and of a view's child list. They differ
/// only in how they cull ([`WalkCull`]) and in whether the list's colour bindings are read
/// (`bind_colors`).
///
/// Everything the walk shares with the other bands of a frame is behind `&` (`list`, the
/// resources, the fonts, `render_state`); everything behind `&mut` is this walk's alone: the
/// pixmap (a band's own rows), the stacks, the surfaces of its open groups, its glyph cache.
/// See `bands` for how a frame is cut into walks.
///
/// Returns how many items it handed to [`render_single_item`] and how many it culled.
#[allow(clippy::too_many_arguments)]
pub(crate) fn walk_items(
    list: &DisplayList,
    cull: WalkCull,
    bind_colors: bool,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    transform_stack: &mut Vec<TransAffine>,
    clip_stack: &mut Vec<Option<AzRect>>,
    real_clip_stack: &mut Vec<Option<AzRect>>,
    mask_stack: &mut Vec<MaskEntry>,
    surfaces: &mut Vec<AzulPixmap>,
    scroll_offset_stack: &mut Vec<(f32, f32)>,
    text_shadow_stack: &mut Vec<StyleBoxShadow>,
    render_state: &CpuRenderState,
) -> Result<(usize, usize), String> {
    walk_range(
        list,
        0..list.items.len(),
        cull,
        bind_colors,
        pixmap,
        dpi_factor,
        renderer_resources,
        font_manager,
        glyph_cache,
        transform_stack,
        clip_stack,
        real_clip_stack,
        mask_stack,
        surfaces,
        scroll_offset_stack,
        text_shadow_stack,
        render_state,
    )
}

/// [`walk_items`] over the items `range` of `list` alone: all of a list, or the inside of one
/// frame (a part of a kept picture painted again, [`refresh_retained_surfaces`]). The stacks
/// are the caller's: for a range that does not start the list they hold what the items before
/// it left open.
#[allow(clippy::too_many_arguments)]
pub(crate) fn walk_range(
    list: &DisplayList,
    range: core::ops::Range<usize>,
    cull: WalkCull,
    bind_colors: bool,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    transform_stack: &mut Vec<TransAffine>,
    clip_stack: &mut Vec<Option<AzRect>>,
    real_clip_stack: &mut Vec<Option<AzRect>>,
    mask_stack: &mut Vec<MaskEntry>,
    surfaces: &mut Vec<AzulPixmap>,
    scroll_offset_stack: &mut Vec<(f32, f32)>,
    text_shadow_stack: &mut Vec<StyleBoxShadow>,
    render_state: &CpuRenderState,
) -> Result<(usize, usize), String> {
    let (mut painted, mut skipped) = (0usize, 0usize);
    // The list's frames (where each ends, what it paints), asked for at the first frame met.
    let mut frames: Option<std::sync::Arc<crate::solver3::display_list::FrameIndex>> = None;
    // The frames this walk is painting whose surface is kept when they close (`retained`).
    let mut keeping: Vec<(usize, RetainedCandidate)> = Vec::new();
    // The opacity groups this walk is painting whose picture is kept when they close.
    let mut keeping_fades: Vec<(usize, FadeCandidate)> = Vec::new();
    let mut next = range.start;
    let end = range.end.min(list.items.len());
    // The grids of the list's levels (`item_grid`), for a list long enough to have any, and
    // where this walk is in each level it has entered: the children that can paint under its
    // cull there. The walk steps from one of them to the next; everything it visits goes
    // through the tests below as before.
    let grid = (list.items.len() > super::item_grid::MIN_GRID_CHILDREN && walk_uses_grids())
        .then(|| {
            let index = frames.get_or_insert_with(|| list.frame_index());
            super::item_grid::ItemGrid::of_list(list, index)
        })
        .flatten()
        .filter(|grid| grid.has_levels());
    let mut cursors: Vec<super::item_grid::Cursor> = Vec::new();
    if let Some(grid) = grid.as_deref() {
        let rect = level_cull_rect(
            &cull,
            mask_stack,
            clip_stack.last().copied().flatten(),
            *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0)),
            text_shadow_stack,
            dpi_factor,
        );
        cursors.extend(grid.cursor_for_walk(range.start, end, rect));
    }
    while next < end {
        if let Some(grid) = grid.as_deref() {
            // Out of the levels that ended; in the innermost one left, on to the next child
            // that can paint (or to the level's end, whose pop is the walk's to make).
            while cursors.last().is_some_and(|cursor| next >= cursor.end) {
                cursors.pop();
            }
            if let Some(cursor) = cursors.last_mut() {
                let target = cursor.step(next, grid);
                if target > next {
                    skipped += target - next;
                    next = target;
                    continue;
                }
            }
        }
        let item_idx = next;
        next += 1;
        let item = &list.items[item_idx];
        crate::cpu_counters::count(crate::cpu_counters::Counter::ItemsWalked, 1);
        // The pop of a frame whose surface is kept: what the `PopReferenceFrame` arm of
        // `render_single_item` does, and the surface, composited, goes to the cache.
        if keeping.last().is_some_and(|(end, _)| *end == item_idx) {
            if let Some((_, candidate)) = keeping.pop() {
                crate::cpu_counters::count(crate::cpu_counters::Counter::ItemsPainted, 1);
                painted += 1;
                if transform_stack.len() > 1 {
                    transform_stack.pop();
                }
                if matches!(mask_stack.last(), Some(MaskEntry::Transform(_))) {
                    if let Some(MaskEntry::Transform(mut group)) = mask_stack.pop() {
                        // (A10-06) The frame the shell shows as a layer is painted and kept,
                        // not composited: a group that lands nowhere composites nothing.
                        if candidate.layer {
                            if let ReferenceFrameGroup::Isolated(isolated) = &mut group {
                                isolated.lands = false;
                            }
                        }
                        let surface = group.finish_keeping(
                            pixmap,
                            surfaces,
                            clip_stack,
                            real_clip_stack,
                            scroll_offset_stack,
                        );
                        if let (Some(surface), Some(retained)) =
                            (surface, render_state.retained.as_deref())
                        {
                            if (surface.width, surface.height) == candidate.key.size {
                                retained.offer(
                                    candidate.key,
                                    candidate.list,
                                    surface,
                                    candidate.part,
                                    candidate.placement,
                                );
                            }
                        }
                    }
                }
                continue;
            }
        }
        // The pop of an opacity group whose picture is kept: the pixels the group painted on
        // transparent, before the pop (handled below like any other) blends them into what
        // was set aside, go to the cache.
        if keeping_fades.last().is_some_and(|(end, _)| *end == item_idx) {
            if let Some((_, candidate)) = keeping_fades.pop() {
                if let (Some(retained), true) = (render_state.retained.as_deref(), surfaces.is_empty()) {
                    candidate.keep(retained, pixmap);
                }
            }
        }
        // Inside a turned or scaled frame the clip the items paint under is the frame's
        // surface's own (the damage, or the view's box, mapped back through the frame), not a
        // rect of the pixels below: there the walk culls by that clip alone.
        let in_group = inside_isolated_transform(mask_stack);
        let is_frame_push = matches!(item, DisplayListItem::PushReferenceFrame { .. });
        let frame = if is_frame_push {
            let index = frames.get_or_insert_with(|| list.frame_index());
            index.frame_at(item_idx).map(|mut frame| {
                // What it paints with the frames inside it where their live matrices put
                // them (A2-cpu-04); an inner perspective keeps the laid-out extent.
                if !frame.leaf {
                    if let Some(extent) = index.extent_with(&list.items, item_idx, &render_state.transforms) {
                        frame.extent = extent;
                    }
                }
                frame
            })
        } else {
            None
        };
        // A frame nothing of which can paint under the clip is stepped over whole, from its
        // push to past its pop: a map's list is two thousand label frames, each of a handful
        // of items, of which a pass reaches a few.
        if let Some(frame) = frame {
            let below = surfaces
                .last()
                .map_or((pixmap.width, pixmap.height), |s| (s.width, s.height));
            if frame_paints_nothing(
                item,
                &frame,
                &cull,
                in_group,
                below,
                dpi_factor,
                clip_stack.last().copied().flatten(),
                *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0)),
                mask_stack,
                text_shadow_stack,
                render_state,
            ) {
                skipped += 1;
                next = frame.end + 1;
                continue;
            }
            // A turned label of a view's list is drawn through its matrix: no surface, no
            // composite (`paint_turned_text_frame`).
            if frame.text_only
                && matches!(cull, WalkCull::ActiveClip)
                && text_shadow_stack.is_empty()
                && paint_turned_text_frame(
                    list,
                    item,
                    &frame,
                    match surfaces.last_mut() {
                        Some(surface) => surface,
                        None => &mut *pixmap,
                    },
                    dpi_factor,
                    clip_stack.last().copied().flatten(),
                    *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0)),
                    font_manager,
                    glyph_cache,
                    render_state,
                )
            {
                painted += 1;
                next = frame.end + 1;
                continue;
            }
            // A frame whose picture is kept from an earlier frame is composited, not painted:
            // only its matrix, or the clip around it, changed.
            if let Some(retained) = render_state.retained.as_deref() {
                if let Some(candidate) = retained_candidate(
                    list,
                    item,
                    &frame,
                    frames.as_ref().and_then(|index| index.inner_of(item_idx)),
                    in_group || !surfaces.is_empty() || !keeping_fades.is_empty(),
                    below,
                    dpi_factor,
                    clip_stack.last().copied().flatten(),
                    *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0)),
                    text_shadow_stack,
                    glyph_cache,
                    font_manager,
                    render_state,
                    retained,
                    false,
                ) {
                    if let Some(surface) = retained.lookup(&candidate.key, candidate.needed) {
                        crate::cpu_counters::count(crate::cpu_counters::Counter::RetainedHits, 1);
                        // (A10-06) The frame the shell shows as a layer is not composited.
                        if !candidate.layer {
                            candidate.group.finish(&surface, pixmap);
                        }
                        skipped += 1;
                        next = frame.end + 1;
                        continue;
                    }
                    crate::cpu_counters::count(crate::cpu_counters::Counter::RetainedMisses, 1);
                    keeping.push((frame.end, candidate));
                }
            }
        }
        // An opacity group whose picture is kept from an earlier frame is blended again, at
        // its live opacity, not painted: a fade writes one value (`fade_candidate`).
        if let (DisplayListItem::PushOpacity { .. }, Some(retained), true) =
            (item, render_state.retained.as_deref(), surfaces.is_empty())
        {
            let index = std::sync::Arc::clone(frames.get_or_insert_with(|| list.frame_index()));
            if let Some(scope) = index.opacity_scope_at(item_idx) {
                let fade = fade_candidate(
                    list,
                    item,
                    item_idx,
                    scope,
                    index.opacity_extent_with(&list.items, item_idx, &render_state.transforms),
                    (pixmap.width, pixmap.height),
                    dpi_factor,
                    clip_stack.last().copied().flatten(),
                    *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0)),
                    mask_stack,
                    text_shadow_stack,
                    glyph_cache,
                    font_manager,
                    render_state,
                    retained,
                );
                if let Some((candidate, opacity, blend)) = fade {
                    if let Some(kept) = retained.lookup(&candidate.key, candidate.part()) {
                        crate::cpu_counters::count(crate::cpu_counters::Counter::RetainedHits, 1);
                        candidate.blend(&kept, pixmap, opacity, blend);
                        skipped += 1;
                        next = scope.end + 1;
                        continue;
                    }
                    crate::cpu_counters::count(crate::cpu_counters::Counter::RetainedMisses, 1);
                    keeping_fades.push((scope.end, candidate));
                }
            }
        }
        let by_clip = in_group || matches!(cull, WalkCull::ActiveClip);
        if by_clip
            && outside_active_clip(
                item,
                text_shadow_stack,
                *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0)),
                clip_stack.last().copied().flatten(),
                mask_stack,
                dpi_factor,
            )
        {
            skipped += 1;
            continue;
        }
        if let WalkCull::Damage(damage) = &cull {
            // Always process state-management items (Push/Pop) regardless of bounds,
            // because skipping a Push while processing its matching Pop corrupts stacks.
            if !item.is_state_management() && !in_group {
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
                    // A shadow paints past the box by its offset, blur and spread
                    // (`outside_active_clip` widens by the same): a rect that meets the
                    // shadow and not the box still has to repaint it.
                    let reach = text_shadow_reach(text_shadow_stack);
                    let test_bounds = if reach > 0.0 {
                        LogicalRect {
                            origin: LogicalPosition {
                                x: test_bounds.origin.x - reach,
                                y: test_bounds.origin.y - reach,
                            },
                            size: LogicalSize {
                                width: test_bounds.size.width + 2.0 * reach,
                                height: test_bounds.size.height + 2.0 * reach,
                            },
                        }
                    } else {
                        test_bounds
                    };
                    if !rects_overlap_or_adjacent(&test_bounds, damage, 0.0) {
                        skipped += 1;
                        continue;
                    }
                }
            }
        }

        painted += 1;
        #[cfg(test)]
        if !item.is_state_management() {
            PAINT_TRACE.with(|trace| {
                if let Some(trace) = trace.borrow_mut().as_mut() {
                    trace.push((list as *const DisplayList as usize, item_idx));
                }
            });
        }
        let _p_item = crate::probe::Probe::span(probe_label_for_item(item));
        let bound_item = if bind_colors {
            item_with_bound_color(item, list.color_binding(item_idx), render_state)
        } else {
            alloc::borrow::Cow::Borrowed(item)
        };
        let item = &*bound_item;
        // What the item's scope paints: a frame's extent, or an opacity group's when its
        // content paints past its box.
        let scope_extent = frame.map(|f| f.extent).or_else(|| {
            matches!(item, DisplayListItem::PushOpacity { .. })
                .then(|| {
                    // With the frames inside the group where their live matrices paint them
                    // (A1-60): the group's region has to hold what a moved child paints.
                    frames
                        .get_or_insert_with(|| list.frame_index())
                        .opacity_extent_with(&list.items, item_idx, &render_state.transforms)
                })
                .flatten()
        });
        render_single_item(
            item,
            list.uniform_text_bgs.get(item_idx).copied().flatten(),
            scope_extent,
            pixmap,
            dpi_factor,
            renderer_resources,
            font_manager,
            glyph_cache,
            transform_stack,
            clip_stack,
            real_clip_stack,
            mask_stack,
            surfaces,
            scroll_offset_stack,
            text_shadow_stack,
            render_state,
        )?;
        // The walk made a push: the level inside it, when that has a grid, with the walk's
        // state as it is now (the push's clip, offset, shadow or group is in it).
        if let Some(grid) = grid.as_deref() {
            if grid.has_level_inside(item_idx) {
                let rect = level_cull_rect(
                    &cull,
                    mask_stack,
                    clip_stack.last().copied().flatten(),
                    *scroll_offset_stack.last().unwrap_or(&(0.0, 0.0)),
                    text_shadow_stack,
                    dpi_factor,
                );
                cursors.extend(grid.cursor_inside(item_idx, rect));
            }
        }
    }
    Ok((painted, skipped))
}

/// A frame the walk is about to open whose surface can be kept (`retained`): the key of its
/// picture, the part of the surface a composite of it reads now, the part a walk of it paints
/// now, and the group it is composited as.
struct RetainedCandidate {
    key: super::retained::SurfaceKey,
    /// Surface px, half-open: what the composite reads (the clip mapped back, and the taps).
    needed: [i32; 4],
    /// Surface px, half-open: what a walk paints (`group_visible_part`).
    part: [i32; 4],
    group: TransformGroup,
    list: std::sync::Arc<DisplayList>,
    placement: super::retained::Placement,
    /// (A10-06) The frame the shell shows as a layer, composited by the shell
    /// (`CpuRenderState::promote` without `composite`): the walk paints and keeps its
    /// surface and puts none of it into the pixels below.
    layer: bool,
}

/// `list` as the shared list the render state holds (the window's own, or a view's), if it is
/// one of them: the list a kept surface holds on to.
/// With its slot: a view's DOM id, `usize::MAX` for the window's own list.
fn shared_list(
    list: &DisplayList,
    render_state: &CpuRenderState,
) -> Option<(std::sync::Arc<DisplayList>, usize)> {
    let is = |shared: &std::sync::Arc<DisplayList>| core::ptr::eq(std::sync::Arc::as_ptr(shared), list);
    if let Some(root) = render_state.root_display_list.as_ref().filter(|root| is(root)) {
        return Some((std::sync::Arc::clone(root), usize::MAX));
    }
    render_state
        .virtual_view_display_lists
        .iter()
        .find(|(_, shared)| is(shared))
        .map(|(dom, shared)| (std::sync::Arc::clone(shared), dom.inner))
}

/// The list in `slot` of the window ([`shared_list`]).
fn list_in_slot(slot: usize, render_state: &CpuRenderState) -> Option<std::sync::Arc<DisplayList>> {
    if slot == usize::MAX {
        return render_state.root_display_list.clone();
    }
    render_state
        .virtual_view_display_lists
        .get(&azul_core::dom::DomId { inner: slot })
        .cloned()
}

/// The live values the picture of a frame depends on besides its items (`inner`, from the
/// list's `FrameIndex`: the keys bound inside the frame, its scroll frames), hashed. `None` for
/// a frame that holds a view: its picture depends on another list. A key without a value
/// hashes as that (the walk then paints with the list's own matrix or opacity, which the list
/// holds).
fn values_inside(
    inner: Option<&crate::solver3::display_list::FrameInner>,
    render_state: &CpuRenderState,
) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let Some(inner) = inner else {
        return Some(hasher.finish());
    };
    if inner.has_view {
        return None;
    }
    for key in &inner.transform_keys {
        match render_state.transforms.get(key) {
            Some(value) => {
                for row in &value.m {
                    for v in row {
                        v.to_bits().hash(&mut hasher);
                    }
                }
            }
            None => u32::MAX.hash(&mut hasher),
        }
    }
    for key in &inner.opacity_keys {
        render_state
            .opacities
            .get(key)
            .map_or(u32::MAX, |o| o.to_bits())
            .hash(&mut hasher);
    }
    for key in &inner.color_keys {
        render_state
            .colors
            .get(key)
            .map(|c| (c.r, c.g, c.b, c.a))
            .hash(&mut hasher);
    }
    for id in &inner.scroll_ids {
        let (x, y) = render_state.scroll_offsets.get(id).copied().unwrap_or((0.0, 0.0));
        (x.to_bits(), y.to_bits()).hash(&mut hasher);
    }
    Some(hasher.finish())
}

/// Where `extent` (already moved by the walk's offset) begins within its first device pixel,
/// in 1/1024 px per axis: the fraction `GroupGeometry::of` floors away to find the surface's
/// origin. Extents that differ by whole device pixels have the same phase. Example at a scale
/// factor of 1: an extent at x = 37.25 has phase 256; panned by 12 px, at 25.25, the same; at
/// 25.75, 768.
#[allow(clippy::cast_possible_truncation)] // a fraction of a pixel times 1024
fn surface_phase(extent: &LogicalRect, dpi_factor: f32) -> (i32, i32) {
    let phase = |v: f32| {
        let device = v * dpi_factor;
        ((device - device.floor()) * 1024.0).round() as i32
    };
    (phase(extent.origin.x), phase(extent.origin.y))
}

/// What every text run and system colour of a picture is painted with, hashed: the
/// anti-aliasing settings, how many fonts are loaded (a face that arrives paints runs that
/// were skipped), the system style.
fn paint_settings(
    glyph_cache: &GlyphCache,
    font_manager: &FontManager<FontRef>,
    render_state: &CpuRenderState,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", glyph_cache.text_aa()).hash(&mut hasher);
    font_manager.parsed_fonts.lock().map_or(0, |fonts| fonts.len()).hash(&mut hasher);
    font_manager.embedded_fonts.lock().map_or(0, |fonts| fonts.len()).hash(&mut hasher);
    render_state
        .system_style
        .as_ref()
        .map_or(0usize, |style| std::sync::Arc::as_ptr(style) as usize)
        .hash(&mut hasher);
    hasher.finish()
}

/// The frame `frame`, pushed by `item`, as one whose surface can be kept, or `None`: it must
/// be self-contained, an isolated group with an affine matrix, not inside another group
/// (`nested`: that group's surface is the one worth keeping) nor under a text shadow of the
/// list around it, large enough to be worth a lookup and small enough for the cache, in a list
/// the render state holds shared, and without a view inside.
///
/// The key's phase is that of the frame's extent on the surface below (`retained`, rule 4).
/// `needed` is `group_part_within` with a margin of 2 px: the composite samples destination
/// pixel centres mapped back, `s = inv(matrix)(d + 0.5) - 0.5`, and reads the texels
/// `floor(s)` and `floor(s) + 1`, at most one texel outside the clip mapped back.
#[allow(clippy::too_many_arguments)]
fn retained_candidate(
    list: &DisplayList,
    item: &DisplayListItem,
    frame: &crate::solver3::display_list::FrameEntry,
    inner: Option<&crate::solver3::display_list::FrameInner>,
    nested: bool,
    below: (u32, u32),
    dpi_factor: f32,
    clip: Option<AzRect>,
    scroll_offset: (f32, f32),
    text_shadow_stack: &[StyleBoxShadow],
    glyph_cache: &GlyphCache,
    font_manager: &FontManager<FontRef>,
    render_state: &CpuRenderState,
    retained: &RetainedFrames,
    // (A10-06) Also a frame whose matrix is the identity or a translation: what the planner
    // of the layer present asks ([`layer_candidate`]). The walk passes `false`: for it only
    // the frame `CpuRenderState::promote` names is one.
    any_matrix: bool,
) -> Option<RetainedCandidate> {
    let DisplayListItem::PushReferenceFrame {
        transform_key,
        initial_transform,
        bounds,
    } = item
    else {
        return None;
    };
    if nested || !frame.self_contained || !text_shadow_stack.is_empty() || !walk_culls_by_clip() {
        return None;
    }
    let m = render_state
        .transforms
        .get(&transform_key.id)
        .map_or(&initial_transform.m, |t| &t.m);
    // (A10-06) The frame the shell shows as a layer is an isolated group whatever its matrix
    // (`ReferenceFrameGroup::begin`), so its surface is kept like any other group's.
    let promoted = render_state.promote.filter(|p| p.transform_key == transform_key.id);
    if !any_matrix && promoted.is_none() && (is_identity_2d(m) || translation_2d(m).is_some()) {
        return None;
    }
    let on_surface = |r: &LogicalRect| LogicalRect {
        origin: LogicalPosition {
            x: r.origin.x - scroll_offset.0,
            y: r.origin.y - scroll_offset.1,
        },
        size: r.size,
    };
    let g = GroupGeometry::of(
        m,
        &on_surface(bounds.inner()),
        &on_surface(&frame.extent),
        dpi_factor,
        clip,
    );
    let px = u64::from(g.width) * u64::from(g.height);
    if px < super::retained::MIN_SURFACE_PX || !retained.can_keep((px as usize).saturating_mul(4)) {
        return None;
    }
    let size = (g.width, g.height);
    // (A10-06) What `ReferenceFrameGroup::begin` paints of this frame: the same box.
    let (x0, y0, x1, y1) = match promoted.and_then(|p| p.paint_margin) {
        Some(margin) => group_part_within(&g.matrix, g.outer_clip, below, size, margin)?,
        None => group_visible_part(&g.matrix, g.outer_clip, below, size)?,
    };
    let (nx0, ny0, nx1, ny1) = group_part_within(&g.matrix, g.outer_clip, below, size, 2)?;
    let (shared, slot) = shared_list(list, render_state)?;
    let list_at = std::sync::Arc::as_ptr(&shared) as usize;
    let values = values_inside(inner, render_state)?;
    Some(RetainedCandidate {
        key: super::retained::SurfaceKey {
            list: list_at,
            push: frame.push,
            size,
            dpi_bits: dpi_factor.to_bits(),
            phase: surface_phase(&on_surface(&frame.extent), dpi_factor),
            values,
            settings: paint_settings(glyph_cache, font_manager, render_state),
        },
        needed: [nx0, ny0, nx1, ny1],
        part: [x0, y0, x1, y1],
        group: TransformGroup {
            width: g.width,
            height: g.height,
            matrix: g.matrix,
            outer_clip: g.outer_clip,
            lands: true,
        },
        list: shared,
        // What `ReferenceFrameGroup::begin` pushes as the walk's offset inside the group.
        placement: super::retained::Placement {
            offset: (
                scroll_offset.0 + g.x as f32 / dpi_factor,
                scroll_offset.1 + g.y as f32 / dpi_factor,
            ),
            slot,
        },
        layer: promoted.is_some_and(|p| !p.composite),
    })
}

/// (A10-06) What the planner of the layer present (`present_layer`) asks of a frame it could
/// show as a layer of its own: the key of the frame's surface, the part of the surface that
/// shows, and how the surface lands.
pub(super) struct LayerCandidate {
    pub(super) key: super::retained::SurfaceKey,
    /// Surface px, half-open: what shows under the clip, with the taps of a composite.
    pub(super) needed: [i32; 4],
    /// Surface pixel -> device pixel of the frame, a column-vector homography.
    pub(super) matrix: Mat3,
    /// The clip open around the frame, device px of the frame, half-open.
    pub(super) clip: Option<(i32, i32, i32, i32)>,
    /// `TransformKey::id` of the frame's push.
    pub(super) transform_key: usize,
}

/// (A10-06) [`retained_candidate`] for the planner of the layer present: `frame`, pushed by
/// `item` of `list`, as a frame whose picture is a kept surface WHATEVER its matrix is (the
/// walk paints it so once `CpuRenderState::promote` names it), under `clip` (the clip a walk
/// without a damage rect has at the push) and at the walk's offset `scroll_offset`, in no
/// group and under no text shadow. `None` for a frame the walk would not keep.
///
/// The key, the surface's size and the matrix are the ones a pass of the walk finds for the
/// same frame at the same offset: both come out of [`retained_candidate`], and neither
/// depends on the clip.
#[allow(clippy::too_many_arguments)]
pub(super) fn layer_candidate(
    list: &DisplayList,
    item: &DisplayListItem,
    frame: &crate::solver3::display_list::FrameEntry,
    inner: Option<&crate::solver3::display_list::FrameInner>,
    below: (u32, u32),
    dpi_factor: f32,
    clip: Option<AzRect>,
    scroll_offset: (f32, f32),
    glyph_cache: &GlyphCache,
    font_manager: &FontManager<FontRef>,
    render_state: &CpuRenderState,
    retained: &RetainedFrames,
) -> Option<LayerCandidate> {
    let DisplayListItem::PushReferenceFrame { transform_key, .. } = item else {
        return None;
    };
    let candidate = retained_candidate(
        list,
        item,
        frame,
        inner,
        false,
        below,
        dpi_factor,
        clip,
        scroll_offset,
        &[],
        glyph_cache,
        font_manager,
        render_state,
        retained,
        true,
    )?;
    Some(LayerCandidate {
        key: candidate.key,
        needed: candidate.needed,
        matrix: candidate.group.matrix,
        clip: candidate.group.outer_clip,
        transform_key: transform_key.id,
    })
}

/// An opacity group the walk is about to open whose picture can be kept (`retained`,
/// "Opacity groups"): the key of the picture, where the group's whole device box lies in the
/// target, and the part of it this walk blends.
struct FadeCandidate {
    key: super::retained::SurfaceKey,
    /// The device pixel of the target that the kept surface's pixel (0, 0) is: the corner of
    /// the group's whole box.
    origin: (i32, i32),
    /// Target px, half-open: the group's box inside the clip and inside the target - what the
    /// group's push sets aside and its pop blends, of the pixels that exist.
    region: [i32; 4],
    list: std::sync::Arc<DisplayList>,
    placement: super::retained::Placement,
}

impl FadeCandidate {
    /// `region` in the kept surface's own pixels: what a hit reads and what a miss keeps.
    fn part(&self) -> [i32; 4] {
        let ([x0, y0, x1, y1], (ox, oy)) = (self.region, self.origin);
        [x0 - ox, y0 - oy, x1 - ox, y1 - oy]
    }

    /// A hit: the kept picture's part blended onto `pixmap` as the group's pop blends what
    /// the group painted ([`blend_group_pixels`], the pop's own loop).
    #[allow(clippy::cast_sign_loss)] // the region lies inside the box: `part` is not negative
    fn blend(
        &self,
        kept: &AzulPixmap,
        pixmap: &mut AzulPixmap,
        opacity: f32,
        blend: azul_css::props::style::effects::StyleMixBlendMode,
    ) {
        let [x0, y0, x1, y1] = self.region;
        let part = self.part();
        blend_group_pixels(
            pixmap,
            kept.data(),
            kept.width,
            (part[0] as u32, part[1] as u32),
            (x0, y0, (x1 - x0) as u32, (y1 - y0) as u32),
            opacity,
            blend,
        );
    }

    /// A miss, when the walk reaches the group's pop: the pixels of `region` - what the group
    /// painted on the transparent pixels its push left there - are offered to the cache, in a
    /// surface of the group's whole box (zeroed by the allocator: only the part costs memory).
    #[allow(clippy::cast_sign_loss)] // a region with x1 > x0, y1 > y0
    fn keep(self, retained: &RetainedFrames, pixmap: &AzulPixmap) {
        let [x0, y0, x1, y1] = self.region;
        let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let Some(mut surface) = AzulPixmap::new_transparent(self.key.size.0, self.key.size.1) else {
            return;
        };
        let part = self.part();
        let painted = snapshot_region(pixmap, x0, y0, w, h);
        write_region(&mut surface, &painted, w, h, part[0], part[1]);
        retained.offer_fade(self.key, self.list, surface, part, self.placement);
    }
}

/// The opacity group pushed by `item` (at `push` of `list`, `scope` from the list's
/// `FrameIndex`) as one whose picture can be kept, with the opacity and the blend mode its pop
/// blends with; or `None`.
///
/// The picture is what the group paints on the transparent pixels of its region. It may be
/// kept when it depends on nothing but the list, the values bound inside the group, the phase
/// of the group's box and the paint settings (the [`super::retained::SurfaceKey`]):
///
/// - the group is BLENDED (an opacity under 1, or a blend mode): its text is then grayscale
///   whatever lies under it and wherever a clip cuts it (`inside_blended_group`). At an
///   opacity of exactly 1 a run may be painted in LCD stripes, chosen by the pixels under the
///   part of it the clip leaves: another picture, and one that depends on the clip;
/// - no text shadow of the list around it is open (the runs inside would paint a halo the
///   group's extent does not hold), and no group, mask or filter is open around it (the one
///   around it decides how text inside is painted; a turned frame's surface is the one worth
///   keeping). A rounded clip, and a frame that is the identity or a move, may be open;
/// - it holds no view (`values_inside`), and its box is large enough to be worth a lookup and
///   small enough for the cache.
///
/// `extent` is what the group paints when that is more than its box (`opacity_extent_with`),
/// `target` the size of the pixmap the walk paints into, `clip` the walk's innermost clip.
/// The box is `device_region` of the extent at the walk's offset - what the `PushOpacity` arm
/// of `render_single_item` sets aside under no clip - and the region the same under `clip`,
/// cut to the target.
///
/// Example (scale factor 2, offset (0, 0)): a group with the extent (10.25, 20) 300 x 200
/// has the box x 20 .. 621 (20.5 floored, 620.5 ceiled), y 40 .. 440: a surface of 601 x 400
/// px with the phase (512, 0). Under a band's clip (0, 100) 2000 x 200 px its region is
/// [20, 100, 621, 300], the part [0, 60, 601, 260] of the surface.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)] // a box of whole pixels
fn fade_candidate(
    list: &DisplayList,
    item: &DisplayListItem,
    push: usize,
    scope: &crate::solver3::display_list::OpacityScope,
    extent: Option<LogicalRect>,
    target: (u32, u32),
    dpi_factor: f32,
    clip: Option<AzRect>,
    scroll_offset: (f32, f32),
    mask_stack: &[MaskEntry],
    text_shadow_stack: &[StyleBoxShadow],
    glyph_cache: &GlyphCache,
    font_manager: &FontManager<FontRef>,
    render_state: &CpuRenderState,
    retained: &RetainedFrames,
) -> Option<(FadeCandidate, f32, azul_css::props::style::effects::StyleMixBlendMode)> {
    use azul_css::props::style::effects::StyleMixBlendMode;
    let DisplayListItem::PushOpacity {
        bounds,
        opacity,
        opacity_key,
        mix_blend_mode,
    } = item
    else {
        return None;
    };
    // The live value, as the `PushOpacity` arm reads it.
    let opacity = opacity_key
        .and_then(|k| render_state.opacities.get(&k.id).copied())
        .unwrap_or(*opacity);
    let blended = opacity < 1.0 || !matches!(mix_blend_mode, StyleMixBlendMode::Normal);
    if !blended || !text_shadow_stack.is_empty() {
        return None;
    }
    let only_moves_around = mask_stack.iter().all(|entry| {
        matches!(
            entry,
            MaskEntry::RoundedClip { .. }
                | MaskEntry::Transform(ReferenceFrameGroup::Identity | ReferenceFrameGroup::Translated)
        )
    });
    if !only_moves_around {
        return None;
    }
    let extent = extent.unwrap_or_else(|| *bounds.inner());
    let on_surface = LogicalRect {
        origin: LogicalPosition {
            x: extent.origin.x - scroll_offset.0,
            y: extent.origin.y - scroll_offset.1,
        },
        size: extent.size,
    };
    let boxed = logical_rect_to_az_rect(&on_surface, dpi_factor);
    let whole = device_region(boxed, None)?;
    let inside_clip = device_region(boxed, clip)?;
    let (ox, oy) = (whole.x as i32, whole.y as i32);
    let (width, height) = (whole.width as i32, whole.height as i32);
    if width <= 0 || height <= 0 || width > MAX_TRANSFORM_GROUP_SIDE || height > MAX_TRANSFORM_GROUP_SIDE {
        return None;
    }
    let px = u64::from(width as u32) * u64::from(height as u32);
    if px < super::retained::MIN_SURFACE_PX || !retained.can_keep_fade((px as usize).saturating_mul(4)) {
        return None;
    }
    let region = [
        (inside_clip.x as i32).max(0),
        (inside_clip.y as i32).max(0),
        ((inside_clip.x + inside_clip.width) as i32).min(target.0 as i32),
        ((inside_clip.y + inside_clip.height) as i32).min(target.1 as i32),
    ];
    if region[2] <= region[0] || region[3] <= region[1] {
        return None;
    }
    let (shared, slot) = shared_list(list, render_state)?;
    let values = values_inside(Some(&scope.inner), render_state)?;
    Some((
        FadeCandidate {
            key: super::retained::SurfaceKey {
                list: std::sync::Arc::as_ptr(&shared) as usize,
                push,
                size: (width as u32, height as u32),
                dpi_bits: dpi_factor.to_bits(),
                phase: surface_phase(&on_surface, dpi_factor),
                values,
                settings: paint_settings(glyph_cache, font_manager, render_state),
            },
            origin: (ox, oy),
            region,
            list: shared,
            placement: super::retained::Placement {
                offset: (
                    scroll_offset.0 + ox as f32 / dpi_factor,
                    scroll_offset.1 + oy as f32 / dpi_factor,
                ),
                slot,
            },
        },
        opacity,
        *mix_blend_mode,
    ))
}

/// What the pop of an opacity group does with the pixels the group painted: the `w` x `h`
/// pixels at `(sx, sy)` of `src` (RGBA rows of `src_w` px) are put onto the pixels at
/// `(x, y)` of `pixmap` at `opacity`, with `blend` (`Blender`: `blend_pixel`'s levels). The
/// pop's own loop, and what a kept picture is blended with (`FadeCandidate::blend`): one
/// arithmetic for both, so a kept group is the group painted.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)] // bounded pixel coords
fn blend_group_pixels(
    pixmap: &mut AzulPixmap,
    src: &[u8],
    src_w: u32,
    (sx, sy): (u32, u32),
    (x, y, w, h): (i32, i32, u32, u32),
    opacity: f32,
    blend: azul_css::props::style::effects::StyleMixBlendMode,
) {
    let pw = pixmap.width as i32;
    let ph = pixmap.height as i32;
    let op = (opacity * 255.0).clamp(0.0, 255.0) as u32;
    // `blend_pixel`'s levels, without its work where the group
    // painted nothing or painted opaque.
    let blender = super::blend::Blender::new(op, blend);
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
            let pi = (dy as usize * pixmap.width as usize + dx as usize) * 4;
            let si = ((sy as usize + py as usize) * src_w as usize + sx as usize + px as usize) * 4;
            if pi + 3 >= pixmap.data.len() || si + 3 >= src.len() {
                continue;
            }
            let from = [src[si], src[si + 1], src[si + 2], src[si + 3]];
            blender.blend(&mut pixmap.data[pi..pi + 4], from);
        }
    }
}

thread_local! {
    /// Tests: paint turned text frames into a surface, as before they were drawn directly.
    #[cfg(test)]
    static TURNED_TEXT_IN_GROUPS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Are turned text frames drawn through their matrix (the default), or into a surface that is
/// composited (`AZ_CPU_TURNED_TEXT=group`: the path before, for comparing the two)?
fn turned_text_is_drawn_directly() -> bool {
    #[cfg(test)]
    if TURNED_TEXT_IN_GROUPS.with(std::cell::Cell::get) {
        return false;
    }
    static DIRECT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DIRECT.get_or_init(|| std::env::var("AZ_CPU_TURNED_TEXT").map_or(true, |v| v != "group"))
}

/// Where a reference frame's matrix puts a point of the surface the frame is painted into:
/// `q` (device px, the place the point is painted at without the frame) lands at
/// `o + L (q - o) + t`, with `o` the frame's origin in device px, `L = [a b; c d]` the
/// matrix's linear part and `t` its translation in device px. The map `GroupGeometry::of`
/// builds for a group, without the group.
struct TurnedMap {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    origin: (f64, f64),
    moved: (f64, f64),
}

impl TurnedMap {
    fn at(&self, qx: f64, qy: f64) -> (f64, f64) {
        let (x, y) = (qx - self.origin.0, qy - self.origin.1);
        (
            self.a * x + self.b * y + self.origin.0 + self.moved.0,
            self.c * x + self.d * y + self.origin.1 + self.moved.1,
        )
    }
}

/// Draws the reference frame `frame`, pushed by `item`, through its matrix, item by item,
/// without a surface: its fills as turned quads, and each text run as two kept coverage
/// pictures, the halo and the ink ([`turned_run_picture`]), tinted and composited. `false`,
/// with nothing drawn, when the frame has to be a group: it is not a self-contained leaf, its
/// matrix is the identity, a move or has a perspective part, a fill is rounded, a run's own
/// clip cuts its ink, or a font cannot be resolved.
///
/// A turned label used to be a surface of its box, its text and halo painted upright into it,
/// and a bilinear composite of that surface (a third of an identity repaint of a map with a
/// thousand turned labels). The pictures are kept by what they show (glyphs, font, size, the
/// matrix's linear part, the halo) and not by where the label stands: the run's first pen is
/// rounded to a whole device pixel, so a label that moves keeps its pictures, and a repaint
/// of a label is two tinted blits.
///
/// Against the group path: the same shapes within half a device pixel (the rounding of the
/// anchor), sharper (outlines turned, not pixels resampled); a halo is no longer cut at the
/// frame's box.
#[allow(clippy::too_many_arguments, clippy::too_many_lines, clippy::cast_possible_truncation)]
fn paint_turned_text_frame(
    list: &DisplayList,
    item: &DisplayListItem,
    frame: &crate::solver3::display_list::FrameEntry,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    clip: Option<AzRect>,
    scroll_offset: (f32, f32),
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
) -> bool {
    let DisplayListItem::PushReferenceFrame {
        transform_key,
        initial_transform,
        bounds,
    } = item
    else {
        return false;
    };
    if !turned_text_is_drawn_directly()
        || !frame.self_contained
        || !frame.leaf
        || !(dpi_factor > 0.0)
        || frame.end > list.items.len()
    {
        return false;
    }
    let m = render_state
        .transforms
        .get(&transform_key.id)
        .map_or(&initial_transform.m, |t| &t.m);
    if is_identity_2d(m)
        || translation_2d(m).is_some()
        || m[0][3].abs() > 1e-6
        || m[1][3].abs() > 1e-6
        || (m[3][3] - 1.0).abs() > 1e-6
    {
        return false;
    }
    let dpi = f64::from(dpi_factor);
    let map = TurnedMap {
        a: f64::from(m[0][0]),
        b: f64::from(m[1][0]),
        c: f64::from(m[0][1]),
        d: f64::from(m[1][1]),
        origin: (
            f64::from(bounds.inner().origin.x - scroll_offset.0) * dpi,
            f64::from(bounds.inner().origin.y - scroll_offset.1) * dpi,
        ),
        moved: (f64::from(m[3][0]) * dpi, f64::from(m[3][1]) * dpi),
    };
    let det = map.a * map.d - map.b * map.c;
    if !det.is_finite() || det.abs() < 1e-9 || ![map.moved.0, map.moved.1].iter().all(|v| v.is_finite()) {
        return false;
    }
    let grow = det.abs().sqrt();
    let items = &list.items[frame.push + 1..frame.end];
    let shadow_px = |v: &azul_css::props::basic::pixel::PixelValue| {
        v.to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
    };
    // A shadow's blur radius and spread, device px before the matrix.
    let halo_of = |sh: &StyleBoxShadow| {
        (
            shadow_blur_radius(shadow_px(&sh.blur_radius.inner).max(0.0), dpi_factor),
            shadow_spread_px(shadow_px(&sh.spread_radius.inner), dpi_factor),
        )
    };
    // Everything in the frame can be drawn this way, or the frame is a group.
    // The reach of the halos open at the item, for the size of a run's pictures.
    let mut open_reaches: Vec<f64> = Vec::new();
    for it in items {
        match it {
            DisplayListItem::PushTextShadow { shadow } => {
                open_reaches.push(turned_halo(Some(halo_of(shadow)), grow).2);
            }
            DisplayListItem::PopTextShadow => {
                open_reaches.pop();
            }
            DisplayListItem::Text { glyphs, font_hash, font_size_px, clip_rect, .. } => {
                if font_manager.resolve_font_by_hash(font_hash.font_hash).is_none()
                    || glyph_ink_reach(font_size_px * dpi_factor * grow as f32).is_none()
                {
                    return false;
                }
                if let Some(ink) = it.visual_bounds() {
                    let c = clip_rect.inner();
                    if ink.origin.x < c.origin.x - 0.5
                        || ink.origin.y < c.origin.y - 0.5
                        || ink.origin.x + ink.size.width > c.origin.x + c.size.width + 0.5
                        || ink.origin.y + ink.size.height > c.origin.y + c.size.height + 0.5
                    {
                        return false;
                    }
                    // A run no picture can be made of (a 1,200 px line under `scale(8)` is
                    // 9,600 px of turned ink) makes the frame a group, which paints the part
                    // that lands. Decided here, before anything is drawn: the paint loop
                    // used to skip such a run after the frame was committed to this path,
                    // and the text was not painted at all (A2-cpu-R06).
                    let run = TurnedRun { glyphs, font_hash: *font_hash, font_size_px: *font_size_px, ink };
                    let reach = open_reaches.iter().copied().fold(turned_halo(None, grow).2, f64::max);
                    if !glyphs.is_empty() && turned_picture_box(&run, &map, reach, dpi_factor).is_none() {
                        return false;
                    }
                }
            }
            DisplayListItem::Rect { border_radius, .. } if !border_radius.is_zero() => return false,
            _ => {}
        }
    }
    let (pw, ph) = (pixmap.width as i32, pixmap.height as i32);
    let clip_box = match clip {
        Some(c) => {
            let (x0, y0, x1, y1) = c.pixel_box();
            (x0.max(0), y0.max(0), x1.min(pw), y1.min(ph))
        }
        None => (0, 0, pw, ph),
    };
    let device = |x: f32, y: f32| {
        (
            f64::from((x - scroll_offset.0) * dpi_factor),
            f64::from((y - scroll_offset.1) * dpi_factor),
        )
    };
    let mut shadows: Vec<StyleBoxShadow> = Vec::new();
    for (at, it) in items.iter().enumerate() {
        match it {
            DisplayListItem::PushTextShadow { shadow } => shadows.push(*shadow),
            DisplayListItem::PopTextShadow => {
                shadows.pop();
            }
            DisplayListItem::Rect { bounds, .. } => {
                let bound = item_with_bound_color(it, list.color_binding(frame.push + 1 + at), render_state);
                let DisplayListItem::Rect { color, .. } = &*bound else {
                    continue;
                };
                if color.a == 0 {
                    continue;
                }
                let r = bounds.inner();
                let corner = |x: f32, y: f32| {
                    let (qx, qy) = device(x, y);
                    map.at(qx, qy)
                };
                let (x1, y1) = (r.origin.x + r.size.width, r.origin.y + r.size.height);
                let mut path = PathStorage::new();
                let (x, y) = corner(r.origin.x, r.origin.y);
                path.move_to(x, y);
                for (cx, cy) in [(x1, r.origin.y), (x1, y1), (r.origin.x, y1)] {
                    let (x, y) = corner(cx, cy);
                    path.line_to(x, y);
                }
                path.close_polygon(PATH_FLAGS_NONE);
                let fill = Rgba8::new(u32::from(color.r), u32::from(color.g), u32::from(color.b), u32::from(color.a));
                agg_fill_path_clipped(pixmap, &mut path, &fill, FillingRule::NonZero, clip);
            }
            DisplayListItem::Text { glyphs, font_hash, font_size_px, color, .. } => {
                crate::cpu_counters::count(crate::cpu_counters::Counter::TextRuns, 1);
                crate::cpu_counters::count(crate::cpu_counters::Counter::Glyphs, glyphs.len() as u64);
                let (Some(ink), Some(first)) = (it.visual_bounds(), glyphs.first()) else {
                    continue;
                };
                let run = TurnedRun { glyphs, font_hash: *font_hash, font_size_px: *font_size_px, ink };
                // The shadows first, outermost first, then the ink: as the upright path paints.
                let pictures = shadows
                    .iter()
                    .filter(|sh| sh.color.a > 0)
                    .map(|sh| {
                        ((shadow_px(&sh.offset_x.inner), shadow_px(&sh.offset_y.inner)), Some(halo_of(sh)), sh.color)
                    })
                    .chain(core::iter::once(((0.0, 0.0), None, *color)));
                for (shift, halo, tint) in pictures {
                    if tint.a == 0 {
                        continue;
                    }
                    // The run's first pen, moved by the shadow's offset, through the matrix,
                    // rounded to a whole device pixel: the picture's anchor.
                    let (q0x, q0y) = device(first.point.x + shift.0, first.point.y + shift.1);
                    let (ax, ay) = map.at(q0x, q0y);
                    if !(ax.is_finite() && ay.is_finite() && ax.abs() < 1.0e6 && ay.abs() < 1.0e6) {
                        continue;
                    }
                    let anchor = (ax.round() as i32, ay.round() as i32);
                    // Culled by the clip BEFORE its picture is built: a picture is rasterised
                    // whole and kept under the matrix's linear part, so a zoom step of a
                    // text-only frame of 2,000 lines built 2,000 pictures at the new scale,
                    // on screen or not (A2-cpu-R06).
                    let Some((bx0, by0, w, h)) =
                        turned_picture_box(&run, &map, turned_halo(halo, grow).2, dpi_factor)
                    else {
                        continue;
                    };
                    let corner = (anchor.0 + bx0, anchor.1 + by0);
                    if corner.0 >= clip_box.2
                        || corner.1 >= clip_box.3
                        || corner.0 + w <= clip_box.0
                        || corner.1 + h <= clip_box.1
                    {
                        continue;
                    }
                    let Some(picture) =
                        turned_run_picture(&run, &map, grow, halo, dpi_factor, font_manager, glyph_cache)
                    else {
                        continue;
                    };
                    let place = (anchor.0 + picture.offset.0, anchor.1 + picture.offset.1);
                    let (x0, y0) = (clip_box.0.max(place.0), clip_box.1.max(place.1));
                    let x1 = clip_box.2.min(place.0 + picture.width as i32);
                    let y1 = clip_box.3.min(place.1 + picture.height as i32);
                    if x1 > x0 && y1 > y0 {
                        blit_tinted_coverage(
                            pixmap,
                            &picture.coverage,
                            picture.width,
                            (x0 - place.0) as u32,
                            (y0 - place.1) as u32,
                            (x1 - x0) as u32,
                            (y1 - y0) as u32,
                            x0,
                            y0,
                            tint,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    true
}

/// One text run of a turned frame.
/// A turned halo's blur radius and spread in the picture's pixels, and how far the picture
/// reaches past the turned ink for it (2 px for the ink alone). `halo` is the blur's radius
/// and the spread in device px before the matrix's scale `grow`.
fn turned_halo(halo: Option<(u32, f32)>, grow: f64) -> (u32, f32, f64) {
    let (radius, spread) = halo.map_or((0, 0.0), |(r, s)| {
        (((f64::from(r) * grow).round() as u32).min(254), s * grow as f32)
    });
    (radius, spread, f64::from(radius) + f64::from(spread).ceil() + 2.0)
}

/// The box of the picture of `run` through the linear part of `map`, `reach` px around the
/// turned ink: its corner from the run's anchor (the first pen through the matrix) and its
/// size, device px. `None` when no picture can be made of it: a corner is not finite or a
/// million pixels away, or a side is over 8192 px. The one place that decides it, asked
/// before a frame is drawn directly (such a run makes the frame a group, which paints the
/// part that lands), before a picture is built (to cull it by the clip) and when it is built.
fn turned_picture_box(run: &TurnedRun<'_>, map: &TurnedMap, reach: f64, dpi_factor: f32) -> Option<(i32, i32, i32, i32)> {
    let first = run.glyphs.first()?;
    let rel = |x: f32, y: f32| {
        (
            f64::from((x - first.point.x) * dpi_factor),
            f64::from((y - first.point.y) * dpi_factor),
        )
    };
    let through = |x: f64, y: f64| (map.a * x + map.b * y, map.c * x + map.d * y);
    let (ix1, iy1) = (run.ink.origin.x + run.ink.size.width, run.ink.origin.y + run.ink.size.height);
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (cx, cy) in [(run.ink.origin.x, run.ink.origin.y), (ix1, run.ink.origin.y), (ix1, iy1), (run.ink.origin.x, iy1)] {
        let (rx, ry) = rel(cx, cy);
        let (x, y) = through(rx, ry);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    if ![x0, y0, x1, y1].iter().all(|v| v.is_finite() && v.abs() < 1.0e6) {
        return None;
    }
    let (bx0, by0) = ((x0 - reach).floor() as i32, (y0 - reach).floor() as i32);
    let (w, h) = (((x1 + reach).ceil() as i32 - bx0).max(1), ((y1 + reach).ceil() as i32 - by0).max(1));
    (w <= 8192 && h <= 8192).then_some((bx0, by0, w, h))
}

struct TurnedRun<'a> {
    glyphs: &'a [GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    /// Where its glyphs can paint, list space.
    ink: LogicalRect,
}

/// The coverage picture of `run` through the linear part of `map`: its ink (`halo` `None`),
/// or its halo (the blur's radius and the spread, device px before the matrix's scale `grow`).
/// One byte a pixel; `offset` is the picture's corner from the run's anchor (its first pen,
/// through the matrix). Kept in the CPU renderer's halo store under what it depends on (never
/// the place), so it is drawn once: glyph outlines through `ConvTransform` at the label's
/// size and turn, and for a halo `dilate_disk` and the stack blur on that coverage (both the
/// same in every direction, so the halo of the turned ink is the turned halo).
///
/// The glyphs' pens are taken relative to the first, in quarter device pixels (the glyph
/// cache's step) for the key and exactly for the picture.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::too_many_lines)]
fn turned_run_picture(
    run: &TurnedRun<'_>,
    map: &TurnedMap,
    grow: f64,
    halo: Option<(u32, f32)>,
    dpi_factor: f32,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
) -> Option<std::sync::Arc<super::text_halo::CpuHalo>> {
    use std::hash::{Hash, Hasher};

    use super::text_halo::{cpu_halos_are_kept, keep_cpu_halo, kept_cpu_halo, CpuHalo, MAX_CPU_HALO_PIXELS};

    let first = run.glyphs.first()?;
    // A pen relative to the first, device px before the matrix.
    let rel = |x: f32, y: f32| {
        (
            f64::from((x - first.point.x) * dpi_factor),
            f64::from((y - first.point.y) * dpi_factor),
        )
    };
    let (radius, spread, reach) = turned_halo(halo, grow);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    0xD1_u8.hash(&mut hasher);
    halo.is_some().hash(&mut hasher);
    radius.hash(&mut hasher);
    spread.to_bits().hash(&mut hasher);
    run.font_hash.font_hash.hash(&mut hasher);
    run.font_hash.synthetic_bold.hash(&mut hasher);
    run.font_size_px.to_bits().hash(&mut hasher);
    dpi_factor.to_bits().hash(&mut hasher);
    for v in [map.a, map.b, map.c, map.d] {
        v.to_bits().hash(&mut hasher);
    }
    run.glyphs.len().hash(&mut hasher);
    for g in run.glyphs {
        let (x, y) = rel(g.point.x, g.point.y);
        g.index.hash(&mut hasher);
        ((x * 4.0).floor() as i64).hash(&mut hasher);
        ((y * 4.0).floor() as i64).hash(&mut hasher);
    }
    let key = hasher.finish();
    if cpu_halos_are_kept() {
        if let Some(kept) = kept_cpu_halo(key) {
            return kept;
        }
    }

    // The box the turned ink can reach, from the anchor: the ink's corners through the matrix.
    let (bx0, by0, w, h) = turned_picture_box(run, map, reach, dpi_factor)?;
    let mut buffer = AzulPixmap::new_transparent(w as u32, h as u32)?;

    let font_ref = font_manager.resolve_font_by_hash(run.font_hash.font_hash)?;
    let parsed_font: &ParsedFont = crate::font_ref_to_parsed_font(&font_ref);
    let units_per_em = f32::from(parsed_font.font_metrics.units_per_em);
    if units_per_em <= 0.0 {
        return None;
    }
    let effective_px = run.font_size_px * dpi_factor;
    let scale = f64::from(effective_px / units_per_em);
    let bold_px = synthetic_bold_px(run.font_hash, effective_px);
    // The face is in use (`FontManager::evict_unused`), though cached paths decode nothing.
    parsed_font.touch();
    {
        let mut pooled = PooledTextRaster::take();
        let (ras, sl) = pooled.parts();
        for g in run.glyphs {
            let glyph_index = g.index as u16;
            // The unhinted outline, in font units: hinting is for an upright pixel grid. The
            // glyph is decoded only when its path is not cached (`get_or_build_decoding`).
            let Some(cached) = glyph_cache.get_or_build_decoding(
                run.font_hash.font_hash,
                glyph_index,
                parsed_font,
                0,
                || parsed_font.get_or_decode_glyph(glyph_index),
            ) else {
                continue;
            };
            let (rx, ry) = rel(g.point.x, g.point.y);
            // Font units -> device px at the pen -> through the matrix -> into the buffer.
            let mut t = TransAffine::new_scaling_uniform(scale);
            t.multiply(&TransAffine::new_translation(rx, ry));
            t.multiply(&TransAffine::new_custom(map.a, map.c, map.b, map.d, 0.0, 0.0));
            t.multiply(&TransAffine::new_translation(-f64::from(bx0), -f64::from(by0)));
            let bold = (bold_px > 0.0 && scale > 0.0).then(|| {
                let mut verts = cached.path.vertices().to_vec();
                embolden::embolden_vertices(&mut verts, f64::from(bold_px) / scale);
                verts
            });
            let verts = bold.as_deref().unwrap_or_else(|| cached.path.vertices());
            let mut outline = agg_rust::conv_curve::ConvCurve::new(agg_rust::conv_transform::ConvTransform::new(
                crate::glyph_cache::SliceVertexSource::new(verts),
                t,
            ));
            ras.add_path(&mut outline, 0);
        }
        let stride = w * 4;
        let mut ra = unsafe { RowAccessor::new_with_buf(buffer.data.as_mut_ptr(), w as u32, h as u32, stride) };
        let pf = PixfmtRgba32::new(&mut ra);
        let mut rb = RendererBase::new(pf);
        render_scanlines_aa_solid(ras, sl, &mut rb, &Rgba8::new(255, 255, 255, 255));
    }
    if halo.is_some() {
        crate::cpu_counters::count(crate::cpu_counters::Counter::HalosDrawn, 1);
        dilate_disk(&mut buffer, spread);
        if radius > 0 {
            let mut ra = unsafe { RowAccessor::new_with_buf(buffer.data.as_mut_ptr(), w as u32, h as u32, w * 4) };
            stack_blur_rgba32(&mut ra, radius, radius);
        }
    }
    let picture = std::sync::Arc::new(CpuHalo {
        width: w as u32,
        height: h as u32,
        offset: (bx0, by0),
        coverage: buffer.data().chunks_exact(4).map(|p| p[3]).collect(),
    });
    if cpu_halos_are_kept() && (i64::from(w) * i64::from(h)) as u64 <= MAX_CPU_HALO_PIXELS {
        keep_cpu_halo(key, Some(std::sync::Arc::clone(&picture)));
    }
    Some(picture)
}

/// The most items of a kept picture's frame that may differ between its list and the list
/// that replaced it for the picture to be repaired instead of dropped.
const MAX_PATCHED_ITEMS: usize = 64;

/// The boxes of a kept surface (surface px, half-open, disjoint) that show something else in
/// `new` than in `old`, the list the surface was painted from: where an item of the frame
/// pushed at `key.push` differs. `None` when the surface cannot be repaired box by box: the
/// lists do not pair (another length, a frame that ends elsewhere), a push or a pop differs, a
/// value inside the frame moved too, a scroll frame is inside (its content is placed by an
/// offset), an item differs inside a frame that is not a leaf or has a perspective matrix, more
/// than [`MAX_PATCHED_ITEMS`] items differ, or the boxes cover more than half the surface.
///
/// An item at list position `p` is at surface pixel `(p - placement.offset) dpi`. An item
/// directly in the frame is damaged where it paints: its visual bounds in either list, widened
/// by the frame's text-shadow reach and by 2 px of anti-aliasing. An item inside an inner
/// (leaf) frame F is damaged where F can paint: F's extent, widened, turned about F's origin
/// `o` by F's live matrix (`o + M (q - o)`, the walk's rule), and its bounding box taken.
///
/// Example (scale factor 1, offset (0, 0)): a tile image at (205, 155) 40 x 40 is replaced:
/// box [203, 153, 247, 197].
#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
fn stale_boxes(
    old: &DisplayList,
    new: &DisplayList,
    key: &super::retained::SurfaceKey,
    placement: &super::retained::Placement,
    dpi_factor: f32,
    render_state: &CpuRenderState,
) -> Option<Vec<[i32; 4]>> {
    if old.items.len() != new.items.len() {
        return None;
    }
    let (old_index, index) = (old.frame_index(), new.frame_index());
    let frame = index.frame_at(key.push)?;
    let old_frame = old_index.frame_at(key.push)?;
    if !frame.self_contained || old_frame.end != frame.end {
        return None;
    }
    // The picture in the surface is placed by the frame's EXTENT (`g = floor((extent - acc)
    // dpi)`, and a hit composites with the live one): a new list whose frame paints from
    // another corner - an item past the frame's box came or went - can have a surface of the
    // same size and phase, the same key, and another place. The repaired picture was then
    // composited displaced by the whole pixels the extent moved (A2-cpu-R03). Only a frame
    // that paints the same extent in both lists is repaired.
    let extent_in = |i: &crate::solver3::display_list::FrameIndex, items: &[DisplayListItem], laid_out: LogicalRect| {
        i.extent_with(items, key.push, &render_state.transforms).unwrap_or(laid_out)
    };
    if extent_in(&old_index, &old.items, old_frame.extent) != extent_in(&index, &new.items, frame.extent) {
        return None;
    }
    let inner = index.inner_of(key.push);
    if inner.is_some_and(|inner| !inner.scroll_ids.is_empty())
        || values_inside(inner, render_state) != Some(key.values)
    {
        return None;
    }
    let (w, h) = (key.size.0 as i32, key.size.1 as i32);
    let to_surface = |r: &LogicalRect, reach: f32| -> [i32; 4] {
        let x0 = ((r.origin.x - reach - placement.offset.0) * dpi_factor).floor() as i32 - 2;
        let y0 = ((r.origin.y - reach - placement.offset.1) * dpi_factor).floor() as i32 - 2;
        let x1 = ((r.origin.x + r.size.width + reach - placement.offset.0) * dpi_factor).ceil() as i32 + 2;
        let y1 = ((r.origin.y + r.size.height + reach - placement.offset.1) * dpi_factor).ceil() as i32 + 2;
        [x0.clamp(0, w), y0.clamp(0, h), x1.clamp(0, w), y1.clamp(0, h)]
    };
    let paints = |item: &DisplayListItem| item.visual_bounds().or_else(|| item.bounds());
    let mut boxes: Vec<[i32; 4]> = Vec::new();
    let mut differing = 0usize;
    // The outermost frame inside the kept one that is open at the item.
    let mut inside: Option<crate::solver3::display_list::FrameEntry> = None;
    for at in key.push..=frame.end.min(new.items.len().saturating_sub(1)) {
        let (a, b) = (&old.items[at], &new.items[at]);
        if at > key.push && inside.is_none() && matches!(b, DisplayListItem::PushReferenceFrame { .. }) {
            inside = index.frame_at(at);
        }
        let closes_inner = inside.is_some_and(|f| f.end == at);
        let same = core::mem::discriminant(a) == core::mem::discriminant(b)
            && a.is_visually_equal(b)
            && old.color_binding(at).map(|k| k.id) == new.color_binding(at).map(|k| k.id);
        if !same {
            if a.is_state_management() || b.is_state_management() {
                return None;
            }
            differing += 1;
            if differing > MAX_PATCHED_ITEMS {
                return None;
            }
            match inside {
                None => {
                    for r in [paints(a), paints(b)].into_iter().flatten() {
                        boxes.push(to_surface(&r, frame.shadow_reach));
                    }
                }
                Some(f) => {
                    let old_f = old_index.frame_at(f.push)?;
                    let DisplayListItem::PushReferenceFrame { transform_key, initial_transform, bounds } =
                        &new.items[f.push]
                    else {
                        return None;
                    };
                    if !f.leaf || !f.self_contained {
                        return None;
                    }
                    let m = render_state
                        .transforms
                        .get(&transform_key.id)
                        .map_or(&initial_transform.m, |t| &t.m);
                    if m[0][3].abs() > 1e-6 || m[1][3].abs() > 1e-6 || (m[3][3] - 1.0).abs() > 1e-6 {
                        return None;
                    }
                    let o = bounds.inner().origin;
                    let reach = f.shadow_reach.max(old_f.shadow_reach);
                    for e in [f.extent, old_f.extent] {
                        let (mut x0, mut y0) = (f32::INFINITY, f32::INFINITY);
                        let (mut x1, mut y1) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
                        for (cx, cy) in [
                            (e.origin.x - reach, e.origin.y - reach),
                            (e.origin.x + e.size.width + reach, e.origin.y - reach),
                            (e.origin.x - reach, e.origin.y + e.size.height + reach),
                            (e.origin.x + e.size.width + reach, e.origin.y + e.size.height + reach),
                        ] {
                            let (qx, qy) = (cx - o.x, cy - o.y);
                            let x = o.x + qx * m[0][0] + qy * m[1][0] + m[3][0];
                            let y = o.y + qx * m[0][1] + qy * m[1][1] + m[3][1];
                            x0 = x0.min(x);
                            y0 = y0.min(y);
                            x1 = x1.max(x);
                            y1 = y1.max(y);
                        }
                        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
                            return None;
                        }
                        let landed = LogicalRect {
                            origin: LogicalPosition { x: x0, y: y0 },
                            size: LogicalSize { width: x1 - x0, height: y1 - y0 },
                        };
                        boxes.push(to_surface(&landed, 0.0));
                    }
                }
            }
        }
        if closes_inner {
            inside = None;
        }
    }
    boxes.retain(|b| b[2] > b[0] && b[3] > b[1]);
    let boxes = disjoint_damage(boxes);
    let area: i64 = boxes.iter().map(|b| i64::from(b[2] - b[0]) * i64::from(b[3] - b[1])).sum();
    (area * 2 <= i64::from(w) * i64::from(h)).then_some(boxes)
}

/// Before a frame is painted: the kept surfaces whose list was replaced (a tile that arrived,
/// a label that was edited: the same items but a few) are repaired instead of dropped. The
/// boxes where the new list differs ([`stale_boxes`]) are cleared and painted again from the
/// new list, into the kept surface, with the stacks a walk has inside the group; the surface
/// then belongs to the new list. A surface that cannot be repaired is dropped. Called by the
/// backend with the frame's render state, before `RetainedFrames::begin_frame`.
///
/// # Errors
///
/// Returns an error string if a repaint fails (the surface is dropped then).
pub fn refresh_retained_surfaces(
    render_state: &CpuRenderState,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    dpi_factor: f32,
) -> Result<(), String> {
    use crate::cpu_counters::{count, Counter};
    let Some(retained) = render_state.retained.as_deref() else {
        return Ok(());
    };
    let current: Vec<usize> = render_state
        .root_display_list
        .iter()
        .chain(render_state.virtual_view_display_lists.values())
        .map(|list| std::sync::Arc::as_ptr(list) as usize)
        .collect();
    for mut entry in retained.take_stale(&current) {
        let repaired = (|| -> Option<Result<(), String>> {
            // The picture of an opacity group is not repaired box by box: dropped, and the
            // group is painted (and kept) by the next walk that reaches it.
            if entry.fade {
                return None;
            }
            let new = list_in_slot(entry.placement.slot, render_state)?;
            if entry.key.dpi_bits != dpi_factor.to_bits()
                || entry.key.settings != paint_settings(glyph_cache, font_manager, render_state)
            {
                return None;
            }
            let boxes = stale_boxes(&entry.list, &new, &entry.key, &entry.placement, dpi_factor, render_state)?;
            let frame = new.frame_index().frame_at(entry.key.push)?;
            let surface = std::sync::Arc::get_mut(&mut entry.surface)?;
            let (w, h) = entry.key.size;
            for b in &boxes {
                let (bw, bh) = (b[2] - b[0], b[3] - b[1]);
                surface.fill_rect(b[0], b[1], bw, bh, 0, 0, 0, 0);
                // What a walk holds inside the group: the box as its clip, the surface's own
                // box as the clip text shadows are made under, the group's offset, and the
                // group itself on the mask stack (text inside it is grayscale, items are
                // culled by the clip).
                let mut transform_stack = vec![TransAffine::new()];
                let mut clip_stack = vec![AzRect::from_xywh(b[0] as f32, b[1] as f32, bw as f32, bh as f32)];
                let mut real_clip_stack = vec![AzRect::from_xywh(0.0, 0.0, w as f32, h as f32)];
                let mut mask_stack = vec![MaskEntry::Transform(ReferenceFrameGroup::Isolated(Box::new(
                    TransformGroup {
                        width: w,
                        height: h,
                        matrix: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                        outer_clip: None,
                        lands: true,
                    },
                )))];
                let mut surfaces: Vec<AzulPixmap> = Vec::new();
                let mut scroll_offset_stack = vec![entry.placement.offset];
                let mut text_shadow_stack: Vec<StyleBoxShadow> = Vec::new();
                // As the walk that painted the surface culls: a frame of the window's own list
                // is painted with its turned text frames as groups (`InGroups`: inside the
                // isolated group on the mask stack it culls by the clip), a view's list with
                // them drawn directly (`ActiveClip`). Repairing a root-list surface as a
                // view's drew the turned text in the repaired boxes another way than the rest
                // of the same caption (A2-cpu-R04).
                let cull = if entry.placement.slot == usize::MAX {
                    WalkCull::InGroups
                } else {
                    WalkCull::ActiveClip
                };
                if let Err(e) = walk_range(
                    &new,
                    frame.push + 1..frame.end,
                    cull,
                    true,
                    surface,
                    dpi_factor,
                    renderer_resources,
                    font_manager,
                    glyph_cache,
                    &mut transform_stack,
                    &mut clip_stack,
                    &mut real_clip_stack,
                    &mut mask_stack,
                    &mut surfaces,
                    &mut scroll_offset_stack,
                    &mut text_shadow_stack,
                    render_state,
                ) {
                    return Some(Err(e));
                }
            }
            entry.key.list = std::sync::Arc::as_ptr(&new) as usize;
            entry.list = new;
            Some(Ok(()))
        })();
        match repaired {
            Some(Ok(())) => {
                count(Counter::RetainedPatched, 1);
                retained.put_back(entry);
            }
            Some(Err(e)) => {
                count(Counter::RetainedInvalidations, 1);
                return Err(e);
            }
            None => count(Counter::RetainedInvalidations, 1),
        }
    }
    Ok(())
}

/// Can nothing of the reference frame `frame`, pushed by `item`, paint in this walk, so that
/// the walk may step from its push to past its pop?
///
/// - A frame that is not self-contained (`FrameEntry::self_contained`) is always walked.
/// - A turned or scaled frame (an isolated group) paints nothing when no part of its surface
///   can land in the pixels below: the test `ReferenceFrameGroup::begin` makes, on the same
///   geometry, before a surface exists. It is counted in `groups_empty` here.
/// - A frame that is the identity or a move paints its items where they are laid out, moved:
///   it paints nothing when its extent, moved and widened by the reach of the text shadows in
///   and around it, lies outside what the walk culls its items by (the clip of a view or of a
///   group, the damage rect). Only for a leaf (`FrameEntry::leaf`): a frame, scroll frame or
///   view inside places content where the extent does not say.
///
/// Example (a move): extent (100, 40) 60 x 20, matrix translate(30, 0), walk offset (10, 0),
/// shadow reach 3: painted box (100 - 10 + 30 - 3, 40 - 3) 66 x 26 = x 117 .. 183, y 37 .. 63.
/// Under a damage rect (0, 0) 110 x 100 it paints nothing; under (0, 0) 120 x 100 it may.
#[allow(clippy::too_many_arguments)]
fn frame_paints_nothing(
    item: &DisplayListItem,
    frame: &crate::solver3::display_list::FrameEntry,
    cull: &WalkCull,
    in_group: bool,
    below: (u32, u32),
    dpi_factor: f32,
    clip: Option<AzRect>,
    scroll_offset: (f32, f32),
    mask_stack: &[MaskEntry],
    text_shadow_stack: &[StyleBoxShadow],
    render_state: &CpuRenderState,
) -> bool {
    let DisplayListItem::PushReferenceFrame {
        transform_key,
        initial_transform,
        bounds,
    } = item
    else {
        return false;
    };
    if !frame.self_contained || !walk_culls_by_clip() || !(dpi_factor > 0.0) {
        return false;
    }
    // The walk paints at `pos - offset` (`scroll_rect` of `render_single_item`).
    let on_surface = |r: &LogicalRect| LogicalRect {
        origin: LogicalPosition {
            x: r.origin.x - scroll_offset.0,
            y: r.origin.y - scroll_offset.1,
        },
        size: r.size,
    };
    let m = render_state
        .transforms
        .get(&transform_key.id)
        .map_or(&initial_transform.m, |t| &t.m);
    let moved_by = if is_identity_2d(m) {
        Some((0.0, 0.0))
    } else {
        translation_2d(m)
    };
    let Some((tx, ty)) = moved_by else {
        let g = GroupGeometry::of(
            m,
            &on_surface(bounds.inner()),
            &on_surface(&frame.extent),
            dpi_factor,
            clip,
        );
        let part = group_visible_part(&g.matrix, g.outer_clip, below, (g.width, g.height));
        let lands =
            g.width > 0 && g.height > 0 && part.is_none_or(|(x0, y0, x1, y1)| x1 > x0 && y1 > y0);
        if !lands {
            crate::cpu_counters::count(crate::cpu_counters::Counter::GroupsEmpty, 1);
        }
        return !lands;
    };
    if !frame.leaf {
        return false;
    }
    let reach = text_shadow_stack
        .iter()
        .map(crate::solver3::display_list::text_shadow_reach)
        .fold(frame.shadow_reach, f32::max);
    let extent = on_surface(&frame.extent);
    let painted_box = LogicalRect {
        origin: LogicalPosition {
            x: extent.origin.x + tx - reach,
            y: extent.origin.y + ty - reach,
        },
        size: LogicalSize {
            width: extent.size.width + 2.0 * reach,
            height: extent.size.height + 2.0 * reach,
        },
    };
    if in_group || matches!(cull, WalkCull::ActiveClip) {
        // As `outside_active_clip` culls an item: by the clip, unless a filter or a mask is open.
        let Some(clip) = clip else {
            return false;
        };
        if mask_stack.iter().any(|e| match e {
            MaskEntry::Filter(_) => true,
            MaskEntry::ImageMask { width, height, .. } => *width > 0 && *height > 0,
            _ => false,
        }) {
            return false;
        }
        let clip_logical = LogicalRect {
            origin: LogicalPosition { x: clip.x / dpi_factor, y: clip.y / dpi_factor },
            size: LogicalSize { width: clip.width / dpi_factor, height: clip.height / dpi_factor },
        };
        return !rects_overlap_or_adjacent(&painted_box, &clip_logical, 0.0);
    }
    match cull {
        WalkCull::Damage(damage) => !rects_overlap_or_adjacent(&painted_box, damage, 0.0),
        WalkCull::InGroups | WalkCull::ActiveClip => false,
    }
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
        I::WebView { .. } => "dl:webview",
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

/// Physical-pixel damage boxes `[x0, y0, x1, y1]` made pairwise disjoint, so a
/// repaint pass per box paints every damaged pixel once. Two overlapping boxes
/// whose bounding box is barely larger than what they cover become that one
/// box (fewer passes); any others are cut apart, each keeping what the boxes
/// before it leave. A bounding box of everything that overlaps is often the
/// whole window: a grown window's stretched top bar meets the newly exposed
/// right strip, and their box covers the untouched middle as well.
fn disjoint_damage(mut boxes: Vec<[i32; 4]>) -> Vec<[i32; 4]> {
    let area = |b: &[i32; 4]| i64::from(b[2] - b[0]) * i64::from(b[3] - b[1]);
    'merge: loop {
        for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                let (a, b) = (boxes[i], boxes[j]);
                if !boxes_overlap(&a, &b) {
                    continue;
                }
                let bound = [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])];
                let shared = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
                let covered = area(&a) + area(&b) - area(&shared);
                // At most a quarter more than what the two cover.
                if area(&bound) * 4 <= covered * 5 {
                    boxes[i] = bound;
                    boxes.swap_remove(j);
                    continue 'merge;
                }
            }
        }
        break;
    }
    let mut out: Vec<[i32; 4]> = Vec::with_capacity(boxes.len());
    for b in boxes {
        let mut pieces = vec![b];
        for d in &out {
            pieces = pieces.into_iter().flat_map(|p| box_minus(p, *d)).collect();
        }
        out.extend(pieces);
    }
    out
}

const fn boxes_overlap(a: &[i32; 4], b: &[i32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// What of box `p` lies outside box `d`: up to four boxes (the bands above
/// and below `d`, then the parts left and right of it in between).
pub(crate) fn box_minus(p: [i32; 4], d: [i32; 4]) -> Vec<[i32; 4]> {
    if !boxes_overlap(&p, &d) {
        return vec![p];
    }
    let mut out = Vec::with_capacity(4);
    if d[1] > p[1] {
        out.push([p[0], p[1], p[2], d[1]]);
    }
    if d[3] < p[3] {
        out.push([p[0], d[3], p[2], p[3]]);
    }
    let (y0, y1) = (p[1].max(d[1]), p[3].min(d[3]));
    if d[0] > p[0] {
        out.push([p[0], y0, d[0], y1]);
    }
    if d[2] < p[2] {
        out.push([d[2], y0, p[2], y1]);
    }
    out
}

/// `AZ_DEBUG_DAMAGE=1` — log what each damaged repaint actually touched.
fn damage_logging_enabled() -> bool {
    use std::sync::OnceLock;
    static E: OnceLock<bool> = OnceLock::new();
    *E.get_or_init(|| std::env::var_os("AZ_DEBUG_DAMAGE").is_some())
}

/// A damage rect snapped to physical pixels, carried both ways so the clear,
/// the clip and the item filter all read the same box.
pub(crate) struct SnappedRect {
    pub(crate) x0: i32,
    pub(crate) y0: i32,
    pub(crate) x1: i32,
    pub(crate) y1: i32,
    pub(crate) logical: LogicalRect,
}

impl SnappedRect {
    pub(crate) fn from_physical(x0: i32, y0: i32, x1: i32, y1: i32, dpi_factor: f32) -> Self {
        Self {
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
        }
    }
}


/// The device pixels an opacity group snapshots and blends: its box (moved
/// by the scroll frames around it) inside the clip open around it - on the
/// incremental path the damage rect. Never the whole clip: a window's groups
/// each blended the whole damage area, 5 ms frames became 230 ms. Empty
/// (`DENY_ALL`) for a group with no box, which still pushes its entry.
pub(crate) fn opacity_group_region(
    bounds: &LogicalRect,
    clip: Option<AzRect>,
    dpi_factor: f32,
) -> AzRect {
    device_region(logical_rect_to_az_rect(bounds, dpi_factor), clip).unwrap_or(AzRect::DENY_ALL)
}

/// The whole device pixels an isolated group (an opacity group, a filter group) sets aside
/// for the box `boxed` under `clip`: the box's own edges outward (a pixel its content covers
/// in part is the group's), cut to the pixels a paint under the clip may write
/// (`AzRect::pixel_box`, the fills' rule - the ONE clip rule of the CPU path, A2-cpu-06).
/// `None` when no pixel is left.
///
/// Both groups used to round on their own: the opacity group took `x as i32` and
/// `width as u32` (a box 10.6 .. 34.4 lost column 33), the filter group `floor` / `ceil` of
/// box and clip alike (one column past a clip that ends between pixels).
fn device_region(boxed: Option<AzRect>, clip: Option<AzRect>) -> Option<AzRect> {
    let r = boxed?;
    let (mut x0, mut y0) = (r.x.floor() as i32, r.y.floor() as i32);
    let (mut x1, mut y1) = ((r.x + r.width).ceil() as i32, (r.y + r.height).ceil() as i32);
    if let Some(c) = clip {
        let (cx0, cy0, cx1, cy1) = c.pixel_box();
        x0 = x0.max(cx0);
        y0 = y0.max(cy0);
        x1 = x1.min(cx1);
        y1 = y1.min(cy1);
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    AzRect::from_xywh(x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32)
}

#[allow(clippy::many_single_char_names)] // r,g,b,a colour channels + loop indices
#[allow(clippy::tuple_array_conversions)] // explicit [r,g,b,a]->(r,g,b,a) is correct; .into() was
                                          // not
pub fn render_display_list_damaged(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
    damage_rects: &[LogicalRect],
) -> Result<Vec<LogicalRect>, String> {
    // The strip/damage raster body - the previously UNSPANNED majority of a
    // scroll frame's present time (7 of 10.2ms measured 2026-08-29).
    let _p = crate::probe::Probe::span("raster_damage_body");

    if damage_rects.is_empty() {
        return Ok(Vec::new()); // nothing changed
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
    let snapped: Vec<SnappedRect> = damage_rects.iter().filter_map(snap_out).collect();

    // Pairwise DISJOINT rects (rects that merely touch stay separate), so the
    // per-rect passes below clear + paint every damaged pixel EXACTLY once - no
    // double alpha-blend where rects overlapped.
    let boxes = disjoint_damage(snapped.iter().map(|r| [r.x0, r.y0, r.x1, r.y1]).collect());
    let rects: Vec<SnappedRect> = boxes
        .into_iter()
        .map(|[x0, y0, x1, y1]| SnappedRect {
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
        .collect();

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
            "[damage] {} rect(s) after merge (from {} requested), {total} px = {:.1}% of the \
             {pw_i}x{ph_i} window, {} display-list item(s)",
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

    // What a rect is cleared to before it is repainted (the same for every rect of the frame).
    static DEBUG_FILL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let clear = if *DEBUG_FILL.get_or_init(|| std::env::var_os("AZ_DEBUG_FILL").is_some()) {
        damage_clear_color()
    } else {
        // Explicit destructure, NOT `.into()`: the array→tuple conversion
        // resolved to the wrong thing and painted damage rects with a bad
        // clear colour, hiding content on every repaint.
        let [r, g, b, a] = render_state.clear_color;
        (r, g, b, a)
    };
    for sr in &rects {
        // One pass, or bands of rows on a few threads when the rect is large (`bands`).
        let (painted, skipped) = super::bands::paint_rect(
            display_list,
            pixmap,
            dpi_factor,
            renderer_resources,
            font_manager,
            glyph_cache,
            render_state,
            sr,
            Some(clear),
        )?;
        if damage_logging_enabled() {
            eprintln!(
                "[damage]   rect {}x{} @ ({}, {}) phys — painted {painted} item(s), skipped \
                 {skipped}",
                sr.x1 - sr.x0,
                sr.y1 - sr.y0,
                sr.x0,
                sr.y0,
            );
        }
    }

    // The rects this call actually CLEARED AND PAINTED. Overlapping requests
    // are merged into their bounding box above, so the painted area can be
    // larger than what was asked for: the caller has to present (and, on a
    // pool-order target, byte-convert) what was written, not what it wanted.
    Ok(rects.iter().map(|r| r.logical).collect())
}

/// Does painting `display_list` need the layered compositor? For a `filter` that the flat
/// walk does not apply - the walk isolates `flood()` and `composite()` groups itself, and a
/// blur (or any other filter) is applied to a layer when layers are composited - and for a
/// `backdrop-filter`, which only the layered compositor applies at all (it filters what is
/// composited under the group's layer; the walk's arm for it is empty).
///
/// Such a list is painted through layers on EVERY frame that paints, not only on a full
/// repaint: the walk that repaints damage would paint the group unfiltered inside the rects
/// it repaints (`CpuBackend::render_frame`).
#[must_use]
pub fn full_repaint_needs_layers(display_list: &DisplayList) -> bool {
    display_list.items.iter().any(|item| match item {
        DisplayListItem::PushFilter { filters, .. } => filters_need_a_layer(filters),
        DisplayListItem::PushBackdropFilter { filters, .. } => !filters.is_empty(),
        _ => false,
    })
}

/// A full repaint of a frame the caller has cleared: the damaged walk over the whole frame,
/// which is painted in bands on a few threads when it is large (`bands`), with the pixels of
/// the walk that repaints its parts later (one path for a frame and for its damage: what a
/// damage rect repaints is by construction what the full frame painted there).
///
/// # Errors
///
/// Returns an error string if rendering fails.
pub fn render_display_list_whole(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
) -> Result<(), String> {
    let _p = crate::probe::Probe::span("raster_whole");
    #[allow(clippy::cast_possible_wrap)] // a pixmap's sides
    let sr = SnappedRect::from_physical(0, 0, pixmap.width() as i32, pixmap.height() as i32, dpi_factor);
    super::bands::paint_rect(
        display_list,
        pixmap,
        dpi_factor,
        renderer_resources,
        font_manager,
        glyph_cache,
        render_state,
        &sr,
        None,
    )
    .map(|_| ())
}

/// One pass of the damaged walk: clear the rect `sr` of `pixmap` to `clear` and paint the items
/// of `display_list` that reach it, clipped to it, with stacks of its own. The unit a frame's
/// repaint is made of: a damage rect, or one band of rows of one (`bands`). A pass writes no
/// pixel outside `sr`, and what it writes inside does not depend on what lies outside, so the
/// passes of disjoint rects are independent of each other and of their order.
///
/// Returns how many items it painted and how many it culled.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)] // a rect inside the pixmap
pub(crate) fn paint_damage_rect(
    display_list: &DisplayList,
    pixmap: &mut AzulPixmap,
    dpi_factor: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    glyph_cache: &mut GlyphCache,
    render_state: &CpuRenderState,
    sr: &SnappedRect,
    // The colour the rect is cleared to first; `None` for a target the caller has cleared (a
    // full repaint fills the frame once).
    clear: Option<(u8, u8, u8, u8)>,
) -> Result<(usize, usize), String> {
    if let Some((cr, cg, cb, ca)) = clear {
        crate::cpu_counters::count(crate::cpu_counters::Counter::PxCleared, u64::from((sr.x1 - sr.x0) as u32) * u64::from((sr.y1 - sr.y0) as u32));
        pixmap.fill_rect(sr.x0, sr.y0, sr.x1 - sr.x0, sr.y1 - sr.y0, cr, cg, cb, ca);
    }

    let base_clip = AzRect::from_xywh(
        sr.x0 as f32,
        sr.y0 as f32,
        (sr.x1 - sr.x0) as f32,
        (sr.y1 - sr.y0) as f32,
    );
    let mut transform_stack = vec![TransAffine::new()];
    let mut clip_stack: Vec<Option<AzRect>> = vec![base_clip];
    let mut real_clip_stack: Vec<Option<AzRect>> = vec![None];
    let mut mask_stack: Vec<MaskEntry> = Vec::new();
    let mut surfaces: Vec<AzulPixmap> = Vec::new();
    let mut scroll_offset_stack: Vec<(f32, f32)> = vec![(0.0, 0.0)];
    let mut text_shadow_stack: Vec<StyleBoxShadow> = Vec::new();
    walk_items(
        display_list,
        WalkCull::Damage(sr.logical),
        true,
        pixmap,
        dpi_factor,
        renderer_resources,
        font_manager,
        glyph_cache,
        &mut transform_stack,
        &mut clip_stack,
        &mut real_clip_stack,
        &mut mask_stack,
        &mut surfaces,
        &mut scroll_offset_stack,
        &mut text_shadow_stack,
        render_state,
    )
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)] // software rasterizer: bounded pixel/coord/colour casts
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
#[allow(clippy::match_same_arms)]
// enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't
// merge)
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
    // What a `PushReferenceFrame` item's frame paints
    // (`reference_frame_extent`), `None` for every other item.
    frame_extent: Option<LogicalRect>,
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
    // The surfaces of the isolated reference frames open around the item,
    // innermost last (see `TransformGroup`); empty: it paints into `pixmap`.
    surfaces: &mut Vec<AzulPixmap>,
    scroll_offset_stack: &mut Vec<(f32, f32)>,
    text_shadow_stack: &mut Vec<StyleBoxShadow>,
    render_state: &CpuRenderState,
) -> Result<(), String> {
    use azul_css::props::style::border::BorderStyle;
    crate::cpu_counters::count(crate::cpu_counters::Counter::ItemsPainted, 1);
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

    // A reference frame opens or closes a surface, so it is the one item that
    // changes where the others paint: it is handled before the target is
    // chosen.
    match item {
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

            // The frame's content is painted MOVED by its live matrix - the
            // layered compositor's layer for this frame, in place. Without
            // this every damage rect repainted a transformed box at its
            // layout place (mid-slide ghosts; a lasting transform ghosted
            // wherever damage touched its layout box).
            let below = surfaces
                .last()
                .map_or((pixmap.width, pixmap.height), |s| (s.width, s.height));
            let group = ReferenceFrameGroup::begin(
                m,
                &scroll_rect(bounds.inner()),
                &scroll_rect(&frame_extent.unwrap_or_else(|| *bounds.inner())),
                surfaces,
                dpi_factor,
                clip_stack,
                real_clip_stack,
                scroll_offset_stack,
                below,
                render_state.promote.filter(|p| p.transform_key == transform_key.id),
            );
            mask_stack.push(MaskEntry::Transform(group));
            return Ok(());
        }
        DisplayListItem::PopReferenceFrame => {
            if transform_stack.len() > 1 {
                transform_stack.pop();
            }
            if matches!(mask_stack.last(), Some(MaskEntry::Transform(_))) {
                if let Some(MaskEntry::Transform(group)) = mask_stack.pop() {
                    group.finish(
                        pixmap,
                        surfaces,
                        clip_stack,
                        real_clip_stack,
                        scroll_offset_stack,
                    );
                }
            }
            return Ok(());
        }
        _ => {}
    }
    // Inside an isolated reference frame every item paints into the frame's
    // own surface; the frame's pop composites it into the one below.
    let pixmap = match surfaces.last_mut() {
        Some(surface) => surface,
        None => pixmap,
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
            outline,
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
                (*color, *width, outline),
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

            // The vertical radius of each corner: a percentage of the box's HEIGHT (the
            // horizontal one above is of its width), so `border-radius: 50%` on a box that is
            // not square is an ellipse, as WebRender draws it. A length is the same on both.
            let radius_y = BorderRadius {
                top_left: border_radius.top_left.to_pixels_internal(
                    bounds.0.size.height,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                top_right: border_radius.top_right.to_pixels_internal(
                    bounds.0.size.height,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                bottom_left: border_radius.bottom_left.to_pixels_internal(
                    bounds.0.size.height,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ),
                bottom_right: border_radius.bottom_right.to_pixels_internal(
                    bounds.0.size.height,
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
                render_border_xy(
                    pixmap,
                    &b,
                    c_top,
                    w_top,
                    s_top,
                    &simple_radius,
                    &radius_y,
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
            let source_clip = real_clip_stack.last().copied().flatten();
            for shadow in text_shadow_stack.iter() {
                render_text_shadow(
                    shadow,
                    glyphs,
                    *font_hash,
                    *font_size_px,
                    pixmap,
                    &text_clip,
                    clip,
                    source_clip,
                    renderer_resources,
                    font_manager,
                    dpi_factor,
                    glyph_cache,
                    (scroll_dx, scroll_dy),
                );
            }
            // LCD text needs an opaque backdrop (see `backdrop_is_opaque`): a
            // run over a transparent layer pixel takes grayscale coverage.
            let run = item
                .visual_bounds()
                .and_then(|ink| text_run_clip(&scroll_rect(&ink), clip, dpi_factor));
            // Inside a turned or scaled frame the group is resampled where it lands, and LCD
            // stripes smear into colour fringes there: grayscale coverage is what such a group
            // wants (and is cheaper). The settled picture is not resampled - its frame is the
            // identity or a translation, no group - and keeps its LCD text.
            // The same for a run inside a group that is blended at its pop (an opacity under
            // 1, a blend mode, an isolated filter): its pixels start transparent and are
            // composited afterwards. Decided from the groups that are open, not from the
            // pixels under the part of the run that the clip leaves: a damage rect or a band
            // seam that cut such a run where its backdrop was opaque on one side only gave
            // the two sides different text (LCD above the seam, grayscale below it).
            let grayscale = inside_isolated_transform(mask_stack)
                || inside_blended_group(mask_stack)
                || run.is_some_and(|run| !backdrop_is_opaque(pixmap, run));
            // The display list's uniform background is a HINT from the
            // ancestors (`compute_uniform_text_bg`: the first one painting an
            // opaque colour) - but a layer painted between that ancestor and
            // the text (a positioned gradient, a band of light) is the real
            // backdrop. The pre-blended tiles bake the hinted colour under
            // every glyph: AzPlayer's start strip showed BLACK boxes (its
            // black body under a blue layer). Trusted only when the pixels the
            // run covers ARE that colour.
            let item_uniform_bg = item_uniform_bg.filter(|(bg, _)| {
                run.is_some_and(|run| backdrop_is_colour(pixmap, run, *bg))
            });
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
                grayscale,
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
        DisplayListItem::Image {
            bounds,
            image,
            border_radius,
        } => {
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
                border_radius,
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
                // The bar fades: its colours at the opacity's share of their alpha (A2-gpu-07:
                // it used to be painted in full at any opacity above zero and gone at zero,
                // a step where WebRender shows the fade).
                let faded = |c: ColorU| ColorU {
                    a: (f32::from(c.a) * scrollbar_opacity.clamp(0.0, 1.0)).round() as u8,
                    ..c
                };
                // Render track
                if info.track_color.a > 0 {
                    render_rect(
                        pixmap,
                        &scroll_rect(info.track_bounds.inner()),
                        faded(info.track_color),
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
                            faded(info.button_color),
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
                            faded(info.button_color),
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
                        faded(info.thumb_color),
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
            // 1. The clip must live in the same coordinate space items draw in (`pos -
            //    accumulated_scroll`) — shift it via scroll_rect() like every drawing arm. A
            //    VirtualView child's PushClip otherwise lands at raw child-local coordinates on the
            //    window.
            // 2. A nested clip can only NARROW the active one. Pushing the rect verbatim let a
            //    child DL's own PushClip REPLACE the VirtualView composite clip, so the child
            //    painted over the whole window (the maps header/toolbar disappeared under the tile
            //    grid).
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
            // The radius. This arm used to destructure `border_radius` and drop
            // it, so every `overflow: hidden` + `border-radius` box clipped its
            // content SQUARE: the map's tiles ran straight into the corners of
            // its rounded frame while the frame's own background was round.
            if let Some(rect) = logical_rect_to_az_rect(&scroll_rect(bounds.inner()), dpi_factor) {
                if !border_radius.is_zero() {
                    let corners = rounded_clip_corners(pixmap, rect, border_radius, dpi_factor);
                    if !corners.is_empty() {
                        mask_stack.push(MaskEntry::RoundedClip {
                            corners,
                            clip_depth: clip_stack.len(),
                        });
                    }
                }
            }
        }
        DisplayListItem::PopClip => {
            // Restore this clip's rounded corners over whatever painted inside
            // it — before the clip itself goes.
            if matches!(
                mask_stack.last(),
                Some(MaskEntry::RoundedClip { clip_depth, .. }) if *clip_depth == clip_stack.len()
            ) {
                if let Some(entry) = mask_stack.pop() {
                    apply_mask(pixmap, &entry);
                }
            }
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
            if super::compositor::map_debug() {
                eprintln!(
                    "[cpu-vview] VirtualView item: child_dom_id={} found={} items={} bounds={:?} \
                     avail_ids={:?}",
                    child_dom_id.inner,
                    child_dl.is_some(),
                    child_dl.as_ref().map_or(0, |d| d.items.len()),
                    bounds.inner(),
                    render_state
                        .virtual_view_display_lists
                        .keys()
                        .map(|k| k.inner)
                        .collect::<Vec<_>>(),
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
                // The view's own frames open their surfaces here and
                // composite them into what this walk paints into.
                let mut view_surfaces: Vec<AzulPixmap> = Vec::new();
                // A view's list is a map or a page far larger than the box it shows. Its bound
                // colours are read like the window's own (a fade inside a view moves a value
                // in the view's cache, and `WindowGpuValues` holds every DOM's).
                walk_items(
                    &child_dl,
                    WalkCull::ActiveClip,
                    true,
                    pixmap,
                    dpi_factor,
                    renderer_resources,
                    font_manager,
                    glyph_cache,
                    transform_stack,
                    clip_stack,
                    real_clip_stack,
                    mask_stack,
                    &mut view_surfaces,
                    scroll_offset_stack,
                    text_shadow_stack,
                    render_state,
                )?;
                scroll_offset_stack.pop();
                real_clip_stack.pop();
                clip_stack.pop();
            }
        }
        DisplayListItem::VirtualViewPlaceholder { .. } =>
        {
            #[cfg(feature = "std")]
            if super::compositor::map_debug() {
                eprintln!(
                    "[cpu-vview] VirtualViewPlaceholder hit (NOT swapped to a VirtualView item — \
                     nothing composites)"
                );
            }
        }
        // The native web view is the platform's to draw, over the window:
        // the item only reserves its rect (`DisplayListItem::WebView`).
        DisplayListItem::WebView { .. } => {}

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
            mix_blend_mode,
        } => {
            // Live value first — the damaged/incremental path must fade at the
            // same opacity the composited path shows.
            let opacity = &opacity_key
                .and_then(|k| render_state.opacities.get(&k.id).copied())
                .unwrap_or(*opacity);
            // The group blends over its box inside the clip open around it
            // (a damage rect, on the incremental path): blending the whole
            // clip per group cost a full-window blend for every group of a
            // window. A group with an empty box still leaves its entry - it
            // pushed nothing before, and its pop then took the entry of the
            // frame around it (a translated box: every later item painted
            // moved by it).
            // All of what the group paints, not only its node's box (`frame_extent` carries
            // the group's extent when its content paints past the box: A2-gpu-06, a child
            // that overflows a fading box, or its shadow, was painted at full strength).
            let rect = opacity_group_region(
                &scroll_rect(&frame_extent.unwrap_or_else(|| *bounds.inner())),
                clip_stack.last().copied().flatten(),
                dpi_factor,
            );
            // The group is isolated, as the compositor's layer for it is:
            // its region is set aside and cleared, the group paints into the
            // cleared pixels, and the pop composites them over what was set
            // aside with the layer blend (`blend_over`), or with the node's
            // `mix-blend-mode` (`blend_pixel`): a multiply needs the pixels
            // below, so a blending group is never a layer of its own.
            let snap = snapshot_region(
                pixmap,
                rect.x as i32,
                rect.y as i32,
                rect.width as u32,
                rect.height as u32,
            );
            write_region(
                pixmap,
                &vec![0u8; snap.len()],
                rect.width as u32,
                rect.height as u32,
                rect.x as i32,
                rect.y as i32,
            );
            mask_stack.push(MaskEntry::Opacity {
                snapshot: snap,
                rect,
                opacity: *opacity,
                blend: *mix_blend_mode,
            });
        }
        DisplayListItem::PopOpacity => {
            // Only an opacity group's own entry: the pop never takes the
            // entry of a frame or clip around it.
            let entry = if matches!(mask_stack.last(), Some(MaskEntry::Opacity { .. })) {
                mask_stack.pop()
            } else {
                None
            };
            if let Some(MaskEntry::Opacity {
                snapshot,
                rect,
                opacity,
                blend,
            }) = entry
            {
                let x = rect.x as i32;
                let y = rect.y as i32;
                let w = rect.width as u32;
                let h = rect.height as u32;
                // What the group painted into its cleared region, then the
                // backdrop back, then the one over the other at its opacity.
                let painted = snapshot_region(pixmap, x, y, w, h);
                write_region(pixmap, &snapshot, w, h, x, y);
                blend_group_pixels(pixmap, &painted, w, (0, 0), (x, y, w, h), opacity, blend);
            }
        }

        // --- Reference frames (CSS transforms) ---
        // Opened and closed above, before the item's target was chosen.
        DisplayListItem::PushReferenceFrame { .. } | DisplayListItem::PopReferenceFrame => {}

        // --- Filter effects ---
        //
        // A blur is realized by the compositor layer path, which allocates a
        // dedicated pixbuf for the filtered subtree in
        // `allocate_layers_from_display_list` and applies the filters at
        // composite time via `apply_layer_filters`; the content between
        // Push/PopFilter is rendered into that layer's pixbuf by this very
        // function, so for it the markers carry no work at item level.
        //
        // `flood()` and `composite()` are different: they need the group's
        // OWN pixels apart from whatever is behind it (a flood fills the
        // group, `composite(in)` keeps it only where the group painted - the
        // icon tint), and no layer is promoted for them. Such a group is
        // ISOLATED right here: the region under it is set aside and cleared,
        // the group paints onto transparent pixels, and the pop filters them
        // and composites the result back over what was there. Every push
        // leaves an entry (`None` when nothing is isolated), so a pop always
        // closes its own push however filters nest.
        DisplayListItem::PushFilter { bounds, filters } => {
            let group = if filter_group_needs_isolation(filters) {
                // The group's box INSIDE the clip, as an opacity group's region is: outside
                // it the group paints nothing in this pass, and the box there holds the
                // pixels of the frame before. Taking the whole box in every pass (every
                // damage rect, every band) filtered nothing over them again: a bare
                // translucent `flood()` got darker with every caret blink elsewhere in the
                // window, and bands and one pass differed (A2-cpu-R10). No group when the
                // box is outside the clip.
                let clip = *clip_stack.last().unwrap();
                device_region(logical_rect_to_az_rect(&scroll_rect(bounds.inner()), dpi_factor), clip)
                    .map(|r| FilterGroup::begin(pixmap, &r, filters))
            } else {
                None
            };
            mask_stack.push(MaskEntry::Filter(group.map(Box::new)));
        }
        DisplayListItem::PopFilter => {
            if matches!(mask_stack.last(), Some(MaskEntry::Filter(_))) {
                if let Some(MaskEntry::Filter(Some(group))) = mask_stack.pop() {
                    group.finish(pixmap, dpi_factor);
                }
            }
        }

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
            // The path the mask was rasterised from, for renderers that clip
            // to paths. This one composites the mask image.
            geometry: _,
        } => {
            let mr = &scroll_rect(mask_rect.inner());
            // Both regions below have to COVER every pixel the masked
            // drawing can touch, so each is its box's pixel-aligned BOUNDING
            // BOX: floor the near edge, ceil the far one. Truncating the
            // origin and ceiling the SIZE is not the same thing - for a box at
            // 9.5..25.5 it gave rows 9..25, leaving row 25 outside the region
            // `apply_mask` blends, so the solid rect under the mask survived
            // there as one fully-opaque row. A titlebar centring a 16px glyph
            // in a 24px button puts every icon on exactly that half pixel.
            //
            // `floor`, not a cast: `as i32` truncates toward zero, which for
            // a negative origin rounds the wrong way and leaks the same row
            // on the other side.
            //
            // The mask is then resampled (bilinearly) onto a region up to one
            // pixel larger in each axis, which spreads its coverage by that
            // much. That is the honest answer for a mask whose rect sits
            // between pixels, and it is what the edge is for.
            let pixel_box = |r: &LogicalRect| -> (i32, i32, u32, u32) {
                let x0 = (r.origin.x * dpi_factor).floor();
                let y0 = (r.origin.y * dpi_factor).floor();
                let x1 = ((r.origin.x + r.size.width) * dpi_factor).ceil();
                let y1 = ((r.origin.y + r.size.height) * dpi_factor).ceil();
                (
                    x0 as i32,
                    y0 as i32,
                    (x1 - x0).max(0.0) as u32,
                    (y1 - y0).max(0.0) as u32,
                )
            };
            let (mask_x, mask_y, mask_w, mask_h) = pixel_box(mr);

            // THE MASKED REGION IS THE ELEMENT, NOT THE MASK RECT.
            //
            // Coverage outside the mask image is zero - that is what a mask
            // means, and what WebRender's clip-image-mask does. Masking only
            // the mask rect left everything the mask does not reach fully
            // painted, so the two backends disagreed about every mask smaller
            // than its element.
            //
            // Only where it lies on the pixmap, though: nothing is drawn
            // anywhere else, and an element a billion pixels wide (an SVG of
            // `width="1e9"`) must not cost a billion pixels of snapshot,
            // mask and resampling.
            let region = on_pixmap(pixel_box(&scroll_rect(bounds.inner())), pixmap);
            let (px_x, px_y, px_w, px_h) = region;

            if px_w > 0 && px_h > 0 && mask_w > 0 && mask_h > 0 {
                let snapshot = snapshot_region(pixmap, px_x, px_y, px_w, px_h);

                // Zero everywhere, then the mask blitted where it sits: the
                // window of the mask rect the region sees, resampled.
                let mut mask_data = vec![0u8; (px_w as usize) * (px_h as usize)];
                let window = mask_window((mask_x, mask_y, mask_w, mask_h), region);
                if let Some((wx, wy, ww, wh)) = window {
                    // A mask without an image (a build with no rasteriser, which
                    // this renderer is never part of) hides nothing, as a mask
                    // whose image cannot be read does.
                    let mask = mask_image
                        .as_ref()
                        .and_then(|image| {
                            extract_mask_window(image, mask_w, mask_h, (wx, wy, ww, wh))
                        })
                        .unwrap_or_else(|| vec![255u8; (ww as usize) * (wh as usize)]);
                    // Where the window's first pixel lands in the region: not
                    // negative and inside it, as the window lies in both.
                    let off_x = (i64::from(mask_x) + i64::from(wx) - i64::from(px_x)) as usize;
                    let off_y = (i64::from(mask_y) + i64::from(wy) - i64::from(px_y)) as usize;
                    let (ww, wh, region_w) = (ww as usize, wh as usize, px_w as usize);
                    for y in 0..wh {
                        let dst = (off_y + y) * region_w + off_x;
                        mask_data[dst..dst + ww].copy_from_slice(&mask[y * ww..(y + 1) * ww]);
                    }
                }

                mask_stack.push(MaskEntry::ImageMask {
                    snapshot,
                    mask_data,
                    origin_x: px_x,
                    origin_y: px_y,
                    width: px_w,
                    height: px_h,
                });
            } else {
                // Every push leaves an entry, as a filter's does: a mask that reaches no
                // pixel of this pixmap (an element scrolled out, a group that lands nowhere)
                // pushed nothing, and its pop then took the entry of the group around it.
                mask_stack.push(MaskEntry::ImageMask {
                    snapshot: Vec::new(),
                    mask_data: Vec::new(),
                    origin_x: 0,
                    origin_y: 0,
                    width: 0,
                    height: 0,
                });
            }
        }
        DisplayListItem::PopImageMaskClip => {
            // Only a mask's own entry: the pop never takes the entry of an opacity group, a
            // frame or a clip around it.
            if matches!(mask_stack.last(), Some(MaskEntry::ImageMask { .. })) {
                if let Some(entry) = mask_stack.pop() {
                    apply_mask(pixmap, &entry);
                }
            }
        }
    }

    Ok(())
}

/// One edge of an axis-aligned fill, snapped to the nearest pixel boundary.
///
/// Saturating rather than wrapping: a coordinate far outside the surface (a
/// large CSS transform, an SVG coordinate) must clamp to the edge of the
/// integer range instead of wrapping around to the opposite side of the
/// screen.
#[allow(clippy::cast_possible_truncation)] // saturating by construction
fn round_edge(v: f32) -> i32 {
    if v.is_nan() {
        return 0;
    }
    let r = v.round();
    if r <= f32::from(i16::MIN) {
        return i32::from(i16::MIN);
    }
    if r >= f32::from(i16::MAX) {
        return i32::from(i16::MAX);
    }
    r as i32
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
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // software rasterizer:
                                                                       // bounded pixel/coord/colour
                                                                       // casts
fn render_stroked_path(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    path: &azul_core::svg::SvgMultiPolygon,
    view_box: Option<(f32, f32, f32, f32)>,
    (color, width, outline): (ColorU, f32, &crate::solver3::display_list::StrokeOutline),
    clip: Option<AzRect>,
    dpi_factor: f32,
) {
    if color.a == 0 || !width.is_finite() || width <= 0.0 {
        return;
    }
    let (sx, sy, tx, ty) = svg_user_space_mapping(bounds, view_box, dpi_factor);
    let (ox, oy) = (bounds.origin.x * dpi_factor, bounds.origin.y * dpi_factor);
    let mx = |x: f32| f64::from((x + tx) * sx + ox);
    let my = |y: f32| f64::from((y + ty) * sy + oy);
    let mut geometry = svg_path_to_agg(path, &mx, &my, false);

    // A non-uniform scale cannot be expressed as one stroke width; the mean
    // is the honest approximation and matches what every SVG renderer does
    // for a non-uniform viewBox mapping.
    let scale = f64::from(f32::midpoint(sx, sy));
    let device_width = (f64::from(width) * scale).max(1.0);
    let mut stroke = svg_stroke_outline(&mut geometry, outline, device_width, scale);

    let agg_color = Rgba8::new(
        u32::from(color.r),
        u32::from(color.g),
        u32::from(color.b),
        u32::from(color.a),
    );
    agg_fill_path_clipped(pixmap, &mut stroke, &agg_color, FillingRule::NonZero, clip);
}

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // software rasterizer:
                                                                       // bounded pixel/coord/colour
                                                                       // casts
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
        //
        // Both edges are ROUNDED, not truncated. Truncation places a
        // fractionally-positioned rect on the pixel it barely touches instead
        // of the one it mostly covers: a 1px caret at x = 62.8 covers a fifth
        // of column 62 and four fifths of column 63, and `as i32` painted 62.
        // It can also erase a hairline outright - x = 62.3, width 0.5
        // truncates to the empty range 62..62 - which is the class of bug
        // where a 1px separator or caret is simply missing at some scroll
        // offsets. Rounding both edges keeps the WIDTH (a difference of two
        // rounded values changes by at most one) and puts it where the
        // antialiased path would put its centre of mass. Integer-aligned
        // rects, which is nearly all of them, are unaffected.
        let (mut fx0, mut fy0) = (round_edge(rect.x), round_edge(rect.y));
        let (mut fx1, mut fy1) = (
            round_edge(rect.x + rect.width),
            round_edge(rect.y + rect.height),
        );
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
        // The edges of the opaque fill above (`round_edge`), not truncated ones: a box at
        // x = 62.8 began at column 62 while it was translucent and at column 63 once it was
        // opaque (a fade moved its edge by a pixel), and one that truncates to no width
        // (x = 62.1, 0.2 wide) was the inverted bar `62 ..= 61`.
        let (bx0, by0) = (round_edge(rect.x), round_edge(rect.y));
        let (bx1, by1) = (
            round_edge(rect.x + rect.width),
            round_edge(rect.y + rect.height),
        );
        if bx1 > bx0 && by1 > by0 {
            rb.blend_bar(
                bx0,
                by0,
                bx1 - 1,
                by1 - 1,
                &agg_color,
                255, // cover=255: alpha is already in the color
            );
        }
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

/// Default text antialiasing.
///
/// Desktop gets LCD subpixel. MOBILE GETS GRAYSCALE, which is what iOS and
/// Android themselves do, and for their reasons rather than ours:
///
/// * The device rotates. Subpixel rendering bakes in one physical RGB stripe order; turn the phone
///   90 degrees and every fringe is wrong.
/// * Phone panels are frequently not RGB stripe at all — `PenTile` and other OLED arrangements have
///   no consistent horizontal triad to address.
/// * The frame is composited and often scaled (the emulator does this too), and any resampling
///   smears the per-channel offsets into visible colour fringing — which reads as "blurry text"
///   rather than as sharpening.
///
/// `AZ_TEXT_AA=lcd` still forces it back on for anyone who wants to look.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub const TEXT_AA_DEFAULT: TextAa = TextAa::Lcd;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub const TEXT_AA_DEFAULT: TextAa = TextAa::Grayscale;

/// Everything that decides how a text run is anti-aliased, as one value.
///
/// The renderer does not read the environment for any of this. Whoever owns a
/// render target decides - a window's backend once at startup with
/// [`TextAaSettings::from_env`], a page image by what it is for - and hands the
/// value to the [`GlyphCache`](crate::glyph_cache::GlyphCache) the target is
/// drawn with, which every text path already receives.
#[derive(Clone, Copy)]
pub struct TextAaSettings {
    /// Subpixel, grayscale or aliased.
    pub mode: TextAa,
    /// The parameters of the linear (colorimetric) LCD blend. `None` selects the
    /// blend from before it, as [`TextAa::Legacy`] does.
    pub lcd_blend: Option<agg_rust::pixfmt_lcd::LcdBlendParams>,
    /// Whether LCD runs over a proven uniform background take the pre-blended
    /// tile fast path. Off is a diagnostic: it splits "tile content wrong" from
    /// "sweep wrong".
    pub lcd_pretile: bool,
}

/// The linear LCD blend's defaults: stripes physically free, no chroma limit,
/// coverage gamma 220.
const LCD_BLEND_DEFAULT: agg_rust::pixfmt_lcd::LcdBlendParams =
    agg_rust::pixfmt_lcd::LcdBlendParams {
        contrast: 0,
        chroma_limit: 0,
        coverage_gamma: 220,
    };

impl Default for TextAaSettings {
    /// [`TEXT_AA_DEFAULT`] with the linear blend and the tile fast path: what a
    /// target gets when nobody chose.
    fn default() -> Self {
        Self {
            mode: TEXT_AA_DEFAULT,
            lcd_blend: Some(LCD_BLEND_DEFAULT),
            lcd_pretile: true,
        }
    }
}

impl PartialEq for TextAaSettings {
    fn eq(&self, other: &Self) -> bool {
        let blend = |b: &Option<agg_rust::pixfmt_lcd::LcdBlendParams>| {
            b.map(|p| (p.contrast, p.chroma_limit, p.coverage_gamma))
        };
        self.mode == other.mode
            && self.lcd_pretile == other.lcd_pretile
            && blend(&self.lcd_blend) == blend(&other.lcd_blend)
    }
}

impl core::fmt::Debug for TextAaSettings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextAaSettings")
            .field("mode", &self.mode)
            .field(
                "lcd_blend",
                &self
                    .lcd_blend
                    .map(|p| (p.contrast, p.chroma_limit, p.coverage_gamma)),
            )
            .field("lcd_pretile", &self.lcd_pretile)
            .finish()
    }
}

impl From<azul_core::resources::TextAntialiasing> for TextAaSettings {
    /// The app's setting as the rasteriser uses it: the platform default
    /// resolved for this target, and the blend levels as its parameters.
    fn from(config: azul_core::resources::TextAntialiasing) -> Self {
        use azul_core::resources::TextAntialiasMode;
        let mode = match config.mode {
            TextAntialiasMode::PlatformDefault => TEXT_AA_DEFAULT,
            TextAntialiasMode::Lcd => TextAa::Lcd,
            TextAntialiasMode::LcdLegacy => TextAa::Legacy,
            TextAntialiasMode::Grayscale => TextAa::Grayscale,
            TextAntialiasMode::Aliased => TextAa::None,
        };
        Self {
            mode,
            lcd_blend: (mode != TextAa::Legacy).then(|| agg_rust::pixfmt_lcd::LcdBlendParams {
                contrast: config.lcd_contrast,
                chroma_limit: config.lcd_chroma_limit,
                coverage_gamma: config.lcd_coverage_gamma,
            }),
            lcd_pretile: config.lcd_pretile,
        }
    }
}

impl TextAaSettings {
    /// Single grayscale coverage per pixel: for a picture that is not shown
    /// pixel for pixel on one known panel - a page image, a thumbnail, anything
    /// that is saved, scaled or printed. Such a picture has no subpixel layout
    /// for LCD stripes to sit on, and they would show as colour fringes.
    #[must_use]
    pub fn grayscale() -> Self {
        Self {
            mode: TextAa::Grayscale,
            ..Self::default()
        }
    }

    /// The settings the process environment asks for: the reading is
    /// [`TextAntialiasing::from_env`](azul_core::resources::TextAntialiasing::from_env),
    /// the one place these variables are looked up. A window does not call
    /// this; it takes [`AppConfig::text_antialiasing`](azul_core::resources::AppConfig),
    /// which `AppConfig::create` filled the same way, once.
    #[must_use]
    pub fn from_env() -> Self {
        azul_core::resources::TextAntialiasing::from_env().into()
    }

    /// [`Self::from_env`] with the variables looked up by `var`.
    #[must_use]
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        azul_core::resources::TextAntialiasing::from_vars(var).into()
    }

    /// Whether text takes the RGB LCD subpixel path.
    #[must_use]
    pub fn lcd_enabled(&self) -> bool {
        matches!(self.mode, TextAa::Lcd | TextAa::Legacy)
    }

    /// Whether glyph coverage is thresholded to 0/255 (see [`TextAa::None`]).
    #[must_use]
    pub fn aliased(&self) -> bool {
        self.mode == TextAa::None
    }
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

/// `FreeType` default "light" 5-tap FIR LUT (0x56/0x4D/0x08) - the arguments
/// are compile-time constants, but this was rebuilt per text run per frame
/// (3x256 f64 mul+floor+cast), measurable inside `glyph_lcd_sweep`.
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

// ---- Glyph geometry agg can take ----
//
// agg rasterizes on `i32` coordinates (24.8 fixed point; the LCD path triples
// x). A pen or clip that is non-finite, or far enough off the pixmap to
// saturate those integers, must never reach it: a pen that a broken layout
// parked at 1e38 saturated to `i32::MAX`, the LCD `* 3` wrapped, the
// rasterizer's x range straddled all of `i32` and `ScanlineU8::reset`
// allocated a ONE-cover span - `index out of bounds: the len is 1 but the
// index is 1` in `add_cell` (the AzMail receipt, and seven AzWidgets aborts on
// 2026-09-30). Every glyph path (LCD sweep, LCD tiles, grayscale) goes through
// the three helpers below.

/// The largest rendered em, in device pixels, a glyph run is rasterized at.
/// With the 4-em ink bound of [`glyph_ink_reach`] it keeps a glyph's own
/// cells far inside agg's `i32` range: 4 x 65536 px x 3 stripes x 256
/// subpixels is about 2e8, a tenth of `i32::MAX`.
const MAX_RASTER_EM_PX: f32 = 65_536.0;

/// How far (device px) a glyph's ink can reach from its pen: 4 em, the bound
/// the LCD sweep's horizontal cull always used. `None` for an em no
/// rasterizer can take (non-finite, not positive, or past
/// [`MAX_RASTER_EM_PX`]): the whole run is skipped.
fn glyph_ink_reach(em_px: f32) -> Option<f32> {
    (em_px.is_finite() && em_px > 0.0 && em_px <= MAX_RASTER_EM_PX).then_some(em_px * 4.0)
}

/// Whether a glyph whose pen sits at device `(x, y)` can put ink on a
/// `width` x `height` pixmap, its ink reaching `reach` px from the pen on
/// every side. A glyph this rejects paints nothing anyway; one it admits
/// keeps every integer offset agg sees within `reach` of the pixmap.
#[allow(clippy::cast_precision_loss)] // pixmap sides are far below 2^24
fn glyph_pen_reaches_pixmap(x: f32, y: f32, reach: f32, width: u32, height: u32) -> bool {
    let (w, h) = (width as f32, height as f32);
    x.is_finite()
        && y.is_finite()
        && reach.is_finite()
        && x >= -reach
        && x <= w + reach
        && y >= -reach
        && y <= h + reach
}

/// The whole pixels a text clip covers on a `width` x `height` pixmap, in
/// agg's inclusive `clip_box_i` form `(x1, y1, x2, y2)` (the truncating rule
/// the text paths always used), or `None` when it covers no pixel.
///
/// Intersecting with the pixmap FIRST keeps every coordinate small enough for
/// agg's integer math (the LCD stripe clip triples x). An empty result must
/// skip the run rather than reach agg: `RendererBase::clip_box_i` NORMALIZES
/// an inverted box, so a clip thinner than a pixel used to paint two.
// The casts run after the clamp to the pixmap.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn text_clip_pixel_box(clip: AzRect, width: u32, height: u32) -> Option<(i32, i32, i32, i32)> {
    let pixmap = AzRect::from_xywh(0.0, 0.0, width as f32, height as f32)?;
    let c = clip.clip(&pixmap)?;
    let x1 = c.x as i32;
    let y1 = c.y as i32;
    let x2 = (c.x + c.width) as i32 - 1;
    let y2 = (c.y + c.height) as i32 - 1;
    (x2 >= x1 && y2 >= y1).then_some((x1, y1, x2, y2))
}

/// RGB LCD subpixel-AA glyph run. Rasterizes each glyph at **3× horizontal
/// resolution** (one sub-sample per R/G/B stripe), then lets [`PixfmtRgba32Lcd`]
/// run a 5-tap FIR (the `FreeType` default "light" filter `[08 4D 56 4D 08]`, which
/// sums to 256) over the sub-samples to produce PER-CHANNEL coverage and blend
/// it into the buffer. Black text on white therefore shows the characteristic
/// R/B subpixel fringes instead of a single grey coverage.
///
/// Assumptions / limitations (documented, since this is opt-in):
/// - **Horizontal RGB** subpixel order. A BGR panel would need the R/B taps swapped; a vertical
///   panel would need a transposed (3× vertical) variant.
/// - **Opaque background.** The pixfmt writes per-channel and forces the touched pixel's alpha to
///   255, so subpixel text composited onto a transparent layer is wrong — as it is for every LCD
///   text pipeline. The default flat render path fills the frame opaque white, which is the
///   intended target.
/// - Uses the glyph **path** cache (`get_or_build`), not the pre-rasterized cell cache, since the
///   cells are 1× horizontal; LCD is thus a little slower.
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
/// The clip a text run paints under, in device pixels: its own `clip_rect`
/// (already scroll-projected by the caller) cut to the active `clip`.
/// `None` = nothing of the run can show.
///
/// ONE rule for every paint path. The pre-tiled LCD path always clipped
/// to this intersection, as `WebRender` clips a text item; the batch sweep
/// and the grayscale path clipped to the stack clip alone and only skipped
/// a run whose `clip_rect` lay wholly outside it, so ink past a run's
/// `clip_rect` painted on two paths and not on the third (a list marker cut
/// by one path and painted by the other).
pub(super) fn text_run_clip(
    clip_rect: &LogicalRect,
    clip: Option<AzRect>,
    dpi_factor: f32,
) -> Option<AzRect> {
    let own = logical_rect_to_az_rect(clip_rect, dpi_factor)?;
    match clip {
        Some(c) => own.clip(&c),
        None => Some(own),
    }
}

/// Whether every pixel of `pixmap` under `rect` (device pixels) is opaque.
///
/// LCD subpixel text blends each colour stripe against the destination and
/// stamps the pixel opaque, so it is only right over an opaque backdrop. An
/// opacity / filter / transform layer starts TRANSPARENT (its content is
/// composited through the effect): LCD glyph edges drawn there were blended
/// against transparent black and stamped opaque - smeared, heavy text in every
/// dimmed (disabled) control. A run over any transparent pixel takes
/// grayscale coverage instead, as browsers draw text in a layer without an
/// opaque background.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded pixel coords
/// Whether every pixel of `rect` is exactly the opaque colour `bg` - the
/// proof a pre-blended LCD tile over `bg` needs before it is stamped there.
fn backdrop_is_colour(pixmap: &AzulPixmap, rect: AzRect, bg: ColorU) -> bool {
    if bg.a != 255 {
        return false;
    }
    let w = pixmap.width as usize;
    let h = pixmap.height as usize;
    let x0 = (rect.x.floor().max(0.0) as usize).min(w);
    let y0 = (rect.y.floor().max(0.0) as usize).min(h);
    let x1 = ((rect.x + rect.width).ceil().max(0.0) as usize).min(w).max(x0);
    let y1 = ((rect.y + rect.height).ceil().max(0.0) as usize).min(h);
    let data = pixmap.data();
    let want = [bg.r, bg.g, bg.b, 255];
    (y0..y1).all(|y| {
        data.get((y * w + x0) * 4..(y * w + x1) * 4)
            .is_none_or(|row| row.chunks_exact(4).all(|p| p == want))
    })
}

fn backdrop_is_opaque(pixmap: &AzulPixmap, rect: AzRect) -> bool {
    let w = pixmap.width as usize;
    let h = pixmap.height as usize;
    let x0 = (rect.x.floor().max(0.0) as usize).min(w);
    let y0 = (rect.y.floor().max(0.0) as usize).min(h);
    let x1 = ((rect.x + rect.width).ceil().max(0.0) as usize).min(w).max(x0);
    let y1 = ((rect.y + rect.height).ceil().max(0.0) as usize).min(h);
    let data = pixmap.data();
    (y0..y1).all(|y| {
        data.get((y * w + x0) * 4..(y * w + x1) * 4)
            .is_none_or(|row| row.chunks_exact(4).all(|p| p[3] == 255))
    })
}

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

    // Geometry agg can take (see `glyph_pen_reaches_pixmap`): the clip as
    // whole pixels on this pixmap - one covering no pixel paints nothing -
    // and the reach of a glyph's ink from its pen, from the RENDERED em.
    let clip_box = match clip {
        Some(c) => match text_clip_pixel_box(c, pixmap.width, pixmap.height) {
            Some(b) => Some(b),
            None => return,
        },
        None => None,
    };
    let em_px = scale * f32::from(parsed_font.font_metrics.units_per_em);
    let Some(max_ink_w) = glyph_ink_reach(em_px) else {
        return;
    };
    let embolden_px = synthetic_bold_px(font_hash, em_px);

    let agg_color = Rgba8::new(
        u32::from(color.r),
        u32::from(color.g),
        u32::from(color.b),
        u32::from(color.a),
    );

    // Accumulate every glyph outline (at 3× horizontal resolution) into one
    // rasterizer, then sweep once — same batching as the grayscale path.
    let mut pooled = PooledTextRaster::take();
    let (ras, sl) = pooled.parts();

    let p_outline = crate::probe::Probe::span("glyph_lcd_outline");
    // The face is in use (`FontManager::evict_unused`), though cached paths decode nothing.
    parsed_font.touch();
    // In a band: the window's cache, asked first and never written (`shared_glyphs`).
    let shared = shared_glyphs();
    let cells_built_before = glyph_cache.cells_built();
    for glyph in glyphs {
        let glyph_index = glyph.index as u16;

        let glyph_x = (glyph.point.x - scroll_offset.0) * dpi_factor;
        let glyph_baseline_y = (glyph.point.y - scroll_offset.1) * dpi_factor;

        // A pen that cannot put ink on the pixmap - NaN, or parked far off it
        // by a broken layout - never reaches agg's integer cell math.
        if !glyph_pen_reaches_pixmap(
            glyph_x,
            glyph_baseline_y,
            max_ink_w,
            pixmap.width,
            pixmap.height,
        ) {
            continue;
        }

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
            // (`max_ink_w` is that 4-em bound, from `glyph_ink_reach`.)
            let cx0 = c.x;
            let cx1 = c.x + c.width;
            if glyph_x - max_ink_w > cx1 + 2.0 || glyph_x + max_ink_w < cx0 - 2.0 {
                continue;
            }
        }

        // A band asks the window's cache for the cells before it builds anything into its
        // own. Without hinting (`ppem == 0`) no path is hinted (`GlyphCache::get_or_build`),
        // so the cells' key is known without a path: a glyph the window holds costs the band
        // no path.
        // (With hinting the key needs the path's kind, which the window's cache says without
        // building the path: `cached_path_kind`, A1-49.)
        let window_kind = if ppem == 0 {
            Some(false)
        } else {
            shared
                .as_deref()
                .and_then(|window| window.cached_path_kind(font_hash.font_hash, glyph_index, ppem))
                .flatten()
        };
        if let Some(window_hinted) = window_kind {
            if let Some(held) = shared.as_deref().and_then(|window| {
                window.cached_cells_lcd(
                    font_hash.font_hash,
                    glyph_index,
                    ppem,
                    glyph_x,
                    glyph_baseline_y,
                    scale,
                    window_hinted,
                    hint_correction,
                    embolden_px,
                )
            }) {
                if let Some((cells, int_x, int_y)) = held {
                    ras.add_cells_offset(cells, int_x * 3, int_y);
                }
                continue;
            }
        }

        // Builds (and caches) the hinted outline; we only need to know which
        // coordinate space it came back in. The glyph is decoded only when
        // its path is not cached yet (`get_or_build_decoding`).
        let Some(cached) = glyph_cache.get_or_build_decoding(
            font_hash.font_hash,
            glyph_index,
            parsed_font,
            ppem,
            || parsed_font.get_or_decode_glyph(glyph_index),
        ) else {
            continue;
        };
        let is_hinted = cached.is_hinted;

        // Cells are rasterized once per (glyph, ppem, scale, 1/3-px bucket)
        // and replayed here at an integer offset. `int_x` is in whole pixels
        // and the cells' x axis is in stripes, hence the *3.
        let Some((cells, int_x, int_y)) = glyph_cache.get_or_build_cells_lcd_emboldened(
            font_hash.font_hash,
            glyph_index,
            ppem,
            glyph_x,
            glyph_baseline_y,
            scale,
            is_hinted,
            hint_correction,
            embolden_px,
        ) else {
            continue;
        };
        ras.add_cells_offset(cells, int_x * 3, int_y);
    }
    count_cells_built(glyph_cache, cells_built_before);

    drop(p_outline);
    let _p_sweep = crate::probe::Probe::span("glyph_lcd_sweep");
    // Blend via the LCD pixel format. It reports width*3, so the rasterizer's 3×
    // x-coordinates address individual R/G/B stripes; the clip box X is likewise
    // in sub-pixel space.
    let w = pixmap.width;
    let h = pixmap.height;
    let stride = (w * 4) as i32;
    // The panel's stripe order, per window (the shell sets it per monitor).
    let order = glyph_cache.lcd_subpixel_order();
    let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride) };
    // FreeType default "light" 5-tap FIR (see lcd_distribution_lut).
    let lut = lcd_distribution_lut();
    if let Some(params) = glyph_cache.text_aa().lcd_blend {
        // Colorimetric path (default): per-stripe compositing in LINEAR
        // light against the actual background pixel, with contrast
        // enhancement and an optional chroma budget — text stays legible
        // on ANY fg/bg pair (thin white on green was unreadable with
        // sRGB-space blending), instead of only on near-b/w pairs.
        let mut pf = agg_rust::pixfmt_lcd::PixfmtRgba32LcdLinear::new(&mut ra, lut, params);
        pf.set_subpixel_order(order);
        if let Some((x1, _, x2, _)) = clip_box {
            // The FIR spread writes 2 stripes past every span; the renderer-
            // base clip box cannot bound those writes (task #17: a damage-rect
            // repaint double-blended the escaped fringe one pixel LEFT of the
            // rect — 239²/255 = 224, the exact measured divergence).
            pf.set_stripe_clip(x1 * 3, (x2 + 1) * 3);
        }
        let mut rb = RendererBase::new(pf);
        if let Some((_, y1, _, y2)) = clip_box {
            // Y-only span clip: vertical has no FIR spread, so scanline
            // clipping is exact. X spans must reach the FIR distribution
            // UNCLIPPED — ink just OUTSIDE the clip contributes fringe to
            // the boundary column INSIDE it (a span-clipped repaint loses
            // that contribution and renders the column lighter than a full
            // repaint). The stripe clip set above bounds the WRITES instead.
            rb.clip_box_i(0, y1, (w as i32) * 3 - 1, y2);
        }
        render_scanlines_aa_solid(ras, sl, &mut rb, &agg_color);
    } else {
        // Legacy sRGB-space blending (AZ_LCD_BLEND=legacy).
        let mut pf = PixfmtRgba32Lcd::new(&mut ra, lut);
        pf.set_subpixel_order(order);
        if let Some((x1, _, x2, _)) = clip_box {
            // Same stripe-clip as the colorimetric arm above.
            pf.set_stripe_clip(x1 * 3, (x2 + 1) * 3);
        }
        let mut rb = RendererBase::new(pf);
        if let Some((_, y1, _, y2)) = clip_box {
            // Y-only span clip: vertical has no FIR spread, so scanline
            // clipping is exact. X spans must reach the FIR distribution
            // UNCLIPPED — ink just OUTSIDE the clip contributes fringe to
            // the boundary column INSIDE it (a span-clipped repaint loses
            // that contribution and renders the column lighter than a full
            // repaint). The stripe clip set above bounds the WRITES instead.
            rb.clip_box_i(0, y1, (w as i32) * 3 - 1, y2);
        }
        render_scanlines_aa_solid(ras, sl, &mut rb, &agg_color);
    }
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
        "[cpurender] BUG: layout emitted font hash {font_hash} that its own FontManager cannot \
         resolve — the display list and the font state are out of sync"
    );
    let seen = SEEN.get_or_init(|| Mutex::new(std::collections::BTreeSet::new()));
    if let Ok(mut seen) = seen.lock() {
        if seen.insert(font_hash) {
            eprintln!(
                "[azul][font] BUG: layout emitted font hash {font_hash} that its own FontManager \
                 cannot resolve (neither a loaded face nor a registered embedded font). The text \
                 using it CANNOT be drawn. This is an azul bug — please report it."
            );
        }
    }
}

#[cfg(not(feature = "std"))]
const fn font_resolution_failed(_font_hash: u64) {}

/// How far each side of a glyph moves out when the run is drawn in
/// synthetic bold ([`FontHash::synthetic_bold`]), in device pixels; 0 for a
/// face drawn as it is. Keyed on the size in device pixels, the one size
/// `WebRender`'s glyph rasterizer sees, so both renderers draw the same stems.
fn synthetic_bold_px(font_hash: FontHash, effective_px: f32) -> f32 {
    if font_hash.synthetic_bold {
        embolden::synthetic_bold_strength_px(effective_px)
    } else {
        0.0
    }
}

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
    crate::cpu_counters::count(crate::cpu_counters::Counter::TextRuns, 1);
    crate::cpu_counters::count(crate::cpu_counters::Counter::Glyphs, glyphs.len() as u64);
    let aa = glyph_cache.text_aa();
    if let Some((bg, proven_rect)) = uniform_bg {
        if aa.lcd_enabled() && !force_grayscale {
            if let Some(params) = aa.lcd_blend {
                if aa.lcd_pretile
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
                    )
                {
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
    use agg_rust::pixfmt_lcd::LcdDistributionLut;

    use crate::glyph_cache::LcdGlyphTile;
    let _ = renderer_resources;
    // The tiles are the face's own glyphs: a thickened run takes the
    // normal path, which builds its cells from the thickened outline.
    if font_hash.synthetic_bold {
        return false;
    }
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
    // An em no rasterizer can take paints nothing (see `glyph_ink_reach`).
    let Some(ink_reach) = glyph_ink_reach(effective_px) else {
        return true;
    };
    let lut = lcd_distribution_lut();

    // The run's clip, as WHOLE PIXELS: `text_run_clip` (the item clip_rect
    // cut to the stack clip, device px) through `text_clip_pixel_box` - the
    // very box the sweep and the grayscale path paint under, so a fractional
    // clip cuts the tiles at the same pixel (this path used to snap the
    // clip_rect OUTWARD on its own and paint one column / row more).
    // NOTE: `clip_rect` arrives ALREADY scroll-projected by the caller
    // (`text_clip = scroll_rect(clip_rect)` in the Text arm) — do not
    // subtract `scroll_offset` here again.
    let Some((bx0, by0, bx1, by1)) = text_run_clip(clip_rect, clip, dpi_factor)
        .and_then(|c| text_clip_pixel_box(c, pixmap.width, pixmap.height))
    else {
        return true; // fully clipped: nothing to paint, and nothing missed
    };
    // `clip_box_i`'s inclusive form, made exclusive for the tile copies.
    let (cx0, cy0, cx1, cy1) = (bx0, by0, bx1 + 1, by1 + 1);
    // The same box, for the sweep the overlapping components take (pass
    // 2a): it used to get the STACK clip alone, so with no stack clip - or
    // one wider than the run's clip_rect - the overlapping glyphs of a run
    // painted past the clip_rect the tiled ones were cut to.
    let run_clip = AzRect::from_xywh(
        cx0 as f32,
        cy0 as f32,
        (cx1 - cx0) as f32,
        (cy1 - cy0) as f32,
    );

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
        tile: std::sync::Arc<LcdGlyphTile>,
        x0: i32,
        y0: i32,
        glyph: GlyphInstance,
        component: usize,
    }
    let mut placed: Vec<Placed> = Vec::with_capacity(glyphs.len());
    let mut next_component = 0usize;
    // The face is in use (`FontManager::evict_unused`), though cached paths decode nothing.
    parsed_font.touch();
    // In a band: the window's cache, asked first and never written (`shared_glyphs`).
    let shared = shared_glyphs();
    for glyph in glyphs {
        let gx = (glyph.point.x - scroll_offset.0) * dpi_factor;
        let gy = (glyph.point.y - scroll_offset.1) * dpi_factor;
        // A pen that cannot put ink on the pixmap never reaches the tile
        // placement (`int_x + tile.dx` overflowed for a saturated pen).
        if !glyph_pen_reaches_pixmap(gx, gy, ink_reach, pixmap.width, pixmap.height) {
            continue;
        }
        let glyph_index = glyph.index as u16;
        // A band asks the window's cache first (A1-49): the kind of the glyph's path, then
        // its tile - a glyph the window holds costs the band nothing.
        let window_kind = shared
            .as_deref()
            .and_then(|window| window.cached_path_kind(font_hash.font_hash, glyph_index, ppem));
        if window_kind == Some(None) {
            continue; // held as "no outline": no tile, as below
        }
        let window_tile = window_kind.flatten().and_then(|is_hinted| {
            shared.as_deref().and_then(|window| {
                window.cached_lcd_tile(
                    font_hash.font_hash,
                    glyph_index,
                    ppem,
                    gx,
                    gy,
                    scale,
                    is_hinted,
                    hint_correction,
                    color,
                    bg,
                )
            })
        });
        let tile = if let Some(tile) = window_tile {
            tile
        } else {
            // This cache: its path first (a tile is built from the path of the cache it is
            // built in), then its tile.
            // A glyph without an outline has no tile: skipped here as below.
            let Some(is_hinted) = glyph_cache
                .get_or_build_decoding(font_hash.font_hash, glyph_index, parsed_font, ppem, || {
                    parsed_font.get_or_decode_glyph(glyph_index)
                })
                .map(|c| c.is_hinted)
            else {
                continue;
            };
            let cells_built_before = glyph_cache.cells_built();
            let tile = glyph_cache.get_or_build_lcd_tile(
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
                lut,
                params,
            );
            // Per glyph: the loop has a `return` of its own below.
            count_cells_built(glyph_cache, cells_built_before);
            tile
        };
        let Some((tile, int_x, int_y)) = tile else {
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
            // Saturating: a proven rect far off the pixmap saturates the
            // casts, and the 1-px inset must not overflow them.
            let px0 = ((prx * dpi_factor).ceil() as i32).saturating_add(1);
            let py0 = ((pry * dpi_factor).ceil() as i32).saturating_add(1);
            let px1 = (((prx + pr.size.width) * dpi_factor).floor() as i32).saturating_sub(1);
            let py1 = (((pry + pr.size.height) * dpi_factor).floor() as i32).saturating_sub(1);
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
    // pixels are disjoint from every tiled glyph's pixels), under the SAME
    // combined clip the tile copies below are cut to.
    if !sweep_glyphs.is_empty() {
        render_glyphs_lcd(
            pixmap,
            run_clip,
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

/// One glyph run into `pixmap` (also `text_raster`'s rasteriser).
pub(super) fn render_text(
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

    // The run's own clip_rect cut to the active clip: the one clip the LCD
    // sweep and the grayscale path below paint under (`text_run_clip`).
    let Some(run_clip) = text_run_clip(clip_rect, clip, dpi_factor) else {
        return; // fully clipped
    };
    let clip = Some(run_clip);

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
    if glyph_cache.text_aa().lcd_enabled() && !force_grayscale {
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

    // Geometry agg can take (see `glyph_pen_reaches_pixmap`): a clip covering
    // no whole pixel paints nothing, and an em no rasterizer can take skips
    // the run.
    let clip_box = match clip {
        Some(c) => match text_clip_pixel_box(c, w, h) {
            Some(b) => Some(b),
            None => return,
        },
        None => None,
    };
    let Some(ink_reach) = glyph_ink_reach(effective_px) else {
        return;
    };
    let embolden_px = synthetic_bold_px(font_hash, effective_px);

    // Create renderer infrastructure once, reuse for all glyphs in this text run.
    // Batches all glyph cells into a single rasterizer pass when possible.
    let mut ra = unsafe { RowAccessor::new_with_buf(pixmap.data.as_mut_ptr(), w, h, stride) };
    let mut pf = PixfmtRgba32::new(&mut ra);
    let mut rb = RendererBase::new(pf);
    if let Some((x1, y1, x2, y2)) = clip_box {
        rb.clip_box_i(x1, y1, x2, y2);
    }
    let mut pooled = PooledTextRaster::take();
    let (ras, sl) = pooled.parts();

    // Accumulate all glyph cells into one rasterizer, then render once.
    // This amortizes sort_cells cost across all glyphs in the run.
    // The face is in use (`FontManager::evict_unused`), though cached paths decode nothing.
    parsed_font.touch();
    // In a band: the window's cache, asked first and never written (`shared_glyphs`).
    let shared = shared_glyphs();
    let cells_built_before = glyph_cache.cells_built();
    for glyph in glyphs {
        let glyph_index = glyph.index as u16;

        let glyph_x = (glyph.point.x - scroll_offset.0) * dpi_factor;
        let glyph_baseline_y = (glyph.point.y - scroll_offset.1) * dpi_factor;
        // A pen that cannot put ink on the pixmap never reaches agg.
        if !glyph_pen_reaches_pixmap(glyph_x, glyph_baseline_y, ink_reach, w, h) {
            continue;
        }

        // A band asks the window's cache for the cells before it builds anything into its
        // own; without hinting (`ppem == 0`: no path is hinted) before the path is looked
        // up (see `render_glyphs_lcd`).
        let window_kind = if ppem == 0 {
            Some(false)
        } else {
            shared
                .as_deref()
                .and_then(|window| window.cached_path_kind(font_hash.font_hash, glyph_index, ppem))
                .flatten()
        };
        if let Some(window_hinted) = window_kind {
            if let Some(held) = shared.as_deref().and_then(|window| {
                window.cached_cells(
                    font_hash.font_hash,
                    glyph_index,
                    ppem,
                    glyph_x,
                    glyph_baseline_y,
                    scale,
                    window_hinted,
                    hint_correction,
                    embolden_px,
                )
            }) {
                if let Some((cells, int_x, int_y)) = held {
                    ras.add_cells_offset(cells, int_x, int_y);
                }
                continue;
            }
        }

        // Lazy decode: the first access to a given gid for this face does
        // the allsorts glyf walk + OwnedGlyph conversion, and only while its
        // path is not cached (`get_or_build_decoding`). A glyph without an
        // outline has no cells: skipped here as below.
        let Some(is_hinted) = glyph_cache
            .get_or_build_decoding(font_hash.font_hash, glyph_index, parsed_font, ppem, || {
                parsed_font.get_or_decode_glyph(glyph_index)
            })
            .map(|c| c.is_hinted)
        else {
            continue;
        };

        let Some((cells, int_x, int_y)) = glyph_cache.get_or_build_cells_emboldened(
            font_hash.font_hash,
            glyph_index,
            ppem,
            glyph_x,
            glyph_baseline_y,
            scale,
            is_hinted,
            hint_correction,
            embolden_px,
        ) else {
            continue;
        };

        ras.add_cells_offset(cells, int_x, int_y);
    }
    count_cells_built(glyph_cache, cells_built_before);

    // Single render pass for all glyphs in this text run
    if glyph_cache.text_aa().aliased() {
        render_scanlines_aliased_solid(ras, sl, &mut rb, &agg_color);
    } else {
        render_scanlines_aa_solid(ras, sl, &mut rb, &agg_color);
    }
}

/// The rasteriser and scanline a text run sweeps its glyph cells with. Every run used to make
/// its own (the rasteriser's cell storage, the scanline's span and cover tables, grown from
/// nothing) and drop them after one sweep; a map draws thousands of runs a frame. A run takes the
/// thread's pair and its drop hands it back, emptied.
struct PooledTextRaster {
    ras: Option<RasterizerScanlineAa>,
    sl: Option<ScanlineU8>,
}

thread_local! {
    static TEXT_RASTER_POOL: std::cell::RefCell<Option<(RasterizerScanlineAa, ScanlineU8)>> =
        const { std::cell::RefCell::new(None) };
    /// Tests: how many rasterisers this thread has had to make.
    #[cfg(test)]
    static TEXT_RASTERS_MADE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Tests: make one per run, as before the pool.
    #[cfg(test)]
    static TEXT_RASTER_POOL_OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl PooledTextRaster {
    fn take() -> Self {
        #[cfg(test)]
        let pooled = if TEXT_RASTER_POOL_OFF.with(std::cell::Cell::get) {
            None
        } else {
            TEXT_RASTER_POOL.with(|p| p.borrow_mut().take())
        };
        #[cfg(not(test))]
        let pooled = TEXT_RASTER_POOL.with(|p| p.borrow_mut().take());
        let (ras, sl) = pooled.unwrap_or_else(|| {
            #[cfg(test)]
            TEXT_RASTERS_MADE.with(|n| n.set(n.get() + 1));
            (RasterizerScanlineAa::new(), ScanlineU8::new())
        });
        Self { ras: Some(ras), sl: Some(sl) }
    }

    /// The rasteriser (empty, non-zero winding) and the scanline.
    fn parts(&mut self) -> (&mut RasterizerScanlineAa, &mut ScanlineU8) {
        let ras = self.ras.as_mut().expect("held until drop");
        ras.filling_rule(FillingRule::NonZero);
        (ras, self.sl.as_mut().expect("held until drop"))
    }
}

impl Drop for PooledTextRaster {
    fn drop(&mut self) {
        if let (Some(mut ras), Some(sl)) = (self.ras.take(), self.sl.take()) {
            ras.reset();
            TEXT_RASTER_POOL.with(|p| *p.borrow_mut() = Some((ras, sl)));
        }
    }
}

/// A box of device pixels in the target pixmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PixelBox {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: u32,
    pub(super) height: u32,
}

/// Where one text shadow is drawn: `buffer` is the offscreen its glyphs are
/// drawn and blurred in, `out` the part of it that lands on the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShadowBoxes {
    buffer: PixelBox,
    out: PixelBox,
}

/// The boxes of a text shadow whose glyphs paint inside `source` and spread
/// `radius` pixels by the blur, under `clip`, on a `target` sized pixmap.
/// `None` when nothing of it lands.
///
/// The shadow lands within the blur's reach of its glyphs, cut to the clip
/// and the target. The buffer reaches the blur's radius past that, so the
/// glyphs just outside the clip still blur into it, but never past the
/// glyphs' own reach: beyond it the buffer would only hold transparency.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)]
fn text_shadow_boxes(
    source: AzRect,
    radius: u32,
    clip: Option<AzRect>,
    target: (u32, u32),
) -> Option<ShadowBoxes> {
    let r = radius.min(i32::MAX as u32) as i32;
    let reach = (
        (source.x.floor() as i32).saturating_sub(r),
        (source.y.floor() as i32).saturating_sub(r),
        ((source.x + source.width).ceil() as i32).saturating_add(r),
        ((source.y + source.height).ceil() as i32).saturating_add(r),
    );
    let mut out = (
        reach.0.max(0),
        reach.1.max(0),
        reach.2.min(target.0 as i32),
        reach.3.min(target.1 as i32),
    );
    if let Some(c) = clip {
        // The pixels a paint under the clip may write (`AzRect::pixel_box`, A2-cpu-06): by
        // `floor` / `ceil` a halo landed in the column and row the clip ends in, which no
        // fill and no glyph under that clip writes. `out` is not below zero before this (it
        // is cut to the target), so a negative near edge, which `pixel_box` truncates towards
        // zero, changes nothing.
        let (cx0, cy0, cx1, cy1) = c.pixel_box();
        out.0 = out.0.max(cx0);
        out.1 = out.1.max(cy0);
        out.2 = out.2.min(cx1);
        out.3 = out.3.min(cy1);
    }
    if out.2 <= out.0 || out.3 <= out.1 {
        return None;
    }
    let buffer = (
        out.0.saturating_sub(r).max(reach.0),
        out.1.saturating_sub(r).max(reach.1),
        out.2.saturating_add(r).min(reach.2),
        out.3.saturating_add(r).min(reach.3),
    );
    let pixel_box = |(x0, y0, x1, y1): (i32, i32, i32, i32)| PixelBox {
        x: x0,
        y: y0,
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
    };
    Some(ShadowBoxes {
        buffer: pixel_box(buffer),
        out: pixel_box(out),
    })
}

/// Paint a single `text-shadow` for a glyph run.
///
/// Renders the glyphs (offset by the shadow's logical offset, tinted with the
/// shadow colour) into a transparent offscreen buffer, blurs that buffer by the
/// shadow's blur radius using the same `stack_blur_rgba32` the box-shadow/filter
/// paths use, then alpha-composites it onto `pixmap` (below where the real
/// glyphs are subsequently drawn).
///
/// The buffer covers only what the glyphs reach ([`text_shadow_boxes`]), so a
/// shadow costs what its glyphs cover: a map draws a halo behind every label.
/// `source_clip` cuts the glyphs (the ancestors' clips, without a damage
/// strip, so a glyph just outside the strip still blurs into it); `clip` cuts
/// what lands, as a browser clips a shadow by the overflow clips around it.
// software rasterizer: bounded blur-radius / stride / pixel casts
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn render_text_shadow(
    shadow: &StyleBoxShadow,
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    pixmap: &mut AzulPixmap,
    clip_rect: &LogicalRect,
    clip: Option<AzRect>,
    source_clip: Option<AzRect>,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
) {
    let color = shadow.color;
    if color.a == 0 || glyphs.is_empty() || dpi_factor <= 0.0 {
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
    let radius = shadow_blur_radius(blur_logical, dpi_factor);
    // The spread (the fourth length) grows the glyphs before the blur: with
    // no blur, a solid halo of that width around the letters.
    let spread = shadow_spread_px(
        shadow
            .spread_radius
            .inner
            .to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE),
        dpi_factor,
    );

    let shadow_clip_rect = LogicalRect {
        origin: LogicalPosition {
            x: clip_rect.origin.x + off_x,
            y: clip_rect.origin.y + off_y,
        },
        size: clip_rect.size,
    };
    let Some(source) = text_run_clip(&shadow_clip_rect, source_clip, dpi_factor) else {
        return;
    };
    let reach = radius.saturating_add(spread.ceil() as u32);
    let Some(boxes) = text_shadow_boxes(source, reach, clip, (pixmap.width, pixmap.height))
    else {
        return;
    };
    let shifted: Vec<GlyphInstance> = glyphs
        .iter()
        .map(|g| {
            let mut g = *g;
            g.point.x += off_x;
            g.point.y += off_y;
            g
        })
        .collect();
    // A halo that was drawn before, at this place or another, is tinted and composited.
    if super::text_halo::cpu_halos_are_kept()
        && draw_kept_text_shadow(
            &shifted,
            &KeptShadow {
                color,
                font_hash,
                font_size_px,
                radius,
                spread,
                reach,
                source,
                shadow_clip_rect,
                out: boxes.out,
            },
            pixmap,
            renderer_resources,
            font_manager,
            dpi_factor,
            glyph_cache,
            scroll_offset,
        )
    {
        return;
    }
    let buffer = boxes.buffer;
    let run = BufferedRun {
        glyphs: &shifted,
        font_hash,
        font_size_px,
        clip_rect: shadow_clip_rect,
        clip: source,
    };
    crate::cpu_counters::count(crate::cpu_counters::Counter::HalosDrawn, 1);
    let Some(tmp) = blurred_glyphs(
        &[run],
        color,
        buffer,
        radius,
        spread,
        renderer_resources,
        font_manager,
        dpi_factor,
        glyph_cache,
        scroll_offset,
    ) else {
        return;
    };

    // Composite the (premultiplied) shadow where it lands.
    let out = boxes.out;
    blit_buffer_sub(
        pixmap,
        &tmp.data,
        tmp.width,
        tmp.height,
        (out.x - buffer.x) as u32,
        (out.y - buffer.y) as u32,
        out.width,
        out.height,
        out.x,
        out.y,
    );
}

/// What a text shadow's halo is drawn from ([`draw_kept_text_shadow`]).
struct KeptShadow {
    /// The shadow's colour: the halo is kept in white and tinted with it.
    color: ColorU,
    font_hash: FontHash,
    font_size_px: f32,
    /// The blur's radius and the spread, device px; `reach` is their sum, rounded up.
    radius: u32,
    spread: f32,
    reach: u32,
    /// Where the glyphs can paint (the run's clip rect cut to the ancestors' clips), device px.
    source: AzRect,
    /// The run's clip rect, shadow offset included, CSS px.
    shadow_clip_rect: LogicalRect,
    /// The part of the halo that lands on the target ([`ShadowBoxes::out`]).
    out: PixelBox,
}

/// The halo of a text shadow from the kept pictures: drawn once, in white, for the whole box its
/// glyphs reach, and tinted and composited wherever the same text stands again. `false` when the
/// shadow cannot be kept (a box too large, a pen out of range): it is drawn the usual way.
///
/// A halo depends on the glyphs relative to each other, their font and size, the clip they paint
/// under, the spread, the blur and the scale, and on where each glyph falls within its device
/// pixel (the glyph cache places a glyph in quarter-pixel steps). It does not depend on where the
/// text stands, but for that: so the key counts every glyph, and the clip, from the first glyph's
/// pixel, and a run that moves by whole pixels - a pan, a zoom composited from a group drawn at
/// its layout place - finds its halo kept. A run that moves by a fraction lands in other
/// quarter-pixel steps and draws its own.
///
/// `shifted` are the glyphs with the shadow's offset added.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::too_many_arguments
)]
fn draw_kept_text_shadow(
    shifted: &[GlyphInstance],
    shadow: &KeptShadow,
    pixmap: &mut AzulPixmap,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
) -> bool {
    use std::hash::{Hash, Hasher};

    use super::text_halo::{keep_cpu_halo, kept_cpu_halo, CpuHalo, MAX_CPU_HALO_PIXELS};

    let Some(first) = shifted.first() else {
        return false;
    };
    let device = |g: &GlyphInstance| {
        (
            (g.point.x - scroll_offset.0) * dpi_factor,
            (g.point.y - scroll_offset.1) * dpi_factor,
        )
    };
    let (first_x, first_y) = device(first);
    let in_range = |v: f32| v.is_finite() && v.abs() < 1.0e6;
    if !(in_range(first_x) && in_range(first_y)) {
        return false;
    }
    let anchor = (first_x.floor() as i32, first_y.floor() as i32);

    // What the halo's pixels depend on: counted from the anchor, in the glyph cache's quarter
    // pixels for the glyphs and in whole pixels for the clip (the text paths clip to whole pixels).
    let quarter = |v: f32, from: i32| (v * 4.0).floor() as i64 - i64::from(from) * 4;
    let whole = |v: f32, from: i32| v.floor() as i64 - i64::from(from);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    shadow.radius.hash(&mut hasher);
    shadow.spread.to_bits().hash(&mut hasher);
    dpi_factor.to_bits().hash(&mut hasher);
    shadow.font_hash.font_hash.hash(&mut hasher);
    shadow.font_size_px.to_bits().hash(&mut hasher);
    let source = shadow.source;
    whole(source.x, anchor.0).hash(&mut hasher);
    whole(source.y, anchor.1).hash(&mut hasher);
    whole(source.x + source.width, anchor.0).hash(&mut hasher);
    whole(source.y + source.height, anchor.1).hash(&mut hasher);
    shifted.len().hash(&mut hasher);
    for glyph in shifted {
        let (x, y) = device(glyph);
        glyph.index.hash(&mut hasher);
        quarter(x, anchor.0).hash(&mut hasher);
        quarter(y, anchor.1).hash(&mut hasher);
    }
    let key = hasher.finish();

    let halo = match kept_cpu_halo(key) {
        Some(kept) => kept,
        None => {
            // The box the glyphs reach: nothing of the halo lies past it. Its edges are the
            // whole pixels the key counts (the text paths clip to whole pixels), a pixel wider on
            // the right and below than `text_shadow_boxes` needs when the clip ends on a pixel.
            let r = shadow.reach.min(i32::MAX as u32) as i32;
            let x0 = (source.x.floor() as i32).saturating_sub(r);
            let y0 = (source.y.floor() as i32).saturating_sub(r);
            let x1 = ((source.x + source.width).floor() as i32).saturating_add(r + 1);
            let y1 = ((source.y + source.height).floor() as i32).saturating_add(r + 1);
            let (width, height) = (i64::from(x1) - i64::from(x0), i64::from(y1) - i64::from(y0));
            let drawn = if width <= 0
                || height <= 0
                || (width * height) as u64 > MAX_CPU_HALO_PIXELS
            {
                None
            } else {
                crate::cpu_counters::count(crate::cpu_counters::Counter::HalosDrawn, 1);
                let buffer = PixelBox {
                    x: x0,
                    y: y0,
                    width: width as u32,
                    height: height as u32,
                };
                let run = BufferedRun {
                    glyphs: shifted,
                    font_hash: shadow.font_hash,
                    font_size_px: shadow.font_size_px,
                    clip_rect: shadow.shadow_clip_rect,
                    clip: source,
                };
                blurred_glyph_coverage(
                    &[run],
                    ColorU::WHITE,
                    buffer,
                    shadow.radius,
                    shadow.spread,
                    renderer_resources,
                    font_manager,
                    dpi_factor,
                    glyph_cache,
                    scroll_offset,
                )
                .map(|(part, coverage)| {
                    // Kept as the part the halo can paint (A8-29): the rest of the box is clear.
                    std::sync::Arc::new(CpuHalo {
                        width: part.width,
                        height: part.height,
                        offset: (part.x - anchor.0, part.y - anchor.1),
                        coverage,
                    })
                })
            };
            keep_cpu_halo(key, drawn.clone());
            drawn
        }
    };
    let Some(halo) = halo else {
        return false;
    };

    // Where it lands: the part of the picture inside `out`.
    let place = (anchor.0 + halo.offset.0, anchor.1 + halo.offset.1);
    let out = shadow.out;
    let x0 = out.x.max(place.0);
    let y0 = out.y.max(place.1);
    let x1 = (out.x + out.width as i32).min(place.0 + halo.width as i32);
    let y1 = (out.y + out.height as i32).min(place.1 + halo.height as i32);
    if x1 > x0 && y1 > y0 {
        blit_tinted_coverage(
            pixmap,
            &halo.coverage,
            halo.width,
            (x0 - place.0) as u32,
            (y0 - place.1) as u32,
            (x1 - x0) as u32,
            (y1 - y0) as u32,
            x0,
            y0,
            shadow.color,
        );
    }
    true
}

/// A shadow's blur radius in device pixels, from its CSS `blur` length:
/// none under half a pixel, at most the stack blur's 254.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a bounded radius
pub(super) fn shadow_blur_radius(blur_logical: f32, dpi_factor: f32) -> u32 {
    let blur_px = blur_logical * dpi_factor;
    if blur_px > 0.5 {
        (blur_px.ceil() as u32).min(254)
    } else {
        0
    }
}

/// A shadow's spread in device pixels, from its CSS `spread` length: at
/// most 64, and none for a negative spread (text does not shrink).
pub(super) fn shadow_spread_px(spread_logical: f32, dpi_factor: f32) -> f32 {
    let spread = spread_logical * dpi_factor;
    if spread.is_finite() && spread > 0.0 {
        spread.min(64.0)
    } else {
        0.0
    }
}

/// Grows what `p` paints by `r` pixels all round: every pixel takes the most
/// of its neighbours within a disk of radius `r`, whose rim is antialiased
/// (a neighbour half a pixel beyond `r` counts half). Over glyphs drawn in
/// one colour it is their outline stroked `2r` wide with round joins and
/// caps - the solid halo of a printed map's labels. Premultiplied pixels of
/// one colour keep that colour, as each channel is grown alike.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::needless_range_loop
)] // a bounded radius and pixel indices
// The halos grow their part through `dilate_disk_channels` (A8-29); this whole-pixmap form is
// the reference the tests hold them to.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn dilate_disk(p: &mut AzulPixmap, r: f32) {
    let (w, h) = (p.width as usize, p.height as usize);
    dilate_disk_channels(p.data_mut(), w, h, r, 0);
}

/// [`dilate_disk`] of the channels `first..4` of the RGBA pixels `data`
/// (`w` x `h`, row by row); the channels before `first` are left as they
/// are. `first == 3` grows the coverage alone - what a kept halo keeps - for
/// a quarter of the work: each channel is grown on its own, so the alpha is
/// the alpha `first == 0` makes.
///
/// Each pixel takes, per channel, the strongest `source * weight` of the
/// pixels with ink (alpha above 0) that the disk around it reaches; a weight
/// is 256 inside the disk and falls to 0 over its antialiased rim. The disk is
/// symmetric, so it is done from the sources (a text shadow's buffer is mostly
/// empty). A row of the disk, `dy` rows from the source, reaches `b` pixels to
/// each side with weights that never rise with the distance. So a RUN of
/// sources in one row holding one value gives a pixel of that disk row the
/// value times the weight at its distance from the nearest pixel of the run (0
/// inside it): one pass over the run and its `2 b` fringe pixels per disk row,
/// where every source walked every tap of its disk row (`2 b + 1`) before.
/// Glyphs have such runs (the inside of a stem, a bar's rows, a flat edge); a
/// pixel alone is a run of one and costs what it did.
///
/// The same bytes as walking every source over every tap: the taps and their
/// weights are computed by the same expression, a disk row's taps are the
/// distances `0..=b` (the weight only falls with the distance, so those with
/// `w > 0` are a prefix), the most of `value * weight(d)` over a run is
/// `value * weight(nearest d)`, and the result is rounded as before.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::needless_range_loop
)] // a bounded radius and pixel indices
fn dilate_disk_channels(data: &mut [u8], w: usize, h: usize, r: f32, first: usize) {
    if r.is_nan() || r <= 0.0 || first > 3 || w == 0 || h == 0 || data.len() < w * h * 4 {
        return;
    }
    let reach = (r + 0.5).ceil() as i32;
    // The disk by rows: `(dy, start, len)` into `weights`, which holds the
    // row's weights at the distances 0, 1, .. len - 1 from a source.
    let mut rows: Vec<(i32, usize, usize)> = Vec::new();
    let mut weights: Vec<u16> = Vec::new();
    for dy in -reach..=reach {
        let start = weights.len();
        for dx in 0..=reach {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            let wt = (r + 0.5 - d).clamp(0.0, 1.0);
            if wt <= 0.0 {
                break; // and every tap further out in this row
            }
            weights.push((wt * 256.0).round() as u16);
        }
        if weights.len() > start {
            rows.push((dy, start, weights.len() - start));
        }
    }
    // 255 x 256 = 65 280: a product fits a u16.
    let channels = 4 - first;
    let mut best = vec![0u16; w * h * channels];
    for sy in 0..h {
        let row = &data[sy * w * 4..(sy + 1) * w * 4];
        let mut x = 0;
        while x < w {
            if row[x * 4 + 3] == 0 {
                x += 1;
                continue;
            }
            let value = &row[x * 4 + first..x * 4 + 4];
            let x0 = x;
            x += 1;
            while x < w && row[x * 4 + 3] != 0 && row[x * 4 + first..x * 4 + 4] == *value {
                x += 1;
            }
            let x1 = x - 1;
            for &(dy, start, len) in &rows {
                let ty = sy as i64 + i64::from(dy);
                if ty < 0 || ty >= h as i64 {
                    continue;
                }
                let ty = ty as usize;
                grow_run(
                    &mut best[ty * w * channels..(ty + 1) * w * channels],
                    value,
                    (x0, x1),
                    &weights[start..start + len],
                );
            }
        }
    }
    for (i, px) in data.chunks_exact_mut(4).take(w * h).enumerate() {
        for c in 0..channels {
            px[first + c] = ((u32::from(best[i * channels + c]) + 128) >> 8).min(255) as u8;
        }
    }
}

/// One disk row of [`dilate_disk_channels`]: the run `x0..=x1` of `value`
/// (one byte per grown channel) into `target` (one row, `value.len()`
/// channels a pixel): a pixel `d` from the run's nearest pixel takes
/// `value * weights[d]` where that is more, for `d` up to `weights.len() - 1`.
#[inline]
fn grow_run(target: &mut [u16], value: &[u8], (x0, x1): (usize, usize), weights: &[u16]) {
    let channels = value.len();
    let w = target.len() / channels;
    #[cfg(test)]
    let mut touched = 0u64;
    let mut put = |x: usize, weight: u16| {
        let o = x * channels;
        for (t, &v) in target[o..o + channels].iter_mut().zip(value) {
            let grown = u16::from(v) * weight;
            if grown > *t {
                *t = grown;
            }
        }
        #[cfg(test)]
        {
            touched += 1;
        }
    };
    for x in x0..=x1 {
        put(x, weights[0]);
    }
    for (d, &weight) in weights.iter().enumerate().skip(1) {
        if let Some(x) = x0.checked_sub(d) {
            put(x, weight);
        }
        if x1 + d < w {
            put(x1 + d, weight);
        }
    }
    #[cfg(test)]
    DILATE_TOUCHED.with(|n| n.set(n.get() + touched));
}

#[cfg(test)]
thread_local! {
    /// Tests: the pixels `grow_run` has written on this thread (once per pixel
    /// and disk row a run reaches), the dilation's work.
    static DILATE_TOUCHED: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

/// Guards of the 2026-11-21 audit, area A8 (`audit/A8.md`): the disk dilation.
#[cfg(test)]
mod audit_a8_dilate_tests {
    use super::*;

    /// `dilate_disk` before A8-01 - every pixel with ink over every tap of
    /// the disk - kept as the reference. (`dilate_disk_tests::reference`
    /// keeps the gather before that.)
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn scatter_reference(p: &mut AzulPixmap, r: f32) {
        if r.is_nan() || r <= 0.0 {
            return;
        }
        let reach = (r + 0.5).ceil() as i32;
        let mut taps: Vec<(i32, i32, u32)> = Vec::new();
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                let w = (r + 0.5 - d).clamp(0.0, 1.0);
                if w > 0.0 {
                    taps.push((dx, dy, (w * 256.0).round() as u32));
                }
            }
        }
        let (w, h) = (p.width as i32, p.height as i32);
        let src = p.data.to_vec();
        let mut best = vec![0u32; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if src[i + 3] == 0 {
                    continue;
                }
                let px = [
                    u32::from(src[i]),
                    u32::from(src[i + 1]),
                    u32::from(src[i + 2]),
                    u32::from(src[i + 3]),
                ];
                for &(dx, dy, weight) in &taps {
                    let (ox, oy) = (x + dx, y + dy);
                    if ox < 0 || oy < 0 || ox >= w || oy >= h {
                        continue;
                    }
                    let o = ((oy * w + ox) * 4) as usize;
                    for c in 0..4 {
                        best[o + c] = best[o + c].max(px[c] * weight);
                    }
                }
            }
        }
        let out = &mut p.data;
        for (o, b) in best.iter().enumerate() {
            out[o] = ((b + 128) >> 8).min(255) as u8;
        }
    }

    /// A fixed-seed xorshift stream.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n.max(1)
        }

        #[allow(clippy::cast_possible_truncation)] // below 256
        fn byte(&mut self, at_most: u8) -> u8 {
            self.below(u64::from(at_most) + 1) as u8
        }
    }

    /// A `w` x `h` buffer like a shadow's glyphs: transparent, with strokes
    /// (rectangles of one pixel value: solid insides, partial edges, bars),
    /// lone pixels of any value, pixels of colour without alpha (which grow
    /// nothing), and ink on every border. `coloured`: the colour under the
    /// alpha is random (premultiplied-looking), else it is the alpha (white).
    #[allow(clippy::cast_possible_truncation)] // bounded by w and h
    fn glyphs_like(w: u32, h: u32, seed: u64, coloured: bool) -> AzulPixmap {
        let mut p = AzulPixmap::new_transparent(w, h).expect("a pixmap");
        let (wu, hu) = (w as usize, h as usize);
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let pixel = |rng: &mut Rng| {
            let a = if rng.below(2) == 0 { 255 } else { rng.byte(254) + 1 };
            if coloured {
                [rng.byte(a), rng.byte(a), rng.byte(a), a]
            } else {
                [a, a, a, a]
            }
        };
        let data = p.data_mut();
        let put = |data: &mut [u8], x: usize, y: usize, px: [u8; 4]| {
            if x < wu && y < hu {
                data[(y * wu + x) * 4..(y * wu + x) * 4 + 4].copy_from_slice(&px);
            }
        };
        for _ in 0..(wu * hu / 150 + 3) {
            let px = pixel(&mut rng);
            let (x0, y0) = (rng.below(w.into()) as usize, rng.below(h.into()) as usize);
            let (len, thick) = if rng.below(2) == 0 {
                (rng.below(u64::from(w) / 2 + 1) as usize + 1, rng.below(4) as usize + 1)
            } else {
                (rng.below(4) as usize + 1, rng.below(u64::from(h) / 2 + 1) as usize + 1)
            };
            for y in y0..y0 + thick {
                for x in x0..x0 + len {
                    put(data, x, y, px);
                }
            }
        }
        for _ in 0..(wu * hu / 40 + 2) {
            let px = pixel(&mut rng);
            put(data, rng.below(w.into()) as usize, rng.below(h.into()) as usize, px);
        }
        for _ in 0..(wu * hu / 200 + 1) {
            let rgb = [rng.byte(255), rng.byte(255), rng.byte(255), 0];
            put(data, rng.below(w.into()) as usize, rng.below(h.into()) as usize, rgb);
        }
        for i in 0..wu.max(hu) {
            let px = pixel(&mut rng);
            match i % 4 {
                0 => put(data, i % wu, 0, px),
                1 => put(data, i % wu, hu - 1, px),
                2 => put(data, 0, i % hu, px),
                _ => put(data, wu - 1, i % hu, px),
            }
        }
        p
    }

    /// Quarter-pixel radii 0 ..= 8, the radii whose `r + 0.5` is a whole
    /// pixel, and two large ones.
    fn radii() -> Vec<f32> {
        #[allow(clippy::cast_precision_loss)] // 0 ..= 32
        let mut r: Vec<f32> = (0..=32).map(|i| i as f32 * 0.25).collect();
        r.extend([0.1, 1.5, 3.5, 12.0, 17.3]);
        r
    }

    /// A8-01: the run dilation makes the bytes the scatter made, for every
    /// radius and buffer shape (one pixel, one row, one column, borders full
    /// of ink, colour without alpha, empty).
    /// Mutation: `weights[0]` -> `weights[1]` for a run's own pixels in
    /// `grow_run`, or `<=` -> `<` in the `break` of the disk rows, fails it.
    #[test]
    fn a_disk_grown_by_runs_is_the_disk_grown_by_pixels() {
        let shapes = [(1, 1), (1, 23), (23, 1), (2, 2), (17, 9), (40, 16)];
        for (w, h) in shapes {
            for seed in 1..=2 {
                for coloured in [false, true] {
                    for r in radii() {
                        let mut fast = glyphs_like(w, h, seed, coloured);
                        let mut slow = glyphs_like(w, h, seed, coloured);
                        dilate_disk(&mut fast, r);
                        scatter_reference(&mut slow, r);
                        assert!(
                            fast.data() == slow.data(),
                            "{w} x {h}, seed {seed}, coloured {coloured}, radius {r}: bytes differ"
                        );
                    }
                }
            }
        }
        for r in [1.0_f32, 2.25, 4.5, 8.0] {
            let mut fast = glyphs_like(130, 40, 7, true);
            let mut slow = glyphs_like(130, 40, 7, true);
            dilate_disk(&mut fast, r);
            scatter_reference(&mut slow, r);
            assert!(fast.data() == slow.data(), "130 x 40, radius {r}: bytes differ");
        }
        let mut empty = AzulPixmap::new_transparent(30, 20).expect("a pixmap");
        dilate_disk(&mut empty, 3.0);
        assert!(empty.data().iter().all(|b| *b == 0), "nothing grows from nothing");
    }

    /// A8-04: growing the alpha alone (`first == 3`) makes the alpha that
    /// growing every channel makes, and leaves the colour as it was; and after
    /// the stack blur too, which also treats each channel on its own. What a
    /// kept halo keeps.
    /// Mutation: `px[first + c]` -> `px[c]` in the write-back fails it.
    #[test]
    fn growing_the_alpha_alone_grows_the_alpha_the_whole_pixels_grow() {
        let alpha = |p: &AzulPixmap| p.data().chunks_exact(4).map(|px| px[3]).collect::<Vec<u8>>();
        let colour = |p: &AzulPixmap| {
            p.data()
                .chunks_exact(4)
                .flat_map(|px| [px[0], px[1], px[2]])
                .collect::<Vec<u8>>()
        };
        for (w, h) in [(1, 9), (17, 9), (40, 16)] {
            for r in radii() {
                for blur in [0_u32, 1, 3] {
                    let before = glyphs_like(w, h, 3, true);
                    let mut whole = glyphs_like(w, h, 3, true);
                    let mut alone = glyphs_like(w, h, 3, true);
                    dilate_disk(&mut whole, r);
                    dilate_disk_channels(alone.data_mut(), w as usize, h as usize, r, 3);
                    if blur == 0 {
                        assert_eq!(colour(&alone), colour(&before), "{w} x {h}, radius {r}");
                    }
                    for p in [&mut whole, &mut alone] {
                        if blur > 0 {
                            let stride = (w * 4) as i32;
                            let mut ra = unsafe {
                                RowAccessor::new_with_buf(p.data_mut().as_mut_ptr(), w, h, stride)
                            };
                            stack_blur_rgba32(&mut ra, blur, blur);
                        }
                    }
                    assert_eq!(alpha(&alone), alpha(&whole), "{w} x {h}, radius {r}, blur {blur}");
                }
            }
        }
    }

    /// A8-01: the work follows the runs, not the disk's area. A solid 64 x 16
    /// block (one run a row), grown by 2 and by 8 px: the pixels written are
    /// 5,376 and 20,736 (x 3.9); one run per pixel, as every pixel walked
    /// every tap before, it is 21,504 and 230,400 (x 10.7).
    /// Mutation: end every run after one pixel (`x0 == x1`) fails it.
    #[test]
    fn the_dilation_work_grows_with_the_radius_not_with_the_disk_area() {
        let work = |r: f32| {
            let (w, h) = (64 + 2 * 10, 16 + 2 * 10);
            let mut p = AzulPixmap::new_transparent(w, h).expect("a pixmap");
            let data = p.data_mut();
            for y in 10..26 {
                for x in 10..74 {
                    data[(y * w as usize + x) * 4..(y * w as usize + x) * 4 + 4]
                        .copy_from_slice(&[255; 4]);
                }
            }
            DILATE_TOUCHED.with(|n| n.set(0));
            dilate_disk(&mut p, r);
            DILATE_TOUCHED.with(core::cell::Cell::get)
        };
        let (small, large) = (work(2.0), work(8.0));
        assert_eq!(small, 5_376, "5 disk rows x 16 runs x (64 + 2 b)");
        assert_eq!(large, 20_736);
        assert!(large < 6 * small, "{large} vs {small}: the work grows with the disk area");
    }
}

/// One glyph run drawn into a shadow's buffer: its glyphs and clip rect in
/// CSS px of the target, its clip in device px of the target.
pub(super) struct BufferedRun<'a> {
    pub(super) glyphs: &'a [GlyphInstance],
    pub(super) font_hash: FontHash,
    pub(super) font_size_px: f32,
    pub(super) clip_rect: LogicalRect,
    pub(super) clip: AzRect,
}

/// `runs` drawn in `color` into a transparent `buffer` - device px of the
/// target, where it will land - grown by `spread` pixels ([`dilate_disk`])
/// and blurred by `radius` pixels: a text shadow before it is composited.
/// Always grayscale coverage: the buffer is transparent, and LCD text
/// assumes an opaque backdrop.
///
/// Grown and blurred in the part of `buffer` they can paint
/// ([`grown_glyphs`]), and put back in a clear buffer of `buffer`'s size;
/// [`blurred_glyphs_in_ink`] hands out that part alone.
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_arguments
)] // the part lies inside the buffer
pub(super) fn blurred_glyphs(
    runs: &[BufferedRun<'_>],
    color: ColorU,
    buffer: PixelBox,
    radius: u32,
    spread: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
) -> Option<AzulPixmap> {
    let (part, pixmap) = blurred_glyphs_in_ink(
        runs,
        color,
        buffer,
        radius,
        spread,
        renderer_resources,
        font_manager,
        dpi_factor,
        glyph_cache,
        scroll_offset,
    )?;
    let Some(pixmap) = pixmap else {
        return AzulPixmap::new_transparent(buffer.width, buffer.height);
    };
    if part == buffer {
        return Some(pixmap);
    }
    let mut whole = AzulPixmap::new_transparent(buffer.width, buffer.height)?;
    let (w, row) = (buffer.width as usize, pixmap.width as usize * 4);
    let (left, top) = ((part.x - buffer.x) as usize, (part.y - buffer.y) as usize);
    for (y, src) in pixmap.data().chunks_exact(row).enumerate() {
        let start = ((top + y) * w + left) * 4;
        whole.data_mut()[start..start + row].copy_from_slice(src);
    }
    Some(whole)
}

/// [`blurred_glyphs`] cut to the part of `buffer` it can paint: that part
/// (device px of the target, inside `buffer`) and its pixels; every pixel of
/// `buffer` outside it is clear. A part of no pixels when nothing is drawn.
/// What the GPU halo (`text_halo.rs` `Scope::draw`) can keep in place of the
/// whole buffer, before it cuts it to its ink.
#[allow(clippy::too_many_arguments, clippy::cast_possible_wrap)]
pub(super) fn blurred_glyphs_in_ink(
    runs: &[BufferedRun<'_>],
    color: ColorU,
    buffer: PixelBox,
    radius: u32,
    spread: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
) -> Option<(PixelBox, Option<AzulPixmap>)> {
    let grown = grown_glyphs(
        runs,
        color,
        buffer,
        radius,
        spread,
        0,
        renderer_resources,
        font_manager,
        dpi_factor,
        glyph_cache,
        scroll_offset,
    )?;
    Some(match grown {
        GrownGlyphs::Clear => (PixelBox { width: 0, height: 0, ..buffer }, None),
        GrownGlyphs::Ink { at, pixmap } => (
            PixelBox {
                x: buffer.x + at.0 as i32,
                y: buffer.y + at.1 as i32,
                width: pixmap.width,
                height: pixmap.height,
            },
            Some(pixmap),
        ),
    })
}

/// The coverage of [`blurred_glyphs`] - the alpha of its pixels, row by row -
/// with the glyphs grown and blurred on their alpha alone: what a kept halo
/// keeps ([`draw_kept_text_shadow`]). The same bytes as the alpha of
/// `blurred_glyphs`: the dilation and the stack blur treat each channel on its
/// own, so the alpha they make depends on the alpha alone; growing the colour
/// as well, a halo's three quarters of the dilation, was thrown away.
///
/// Cut to the part of `buffer` it can paint, which it returns with the
/// coverage (device px of the target; no pixels when nothing is drawn): every
/// pixel of `buffer` outside that part is clear.
#[allow(clippy::too_many_arguments, clippy::cast_possible_wrap)]
pub(super) fn blurred_glyph_coverage(
    runs: &[BufferedRun<'_>],
    color: ColorU,
    buffer: PixelBox,
    radius: u32,
    spread: f32,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
) -> Option<(PixelBox, Vec<u8>)> {
    let grown = grown_glyphs(
        runs,
        color,
        buffer,
        radius,
        spread,
        3,
        renderer_resources,
        font_manager,
        dpi_factor,
        glyph_cache,
        scroll_offset,
    )?;
    Some(match grown {
        GrownGlyphs::Clear => (PixelBox { width: 0, height: 0, ..buffer }, Vec::new()),
        GrownGlyphs::Ink { at, pixmap } => (
            PixelBox {
                x: buffer.x + at.0 as i32,
                y: buffer.y + at.1 as i32,
                width: pixmap.width,
                height: pixmap.height,
            },
            pixmap.data().chunks_exact(4).map(|p| p[3]).collect(),
        ),
    })
}

/// What [`grown_glyphs`] made: nothing drawn, or the part of the buffer the
/// glyphs can paint - at `at` (px from the buffer's top left) - grown and
/// blurred.
enum GrownGlyphs {
    Clear,
    Ink { at: (u32, u32), pixmap: AzulPixmap },
}

/// `runs` drawn into `buffer` ([`buffered_glyphs`]), then grown by `spread`
/// (channels `first..4`, [`dilate_disk_channels`]) and blurred by `radius`
/// in the part of the buffer that can come out non-clear ([`halo_work_box`]),
/// cut out first. The halo's box is the text's visual bounds plus its reach
/// (1 em left and right, 1.5 em above, 0.75 em below the pens): about three
/// times the ink, and the dilation's accumulator (2 bytes a pixel a channel)
/// and both passes of the blur were paid over all of it.
///
/// The same pixels as growing and blurring the whole buffer: outside the
/// part, the whole buffer's pixels are clear before the dilation and stay
/// clear (they lie beyond the dilation's reach plus the blur's radius from
/// every drawn pixel), and inside it nothing changes - the dilation only
/// writes within its reach, and the blur's edge extension at the part's edge
/// repeats a clear pixel, as the whole buffer has clear pixels there.
#[allow(clippy::too_many_arguments)]
fn grown_glyphs(
    runs: &[BufferedRun<'_>],
    color: ColorU,
    buffer: PixelBox,
    radius: u32,
    spread: f32,
    first: usize,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
) -> Option<GrownGlyphs> {
    let drawn = buffered_glyphs(
        runs,
        color,
        buffer,
        renderer_resources,
        font_manager,
        dpi_factor,
        glyph_cache,
        scroll_offset,
    )?;
    let Some((x0, y0, x1, y1)) = halo_work_box(&drawn, spread, radius) else {
        return Some(GrownGlyphs::Clear);
    };
    let mut part = if (x0, y0, x1, y1) == (0, 0, drawn.width, drawn.height) {
        drawn
    } else {
        let mut part = AzulPixmap::new_transparent(x1 - x0, y1 - y0)?;
        let (w, row) = (drawn.width as usize, (x1 - x0) as usize * 4);
        for (y, dst) in part.data_mut().chunks_exact_mut(row).enumerate() {
            let start = ((y0 as usize + y) * w + x0 as usize) * 4;
            dst.copy_from_slice(&drawn.data()[start..start + row]);
        }
        part
    };
    #[cfg(test)]
    HALO_WORK_PIXELS.with(|n| n.set(n.get() + u64::from(part.width) * u64::from(part.height)));
    let (w, h) = (part.width as usize, part.height as usize);
    dilate_disk_channels(part.data_mut(), w, h, spread, first);
    blur_buffer(&mut part, radius);
    Some(GrownGlyphs::Ink { at: (x0, y0), pixmap: part })
}

#[cfg(test)]
std::thread_local! {
    /// Pixels [`grown_glyphs`] grew and blurred on this thread.
    static HALO_WORK_PIXELS: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
}

/// The part of `drawn` (glyphs drawn, not yet grown or blurred) that growing
/// by `spread` and blurring by `radius` can make non-clear, with one clear
/// pixel more all round, as `(x0, y0, x1, y1)` pixels of `drawn` (end
/// exclusive): the box of its drawn pixels (alpha not 0; a clear pixel is
/// clear in every channel, the colour being premultiplied) grown by the
/// dilation's reach `ceil(spread + 0.5)` (as `dilate_disk_channels`), the
/// blur's `radius`, and 1. `None` when nothing is drawn.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a bounded reach
fn halo_work_box(drawn: &AzulPixmap, spread: f32, radius: u32) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = (drawn.width as usize, drawn.height as usize);
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for (y, row) in drawn.data().chunks_exact(w * 4).enumerate().take(h) {
        let Some(first) = row.chunks_exact(4).position(|p| p[3] != 0) else {
            continue;
        };
        let last = row.chunks_exact(4).rposition(|p| p[3] != 0).unwrap_or(first);
        x0 = x0.min(first);
        x1 = x1.max(last + 1);
        y0 = y0.min(y);
        y1 = y + 1;
    }
    if x0 == usize::MAX {
        return None;
    }
    // `dilate_disk_channels` grows nothing for a NaN or non-positive radius.
    let reach = if spread > 0.0 {
        (spread + 0.5).ceil() as usize
    } else {
        0
    };
    let m = reach.saturating_add(radius as usize).saturating_add(1);
    Some((
        x0.saturating_sub(m) as u32,
        y0.saturating_sub(m) as u32,
        x1.saturating_add(m).min(w) as u32,
        y1.saturating_add(m).min(h) as u32,
    ))
}

/// The stack blur of a text shadow's buffer by `radius` pixels (none for 0).
#[allow(clippy::cast_possible_wrap)] // a bounded stride
fn blur_buffer(tmp: &mut AzulPixmap, radius: u32) {
    if radius > 0 {
        let (w, h) = (tmp.width, tmp.height);
        let stride = (w * 4) as i32;
        let mut ra = unsafe { RowAccessor::new_with_buf(tmp.data.as_mut_ptr(), w, h, stride) };
        stack_blur_rgba32(&mut ra, radius, radius);
    }
}

/// `runs` drawn in `color` into a transparent `buffer` (device px of the
/// target, where it will land): the glyphs of a text shadow before they are
/// grown and blurred. Grayscale coverage: the buffer is transparent.
#[allow(clippy::cast_possible_wrap, clippy::cast_precision_loss, clippy::too_many_arguments)]
fn buffered_glyphs(
    runs: &[BufferedRun<'_>],
    color: ColorU,
    buffer: PixelBox,
    renderer_resources: &RendererResources,
    font_manager: &FontManager<FontRef>,
    dpi_factor: f32,
    glyph_cache: &mut GlyphCache,
    scroll_offset: (f32, f32),
) -> Option<AzulPixmap> {
    // Zeroed by the allocator: the bytes `new` + `fill(0, 0, 0, 0)` wrote.
    let mut tmp = AzulPixmap::new_transparent(buffer.width, buffer.height)?;
    // The buffer starts at (buffer.x, buffer.y) of the target: the glyphs and
    // their clips move by that much to land in it.
    let (dx, dy) = (buffer.x as f32 / dpi_factor, buffer.y as f32 / dpi_factor);
    for run in runs {
        let clip_rect = LogicalRect {
            origin: LogicalPosition {
                x: run.clip_rect.origin.x - dx,
                y: run.clip_rect.origin.y - dy,
            },
            size: run.clip_rect.size,
        };
        let clip = AzRect::from_xywh(
            run.clip.x - buffer.x as f32,
            run.clip.y - buffer.y as f32,
            run.clip.width,
            run.clip.height,
        );
        if clip.is_none() {
            continue;
        }
        render_text(
            run.glyphs,
            run.font_hash,
            run.font_size_px,
            color,
            &mut tmp,
            &clip_rect,
            clip,
            renderer_resources,
            font_manager,
            dpi_factor,
            glyph_cache,
            (scroll_offset.0 + dx, scroll_offset.1 + dy),
            true,
        );
    }
    Some(tmp)
}

/// Guards of the 2026-11-21 audit, area A8 (`audit/A8.md`): the kept halo's coverage.
#[cfg(all(test, feature = "std"))]
mod audit_a8_halo_tests {
    use super::{
        lcd_pretile_tests::{load_test_font_pub, rr_with_pub, shape_pub},
        *,
    };

    /// A8-04: the coverage a kept halo keeps is, byte for byte, the alpha of
    /// the halo `blurred_glyphs` draws (in all four channels), for the
    /// editor's solid spread, a spread with a blur, and a blur alone.
    /// Mutation: `dilate_disk_channels(.., 3)` -> `(.., 2)` in
    /// `blurred_glyph_coverage` (grows a colour channel instead) fails it.
    #[test]
    fn a_kept_halos_coverage_is_the_alpha_of_the_drawn_halo() {
        let Some(font) = load_test_font_pub() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, fm, font_hash) = rr_with_pub(&font);
        let glyphs = shape_pub(&font, "Tuebingen", 24.0, 12.25, 40.0);
        let clip_rect = LogicalRect {
            origin: LogicalPosition { x: 8.0, y: 12.0 },
            size: LogicalSize { width: 150.0, height: 40.0 },
        };
        let clip = AzRect::from_xywh(8.0, 12.0, 150.0, 40.0).expect("a clip");
        let buffer = PixelBox { x: 0, y: 0, width: 180, height: 70 };
        for (radius, spread) in [(0_u32, 1.2_f32), (0, 2.5), (2, 1.5), (3, 0.0)] {
            let run = || BufferedRun {
                glyphs: &glyphs,
                font_hash,
                font_size_px: 24.0,
                clip_rect,
                clip,
            };
            let mut cache = GlyphCache::new();
            let whole = blurred_glyphs(
                &[run()],
                ColorU::WHITE,
                buffer,
                radius,
                spread,
                &rr,
                &fm,
                1.0,
                &mut cache,
                (0.0, 0.0),
            )
            .expect("a buffer");
            let coverage = blurred_glyph_coverage(
                &[run()],
                ColorU::WHITE,
                buffer,
                radius,
                spread,
                &rr,
                &fm,
                1.0,
                &mut cache,
                (0.0, 0.0),
            )
            .expect("a buffer");
            let alpha: Vec<u8> = whole.data().chunks_exact(4).map(|p| p[3]).collect();
            assert!(alpha.iter().any(|a| *a > 0), "the glyphs paint");
            // the kept part (A8-29) put back into the whole box
            let (part, coverage) = coverage;
            let mut placed = vec![0u8; alpha.len()];
            for y in 0..part.height as usize {
                for x in 0..part.width as usize {
                    let at = (part.y as usize + y) * buffer.width as usize + part.x as usize + x;
                    placed[at] = coverage[y * part.width as usize + x];
                }
            }
            assert_eq!(placed, alpha, "radius {radius}, spread {spread}");
        }
    }

    /// A8-29: a halo is grown and blurred in the part of its box its glyphs
    /// can paint, and that gives the pixels growing and blurring the whole box
    /// gives (the old pipeline, kept here as the reference), for spreads and
    /// blurs up to past the part's margin, glyphs against every edge of the
    /// box, and a box with nothing drawn; the work is a fraction of the box.
    /// Mutation: the blur radius left out of `halo_work_box`'s margin fails
    /// the equality (a blur of 5 or 9 px reaches past the part).
    #[test]
    fn a_halo_grown_in_its_ink_box_is_the_halo_grown_in_its_whole_box() {
        let Some(font) = load_test_font_pub() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, fm, font_hash) = rr_with_pub(&font);
        // a label in a box padded as the text's visual bounds pad it, a
        // label pressed against the top left, one past the bottom right
        let cases = [
            ("Tuebingen", 24.0_f32, 40.0_f32, 50.0_f32, PixelBox { x: 0, y: 0, width: 220, height: 110 }),
            ("Neckar", 18.0, 1.0, 15.0, PixelBox { x: 0, y: 0, width: 90, height: 40 }),
            ("Altstadt", 20.0, 120.0, 70.0, PixelBox { x: 100, y: 40, width: 80, height: 40 }),
            ("   ", 20.0, 20.0, 30.0, PixelBox { x: 0, y: 0, width: 60, height: 40 }),
        ];
        for (text, size, x, baseline, buffer) in cases {
            let glyphs = shape_pub(&font, text, size, x, baseline);
            let clip_rect = LogicalRect {
                origin: LogicalPosition { x: 0.0, y: 0.0 },
                size: LogicalSize { width: 400.0, height: 200.0 },
            };
            let clip = AzRect::from_xywh(0.0, 0.0, 400.0, 200.0).expect("a clip");
            let run = || BufferedRun {
                glyphs: &glyphs,
                font_hash,
                font_size_px: size,
                clip_rect,
                clip,
            };
            for (radius, spread) in [(0_u32, 1.2_f32), (0, 6.5), (2, 1.5), (5, 0.0), (9, 3.0)] {
                let mut cache = GlyphCache::new();
                let mut reference = buffered_glyphs(
                    &[run()],
                    ColorU::WHITE,
                    buffer,
                    &rr,
                    &fm,
                    1.0,
                    &mut cache,
                    (0.0, 0.0),
                )
                .expect("a buffer");
                dilate_disk(&mut reference, spread);
                blur_buffer(&mut reference, radius);
                let work = || HALO_WORK_PIXELS.with(core::cell::Cell::get);
                let before = work();
                let halo = blurred_glyphs(
                    &[run()],
                    ColorU::WHITE,
                    buffer,
                    radius,
                    spread,
                    &rr,
                    &fm,
                    1.0,
                    &mut cache,
                    (0.0, 0.0),
                )
                .expect("a buffer");
                assert_eq!(
                    halo.data(),
                    reference.data(),
                    "{text:?} radius {radius}, spread {spread}"
                );
                let area = u64::from(buffer.width) * u64::from(buffer.height);
                assert!(work() - before <= area, "never more than the box");
                if text == "Tuebingen" && radius <= 2 {
                    assert!(
                        (work() - before) * 10 < area * 7,
                        "{} of {area} px: the padding is not grown",
                        work() - before
                    );
                }
            }
        }
    }

    /// A8-03: a repaint of a run whose glyph paths are cached decodes no glyph
    /// (grayscale and the LCD sweep), and paints the pixels the first paint did.
    /// Mutation: in `get_or_build_decoding`, count and call `decode()` before the
    /// `contains_key` check (a decode per glyph, as before) fails it.
    #[test]
    fn a_repaint_of_cached_glyphs_decodes_no_glyph() {
        let Some(font) = load_test_font_pub() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, fm, font_hash) = rr_with_pub(&font);
        let glyphs = shape_pub(&font, "Tuebingen Altstadt", 18.0, 10.25, 30.0);
        let clip_rect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize { width: 300.0, height: 60.0 },
        };
        let decodes = || crate::glyph_cache::GLYPH_DECODES.with(core::cell::Cell::get);
        for grayscale in [true, false] {
            let paint = |cache: &mut GlyphCache| {
                let mut pm = AzulPixmap::new(300, 60).expect("a pixmap");
                pm.fill(255, 255, 255, 255);
                render_text(
                    &glyphs,
                    font_hash,
                    18.0,
                    ColorU { r: 10, g: 10, b: 10, a: 255 },
                    &mut pm,
                    &clip_rect,
                    None,
                    &rr,
                    &fm,
                    1.0,
                    cache,
                    (0.0, 0.0),
                    grayscale,
                );
                pm
            };
            let mut cache = GlyphCache::new();
            let before = decodes();
            let first = paint(&mut cache);
            let after_first = decodes();
            assert!(after_first > before, "grayscale {grayscale}: the first paint decodes");
            let second = paint(&mut cache);
            assert_eq!(decodes(), after_first, "grayscale {grayscale}: a repaint decodes nothing");
            assert!(first.data() == second.data(), "grayscale {grayscale}: the same pixels");
            assert!(first.data().iter().any(|b| *b < 200), "grayscale {grayscale}: text paints");
        }
    }
}

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // software rasterizer:
                                                                       // bounded pixel/coord/colour
                                                                       // casts
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
    render_border_xy(
        pixmap,
        bounds,
        color,
        width,
        border_style,
        border_radius,
        border_radius,
        clip,
        dpi_factor,
    );
}

/// [`render_border`] with a horizontal (`border_radius`) and a vertical (`radius_y`) radius
/// for each corner: an elliptical corner, which a percentage radius of a box that is not
/// square is (CSS resolves the horizontal radius against the box's width and the vertical one
/// against its height).
#[allow(clippy::too_many_arguments)]
fn render_border_xy(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    color: ColorU,
    width: f32,
    border_style: azul_css::props::style::border::BorderStyle,
    border_radius: &BorderRadius,
    radius_y: &BorderRadius,
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
    let mut path = build_rounded_rect_path_xy(&rect, border_radius, radius_y, dpi_factor);

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
        let inner_radius_y = BorderRadius {
            top_left: (radius_y.top_left - width).max(0.0),
            top_right: (radius_y.top_right - width).max(0.0),
            bottom_right: (radius_y.bottom_right - width).max(0.0),
            bottom_left: (radius_y.bottom_left - width).max(0.0),
        };
        let mut inner =
            build_rounded_rect_path_xy(&ir, &inner_radius, &inner_radius_y, dpi_factor);
        path.concat_path(&mut inner, 0);
    }

    // 3. Render based on border style
    match border_style {
        BorderStyle::Dashed | BorderStyle::Dotted => {
            // For dashed/dotted: stroke the border path with dash pattern
            use agg_rust::{conv_dash::ConvDash, conv_stroke::ConvStroke};

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
            // At least one device pixel: a border thinner than one (0.5px at dpi 1) had
            // `swi == 0`, and the strips below were then the inverted bars `yi ..= yi - 1`
            // (nothing, or - normalised - two rows, one of them outside the box). WebRender
            // draws one device pixel for it (`floor`, at least 1).
            let swi = (sw as i32).max(1);
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
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
fn render_border_sides(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    colors: [ColorU; 4], // top, right, bottom, left
    widths: [f32; 4],    // top, right, bottom, left
    styles: [azul_css::props::style::border::BorderStyle; 4],
    border_radius: &BorderRadius,
    clip: Option<AzRect>,
    dpi_factor: f32,
) {
    use azul_css::props::style::border::BorderStyle;

    let Some(rect) = logical_rect_to_az_rect(bounds, dpi_factor) else {
        return;
    };
    // A side with `border-style: none` or `hidden` paints nothing, whatever width and colour
    // the cascade left on it (`render_border` returns for them; this path ignored the styles
    // and painted such a side solid - A1-46). Its width still insets the other sides.
    let mut colors = colors;
    for (color, style) in colors.iter_mut().zip(styles) {
        if matches!(style, BorderStyle::None | BorderStyle::Hidden) {
            color.a = 0;
        }
    }

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

    // A side that is drawn in dashes or dots.
    let patterned = |side: usize| {
        widths[side] > 0.0
            && colors[side].a > 0
            && matches!(styles[side], BorderStyle::Dashed | BorderStyle::Dotted)
    };

    if border_radius.is_zero() && !(0..4).any(patterned) {
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
            // At least one device pixel (A1-46): a side thinner than one (0.5 px at dpi 1)
            // had `iy as i32 - 1 < oy as i32`, an inverted bar, which `blend_bar` normalises
            // into two rows, one of them outside the box. `render_border` has the same rule.
            rb.blend_bar(
                ox as i32,
                oy as i32,
                (ox + ow) as i32 - 1,
                (iy as i32 - 1).max(oy as i32),
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
                ((iy + ih) as i32).min((oy + oh) as i32 - 1),
                (ox + ow) as i32 - 1,
                (oy + oh) as i32 - 1,
                &ac,
                255,
            );
        }
        // The rows between the top and the bottom side: none when the two meet.
        let between = iy as i32 <= (iy + ih) as i32 - 1;
        // Left: between top and bottom
        if widths[3] > 0.0 && colors[3].a > 0 && between {
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
                (ix as i32 - 1).max(ox as i32),
                (iy + ih) as i32 - 1,
                &ac,
                255,
            );
        }
        // Right
        if widths[1] > 0.0 && colors[1].a > 0 && between {
            let c = colors[1];
            let ac = Rgba8::new(
                u32::from(c.r),
                u32::from(c.g),
                u32::from(c.b),
                u32::from(c.a),
            );
            rb.blend_bar(
                ((ix + iw) as i32).min((ox + ow) as i32 - 1),
                iy as i32,
                (ox + ow) as i32 - 1,
                (iy + ih) as i32 - 1,
                &ac,
                255,
            );
        }
    } else {
        // A radius, or a side in dashes or dots (A1-47): the border's RING - the rounded outer
        // edge minus the inner edge - filled once, each pixel in the colour of the side it
        // belongs to (`BorderSideSpans`). The sides used to be four straight trapezoids,
        // filled solid: under a radius the border had square corners and the arcs between
        // them were not painted at all, and a dashed or dotted side was a solid one.
        let mut ring = build_rounded_rect_path(&rect, border_radius, dpi_factor);
        if let Some(inner) = AzRect::from_xywh(ix as f32, iy as f32, iw as f32, ih as f32) {
            // The inner edge's corners are elliptical: the outer radius less the width of
            // the side it meets on each axis (CSS Backgrounds 3, 5.2); `widths` are logical,
            // as the radii are.
            let less = |r: f32, w: f32| (r - w.max(0.0)).max(0.0);
            let [wt_l, wr_l, wb_l, wl_l] = widths;
            let inner_x = BorderRadius {
                top_left: less(border_radius.top_left, wl_l),
                top_right: less(border_radius.top_right, wr_l),
                bottom_right: less(border_radius.bottom_right, wr_l),
                bottom_left: less(border_radius.bottom_left, wl_l),
            };
            let inner_y = BorderRadius {
                top_left: less(border_radius.top_left, wt_l),
                top_right: less(border_radius.top_right, wt_l),
                bottom_right: less(border_radius.bottom_right, wb_l),
                bottom_left: less(border_radius.bottom_left, wb_l),
            };
            let mut hole = build_rounded_rect_path_xy(&inner, &inner_x, &inner_y, dpi_factor);
            ring.concat_path(&mut hole, 0);
        }
        // The pattern `render_border_xy` strokes: a dash of three widths and a gap of one, a
        // dot of one width and a gap of one.
        let pattern = |side: usize, width: f64| {
            if !patterned(side) {
                (0.0, 0.0)
            } else if styles[side] == BorderStyle::Dashed {
                (3.0 * width, width)
            } else {
                (width, width)
            }
        };
        let mut spans = BorderSideSpans {
            outer: (ox, oy, ow, oh),
            widths: [wt, wr, wb, wl],
            colors,
            dashes: [pattern(0, wt), pattern(1, wr), pattern(2, wb), pattern(3, wl)],
        };
        super::pixmap::agg_fill_spans_clipped(pixmap, &mut ring, FillingRule::EvenOdd, &mut spans, clip);
    }
}

/// The colour of each pixel of a border whose sides differ ([`render_border_sides`]): a pixel
/// belongs to the side it is least deep in, the depth counted in that side's own width. In a
/// square corner that is the mitre from the outer to the inner corner; in a rounded one the
/// same line carried on through the arc. A side without a width owns nothing; a side without
/// a style (its colour made transparent) owns its pixels and leaves them unpainted. Along a
/// dashed or dotted side the alpha is the part of the pixel that lies in a dash.
struct BorderSideSpans {
    /// The border box, device px: x, y, width, height.
    outer: (f64, f64, f64, f64),
    /// Device widths: top, right, bottom, left.
    widths: [f64; 4],
    colors: [ColorU; 4],
    /// Dash and gap, device px, per side; `(0, 0)` for a solid one.
    dashes: [(f64, f64); 4],
}

/// How much of the pixel `[at - 0.5, at + 0.5]` along a side lies in a dash: dashes of `dash`
/// px start every `dash + gap` px from 0.
fn dash_coverage(at: f64, dash: f64, gap: f64) -> f64 {
    let period = dash + gap;
    // The length in dashes from 0 to `v`.
    let filled = |v: f64| {
        let k = (v / period).floor();
        k * dash + (v - k * period).min(dash)
    };
    (filled(at + 0.5) - filled(at - 0.5)).clamp(0.0, 1.0)
}

impl agg_rust::renderer_scanline::SpanGenerator for BorderSideSpans {
    type Color = Rgba8;

    fn prepare(&mut self) {}

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)] // an alpha, a span index
    fn generate(&mut self, span: &mut [Rgba8], x: i32, y: i32, len: u32) {
        let (ox, oy, ow, oh) = self.outer;
        let py = f64::from(y) + 0.5;
        for (i, out) in span.iter_mut().take(len as usize).enumerate() {
            let px = f64::from(x) + i as f64 + 0.5;
            // How deep the pixel is in each side: top, right, bottom, left.
            let depth = [py - oy, ox + ow - px, oy + oh - py, px - ox];
            let mut owner: Option<(usize, f64)> = None;
            for side in 0..4 {
                if self.widths[side] > 0.0 {
                    let d = depth[side] / self.widths[side];
                    if owner.is_none_or(|(_, least)| d < least) {
                        owner = Some((side, d));
                    }
                }
            }
            *out = match owner {
                Some((side, _)) if self.colors[side].a > 0 => {
                    let c = self.colors[side];
                    let (dash, gap) = self.dashes[side];
                    let in_dash = if dash > 0.0 && gap > 0.0 {
                        // Along the side: x for top and bottom, y for left and right.
                        let along = if side % 2 == 0 { px - ox } else { py - oy };
                        dash_coverage(along, dash, gap)
                    } else {
                        1.0
                    };
                    Rgba8::new(
                        u32::from(c.r),
                        u32::from(c.g),
                        u32::from(c.b),
                        (f64::from(c.a) * in_dash).round() as u32,
                    )
                }
                _ => Rgba8::new(0, 0, 0, 0),
            };
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
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine
                                 // (one branch per case)
fn render_image(
    pixmap: &mut AzulPixmap,
    bounds: &LogicalRect,
    image: &ImageRef,
    border_radius: &BorderRadius,
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

    // A `border-radius` on the <img> confines the pixels as well as the border:
    // resolve the corners once here and hand them to whichever path paints.
    let radii = resolved_corner_radii(&rect, border_radius, dpi_factor);
    let mask = if radii.iter().any(|r| *r > 0.0) {
        Some((&rect, radii))
    } else {
        None
    };

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
                // The placeholder stands in for the image, so it takes the image's
                // shape: a `border-radius` on the <img> has to round the grey too,
                // or a rounded surface shows square grey corners poking out past
                // its own border (the frontpage `opengl` shot, whose GL callback
                // has no GPU to run on and so is all placeholder).
                let mut path = build_rounded_rect_path(&rect, border_radius, dpi_factor);
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
                    "[azul][cpurender] a RenderImageCallback image was composited as a flat grey \
                     placeholder: the frame was rendered without content preparation \
                     (LayoutWindow::prepare_frame_cpu), so the callback's content cannot appear \
                     (logged once)"
                );
            });
            let gray = Rgba8::new(200, 200, 200, 255);
            // The placeholder stands in for the image, so it takes the image's
            // shape: a `border-radius` on the <img> has to round the grey too,
            // or a rounded surface shows square grey corners poking out past
            // its own border (the frontpage `opengl` shot, whose GL callback
            // has no GPU to run on and so is all placeholder).
            let mut path = build_rounded_rect_path(&rect, border_radius, dpi_factor);
            agg_fill_path(pixmap, &mut path, &gray, FillingRule::NonZero);
            return;
        }
        // A `NullImage` has no pixels, so nothing is drawn - as WebRender
        // draws nothing for it (no pixels to upload) and a browser's `<img>`
        // without a picture, `<video>` without a frame or blank `<canvas>`
        // shows the box's own background. Both kinds: an `<img src>` nobody
        // supplied (the XML loaders' placeholder, its tag the src; it stays
        // in the display list for a renderer that resolves srcs itself,
        // printpdf's HTML bridge), and an app's "nothing to show yet" without
        // a tag (a remote video tile before its first frame, an empty
        // monitor, a render callback's fallback, a GL texture's CPU copy).
        // The grey placeholders above are for pixels that exist but cannot be
        // read here.
        DecodedImage::NullImage { .. } | DecodedImage::Gl(_) => return,
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
        mask,
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
    use azul_core::resources::{Nv12Layout, RawImageFormat, YuvCoefficients};
    // NV12 (a camera / decoder frame): the row's luma with the chroma of its
    // row pair, through the one YCbCr table - the same values
    // `SrcImage::pixel` gives, so the blit stays byte-identical to `sample`.
    if let Some(coeffs) = YuvCoefficients::of(src.format) {
        let layout = Nv12Layout::new(src.width as usize, src.height as usize);
        coeffs.row_to_rgba(src.bytes, &layout, y as usize, out);
        return;
    }
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
///
/// The row's pixels in the pixmap are taken as ONE slice, cut where the buffer ends (a pixel
/// that does not fit whole is not written, as before): the loop used to check the buffer's
/// length for every pixel and index it four to eight times, each index a check of its own.
/// Eight pixels that are all opaque - a map tile, a photo, a video frame - are a copy of 32
/// bytes; anything else goes pixel by pixel ([`composite_rgba_pixel`]). The same bytes.
fn composite_rgba_row(pixmap: &mut AzulPixmap, di_base: usize, stage: &[u8]) {
    let data: &mut [u8] = &mut pixmap.data;
    let Some(room) = data.len().checked_sub(di_base) else {
        return;
    };
    let n = (room / 4).min(stage.len() / 4);
    let dst = &mut data[di_base..di_base + n * 4];
    let src = &stage[..n * 4];
    let mut dst_chunks = dst.chunks_exact_mut(32);
    let mut src_chunks = src.chunks_exact(32);
    for (d, q) in (&mut dst_chunks).zip(&mut src_chunks) {
        if q.iter().skip(3).step_by(4).all(|alpha| *alpha == 255) {
            d.copy_from_slice(q);
        } else {
            for (d, q) in d.chunks_exact_mut(4).zip(q.chunks_exact(4)) {
                composite_rgba_pixel(d, q);
            }
        }
    }
    for (d, q) in dst_chunks
        .into_remainder()
        .chunks_exact_mut(4)
        .zip(src_chunks.remainder().chunks_exact(4))
    {
        composite_rgba_pixel(d, q);
    }
}

/// One premultiplied RGBA pixel `q` over the pixel `d`.
#[inline]
#[allow(clippy::cast_possible_truncation)] // every result is clamped to 255
fn composite_rgba_pixel(d: &mut [u8], q: &[u8]) {
    let sa = u32::from(q[3]);
    if sa == 255 {
        d.copy_from_slice(q);
    } else if sa > 0 {
        // The source is PREMULTIPLIED: an `ImageRef` stores its pixels
        // premultiplied (`RawImage` load premultiplies straight input,
        // the GPU's convention), and the samples keep that. So
        // dst = src + dst * (255 - sa) / 255 - the straight-alpha formula
        // (src * sa) multiplied by alpha a second time and painted every
        // translucent image too dark (white at 85% over white: 222).
        let da = 255 - sa;
        d[0] = (u32::from(q[0]) + u32::from(d[0]) * da / 255).min(255) as u8;
        d[1] = (u32::from(q[1]) + u32::from(d[1]) * da / 255).min(255) as u8;
        d[2] = (u32::from(q[2]) + u32::from(d[2]) * da / 255).min(255) as u8;
        d[3] = (sa + u32::from(d[3]) * da / 255).min(255) as u8;
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
/// path that actually matters — a `HiDPI` window compositing a logical-sized
/// canvas, i.e. a 2× bilinear UPSCALE over the whole window — that is ~19 ns
/// per destination pixel, and `AzPaint` at 1055×677 points blits 2.35 M of them
/// on every pointer move: ~45 ms per frame, which is the whole frame budget
/// three times over. It measured 12× the nearest-neighbour blit it replaced.
///
/// This keeps the quality and gets the cost back:
///
/// * the scale, tap counts and per-column tap positions are computed once,
/// * each source row is converted to straight RGBA once (`RgbaRow`) and reused by every destination
///   row that samples it — a 2× upscale touches one new source row every two destination rows,
/// * the bilinear case is separable: the horizontal partial `p00·(1−tx) + p10·tx` is kept in f32
///   per source row, so the per-destination -pixel work is one lerp of two f32 rows. Keeping the
///   partial in f32 is what makes the result bit-identical rather than merely close.
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
    // Rounded-rect the blit is confined to, as (device rect, resolved radii).
    // `None` for a square image, which skips the mask entirely.
    mask: Option<(&AzRect, [f32; 4])>,
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

    if src.width == dst_w && src.height == dst_h && mask.is_none() {
        // ---- 1:1: a frame that already has the tile's device size ---------
        // `sample` at scale 1 is the source pixel itself (the bilinear
        // partner row and column get weight 0), so each VISIBLE source row
        // is converted once and composited as it is: no lerp, and no read of
        // the row below. This is the steady state of a video tile whose
        // frames are resampled early.
        if matches!(src.format, azul_core::resources::RawImageFormat::RGBA8) {
            // The source row IS the row to composite (`source_row_to_rgba` copies an RGBA8
            // row as it is): composited from the image's own bytes, without the copy of the
            // whole row - the columns outside the window included - and without a buffer.
            // `is_sampleable` (above) holds `bytes.len() >= width * height * 4`, and the
            // window lies inside the image (`dst` is its size here).
            for py in py_lo..py_hi {
                conversions.0 += 1; // a row read, as before
                let ty_px = (dst_y + py as i32) as u32;
                let tx_px = (dst_x + px_lo as i32) as u32;
                let row = py as usize * src_w * 4;
                let Some(visible) = src.bytes.get(row + px_lo as usize * 4..row + px_hi as usize * 4) else {
                    return;
                };
                composite_rgba_row(pixmap, ((ty_px * pw + tx_px) * 4) as usize, visible);
            }
            return;
        }
        let mut rgba = RgbaRow::new(src_w);
        for py in py_lo..py_hi {
            source_row_to_rgba(src, py, &mut rgba.bytes);
            conversions.0 += 1;
            let ty_px = (dst_y + py as i32) as u32;
            let tx_px = (dst_x + px_lo as i32) as u32;
            composite_rgba_row(
                pixmap,
                ((ty_px * pw + tx_px) * 4) as usize,
                &rgba.bytes[px_lo as usize * 4..px_hi as usize * 4],
            );
        }
        return;
    }

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
            if let Some((mrect, radii)) = mask {
                mask_row_to_rounded_rect(&mut stage, tx_px as f32, ty_px as f32, mrect, radii);
            }
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
    let tap_count = nx * ny;
    let half = tap_count / 2;
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
            let idx = if let Some(i) = rows.iter().position(|r| r.y == Some(y)) {
                i
            } else {
                let victim = (0..rows.len())
                    .find(|i| claimed & (1u32 << i) == 0)
                    .unwrap_or(0);
                source_row_to_rgba(src, y, &mut rows[victim].bytes);
                rows[victim].y = Some(y);
                conversions.0 += 1;
                victim
            };
            claimed |= 1u32 << idx;
            *slot = idx;
        }
        for (i, taps) in cols.iter().enumerate() {
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for &row_idx in live.iter().take(ny as usize) {
                let row = &rows[row_idx].bytes;
                for &off in taps.iter().take(nx as usize) {
                    let base = off as usize;
                    r += u32::from(row[base]);
                    g += u32::from(row[base + 1]);
                    b += u32::from(row[base + 2]);
                    a += u32::from(row[base + 3]);
                }
            }
            let dst = &mut stage[i * 4..i * 4 + 4];
            dst[0] = ((r + half) / tap_count) as u8;
            dst[1] = ((g + half) / tap_count) as u8;
            dst[2] = ((b + half) / tap_count) as u8;
            dst[3] = ((a + half) / tap_count) as u8;
        }
        let ty_px = (dst_y + py as i32) as u32;
        let tx_px = (dst_x + px_lo as i32) as u32;
        if let Some((mrect, radii)) = mask {
            mask_row_to_rounded_rect(&mut stage, tx_px as f32, ty_px as f32, mrect, radii);
        }
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

/// The path of `rect` with its edges ROUNDED to whole device pixels - the
/// rule an opaque [`render_rect`] fills by ([`round_edge`]) and browsers
/// paint a box's background by: a box's fractional edge is not an
/// anti-aliased one. Two gradient boxes sharing a pixel each covered part of
/// it, and what lay under them showed through (a red seam between floats,
/// WPT background-gradient-subpixel-fills-area).
fn build_pixel_snapped_rect_path(rect: &AzRect) -> PathStorage {
    let x0 = f64::from(round_edge(rect.x));
    let y0 = f64::from(round_edge(rect.y));
    let x1 = f64::from(round_edge(rect.x + rect.width));
    let y1 = f64::from(round_edge(rect.y + rect.height));
    let mut path = PathStorage::new();
    path.move_to(x0, y0);
    path.line_to(x1, y0);
    path.line_to(x1, y1);
    path.line_to(x0, y1);
    path.close_polygon(PATH_FLAGS_NONE);
    path
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

/// The four corner radii of `rect` in DEVICE pixels, scaled down together if
/// any edge's pair would overlap (CSS Backgrounds 3 §5.5).
///
/// Returned as `[top_left, top_right, bottom_right, bottom_left]`.
fn resolved_corner_radii(rect: &AzRect, border_radius: &BorderRadius, dpi_factor: f32) -> [f32; 4] {
    let mut r = [
        (border_radius.top_left * dpi_factor).max(0.0),
        (border_radius.top_right * dpi_factor).max(0.0),
        (border_radius.bottom_right * dpi_factor).max(0.0),
        (border_radius.bottom_left * dpi_factor).max(0.0),
    ];
    let (w, h) = (rect.width.max(0.0), rect.height.max(0.0));
    // Each edge can only give up its own length: if the two radii meeting on it
    // sum to more, every radius shrinks by the same factor so the corners still
    // meet tangentially instead of crossing over.
    let mut f: f32 = 1.0;
    for (sum, len) in [
        (r[0] + r[1], w), // top
        (r[3] + r[2], w), // bottom
        (r[0] + r[3], h), // left
        (r[1] + r[2], h), // right
    ] {
        if sum > 0.0 && sum > len {
            f = f.min(len / sum);
        }
    }
    if f < 1.0 {
        for v in &mut r {
            *v *= f;
        }
    }
    r
}

/// How much of the pixel centred at (`x`, `y`) the rounded rectangle covers:
/// 1.0 well inside, 0.0 well outside, and a one-pixel ramp across the arc so
/// the corner reads as a curve rather than a staircase.
fn rounded_rect_coverage(x: f32, y: f32, rect: &AzRect, radii: [f32; 4]) -> f32 {
    let (x0, y0) = (rect.x, rect.y);
    let (x1, y1) = (rect.x + rect.width, rect.y + rect.height);
    let [tl, tr, br, bl] = radii;

    // Only the quarter-disc region of a corner is curved; everywhere else the
    // rectangle is straight and fully covered.
    let (cx, cy, r) = if x < x0 + tl && y < y0 + tl {
        (x0 + tl, y0 + tl, tl)
    } else if x > x1 - tr && y < y0 + tr {
        (x1 - tr, y0 + tr, tr)
    } else if x > x1 - br && y > y1 - br {
        (x1 - br, y1 - br, br)
    } else if x < x0 + bl && y > y1 - bl {
        (x0 + bl, y1 - bl, bl)
    } else {
        return 1.0;
    };
    if r <= 0.0 {
        return 1.0;
    }
    let d = (x - cx).hypot(y - cy);
    (r + 0.5 - d).clamp(0.0, 1.0)
}

/// Fade a staged RGBA row's alpha to the rounded-rect coverage, so an image
/// fills its `border-radius` instead of its bounding box.
///
/// The mask is applied to the staged row rather than inside the sampling loops
/// because both the upscale and the downscale branch funnel through the same
/// `composite_rgba_row`: one place to be correct, and the hot per-pixel maths
/// above it is untouched. Rows that clear the corner bands cost one compare.
fn mask_row_to_rounded_rect(
    stage: &mut [u8],
    row_x0: f32,
    row_y: f32,
    rect: &AzRect,
    radii: [f32; 4],
) {
    let [tl, tr, br, bl] = radii;
    let y = row_y + 0.5;
    let top = tl.max(tr);
    let bottom = bl.max(br);
    // Between the corner bands every pixel of the row is fully inside.
    if y >= rect.y + top && y <= rect.y + rect.height - bottom {
        return;
    }
    for (i, px) in stage.chunks_exact_mut(4).enumerate() {
        let cov = rounded_rect_coverage(row_x0 + i as f32 + 0.5, y, rect, radii);
        if cov < 1.0 {
            px[3] = (f32::from(px[3]) * cov) as u8;
        }
    }
}

/// The largest radius, in device px, that arcs are stepped finely for.
/// AGG steps an arc by angle, finer the larger the radius: a radius of
/// a trillion pixels takes millions of steps, and past ~1e15 the
/// step rounds to 0 and the arc never ends. Beyond this the tolerance
/// grows with the radius: three pixels for a radius of a hundred million.
const MAX_FINE_ARC_RADIUS: f64 = 4_194_304.0;

/// How much longer than its other segments the LAST segment of an AGG arc can
/// be. `Arc::vertex` steps the angle by `da` and ends with the end point as
/// soon as the next step would come within a quarter step of it
/// (`angle < end - da / 4`), so what is left for the last segment is up to
/// `da + da / 4`. A step held to [`MAX_ARC_SEGMENT_PX`] left last segments of
/// 30.9 px (a radius of 1000 px at 1x), 32.2 (1500) and 33.6 (2000): longer
/// than the margin a cut needs (caught by
/// `no_arc_segment_of_a_rounded_box_is_longer_than_the_margin`).
const ARC_LAST_SEGMENT_FACTOR: f64 = 1.25;

/// The approximation scale AGG steps the corner arcs of a rounded box by.
///
/// AGG steps an arc so that a segment is about `sqrt(radius / scale)` px long,
/// the last one up to [`ARC_LAST_SEGMENT_FACTOR`] times that.
/// The rasterizer cuts a path's geometry [`GEOMETRY_MARGIN`] beyond the visible
/// pixels, which is exact only while no segment is longer than that: a cut inside
/// a long segment leaves its pixels a level off between a fill under a clip and
/// one under none (the incremental frame against the full redraw). So the scale
/// is the output's own (`dpi`, never coarser than 1) - or, for a radius whose
/// segments that would leave longer than [`MAX_ARC_SEGMENT_PX`] (the last one
/// included), the finer one that holds every segment to it: a step of
/// `MAX_ARC_SEGMENT_PX / ARC_LAST_SEGMENT_FACTOR` (22.4 px) - bounded by
/// [`MAX_FINE_ARC_RADIUS`]. Below `22.4^2 * dpi` (502 px at 1x) nothing
/// changed.
fn arc_approximation_scale(dpi_factor: f32, largest_radius_px: f64) -> f64 {
    let largest = largest_radius_px.max(1.0);
    let step = MAX_ARC_SEGMENT_PX / ARC_LAST_SEGMENT_FACTOR;
    let for_segments = largest / (step * step);
    f64::from(dpi_factor.max(1.0))
        .max(for_segments)
        .min(MAX_FINE_ARC_RADIUS / largest)
}

fn build_rounded_rect_path(
    rect: &AzRect,
    border_radius: &BorderRadius,
    dpi_factor: f32,
) -> PathStorage {
    build_rounded_rect_path_xy(rect, border_radius, border_radius, dpi_factor)
}

/// [`build_rounded_rect_path`] with a horizontal (`border_radius`) and a vertical (`radius_y`)
/// radius per corner: quarter ellipses. A corner with either radius at zero is square.
///
/// Radii that do not fit are scaled down together by the rule of CSS Backgrounds 3 section
/// 5.5: the horizontal radii of the two corners on the top (and on the bottom) edge may sum
/// to the box's width, the vertical radii of the two on the left (and on the right) edge to
/// its height. AGG's own `RoundedRect::normalize_radius` is NOT used: it holds the horizontal
/// radii of an edge against the HEIGHT and the vertical ones against the WIDTH, which is the
/// same thing only for four equal circular corners - it shrank the quarter ellipses of a
/// 100x40 box with `border-radius: 50%` (50 by 20) to 20 by 8, shrank two 30 px corners on
/// the top edge of a 100x40 box to 20 px, and left two 30 px corners on its left edge
/// (60 px on a 40 px edge) crossing each other.
fn build_rounded_rect_path_xy(
    rect: &AzRect,
    border_radius: &BorderRadius,
    radius_y: &BorderRadius,
    dpi_factor: f32,
) -> PathStorage {
    let mut path = PathStorage::new();

    let x = f64::from(rect.x);
    let y = f64::from(rect.y);
    let w = f64::from(rect.width);
    let h = f64::from(rect.height);
    // A rect with a non-finite edge is no shape; a non-finite radius no
    // rounding.
    if ![x, y, w, h].iter().all(|v| v.is_finite()) {
        return path;
    }
    let radius = |corner: f32| {
        let px = f64::from(corner * dpi_factor);
        if px.is_finite() {
            px
        } else {
            0.0
        }
    };

    // (horizontal, vertical) per corner; a corner that has no radius on one axis has none.
    let corner = |x: f32, y: f32| {
        let (x, y) = (radius(x), radius(y));
        if x <= 0.0 || y <= 0.0 {
            (0.0, 0.0)
        } else {
            (x, y)
        }
    };
    let (mut tl, mut tl_y) = corner(border_radius.top_left, radius_y.top_left);
    let (mut tr, mut tr_y) = corner(border_radius.top_right, radius_y.top_right);
    let (mut br, mut br_y) = corner(border_radius.bottom_right, radius_y.bottom_right);
    let (mut bl, mut bl_y) = corner(border_radius.bottom_left, radius_y.bottom_left);

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
    // The radii that fit (see the doc comment): one factor for all eight.
    let mut fit = 1.0_f64;
    for (sum, edge) in [
        (tl + tr, w),     // top
        (bl + br, w),     // bottom
        (tl_y + bl_y, h), // left
        (tr_y + br_y, h), // right
    ] {
        if sum > edge.max(0.0) {
            fit = fit.min(edge.max(0.0) / sum);
        }
    }
    if fit < 1.0 {
        for r in [
            &mut tl, &mut tl_y, &mut tr, &mut tr_y, &mut br, &mut br_y, &mut bl, &mut bl_y,
        ] {
            *r *= fit;
        }
    }
    rr.radius_all(tl, tl_y, tr, tr_y, br, br_y, bl, bl_y);
    // The radius the arcs are stepped for, of the radii that are DRAWN (a pill's
    // `border-radius: 9999px` on a 40 px box is a 20 px arc and is stepped as one). AGG steps
    // a quarter ellipse by the mean of its two radii, so its longest segment is that of a
    // circle of `longer^2 / mean`; a circle's is its radius.
    let stepped_as = |x: f64, y: f64| {
        if x == y {
            x
        } else {
            let longer = x.max(y);
            longer * longer / ((x + y) * 0.5)
        }
    };
    let largest = stepped_as(tl, tl_y)
        .max(stepped_as(tr, tr_y))
        .max(stepped_as(br, br_y))
        .max(stepped_as(bl, bl_y));
    rr.set_approximation_scale(arc_approximation_scale(dpi_factor, largest));

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
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant
                                  // (or cross-type bindings that can't merge)
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
    use std::collections::{BTreeMap, HashMap};

    use azul_core::{
        dom::DomId,
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::{IdNamespace, RendererResources},
        selection::{SelectionState, TextSelection},
    };

    use crate::{
        font_traits::TextLayoutCache,
        solver3::{self, cache::LayoutCache, display_list::DisplayList},
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
        use crate::{
            solver3::getters::collect_and_resolve_font_chains_with_registration_in_viewport,
            text3::default::PathLoader,
        };

        let platform = azul_css::system::Platform::current();

        // The preview's text is laid out in `viewport`: a `vw` font size's
        // chain is collected at the size the text asks for.
        let chains = collect_and_resolve_font_chains_with_registration_in_viewport(
            styled_dom,
            &preview_font_manager.fc_cache,
            &preview_font_manager,
            &platform,
            azul_css::props::basic::PhysicalSize::new(
                viewport.size.width,
                viewport.size.height,
            ),
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
        overrides_only_hint: None,
        last_reconcile_was_skipped: false,
        last_reconcile_structure_preserved: false,
        last_build_was_patched: false,
        last_patch_damage: None,
        build_seq: 0,
        last_full_build_seq: 0,
        patch_damage_log: Vec::new(),
        last_dynamic_context: None,
        last_cascade_epoch: 0,
        paged_css_dirty: Vec::new(),
        font_epoch: 0,
        laid_out_with_overlay: false,
        previous_sizes: Vec::new(),
        dom_diff_clean: None,
        dom_pairing: None,
        last_fingerprint_skips: 0,
        last_patch_move: None,
            layout_seq: 0,
            last_patch_move_from_seq: 0,
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
        LogicalRect::new(LogicalPosition::zero(), viewport.size),
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
        Default::default(), // owner_colors (U1): no live participants here
        Vec::new(),         // seat_focus_rings (9b-ii-a-i-d-iii): headless preview paints no ring
        false,              // paint_selection_handles (U2-a): headless preview paints no handles
        None,               // preedit_text: not needed for headless preview rendering
        &azul_core::resources::ImageCache::default(),
        None, // content overlay: no live window in headless preview
        system_style.clone(),
        get_system_time_fn,
        &[],
        // A preview has no VirtualView history: every view sizes from the
        // outside on its first (and only) pass.
        &BTreeMap::new(),
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
/// * a TRANSPARENT background (alpha 0). An icon has to composite over whatever is behind it;
///   rendered on opaque white it arrives as a white tile sitting in the titlebar instead of a glyph
///   on it.
/// * the RGBA buffer, so a caller building an `ImageRef` does not encode a PNG and immediately
///   decode it again.
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
    use azul_core::styled_dom::StyledDom;

    use crate::font_traits::FontManager;

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

/// What [`render_pages`] draws: pages `first_page .. first_page + page_count` of a document
/// laid out on pages of `page_width` x `page_height` logical px, at `dpi_factor` device pixels
/// per logical pixel, on `background_color`.
#[derive(Debug, Clone, Copy)]
pub struct PageRenderOptions {
    /// The page's width in logical px (the width the document is laid out at).
    pub page_width: f32,
    /// The page's height in logical px (where the pages break).
    pub page_height: f32,
    /// Device pixels per logical pixel: a picture shown on a 2x display is drawn at 2.
    pub dpi_factor: f32,
    /// What a page shows where the document paints nothing (the paper).
    pub background_color: ColorU,
    /// The first page drawn (0-based).
    pub first_page: usize,
    /// How many pages are drawn at most; fewer past the document's end.
    pub page_count: usize,
    /// How the text is anti-aliased. A picture that is saved, scaled or printed
    /// wants [`TextAaSettings::grayscale`]; one shown pixel for pixel in a
    /// window can take the window's settings.
    pub text_aa: TextAaSettings,
}

/// Lays `styled_dom` out on pages with the break `policy` - the pages
/// `Pdf::compute_pagination_with_policy` counts - and draws a run of them with the CPU
/// renderer, one under the other in one picture: page `first_page + k` is the band
/// `k * page_height * dpi_factor` pixels down. A reader draws the pages around the one on
/// screen this way ahead of time, off the UI thread, and shows each page as a clip window over
/// the picture; one picture per run means one layout of the document per run.
///
/// # Errors
///
/// When the document cannot be laid out, a picture cannot be allocated, or the run starts past
/// the document's end.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded page pixel sizes
pub fn render_pages(
    styled_dom: &azul_core::styled_dom::StyledDom,
    font_manager: &mut FontManager<FontRef>,
    image_cache: &azul_core::resources::ImageCache,
    policy: crate::solver3::page_breaks::BreakPolicy,
    opts: PageRenderOptions,
) -> Result<AzulPixmap, String> {
    use std::collections::BTreeMap;

    use azul_core::{
        dom::DomId,
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::{IdNamespace, RendererResources},
    };

    use crate::{
        font_traits::TextLayoutCache,
        paged::FragmentationContext,
        solver3::{paged_layout::layout_document_paged_with_config, pagination::FakePageConfig},
        text3::default::PathLoader,
    };

    let content_size = LogicalSize::new(opts.page_width, opts.page_height);
    let mut layout_cache = crate::Solver3LayoutCache::default();
    let mut text_cache = TextLayoutCache::new();
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = None;
    let loader = PathLoader::new();
    let font_loader = |bytes, index| loader.load_font_shared(bytes, index);
    let page_config = FakePageConfig {
        break_policy: policy,
        ..FakePageConfig::new()
    };
    let pages = layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        FragmentationContext::new_paged(content_size),
        styled_dom,
        viewport,
        font_manager,
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        page_config,
        image_cache,
        azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .map_err(|e| format!("the pages cannot be laid out: {e:?}"))?;

    let drawn: Vec<&DisplayList> = pages
        .iter()
        .skip(opts.first_page)
        .take(opts.page_count)
        .collect();
    if drawn.is_empty() {
        return Err(format!(
            "page {} is past the end ({} pages)",
            opts.first_page,
            pages.len()
        ));
    }
    let page_w = ((opts.page_width * opts.dpi_factor).round() as u32).max(1);
    let page_h = ((opts.page_height * opts.dpi_factor).round() as u32).max(1);
    let total_h = page_h
        .checked_mul(drawn.len() as u32)
        .ok_or_else(|| "the run of pages is too tall".to_string())?;
    let mut picture = AzulPixmap::new(page_w, total_h)
        .ok_or_else(|| format!("cannot create a picture of {page_w} x {total_h}"))?;
    let mut glyph_cache = GlyphCache::new();
    glyph_cache.set_text_aa(opts.text_aa);
    let state = CpuRenderState::new(ScrollOffsetMap::new());
    let bg = opts.background_color;
    for (k, display_list) in drawn.into_iter().enumerate() {
        let mut page = AzulPixmap::new(page_w, page_h)
            .ok_or_else(|| format!("cannot create a page of {page_w} x {page_h}"))?;
        page.fill(bg.r, bg.g, bg.b, bg.a);
        render_display_list_with_state(
            display_list,
            &mut page,
            opts.dpi_factor,
            &renderer_resources,
            font_manager,
            &mut glyph_cache,
            &state,
        )?;
        write_region(&mut picture, page.data(), page_w, page_h, 0, (k as u32 * page_h) as i32);
    }
    Ok(picture)
}

/// Where `text` breaks into lines no wider than `max_width`, given the advance of
/// each character: the byte range of every line. Lines break at spaces, which the
/// break swallows, and at newlines; a word longer than a whole line breaks between its
/// characters. Text that is not a tooltip's is laid out by `text3`; a tooltip's text is
/// drawn by the shells that have no text drawing of their own (Wayland, X11), and this
/// is the one place its lines are broken.
#[must_use]
pub fn wrap_text_lines(
    text: &str,
    max_width: f32,
    advance: impl Fn(char) -> f32,
) -> Vec<core::ops::Range<usize>> {
    let mut lines = Vec::new();
    let mut offset = 0;
    for paragraph in text.split('\n') {
        let line = paragraph.strip_suffix('\r').unwrap_or(paragraph);
        wrap_paragraph(line, offset, max_width, &advance, &mut lines);
        offset += paragraph.len() + 1;
    }
    lines
}

/// One newline-free paragraph of [`wrap_text_lines`], starting at byte `offset`.
fn wrap_paragraph(
    paragraph: &str,
    offset: usize,
    max_width: f32,
    advance: &impl Fn(char) -> f32,
    lines: &mut Vec<core::ops::Range<usize>>,
) {
    let width_of = |range: core::ops::Range<usize>| -> f32 {
        paragraph[range].chars().map(advance).sum()
    };
    let trimmed_end =
        |start: usize, end: usize| start + paragraph[start..end].trim_end_matches(' ').len();
    let mut start = 0;
    let mut width = 0.0;
    let mut last_space = None;
    for (i, c) in paragraph.char_indices() {
        let w = advance(c);
        if c == ' ' {
            last_space = Some(i);
            width += w;
            continue;
        }
        if width + w > max_width && i > start {
            match last_space {
                // A space with text before it on this line: break there.
                Some(space) if !paragraph[start..space].trim_end_matches(' ').is_empty() => {
                    lines.push(offset + start..offset + trimmed_end(start, space));
                    start = space + 1;
                    while start < i && paragraph[start..].starts_with(' ') {
                        start += 1;
                    }
                }
                // One word fills the line: break it before this character.
                _ => {
                    lines.push(offset + start..offset + trimmed_end(start, i));
                    start = i;
                }
            }
            last_space = None;
            width = width_of(start..i);
        }
        width += w;
    }
    lines.push(offset + start..offset + trimmed_end(start, paragraph.len()));
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
pub fn render_text_run_to_pixmap(
    fc_cache: &rust_fontconfig::FcFontCache,
    text: &str,
    font_size_px: f32,
    text_color: ColorU,
    bg_color: ColorU,
    padding_px: f32,
    dpi_factor: f32,
) -> Option<AzulPixmap> {
    render_wrapped_text_to_pixmap(
        fc_cache,
        text,
        font_size_px,
        text_color,
        bg_color,
        padding_px,
        dpi_factor,
        LogicalSize::new(f32::INFINITY, f32::INFINITY),
    )
}

/// [`render_text_run_to_pixmap`] for text that wraps - a tooltip's: its lines
/// break where they would run wider than `max_text_size.width`
/// ([`wrap_text_lines`]), and lines that would make the text taller than
/// `max_text_size.height` are left out. Both limits are the TEXT's, padding not
/// counted. The pixmap is as wide as the widest line and as tall as the lines,
/// plus the padding on every side - the size the tooltip's surface is shown at.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
#[must_use]
// bounded pixel-dimension casts; explicit a*b+c kept (see render_box_shadow)
#[allow(
    clippy::suboptimal_flops,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn render_wrapped_text_to_pixmap(
    fc_cache: &rust_fontconfig::FcFontCache,
    text: &str,
    font_size_px: f32,
    text_color: ColorU,
    bg_color: ColorU,
    padding_px: f32,
    dpi_factor: f32,
    max_text_size: LogicalSize,
) -> Option<AzulPixmap> {
    use azul_core::resources::{FontKey, IdNamespace};
    use rust_fontconfig::{FcPattern, OwnedFontSource};

    // 1. Resolve a default (sans-serif) system font, falling back to any font.
    //    `query_with_fallback` IS that ladder — exact, then family-relaxed, then coverage-only — so
    //    it replaces the hand-rolled `or_else` chain and keeps the relaxation rules in one place,
    //    where fontconfig's own live.
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

    // 2. Register the font in a throwaway FontManager. This helper builds its own one-item display
    //    list, so it also has to supply the font state that list is written against — through the
    //    SAME manager every other renderer consults, never a parallel RendererResources map.
    let rr = RendererResources::default();
    let font_ref = crate::parsed_font_to_font_ref(parsed.clone());
    let hash = crate::font_ref_to_parsed_font(&font_ref).hash;
    let fm: FontManager<FontRef> =
        FontManager::new(rust_fontconfig::FcFontCache::default()).ok()?;
    fm.insert_font(rust_fontconfig::FontId::new(), font_ref);
    let font_hash = FontHash::from_hash(hash);

    // 3. Break the text into lines and shape each (simple per-char advances; tooltips are short
    //    and unstyled, so the full bidi/complex shaper isn't reachable here — same simplification
    //    as the pagination header path).
    let ascent = parsed.font_metrics.ascent * scale;
    let descent = parsed.font_metrics.descent * scale; // typically negative
    let line_height = ascent - descent;
    let advance_of = |c: char| {
        let gid = parsed.lookup_glyph_index(c as u32).unwrap_or(0);
        f32::from(parsed.get_horizontal_advance(gid)) * scale
    };
    let mut lines = wrap_text_lines(text, max_text_size.width, advance_of);
    // Lines past the height limit are left out; the first always stays.
    if line_height > 0.0 && max_text_size.height.is_finite() {
        let fitting = (max_text_size.height / line_height).floor().max(1.0) as usize;
        lines.truncate(fitting);
    }
    let mut glyphs = Vec::new();
    let mut text_width: f32 = 0.0;
    for (row, line) in lines.iter().enumerate() {
        let baseline_y = padding_px + ascent + row as f32 * line_height;
        let mut pen_x = padding_px;
        for c in text[line.clone()].chars() {
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
        text_width = text_width.max(pen_x - padding_px);
    }

    // 4. Size the pixmap to the shaped lines (logical units; device pixels via dpi).
    let logical_w = (text_width + padding_px * 2.0).max(1.0);
    let logical_h = (line_height * lines.len().max(1) as f32 + padding_px * 2.0).max(1.0);
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
        line_box: clip_rect,
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

#[cfg(test)]
mod wrap_tests {
    use super::wrap_text_lines;

    /// Every character ten wide, spaces too.
    fn ten(_: char) -> f32 {
        10.0
    }

    fn lines_of(text: &str, max_width: f32) -> Vec<&str> {
        wrap_text_lines(text, max_width, ten)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    /// A tooltip longer than its maximum width wraps at spaces into lines no
    /// wider than it, instead of running on in one line that is cut off.
    #[test]
    fn a_long_tooltip_wraps_at_its_maximum_width() {
        let lines = lines_of("Select a file to move it", 100.0);
        assert_eq!(lines, vec!["Select a", "file to", "move it"]);
        assert!(
            lines.iter().all(|line| line.chars().count() as f32 * 10.0 <= 100.0),
            "no line is wider than the maximum: {lines:?}"
        );
    }

    #[test]
    fn a_word_longer_than_a_line_breaks_between_its_characters() {
        assert_eq!(lines_of("abcdefghijkl", 50.0), vec!["abcde", "fghij", "kl"]);
        assert_eq!(lines_of("go abcdefghijkl", 50.0), vec!["go", "abcde", "fghij", "kl"]);
    }

    #[test]
    fn newlines_break_lines_and_short_text_stays_on_one() {
        assert_eq!(lines_of("Open\r\nSave", 1000.0), vec!["Open", "Save"]);
        assert_eq!(lines_of("Save", 1000.0), vec!["Save"]);
        assert_eq!(lines_of("", 1000.0), vec![""], "empty text is one empty line");
        assert_eq!(
            lines_of("no limit at all", f32::INFINITY),
            vec!["no limit at all"]
        );
    }
}

// ============================================================================
// Direct SVG-to-image renderer (bypasses CSS layout)
// ============================================================================

#[cfg(all(test, feature = "std", feature = "text_layout", feature = "font_loading"))]
mod rounded_clip_tests {
    use rust_fontconfig::FcFontCache;

    use super::*;

    fn px(p: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.width + x) * 4) as usize;
        [p.data[i], p.data[i + 1], p.data[i + 2], p.data[i + 3]]
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition { x, y },
            size: LogicalSize { width: w, height: h },
        }
    }

    /// The map bug, minimised: a red fill inside a rounded `overflow: hidden`
    /// clip must NOT reach the corners, and must still fill the middle and the
    /// straight edges.
    #[test]
    fn content_inside_a_rounded_clip_is_cut_to_the_radius() {
        let (w, h) = (100u32, 100u32);
        let radius = 20.0;
        let red = ColorU { r: 255, g: 0, b: 0, a: 255 };
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushClip {
                    bounds: rect(10.0, 10.0, 80.0, 80.0).into(),
                    border_radius: BorderRadius {
                        top_left: radius,
                        top_right: radius,
                        bottom_left: radius,
                        bottom_right: radius,
                    },
                },
                // Paints the WHOLE canvas; the clip alone decides what shows.
                DisplayListItem::Rect {
                    bounds: rect(0.0, 0.0, 100.0, 100.0).into(),
                    color: red,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopClip,
            ],
            ..Default::default()
        };
        let rr = RendererResources::default();
        let fm = FontManager::<FontRef>::new(FcFontCache::default()).expect("font manager");
        let mut gc = GlyphCache::new();
        let mut pm = AzulPixmap::new(w, h).unwrap();
        pm.fill(255, 255, 255, 255);
        render_display_list(&dl, &mut pm, 1.0, &rr, &fm, &mut gc).unwrap();

        let white = [255, 255, 255, 255];
        // Outside the clip rectangle entirely: untouched (the rect clip's job).
        assert_eq!(px(&pm, 2, 2), white, "outside the clip rect");
        // Centre and the middle of each straight edge: painted.
        assert_eq!(px(&pm, 50, 50)[0..3], [255, 0, 0], "centre");
        assert_eq!(px(&pm, 50, 11)[0..3], [255, 0, 0], "top edge middle");
        assert_eq!(px(&pm, 11, 50)[0..3], [255, 0, 0], "left edge middle");
        // The very corner pixel of the clip rect lies beyond the arc: it must
        // show what was there before (white), not the red content. This is the
        // assertion that failed before the fix.
        assert_eq!(px(&pm, 10, 10), white, "top-left corner must be cut");
        assert_eq!(px(&pm, 89, 10), white, "top-right corner must be cut");
        assert_eq!(px(&pm, 10, 89), white, "bottom-left corner must be cut");
        assert_eq!(px(&pm, 89, 89), white, "bottom-right corner must be cut");
    }

    /// A square clip (no radius) keeps its corners — the fix must not round
    /// clips that were never rounded.
    #[test]
    fn a_square_clip_keeps_its_corners() {
        let red = ColorU { r: 255, g: 0, b: 0, a: 255 };
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushClip {
                    bounds: rect(10.0, 10.0, 80.0, 80.0).into(),
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::Rect {
                    bounds: rect(0.0, 0.0, 100.0, 100.0).into(),
                    color: red,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopClip,
            ],
            ..Default::default()
        };
        let rr = RendererResources::default();
        let fm = FontManager::<FontRef>::new(FcFontCache::default()).expect("font manager");
        let mut gc = GlyphCache::new();
        let mut pm = AzulPixmap::new(100, 100).unwrap();
        pm.fill(255, 255, 255, 255);
        render_display_list(&dl, &mut pm, 1.0, &rr, &fm, &mut gc).unwrap();
        assert_eq!(px(&pm, 10, 10)[0..3], [255, 0, 0], "square corner stays painted");
    }
}

#[cfg(all(test, feature = "std"))]
mod text_shadow_tests {
    use azul_core::resources::{FontKey, IdNamespace};
    use azul_css::props::{
        basic::pixel::{PixelValue, PixelValueNoPercent},
        style::box_shadow::StyleBoxShadow,
    };

    use super::*;
    use crate::{
        font::parsed::ParsedFont,
        solver3::display_list::{DisplayList, WindowLogicalRect},
    };

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
        (rr, fm, FontHash::from_hash(hash))
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
            line_box: clip_rect,
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
            "text-shadow must paint red shadow pixels beyond the baseline (plain {red_plain}, \
             shadow {red_shadow})"
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

    /// `text-shadow: 0 0 0 3px red` - no offset, no blur, a spread - is a
    /// solid halo: the letters grown by 3px all round, in full red, and
    /// nothing farther out. Measured on the shadow alone (transparent text):
    /// against the same shadow without a spread, which is the letters' shape.
    #[test]
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)] // small test pixmaps
    fn text_shadow_spread_is_a_solid_halo() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, fm, font_hash) = renderer_resources_with(&font);
        let (w, h) = (200u32, 60u32);
        let font_size = 32.0;
        let glyphs = shape(&font, "Hi", font_size, 20.0, 42.0);
        #[allow(clippy::cast_precision_loss)]
        let clip_rect: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: w as f32,
                height: h as f32,
            },
        }
        .into();
        let px = |v: f32| PixelValueNoPercent { inner: PixelValue::px(v) };
        // The red shadow's coverage (0..1) at every pixel, the text itself
        // transparent.
        let coverage = |spread: f32| -> Vec<f32> {
            let shadow = StyleBoxShadow {
                offset_x: px(0.0),
                offset_y: px(0.0),
                blur_radius: px(0.0),
                spread_radius: px(spread),
                color: ColorU { r: 255, g: 0, b: 0, a: 255 },
                clip_mode: azul_css::props::style::box_shadow::BoxShadowClipMode::Outset,
            };
            let text_item = DisplayListItem::Text {
                glyphs: glyphs.clone(),
                font_hash,
                font_size_px: font_size,
                color: ColorU { r: 0, g: 0, b: 0, a: 0 },
                clip_rect,
                line_box: clip_rect,
                source_node_index: None,
            };
            let mut p = AzulPixmap::new(w, h).unwrap();
            p.fill(255, 255, 255, 255);
            let dl = DisplayList {
                items: vec![
                    DisplayListItem::PushTextShadow { shadow },
                    text_item,
                    DisplayListItem::PopTextShadow,
                ],
                ..Default::default()
            };
            render_display_list(&dl, &mut p, 1.0, &rr, &fm, &mut GlyphCache::new()).unwrap();
            p.data()
                .chunks_exact(4)
                .map(|c| 1.0 - f32::from(c[1]) / 255.0)
                .collect()
        };
        let letters = coverage(0.0);
        let halo = coverage(3.0);
        let at = |img: &[f32], x: i32, y: i32| {
            if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                0.0
            } else {
                img[(y as u32 * w + x as u32) as usize]
            }
        };
        // Whether `letters` reaches `level` within distance `d` of (x, y).
        let near = |x: i32, y: i32, d: f32, level: f32| {
            let r = d.ceil() as i32;
            (-r..=r).any(|dy| {
                (-r..=r).any(|dx| {
                    ((dx * dx + dy * dy) as f32).sqrt() <= d && at(&letters, x + dx, y + dy) >= level
                })
            })
        };
        let (mut inside, mut solid, mut stray) = (0, 0, 0);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let c = at(&halo, x, y);
                if near(x, y, 2.0, 0.95) {
                    // Within 2px of a fully covered pixel of a letter: the
                    // 3px halo covers it fully.
                    inside += 1;
                    if c >= 0.97 {
                        solid += 1;
                    }
                } else if !near(x, y, 4.5, 0.01) && c > 0.01 {
                    // Farther than the spread and its rim from any ink.
                    stray += 1;
                }
            }
        }
        assert!(inside > 300, "premise: the letters cover something ({inside} px)");
        assert_eq!(solid, inside, "the halo is solid up to 2px out from the letters");
        assert_eq!(stray, 0, "nothing is painted beyond the spread");
        let grown: f32 = halo.iter().sum::<f32>() - letters.iter().sum::<f32>();
        assert!(grown > 200.0, "the halo is larger than the letters ({grown} px)");
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
                line_box: clip_rect,
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

    fn device_rect(x: f32, y: f32, width: f32, height: f32) -> AzRect {
        AzRect::from_xywh(x, y, width, height).expect("a rect with an area")
    }

    /// A map shows a hundred labels with a halo each: drawing and blurring a
    /// whole window per label made one frame of a pan cost most of a second.
    #[test]
    fn a_text_shadow_draws_and_blurs_only_the_box_its_glyphs_reach_not_the_whole_window() {
        let boxes = text_shadow_boxes(device_rect(500.0, 300.0, 80.0, 20.0), 2, None, (2200, 1440))
            .expect("a shadow in the window");
        let reach = PixelBox {
            x: 498,
            y: 298,
            width: 84,
            height: 24,
        };
        assert_eq!(boxes.buffer, reach);
        assert_eq!(boxes.out, reach);
    }

    #[test]
    fn a_text_shadow_lands_only_inside_its_clip_but_blurs_what_lies_just_outside_it() {
        let clip = device_rect(0.0, 0.0, 540.0, 1440.0);
        let boxes =
            text_shadow_boxes(device_rect(500.0, 300.0, 80.0, 20.0), 2, Some(clip), (2200, 1440))
                .expect("a shadow in the clip");
        assert_eq!(
            boxes.out,
            PixelBox {
                x: 498,
                y: 298,
                width: 42,
                height: 24,
            }
        );
        // The glyphs within the blur's reach of the clip's edge still blur into it.
        assert_eq!(
            boxes.buffer,
            PixelBox {
                x: 498,
                y: 298,
                width: 44,
                height: 24,
            }
        );
        let far = device_rect(1000.0, 0.0, 100.0, 100.0);
        assert_eq!(text_shadow_boxes(device_rect(500.0, 300.0, 80.0, 20.0), 2, Some(far), (2200, 1440)), None);
    }

    /// Two perpendicular strips that meet in a corner stay two strips.
    ///
    /// A grow resize damages an L: the newly-exposed right column over the
    /// full new height, and the newly-exposed bottom row over the old width.
    /// Anything the diff adds at the seam - a `width: 100%` bar's exposed
    /// piece - makes the column reach down into the row, so the two OVERLAP
    /// in the corner. Merging an overlap into its bounding box is right when
    /// the box is about as big as the pair; here it is the WHOLE WINDOW, and
    /// a resize that exposed 58% of it repaints and reports 100%.
    ///
    /// `coalesce_damage_rects` already refuses this merge, with that exact
    /// reasoning. The rasteriser then merged the pair anyway, because its own
    /// merge only asks whether they overlap - so the decision the coalescer
    /// made was undone one step later.
    #[test]
    fn two_strips_meeting_in_a_corner_are_not_merged_into_the_whole_surface() {
        let rr = RendererResources::default();
        let fm = empty_font_manager();
        let mut gc = GlyphCache::new();
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let dl = DisplayList::default();

        let (w, h) = (800u32, 900u32);
        let mut pm = AzulPixmap::new(w, h).unwrap();
        pm.fill(0, 200, 0, 255);

        let rect = |x: f32, y: f32, width: f32, height: f32| LogicalRect {
            origin: LogicalPosition { x, y },
            size: LogicalSize { width, height },
        };
        // The exposed right column, reaching up to y=0 because the bar at the
        // top grew with the window, and the exposed bottom row.
        let l = vec![rect(492.0, 0.0, 308.0, 900.0), rect(0.0, 600.0, 500.0, 300.0)];
        let painted =
            render_display_list_damaged(&dl, &mut pm, 1.0, &rr, &fm, &mut gc, &state, &l).unwrap();

        let area: f32 = painted.iter().map(|r| r.size.width * r.size.height).sum();
        let whole = (w * h) as f32;
        assert!(
            area <= whole * 0.75,
            "an L must not be repainted as the whole surface: {area} of {whole} in {painted:?}"
        );
    }

    /// A repaint of a damaged strip clears and paints only that strip. A
    /// shadow composited past the strip lands again on pixels that already
    /// show it, and its halo darkens a little more with every repaint.
    #[test]
    fn a_text_shadow_leaves_the_pixels_outside_the_damaged_strip_alone() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, fm, font_hash) = renderer_resources_with(&font);
        let (w, h) = (200u32, 80u32);
        let glyphs = shape(&font, "Hi", 32.0, 40.0, 50.0);
        #[allow(clippy::cast_precision_loss)]
        let clip_rect: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: w as f32,
                height: h as f32,
            },
        }
        .into();
        let shadow = StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::px(0.0),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::px(0.0),
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::px(6.0),
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
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushTextShadow { shadow },
                DisplayListItem::Text {
                    glyphs,
                    font_hash,
                    font_size_px: 32.0,
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0,
                    },
                    clip_rect,
                    line_box: clip_rect,
                    source_node_index: None,
                },
                DisplayListItem::PopTextShadow,
            ],
            ..Default::default()
        };
        let mut pm = AzulPixmap::new(w, h).unwrap();
        pm.fill(0, 200, 0, 255);
        let strip = vec![LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 60.0,
                height: 80.0,
            },
        }];
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut gc = GlyphCache::new();
        render_display_list_damaged(&dl, &mut pm, 1.0, &rr, &fm, &mut gc, &state, &strip).unwrap();
        let wide = w as usize;
        let green = [0u8, 200, 0, 255];
        let mut touched = Vec::new();
        let mut shadowed = 0usize;
        for (i, p) in pm.data().chunks_exact(4).enumerate() {
            let x = i % wide;
            if x >= 60 && p != green {
                touched.push((x, i / wide));
            } else if (40..60).contains(&x) && p[0] > 0 {
                shadowed += 1;
            }
        }
        assert!(
            touched.is_empty(),
            "the shadow painted {} pixels right of the damaged strip, the first at {:?}",
            touched.len(),
            touched.first()
        );
        assert!(shadowed > 0, "the shadow paints inside the strip");
    }

    /// A2-cpu-18 (`audit/A2-cpu.md`). A damage rect that meets a text's SHADOW and not its box
    /// repaints the shadow.
    ///
    /// Hand-computed: "Hi" at 32 px, baseline y = 30, line box rows 0 .. 34; a shadow 40 px
    /// below it (`text-shadow: 0 40px red`, no blur), so its ink is in rows of about 47 .. 70.
    /// A damage rect of rows 50 .. 70 does not meet the line box: the damaged walk culled the
    /// run by its box alone (the cull inside groups and views, `outside_active_clip`, widens
    /// the box by the shadow's reach, 40 px), cleared the rect and painted nothing into it.
    /// Expected: the rows 50 .. 70 of the shadow are there again.
    ///
    /// mutation: dropping the `reach` widening of `test_bounds` in `walk_items` fails here.
    #[test]
    fn a2_cpu_18_a_damage_rect_on_a_text_shadow_alone_repaints_the_shadow() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, fm, font_hash) = renderer_resources_with(&font);
        let (w, h) = (200u32, 120u32);
        let glyphs = shape(&font, "Hi", 32.0, 40.0, 30.0);
        #[allow(clippy::cast_precision_loss)]
        let clip_rect: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: w as f32,
                height: h as f32,
            },
        }
        .into();
        let line_box: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 200.0,
                height: 34.0,
            },
        }
        .into();
        let shadow = StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::px(0.0),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::px(40.0),
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
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushTextShadow { shadow },
                DisplayListItem::Text {
                    glyphs,
                    font_hash,
                    font_size_px: 32.0,
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0,
                    },
                    clip_rect,
                    line_box,
                    source_node_index: None,
                },
                DisplayListItem::PopTextShadow,
            ],
            ..Default::default()
        };
        let mut pm = AzulPixmap::new(w, h).unwrap();
        pm.fill(0, 200, 0, 255);
        // Rows 50 .. 70, the whole width: below the line box (rows 0 .. 34), on the shadow.
        let strip = vec![LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 50.0 },
            size: LogicalSize {
                width: 200.0,
                height: 20.0,
            },
        }];
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut gc = GlyphCache::new();
        render_display_list_damaged(&dl, &mut pm, 1.0, &rr, &fm, &mut gc, &state, &strip).unwrap();
        let wide = w as usize;
        let mut shadowed = 0usize;
        for (i, p) in pm.data().chunks_exact(4).enumerate() {
            let y = i / wide;
            if (50..70).contains(&y) && p[0] > 150 && p[1] < 100 && p[2] < 100 {
                shadowed += 1;
            }
        }
        assert!(
            shadowed > 0,
            "the damage rect was cleared and the shadow in it not repainted: no red pixel in \
             rows 50 .. 70"
        );
    }

    /// A2-cpu-05 (`audit/A2-cpu.md`). One label of a view changes the colour of its halo: the
    /// damage is that label and no other, and the frame the damage repaints is the frame a
    /// full paint gives.
    ///
    /// Three labels on the baselines 30, 90 and 150 of the view's content, each in its own box
    /// `(16, y - 28) 150 x 40` under a halo of 2 px spread; the view at (10, 10) with its
    /// content at (-6, 4). The middle label's rows on screen are `62 - 5 + 14 .. 102 + 5 + 14`
    /// = 71 .. 121 (its clip cut ink, the reach 2, the slack 3, the view's offset); the first
    /// label's box ends at row 56, the third one's starts at row 136.
    ///
    /// mutation: the `Text` arm of the shadow match in `compute_display_list_damage_impl`
    /// removed: the view's whole box is damaged (the rows assertion fails); `grow` without
    /// the reach: the halo's rim keeps its old colour (the pixel comparison fails).
    #[test]
    fn a2_cpu_05_one_labels_halo_changes_colour_in_a_view_and_only_that_label_is_repainted() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, fm, font_hash) = renderer_resources_with(&font);
        let child = azul_core::dom::DomId { inner: 1 };
        let (w, h) = (300u32, 220u32);
        let rect_of = |x: f32, y: f32, width: f32, height: f32| -> WindowLogicalRect {
            LogicalRect {
                origin: LogicalPosition { x, y },
                size: LogicalSize { width, height },
            }
            .into()
        };
        let length = |v: f32| PixelValueNoPercent { inner: PixelValue::px(v) };
        let white = ColorU { r: 255, g: 255, b: 255, a: 255 };
        let yellow = ColorU { r: 255, g: 230, b: 0, a: 255 };
        let labels = |middle: ColorU| {
            let mut items = Vec::new();
            for (i, halo) in [white, middle, white].into_iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let y = 30.0 + 60.0 * i as f32;
                let own_box = rect_of(16.0, y - 28.0, 150.0, 40.0);
                items.push(DisplayListItem::PushTextShadow {
                    shadow: StyleBoxShadow {
                        offset_x: length(0.0),
                        offset_y: length(0.0),
                        blur_radius: length(0.0),
                        spread_radius: length(2.0),
                        color: halo,
                        clip_mode: azul_css::props::style::box_shadow::BoxShadowClipMode::Outset,
                    },
                });
                items.push(DisplayListItem::Text {
                    glyphs: shape(&font, "Label", 24.0, 20.0, y),
                    font_hash,
                    font_size_px: 24.0,
                    color: ColorU { r: 0, g: 0, b: 0, a: 255 },
                    clip_rect: own_box,
                    line_box: own_box,
                    source_node_index: None,
                });
                items.push(DisplayListItem::PopTextShadow);
            }
            std::sync::Arc::new(DisplayList {
                items,
                ..Default::default()
            })
        };
        let parent = DisplayList {
            items: vec![
                DisplayListItem::Rect {
                    bounds: rect_of(0.0, 0.0, 300.0, 220.0),
                    color: ColorU { r: 232, g: 232, b: 232, a: 255 },
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::VirtualView {
                    child_dom_id: child,
                    bounds: rect_of(10.0, 10.0, 260.0, 200.0),
                    clip_rect: rect_of(10.0, 10.0, 260.0, 200.0),
                    content_offset: LogicalPosition { x: -6.0, y: 4.0 },
                },
            ],
            ..Default::default()
        };
        let views_of = |list: &std::sync::Arc<DisplayList>| {
            let mut views = std::collections::BTreeMap::new();
            views.insert(child, std::sync::Arc::clone(list));
            views
        };
        let whole = vec![LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 300.0,
                height: 220.0,
            },
        }];
        let paint = |pm: &mut AzulPixmap, list: &std::sync::Arc<DisplayList>, damage: &[LogicalRect]| {
            let state = CpuRenderState::new(ScrollOffsetMap::new())
                .with_virtual_view_display_lists(views_of(list));
            let mut gc = GlyphCache::new();
            render_display_list_damaged(&parent, pm, 1.0, &rr, &fm, &mut gc, &state, damage)
                .unwrap();
        };
        let (before, now) = (labels(white), labels(yellow));

        // The frame before, then the damage of the change repainted into it.
        let mut frame = AzulPixmap::new(w, h).unwrap();
        frame.fill(255, 255, 255, 255);
        paint(&mut frame, &before, &whole);
        let frame_before = frame.data().to_vec();
        let damage = crate::cpurender::compute_virtual_view_damage(
            &parent,
            &views_of(&now),
            &views_of(&before),
            &ScrollOffsetMap::new(),
        );
        assert!(!damage.is_empty(), "a halo that changed colour is damage");
        assert!(
            damage
                .iter()
                .all(|r| r.origin.y >= 56.0 && r.origin.y + r.size.height <= 136.0),
            "the damage is the middle label's (rows 71 .. 121), not the first label's (to row \
             56), the third one's (from row 136) or the view's box: {damage:?}"
        );
        paint(&mut frame, &now, &damage);

        // A full paint of the new lists.
        let mut full = AzulPixmap::new(w, h).unwrap();
        full.fill(255, 255, 255, 255);
        paint(&mut full, &now, &whole);
        assert!(
            full.data() != frame_before.as_slice(),
            "harness: the halo's new colour shows"
        );
        let differing = frame
            .data()
            .chunks_exact(4)
            .zip(full.data().chunks_exact(4))
            .filter(|(p, q)| p[..3] != q[..3])
            .count();
        assert_eq!(
            differing, 0,
            "the frame the damage repainted differs from a full paint in {differing} pixels \
             (damage {damage:?})"
        );
    }

    /// A label under a `text-shadow` of `blur` and `spread` px in `color`, its first pen at
    /// `(x, y)`, its clip rect its own line box (it moves with the text, as a label's does): the
    /// frame, and how many halos were drawn for it.
    #[allow(clippy::too_many_arguments)]
    fn shadowed_label(
        font: &ParsedFont,
        resources: &(RendererResources, FontManager<FontRef>, FontHash),
        text: &str,
        (x, y): (f32, f32),
        (blur, spread): (f32, f32),
        color: ColorU,
        glyph_cache: &mut GlyphCache,
    ) -> (AzulPixmap, u64) {
        let (rr, fm, font_hash) = resources;
        let (w, h) = (260u32, 90u32);
        let glyphs = shape(font, text, 24.0, x, y);
        let clip_rect: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: x - 4.0, y: y - 28.0 },
            size: LogicalSize { width: 150.0, height: 40.0 },
        }
        .into();
        let length = |v: f32| PixelValueNoPercent { inner: PixelValue::px(v) };
        let shadow = StyleBoxShadow {
            offset_x: length(0.0),
            offset_y: length(0.0),
            blur_radius: length(blur),
            spread_radius: length(spread),
            color,
            clip_mode: azul_css::props::style::box_shadow::BoxShadowClipMode::Outset,
        };
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushTextShadow { shadow },
                DisplayListItem::Text {
                    glyphs,
                    font_hash: *font_hash,
                    font_size_px: 24.0,
                    color: ColorU { r: 0, g: 0, b: 0, a: 255 },
                    clip_rect,
                    line_box: clip_rect,
                    source_node_index: None,
                },
                DisplayListItem::PopTextShadow,
            ],
            ..Default::default()
        };
        let mut pm = AzulPixmap::new(w, h).unwrap();
        pm.fill(255, 255, 255, 255);
        crate::cpu_counters::reset();
        render_display_list(&dl, &mut pm, 1.0, rr, fm, glyph_cache).unwrap();
        (pm, crate::cpu_counters::take().halos_drawn)
    }

    /// The most a channel of `a` differs from `b`, and how many channels differ at all.
    fn channel_difference(a: &AzulPixmap, b: &AzulPixmap) -> (i32, usize) {
        a.data().iter().zip(b.data()).fold((0, 0), |(most, n), (p, q)| {
            let d = (i32::from(*p) - i32::from(*q)).abs();
            (most.max(d), n + usize::from(d > 0))
        })
    }

    /// A label that moves by whole pixels, or is shadowed in another colour, finds its halo
    /// kept: drawn once, then tinted and composited, and the frame is the one the halo drawn
    /// afresh gives. A label that moves by a fraction of a pixel falls in other quarter pixels of
    /// the glyph cache and draws its own.
    #[test]
    fn a_text_shadow_is_drawn_once_wherever_the_same_text_stands() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let resources = renderer_resources_with(&font);
        let red = ColorU { r: 220, g: 20, b: 20, a: 255 };
        let faint = ColorU { r: 20, g: 60, b: 200, a: 160 };
        // A blur, the editor's solid spread, and both.
        for halo in [(4.0, 0.0), (0.0, 2.0), (2.0, 1.5)] {
            let mut kept = GlyphCache::new();
            let mut fresh = GlyphCache::new();
            let tag = format!("halo {halo:?}");
            // (position, colour, halos it draws)
            let frames = [
                ((20.0, 50.0), red, 1),
                ((61.0, 53.0), red, 0),
                ((20.0, 50.0), faint, 0),
                ((33.0, 44.0), faint, 0),
                ((20.5, 50.0), red, 1),
                ((20.5, 50.0), red, 0),
            ];
            for (at, color, drawn) in frames {
                let (frame, n) = shadowed_label(&font, &resources, "Tuebingen", at, halo, color, &mut kept);
                assert_eq!(n, drawn, "{tag}: halos drawn for {at:?} {color:?}");
                super::super::text_halo::keep_cpu_halos(false);
                let (old, _) =
                    shadowed_label(&font, &resources, "Tuebingen", at, halo, color, &mut fresh);
                super::super::text_halo::keep_cpu_halos(true);
                let (most, n) = channel_difference(&frame, &old);
                assert!(
                    most <= 2,
                    "{tag}: the kept halo at {at:?} {color:?} is the drawn one within 2/255 \
                     (differs by {most} in {n} channels)"
                );
            }
        }
    }

    /// The key is the glyph cache's own quarter pixels: one glyph that moves within its quarter
    /// keeps its halo (and the frame the halo drawn afresh gives), one that crosses into the next
    /// quarter, or a whole pixel and a quarter on, is drawn where it falls.
    #[test]
    fn a_text_shadow_is_kept_by_the_quarter_pixel_the_glyph_cache_draws_in() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let resources = renderer_resources_with(&font);
        let red = ColorU { r: 220, g: 20, b: 20, a: 255 };
        let mut kept = GlyphCache::new();
        let mut fresh = GlyphCache::new();
        for (at, drawn) in [
            ((20.0, 50.0), 1),
            ((20.2, 50.0), 0),
            ((20.3, 50.0), 1),
            ((40.3, 50.0), 0),
            ((40.5, 50.0), 1),
            ((40.5, 51.0), 0),
        ] {
            let (frame, n) = shadowed_label(&font, &resources, "T", at, (3.0, 1.0), red, &mut kept);
            assert_eq!(n, drawn, "halos drawn for a glyph at {at:?}");
            super::super::text_halo::keep_cpu_halos(false);
            let (old, _) = shadowed_label(&font, &resources, "T", at, (3.0, 1.0), red, &mut fresh);
            super::super::text_halo::keep_cpu_halos(true);
            let (most, n) = channel_difference(&frame, &old);
            assert!(most <= 2, "at {at:?}: differs from the drawn halo by {most} in {n} channels");
        }
    }

    /// Black text on a white page, `count` runs of it, painted into a 260 x 90 frame, the page
    /// optionally inside a frame turned by `degrees` (an isolated group, resampled where it lands).
    fn text_page(
        font: &ParsedFont,
        resources: &(RendererResources, FontManager<FontRef>, FontHash),
        count: usize,
        turned_by: Option<f32>,
    ) -> AzulPixmap {
        let (rr, fm, font_hash) = resources;
        let (w, h) = (260u32, 90u32);
        let page: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize { width: w as f32, height: h as f32 },
        }
        .into();
        let mut items = Vec::new();
        if let Some(degrees) = turned_by {
            let (s, c) = degrees.to_radians().sin_cos();
            items.push(DisplayListItem::PushReferenceFrame {
                transform_key: azul_core::resources::TransformKey { id: 1 },
                initial_transform: azul_core::transform::ComputedTransform3D {
                    m: [[c, s, 0.0, 0.0], [-s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
                },
                bounds: page,
            });
        }
        items.push(DisplayListItem::Rect {
            bounds: page,
            color: ColorU { r: 255, g: 255, b: 255, a: 255 },
            border_radius: BorderRadius::default(),
        });
        for k in 0..count {
            let (x, y) = (8.0 + (k % 3) as f32 * 80.0, 20.0 + (k / 3) as f32 * 14.0);
            let clip_rect: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
                origin: LogicalPosition { x: x - 2.0, y: y - 12.0 },
                size: LogicalSize { width: 78.0, height: 16.0 },
            }
            .into();
            items.push(DisplayListItem::Text {
                glyphs: shape(font, "Tuebingen", 12.0, x, y),
                font_hash: *font_hash,
                font_size_px: 12.0,
                color: ColorU { r: 0, g: 0, b: 0, a: 255 },
                clip_rect,
                line_box: clip_rect,
                source_node_index: None,
            });
        }
        if turned_by.is_some() {
            items.push(DisplayListItem::PopReferenceFrame);
        }
        let dl = DisplayList { items, ..Default::default() };
        let mut pm = AzulPixmap::new(w, h).unwrap();
        pm.fill(255, 255, 255, 255);
        let mut gc = GlyphCache::new();
        render_display_list(&dl, &mut pm, 1.0, rr, fm, &mut gc).unwrap();
        pm
    }

    /// The most `|r - b|` of any pixel: LCD stripes make it positive at a glyph's edge, grayscale
    /// coverage over a gray page leaves it 0.
    fn colour_fringe(pm: &AzulPixmap) -> i32 {
        pm.data()
            .chunks_exact(4)
            .map(|p| (i32::from(p[0]) - i32::from(p[2])).abs())
            .max()
            .unwrap_or(0)
    }

    /// Text in a group that is resampled is grayscale (LCD stripes would smear into fringes in
    /// the resample, and cost more); the same text in the settled picture stays LCD.
    #[test]
    fn text_inside_a_resampled_group_is_grayscale_and_the_settled_picture_keeps_its_lcd() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let resources = renderer_resources_with(&font);
        let settled = text_page(&font, &resources, 6, None);
        assert!(
            colour_fringe(&settled) > 8,
            "the settled picture is LCD text (fringe {})",
            colour_fringe(&settled)
        );
        for degrees in [3.0_f32, 20.0] {
            let turned = text_page(&font, &resources, 6, Some(degrees));
            assert_eq!(
                colour_fringe(&turned),
                0,
                "the text of a group turned by {degrees} degrees is grayscale"
            );
        }
    }

    /// A run takes the thread's rasteriser and scanline and hands them back: a page of runs makes
    /// one, however many it draws and however often it is painted, and paints the same pixels as
    /// a rasteriser made for every run.
    #[test]
    fn the_text_runs_of_a_thread_share_one_rasteriser() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let resources = renderer_resources_with(&font);
        let made = || TEXT_RASTERS_MADE.with(std::cell::Cell::get);
        let before = made();
        let first = text_page(&font, &resources, 9, None);
        let second = text_page(&font, &resources, 9, Some(7.0));
        let again = text_page(&font, &resources, 9, None);
        assert!(made() - before <= 1, "one rasteriser for 27 runs: {}", made() - before);
        assert_eq!(channel_difference(&first, &again), (0, 0), "a reused rasteriser paints the same");
        TEXT_RASTER_POOL_OFF.with(|c| c.set(true));
        let made_before = made();
        let unpooled = text_page(&font, &resources, 9, None);
        let unpooled_turned = text_page(&font, &resources, 9, Some(7.0));
        TEXT_RASTER_POOL_OFF.with(|c| c.set(false));
        assert!(made() - made_before >= 9, "without the pool every run makes one: {}", made() - made_before);
        assert_eq!(channel_difference(&first, &unpooled), (0, 0), "the pool changes no pixel");
        assert_eq!(channel_difference(&second, &unpooled_turned), (0, 0), "nor in a group");
    }

    // ---- culling inside isolated groups and view lists (CPU step 2) ----

    /// `n x n` opaque cells, 10 px apart, each its own colour, from (0, 0): a page far larger than
    /// what a window shows of it.
    fn cell_items(n: usize) -> Vec<DisplayListItem> {
        let mut items = Vec::new();
        for i in 0..n {
            for j in 0..n {
                let bounds: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
                    origin: LogicalPosition { x: (i * 10) as f32, y: (j * 10) as f32 },
                    size: LogicalSize { width: 9.0, height: 9.0 },
                }
                .into();
                items.push(DisplayListItem::Rect {
                    bounds,
                    color: ColorU {
                        r: ((i * 37 + j * 11) % 251) as u8,
                        g: ((i * 5 + j * 53) % 241) as u8,
                        b: ((i * 17 + j * 3 + 90) % 211) as u8,
                        a: 255,
                    },
                    border_radius: BorderRadius::default(),
                });
            }
        }
        items
    }

    fn frame_of(key: usize, m: [[f32; 4]; 4], inner: Vec<DisplayListItem>) -> Vec<DisplayListItem> {
        let page: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize { width: 400.0, height: 400.0 },
        }
        .into();
        let mut items = vec![DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: key },
            initial_transform: azul_core::transform::ComputedTransform3D { m },
            bounds: page,
        }];
        items.extend(inner);
        items.push(DisplayListItem::PopReferenceFrame);
        items
    }

    fn scaled(k: f32, tx: f32, ty: f32) -> [[f32; 4]; 4] {
        [[k, 0.0, 0.0, 0.0], [0.0, k, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [tx, ty, 0.0, 1.0]]
    }

    fn turned_and_scaled(degrees: f32, k: f32, tx: f32, ty: f32) -> [[f32; 4]; 4] {
        let (s, c) = degrees.to_radians().sin_cos();
        [[k * c, k * s, 0.0, 0.0], [-k * s, k * c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [tx, ty, 0.0, 1.0]]
    }

    /// `dl` painted into a 200 x 120 white frame (all of it, or the `strips` of it), with the walk
    /// culling by the clip or not: the frame and how many items were painted.
    fn painted_culling(
        dl: &DisplayList,
        state: &CpuRenderState,
        strips: Option<&[LogicalRect]>,
        culling: bool,
        dpi: f32,
    ) -> (AzulPixmap, u64) {
        let (rr, fm) = (RendererResources::default(), empty_font_manager());
        let mut pm = AzulPixmap::new(200, 120).unwrap();
        pm.fill(255, 255, 255, 255);
        let mut gc = GlyphCache::new();
        GROUP_CULL_OFF.with(|c| c.set(!culling));
        crate::cpu_counters::reset();
        match strips {
            Some(strips) => {
                render_display_list_damaged(dl, &mut pm, dpi, &rr, &fm, &mut gc, state, strips).unwrap();
            }
            None => render_display_list_with_state(dl, &mut pm, dpi, &rr, &fm, &mut gc, state).unwrap(),
        }
        let painted = crate::cpu_counters::take().items_painted;
        GROUP_CULL_OFF.with(|c| c.set(false));
        (pm, painted)
    }

    /// What a group paints is only what lands: the picture is the one the unculled walk paints,
    /// pixel for pixel, and far fewer items are painted - through a scale, a turn, two frames in
    /// one another, and a damaged strip.
    #[test]
    fn a_group_paints_only_what_lands_and_the_picture_is_the_unculled_one() {
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let strip = [LogicalRect {
            origin: LogicalPosition { x: 50.0, y: 20.0 },
            size: LogicalSize { width: 40.0, height: 30.0 },
        }];
        let inner_turned = frame_of(2, turned_and_scaled(13.0, 0.8, 10.0, 5.0), cell_items(60));
        let cases: Vec<(&str, DisplayList, Option<&[LogicalRect]>)> = vec![
            ("scaled", DisplayList { items: frame_of(1, scaled(1.5, -60.0, -30.0), cell_items(40)), ..Default::default() }, None),
            ("scaled, damaged strip", DisplayList { items: frame_of(1, scaled(1.5, -60.0, -30.0), cell_items(40)), ..Default::default() }, Some(&strip)),
            ("turned and scaled", DisplayList { items: frame_of(1, turned_and_scaled(10.0, 1.3, 20.0, -20.0), cell_items(40)), ..Default::default() }, None),
            ("turned 45 degrees", DisplayList { items: frame_of(1, turned_and_scaled(45.0, 1.0, 110.0, -40.0), cell_items(40)), ..Default::default() }, None),
            ("turned 70 degrees, damaged strip", DisplayList { items: frame_of(1, turned_and_scaled(70.0, 0.9, 150.0, -60.0), cell_items(40)), ..Default::default() }, Some(&strip)),
            ("shrunk", DisplayList { items: frame_of(1, scaled(0.4, 30.0, 10.0), cell_items(40)), ..Default::default() }, None),
            ("a turned frame in a scaled one", DisplayList { items: frame_of(1, scaled(1.2, -30.0, -10.0), inner_turned), ..Default::default() }, None),
        ];
        for (name, dl, strips, dpi) in cases.iter().flat_map(|(n, d, s)| [1.0_f32, 2.0].map(move |dpi| (*n, d, *s, dpi))) {
            let name = format!("{name} at {dpi}x");
            let name = name.as_str();
            let (culled, painted) = painted_culling(dl, &state, strips, true, dpi);
            let (whole, painted_whole) = painted_culling(dl, &state, strips, false, dpi);
            assert_eq!(channel_difference(&culled, &whole), (0, 0), "{name}: the picture is the unculled one");
            let coloured = culled.data().chunks_exact(4).filter(|p| p[..3] != [255, 255, 255]).count();
            assert!(coloured > 400, "{name}: the picture shows the page ({coloured} px), the equality is not vacuous");
            // The shrunk page (0.4) lands almost whole in the frame: nothing to cull but the margin.
            let at_most = match name {
                n if n.starts_with("shrunk") => painted_whole,
                n if n.starts_with("a turned frame") => painted_whole * 3 / 4,
                _ => painted_whole / 2,
            };
            assert!(painted <= at_most, "{name}: {painted} of {painted_whole} items painted, at most {at_most}");
        }
    }

    /// A matrix that cannot be inverted, or has a perspective part, keeps the whole group; so
    /// does a clip that is not there; one that lands nowhere keeps none; the box that lands is
    /// the clip mapped back, widened by the margin.
    #[test]
    fn the_part_of_a_group_that_lands_is_the_clip_mapped_back_through_its_matrix() {
        // p = 2 s + (10, 10)
        let m: Mat3 = [2.0, 0.0, 10.0, 0.0, 2.0, 10.0, 0.0, 0.0, 1.0];
        let big = (200, 200);
        let margin = GROUP_VISIBLE_MARGIN;
        // the clip (30, 30)-(50, 50) is s in (10, 10)-(20, 20)
        assert_eq!(
            group_visible_part(&m, Some((30, 30, 50, 50)), (400, 400), big),
            Some((0.max(10 - margin), 0.max(10 - margin), 20 + margin, 20 + margin))
        );
        // no clip: all of the surface that lands in a 100 x 100 target: s up to 45
        assert_eq!(
            group_visible_part(&m, None, (100, 100), big),
            Some((0, 0, 45 + margin, 45 + margin))
        );
        // nothing lands: the clip is outside the target
        assert_eq!(group_visible_part(&m, Some((500, 500, 600, 600)), (100, 100), big), Some((0, 0, 0, 0)));
        // singular: kept whole
        let singular: Mat3 = [0.0, 0.0, 10.0, 0.0, 2.0, 10.0, 0.0, 0.0, 1.0];
        assert_eq!(group_visible_part(&singular, Some((30, 30, 50, 50)), (400, 400), big), None);
        // perspective: kept whole
        let perspective: Mat3 = [2.0, 0.0, 10.0, 0.0, 2.0, 10.0, 0.001, 0.0, 1.0];
        assert_eq!(group_visible_part(&perspective, Some((30, 30, 50, 50)), (400, 400), big), None);
        // turned by 90 degrees: s = rotated clip
        let quarter: Mat3 = [0.0, -1.0, 100.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let part = group_visible_part(&quarter, Some((40, 30, 60, 50)), (400, 400), big).expect("a box");
        assert!(part.2 > part.0 && part.3 > part.1, "{part:?}");
    }

    /// The items of a view far larger than its box: only those that can show are painted, and
    /// the picture is the unculled one.
    #[test]
    fn a_view_paints_only_the_items_of_its_list_that_show_in_its_box() {
        use azul_core::dom::DomId;
        let child = DisplayList { items: cell_items(40), ..Default::default() };
        let child_len = child.items.len() as u64;
        let view = DisplayListItem::VirtualView {
            child_dom_id: DomId { inner: 1 },
            bounds: LogicalRect {
                origin: LogicalPosition { x: 20.0, y: 10.0 },
                size: LogicalSize { width: 100.0, height: 60.0 },
            }
            .into(),
            clip_rect: LogicalRect {
                origin: LogicalPosition { x: 20.0, y: 10.0 },
                size: LogicalSize { width: 100.0, height: 60.0 },
            }
            .into(),
            content_offset: LogicalPosition { x: 0.0, y: 0.0 },
        };
        let host = DisplayList { items: vec![view], ..Default::default() };
        let state = CpuRenderState::new(ScrollOffsetMap::new())
            .with_virtual_view_display_lists(
                [(DomId { inner: 1 }, std::sync::Arc::new(child))].into_iter().collect(),
            );
        for dpi in [1.0_f32, 2.0] {
            let (culled, painted) = painted_culling(&host, &state, None, true, dpi);
            let (whole, painted_whole) = painted_culling(&host, &state, None, false, dpi);
            assert_eq!(channel_difference(&culled, &whole), (0, 0), "{dpi}x: the picture is the unculled one");
            assert!(painted_whole >= child_len, "{dpi}x: without the cull every item is painted: {painted_whole}");
            assert!(painted < 200, "{dpi}x: the box shows about 60 of {child_len} cells: {painted} painted");
        }
    }
}

#[cfg(all(test, feature = "std"))]
#[allow(clippy::float_cmp)] // exact compares on values the code copies through verbatim
#[allow(clippy::many_single_char_names)] // domain-standard coordinate/colour names
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // bounded test-fixture
                                                                        // casts
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
                offset_px: azul_css::props::basic::FloatValue::const_new(0),
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
        surfaces: Vec<AzulPixmap>,
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
                surfaces: Vec::new(),
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
            &mut st.surfaces,
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
    fn gradient_lut_linear_stops_beyond_the_line_color_its_ends_by_interpolation() {
        // -500% .. +900% black -> white: the line itself starts 500/1400 of
        // the way to white (CSS Images 3: a stop outside 0..100% is not moved
        // onto the line, the line samples the ramp). A saturating 1e30% must
        // not panic.
        let lut = build_gradient_lut_linear(
            &lin_stops(&[(-500.0, BLACK), (900.0, WHITE), (1e30, RED)]),
            None,
        );
        assert_eq!(lut.size(), 256);
        let start = lut.get(0).r;
        assert!((88..=94).contains(&start), "0% is gray 91, got {start}");
        let end = lut.get(255).r;
        assert!((104..=112).contains(&end), "100% is gray 109, got {end}");
        assert_eq!(lut.get(255).a, 255);
    }

    #[test]
    fn gradient_lut_linear_unsorted_stops_are_sorted_by_offset() {
        // Stops handed over back-to-front must still ramp from offset 0 upward.
        let lut = build_gradient_lut_linear(&lin_stops(&[(100.0, WHITE), (0.0, BLACK)]), None);
        assert_eq!(lut.get(0).r, 0);
        assert_eq!(lut.get(255).r, 255);
    }

    #[test]
    fn gradient_lut_linear_a_hard_stop_splits_the_line_into_two_colors() {
        // Two stops at the SAME offset are a hard color change (CSS Images 3).
        // agg keeps one stop per offset, which left this LUT transparent - a
        // `red 50%, blue 50%` background painted nothing.
        let lut = build_gradient_lut_linear(&lin_stops(&[(50.0, RED), (50.0, BLUE)]), None);
        assert_eq!(lut.size(), 256);
        for i in [0usize, 64, 126] {
            let c = lut.get(i);
            assert!(
                c.r == 255 && c.b == 0 && c.a == 255,
                "entry {i} is red: {c:?}"
            );
        }
        for i in [129usize, 192, 255] {
            let c = lut.get(i);
            assert!(
                c.b == 255 && c.r == 0 && c.a == 255,
                "entry {i} is blue: {c:?}"
            );
        }
    }

    #[test]
    fn gradient_lut_resolution_follows_the_line_length() {
        assert_eq!(gradient_lut_size(100.0), GRADIENT_LUT_MIN);
        assert_eq!(gradient_lut_size(600.2), 601);
        assert_eq!(gradient_lut_size(1.0e9), GRADIENT_LUT_MAX);
        assert_eq!(gradient_lut_size(f64::NAN), GRADIENT_LUT_MIN);
        // A 3 px bar of a 600 px line owns its 3 entries.
        let red = (RED, 0.0_f32, 3.0 / 600.0);
        let lut = build_gradient_lut(
            &[(red.1, red.0), (red.2, red.0), (red.2, WHITE), (1.0, WHITE)],
            gradient_lut_size(600.0),
        );
        assert_eq!(lut.size(), 600);
        assert_eq!(lut.get(1).r, 255);
        assert_eq!(lut.get(1).g, 0, "entry 1 (px 1..2) is red");
        assert_eq!(lut.get(4).g, 255, "entry 4 (px 4..5) is white");
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
                offset_px: azul_css::props::basic::FloatValue::const_new(0),
            },
            NormalizedLinearColorStop {
                offset: PercentageValue::new(100.0),
                color: ColorOrSystem::Color(WHITE),
                offset_px: azul_css::props::basic::FloatValue::const_new(0),
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
                    "w={w}, h={h} produced NaN ({x}, {y}) — a NaN center poisons the gradient \
                     transform"
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

    /// THE caret bug: a fractionally-positioned hairline was TRUNCATED onto
    /// the pixel it barely touches instead of the one it mostly covers.
    ///
    /// A 1px caret at x = 62.8 covers a fifth of column 62 and four fifths of
    /// column 63; `as i32` painted 62, a pixel to the left of where the
    /// layout put it - visible as a caret that misses its own column, and the
    /// reason a pixel-level caret assertion failed for months.
    #[test]
    fn a_fractional_hairline_lands_on_the_pixel_it_mostly_covers() {
        let mut p = pixmap(70, 6);
        render_rect(
            &mut p,
            &lrect(62.8, 1.0, 1.0, 4.0),
            RED,
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert!(
            is_reddish(px_at(&p, 63, 2)),
            "the hairline belongs to column 63, which it 80% covers"
        );
        assert_eq!(
            px_at(&p, 62, 2),
            [255, 255, 255, 255],
            "and NOT to column 62, which it barely touches"
        );
    }

    /// The other half of the same bug: truncation can erase a hairline
    /// completely. `x = 62.3, width = 0.5` truncates to the empty range
    /// 62..62 - a 1px separator or caret that is simply MISSING at some
    /// offsets and present at others.
    #[test]
    fn a_sub_pixel_rect_still_paints_something() {
        let mut p = pixmap(70, 6);
        render_rect(
            &mut p,
            &lrect(62.3, 1.0, 0.5, 4.0),
            RED,
            &BorderRadius::default(),
            None,
            1.0,
        );
        let red = p
            .data()
            .chunks_exact(4)
            .filter(|c| c[0] > 200 && c[1] < 60)
            .count();
        assert!(red > 0, "a sub-pixel rect must not vanish");
    }

    /// The control: an integer-aligned rect is unaffected - rounding and
    /// truncation agree there, which is nearly every rect in a document.
    #[test]
    fn an_integer_aligned_rect_is_unchanged_by_the_rounding() {
        let mut p = pixmap(10, 10);
        render_rect(
            &mut p,
            &lrect(2.0, 2.0, 4.0, 4.0),
            RED,
            &BorderRadius::default(),
            None,
            1.0,
        );
        let red = p
            .data()
            .chunks_exact(4)
            .filter(|c| c[0] > 200 && c[1] < 60)
            .count();
        assert_eq!(red, 16, "still exactly 4x4");
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

    /// CSS Images 3 mixes colour stops in premultiplied space: red fading to
    /// a transparent blue stays red while it fades. Chrome paints the middle
    /// of `linear-gradient(red, rgba(0, 0, 255, 0))` over white as
    /// (255, 128, 128), and so does WebRender. Mixed straight, the middle
    /// would be a purple (191, 128, 191).
    #[test]
    fn a_gradient_fading_to_transparent_keeps_its_colour_as_chrome_paints_it() {
        const CLEAR_BLUE: ColorU = ColorU {
            r: 0,
            g: 0,
            b: 255,
            a: 0,
        };
        let mut p = pixmap(4, 256);
        render_linear_gradient(
            &mut p,
            &lrect(0.0, 0.0, 4.0, 256.0),
            &linear(lin_stops(&[(0.0, RED), (100.0, CLEAR_BLUE)])),
            &BorderRadius::default(),
            None,
            1.0,
            None,
        );
        let middle = px_at(&p, 2, 128);
        assert!(
            middle[0] >= 250
                && (120..=136).contains(&middle[1])
                && (120..=136).contains(&middle[2]),
            "the middle is half-transparent red over white: {middle:?}"
        );

        let lut = build_gradient_lut(&[(0.0, RED), (1.0, CLEAR_BLUE)], 256);
        let half = lut.get(128);
        assert!(
            half.r >= 250 && half.b <= 5 && (120..=136).contains(&half.a),
            "the LUT keeps the red while the alpha falls: {half:?}"
        );
    }

    #[test]
    fn adjacent_gradients_at_a_fractional_edge_leave_no_seam() {
        // WPT css/css-backgrounds/background-gradient-subpixel-fills-area:
        // ten floats 39.6875px wide, each with a gradient background, over
        // a red list - a red seam showed at every boundary. Each gradient's
        // fractional edge was an anti-aliased PATH edge, so the shared pixel
        // was covered 0.6875 by one and 0.3125 by the other: a quarter of
        // the red stayed. Opaque rects are snapped to whole pixels
        // (`render_rect`, round_edge) and so are a browser's backgrounds;
        // the column the two gradients share is theirs, not red.
        let mut p = pixmap(24, 4);
        render_rect(
            &mut p,
            &lrect(0.0, 0.0, 24.0, 4.0),
            RED,
            &BorderRadius::default(),
            None,
            1.0,
        );
        let blue = linear(lin_stops(&[(0.0, BLUE), (100.0, BLUE)]));
        for x in [0.0, 9.6875] {
            render_linear_gradient(
                &mut p,
                &lrect(x, 0.0, 9.6875, 4.0),
                &blue,
                &BorderRadius::default(),
                None,
                1.0,
                None,
            );
        }
        let seam = px_at(&p, 9, 2);
        assert!(
            seam[0] < 8 && seam[2] > 247,
            "the shared column is the gradients' blue, no red under a seam: {seam:?}"
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
            "a zero-offset outset shadow must not paint under the border box              (only \
             the 1px anti-seam sliver may darken), got {dark}"
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

    /// The mask is rasterised in LOGICAL pixels and applied in DEVICE pixels,
    /// so on a HiDPI display (or any zoom) it is scaled UP. Nearest-neighbour
    /// threw away the coverage the rasteriser had just computed and turned a
    /// curve into a staircase; upscaling INTERPOLATES now.
    #[test]
    fn extract_mask_data_upscales_smoothly() {
        let img = r8_image(2, 2, vec![0, 255, 255, 0]);
        let mask = extract_mask_data(&img, 4, 4).expect("mask must extract");
        assert_eq!(mask.len(), 16);

        // The corners still read as their own texels...
        assert_eq!(mask[0], 0, "top-left corner");
        assert_eq!(mask[3], 255, "top-right corner");
        // ... and BETWEEN them there is now a gradient rather than a step,
        // which is the entire point.
        let interior: Vec<u8> = mask[1..3].to_vec();
        assert!(
            interior.iter().any(|v| *v > 0 && *v < 255),
            "an upscaled edge must interpolate, got {interior:?}"
        );
    }

    /// Downscaling averages instead of picking one texel: sampling a single
    /// source pixel out of a 4x4 block reports whatever happens to be in the
    /// corner, which for a mask means an edge that flickers as it moves.
    #[test]
    fn extract_mask_data_downscales_by_averaging_without_reading_out_of_bounds() {
        let img = r8_image(4, 4, (0..16).map(|i| i as u8 * 16).collect());
        let mask = extract_mask_data(&img, 1, 1).expect("mask must extract");
        assert_eq!(mask.len(), 1);
        assert!(
            mask[0] > 0,
            "a 1x1 reduction of a ramp must not report the corner texel alone"
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
                blend: azul_css::props::style::effects::StyleMixBlendMode::Normal,
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

    /// The GPU values a window paints with are those of EVERY DOM: a node inside a
    /// `VirtualView` binds its key in the view's own cache. Reading the root's alone
    /// (what the CPU paths did) left a value-only write inside a view undrawn.
    #[test]
    fn window_values_hold_the_values_of_every_dom_a_views_included() {
        use crate::managers::gpu_state::GpuStateManager;
        use azul_core::resources::ColorKey;

        let scaled = ComputedTransform3D {
            m: [
                [2.0, 0.0, 0.0, 0.0],
                [0.0, 2.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        };
        let child = DomId { inner: 1 };
        let mut gpu = GpuStateManager::default();
        {
            let root = gpu.get_or_create_cache(DomId::ROOT_ID);
            root.css_transform_keys
                .insert(NodeId::new(1), TransformKey { id: 10 });
            root.css_current_transform_values
                .insert(NodeId::new(1), ComputedTransform3D::IDENTITY);
            // The root's own scrollbar: its fade belongs to the root.
            root.scrollbar_v_opacity_keys
                .insert((DomId::ROOT_ID, NodeId::new(0)), OpacityKey { id: 11 });
            root.scrollbar_v_opacity_values
                .insert((DomId::ROOT_ID, NodeId::new(0)), 0.75);
        }
        {
            let view = gpu.get_or_create_cache(child);
            view.css_transform_keys
                .insert(NodeId::new(4), TransformKey { id: 20 });
            view.css_current_transform_values
                .insert(NodeId::new(4), scaled);
            view.opacity_keys.insert(NodeId::new(4), OpacityKey { id: 21 });
            view.current_opacity_values.insert(NodeId::new(4), 0.5);
            view.color_keys.insert(NodeId::new(5), ColorKey { id: 22 });
            view.current_color_values.insert(NodeId::new(5), BLACK);
        }

        let window = WindowGpuValues::of_doms(&gpu, [DomId::ROOT_ID, child]);
        assert_eq!(window.transforms.len(), 2, "the root's and the view's: {:?}", window.transforms);
        assert_eq!(window.transforms.get(&10).map(|t| t.m), Some(ComputedTransform3D::IDENTITY.m));
        assert_eq!(window.transforms.get(&20).map(|t| t.m), Some(scaled.m));
        assert_eq!(window.opacities.get(&21), Some(&0.5), "a view's opacity");
        assert_eq!(window.opacities.get(&11), Some(&0.75), "the root's scrollbar fade");
        assert_eq!(window.colors.get(&22), Some(&BLACK), "a view's fading background");

        // The root alone - what a CPU frame read before - holds none of the view's.
        let root_only = WindowGpuValues::of_doms(&gpu, [DomId::ROOT_ID]);
        assert!(!root_only.transforms.contains_key(&20));
        assert!(!root_only.opacities.contains_key(&21));
        assert!(root_only.colors.is_empty());

        // A DOM without a cache binds nothing.
        let nothing = WindowGpuValues::of_doms(&gpu, [DomId { inner: 9 }]);
        assert_eq!(nothing, WindowGpuValues::default());

        // The state a frame paints with carries all of them.
        let state = CpuRenderState::new(ScrollOffsetMap::new()).with_gpu_values(window.clone());
        assert_eq!(state.transforms.get(&20).map(|t| t.m), Some(scaled.m));
        assert_eq!(state.opacities.get(&21), Some(&0.5));
        assert_eq!(state.colors.get(&22), Some(&BLACK));
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
    /// A turned frame turns everything its content paints, not only its box:
    /// a box's shadow outside the box is scaled with it. The frame's surface
    /// was the box, so the shadow past the box was cut off.
    #[test]
    fn a_scaled_frame_keeps_the_shadow_its_box_casts_outside_it() {
        let black = ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        let half = azul_core::transform::ComputedTransform3D {
            m: [
                [0.5, 0.0, 0.0, 0.0],
                [0.0, 0.5, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        };
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushReferenceFrame {
                    transform_key: TransformKey { id: 1 },
                    initial_transform: half,
                    bounds: wrect(4.0, 4.0, 8.0, 8.0),
                },
                DisplayListItem::BoxShadow {
                    bounds: wrect(4.0, 4.0, 8.0, 8.0),
                    shadow: shadow(6.0, 0.0, 0.0, black),
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::Rect {
                    bounds: wrect(4.0, 4.0, 8.0, 8.0),
                    color: WHITE,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopReferenceFrame,
            ],
            ..Default::default()
        };
        let mut p = pixmap(24, 24);
        run_list(&dl, &mut p, 1.0).expect("must render");
        // The shadow lies at (10..18, 10..18) laid out, at (7..11, 7..11)
        // scaled about the box's origin (4, 4).
        assert!(
            px_at(&p, 10, 10)[0] < 64,
            "the scaled shadow past the box is {:?}, not shadow-dark",
            px_at(&p, 10, 10)
        );
    }

    /// An isolated reference frame paints its content into a surface of its
    /// own, not into the window at its layout place. A box laid out past the
    /// window's bottom edge and scaled into view by its transform keeps the
    /// part the window would cut off: painted in place, its last rows were
    /// lost before the matrix brought them into view.
    #[test]
    fn a_box_laid_out_past_the_window_and_scaled_into_view_is_painted_whole() {
        let red = ColorU {
            r: 200,
            g: 0,
            b: 0,
            a: 255,
        };
        let half_height = azul_core::transform::ComputedTransform3D {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 0.5, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        };
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushReferenceFrame {
                    transform_key: TransformKey { id: 1 },
                    initial_transform: half_height,
                    bounds: wrect(0.0, 12.0, 20.0, 12.0),
                },
                DisplayListItem::Rect {
                    bounds: wrect(0.0, 12.0, 20.0, 12.0),
                    color: red,
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopReferenceFrame,
            ],
            ..Default::default()
        };
        let mut p = pixmap(20, 20);
        run_list(&dl, &mut p, 1.0).expect("must render");
        // The box spans rows 12..24 laid out, rows 12..18 scaled about its
        // origin: every one of those rows is red, the one below it is not.
        for y in 12..18 {
            assert_eq!(
                px_at(&p, 10, y)[0],
                200,
                "row {y} of the scaled box is {:?}: the part laid out past the window was lost",
                px_at(&p, 10, y)
            );
        }
        assert_eq!(px_at(&p, 10, 19), [255, 255, 255, 255], "below the scaled box");
    }

    /// An outer shadow is drawn outside the border box only (CSS Backgrounds
    /// 3, 7.1), so a translucent box shows what is behind it and not its own
    /// shadow. A rounded box was not cut out of its shadow at all: blue at
    /// alpha 160 over white came out nearly black at its centre.
    #[test]
    fn a_translucent_rounded_box_does_not_show_its_own_shadow() {
        let radius = BorderRadius {
            top_left: 8.0,
            top_right: 8.0,
            bottom_left: 8.0,
            bottom_right: 8.0,
        };
        let black = ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        let dl = DisplayList {
            items: vec![
                DisplayListItem::BoxShadow {
                    bounds: wrect(10.0, 10.0, 40.0, 40.0),
                    shadow: shadow(4.0, 2.0, 0.0, black),
                    border_radius: radius,
                },
                DisplayListItem::Rect {
                    bounds: wrect(10.0, 10.0, 40.0, 40.0),
                    color: ColorU {
                        r: 30,
                        g: 30,
                        b: 200,
                        a: 160,
                    },
                    border_radius: radius,
                },
            ],
            ..Default::default()
        };
        let mut p = pixmap(64, 64);
        run_list(&dl, &mut p, 1.0).expect("must render");
        // Blue at 160 / 255 over white: (114, 114, 221).
        let [red, _, blue, _] = px_at(&p, 30, 30);
        assert!(
            (i32::from(red) - 114).abs() <= 2 && (i32::from(blue) - 221).abs() <= 2,
            "the centre of the translucent box is {:?}, not blue over white (114, 114, 221): its \
             own shadow shows through it",
            px_at(&p, 30, 30)
        );
        // The shadow itself still falls outside the box, to its right.
        assert!(
            px_at(&p, 52, 40)[0] < 128,
            "the shadow beside the box is gone: {:?}",
            px_at(&p, 52, 40)
        );
    }

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
    fn text_aa_settings_come_from_the_variables_given_and_from_nothing_else() {
        fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
            let pairs: Vec<(String, String)> = pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect();
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.clone())
            }
        }

        // Nothing set: the documented default, whatever this process's
        // environment says.
        let unset = TextAaSettings::from_vars(vars(&[]));
        assert_eq!(unset, TextAaSettings::default());
        assert_eq!(unset.mode, TEXT_AA_DEFAULT);
        assert!(unset.lcd_blend.is_some() && unset.lcd_pretile);

        for (value, mode) in [
            ("lcd", TextAa::Lcd),
            ("LEGACY", TextAa::Legacy),
            ("grayscale", TextAa::Grayscale),
            ("greyscale", TextAa::Grayscale),
            ("none", TextAa::None),
            ("0", TextAa::None),
            ("nonsense", TEXT_AA_DEFAULT),
        ] {
            let got = TextAaSettings::from_vars(vars(&[("AZ_TEXT_AA", value)]));
            assert_eq!(got.mode, mode, "AZ_TEXT_AA={value}");
            // The derived predicates must agree with the mode, or a caller can
            // end up on the LCD path while the blend thinks it is aliased.
            assert_eq!(got.lcd_enabled(), matches!(mode, TextAa::Lcd | TextAa::Legacy));
            assert_eq!(got.aliased(), mode == TextAa::None);
            assert_eq!(got.lcd_blend.is_none(), mode == TextAa::Legacy);
        }

        // Both spellings of the legacy blend, the blend's levels, both spellings
        // of "no tiles".
        assert!(TextAaSettings::from_vars(vars(&[("AZ_LCD_BLEND", "Legacy")]))
            .lcd_blend
            .is_none());
        let tuned = TextAaSettings::from_vars(vars(&[
            ("AZ_LCD_CONTRAST", "7"),
            ("AZ_LCD_CHROMA_LIMIT", "9"),
            ("AZ_LCD_COVERAGE_GAMMA", "200"),
        ]))
        .lcd_blend
        .expect("the linear blend");
        assert_eq!(
            (tuned.contrast, tuned.chroma_limit, tuned.coverage_gamma),
            (7, 9, 200)
        );
        assert!(!TextAaSettings::from_vars(vars(&[("AZ_LCD_PRETILE", "0")])).lcd_pretile);
        assert!(!TextAaSettings::from_vars(vars(&[("AZ_NO_LCD_PRETILE", "")])).lcd_pretile);

        assert_eq!(TextAaSettings::grayscale().mode, TextAa::Grayscale);
        assert!(!TextAaSettings::grayscale().lcd_enabled());
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
                    mix_blend_mode: azul_css::props::style::effects::StyleMixBlendMode::Normal,
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

    /// A translucent box inside an opacity group is blended once with each
    /// alpha. Blue (30, 30, 200) at alpha 160 in a group at opacity 0.7, over
    /// white, covers 160 / 255 * 0.7 of the pixel: blue lands at about 231
    /// and red at about 156. Reading the group's pixels, which hold each
    /// colour already multiplied by its alpha, as straight colours gave 198
    /// blue: the box drawn darker than any browser draws it.
    #[test]
    fn a_translucent_box_in_an_opacity_group_keeps_its_own_colour() {
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushOpacity {
                    bounds: wrect(0.0, 0.0, 4.0, 4.0),
                    opacity: 0.7,
                    opacity_key: None,
                    mix_blend_mode: azul_css::props::style::effects::StyleMixBlendMode::Normal,
                },
                DisplayListItem::Rect {
                    bounds: wrect(0.0, 0.0, 4.0, 4.0),
                    color: ColorU {
                        r: 30,
                        g: 30,
                        b: 200,
                        a: 160,
                    },
                    border_radius: BorderRadius::default(),
                },
                DisplayListItem::PopOpacity,
            ],
            ..Default::default()
        };
        let mut p = pixmap(4, 4);
        run_list(&dl, &mut p, 1.0).expect("must render");
        let [red, _, blue, alpha] = px_at(&p, 1, 1);
        assert!(
            (i32::from(blue) - 231).abs() <= 2 && (i32::from(red) - 156).abs() <= 2 && alpha == 255,
            "the box in the group is painted {:?}, not about (156, 156, 231, 255)",
            px_at(&p, 1, 1)
        );
    }

    #[test]
    fn push_opacity_with_degenerate_bounds_pushes_an_empty_group_its_pop_takes() {
        // No rect -> an empty group: it still leaves its own entry, so the
        // matching PopOpacity takes that one and never an entry of the frame
        // or clip around it.
        let mut p = pixmap(8, 8);
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut st = Stacks::new();
        run_item(
            &DisplayListItem::PushOpacity {
                bounds: wrect(0.0, 0.0, f32::NAN, 0.0),
                opacity: 0.5,
                opacity_key: None,
                mix_blend_mode: azul_css::props::style::effects::StyleMixBlendMode::Normal,
            },
            &mut p,
            &mut st,
            &state,
        )
        .unwrap();
        assert_eq!(st.masks.len(), 1);
        assert!(matches!(st.masks[0], MaskEntry::Opacity { .. }));
        assert_eq!(
            run_item(&DisplayListItem::PopOpacity, &mut p, &mut st, &state),
            Ok(())
        );
        assert!(st.masks.is_empty());
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
                    mask_image: Some(mask),
                    mask_rect: wrect(0.0, 0.0, 4.0, 4.0),
                    geometry: None,
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

    /// A mask covers its element, not just its own rect: coverage outside the
    /// mask image is zero. The CPU renderer used to mask only the mask rect,
    /// leaving the rest of the element fully painted while WebRender clipped
    /// it - the same DOM drew differently on the two backends.
    #[test]
    fn a_mask_smaller_than_the_element_clips_the_rest_of_it() {
        let mask = r8_image(1, 1, vec![255]);
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushImageMaskClip {
                    bounds: wrect(0.0, 0.0, 4.0, 4.0),
                    mask_image: Some(mask),
                    mask_rect: wrect(0.0, 0.0, 2.0, 2.0),
                    geometry: None,
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
        assert_eq!(px_at(&p, 0, 0), [0, 0, 0, 255], "inside the mask: painted");
        assert_eq!(
            px_at(&p, 3, 3),
            [255, 255, 255, 255],
            "outside the mask: clipped, not left painted"
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
                mask_image: Some(mask),
                mask_rect: wrect(0.0, 0.0, 0.0, 0.0),
                geometry: None,
            },
            &mut p,
            &mut st,
            &state,
        )
        .unwrap();
        // Since A1-05 every mask push leaves an entry, so that its pop takes its own and not
        // the entry of the group around it (this test asked for no entry before). A 0-sized
        // mask rect leaves an EMPTY one: no snapshot, no mask, no pixel to restore.
        assert_eq!(st.masks.len(), 1, "a 0-sized mask rect pushes one entry, its own");
        assert!(
            matches!(
                st.masks.last(),
                Some(MaskEntry::ImageMask { snapshot, mask_data, width: 0, height: 0, .. })
                    if snapshot.is_empty() && mask_data.is_empty()
            ),
            "and the entry is empty: it took no snapshot and masks nothing: {:?}",
            st.masks.last()
        );
    }

    // ==================================================================
    // text items without fonts
    // ==================================================================

    /// A `font_hash` layout emitted that its own `FontManager` cannot resolve is a
    /// broken invariant, not a missing asset, so `font_resolution_failed` fires a
    /// `debug_assert`. The two build profiles therefore owe DIFFERENT contracts and
    /// this pins both:
    ///
    ///   - debug: die on it. That gate exists so a test catches the desync, and a test that
    ///     swallowed it would be the exact silent failure it guards.
    ///   - release: drop that one text run and keep the frame — losing a line of text must never
    ///     take the window down in front of a user.
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
                font_hash: FontHash::from_hash(0xdead_beef),
                font_size_px: 16.0,
                color: BLACK,
                clip_rect: wrect(0.0, 0.0, 16.0, 16.0),
                line_box: wrect(0.0, 0.0, 16.0, 16.0),
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
                    font_hash: FontHash::from_hash(1),
                    font_size_px: 16.0,
                    color,
                    clip_rect: wrect(0.0, 0.0, 16.0, 16.0),
                    line_box: wrect(0.0, 0.0, 16.0, 16.0),
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
                "NEAREST SAMPLING: a 4x4 checkerboard shrunk into one pixel must average to mid \
                 grey, got {name} = {c} (pure black/white = one source pixel won)"
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
                    // The samples are PREMULTIPLIED (an ImageRef's storage):
                    // src + dst * (255 - sa) / 255.
                    let da = 255 - sa;
                    pixmap.data[di] =
                        (u32::from(sr) + u32::from(pixmap.data[di]) * da / 255).min(255) as u8;
                    pixmap.data[di + 1] =
                        (u32::from(sg) + u32::from(pixmap.data[di + 1]) * da / 255).min(255) as u8;
                    pixmap.data[di + 2] =
                        (u32::from(sb) + u32::from(pixmap.data[di + 2]) * da / 255).min(255) as u8;
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
                    None,
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

    /// A deterministic, non-uniform NV12 source (both planes).
    fn noisy_nv12(w: u32, h: u32) -> Vec<u8> {
        let len = azul_core::resources::Nv12Layout::new(w as usize, h as usize)
            .checked_total_len()
            .expect("small");
        (0..len).map(|i| ((i * 37 + 11) % 256) as u8).collect()
    }

    #[test]
    fn an_nv12_frame_blits_byte_identical_to_image_scale_sample() {
        // A camera / decoder frame in NV12 goes through the same blit as any
        // image: its rows are converted to RGB once each (the fused
        // convert + scale pass), and every painted byte is the byte the
        // reference sampler gives.
        use azul_core::resources::RawImageFormat as F;
        for fmt in [F::NV12Rec601Video, F::NV12Rec709Full] {
            let (sw, sh) = (13u32, 7u32);
            let bytes = noisy_nv12(sw, sh);
            let src = crate::image_scale::SrcImage {
                bytes: &bytes,
                format: fmt,
                width: sw,
                height: sh,
            };
            assert!(src.is_sampleable(), "{fmt:?} must be sampleable");
            for (dw, dh) in [(13u32, 7u32), (40, 23), (5, 3), (1, 1)] {
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
                    None,
                    &mut RowConversions::new(),
                );
                reference_blit(&mut want, &src, 2, 2, dw, dh);
                assert_eq!(
                    snap(&fast),
                    snap(&want),
                    "{fmt:?} {sw}x{sh} -> {dw}x{dh}: NV12 must blit byte-identical to the sampler"
                );
            }
        }
    }

    #[test]
    fn a_frame_at_the_tiles_size_converts_each_visible_row_exactly_once() {
        // A video frame that arrives at the tile's device size (the point of
        // resampling early) is a 1:1 blit: each VISIBLE source row is
        // converted once and copied. The bilinear path used to read one more
        // row (the lerp partner at weight 0) for every run of rows.
        use azul_core::resources::RawImageFormat as F;
        let (w, h) = (64u32, 32u32);
        let bytes = noisy_nv12(w, h);
        let src = crate::image_scale::SrcImage {
            bytes: &bytes,
            format: F::NV12Rec709Video,
            width: w,
            height: h,
        };
        let mut p = pixmap(w, h);
        let mut conv = RowConversions::new();
        // Only rows 8..24 are damaged.
        blit_sampled_image(&mut p, &src, 0, 0, w, h, (0, 8, w, 24), None, &mut conv);
        assert_eq!(
            conv.0, 16,
            "a 1:1 blit of 16 visible rows converts exactly those 16 source rows"
        );
        let mut want = pixmap(w, h);
        reference_blit(&mut want, &src, 0, 0, w, h);
        let row = (w * 4) as usize;
        assert_eq!(
            &snap(&p)[8 * row..24 * row],
            &snap(&want)[8 * row..24 * row],
            "and paints what the sampler gives"
        );
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
        blit_sampled_image(&mut p, &src, 0, 0, dw, dh, (0, 0, dw, dh), None, &mut conv);
        assert!(
            conv.0 <= sh as usize + 1,
            "a {sh}-row source must be converted about once per row, not per destination row: {} \
             conversions",
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
        blit_sampled_image(&mut p, &src, 0, 0, dw, dh, (0, 0, dw, dh), None, &mut conv);
        assert!(
            conv.0 <= sh as usize,
            "a {sh}-row source downscaled 4x must convert at most {sh} rows, got {}",
            conv.0
        );

        // A CLIPPED blit touches only the source rows its window needs — the
        // point of narrowing the loop.
        let mut p = pixmap(dw, dh);
        let mut conv = RowConversions::new();
        blit_sampled_image(&mut p, &src, 0, 0, dw, dh, (0, 0, dw, 4), None, &mut conv);
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

    /// An app's "nothing to show yet" - an image node holding a null image
    /// without a src (AzMeet's remote tile before its first frame,
    /// AzVideocut's empty monitor, a canvas callback's fallback) - paints
    /// nothing, as WebRender does (a null image has no pixels to upload) and
    /// as a browser's `<video>` or `<canvas>` with no frame yet: the box's own
    /// background shows. The CPU renderer alone painted a flat #C8C8C8 tile.
    #[test]
    fn a_null_image_with_nothing_to_show_yet_paints_nothing() {
        for radius in [0.0, 6.0] {
            let dl = DisplayList {
                items: vec![DisplayListItem::Image {
                    bounds: wrect(0.0, 0.0, 6.0, 6.0),
                    image: ImageRef::null_image(2, 2, RawImageFormat::RGBA8, Vec::new()),
                    border_radius: BorderRadius {
                        top_left: radius,
                        top_right: radius,
                        bottom_left: radius,
                        bottom_right: radius,
                    },
                }],
                ..Default::default()
            };
            let mut p = pixmap(8, 8);
            let before = snap(&p);
            run_list(&dl, &mut p, 1.0).expect("must render");
            assert_eq!(before, p.data(), "radius {radius}: nothing is painted");
        }
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

    fn damaged(
        dl: &DisplayList,
        p: &mut AzulPixmap,
        rects: &[LogicalRect],
    ) -> Result<Vec<LogicalRect>, String> {
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
                bytes: bytes.into(),
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
    use azul_css::props::{
        basic::pixel::{PixelValue, PixelValueNoPercent},
        style::box_shadow::{BoxShadowClipMode, StyleBoxShadow},
    };

    use super::*;

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
    use azul_css::props::{
        basic::pixel::{PixelValue, PixelValueNoPercent},
        style::box_shadow::{BoxShadowClipMode, StyleBoxShadow},
    };

    use super::*;

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

    pub(in crate::cpurender) fn load_test_font_pub() -> Option<ParsedFont> {
        load_test_font()
    }
    pub(in crate::cpurender) fn rr_with_pub(
        f: &ParsedFont,
    ) -> (RendererResources, FontManager<FontRef>, FontHash) {
        rr_with(f)
    }
    pub(in crate::cpurender) fn shape_pub(
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
        (rr, fm, FontHash::from_hash(hash))
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
            "split-run path diverges from the sweep on {diff} bytes —              overlapping \
             components must merge coverage, not composite tiles"
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

        let aa = TextAaSettings::default();
        if !aa.lcd_enabled() || aa.lcd_blend.is_none() {
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
            "pre-blended tiles diverge from the sweep on {diff} bytes — same pipeline must mean \
             same pixels (check FIR padding and tile placement)"
        );
    }

    /// A FRACTIONAL clip cuts tiled LCD text where it cuts the other text
    /// paths (Engine backlog 5, FB3's "text clip twins"): the sweep and the
    /// grayscale path paint the whole pixels `text_clip_pixel_box` gives
    /// `text_run_clip`, while the tile path kept its own i32 box, snapped
    /// OUTWARD from the clip_rect - a clip ending at x = 100.5 let the tiles
    /// paint column 100, which the sweep leaves alone.
    #[test]
    fn a_fractional_clip_cuts_tiled_lcd_text_where_it_cuts_grayscale_text() {
        let Some(font) = load_test_font() else {
            eprintln!("no system test font — skipping");
            return;
        };
        let aa = TextAaSettings::default();
        if !aa.lcd_enabled() || aa.lcd_blend.is_none() || !aa.lcd_pretile {
            eprintln!("no LCD tile path in this configuration — skipping");
            return;
        }
        let (rr, fm, font_hash) = rr_with(&font);
        let font_size = 24.0;
        let glyphs = shape(&font, "HHHHHHHHHHHHHHHHHHHH", font_size, 8.0, 40.0);
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
        let proven = LogicalRect {
            origin: LogicalPosition {
                x: -10_000.0,
                y: -10_000.0,
            },
            size: LogicalSize {
                width: 20_000.0,
                height: 20_000.0,
            },
        };
        // Every right edge x + 0.5 across the run's middle (one of them lands
        // in a stem), and a fractional bottom edge through the glyphs.
        for right in 60..140u16 {
            let clip_rect = LogicalRect {
                origin: LogicalPosition { x: 0.0, y: 0.0 },
                size: LogicalSize {
                    width: f32::from(right) + 0.5,
                    height: 35.5,
                },
            };
            let paint = |uniform_bg| {
                let mut pm = AzulPixmap::new(320, 60).unwrap();
                pm.fill(bg.r, bg.g, bg.b, 255);
                let mut gc = GlyphCache::new();
                render_text_with_bg(
                    &glyphs,
                    font_hash,
                    font_size,
                    color,
                    &mut pm,
                    &clip_rect,
                    None,
                    &rr,
                    &fm,
                    1.0,
                    &mut gc,
                    (0.0, 0.0),
                    false,
                    uniform_bg,
                );
                pm
            };
            let sweep = paint(None);
            let tiles = paint(Some((bg, proven.into())));
            let diff: Vec<usize> = sweep
                .data
                .iter()
                .zip(tiles.data.iter())
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .map(|(i, _)| i / 4)
                .collect();
            assert!(
                diff.is_empty(),
                "clip right edge {right}.5, bottom 35.5: the tiles paint {} pixels the sweep does \
                 not, first at ({}, {})",
                diff.len(),
                diff[0] % 320,
                diff[0] / 320
            );
        }
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
            line_box: clip_rect,
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
                    "{what}: pixel {i} channel {c} = {} is outside the ink gamut [{}, {}] — text \
                     blended against something that is not the backdrop (transparent layer?)",
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
        // The surface is 2px taller than the page: a scroll frame that covers
        // the WHOLE root is the page's own and is painted in place (see
        // `allocate_layers_from_display_list`), and this test is about a
        // frame that gets a layer. The backdrop covers the whole surface.
        let surface: crate::solver3::display_list::WindowLogicalRect = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 200.0,
                height: 42.0,
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
                bounds: surface,
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
                line_box: page,
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

        let mut plain = AzulPixmap::new(200, 42).unwrap();
        plain.fill(255, 255, 255, 255);
        let mut gc1 = GlyphCache::new();
        render_display_list(&dl, &mut plain, 1.0, &rr, &fm, &mut gc1).unwrap();
        assert_ink_gamut(&plain, fg, bg, "flat render");

        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut layered = AzulPixmap::new(200, 42).unwrap();
        layered.fill(255, 255, 255, 255);
        let mut gc2 = GlyphCache::new();
        let mut comp = CompositorState::new(200, 42);
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
            "text inside a scroll-frame layer diverges from the flat render on {ldiff} bytes: the \
             layer was swept against a transparent backdrop instead of the page"
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
            line_box: clip_rect,
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
            "plain vs damaged-full-rect diverge on {} bytes, first at px ({}, {}): plain={:?} \
             damaged={:?}",
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
            "the pre-fix `(tx1 - tx0) as u32 * 4` would overflow here — the empty-rect guard in \
             pass 2b must skip before this math runs"
        );
    }
}

/// A glyph run whose geometry no rasterizer can place is skipped, never
/// handed to agg - and a clip thinner than a pixel paints nothing.
///
/// agg rasterizes on `i32` coordinates (24.8 fixed point, x tripled on the
/// LCD path). A pen that a broken layout parked at 1e38 saturated to
/// `i32::MAX`, the LCD `* 3` wrapped, the rasterizer's x range straddled all
/// of `i32` and `ScanlineU8::reset` allocated a one-cover span: `index out of
/// bounds: the len is 1 but the index is 1` in `add_cell`. That panic took
/// down the AzMail reading pane (a receipt table) and aborted AzWidgets seven
/// times on 2026-09-30 with no table involved, always from `render_glyphs_lcd`.
#[cfg(all(test, feature = "std"))]
mod unplaceable_glyph_geometry_tests {
    use super::*;

    const W: u32 = 96;
    const H: u32 = 32;
    const FONT_PX: f32 = 16.0;
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

    fn white_pixmap() -> AzulPixmap {
        let mut pm = AzulPixmap::new(W, H).expect("a 96x32 pixmap");
        pm.fill(255, 255, 255, 255);
        pm
    }

    /// How many pixels are not pure white.
    fn inked(pm: &AzulPixmap) -> usize {
        pm.data()
            .chunks_exact(4)
            .filter(|p| p[0] != 255 || p[1] != 255 || p[2] != 255)
            .count()
    }

    fn whole_window() -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: W as f32,
                height: H as f32,
            },
        }
    }

    /// `HHHHHH` on a baseline at y = 22.
    fn sane_run(font: &ParsedFont) -> Vec<GlyphInstance> {
        lcd_pretile_tests::shape_pub(font, "HHHHHH", FONT_PX, 2.0, 22.0)
    }

    /// The sane run, then the same glyph at pens nothing can place: NaN on
    /// either axis, both infinities and +-1e38.
    fn run_with_unplaceable_pens(font: &ParsedFont) -> Vec<GlyphInstance> {
        let mut glyphs = sane_run(font);
        let template = glyphs[0];
        for (x, y) in [
            (f32::NAN, 22.0),
            (22.0, f32::NAN),
            (f32::INFINITY, 22.0),
            (f32::NEG_INFINITY, 22.0),
            (1.0e38, 22.0),
            (-1.0e38, 22.0),
        ] {
            glyphs.push(GlyphInstance {
                point: LogicalPosition { x, y },
                ..template
            });
        }
        glyphs
    }

    /// The LCD batch sweep, called directly.
    fn lcd_sweep(
        pm: &mut AzulPixmap,
        clip: Option<AzRect>,
        glyphs: &[GlyphInstance],
        font: &ParsedFont,
        font_hash: FontHash,
    ) {
        let scale = FONT_PX / f32::from(font.font_metrics.units_per_em);
        let mut gc = GlyphCache::new();
        render_glyphs_lcd(
            pm,
            clip,
            glyphs,
            font,
            font_hash,
            0,
            scale,
            1.0,
            BLACK,
            1.0,
            (0.0, 0.0),
            &mut gc,
        );
    }

    /// The grayscale path (the one text-shadows take).
    fn grayscale(
        pm: &mut AzulPixmap,
        clip: Option<AzRect>,
        glyphs: &[GlyphInstance],
        rr: &RendererResources,
        fm: &FontManager<FontRef>,
        font_hash: FontHash,
    ) {
        let mut gc = GlyphCache::new();
        render_text(
            glyphs,
            font_hash,
            FONT_PX,
            BLACK,
            pm,
            &whole_window(),
            clip,
            rr,
            fm,
            1.0,
            &mut gc,
            (0.0, 0.0),
            true,
        );
    }

    #[test]
    fn the_lcd_sweep_skips_the_pens_it_cannot_place() {
        let Some(font) = lcd_pretile_tests::load_test_font_pub() else {
            eprintln!("no system test font - skipping");
            return;
        };
        let (_rr, _fm, font_hash) = lcd_pretile_tests::rr_with_pub(&font);
        let glyphs = run_with_unplaceable_pens(&font);

        let mut unclipped = white_pixmap();
        lcd_sweep(&mut unclipped, None, &glyphs, &font, font_hash);
        let mut clipped = white_pixmap();
        lcd_sweep(
            &mut clipped,
            AzRect::from_xywh(0.0, 0.0, W as f32, H as f32),
            &glyphs,
            &font,
            font_hash,
        );

        // Skipped glyph by glyph: the placeable ones still paint.
        assert!(inked(&unclipped) > 0, "the sane glyphs of the run paint");
        assert!(
            inked(&clipped) > 0,
            "the sane glyphs of the run paint under a clip"
        );
    }

    #[test]
    fn the_grayscale_and_pretiled_paths_skip_the_pens_they_cannot_place() {
        let Some(font) = lcd_pretile_tests::load_test_font_pub() else {
            eprintln!("no system test font - skipping");
            return;
        };
        let (rr, fm, font_hash) = lcd_pretile_tests::rr_with_pub(&font);
        let glyphs = run_with_unplaceable_pens(&font);

        let mut gray = white_pixmap();
        grayscale(&mut gray, None, &glyphs, &rr, &fm, font_hash);
        assert!(
            inked(&gray) > 0,
            "the sane glyphs paint on the grayscale path"
        );

        // The pre-blended tile path (a proven uniform background).
        let mut tiled = white_pixmap();
        let mut gc = GlyphCache::new();
        render_text_with_bg(
            &glyphs,
            font_hash,
            FONT_PX,
            BLACK,
            &mut tiled,
            &whole_window(),
            None,
            &rr,
            &fm,
            1.0,
            &mut gc,
            (0.0, 0.0),
            false,
            Some((
                WHITE,
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
        assert!(inked(&tiled) > 0, "the sane glyphs paint on the tile path");
    }

    #[test]
    fn a_clip_thinner_than_a_pixel_paints_nothing() {
        let Some(font) = lcd_pretile_tests::load_test_font_pub() else {
            eprintln!("no system test font - skipping");
            return;
        };
        let (rr, fm, font_hash) = lcd_pretile_tests::rr_with_pub(&font);
        let glyphs = sane_run(&font);
        // A sliver across every row the H crossbars can sit on, and one down
        // every column the first stem can sit in: both cover no whole pixel.
        let slivers = [
            (0.0, 14.2, W as f32, 0.4),
            (0.0, 15.2, W as f32, 0.4),
            (0.0, 16.2, W as f32, 0.4),
            (3.2, 0.0, 0.4, H as f32),
            (4.2, 0.0, 0.4, H as f32),
        ];
        for (x, y, w, h) in slivers {
            let clip = AzRect::from_xywh(x, y, w, h);
            assert!(clip.is_some(), "the sliver is a valid rect");

            let mut pm = white_pixmap();
            lcd_sweep(&mut pm, clip, &glyphs, &font, font_hash);
            assert_eq!(
                inked(&pm),
                0,
                "the LCD sweep painted outside the sub-pixel clip {clip:?}"
            );

            let mut pm = white_pixmap();
            grayscale(&mut pm, clip, &glyphs, &rr, &fm, font_hash);
            assert_eq!(
                inked(&pm),
                0,
                "the grayscale path painted outside the sub-pixel clip {clip:?}"
            );
        }
    }

    /// Ink in the columns from `x0` rightwards.
    fn inked_right_of(pm: &AzulPixmap, x0: u32) -> usize {
        let w = pm.width() as usize;
        pm.data()
            .chunks_exact(4)
            .enumerate()
            .filter(|(i, p)| (i % w) as u32 >= x0 && (p[0] != 255 || p[1] != 255 || p[2] != 255))
            .count()
    }

    /// A text item is clipped to its OWN `clip_rect` (cut to the active
    /// clip) on every paint path, as WebRender clips it. The pre-tiled LCD
    /// path did; the batch sweep and the grayscale path clipped to the
    /// STACK clip alone and only skipped a run whose clip_rect lay wholly
    /// outside it - so ink past a run's clip_rect painted on two paths and
    /// not on the third (a list marker cut by one, painted by the other).
    #[test]
    fn a_runs_ink_outside_its_own_clip_rect_is_cut_on_every_paint_path() {
        let Some(font) = lcd_pretile_tests::load_test_font_pub() else {
            eprintln!("no system test font - skipping");
            return;
        };
        let (rr, fm, font_hash) = lcd_pretile_tests::rr_with_pub(&font);
        let glyphs = sane_run(&font);

        // Premise: unclipped, the six H's ink well past x = 20.
        let mut open = white_pixmap();
        grayscale(&mut open, None, &glyphs, &rr, &fm, font_hash);
        assert!(
            inked_right_of(&open, 20) > 0,
            "premise: the run inks past x = 20 when nothing clips it"
        );

        // The run's own clip ends at x = 20. The stack clip is absent, or
        // the whole window: wider than the run's clip either way.
        let own = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 20.0,
                height: H as f32,
            },
        };
        for stack in [None, AzRect::from_xywh(0.0, 0.0, W as f32, H as f32)] {
            let mut pm = white_pixmap();
            let mut gc = GlyphCache::new();
            render_text(
                &glyphs,
                font_hash,
                FONT_PX,
                BLACK,
                &mut pm,
                &own,
                stack,
                &rr,
                &fm,
                1.0,
                &mut gc,
                (0.0, 0.0),
                true,
            );
            assert!(
                inked(&pm) > 0,
                "the H's inside the run's clip paint (stack clip {stack:?})"
            );
            assert_eq!(
                inked_right_of(&pm, 20),
                0,
                "the grayscale path painted ink right of the run's own clip_rect (x >= 20) \
                 under stack clip {stack:?}"
            );

            // The pre-blended tile path (a proven uniform background): the
            // same rule.
            let mut tiled = white_pixmap();
            let mut gc = GlyphCache::new();
            render_text_with_bg(
                &glyphs,
                font_hash,
                FONT_PX,
                BLACK,
                &mut tiled,
                &own,
                stack,
                &rr,
                &fm,
                1.0,
                &mut gc,
                (0.0, 0.0),
                false,
                Some((
                    WHITE,
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
            assert_eq!(
                inked_right_of(&tiled, 20),
                0,
                "the tile path painted ink right of the run's own clip_rect (x >= 20) under \
                 stack clip {stack:?}"
            );
        }
    }

    // ---- the three guards, without a font ----

    #[test]
    fn an_em_no_rasterizer_can_take_has_no_ink_reach() {
        assert_eq!(glyph_ink_reach(16.0), Some(64.0), "4 em");
        for bad in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.0,
            -16.0,
            MAX_RASTER_EM_PX * 2.0,
            1.0e38,
        ] {
            assert_eq!(glyph_ink_reach(bad), None, "em {bad}");
        }
    }

    #[test]
    fn a_pen_reaches_the_pixmap_only_from_within_its_ink_reach() {
        // Inside, and just outside on every side within the reach.
        assert!(glyph_pen_reaches_pixmap(10.0, 20.0, 64.0, W, H));
        assert!(glyph_pen_reaches_pixmap(-60.0, 20.0, 64.0, W, H));
        assert!(glyph_pen_reaches_pixmap(W as f32 + 60.0, 20.0, 64.0, W, H));
        assert!(glyph_pen_reaches_pixmap(10.0, -60.0, 64.0, W, H));
        assert!(glyph_pen_reaches_pixmap(10.0, H as f32 + 60.0, 64.0, W, H));
        // Beyond the reach, non-finite, or far past agg's integer range.
        for (x, y) in [
            (-65.0, 20.0),
            (W as f32 + 65.0, 20.0),
            (10.0, -65.0),
            (10.0, H as f32 + 65.0),
            (f32::NAN, 20.0),
            (10.0, f32::NAN),
            (f32::INFINITY, 20.0),
            (f32::NEG_INFINITY, 20.0),
            (1.0e38, 20.0),
            (-1.0e38, 20.0),
            (8.0e8, 20.0),
            (10.0, 1.0e38),
        ] {
            assert!(
                !glyph_pen_reaches_pixmap(x, y, 64.0, W, H),
                "pen ({x}, {y}) must be skipped"
            );
        }
        assert!(
            !glyph_pen_reaches_pixmap(10.0, 20.0, f32::NAN, W, H),
            "a NaN reach admits nothing"
        );
    }

    #[test]
    fn a_text_clip_becomes_the_pixels_it_covers_or_nothing() {
        let rect = |x, y, w, h| AzRect::from_xywh(x, y, w, h).expect("a valid rect");
        // Inside the pixmap: the truncating rule the text paths always used.
        assert_eq!(
            text_clip_pixel_box(rect(3.7, 2.2, 10.0, 5.0), W, H),
            Some((3, 2, 12, 6))
        );
        // Larger than the pixmap, or huge: clamped to it, so the LCD stripe
        // clip's `* 3` cannot overflow.
        assert_eq!(
            text_clip_pixel_box(rect(-50.0, -50.0, 1000.0, 1000.0), W, H),
            Some((0, 0, W as i32 - 1, H as i32 - 1))
        );
        assert_eq!(
            text_clip_pixel_box(rect(-1.0e30, -1.0e30, 2.0e30, 2.0e30), W, H),
            Some((0, 0, W as i32 - 1, H as i32 - 1))
        );
        // Thinner than a pixel on either axis, or off the pixmap: nothing.
        assert_eq!(text_clip_pixel_box(rect(4.2, 0.0, 0.4, 10.0), W, H), None);
        assert_eq!(text_clip_pixel_box(rect(0.0, 16.2, 10.0, 0.4), W, H), None);
        assert_eq!(
            text_clip_pixel_box(rect(1.0e30, 0.0, 10.0, 10.0), W, H),
            None
        );
        assert_eq!(
            text_clip_pixel_box(rect(-500.0, 0.0, 10.0, 10.0), W, H),
            None
        );
    }
}

#[cfg(test)]
mod disjoint_damage_tests {
    use super::disjoint_damage;

    fn area(boxes: &[[i32; 4]]) -> i64 {
        boxes.iter().map(|b| i64::from(b[2] - b[0]) * i64::from(b[3] - b[1])).sum()
    }

    fn covers(boxes: &[[i32; 4]], x: i32, y: i32) -> usize {
        boxes.iter().filter(|b| b[0] <= x && x < b[2] && b[1] <= y && y < b[3]).count()
    }

    /// A window grown from 500x600 to 800x900 whose top bar stretched: the bar's
    /// row, the exposed right strip and the exposed bottom strip. They overlap
    /// in corners, and their bounding box is the whole window; the repaint is
    /// the area they cover, each pixel once.
    #[test]
    fn overlapping_damage_is_cut_apart_not_boxed_together() {
        let bar = [0, 0, 800, 30];
        let right = [500, 0, 800, 900];
        let bottom = [0, 600, 800, 900];
        let boxes = disjoint_damage(vec![bar, right, bottom]);
        let union = 500 * 30 + 300 * 900 + 500 * 300;
        assert_eq!(area(&boxes), union, "the area they cover, not their bounding box: {boxes:?}");
        for (x, y) in [(10, 10), (600, 10), (600, 700), (10, 700), (499, 29), (500, 600)] {
            assert_eq!(covers(&boxes, x, y), 1, "({x}, {y}) is painted once: {boxes:?}");
        }
        assert_eq!(covers(&boxes, 10, 300), 0, "the untouched middle is not painted: {boxes:?}");
    }

    /// Two rects that overlap almost entirely are one box: cutting them apart
    /// would only add passes.
    #[test]
    fn nearly_the_same_damage_is_one_box() {
        let boxes = disjoint_damage(vec![[0, 0, 100, 100], [2, 2, 102, 102]]);
        assert_eq!(boxes, vec![[0, 0, 102, 102]]);
    }

    /// Rects that only touch stay as they are.
    #[test]
    fn touching_damage_stays_apart() {
        let boxes = disjoint_damage(vec![[0, 0, 10, 10], [10, 0, 20, 10]]);
        assert_eq!(boxes.len(), 2);
        assert_eq!(area(&boxes), 200);
    }
}

#[cfg(test)]
mod an_opacity_group_blends_only_its_box {
    use super::*;

    fn lr(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    /// A 20 x 10 group inside a full-window damage rect touches its own
    /// 20 x 10 (40 x 20 at 2x), never the window.
    #[test]
    fn a_group_blends_its_box_inside_the_damage_not_the_damage() {
        let damage = AzRect::from_xywh(0.0, 0.0, 2560.0, 1600.0);
        let r = opacity_group_region(&lr(100.0, 50.0, 20.0, 10.0), damage, 2.0);
        assert_eq!((r.x, r.y, r.width, r.height), (200.0, 100.0, 40.0, 20.0));
    }

    /// The part of a group outside the damage rect is not touched.
    #[test]
    fn a_group_half_outside_the_damage_blends_the_half_inside() {
        let damage = AzRect::from_xywh(0.0, 0.0, 110.0, 200.0);
        let r = opacity_group_region(&lr(100.0, 50.0, 20.0, 10.0), damage, 1.0);
        assert_eq!((r.x, r.y, r.width, r.height), (100.0, 50.0, 10.0, 10.0));
    }

    /// A group with no box touches nothing (and still leaves its entry).
    #[test]
    fn a_group_without_a_box_touches_nothing() {
        let r = opacity_group_region(&lr(10.0, 10.0, 0.0, 0.0), None, 1.0);
        assert_eq!(r.width * r.height, 0.0);
    }
}

/// A rounded box with a radius of a thousand pixels or more keeps the segments of
/// its corner arcs within the geometry margin of the rasterizer (`FIX-d0950e410-1`).
///
/// The rasterizer cuts a path's geometry [`GEOMETRY_MARGIN`] (32 px) beyond the
/// visible pixels, which lets a fill under a clip give the pixels of a fill under
/// none only while no segment of the path is longer than the margin. AGG steps an arc
/// so that a segment is about `sqrt(radius / scale)` px - 31.6 px for a radius of
/// 1000 px at 1x, 44.7 px for 2000: the cases the margin of d0950e410 (measured on
/// radii up to 60) did not reach.
#[cfg(test)]
mod a_big_rounded_boxs_arcs_stay_within_the_geometry_margin_tests {
    use super::*;

    /// A box of `2 * radius_px` with corners of `radius_px` device px (a circle),
    /// its top-left at `(x, y)`, as the display list gives it at `dpi`.
    fn circle(radius_px: f64, dpi: f32, x: f32, y: f32) -> PathStorage {
        let side = (2.0 * radius_px) as f32;
        let rect = AzRect::from_xywh(x, y, side, side).expect("a box");
        let corner = (radius_px / f64::from(dpi)) as f32;
        build_rounded_rect_path(
            &rect,
            &BorderRadius {
                top_left: corner,
                top_right: corner,
                bottom_left: corner,
                bottom_right: corner,
            },
            dpi,
        )
    }

    /// The longest distance between two consecutive vertices of `path`.
    fn longest_segment(path: &PathStorage) -> f64 {
        let (mut x, mut y) = (0.0, 0.0);
        let mut previous: Option<(f64, f64)> = None;
        let mut longest = 0.0_f64;
        for i in 0..path.total_vertices() {
            let cmd = path.vertex_idx(i, &mut x, &mut y);
            if !agg_rust::basics::is_vertex(cmd) {
                previous = None;
                continue;
            }
            if !agg_rust::basics::is_move_to(cmd) {
                if let Some((px, py)) = previous {
                    longest = longest.max((x - px).hypot(y - py));
                }
            }
            previous = Some((x, y));
        }
        longest
    }

    /// The corner arcs of a rounded box leave no segment longer than the margin
    /// minus the two pixels a cut's rounding needs - at any radius a box can have,
    /// and at both dpis. (On the old step a radius of 1000 px at 1x left segments of
    /// 31.6 px, 2000 px of 44.7 px, 8000 px of 89 px.)
    #[test]
    fn no_arc_segment_of_a_rounded_box_is_longer_than_the_margin() {
        let limit = GEOMETRY_MARGIN - 2.0;
        for dpi in [1.0_f32, 2.0] {
            for radius in [500.0, 784.0, 1000.0, 1500.0, 2000.0, 4000.0, 8000.0] {
                let longest = longest_segment(&circle(radius, dpi, 0.0, 0.0));
                assert!(
                    longest > 1.0 && longest <= limit,
                    "radius {radius} px at {dpi}x: the longest segment is {longest:.2} px, \
                     the margin allows {limit} px"
                );
            }
        }
    }

    /// Below the point where a segment would pass the cap, nothing changed: the
    /// scale is the output's own, never coarser than 1, and the work cap of an
    /// absurd radius still wins.
    #[test]
    fn the_arc_scale_is_the_old_one_wherever_the_segments_were_short_enough() {
        assert_eq!(arc_approximation_scale(1.0, 100.0), 1.0);
        assert_eq!(arc_approximation_scale(2.0, 100.0), 2.0);
        assert_eq!(arc_approximation_scale(0.5, 100.0), 1.0);
        assert_eq!(arc_approximation_scale(1.0, 0.0), 1.0);
        // The boundary: a step of 28 / 1.25 = 22.4 px, (22.4 px)^2 = 501.76 px
        // at 1x - so that AGG's last segment, up to 1.25 steps, is 28 px.
        assert_eq!(arc_approximation_scale(1.0, 500.0), 1.0);
        assert_eq!(arc_approximation_scale(2.0, 1000.0), 2.0);
        // Past it the scale grows with the radius: a step of 22.4 px.
        assert!((arc_approximation_scale(1.0, 1003.52) - 2.0).abs() < 1e-9);
        assert!((arc_approximation_scale(1.0, 5017.6) - 10.0).abs() < 1e-9);
        // An absurd radius is stepped as coarsely as it always was.
        assert_eq!(
            arc_approximation_scale(1.0, 1.0e9),
            MAX_FINE_ARC_RADIUS / 1.0e9
        );
    }

    /// The pixels: a circle of 1000, 1500 and 2000 px filled under a clip has, inside
    /// the clip, the coverage it has under none - the pass that repaints a damage rect
    /// gives what the full repaint gives there. The clip's left edge is walked across
    /// the top arc, so the cut of the geometry (32 px beyond the clip) lands in every
    /// position of a segment somewhere in the grid. The circle's top arc runs through
    /// a 400 x 300 pixmap just below y = 150.
    #[test]
    fn a_circle_of_a_thousand_pixels_filled_under_a_clip_has_inside_it_what_it_has_under_none() {
        fn fill(radius: f64, clip: Option<AzRect>) -> AzulPixmap {
            let mut path = circle(radius, 1.0, (200.0 - radius) as f32, 150.0);
            let mut p = AzulPixmap::new(400, 300).expect("a pixmap");
            p.fill(0, 0, 0, 0);
            agg_fill_path_clipped(
                &mut p,
                &mut path,
                &Rgba8::new(0, 0, 0, 255),
                FillingRule::NonZero,
                clip,
            );
            p
        }
        let alpha = |p: &AzulPixmap, x: u32, y: u32| p.data()[((y * p.width + x) * 4 + 3) as usize];

        let mut different = Vec::new();
        for radius in [1000.0, 1500.0, 2000.0] {
            let unclipped = fill(radius, None);
            let arc = alpha(&unclipped, 200, 151);
            assert!(
                arc > 0,
                "harness: the circle's top arc runs through the pixmap (radius {radius}): {arc}"
            );
            for left in (96..=170).step_by(2) {
                let clip = AzRect::from_xywh(left as f32, 20.0, 150.0, 200.0).expect("a clip");
                let clipped = fill(radius, Some(clip));
                for y in 20..220 {
                    for x in left..left + 150 {
                        let (c, u) = (alpha(&clipped, x, y), alpha(&unclipped, x, y));
                        if c != u {
                            different.push((radius, left, x, y, c, u));
                        }
                    }
                }
            }
        }
        assert!(
            different.is_empty(),
            "(radius, clip left, x, y, coverage under the clip, coverage under none) differ: \
             {:?}",
            &different[..different.len().min(20)]
        );
    }
}

#[cfg(test)]
mod dilate_disk_tests {
    //! `dilate_disk` grows a text shadow's glyphs by a disk. It walked every
    //! pixel of the shadow's buffer over every tap of the disk; it now walks the
    //! pixels that have ink. The result must be the same bytes.

    use super::*;

    /// The gather implementation `dilate_disk` replaced, kept as the reference.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn reference(p: &mut AzulPixmap, r: f32) {
        if r.is_nan() || r <= 0.0 {
            return;
        }
        let reach = (r + 0.5).ceil() as i32;
        let mut taps: Vec<(i32, i32, u32)> = Vec::new();
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                let w = (r + 0.5 - d).clamp(0.0, 1.0);
                if w > 0.0 {
                    taps.push((dx, dy, (w * 256.0).round() as u32));
                }
            }
        }
        let (w, h) = (p.width as i32, p.height as i32);
        let src = p.data.to_vec();
        let out = &mut p.data;
        for y in 0..h {
            for x in 0..w {
                let mut best = [0u32; 4];
                for &(dx, dy, weight) in &taps {
                    let (sx, sy) = (x + dx, y + dy);
                    if sx < 0 || sy < 0 || sx >= w || sy >= h {
                        continue;
                    }
                    let i = ((sy * w + sx) * 4) as usize;
                    if src[i + 3] == 0 {
                        continue;
                    }
                    for c in 0..4 {
                        best[c] = best[c].max(u32::from(src[i + c]) * weight);
                    }
                }
                let i = ((y * w + x) * 4) as usize;
                for c in 0..4 {
                    out[i + c] = ((best[c] + 128) >> 8).min(255) as u8;
                }
            }
        }
    }

    /// A pixmap with `ink` per mille of its pixels set to a random colour, from a
    /// fixed seed.
    fn sparse(w: u32, h: u32, ink: u32) -> AzulPixmap {
        let mut p = AzulPixmap::new(w, h).expect("a pixmap");
        p.fill(0, 0, 0, 0);
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for i in 0..(w * h) as usize {
            if (next() % 1000) as u32 >= ink {
                continue;
            }
            let a = (next() % 255) as u8 + 1;
            // premultiplied-looking: the colour never above the alpha
            for c in 0..3 {
                p.data[i * 4 + c] = (next() % (u64::from(a) + 1)) as u8;
            }
            p.data[i * 4 + 3] = a;
        }
        p
    }

    #[test]
    fn a_dilated_buffer_is_the_buffer_the_gather_made() {
        for (w, h, ink) in [(37, 29, 120), (64, 40, 30), (90, 17, 700), (5, 5, 1000), (1, 40, 500)] {
            for r in [0.4_f32, 1.0, 2.5, 4.0, 6.3] {
                let mut fast = sparse(w, h, ink);
                let mut slow = sparse(w, h, ink);
                dilate_disk(&mut fast, r);
                reference(&mut slow, r);
                assert!(
                    fast.data.to_vec() == slow.data.to_vec(),
                    "{w} x {h}, {ink} per mille ink, radius {r}: the dilated bytes differ"
                );
            }
        }
    }

    /// The cost of a solid halo's dilation on a label's buffer (700 x 100 px, a
    /// tenth of it ink, a radius of 4.5 px: a 1 .. 2 line label at 2x), against the
    /// gather it replaced. Not an assertion - timings are the machine's - run it
    /// with `--ignored --nocapture` to see the two.
    #[test]
    #[ignore = "a timing, not a check"]
    fn how_long_a_labels_halo_takes_to_dilate() {
        let timed = |f: &dyn Fn(&mut AzulPixmap)| {
            let mut best = std::time::Duration::MAX;
            for _ in 0..3 {
                let mut p = sparse(700, 100, 100);
                let start = std::time::Instant::now();
                f(&mut p);
                best = best.min(start.elapsed());
            }
            best
        };
        let gather = timed(&|p| reference(p, 4.5));
        let scatter = timed(&|p| dilate_disk(p, 4.5));
        eprintln!(
            "dilate 700 x 100 px, 10% ink, radius 4.5: gather {gather:?}, by ink {scatter:?} ({:.1}x)",
            gather.as_secs_f64() / scatter.as_secs_f64()
        );
    }

    #[test]
    fn a_radius_that_grows_nothing_changes_nothing() {
        for r in [0.0_f32, -3.0, f32::NAN] {
            let mut p = sparse(20, 20, 300);
            let before = p.data.to_vec();
            dilate_disk(&mut p, r);
            assert!(p.data.to_vec() == before, "radius {r}");
        }
    }

    /// The point of the change: the work follows the ink. An empty buffer costs
    /// nothing however large it is, and one pixel of ink reaches only its disk.
    #[test]
    fn an_empty_buffer_stays_empty_and_one_pixel_grows_to_its_disk() {
        let mut empty = AzulPixmap::new(300, 200).expect("a pixmap");
        empty.fill(0, 0, 0, 0);
        dilate_disk(&mut empty, 5.0);
        assert!(empty.data.to_vec().iter().all(|b| *b == 0));

        let mut dot = AzulPixmap::new(41, 41).expect("a pixmap");
        dot.fill(0, 0, 0, 0);
        let centre = (20 * 41 + 20) * 4;
        for c in 0..4 {
            dot.data[centre + c] = 255;
        }
        dilate_disk(&mut dot, 4.0);
        let covered = dot.data.to_vec().chunks(4).filter(|px| px[3] > 0).count();
        // a disk of radius 4.5 around the dot: between its area (63) and the square's (81)
        assert!((57..=81).contains(&covered), "{covered} pixels covered");
        assert_eq!(dot.data[centre + 3], 255, "the dot itself stays");
    }
}

/// Guards of the 2026-11-21 audit, area A1 (`audit/A1.md`): the ones that need this file's
/// private items.
#[cfg(test)]
mod audit_a1_tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: LogicalPosition { x, y },
            size: LogicalSize { width: w, height: h },
        }
    }

    /// A row-vector 4x4 that scales by `k` and then moves by `(tx, ty)` logical px.
    fn scale_then_move(k: f32, tx: f32, ty: f32) -> [[f32; 4]; 4] {
        [
            [k, 0.0, 0.0, 0.0],
            [0.0, k, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [tx, ty, 0.0, 1.0],
        ]
    }

    /// A1-02 (structure): the geometry of a group is what `ReferenceFrameGroup::begin` opened
    /// before it was split off. Hand-computed: a 40 x 30 box at (10, 20), scaled by 2 about its
    /// origin and moved by 5 logical px in x, at DPI 2. Device box: x = 20, y = 40, 80 x 60.
    /// Group pixel s lands at o + M (p - o) with o = (20, 40), p = (20, 40) + s and the move
    /// 5 * 2 = 10 device px: (0, 0) -> (30, 40), (80, 60) -> (190, 160).
    /// Mutation: `* dpi` -> `/ dpi` on the matrix's translation lands (0, 0) at (22.5, 40).
    #[test]
    fn the_geometry_of_a_group_is_its_device_box_and_the_matrix_about_its_origin() {
        let b = rect(10.0, 20.0, 40.0, 30.0);
        let clip = AzRect::from_xywh(4.0, 6.0, 300.0, 200.0);
        let g = GroupGeometry::of(&scale_then_move(2.0, 5.0, 0.0), &b, &b, 2.0, clip);
        assert_eq!((g.x, g.y, g.width, g.height), (20, 40, 80, 60));
        assert_eq!(g.outer_clip, Some((4, 6, 304, 206)));
        let land = |sx: f64, sy: f64| {
            (
                g.matrix[0] * sx + g.matrix[1] * sy + g.matrix[2],
                g.matrix[3] * sx + g.matrix[4] * sy + g.matrix[5],
            )
        };
        assert_eq!(land(0.0, 0.0), (30.0, 40.0));
        assert_eq!(land(80.0, 60.0), (190.0, 160.0));
        assert!(mat3_is_affine(&g.matrix));
        // The part of the surface that can land under the clip: the clip mapped back,
        // ((4 - 30) / 2, (6 - 40) / 2) .. ((304 - 30) / 2, (206 - 40) / 2) = (-13, -17) ..
        // (137, 83), widened and cut to the 80 x 60 surface: all of it. With no margin and a
        // clip of the right half of where it lands, x from (110 - 30) / 2 = 40.
        assert_eq!(
            group_visible_part(&g.matrix, g.outer_clip, (400, 300), (g.width, g.height)),
            Some((0, 0, 80, 60))
        );
        assert_eq!(
            group_part_within(&g.matrix, Some((110, 0, 400, 300)), (400, 300), (g.width, g.height), 0),
            Some((40, 0, 80, 60))
        );
    }

    fn wrect(x: f32, y: f32, w: f32, h: f32) -> crate::solver3::display_list::WindowLogicalRect {
        rect(x, y, w, h).into()
    }

    fn fill(x: f32, y: f32, w: f32, h: f32, rgb: [u8; 3]) -> DisplayListItem {
        DisplayListItem::Rect {
            bounds: wrect(x, y, w, h),
            color: ColorU { r: rgb[0], g: rgb[1], b: rgb[2], a: 255 },
            border_radius: BorderRadius::default(),
        }
    }

    /// A reference frame of `key` with the box `(0, 0) w x h` and the matrix `m` around `inner`.
    fn frame(key: usize, m: [[f32; 4]; 4], (w, h): (f32, f32), inner: Vec<DisplayListItem>) -> Vec<DisplayListItem> {
        let mut items = vec![DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: key },
            initial_transform: azul_core::transform::ComputedTransform3D { m },
            bounds: wrect(0.0, 0.0, w, h),
        }];
        items.extend(inner);
        items.push(DisplayListItem::PopReferenceFrame);
        items
    }

    /// A row-vector 4x4 that turns by `degrees`, scales by `k` and then moves by `(tx, ty)`.
    fn turn_scale_move(degrees: f32, k: f32, tx: f32, ty: f32) -> [[f32; 4]; 4] {
        let (s, c) = degrees.to_radians().sin_cos();
        [
            [k * c, k * s, 0.0, 0.0],
            [-k * s, k * c, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [tx, ty, 0.0, 1.0],
        ]
    }

    fn list(items: Vec<DisplayListItem>) -> DisplayList {
        DisplayList { items, ..Default::default() }
    }

    fn px(p: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.width + x) * 4) as usize;
        [p.data[i], p.data[i + 1], p.data[i + 2], p.data[i + 3]]
    }

    /// `dl` painted in full into a white `w` x `h` frame at DPI 1, and what the walk counted.
    fn full(dl: &DisplayList, state: &CpuRenderState, (w, h): (u32, u32)) -> (AzulPixmap, crate::cpu_counters::CpuFrameCounters) {
        let (rr, fm) = (RendererResources::default(), empty_font_manager());
        let mut pm = AzulPixmap::new(w, h).unwrap();
        pm.fill(255, 255, 255, 255);
        let mut gc = GlyphCache::new();
        crate::cpu_counters::reset();
        render_display_list_with_state(dl, &mut pm, 1.0, &rr, &fm, &mut gc, state).unwrap();
        (pm, crate::cpu_counters::take())
    }

    /// A1-03 (c): a group whose box lands nowhere gets no surface. Two frames with a 40 x 30
    /// box: one turned by 30 degrees and moved to (1000, 1000), far outside the 200 x 120 frame
    /// (the frame's rect mapped back through it is x in [-1366, -1132], nowhere near the box),
    /// one scaled by 1.5 and moved to (20, 10). One surface is made, of 40 x 30 px, the other
    /// group is counted empty, and the picture is the one the unculled walk paints.
    /// Mutation: `lands` always true in `ReferenceFrameGroup::begin` counts two groups opened.
    #[test]
    fn a_group_that_lands_nowhere_gets_no_surface() {
        let mut items = frame(1, turn_scale_move(30.0, 1.0, 1000.0, 1000.0), (40.0, 30.0), vec![fill(5.0, 5.0, 20.0, 10.0, [255, 0, 0])]);
        items.extend(frame(2, turn_scale_move(0.0, 1.5, 20.0, 10.0), (40.0, 30.0), vec![fill(5.0, 5.0, 20.0, 10.0, [0, 0, 255])]));
        let dl = list(items);
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let (culled, counted) = full(&dl, &state, (200, 120));
        assert_eq!(counted.groups_opened, 1, "one group lands: {counted:?}");
        assert_eq!(counted.groups_empty, 1, "the other lands nowhere: {counted:?}");
        assert_eq!(counted.group_px, 40 * 30, "only the group that lands has a surface: {counted:?}");
        assert_eq!(counted.group_px_painted, 40 * 30, "and all of it can land: {counted:?}");
        // The blue rect (5, 5) 20 x 10 scaled by 1.5 and moved by (20, 10): x 27.5 .. 57.5,
        // y 17.5 .. 32.5.
        assert_eq!(px(&culled, 35, 22), [0, 0, 255, 255]);
        assert_eq!(px(&culled, 100, 100), [255, 255, 255, 255]);
        GROUP_CULL_OFF.with(|c| c.set(true));
        let (whole, counted_whole) = full(&dl, &state, (200, 120));
        GROUP_CULL_OFF.with(|c| c.set(false));
        assert_eq!(counted_whole.groups_opened, 2, "unculled, both groups have a surface");
        assert!(culled.data() == whole.data(), "the picture is the unculled one");
    }

    /// A1-04 (e): a surface made transparent by the allocator is transparent black, of the size
    /// asked for; no size, no surface.
    #[test]
    fn a_transparent_pixmap_is_zeroed_bytes() {
        let p = AzulPixmap::new_transparent(3, 2).expect("a pixmap");
        assert_eq!((p.width(), p.height(), p.data().len()), (3, 2, 24));
        assert!(p.data().iter().all(|b| *b == 0));
        assert!(AzulPixmap::new_transparent(0, 5).is_none());
        assert!(AzulPixmap::new_transparent(5, 0).is_none());
        assert!(AzulPixmap::new_transparent(u32::MAX, u32::MAX).is_none(), "an absurd size is refused, not aborted on");
    }

    /// A1-05: a mask that reaches no pixel of the pixmap leaves an (empty) entry, so its pop
    /// closes it and not the group around it. A half-opaque group with a green fill at
    /// (10, 10) 50 x 50 and, inside it, a mask at (1000, 1000): the fill ends up half green over
    /// white, (128, 255, 128). Before, the mask's pop took the opacity group's entry, the
    /// group's own pop found none, and the fill stayed opaque green over a cleared box.
    /// Mutation: `PopImageMaskClip` popping whatever is on top paints (0, 255, 0).
    #[test]
    fn a_mask_that_reaches_no_pixel_does_not_close_the_group_around_it() {
        let dl = list(vec![
            DisplayListItem::PushOpacity {
                bounds: wrect(10.0, 10.0, 50.0, 50.0),
                opacity: 0.5,
                opacity_key: None,
                mix_blend_mode: Default::default(),
            },
            fill(10.0, 10.0, 50.0, 50.0, [0, 255, 0]),
            DisplayListItem::PushImageMaskClip {
                bounds: wrect(1000.0, 1000.0, 10.0, 10.0),
                mask_image: None,
                mask_rect: wrect(1000.0, 1000.0, 10.0, 10.0),
                geometry: None,
            },
            DisplayListItem::PopImageMaskClip,
            DisplayListItem::PopOpacity,
        ]);
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let (painted, _) = full(&dl, &state, (100, 80));
        let [r, g, b, a] = px(&painted, 30, 30);
        assert!((120..=136).contains(&r) && g == 255 && (120..=136).contains(&b) && a == 255, "half green over white, got {:?}", [r, g, b, a]);
        assert_eq!(px(&painted, 80, 70), [255, 255, 255, 255], "outside the group nothing changed");
    }

    // ---- text and a page of everything that could show a seam -------------------------------

    fn test_font() -> Option<ParsedFont> {
        let candidates = [
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "C:/Windows/Fonts/arial.ttf",
        ];
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let arc = std::sync::Arc::new(rust_fontconfig::FontBytes::Owned(std::sync::Arc::from(bytes.as_slice())));
                if let Some(font) = ParsedFont::from_bytes(&bytes, 0, &mut Vec::new()).map(|f| f.with_source_bytes(arc)) {
                    return Some(font);
                }
            }
        }
        None
    }

    /// The resources and the font manager a walk needs, with `font` (if there is one) in them.
    fn resources(font: Option<&ParsedFont>) -> (RendererResources, FontManager<FontRef>, Option<FontHash>) {
        let rr = RendererResources::default();
        let Some(font) = font else {
            return (rr, empty_font_manager(), None);
        };
        let font_ref = crate::parsed_font_to_font_ref(font.clone());
        let hash = crate::font_ref_to_parsed_font(&font_ref).hash;
        let fm: FontManager<FontRef> = FontManager::new(rust_fontconfig::FcFontCache::default()).expect("FontManager::new");
        fm.insert_font(rust_fontconfig::FontId::new(), font_ref);
        (rr, fm, Some(FontHash::from_hash(hash)))
    }

    fn glyphs_of(font: &ParsedFont, text: &str, size: f32, x: f32, y: f32) -> Vec<GlyphInstance> {
        let scale = size / f32::from(font.font_metrics.units_per_em);
        let mut pen_x = x;
        let mut out = Vec::new();
        for c in text.chars() {
            let gid = font.lookup_glyph_index(c as u32).unwrap_or(0);
            let advance = f32::from(font.get_horizontal_advance(gid)) * scale;
            out.push(GlyphInstance {
                index: u32::from(gid),
                point: LogicalPosition { x: pen_x, y },
                size: LogicalSize { width: advance, height: size },
            });
            pen_x += advance;
        }
        out
    }

    /// A run of `text` with its baseline at `(x, y)`, in black; nothing without a font.
    fn label(font: Option<(&ParsedFont, FontHash)>, text: &str, size: f32, x: f32, y: f32) -> Vec<DisplayListItem> {
        let Some((font, font_hash)) = font else {
            return Vec::new();
        };
        let clip_rect = wrect(x - 4.0, y - size * 1.2, size * 0.7 * text.len() as f32 + 8.0, size * 1.6);
        vec![DisplayListItem::Text {
            glyphs: glyphs_of(font, text, size, x, y),
            font_hash,
            font_size_px: size,
            color: ColorU { r: 0, g: 0, b: 0, a: 255 },
            clip_rect,
            line_box: clip_rect,
            source_node_index: None,
        }]
    }

    fn halo(blur: f32, spread: f32, rgb: [u8; 3]) -> StyleBoxShadow {
        use azul_css::props::basic::pixel::{PixelValue, PixelValueNoPercent};
        let length = |v: f32| PixelValueNoPercent { inner: PixelValue::px(v) };
        StyleBoxShadow {
            offset_x: length(0.0),
            offset_y: length(0.0),
            blur_radius: length(blur),
            spread_radius: length(spread),
            color: ColorU { r: rgb[0], g: rgb[1], b: rgb[2], a: 255 },
            clip_mode: azul_css::props::style::box_shadow::BoxShadowClipMode::Outset,
        }
    }

    /// `inner` under a text shadow.
    fn haloed(shadow: StyleBoxShadow, inner: Vec<DisplayListItem>) -> Vec<DisplayListItem> {
        let mut items = vec![DisplayListItem::PushTextShadow { shadow }];
        items.extend(inner);
        items.push(DisplayListItem::PopTextShadow);
        items
    }

    /// A page 900 x 700 with something of every kind across the rows 234 and 468, where a
    /// repaint of the whole page on 4 threads puts its seams (3 bands of 234 rows: 630,000 px
    /// are 3 bands' worth): a rounded fill, a box shadow, a label with a solid halo and one with
    /// a blurred halo, plain text, a half-opaque group with a fill and text, a scaled group of
    /// cells (the separable composite), and a turned label with a halo (the general composite).
    fn seam_page(font: Option<(&ParsedFont, FontHash)>) -> DisplayList {
        let mut items = vec![fill(0.0, 0.0, 900.0, 700.0, [236, 233, 225])];
        // 1. a rounded fill across row 234
        items.push(DisplayListItem::Rect {
            bounds: wrect(40.0, 200.0, 200.0, 80.0),
            color: ColorU { r: 40, g: 120, b: 200, a: 255 },
            border_radius: BorderRadius { top_left: 14.0, top_right: 14.0, bottom_left: 14.0, bottom_right: 14.0 },
        });
        // 2. a box shadow across row 468
        items.push(DisplayListItem::BoxShadow {
            bounds: wrect(60.0, 440.0, 160.0, 60.0),
            shadow: StyleBoxShadow { color: ColorU { r: 0, g: 0, b: 0, a: 160 }, ..halo(9.0, 2.0, [0, 0, 0]) },
            border_radius: BorderRadius::default(),
        });
        items.push(fill(60.0, 440.0, 160.0, 60.0, [250, 250, 250]));
        // 3. labels with halos across row 234 (baselines at 240 and 236), plain text across 468
        items.extend(haloed(halo(0.0, 3.0, [255, 255, 255]), label(font, "Neckarhalde", 24.0, 300.0, 240.0)));
        items.extend(haloed(halo(2.0, 0.0, [255, 240, 120]), label(font, "Ammergasse", 18.0, 520.0, 236.0)));
        items.extend(label(font, "Wilhelmstrasse 12", 16.0, 300.0, 473.0));
        // 4. a half-opaque group across row 468
        items.push(DisplayListItem::PushOpacity {
            bounds: wrect(540.0, 430.0, 180.0, 80.0),
            opacity: 0.5,
            opacity_key: None,
            mix_blend_mode: Default::default(),
        });
        items.push(fill(540.0, 430.0, 180.0, 80.0, [200, 30, 30]));
        items.extend(label(font, "Schlossberg", 20.0, 552.0, 474.0));
        items.push(DisplayListItem::PopOpacity);
        // 5. a scaled group of 12 x 12 cells of 9 px (box 120 x 120, scaled by 1.7 from
        //    (700, 140): rows 140 .. 344, across row 234)
        let mut cells = Vec::new();
        for i in 0..12u8 {
            for j in 0..12u8 {
                cells.push(fill(f32::from(i) * 10.0, f32::from(j) * 10.0, 9.0, 9.0, [i * 20, j * 20, 90 + i * 5 + j * 5]));
            }
        }
        items.extend(frame(1, turn_scale_move(0.0, 1.7, 700.0, 140.0), (120.0, 120.0), cells));
        // 6. a turned label with a halo (box 220 x 44 turned by 20 degrees about (0, 0) and moved
        //    to (40, 560): its far end is 220 sin 20 = 75 px lower, rows 560 .. 676), and one
        //    across row 468 (moved to (330, 400): rows 400 .. 516)
        for (key, tx, ty) in [(2usize, 40.0_f32, 560.0_f32), (3, 330.0, 400.0)] {
            let mut inner = vec![fill(0.0, 0.0, 220.0, 44.0, [255, 255, 210])];
            inner.extend(haloed(halo(0.0, 2.0, [255, 255, 255]), label(font, "Hirschauer Steige", 22.0, 6.0, 30.0)));
            items.extend(frame(key, turn_scale_move(20.0, 1.0, tx, ty), (220.0, 44.0), inner));
        }
        list(items)
    }

    /// `dl` repainted in full (one damage rect, the whole frame) into a white `w` x `h` frame
    /// on `threads` threads, and what was counted.
    fn repaint(
        dl: &DisplayList,
        (rr, fm): (&RendererResources, &FontManager<FontRef>),
        (w, h): (u32, u32),
        dpi: f32,
        threads: usize,
    ) -> (AzulPixmap, crate::cpu_counters::CpuFrameCounters) {
        let state = CpuRenderState::new(ScrollOffsetMap::new()).with_paint_threads(threads);
        let mut pm = AzulPixmap::new(w, h).unwrap();
        pm.fill(255, 255, 255, 255);
        let mut gc = GlyphCache::new();
        let whole = [rect(0.0, 0.0, w as f32 / dpi, h as f32 / dpi)];
        crate::cpu_counters::reset();
        render_display_list_damaged(dl, &mut pm, dpi, rr, fm, &mut gc, &state, &whole).unwrap();
        (pm, crate::cpu_counters::take())
    }

    fn differing_pixels(a: &AzulPixmap, b: &AzulPixmap) -> usize {
        a.data().chunks_exact(4).zip(b.data().chunks_exact(4)).filter(|(p, q)| p != q).count()
    }

    /// A1-06 (a): a repaint in bands on four threads is the repaint of one pass, pixel for
    /// pixel, on a page with every kind of item across the seams (`seam_page`), at DPI 1, 1.5
    /// and 2. The banded repaint runs FIRST, so its threads draw the halos and glyphs
    /// themselves; the single pass then finds the halos kept. The bands are counted, and the
    /// band threads' counts reach the frame's counters (as many items walked per band as the
    /// one pass walks).
    /// Mutations: a band painted at `y - y0` (in its own coordinates) moves everything below
    /// the first seam; copying back `rows` instead of the band's columns is caught by the
    /// narrower rect of the second test; a band thread without the frame's cull switches or a
    /// glyph cache with other settings changes pixels; `merge` left out loses the walked items.
    #[test]
    fn a_repaint_in_bands_is_the_repaint_of_one_pass() {
        let font = test_font();
        let (rr, fm, font_hash) = resources(font.as_ref());
        let dl = seam_page(font.as_ref().zip(font_hash));
        for dpi in [1.0_f32, 1.5, 2.0] {
            let (banded, counted) = repaint(&dl, (&rr, &fm), (900, 700), dpi, 4);
            let (single, counted_single) = repaint(&dl, (&rr, &fm), (900, 700), dpi, 1);
            assert_eq!(counted.bands, 3, "dpi {dpi}: 630,000 px are three bands: {counted:?}");
            assert_eq!(counted.band_threads, 2, "dpi {dpi}: this thread and two more: {counted:?}");
            assert_eq!((counted_single.bands, counted_single.band_threads), (0, 0), "dpi {dpi}: one pass");
            // Every band walks the list once, stepping over the frames that cannot paint in
            // its rows (A1-07 (h): the third band steps over the scaled group's 144 cells, the
            // first over the two turned labels; at DPI 1 that is 364 items against 3 x 173),
            // and every band's count reaches the frame's: one band alone walks at most what
            // the one pass walks, so more than that is more than one thread's count. (The
            // test asked for exactly 3 x the pass, which held before the walk stepped.)
            assert!(
                counted.items_walked > counted_single.items_walked
                    && counted.items_walked <= 3 * counted_single.items_walked,
                "dpi {dpi}: every band walks the list at most once, and its count reaches the \
                 frame's: {} in bands, {} in one pass",
                counted.items_walked,
                counted_single.items_walked
            );
            assert_eq!(counted.px_cleared, counted_single.px_cleared, "dpi {dpi}: every pixel is cleared once");
            let coloured = single.data().chunks_exact(4).filter(|p| p[..3] != [236, 233, 225]).count();
            assert!(coloured > 20_000, "dpi {dpi}: the page shows its content ({coloured} px), the comparison is not vacuous");
            assert_eq!(differing_pixels(&banded, &single), 0, "dpi {dpi}: the bands are the pass");
        }
    }

    /// A1-06 (a): the same for a damage rect inside the frame, (60, 30) 780 x 640 of the page:
    /// what a band writes back is its rect and nothing left or right of it, and the pixels
    /// outside the damage are untouched (they are a marker colour no item paints).
    #[test]
    fn a_damage_rect_in_bands_leaves_the_pixels_around_it_alone() {
        let font = test_font();
        let (rr, fm, font_hash) = resources(font.as_ref());
        let dl = seam_page(font.as_ref().zip(font_hash));
        let damage = [rect(60.0, 30.0, 780.0, 640.0)];
        let paint = |threads: usize| {
            let state = CpuRenderState::new(ScrollOffsetMap::new()).with_paint_threads(threads);
            let mut pm = AzulPixmap::new(900, 700).unwrap();
            pm.fill(1, 2, 3, 255);
            let mut gc = GlyphCache::new();
            crate::cpu_counters::reset();
            render_display_list_damaged(&dl, &mut pm, 1.0, &rr, &fm, &mut gc, &state, &damage).unwrap();
            (pm, crate::cpu_counters::take())
        };
        let (banded, counted) = paint(4);
        let (single, _) = paint(1);
        assert_eq!(counted.bands, 2, "780 x 640 = 499,200 px are two bands: {counted:?}");
        assert_eq!(differing_pixels(&banded, &single), 0);
        for (x, y) in [(59, 300), (840, 300), (400, 29), (400, 670), (0, 0), (899, 699)] {
            assert_eq!(px(&banded, x, y), [1, 2, 3, 255], "({x}, {y}) is outside the damage");
        }
        assert_ne!(px(&banded, 60, 300), [1, 2, 3, 255], "the damage's first column is painted");
        assert_ne!(px(&banded, 839, 669), [1, 2, 3, 255], "and its last pixel");
    }

    /// A1-29: `release_cpu_halos` drops what this thread's CPU renderer keeps - a halo and a
    /// remembered "not kept" alike - and the store keeps new ones afterwards.
    /// Mutation: a release that leaves the memo as it is still finds both.
    #[test]
    fn released_halos_are_gone_and_the_store_keeps_new_ones() {
        use super::super::text_halo::{keep_cpu_halo, kept_cpu_halo, release_cpu_halos, CpuHalo};
        let halo = || {
            Some(std::sync::Arc::new(CpuHalo { width: 2, height: 1, offset: (0, 0), coverage: vec![255, 0] }))
        };
        let (a, b) = (0xA129_0001_u64, 0xA129_0002_u64);
        keep_cpu_halo(a, halo());
        keep_cpu_halo(b, None);
        assert!(matches!(kept_cpu_halo(a), Some(Some(_))), "premise: kept");
        assert!(matches!(kept_cpu_halo(b), Some(None)), "premise: remembered as not kept");
        release_cpu_halos();
        assert!(kept_cpu_halo(a).is_none() && kept_cpu_halo(b).is_none(), "released");
        keep_cpu_halo(a, halo());
        assert!(matches!(kept_cpu_halo(a), Some(Some(_))), "and the store keeps on keeping");
        release_cpu_halos();
    }

    /// A1-06 (a), wired to A8-38: the bands of a repaint read the window's glyph cache and
    /// build what it lacks into caches of their own, which the window's cache takes back when
    /// they are done; no thread writes the window's cache while they run. A window with
    /// grayscale text (not the default): the banded repaint is the single pass, pixel for
    /// pixel; afterwards the window's cache has its settings and holds the cells the bands
    /// built, though it built none itself; and a second banded repaint, whose bands find
    /// those cells in it, paints the same pixels.
    /// Mutations: `band_glyph_cache` without `set_text_aa` draws LCD text in the bands;
    /// `absorb` left out leaves the window's cache empty; a band that painted with the
    /// window's cache itself would count its builds there.
    #[test]
    fn the_bands_read_the_windows_glyph_cache_and_hand_back_what_they_built() {
        let Some(font) = test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, fm, font_hash) = resources(Some(&font));
        let dl = seam_page(Some(&font).zip(font_hash));
        let grayscale = TextAaSettings::grayscale();
        let paint = |gc: &mut GlyphCache, threads: usize| {
            let state = CpuRenderState::new(ScrollOffsetMap::new()).with_paint_threads(threads);
            let mut pm = AzulPixmap::new(900, 700).unwrap();
            pm.fill(255, 255, 255, 255);
            let whole = [rect(0.0, 0.0, 900.0, 700.0)];
            crate::cpu_counters::reset();
            render_display_list_damaged(&dl, &mut pm, 1.0, &rr, &fm, gc, &state, &whole).unwrap();
            (pm, crate::cpu_counters::take())
        };
        let mut window = GlyphCache::new();
        let _ = window.set_text_aa(grayscale);
        let (banded, counted) = paint(&mut window, 4);
        assert_eq!(counted.bands, 3, "630,000 px are three bands: {counted:?}");
        assert!(window.text_aa() == grayscale, "the window's cache is back in its place, with its settings");
        let held = window.cells_len();
        assert!(held > 0, "and holds what the bands built");
        assert_eq!(window.cells_built(), 0, "it built none of it: nothing wrote it while the bands ran");
        // The builds of the band caches reach the frame's counters through their threads':
        // at least one per cell set the window holds now (two bands may each have built one
        // that crosses their seam).
        assert!(counted.cells_built >= held as u64, "{counted:?} for {held} cell sets");
        let (again, counted_again) = paint(&mut window, 4);
        assert_eq!(counted_again.cells_built, 0, "every band finds every cell set in the window's cache");
        assert_eq!(window.cells_len(), held, "a second repaint needs no cell the first did not leave");
        assert_eq!(differing_pixels(&again, &banded), 0, "the cells a band finds are the cells a band builds");
        let mut one = GlyphCache::new();
        let _ = one.set_text_aa(grayscale);
        let (single, _) = paint(&mut one, 1);
        assert_eq!(differing_pixels(&banded, &single), 0, "grayscale text in every band, as in one pass");
    }

    // ---- stepping over frames ---------------------------------------------------------------

    const IDENTITY: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    /// A label as a map's list holds one: a frame at `(x, y)` with the matrix `m` around a
    /// stacking context and its ink, a 40 x 12 fill. Five items.
    fn label_frame(key: usize, m: [[f32; 4]; 4], x: f32, y: f32) -> Vec<DisplayListItem> {
        vec![
            DisplayListItem::PushReferenceFrame {
                transform_key: azul_core::resources::TransformKey { id: key },
                initial_transform: azul_core::transform::ComputedTransform3D { m },
                bounds: wrect(x, y, 40.0, 12.0),
            },
            DisplayListItem::PushStackingContext { z_index: 0, bounds: wrect(x, y, 40.0, 12.0) },
            fill(x, y, 40.0, 12.0, [(key % 200) as u8, 60, 160]),
            DisplayListItem::PopStackingContext,
            DisplayListItem::PopReferenceFrame,
        ]
    }

    /// `n` labels on a grid of 20 columns, 50 px apart, rows 20 px apart; every other one moved
    /// by 3 px by its matrix, every seventh turned by 10 degrees.
    fn label_grid(n: usize) -> DisplayList {
        let mut items = Vec::new();
        for i in 0..n {
            let (x, y) = ((i % 20) as f32 * 50.0, (i / 20) as f32 * 20.0);
            let m = if i % 7 == 6 {
                turn_scale_move(10.0, 1.0, 0.0, 0.0)
            } else if i % 2 == 1 {
                turn_scale_move(0.0, 1.0, 3.0, 0.0)
            } else {
                IDENTITY
            };
            items.extend(label_frame(i + 1, m, x, y));
        }
        list(items)
    }

    /// A1-07 (h): a walk steps over the frames that cannot paint under its damage rect. A grid
    /// of n labels of five items each and a damage rect (0, 0) 100 x 40 over its corner: the
    /// labels that can reach it are those of columns 0 .. 2 and rows 0 .. 2 (the third of each
    /// only touches it), 9 of them, and two turned ones whose group lies within the 64 px the
    /// walk paints around what lands, so the walk hands n pushes and 11 x 4 more items to the
    /// cull instead of 5 n; four times the labels walk four times the pushes and the same few
    /// items, and the picture is the one the unculled walk paints.
    /// Mutations: without the step every item is walked (5 n); a step to `frame.end` instead of
    /// `frame.end + 1` hands a frame's pop to the walk and paints the label's neighbours moved.
    #[test]
    fn a_walk_steps_over_the_frames_that_cannot_paint_under_its_damage() {
        let (rr, fm) = (RendererResources::default(), empty_font_manager());
        let damage = [rect(0.0, 0.0, 100.0, 40.0)];
        let state = CpuRenderState::new(ScrollOffsetMap::new()).with_paint_threads(1);
        let mut walked = Vec::new();
        for n in [200usize, 800] {
            let dl = label_grid(n);
            let paint = |cull: bool| {
                let mut pm = AzulPixmap::new(1000, 820).unwrap();
                pm.fill(255, 255, 255, 255);
                let mut gc = GlyphCache::new();
                GROUP_CULL_OFF.with(|c| c.set(!cull));
                crate::cpu_counters::reset();
                render_display_list_damaged(&dl, &mut pm, 1.0, &rr, &fm, &mut gc, &state, &damage).unwrap();
                GROUP_CULL_OFF.with(|c| c.set(false));
                (pm, crate::cpu_counters::take())
            };
            let (stepped, counted) = paint(true);
            let (whole, counted_whole) = paint(false);
            assert_eq!(counted_whole.items_walked, 5 * n as u64, "n = {n}: unculled, every item is walked");
            assert!(
                counted.items_walked >= n as u64 && counted.items_walked <= n as u64 + 4 * 20,
                "n = {n}: the pushes and the items of the few labels that reach the rect, got {}",
                counted.items_walked
            );
            assert!(stepped.data() == whole.data(), "n = {n}: the picture is the unculled one");
            assert_ne!(px(&stepped, 10, 5), [255, 255, 255, 255], "n = {n}: the first label is painted");
            walked.push(counted.items_walked);
        }
        assert!(walked[1] < 6 * walked[0], "four times the labels: {walked:?}");
        assert!(walked[1] - walked[0] <= 600 + 8, "600 more pushes, no more items: {walked:?}");
    }

    /// A1-07 (h): the rule itself, on the example of `frame_paints_nothing`. Extent
    /// (100, 40) 60 x 20, matrix translate(30, 0), walk offset (10, 0), shadow reach 3: the box
    /// that can be painted is x 117 .. 183, y 37 .. 63.
    /// Mutation: the reach left out gives x 120 .. 180, and the damage rect ending at 118 would
    /// step over a label whose halo reaches it; `+ tx` -> `- tx` gives 57 .. 123 and walks the
    /// frame under a rect it cannot reach.
    #[test]
    fn a_frame_is_stepped_over_only_when_its_moved_extent_and_its_halo_miss_the_cull() {
        use crate::solver3::display_list::FrameEntry;
        let push = DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 77 },
            initial_transform: azul_core::transform::ComputedTransform3D { m: turn_scale_move(0.0, 1.0, 30.0, 0.0) },
            bounds: wrect(100.0, 40.0, 60.0, 20.0),
        };
        let entry = FrameEntry {
            push: 0,
            end: 4,
            extent: rect(100.0, 40.0, 60.0, 20.0),
            self_contained: true,
            leaf: true,
            shadow_reach: 3.0,
            text_only: false,
        };
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let nothing = |entry: &FrameEntry, cull: WalkCull, clip: Option<AzRect>| {
            frame_paints_nothing(&push, entry, &cull, false, (400, 300), 1.0, clip, (10.0, 0.0), &[], &[], &state)
        };
        let damage = |w: f32| WalkCull::Damage(rect(0.0, 0.0, w, 100.0));
        assert!(nothing(&entry, damage(110.0), None), "117 is past a rect that ends at 110");
        assert!(!nothing(&entry, damage(118.0), None), "the halo's 3 px reach a rect that ends at 118");
        assert!(!nothing(&entry, damage(200.0), None));
        assert!(nothing(&entry, WalkCull::Damage(rect(0.0, 70.0, 400.0, 30.0)), None), "y 37 .. 63 is above a rect from 70");
        // A full repaint culls nothing outside groups; a view culls by its clip.
        assert!(!nothing(&entry, WalkCull::InGroups, None));
        assert!(nothing(&entry, WalkCull::ActiveClip, AzRect::from_xywh(0.0, 0.0, 110.0, 100.0)));
        assert!(!nothing(&entry, WalkCull::ActiveClip, AzRect::from_xywh(0.0, 0.0, 118.0, 100.0)));
        assert!(!nothing(&entry, WalkCull::ActiveClip, None), "no clip, nothing to cull by");
        // Not a leaf, or not self-contained: walked.
        assert!(!nothing(&FrameEntry { leaf: false, ..entry }, damage(110.0), None));
        assert!(!nothing(&FrameEntry { self_contained: false, ..entry }, damage(110.0), None));
        // The live matrix decides, not the list's: moved back by a value of translate(-80, 0),
        // the box is x 7 .. 73 and reaches the rect.
        let mut live = CpuRenderState::new(ScrollOffsetMap::new());
        live.transforms.insert(77, azul_core::transform::ComputedTransform3D { m: turn_scale_move(0.0, 1.0, -80.0, 0.0) });
        assert!(!frame_paints_nothing(&push, &entry, &damage(110.0), false, (400, 300), 1.0, None, (10.0, 0.0), &[], &[], &live));
    }

    /// A1-06 (a): the halos the frame's thread keeps are shared with a band thread, and come
    /// back with what the band kept. Mutation: a band thread that does not adopt the store
    /// finds nothing kept (and would draw every halo of its band on every frame); a store not
    /// restored loses the frame thread's halos after the first repaint in bands.
    #[test]
    fn a_band_thread_reads_and_keeps_the_halos_of_the_frames_thread() {
        use super::super::text_halo::{
            adopt_cpu_halos, keep_cpu_halo, kept_cpu_halo, restore_cpu_halos, share_cpu_halos, CpuHalo,
        };
        fn halo(width: u32) -> Option<std::sync::Arc<CpuHalo>> {
            Some(std::sync::Arc::new(CpuHalo { width, height: 1, offset: (0, 0), coverage: vec![7; width as usize] }))
        }
        let (mine, theirs) = (0x00A1_0001_u64, 0x00A1_0002_u64);
        keep_cpu_halo(mine, halo(3));
        let shared = share_cpu_halos();
        assert!(kept_cpu_halo(mine).is_none(), "moved out of this thread's memo while it is shared");
        let for_band = std::sync::Arc::clone(&shared);
        std::thread::spawn(move || {
            assert!(kept_cpu_halo(mine).is_none(), "a new thread has kept nothing");
            adopt_cpu_halos(Some(for_band));
            assert_eq!(kept_cpu_halo(mine).flatten().map(|h| h.width), Some(3), "the band reads the frame's halo");
            keep_cpu_halo(theirs, halo(5));
            adopt_cpu_halos(None);
        })
        .join()
        .expect("the band thread");
        restore_cpu_halos(shared);
        assert_eq!(kept_cpu_halo(mine).flatten().map(|h| h.width), Some(3), "back in this thread's memo");
        assert_eq!(kept_cpu_halo(theirs).flatten().map(|h| h.width), Some(5), "with what the band kept");
    }

    // ---- kept group surfaces ----------------------------------------------------------------

    use std::sync::Arc;

    /// A window (900 x 700) that is one view, and the view's list: a picture root frame (key 1,
    /// box 1000 x 800) around a ground, 320 cells, an upright label (key 2), and a label turned
    /// by its own matrix (key 3, 15 degrees unless the render state says otherwise) with text
    /// and a halo when there is a font. `edited` paints one cell in another colour (a tile that
    /// arrived, a label that was edited: another list).
    struct Scene {
        host: Arc<DisplayList>,
        child: Arc<DisplayList>,
    }

    fn zoom_scene(font: Option<(&ParsedFont, FontHash)>, content_offset: (f32, f32), edited: bool) -> Scene {
        let mut inner = vec![fill(0.0, 0.0, 1000.0, 800.0, [236, 233, 225])];
        for i in 0..20u8 {
            for j in 0..16u8 {
                let rgb = if edited && i == 4 && j == 3 { [255, 0, 255] } else { [i * 12, j * 15, 80 + i * 4 + j * 4] };
                inner.push(fill(f32::from(i) * 50.0 + 5.0, f32::from(j) * 50.0 + 5.0, 40.0, 40.0, rgb));
            }
        }
        inner.extend(label_frame(2, IDENTITY, 100.0, 100.0));
        inner.push(DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 3 },
            initial_transform: azul_core::transform::ComputedTransform3D { m: turn_scale_move(15.0, 1.0, 0.0, 0.0) },
            bounds: wrect(300.0, 200.0, 220.0, 44.0),
        });
        inner.push(fill(300.0, 200.0, 220.0, 44.0, [255, 255, 210]));
        inner.extend(haloed(halo(0.0, 2.0, [255, 255, 255]), label(font, "Hirschauer Steige", 22.0, 306.0, 230.0)));
        inner.push(DisplayListItem::PopReferenceFrame);
        let mut child = vec![DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 1 },
            initial_transform: azul_core::transform::ComputedTransform3D { m: IDENTITY },
            bounds: wrect(0.0, 0.0, 1000.0, 800.0),
        }];
        child.extend(inner);
        child.push(DisplayListItem::PopReferenceFrame);
        let host = vec![DisplayListItem::VirtualView {
            child_dom_id: azul_core::dom::DomId { inner: 1 },
            bounds: wrect(0.0, 0.0, 900.0, 700.0),
            clip_rect: wrect(0.0, 0.0, 900.0, 700.0),
            content_offset: LogicalPosition { x: content_offset.0, y: content_offset.1 },
        }];
        Scene { host: Arc::new(list(host)), child: Arc::new(list(child)) }
    }

    /// The render state of a frame of `scene`: the picture root scaled by `k` and moved by `t`,
    /// the turned label at `turned` degrees, kept surfaces in `retained` (if any).
    fn zoom_state(
        scene: &Scene,
        (k, t): (f32, (f32, f32)),
        turned: f32,
        retained: Option<&Arc<RetainedFrames>>,
        threads: usize,
    ) -> CpuRenderState {
        let mut state = CpuRenderState::new(ScrollOffsetMap::new())
            .with_virtual_view_display_lists(
                [(azul_core::dom::DomId { inner: 1 }, Arc::clone(&scene.child))].into_iter().collect(),
            )
            .with_root_display_list(Arc::clone(&scene.host))
            .with_paint_threads(threads);
        if let Some(retained) = retained {
            state = state.with_retained_frames(Arc::clone(retained));
        }
        let value = |m| azul_core::transform::ComputedTransform3D { m };
        state.transforms.insert(1, value(turn_scale_move(0.0, k, t.0, t.1)));
        state.transforms.insert(3, value(turn_scale_move(turned, 1.0, 0.0, 0.0)));
        state
    }

    /// One frame of `scene` as the backend paints it: the kept surfaces of other lists dropped,
    /// the view (the whole window) repainted, what was painted kept. Returns what was counted.
    fn zoom_frame(
        scene: &Scene,
        state: &CpuRenderState,
        pm: &mut AzulPixmap,
        (rr, fm): (&RendererResources, &FontManager<FontRef>),
    ) -> crate::cpu_counters::CpuFrameCounters {
        let mut gc = GlyphCache::new();
        let whole = [rect(0.0, 0.0, 900.0, 700.0)];
        crate::cpu_counters::reset();
        if let Some(retained) = &state.retained {
            refresh_retained_surfaces(state, rr, fm, &mut gc, 1.0).unwrap();
            retained.begin_frame(&[Arc::as_ptr(&scene.host) as usize, Arc::as_ptr(&scene.child) as usize]);
        }
        render_display_list_damaged(&scene.host, pm, 1.0, rr, fm, &mut gc, state, &whole).unwrap();
        if let Some(retained) = &state.retained {
            retained.commit();
        }
        crate::cpu_counters::take()
    }

    /// The same frame painted afresh: no kept surface, one pass, a new target.
    fn fresh_frame(
        scene: &Scene,
        zoom: (f32, (f32, f32)),
        turned: f32,
        resources: (&RendererResources, &FontManager<FontRef>),
    ) -> AzulPixmap {
        let mut pm = AzulPixmap::new(900, 700).unwrap();
        pm.fill(255, 255, 255, 255);
        let _ = zoom_frame(scene, &zoom_state(scene, zoom, turned, None, 1), &mut pm, resources);
        pm
    }

    /// A1-08 (b): a zoom step composites the picture it kept, and the frame is the one painted
    /// afresh, pixel for pixel; on one thread and in bands.
    ///
    /// The view is 900 x 700 and the picture root's surface 1000 x 800. Step 1 (k = 1.5) paints
    /// the part that lands, 600 x 467 px and 64 more on each side: [0, 0, 664, 531]. Step 2
    /// (k = 1.8) reads 500 x 389 px of it (and 2 more): kept, composited, nothing painted.
    /// Step 3 (k = 1.2, moved by (-100, -50)) reads x 81 .. 836, past what was painted: painted
    /// again ([19, 0, 898, 689]) and kept. Step 4 (k = 1.3, same move) reads x 74 .. 772,
    /// y 36 .. 579: kept.
    /// Mutations: a hit that ignores `needed` (rule 7) composites the transparent rest of the
    /// surface at step 3; a commit that drops the bands' parts but the last makes step 2 a miss
    /// in bands; compositing a kept surface with the matrix it was painted under shows step 1's
    /// picture at step 2.
    #[test]
    fn a_zoom_step_composites_the_kept_picture_and_it_is_the_picture_painted_afresh() {
        let font = test_font();
        let (rr, fm, font_hash) = resources(font.as_ref());
        let scene = zoom_scene(font.as_ref().zip(font_hash), (0.0, 0.0), false);
        for threads in [1usize, 4] {
            let bands = if threads == 1 { 1 } else { 3 };
            let retained = Arc::new(RetainedFrames::with_max_bytes(64 << 20));
            let mut pm = AzulPixmap::new(900, 700).unwrap();
            pm.fill(255, 255, 255, 255);
            let steps: [((f32, (f32, f32)), bool); 4] = [
                ((1.5, (0.0, 0.0)), false),
                ((1.8, (0.0, 0.0)), true),
                ((1.2, (-100.0, -50.0)), false),
                ((1.3, (-100.0, -50.0)), true),
            ];
            for (step, (zoom, kept)) in steps.into_iter().enumerate() {
                let state = zoom_state(&scene, zoom, 15.0, Some(&retained), threads);
                let counted = zoom_frame(&scene, &state, &mut pm, (&rr, &fm));
                let what = format!("{threads} thread(s), step {}", step + 1);
                if kept {
                    assert_eq!((counted.retained_hits, counted.retained_misses), (bands, 0), "{what}: composited: {counted:?}");
                    assert_eq!(counted.groups_opened, 0, "{what}: no surface is made: {counted:?}");
                    assert!(counted.items_painted <= 2 * bands, "{what}: nothing of the view's list is painted: {counted:?}");
                } else {
                    assert_eq!((counted.retained_hits, counted.retained_misses), (0, bands), "{what}: painted: {counted:?}");
                    assert!(counted.items_painted > 100, "{what}: the view's list is painted: {counted:?}");
                }
                let fresh = fresh_frame(&scene, zoom, 15.0, (&rr, &fm));
                assert_eq!(differing_pixels(&pm, &fresh), 0, "{what}: the frame is the one painted afresh");
                let coloured = pm.data().chunks_exact(4).filter(|p| p[..3] != [255, 255, 255]).count();
                assert!(coloured > 300_000, "{what}: the view shows the picture ({coloured} px)");
            }
            assert_eq!(retained.len(), 1, "one surface for the one frame");
            assert_eq!(retained.bytes(), 1000 * 800 * 4);
        }
    }

    /// A1-08 (b), the invalidation rules, each against the frame painted afresh. After a first
    /// step that keeps the picture:
    /// - rule 5, a value inside the frame: the turned label's own matrix is written (15 -> 40
    ///   degrees). The picture is painted again, and the surface kept for it replaces the old.
    /// - rule 4, the phase: the view pans by whole pixels in eight directions: composited, the
    ///   picture where it lands now; by half a pixel: painted.
    /// - rule 1, the list: a cell changes colour (a tile arrives) or a label's fill does (an
    ///   edit): the surface is repaired in the boxes that differ and moves to the new list; a
    ///   list of another length drops it. The old list is let go of either way.
    /// - the control: the same frame again is composited.
    /// Mutations: `values` left out of the key shows the label at 15 degrees; the phase left
    /// out composites the picture of whole pixels at half a pixel; a hit composited with the
    /// geometry it was painted under shows the picture where it was before the pan; a key without the list (or an entry that does not hold
    /// it) shows the old cell.
    #[test]
    fn a_kept_picture_is_painted_again_when_what_it_shows_changed() {
        let font = test_font();
        let (rr, fm, font_hash) = resources(font.as_ref());
        let font = font.as_ref().zip(font_hash);
        let zoom = (1.5, (0.0, 0.0));
        let kept_scene = || {
            let scene = zoom_scene(font, (0.0, 0.0), false);
            let retained = Arc::new(RetainedFrames::with_max_bytes(64 << 20));
            let mut pm = AzulPixmap::new(900, 700).unwrap();
            pm.fill(255, 255, 255, 255);
            let first = zoom_frame(&scene, &zoom_state(&scene, zoom, 15.0, Some(&retained), 1), &mut pm, (&rr, &fm));
            assert_eq!((first.retained_hits, first.retained_misses, retained.len()), (0, 1, 1));
            (scene, retained, pm)
        };

        // The control: nothing changed.
        let (scene, retained, mut pm) = kept_scene();
        let again = zoom_frame(&scene, &zoom_state(&scene, zoom, 15.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!((again.retained_hits, again.retained_misses), (1, 0));
        assert_eq!(differing_pixels(&pm, &fresh_frame(&scene, zoom, 15.0, (&rr, &fm))), 0);

        // Rule 5: a value inside the frame.
        let (scene, retained, mut pm) = kept_scene();
        let turned = zoom_frame(&scene, &zoom_state(&scene, zoom, 40.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!((turned.retained_hits, turned.retained_misses), (0, 1), "the label turned: {turned:?}");
        assert_eq!(turned.retained_invalidations, 1, "the surface of 15 degrees is replaced: {turned:?}");
        let fresh = fresh_frame(&scene, zoom, 40.0, (&rr, &fm));
        assert_eq!(differing_pixels(&pm, &fresh), 0, "the label at 40 degrees");
        assert!(differing_pixels(&fresh, &fresh_frame(&scene, zoom, 15.0, (&rr, &fm))) > 500, "harness: the turn shows");
        assert_eq!(retained.len(), 1);

        // Rule 4: the phase. A pan by whole device pixels, in the eight directions of a shift,
        // keeps the picture: composited where it lands now, nothing of the view's list painted
        // (a pan during a zoom). A pan by half a pixel is another picture.
        for by in [(12.0_f32, 0.0_f32), (-12.0, 0.0), (0.0, 7.0), (0.0, -7.0), (5.0, 3.0), (-5.0, 3.0), (5.0, -3.0), (-5.0, -3.0)] {
            let (scene, retained, mut pm) = kept_scene();
            let panned = Scene { host: zoom_scene(font, by, false).host, child: Arc::clone(&scene.child) };
            let pan = zoom_frame(&panned, &zoom_state(&panned, zoom, 15.0, Some(&retained), 1), &mut pm, (&rr, &fm));
            assert_eq!((pan.retained_hits, pan.retained_misses), (1, 0), "panned by {by:?}: {pan:?}");
            assert!(pan.items_painted <= 2, "panned by {by:?}: nothing of the view's list is painted: {pan:?}");
            let fresh = fresh_frame(&panned, zoom, 15.0, (&rr, &fm));
            assert_eq!(differing_pixels(&pm, &fresh), 0, "panned by {by:?}: the picture where it lands now");
            assert!(differing_pixels(&fresh, &fresh_frame(&scene, zoom, 15.0, (&rr, &fm))) > 500, "harness: the pan shows");
        }
        let (scene, retained, mut pm) = kept_scene();
        let panned = Scene { host: zoom_scene(font, (-12.5, 0.0), false).host, child: Arc::clone(&scene.child) };
        let pan = zoom_frame(&panned, &zoom_state(&panned, zoom, 15.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!((pan.retained_hits, pan.retained_misses), (0, 1), "half a pixel: another phase: {pan:?}");
        assert_eq!(differing_pixels(&pm, &fresh_frame(&panned, zoom, 15.0, (&rr, &fm))), 0);

        // Rule 1: the list. A list that replaces the kept one and differs in one item directly
        // in the frame (a tile that arrived): the surface is repaired in that item's box
        // ([203, 153, 247, 197] for the cell at (205, 155) 40 x 40), moves to the new list, and
        // the frame is composited from it.
        let (scene, retained, mut pm) = kept_scene();
        assert_eq!(Arc::strong_count(&scene.child), 2, "the kept surface holds its list");
        let edited = Scene { host: Arc::clone(&scene.host), child: zoom_scene(font, (0.0, 0.0), true).child };
        let edit = zoom_frame(&edited, &zoom_state(&edited, zoom, 15.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!((edit.retained_patched, edit.retained_invalidations), (1, 0), "repaired: {edit:?}");
        assert_eq!((edit.retained_hits, edit.retained_misses), (1, 0), "and composited: {edit:?}");
        assert!(edit.items_painted < 40, "only what reaches the cell's box is painted: {edit:?}");
        assert_eq!(Arc::strong_count(&scene.child), 1, "the old list is let go of");
        let fresh = fresh_frame(&edited, zoom, 15.0, (&rr, &fm));
        assert_eq!(differing_pixels(&pm, &fresh), 0, "the edited cell");
        // The cell (4, 3) is at (205, 155) 40 x 40 of the picture, x 307 .. 367 on screen.
        assert_eq!(px(&pm, 330, 250), [255, 0, 255, 255]);
        assert_eq!(retained.held_lists(), vec![Arc::as_ptr(&edited.child) as usize]);

        // An item inside an inner frame (a label that was edited): the box where that frame
        // can paint through its matrix is repaired.
        let (scene, retained, mut pm) = kept_scene();
        let mut items = scene.child.items.clone();
        let label_push = items
            .iter()
            .position(|i| matches!(i, DisplayListItem::PushReferenceFrame { transform_key, .. } if transform_key.id == 3))
            .expect("the turned label");
        items[label_push + 1] = fill(300.0, 200.0, 220.0, 44.0, [120, 255, 120]);
        let relabelled = Scene { host: Arc::clone(&scene.host), child: Arc::new(list(items)) };
        let edit = zoom_frame(&relabelled, &zoom_state(&relabelled, zoom, 15.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!((edit.retained_patched, edit.retained_hits, edit.retained_misses), (1, 1, 0), "{edit:?}");
        let fresh = fresh_frame(&relabelled, zoom, 15.0, (&rr, &fm));
        assert_eq!(differing_pixels(&pm, &fresh), 0, "the edited label");
        assert!(differing_pixels(&fresh, &fresh_frame(&scene, zoom, 15.0, (&rr, &fm))) > 500, "harness: the edit shows");

        // A list of another length does not pair: the surface is dropped and the frame painted.
        let (scene, retained, mut pm) = kept_scene();
        let mut items = scene.child.items.clone();
        let last = items.len() - 1;
        items.insert(last, fill(700.0, 500.0, 60.0, 60.0, [0, 0, 0]));
        let longer = Scene { host: Arc::clone(&scene.host), child: Arc::new(list(items)) };
        let edit = zoom_frame(&longer, &zoom_state(&longer, zoom, 15.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!((edit.retained_patched, edit.retained_invalidations), (0, 1), "dropped: {edit:?}");
        assert_eq!((edit.retained_hits, edit.retained_misses), (0, 1), "painted: {edit:?}");
        assert_eq!(Arc::strong_count(&scene.child), 1, "the old list is let go of");
        assert_eq!(differing_pixels(&pm, &fresh_frame(&longer, zoom, 15.0, (&rr, &fm))), 0);
    }

    /// A1-08 (b), rule 6: what text is painted with is part of the key.
    #[test]
    fn the_settings_a_picture_is_painted_with_are_part_of_its_key() {
        let fm = empty_font_manager();
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let mut smooth = GlyphCache::new();
        let _ = smooth.set_text_aa(TextAaSettings { mode: TextAa::Grayscale, ..TextAaSettings::default() });
        let mut stripes = GlyphCache::new();
        let _ = stripes.set_text_aa(TextAaSettings { mode: TextAa::Lcd, ..TextAaSettings::default() });
        assert_eq!(paint_settings(&smooth, &fm, &state), paint_settings(&smooth, &fm, &state));
        assert_ne!(paint_settings(&smooth, &fm, &state), paint_settings(&stripes, &fm, &state));
    }

    /// A1-15: a fill bound to a colour key inside a view is painted in the key's live value,
    /// as in the window's own list (a colour fade inside a view painted the colour the list
    /// was built with until the view rendered again). Without a value, the list's colour.
    /// Mutation: `bind_colors` false for a view's list paints (200, 0, 0) in both cases.
    #[test]
    fn a_bound_colour_inside_a_view_is_painted_in_its_live_value() {
        let mut child = list(vec![fill(0.0, 0.0, 50.0, 40.0, [200, 0, 0])]);
        child.color_bindings = vec![Some(azul_core::resources::ColorKey { id: 9 })];
        let host = list(vec![DisplayListItem::VirtualView {
            child_dom_id: azul_core::dom::DomId { inner: 1 },
            bounds: wrect(10.0, 10.0, 50.0, 40.0),
            clip_rect: wrect(10.0, 10.0, 50.0, 40.0),
            content_offset: LogicalPosition { x: 0.0, y: 0.0 },
        }]);
        let child = Arc::new(child);
        let with = |live: Option<ColorU>| {
            let mut state = CpuRenderState::new(ScrollOffsetMap::new()).with_virtual_view_display_lists(
                [(azul_core::dom::DomId { inner: 1 }, Arc::clone(&child))].into_iter().collect(),
            );
            if let Some(colour) = live {
                state.colors.insert(9, colour);
            }
            full(&host, &state, (80, 60)).0
        };
        assert_eq!(px(&with(Some(ColorU { r: 0, g: 0, b: 250, a: 255 })), 30, 30), [0, 0, 250, 255]);
        assert_eq!(px(&with(None), 30, 30), [200, 0, 0, 255]);
        assert_eq!(px(&with(None), 5, 5), [255, 255, 255, 255], "outside the view");
    }

    // ---- turned text without a surface ------------------------------------------------------

    /// A view (400 x 300) of three labels turned by their own matrices (20, -35 and 90
    /// degrees), each a fill with haloed text, on a ground.
    fn turned_labels(font: (&ParsedFont, FontHash)) -> (DisplayList, CpuRenderState) {
        let mut child = vec![fill(0.0, 0.0, 400.0, 300.0, [236, 233, 225])];
        for (key, degrees, x, y) in [(11usize, 20.0_f32, 60.0_f32, 60.0_f32), (12, -35.0, 200.0, 150.0), (13, 90.0, 330.0, 40.0)] {
            child.push(DisplayListItem::PushReferenceFrame {
                transform_key: azul_core::resources::TransformKey { id: key },
                initial_transform: azul_core::transform::ComputedTransform3D { m: turn_scale_move(degrees, 1.0, 0.0, 0.0) },
                bounds: wrect(x, y, 150.0, 30.0),
            });
            child.push(fill(x, y, 150.0, 30.0, [255, 255, 210]));
            child.extend(haloed(halo(0.0, 2.0, [255, 255, 255]), label(Some(font), "Ammergasse", 20.0, x + 6.0, y + 22.0)));
            child.push(DisplayListItem::PopReferenceFrame);
        }
        let host = list(vec![DisplayListItem::VirtualView {
            child_dom_id: azul_core::dom::DomId { inner: 1 },
            bounds: wrect(0.0, 0.0, 400.0, 300.0),
            clip_rect: wrect(0.0, 0.0, 400.0, 300.0),
            content_offset: LogicalPosition { x: 0.0, y: 0.0 },
        }]);
        let state = CpuRenderState::new(ScrollOffsetMap::new()).with_virtual_view_display_lists(
            [(azul_core::dom::DomId { inner: 1 }, Arc::new(list(child)))].into_iter().collect(),
        );
        (host, state)
    }

    /// The box of the pixels of `p` that are not the ground: `(x0, y0, x1, y1)`, inclusive.
    fn ink_box(p: &AzulPixmap) -> (u32, u32, u32, u32) {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..p.height {
            for x in 0..p.width {
                if px(p, x, y)[..3] != [236, 233, 225] {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0, y0, x1, y1)
    }

    /// A1-20 (d): the turned labels of a view are drawn through their matrices, without a
    /// surface: no group is opened where the group path opens one a label, a second paint
    /// finds every picture kept (no halo drawn), a damage rect across a label repaints the
    /// pixels a full paint gives (damaged == full), and the labels stand where the group path
    /// puts them (the box of everything painted agrees within 2 px: the anchor is rounded to a
    /// whole pixel and the edges are sharper).
    /// Mutations: the matrix applied about the window's origin instead of the frame's moves
    /// the labels far out of that box; a blit not cut to the clip paints outside the damage
    /// rect (damaged != full); `new_custom(a, b, c, d)` (the shears swapped) mirrors the turn.
    #[test]
    fn a_turned_label_of_a_view_is_drawn_without_a_surface() {
        let Some(font) = test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let (rr, fm, font_hash) = resources(Some(&font));
        let (host, state) = turned_labels((&font, font_hash.expect("the font's hash")));
        let paint = || {
            let mut pm = AzulPixmap::new(400, 300).unwrap();
            pm.fill(255, 255, 255, 255);
            let mut gc = GlyphCache::new();
            crate::cpu_counters::reset();
            render_display_list_with_state(&host, &mut pm, 1.0, &rr, &fm, &mut gc, &state).unwrap();
            (pm, crate::cpu_counters::take())
        };
        let (direct, counted) = paint();
        assert_eq!(counted.groups_opened, 0, "no surface for a turned label: {counted:?}");
        assert_eq!(counted.text_runs, 3, "the three runs are drawn: {counted:?}");
        let (again, counted_again) = paint();
        assert_eq!(counted_again.halos_drawn, 0, "the pictures are kept: {counted_again:?}");
        assert!(direct.data() == again.data(), "and a kept picture is the picture drawn");

        TURNED_TEXT_IN_GROUPS.with(|g| g.set(true));
        let (grouped, counted_grouped) = paint();
        TURNED_TEXT_IN_GROUPS.with(|g| g.set(false));
        assert_eq!(counted_grouped.groups_opened, 3, "the path before: a surface a label");
        let (a, b) = (ink_box(&direct), ink_box(&grouped));
        for (name, direct_edge, grouped_edge) in [("left", a.0, b.0), ("top", a.1, b.1), ("right", a.2, b.2), ("bottom", a.3, b.3)] {
            assert!(direct_edge.abs_diff(grouped_edge) <= 2, "{name}: {direct_edge} drawn directly, {grouped_edge} through a group");
        }
        let painted = direct.data().chunks_exact(4).filter(|p| p[..3] != [236, 233, 225]).count();
        assert!(painted > 9000, "three labels of 150 x 30 px are there ({painted} px)");

        // A damage rect across the first label and a corner of the second, on the full frame.
        let mut damaged = direct.clone_pixmap();
        let mut gc = GlyphCache::new();
        let strip = [rect(100.0, 60.0, 130.0, 90.0)];
        render_display_list_damaged(&host, &mut damaged, 1.0, &rr, &fm, &mut gc, &state.with_paint_threads(1), &strip).unwrap();
        assert_eq!(differing_pixels(&damaged, &direct), 0, "damaged == full");
    }

    /// A1-21: a full repaint goes through the walk that repaints damage, in bands: the frame of
    /// `render_display_list_whole` on four threads is the frame of the plain full walk, pixel
    /// for pixel, it clears nothing (the caller filled the frame), and a list without a filter
    /// that only layers apply does not need the layered compositor.
    /// Mutation: a band that clears its rect although the frame was filled counts the
    /// cleared pixels; a whole-frame rect of the logical size at a scale factor of 2 leaves
    /// three quarters of the frame unpainted.
    #[test]
    fn a_full_repaint_in_bands_is_the_full_walk() {
        let font = test_font();
        let (rr, fm, font_hash) = resources(font.as_ref());
        let dl = seam_page(font.as_ref().zip(font_hash));
        assert!(!full_repaint_needs_layers(&dl));
        for dpi in [1.0_f32, 2.0] {
            let paint = |banded: bool| {
                let state = CpuRenderState::new(ScrollOffsetMap::new()).with_paint_threads(4);
                let mut pm = AzulPixmap::new(900, 700).unwrap();
                pm.fill(255, 255, 255, 255);
                let mut gc = GlyphCache::new();
                crate::cpu_counters::reset();
                if banded {
                    render_display_list_whole(&dl, &mut pm, dpi, &rr, &fm, &mut gc, &state).unwrap();
                } else {
                    render_display_list_with_state(&dl, &mut pm, dpi, &rr, &fm, &mut gc, &state).unwrap();
                }
                (pm, crate::cpu_counters::take())
            };
            let (banded, counted) = paint(true);
            let (plain, _) = paint(false);
            assert_eq!((counted.bands, counted.px_cleared), (3, 0), "dpi {dpi}: {counted:?}");
            assert_eq!(differing_pixels(&banded, &plain), 0, "dpi {dpi}: the banded full repaint is the full walk");
        }
    }

    /// A1-23: a run that a band seam (or the edge of a damage rect) cuts where its backdrop is
    /// opaque on one side only is the same text on both sides. A half-opaque group across
    /// row 234 of the 900 x 700 page, with an opaque fill over its upper part alone (rows 200
    /// .. 234) and a run of 36 px with its baseline at row 246: above the seam the run stands
    /// on the fill, below it on the group's cleared pixels. In bands, in one pass, and as one
    /// pass over the lower band alone on top of the full frame, the pixels are the same.
    /// Mutation: the choice made from the pixels under the clipped run paints LCD text above
    /// the seam in bands and grayscale text in one pass.
    #[test]
    fn a_run_cut_where_its_backdrop_changes_is_the_same_text_on_both_sides() {
        let font = test_font();
        let (rr, fm, font_hash) = resources(font.as_ref());
        let mut items = vec![fill(0.0, 0.0, 900.0, 700.0, [236, 233, 225])];
        items.push(DisplayListItem::PushOpacity {
            bounds: wrect(100.0, 180.0, 400.0, 120.0),
            opacity: 0.5,
            opacity_key: None,
            mix_blend_mode: Default::default(),
        });
        items.push(fill(100.0, 200.0, 400.0, 34.0, [250, 250, 250]));
        items.extend(label(font.as_ref().zip(font_hash), "Hagelloch", 36.0, 120.0, 246.0));
        items.push(DisplayListItem::PopOpacity);
        let dl = list(items);
        let (banded, counted) = repaint(&dl, (&rr, &fm), (900, 700), 1.0, 4);
        let (single, _) = repaint(&dl, (&rr, &fm), (900, 700), 1.0, 1);
        assert_eq!(counted.bands, 3, "the seams are at rows 234 and 468");
        assert_eq!(differing_pixels(&banded, &single), 0, "bands == one pass");
        // A damage rect that is the lower band alone, on the full frame: nothing changes.
        let mut damaged = single.clone_pixmap();
        let mut gc = GlyphCache::new();
        let state = CpuRenderState::new(ScrollOffsetMap::new()).with_paint_threads(1);
        render_display_list_damaged(&dl, &mut damaged, 1.0, &rr, &fm, &mut gc, &state, &[rect(0.0, 234.0, 900.0, 234.0)]).unwrap();
        assert_eq!(differing_pixels(&damaged, &single), 0, "damaged == full");
    }

    /// A2-gpu-07: a scrollbar is painted at its opacity, not in full or not at all. A black
    /// thumb (20, 10) 10 x 40 on white with its opacity key at 0.5 is mid grey, 128 (127 or
    /// 128 by the rounding of the blend); at 1 black; without a value black; at 0 not painted.
    /// Mutation: the colours not faded paint black at 0.5.
    #[test]
    fn a_scrollbar_is_painted_at_its_opacity() {
        use crate::solver3::display_list::ScrollbarDrawInfo;
        let black = ColorU { r: 0, g: 0, b: 0, a: 255 };
        let clear = ColorU { r: 0, g: 0, b: 0, a: 0 };
        let dl = list(vec![DisplayListItem::ScrollBarStyled {
            info: Box::new(ScrollbarDrawInfo {
                bounds: wrect(20.0, 10.0, 10.0, 40.0),
                orientation: azul_core::dom::ScrollbarOrientation::Vertical,
                track_bounds: wrect(20.0, 10.0, 10.0, 40.0),
                track_color: clear,
                thumb_bounds: wrect(20.0, 10.0, 10.0, 40.0),
                thumb_color: black,
                thumb_border_radius: BorderRadius::default(),
                button_decrement_bounds: None,
                button_increment_bounds: None,
                button_color: clear,
                opacity_key: Some(azul_core::resources::OpacityKey { id: 5 }),
                thumb_transform_key: None,
                thumb_initial_transform: azul_core::transform::ComputedTransform3D { m: IDENTITY },
                hit_id: None,
                clip_to_container_border: false,
                container_border_radius: BorderRadius::default(),
                visibility: Default::default(),
            }),
        }]);
        let at = |opacity: Option<f32>| {
            let mut state = CpuRenderState::new(ScrollOffsetMap::new());
            if let Some(opacity) = opacity {
                state.opacities.insert(5, opacity);
            }
            px(&full(&dl, &state, (60, 60)).0, 25, 30)
        };
        let half = at(Some(0.5));
        assert!((126..=129).contains(&half[0]) && half[0] == half[1] && half[1] == half[2], "half black on white: {half:?}");
        assert_eq!(at(Some(1.0)), [0, 0, 0, 255]);
        assert_eq!(at(None), [0, 0, 0, 255], "no value: the bar shows");
        assert_eq!(at(Some(0.0)), [255, 255, 255, 255], "faded out");
    }

    /// A1-25: the blurred box shadows the frame's thread keeps are shared with a band thread,
    /// and come back with what the band kept. Mutation: a band thread that does not adopt the
    /// cache finds nothing kept and blurs every shadow of its band on every frame.
    #[test]
    fn a_band_thread_reads_and_keeps_the_box_shadows_of_the_frames_thread() {
        fn entry(w: u32) -> ShadowCacheEntry {
            ShadowCacheEntry { data: Arc::new(vec![9; (w * 4) as usize]), w, h: 1 }
        }
        fn keep(key: u64, w: u32) {
            with_shadow_cache(|c| {
                c.2 += (w * 4) as usize;
                c.0.insert(key, entry(w));
                c.1.push_back(key);
            });
        }
        let kept = |key: u64| with_shadow_cache(|c| c.0.get(&key).map(|e| e.w));
        keep(71, 3);
        let shared = share_shadow_cache();
        assert_eq!(kept(71), None, "moved out of this thread's cache while it is shared");
        let for_band = Arc::clone(&shared);
        std::thread::spawn(move || {
            adopt_shadow_cache(Some(for_band));
            assert_eq!(with_shadow_cache(|c| c.0.get(&71).map(|e| e.w)), Some(3), "the band reads the frame's shadow");
            keep(72, 5);
            adopt_shadow_cache(None);
        })
        .join()
        .expect("the band thread");
        restore_shadow_cache(shared);
        assert_eq!((kept(71), kept(72)), (Some(3), Some(5)), "back, with what the band kept");
        assert_eq!(with_shadow_cache(|c| c.2), 32, "and the bytes of both");
    }

    /// A2-gpu-06: an opacity group fades everything it paints, not only what lies in its
    /// node's box. A half-opaque group with the box (10, 10) 40 x 40 and a green fill
    /// (10, 10) 100 x 40 that overflows it to x = 110: inside the box and past it the fill is
    /// half green over white, (128, 255, 128); past the box it used to be painted in full.
    /// Mutation: the group's region taken from its box paints (0, 255, 0) at x = 80.
    #[test]
    fn an_opacity_group_fades_what_overflows_its_box() {
        let dl = list(vec![
            DisplayListItem::PushOpacity {
                bounds: wrect(10.0, 10.0, 40.0, 40.0),
                opacity: 0.5,
                opacity_key: None,
                mix_blend_mode: Default::default(),
            },
            fill(10.0, 10.0, 100.0, 40.0, [0, 255, 0]),
            DisplayListItem::PopOpacity,
        ]);
        assert_eq!(dl.frame_index().opacity_extent(0), Some(rect(10.0, 10.0, 100.0, 40.0)));
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let (painted, _) = full(&dl, &state, (150, 80));
        for x in [20, 80, 105] {
            let [r, g, b, a] = px(&painted, x, 30);
            assert!((120..=136).contains(&r) && g == 255 && (120..=136).contains(&b) && a == 255, "x = {x}: half green over white, got {:?}", [r, g, b, a]);
        }
        assert_eq!(px(&painted, 115, 30), [255, 255, 255, 255], "past the fill");
        // A group whose content stays in its box keeps no extent of its own.
        let inside = list(vec![
            DisplayListItem::PushOpacity {
                bounds: wrect(10.0, 10.0, 40.0, 40.0),
                opacity: 0.5,
                opacity_key: None,
                mix_blend_mode: Default::default(),
            },
            fill(15.0, 15.0, 20.0, 20.0, [0, 255, 0]),
            DisplayListItem::PopOpacity,
        ]);
        assert_eq!(inside.frame_index().opacity_extent(0), None);
    }

    /// A solid border of `width` px in black around the box `(0, 0) w x h`, `radius` at every
    /// corner, as the display list carries it.
    fn ring(w: f32, h: f32, width: f32, radius: azul_css::props::basic::pixel::PixelValue) -> DisplayListItem {
        use azul_css::{css::CssPropertyValue, props::{basic::pixel::PixelValue, style as s}};
        use crate::solver3::display_list::{StyleBorderColors, StyleBorderStyles, StyleBorderWidths};
        let (inner, black, solid) = (PixelValue::px(width), ColorU { r: 0, g: 0, b: 0, a: 255 }, s::BorderStyle::Solid);
        DisplayListItem::Border {
            bounds: wrect(0.0, 0.0, w, h),
            widths: StyleBorderWidths {
                top: Some(CssPropertyValue::Exact(s::LayoutBorderTopWidth { inner })),
                right: Some(CssPropertyValue::Exact(s::LayoutBorderRightWidth { inner })),
                bottom: Some(CssPropertyValue::Exact(s::LayoutBorderBottomWidth { inner })),
                left: Some(CssPropertyValue::Exact(s::LayoutBorderLeftWidth { inner })),
            },
            colors: StyleBorderColors {
                top: Some(CssPropertyValue::Exact(s::StyleBorderTopColor { inner: black })),
                right: Some(CssPropertyValue::Exact(s::StyleBorderRightColor { inner: black })),
                bottom: Some(CssPropertyValue::Exact(s::StyleBorderBottomColor { inner: black })),
                left: Some(CssPropertyValue::Exact(s::StyleBorderLeftColor { inner: black })),
            },
            styles: StyleBorderStyles {
                top: Some(CssPropertyValue::Exact(s::StyleBorderTopStyle { inner: solid })),
                right: Some(CssPropertyValue::Exact(s::StyleBorderRightStyle { inner: solid })),
                bottom: Some(CssPropertyValue::Exact(s::StyleBorderBottomStyle { inner: solid })),
                left: Some(CssPropertyValue::Exact(s::StyleBorderLeftStyle { inner: solid })),
            },
            border_radius: s::StyleBorderRadius { top_left: radius, top_right: radius, bottom_left: radius, bottom_right: radius },
        }
    }

    /// A1-30: `border-radius: 50%` on a box that is not square is an ellipse. A 100 x 40 box
    /// with a 2 px border: the horizontal radius is 50 (half the width), the vertical one 20
    /// (half the height), so the outer edge is the ellipse `((x-50)/50)^2 + ((y-20)/20)^2 = 1`
    /// and the inner one the ellipse of 48 by 18 about the same centre. At x = 75..76 the outer
    /// edge is at y = 20 - 20 * sqrt(1 - 0.25) = 2.68 (2.92 at x = 76) and the inner one at
    /// y = 20 - 18 * sqrt(1 - (25/48)^2) = 4.63: the pixel (75, 3), y in 3..4, lies wholly in
    /// the ring and is black; the pixel (75, 1), y in 1..2, wholly above it and is the page.
    /// Mutation: the vertical radii resolved against the WIDTH again (`radius_y` from
    /// `bounds.0.size.width` in the `Border` arm) gives 50 by 50, which fits as 20 by 20: a
    /// stadium whose top edge is straight from x = 20 to 80, (75, 1) black and (75, 3) white.
    /// So does `rr.normalize_radius()` put back (50 by 20 becomes 20 by 8).
    #[test]
    fn a_percentage_border_radius_is_an_ellipse_on_a_box_that_is_not_square() {
        let dl = list(vec![ring(100.0, 40.0, 2.0, azul_css::props::basic::pixel::PixelValue::percent(50.0))]);
        let (p, _) = full(&dl, &CpuRenderState::new(ScrollOffsetMap::new()), (100, 40));
        assert_eq!(px(&p, 75, 3), [0, 0, 0, 255], "in the ring of the ellipse");
        assert_eq!(px(&p, 75, 1), [255, 255, 255, 255], "above the ellipse, under a stadium's edge");
        // The same by symmetry at the three other quadrants.
        for (x, y) in [(24, 3), (75, 36), (24, 36)] {
            assert_eq!(px(&p, x, y), [0, 0, 0, 255], "in the ring at ({x}, {y})");
        }
        for (x, y) in [(24, 1), (75, 38), (24, 38)] {
            assert_eq!(px(&p, x, y), [255, 255, 255, 255], "outside the ellipse at ({x}, {y})");
        }
        // A length is the same on both axes: 10 px corners are circles. The pixel (1, 1) is
        // outside the circle about (10, 10) (its nearest corner (2, 2) is 11.3 px away) and
        // (2, 5), x in 2..3, is past the inner circle of 8 px (its nearest point to (10, 10),
        // (3, 6), is 8.06 px away) and inside the outer one (its farthest, (2, 5), 9.43 px).
        let dl = list(vec![ring(100.0, 40.0, 2.0, azul_css::props::basic::pixel::PixelValue::px(10.0))]);
        let (p, _) = full(&dl, &CpuRenderState::new(ScrollOffsetMap::new()), (100, 40));
        assert_eq!(px(&p, 1, 1), [255, 255, 255, 255]);
        assert_eq!(px(&p, 2, 5), [0, 0, 0, 255]);
    }

    /// A1-30: corner radii that do not fit are scaled by the CSS rule (horizontal radii of an
    /// edge's two corners against the WIDTH, vertical ones against the HEIGHT), not by AGG's.
    /// A red 100 x 40 box.
    /// - 30 px at the two TOP corners: 60 <= 100 along the top edge and 30 <= 40 down each
    ///   side, so they stay 30 px. The pixel (7, 7) is outside the circle about (30, 30): its
    ///   nearest corner (8, 8) is 22 * 1.414 = 31.1 px from the centre. AGG's rule held the
    ///   60 against the height and drew 20 px corners, with (7, 7) inside (its farthest corner
    ///   is 13 * 1.414 = 18.4 px from (20, 20)).
    /// - 30 px at the two LEFT corners: 60 > 40 down the left edge, so both become 20 px and
    ///   meet at (0, 20). The pixel (10, 4) is inside the circle about (20, 20): its farthest
    ///   corner (10, 4) is sqrt(100 + 256) = 18.9 px away. AGG's rule never looked at that
    ///   edge and left two 30 px arcs crossing, with (10, 4) outside the one about (30, 30)
    ///   (its nearest corner (11, 5) is 31.4 px away).
    /// Mutation: `rr.normalize_radius()` in place of the `fit` loop fails both.
    #[test]
    fn corner_radii_that_do_not_fit_are_scaled_by_the_css_rule() {
        let state = CpuRenderState::new(ScrollOffsetMap::new());
        let red = ColorU { r: 255, g: 0, b: 0, a: 255 };
        let boxed = |border_radius: BorderRadius| {
            let dl = list(vec![DisplayListItem::Rect { bounds: wrect(0.0, 0.0, 100.0, 40.0), color: red, border_radius }]);
            full(&dl, &state, (100, 40)).0
        };
        let top = boxed(BorderRadius { top_left: 30.0, top_right: 30.0, bottom_left: 0.0, bottom_right: 0.0 });
        assert_eq!(px(&top, 7, 7), [255, 255, 255, 255], "30 px corners on the top edge stay 30 px");
        assert_eq!(px(&top, 92, 7), [255, 255, 255, 255]);
        assert_eq!(px(&top, 50, 1), [255, 0, 0, 255]);
        let left = boxed(BorderRadius { top_left: 30.0, top_right: 0.0, bottom_left: 30.0, bottom_right: 0.0 });
        assert_eq!(px(&left, 10, 4), [255, 0, 0, 255], "two 30 px corners on a 40 px edge are 20 px each");
        assert_eq!(px(&left, 10, 35), [255, 0, 0, 255]);
        assert_eq!(px(&left, 1, 1), [255, 255, 255, 255]);
    }

    /// A1-38: which lists the layered compositor has to paint. A `backdrop-filter` (only the
    /// layers apply it at all) and a filter other than `flood()` / `composite()`; not a list
    /// of those two alone, which the walk isolates and applies itself, and not a backdrop
    /// filter without filters. The walk isolates only a list it applies whole.
    /// Mutation: `PushBackdropFilter` left out of `full_repaint_needs_layers` sends a page
    /// with a frosted panel through the flat walk, whose arm for it is empty.
    #[test]
    fn a_backdrop_filter_and_every_filter_the_walk_does_not_apply_need_the_layers() {
        use azul_css::props::basic::length::PercentageValue;
        let around = |push: DisplayListItem, pop: DisplayListItem| list(vec![push, fill(0.0, 0.0, 10.0, 10.0, [255, 0, 0]), pop]);
        let invert = || StyleFilter::Invert(PercentageValue::new(100.0));
        let flood = || StyleFilter::Flood(ColorU { r: 0, g: 0, b: 255, a: 255 });
        let filter = |filters: Vec<StyleFilter>| around(DisplayListItem::PushFilter { bounds: wrect(0.0, 0.0, 10.0, 10.0), filters }, DisplayListItem::PopFilter);
        let backdrop = |filters: Vec<StyleFilter>| {
            around(DisplayListItem::PushBackdropFilter { bounds: wrect(0.0, 0.0, 10.0, 10.0), filters }, DisplayListItem::PopBackdropFilter)
        };
        assert!(full_repaint_needs_layers(&backdrop(vec![invert()])));
        assert!(!full_repaint_needs_layers(&backdrop(Vec::new())));
        assert!(full_repaint_needs_layers(&filter(vec![invert()])));
        assert!(full_repaint_needs_layers(&filter(vec![flood(), invert()])));
        assert!(!full_repaint_needs_layers(&filter(vec![flood()])));
        assert!(filter_group_needs_isolation(&[flood()]));
        assert!(!filter_group_needs_isolation(&[flood(), invert()]), "the layer applies the whole list");
        assert!(!filter_group_needs_isolation(&[invert()]));
    }

    /// A2-cpu-R03: a kept picture is not repaired for a list whose frame paints another
    /// extent. A view of 900 x 700 whose child is a frame (key 1, box (100, 0) 800 x 300,
    /// scaled by 1.5) around A, a 100 x 300 fill at (0, 0) LEFT of the box, a ground over the
    /// box and a mark at (150, 50): the extent is (0, 0) 900 x 300, the picture is placed
    /// from x = 0. The next list holds B, 100 x 300 at (900, 0) RIGHT of the box, in A's
    /// slot: the extent is (100, 0) 900 x 300 - the same size, the same phase, the same key,
    /// placed from x = 100. The two lists differ in one item (31,200 of 270,000 px), so the
    /// repair took the surface over; the hit then composited the old picture 100 px (150 on
    /// screen) to the right. Now the frame is painted again, and the frame on screen is the
    /// one painted afresh.
    /// Mutation: `stale_boxes` without the comparison of the two extents repairs
    /// (`retained_patched` 1) and shows the mark 150 px to the right.
    #[test]
    fn a_kept_picture_is_not_repaired_for_a_list_whose_frame_paints_another_extent() {
        let (rr, fm, _) = resources(None);
        let scene = |b_for_a: bool| {
            let first = if b_for_a { fill(900.0, 0.0, 100.0, 300.0, [0, 0, 255]) } else { fill(0.0, 0.0, 100.0, 300.0, [255, 0, 0]) };
            let child = vec![
                DisplayListItem::PushReferenceFrame {
                    transform_key: azul_core::resources::TransformKey { id: 1 },
                    initial_transform: azul_core::transform::ComputedTransform3D { m: IDENTITY },
                    bounds: wrect(100.0, 0.0, 800.0, 300.0),
                },
                first,
                fill(100.0, 0.0, 800.0, 300.0, [0, 160, 0]),
                fill(150.0, 50.0, 60.0, 60.0, [250, 250, 0]),
                DisplayListItem::PopReferenceFrame,
            ];
            Scene { host: zoom_scene(None, (0.0, 0.0), false).host, child: Arc::new(list(child)) }
        };
        let zoom = (1.5, (0.0, 0.0));
        let retained = Arc::new(RetainedFrames::with_max_bytes(64 << 20));
        let mut pm = AzulPixmap::new(900, 700).unwrap();
        pm.fill(255, 255, 255, 255);
        let old = scene(false);
        let first = zoom_frame(&old, &zoom_state(&old, zoom, 15.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!(retained.len(), 1, "the frame's picture is kept: {first:?}");
        let new = Scene { host: Arc::clone(&old.host), child: scene(true).child };
        let second = zoom_frame(&new, &zoom_state(&new, zoom, 15.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!(second.retained_patched, 0, "another extent: painted again, not repaired: {second:?}");
        let fresh = fresh_frame(&new, zoom, 15.0, (&rr, &fm));
        assert_eq!(differing_pixels(&pm, &fresh), 0, "the frame painted afresh");
        assert!(differing_pixels(&fresh, &fresh_frame(&old, zoom, 15.0, (&rr, &fm))) > 500, "harness: the lists differ on screen");
    }

    /// A2-cpu-R05: a frame's extent holds the halo of the runs under a text shadow. A frame
    /// with a 10 x 10 box around the run "mmm" (12 px, pen at (100, 50)): without a shadow
    /// its extent ends where the run's ink box ends; under a shadow with a blur of 4 and a
    /// spread of 8 (reach 4 + 8 = 12) it ends 12 px further right and further down, and
    /// starts 12 px further up - what the rasteriser paints, what a moved frame has to
    /// damage and what a group's surface has to hold.
    /// Mutation: `GroupBounds::step` without the shadows gives the same extent for both.
    #[test]
    fn a_frames_extent_holds_the_halo_of_the_runs_under_a_text_shadow() {
        let Some(font) = test_font() else {
            return;
        };
        let (_, _, font_hash) = resources(Some(&font));
        let run = label(font_hash.map(|hash| (&font, hash)), "mmm", 12.0, 100.0, 50.0);
        let ink = run[0].visual_bounds().expect("the run's ink box");
        let extent_of = |inner: Vec<DisplayListItem>| {
            list(frame(1, IDENTITY, (10.0, 10.0), inner)).frame_index().extent_of(0).expect("the frame's extent")
        };
        let (plain, under) = (extent_of(run.clone()), extent_of(haloed(halo(4.0, 8.0, [0, 0, 0]), run.clone())));
        let (right, bottom) = (|r: &LogicalRect| r.origin.x + r.size.width, |r: &LogicalRect| r.origin.y + r.size.height);
        assert!((right(&plain) - right(&ink)).abs() < 0.01 && (bottom(&plain) - bottom(&ink)).abs() < 0.01, "the ink alone: {plain:?} {ink:?}");
        assert!((right(&under) - (right(&ink) + 12.0)).abs() < 0.01, "12 px of halo to the right: {under:?}");
        assert!((bottom(&under) - (bottom(&ink) + 12.0)).abs() < 0.01, "and below: {under:?}");
        assert!((under.origin.y - (ink.origin.y - 12.0).min(0.0)).abs() < 0.01, "and above, or the box's top: {under:?}");
    }

    /// A2-cpu-R04: a kept picture of the WINDOW's own list is repaired the way it was painted.
    /// The window's list is one frame (key 1, 600 x 400, scaled by 1.5) around a ground, a
    /// 200 x 60 plate at (40, 40) and, over the plate, a caption turned by 15 degrees by its
    /// own frame (key 3). The next list paints the plate in another colour: the picture is
    /// repaired in the plate's box, and the frame is the one painted afresh - in the window's
    /// list a turned caption is a group, in the repaired box too.
    /// Mutation: the repair walking the list as a view's (`WalkCull::ActiveClip`) draws the
    /// caption directly inside the box (anchor rounded, outlines turned) and as a group
    /// outside it: pixels differ from the fresh frame.
    #[test]
    fn a_kept_picture_of_the_windows_list_is_repaired_as_it_was_painted() {
        let Some(font) = test_font() else {
            return;
        };
        let (rr, fm, font_hash) = resources(Some(&font));
        let page = |plate: [u8; 3]| {
            let mut inner = vec![fill(0.0, 0.0, 600.0, 400.0, [236, 233, 225]), fill(40.0, 40.0, 200.0, 60.0, plate)];
            inner.push(DisplayListItem::PushReferenceFrame {
                transform_key: azul_core::resources::TransformKey { id: 3 },
                initial_transform: azul_core::transform::ComputedTransform3D { m: turn_scale_move(15.0, 1.0, 0.0, 0.0) },
                bounds: wrect(50.0, 50.0, 180.0, 30.0),
            });
            inner.extend(label(font_hash.map(|hash| (&font, hash)), "Steige", 22.0, 56.0, 74.0));
            inner.push(DisplayListItem::PopReferenceFrame);
            Arc::new(list(frame(1, IDENTITY, (600.0, 400.0), inner)))
        };
        let paint = |dl: &Arc<DisplayList>, retained: Option<&Arc<RetainedFrames>>, pm: &mut AzulPixmap| {
            let mut state = CpuRenderState::new(ScrollOffsetMap::new()).with_root_display_list(Arc::clone(dl)).with_paint_threads(1);
            if let Some(retained) = retained {
                state = state.with_retained_frames(Arc::clone(retained));
            }
            state.transforms.insert(1, azul_core::transform::ComputedTransform3D { m: turn_scale_move(0.0, 1.5, 0.0, 0.0) });
            let mut gc = GlyphCache::new();
            crate::cpu_counters::reset();
            if let Some(retained) = retained {
                refresh_retained_surfaces(&state, &rr, &fm, &mut gc, 1.0).unwrap();
                retained.begin_frame(&[Arc::as_ptr(dl) as usize]);
            }
            render_display_list_damaged(dl, pm, 1.0, &rr, &fm, &mut gc, &state, &[rect(0.0, 0.0, 900.0, 600.0)]).unwrap();
            if let Some(retained) = retained {
                retained.commit();
            }
            crate::cpu_counters::take()
        };
        let white = || {
            let mut pm = AzulPixmap::new(900, 600).unwrap();
            pm.fill(255, 255, 255, 255);
            pm
        };
        let retained = Arc::new(RetainedFrames::with_max_bytes(64 << 20));
        let mut pm = white();
        let _ = paint(&page([255, 255, 210]), Some(&retained), &mut pm);
        if retained.len() != 1 {
            return; // the window's list keeps no picture here: nothing to repair
        }
        let edited = page([200, 230, 255]);
        let second = paint(&edited, Some(&retained), &mut pm);
        assert_eq!(second.retained_patched, 1, "one item differs: repaired: {second:?}");
        let mut fresh = white();
        let _ = paint(&edited, None, &mut fresh);
        assert_eq!(differing_pixels(&pm, &fresh), 0, "the caption in the repaired box is the caption painted afresh");
    }

    /// A view of 400 x 300 whose child is one text-only frame (key 21, the matrix `m`) around
    /// `inner`, on no ground.
    fn text_frame_view(m: [[f32; 4]; 4], frame_box: (f32, f32, f32, f32), inner: Vec<DisplayListItem>) -> (DisplayList, CpuRenderState) {
        let mut child = vec![DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 21 },
            initial_transform: azul_core::transform::ComputedTransform3D { m },
            bounds: wrect(frame_box.0, frame_box.1, frame_box.2, frame_box.3),
        }];
        child.extend(inner);
        child.push(DisplayListItem::PopReferenceFrame);
        let host = list(vec![DisplayListItem::VirtualView {
            child_dom_id: azul_core::dom::DomId { inner: 1 },
            bounds: wrect(0.0, 0.0, 400.0, 300.0),
            clip_rect: wrect(0.0, 0.0, 400.0, 300.0),
            content_offset: LogicalPosition { x: 0.0, y: 0.0 },
        }]);
        let state = CpuRenderState::new(ScrollOffsetMap::new()).with_virtual_view_display_lists(
            [(azul_core::dom::DomId { inner: 1 }, Arc::new(list(child)))].into_iter().collect(),
        );
        (host, state)
    }

    /// A2-cpu-R06: a run no picture can be made of makes its frame a group, and is painted.
    /// A text-only frame scaled by 8 around a line of 120 letters at 20 px (its ink over
    /// 1,100 px wide: more than 8,192 px through the matrix). Drawn directly the run was
    /// skipped after the frame had been committed to that path - no text at all; as a group
    /// the part that lands in the view is painted: letters 160 px high, black on white.
    /// Mutation: `turned_picture_box` not asked in the pre-check loop of
    /// `paint_turned_text_frame` leaves the view white.
    #[test]
    fn a_turned_run_too_large_for_a_picture_is_painted_as_a_group() {
        let Some(font) = test_font() else {
            return;
        };
        let (rr, fm, font_hash) = resources(Some(&font));
        let line = "m".repeat(120);
        let run = label(font_hash.map(|hash| (&font, hash)), &line, 20.0, 16.0, 32.0);
        let ink = run[0].visual_bounds().expect("the line's ink box");
        assert!(ink.size.width * 8.0 > 8192.0, "harness: the line is too wide for a picture: {ink:?}");
        let (host, state) = text_frame_view(turn_scale_move(0.0, 8.0, 0.0, 0.0), (10.0, 10.0, 2400.0, 30.0), run);
        let mut pm = AzulPixmap::new(400, 300).unwrap();
        pm.fill(255, 255, 255, 255);
        let mut gc = GlyphCache::new();
        crate::cpu_counters::reset();
        render_display_list_with_state(&host, &mut pm, 1.0, &rr, &fm, &mut gc, &state).unwrap();
        let counted = crate::cpu_counters::take();
        let dark = (0..300u32).flat_map(|y| (0..400u32).map(move |x| (x, y))).filter(|&(x, y)| px(&pm, x, y)[0] < 64).count();
        assert!(dark > 2000, "the letters that land in the view are painted: {dark} dark px, {counted:?}");
        assert_eq!(counted.groups_opened, 1, "as a group: {counted:?}");
    }

    /// A2-cpu-R06: a turned run that misses the clip gets no picture. A text-only frame turned
    /// by 7.25 degrees around 40 haloed runs, 40 px apart from y = 20 to 1,580, each another
    /// text (so each another picture): the view is 300 px high, the runs from the ninth on are
    /// below it. Halo pictures are built for the runs that can land (their boxes, 2 px of
    /// halo around the ink, placed at the anchor) and for no other.
    /// Mutation: the picture built before its box is tested against the clip builds 40.
    #[test]
    fn a_turned_run_outside_the_clip_gets_no_picture() {
        let Some(font) = test_font() else {
            return;
        };
        let (rr, fm, font_hash) = resources(Some(&font));
        let mut inner = Vec::new();
        for i in 0..40u32 {
            let text = format!("Weg {i:02} R06");
            inner.extend(haloed(
                halo(0.0, 2.0, [255, 255, 255]),
                label(font_hash.map(|hash| (&font, hash)), &text, 18.0, 30.0, 20.0 + 40.0 * i as f32),
            ));
        }
        let (host, state) = text_frame_view(turn_scale_move(7.25, 1.0, 0.0, 0.0), (0.0, 0.0, 400.0, 1600.0), inner);
        let mut pm = AzulPixmap::new(400, 300).unwrap();
        pm.fill(255, 255, 255, 255);
        let mut gc = GlyphCache::new();
        crate::cpu_counters::reset();
        render_display_list_with_state(&host, &mut pm, 1.0, &rr, &fm, &mut gc, &state).unwrap();
        let counted = crate::cpu_counters::take();
        assert_eq!(counted.groups_opened, 0, "drawn directly: {counted:?}");
        assert!((4..=12).contains(&counted.halos_drawn), "halo pictures for the runs in the view alone: {counted:?}");
    }

    /// A2-cpu-R10: an isolated filter group takes its box INSIDE the pass's clip. A 24 x 24
    /// box at (10, 10) with `filter: flood(rgba(0, 0, 0, 0.5))` on a white page: painted in
    /// full it is half black over white. A repaint of a 10 x 10 rect at (60, 20), beside the
    /// box, leaves the box's pixels as they are.
    /// Mutation: the group begun on its whole box in every pass floods the box again with
    /// nothing painted in it and writes half black over the pixels that already hold it:
    /// darker with every repaint elsewhere.
    #[test]
    fn a_filter_group_outside_the_repainted_rect_is_left_alone() {
        let (rr, fm) = (RendererResources::default(), empty_font_manager());
        let dl = list(vec![
            fill(0.0, 0.0, 100.0, 60.0, [255, 255, 255]),
            DisplayListItem::PushFilter {
                bounds: wrect(10.0, 10.0, 24.0, 24.0),
                filters: vec![StyleFilter::Flood(ColorU { r: 0, g: 0, b: 0, a: 128 })],
            },
            fill(12.0, 12.0, 8.0, 8.0, [255, 0, 0]),
            DisplayListItem::PopFilter,
        ]);
        let state = CpuRenderState::new(ScrollOffsetMap::new()).with_paint_threads(1);
        let (mut pm, _) = full(&dl, &state, (100, 60));
        let before = px(&pm, 20, 20);
        assert!((120..=135).contains(&before[0]) && before[3] == 255, "half black over white: {before:?}");
        let mut gc = GlyphCache::new();
        for _ in 0..3 {
            render_display_list_damaged(&dl, &mut pm, 1.0, &rr, &fm, &mut gc, &state, &[rect(60.0, 20.0, 10.0, 10.0)]).unwrap();
        }
        assert_eq!(px(&pm, 20, 20), before, "the box is outside the repainted rect");
        assert_eq!(px(&pm, 64, 24), [255, 255, 255, 255], "the rect itself is the page");
        // A repaint of a rect that cuts the box paints the flood there once.
        render_display_list_damaged(&dl, &mut pm, 1.0, &rr, &fm, &mut gc, &state, &[rect(20.0, 0.0, 30.0, 60.0)]).unwrap();
        assert_eq!(px(&pm, 25, 20), before, "inside the rect and the box");
        assert_eq!(px(&pm, 15, 20), before, "inside the box, outside the rect");
    }

    /// A2-cpu-R01: a frame's extent takes a frame inside it through a matrix with perspective
    /// as the rasteriser composites it. A column (0, 0) 400 x 100 holds a card of the same
    /// box whose matrix is the identity but for `m03 = -0.001` (a tilt): a point `q` lands at
    /// `q / w`, `w = 1 - 0.001 qx`. The corner (400, 100) has w = 0.6 and lands at
    /// (666.7, 166.7): the column's extent is (0, 0) 666.7 x 166.7. With `m03 = -0.003` the
    /// corner is behind the eye (w = -0.2): no bound.
    /// Mutation: the affine sums give (0, 0) 400 x 100 for the first.
    #[test]
    fn a_frames_extent_takes_a_tilted_frame_inside_it_through_its_perspective() {
        let column = |m03: f32| {
            let mut m = IDENTITY;
            m[0][3] = m03;
            let mut inner = vec![DisplayListItem::PushReferenceFrame {
                transform_key: azul_core::resources::TransformKey { id: 2 },
                initial_transform: azul_core::transform::ComputedTransform3D { m },
                bounds: wrect(0.0, 0.0, 400.0, 100.0),
            }];
            inner.push(fill(0.0, 0.0, 400.0, 100.0, [200, 0, 0]));
            inner.push(DisplayListItem::PopReferenceFrame);
            list(frame(1, IDENTITY, (400.0, 100.0), inner))
        };
        let tilted = column(-0.001);
        let extent = tilted.frame_index().extent_with(&tilted.items, 0, &HashMap::new()).expect("in front of the eye");
        assert!((extent.origin.x, extent.origin.y) == (0.0, 0.0), "{extent:?}");
        assert!((extent.size.width - 666.67).abs() < 0.1 && (extent.size.height - 166.67).abs() < 0.1, "{extent:?}");
        let behind = column(-0.003);
        assert_eq!(behind.frame_index().extent_with(&behind.items, 0, &HashMap::new()), None);
        let flat = column(0.0);
        assert_eq!(flat.frame_index().extent_with(&flat.items, 0, &HashMap::new()), Some(rect(0.0, 0.0, 400.0, 100.0)));
    }

    /// A1-46: borders with sides that differ. A 40 x 20 box at (10, 10): a red top side of
    /// half a pixel, blue sides of 2 px right and below, and a left side of 2 px with
    /// `border-style: none`.
    /// - The top side is one device pixel, row 10, and nothing above the box (row 9).
    /// - The left side paints nothing: (10, 20) is the page. The right one is the columns
    ///   48 and 49, the bottom one the rows 28 and 29.
    /// Mutations: the top bar's last row as `iy as i32 - 1` (9, before the first, 10) is
    /// normalised by `blend_bar` into the rows 9 and 10 - red above the box; the styles
    /// ignored paint the left side blue.
    #[test]
    fn a_border_side_thinner_than_a_pixel_is_one_row_and_a_side_without_a_style_is_not_painted() {
        use azul_css::props::style::border::BorderStyle;
        let (red, blue) = (ColorU { r: 255, g: 0, b: 0, a: 255 }, ColorU { r: 0, g: 0, b: 255, a: 255 });
        let mut pm = AzulPixmap::new(60, 40).unwrap();
        pm.fill(255, 255, 255, 255);
        render_border_sides(
            &mut pm,
            &rect(10.0, 10.0, 40.0, 20.0),
            [red, blue, blue, blue],
            [0.5, 2.0, 2.0, 2.0],
            [BorderStyle::Solid, BorderStyle::Solid, BorderStyle::Solid, BorderStyle::None],
            &BorderRadius::default(),
            None,
            1.0,
        );
        let (white, red, blue) = ([255, 255, 255, 255], [255, 0, 0, 255], [0, 0, 255, 255]);
        assert_eq!(px(&pm, 30, 10), red, "the top side: one device pixel");
        assert_eq!(px(&pm, 30, 9), white, "and nothing above the box");
        assert_eq!(px(&pm, 30, 11), white);
        assert_eq!(px(&pm, 10, 20), white, "border-style: none paints nothing");
        assert_eq!((px(&pm, 48, 20), px(&pm, 49, 20), px(&pm, 47, 20)), (blue, blue, white));
        assert_eq!((px(&pm, 30, 28), px(&pm, 30, 29), px(&pm, 30, 27)), (blue, blue, white));
    }

    /// A2-cpu-06: the pixels an isolated group sets aside. A box from 10.6 to 34.4 (device
    /// px) reaches into the pixels 10 .. 34: the region is [10, 35), 25 wide - the opacity
    /// group's own rounding (`x as i32`, `width as u32`) gave [10, 33) and left the columns
    /// 33 and 34 outside the group. Under a clip that ends at 33.5 a paint may write up to
    /// column 32 (`pixel_box`): the region is [10, 33) - `floor` / `ceil` of the clip, the
    /// filter group's rounding, gave [10, 34), one column past the clip.
    /// Mutations: the far edge as `x as i32 + width as i32` fails the first; the clip's far
    /// edge by `ceil` the second.
    #[test]
    fn an_isolated_group_takes_its_boxs_pixels_cut_to_what_the_clip_lets_paint() {
        let boxed = AzRect::from_xywh(10.6, 5.0, 23.8, 10.0);
        let whole = device_region(boxed, None).expect("a region");
        assert_eq!((whole.x, whole.y, whole.width, whole.height), (10.0, 5.0, 25.0, 10.0));
        let cut = device_region(boxed, AzRect::from_xywh(0.0, 0.0, 33.5, 100.0)).expect("a region");
        assert_eq!((cut.x, cut.width), (10.0, 23.0));
        assert!(device_region(boxed, AzRect::from_xywh(40.0, 0.0, 10.0, 100.0)).is_none(), "outside the clip");
        assert!(device_region(None, None).is_none());
    }

    // ---- read-through, round 4 --------------------------------------------------------------

    /// A1-50: a row composited through one slice, eight opaque pixels at a time, is the row
    /// composited pixel by pixel with a check of the buffer each (the loop before): rows of 0
    /// to 70 pixels, all opaque, all empty, mixed, and opaque with one translucent pixel, at
    /// four places of a 100 px buffer, the last ones running past its end.
    /// Mutation: `all` -> `any` in the test of a run copies translucent pixels; a run of 31
    /// bytes for 32 leaves the eighth pixel of each run unwritten.
    #[test]
    fn a_row_composited_in_runs_is_the_row_composited_pixel_by_pixel() {
        fn reference(data: &mut [u8], di_base: usize, stage: &[u8]) {
            for (i, q) in stage.chunks_exact(4).enumerate() {
                let di = di_base + i * 4;
                if di + 3 >= data.len() {
                    break;
                }
                let sa = u32::from(q[3]);
                if sa == 255 {
                    data[di..di + 4].copy_from_slice(&[q[0], q[1], q[2], 255]);
                } else if sa > 0 {
                    let da = 255 - sa;
                    for c in 0..3 {
                        data[di + c] = (u32::from(q[c]) + u32::from(data[di + c]) * da / 255).min(255) as u8;
                    }
                    data[di + 3] = (sa + u32::from(data[di + 3]) * da / 255).min(255) as u8;
                }
            }
        }
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for len in [0_usize, 1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 64, 70] {
            for kind in 0..4 {
                let mut stage = Vec::with_capacity(len * 4);
                for i in 0..len {
                    let r = next();
                    let a: u8 = match kind {
                        0 => 255,
                        1 => 0,
                        2 => [255, 0, (r >> 8) as u8][(r % 3) as usize],
                        _ if i == len / 2 => 128,
                        _ => 255,
                    };
                    let premultiplied = |v: u64| ((v & 0xff) as u32 * u32::from(a) / 255) as u8;
                    stage.extend_from_slice(&[premultiplied(r >> 16), premultiplied(r >> 24), premultiplied(r >> 32), a]);
                }
                for at in [0_usize, 3, 90, 96] {
                    let mut pm = AzulPixmap::new(25, 4).unwrap();
                    for (i, b) in pm.data_mut().iter_mut().enumerate() {
                        *b = (i * 37 % 251) as u8;
                    }
                    let mut expected = pm.data().to_vec();
                    reference(&mut expected, at * 4, &stage);
                    composite_rgba_row(&mut pm, at * 4, &stage);
                    assert!(pm.data() == expected.as_slice(), "{len} px, kind {kind}, at pixel {at}");
                }
            }
        }
        let mut pm = AzulPixmap::new(2, 1).unwrap();
        composite_rgba_row(&mut pm, 12, &[1, 2, 3, 255]);
        assert_eq!(pm.data(), &[255_u8; 8][..], "a row that starts past the buffer writes nothing");
    }

    /// A1-50: an RGBA8 image at its own size is composited from its own rows, without a copy
    /// of each row: a window of it paints the pixels the same picture stored as BGRA8 paints
    /// (which goes through a converted row), and nothing outside the window.
    /// Mutation: `row + px_lo * 4` without the `row` paints every row with the first.
    #[test]
    fn an_rgba_image_at_its_own_size_is_composited_from_its_own_rows() {
        use azul_core::resources::RawImageFormat as F;
        let (w, h) = (37_u32, 9_u32);
        let mut rgba = Vec::new();
        for i in 0..w * h {
            let a = [255_u8, 255, 0, 96][(i % 4) as usize];
            let premultiplied = |v: u32| (v % 256 * u32::from(a) / 255) as u8;
            rgba.extend_from_slice(&[premultiplied(i * 7), premultiplied(i * 13), premultiplied(i * 29), a]);
        }
        let bgra: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
        let paint = |bytes: &[u8], format: F| {
            let src = crate::image_scale::SrcImage { bytes, format, width: w, height: h };
            let mut pm = AzulPixmap::new(w + 6, h + 4).unwrap();
            pm.fill(10, 200, 30, 255);
            let mut rows = RowConversions::new();
            blit_sampled_image(&mut pm, &src, 3, 2, w, h, (5, 1, 30, 8), None, &mut rows);
            (pm, rows.0)
        };
        let (direct, rows) = paint(&rgba, F::RGBA8);
        let (converted, rows_converted) = paint(&bgra, F::BGRA8);
        assert!(direct.data() == converted.data(), "the same picture, the same pixels");
        assert_eq!((rows, rows_converted), (7, 7), "the window's seven rows, each read once");
        assert_eq!(px(&direct, 3 + 4, 2 + 3), [10, 200, 30, 255], "left of the window: not painted");
        assert_eq!(px(&direct, 3 + 6, 2 + 8), [10, 200, 30, 255], "below the window: not painted");
        let opaque = (5..30).filter(|x| px(&direct, 3 + x, 2 + 4) != [10, 200, 30, 255]).count();
        assert!(opaque > 10, "and the window is painted ({opaque} px of its fourth row differ from the ground)");
    }


    /// A2-cpu-06 (the shadow sites): a box shadow and a text halo land on the pixels a paint
    /// under the clip may write (`AzRect::pixel_box`). Under the clip (0, 0) 30.5 x 40 fills
    /// write the columns 0 .. 29: the shadow of a box at (10, 10) 40 x 20, which reaches over
    /// the clip's right edge, stops at column 29 too; with the far edge rounded up it wrote
    /// column 30. And the boxes of a halo with a reach of 2 around ink at (10, 10) 20 x 10
    /// under the clip (0.5, 0.5) 20 x 30: out = [8, 8, 20, 22) (it was [8, 8, 21, 22)).
    /// Mutation: `ceil` on the far edge in either place.
    #[test]
    fn a_shadow_lands_on_the_pixels_a_fill_under_its_clip_may_write() {
        let mut pm = AzulPixmap::new(60, 50).unwrap();
        pm.fill(255, 255, 255, 255);
        let shadow = StyleBoxShadow { color: ColorU { r: 0, g: 0, b: 0, a: 255 }, ..halo(0.0, 4.0, [0, 0, 0]) };
        let clip = AzRect::from_xywh(0.0, 0.0, 30.5, 40.0);
        render_box_shadow(&mut pm, &rect(10.0, 10.0, 40.0, 20.0), &shadow, &BorderRadius::default(), clip, 1.0).unwrap();
        assert_eq!(px(&pm, 8, 20), [0, 0, 0, 255], "premise: the spread's ring left of the box is painted");
        assert_eq!(px(&pm, 29, 8), [0, 0, 0, 255], "the last column a fill under the clip writes");
        assert_eq!(px(&pm, 30, 8), [255, 255, 255, 255], "the column the clip ends in is not written");

        let source = AzRect::from_xywh(10.0, 10.0, 20.0, 10.0).expect("ink");
        let boxes = text_shadow_boxes(source, 2, AzRect::from_xywh(0.5, 0.5, 20.0, 30.0), (100, 100)).expect("it lands");
        assert_eq!(boxes.out, PixelBox { x: 8, y: 8, width: 12, height: 14 });
        let unclipped = text_shadow_boxes(source, 2, None, (100, 100)).expect("it lands");
        assert_eq!(unclipped.out, PixelBox { x: 8, y: 8, width: 24, height: 14 });
    }


    /// A1-53: a use of a kept box shadow makes it the most recently used without a search of
    /// the order, the order stays short however often shadows are used, and the least
    /// recently used one still goes first. Three buffers of 6 MB under the 16 MB bound: 1 and
    /// 2 are kept, 1 is used, 3 does not fit beside both: 2 goes. Then 20,000 uses of 1 and 3
    /// in turn: the order holds at most 2 x 2 + 64 places (one more before it is compacted),
    /// not 20,000. Then 1 is used last and 4 evicts 3.
    /// Mutation: the `contains` test left out of `keep_shadow` evicts 1 (its first place is
    /// the front of the order); without the compaction the order grows by a place per use.
    #[test]
    fn a_used_shadow_is_the_newest_without_a_search_of_the_order() {
        let six_mb = 6 << 20;
        let mut cache = ShadowCache::default();
        let keep = |cache: &mut ShadowCache, key: u64| keep_shadow(cache, key, &Arc::new(vec![0_u8; six_mb]), 1, 1);
        let held = |cache: &ShadowCache| {
            let mut keys: Vec<u64> = cache.0.keys().copied().collect();
            keys.sort_unstable();
            keys
        };
        keep(&mut cache, 1);
        keep(&mut cache, 2);
        assert!(kept_shadow(&mut cache, 1).is_some());
        keep(&mut cache, 3);
        assert_eq!(held(&cache), vec![1, 3], "the least recently used made room");
        assert_eq!(cache.2, 2 * six_mb);
        for i in 0..20_000_u64 {
            assert!(kept_shadow(&mut cache, [1, 3][(i % 2) as usize]).is_some());
        }
        assert!(cache.1.len() <= 2 * 2 + 64 + 1, "the order holds {} places for two shadows", cache.1.len());
        assert!(kept_shadow(&mut cache, 1).is_some());
        keep(&mut cache, 4);
        assert_eq!(held(&cache), vec![1, 4], "3 was used before 1");
        assert!(kept_shadow(&mut cache, 9).is_none());
        // A buffer over the bound is not kept, and what is kept stays (making room for it
        // emptied the cache before).
        keep_shadow(&mut cache, 5, &Arc::new(vec![0_u8; SHADOW_CACHE_MAX_BYTES + 4]), 1, 1);
        assert!(kept_shadow(&mut cache, 5).is_none());
        assert_eq!(held(&cache), vec![1, 4]);
        assert_eq!(cache.2, cache.0.values().map(|e| e.data.len()).sum::<usize>());
    }


    /// A1-54: the shadow of a rounded box with the box cut out of it, and an inner shadow cut
    /// to its padding box, are made once and kept; they were made again on every draw. An
    /// outer shadow keeps two buffers (the blur, and the blur with the box cut out where it
    /// lies in the buffer), an inner one keeps one; a draw at another place a whole number of
    /// pixels away makes nothing new, and what is drawn from the kept buffer is, byte for
    /// byte, what a draw with an empty cache paints there. Half a pixel further the box lies
    /// elsewhere in the buffer: a third buffer.
    /// Mutation: the place of the box left out of the cut's key draws the half-pixel shadow
    /// with the whole-pixel cut (the last assert); a cut kept under the blur's key fails the
    /// counts.
    #[test]
    fn a_rounded_or_inner_shadow_is_cut_once_and_kept() {
        let clear = || SHADOW_BLUR_CACHE.with(|c| *c.borrow_mut() = ShadowCache::default());
        let kept = || SHADOW_BLUR_CACHE.with(|c| c.borrow().0.len());
        let radius = BorderRadius { top_left: 6.0, top_right: 6.0, bottom_left: 6.0, bottom_right: 6.0 };
        let draw = |shadow: &StyleBoxShadow, x: f32, y: f32| {
            let mut pm = AzulPixmap::new(120, 90).unwrap();
            pm.fill(255, 255, 255, 255);
            render_box_shadow(&mut pm, &rect(x, y, 50.0, 30.0), shadow, &radius, None, 1.0).unwrap();
            pm
        };
        let outer = StyleBoxShadow { color: ColorU { r: 0, g: 0, b: 0, a: 160 }, ..halo(5.0, 2.0, [0, 0, 0]) };
        let inner = StyleBoxShadow {
            color: ColorU { r: 0, g: 0, b: 0, a: 160 },
            clip_mode: azul_css::props::style::box_shadow::BoxShadowClipMode::Inset,
            ..halo(5.0, 2.0, [0, 0, 0])
        };
        for (shadow, buffers) in [(&outer, 2_usize), (&inner, 1)] {
            clear();
            let first = draw(shadow, 20.0, 20.0);
            assert!(first.data().chunks_exact(4).any(|p| p != [255, 255, 255, 255]), "premise: a shadow is painted");
            assert_eq!(kept(), buffers);
            let moved = draw(shadow, 33.0, 27.0);
            assert_eq!(kept(), buffers, "moved by whole pixels: nothing new is made");
            let again = draw(shadow, 20.0, 20.0);
            assert!(first.data() == again.data(), "the kept shadow is the shadow that was drawn");
            clear();
            let cold = draw(shadow, 33.0, 27.0);
            assert!(moved.data() == cold.data(), "and elsewhere it is the shadow a first draw paints there");
        }
        clear();
        let _ = draw(&outer, 20.0, 20.0);
        let half = draw(&outer, 20.5, 20.0);
        assert_eq!(kept(), 3, "half a pixel further the box lies elsewhere in the buffer: another cut");
        clear();
        assert!(half.data() == draw(&outer, 20.5, 20.0).data(), "which is the cut a first draw makes there");
    }


    /// A black-to-white radial gradient about the centre of a 200 x 100 box, painted.
    fn radial_gradient_picture(
        shape: azul_css::props::style::background::Shape,
        size: azul_css::props::style::background::RadialGradientSize,
        repeat: bool,
        stops_percent: &[(f32, u8)],
    ) -> AzulPixmap {
        use azul_css::props::{
            basic::{length::PercentageValue, FloatValue},
            style::background::{
                BackgroundPositionHorizontal, BackgroundPositionVertical, ExtendMode, NormalizedLinearColorStop,
                RadialGradient, StyleBackgroundPosition,
            },
        };
        let stops: Vec<NormalizedLinearColorStop> = stops_percent
            .iter()
            .map(|(percent, grey)| NormalizedLinearColorStop {
                offset: PercentageValue::new(*percent),
                color: ColorOrSystem::Color(ColorU { r: *grey, g: *grey, b: *grey, a: 255 }),
                offset_px: FloatValue::const_new(0),
            })
            .collect();
        let gradient = RadialGradient {
            shape,
            size,
            position: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Center,
                vertical: BackgroundPositionVertical::Center,
            },
            extend_mode: if repeat { ExtendMode::Repeat } else { ExtendMode::Clamp },
            stops: stops.into(),
        };
        let mut pm = AzulPixmap::new(200, 100).unwrap();
        pm.fill(255, 0, 0, 255);
        render_radial_gradient(&mut pm, &rect(0.0, 0.0, 200.0, 100.0), &gradient, &BorderRadius::default(), None, 1.0, None);
        pm
    }

    /// A1-51: an elliptical radial gradient is an ellipse. Black to white about the centre of a
    /// 200 x 100 box, to the farthest corner (the CSS default): the radii are 100 sqrt(2) =
    /// 141.4 and 50 sqrt(2) = 70.7 px. The pixel 90.5 px right of the centre is at
    /// 90.5 / 141.4 = 0.640 of the ray, the pixel 45.5 px below it at 45.5 / 70.7 = 0.643:
    /// the same grey (163 and 164). Painted as the circle of the corner's distance (111.8 px),
    /// they were 0.81 and 0.41 of the ray: 206 and 104. A `circle` still is that circle. To
    /// the closest side the radii are 100 and 50: 50.5 px right and 25.5 px below are both
    /// half way (129, 130).
    /// Mutation: one radius for both axes of the ellipse.
    #[test]
    fn an_elliptical_radial_gradient_is_an_ellipse() {
        use azul_css::props::style::background::{RadialGradientSize as Size, Shape};
        let grey = |pm: &AzulPixmap, x: u32, y: u32| {
            let p = px(pm, x, y);
            assert!(p[0] == p[1] && p[1] == p[2] && p[3] == 255, "a grey, got {p:?}");
            i32::from(p[0])
        };
        let stops = [(0.0, 0), (100.0, 255)];
        let ellipse = radial_gradient_picture(Shape::Ellipse, Size::FarthestCorner, false, &stops);
        let (right, below) = (grey(&ellipse, 190, 50), grey(&ellipse, 100, 95));
        assert!((right - 163).abs() <= 4 && (below - 164).abs() <= 4, "0.64 of the ray on both axes: {right}, {below}");
        let circle = radial_gradient_picture(Shape::Circle, Size::FarthestCorner, false, &stops);
        let (right, below) = (grey(&circle, 190, 50), grey(&circle, 100, 95));
        assert!((right - 206).abs() <= 4 && (below - 104).abs() <= 4, "a circle of 111.8 px: {right}, {below}");
        let side = radial_gradient_picture(Shape::Ellipse, Size::ClosestSide, false, &stops);
        let (right, below) = (grey(&side, 150, 50), grey(&side, 100, 75));
        assert!((right - 129).abs() <= 4 && (below - 130).abs() <= 4, "half way on both axes: {right}, {below}");
    }

    /// A1-52: a repeating radial gradient repeats. A circle to the closest side of a 200 x 100
    /// box has a ray of 50 px; black at 0 % and white at 20 % is a period of 10 px. The pixels
    /// 2.5, 12.5 and 22.5 px right of the centre are a quarter into a period (64), those 7.5
    /// and 17.5 px out three quarters (191). Painted once and clamped, everything beyond
    /// 10 px was white. With the stops at 10 % and 30 % the period begins 5 px out: 7.5 and
    /// 17.5 px are a quarter in, 2.5 px is three quarters into the period before.
    /// Mutation: `GradientRadialD` in place of `RadialRepeatFrom` clamps; a start of 0 shifts
    /// the second picture by half a period.
    #[test]
    fn a_repeating_radial_gradient_repeats_from_its_first_stop() {
        use azul_css::props::style::background::{RadialGradientSize as Size, Shape};
        let grey = |pm: &AzulPixmap, x: u32| i32::from(px(pm, x, 50)[0]);
        let near = |got: i32, want: i32| (got - want).abs() <= 10;
        let from_centre = radial_gradient_picture(Shape::Circle, Size::ClosestSide, true, &[(0.0, 0), (20.0, 255)]);
        for x in [102, 112, 122] {
            assert!(near(grey(&from_centre, x), 64), "x {x}: a quarter into a period, got {}", grey(&from_centre, x));
        }
        for x in [107, 117] {
            assert!(near(grey(&from_centre, x), 191), "x {x}: three quarters, got {}", grey(&from_centre, x));
        }
        let from_five = radial_gradient_picture(Shape::Circle, Size::ClosestSide, true, &[(10.0, 0), (30.0, 255)]);
        for (x, want) in [(107, 64), (117, 64), (102, 191), (112, 191)] {
            assert!(near(grey(&from_five, x), want), "x {x}: wanted {want}, got {}", grey(&from_five, x));
        }
        let clamped = radial_gradient_picture(Shape::Circle, Size::ClosestSide, false, &[(0.0, 0), (20.0, 255)]);
        assert_eq!(grey(&clamped, 122), 255, "not repeating: white beyond the last stop");
    }


    /// A conic gradient about the centre of a 100 x 100 box from 12 o'clock, painted.
    fn conic_gradient_picture(repeat: bool, stops_degrees: &[(f32, [u8; 3])]) -> AzulPixmap {
        use azul_css::props::{
            basic::angle::AngleValue,
            style::background::{
                BackgroundPositionHorizontal, BackgroundPositionVertical, ConicGradient, ExtendMode,
                NormalizedRadialColorStop, StyleBackgroundPosition,
            },
        };
        let stops: Vec<NormalizedRadialColorStop> = stops_degrees
            .iter()
            .map(|(degrees, rgb)| NormalizedRadialColorStop {
                angle: AngleValue::deg(*degrees),
                color: ColorOrSystem::Color(ColorU { r: rgb[0], g: rgb[1], b: rgb[2], a: 255 }),
            })
            .collect();
        let gradient = ConicGradient {
            extend_mode: if repeat { ExtendMode::Repeat } else { ExtendMode::Clamp },
            center: StyleBackgroundPosition {
                horizontal: BackgroundPositionHorizontal::Center,
                vertical: BackgroundPositionVertical::Center,
            },
            angle: AngleValue::deg(0.0),
            stops: stops.into(),
        };
        let mut pm = AzulPixmap::new(100, 100).unwrap();
        pm.fill(0, 255, 0, 255);
        render_conic_gradient(&mut pm, &rect(0.0, 0.0, 100.0, 100.0), &gradient, &BorderRadius::default(), None, 1.0, None);
        pm
    }

    /// A1-58 (found by A3-solver): a conic gradient runs over the whole turn. `conic-gradient(
    /// red, blue)` on a 100 x 100 box, centre (50, 50): the pixel (90, 50), at 3 o'clock, is a
    /// quarter of the turn: rgb(191, 0, 64); (50, 90), 6 o'clock, half: rgb(128, 0, 128);
    /// (10, 50), 9 o'clock, three quarters: rgb(64, 0, 191). Over half a turn and mirrored
    /// (agg's `GradientConic`) they were rgb(128, 0, 128), rgb(0, 0, 255), rgb(128, 0, 128).
    /// Mutation: `atan2(..).abs() / PI` in `ConicTurn`.
    #[test]
    fn a_conic_gradient_runs_over_the_whole_turn() {
        let pm = conic_gradient_picture(false, &[(0.0, [255, 0, 0]), (360.0, [0, 0, 255])]);
        for ((x, y), want) in [((90, 50), [191, 0, 64]), ((50, 90), [128, 0, 128]), ((10, 50), [64, 0, 191])] {
            let got = px(&pm, x, y);
            assert!(
                (0..3).all(|c| got[c].abs_diff(want[c]) <= 3) && got[3] == 255,
                "({x}, {y}): wanted {want:?}, got {got:?}"
            );
        }
    }

    /// A1-59: a repeating conic gradient repeats the part of the turn between its first and
    /// its last stop. Black at 0deg, white at 90deg: the pixels at 45, 135, 225 and 315
    /// degrees ((80, 19), (80, 80), (19, 80), (19, 19): 45.5 .. 46 degrees into their quarter)
    /// are all half way: grey 128. Clamped, everything past 3 o'clock was white.
    /// Mutation: `repeat: None` for a repeating gradient.
    #[test]
    fn a_repeating_conic_gradient_repeats_its_part_of_the_turn() {
        let pm = conic_gradient_picture(true, &[(0.0, [0, 0, 0]), (90.0, [255, 255, 255])]);
        for (x, y) in [(80, 19), (80, 80), (19, 80), (19, 19)] {
            let got = px(&pm, x, y);
            assert!(got[0] == got[1] && got[1] == got[2], "a grey at ({x}, {y}): {got:?}");
            assert!(got[0].abs_diff(128) <= 8, "half way into its quarter at ({x}, {y}): {got:?}");
        }
        let clamped = conic_gradient_picture(false, &[(0.0, [0, 0, 0]), (90.0, [255, 255, 255])]);
        assert_eq!(px(&clamped, 80, 80), [255, 255, 255, 255], "not repeating: white past the last stop");
    }


    /// A1-60 (found by A3-solver): what is painted inside a reference frame counts, in every
    /// bound gathered over a group's items, where the frame's matrix puts it - not where it is
    /// laid out. A column, box (0, 0) 200 x 400, holds a card, box (0, 0) 200 x 80 filled, built
    /// with `translate(-100, 0)`: the card paints x -100 .. 100, so the column's laid-out
    /// extent is (-100, 0) 300 x 400 (it was (0, 0) 200 x 400: only `extent_with` knew). The
    /// same for an opacity group around the card; and with the card's LIVE matrix
    /// `translate(50, 0)` the group paints (0, 0) 250 x 400.
    /// Mutation: the `PushReferenceFrame` arm left out of `GroupBounds::step`.
    #[test]
    fn a_frame_inside_a_group_counts_where_its_matrix_paints_it() {
        use azul_core::transform::ComputedTransform3D;
        let card = frame(
            2,
            turn_scale_move(0.0, 1.0, -100.0, 0.0),
            (200.0, 80.0),
            vec![fill(0.0, 0.0, 200.0, 80.0, [200, 0, 0])],
        );
        let column = list(frame(1, IDENTITY, (200.0, 400.0), card.clone()));
        let want = rect(-100.0, 0.0, 300.0, 400.0);
        assert_eq!(column.frame_index().extent_of(0), Some(want));
        assert_eq!(crate::solver3::display_list::reference_frame_extent_walked(&column.items, 0), Some(want));
        assert_eq!(column.frame_index().extent_with(&column.items, 0, &HashMap::new()), Some(want));

        let mut items = vec![DisplayListItem::PushOpacity {
            bounds: wrect(0.0, 0.0, 200.0, 400.0),
            opacity: 0.5,
            opacity_key: None,
            mix_blend_mode: Default::default(),
        }];
        items.extend(card);
        items.push(DisplayListItem::PopOpacity);
        let faded = list(items);
        let index = faded.frame_index();
        assert_eq!(index.opacity_extent(0), Some(want));
        assert_eq!(index.opacity_extent_with(&faded.items, 0, &HashMap::new()), Some(want));
        let mut live = HashMap::new();
        live.insert(2, ComputedTransform3D { m: turn_scale_move(0.0, 1.0, 50.0, 0.0) });
        assert_eq!(index.opacity_extent_with(&faded.items, 0, &live), Some(rect(0.0, 0.0, 250.0, 400.0)));
    }

    /// A1-60: a child that a transform moves out of a faded group's box is faded where it is
    /// painted. A half-opaque group, box (10, 10) 40 x 40, holds a frame of the same box with a
    /// red fill, moved by 60 px: the fill is painted at x 70 .. 110, half red over white
    /// (255, 128, 128), and nothing is painted where it is laid out. With the group's region
    /// taken from where the child is laid out, the moved fill was painted outside it at full
    /// red. Both for a frame built with the matrix and for one moved by a live value.
    /// Mutation: `opacity_extent` in place of `opacity_extent_with` in the walk fails the live
    /// case; the frame arm left out of `GroupBounds::step` fails both.
    #[test]
    fn a_child_moved_out_of_a_faded_groups_box_is_faded_where_it_is_painted() {
        use azul_core::transform::ComputedTransform3D;
        let group = |built_with: [[f32; 4]; 4]| {
            list(vec![
                DisplayListItem::PushOpacity {
                    bounds: wrect(10.0, 10.0, 40.0, 40.0),
                    opacity: 0.5,
                    opacity_key: None,
                    mix_blend_mode: Default::default(),
                },
                DisplayListItem::PushReferenceFrame {
                    transform_key: azul_core::resources::TransformKey { id: 7 },
                    initial_transform: ComputedTransform3D { m: built_with },
                    bounds: wrect(10.0, 10.0, 40.0, 40.0),
                },
                fill(10.0, 10.0, 40.0, 40.0, [255, 0, 0]),
                DisplayListItem::PopReferenceFrame,
                DisplayListItem::PopOpacity,
            ])
        };
        let moved = turn_scale_move(0.0, 1.0, 60.0, 0.0);
        let (built, _) = full(&group(moved), &CpuRenderState::new(ScrollOffsetMap::new()), (160, 60));
        let mut state = CpuRenderState::new(ScrollOffsetMap::new());
        state.transforms.insert(7, ComputedTransform3D { m: moved });
        let (live, _) = full(&group(IDENTITY), &state, (160, 60));
        for (what, painted) in [("built with the matrix", &built), ("moved by a live value", &live)] {
            let [r, g, b, a] = px(painted, 90, 30);
            assert!(
                r == 255 && (120..=136).contains(&g) && (120..=136).contains(&b) && a == 255,
                "{what}: half red over white where the child is painted, got {:?}",
                [r, g, b, a]
            );
            assert_eq!(px(painted, 30, 30), [255, 255, 255, 255], "{what}: nothing where it is laid out");
        }
    }


    /// A1-47: a border whose sides differ follows its radius, and each pixel is the side's it
    /// lies least deep in. Box (10, 10) 60 x 40, radius 10, every side 4 px, the top red, the
    /// others blue. The outer arc of the top-left corner is about (20, 20), radius 10; the
    /// inner one radius 6.
    /// - (11, 11) is 12.0 px from the arc's centre: outside the border. The sides were square
    ///   trapezoids and painted it.
    /// - (14, 15) and (15, 14) lie whole in the ring (their corners are 6.4 to 7.8 px from the
    ///   centre), right of the left side's strip (x < 14) and below the top side's (y < 14):
    ///   no trapezoid held them. (14, 15) is 1.375 widths into the top side and 1.125 into
    ///   the left one: blue; (15, 14) the other way round: red.
    /// - (16, 13) is 0.875 widths into the top side, 1.625 into the left: red.
    /// - (40, 11), on the straight part of the top side: red; (40, 30), inside: not painted.
    /// Mutation: the outer path built without the radius paints (11, 11); the depth not
    /// divided by the width, with a top side of 8 px, moves the mitre.
    #[test]
    fn a_border_with_different_sides_follows_its_radius() {
        use azul_css::props::style::border::BorderStyle;
        let (red, blue) = (ColorU { r: 255, g: 0, b: 0, a: 255 }, ColorU { r: 0, g: 0, b: 255, a: 255 });
        let white = [255, 255, 255, 255];
        let mut pm = AzulPixmap::new(90, 70).unwrap();
        pm.fill(255, 255, 255, 255);
        let radius = BorderRadius { top_left: 10.0, top_right: 10.0, bottom_left: 10.0, bottom_right: 10.0 };
        render_border_sides(
            &mut pm,
            &rect(10.0, 10.0, 60.0, 40.0),
            [red, blue, blue, blue],
            [4.0; 4],
            [BorderStyle::Solid; 4],
            &radius,
            None,
            1.0,
        );
        assert_eq!(px(&pm, 11, 11), white, "outside the rounded corner");
        assert_eq!(px(&pm, 14, 15), [0, 0, 255, 255], "on the arc, the left side's");
        assert_eq!(px(&pm, 15, 14), [255, 0, 0, 255], "on the arc, the top side's");
        assert_eq!(px(&pm, 16, 13), [255, 0, 0, 255], "in the top side's strip, on the arc");
        assert_eq!(px(&pm, 40, 11), [255, 0, 0, 255], "the top side");
        assert_eq!(px(&pm, 11, 30), [0, 0, 255, 255], "the left side");
        assert_eq!(px(&pm, 40, 30), white, "inside the border");
        assert_eq!(px(&pm, 16, 16), white, "inside the inner arc (5.0 px from its centre, radius 6)");
    }

    /// A1-47: a dashed or dotted side of a border whose sides differ is drawn in dashes. Box
    /// (10, 10) 80 x 30 without a radius, every side 4 px, the top dashed red (dashes of 12 px
    /// and gaps of 4 from the box's left edge: 10 .. 22, 26 .. 38, ...), the others solid blue.
    /// The pixel (30, 11) is in the second dash: red. (23, 11) is in the first gap (22 .. 26):
    /// not painted - it was red, the side was drawn solid. A dotted bottom side (4 px on,
    /// 4 px off): (20, 37) in a dot, (24, 37) in a gap.
    /// Mutation: the pattern taken as `(0, 0)` for every side paints the gaps.
    #[test]
    fn a_dashed_side_of_a_border_with_different_sides_is_dashed() {
        use azul_css::props::style::border::BorderStyle;
        let (red, blue) = (ColorU { r: 255, g: 0, b: 0, a: 255 }, ColorU { r: 0, g: 0, b: 255, a: 255 });
        let white = [255, 255, 255, 255];
        let mut pm = AzulPixmap::new(100, 50).unwrap();
        pm.fill(255, 255, 255, 255);
        render_border_sides(
            &mut pm,
            &rect(10.0, 10.0, 80.0, 30.0),
            [red, blue, red, blue],
            [4.0; 4],
            [BorderStyle::Dashed, BorderStyle::Solid, BorderStyle::Dotted, BorderStyle::Solid],
            &BorderRadius::default(),
            None,
            1.0,
        );
        assert_eq!(px(&pm, 30, 11), [255, 0, 0, 255], "in a dash of the top side");
        assert_eq!(px(&pm, 23, 11), white, "in a gap of the top side");
        assert_eq!(px(&pm, 20, 37), [255, 0, 0, 255], "in a dot of the bottom side (18 .. 22)");
        assert_eq!(px(&pm, 24, 37), white, "in a gap of the bottom side (22 .. 26)");
        assert_eq!(px(&pm, 11, 25), [0, 0, 255, 255], "the solid left side");
        assert_eq!(px(&pm, 88, 25), [0, 0, 255, 255], "the solid right side");
        assert_eq!(px(&pm, 50, 25), white, "inside the border");
        // The coverage of a pixel across a dash's end: half in, half out.
        assert!((dash_coverage(12.0, 12.0, 4.0) - 0.5).abs() < 1e-9);
        assert!((dash_coverage(5.5, 12.0, 4.0) - 1.0).abs() < 1e-9 && dash_coverage(14.0, 12.0, 4.0).abs() < 1e-9);
    }

    // ---- A1-26: kept opacity groups ---------------------------------------------------------

    /// A window (900 x 700) with a group that fades: a ground, 48 cells, then the group (key
    /// 7, box (100, 80) 500 x 400) around a panel, 30 cells, a label with a halo (when there is
    /// a font) and a card in a frame of its own (key 8, moved by the render state), then a bar
    /// painted over the group's lower edge. `edit` 1 recolours a cell OUTSIDE the group (a tile
    /// that arrived), `edit` 2 the panel inside it (a label that was edited).
    fn fade_scene(font: Option<(&ParsedFont, FontHash)>, edit: u8) -> Arc<DisplayList> {
        let mut items = vec![fill(0.0, 0.0, 900.0, 700.0, [236, 233, 225])];
        for i in 0..8u8 {
            for j in 0..6u8 {
                let rgb = if edit == 1 && i == 7 && j == 5 { [255, 0, 255] } else { [i * 30, j * 40, 120] };
                items.push(fill(f32::from(i) * 110.0 + 10.0, f32::from(j) * 115.0 + 8.0, 90.0, 95.0, rgb));
            }
        }
        items.push(DisplayListItem::PushOpacity {
            bounds: wrect(100.0, 80.0, 500.0, 400.0),
            opacity: 1.0,
            opacity_key: Some(azul_core::resources::OpacityKey { id: 7 }),
            mix_blend_mode: Default::default(),
        });
        items.push(fill(100.0, 80.0, 500.0, 400.0, if edit == 2 { [90, 200, 90] } else { [250, 250, 250] }));
        for i in 0..6u8 {
            for j in 0..5u8 {
                items.push(fill(120.0 + f32::from(i) * 78.0, 150.0 + f32::from(j) * 60.0, 60.0, 44.0, [200 - i * 20, 60 + j * 30, 40]));
            }
        }
        items.extend(haloed(halo(0.0, 2.0, [255, 255, 255]), label(font, "Hirschauer Steige", 22.0, 130.0, 120.0)));
        items.push(DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 8 },
            initial_transform: azul_core::transform::ComputedTransform3D { m: IDENTITY },
            bounds: wrect(420.0, 400.0, 120.0, 60.0),
        });
        items.push(fill(420.0, 400.0, 120.0, 60.0, [30, 30, 160]));
        items.push(DisplayListItem::PopReferenceFrame);
        items.push(DisplayListItem::PopOpacity);
        items.push(fill(80.0, 460.0, 300.0, 30.0, [20, 20, 20]));
        Arc::new(list(items))
    }

    /// The render state of a frame of `scene`: the group at `opacity`, its card moved by
    /// `card` px to the right, kept surfaces in `retained` (if any).
    fn fade_state(scene: &Arc<DisplayList>, opacity: f32, card: f32, retained: Option<&Arc<RetainedFrames>>, threads: usize) -> CpuRenderState {
        let mut state = CpuRenderState::new(ScrollOffsetMap::new())
            .with_root_display_list(Arc::clone(scene))
            .with_paint_threads(threads);
        if let Some(retained) = retained {
            state = state.with_retained_frames(Arc::clone(retained));
        }
        state.opacities.insert(7, opacity);
        state.transforms.insert(8, azul_core::transform::ComputedTransform3D { m: turn_scale_move(0.0, 1.0, card, 0.0) });
        state
    }

    /// One frame of `scene` as the backend paints it (the whole window repainted), and what
    /// was counted.
    fn fade_frame(
        scene: &Arc<DisplayList>,
        state: &CpuRenderState,
        pm: &mut AzulPixmap,
        (rr, fm): (&RendererResources, &FontManager<FontRef>),
    ) -> crate::cpu_counters::CpuFrameCounters {
        let mut gc = GlyphCache::new();
        let whole = [rect(0.0, 0.0, 900.0, 700.0)];
        crate::cpu_counters::reset();
        if let Some(retained) = &state.retained {
            refresh_retained_surfaces(state, rr, fm, &mut gc, 1.0).unwrap();
            retained.begin_frame(&[Arc::as_ptr(scene) as usize]);
        }
        render_display_list_damaged(scene, pm, 1.0, rr, fm, &mut gc, state, &whole).unwrap();
        if let Some(retained) = &state.retained {
            retained.commit();
        }
        crate::cpu_counters::take()
    }

    /// The same frame painted afresh: nothing kept, one pass, a new target.
    fn fresh_fade_frame(scene: &Arc<DisplayList>, opacity: f32, card: f32, resources: (&RendererResources, &FontManager<FontRef>)) -> AzulPixmap {
        let mut pm = AzulPixmap::new(900, 700).unwrap();
        pm.fill(255, 255, 255, 255);
        let _ = fade_frame(scene, &fade_state(scene, opacity, card, None, 1), &mut pm, resources);
        pm
    }

    /// A1-26: a fade blends the picture it kept. The group of `fade_scene` fades out and in:
    /// opacity 1.0, 0.8, 0.6, 0.4, 0.2, 0.5, 1.0. Every frame is, pixel for pixel, the frame
    /// painted afresh; on one thread and in bands (900 x 700 on 4 threads: 3 bands of 234 rows,
    /// and the group's rows 80 .. 480 reach all three).
    ///
    /// - 1.0: the group is not blended (its text may be LCD): painted, nothing looked up.
    /// - 0.8: painted and kept (a miss per pass).
    /// - 0.6, 0.4, 0.2, 0.5: blended from the kept picture (a hit per pass); nothing of the
    ///   group is painted: on one thread the walk paints exactly the items outside the group.
    /// - 1.0: painted again, nothing looked up.
    ///
    /// The kept surface is the group's box, 500 x 400 px = 800,000 bytes.
    /// Mutations: the group's own opacity key counted among the values inside the group
    /// (`bind_inside` before the push) makes every step a miss; a hit that blends with the
    /// opacity the picture was kept under shows 0.8 at 0.6; a hit that does not step past the
    /// pop paints the group's items over the blended picture.
    #[test]
    fn a_fade_blends_the_picture_it_kept_and_it_is_the_frame_painted_afresh() {
        let font = test_font();
        let (rr, fm, font_hash) = resources(font.as_ref());
        let scene = fade_scene(font.as_ref().zip(font_hash), 0);
        let push = scene.items.iter().position(|i| matches!(i, DisplayListItem::PushOpacity { .. })).unwrap();
        let pop = scene.items.iter().position(|i| matches!(i, DisplayListItem::PopOpacity)).unwrap();
        let outside = (scene.items.len() - (pop - push + 1)) as u64;
        for threads in [1usize, 4] {
            let passes = if threads == 1 { 1 } else { 3 };
            let retained = Arc::new(RetainedFrames::with_max_bytes(64 << 20));
            let mut pm = AzulPixmap::new(900, 700).unwrap();
            pm.fill(255, 255, 255, 255);
            let steps: [(f32, (u64, u64)); 7] = [
                (1.0, (0, 0)),
                (0.8, (0, passes)),
                (0.6, (passes, 0)),
                (0.4, (passes, 0)),
                (0.2, (passes, 0)),
                (0.5, (passes, 0)),
                (1.0, (0, 0)),
            ];
            for (step, (opacity, hits_misses)) in steps.into_iter().enumerate() {
                let state = fade_state(&scene, opacity, 0.0, Some(&retained), threads);
                let counted = fade_frame(&scene, &state, &mut pm, (&rr, &fm));
                let what = format!("{threads} thread(s), step {} (opacity {opacity})", step + 1);
                assert_eq!((counted.retained_hits, counted.retained_misses), hits_misses, "{what}: {counted:?}");
                if hits_misses.0 > 0 && threads == 1 {
                    assert_eq!(counted.items_painted, outside, "{what}: nothing of the group is painted: {counted:?}");
                    assert_eq!(counted.text_runs, 0, "{what}: no text of the group: {counted:?}");
                }
                if hits_misses.1 > 0 && threads == 1 {
                    assert_eq!(counted.items_painted, scene.items.len() as u64, "{what}: the group is painted: {counted:?}");
                }
                let fresh = fresh_fade_frame(&scene, opacity, 0.0, (&rr, &fm));
                assert_eq!(differing_pixels(&pm, &fresh), 0, "{what}: the frame is the one painted afresh");
            }
            assert_eq!((retained.len(), retained.bytes()), (1, 500 * 400 * 4), "{threads} thread(s): the group's box");
        }
        // Harness: the fade shows (the panel at 0.2 is not the panel at 0.8), and so does the group.
        let (strong, weak) = (fresh_fade_frame(&scene, 0.8, 0.0, (&rr, &fm)), fresh_fade_frame(&scene, 0.2, 0.0, (&rr, &fm)));
        assert!(differing_pixels(&strong, &weak) > 100_000);
        // The bar painted after the group lies over it: (200, 470) is the bar's colour.
        assert_eq!(px(&weak, 200, 470), [20, 20, 20, 255]);
    }

    /// A1-26, the invalidation rules of a kept group, each against the frame painted afresh.
    /// After a frame at 0.8 that keeps the group's picture:
    /// - the control: a frame at 0.6 blends it (a hit);
    /// - rule 5, a value inside the group: the card's matrix is written (moved by 30 px): the
    ///   group is painted again and the picture kept for it replaces the old one;
    /// - rule 1, the list: a cell OUTSIDE the group changes colour (a tile arrives during the
    ///   fade) or the panel INSIDE it does (an edit): the picture is dropped with its list (it
    ///   is not repaired), the group is painted and kept again, and the next step blends it;
    /// - no cache (`AZ_CPU_RETAINED_MB=0`): painted, nothing kept.
    /// Mutations: `values` without the keys inside the group shows the card where it was; an
    /// entry that outlives its list shows the old panel.
    #[test]
    fn a_kept_group_is_painted_again_when_what_it_shows_changed() {
        let font = test_font();
        let (rr, fm, font_hash) = resources(font.as_ref());
        let font = font.as_ref().zip(font_hash);
        let kept_scene = || {
            let scene = fade_scene(font, 0);
            let retained = Arc::new(RetainedFrames::with_max_bytes(64 << 20));
            let mut pm = AzulPixmap::new(900, 700).unwrap();
            pm.fill(255, 255, 255, 255);
            let first = fade_frame(&scene, &fade_state(&scene, 0.8, 0.0, Some(&retained), 1), &mut pm, (&rr, &fm));
            assert_eq!((first.retained_hits, first.retained_misses, retained.len()), (0, 1, 1));
            (scene, retained, pm)
        };

        // The control.
        let (scene, retained, mut pm) = kept_scene();
        let next = fade_frame(&scene, &fade_state(&scene, 0.6, 0.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!((next.retained_hits, next.retained_misses), (1, 0), "{next:?}");
        assert_eq!(differing_pixels(&pm, &fresh_fade_frame(&scene, 0.6, 0.0, (&rr, &fm))), 0);

        // Rule 5: a value inside the group.
        let (scene, retained, mut pm) = kept_scene();
        let moved = fade_frame(&scene, &fade_state(&scene, 0.6, 30.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!((moved.retained_hits, moved.retained_misses), (0, 1), "the card moved: {moved:?}");
        assert_eq!(moved.retained_invalidations, 1, "the picture of the card's old place is replaced: {moved:?}");
        let fresh = fresh_fade_frame(&scene, 0.6, 30.0, (&rr, &fm));
        assert_eq!(differing_pixels(&pm, &fresh), 0, "the card 30 px to the right");
        assert!(differing_pixels(&fresh, &fresh_fade_frame(&scene, 0.6, 0.0, (&rr, &fm))) > 500, "harness: the move shows");
        assert_eq!(retained.len(), 1);
        let after = fade_frame(&scene, &fade_state(&scene, 0.4, 30.0, Some(&retained), 1), &mut pm, (&rr, &fm));
        assert_eq!((after.retained_hits, after.retained_misses), (1, 0), "{after:?}");
        assert_eq!(differing_pixels(&pm, &fresh_fade_frame(&scene, 0.4, 30.0, (&rr, &fm))), 0);

        // Rule 1: the list, changed outside the group and inside it.
        for edit in [1u8, 2] {
            let (scene, retained, mut pm) = kept_scene();
            assert_eq!(Arc::strong_count(&scene), 2, "the kept picture holds its list");
            let edited = fade_scene(font, edit);
            let frame = fade_frame(&edited, &fade_state(&edited, 0.6, 0.0, Some(&retained), 1), &mut pm, (&rr, &fm));
            assert_eq!((frame.retained_patched, frame.retained_invalidations), (0, 1), "edit {edit}: dropped: {frame:?}");
            assert_eq!((frame.retained_hits, frame.retained_misses), (0, 1), "edit {edit}: painted: {frame:?}");
            assert_eq!(Arc::strong_count(&scene), 1, "edit {edit}: the old list is let go of");
            let fresh = fresh_fade_frame(&edited, 0.6, 0.0, (&rr, &fm));
            assert_eq!(differing_pixels(&pm, &fresh), 0, "edit {edit}: the edited list");
            assert!(differing_pixels(&fresh, &fresh_fade_frame(&scene, 0.6, 0.0, (&rr, &fm))) > 500, "harness: edit {edit} shows");
            let next = fade_frame(&edited, &fade_state(&edited, 0.4, 0.0, Some(&retained), 1), &mut pm, (&rr, &fm));
            assert_eq!((next.retained_hits, next.retained_misses), (1, 0), "edit {edit}: kept again: {next:?}");
            assert_eq!(differing_pixels(&pm, &fresh_fade_frame(&edited, 0.4, 0.0, (&rr, &fm))), 0);
        }

        // A cache that keeps nothing: the group is no candidate at all.
        let scene = fade_scene(font, 0);
        let none = Arc::new(RetainedFrames::with_max_bytes(0));
        let mut pm = AzulPixmap::new(900, 700).unwrap();
        pm.fill(255, 255, 255, 255);
        for opacity in [0.8_f32, 0.6] {
            let frame = fade_frame(&scene, &fade_state(&scene, opacity, 0.0, Some(&none), 1), &mut pm, (&rr, &fm));
            assert_eq!((frame.retained_hits, frame.retained_misses, none.len()), (0, 0, 0), "{frame:?}");
            assert_eq!(differing_pixels(&pm, &fresh_fade_frame(&scene, opacity, 0.0, (&rr, &fm))), 0);
        }
    }

    /// A1-26: which opacity groups a walk may step over (`FrameIndex::opacity_scope_at`), and
    /// what their pictures depend on. In one list:
    /// - a group around a fill, an inner group (key 5), a frame (key 6) and a bound colour:
    ///   it ends at its pop and depends on the keys 5 and 6 and the colour key 9 - not on its
    ///   own key 4;
    /// - the inner group: ends at its own pop, depends on nothing;
    /// - a group whose pop comes after the pop of a clip that was opened inside it: none;
    /// - a group opened inside a frame and popped after the frame's pop: none.
    /// Each of the last two is refused by more than one rule (the pop did not close its own
    /// push, the group was marked at the pop that went wrong, the scope stack is not as long
    /// as before the push): the test pins the answer, not which rule gives it.
    /// Mutation: a scope recorded for every popped group gives the third one (a walk that
    /// steps over it leaves the clip stack one short) and the fourth (it never pops the frame).
    #[test]
    fn an_opacity_group_is_stepped_over_only_when_it_closes_what_it_opened() {
        let group = |key: usize| DisplayListItem::PushOpacity {
            bounds: wrect(0.0, 0.0, 300.0, 300.0),
            opacity: 0.5,
            opacity_key: Some(azul_core::resources::OpacityKey { id: key }),
            mix_blend_mode: Default::default(),
        };
        let mut items = vec![group(4), fill(0.0, 0.0, 10.0, 10.0, [1, 2, 3]), group(5), fill(0.0, 0.0, 5.0, 5.0, [1, 2, 3]), DisplayListItem::PopOpacity];
        items.extend(frame(6, IDENTITY, (20.0, 20.0), vec![fill(0.0, 0.0, 20.0, 20.0, [9, 9, 9])]));
        items.push(DisplayListItem::PopOpacity);
        let first_end = items.len() - 1;
        // 3: a clip opened inside the group and closed after it.
        let third = items.len();
        items.push(group(10));
        items.push(DisplayListItem::PushClip { bounds: wrect(0.0, 0.0, 50.0, 50.0), border_radius: BorderRadius::default() });
        items.push(DisplayListItem::PopOpacity);
        items.push(DisplayListItem::PopClip);
        // 4: a group opened inside a frame, popped after the frame.
        let frame_push = items.len();
        items.push(DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 12 },
            initial_transform: azul_core::transform::ComputedTransform3D { m: IDENTITY },
            bounds: wrect(0.0, 0.0, 40.0, 40.0),
        });
        let fourth = items.len();
        items.push(group(11));
        items.push(DisplayListItem::PopReferenceFrame);
        items.push(DisplayListItem::PopOpacity);
        let mut dl = list(items);
        dl.color_bindings = vec![None; dl.items.len()];
        dl.color_bindings[1] = Some(azul_core::resources::ColorKey { id: 9 });
        let index = dl.frame_index();
        let first = index.opacity_scope_at(0).expect("the first group closes what it opened");
        assert_eq!(first.end, first_end);
        assert_eq!((first.inner.opacity_keys.clone(), first.inner.transform_keys.clone(), first.inner.color_keys.clone()), (vec![5], vec![6], vec![9]));
        assert!(first.inner.scroll_ids.is_empty() && !first.inner.has_view);
        let inner = index.opacity_scope_at(2).expect("the group inside it");
        assert_eq!((inner.end, inner.inner.clone()), (4, crate::solver3::display_list::FrameInner::default()));
        assert!(index.opacity_scope_at(third).is_none(), "its pop closes a clip's scope");
        assert!(index.opacity_scope_at(fourth).is_none(), "the frame around its push ends inside it");
        assert!(index.opacity_scope_at(1).is_none() && index.opacity_scope_at(frame_push).is_none(), "no group is pushed there");
    }

    // ---- A1-07b: the grids of a list's levels -----------------------------------------------

    /// A fixed sequence of numbers per seed.
    struct Lcg(u64);

    impl Lcg {
        fn below(&mut self, n: u32) -> u32 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) % u64::from(n.max(1))) as u32
        }

        /// A coordinate below `max`, on quarter pixels.
        fn coord(&mut self, max: u32) -> f32 {
            self.below(max * 4) as f32 / 4.0
        }

        fn rgb(&mut self) -> [u8; 3] {
            [self.below(256) as u8, self.below(256) as u8, self.below(256) as u8]
        }

        /// `count` fills of up to 62 x 42 px with their corners inside `(x, y) w x h`.
        fn fills(&mut self, count: usize, (x, y, w, h): (f32, f32, u32, u32)) -> Vec<DisplayListItem> {
            (0..count)
                .map(|_| {
                    let (fx, fy) = (x + self.coord(w), y + self.coord(h));
                    let (fw, fh) = (self.coord(60) + 2.0, self.coord(40) + 2.0);
                    let rgb = self.rgb();
                    fill(fx, fy, fw, fh, rgb)
                })
                .collect()
        }
    }

    /// A list of about 2,000 items over (0, 0) 1600 x 1400 with every kind of scope a level
    /// can hold, from the sequence of `seed`, and the live values it is painted with (scroll
    /// offsets, matrices): 260 loose fills, then forty blocks of thirty more loose fills or of
    /// one scope: clips (square and rounded), opacity groups, text shadows, moved frames,
    /// scroll frames in their clips, masks; then a clip around 320 fills (a level with a grid
    /// inside the top level's), a frame scaled by 1.3 around 330 fills (a level whose items
    /// are culled by the clip of the frame's surface), a box shadow and two large fills.
    fn random_list(seed: u64) -> (DisplayList, ScrollOffsetMap, HashMap<usize, azul_core::transform::ComputedTransform3D>) {
        let mut rng = Lcg(seed);
        let mut scrolls = ScrollOffsetMap::new();
        let mut transforms = HashMap::new();
        let world = (0.0, 0.0, 1600, 1400);
        let mut items = vec![fill(0.0, 0.0, 1600.0, 1400.0, [236, 233, 225])];
        // (So that the top level has a grid whatever the sequence deals.)
        items.extend(rng.fills(260, world));
        for block in 0..40usize {
            let (bx, by) = (rng.coord(1400), rng.coord(1200));
            let inside = (bx, by, 200, 150);
            match rng.below(9) {
                0..=2 => items.extend(rng.fills(30, world)),
                3 => {
                    let r = if rng.below(2) == 0 { 0.0 } else { 12.0 };
                    items.push(DisplayListItem::PushClip {
                        bounds: wrect(bx, by, 200.0, 150.0),
                        border_radius: BorderRadius { top_left: r, top_right: r, bottom_left: r, bottom_right: r },
                    });
                    items.extend(rng.fills(20, (bx - 30.0, by - 30.0, 260, 210)));
                    items.push(DisplayListItem::PopClip);
                }
                4 => {
                    items.push(DisplayListItem::PushOpacity {
                        bounds: wrect(bx, by, 200.0, 150.0),
                        opacity: 0.6,
                        opacity_key: None,
                        mix_blend_mode: Default::default(),
                    });
                    items.extend(rng.fills(10, inside));
                    items.push(DisplayListItem::PopOpacity);
                }
                5 => {
                    let (blur, spread) = (rng.coord(6), rng.coord(4));
                    items.extend(haloed(halo(blur, spread, [255, 255, 255]), rng.fills(10, inside)));
                }
                6 => {
                    let key = 100 + block;
                    transforms.insert(
                        key,
                        azul_core::transform::ComputedTransform3D {
                            m: turn_scale_move(0.0, 1.0, rng.coord(80) - 40.0, rng.coord(60) - 30.0),
                        },
                    );
                    items.push(DisplayListItem::PushStackingContext { z_index: 0, bounds: wrect(bx, by, 200.0, 150.0) });
                    items.push(DisplayListItem::PushReferenceFrame {
                        transform_key: azul_core::resources::TransformKey { id: key },
                        initial_transform: azul_core::transform::ComputedTransform3D { m: IDENTITY },
                        bounds: wrect(bx, by, 200.0, 150.0),
                    });
                    items.extend(rng.fills(10, inside));
                    items.push(DisplayListItem::PopReferenceFrame);
                    items.push(DisplayListItem::PopStackingContext);
                }
                7 => {
                    let id = 500 + block as u64;
                    scrolls.insert(id, (rng.coord(40), rng.coord(300)));
                    items.push(DisplayListItem::PushClip { bounds: wrect(bx, by, 200.0, 150.0), border_radius: BorderRadius::default() });
                    items.push(DisplayListItem::PushScrollFrame {
                        clip_bounds: wrect(bx, by, 200.0, 150.0),
                        content_size: LogicalSize { width: 240.0, height: 450.0 },
                        scroll_id: id,
                    });
                    items.extend(rng.fills(20, (bx, by, 240, 450)));
                    items.push(DisplayListItem::PopScrollFrame);
                    items.push(DisplayListItem::PopClip);
                }
                _ => {
                    items.push(DisplayListItem::PushImageMaskClip {
                        bounds: wrect(bx, by, 200.0, 150.0),
                        mask_image: None,
                        mask_rect: wrect(bx + 20.0, by + 20.0, 120.0, 90.0),
                        geometry: None,
                    });
                    items.extend(rng.fills(5, inside));
                    items.push(DisplayListItem::PopImageMaskClip);
                }
            }
        }
        items.push(DisplayListItem::PushClip { bounds: wrect(100.0, 60.0, 1300.0, 1100.0), border_radius: BorderRadius::default() });
        items.extend(rng.fills(320, world));
        items.push(DisplayListItem::PopClip);
        transforms.insert(900, azul_core::transform::ComputedTransform3D { m: turn_scale_move(0.0, 1.3, 40.0, 30.0) });
        items.push(DisplayListItem::PushReferenceFrame {
            transform_key: azul_core::resources::TransformKey { id: 900 },
            initial_transform: azul_core::transform::ComputedTransform3D { m: IDENTITY },
            bounds: wrect(0.0, 0.0, 1000.0, 900.0),
        });
        items.extend(rng.fills(330, (0.0, 0.0, 940, 860)));
        items.push(DisplayListItem::PopReferenceFrame);
        items.push(DisplayListItem::BoxShadow {
            bounds: wrect(300.0, 200.0, 160.0, 60.0),
            shadow: StyleBoxShadow { color: ColorU { r: 0, g: 0, b: 0, a: 160 }, ..halo(9.0, 2.0, [0, 0, 0]) },
            border_radius: BorderRadius::default(),
        });
        items.push(fill(20.0, 640.0, 1500.0, 12.0, [10, 10, 10]));
        items.push(fill(700.0, 0.0, 14.0, 1400.0, [10, 10, 10]));
        (list(items), scrolls, transforms)
    }

    /// The damage rects `damage` of `dl` repainted into a white 900 x 700 frame on one thread,
    /// with the levels' grids or (`grid` false) with every item tested: the frame, the items
    /// handed to the painter (list and index, in order), and what was counted.
    fn traced_repaint(
        dl: &DisplayList,
        state: &CpuRenderState,
        damage: &[LogicalRect],
        dpi: f32,
        grid: bool,
    ) -> (AzulPixmap, Vec<(usize, usize)>, crate::cpu_counters::CpuFrameCounters) {
        let (rr, fm) = (RendererResources::default(), empty_font_manager());
        let mut pm = AzulPixmap::new(900, 700).unwrap();
        pm.fill(255, 255, 255, 255);
        let mut gc = GlyphCache::new();
        GRID_OFF.with(|c| c.set(!grid));
        PAINT_TRACE.with(|trace| *trace.borrow_mut() = Some(Vec::new()));
        crate::cpu_counters::reset();
        let painted = render_display_list_damaged(dl, &mut pm, dpi, &rr, &fm, &mut gc, state, damage);
        let counted = crate::cpu_counters::take();
        let trace = PAINT_TRACE.with(|trace| trace.borrow_mut().take()).unwrap_or_default();
        GRID_OFF.with(|c| c.set(false));
        painted.unwrap();
        (pm, trace, counted)
    }

    /// A1-07b: a walk that asks the grids of a list's levels hands the painter the items the
    /// walk that tests every item hands it, in the same order, and paints the same pixels.
    /// Three lists from fixed sequences (`random_list`), each as a window's own list (culled
    /// by the damage rect; by the clip of its surface inside the scaled frame) at a scale
    /// factor of 1 and of 2, and as a view's list (culled by the view's clip), under twelve
    /// damage rects each: positions and sizes from the sequence, up to 300 x 200 px.
    /// The lists' top level, the clip around 320 fills and the scaled frame around 330 have a
    /// grid; over all the rects the grids make the walk visit less than a third of the items.
    /// Mutations: a rect that is not moved by the walk's offset (`level_cull_rect`) loses
    /// the content of scroll frames and of the view; a cursor that does not wait for the end
    /// of a child it entered steps out of a clip without its pop (the clip stack is one short
    /// and later items are cut); a level's rect taken before the push's own clip, offset or
    /// shadow is on the stacks loses the items a moved frame brings into the rect.
    #[test]
    fn a_walk_through_the_grids_paints_the_items_of_the_walk_that_tests_every_item() {
        let (mut walked_grid, mut walked_scan, mut painted_items) = (0u64, 0u64, 0usize);
        for seed in [11u64, 12, 13] {
            let (dl, scrolls, transforms) = random_list(seed);
            let grid = crate::cpurender::item_grid::ItemGrid::of(&dl.items);
            assert!(grid.has_levels(), "seed {seed}: the list is a tree and its top level has a grid");
            let child = Arc::new(dl);
            let host = Arc::new(list(vec![
                fill(0.0, 0.0, 900.0, 700.0, [200, 200, 200]),
                DisplayListItem::VirtualView {
                    child_dom_id: azul_core::dom::DomId { inner: 1 },
                    bounds: wrect(50.0, 40.0, 800.0, 600.0),
                    clip_rect: wrect(50.0, 40.0, 800.0, 600.0),
                    content_offset: LogicalPosition { x: 120.5, y: 80.25 },
                },
            ]));
            let state_of = |as_view: bool| {
                let mut state = CpuRenderState::new(scrolls.clone()).with_paint_threads(1);
                state.transforms = transforms.clone();
                if as_view {
                    state = state
                        .with_virtual_view_display_lists(
                            [(azul_core::dom::DomId { inner: 1 }, Arc::clone(&child))].into_iter().collect(),
                        )
                        .with_root_display_list(Arc::clone(&host));
                }
                state
            };
            let mut rng = Lcg(seed * 977);
            for (as_view, dpi) in [(false, 1.0_f32), (false, 2.0), (true, 1.0)] {
                let state = state_of(as_view);
                let painted_list: &DisplayList = if as_view { &host } else { &child };
                for _ in 0..12 {
                    let (w, h) = (rng.coord(300) + 4.0, rng.coord(200) + 4.0);
                    let damage = [rect(rng.coord(800) / dpi, rng.coord(600) / dpi, w / dpi, h / dpi)];
                    let (with_grid, trace, counted) = traced_repaint(painted_list, &state, &damage, dpi, true);
                    let (scanned, trace_scan, counted_scan) = traced_repaint(painted_list, &state, &damage, dpi, false);
                    let what = format!("seed {seed}, view {as_view}, dpi {dpi}, damage {:?}", damage[0]);
                    assert_eq!(trace, trace_scan, "{what}: the same items in the same order");
                    assert_eq!(differing_pixels(&with_grid, &scanned), 0, "{what}: the same pixels");
                    assert!(counted.items_walked <= counted_scan.items_walked, "{what}: never more items visited");
                    walked_grid += counted.items_walked;
                    walked_scan += counted_scan.items_walked;
                    painted_items += trace.len();
                }
            }
        }
        assert!(painted_items > 1000, "the comparison is not vacuous: {painted_items} items were painted");
        assert!(walked_grid * 3 < walked_scan, "the grids are asked: {walked_grid} items visited against {walked_scan}");
    }

    /// A page of `n` rows of 24 px in a scroll frame (id 5): a ground, a clip, the scroll
    /// frame, and per row a stacking context around two fills.
    fn page(n: usize) -> DisplayList {
        let mut items = vec![fill(0.0, 0.0, 900.0, 700.0, [250, 250, 250])];
        items.push(DisplayListItem::PushClip { bounds: wrect(0.0, 0.0, 900.0, 700.0), border_radius: BorderRadius::default() });
        items.push(DisplayListItem::PushScrollFrame {
            clip_bounds: wrect(0.0, 0.0, 900.0, 700.0),
            content_size: LogicalSize { width: 900.0, height: n as f32 * 24.0 },
            scroll_id: 5,
        });
        for row in 0..n {
            let y = row as f32 * 24.0;
            items.push(DisplayListItem::PushStackingContext { z_index: 0, bounds: wrect(10.0, y, 710.0, 20.0) });
            items.push(fill(10.0, y, 600.0, 20.0, [(row % 200) as u8, 90, 160]));
            items.push(fill(620.0, y + 2.0, 100.0, 16.0, [30, (row % 150) as u8, 60]));
            items.push(DisplayListItem::PopStackingContext);
        }
        items.push(DisplayListItem::PopScrollFrame);
        items.push(DisplayListItem::PopClip);
        list(items)
    }

    /// A1-07b, the size sweep: under a fixed small damage rect the walk visits the same few
    /// items of a page of 500 rows and of one of 2,000. The page is scrolled by 300 px; the
    /// damage (40, 200) 100 x 30 shows the content's y 500 .. 530: the rows 20, 21 and 22
    /// (y 480, 504, 528; row 20 ends at 500 and touches the rect), of each the first fill
    /// (x 10 .. 610; the second one, from x 620, is tested and culled). The walk visits the
    /// ground, the clip's and the scroll frame's push and pop, and the four items of each of
    /// those rows: 17 items, whatever the page's length. The walk that tests every item
    /// visits 4 n + 5.
    /// Mutation: a grid whose cells grow with the list (a fixed number of cells) hands out
    /// four times the rows for four times the page.
    #[test]
    fn four_times_the_rows_do_not_cost_a_small_repaint_more() {
        let damage = [rect(40.0, 200.0, 100.0, 30.0)];
        let mut state = CpuRenderState::new(ScrollOffsetMap::new()).with_paint_threads(1);
        state.scroll_offsets.insert(5, (0.0, 300.0));
        let mut walked = Vec::new();
        for n in [500usize, 2000] {
            let dl = page(n);
            let (with_grid, trace, counted) = traced_repaint(&dl, &state, &damage, 1.0, true);
            let (scanned, trace_scan, counted_scan) = traced_repaint(&dl, &state, &damage, 1.0, false);
            assert_eq!(trace, trace_scan, "n = {n}");
            assert_eq!(differing_pixels(&with_grid, &scanned), 0, "n = {n}");
            // The ground and the first fill of the rows 20 .. 22 (a row is four items from
            // index 3 on: a push, two fills, a pop).
            let wanted: Vec<usize> = vec![0, 3 + 20 * 4 + 1, 3 + 21 * 4 + 1, 3 + 22 * 4 + 1];
            assert_eq!(trace.iter().map(|(_, index)| *index).collect::<Vec<_>>(), wanted, "n = {n}");
            assert_eq!(counted_scan.items_walked, 4 * n as u64 + 5, "n = {n}: every item is tested");
            assert_eq!(counted.items_walked, 17, "n = {n}: the three rows and what is around them");
            assert_ne!(px(&with_grid, 100, 215), [255, 255, 255, 255], "n = {n}: the rows are painted");
            walked.push(counted.items_walked);
        }
        assert!(walked[1] * 2 < walked[0] * 3, "four times the rows, under 1.5 times the items: {walked:?}");
    }

    /// A1-07b: the bands of a repaint ask the grids with their own rects, and the frame is the
    /// one a single pass paints while testing every item. The whole 900 x 700 frame of a
    /// `random_list` on four threads (three bands of 234 rows: each asks for a third of the
    /// rows) against one pass without grids.
    /// Mutation: a cursor made from the frame's rect instead of the band's paints the same
    /// pixels but visits every child in every band (the count); a grid that is not shared
    /// read-only between the band threads does not compile.
    #[test]
    fn bands_that_ask_the_grids_paint_the_frame_of_one_pass_that_tests_every_item() {
        let (rr, fm) = (RendererResources::default(), empty_font_manager());
        let (dl, scrolls, transforms) = random_list(21);
        let whole = [rect(0.0, 0.0, 900.0, 700.0)];
        let paint = |threads: usize, grid: bool| {
            let mut state = CpuRenderState::new(scrolls.clone()).with_paint_threads(threads);
            state.transforms = transforms.clone();
            let mut pm = AzulPixmap::new(900, 700).unwrap();
            pm.fill(255, 255, 255, 255);
            let mut gc = GlyphCache::new();
            GRID_OFF.with(|c| c.set(!grid));
            crate::cpu_counters::reset();
            let painted = render_display_list_damaged(&dl, &mut pm, 1.0, &rr, &fm, &mut gc, &state, &whole);
            GRID_OFF.with(|c| c.set(false));
            painted.unwrap();
            (pm, crate::cpu_counters::take())
        };
        let (banded, counted) = paint(4, true);
        let (banded_scan, counted_banded_scan) = paint(4, false);
        let (single, counted_single) = paint(1, false);
        assert_eq!((counted.bands, counted_single.bands), (3, 0));
        assert_eq!(differing_pixels(&banded, &single), 0, "bands with grids against one pass without");
        assert_eq!(differing_pixels(&banded_scan, &single), 0, "premise: the bands are the pass");
        assert!(counted.items_walked < counted_banded_scan.items_walked, "a band visits what its rows can show: {counted:?}");
        let coloured = single.data().chunks_exact(4).filter(|p| p[..3] != [236, 233, 225]).count();
        assert!(coloured > 100_000, "the frame shows the list ({coloured} px)");
    }

    /// A1-63: a shape filled under a clip is, inside the clip, the shape filled without one,
    /// pixel for pixel, also when its edges are long and slanted. A quad like a turned
    /// label's fill (its corners at (60.3, 60.1), (201.2, 111.4), (190.9, 139.6) and
    /// (49.7, 88.2)) under six clips - a strip across it, a column at its left end, three
    /// pixels on an edge, each half of the frame, a thin band: inside the clip the pixels of
    /// the fill without a clip, outside it nothing.
    /// Mutation: the geometry cut 32 px outside the pass's clip (as it was) cuts the quad's
    /// long edges at x = 68 for the strip from x = 100, and what is left of each edge runs
    /// a hair off the true one: a level off in a few pixels along it
    /// (`a_turned_label_of_a_view_is_drawn_without_a_surface`, damaged != full, 5 px).
    #[test]
    fn a_long_slanted_edge_under_a_clip_is_the_edge_of_the_full_paint() {
        let fill_colour = Rgba8::new(20, 60, 200, 255);
        let paint = |clip: Option<AzRect>| {
            let mut pm = AzulPixmap::new(400, 300).unwrap();
            pm.fill(255, 255, 255, 255);
            let mut path = PathStorage::new();
            path.move_to(60.3, 60.1);
            path.line_to(201.2, 111.4);
            path.line_to(190.9, 139.6);
            path.line_to(49.7, 88.2);
            path.close_polygon(PATH_FLAGS_NONE);
            agg_fill_path_clipped(&mut pm, &mut path, &fill_colour, FillingRule::NonZero, clip);
            pm
        };
        let whole = paint(None);
        let filled = whole.data().chunks_exact(4).filter(|p| p[..3] != [255, 255, 255]).count();
        assert!(filled > 3500, "the quad of about 150 x 30 px is there ({filled} px)");
        for (x, y, w, h) in [(100u32, 60u32, 130u32, 90u32), (70, 50, 20, 100), (150, 100, 3, 3), (0, 0, 120, 100), (130, 0, 270, 300), (95, 75, 40, 12)] {
            let clipped = paint(AzRect::from_xywh(x as f32, y as f32, w as f32, h as f32));
            let mut differing = 0usize;
            for cy in 0..300u32 {
                for cx in 0..400u32 {
                    let inside = cx >= x && cx < x + w && cy >= y && cy < y + h;
                    let want = if inside { px(&whole, cx, cy) } else { [255, 255, 255, 255] };
                    differing += usize::from(px(&clipped, cx, cy) != want);
                }
            }
            assert_eq!(differing, 0, "under the clip ({x}, {y}) {w} x {h}");
        }
    }

    /// A1-65: nothing is handed to the painter under a clip of no area. The case the grids'
    /// test found (seed 11, as a view, damage 44.5 x 106.5 @ (495.5, 254.5), item 754): a
    /// fill at (1153.75, 930.25) 59.75 x 18.5 of the view's list is painted at (1324.25,
    /// 1050.5) (the view's origin and offset add (170.5, 120.25)); it lies in a clip that is
    /// on screen at (1330, 1054.5) 200 x 150, far outside the damage's clip (495, 254)
    /// 45 x 107. What `intersect_clips` leaves of the two is a rect of no area at (1330,
    /// 1054.5) - and the fill holds that point, so the closed test against the clip let it
    /// through: an item painted by the walk that tests every item (nothing of it drawn), and
    /// rightly stepped over by the walk that asks the grid.
    /// Mutation: without the test for an empty clip the first assertion fails.
    #[test]
    fn an_item_under_a_clip_of_no_area_is_culled() {
        let item = fill(1153.75, 930.25, 59.75, 18.5, [1, 2, 3]);
        let offset = (-170.5, -120.25);
        let merged = intersect_clips(
            AzRect::from_xywh(495.0, 254.0, 45.0, 107.0),
            AzRect::from_xywh(1330.0, 1054.5, 200.0, 150.0),
        );
        assert!(merged.is_some_and(|c| c.width == 0.0 && c.height == 0.0), "premise: {merged:?}");
        assert!(outside_active_clip(&item, &[], offset, merged, &[], 1.0), "no area: nothing is painted");
        // A clip with an area culls by the bounds, as before.
        assert!(!outside_active_clip(&item, &[], offset, AzRect::from_xywh(1300.0, 1040.0, 100.0, 100.0), &[], 1.0));
        assert!(outside_active_clip(&item, &[], offset, AzRect::from_xywh(0.0, 0.0, 100.0, 100.0), &[], 1.0));
        assert!(!outside_active_clip(&item, &[], offset, None, &[], 1.0), "no clip: nothing is culled");
    }
}
