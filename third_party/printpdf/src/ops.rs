//! The content of a page: [`PdfPage`] and its list of [`Op`]s.
//!
//! The visible part of a PDF page is a *content stream*: a sequence of operators that each
//! take a few operands, for example `12 TL` (set the line height) or `0 0 100 50 re` (add a
//! rectangle). Reading it top to bottom changes a *graphics state* (colors, line width, font,
//! transformation matrix) and paints text, paths and images with it.
//!
//! [`Op`] has one variant per operator, so a page is a `Vec<Op>`:
//!
//! ```
//! use printpdf::*;
//!
//! let ops = vec![
//!     Op::SaveGraphicsState,                                   // q
//!     Op::SetOutlineThickness { pt: Pt(2.0) },                 // w
//!     Op::SetOutlineColor { col: Color::Rgb(Rgb::new(1.0, 0.0, 0.0, None)) }, // RG
//!     Op::DrawLine {                                           // m, l, S
//!         line: Line {
//!             points: vec![
//!                 LinePoint { p: Point::new(Mm(10.0), Mm(10.0)), bezier: false },
//!                 LinePoint { p: Point::new(Mm(100.0), Mm(50.0)), bezier: false },
//!             ],
//!             is_closed: false,
//!         },
//!     },
//!     Op::RestoreGraphicsState,                                // Q
//! ];
//! let page = PdfPage::new(Mm(210.0), Mm(297.0), ops);
//! assert_eq!(page.ops.len(), 5);
//! ```
//!
//! The variants fall into these groups:
//!
//! - **Graphics state**: `SaveGraphicsState`, `RestoreGraphicsState`, `LoadGraphicsState`,
//!   `SetTransformationMatrix`, `SetRenderingIntent`.
//! - **Text**: `StartTextSection` and `EndTextSection` enclose `SetFont`, `SetTextCursor`,
//!   `ShowText` and the spacing operators. Text shown outside such a section is not displayed
//!   by most viewers.
//! - **Paths**: `DrawLine`, `DrawRectangle`, `DrawPolygon`, with the stroke settings
//!   `SetOutlineThickness`, `SetLineDashPattern`, `SetLineJoinStyle`, `SetLineCapStyle`,
//!   `SetMiterLimit`, and `PaintShading` for gradients.
//! - **Color**: `SetFillColor`, `SetOutlineColor`, `SetColorSpaceFill`, `SetColorSpaceStroke`.
//! - **XObjects and images**: `UseXobject` places a registered image or drawing, the
//!   `*InlineImage` variants delimit an image embedded in the stream.
//! - **Marked content and layers**: `BeginLayer`/`EndLayer`, `BeginMarkedContent` and
//!   friends tag a range of operators, `Marker` marks a point.
//! - **Other**: `LinkAnnotation` (a clickable area), `Unknown` (any other operator).
//!
//! The types serialize to JSON as `{"type": "<variant in kebab-case>", "data": {...}}`.

use serde_derive::{Deserialize, Serialize};

pub use crate::text::TextItem;
use crate::{
    color::Color,
    font::ParsedFont,
    form_walk::{form_in, scope_of, FormWalk},
    xobject::FormXObject,
    graphics::{
        Line, LineCapStyle, LineDashPattern, LineJoinStyle, Point, Polygon, Rect, TextRenderingMode,
    },
    matrix::{CurTransMat, TextMatrix},
    units::{Mm, Pt},
    BuiltinFont, DictItem, ExtendedGraphicsStateId, ShadingId, FontId, LayerInternalId, LinkAnnotation,
    PdfResources, PdfToSvgOptions, PdfWarnMsg, RenderingIntent, XObjectId, XObjectTransform,
};

/// The font that `Op::SetFont` selects: one of the standard 14 fonts or an embedded font.
///
/// Serialized to JSON as `{"type": "builtin" | "external", "data": ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum PdfFontHandle {
    /// One of the 14 standard PDF fonts. It is not embedded in the file.
    Builtin(BuiltinFont),
    /// A font registered with [`PdfDocument::add_font`](crate::PdfDocument::add_font).
    External(FontId),
}

