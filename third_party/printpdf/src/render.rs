//! Renders a [`PdfPage`] to an SVG string, for previews in a browser.
//!
//! A page is a list of [`Op`](crate::Op)s that change a graphics state (colors, line width, the
//! current font, the transformation matrix) and draw with it. The renderer walks that list,
//! keeps a stack of graphics states for `q` and `Q` (256 levels; deeper `q` operators are ignored
//! with a warning), and writes one SVG element per drawing op:
//! text, lines, polygons and rectangles, images and link annotations. The SVG is `width` by
//! `height` points (the media box) with the origin moved to the top left, as SVG expects. It stops
//! at 256 MiB, with a warning.
//!
//! Supported: text with embedded and standard fonts, glyph ids painted with the font's own
//! outlines, stroke and fill colors (RGB, CMYK, grayscale), dash patterns, joins and caps, the
//! transformation and text matrices, images, link annotations, layers and marked content (as
//! `<g>` groups).
//!
//! Not rendered: gradients (`sh`), transparency, blend modes and soft masks from an ExtGState,
//! inline images (a warning is added), and colorspace changes (`cs`, `CS`). Unknown operators
//! become an XML comment. A font is embedded in the SVG as a `@font-face` rule, so the SVG can be
//! large.
//!
//! The entry points are [`PdfPage::to_svg`](crate::PdfPage::to_svg), [`render_to_svg`] and
//! [`render_to_svg_async`].

use std::collections::BTreeMap;

use base64::Engine;
use serde_derive::{Deserialize, Serialize};

use crate::{
    form_walk::{form_in, inheriting_streams, scope_of, FormWalk},
    ops::{Op, PdfPage},
    xobject::FormXObject,
    Actions, BlackGenerationExtraFunction,
    BlackGenerationFunction, BlendMode, BuiltinFont, BuiltinOrExternalFontId, ChangedField, Color,
    CurTransMat, Destination, ExtendedGraphicsState, FontId, HalftoneType, Line, LineCapStyle,
    LinePoint,
    LineDashPattern, LineJoinStyle, OutputImageFormat, OverprintMode, PaintMode, PdfResources,
    PdfWarnMsg, Point, Polygon, Pt, RenderingIntent, SoftMask, TextItem, TextMatrix,
    TextRenderingMode, TransferExtraFunction, TransferFunction, UnderColorRemovalExtraFunction,
    UnderColorRemovalFunction, WindingOrder, XObject, XObjectId, XObjectTransform,
};

/// Options for rendering a page to SVG.
///
/// Used by [`PdfPage::to_svg`](crate::PdfPage::to_svg).
///
/// Serialized as JSON with camelCase keys.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfToSvgOptions {
    /// Formats to embed images in, in order of preference: each image is encoded with the first
    /// format that works, and left out when none does. `[Jpeg, Png, Avif]` tries JPEG, then
    /// PNG, then AVIF.
    ///
    /// [`Default`] is `[Png, Jpeg, Bmp]`, which most SVG converters read. A JSON object without
    /// this key gets the list of [`PdfToSvgOptions::web`] instead.
    #[serde(default = "default_image_formats_web")]
    pub image_formats: Vec<OutputImageFormat>,
}

impl Default for PdfToSvgOptions {
    fn default() -> Self {
        Self {
            image_formats: vec![
                OutputImageFormat::Png,
                OutputImageFormat::Jpeg,
                OutputImageFormat::Bmp,
            ],
        }
    }
}

fn default_image_formats_web() -> Vec<OutputImageFormat> {
    vec![
        OutputImageFormat::Avif,
        OutputImageFormat::Webp,
        OutputImageFormat::Jpeg,
        OutputImageFormat::Png,
        OutputImageFormat::Bmp,
        OutputImageFormat::Tiff,
        OutputImageFormat::Gif,
        OutputImageFormat::Tga,
    ]
}

impl PdfToSvgOptions {
    /// Options for browsers: tries AVIF and WebP first, which are the smallest, then JPEG, PNG,
    /// BMP, TIFF, GIF and TGA.
    pub fn web() -> Self {
        Self {
            image_formats: default_image_formats_web(),
        }
    }
}

// The deepest `q` nesting that is kept. ISO 32000-1 (Annex C) puts the limit of a conforming file
// at 28. A page that nests deeper is cut off, as every `q` copies the whole state.
const MAX_GRAPHICS_STATE_DEPTH: usize = 256;

// The most SVG, in bytes, a page is rendered to. Every `Do` writes the whole image again as a data
// URL, so a page that draws one image many times would grow without a bound.
const MAX_SVG_BYTES: usize = 256 * 1024 * 1024;

// The graphics state of the page while it is rendered, pushed and popped by `q` and `Q`.
#[derive(Debug, Clone, Default)]
struct GraphicsState {
    text_cursor: Point,
    current_font: Option<BuiltinOrExternalFontId>,
    transform_matrix: Option<CurTransMat>,
    text_matrix: Option<TextMatrix>,
    fill_color: Option<Color>,
    stroke_color: Option<Color>,
    stroke_width: Option<Pt>,
    dash_array: Option<LineDashPattern>,
    line_join: Option<LineJoinStyle>,
    line_cap: Option<LineCapStyle>,
    character_spacing: Option<f32>,
    line_offset: Option<f32>,
    miter_limit: Option<Pt>,
    horizontal_scaling: Option<f32>,
    text_leading: Option<Pt>,
    rendering_intent: Option<RenderingIntent>,
    text_rendering_mode: Option<TextRenderingMode>,
    overprint_mode: Option<OverprintMode>,
    overprint_stroke: bool,
    overprint_fill: bool,
    in_compatibility_section: bool,
    word_spacing: Option<Pt>,
    // The sizes set at this level only; `get_font_size` looks through the levels below, so a `q`
    // does not copy a table that grows with the page.
    font_sizes: BTreeMap<BuiltinOrExternalFontId, Pt>,

    // Extra options
    black_generation: Option<BlackGenerationFunction>,
    black_generation_extra: Option<BlackGenerationExtraFunction>,
    under_color_removal: Option<UnderColorRemovalFunction>,
    under_color_removal_extra: Option<UnderColorRemovalExtraFunction>,
    transfer_function: Option<TransferFunction>,
    transfer_extra_function: Option<TransferExtraFunction>,
    halftone_dictionary: Option<HalftoneType>,
    flatness_tolerance: Option<f32>,
    smoothness_tolerance: Option<f32>,
    stroke_adjustment: Option<bool>,
    blend_mode: Option<BlendMode>,
    soft_mask: Option<SoftMask>,
    current_stroke_alpha: Option<f32>,
    current_fill_alpha: Option<f32>,
    alpha_is_shape: Option<bool>,
    text_knockout: Option<bool>,
}

struct GraphicsStateVec {
    _internal: Vec<GraphicsState>,
    // `q` operators beyond `MAX_GRAPHICS_STATE_DEPTH`, which pushed nothing; their `Q` pops nothing.
    skipped_saves: usize,
    // The states below the content that is being rendered. A `Q` that would pop one of them is
    // ignored: a form cannot restore the state of the content that draws it.
    floor: usize,
}

// Where the states of a form start, to drop them when it ends.
struct FormMark {
    len: usize,
    floor: usize,
    skipped_saves: usize,
}

