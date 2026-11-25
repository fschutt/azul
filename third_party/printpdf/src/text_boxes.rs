//! Text extraction with positions.
//!
//! [`PdfPage::extract_text`] returns only the text of a page. [`PdfPage::extract_text_boxes`]
//! also says where every line, word and glyph sits, which is what a viewer needs for
//! hit-testing, selection rectangles and copy and paste: render the page with
//! [`PdfPage::to_svg`] and draw these boxes over it. The result follows the hOCR hierarchy
//! (`ocr_page`, `ocr_line`, `ocrx_word`) as serde JSON instead of HTML attributes.
//!
//! # Coordinate space
//!
//! Boxes are `[x0, y0, x1, y1]` (left, top, right, bottom) in points (1/72 inch), with the
//! origin at the **top left** of the page and y growing **downward**. This is the space of the
//! SVG from `to_svg`, so boxes can be overlaid on it without further math. PDF user space has
//! its origin at the bottom left; the flip `y = page_height - y_pdf` is applied here.
//!
//! A glyph box is as wide as the glyph's advance and as tall as the font's ascent plus
//! descent at the current size. For rotated or skewed text it is the axis-aligned hull of the
//! transformed glyph.
//!
//! # How glyphs are placed
//!
//! Advances come from the font program (the same source the renderer uses). The pen moves by
//! the rule of ISO 32000-1, 9.4.4:
//!
//! ```text
//! tx = ((w_glyph/1000)·size + Tc + (code==space ? Tw : 0)) · Tz/100
//! ```
//!
//! Character spacing (`Tc`), word spacing (`Tw`), horizontal scaling (`Tz`), text rise (`Ts`),
//! the text matrix, the current transformation matrix with save and restore, and the leading
//! are all taken into account. Words end at a space or at a large gap in a `TJ` array; lines
//! end at a text section end, a line break or a move of the text cursor.
//!
//! # Examples
//!
//! ```
//! use printpdf::*;
//!
//! let ops = vec![
//!     Op::StartTextSection,
//!     Op::SetTextCursor { pos: Point { x: Pt(72.0), y: Pt(700.0) } },
//!     Op::SetFont { font: PdfFontHandle::Builtin(BuiltinFont::Helvetica), size: Pt(12.0) },
//!     Op::ShowText { items: vec![TextItem::Text("Hello world".to_string())] },
//!     Op::EndTextSection,
//! ];
//! let mut doc = PdfDocument::new("boxes");
//! doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), ops));
//!
//! let pages = doc.extract_text_boxes();
//! let line = &pages[0].lines[0];
//! assert_eq!(line.words[0].text, "Hello");
//! let [x0, _y0, x1, _y1] = line.words[0].bbox;
//! assert!(x1 > x0);
//! ```

use std::collections::BTreeMap;

use serde_derive::{Deserialize, Serialize};

use crate::{
    form_walk::{form_in, scope_of, FormWalk},
    matrix::CurTransMat,
    ops::{Op, PdfFontHandle},
    text::TextItem,
    xobject::FormXObject,
    BuiltinFont, Color, FontId, ParsedFont, PdfPage, PdfResources, TextRenderingMode,
    XObjectTransform,
};

/// A box `[x0, y0, x1, y1]` in pt: left, top, right, bottom, with the origin at the top left
/// of the page (hOCR `bbox` order).
pub type BBox = [f32; 4];

fn bbox_union(a: BBox, b: BBox) -> BBox {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// All text geometry of one page (hOCR `ocr_page`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageTextBoxes {
    /// Page width in pt.
    pub width: f32,
    /// Page height in pt.
    pub height: f32,
    /// Text lines in content-stream order.
    pub lines: Vec<TextLine>,
}

/// One baseline run of text (hOCR `ocr_line`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextLine {
    /// Hull of all word boxes, `[x0, y0, x1, y1]` pt, top-left origin.
    pub bbox: BBox,
    /// The line's baseline y (top-left-origin pt) — where the glyphs sit;
    /// `bbox` extends above (ascent) and below (descent) of this.
    pub baseline: f32,
    /// The words of the line, in content-stream order.
    pub words: Vec<TextWord>,
}

