//! Draws an HVIF icon (`azul_core::hvif`) at a pixel size, the way Haiku's
//! icon renderer does (with AGG, as Haiku's): only the shapes whose level of
//! detail fits the size, HINTED shapes snapped to whole pixels (a 16 px icon
//! stays crisp where a scaled vector drawing goes soft), gradients as
//! gradients, a stroke transformer as a stroke.

use agg_rust::{
    basics::{FillingRule, VertexSource, PATH_CMD_STOP},
    color::Rgba8,
    conv_contour::ConvContour,
    conv_curve::ConvCurve,
    conv_stroke::ConvStroke,
    conv_transform::ConvTransform,
    gradient_lut::GradientLut,
    math_stroke::{LineCap as AggCap, LineJoin as AggJoin},
    path_storage::PathStorage,
    span_gradient::{
        GradientConic, GradientDiamond, GradientRadial, GradientSqrtXY, GradientX, GradientXY,
    },
    trans_affine::TransAffine,
};
use azul_core::hvif::{
    HvifAffine, HvifGradient, HvifGradientKind, Hvif, HvifLineCap, HvifLineJoin, HvifPath, HvifRgba, HvifShape, HvifStyle, HvifTransformer,
    GRID,
};

use super::{agg_fill_gradient_clipped, agg_fill_path_clipped, AzulPixmap};

/// The largest size an HVIF icon is drawn at (pixels a side).
pub const MAX_HVIF_SIZE: u32 = 1024;

/// `icon` drawn `size` x `size` device pixels on a transparent backdrop,
/// AGG's premultiplied RGBA. `None` for a size of 0 or above
/// [`MAX_HVIF_SIZE`].
#[must_use]
pub fn render_hvif(icon: &Hvif, size: u32) -> Option<AzulPixmap> {
    if size == 0 || size > MAX_HVIF_SIZE {
        return None;
    }
    let mut pixmap = AzulPixmap::new(size, size)?;
    pixmap.fill(0, 0, 0, 0);
    let scale = size as f32 / GRID;
    let global = TransAffine::new_scaling_uniform(f64::from(scale));
    for shape in &icon.shapes {
        if !shape.visible_at(scale) {
            continue;
        }
        let Some(style) = icon.styles.get(shape.style) else {
            continue;
        };
        draw_shape(&mut pixmap, icon, shape, style, &global);
    }
    Some(pixmap)
}

/// The HVIF icon in `bytes`, drawn `size` x `size` as a premultiplied RGBA8
/// image.
///
/// # Errors
///
/// When the bytes are no HVIF icon or the size is out of range.
pub fn render_hvif_to_raw_image(
    bytes: &[u8],
    size: u32,
) -> Result<azul_core::resources::RawImage, String> {
    let icon = Hvif::parse(bytes).map_err(|e| format!("not an HVIF icon: {e:?}"))?;
    let pixmap = render_hvif(&icon, size).ok_or_else(|| format!("bad icon size {size}"))?;
    Ok(azul_core::resources::RawImage {
        pixels: azul_core::resources::RawImageData::U8(pixmap.data().to_vec().into()),
        width: pixmap.width as usize,
        height: pixmap.height as usize,
        premultiplied_alpha: true,
        data_format: azul_core::resources::RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })
}

fn agg_affine(m: &HvifAffine) -> TransAffine {
    TransAffine::new_custom(
        f64::from(m.a),
        f64::from(m.b),
        f64::from(m.c),
        f64::from(m.d),
        f64::from(m.e),
        f64::from(m.f),
    )
}

fn rgba8(c: HvifRgba) -> Rgba8 {
    Rgba8::new(u32::from(c.r), u32::from(c.g), u32::from(c.b), u32::from(c.a))
}

/// The paths of `shape` in its own space, curves kept (AGG's `curve4`).
fn shape_path(icon: &Hvif, shape: &HvifShape) -> PathStorage {
    let mut storage = PathStorage::new();
    for path in shape.paths.iter().filter_map(|i| icon.paths.get(*i)) {
        add_path(&mut storage, path);
    }
    storage
}