impl GraphicsStateVec {
    pub fn new() -> Self {
        Self {
            _internal: vec![GraphicsState::default()],
            skipped_saves: 0,
            floor: 0,
        }
    }
    // Starts the states of a form: a copy of the current one with the text position reset, and
    // `form_to_page` (the matrix of the form and the placement of its `Do`) applied before the
    // current transformation. `None` when the stack is full.
    pub fn enter_form(&mut self, form_to_page: [f32; 6]) -> Option<FormMark> {
        if self._internal.len() >= MAX_GRAPHICS_STATE_DEPTH {
            return None;
        }
        let outer = self.get_transform_matrix().as_array();
        let mark = FormMark {
            len: self._internal.len(),
            floor: self.floor,
            skipped_saves: self.skipped_saves,
        };
        self.save_gs()?;
        let state = self._internal.last_mut()?;
        state.transform_matrix = Some(CurTransMat::Raw(CurTransMat::combine_matrix(
            form_to_page,
            outer,
        )));
        state.text_cursor = Point::default();
        state.text_matrix = None;
        self.floor = self._internal.len();
        self.skipped_saves = 0;
        Some(mark)
    }
    // Drops the states of the form that `enter_form` started.
    pub fn exit_form(&mut self, mark: FormMark) {
        self._internal.truncate(mark.len);
        if self._internal.is_empty() {
            self._internal.push(GraphicsState::default());
        }
        self.floor = mark.floor;
        self.skipped_saves = mark.skipped_saves;
    }
    // `None` when the stack is full and nothing was pushed.
    pub fn save_gs(&mut self) -> Option<()> {
        if self._internal.len() >= MAX_GRAPHICS_STATE_DEPTH {
            self.skipped_saves = self.skipped_saves.saturating_add(1);
            return None;
        }
        let last = self._internal.last_mut()?;
        let font_sizes = std::mem::take(&mut last.font_sizes);
        let next = last.clone();
        last.font_sizes = font_sizes;
        self._internal.push(next);
        Some(())
    }
    // `None` when the `Q` matches a `q` that `save_gs` skipped and nothing was popped.
    pub fn restore_gs(&mut self) -> Option<()> {
        if self.skipped_saves > 0 {
            self.skipped_saves = self.skipped_saves.saturating_sub(1);
            return None;
        }
        if self._internal.len() <= self.floor {
            return None;
        }
        self._internal.pop();
        if self._internal.is_empty() {
            self._internal.push(GraphicsState::default());
        }
        Some(())
    }
    pub fn load_gs(&mut self, g: &ExtendedGraphicsState) -> Option<()> {
        let last = self._internal.last_mut()?;

        // Only apply fields that are marked as changed
        if g.changed_fields.contains(&ChangedField::OverprintFill) {
            last.overprint_fill = g.overprint_fill;
        }
        if g.changed_fields.contains(&ChangedField::OverprintStroke) {
            last.overprint_stroke = g.overprint_stroke;
        }
        if g.changed_fields.contains(&ChangedField::Font) {
            last.current_font = g.font.clone();
        }
        if g.changed_fields.contains(&ChangedField::LineDashPattern) {
            last.dash_array = g.line_dash_pattern.clone();
        }
        if g.changed_fields.contains(&ChangedField::LineWidth) {
            last.stroke_width = Some(Pt(g.line_width));
        }
        if g.changed_fields.contains(&ChangedField::LineCap) {
            last.line_cap = Some(g.line_cap);
        }
        if g.changed_fields.contains(&ChangedField::LineJoin) {
            last.line_join = Some(g.line_join);
        }
        if g.changed_fields.contains(&ChangedField::RenderingIntent) {
            last.rendering_intent = Some(g.rendering_intent);
        }
        if g.changed_fields.contains(&ChangedField::OverprintMode) {
            last.overprint_mode = Some(g.overprint_mode);
        }
        if g.changed_fields.contains(&ChangedField::MiterLimit) {
            last.miter_limit = Some(Pt(g.miter_limit));
        }
        if g.changed_fields.contains(&ChangedField::BlackGeneration) {
            last.black_generation = g.black_generation;
        }
        if g.changed_fields
            .contains(&ChangedField::BlackGenerationExtra)
        {
            last.black_generation_extra = g.black_generation_extra;
        }
        if g.changed_fields.contains(&ChangedField::UnderColorRemoval) {
            last.under_color_removal = g.under_color_removal;
        }
        if g.changed_fields
            .contains(&ChangedField::UnderColorRemovalExtra)
        {
            last.under_color_removal_extra = g.under_color_removal_extra;
        }
        if g.changed_fields.contains(&ChangedField::TransferFunction) {
            last.transfer_function = g.transfer_function;
        }
        if g.changed_fields
            .contains(&ChangedField::TransferFunctionExtra)
        {
            last.transfer_extra_function = g.transfer_extra_function;
        }
        if g.changed_fields.contains(&ChangedField::HalftoneDictionary) {
            last.halftone_dictionary = g.halftone_dictionary.clone();
        }
        if g.changed_fields.contains(&ChangedField::SoftMask) {
            last.soft_mask = g.soft_mask.clone();
        }
        if g.changed_fields.contains(&ChangedField::FlatnessTolerance) {
            last.flatness_tolerance = Some(g.flatness_tolerance);
        }
        if g.changed_fields
            .contains(&ChangedField::SmoothnessTolerance)
        {
            last.smoothness_tolerance = Some(g.smoothness_tolerance);
        }
        if g.changed_fields.contains(&ChangedField::StrokeAdjustment) {
            last.stroke_adjustment = Some(g.stroke_adjustment);
        }
        if g.changed_fields.contains(&ChangedField::BlendMode) {
            last.blend_mode = Some(g.blend_mode);
        }
        if g.changed_fields.contains(&ChangedField::CurrentStrokeAlpha) {
            last.current_stroke_alpha = Some(g.current_stroke_alpha);
        }
        if g.changed_fields.contains(&ChangedField::CurrentFillAlpha) {
            last.current_fill_alpha = Some(g.current_fill_alpha);
        }
        if g.changed_fields.contains(&ChangedField::AlphaIsShape) {
            last.alpha_is_shape = Some(g.alpha_is_shape);
        }
        if g.changed_fields.contains(&ChangedField::TextKnockout) {
            last.text_knockout = Some(g.text_knockout);
        }

        Some(())
    }
    pub fn set_rendering_intent(&mut self, c: RenderingIntent) -> Option<()> {
        self._internal.last_mut()?.rendering_intent = Some(c);
        Some(())
    }
    pub fn set_horizontal_scaling(&mut self, percent: f32) -> Option<()> {
        self._internal.last_mut()?.horizontal_scaling = Some(percent);
        Some(())
    }
    pub fn set_line_height(&mut self, lh: Pt) -> Option<()> {
        self._internal.last_mut()?.text_leading = Some(lh);
        Some(())
    }
    pub fn set_text_cursor(&mut self, tc: Point) -> Option<()> {
        self._internal.last_mut()?.text_cursor = tc;
        Some(())
    }
    pub fn set_character_spacing(&mut self, cs: f32) -> Option<()> {
        self._internal.last_mut()?.character_spacing = Some(cs);
        Some(())
    }
    // `cm` applies before the transformation that is in effect (ISO 32000-1, 8.3.4).
    pub fn set_cur_trans_mat(&mut self, cm: CurTransMat) -> Option<()> {
        let last = self._internal.last_mut()?;
        last.transform_matrix = Some(match last.transform_matrix {
            None | Some(CurTransMat::Identity) => cm,
            Some(current) => CurTransMat::Raw(CurTransMat::combine_matrix(
                cm.as_array(),
                current.as_array(),
            )),
        });
        Some(())
    }
    pub fn set_text_mat(&mut self, tm: TextMatrix) -> Option<()> {
        self._internal.last_mut()?.text_matrix = Some(tm);
        Some(())
    }
    pub fn set_dash_pattern(&mut self, da: LineDashPattern) -> Option<()> {
        self._internal.last_mut()?.dash_array = Some(da);
        Some(())
    }
    pub fn set_fill_color(&mut self, c: Color) -> Option<()> {
        self._internal.last_mut()?.fill_color = Some(c);
        Some(())
    }
    pub fn set_outline_color(&mut self, c: Color) -> Option<()> {
        self._internal.last_mut()?.stroke_color = Some(c);
        Some(())
    }
    pub fn set_line_join(&mut self, c: LineJoinStyle) -> Option<()> {
        self._internal.last_mut()?.line_join = Some(c);
        Some(())
    }
    pub fn set_line_cap(&mut self, c: LineCapStyle) -> Option<()> {
        self._internal.last_mut()?.line_cap = Some(c);
        Some(())
    }
    pub fn set_line_offset(&mut self, c: f32) -> Option<()> {
        self._internal.last_mut()?.line_offset = Some(c);
        Some(())
    }
    pub fn set_stroke_width(&mut self, c: Pt) -> Option<()> {
        self._internal.last_mut()?.stroke_width = Some(c);
        Some(())
    }
    pub fn set_word_spacing(&mut self, c: Pt) -> Option<()> {
        self._internal.last_mut()?.word_spacing = Some(c);
        Some(())
    }
    pub fn set_font_size(&mut self, font: &FontId, c: Pt) -> Option<()> {
        let last = self._internal.last_mut()?;
        let id = BuiltinOrExternalFontId::External(font.clone());
        last.font_sizes.insert(id.clone(), c);
        // SetFont selects the current font for subsequent ShowText ops. Without
        // this, get_current_font() returns None and text falls back to Times.
        last.current_font = Some(id);
        Some(())
    }
    pub fn set_font_size_builtin(&mut self, font: &BuiltinFont, c: Pt) -> Option<()> {
        let last = self._internal.last_mut()?;
        let id = BuiltinOrExternalFontId::Builtin(*font);
        last.font_sizes.insert(id.clone(), c);
        last.current_font = Some(id);
        Some(())
    }
    pub fn set_miter_limit(&mut self, c: Pt) -> Option<()> {
        self._internal.last_mut()?.miter_limit = Some(c);
        Some(())
    }
    pub fn set_text_rendering_mode(&mut self, c: TextRenderingMode) -> Option<()> {
        self._internal.last_mut()?.text_rendering_mode = Some(c);
        Some(())
    }
    pub fn begin_compatibility_section(&mut self) -> Option<()> {
        self._internal.last_mut()?.in_compatibility_section = true;
        Some(())
    }
    pub fn end_compatibility_section(&mut self) -> Option<()> {
        self._internal.last_mut()?.in_compatibility_section = false;
        Some(())
    }

    pub fn get_current(&self) -> Option<&GraphicsState> {
        self._internal.last()
    }

    pub fn get_font_size(&self, font: &BuiltinOrExternalFontId) -> Pt {
        self._internal
            .iter()
            .rev()
            .find_map(|gs| gs.font_sizes.get(font))
            .copied()
            .unwrap_or(Pt(12.0)) // Default font size
    }

    pub fn get_text_cursor(&self) -> Point {
        self.get_current()
            .map(|gs| gs.text_cursor)
            .unwrap_or_default()
    }

    pub fn get_transform_matrix(&self) -> CurTransMat {
        self.get_current()
            .and_then(|gs| gs.transform_matrix)
            .unwrap_or(CurTransMat::Identity)
    }

    pub fn get_text_matrix(&self) -> TextMatrix {
        self.get_current()
            .and_then(|gs| gs.text_matrix)
            .unwrap_or(TextMatrix::Translate(Pt(0.0), Pt(0.0)))
    }