impl PdfFontHandle {
    /// Returns the name under which the font is listed in the page resources (`F1` to `F14` for
    /// builtin fonts, the [`FontId`] string otherwise).
    pub fn get_resource_name(&self) -> String {
        match self {
            PdfFontHandle::Builtin(bf) => bf.get_pdf_id().to_string(),
            PdfFontHandle::External(fid) => fid.0.clone(),
        }
    }
}

/// One page: its size and the operations that draw it.
///
/// The origin of the page is its bottom left corner and y grows upwards (see the crate
/// documentation). Serialized as camelCase JSON.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfPage {
    /// The full page area, in points (`/MediaBox`).
    pub media_box: Rect,
    /// The area of the page after trimming in print production (`/TrimBox`).
    pub trim_box: Rect,
    /// The area a viewer shows (`/CropBox`).
    pub crop_box: Rect,
    /// The content stream, executed from first to last.
    pub ops: Vec<Op>,
}

impl PdfPage {
    /// Creates a page of `width` x `height` with the given operations.
    ///
    /// Media box, trim box and crop box all start at the bottom left corner `(0, 0)` and cover
    /// the whole page.
    pub fn new(width: Mm, height: Mm, ops: Vec<Op>) -> Self {
        Self {
            media_box: Rect::from_wh(width.into(), height.into()),
            trim_box: Rect::from_wh(width.into(), height.into()),
            crop_box: Rect::from_wh(width.into(), height.into()),
            ops,
        }
    }

    pub(crate) fn get_media_box(&self) -> lopdf::Object {
        self.media_box.to_array().into()
    }

    pub(crate) fn get_trim_box(&self) -> lopdf::Object {
        self.trim_box.to_array().into()
    }

    pub(crate) fn get_crop_box(&self) -> lopdf::Object {
        self.crop_box.to_array().into()
    }