fn add_path(storage: &mut PathStorage, path: &HvifPath) {
    let Some(first) = path.points.first() else {
        return;
    };
    storage.move_to(f64::from(first.x), f64::from(first.y));
    let segment = |storage: &mut PathStorage, from: &azul_core::hvif::HvifPathPoint, to: &azul_core::hvif::HvifPathPoint| {
        let straight = from.out_x == from.x
            && from.out_y == from.y
            && to.in_x == to.x
            && to.in_y == to.y;
        if straight {
            storage.line_to(f64::from(to.x), f64::from(to.y));
        } else {
            storage.curve4(
                f64::from(from.out_x),
                f64::from(from.out_y),
                f64::from(to.in_x),
                f64::from(to.in_y),
                f64::from(to.x),
                f64::from(to.y),
            );
        }
    };
    for pair in path.points.windows(2) {
        segment(storage, &pair[0], &pair[1]);
    }
    if path.closed {
        if let Some(last) = path.points.last() {
            if path.points.len() > 1 {
                segment(storage, last, first);
            }
        }
        storage.close_polygon(0);
    }
}

/// Snaps every vertex to whole device pixels: a hinted shape's edges land on
/// pixel boundaries, so a small icon draws hard edges, not half-covered ones.
struct Hinted<VS: VertexSource> {
    inner: VS,
}

impl<VS: VertexSource> VertexSource for Hinted<VS> {
    fn rewind(&mut self, path_id: u32) {
        self.inner.rewind(path_id);
    }

    fn vertex(&mut self, x: &mut f64, y: &mut f64) -> u32 {
        let cmd = self.inner.vertex(x, y);
        if cmd != PATH_CMD_STOP {
            *x = x.round();
            *y = y.round();
        }
        cmd
    }
}

fn agg_join(join: HvifLineJoin) -> AggJoin {
    match join {
        HvifLineJoin::Miter => AggJoin::Miter,
        HvifLineJoin::MiterRevert => AggJoin::MiterRevert,
        HvifLineJoin::Round => AggJoin::Round,
        HvifLineJoin::Bevel => AggJoin::Bevel,
        HvifLineJoin::MiterRound => AggJoin::MiterRound,
    }
}

fn agg_cap(cap: HvifLineCap) -> AggCap {
    match cap {
        HvifLineCap::Butt => AggCap::Butt,
        HvifLineCap::Square => AggCap::Square,
        HvifLineCap::Round => AggCap::Round,
    }
}

/// One shape: its paths through its transformers (affines, a contour, a
/// stroke - in the path's space, as Haiku applies them), then its transform
/// and the size's scale, then hinting, filled with its style.
fn draw_shape(pixmap: &mut AzulPixmap, icon: &Hvif, shape: &HvifShape, style: &HvifStyle, global: &TransAffine) {
    let mut path = shape_path(icon, shape);
    // HvifAffine transformers go before the shape's own transform.
    let mut to_device = TransAffine::new();
    for transformer in &shape.transformers {
        match transformer {
            HvifTransformer::Affine(m) => {
                to_device.multiply(&agg_affine(m));
            }
            HvifTransformer::Perspective(m) => {
                // Drawn as its affine part.
                to_device.multiply(&TransAffine::new_custom(
                    f64::from(m[0]),
                    f64::from(m[1]),
                    f64::from(m[3]),
                    f64::from(m[4]),
                    f64::from(m[6]),
                    f64::from(m[7]),
                ));
            }
            HvifTransformer::Contour { .. } | HvifTransformer::Stroke { .. } => {}
        }
    }
    to_device.multiply(&agg_affine(&shape.transform));
    to_device.multiply(global);

    let contour = shape.transformers.iter().find_map(|t| match t {
        HvifTransformer::Contour {
            width,
            join,
            miter_limit,
        } => Some((*width, *join, *miter_limit)),
        _ => None,
    });
    let curves = ConvCurve::new(&mut path);
    match (shape.stroke(), contour) {
        (Some((width, join, cap, miter_limit)), _) => {
            let mut stroke = ConvStroke::new(curves);
            stroke.set_width(f64::from(width));
            stroke.set_line_join(agg_join(join));
            stroke.set_line_cap(agg_cap(cap));
            stroke.set_miter_limit(f64::from(miter_limit));
            paint(pixmap, ConvTransform::new(stroke, to_device), shape, style, &to_device);
        }
        (None, Some((width, join, miter_limit))) => {
            let mut grown = ConvContour::new(curves);
            grown.set_width(f64::from(width));
            grown.set_line_join(agg_join(join));
            grown.set_miter_limit(f64::from(miter_limit));
            paint(pixmap, ConvTransform::new(grown, to_device), shape, style, &to_device);
        }
        (None, None) => {
            paint(pixmap, ConvTransform::new(curves, to_device), shape, style, &to_device);
        }
    }
}