    pub fn get_fill_color(&self) -> Color {
        self.get_current()
            .and_then(|gs| gs.fill_color.clone())
            .unwrap_or(Color::Rgb(crate::Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                icc_profile: None,
            }))
    }

    pub fn get_stroke_color(&self) -> Color {
        self.get_current()
            .and_then(|gs| gs.stroke_color.clone())
            .unwrap_or(Color::Rgb(crate::Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                icc_profile: None,
            }))
    }

    pub fn get_stroke_width(&self) -> Pt {
        self.get_current()
            .and_then(|gs| gs.stroke_width)
            .unwrap_or(Pt(1.0))
    }

    pub fn get_dash_array(&self) -> Option<LineDashPattern> {
        self.get_current()?.dash_array.clone()
    }

    pub fn get_line_join(&self) -> LineJoinStyle {
        self.get_current()
            .and_then(|gs| gs.line_join)
            .unwrap_or(LineJoinStyle::Miter)
    }

    pub fn get_line_cap(&self) -> LineCapStyle {
        self.get_current()
            .and_then(|gs| gs.line_cap)
            .unwrap_or(LineCapStyle::Butt)
    }

    pub fn get_text_rendering_mode(&self) -> TextRenderingMode {
        self.get_current()
            .and_then(|gs| gs.text_rendering_mode)
            .unwrap_or(TextRenderingMode::Fill)
    }

    pub fn get_character_spacing(&self) -> f32 {
        self.get_current()
            .and_then(|gs| gs.character_spacing)
            .unwrap_or(0.0)
    }

    pub fn get_word_spacing(&self) -> f32 {
        self.get_current()
            .and_then(|gs| gs.word_spacing)
            .unwrap_or(Pt(0.0))
            .0
    }

    pub fn get_text_leading(&self) -> Pt {
        self.get_current()
            .and_then(|gs| gs.text_leading)
            .unwrap_or(Pt(0.0))
    }

    pub fn get_horizontal_scaling(&self) -> f32 {
        self.get_current()
            .and_then(|gs| gs.horizontal_scaling)
            .unwrap_or(100.0)
    }

    pub fn get_current_font(&self) -> Option<BuiltinOrExternalFontId> {
        self.get_current()?.current_font.clone()
    }
}

/// Renders `page` to an SVG string.
///
/// `resources` must hold the fonts, images, graphics states and layers the page refers to (the
/// resources of the document the page belongs to). Problems that do not stop rendering, such as a
/// font that cannot be embedded, are added to `warnings`.
///
/// The SVG is as wide and high as the media box in points, with the origin at the top left.
/// It draws text (embedded fonts become `@font-face` rules, so the SVG can be large), lines,
/// polygons, rectangles, images and link annotations, with colors, dash patterns, joins, caps and
/// the transformation and text matrices; layers and marked content become `<g>` groups.
///
/// Not rendered: gradients (`sh`), transparency, blend modes and soft masks, colorspace changes
/// and inline images (these add a warning). An unknown operator becomes an XML comment.
pub(crate) fn render_to_svg(
    page: &PdfPage,
    resources: &PdfResources,
    opts: &PdfToSvgOptions,
    warnings: &mut Vec<PdfWarnMsg>,
) -> String {
    let map = encoded_image_data_map(page, resources, opts);
    render_to_svg_internal(page, resources, map, &opts.image_formats, warnings)
}

/// Like [`render_to_svg`], but encodes the images with the async encoder, for targets where
/// blocking is not allowed (wasm).
pub(crate) async fn render_to_svg_async(
    page: &PdfPage,
    resources: &PdfResources,
    opts: &PdfToSvgOptions,
    warnings: &mut Vec<PdfWarnMsg>,
) -> String {
    let map = encoded_image_data_map_async(page, resources, opts).await;
    render_to_svg_internal(page, resources, map, &opts.image_formats, warnings)
}

async fn encoded_image_data_map_async(
    page: &PdfPage,
    resources: &PdfResources,
    opts: &PdfToSvgOptions,
) -> BTreeMap<XObjectId, (Vec<u8>, OutputImageFormat)> {
    let resources_needed = page.get_xobject_ids();
    let mut map = BTreeMap::new();
    let mut tried = std::collections::BTreeSet::new();
    for f in resources_needed {
        if !tried.insert(f.clone()) {
            continue; // the page draws this image again
        }
        let encoded = match resources.xobjects.map.get(&f) {
            Some(XObject::Image(i)) => i.encode_to_bytes_async(&opts.image_formats).await,
            _ => continue,
        };
        match encoded {
            Ok(o) => {
                map.insert(f.clone(), o);
            }
            Err(_) => continue,
        }
    }
    map
}

fn encoded_image_data_map(
    page: &PdfPage,
    resources: &PdfResources,
    opts: &PdfToSvgOptions,
) -> BTreeMap<XObjectId, (Vec<u8>, OutputImageFormat)> {
    let resources_needed = page.get_xobject_ids();
    let mut map = BTreeMap::new();
    let mut tried = std::collections::BTreeSet::new();
    for f in resources_needed {
        if !tried.insert(f.clone()) {
            continue; // the page draws this image again
        }
        let encoded = match resources.xobjects.map.get(&f) {
            Some(XObject::Image(i)) => i.encode_to_bytes(&opts.image_formats),
            _ => continue,
        };
        match encoded {
            Ok(o) => {
                map.insert(f.clone(), o);
            }
            Err(_) => continue,
        }
    }
    map
}

fn render_to_svg_internal(
    page: &PdfPage,
    resources: &PdfResources,
    map: BTreeMap<XObjectId, (Vec<u8>, OutputImageFormat)>,
    image_formats: &[OutputImageFormat],
    warnings: &mut Vec<PdfWarnMsg>,
) -> String {
    let width = page.media_box.width.0;
    let height = page.media_box.height.0;

    let mut head = String::new();
    head.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}px" height="{h}px" viewBox="0 0 {w} {h}">"#,
        w = width,
        h = height
    ));
    head.push('\n');

    // Embed the fonts as @font-face rules: the fonts of the page, and of the forms that have
    // no resources of their own and so use the ones of the document.
    let mut streams: Vec<&[Op]> = vec![page.ops.as_slice()];
    streams.extend(inheriting_streams(resources));
    let (_, subset_fonts) = crate::serialize::prepare_fonts_for_streams(
        &resources.fonts.map,
        &streams,
        true,
        warnings,
    );
    let mut font_faces = String::new();
    // Use the original font bytes rather than the PDF subset: the subset has no OS/2 table,
    // so browsers reject it as a webfont and fall back to a default serif. The `<text>`
    // elements hold real Unicode text, which the original cmap maps directly.
    for (font_id, font) in subset_fonts.iter() {
        let original = resources
            .fonts
            .map
            .get(font_id)
            .and_then(|f| f.parsed_font.source_bytes());
        let bytes: &[u8] = match &original {
            Some(b) => b.as_slice(),
            None => &font.subset_font_bytes,
        };
        font_faces.push_str(&font_face_rule(&svg_font_family(&font_id.0), bytes));
    }

    let mut renderer = SvgRenderer {
        body: String::new(),
        fixed_len: head.len() + font_faces.len(),
        gst: GraphicsStateVec::new(),
        in_text_section: false,
        warned_about_depth: false,
        current_svg_group: Vec::new(), // open `<g>` elements, innermost last
        glyph_outlines: GlyphOutlineDefs::default(),
        height,
        root: resources,
        image_map: &map,
        image_formats,
        form_images: BTreeMap::new(),
        form_fonts: BTreeMap::new(),
        form_tags: BTreeMap::new(),
        walk: FormWalk::new(),
        full: false,
    };
    renderer.render_ops(&page.ops, resources, "", warnings);
    renderer.finish(head, font_faces)
}

/// The `@font-face` rule that makes the font program `bytes` available as `family`.
fn font_face_rule(family: &str, bytes: &[u8]) -> String {
    format!(
        r#"@font-face {{ font-family: "{}"; src: url("data:font/otf;charset=utf-8;base64,{}"); }}"#,
        family,
        base64::prelude::BASE64_STANDARD.encode(bytes),
    )
}

/// The id under which a font of the scope with the tag `tag` is known in the SVG: fonts of
/// different forms can have the same name, and one SVG holds them all.
fn qualify(tag: &str, id: &FontId) -> FontId {
    if tag.is_empty() {
        id.clone()
    } else {
        FontId(format!("{tag}{}", id.0))
    }
}

/// An image encoded in one of the formats the SVG embeds.
type EncodedImage = (Vec<u8>, OutputImageFormat);

/// What rendering a page keeps while it walks through the ops of the page and of the forms it
/// draws.
struct SvgRenderer<'a> {
    /// The SVG elements written so far.
    body: String,
    /// The bytes of the header and the font rules, which count against [`MAX_SVG_BYTES`].
    fixed_len: usize,
    gst: GraphicsStateVec,
    in_text_section: bool,
    warned_about_depth: bool,
    current_svg_group: Vec<String>,
    glyph_outlines: GlyphOutlineDefs,
    height: f32,
    /// The resources of the document, which hold the layers.
    root: &'a PdfResources,
    /// The images the page draws, encoded.
    image_map: &'a BTreeMap<XObjectId, EncodedImage>,
    image_formats: &'a [OutputImageFormat],
    /// The images of forms, encoded when they are first drawn, by the tag of their scope and id.
    form_images: BTreeMap<(String, XObjectId), Option<EncodedImage>>,
    /// The fonts of forms with resources of their own, by `qualify`d id.
    form_fonts: BTreeMap<FontId, &'a crate::font::PdfFont>,
    /// The tag of each form that has resources of its own, by the address of the form.
    form_tags: BTreeMap<usize, String>,
    walk: FormWalk,
    /// [`MAX_SVG_BYTES`] is reached: nothing more is written.
    full: bool,
}

