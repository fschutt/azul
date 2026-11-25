//! Turns an azul display list into the [`Op`]s of a PDF page.
//!
//! After layout, azul describes a page as a display list: rectangles, borders, gradients, shadows,
//! images, clips and blocks of already shaped text with the position of every glyph. This module
//! converts each item into PDF drawing operations. Layout works in CSS px with the origin at the
//! top left; the output is in points with the origin at the bottom left of the page, shifted by
//! the page margins.
//!
//! Text is written glyph by glyph: each glyph gets its own text matrix (`Tm`) and a `ShowText`
//! of its glyph id, so the line breaking and shaping of the layout engine are kept exactly. Each
//! run is wrapped in an `/ActualText` marked-content span so that copy and paste and text
//! extraction still see the real string.
//!
//! Some features need document-level resources that a list of ops cannot hold: transparency
//! (ExtGState), gradients (shadings) and rasterized effects. The conversion collects these in a
//! [`BridgeResources`], which the caller registers on the document with
//! [`BridgeResources::register_into`]. Images are looked up in a [`ResolvedImages`] table.
//!
//! The entry point is [`display_list_to_printpdf_ops_with_margins`].

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use azul_core::{
    geom::{LogicalRect, LogicalSize, LogicalPosition},
    resources::DecodedImage,
};
use azul_css::props::basic::ColorU;
use azul_layout::{
    solver3::display_list::{DisplayList, DisplayListItem},
    text3::cache::{FontManager, ParsedFontTrait, UnifiedLayout},
};

use crate::{Color, Mm, Op, Pt, Rgb, FontId, RawImage, XObjectId, XObjectTransform,
    ExtendedGraphicsState, ExtendedGraphicsStateId, XObject, PdfWarnMsg};
use crate::shading::{GradientStop, Shading, ShadingGeometry, ShadingId};
use azul_css::props::basic::{
    color::ColorOrSystem,
    geometry::{LayoutPoint, LayoutRect, LayoutSize},
};

/// Decoded `<img>` images by the `src` value in the HTML.
///
/// Each entry holds the [`XObjectId`] the image is registered under on the document and the
/// decoded [`RawImage`], whose pixel size the bridge needs to place it.
pub type ResolvedImages = BTreeMap<String, (XObjectId, RawImage)>;

/// Document-level resources that the converted ops refer to by id.
///
/// The conversion returns a flat list of ops and cannot touch the document, but transparency
/// needs an ExtGState, linear and radial gradients need a shading, and conic gradients and
/// blurred shadows need an image or form XObject. The bridge creates each with a fresh id,
/// writes the op that uses the id and records the definition here. Register them on the document
/// with [`register_into`](Self::register_into) before saving, or the ops refer to resources that
/// do not exist.
#[derive(Default)]
pub struct BridgeResources {
    /// Graphics states for fill and stroke alpha and soft masks.
    pub extgstates: Vec<(ExtendedGraphicsStateId, ExtendedGraphicsState)>,
    /// Axial and radial shadings, for linear and radial gradients.
    pub shadings: Vec<(ShadingId, Shading)>,
    /// Image and form XObjects the bridge made, for conic gradients and blurred shadows.
    pub xobjects: Vec<(XObjectId, XObject)>,
}

impl BridgeResources {
    fn add_extgstate(&mut self, gs: ExtendedGraphicsState) -> ExtendedGraphicsStateId {
        let id = ExtendedGraphicsStateId::new();
        self.extgstates.push((id.clone(), gs));
        id
    }

    fn add_shading(&mut self, shading: Shading) -> ShadingId {
        let id = ShadingId::new();
        self.shadings.push((id.clone(), shading));
        id
    }

    /// Moves the resources into `resources`. Entries with the same id that are already there
    /// are kept.
    pub fn register_into(self, resources: &mut crate::PdfResources) {
        for (id, gs) in self.extgstates {
            resources.extgstates.map.entry(id).or_insert(gs);
        }
        for (id, sh) in self.shadings {
            resources.shadings.map.entry(id).or_insert(sh);
        }
        for (id, xobj) in self.xobjects {
            resources.xobjects.map.entry(id).or_insert(xobj);
        }
    }
}

/// The [`XObjectId`] of the `<img src="...">` image with this `src`.
///
/// The id only depends on the `src` (`HtmlImg_` and the `src` with everything but letters and
/// digits replaced by `_`), so the bridge that writes the `UseXobject` op and the code that
/// registers the image agree without sharing state. Two `src` values that differ only in such
/// characters get the same id.
pub fn image_xobject_id(src_key: &str) -> XObjectId {
    // Avoid characters that are awkward in a PDF name.
    let sanitized: String = src_key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    XObjectId(format!("HtmlImg_{}", sanitized))
}

/// Decodes every entry of an `images` map (`src` to image file bytes) into a [`ResolvedImages`]
/// table. Entries that cannot be decoded are skipped, and their `<img>` draws nothing.
///
/// This decodes the whole map, used or not. The HTML renderers decode only the images a page
/// draws, see [`referenced_image_srcs`].
pub fn resolve_html_images(images: &BTreeMap<String, Vec<u8>>) -> ResolvedImages {
    let mut out = ResolvedImages::new();
    for (key, bytes) in images.iter() {
        if let Ok(raw) = decode_html_image(bytes) {
            out.insert(key.clone(), (image_xobject_id(key), raw));
        }
    }
    out
}

// Every HTML image decode goes through here, so the tests can count them.
fn decode_html_image(bytes: &[u8]) -> Result<RawImage, String> {
    #[cfg(test)]
    {
        HTML_IMAGE_DECODES.with(|n| n.set(n.get() + 1));
    }
    RawImage::decode_from_bytes(bytes, &mut Vec::new())
}

#[cfg(test)]
thread_local! {
    // How many images this thread decoded: tests check that a render decodes only the pictures
    // its pages show.
    pub(crate) static HTML_IMAGE_DECODES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// azul hands an HTML `<img>` to the display list as a `NullImage` whose `tag` is the `src`
// string; the bytes are looked up by it.
fn html_image_src(item: &DisplayListItem) -> Option<String> {
    match item {
        DisplayListItem::Image { image, .. } => match image.get_data() {
            DecodedImage::NullImage { tag, .. } if !tag.is_empty() => {
                String::from_utf8(tag.clone()).ok()
            }
            _ => None,
        },
        _ => None,
    }
}

/// The `src` of every `<img>` the display lists draw, once each. These are the only entries of
/// the images map a render needs.
///
/// CSS `background-image: url(...)` is not included: the renderer gives azul an empty image
/// cache, so such images are not drawn.
pub fn referenced_image_srcs(display_lists: &[DisplayList]) -> BTreeSet<String> {
    display_lists
        .iter()
        .flat_map(|display_list| display_list.items.iter())
        .filter_map(html_image_src)
        .collect()
}

// Where a render looks up the bytes of an `<img src>`: the `XmlRenderOptions::images` map, or
// the `Base64OrRaw` map given to `PdfDocument::from_html*` (decoded only when a page draws it).
pub(crate) trait HtmlImageSource {
    // `None` if there is no entry for `src`, `Some(Err(..))` if it cannot be turned into bytes
    // (bad base64).
    fn image_bytes(&self, src: &str) -> Option<Result<Cow<'_, [u8]>, String>>;
}

impl HtmlImageSource for BTreeMap<String, Vec<u8>> {
    fn image_bytes(&self, src: &str) -> Option<Result<Cow<'_, [u8]>, String>> {
        self.get(src).map(|bytes| Ok(Cow::Borrowed(bytes.as_slice())))
    }
}

impl HtmlImageSource for BTreeMap<String, crate::Base64OrRaw> {
    fn image_bytes(&self, src: &str) -> Option<Result<Cow<'_, [u8]>, String>> {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        self.get(src).map(|entry| match entry {
            crate::Base64OrRaw::Raw(bytes) => Ok(Cow::Borrowed(bytes.as_slice())),
            // Plain standard base64 alphabet.
            crate::Base64OrRaw::B64(text) => match STANDARD.decode(text) {
                Ok(bytes) => Ok(Cow::Owned(bytes)),
                Err(e) => Err(format!("Base64 decode error: {}", e)),
            },
        })
    }
}

// Decodes the images a render draws, once each: every `src` in `referenced`, looked up in
// `images`. The other entries are not touched. A `src` without an entry, or one that cannot be
// read or decoded, is reported in `warnings` and its `<img>` draws nothing.
pub(crate) fn resolve_referenced_html_images(
    referenced: &BTreeSet<String>,
    images: &dyn HtmlImageSource,
    warnings: &mut Vec<PdfWarnMsg>,
) -> ResolvedImages {
    let mut out = ResolvedImages::new();
    for src in referenced {
        let decoded = match images.image_bytes(src) {
            None => Err("no entry with this name in the images map".to_string()),
            Some(Err(e)) => Err(e),
            Some(Ok(bytes)) => decode_html_image(&bytes),
        };
        match decoded {
            Ok(raw) => {
                out.insert(src.clone(), (image_xobject_id(src), raw));
            }
            Err(e) => {
                // A `src` can be a whole data URL: keep the message readable.
                let shown: String = src.chars().take(80).collect();
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!("<img src=\"{}\">: not drawn: {}", shown, e),
                ));
            }
        }
    }
    out
}

use super::border::{
    BorderConfig, extract_border_widths, extract_border_colors, 
    extract_border_styles, extract_border_radii, render_border,
};

/// Convert azul ColorU to printpdf Color
fn convert_color(color: &ColorU) -> Color {
    Color::Rgb(Rgb {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        icc_profile: None,
    })
}

// Opens a `q` scope for a text run whose color is not fully opaque and sets the fill color
// inside it. Returns `false` and emits nothing for opaque colors.
//
// Alpha 0 (`color: transparent`) uses text rendering mode 3 (invisible), so the text stays
// selectable and searchable. Other alpha values get an ExtGState when `bridge_res` is given;
// without it the text is opaque. The caller's `Q` after `ET` ends the scope.
fn begin_translucent_text(
    ops: &mut Vec<Op>,
    bridge_res: Option<&mut BridgeResources>,
    color: &ColorU,
) -> bool {
    if color.a == 255 {
        return false;
    }
    ops.push(Op::SaveGraphicsState);
    ops.push(Op::SetFillColor { col: convert_color(color) });
    match (color.a, bridge_res) {
        (0, _) => ops.push(Op::SetTextRenderingMode { mode: crate::TextRenderingMode::Invisible }),
        (_, Some(res)) => apply_fill_alpha(ops, res, color),
        (_, None) => {}
    }
    true
}