    /// Renders the page to an SVG string. `resources` must hold the fonts and XObjects the page
    /// uses, normally `&document.resources`.
    pub fn to_svg(
        &self,
        resources: &PdfResources,
        opts: &PdfToSvgOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> String {
        crate::render::render_to_svg(self, resources, opts, warnings)
    }

    /// Like [`PdfPage::to_svg`], with the async image encoder.
    pub async fn to_svg_async(
        &self,
        resources: &PdfResources,
        opts: &PdfToSvgOptions,
        warnings: &mut Vec<PdfWarnMsg>,
    ) -> String {
        crate::render::render_to_svg_async(self, resources, opts, warnings).await
    }

    /// Ids of the XObjects the page places with `Op::UseXobject`, in order of use. An id that is
    /// used twice appears twice.
    pub fn get_xobject_ids(&self) -> Vec<XObjectId> {
        self.ops
            .iter()
            .filter_map(|s| match s {
                Op::UseXobject { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    /// Ids of the embedded fonts that `Op::SetFont` selects on this page, without duplicates, in
    /// order of first use.
    pub fn get_external_font_ids(&self) -> Vec<FontId> {
        let mut ids = Vec::new();
        for op in &self.ops {
            if let Op::SetFont {
                font: PdfFontHandle::External(id),
                ..
            } = op
            {
                if !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
        }
        ids
    }

    /// Ids of the layers that `Op::BeginLayer` opens on this page, in order of use.
    pub fn get_layers(&self) -> Vec<LayerInternalId> {
        self.ops
            .iter()
            .filter_map(|s| match s {
                Op::BeginLayer { layer_id } => Some(layer_id.clone()),
                _ => None,
            })
            .collect()
    }

    /// Extracts the text of the page: one string per text section (`BT` to `ET`), with `\r\n`
    /// between lines. Empty sections are left out.
    ///
    /// [`TextItem::GlyphIds`] are turned back into characters with the CID stored with the
    /// glyph, or else with the cmap of the current font from `resources`; unknown glyphs
    /// become U+FFFD. Lines are detected from the vertical text position only, so columns and
    /// tables are not reconstructed.
    ///
    /// The text of the forms the page draws ([`Op::UseXobject`] of an
    /// [`XObject::Form`](crate::XObject::Form)) is part of the page, in the order it is drawn; a
    /// form looks up its fonts in its own resources. A form that draws itself, forms nested
    /// deeper than 16 levels, and more than 10 000 forms or 2 000 000 ops of forms on one page
    /// are skipped.
    pub fn extract_text(&self, resources: &PdfResources) -> Vec<String> {
        let mut extraction = TextExtraction::new();
        extraction.run(&self.ops, resources, &mut FormWalk::new());
        extraction.finish()
    }

    // Reverse lookup through the cmap; only the Basic Multilingual Plane is searched.
    fn glyph_id_to_char(font: &ParsedFont, gid: u16) -> Option<char> {
        for codepoint in 0..0x10000u32 {
            if let Some(mapped_gid) = font.lookup_glyph_index(codepoint) {
                if mapped_gid == gid {
                    return char::from_u32(codepoint);
                }
            }
        }
        None
    }
}

/// The text of a page while its ops are read: what has been read, and where the pen is.
struct TextExtraction<'a> {
    text_chunks: Vec<String>,
    current_chunk: String,
    in_text_section: bool,
    cur_text_cursor: Point,
    cur_text_matrix_y: Option<f32>,
    current_font: Option<&'a ParsedFont>,
    cur_font_size: f32,
    /// Where the run shown last *ended* horizontally (text space). Tells an intra-word
    /// positioning move from a real word gap when a document places every run with its own Tm.
    pen_x: Option<f32>,
}

impl<'a> TextExtraction<'a> {
    fn new() -> Self {
        TextExtraction {
            text_chunks: Vec::new(),
            current_chunk: String::new(),
            in_text_section: false,
            cur_text_cursor: Point {
                x: Pt(0.0),
                y: Pt(0.0),
            },
            cur_text_matrix_y: None,
            current_font: None,
            cur_font_size: 12.0,
            pen_x: None,
        }
    }

    /// Reads `ops`, which name their fonts and XObjects in `scope`.
    fn run(&mut self, ops: &[Op], scope: &'a PdfResources, walk: &mut FormWalk) {
        for op in ops {
            match op {
                Op::StartTextSection => {
                    self.in_text_section = true;
                }
                Op::EndTextSection => {
                    self.in_text_section = false;
                    if !self.current_chunk.is_empty() {
                        self.text_chunks.push(self.current_chunk.trim().to_string());
                        self.current_chunk = String::new();
                    }
                }
                Op::SetFont { font, size } => {
                    self.cur_font_size = size.0;
                    match font {
                        crate::ops::PdfFontHandle::External(font_id) => {
                            self.current_font =
                                scope.fonts.map.get(font_id).map(|pf| &pf.parsed_font);
                        }
                        crate::ops::PdfFontHandle::Builtin(_) => {
                            // Builtin text is `TextItem::Text`; drop the previous
                            // external font so it is not used to decode glyph ids.
                            self.current_font = None;
                        }
                    }
                }
                Op::SetTextMatrix { matrix } => {
                    // Only a vertical move is a line break: documents that position
                    // every run with its own Tm (common with shaped text) would
                    // otherwise get one line per glyph run.
                    let m = matrix.as_array();
                    let (new_x, new_y) = (m[4], m[5]);
                    match self.cur_text_matrix_y {
                        Some(prev_y) if (prev_y - new_y).abs() > 0.5 => {
                            self.current_chunk.push_str("\r\n");
                        }
                        Some(_) => {
                            // Same baseline: a word gap is a move that lands visibly past
                            // where the previous run ended; per-glyph moves land exactly
                            // at the pen position.
                            let gap_threshold = (self.cur_font_size * 0.15).max(0.1);
                            let is_gap = match self.pen_x {
                                Some(px) => new_x - px > gap_threshold,
                                None => false,
                            };
                            if is_gap
                                && !self.current_chunk.is_empty()
                                && !self.current_chunk.ends_with(char::is_whitespace)
                            {
                                self.current_chunk.push(' ');
                            }
                        }
                        None => {}
                    }
                    self.cur_text_matrix_y = Some(new_y);
                    self.pen_x = Some(new_x);
                }
                Op::SetTextCursor { pos } => {
                    if (self.cur_text_cursor.y.0.abs() - pos.y.0.abs()).abs() > 3.0 {
                        self.current_chunk.push_str("\r\n");
                    }
                    self.cur_text_cursor = *pos;
                    self.pen_x = None;
                }
                Op::ShowText { items } if self.in_text_section => {
                    for item in items {
                        match item {
                            TextItem::Offset(o) => {
                                if *o < -100.0 {
                                    self.current_chunk.push(' ');
                                }
                                if let Some(px) = self.pen_x.as_mut() {
                                    *px -= o / 1000.0 * self.cur_font_size;
                                }
                            }
                            TextItem::Text(t) => {
                                self.current_chunk.push_str(t);
                                if let Some(px) = self.pen_x.as_mut() {
                                    let mut adv_em = 0.0f32;
                                    for c in t.chars() {
                                        adv_em += self
                                            .current_font
                                            .and_then(|f| {
                                                f.lookup_glyph_index(c as u32)
                                                    .map(|gid| glyph_advance_em(f, gid))
                                            })
                                            .unwrap_or(0.5);
                                    }
                                    *px += adv_em * self.cur_font_size;
                                }
                            }
                            TextItem::GlyphIds(glyphs) => {
                                // The Unicode attached at parse time (from the font's
                                // ToUnicode CMap) is preferred: it handles ligatures and
                                // works without the font resource.
                                for codepoint in glyphs {
                                    if let Some(cid) = codepoint.cid.as_ref() {
                                        self.current_chunk.push_str(cid);
                                    } else if let Some(c) = self.current_font.and_then(|font| {
                                        PdfPage::glyph_id_to_char(font, codepoint.gid)
                                    }) {
                                        self.current_chunk.push(c);
                                    } else {
                                        self.current_chunk.push('\u{FFFD}');
                                    }
                                    if let Some(px) = self.pen_x.as_mut() {
                                        let adv_em = self
                                            .current_font
                                            .map(|f| glyph_advance_em(f, codepoint.gid))
                                            .unwrap_or(0.5);
                                        *px += adv_em * self.cur_font_size
                                            - codepoint.offset / 1000.0 * self.cur_font_size;
                                    }
                                }
                            }
                        }
                    }
                }
                Op::AddLineBreak if self.in_text_section => {
                    self.current_chunk.push_str("\r\n");
                    self.pen_x = None;
                }
                Op::MoveTextCursorAndSetLeading { .. } if self.in_text_section => {
                    self.current_chunk.push_str("\r\n");
                    self.pen_x = None;
                }
                Op::MoveToNextLineShowText { text } if self.in_text_section => {
                    self.current_chunk.push_str("\r\n");
                    self.current_chunk.push_str(text);
                    self.current_chunk.push(' ');
                }
                Op::SetSpacingMoveAndShowText { text, .. } if self.in_text_section => {
                    self.current_chunk.push_str("\r\n");
                    self.current_chunk.push_str(text);
                    self.current_chunk.push(' ');
                }
                Op::UseXobject { id, .. } => {
                    if let Some(form) = form_in(scope, id) {
                        if walk.enter(form).is_ok() {
                            self.run_form(form, scope, walk);
                            walk.leave();
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Reads the ops of `form`, drawn in `outer`. The text state is the form's own and is
    /// restored when it ends, as the graphics state is for a `Do`.
    fn run_form(&mut self, form: &'a FormXObject, outer: &'a PdfResources, walk: &mut FormWalk) {
        let saved = (
            self.in_text_section,
            self.cur_text_cursor,
            self.cur_text_matrix_y,
            self.current_font,
            self.cur_font_size,
            self.pen_x,
        );
        self.in_text_section = false;
        self.run(&form.ops, scope_of(form, outer), walk);
        (
            self.in_text_section,
            self.cur_text_cursor,
            self.cur_text_matrix_y,
            self.current_font,
            self.cur_font_size,
            self.pen_x,
        ) = saved;
    }

    /// The text of the sections that were read, without the empty ones.
    fn finish(mut self) -> Vec<String> {
        if !self.current_chunk.is_empty() {
            self.text_chunks.push(self.current_chunk.trim().to_string());
        }

        self.text_chunks.retain(|chunk| !chunk.is_empty());
        self.text_chunks
    }
}

// Horizontal advance of one glyph in em (relative to the font size), to tell per-glyph
// positioning moves from word gaps in text extraction. 0.5 em when the metric is missing.
#[cfg(feature = "text_layout")]
pub(crate) fn glyph_advance_em(font: &ParsedFont, gid: u16) -> f32 {
    let upm = font.font_metrics.units_per_em as f32;
    if upm <= 0.0 {
        return 0.5;
    }
    let adv = font.get_horizontal_advance(gid) as f32;
    if adv <= 0.0 {
        0.5
    } else {
        adv / upm
    }
}

// Same as above for the `ParsedFont` without `text_layout`, which stores `glyph_widths`
// in font units next to `units_per_em`.
#[cfg(not(feature = "text_layout"))]
pub(crate) fn glyph_advance_em(font: &ParsedFont, gid: u16) -> f32 {
    let upm = font.units_per_em as f32;
    if upm <= 0.0 {
        return 0.5;
    }
    match font.get_glyph_width(gid) {
        Some(w) if w > 0 => w as f32 / upm,
        _ => 0.5,
    }
}

/// What a layer is meant for (`/Intent` of an optional content group).
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerIntent {
    /// For viewing: the layer is shown and hidden by viewers.
    View,
    /// For design: the layer is only meaningful to the authoring application.
    Design,
}

impl LayerIntent {
    /// The PDF name of the intent.
    pub fn to_string(&self) -> &'static str {
        match self {
            LayerIntent::View => "View",
            LayerIntent::Design => "Design",
        }
    }
}

/// The kind of content in a layer, written as `/Subtype` of its creator info.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerSubtype {
    /// Artwork such as drawings and text.
    Artwork,
}

impl LayerSubtype {
    /// The PDF name of the subtype.
    pub fn to_string(&self) -> &'static str {
        match self {
            LayerSubtype::Artwork => "Artwork",
        }
    }
}

/// A layer (optional content group) that a viewer can show or hide.
///
/// Register it with [`PdfDocument::add_layer`](crate::PdfDocument::add_layer) and wrap the
/// content between `Op::BeginLayer` and `Op::EndLayer`.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    /// The name shown in the layers panel of a viewer.
    pub name: String,
    /// The application that created the layer (`/CreatorInfo`).
    pub creator: String,
    /// What the layer is for. Not written to the file: every layer is saved with the intent
    /// `[View Design]`.
    pub intent: LayerIntent,
    /// The kind of content in the layer.
    pub usage: LayerSubtype,
}

impl Layer {
    /// Creates a layer called `name` with the creator "Adobe Illustrator 14.0", intent
    /// [`LayerIntent::Design`] and subtype [`LayerSubtype::Artwork`].
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            creator: "Adobe Illustrator 14.0".to_string(),
            intent: LayerIntent::Design,
            usage: LayerSubtype::Artwork,
        }
    }
}

/// One operation of a page's content stream, mostly one PDF operator with its operands.
///
/// The group overview is in the [module documentation](self). The documentation of each
/// variant names the operator it is written as. Lengths are in points ([`Pt`]) and positions
/// are measured from the bottom left corner of the page unless noted otherwise.
///
/// Serialized to JSON as `{"type": "<variant name in kebab-case>", "data": {<fields>}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum Op {
    /// `MP`: marks a point in the stream with a tag. Draws nothing; useful for debugging.
    Marker {
        /// Tag of the marked point.
        id: String,
    },
    /// `CS`: selects the color space (a named resource) for stroking operations.
    SetColorSpaceStroke {
        /// Resource name of the color space.
        id: String,
    },
    /// `cs`: selects the color space (a named resource) for fill operations.
    SetColorSpaceFill {
        /// Resource name of the color space.
        id: String,
    },
    /// `BDC` with the `/OC` tag: starts content that belongs to a layer. End it with
    /// [`Op::EndLayer`].
    BeginLayer {
        /// A layer registered with [`PdfDocument::add_layer`](crate::PdfDocument::add_layer).
        layer_id: LayerInternalId,
    },
    /// `EMC`: ends the content of a layer.
    EndLayer,
    /// `q`: pushes the graphics state (colors, line style, matrix, font, ...) on a stack.
    SaveGraphicsState,
    /// `Q`: pops the graphics state pushed by the last [`Op::SaveGraphicsState`].
    RestoreGraphicsState,
    /// `gs`: applies an extended graphics state (blend mode, opacity, overprint, ...).
    LoadGraphicsState {
        /// A state registered with
        /// [`PdfDocument::add_graphics_state`](crate::PdfDocument::add_graphics_state).
        gs: ExtendedGraphicsStateId,
    },
    /// `BT`: starts a text object. Text operators belong between this and [`Op::EndTextSection`].
    StartTextSection,
    /// `ET`: ends a text object.
    EndTextSection,
    /// `Tf`: sets the font and size (in points) of the text that follows.
    SetFont {
        /// The font to use.
        font: PdfFontHandle,
        /// The font size.
        size: Pt,
    },
    /// `Tj` / `TJ`: shows text at the current text position. A font must be set first with
    /// [`Op::SetFont`].
    ShowText {
        /// Strings, glyph runs and spacing adjustments, shown one after another.
        items: Vec<TextItem>,
    },
    /// `T*`: moves to the start of the next line, one line height (see [`Op::SetLineHeight`])
    /// below the current one.
    AddLineBreak,
    /// `TL`: sets the line height ("leading") used by [`Op::AddLineBreak`].
    SetLineHeight {
        /// Distance between two baselines.
        lh: Pt,
    },
    /// `Tw`: sets the extra space added to every space character. The default is 0.
    SetWordSpacing {
        /// The extra space.
        pt: Pt,
    },
    /// `Td`: moves the text position by `pos` from the start of the current line. Right after
    /// [`Op::StartTextSection`] this is the position on the page, measured from the bottom left
    /// corner.
    SetTextCursor {
        /// The offset (the position on the page for the first move in a text section).
        pos: Point,
    },
    /// `g` / `rg` / `k`: sets the fill color of text and shapes (gray, RGB or CMYK, by the kind
    /// of [`Color`]).
    SetFillColor {
        /// The color; components are in the range 0.0 to 1.0.
        col: Color,
    },
    /// `G` / `RG` / `K`: sets the stroke color of outlines (gray, RGB or CMYK, by the kind of
    /// [`Color`]).
    SetOutlineColor {
        /// The color; components are in the range 0.0 to 1.0.
        col: Color,
    },
    /// `w`: sets the line width used for strokes.
    SetOutlineThickness {
        /// The line width.
        pt: Pt,
    },
    /// `d`: sets the dash pattern of strokes.
    SetLineDashPattern {
        /// Dash and gap lengths and the start offset.
        dash: LineDashPattern,
    },
    /// `j`: sets how the corners of a stroked path are joined (miter, round or bevel).
    SetLineJoinStyle {
        /// The join style.
        join: LineJoinStyle,
    },
    /// `J`: sets how the ends of a stroked line look (butt, round or projecting square).
    SetLineCapStyle {
        /// The cap style.
        cap: LineCapStyle,
    },
    /// `M`: sets the miter limit, which turns a sharp miter join into a bevel.
    SetMiterLimit {
        /// The limit, written as the plain number.
        limit: Pt,
    },
    /// `Tr`: sets whether text is filled, stroked, both, or used as a clipping path.
    SetTextRenderingMode {
        /// The rendering mode.
        mode: TextRenderingMode,
    },
    /// `Tc`: sets the extra space added after every glyph. The default is 0.
    SetCharacterSpacing {
        /// The extra space, written as the plain number (text space units).
        multiplier: f32,
    },
    /// `Ts`: sets the text rise, the offset of the baseline up (positive) or down. Used for
    /// superscript and subscript. The default is 0.
    SetLineOffset {
        /// The offset, written as the plain number (text space units).
        multiplier: f32,
    },
    /// `m`, `l`, `c`, `h`, `S`: strokes a line made of straight and bezier segments with the
    /// current outline settings. It is closed first when `is_closed` is set.
    DrawLine {
        /// The line.
        line: Line,
    },
    /// `re` followed by the paint operator of the rectangle's `mode` (`f`, `S`, `B` or `W n`
    /// for fill, stroke, fill and stroke, clip). Without a mode nothing is painted.
    DrawRectangle {
        /// The rectangle; its `mode` and `winding_order` decide how it is painted.
        rectangle: Rect,
    },
    /// `m`, `l`, `c` for every ring, `h`, then the paint operator of the polygon's `mode`
    /// (fill, stroke, close and fill and stroke, or clip).
    DrawPolygon {
        /// The polygon.
        polygon: Polygon,
    },
    /// `cm`: multiplies the current transformation matrix, so it moves, rotates or scales
    /// everything drawn afterwards. Wrap it in [`Op::SaveGraphicsState`] and
    /// [`Op::RestoreGraphicsState`] to limit its effect.
    SetTransformationMatrix {
        /// The matrix to apply.
        matrix: CurTransMat,
    },
    /// `Tm`: replaces the text matrix, which positions, rotates and scales the text of the
    /// current text section.
    SetTextMatrix {
        /// The new text matrix.
        matrix: TextMatrix,
    },
    /// A clickable area. It is not part of the content stream: when saving, it is added to the
    /// `/Annots` of the page.
    LinkAnnotation {
        /// The area and what happens on a click.
        link: LinkAnnotation,
    },
    /// `q`, `cm`, `Do`, `Q`: draws an XObject (an image or a drawing) with the given placement.
    UseXobject {
        /// An XObject registered with [`PdfDocument::add_image`](crate::PdfDocument::add_image)
        /// or [`PdfDocument::add_xobject`](crate::PdfDocument::add_xobject).
        id: XObjectId,
        /// Position, rotation and scale of the XObject. Images are first scaled to their pixel
        /// size at the transform's DPI.
        transform: XObjectTransform,
    },
    /// `TD`: moves the text position by `(tx, ty)` and sets the line height to `-ty`.
    MoveTextCursorAndSetLeading {
        /// Horizontal move, in text space units.
        tx: f32,
        /// Vertical move, in text space units.
        ty: f32,
    },
    /// `ri`: sets the rendering intent, which says how colors are converted when the output
    /// device cannot show them exactly.
    SetRenderingIntent {
        /// The intent.
        intent: RenderingIntent,
    },
    /// `Tz`: sets the horizontal scaling of glyphs. 100 is normal width.
    SetHorizontalScaling {
        /// The scale in percent.
        percent: f32,
    },
    /// `BI`: begins an inline image, an image stored inside the content stream.
    BeginInlineImage,
    /// `ID`: begins the data of an inline image.
    BeginInlineImageData,
    /// `EI`: ends an inline image.
    EndInlineImage,
    /// `BMC`: begins a marked-content sequence with a tag.
    BeginMarkedContent {
        /// The tag naming the kind of content.
        tag: String,
    },
    /// `BDC`: begins a marked-content sequence with a tag and a property dictionary.
    BeginMarkedContentWithProperties {
        /// The tag naming the kind of content.
        tag: String,
        /// The property dictionary.
        properties: DictItem,
    },
    /// `BDC` with the `/OC` tag: begins optional content, which a viewer can show or hide.
    BeginOptionalContent {
        /// The layer the content belongs to.
        layer_id: LayerInternalId,
    },
    /// `DP`: defines a marked-content point with properties.
    DefineMarkedContentPoint {
        /// The tag naming the point.
        tag: String,
        /// The properties of the point.
        properties: Vec<DictItem>,
    },
    /// `EMC`: ends the current marked-content sequence.
    EndMarkedContent,
    /// `EMC`: ends a marked-content sequence started with properties.
    EndMarkedContentWithProperties,
    /// `EMC`: ends optional content.
    EndOptionalContent,
    /// `BX`: begins a compatibility section, in which unknown operators are ignored.
    BeginCompatibilitySection,
    /// `EX`: ends a compatibility section.
    EndCompatibilitySection,
    /// `'`: moves to the start of the next line and shows text.
    MoveToNextLineShowText {
        /// The text, encoded for the current font.
        text: String,
    },
    /// `"`: sets word and character spacing, moves to the start of the next line and shows
    /// text.
    SetSpacingMoveAndShowText {
        /// Word spacing (like `Tw`).
        word_spacing: f32,
        /// Character spacing (like `Tc`).
        char_spacing: f32,
        /// The text, encoded for the current font.
        text: String,
    },
    /// `sh`: paints a shading (a gradient) over the current clipping region.
    PaintShading {
        /// A shading registered with
        /// [`PdfDocument::add_shading`](crate::PdfDocument::add_shading).
        id: ShadingId,
    },
    /// Any other operator. It is written only when [`PdfSaveOptions::secure`](crate::PdfSaveOptions::secure)
    /// is `false`, and dropped otherwise.
    Unknown {
        /// The operator name.
        key: String,
        /// The operands.
        value: Vec<DictItem>,
    },
}