impl<'a> SvgRenderer<'a> {
    /// Writes the elements for `ops`, which name their fonts, XObjects and graphics states in
    /// `scope`. `tag` is empty for the page and names the scope of a form otherwise.
    fn render_ops(
        &mut self,
        ops: &[Op],
        scope: &'a PdfResources,
        tag: &str,
        warnings: &mut Vec<PdfWarnMsg>,
    ) {
        let height = self.height;
        for op in ops {
            if self.full || self.fixed_len + self.body.len() > MAX_SVG_BYTES {
                if !self.full {
                    self.full = true;
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        format!(
                            "the SVG is larger than {MAX_SVG_BYTES} bytes, the rest of the page is left out"
                        ),
                    ));
                }
                break;
            }
            match op {
                Op::SetColorSpaceFill { .. } => {
                    // Colorspace changes are ignored.
                }
                Op::SetColorSpaceStroke { .. } => {
                    // Colorspace changes are ignored.
                }
                Op::SetRenderingIntent { intent } => {
                    self.gst.set_rendering_intent(*intent);
                }
                Op::SetHorizontalScaling { percent } => {
                    self.gst.set_horizontal_scaling(*percent);
                }
                Op::SetLineOffset { multiplier } => {
                    self.gst.set_line_offset(*multiplier);
                }
                Op::SetLineHeight { lh } => {
                    self.gst.set_line_height(*lh);
                }
                Op::SetTextCursor { pos } => {
                    self.gst.set_text_cursor(*pos);
                }
                Op::SetTransformationMatrix { matrix } => {
                    self.gst.set_cur_trans_mat(*matrix);
                }
                Op::SetTextMatrix { matrix } => {
                    self.gst.set_text_mat(*matrix);
                }
                Op::SetFillColor { col } => {
                    self.gst.set_fill_color(col.clone());
                }
                Op::SetOutlineColor { col } => {
                    self.gst.set_outline_color(col.clone());
                }
                Op::SetOutlineThickness { pt } => {
                    self.gst.set_stroke_width(*pt);
                }
                Op::SetLineDashPattern { dash } => {
                    self.gst.set_dash_pattern(dash.clone());
                }
                Op::SetLineJoinStyle { join } => {
                    self.gst.set_line_join(*join);
                }
                Op::SetLineCapStyle { cap } => {
                    self.gst.set_line_cap(*cap);
                }
                Op::SetMiterLimit { limit } => {
                    self.gst.set_miter_limit(*limit);
                }
                Op::SetTextRenderingMode { mode } => {
                    self.gst.set_text_rendering_mode(*mode);
                }
                Op::SetCharacterSpacing { multiplier } => {
                    self.gst.set_character_spacing(*multiplier);
                }
                Op::SetWordSpacing { pt } => {
                    self.gst.set_word_spacing(*pt);
                }
                Op::SetFont { font, size } => {
                    match font {
                        crate::ops::PdfFontHandle::Builtin(builtin) => {
                            self.gst.set_font_size_builtin(builtin, *size);
                        }
                        crate::ops::PdfFontHandle::External(font_id) => {
                            self.gst.set_font_size(&qualify(tag, font_id), *size);
                        }
                    }
                }
                Op::ShowText { items } => {
                    if self.in_text_section {
                        // No font set: fall back to the default builtin font.
                        let current_font = self
                            .gst
                            .get_current_font()
                            .unwrap_or(BuiltinOrExternalFontId::Builtin(BuiltinFont::default()));
                        let program = self.font_program(&current_font);
                        let text_svg = render_show_text(
                            items,
                            &current_font,
                            program,
                            &self.gst,
                            height,
                            &mut self.glyph_outlines,
                        );
                        self.body.push_str(&text_svg);
                    }
                }

                Op::BeginMarkedContent { tag } => {
                    self.body.push_str(&format!(
                        "<g class=\"marked-content\" data-tag=\"{}\">",
                        escape_xml_attr(tag)
                    ));
                    self.current_svg_group.push(String::from("marked"));
                }
                Op::BeginMarkedContentWithProperties { tag, properties: _ } => {
                    self.body.push_str(&format!(
                        "<g class=\"marked-content\" data-tag=\"{}\">",
                        escape_xml_attr(tag)
                    ));
                    self.current_svg_group.push(String::from("marked"));
                }
                Op::DefineMarkedContentPoint { tag, properties: _ } => {
                    self.body.push_str(&format!(
                        "<g class=\"marked-content-point\" data-tag=\"{}\"></g>",
                        escape_xml_attr(tag)
                    ));
                }
                Op::EndMarkedContent | Op::EndMarkedContentWithProperties => {
                    if let Some(group_type) = self.current_svg_group.last() {
                        if group_type == "marked" {
                            self.current_svg_group.pop();
                            self.body.push_str("</g>");
                        }
                    }
                }
                Op::BeginCompatibilitySection => {
                    self.gst.begin_compatibility_section();
                }
                Op::EndCompatibilitySection => {
                    self.gst.end_compatibility_section();
                }
                Op::Marker { id } => {
                    self.body.push_str(&format!(
                        "<g class='marker' id='{}' style='display:none;'></g>",
                        escape_xml_attr(id)
                    ));
                }
                Op::BeginLayer { layer_id } | Op::BeginOptionalContent { layer_id } => {
                    if let Some(layer) = self.root.layers.map.get(layer_id) {
                        self.body.push_str(&format!(
                            "<g class=\"layer\" id=\"{}\" data-name=\"{}\">",
                            escape_xml_attr(&layer_id.0),
                            escape_xml_attr(&layer.name)
                        ));
                    } else {
                        self.body.push_str(&format!(
                            "<g class=\"layer\" id=\"{}\">",
                            escape_xml_attr(&layer_id.0)
                        ));
                    }
                    self.current_svg_group.push(String::from("layer"));
                }
                Op::EndLayer | Op::EndOptionalContent => {
                    if let Some(group_type) = self.current_svg_group.last() {
                        if group_type == "layer" {
                            self.current_svg_group.pop();
                            self.body.push_str("</g>");
                        }
                    }
                }
                Op::SaveGraphicsState => {
                    if self.gst.save_gs().is_some() {
                        self.body.push_str("<g>"); // Group to isolate graphics state changes
                        self.current_svg_group.push(String::from("gs"));
                    } else if !self.warned_about_depth {
                        self.warned_about_depth = true;
                        warnings.push(PdfWarnMsg::warning(
                            0,
                            0,
                            format!(
                                "graphics state nesting is deeper than {MAX_GRAPHICS_STATE_DEPTH}, \
                                 the `q` operators beyond it and their `Q` are ignored"
                            ),
                        ));
                    }
                }
                Op::RestoreGraphicsState => {
                    if self.gst.restore_gs().is_some() {
                        if let Some(group_type) = self.current_svg_group.last() {
                            if group_type == "gs" {
                                self.current_svg_group.pop();
                                self.body.push_str("</g>");
                            }
                        }
                    }
                }
                Op::LoadGraphicsState { gs } => {
                    if let Some(graphics_state) = scope.extgstates.map.get(gs) {
                        self.gst.load_gs(graphics_state);
                    }
                }

                Op::StartTextSection => {
                    self.in_text_section = true;
                }
                Op::EndTextSection => {
                    self.in_text_section = false;
                }
                Op::AddLineBreak => {
                    // `T*` keeps the x of the line start and drops y by the leading.
                    let leading = self.gst.get_text_leading();
                    let current_cursor = self.gst.get_text_cursor();
                    self.gst.set_text_cursor(Point {
                        x: current_cursor.x,
                        y: Pt(current_cursor.y.0 - leading.0),
                    });
                }
                Op::MoveTextCursorAndSetLeading { tx, ty } => {
                    let current_cursor = self.gst.get_text_cursor();
                    self.gst.set_text_cursor(Point {
                        x: Pt(current_cursor.x.0 + tx),
                        y: Pt(current_cursor.y.0 + ty),
                    });
                    self.gst.set_line_height(Pt(-ty)); // Leading is -ty
                }
                Op::MoveToNextLineShowText { text } => {
                    if self.in_text_section {
                        // Next line, as with `T*`.
                        let leading = self.gst.get_text_leading();
                        let current_cursor = self.gst.get_text_cursor();
                        self.gst.set_text_cursor(Point {
                            x: current_cursor.x,
                            y: Pt(current_cursor.y.0 - leading.0),
                        });

                        let items = vec![TextItem::Text(text.clone())];

                        if let Some(font) = self.gst.get_current_font() {
                            let text_svg = render_text_items_to_svg(
                                &items,
                                &font,
                                &self.gst,
                                height,
                                TextLayer::Painted,
                            );
                            self.body.push_str(&text_svg);
                        }
                    }
                }
                Op::SetSpacingMoveAndShowText {
                    word_spacing,
                    char_spacing,
                    text,
                } => {
                    if self.in_text_section {
                        self.gst.set_word_spacing(Pt(*word_spacing));
                        self.gst.set_character_spacing(*char_spacing);

                        // Next line, as with `T*`.
                        let leading = self.gst.get_text_leading();
                        let current_cursor = self.gst.get_text_cursor();
                        self.gst.set_text_cursor(Point {
                            x: current_cursor.x,
                            y: Pt(current_cursor.y.0 - leading.0),
                        });

                        let items = vec![TextItem::Text(text.clone())];

                        if let Some(font) = self.gst.get_current_font() {
                            let text_svg = render_text_items_to_svg(
                                &items,
                                &font,
                                &self.gst,
                                height,
                                TextLayer::Painted,
                            );
                            self.body.push_str(&text_svg);
                        }
                    }
                }

                Op::DrawLine { line } => {
                    let line_svg = render_line_to_svg(line, &self.gst, height);
                    self.body.push_str(&line_svg);
                }
                Op::DrawPolygon { polygon } => {
                    let polygon_svg = render_polygon_to_svg(polygon, &self.gst, height);
                    self.body.push_str(&polygon_svg);
                }
                Op::DrawRectangle { rectangle } => {
                    let polygon_svg =
                        render_polygon_to_svg(&rectangle.to_polygon(), &self.gst, height);
                    self.body.push_str(&polygon_svg);
                }
                Op::UseXobject { id, transform } => {
                    if let Some(form) = form_in(scope, id) {
                        self.render_form(form, id, transform, scope, tag, warnings);
                    } else {
                        let xobject_svg = self.render_image(id, transform, scope, tag);
                        self.body.push_str(&xobject_svg);
                    }
                }
                Op::LinkAnnotation { link } => {
                    match &link.actions {
                        Actions::Goto(destination) => {
                            let page_num = match destination {
                                Destination::Xyz { page, .. } => page,
                            };

                            self.body.push_str(&format!("<a href=\"#page{}\">", page_num));

                            self.body.push_str(&format!(
                                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" \
                                 stroke=\"none\" pointer-events=\"all\"/>",
                                link.rect.x.0,
                                height - link.rect.y.0 - link.rect.height.0, // PDF y is up, SVG y is down
                                link.rect.width.0,
                                link.rect.height.0
                            ));

                            self.body.push_str("</a>");
                        }
                        Actions::Uri(uri) => match safe_link_target(uri) {
                            Some(href) => {
                                self.body
                                    .push_str(&format!("<a href=\"{}\" target=\"_blank\">", href));

                                self.body.push_str(&format!(
                                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" \
                                     stroke=\"none\" pointer-events=\"all\"/>",
                                    link.rect.x.0,
                                    height - link.rect.y.0 - link.rect.height.0,
                                    link.rect.width.0,
                                    link.rect.height.0
                                ));

                                self.body.push_str("</a>");
                            }
                            None => warnings.push(PdfWarnMsg::warning(
                                0,
                                0,
                                "A link whose URI is not http, https, mailto, tel or a relative \
                                 reference is left out of the SVG"
                                    .to_string(),
                            )),
                        },
                    }
                }

                Op::BeginInlineImage | Op::BeginInlineImageData | Op::EndInlineImage => {
                    warnings.push(PdfWarnMsg::info(
                        0,
                        0,
                        "Inline image rendering not fully implemented in SVG output".to_string(),
                    ));
                }

                // Gradients (`sh`) are not rendered.
                Op::PaintShading { .. } => {}

                Op::Unknown { key, value: _ } => {
                    self.body.push_str(&format!(
                        "<!-- Unknown PDF operator: {} -->",
                        comment_text(key)
                    ));
                }
            }
        }
    }

    /// The font program of an external font that is selected, in the page's resources or in
    /// the resources of the form that selected it.
    fn font_program(&self, font: &BuiltinOrExternalFontId) -> Option<&'a crate::ParsedFont> {
        let BuiltinOrExternalFontId::External(id) = font else {
            return None;
        };
        self.form_fonts
            .get(id)
            .copied()
            .or_else(|| self.root.fonts.map.get(id))
            .map(|font| &font.parsed_font)
    }

    /// The tag of the scope of `form`, the same for every time the form is drawn.
    fn tag_of(&mut self, form: &FormXObject) -> String {
        let address = form as *const FormXObject as usize;
        let next = self.form_tags.len();
        self.form_tags
            .entry(address)
            .or_insert_with(|| format!("form{next}:"))
            .clone()
    }

    /// Writes the elements for the form `form`, drawn in `outer` by the `Do` of `id`.
    ///
    /// The form's matrix, then the placement of the `Do`, then the transformation of the content
    /// that draws it make the transformation of the form. Its graphics state starts as the one
    /// at the `Do` and is dropped at its end, whatever `q` and `Q` the form leaves unbalanced.
    /// The form is not clipped to its `/BBox`.
    fn render_form(
        &mut self,
        form: &'a FormXObject,
        id: &XObjectId,
        transform: &XObjectTransform,
        outer: &'a PdfResources,
        tag: &str,
        warnings: &mut Vec<PdfWarnMsg>,
    ) {
        if let Err(skip) = self.walk.enter(form) {
            if self.walk.first_report(skip) {
                warnings.push(PdfWarnMsg::warning(
                    0,
                    0,
                    format!("form {} is not drawn: {}", id.0, skip.describe()),
                ));
            }
            return;
        }

        let size = if transform.no_auto_scale {
            None
        } else {
            outer.xobjects.map.get(id).and_then(|x| x.get_width_height())
        };
        let matrix = form
            .matrix
            .map_or(CurTransMat::Identity.as_array(), |m| m.as_array());
        let to_page = CurTransMat::combine_matrix(matrix, transform.placement_matrix(size));

        match self.gst.enter_form(to_page) {
            None => {
                if !self.warned_about_depth {
                    self.warned_about_depth = true;
                    warnings.push(PdfWarnMsg::warning(
                        0,
                        0,
                        format!(
                            "graphics state nesting is deeper than {MAX_GRAPHICS_STATE_DEPTH}, \
                             the form {} is not drawn",
                            id.0
                        ),
                    ));
                }
            }
            Some(mark) => {
                // The fonts of a form with resources of its own are known by a name of its own.
                let form_tag = match &form.resources {
                    Some(local) => {
                        let form_tag = self.tag_of(form);
                        for (font_id, font) in local.fonts.map.iter() {
                            self.form_fonts.insert(qualify(&form_tag, font_id), font);
                        }
                        form_tag
                    }
                    None => tag.to_string(),
                };

                let group_base = self.current_svg_group.len();
                self.body.push_str(&format!(
                    "<g class=\"form\" data-name=\"{}\">",
                    escape_xml_attr(&id.0)
                ));
                self.current_svg_group.push(String::from("form"));
                let was_in_text = std::mem::replace(&mut self.in_text_section, false);

                self.render_ops(&form.ops, scope_of(form, outer), &form_tag, warnings);

                self.in_text_section = was_in_text;
                // Close what the form left open, and the form.
                while self.current_svg_group.len() > group_base {
                    self.current_svg_group.pop();
                    self.body.push_str("</g>");
                }
                self.gst.exit_form(mark);
            }
        }
        self.walk.leave();
    }

    /// The `<image>` for the image `id` of `scope`, or nothing when it is no image that can be
    /// shown. The images of the page are encoded already, the ones of forms are encoded here.
    fn render_image(
        &mut self,
        id: &XObjectId,
        transform: &XObjectTransform,
        scope: &PdfResources,
        tag: &str,
    ) -> String {
        let xobject = scope.xobjects.map.get(id);
        if !self.image_map.contains_key(id) {
            let key = (tag.to_string(), id.clone());
            if let (false, Some(XObject::Image(image))) =
                (self.form_images.contains_key(&key), xobject)
            {
                let encoded = image.encode_to_bytes(self.image_formats).ok();
                self.form_images.insert(key, encoded);
            }
        }
        let data = self.image_map.get(id).or_else(|| {
            self.form_images
                .get(&(tag.to_string(), id.clone()))
                .and_then(|encoded| encoded.as_ref())
        });
        render_image_to_svg(xobject, data, transform, self.height, &self.gst)
    }

    /// The SVG: the header, the font rules of the page and of the forms, what was written, the
    /// groups it left open and the glyph outlines.
    fn finish(mut self, head: String, mut font_faces: String) -> String {
        // Close the groups the page left open.
        for _group_type in self.current_svg_group.iter().rev() {
            self.body.push_str("</g>");
        }
        self.body.push_str(&self.glyph_outlines.to_svg());

        for (font_id, font) in self.form_fonts.iter() {
            if let Some(bytes) = font.parsed_font.source_bytes() {
                font_faces.push_str(&font_face_rule(
                    &svg_font_family(&font_id.0),
                    bytes.as_slice(),
                ));
            }
        }

        let mut svg = head;
        if !font_faces.is_empty() {
            svg.push_str("<style>\n");
            svg.push_str(&font_faces);
            svg.push_str("</style>\n");
        }
        svg.push_str(&self.body);
        svg.push_str("</svg>");
        svg
    }
}