// For a translucent color, creates an ExtGState with that fill and stroke alpha and loads it.
// Call it inside a `q`/`Q` scope so the alpha stays local. Does nothing for opaque colors.
fn apply_fill_alpha(ops: &mut Vec<Op>, bridge_res: &mut BridgeResources, color: &ColorU) {
    if color.a == 255 {
        return;
    }
    let alpha = color.a as f32 / 255.0;
    let mut gs = ExtendedGraphicsState::default();
    gs.set_current_fill_alpha(alpha);
    gs.set_current_stroke_alpha(alpha);
    let id = bridge_res.add_extgstate(gs);
    ops.push(Op::LoadGraphicsState { gs: id });
}

// A clip polygon (`W n`) covering `bounds` (CSS px) with corner radii `[tl, tr, br, bl]` (CSS
// px), in PDF pt. Used to keep a gradient paint inside its element box.
fn make_clip_polygon(
    transform: &CoordTransform,
    bounds: &LogicalRect,
    radii_px: [f32; 4],
    page_height: f32,
    margin_left: f32,
    margin_top: f32,
) -> crate::graphics::Polygon {
    let radii = crate::html::border::BorderRadii {
        top_left: (radii_px[0] * CSS_PX_TO_PT, radii_px[0] * CSS_PX_TO_PT),
        top_right: (radii_px[1] * CSS_PX_TO_PT, radii_px[1] * CSS_PX_TO_PT),
        bottom_right: (radii_px[2] * CSS_PX_TO_PT, radii_px[2] * CSS_PX_TO_PT),
        bottom_left: (radii_px[3] * CSS_PX_TO_PT, radii_px[3] * CSS_PX_TO_PT),
    };
    if radii_px.iter().any(|r| *r > 0.0) {
        let b = bounds_px_to_pt(bounds);
        let points = crate::html::border::create_rounded_rect_path_with_margins(
            b.origin.x, b.origin.y, b.size.width, b.size.height,
            &radii, page_height, margin_left, margin_top,
        );
        crate::graphics::Polygon {
            rings: vec![crate::graphics::PolygonRing { points }],
            mode: crate::graphics::PaintMode::Clip,
            winding_order: crate::graphics::WindingOrder::NonZero,
        }
    } else {
        let x = transform.x(bounds.origin.x);
        let y = transform.rect_y(bounds.origin.y, bounds.size.height);
        let w = transform.dim(bounds.size.width);
        let h = transform.dim(bounds.size.height);
        let mut p = make_rect_polygon_pt(x, y, w, h);
        p.mode = crate::graphics::PaintMode::Clip;
        p
    }
}

// System colors depend on the theme and become opaque black.
fn resolve_stop_color(c: &ColorOrSystem) -> ColorU {
    match c {
        ColorOrSystem::Color(col) => *col,
        ColorOrSystem::System(_) => ColorU { r: 0, g: 0, b: 0, a: 255 },
    }
}

// Per-stop alpha is dropped: PDF axial and radial shadings have no alpha channel.
fn normalize_gradient_stops(
    stops: &azul_css::props::style::background::NormalizedLinearColorStopVec,
) -> Vec<GradientStop> {
    stops
        .iter()
        .map(|s| {
            let c = resolve_stop_color(&s.color);
            GradientStop {
                offset: s.offset.normalized().clamp(0.0, 1.0),
                color: [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0],
            }
        })
        .collect()
}

/// Converts the display list of one page into PDF ops.
///
/// The display list is in CSS px with the origin at the top left of the content area.
/// `page_size` is the full page in points (not the content area) and `margin_left_pt` and
/// `margin_top_pt` the page margins in points; a layout point `(x, y)` in px ends up at
/// `(x * 72/96 + margin_left, page_height - y * 72/96 - margin_top)` on the page. The fonts of the
/// text must be loaded in `font_manager`, and `<img>` images must be in `images`. Resources the
/// ops refer to are added to `bridge_res`, which the caller registers on the document.
///
/// The result is always `Ok`; the `Result` is part of the signature.
pub fn display_list_to_printpdf_ops_with_margins<T: ParsedFontTrait + 'static>(
    display_list: &DisplayList,
    page_size: LogicalSize,
    margin_left_pt: f32,
    margin_top_pt: f32,
    font_manager: &FontManager<T>,
    images: &ResolvedImages,
    bridge_res: &mut BridgeResources,
) -> Result<Vec<Op>, String> {
    let mut ops = Vec::new();
    let page_height = page_size.height;
    let transform = CoordTransform::new(page_height, margin_left_pt, margin_top_pt);

    let mut current_text_layout: Option<(&azul_layout::text3::cache::UnifiedLayout, LogicalRect)> = None;

    // A text block is sequenced as `TextLayout` (the shaped glyphs), then the element backgrounds,
    // then the `Text` runs, so that backgrounds (`<span style="background-color: yellow">`)
    // paint under their text. The bridge draws text from the `TextLayout` item, because the
    // `Text` runs carry no glyph ids. Drawing it at the `TextLayout` would paint the backgrounds
    // over it, so the layout is held here and emitted at the first following `Text` item, or
    // before the next `TextLayout`, the next clip or opacity scope change, or the end of the list.
    let mut pending_text: Option<(&azul_layout::text3::cache::UnifiedLayout, LogicalRect, ColorU)> = None;

    for item in display_list.items.iter() {
        // Flush the deferred text here so that it stays inside the graphics-state scope it was
        // laid out in.
        if matches!(
            item,
            DisplayListItem::TextLayout { .. }
                | DisplayListItem::Text { .. }
                | DisplayListItem::PushClip { .. }
                | DisplayListItem::PopClip
                | DisplayListItem::PushOpacity { .. }
                | DisplayListItem::PopOpacity
        ) {
            if let Some((layout, bounds, color)) = pending_text.take() {
                render_unified_layout_with_margins(
                    &mut ops, layout, &bounds, color, &transform, font_manager, bridge_res,
                );
            }
        }

        if let DisplayListItem::TextLayout { layout, bounds, color, .. } = item {
            if let Some(unified_layout) =
                layout.downcast_ref::<azul_layout::text3::cache::UnifiedLayout>()
            {
                pending_text = Some((unified_layout, *bounds.inner(), *color));
                current_text_layout = Some((unified_layout, *bounds.inner()));
            }
            continue;
        }

        convert_display_list_item_with_margins(
            &mut ops,
            item,
            page_height,
            margin_left_pt,
            margin_top_pt,
            &mut current_text_layout,
            font_manager,
            images,
            bridge_res,
        );
    }

    // A `TextLayout` at the end of the list.
    if let Some((layout, bounds, color)) = pending_text.take() {
        render_unified_layout_with_margins(&mut ops, layout, &bounds, color, &transform, font_manager, bridge_res);
    }

    Ok(ops)
}

/// Like [`display_list_to_printpdf_ops_with_margins`] with no margins and no images.
///
/// Gradients, shadows and transparency need resources that this function drops, so use the
/// version with margins and register the [`BridgeResources`] when the display list has them.
pub fn display_list_to_printpdf_ops<T: ParsedFontTrait + 'static>(
    display_list: &DisplayList,
    page_size: LogicalSize,
    font_manager: &FontManager<T>,
) -> Result<Vec<Op>, String> {
    let mut bridge_res = BridgeResources::default();
    display_list_to_printpdf_ops_with_margins(display_list, page_size, 0.0, 0.0, font_manager, &ResolvedImages::new(), &mut bridge_res)
}

// The layout engine works in CSS px (96 per inch), PDF in points (72 per inch).
const CSS_PX_TO_PT: f32 = 72.0 / 96.0;

#[inline]
fn pt_to_mm(pt: f32) -> Mm {
    Mm::from(Pt(pt))
}

/// Convert a CSS-px `LogicalRect` to PDF-pt `LogicalRect`.
fn bounds_px_to_pt(b: &LogicalRect) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition::new(
            b.origin.x * CSS_PX_TO_PT,
            b.origin.y * CSS_PX_TO_PT,
        ),
        size: LogicalSize::new(
            b.size.width * CSS_PX_TO_PT,
            b.size.height * CSS_PX_TO_PT,
        ),
    }
}

/// Build a filled rectangle polygon from PDF-pt coordinates.
fn make_rect_polygon_pt(x: f32, y: f32, w: f32, h: f32) -> crate::graphics::Polygon {
    let lp = |px: f32, py: f32| crate::graphics::LinePoint {
        p: crate::graphics::Point::new(pt_to_mm(px), pt_to_mm(py)),
        bezier: false,
    };
    crate::graphics::Polygon {
        rings: vec![crate::graphics::PolygonRing {
            points: vec![
                lp(x, y),
                lp(x + w, y),
                lp(x + w, y + h),
                lp(x, y + h),
            ],
        }],
        mode: crate::graphics::PaintMode::Fill,
        winding_order: crate::graphics::WindingOrder::NonZero,
    }
}

// Converts layout coordinates (CSS px, origin top left) to PDF points (origin bottom left).
#[derive(Clone, Copy)]
struct CoordTransform {
    /// Full page height in PDF pt (for Y-axis flip)
    page_height: f32,
    /// Left margin in PDF pt
    margin_left: f32,
    /// Top margin in PDF pt
    margin_top: f32,
}

impl CoordTransform {
    fn new(page_height: f32, margin_left: f32, margin_top: f32) -> Self {
        Self { page_height, margin_left, margin_top }
    }
    
    /// Transform X coordinate from layout space (CSS px) to PDF space (pt)
    #[inline]
    fn x(&self, layout_x: f32) -> f32 {
        layout_x * CSS_PX_TO_PT + self.margin_left
    }
    
    /// Transform Y coordinate from layout space (CSS px) to PDF space (pt)
    /// Layout Y is from top (increases downward), PDF Y is from bottom (increases upward)
    #[inline]
    fn y(&self, layout_y: f32) -> f32 {
        self.page_height - layout_y * CSS_PX_TO_PT - self.margin_top
    }
    
    /// Transform a rectangle's Y position (accounts for rectangle height in CSS px)
    #[inline]
    fn rect_y(&self, layout_y: f32, height: f32) -> f32 {
        self.page_height - (layout_y + height) * CSS_PX_TO_PT - self.margin_top
    }
    
    /// Convert a dimension (width, height, font size) from CSS px to PDF pt
    #[inline]
    fn dim(&self, px_value: f32) -> f32 {
        px_value * CSS_PX_TO_PT
    }
}

/// Maps SVG geometry from the user space of its `<svg>` to PDF points on the page.
///
/// A shape's coordinates are in the `viewBox` of the nearest `<svg>`, and the
/// display list says which box (`bounds`, CSS px) that view box was laid out
/// into. The mapping is the one azul's own renderers use
/// (`svg_user_space_mapping`): the view box is scaled onto the box, each axis by
/// itself. Without a view box the geometry is in CSS px already, relative to
/// the box.
#[derive(Clone, Copy)]
struct SvgToPage {
    /// CSS px per user unit, per axis.
    scale: (f32, f32),
    /// The view box's origin in user units.
    min: (f32, f32),
    /// The box's origin in CSS px.
    origin: (f32, f32),
    transform: CoordTransform,
}