/// One whitespace-delimited word (hOCR `ocrx_word`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextWord {
    /// Hull of the word's glyph boxes.
    pub bbox: BBox,
    /// The decoded text (from ToUnicode / the font's cmap).
    pub text: String,
    /// Font size in pt.
    pub font_size: f32,
    /// The font resource this word was shown with, if an external font.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<FontId>,
    /// The standard-14 font this word was shown with (`font` is `None` then):
    /// a viewer draws it with a metric-compatible face of that name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin: Option<BuiltinFont>,
    /// The fill colour in effect when the word was shown (`None` = the
    /// initial black).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// Shown with a text rendering mode that neither fills nor strokes (`Tr 3`, or the clipping
    /// mode `Tr 7`): the searchable text an OCR tool lays over a scanned page. A viewer
    /// selects it but does not draw it.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub invisible: bool,
    /// Per-glyph boxes, for character-precise selection/hit-testing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glyphs: Vec<GlyphBox>,
}

/// A single positioned glyph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlyphBox {
    /// Box of the glyph, `[x0, y0, x1, y1]` in pt, top-left origin.
    pub bbox: BBox,
    /// The decoded text of this glyph (may be multiple chars for ligatures).
    pub text: String,
}

/// 2x3 matrix `[a b c d e f]`, PDF order.
#[derive(Debug, Clone, Copy)]
struct Mat([f32; 6]);