fn color_to_svg(color: &Color) -> String {
    match color {
        Color::Rgb(rgb) => {
            let r = (rgb.r * 255.0).round() as u8;
            let g = (rgb.g * 255.0).round() as u8;
            let b = (rgb.b * 255.0).round() as u8;
            format!("rgb({}, {}, {})", r, g, b)
        }
        Color::Cmyk(cmyk) => {
            // Convert CMYK to RGB for SVG
            let r = (1.0 - cmyk.c) * (1.0 - cmyk.k);
            let g = (1.0 - cmyk.m) * (1.0 - cmyk.k);
            let b = (1.0 - cmyk.y) * (1.0 - cmyk.k);
            let r = (r * 255.0).round() as u8;
            let g = (g * 255.0).round() as u8;
            let b = (b * 255.0).round() as u8;
            format!("rgb({}, {}, {})", r, g, b)
        }
        Color::Greyscale(gs) => {
            let v = (gs.percent * 255.0).round() as u8;
            format!("rgb({}, {}, {})", v, v, v)
        }
        Color::SpotColor(spot) => {
            // Convert spot color to RGB for SVG
            let r = (1.0 - spot.c) * (1.0 - spot.k);
            let g = (1.0 - spot.m) * (1.0 - spot.k);
            let b = (1.0 - spot.y) * (1.0 - spot.k);
            let r = (r * 255.0).round() as u8;
            let g = (g * 255.0).round() as u8;
            let b = (b * 255.0).round() as u8;
            format!("rgb({}, {}, {})", r, g, b)
        }
    }
}