impl SvgToPage {
    fn new(
        bounds: &LogicalRect,
        view_box: Option<(f32, f32, f32, f32)>,
        transform: CoordTransform,
    ) -> Self {
        let (scale, min) = match view_box {
            Some((min_x, min_y, w, h)) if w > 0.0 && h > 0.0 => (
                (bounds.size.width / w, bounds.size.height / h),
                (min_x, min_y),
            ),
            _ => ((1.0, 1.0), (0.0, 0.0)),
        };
        Self {
            scale,
            min,
            origin: (bounds.origin.x, bounds.origin.y),
            transform,
        }
    }

    /// A point of the path, on the page.
    fn point(&self, p: &azul_css::props::basic::SvgPoint) -> crate::graphics::Point {
        let x = self.origin.0 + (p.x - self.min.0) * self.scale.0;
        let y = self.origin.1 + (p.y - self.min.1) * self.scale.1;
        crate::graphics::Point::new(
            pt_to_mm(self.transform.x(x)),
            pt_to_mm(self.transform.y(y)),
        )
    }

    /// CSS px per user unit for a length that has no direction, such as a stroke
    /// width. Where the axes scale differently no single width is exact; the
    /// mean is what azul's CPU renderer strokes with.
    fn mean_scale(&self) -> f32 {
        (self.scale.0 + self.scale.1) / 2.0
    }
}

/// The sub-paths of an SVG path as printpdf lines, one per run of connected
/// segments. A run that ends where it began is closed, so that its last corner
/// gets a join and not two caps.
fn svg_path_to_lines(
    path: &azul_core::svg::SvgMultiPolygon,
    to_page: &SvgToPage,
) -> Vec<crate::graphics::Line> {
    use azul_core::svg::SvgPathElement;
    use azul_css::props::basic::SvgPoint;

    const EPSILON: f32 = 1e-4;
    let same = |a: &SvgPoint, b: &SvgPoint| (a.x - b.x).abs() < EPSILON && (a.y - b.y).abs() < EPSILON;
    let corner = |p: &SvgPoint| crate::graphics::LinePoint { p: to_page.point(p), bezier: false };
    let control = |p: &SvgPoint| crate::graphics::LinePoint { p: to_page.point(p), bezier: true };

    // (points, where the run began, where it is now, whether its last segment was straight)
    struct Run {
        points: Vec<crate::graphics::LinePoint>,
        start: SvgPoint,
        end: SvgPoint,
        last_is_line: bool,
    }
    let finish = |run: Run, lines: &mut Vec<crate::graphics::Line>| {
        let Run { mut points, start, end, last_is_line } = run;
        if points.len() < 2 {
            return;
        }
        let is_closed = points.len() > 2 && same(&start, &end);
        if is_closed && last_is_line {
            // The closing operator draws this last straight segment itself.
            points.pop();
        }
        lines.push(crate::graphics::Line { points, is_closed });
    };

    let mut lines = Vec::new();
    for ring in path.rings.as_ref() {
        let mut run: Option<Run> = None;
        for element in ring.items.as_ref() {
            let (start, end) = match element {
                SvgPathElement::Line(l) => (l.start, l.end),
                SvgPathElement::QuadraticCurve(q) => (q.start, q.end),
                SvgPathElement::CubicCurve(c) => (c.start, c.end),
            };
            if run.as_ref().is_some_and(|r| !same(&r.end, &start)) {
                if let Some(done) = run.take() {
                    finish(done, &mut lines);
                }
            }
            let current = run.get_or_insert_with(|| Run {
                points: vec![corner(&start)],
                start,
                end: start,
                last_is_line: false,
            });
            match element {
                SvgPathElement::Line(_) => {
                    current.points.push(corner(&end));
                    current.last_is_line = true;
                }
                SvgPathElement::QuadraticCurve(q) => {
                    // PDF has cubic curves only: the same curve, one degree higher.
                    let lift = |from: &SvgPoint| SvgPoint {
                        x: from.x + (q.ctrl.x - from.x) * 2.0 / 3.0,
                        y: from.y + (q.ctrl.y - from.y) * 2.0 / 3.0,
                    };
                    current.points.push(control(&lift(&q.start)));
                    current.points.push(control(&lift(&q.end)));
                    current.points.push(corner(&end));
                    current.last_is_line = false;
                }
                SvgPathElement::CubicCurve(c) => {
                    current.points.push(control(&c.ctrl_1));
                    current.points.push(control(&c.ctrl_2));
                    current.points.push(corner(&end));
                    current.last_is_line = false;
                }
            }
            current.end = end;
        }
        if let Some(done) = run.take() {
            finish(done, &mut lines);
        }
    }
    lines
}