/// Fills `outline` (device space) with `style`, hinted when the shape asks.
fn paint<VS: VertexSource>(
    pixmap: &mut AzulPixmap,
    outline: VS,
    shape: &HvifShape,
    style: &HvifStyle,
    to_device: &TransAffine,
) {
    if shape.hinting {
        fill(pixmap, &mut Hinted { inner: outline }, style, to_device);
    } else {
        let mut outline = outline;
        fill(pixmap, &mut outline, style, to_device);
    }
}

fn fill(pixmap: &mut AzulPixmap, outline: &mut dyn VertexSource, style: &HvifStyle, to_device: &TransAffine) {
    match style {
        HvifStyle::Solid(color) => {
            agg_fill_path_clipped(pixmap, outline, &rgba8(*color), FillingRule::NonZero, None);
        }
        HvifStyle::Gradient(gradient) => fill_gradient(pixmap, outline, gradient, to_device),
    }
}

/// A gradient, as Haiku draws it: its space (linear -64..64 along x, the
/// others 0..64 from the centre) through its transform, the shape's and the
/// size's; AGG walks it back from each pixel.
fn fill_gradient(
    pixmap: &mut AzulPixmap,
    outline: &mut dyn VertexSource,
    gradient: &HvifGradient,
    to_device: &TransAffine,
) {
    let mut lut = GradientLut::new(256);
    for stop in &gradient.stops {
        lut.add_color(f64::from(stop.offset) / 255.0, rgba8(stop.color));
    }
    if gradient.stops.len() == 1 {
        // AGG needs two stops: one colour all the way.
        lut.add_color(1.0, rgba8(gradient.stops[0].color));
    }
    lut.build_lut();
    let mut to_gradient = agg_affine(&gradient.transform);
    to_gradient.multiply(to_device);
    to_gradient.invert();
    match gradient.kind {
        HvifGradientKind::Linear => {
            agg_fill_gradient_clipped(pixmap, outline, &lut, GradientX, to_gradient, -64.0, 64.0, None);
        }
        HvifGradientKind::Circular => {
            agg_fill_gradient_clipped(pixmap, outline, &lut, GradientRadial, to_gradient, 0.0, 64.0, None);
        }
        HvifGradientKind::Diamond => {
            agg_fill_gradient_clipped(pixmap, outline, &lut, GradientDiamond, to_gradient, 0.0, 64.0, None);
        }
        HvifGradientKind::Conic => {
            agg_fill_gradient_clipped(pixmap, outline, &lut, GradientConic, to_gradient, 0.0, 64.0, None);
        }
        HvifGradientKind::Xy => {
            agg_fill_gradient_clipped(pixmap, outline, &lut, GradientXY, to_gradient, 0.0, 64.0, None);
        }
        HvifGradientKind::SqrtXy => {
            agg_fill_gradient_clipped(pixmap, outline, &lut, GradientSqrtXY, to_gradient, 0.0, 64.0, None);
        }
    }
}