/// `text` for XML character data. The five markup characters become entities, and the characters
/// XML 1.0 does not allow (most control characters) become U+FFFD, so that a string from a PDF
/// cannot end the text it is written into or make the SVG unreadable.
fn escape_xml_text(text: &str) -> String {
    escape_xml(text, false)
}

/// `text` for the value of an XML attribute in either kind of quotes. Like [`escape_xml_text`],
/// and tabs and line breaks become character references, which a parser would turn into spaces.
fn escape_xml_attr(text: &str) -> String {
    escape_xml(text, true)
}

fn escape_xml(text: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' if attribute => out.push_str(&format!("&#{};", u32::from(c))),
            '\t' | '\n' | '\r' => out.push(c),
            c if u32::from(c) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => out.push('\u{FFFD}'),
            c => out.push(c),
        }
    }
    out
}

/// The `href` of a link to `uri`, escaped for an attribute, or `None` when a browser could run it
/// as script or it leaves the web (`javascript:`, `data:`, `vbscript:` and every other scheme).
///
/// Only `http`, `https`, `mailto` and `tel` are kept, and a reference without a scheme (a relative
/// link or a `#fragment`). Browsers ignore leading control characters and spaces and drop tabs and
/// line breaks inside a URL, so the scheme is read after the same clean-up.
fn safe_link_target(uri: &str) -> Option<String> {
    let cleaned: String = uri
        .trim_start_matches(|c: char| c.is_ascii_control() || c == ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    // A scheme is a letter followed by letters, digits, `+`, `-` and `.` (RFC 3986, 3.1).
    let scheme = cleaned.split_once(':').and_then(|(before, _)| {
        let mut chars = before.chars();
        let is_scheme = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        is_scheme.then(|| before.to_ascii_lowercase())
    });
    match scheme.as_deref() {
        None | Some("http" | "https" | "mailto" | "tel") => Some(escape_xml_attr(&cleaned)),
        Some(_) => None,
    }
}

/// The CSS family name for the external font `name`: ASCII letters and digits, `.`, `-`, `+` and
/// `_` stay, every other character becomes `_` and its code point in hexadecimal. The result is
/// safe in an XML attribute, in XML text and in a CSS string, and two different names stay
/// different.
fn svg_font_family(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_') {
            out.push(c);
        } else {
            out.push_str(&format!("_{:x}", u32::from(c)));
        }
    }
    out
}

/// `text` for an XML comment: letters, digits and the characters of PDF operators stay, every
/// other character becomes `_`, so the comment cannot contain `--` or end early.
fn comment_text(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '*' | '\'' | '"') { c } else { '_' })
        .collect()
}

/// SVG path data for `points` (PDF user space, y up) with the y axis
/// flipped to the page (`H - y`): `M` the first point, `L` a plain point, `C`
/// a cubic - two bezier handles and its end point, the way `c` / `v` / `y`
/// are parsed (a lone handle, which no operator produces, is a `Q`).
fn svg_path_data(points: &[LinePoint], page_height: f32) -> String {
    let at = |p: &LinePoint| format!("{},{}", p.p.x.0, page_height - p.p.y.0);
    let Some(first) = points.first() else {
        return String::new();
    };
    let mut d = format!("M{}", at(first));
    let mut i = 1;
    while let Some(point) = points.get(i) {
        match (point.bezier, points.get(i + 1), points.get(i + 2)) {
            (true, Some(c2), Some(end)) if c2.bezier => {
                d.push_str(&format!(" C{} {} {}", at(point), at(c2), at(end)));
                i += 3;
            }
            (true, Some(end), _) => {
                d.push_str(&format!(" Q{} {}", at(point), at(end)));
                i += 2;
            }
            _ => {
                d.push_str(&format!(" L{}", at(point)));
                i += 1;
            }
        }
    }
    d
}

fn get_svg_transform(
    ctm: &CurTransMat,
    text_matrix: &Option<TextMatrix>,
    page_height: f32,
) -> String {
    let mut transform = String::new();

    // The path points are already flipped to the page, u = (x, H - y), so the transform is
    // flip . CTM . flip, which rewritten in u is (a x + c y + e, H - (b x + d y + f)).
    let ctm_array = ctm.as_array();
    if *ctm != CurTransMat::Identity {
        let [a, b, c, d, e, f] = ctm_array;
        transform.push_str(&format!(
            "matrix({} {} {} {} {} {})",
            a,
            -b,
            -c,
            d,
            e + c * page_height,
            page_height - d * page_height - f
        ));
    }

    // Then apply text matrix if present
    if let Some(tm) = text_matrix {
        let tm_array = tm.as_array();
        if !transform.is_empty() {
            transform.push(' ');
        }
        transform.push_str(&format!(
            "matrix({} {} {} {} {} {})",
            tm_array[0],
            -tm_array[1],
            -tm_array[2],
            tm_array[3],
            tm_array[4],
            -tm_array[5] // No page_height adjustment needed here
        ));
    }

    transform
}

// Compose two affine transforms stored as PDF matrix arrays [a,b,c,d,e,f],
// each mapping (x,y) -> (a*x + c*y + e, b*x + d*y + f). Returns `outer` applied
// after `inner` (i.e. outer ∘ inner).
fn compose_matrix(outer: &[f32; 6], inner: &[f32; 6]) -> [f32; 6] {
    let [pa, pb, pc, pd, pe, pf] = *outer;
    let [qa, qb, qc, qd, qe, qf] = *inner;
    [
        pa * qa + pc * qb,
        pb * qa + pd * qb,
        pa * qc + pc * qd,
        pb * qc + pd * qd,
        pa * qe + pc * qf + pe,
        pb * qe + pd * qf + pf,
    ]
}

/// Glyph outlines that text runs paint with `<use>`: one `<path>` per (font,
/// glyph id), written once into a `<defs>` at the end of the page.
#[derive(Debug, Default)]
struct GlyphOutlineDefs {
    /// `<path>` id -> path data in font units (y up). Empty for a glyph with
    /// no outline (a space), which is cached so it is decoded only once.
    paths: BTreeMap<String, String>,
}

impl GlyphOutlineDefs {
    fn to_svg(&self) -> String {
        let mut defs = String::new();
        for (id, d) in self.paths.iter().filter(|(_, d)| !d.is_empty()) {
            defs.push_str(&format!("<path id=\"{}\" d=\"{}\"/>", id, d));
        }
        if defs.is_empty() {
            defs
        } else {
            format!("<defs>{}</defs>", defs)
        }
    }
}

/// Renders one `ShowText`.
///
/// Glyph ids from a shaper (`TextItem::GlyphIds`) are painted with the glyphs' own outlines, as
/// the PDF shows them. Writing them as Unicode text would let the browser shape them a second
/// time (isolated Arabic letters, broken ligatures). The `<text>` is still written, unpainted,
/// so the preview stays selectable and searchable.
///
/// Everything else (`TextItem::Text`, standard fonts, faces without outlines) is a painted
/// `<text>`.
fn render_show_text(
    items: &[TextItem],
    font: &BuiltinOrExternalFontId,
    program: Option<&crate::ParsedFont>,
    gst: &GraphicsStateVec,
    page_height: f32,
    outlines: &mut GlyphOutlineDefs,
) -> String {
    #[cfg(feature = "text_layout")]
    if let BuiltinOrExternalFontId::External(font_id) = font {
        if let Some(parsed_font) = program {
            if let Some(glyphs) = render_glyph_outlines(
                items,
                font_id,
                parsed_font,
                gst,
                page_height,
                outlines,
            ) {
                return glyphs
                    + &render_text_items_to_svg(
                        items,
                        font,
                        gst,
                        page_height,
                        TextLayer::Unpainted,
                    );
            }
        }
    }
    #[cfg(not(feature = "text_layout"))]
    let _ = (program, outlines); // no glyph outlines without azul's font parser

    render_text_items_to_svg(items, font, gst, page_height, TextLayer::Painted)
}

/// `id` of the outline `<path>` of `gid` in `font_id`: XML-safe, and the same on
/// every page of a document, so a viewer that inlines several pages' SVGs into
/// one HTML document resolves `<use>` to the same glyph whichever page's
/// `<defs>` it finds first.
#[cfg(feature = "text_layout")]
fn glyph_outline_id(font_id: &crate::FontId, gid: u16) -> String {
    let mut id = String::from("glyph-");
    for c in font_id.0.chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c);
        } else {
            id.push_str(&format!("_{:x}", c as u32));
        }
    }
    id.push_str(&format!("-{}", gid));
    id
}