fn convert_display_list_item_with_margins<'a, T: ParsedFontTrait + 'static>(
    ops: &mut Vec<Op>,
    item: &'a DisplayListItem,
    page_height: f32,
    margin_left: f32,
    margin_top: f32,
    current_text_layout: &mut Option<(&'a UnifiedLayout, LogicalRect)>,
    // Text is emitted by the deferred flush in `display_list_to_printpdf_ops_with_margins`.
    _font_manager: &FontManager<T>,
    images: &ResolvedImages,
    bridge_res: &mut BridgeResources,
) {
    let transform = CoordTransform::new(page_height, margin_left, margin_top);
    
    match item {
        DisplayListItem::Rect {
            bounds,
            color,
            border_radius,
        } => {
            // Skip rectangles with zero size (layout artifacts from empty inline elements)
            if bounds.size().width == 0.0 || bounds.size().height == 0.0 {
                return;
            }
            
            // Convert rectangle to PDF polygon
            ops.push(Op::SaveGraphicsState);
            apply_fill_alpha(ops, bridge_res, color);

            // Convert DisplayList BorderRadius (CSS px) to border-module BorderRadii,
            // scaled to PDF pt to match the pt-space `b` computed below.
            let radii = crate::html::border::BorderRadii {
                top_left: (border_radius.top_left * CSS_PX_TO_PT, border_radius.top_left * CSS_PX_TO_PT),
                top_right: (border_radius.top_right * CSS_PX_TO_PT, border_radius.top_right * CSS_PX_TO_PT),
                bottom_right: (border_radius.bottom_right * CSS_PX_TO_PT, border_radius.bottom_right * CSS_PX_TO_PT),
                bottom_left: (border_radius.bottom_left * CSS_PX_TO_PT, border_radius.bottom_left * CSS_PX_TO_PT),
            };
            
            // Check if we have border radius
            let has_radius = radii.top_left.0 > 0.0 || radii.top_left.1 > 0.0
                || radii.top_right.0 > 0.0 || radii.top_right.1 > 0.0
                || radii.bottom_right.0 > 0.0 || radii.bottom_right.1 > 0.0
                || radii.bottom_left.0 > 0.0 || radii.bottom_left.1 > 0.0;
            
            if has_radius {
                // Use rounded rectangle path for filling (with margin-adjusted coordinates)
                // Convert layout coordinates from CSS px to PDF pt before passing
                let b = bounds_px_to_pt(bounds.inner());
                let points = crate::html::border::create_rounded_rect_path_with_margins(
                    b.origin.x,
                    b.origin.y,
                    b.size.width,
                    b.size.height,
                    &radii,
                    page_height,
                    margin_left,
                    margin_top,
                );
                
                let polygon = crate::graphics::Polygon {
                    rings: vec![crate::graphics::PolygonRing { points }],
                    mode: crate::graphics::PaintMode::Fill,
                    winding_order: crate::graphics::WindingOrder::NonZero,
                };
                
                ops.push(Op::SetFillColor {
                    col: convert_color(color),
                });
                ops.push(Op::DrawPolygon { polygon });
            } else {
                // Simple rectangle without border radius
                let x = transform.x(bounds.origin().x);
                let y = transform.rect_y(bounds.origin().y, bounds.size().height);
                let w = transform.dim(bounds.size().width);
                let h = transform.dim(bounds.size().height);

                ops.push(Op::SetFillColor {
                    col: convert_color(color),
                });
                ops.push(Op::DrawPolygon { polygon: make_rect_polygon_pt(x, y, w, h) });
            }
            
            ops.push(Op::RestoreGraphicsState);
        }

        DisplayListItem::TextLayout {
            layout,
            bounds,
            font_hash: _,
            font_size_px: _,
            color: _,
        } => {
            // Not drawn here: `display_list_to_printpdf_ops_with_margins` defers the text so the
            // backgrounds between the `TextLayout` and its `Text` runs paint under it.
            if let Some(unified_layout) = layout.downcast_ref::<azul_layout::text3::cache::UnifiedLayout>() {
                *current_text_layout = Some((unified_layout, *bounds.inner()));
            }
        }

        DisplayListItem::Text { glyphs, font_hash, font_size_px, color, .. } => {
            // `Text` items with font hash 0 are headers and footers: they use Unicode codepoints
            // as glyph indices and are drawn with a standard PDF font. All other `Text` items
            // repeat text that the `TextLayout` item draws, and are skipped.
            if glyphs.is_empty() {
                return;
            }
            
            let use_builtin_font = font_hash.font_hash == 0;
            
            if !use_builtin_font {
                return;
            }
            
            ops.push(Op::SaveGraphicsState);
            
            // Set text color
            ops.push(Op::SetFillColor {
                col: convert_color(color),
            });
            match color.a {
                255 => {}
                0 => ops.push(Op::SetTextRenderingMode { mode: crate::TextRenderingMode::Invisible }),
                _ => apply_fill_alpha(ops, bridge_res, color),
            }
            
            if use_builtin_font {
                // Use Helvetica for system-generated text (headers/footers)
                ops.push(Op::SetFont {
                    font: crate::ops::PdfFontHandle::Builtin(crate::BuiltinFont::Helvetica),
                    size: Pt(transform.dim(*font_size_px)),
                });
            } else {
                // Use external font by hash
                let font_id = FontId(format!("F{}", font_hash.font_hash));
                ops.push(Op::SetFont {
                    font: crate::ops::PdfFontHandle::External(font_id),
                    size: Pt(transform.dim(*font_size_px)),
                });
            }
            
            // Start text section
            ops.push(Op::StartTextSection);
            
            // Render glyphs - collect all characters into a single text string for better rendering
            // Group glyphs by approximate y-position to handle potential line breaks
            let mut text_string = String::new();
            let mut first_glyph_pos: Option<(f32, f32)> = None;
            
            for glyph in glyphs {
                if first_glyph_pos.is_none() {
                    first_glyph_pos = Some((glyph.point.x, glyph.point.y));
                }
                
                // For builtin fonts, glyph.index is the Unicode codepoint
                if use_builtin_font {
                    if let Some(ch) = char::from_u32(glyph.index) {
                        text_string.push(ch);
                    }
                }
            }
            
            if let Some((x, y)) = first_glyph_pos {
                // Convert coordinates
                let pdf_x = transform.x(x);
                let pdf_y = transform.y(y);
                
                // Set position for this text
                ops.push(Op::SetTextMatrix {
                    matrix: crate::matrix::TextMatrix::Raw([
                        1.0, 0.0,   // No scaling/rotation
                        0.0, 1.0,
                        pdf_x, pdf_y,
                    ]),
                });
                
                if use_builtin_font && !text_string.is_empty() {
                    ops.push(Op::ShowText {
                        items: vec![crate::text::TextItem::Text(text_string)],
                    });
                } else if !use_builtin_font {
                    // For external fonts, use glyph IDs
                    let glyph_ids: Vec<crate::text::Codepoint> = glyphs.iter().map(|g| {
                        crate::text::Codepoint::new(g.index as u16, 0.0)
                    }).collect();
                    
                    ops.push(Op::ShowText {
                        items: vec![crate::text::TextItem::GlyphIds(glyph_ids)],
                    });
                }
            }
            
            // End text section
            ops.push(Op::EndTextSection);
            
            ops.push(Op::RestoreGraphicsState);
        }

        DisplayListItem::Border {
            bounds,
            widths,
            colors,
            styles,
            border_radius,
        } => {
            // Use comprehensive border rendering with margin support
            // Convert bounds from CSS px to PDF pt
            let bounds_pt = bounds_px_to_pt(bounds.inner());
            let config = BorderConfig {
                bounds: bounds_pt,
                widths: extract_border_widths(widths),
                colors: extract_border_colors(colors),
                styles: extract_border_styles(styles),
                radii: extract_border_radii(border_radius),
                page_height,
                margin_left,
                margin_top,
            };
            
            render_border(ops, &config);
        }

        DisplayListItem::Image { bounds, .. } => {
            // The display-list `ImageRef` for an HTML `<img>` is a `NullImage`
            // whose `tag` carries the original `src` string (set by azul's
            // `xml_node_to_dom_fast`). We use that key to look up the decoded
            // bytes — already registered as a PDF Image XObject on the document —
            // and emit a `UseXobject` op positioned at `bounds`.
            let Some(src_key) = html_image_src(item) else { return; };
            let Some((xobject_id, raw_image)) = images.get(&src_key) else { return; };

            // Target placement rectangle in PDF pt.
            let b = bounds_px_to_pt(bounds.inner());
            let target_w_pt = b.size.width;
            let target_h_pt = b.size.height;
            if target_w_pt <= 0.0 || target_h_pt <= 0.0 {
                return;
            }

            // The serializer maps the image's unit square to its natural size in
            // pt at `IMAGE_DPI` first (see `XObjectTransform::get_ctms`), so we
            // scale by target/natural to make the image fill `bounds` exactly.
            const IMAGE_DPI: f32 = 300.0;
            let natural_w_pt = crate::Px(raw_image.width).into_pt(IMAGE_DPI).0;
            let natural_h_pt = crate::Px(raw_image.height).into_pt(IMAGE_DPI).0;
            if natural_w_pt <= 0.0 || natural_h_pt <= 0.0 {
                return;
            }
            let scale_x = target_w_pt / natural_w_pt;
            let scale_y = target_h_pt / natural_h_pt;

            // Bottom-left corner of the image box in PDF coordinates (origin
            // bottom-left). `transform.rect_y` already accounts for the rect
            // height and top margin; `transform.x` adds the left margin.
            let pdf_x = transform.x(bounds.origin().x);
            let pdf_y = transform.rect_y(bounds.origin().y, bounds.size().height);

            ops.push(Op::UseXobject {
                id: xobject_id.clone(),
                transform: XObjectTransform {
                    translate_x: Some(Pt(pdf_x)),
                    translate_y: Some(Pt(pdf_y)),
                    rotate: None,
                    scale_x: Some(scale_x),
                    scale_y: Some(scale_y),
                    dpi: Some(IMAGE_DPI),
                    // The scale math above is expressed against the automatic
                    // unit-square -> natural-size mapping, so keep it enabled.
                    no_auto_scale: false,
                },
            });
        }

        DisplayListItem::Underline { bounds, color, thickness: _ }
        | DisplayListItem::Strikethrough { bounds, color, thickness: _ }
        | DisplayListItem::Overline { bounds, color, thickness: _ } => {
            // Text decorations are rendered as simple filled rectangles.
            // The decoration thickness is already encoded in bounds.size.height.
            if bounds.size().width > 0.0 && bounds.size().height > 0.0 {
                let x = transform.x(bounds.origin().x);
                let y = transform.rect_y(bounds.origin().y, bounds.size().height);
                let w = transform.dim(bounds.size().width);
                let h = transform.dim(bounds.size().height);

                ops.push(Op::SaveGraphicsState);
                apply_fill_alpha(ops, bridge_res, color);
                ops.push(Op::SetFillColor { col: convert_color(color) });
                ops.push(Op::DrawPolygon { polygon: make_rect_polygon_pt(x, y, w, h) });
                ops.push(Op::RestoreGraphicsState);
            }
        }

        DisplayListItem::LinearGradient { bounds, gradient, border_radius } => {
            let inner = bounds.inner();
            if inner.size.width <= 0.0 || inner.size.height <= 0.0 {
                return;
            }
            // Gradient-line endpoints in local px (azul's CSS direction math),
            // mapped to absolute PDF pt (the transform applies px->pt + Y-flip).
            let rect = LayoutRect::new(
                LayoutPoint::new(0, 0),
                LayoutSize::new(inner.size.width as isize, inner.size.height as isize),
            );
            let (p0, p1) = gradient.direction.to_points(&rect);
            let coords = [
                transform.x(inner.origin.x + p0.x as f32),
                transform.y(inner.origin.y + p0.y as f32),
                transform.x(inner.origin.x + p1.x as f32),
                transform.y(inner.origin.y + p1.y as f32),
            ];
            let stops = normalize_gradient_stops(&gradient.stops);
            if stops.is_empty() {
                return;
            }
            let id = bridge_res.add_shading(Shading {
                geometry: ShadingGeometry::Axial { coords },
                stops,
                extend: (true, true),
            });
            // Clip to the (optionally rounded) box, then paint the shading.
            ops.push(Op::SaveGraphicsState);
            ops.push(Op::DrawPolygon {
                polygon: make_clip_polygon(
                    &transform, inner,
                    [border_radius.top_left, border_radius.top_right,
                     border_radius.bottom_right, border_radius.bottom_left],
                    page_height, margin_left, margin_top,
                ),
            });
            ops.push(Op::PaintShading { id });
            ops.push(Op::RestoreGraphicsState);
        }

        DisplayListItem::RadialGradient { bounds, gradient, border_radius } => {
            let inner = bounds.inner();
            if inner.size.width <= 0.0 || inner.size.height <= 0.0 {
                return;
            }
            // The gradient is centered in the box with the farthest-corner radius (the CSS
            // default). Position and size keywords and ellipse aspect ratios are approximated
            // as a centered circle.
            let cx = inner.size.width / 2.0;
            let cy = inner.size.height / 2.0;
            let r_px = cx.max(inner.size.width - cx).hypot(cy.max(inner.size.height - cy));
            let center_x = transform.x(inner.origin.x + cx);
            let center_y = transform.y(inner.origin.y + cy);
            let r_pt = r_px * CSS_PX_TO_PT;
            let coords = [center_x, center_y, 0.0, center_x, center_y, r_pt];
            let stops = normalize_gradient_stops(&gradient.stops);
            if stops.is_empty() {
                return;
            }
            let id = bridge_res.add_shading(Shading {
                geometry: ShadingGeometry::Radial { coords },
                stops,
                extend: (true, true),
            });
            ops.push(Op::SaveGraphicsState);
            ops.push(Op::DrawPolygon {
                polygon: make_clip_polygon(
                    &transform, inner,
                    [border_radius.top_left, border_radius.top_right,
                     border_radius.bottom_right, border_radius.bottom_left],
                    page_height, margin_left, margin_top,
                ),
            });
            ops.push(Op::PaintShading { id });
            ops.push(Op::RestoreGraphicsState);
        }

        DisplayListItem::PushOpacity { opacity, .. } => {
            // Approximate group opacity by setting fill+stroke alpha for the
            // wrapped content until PopOpacity. (True isolated group opacity
            // would need a transparency-group XObject; per-primitive alpha is
            // correct for non-overlapping content, which is the common case.)
            ops.push(Op::SaveGraphicsState);
            let a = (*opacity).clamp(0.0, 1.0);
            if a < 1.0 {
                let mut gs = ExtendedGraphicsState::default();
                gs.set_current_fill_alpha(a);
                gs.set_current_stroke_alpha(a);
                let id = bridge_res.add_extgstate(gs);
                ops.push(Op::LoadGraphicsState { gs: id });
            }
        }

        DisplayListItem::PopOpacity => {
            ops.push(Op::RestoreGraphicsState);
        }

        DisplayListItem::PushClip { bounds, border_radius } => {
            // Begin a clip scope: save the graphics state, then intersect the
            // current clip path with this (optionally rounded) rectangle. The
            // matching `PopClip` restores the state (un-clips). Nesting is handled
            // by PDF's own q/Q graphics-state stack, so no separate clip stack is
            // needed here. `PaintMode::Clip` makes `polygon_to_stream_ops` emit the
            // path followed by `W`/`W*` + `n` instead of a fill/stroke.
            ops.push(Op::SaveGraphicsState);

            let radii = crate::html::border::BorderRadii {
                top_left: (border_radius.top_left * CSS_PX_TO_PT, border_radius.top_left * CSS_PX_TO_PT),
                top_right: (border_radius.top_right * CSS_PX_TO_PT, border_radius.top_right * CSS_PX_TO_PT),
                bottom_right: (border_radius.bottom_right * CSS_PX_TO_PT, border_radius.bottom_right * CSS_PX_TO_PT),
                bottom_left: (border_radius.bottom_left * CSS_PX_TO_PT, border_radius.bottom_left * CSS_PX_TO_PT),
            };
            let has_radius = radii.top_left.0 > 0.0 || radii.top_right.0 > 0.0
                || radii.bottom_right.0 > 0.0 || radii.bottom_left.0 > 0.0;

            let polygon = if has_radius {
                let b = bounds_px_to_pt(bounds.inner());
                let points = crate::html::border::create_rounded_rect_path_with_margins(
                    b.origin.x, b.origin.y, b.size.width, b.size.height,
                    &radii, page_height, margin_left, margin_top,
                );
                crate::graphics::Polygon {
                    rings: vec![crate::graphics::PolygonRing { points }],
                    mode: crate::graphics::PaintMode::Clip,
                    winding_order: crate::graphics::WindingOrder::NonZero,
                }
            } else {
                let x = transform.x(bounds.origin().x);
                let y = transform.rect_y(bounds.origin().y, bounds.size().height);
                let w = transform.dim(bounds.size().width);
                let h = transform.dim(bounds.size().height);
                let mut p = make_rect_polygon_pt(x, y, w, h);
                p.mode = crate::graphics::PaintMode::Clip;
                p
            };
            ops.push(Op::DrawPolygon { polygon });
        }

        DisplayListItem::PopClip => {
            ops.push(Op::RestoreGraphicsState);
        }

        DisplayListItem::StrokedPath {
            bounds,
            path,
            view_box,
            color,
            width,
            outline,
            // The stroke pre-rasterised for renderers that cannot stroke a path.
            // A PDF can, so the geometry is written and stays sharp at any zoom.
            mask: _,
        } => {
            if color.a == 0 || !width.is_finite() || *width <= 0.0 {
                return;
            }
            let to_page = SvgToPage::new(bounds.inner(), *view_box, transform);
            let lines = svg_path_to_lines(path, &to_page);
            if lines.is_empty() {
                return;
            }
            // One user unit of the path, in PDF pt: stroke width and dashes are
            // given in user units and scale with the geometry.
            let unit = to_page.mean_scale() * CSS_PX_TO_PT;

            ops.push(Op::SaveGraphicsState);
            apply_fill_alpha(ops, bridge_res, color);
            ops.push(Op::SetOutlineColor { col: convert_color(color) });
            ops.push(Op::SetOutlineThickness { pt: Pt(*width * unit) });
            ops.push(Op::SetLineCapStyle {
                cap: match outline.cap {
                    azul_core::svg::SvgLineCap::Butt => crate::graphics::LineCapStyle::Butt,
                    azul_core::svg::SvgLineCap::Round => crate::graphics::LineCapStyle::Round,
                    azul_core::svg::SvgLineCap::Square => {
                        crate::graphics::LineCapStyle::ProjectingSquare
                    }
                },
            });
            match outline.join {
                // PDF has no `miter-clip`: past the limit it bevels, as `miter` does.
                azul_core::svg::SvgLineJoin::Miter | azul_core::svg::SvgLineJoin::MiterClip => {
                    ops.push(Op::SetLineJoinStyle { join: crate::graphics::LineJoinStyle::Miter });
                    if outline.miter_limit.is_finite() && outline.miter_limit >= 1.0 {
                        // A ratio, not a length, although the op takes a `Pt`.
                        ops.push(Op::SetMiterLimit { limit: Pt(outline.miter_limit) });
                    }
                }
                azul_core::svg::SvgLineJoin::Round => {
                    ops.push(Op::SetLineJoinStyle { join: crate::graphics::LineJoinStyle::Round });
                }
                azul_core::svg::SvgLineJoin::Bevel => {
                    ops.push(Op::SetLineJoinStyle { join: crate::graphics::LineJoinStyle::Bevel });
                }
            }
            let dash_total: f32 = outline.dashes.iter().map(|(dash, gap)| dash + gap).sum();
            if dash_total.is_finite() && dash_total > 0.0 {
                let mut pattern = smallvec::SmallVec::new();
                for (dash, gap) in &outline.dashes {
                    pattern.push(dash * unit);
                    pattern.push(gap * unit);
                }
                ops.push(Op::SetLineDashPattern {
                    dash: crate::graphics::LineDashPattern {
                        offset: outline.dash_offset * unit,
                        pattern,
                    },
                });
            }
            for line in lines {
                ops.push(Op::DrawLine { line });
            }
            ops.push(Op::RestoreGraphicsState);
        }

        DisplayListItem::PushImageMaskClip { bounds, geometry, .. } => {
            // What follows, up to the matching pop, is painted through a mask.
            // The fill of an SVG shape arrives like this: the fill colour (or
            // gradient) as a rectangle, under a mask made from the path. Where
            // the item carries that path, the PDF clips to it, which is exact at
            // any zoom. The scope is opened in every case so that the pop below
            // always has a `q` to close.
            ops.push(Op::SaveGraphicsState);
            if let Some(geometry) = geometry {
                let to_page = SvgToPage::new(bounds.inner(), geometry.view_box, transform);
                let rings: Vec<crate::graphics::PolygonRing> =
                    svg_path_to_lines(&geometry.path, &to_page)
                        .into_iter()
                        .map(|line| crate::graphics::PolygonRing { points: line.points })
                        .collect();
                let mut polygon = if rings.is_empty() {
                    // A path that encloses nothing lets nothing through. A clip
                    // operator needs a path, so this one is a rectangle of no size.
                    make_rect_polygon_pt(0.0, 0.0, 0.0, 0.0)
                } else {
                    crate::graphics::Polygon {
                        rings,
                        mode: crate::graphics::PaintMode::Clip,
                        winding_order: if geometry.even_odd {
                            crate::graphics::WindingOrder::EvenOdd
                        } else {
                            crate::graphics::WindingOrder::NonZero
                        },
                    }
                };
                polygon.mode = crate::graphics::PaintMode::Clip;
                ops.push(Op::DrawPolygon { polygon });
            }
            // A mask that is only an image (no geometry) would need a soft mask;
            // its content is still painted unclipped.
        }

        DisplayListItem::PopImageMaskClip => {
            ops.push(Op::RestoreGraphicsState);
        }

        _ => {
            // Remaining items not yet wired for PDF:
            //  - PushScrollFrame/PushStackingContext/Push(Backdrop)Filter/
            //    PushTextShadow and their Pops, VirtualView*, ScrollBar*,
            //    Selection/Cursor (screen-only).
            // These intentionally emit nothing (and crucially no unbalanced q/Q).
        }
    }
}

/// Renders a shaped text layout to ops, without going through a display list.
///
/// The layout is placed at the page origin: `bounds_width` and `bounds_height` give the size of
/// its box, `page_height` is the page height used to flip y, and the coordinates are taken as
/// they are, in the unit of the layout, without px to pt conversion or margins.
pub fn render_unified_layout_public<T: ParsedFontTrait + 'static>(
    layout: &UnifiedLayout,
    bounds_width: f32,
    bounds_height: f32, 
    color: ColorU,
    page_height: f32,
    _font_manager: &FontManager<T>,
) -> Vec<Op> {
    let mut ops = Vec::new();
    let bounds = LogicalRect { 
        origin: LogicalPosition::new(0.0, 0.0),
        size: LogicalSize::new(bounds_width, bounds_height),
    };
    render_unified_layout_impl(&mut ops, layout, &bounds, color, page_height, _font_manager);
    ops
}