impl Mat {
    const IDENTITY: Mat = Mat([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// `self ∘ other`: apply `other` first, then `self`.
    fn mul(self, o: Mat) -> Mat {
        let a = self.0;
        let b = o.0;
        Mat([
            a[0] * b[0] + a[2] * b[1],
            a[1] * b[0] + a[3] * b[1],
            a[0] * b[2] + a[2] * b[3],
            a[1] * b[2] + a[3] * b[3],
            a[0] * b[4] + a[2] * b[5] + a[4],
            a[1] * b[4] + a[3] * b[5] + a[5],
        ])
    }

    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        let m = self.0;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }
}

/// Text-space glyph quad -> device AABB (top-left origin).
fn quad_to_bbox(m: &Mat, x0: f32, y0: f32, x1: f32, y1: f32, page_height: f32) -> BBox {
    let corners = [
        m.apply(x0, y0),
        m.apply(x1, y0),
        m.apply(x0, y1),
        m.apply(x1, y1),
    ];
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (x, y) in corners {
        // PDF user space (bottom-left up) -> SVG/top-left space.
        let y = page_height - y;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    [min_x, min_y, max_x, max_y]
}

/// Per-glyph metrics source: an external font's parsed face, or a builtin
/// standard-14 face (whose subset program ships with printpdf).
enum FontRef<'a> {
    External(FontId, &'a ParsedFont),
    Builtin(BuiltinFont, ParsedFont),
    None,
}

impl FontRef<'_> {
    fn parsed(&self) -> Option<&ParsedFont> {
        match self {
            FontRef::External(_, f) => Some(f),
            FontRef::Builtin(_, f) => Some(f),
            FontRef::None => None,
        }
    }

    fn id(&self) -> Option<FontId> {
        match self {
            FontRef::External(id, _) => Some(id.clone()),
            _ => None,
        }
    }

    fn builtin(&self) -> Option<BuiltinFont> {
        match self {
            FontRef::Builtin(b, _) => Some(*b),
            _ => None,
        }
    }
}

fn advance_em(font: Option<&ParsedFont>, gid: u16) -> f32 {
    font.map(|f| crate::ops::glyph_advance_em(f, gid)).unwrap_or(0.5)
}

/// (ascent_em, descent_em) of the current font; the conventional 0.8/-0.2
/// when no metrics are available.
fn vertical_extent_em(font: Option<&ParsedFont>) -> (f32, f32) {
    let Some(f) = font else { return (0.8, -0.2) };
    #[cfg(feature = "text_layout")]
    {
        let upm = f.pdf_font_metrics.units_per_em as f32;
        if upm > 0.0 {
            let asc = f.font_metrics.ascent / upm;
            let desc = f.font_metrics.descent / upm;
            if asc > 0.0 {
                return (asc, desc.min(0.0));
            }
        }
    }
    #[cfg(not(feature = "text_layout"))]
    {
        let upm = f.units_per_em as f32;
        if upm > 0.0 {
            let asc = f.font_metrics.ascent as f32 / upm;
            let desc = f.font_metrics.descent as f32 / upm;
            if asc > 0.0 {
                return (asc, desc.min(0.0));
            }
        }
    }
    (0.8, -0.2)
}

/// The walker's text state (ISO 32000-1, 9.3).
#[derive(Clone)]
struct TextState {
    font_size: f32,
    char_spacing: f32,      // Tc, pt
    word_spacing: f32,      // Tw, pt
    h_scale: f32,           // Tz, fraction (1.0 = 100%)
    rise: f32,              // Ts, pt
    leading: f32,           // TL, pt
    /// Line matrix translation: where the current LINE starts (text space).
    line_origin: (f32, f32),
    /// Full text matrix from SetTextMatrix, if any (composed on top of
    /// line_origin translation).
    tm: Mat,
    /// Pen x offset from the line origin, along the baseline (text space pt).
    pen_x: f32,
}

impl Default for TextState {
    fn default() -> Self {
        TextState {
            font_size: 12.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            h_scale: 1.0,
            rise: 0.0,
            leading: 0.0,
            line_origin: (0.0, 0.0),
            tm: Mat::IDENTITY,
            pen_x: 0.0,
        }
    }
}

impl TextState {
    /// Device matrix for the current pen position: CTM ∘ Tm ∘ translate(line
    /// origin + pen).
    fn device_matrix(&self, ctm: Mat) -> Mat {
        let translate = Mat([
            1.0,
            0.0,
            0.0,
            1.0,
            self.line_origin.0 + self.pen_x,
            self.line_origin.1 + self.rise,
        ]);
        ctm.mul(self.tm).mul(translate)
    }
}

/// The graphics state a word is painted with, besides its font: saved and
/// restored with `q` / `Q` like the CTM.
#[derive(Debug, Clone, Default)]
struct Paint {
    /// `rg` / `g` / `k` / `sc`: `None` = the initial black.
    fill: Option<Color>,
    /// `Tr 3` (or `Tr 7`): the glyphs are neither filled nor stroked.
    invisible: bool,
}

/// Accumulates glyphs into words into lines.
struct Builder {
    lines: Vec<TextLine>,
    cur_line: Option<TextLine>,
    cur_word: Option<TextWord>,
}

impl Builder {
    fn new() -> Self {
        Builder {
            lines: Vec::new(),
            cur_line: None,
            cur_word: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_glyph(
        &mut self,
        text: &str,
        bbox: BBox,
        baseline_y: f32,
        font_size: f32,
        font: Option<FontId>,
        builtin: Option<BuiltinFont>,
        paint: &Paint,
    ) {
        let word = self.cur_word.get_or_insert_with(|| TextWord {
            bbox,
            text: String::new(),
            font_size,
            font: font.clone(),
            builtin,
            color: paint.fill.clone(),
            invisible: paint.invisible,
            glyphs: Vec::new(),
        });
        word.bbox = bbox_union(word.bbox, bbox);
        word.text.push_str(text);
        word.glyphs.push(GlyphBox {
            bbox,
            text: text.to_string(),
        });

        let line = self.cur_line.get_or_insert_with(|| TextLine {
            bbox,
            baseline: baseline_y,
            words: Vec::new(),
        });
        line.bbox = bbox_union(line.bbox, bbox);
    }

    fn end_word(&mut self) {
        if let Some(word) = self.cur_word.take() {
            if !word.text.trim().is_empty() {
                let line = self.cur_line.get_or_insert_with(|| TextLine {
                    bbox: word.bbox,
                    baseline: word.bbox[3],
                    words: Vec::new(),
                });
                line.bbox = bbox_union(line.bbox, word.bbox);
                line.words.push(word);
            }
        }
    }

    fn end_line(&mut self) {
        self.end_word();
        if let Some(line) = self.cur_line.take() {
            if !line.words.is_empty() {
                self.lines.push(line);
            }
        }
    }

    fn finish(mut self) -> Vec<TextLine> {
        self.end_line();
        self.lines
    }
}

impl PdfPage {
    /// Extracts every text run of the page as line, word and glyph boxes with decoded text.
    ///
    /// `resources` are the resources of the document the page belongs to; external fonts are
    /// looked up there to measure glyphs. Text in a font that cannot be found is measured with
    /// a fixed advance of 0.5 em per glyph. Boxes are in pt with the origin at the top left of
    /// the page (see the [module docs](crate::text_boxes)); the page size is its media box.
    ///
    /// The text of the forms the page draws ([`Op::UseXobject`] of an
    /// [`XObject::Form`](crate::XObject::Form)) is part of the page: the boxes are placed with the
    /// matrix of the form and the placement of the `Do`, and the form looks up its fonts in its
    /// own resources. Forms are not clipped to their `/BBox`. A form that draws itself, forms
    /// nested deeper than 16 levels, and more than 10 000 forms or 2 000 000 ops of forms on one
    /// page are skipped.
    pub fn extract_text_boxes(&self, resources: &PdfResources) -> PageTextBoxes {
        let page_width = self.media_box.width.0;
        let page_height = self.media_box.height.0;

        let mut walker = BoxWalker::new(page_height);
        walker.run(&self.ops, resources, &mut FormWalk::new());

        PageTextBoxes {
            width: page_width,
            height: page_height,
            lines: walker.builder.finish(),
        }
    }
}

/// Reads the ops of a page, and of the forms it draws, into words and lines.
struct BoxWalker<'a> {
    builder: Builder,
    st: TextState,
    ctm: Mat,
    ctm_stack: Vec<(Mat, Paint)>,
    paint: Paint,
    in_text: bool,
    font: FontRef<'a>,
    builtin_cache: BTreeMap<BuiltinFont, Option<ParsedFont>>,
    page_height: f32,
}

impl<'a> BoxWalker<'a> {
    fn new(page_height: f32) -> Self {
        BoxWalker {
            builder: Builder::new(),
            st: TextState::default(),
            ctm: Mat::IDENTITY,
            ctm_stack: Vec::new(),
            paint: Paint::default(),
            in_text: false,
            font: FontRef::None,
            builtin_cache: BTreeMap::new(),
            page_height,
        }
    }