/// SVG path data of a glyph's outline, in font units (y up). Empty for a glyph
/// without contours; `None` if the face cannot decode the glyph at all.
#[cfg(feature = "text_layout")]
fn glyph_outline_path(font: &crate::ParsedFont, gid: u16) -> Option<String> {
    use azul_core::resources::GlyphOutlineOperation as Seg;

    let glyph = font.get_or_decode_glyph(gid)?;
    let mut d = String::new();
    for contour in &glyph.outline {
        for seg in contour.operations.as_slice() {
            let s = match seg {
                Seg::MoveTo(p) => format!("M{} {}", p.x, p.y),
                Seg::LineTo(p) => format!("L{} {}", p.x, p.y),
                Seg::QuadraticCurveTo(q) => {
                    format!("Q{} {} {} {}", q.ctrl_1_x, q.ctrl_1_y, q.end_x, q.end_y)
                }
                Seg::CubicCurveTo(c) => format!(
                    "C{} {} {} {} {} {}",
                    c.ctrl_1_x, c.ctrl_1_y, c.ctrl_2_x, c.ctrl_2_y, c.end_x, c.end_y
                ),
                Seg::ClosePath => "Z".to_string(),
            };
            d.push_str(&s);
        }
    }
    Some(d)
}

/// Paints a run of glyph ids with their outlines, one `<use>` per glyph.
///
/// The pen advances through the run as a PDF viewer's does (ISO 32000-1
/// 9.4.4): the glyph's `/W` width as printpdf writes it (whole thousandths of
/// an em, which the merged TJ runs of a saved PDF are kerned against),
/// character spacing, and the TJ adjustment after it. Word spacing does not
/// apply: the codes are two bytes.
///
/// `None` if the run is not drawable this way: it has `TextItem::Text` (no
/// glyph ids), or none of its visible glyphs has an outline.
#[cfg(feature = "text_layout")]
fn render_glyph_outlines(
    items: &[TextItem],
    font_id: &crate::FontId,
    font: &crate::ParsedFont,
    gst: &GraphicsStateVec,
    page_height: f32,
    outlines: &mut GlyphOutlineDefs,
) -> Option<String> {
    if items.iter().any(|i| matches!(i, TextItem::Text(_))) {
        return None;
    }
    let units_per_em = f32::from(font.pdf_font_metrics.units_per_em);
    if units_per_em <= 0.0 {
        return None;
    }
    let font_size = gst
        .get_font_size(&BuiltinOrExternalFontId::External(font_id.clone()))
        .0;
    let scale = font_size / units_per_em;
    let char_spacing = gst.get_character_spacing();
    let text_transform = text_space_to_svg(gst, page_height);
    let (fill, stroke) = text_paint(gst);
    // The stroke width is in text space; the `<use>` scales by `scale`.
    let stroke_width = if stroke == "none" {
        String::new()
    } else {
        format!(" stroke-width=\"{}\"", gst.get_stroke_width().0 / scale)
    };

    let mut uses = String::new();
    let mut painted = false;
    let mut unpaintable = false;
    let mut pen = 0.0f32;
    for item in items {
        match item {
            TextItem::Offset(o) => pen -= o / 1000.0 * font_size,
            TextItem::GlyphIds(glyphs) => {
                for g in glyphs {
                    let id = glyph_outline_id(font_id, g.gid);
                    let has_outline = !outlines
                        .paths
                        .entry(id.clone())
                        .or_insert_with(|| glyph_outline_path(font, g.gid).unwrap_or_default())
                        .is_empty();
                    if has_outline {
                        let m =
                            compose_matrix(&text_transform, &[scale, 0.0, 0.0, -scale, pen, 0.0]);
                        uses.push_str(&format!(
                            "<use xlink:href=\"#{}\" transform=\"matrix({} {} {} {} {} {})\"{}/>",
                            id, m[0], m[1], m[2], m[3], m[4], m[5], stroke_width
                        ));
                        painted = true;
                    } else if g.cid.as_deref().is_some_and(|t| !t.trim().is_empty()) {
                        unpaintable = true;
                    }
                    let width = crate::serialize::get_scaled_glyph_width(font, g.gid) as f32;
                    pen += (width - g.offset) / 1000.0 * font_size + char_spacing;
                }
            }
            TextItem::Text(_) => return None,
        }
    }
    // A face that decodes no outlines at all (no glyf/CFF data): leave the run
    // to the browser rather than paint nothing.
    if unpaintable && !painted {
        return None;
    }
    Some(format!(
        "<g fill=\"{}\" stroke=\"{}\">{}</g>",
        fill, stroke, uses
    ))
}

// Builds the SVG `transform` matrix that maps PDF text space (y-up, origin at
// bottom-left) to SVG user space (y-down, origin at top-left) while keeping the
// glyphs upright (not mirrored).
//
// PDF maps a text-space point through M = CTM ∘ Tm to a device point, which we
// then flip with (x, y) -> (x, H - y). SVG glyph outlines are already y-down,
// so the required transform is Flip ∘ M ∘ Flip0 (Flip0 = diag(1,-1)), which
// works out to [a, -b, -c, d, e, H - f]. For the common translate-only case
// (a=d=1, b=c=0) this is a plain translate(e, H - f) with upright glyphs.
//
// Horizontal scaling (Tz) is folded into the x axis (text space, innermost), so
// it scales both the glyph shapes and the pen advances, as in PDF.
fn text_space_to_svg(gst: &GraphicsStateVec, page_height: f32) -> [f32; 6] {
    // The glyph position lives entirely in this matrix (translation part), so
    // the <text> x/y attributes stay at 0 to avoid double-applying the offset.
    //
    // Text is positioned two ways: azul's from_html sets a full text matrix (Tm),
    // while direct-API and parsed PDFs position with SetTextCursor / line breaks
    // (the `Td` operator), tracked in `text_cursor`. `text_matrix` alone misses
    // the latter, dropping every glyph to the page bottom (f = page_height). Use
    // the cursor's translation when it has been set, keeping the matrix's linear
    // (scale/rotate) part.
    let tm_arr = gst.get_text_matrix().as_array();
    let cursor = gst.get_text_cursor();
    let eff_tm = if cursor.x.0 != 0.0 || cursor.y.0 != 0.0 {
        [
            tm_arr[0], tm_arr[1], tm_arr[2], tm_arr[3], cursor.x.0, cursor.y.0,
        ]
    } else {
        tm_arr
    };
    let m = compose_matrix(&gst.get_transform_matrix().as_array(), &eff_tm);
    let h_scale = gst.get_horizontal_scaling() / 100.0; // percentage -> multiplier
    [
        m[0] * h_scale,
        -m[1] * h_scale,
        -m[2],
        m[3],
        m[4],
        page_height - m[5],
    ]
}

/// SVG `fill` and `stroke` for the current text rendering mode.
fn text_paint(gst: &GraphicsStateVec) -> (String, String) {
    match gst.get_text_rendering_mode() {
        TextRenderingMode::Fill => (color_to_svg(&gst.get_fill_color()), "none".to_string()),
        TextRenderingMode::Stroke => ("none".to_string(), color_to_svg(&gst.get_stroke_color())),
        TextRenderingMode::FillStroke => (
            color_to_svg(&gst.get_fill_color()),
            color_to_svg(&gst.get_stroke_color()),
        ),
        TextRenderingMode::Invisible => ("none".to_string(), "none".to_string()),
        TextRenderingMode::FillClip => (color_to_svg(&gst.get_fill_color()), "none".to_string()),
        TextRenderingMode::StrokeClip => {
            ("none".to_string(), color_to_svg(&gst.get_stroke_color()))
        }
        TextRenderingMode::FillStrokeClip => (
            color_to_svg(&gst.get_fill_color()),
            color_to_svg(&gst.get_stroke_color()),
        ),
        TextRenderingMode::Clip => ("none".to_string(), "none".to_string()),
    }
}

/// Whether a `<text>` element is what the viewer sees, or only the layer that
/// makes glyphs drawn some other way selectable and searchable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextLayer {
    Painted,
    Unpainted,
}