fn render_unified_layout_impl<T: ParsedFontTrait + 'static>(
    ops: &mut Vec<Op>,
    layout: &UnifiedLayout,
    bounds: &LogicalRect,
    _color: ColorU,  // Unused: per-glyph color from layout takes precedence
    page_height: f32,
    font_manager: &FontManager<T>,
) {
    use azul_layout::text3::glyphs::get_glyph_runs_pdf;

    let loaded_fonts = font_manager.get_loaded_fonts();
    
    // Glyph runs are grouped by font, color, style and line.
    let glyph_runs = get_glyph_runs_pdf(layout, &loaded_fonts);

    if glyph_runs.is_empty() {
        return;
    }

    // Skips color ops that would not change anything.
    let mut current_color: Option<ColorU> = None;

    // Each run gets its own text section.
    for run in glyph_runs.iter() {
        if run.glyphs.is_empty() {
            continue;
        }

        let scoped = begin_translucent_text(ops, None, &run.color);
        if scoped {
            current_color = None;
        } else if current_color != Some(run.color) {
            ops.push(Op::SetFillColor {
                col: convert_color(&run.color),
            });
            current_color = Some(run.color);
        }

        // Set font if it changed OR if we're starting a new text section (BEFORE text section)
        // Note: Font must be set inside each text section (BT...ET), so we set it every time
        let font_id = FontId(format!("F{}", run.font_hash));
        ops.push(Op::SetFont {
            font: crate::ops::PdfFontHandle::External(font_id.clone()),
            size: Pt(run.font_size_px * CSS_PX_TO_PT),
        });

        ops.push(Op::StartTextSection);

        // Layout coordinates are in CSS px; convert to PDF pt.
        // This gives us complete control over positioning for RTL, vertical text, etc.
        // and avoids relying on PDF's font metrics for cursor advancement
        for glyph in &run.glyphs {
            // Calculate absolute position for this glyph (in CSS px)
            let glyph_x_px = bounds.origin.x + glyph.position.x;
            let glyph_y_px = bounds.origin.y + glyph.position.y;
            
            // Convert from CSS px to PDF pt, then flip Y axis
            // HTML: origin at top-left, Y increases downward
            // PDF: origin at bottom-left, Y increases upward
            let pdf_x = glyph_x_px * CSS_PX_TO_PT;
            let pdf_y = page_height - glyph_y_px * CSS_PX_TO_PT;

            // The matrix values are in PDF user space units (points)
            ops.push(Op::SetTextMatrix {
                matrix: crate::matrix::TextMatrix::Raw([
                    1.0,    // a: Horizontal scaling (1.0 = no scaling)
                    0.0,    // b: Horizontal skewing
                    0.0,    // c: Vertical skewing
                    1.0,    // d: Vertical scaling (1.0 = no scaling)
                    pdf_x,  // e: Horizontal translation (in points)
                    pdf_y,  // f: Vertical translation (in points)
                ]),
            });

            ops.push(Op::ShowText {
                items: vec![crate::text::TextItem::GlyphIds(vec![
                    crate::text::Codepoint {
                        gid: glyph.glyph_id,
                        offset: 0.0,
                        cid: Some(glyph.unicode_codepoint.clone()),
                    }
                ])],
            });
        }

        ops.push(Op::EndTextSection);
        if scoped {
            ops.push(Op::RestoreGraphicsState);
        }
    }
}

/// Build a `BDC /Span << /ActualText (UTF-16BE) >>` marked-content op. Wrapping a
/// run of individually-positioned glyphs in this span makes PDF text extraction
/// (copy/paste, selection, accessibility) use `text` verbatim instead of trying to
/// reconstruct it from per-glyph positions — which otherwise inserts a spurious
/// space between every glyph. Close the span with `Op::EndMarkedContent`.
fn actualtext_span_begin(text: &str) -> Op {
    crate::text::actual_text_span(text)
}