    /// Reads `ops`, which name their fonts and XObjects in `scope`.
    fn run(&mut self, ops: &[Op], scope: &'a PdfResources, walk: &mut FormWalk) {
        for op in ops {
            match op {
                Op::StartTextSection => {
                    self.in_text = true;
                    self.st = TextState {
                        font_size: self.st.font_size,
                        char_spacing: self.st.char_spacing,
                        word_spacing: self.st.word_spacing,
                        h_scale: self.st.h_scale,
                        leading: self.st.leading,
                        ..TextState::default()
                    };
                }
                Op::EndTextSection => {
                    self.in_text = false;
                    self.builder.end_line();
                }
                Op::SaveGraphicsState => self.ctm_stack.push((self.ctm, self.paint.clone())),
                Op::SetFillColor { col } => self.paint.fill = Some(col.clone()),
                Op::SetTextRenderingMode { mode } => {
                    self.paint.invisible =
                        matches!(mode, TextRenderingMode::Invisible | TextRenderingMode::Clip);
                }
                Op::RestoreGraphicsState => {
                    if let Some((m, p)) = self.ctm_stack.pop() {
                        self.ctm = m;
                        self.paint = p;
                    }
                }
                Op::SetTransformationMatrix { matrix } => {
                    let m = matrix.as_array();
                    self.ctm = self.ctm.mul(Mat([m[0], m[1], m[2], m[3], m[4], m[5]]));
                }
                Op::SetFont { font: handle, size } => {
                    self.st.font_size = size.0;
                    self.font = match handle {
                        PdfFontHandle::External(id) => match scope.fonts.map.get(id) {
                            Some(pf) => FontRef::External(id.clone(), &pf.parsed_font),
                            None => FontRef::None,
                        },
                        PdfFontHandle::Builtin(b) => {
                            // The face only measures glyphs: one that does not parse is
                            // measured with the fixed advance, like a font that is missing.
                            let parsed = self
                                .builtin_cache
                                .entry(*b)
                                .or_insert_with(|| b.get_parsed_font());
                            match parsed {
                                Some(parsed) => FontRef::Builtin(*b, parsed.clone()),
                                None => FontRef::None,
                            }
                        }
                    };
                }
                Op::SetLineHeight { lh } => self.st.leading = lh.0,
                Op::SetWordSpacing { pt } => self.st.word_spacing = pt.0,
                Op::SetCharacterSpacing { multiplier } => self.st.char_spacing = *multiplier,
                Op::SetHorizontalScaling { percent } => self.st.h_scale = *percent / 100.0,
                Op::SetLineOffset { multiplier } => self.st.rise = *multiplier,
                Op::SetTextCursor { pos } => {
                    // The parser normalizes Td/TD into absolute cursors.
                    if (pos.y.0 - self.st.line_origin.1).abs() > f32::EPSILON
                        || self.st.pen_x != 0.0
                    {
                        self.builder.end_line();
                    }
                    self.st.line_origin = (pos.x.0, pos.y.0);
                    self.st.pen_x = 0.0;
                }
                Op::MoveTextCursorAndSetLeading { tx, ty } => {
                    self.builder.end_line();
                    self.st.leading = -*ty;
                    self.st.line_origin = (self.st.line_origin.0 + tx, self.st.line_origin.1 + ty);
                    self.st.pen_x = 0.0;
                }
                Op::SetTextMatrix { matrix } => {
                    let m = matrix.as_array();
                    let new = Mat([m[0], m[1], m[2], m[3], m[4], m[5]]);
                    // A vertical move is a new line; a horizontal one usually a
                    // positioned run on the same baseline.
                    if (new.0[5] - self.st.tm.0[5]).abs() > 0.5 {
                        self.builder.end_line();
                    }
                    self.st.tm = new;
                    self.st.line_origin = (0.0, 0.0);
                    self.st.pen_x = 0.0;
                }
                Op::AddLineBreak => {
                    self.builder.end_line();
                    self.st.line_origin = (self.st.line_origin.0, self.st.line_origin.1 - self.st.leading);
                    self.st.pen_x = 0.0;
                }
                Op::ShowText { items } if self.in_text => {
                    show_items(
                        items,
                        &mut self.st,
                        self.ctm,
                        &self.font,
                        &self.paint,
                        &mut self.builder,
                        self.page_height,
                    );
                }
                Op::MoveToNextLineShowText { text } if self.in_text => {
                    self.builder.end_line();
                    self.st.line_origin = (self.st.line_origin.0, self.st.line_origin.1 - self.st.leading);
                    self.st.pen_x = 0.0;
                    let items = [TextItem::Text(text.clone())];
                    show_items(
                        &items,
                        &mut self.st,
                        self.ctm,
                        &self.font,
                        &self.paint,
                        &mut self.builder,
                        self.page_height,
                    );
                }
                Op::SetSpacingMoveAndShowText {
                    word_spacing,
                    char_spacing,
                    text,
                } if self.in_text => {
                    self.st.word_spacing = *word_spacing;
                    self.st.char_spacing = *char_spacing;
                    self.builder.end_line();
                    self.st.line_origin = (self.st.line_origin.0, self.st.line_origin.1 - self.st.leading);
                    self.st.pen_x = 0.0;
                    let items = [TextItem::Text(text.clone())];
                    show_items(
                        &items,
                        &mut self.st,
                        self.ctm,
                        &self.font,
                        &self.paint,
                        &mut self.builder,
                        self.page_height,
                    );
                }
                Op::UseXobject { id, transform } => {
                    if let Some(form) = form_in(scope, id) {
                        if walk.enter(form).is_ok() {
                            self.run_form(form, id, transform, scope, walk);
                            walk.leave();
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Reads the ops of `form`, drawn in `outer` with the placement `transform`. The graphics
    /// state is the form's own and is restored when it ends, as for a `Do`.
    fn run_form(
        &mut self,
        form: &'a FormXObject,
        id: &crate::XObjectId,
        transform: &XObjectTransform,
        outer: &'a PdfResources,
        walk: &mut FormWalk,
    ) {
        // Where the form is: its matrix, then the `cm` of the placement, then the page's.
        let size = if transform.no_auto_scale {
            None
        } else {
            outer.xobjects.map.get(id).and_then(|x| x.get_width_height())
        };
        let placement = Mat(transform.placement_matrix(size));
        let matrix = form
            .matrix
            .map_or(Mat::IDENTITY, |m| Mat(CurTransMat::as_array(&m)));

        self.builder.end_line();
        let saved_st = self.st.clone();
        let saved_ctm = self.ctm;
        let saved_stack = std::mem::take(&mut self.ctm_stack);
        let saved_paint = self.paint.clone();
        let saved_in_text = self.in_text;
        let saved_font = std::mem::replace(&mut self.font, FontRef::None);

        self.ctm = self.ctm.mul(placement).mul(matrix);
        self.in_text = false;
        self.run(&form.ops, scope_of(form, outer), walk);
        self.builder.end_line();

        self.st = saved_st;
        self.ctm = saved_ctm;
        self.ctm_stack = saved_stack;
        self.paint = saved_paint;
        self.in_text = saved_in_text;
        self.font = saved_font;
    }
}

fn show_items(
    items: &[TextItem],
    st: &mut TextState,
    ctm: Mat,
    font: &FontRef,
    paint: &Paint,
    builder: &mut Builder,
    page_height: f32,
) {
    let parsed = font.parsed();
    let font_id = font.id();
    let builtin = font.builtin();
    let (asc_em, desc_em) = vertical_extent_em(parsed);

    let emit = |st: &mut TextState,
                    builder: &mut Builder,
                    text: &str,
                    adv_em: f32,
                    is_space: bool| {
        // ISO 32000-1 §9.4.4: word spacing applies to single-byte code 32
        // only: for the simple-font path that is the space char; CID
        // glyphs never get Tw here (2-byte codes).
        let adv_pt = (adv_em * st.font_size
            + st.char_spacing
            + if is_space { st.word_spacing } else { 0.0 })
            * st.h_scale;

        if is_space {
            builder.end_word();
            st.pen_x += adv_pt;
            return;
        }

        let m = st.device_matrix(ctm);
        let bbox = quad_to_bbox(
            &m,
            0.0,
            desc_em * st.font_size,
            adv_em * st.font_size * st.h_scale,
            asc_em * st.font_size,
            page_height,
        );
        let (_, baseline_pdf) = m.apply(0.0, 0.0);
        builder.push_glyph(
            text,
            bbox,
            page_height - baseline_pdf,
            st.font_size,
            font_id.clone(),
            builtin,
            paint,
        );
        st.pen_x += adv_pt;
    };

    for item in items {
        match item {
            TextItem::Text(t) => {
                for c in t.chars() {
                    let adv = parsed
                        .and_then(|f| {
                            f.lookup_glyph_index(c as u32)
                                .map(|gid| crate::ops::glyph_advance_em(f, gid))
                        })
                        .unwrap_or(0.5);
                    let mut buf = [0u8; 4];
                    emit(st, builder, c.encode_utf8(&mut buf), adv, c == ' ');
                }
            }
            TextItem::GlyphIds(glyphs) => {
                for g in glyphs {
                    if g.offset != 0.0 {
                        // TJ offset: negative moves right by |o|/1000 em.
                        let shift = -g.offset / 1000.0 * st.font_size * st.h_scale;
                        if shift > st.font_size * 0.15 {
                            builder.end_word();
                        }
                        st.pen_x += shift;
                    }
                    let text = g.cid.clone().unwrap_or_else(|| "\u{FFFD}".to_string());
                    let adv = advance_em(parsed, g.gid);
                    let is_space = text == " ";
                    emit(st, builder, &text, adv, is_space);
                }
            }
            TextItem::Offset(o) => {
                let shift = -o / 1000.0 * st.font_size * st.h_scale;
                // A large rightward shift is a word gap (same heuristic as
                // extract_text: < -100 thousandths).
                if *o < -100.0 {
                    builder.end_word();
                }
                st.pen_x += shift;
            }
        }
    }
}

impl crate::PdfDocument {
    /// [`PdfPage::extract_text_boxes`] for every page, in page order.
    pub fn extract_text_boxes(&self) -> Vec<PageTextBoxes> {
        self.pages
            .iter()
            .map(|p| p.extract_text_boxes(&self.resources))
            .collect()
    }
}