fn render_text_items_to_svg(
    items: &[TextItem],
    font_id: &BuiltinOrExternalFontId,
    gst: &GraphicsStateVec,
    page_height: f32,
    layer: TextLayer,
) -> String {
    let mut result = String::new();

    // Get font size
    let font_size = gst.get_font_size(font_id);

    // Get font family name
    let font_family = match font_id {
        BuiltinOrExternalFontId::Builtin(builtin_font) => {
            builtin_font.get_svg_font_family().to_string()
        }
        // The id of an external font is its family name (see the `@font-face` rule).
        BuiltinOrExternalFontId::External(font_id) => svg_font_family(&font_id.0),
    };

    let text_transform = text_space_to_svg(gst, page_height);
    let (x, y) = (0.0f32, 0.0f32);

    // Get text rendering mode styling
    let (fill, stroke) = match layer {
        TextLayer::Painted => text_paint(gst),
        // Still a fill (not `none`), so the text keeps receiving pointer events
        // for selection; the `fill-opacity="0"` below hides it.
        TextLayer::Unpainted => (color_to_svg(&gst.get_fill_color()), "none".to_string()),
    };

    // Get other text style properties
    let stroke_width = gst.get_stroke_width().0;
    let font_weight = match font_id {
        BuiltinOrExternalFontId::Builtin(bf) => bf.get_font_weight(),
        _ => "normal",
    };
    let font_style = match font_id {
        BuiltinOrExternalFontId::Builtin(bf) => bf.get_font_style(),
        _ => "normal",
    };

    // Process text content with any offsets
    let mut processed_text = String::new();
    let mut x_offset = 0.0;

    for item in items {
        match item {
            TextItem::Text(text) => {
                // Escape text for XML
                let escaped = escape_xml_text(text);

                if x_offset != 0.0 {
                    // Use tspan with dx for positioning if we have an offset
                    processed_text
                        .push_str(&format!("<tspan dx=\"{}\">{}</tspan>", x_offset, escaped));
                    x_offset = 0.0;
                } else {
                    processed_text.push_str(&escaped);
                }
            }
            TextItem::GlyphIds(glyphs) => {
                // Convert glyph IDs to Unicode text using the CID mapping
                // Each Codepoint may carry cid: Option<String> with the Unicode character
                for cp in glyphs {
                    // A glyph with no Unicode attached must still occupy its slot:
                    // dropping it silently un-syncs the preview from the PDF (the
                    // extraction path shows U+FFFD for the same glyph — keep the
                    // two consistent).
                    let text = cp.cid.clone().unwrap_or_else(|| '\u{FFFD}'.to_string());
                    let escaped = escape_xml_text(&text);
                    if x_offset != 0.0 {
                        processed_text.push_str(&format!(
                            "<tspan dx=\"{}\">{}</tspan>", x_offset, escaped
                        ));
                        x_offset = 0.0;
                    } else {
                        processed_text.push_str(&escaped);
                    }
                    if cp.offset != 0.0 {
                        x_offset -= cp.offset * font_size.0 / 1000.0;
                    }
                }
            }
            TextItem::Offset(offset) => {
                // Convert from thousandths of an em to points
                // In PDF, positive offset means moving LEFT (decreasing X)
                x_offset -= *offset * font_size.0 / 1000.0;
            }
        }
    }

    // Get character spacing and word spacing
    let char_spacing = gst.get_character_spacing();
    let word_spacing = gst.get_word_spacing();

    // Transform combining CTM and text matrix (already y-flipped, upright, with
    // horizontal scaling folded in): a single valid `transform` attribute.
    let transform = format!(
        "matrix({} {} {} {} {} {})",
        text_transform[0],
        text_transform[1],
        text_transform[2],
        text_transform[3],
        text_transform[4],
        text_transform[5]
    );

    // Create the SVG text element
    result.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" \
         font-style=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"",
        x, y, font_family, font_size.0, font_weight, font_style, fill, stroke, stroke_width
    ));
    if layer == TextLayer::Unpainted {
        result.push_str(" fill-opacity=\"0\"");
    }

    // Add optional attributes
    if !transform.is_empty() {
        result.push_str(&format!(" transform=\"{}\"", transform));
    }

    if char_spacing != 0.0 {
        result.push_str(&format!(" letter-spacing=\"{}\"", char_spacing));
    }

    if word_spacing != 0.0 {
        result.push_str(&format!(" word-spacing=\"{}\"", word_spacing));
    }

    // Close tag opening and add content
    result.push_str(&format!(">{}</text>", processed_text));

    result
}

fn render_line_to_svg(line: &Line, gst: &GraphicsStateVec, page_height: f32) -> String {
    if line.points.is_empty() {
        return String::new();
    }

    // Generate SVG path data
    let mut path_data = svg_path_data(&line.points, page_height);

    if line.is_closed {
        path_data.push_str(" Z"); // Close the path
    }

    // Get styling from graphics state
    let stroke = color_to_svg(&gst.get_stroke_color());
    let stroke_width = gst.get_stroke_width().0;
    let line_join = gst.get_line_join().to_svg_string();
    let line_cap = gst.get_line_cap().get_svg_id();

    // Handle dash pattern if present
    let dash_array = match gst.get_dash_array() {
        Some(dash) => {
            // Dispatch SmallVec storage only once, by taking slice.  
            let dash_pattern = dash.pattern.as_slice(); 
            if dash_pattern.is_empty() {
                String::new()
            } else {
                format!(
                    " stroke-dasharray=\"{}\" stroke-dashoffset=\"{}\"",
                    dash_pattern
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                    dash.offset
                )
            }
        }
        None => String::new(),
    };

    // Get transformation if present
    let transform = get_svg_transform(
        &gst.get_transform_matrix(),
        &None, // Line doesn't use text matrix
        page_height,
    );

    let transform_attr = if !transform.is_empty() {
        format!(" transform=\"{}\"", transform)
    } else {
        String::new()
    };

    // Create the SVG path element
    format!(
        "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linejoin=\"{}\" \
         stroke-linecap=\"{}\"{}{} />",
        path_data, stroke, stroke_width, line_join, line_cap, dash_array, transform_attr
    )
}

fn render_polygon_to_svg(polygon: &Polygon, gst: &GraphicsStateVec, page_height: f32) -> String {
    if polygon.rings.is_empty() {
        return String::new();
    }

    // Generate SVG path data
    let mut path_data = String::new();

    for ring in &polygon.rings {
        if ring.points.is_empty() {
            continue;
        }

        if !path_data.is_empty() {
            path_data.push(' ');
        }
        path_data.push_str(&svg_path_data(&ring.points, page_height));

        // Close the path for this ring
        path_data.push_str(" Z");
    }

    // Get styling based on PaintMode
    let (fill, stroke) = match polygon.mode {
        PaintMode::Fill => (color_to_svg(&gst.get_fill_color()), "none".to_string()),
        PaintMode::Stroke => ("none".to_string(), color_to_svg(&gst.get_stroke_color())),
        PaintMode::FillStroke => (
            color_to_svg(&gst.get_fill_color()),
            color_to_svg(&gst.get_stroke_color()),
        ),
        PaintMode::Clip => ("none".to_string(), "none".to_string()), /* Clip is handled
                                                                      * differently in SVG */
    };

    let stroke_width = gst.get_stroke_width().0;
    let line_join = gst.get_line_join().to_svg_string();
    let line_cap = gst.get_line_cap().get_svg_id();

    // Convert PDF winding order to SVG fill-rule
    let fill_rule = match polygon.winding_order {
        WindingOrder::NonZero => "nonzero",
        WindingOrder::EvenOdd => "evenodd",
    };

    // Handle dash pattern if present
    let dash_array = match gst.get_dash_array() {
        Some(dash) => {
            // Dispatch SmallVec storage only once, by taking slice.
            let dash_pattern = dash.pattern.as_slice(); 
            if dash_pattern.is_empty() {
                String::new()
            } else {
                format!(
                    " stroke-dasharray=\"{}\" stroke-dashoffset=\"{}\"",
                    dash_pattern
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                    dash.offset
                )
            }
        }
        None => String::new(),
    };

    // Get transformation if present
    let transform = get_svg_transform(
        &gst.get_transform_matrix(),
        &None, // Polygon doesn't use text matrix
        page_height,
    );

    let transform_attr = if !transform.is_empty() {
        format!(" transform=\"{}\"", transform)
    } else {
        String::new()
    };

    // Create the SVG path element
    format!(
        "<path d=\"{}\" fill=\"{}\" fill-rule=\"{}\" stroke=\"{}\" stroke-width=\"{}\" \
         stroke-linejoin=\"{}\" stroke-linecap=\"{}\"{}{} />",
        path_data,
        fill,
        fill_rule,
        stroke,
        stroke_width,
        line_join,
        line_cap,
        dash_array,
        transform_attr
    )
}

fn render_image_to_svg(
    xobject: Option<&XObject>,
    image_data: Option<&EncodedImage>,
    transform: &XObjectTransform,
    page_height: f32,
    gst: &GraphicsStateVec,
) -> String {
    if let (Some(xobject), Some((bytes, fmt))) = (xobject, image_data) {
        if let Some((width, height)) = xobject.get_width_height() {
            let dpi = transform.dpi.unwrap_or(300.0);
            let w_pt = width.into_pt(dpi).0;
            let h_pt = height.into_pt(dpi).0;

            let base64_str = base64::prelude::BASE64_STANDARD.encode(bytes);
            let data_url = format!("data:{};base64,{}", fmt.mime_type(), base64_str);

            let mut transforms = Vec::new();

            if let Some(scale_x) = transform.scale_x {
                let scale_y = transform.scale_y.unwrap_or(scale_x);
                transforms.push(format!("scale({}, {})", scale_x, scale_y));
            }

            if let Some(rotate) = &transform.rotate {
                transforms.push(format!(
                    "rotate({}, {}, {})",
                    rotate.angle_ccw_degrees,
                    rotate.rotation_center_x.into_pt(dpi).0,
                    page_height - rotate.rotation_center_y.into_pt(dpi).0
                ));
            }

            let tx = transform.translate_x.unwrap_or(Pt(0.0));
            let ty = transform.translate_y.unwrap_or(Pt(0.0));
            transforms.push(format!("translate({}, {})", tx.0, page_height - ty.0));

            let ctm = gst.get_transform_matrix();
            if ctm != CurTransMat::Identity {
                transforms.push(ctm.as_css_val());
            }

            let transform_attr = if !transforms.is_empty() {
                format!(" transform=\"{}\"", transforms.join(" "))
            } else {
                String::new()
            };

            return format!(
                "<image width=\"{}\" height=\"{}\" href=\"{}\"{} />",
                w_pt, h_pt, data_url, transform_attr
            );
        }
    }

    String::new()
}