/// Render UnifiedLayout to PDF operations with margin support
fn render_unified_layout_with_margins<T: ParsedFontTrait + 'static>(
    ops: &mut Vec<Op>,
    layout: &UnifiedLayout,
    bounds: &LogicalRect,
    _color: ColorU,  // Unused: per-glyph color from layout takes precedence
    transform: &CoordTransform,
    font_manager: &FontManager<T>,
    bridge_res: &mut BridgeResources,
) {
    use azul_layout::text3::glyphs::get_glyph_runs_pdf;

    let loaded_fonts = font_manager.get_loaded_fonts();
    
    // Get PDF-optimized glyph runs (grouped by font/color/style/line)
    let glyph_runs = get_glyph_runs_pdf(layout, &loaded_fonts);

    if glyph_runs.is_empty() {
        return;
    }

    // Two passes keep the z-order right: first the inline backgrounds
    // (`<span style="background-color: yellow">`) of all runs, then the text on top, so a
    // background never covers the text of an earlier run.
    for run in glyph_runs.iter() {
        if run.glyphs.is_empty() {
            continue;
        }
        
        if let Some(bg_color) = run.background_color {
            if bg_color.a > 0 {
                if let (Some(first_glyph), Some(last_glyph)) = 
                    (run.glyphs.first(), run.glyphs.last()) 
                {
                    let font_size = run.font_size_px;
                    // Ascent and descent are estimated from the font size.
                    let ascent = font_size * 0.8;
                    let descent = font_size * 0.2;
                    
                    let bg_start_x = bounds.origin.x + first_glyph.position.x;
                    let bg_end_x = bounds.origin.x + last_glyph.position.x + last_glyph.advance;
                    let bg_width = bg_end_x - bg_start_x;
                    
                    let baseline_y = bounds.origin.y + first_glyph.position.y;
                    let bg_top_y = baseline_y - ascent;
                    let bg_height = ascent + descent;
                    
                    let pdf_x = transform.x(bg_start_x);
                    let pdf_y = transform.rect_y(bg_top_y, bg_height);
                    
                    let pdf_w = transform.dim(bg_width);
                    let pdf_h = transform.dim(bg_height);
                    
                    ops.push(Op::SaveGraphicsState);
                    ops.push(Op::SetFillColor {
                        col: convert_color(&bg_color),
                    });
                    apply_fill_alpha(ops, bridge_res, &bg_color);
                    ops.push(Op::DrawPolygon { polygon: make_rect_polygon_pt(pdf_x, pdf_y, pdf_w, pdf_h) });
                    ops.push(Op::RestoreGraphicsState);
                }
            }
        }
    }

    let mut current_color: Option<ColorU> = None;

    for run in glyph_runs.iter() {
        if run.glyphs.is_empty() {
            continue;
        }

        let scoped = begin_translucent_text(ops, Some(&mut *bridge_res), &run.color);
        if scoped {
            current_color = None;
        } else if current_color != Some(run.color) {
            ops.push(Op::SetFillColor {
                col: convert_color(&run.color),
            });
            current_color = Some(run.color);
        }

        let font_id = FontId(format!("F{}", run.font_hash));
        ops.push(Op::SetFont {
            font: crate::ops::PdfFontHandle::External(font_id.clone()),
            size: Pt(transform.dim(run.font_size_px)),
        });

        ops.push(Op::StartTextSection);

        // The /ActualText span gives copy and paste and accessibility the real string; with a
        // text matrix per glyph, extractors would see every glyph as a separate word. Saving
        // with `optimize` merges the runs again (see text_merge.rs).
        let run_text: String = run.glyphs.iter().map(|g| g.unicode_codepoint.as_str()).collect();
        let wrapped = !run_text.is_empty();
        if wrapped {
            ops.push(actualtext_span_begin(&run_text));
        }

        for glyph in &run.glyphs {
            // The glyphs of a page are already clipped by pagination. `glyph.position` is relative
            // to the origin of the text block.
            let glyph_x_layout = bounds.origin.x + glyph.position.x;
            let glyph_y_layout = bounds.origin.y + glyph.position.y;

            let pdf_x = transform.x(glyph_x_layout);
            let pdf_y = transform.y(glyph_y_layout);

            ops.push(Op::SetTextMatrix {
                matrix: crate::matrix::TextMatrix::Raw([
                    1.0,    // a: Horizontal scaling
                    0.0,    // b: Horizontal skewing
                    0.0,    // c: Vertical skewing
                    1.0,    // d: Vertical scaling
                    pdf_x,  // e: Horizontal translation (in points)
                    pdf_y,  // f: Vertical translation (in points)
                ]),
            });

            ops.push(Op::ShowText {
                items: vec![crate::text::TextItem::GlyphIds(vec![
                    crate::text::Codepoint {
                        gid: glyph.glyph_id,
                        offset: 0.0,
                        cid: Some(glyph.unicode_codepoint.clone()),
                    }
                ])],
            });
        }

        if wrapped {
            ops.push(Op::EndMarkedContent);
        }

        ops.push(Op::EndTextSection);
        if scoped {
            ops.push(Op::RestoreGraphicsState);
        }
    }

}

/// Moves the coordinates of the ops by `offset_x` to the right and `offset_y` up (PDF y points
/// up), for example to add a margin to ops that were converted without one.
pub fn apply_margin_offset(ops: &mut [Op], offset_x: crate::Mm, offset_y: crate::Mm) {
    if offset_x.0 == 0.0 && offset_y.0 == 0.0 {
        return;
    }
    
    let offset_x_pt = crate::Pt(offset_x.0 * 2.83465);
    let offset_y_pt = crate::Pt(offset_y.0 * 2.83465);
    
    for op in ops.iter_mut() {
        match op {
            Op::DrawPolygon { polygon } => {
                for ring in &mut polygon.rings {
                    for point in &mut ring.points {
                        point.p.x = crate::Pt(point.p.x.0 + offset_x_pt.0);
                        point.p.y = crate::Pt(point.p.y.0 + offset_y_pt.0);
                    }
                }
            }
            Op::DrawLine { line } => {
                for point in &mut line.points {
                    point.p.x = crate::Pt(point.p.x.0 + offset_x_pt.0);
                    point.p.y = crate::Pt(point.p.y.0 + offset_y_pt.0);
                }
            }
            Op::SetTextCursor { pos } => {
                pos.x = crate::Pt(pos.x.0 + offset_x_pt.0);
                pos.y = crate::Pt(pos.y.0 + offset_y_pt.0);
            }
            Op::UseXobject { transform, .. } => {
                // Adjust the translation in the transform
                transform.translate_x = Some(crate::Pt(
                    transform.translate_x.unwrap_or(crate::Pt(0.0)).0 + offset_x_pt.0
                ));
                transform.translate_y = Some(crate::Pt(
                    transform.translate_y.unwrap_or(crate::Pt(0.0)).0 + offset_y_pt.0
                ));
            }
            Op::DrawRectangle { rectangle } => {
                rectangle.x = crate::Pt(rectangle.x.0 + offset_x_pt.0);
                rectangle.y = crate::Pt(rectangle.y.0 + offset_y_pt.0);
            }
            Op::LinkAnnotation { link } => {
                link.rect.x = crate::Pt(link.rect.x.0 + offset_x_pt.0);
                link.rect.y = crate::Pt(link.rect.y.0 + offset_y_pt.0);
            }
            // These operations don't have coordinates that need adjustment
            Op::Marker { .. }
            | Op::SetColorSpaceStroke { .. }
            | Op::SetColorSpaceFill { .. }
            | Op::BeginLayer { .. }
            | Op::EndLayer
            | Op::SaveGraphicsState
            | Op::RestoreGraphicsState
            | Op::LoadGraphicsState { .. }
            | Op::PaintShading { .. }
            | Op::StartTextSection
            | Op::EndTextSection
            | Op::SetFont { .. }
            | Op::ShowText { .. }
            | Op::AddLineBreak
            | Op::SetLineHeight { .. }
            | Op::SetWordSpacing { .. }
            | Op::SetFillColor { .. }
            | Op::SetOutlineColor { .. }
            | Op::SetOutlineThickness { .. }
            | Op::SetLineDashPattern { .. }
            | Op::SetLineJoinStyle { .. }
            | Op::SetLineCapStyle { .. }
            | Op::SetMiterLimit { .. }
            | Op::SetTextRenderingMode { .. }
            | Op::SetCharacterSpacing { .. }
            | Op::SetLineOffset { .. }
            | Op::SetTransformationMatrix { .. }
            | Op::SetTextMatrix { .. }
            | Op::MoveTextCursorAndSetLeading { .. }
            | Op::SetRenderingIntent { .. }
            | Op::SetHorizontalScaling { .. }
            | Op::BeginInlineImage
            | Op::BeginInlineImageData
            | Op::EndInlineImage
            | Op::BeginMarkedContent { .. }
            | Op::BeginMarkedContentWithProperties { .. }
            | Op::BeginOptionalContent { .. }
            | Op::DefineMarkedContentPoint { .. }
            | Op::EndMarkedContent
            | Op::EndMarkedContentWithProperties
            | Op::EndOptionalContent
            | Op::BeginCompatibilitySection
            | Op::EndCompatibilitySection
            | Op::MoveToNextLineShowText { .. }
            | Op::SetSpacingMoveAndShowText { .. }
            | Op::Unknown { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
    use azul_core::resources::{ImageRef, RawImageFormat};
    use azul_layout::solver3::display_list::{
        BorderRadius, DisplayList, DisplayListItem, WindowLogicalRect,
    };
    use azul_layout::text3::cache::FontManager;

    fn empty_font_manager() -> FontManager<azul_css::props::basic::FontRef> {
        FontManager::new(rust_fontconfig::FcFontCache::default())
            .expect("build empty FontManager")
    }

    /// The real cat.jpg used by the `html_image` example, decoded and placed in a
    /// display-list Image item, must produce an `Op::UseXobject` referencing the
    /// deterministic id and scaled to the item bounds.
    #[cfg(all(feature = "images", feature = "jpeg"))]
    #[test]
    fn image_item_emits_use_xobject() {
        let cat_jpg: &[u8] = include_bytes!("../../examples/assets/img/cat.jpg");
        let resolved = resolve_html_images(
            &[("cat.jpg".to_string(), cat_jpg.to_vec())]
                .into_iter()
                .collect(),
        );
        let (_id, raw) = resolved.get("cat.jpg").expect("cat.jpg decoded");
        assert!(raw.width > 0 && raw.height > 0, "decoded image has dimensions");

        // ImageRef carrying the src as its NullImage tag (mirrors what azul's
        // xml_node_to_dom_fast produces for <img src="cat.jpg">).
        let image_ref = ImageRef::null_image(
            raw.width,
            raw.height,
            RawImageFormat::RGBA8,
            b"cat.jpg".to_vec(),
        );

        // 200x100 px box at (10, 20) px.
        let bounds = WindowLogicalRect(LogicalRect {
            origin: LogicalPosition { x: 10.0, y: 20.0 },
            size: LogicalSize { width: 200.0, height: 100.0 },
        });

        let mut dl = DisplayList::default();
        dl.items.push(DisplayListItem::Image {
            bounds,
            image: image_ref,
            border_radius: BorderRadius::default(),
        });

        let fm = empty_font_manager();
        let page = LogicalSize { width: 595.0, height: 842.0 }; // A4 pt
        let ops = display_list_to_printpdf_ops_with_margins(&dl, page, 0.0, 0.0, &fm, &resolved, &mut BridgeResources::default())
            .expect("bridge conversion");

        let use_xobject = ops.iter().find_map(|op| match op {
            Op::UseXobject { id, transform } => Some((id.clone(), *transform)),
            _ => None,
        });
        let (id, transform) = use_xobject.expect("an Op::UseXobject must be emitted for the image");

        assert_eq!(id.0, "HtmlImg_cat_jpg", "deterministic xobject id");
        assert_eq!(id, image_xobject_id("cat.jpg"), "id matches helper");

        // The image should be scaled to fill the 200x100 px box (= 150x75 pt).
        const IMAGE_DPI: f32 = 300.0;
        let natural_w_pt = crate::Px(raw.width).into_pt(IMAGE_DPI).0;
        let natural_h_pt = crate::Px(raw.height).into_pt(IMAGE_DPI).0;
        let target_w_pt = 200.0 * (72.0 / 96.0);
        let target_h_pt = 100.0 * (72.0 / 96.0);
        let sx = transform.scale_x.expect("scale_x set");
        let sy = transform.scale_y.expect("scale_y set");
        assert!((natural_w_pt * sx - target_w_pt).abs() < 0.5, "image width fills bounds");
        assert!((natural_h_pt * sy - target_h_pt).abs() < 0.5, "image height fills bounds");

        // Bottom-left corner: x = 10px -> pt + left margin(0); y flipped.
        let tx = transform.translate_x.expect("translate_x set").0;
        let ty = transform.translate_y.expect("translate_y set").0;
        assert!((tx - 10.0 * (72.0 / 96.0)).abs() < 0.5, "x placement");
        // page_height - (y + h)*px_to_pt = 842 - (20+100)*0.75
        let expect_ty = 842.0 - (20.0 + 100.0) * (72.0 / 96.0);
        assert!((ty - expect_ty).abs() < 0.5, "y placement (bottom-left, flipped)");
    }

    /// The pictures a render needs are the `<img src>` tags its display lists
    /// draw, each once however many pages or items draw it; an image without a
    /// tag is not an `<img src>`.
    #[test]
    fn the_srcs_a_render_needs_are_the_img_tags_its_pages_draw_once_each() {
        let image = |tag: &str| DisplayListItem::Image {
            bounds: WindowLogicalRect(LogicalRect {
                origin: LogicalPosition { x: 0.0, y: 0.0 },
                size: LogicalSize { width: 10.0, height: 10.0 },
            }),
            image: ImageRef::null_image(10, 10, RawImageFormat::RGBA8, tag.as_bytes().to_vec()),
            border_radius: BorderRadius::default(),
        };
        let mut first = DisplayList::default();
        first.items.push(image("plate-1.png"));
        first.items.push(image(""));
        let mut second = DisplayList::default();
        second.items.push(image("plate-1.png"));
        second.items.push(image("plate-2.jpg"));

        let srcs: Vec<String> = referenced_image_srcs(&[first, second]).into_iter().collect();

        assert_eq!(srcs, ["plate-1.png", "plate-2.jpg"]);
    }

    /// An image whose src has no matching entry in the resolved map renders
    /// nothing (no UseXobject, no panic).
    #[test]
    fn image_item_without_bytes_is_noop() {
        let image_ref =
            ImageRef::null_image(100, 50, RawImageFormat::RGBA8, b"missing.png".to_vec());
        let bounds = WindowLogicalRect(LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize { width: 100.0, height: 50.0 },
        });
        let mut dl = DisplayList::default();
        dl.items.push(DisplayListItem::Image {
            bounds,
            image: image_ref,
            border_radius: BorderRadius::default(),
        });

        let fm = empty_font_manager();
        let ops = display_list_to_printpdf_ops_with_margins(
            &dl,
            LogicalSize { width: 595.0, height: 842.0 },
            0.0,
            0.0,
            &fm,
            &ResolvedImages::new(),
            &mut BridgeResources::default(),
        )
        .expect("bridge conversion");

        assert!(
            !ops.iter().any(|op| matches!(op, Op::UseXobject { .. })),
            "no UseXobject when bytes are missing"
        );
    }

    /// A translucent Rect (alpha < 255) must emit a LoadGraphicsState op and
    /// record exactly one ExtGState carrying the fill alpha on BridgeResources.
    #[test]
    fn translucent_rect_emits_extgstate_alpha() {
        let bounds = WindowLogicalRect(LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize { width: 50.0, height: 50.0 },
        });
        let mut dl = DisplayList::default();
        dl.items.push(DisplayListItem::Rect {
            bounds,
            color: ColorU { r: 255, g: 0, b: 0, a: 128 }, // 50% red
            border_radius: BorderRadius::default(),
        });

        let fm = empty_font_manager();
        let mut bridge_res = BridgeResources::default();
        let ops = display_list_to_printpdf_ops_with_margins(
            &dl,
            LogicalSize { width: 595.0, height: 842.0 },
            0.0,
            0.0,
            &fm,
            &ResolvedImages::new(),
            &mut bridge_res,
        )
        .expect("bridge conversion");

        assert!(
            ops.iter().any(|op| matches!(op, Op::LoadGraphicsState { .. })),
            "translucent fill must load an ExtGState"
        );
        assert_eq!(bridge_res.extgstates.len(), 1, "one ExtGState recorded");
        let (_, gs) = &bridge_res.extgstates[0];
        assert!((gs.current_fill_alpha() - 128.0 / 255.0).abs() < 1e-3, "fill alpha set");
    }

    /// An opaque Rect (alpha == 255) must NOT create any ExtGState.
    #[test]
    fn opaque_rect_has_no_extgstate() {
        let bounds = WindowLogicalRect(LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize { width: 50.0, height: 50.0 },
        });
        let mut dl = DisplayList::default();
        dl.items.push(DisplayListItem::Rect {
            bounds,
            color: ColorU { r: 0, g: 0, b: 255, a: 255 },
            border_radius: BorderRadius::default(),
        });
        let fm = empty_font_manager();
        let mut bridge_res = BridgeResources::default();
        let _ = display_list_to_printpdf_ops_with_margins(
            &dl, LogicalSize { width: 595.0, height: 842.0 }, 0.0, 0.0, &fm,
            &ResolvedImages::new(), &mut bridge_res,
        ).expect("bridge conversion");
        assert!(bridge_res.extgstates.is_empty(), "opaque fill needs no ExtGState");
    }

    /// End-to-end serialization: a document carrying the decoded image as an
    /// Image XObject must serialize to a PDF containing `/Subtype /Image`.
    #[cfg(all(feature = "images", feature = "jpeg"))]
    #[test]
    fn registered_image_serializes_as_xobject() {
        let cat_jpg: &[u8] = include_bytes!("../../examples/assets/img/cat.jpg");
        let mut warnings = Vec::new();
        let raw = RawImage::decode_from_bytes(cat_jpg, &mut warnings).expect("decode cat.jpg");

        let mut doc = crate::PdfDocument::new("img test");
        let id = image_xobject_id("cat.jpg");
        doc.resources
            .xobjects
            .map
            .insert(id.clone(), crate::XObject::Image(raw.clone()));
        // A page that uses the image so the XObject is referenced.
        doc.pages.push(crate::PdfPage::new(
            crate::Mm(210.0),
            crate::Mm(297.0),
            vec![Op::UseXobject {
                id,
                transform: XObjectTransform::default(),
            }],
        ));

        let bytes = doc.save(&crate::PdfSaveOptions::default(), &mut Vec::new());
        // lopdf renders the dict without spaces (e.g. `/Subtype/Image`); accept
        // both forms to be robust to lopdf formatting changes.
        let contains = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
        let has_image = contains(b"/Subtype/Image") || contains(b"/Subtype /Image");
        assert!(
            has_image,
            "serialized PDF must contain an Image XObject (len={})",
            bytes.len()
        );
        // The cat.jpg is a JPEG, so it round-trips as a DCTDecode-filtered stream.
        // (The serializer may down-scale the pixels to fit a size budget, so we do
        // not assert exact dimensions — only that an image stream is present.)
        let has_filter = contains(b"/Filter/DCTDecode") || contains(b"/Filter /DCTDecode");
        assert!(has_filter, "image XObject should be a DCTDecode (JPEG) stream");
        let has_width = contains(b"/Width") && contains(b"/Height");
        assert!(has_width, "serialized PDF must declare image width/height");
    }

    fn svg_point(x: f32, y: f32) -> azul_css::props::basic::SvgPoint {
        azul_css::props::basic::SvgPoint { x, y }
    }

    fn svg_line(from: (f32, f32), to: (f32, f32)) -> azul_core::svg::SvgPathElement {
        azul_core::svg::SvgPathElement::Line(azul_core::svg::SvgLine {
            start: svg_point(from.0, from.1),
            end: svg_point(to.0, to.1),
        })
    }

    fn one_ring(items: Vec<azul_core::svg::SvgPathElement>) -> azul_core::svg::SvgMultiPolygon {
        azul_core::svg::SvgMultiPolygon {
            rings: vec![azul_core::svg::SvgPath { items: items.into() }].into(),
        }
    }

    fn plain_stroke() -> azul_layout::solver3::display_list::StrokeOutline {
        azul_layout::solver3::display_list::StrokeOutline {
            cap: azul_core::svg::SvgLineCap::Butt,
            join: azul_core::svg::SvgLineJoin::Miter,
            miter_limit: 4.0,
            dashes: Vec::new(),
            dash_offset: 0.0,
        }
    }

    fn ops_for(item: DisplayListItem) -> Vec<Op> {
        let mut dl = DisplayList::default();
        dl.items.push(item);
        display_list_to_printpdf_ops_with_margins(
            &dl,
            LogicalSize { width: 595.0, height: 842.0 },
            0.0,
            0.0,
            &empty_font_manager(),
            &ResolvedImages::new(),
            &mut BridgeResources::default(),
        )
        .expect("bridge conversion")
    }

    /// The stroke of an SVG shape reaches the bridge as a `StrokedPath` carrying its geometry and
    /// is written as a stroked path, curve and all.
    #[test]
    fn stroked_svg_path_is_written_as_a_pdf_stroke() {
        // <svg width="120" height="60" viewBox="0 0 120 60"> laid out at (10, 20):
        // <path d="M10 50 C40 0 80 0 110 50" stroke="#000" stroke-width="3"/>
        let ops = ops_for(DisplayListItem::StrokedPath {
            bounds: WindowLogicalRect(LogicalRect {
                origin: LogicalPosition { x: 10.0, y: 20.0 },
                size: LogicalSize { width: 120.0, height: 60.0 },
            }),
            path: one_ring(vec![azul_core::svg::SvgPathElement::CubicCurve(
                azul_css::props::basic::SvgCubicCurve {
                    start: svg_point(10.0, 50.0),
                    ctrl_1: svg_point(40.0, 0.0),
                    ctrl_2: svg_point(80.0, 0.0),
                    end: svg_point(110.0, 50.0),
                },
            )]),
            view_box: Some((0.0, 0.0, 120.0, 60.0)),
            color: ColorU { r: 0, g: 0, b: 0, a: 255 },
            width: 3.0,
            outline: plain_stroke(),
            mask: None,
        });

        let thickness = ops.iter().find_map(|op| match op {
            Op::SetOutlineThickness { pt } => Some(pt.0),
            _ => None,
        });
        // 3 user units = 3 CSS px = 2.25 pt
        assert!((thickness.expect("a stroke width") - 2.25).abs() < 1e-3);

        let lines: Vec<&crate::graphics::Line> = ops
            .iter()
            .filter_map(|op| match op {
                Op::DrawLine { line } => Some(line),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 1, "one sub-path");
        let line = lines[0];
        assert!(!line.is_closed);
        let flags: Vec<bool> = line.points.iter().map(|p| p.bezier).collect();
        assert_eq!(flags, [false, true, true, false], "start, two control points, end");
        // Start point: x = (10 + 10) px = 15 pt; y = 842 - (20 + 50) px * 0.75 = 789.5 pt
        assert!((line.points[0].p.x.0 - 15.0).abs() < 1e-2);
        assert!((line.points[0].p.y.0 - 789.5).abs() < 1e-2);
        // End point: x = (10 + 110) px = 90 pt, same height
        assert!((line.points[3].p.x.0 - 90.0).abs() < 1e-2);
        assert!((line.points[3].p.y.0 - 789.5).abs() < 1e-2);

        let saves = ops.iter().filter(|op| matches!(op, Op::SaveGraphicsState)).count();
        let restores = ops.iter().filter(|op| matches!(op, Op::RestoreGraphicsState)).count();
        assert_eq!(saves, restores, "the stroke state stays local");
        assert!(ops.iter().any(|op| matches!(op, Op::SetOutlineColor { .. })));
    }

    /// Width and dashes are in the path's user units: an icon drawn in a 24-unit
    /// view box and laid out twice as large gets a stroke twice as wide. A ring
    /// that returns to its start is closed, so its last corner is joined.
    #[test]
    fn stroke_scales_with_the_view_box_and_closes_rings() {
        let mut outline = plain_stroke();
        outline.join = azul_core::svg::SvgLineJoin::Round;
        outline.cap = azul_core::svg::SvgLineCap::Round;
        outline.dashes = vec![(4.0, 2.0)];
        outline.dash_offset = 1.0;
        let ops = ops_for(DisplayListItem::StrokedPath {
            bounds: WindowLogicalRect(LogicalRect {
                origin: LogicalPosition { x: 0.0, y: 0.0 },
                size: LogicalSize { width: 48.0, height: 48.0 },
            }),
            path: one_ring(vec![
                svg_line((2.0, 22.0), (12.0, 2.0)),
                svg_line((12.0, 2.0), (22.0, 22.0)),
                svg_line((22.0, 22.0), (2.0, 22.0)),
            ]),
            view_box: Some((0.0, 0.0, 24.0, 24.0)),
            color: ColorU { r: 200, g: 0, b: 0, a: 255 },
            width: 2.0,
            outline,
            mask: None,
        });

        // One user unit = 2 CSS px = 1.5 pt
        let thickness = ops.iter().find_map(|op| match op {
            Op::SetOutlineThickness { pt } => Some(pt.0),
            _ => None,
        });
        assert!((thickness.expect("a stroke width") - 3.0).abs() < 1e-3);
        let dash = ops.iter().find_map(|op| match op {
            Op::SetLineDashPattern { dash } => Some(dash.clone()),
            _ => None,
        });
        let dash = dash.expect("a dash pattern");
        assert_eq!(dash.pattern.as_slice(), [6.0, 3.0]);
        assert!((dash.offset - 1.5).abs() < 1e-3);
        assert!(ops.iter().any(|op| matches!(
            op,
            Op::SetLineJoinStyle { join: crate::graphics::LineJoinStyle::Round }
        )));
        assert!(ops.iter().any(|op| matches!(
            op,
            Op::SetLineCapStyle { cap: crate::graphics::LineCapStyle::Round }
        )));

        let line = ops
            .iter()
            .find_map(|op| match op {
                Op::DrawLine { line } => Some(line),
                _ => None,
            })
            .expect("the triangle");
        assert!(line.is_closed);
        assert_eq!(line.points.len(), 3, "three corners; the closing side is implied");
        // Top corner: x = 12 units = 24 px = 18 pt; y = 842 - 2 units * 2 px * 0.75
        assert!((line.points[1].p.x.0 - 18.0).abs() < 1e-2);
        assert!((line.points[1].p.y.0 - 839.0).abs() < 1e-2);
    }

    /// A stroke nobody could see writes nothing at all, not even an empty q/Q.
    #[test]
    fn invisible_stroke_writes_nothing() {
        let item = |color: ColorU, width: f32| DisplayListItem::StrokedPath {
            bounds: WindowLogicalRect(LogicalRect {
                origin: LogicalPosition { x: 0.0, y: 0.0 },
                size: LogicalSize { width: 10.0, height: 10.0 },
            }),
            path: one_ring(vec![svg_line((0.0, 0.0), (10.0, 10.0))]),
            view_box: None,
            color,
            width,
            outline: plain_stroke(),
            mask: None,
        };
        assert!(ops_for(item(ColorU { r: 0, g: 0, b: 0, a: 0 }, 2.0)).is_empty());
        assert!(ops_for(item(ColorU { r: 0, g: 0, b: 0, a: 255 }, 0.0)).is_empty());
    }

    fn masked_rect(geometry: Option<azul_layout::solver3::display_list::MaskGeometry>) -> Vec<Op> {
        let bounds = WindowLogicalRect(LogicalRect {
            origin: LogicalPosition { x: 10.0, y: 20.0 },
            size: LogicalSize { width: 120.0, height: 60.0 },
        });
        let mut dl = DisplayList::default();
        dl.items.push(DisplayListItem::PushImageMaskClip {
            bounds,
            mask_image: None,
            mask_rect: bounds,
            geometry,
        });
        dl.items.push(DisplayListItem::Rect {
            bounds,
            color: ColorU { r: 0, g: 0, b: 0, a: 255 },
            border_radius: BorderRadius::default(),
        });
        dl.items.push(DisplayListItem::PopImageMaskClip);
        display_list_to_printpdf_ops_with_margins(
            &dl,
            LogicalSize { width: 595.0, height: 842.0 },
            0.0,
            0.0,
            &empty_font_manager(),
            &ResolvedImages::new(),
            &mut BridgeResources::default(),
        )
        .expect("bridge conversion")
    }

    /// The fill of an SVG shape is its fill colour painted as a rectangle under a mask made from
    /// the path. The PDF clips to the path before the rectangle is filled, and leaves the clip
    /// afterwards.
    #[test]
    fn svg_fill_is_clipped_to_its_path() {
        // <path d="M10 50 L60 10 L110 50 Z" fill-rule="evenodd"/> in a 120x60 view box
        let ops = masked_rect(Some(azul_layout::solver3::display_list::MaskGeometry {
            path: one_ring(vec![
                svg_line((10.0, 50.0), (60.0, 10.0)),
                svg_line((60.0, 10.0), (110.0, 50.0)),
                svg_line((110.0, 50.0), (10.0, 50.0)),
            ]),
            view_box: Some((0.0, 0.0, 120.0, 60.0)),
            even_odd: true,
        }));

        let polygons: Vec<(usize, &crate::graphics::Polygon)> = ops
            .iter()
            .enumerate()
            .filter_map(|(i, op)| match op {
                Op::DrawPolygon { polygon } => Some((i, polygon)),
                _ => None,
            })
            .collect();
        let (clip_at, clip) = polygons
            .iter()
            .find(|(_, p)| matches!(p.mode, crate::graphics::PaintMode::Clip))
            .expect("a clip path");
        let (fill_at, _) = polygons
            .iter()
            .find(|(_, p)| matches!(p.mode, crate::graphics::PaintMode::Fill))
            .expect("the rectangle that carries the fill colour");
        assert!(clip_at < fill_at, "the clip is set before the fill is painted");
        assert!(matches!(clip.winding_order, crate::graphics::WindingOrder::EvenOdd));
        assert_eq!(clip.rings.len(), 1);
        assert_eq!(clip.rings[0].points.len(), 3, "three corners; a ring closes itself");
        // The apex: x = (10 + 60) px = 52.5 pt; y = 842 - (20 + 10) px * 0.75 = 819.5 pt
        assert!((clip.rings[0].points[1].p.x.0 - 52.5).abs() < 1e-2);
        assert!((clip.rings[0].points[1].p.y.0 - 819.5).abs() < 1e-2);

        let saves = ops.iter().filter(|op| matches!(op, Op::SaveGraphicsState)).count();
        let restores = ops.iter().filter(|op| matches!(op, Op::RestoreGraphicsState)).count();
        assert_eq!(saves, restores, "the clip ends with the mask");
        let last_restore = ops.iter().rposition(|op| matches!(op, Op::RestoreGraphicsState));
        assert!(last_restore.expect("a restore") > *fill_at, "the fill is inside the clip scope");
    }

    /// A mask that is only an image has no path to clip to. Its content is
    /// painted, and the scope the mask opens is still closed again.
    #[test]
    fn image_mask_without_geometry_keeps_the_state_balanced() {
        let ops = masked_rect(None);
        assert!(!ops.iter().any(|op| matches!(
            op,
            Op::DrawPolygon { polygon } if matches!(polygon.mode, crate::graphics::PaintMode::Clip)
        )));
        let saves = ops.iter().filter(|op| matches!(op, Op::SaveGraphicsState)).count();
        let restores = ops.iter().filter(|op| matches!(op, Op::RestoreGraphicsState)).count();
        assert_eq!(saves, restores);
        assert!(ops.iter().any(|op| matches!(op, Op::DrawPolygon { .. })), "the content is painted");
    }
}
